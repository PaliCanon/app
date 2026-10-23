//! The dictionary offline (task 008): three SQLite assets the bundle ships
//! under `Resources/dictionary/`, read on demand. `dictionary.db` holds
//! every DPD entry (the prefix search, as the API orders it: the exact
//! headword first, then by frequency, length, lemma); `dpd.db` the forms
//! (a word as written to the keys of its senses); `passages.db` every
//! entry's line from the suttas, precomputed by learn-pali's
//! `build-deck passages`. A file missing or unreadable answers
//! `Unavailable` for its call only, never a crash.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use palicanon_app_bridge::{
    Passage, PassageAnswer, PassageBlank, PassageLine, SensesAnswer, WordEntry, WordSense,
    WordsAnswer,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use unicode_normalization::UnicodeNormalization;

/// The dictionary asset's file name in the resources.
pub const DICTIONARY_FILE: &str = "dictionary.db";
/// The forms asset's file name.
pub const FORMS_FILE: &str = "dpd.db";
/// The passages asset's file name.
pub const PASSAGES_FILE: &str = "passages.db";
/// How many entries a search answers at most (the API's default).
pub const SEARCH_LIMIT: usize = 20;

/// The three assets, opened read-only the first time each is asked.
pub struct Dictionary {
    dir: PathBuf,
    entries: Mutex<Option<Connection>>,
    forms: Mutex<Option<Connection>>,
    passages: Mutex<Option<Connection>>,
}

impl Dictionary {
    /// A dictionary over the assets in `dir`; nothing is opened yet.
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Dictionary {
            dir,
            entries: Mutex::new(None),
            forms: Mutex::new(None),
            passages: Mutex::new(None),
        }
    }

    /// Runs `query` over the asset `file`, opening it once; `None` when
    /// the file cannot be opened or the query fails (logged).
    fn with<T>(
        &self,
        slot: &Mutex<Option<Connection>>,
        file: &str,
        query: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> Option<T> {
        let mut guard = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard.is_none() {
            let path = self.dir.join(file);
            match Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
                Ok(conn) => *guard = Some(conn),
                Err(e) => {
                    log::warn!("dictionary: cannot open {}: {e}", path.display());
                    return None;
                }
            }
        }
        let conn = guard.as_ref()?;
        match query(conn) {
            Ok(value) => Some(value),
            Err(e) => {
                log::warn!("dictionary: {file}: {e}");
                None
            }
        }
    }

    /// The entries whose folded headword starts with `q` folded (fewer
    /// than two letters: none).
    pub fn search(&self, q: &str) -> WordsAnswer {
        let prefix = fold(q.trim());
        if prefix.chars().count() < 2 {
            return WordsAnswer::Entries(Vec::new());
        }
        self.with(&self.entries, DICTIONARY_FILE, |conn| {
            search_entries(conn, &prefix, SEARCH_LIMIT)
        })
        .map_or(WordsAnswer::Unavailable, WordsAnswer::Entries)
    }

    /// The precomputed line for `lemma`.
    pub fn passage(&self, lemma: &str) -> PassageAnswer {
        self.with(&self.passages, PASSAGES_FILE, |conn| {
            passage_row(conn, lemma)
        })
        .unwrap_or(PassageAnswer::Unavailable)
    }

    /// The senses of `form`, the word as written in a line.
    pub fn lookup(&self, form: &str) -> SensesAnswer {
        let word = normalise(form);
        if word.is_empty() {
            return SensesAnswer::Senses(Vec::new());
        }
        self.with(&self.forms, FORMS_FILE, |conn| senses_of(conn, &word))
            .map_or(SensesAnswer::Unavailable, SensesAnswer::Senses)
    }
}

/// Lowercases, decomposes and drops the combining marks (the API's fold):
/// `ṁ`, `ṃ`, `ā`, `ñ`, `ṭ` fold to `m`, `m`, `a`, `n`, `t`.
#[must_use]
pub fn fold(text: &str) -> String {
    text.to_lowercase()
        .nfd()
        .filter(|c| !('\u{0300}'..='\u{036F}').contains(c))
        .collect()
}

/// A form as the forms asset keys it: composed, lower case, the
/// punctuation around it dropped.
#[must_use]
pub fn normalise(form: &str) -> String {
    form.nfc()
        .collect::<String>()
        .to_lowercase()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_string()
}

fn search_entries(conn: &Connection, prefix: &str, n: usize) -> rusqlite::Result<Vec<WordEntry>> {
    let upper = format!("{prefix}\u{10FFFF}");
    let mut stmt = conn.prepare_cached(
        "SELECT headword, lemma, pos, meaning, meaning_fr, example, source, sutta \
         FROM entries WHERE folded >= ?1 AND folded < ?2 \
         ORDER BY (folded = ?1) DESC, freq DESC, length(folded), lemma LIMIT ?3",
    )?;
    let n = i64::try_from(n).unwrap_or(i64::MAX);
    let blank_to_none = |s: Option<String>| s.filter(|s| !s.is_empty());
    let rows = stmt.query_map(params![prefix, upper, n], |r| {
        Ok(WordEntry {
            headword: r.get(0)?,
            lemma: r.get(1)?,
            pos: r.get(2)?,
            meaning: r.get(3)?,
            meaning_fr: blank_to_none(r.get(4)?),
            example: blank_to_none(r.get(5)?),
            source: blank_to_none(r.get(6)?),
            sutta: blank_to_none(r.get(7)?),
        })
    })?;
    rows.collect()
}

fn passage_row(conn: &Connection, lemma: &str) -> rusqlite::Result<PassageAnswer> {
    let mut stmt = conn.prepare_cached(
        "SELECT text, source, sutta, blank_before, blank_answer, blank_after, \
         en_text, en_translator, fr_text, fr_translator, vi_text, vi_translator, miss \
         FROM passages WHERE lemma = ?1",
    )?;
    let row = stmt
        .query_row([lemma], |r| {
            let line = |text: Option<String>, translator: Option<String>| {
                Some(PassageLine {
                    text: text?,
                    translator: translator?,
                })
            };
            let blank = match (r.get(3)?, r.get(4)?, r.get(5)?) {
                (Some(before), Some(answer), Some(after)) => Some(PassageBlank {
                    before,
                    answer,
                    after,
                }),
                _ => None,
            };
            let miss: Option<String> = r.get(12)?;
            Ok((
                Passage {
                    text: r.get(0)?,
                    source: r.get(1)?,
                    sutta: r.get(2)?,
                    blank,
                    en: line(r.get(6)?, r.get(7)?),
                    fr: line(r.get(8)?, r.get(9)?),
                    vi: line(r.get(10)?, r.get(11)?),
                },
                miss,
            ))
        })
        .optional()?;
    Ok(match row {
        None => PassageAnswer::Miss("no_example".to_string()),
        Some((_, Some(miss))) => PassageAnswer::Miss(miss),
        Some((passage, None)) => PassageAnswer::Found(Box::new(passage)),
    })
}

fn senses_of(conn: &Connection, word: &str) -> rusqlite::Result<Vec<WordSense>> {
    let keys: Option<String> = conn
        .query_row("SELECT keys FROM words WHERE word = ?1", [word], |r| {
            r.get(0)
        })
        .optional()?;
    let Some(keys) = keys else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare_cached("SELECT lemma, pos, en, fr FROM senses WHERE key = ?1")?;
    let mut out = Vec::new();
    for key in keys.split(',') {
        let sense = stmt
            .query_row([key], |r| {
                Ok(WordSense {
                    lemma: r.get(0)?,
                    pos: r.get(1)?,
                    en: r.get(2)?,
                    fr: r.get::<_, Option<String>>(3)?.filter(|s| !s.is_empty()),
                })
            })
            .optional()?;
        out.extend(sense);
    }
    Ok(out)
}

/// Writes fixture assets into `dir` (tests, and the command tests): one
/// entry with a passage, one without, one form with two senses.
///
/// # Panics
/// The directory cannot be written.
#[doc(hidden)]
pub fn write_fixtures(dir: &Path) {
    std::fs::create_dir_all(dir).expect("the fixture directory");
    let entries = Connection::open(dir.join(DICTIONARY_FILE)).expect("dictionary.db");
    entries
        .execute_batch(
            "CREATE TABLE entries (id INTEGER PRIMARY KEY, lemma TEXT NOT NULL UNIQUE, headword TEXT NOT NULL, folded TEXT NOT NULL, pos TEXT NOT NULL, meaning TEXT NOT NULL, meaning_fr TEXT, example TEXT NOT NULL, source TEXT NOT NULL, sutta TEXT NOT NULL, freq INTEGER NOT NULL);
             INSERT INTO entries VALUES (1, 'dhamma 1.01', 'dhamma', 'dhamma', 'masc', 'nature; character', 'nature', 'natthi idha <b>dhammo</b>', 'SN22.87', 'Vakkalisutta', 50);
             INSERT INTO entries VALUES (2, 'dhammā', 'dhammā', 'dhamma', 'fem', 'a plant', NULL, '', '', '', 1);
             INSERT INTO entries VALUES (3, 'dhammacakkhu', 'dhammacakkhu', 'dhammacakkhu', 'nt', 'eye of the teaching', NULL, '', '', '', 9);
             INSERT INTO entries VALUES (4, 'vakkali', 'vakkali', 'vakkali', 'masc', 'a monk', NULL, 'alaṃ <b>vakkali</b>', 'SN22.87', 'Vakkalisutta', 3);",
        )
        .expect("the entries");
    let forms = Connection::open(dir.join(FORMS_FILE)).expect("dpd.db");
    forms
        .execute_batch(
            "CREATE TABLE words (word TEXT PRIMARY KEY, keys TEXT NOT NULL);
             CREATE TABLE senses (key TEXT PRIMARY KEY, lemma TEXT NOT NULL, pos TEXT NOT NULL, en TEXT NOT NULL, fr TEXT);
             INSERT INTO words VALUES ('dhammaṃ', 'A,B');
             INSERT INTO senses VALUES ('A', 'dhamma 1.01', 'masc', 'nature; character', 'nature');
             INSERT INTO senses VALUES ('B', 'dhamma 1.02', 'masc', 'teaching', '');",
        )
        .expect("the forms");
    let passages = Connection::open(dir.join(PASSAGES_FILE)).expect("passages.db");
    passages
        .execute_batch(
            "CREATE TABLE passages (lemma TEXT PRIMARY KEY, text TEXT NOT NULL, source TEXT NOT NULL, sutta TEXT NOT NULL, blank_before TEXT, blank_answer TEXT, blank_after TEXT, en_text TEXT, en_translator TEXT, fr_text TEXT, fr_translator TEXT, vi_text TEXT, vi_translator TEXT, miss TEXT);
             INSERT INTO passages VALUES ('dhamma 1.01', 'natthi idha dhammo', 'SN22.87', 'Vakkalisutta', 'natthi idha ', 'dhammo', '', 'There is no nature here.', 'sujato', NULL, NULL, NULL, NULL, NULL);
             INSERT INTO passages VALUES ('vakkali', 'alaṃ vakkali', 'SN22.87', 'Vakkalisutta', 'alaṃ ', 'vakkali', '', NULL, NULL, NULL, NULL, NULL, NULL, 'not_matched');",
        )
        .expect("the passages");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Dictionary {
        let dir = std::env::temp_dir().join(format!("pc-dictionary-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_fixtures(&dir);
        Dictionary::new(dir)
    }

    #[test]
    fn the_search_folds_the_query_and_orders_exact_then_frequent() {
        let d = fixture("search");
        let WordsAnswer::Entries(found) = d.search("Dhammā") else {
            panic!("entries")
        };
        let lemmas: Vec<&str> = found.iter().map(|e| e.lemma.as_str()).collect();
        assert_eq!(lemmas, ["dhamma 1.01", "dhammā", "dhammacakkhu"]);
        assert_eq!(found[0].meaning_fr.as_deref(), Some("nature"));
        assert_eq!(found[1].example, None, "an empty example is none");
        assert_eq!(
            d.search("d"),
            WordsAnswer::Entries(Vec::new()),
            "one letter"
        );
        assert_eq!(d.search("zz"), WordsAnswer::Entries(Vec::new()));
    }

    #[test]
    fn the_passage_is_found_missed_by_name_or_absent() {
        let d = fixture("passage");
        let PassageAnswer::Found(p) = d.passage("dhamma 1.01") else {
            panic!("found")
        };
        assert_eq!(p.blank.as_ref().map(|b| b.answer.as_str()), Some("dhammo"));
        assert_eq!(p.en.as_ref().map(|l| l.translator.as_str()), Some("sujato"));
        assert_eq!(p.fr, None);
        assert_eq!(
            d.passage("vakkali"),
            PassageAnswer::Miss("not_matched".into())
        );
        assert_eq!(
            d.passage("nobody"),
            PassageAnswer::Miss("no_example".into())
        );
    }

    #[test]
    fn the_lookup_normalises_the_form_and_lists_its_senses_in_key_order() {
        let d = fixture("lookup");
        let SensesAnswer::Senses(senses) = d.lookup("“Dhammaṃ,") else {
            panic!("senses")
        };
        assert_eq!(senses.len(), 2);
        assert_eq!(senses[0].lemma, "dhamma 1.01");
        assert_eq!(senses[0].fr.as_deref(), Some("nature"));
        assert_eq!(senses[1].fr, None, "an empty fr is none");
        assert_eq!(d.lookup("nobody"), SensesAnswer::Senses(Vec::new()));
        assert_eq!(d.lookup("…"), SensesAnswer::Senses(Vec::new()));
    }

    #[test]
    fn a_missing_asset_answers_unavailable_for_its_call_only() {
        let d = fixture("missing");
        std::fs::remove_file(d.dir.join(PASSAGES_FILE)).unwrap();
        assert_eq!(d.passage("dhamma 1.01"), PassageAnswer::Unavailable);
        assert!(matches!(d.search("dha"), WordsAnswer::Entries(_)));
        let none = Dictionary::new(std::env::temp_dir().join("pc-dictionary-nowhere"));
        assert_eq!(none.search("dha"), WordsAnswer::Unavailable);
        assert_eq!(none.lookup("dhammaṃ"), SensesAnswer::Unavailable);
    }

    #[test]
    fn fold_and_normalise_as_the_api_does() {
        assert_eq!(fold("Ṭhānaṃ"), "thanam");
        assert_eq!(fold("ṁ"), "m");
        assert_eq!(normalise("‘Saṅkhārā’"), "saṅkhārā");
        assert_eq!(normalise("  "), "");
    }
}

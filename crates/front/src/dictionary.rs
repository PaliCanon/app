//! Learn's dictionary in the app (task 008): the fragment's `Dictionary`
//! port over the desktop's three commands, which read the shipped assets.
//! The port answers through callbacks on the browser's thread; the wire
//! types are the bridge's, mapped here to the fragment's.

use std::rc::Rc;

use learn_core::deck::{Blank, Example, Translation};
use learn_pali::fragment::{
    Dictionary, Entry, LookupAnswer, Miss, PassageAnswer as LearnPassage, SearchAnswer, Sense,
};
use palicanon_app_bridge::{Passage, PassageAnswer, SensesAnswer, WordEntry, WordsAnswer};

/// What the desktop answers, one call at a time, through `done`.
pub trait DictionaryPort: 'static {
    /// `dictionary_search(q)`.
    fn search(&self, q: String, done: Box<dyn FnOnce(Result<WordsAnswer, String>)>);
    /// `dictionary_passage(lemma)`.
    fn passage(&self, lemma: String, done: Box<dyn FnOnce(Result<PassageAnswer, String>)>);
    /// `dictionary_lookup(form)`.
    fn lookup(&self, form: String, done: Box<dyn FnOnce(Result<SensesAnswer, String>)>);
}

/// The fragment's dictionary over a [`DictionaryPort`].
pub struct AppDictionary {
    port: Rc<dyn DictionaryPort>,
}

impl AppDictionary {
    /// A dictionary over `port`.
    #[must_use]
    pub fn new(port: Rc<dyn DictionaryPort>) -> Self {
        AppDictionary { port }
    }
}

impl Dictionary for AppDictionary {
    fn search(&self, q: String, on_done: Box<dyn FnOnce(SearchAnswer)>) {
        self.port
            .search(q, Box::new(move |answer| on_done(search_of(answer))));
    }

    fn passage(&self, entry: Entry, on_done: Box<dyn FnOnce(LearnPassage)>) {
        self.port.passage(
            entry.lemma,
            Box::new(move |answer| on_done(passage_of(answer))),
        );
    }

    fn lookup(&self, form: String, on_done: Box<dyn FnOnce(LookupAnswer)>) {
        self.port
            .lookup(form, Box::new(move |answer| on_done(lookup_of(answer))));
    }
}

/// The fragment's search answer: an error or a missing asset is Unreachable.
#[must_use]
pub fn search_of(answer: Result<WordsAnswer, String>) -> SearchAnswer {
    match answer {
        Ok(WordsAnswer::Entries(entries)) => {
            SearchAnswer::Entries(entries.into_iter().map(entry_of).collect())
        }
        Ok(WordsAnswer::Unavailable) | Err(_) => SearchAnswer::Unreachable,
    }
}

fn entry_of(e: WordEntry) -> Entry {
    Entry {
        headword: e.headword,
        lemma: e.lemma,
        pos: e.pos,
        meaning: e.meaning,
        example: e.example,
        source: e.source,
        sutta: e.sutta,
    }
}

/// The fragment's passage answer: a miss by name, a missing asset or an
/// error is Transient (the word is still added, without its line).
#[must_use]
pub fn passage_of(answer: Result<PassageAnswer, String>) -> LearnPassage {
    match answer {
        Ok(PassageAnswer::Found(passage)) => LearnPassage::Found(example_of(*passage)),
        Ok(PassageAnswer::Miss(name)) => LearnPassage::Miss(miss_of(&name)),
        Ok(PassageAnswer::Unavailable) | Err(_) => LearnPassage::Transient,
    }
}

fn miss_of(name: &str) -> Miss {
    match name {
        "no_example" => Miss::NoExample,
        "unmapped_source" => Miss::UnmappedSource,
        "text_not_served" | "no_pali" | "fetch" => Miss::TextNotServed,
        _ => Miss::NotMatched,
    }
}

fn example_of(p: Passage) -> Example {
    let mut translations = std::collections::BTreeMap::new();
    for (lang, line) in [("en", p.en), ("fr", p.fr), ("vi", p.vi)] {
        if let Some(line) = line {
            translations.insert(
                lang.to_string(),
                Translation {
                    text: line.text,
                    translator: line.translator,
                },
            );
        }
    }
    Example {
        text: p.text,
        source: p.source,
        sutta: p.sutta,
        blank: p.blank.map(|b| Blank {
            before: b.before,
            answer: b.answer,
            after: b.after,
        }),
        translations,
    }
}

/// The fragment's lookup answer.
#[must_use]
pub fn lookup_of(answer: Result<SensesAnswer, String>) -> LookupAnswer {
    match answer {
        Ok(SensesAnswer::Senses(senses)) => LookupAnswer::Senses(
            senses
                .into_iter()
                .map(|s| Sense {
                    lemma: s.lemma,
                    pos: s.pos,
                    en: s.en,
                    fr: s.fr,
                })
                .collect(),
        ),
        Ok(SensesAnswer::Unavailable) | Err(_) => LookupAnswer::Unreachable,
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::cell::RefCell;

    use palicanon_app_bridge::{PassageBlank, PassageLine, WordSense};

    use super::*;

    /// A port that answers at once and remembers what it was asked.
    struct Fake {
        asked: RefCell<Vec<String>>,
    }

    impl DictionaryPort for Fake {
        fn search(&self, q: String, done: Box<dyn FnOnce(Result<WordsAnswer, String>)>) {
            self.asked.borrow_mut().push(format!("search {q}"));
            done(Ok(WordsAnswer::Entries(vec![WordEntry {
                headword: "dhamma".into(),
                lemma: "dhamma 1.01".into(),
                pos: "masc".into(),
                meaning: "nature".into(),
                meaning_fr: Some("nature".into()),
                example: Some("natthi idha <b>dhammo</b>".into()),
                source: Some("SN22.87".into()),
                sutta: Some("Vakkalisutta".into()),
            }])));
        }

        fn passage(&self, lemma: String, done: Box<dyn FnOnce(Result<PassageAnswer, String>)>) {
            self.asked.borrow_mut().push(format!("passage {lemma}"));
            done(Ok(PassageAnswer::Found(Box::new(Passage {
                text: "natthi idha dhammo".into(),
                source: "SN22.87".into(),
                sutta: "Vakkalisutta".into(),
                blank: Some(PassageBlank {
                    before: "natthi idha ".into(),
                    answer: "dhammo".into(),
                    after: String::new(),
                }),
                en: Some(PassageLine {
                    text: "There is no nature here.".into(),
                    translator: "sujato".into(),
                }),
                fr: None,
                vi: None,
            }))));
        }

        fn lookup(&self, form: String, done: Box<dyn FnOnce(Result<SensesAnswer, String>)>) {
            self.asked.borrow_mut().push(format!("lookup {form}"));
            done(Ok(SensesAnswer::Senses(vec![WordSense {
                lemma: "dhamma 1.01".into(),
                pos: "masc".into(),
                en: "nature".into(),
                fr: None,
            }])));
        }
    }

    fn entry() -> Entry {
        Entry {
            headword: "dhamma".into(),
            lemma: "dhamma 1.01".into(),
            pos: "masc".into(),
            meaning: "nature".into(),
            example: None,
            source: None,
            sutta: None,
        }
    }

    #[test]
    fn the_three_calls_reach_the_port_and_map_the_answers() {
        let fake = Rc::new(Fake {
            asked: RefCell::new(Vec::new()),
        });
        let dictionary = AppDictionary::new(fake.clone());
        let got = Rc::new(RefCell::new(Vec::new()));
        let g = got.clone();
        dictionary.search(
            "dha".into(),
            Box::new(move |a| g.borrow_mut().push(format!("{a:?}"))),
        );
        let g = got.clone();
        dictionary.passage(
            entry(),
            Box::new(move |a| g.borrow_mut().push(format!("{a:?}"))),
        );
        let g = got.clone();
        dictionary.lookup(
            "dhammaṃ".into(),
            Box::new(move |a| g.borrow_mut().push(format!("{a:?}"))),
        );
        assert_eq!(
            *fake.asked.borrow(),
            ["search dha", "passage dhamma 1.01", "lookup dhammaṃ"]
        );
        let got = got.borrow();
        assert!(
            got[0].contains("Entries") && got[0].contains("dhamma 1.01"),
            "{}",
            got[0]
        );
        assert!(
            got[1].contains("Found") && got[1].contains("sujato"),
            "{}",
            got[1]
        );
        assert!(
            got[2].contains("Senses") && got[2].contains("nature"),
            "{}",
            got[2]
        );
    }

    #[test]
    fn a_missing_asset_or_an_error_is_unreachable_or_transient() {
        assert_eq!(
            search_of(Ok(WordsAnswer::Unavailable)),
            SearchAnswer::Unreachable
        );
        assert_eq!(search_of(Err("ipc".into())), SearchAnswer::Unreachable);
        assert_eq!(
            passage_of(Ok(PassageAnswer::Unavailable)),
            LearnPassage::Transient
        );
        assert_eq!(passage_of(Err("ipc".into())), LearnPassage::Transient);
        assert_eq!(lookup_of(Err("ipc".into())), LookupAnswer::Unreachable);
    }

    #[test]
    fn a_miss_keeps_its_reason() {
        assert_eq!(
            passage_of(Ok(PassageAnswer::Miss("unmapped_source".into()))),
            LearnPassage::Miss(Miss::UnmappedSource)
        );
        assert_eq!(
            passage_of(Ok(PassageAnswer::Miss("no_example".into()))),
            LearnPassage::Miss(Miss::NoExample)
        );
        assert_eq!(
            passage_of(Ok(PassageAnswer::Miss("not_matched".into()))),
            LearnPassage::Miss(Miss::NotMatched)
        );
        assert_eq!(
            passage_of(Ok(PassageAnswer::Miss("text_not_served".into()))),
            LearnPassage::Miss(Miss::TextNotServed)
        );
    }

    #[test]
    fn the_example_carries_the_blank_and_each_languages_line() {
        let example = example_of(Passage {
            text: "alaṃ vakkali".into(),
            source: "SN22.87".into(),
            sutta: "Vakkalisutta".into(),
            blank: None,
            en: Some(PassageLine {
                text: "Enough, Vakkali!".into(),
                translator: "sujato".into(),
            }),
            fr: Some(PassageLine {
                text: "Assez, Vakkali !".into(),
                translator: "christelle".into(),
            }),
            vi: None,
        });
        assert_eq!(example.blank, None);
        assert_eq!(example.translations.len(), 2);
        assert_eq!(example.translations["fr"].translator, "christelle");
    }
}

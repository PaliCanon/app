//! Export and import of the progress through native dialogs: the Rust side
//! opens the dialog (the dialog plugin's Rust API), writes or reads the file
//! the learner chose, and the front never holds a path. No fs plugin; the
//! webview is granted no dialog or fs permission.
//!
//! The decisions are pure and tested here: the suggested name reduced to a
//! plain `.json` file name ([`reduce_name`]), the write of an export within
//! the cap ([`write_export`]), the read of an import within the cap
//! ([`read_import`]).

use std::io::Read;
use std::path::Path;

use palicanon_app_bridge::{ImportAnswer, MAX_TEXT_BYTES};

/// The name suggested when the front's is empty once reduced.
pub const FALLBACK_NAME: &str = "pc_learn.json";

/// The front's suggested `name` reduced to a file name: its last path
/// component only (`Path::file_name`, so no separator), no NUL, no `/` or
/// `\`, and `.json` forced at its end.
pub fn reduce_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let base = Path::new(last)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let clean: String = base
        .chars()
        .filter(|c| *c != '\0' && *c != '/' && *c != '\\' && *c != ':')
        .collect();
    let clean = clean.trim().trim_start_matches('.');
    if clean.is_empty() {
        return FALLBACK_NAME.to_string();
    }
    if clean.to_ascii_lowercase().ends_with(".json") {
        clean.to_string()
    } else {
        format!("{clean}.json")
    }
}

/// Writes the export `text` at `path`, the place the learner chose.
///
/// # Errors
/// A text over [`MAX_TEXT_BYTES`], or the write's I/O error, as the message
/// the native dialog shows.
pub fn write_export(path: &Path, text: &str) -> Result<(), String> {
    check_cap(text)?;
    std::fs::write(path, text).map_err(|e| {
        format!(
            "The progress could not be written to {}: {e}",
            path.display()
        )
    })
}

/// Refuses a text over the cap.
///
/// # Errors
/// A text over [`MAX_TEXT_BYTES`].
pub fn check_cap(text: &str) -> Result<(), String> {
    if text.len() > MAX_TEXT_BYTES {
        Err(format!(
            "The progress is {} bytes, over the {MAX_TEXT_BYTES} bytes the app writes.",
            text.len()
        ))
    } else {
        Ok(())
    }
}

/// Reads the import at `path`, at most [`MAX_TEXT_BYTES`]: its text, or
/// `Failed` (too large, not UTF-8, unreadable).
pub fn read_import(path: &Path) -> ImportAnswer {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) => return ImportAnswer::Failed(format!("cannot open {}: {e}", path.display())),
    };
    let mut bytes = Vec::new();
    let cap = u64::try_from(MAX_TEXT_BYTES).unwrap_or(u64::MAX);
    if let Err(e) = file.take(cap + 1).read_to_end(&mut bytes) {
        return ImportAnswer::Failed(format!("cannot read {}: {e}", path.display()));
    }
    if bytes.len() > MAX_TEXT_BYTES {
        return ImportAnswer::Failed(format!("{} is over 4 MiB", path.display()));
    }
    match String::from_utf8(bytes) {
        Ok(text) => ImportAnswer::Text(text),
        Err(_) => ImportAnswer::Failed(format!("{} is not text", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pc-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_name_is_reduced_to_a_plain_json_file_name() {
        for (given, reduced) in [
            ("pc_learn-2026-10-16.json", "pc_learn-2026-10-16.json"),
            ("../../etc/passwd", "passwd.json"),
            ("/abs/dir/x.json", "x.json"),
            ("a\\b\\c.json", "c.json"),
            ("we\0ird.json", "weird.json"),
            ("notes.txt", "notes.txt.json"),
            ("UPPER.JSON", "UPPER.JSON"),
            ("C:x", "Cx.json"),
            ("", FALLBACK_NAME),
            ("..", FALLBACK_NAME),
            ("dir/", FALLBACK_NAME),
            (".hidden", "hidden.json"),
        ] {
            assert_eq!(reduce_name(given), reduced, "{given:?}");
        }
    }

    #[test]
    fn an_export_is_written_and_one_over_the_cap_is_refused() {
        let dir = temp("export");
        let path = dir.join("p.json");
        write_export(&path, "{}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
        let big = "x".repeat(MAX_TEXT_BYTES + 1);
        assert!(write_export(&dir.join("big.json"), &big).is_err());
        assert!(!dir.join("big.json").exists());
        assert!(write_export(&dir.join("no/such/dir.json"), "{}").is_err());
    }

    #[test]
    fn an_import_is_read_at_most_to_the_cap() {
        let dir = temp("import");
        let ok = dir.join("ok.json");
        std::fs::write(&ok, "{\"version\":1}").unwrap();
        assert_eq!(
            read_import(&ok),
            ImportAnswer::Text("{\"version\":1}".into())
        );
        let exact = dir.join("exact.json");
        std::fs::write(&exact, " ".repeat(MAX_TEXT_BYTES)).unwrap();
        assert!(matches!(read_import(&exact), ImportAnswer::Text(t) if t.len() == MAX_TEXT_BYTES));
        let big = dir.join("big.json");
        std::fs::write(&big, " ".repeat(MAX_TEXT_BYTES + 1)).unwrap();
        assert!(matches!(read_import(&big), ImportAnswer::Failed(_)));
        let binary = dir.join("bin.json");
        std::fs::write(&binary, [0xff, 0xfe, 0x00]).unwrap();
        assert!(matches!(read_import(&binary), ImportAnswer::Failed(_)));
        assert!(matches!(
            read_import(&dir.join("absent.json")),
            ImportAnswer::Failed(_)
        ));
    }
}

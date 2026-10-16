//! The app's own strings, by interface language. ui-lib carries its own
//! (the rail's Settings, the language names); these are the ones the app adds.

use palicanon_ui_lib::Language;

/// The app's strings in one language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Strings {
    /// The rail entry of the map.
    pub nav_map: &'static str,
    /// The page of an address the app does not know.
    pub not_found: &'static str,
    /// The footer's links.
    pub footer_acknowledgements: &'static str,
    /// The footer's links.
    pub footer_terms: &'static str,
    /// The footer's links.
    pub footer_privacy: &'static str,
    /// The footer's links.
    pub footer_support: &'static str,
}

/// The strings of `language`.
pub fn of(language: Language) -> Strings {
    match language {
        Language::En => Strings {
            nav_map: "Map",
            not_found: "Not found",
            footer_acknowledgements: "Acknowledgements",
            footer_terms: "Terms",
            footer_privacy: "Privacy",
            footer_support: "Support",
        },
        Language::Fr => Strings {
            nav_map: "Carte",
            not_found: "Introuvable",
            footer_acknowledgements: "Crédits",
            footer_terms: "Conditions",
            footer_privacy: "Confidentialité",
            footer_support: "Soutenir",
        },
        Language::Vi => Strings {
            nav_map: "Bản đồ",
            not_found: "Không tìm thấy",
            footer_acknowledgements: "Tri ân",
            footer_terms: "Điều khoản",
            footer_privacy: "Quyền riêng tư",
            footer_support: "Ủng hộ",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_every_string_and_they_differ() {
        let all: Vec<Strings> = Language::ALL.into_iter().map(of).collect();
        for s in &all {
            for text in [
                s.nav_map,
                s.not_found,
                s.footer_acknowledgements,
                s.footer_terms,
                s.footer_privacy,
                s.footer_support,
            ] {
                assert_ne!(text.trim().len(), 0);
            }
        }
        assert_eq!(all[0].nav_map, "Map");
        assert_ne!(all[0], all[1]);
        assert_ne!(all[1], all[2]);
        assert_ne!(all[0], all[2]);
    }
}

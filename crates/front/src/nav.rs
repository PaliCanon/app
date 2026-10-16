//! The rail's entries and the footer's links, as data, by language.

use palicanon_ui_lib::{FooterLink, IconName, Language, NavEntry, NavSection};

use crate::strings;

/// The rail: the Map alone for now (Learn and Search join it with task 005,
/// and `/` then goes to Learn).
pub fn sections(language: Language) -> Vec<NavSection> {
    vec![NavSection {
        title: None,
        entries: vec![NavEntry {
            label: strings::of(language).nav_map,
            // The globe stands in for a map icon until ui-lib has one.
            icon: IconName::Globe,
            href: "/map",
        }],
    }]
}

/// The footer's links: the main site's pages, which open in the system
/// browser (the desktop shell's navigation rule).
pub fn footer_links(language: Language) -> Vec<FooterLink> {
    let t = strings::of(language);
    vec![
        FooterLink {
            label: t.footer_acknowledgements,
            href: "https://palicanon.app/acknowledgements",
        },
        FooterLink {
            label: t.footer_terms,
            href: "https://palicanon.app/terms",
        },
        FooterLink {
            label: t.footer_privacy,
            href: "https://palicanon.app/privacy",
        },
        FooterLink {
            label: t.footer_support,
            href: "https://palicanon.app/support",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use palicanon_ui_lib::IconName;

    #[test]
    fn the_rail_lists_the_map_alone() {
        for language in Language::ALL {
            let sections = sections(language);
            assert_eq!(sections.len(), 1);
            let entries = &sections[0].entries;
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].href, "/map");
            assert_eq!(entries[0].icon, IconName::Globe);
        }
    }

    #[test]
    fn the_map_entry_is_labelled_in_every_language() {
        for (language, label) in [
            (Language::En, "Map"),
            (Language::Fr, "Carte"),
            (Language::Vi, "Bản đồ"),
        ] {
            assert_eq!(sections(language)[0].entries[0].label, label);
        }
    }

    #[test]
    fn the_footer_links_are_the_main_sites_pages_over_https() {
        for language in Language::ALL {
            let links = footer_links(language);
            let hrefs: Vec<&str> = links.iter().map(|l| l.href).collect();
            assert_eq!(
                hrefs,
                [
                    "https://palicanon.app/acknowledgements",
                    "https://palicanon.app/terms",
                    "https://palicanon.app/privacy",
                    "https://palicanon.app/support",
                ]
            );
        }
    }

    #[test]
    fn the_footer_links_are_labelled_in_the_language() {
        let fr: Vec<&str> = footer_links(Language::Fr).iter().map(|l| l.label).collect();
        assert_eq!(fr, ["Crédits", "Conditions", "Confidentialité", "Soutenir"]);
    }
}

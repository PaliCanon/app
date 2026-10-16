//! The rail's entries and the footer's links, as data, by language.

use palicanon_ui_lib::{FooterLink, IconName, Language, NavEntry, NavSection};

use crate::strings;

/// The rail: the app's three services, Learn, Search and the Map, each a
/// route of the app (the labels are Learn's and Search's own words).
pub fn sections(language: Language) -> Vec<NavSection> {
    let t = strings::of(language);
    vec![NavSection {
        title: None,
        entries: vec![
            // `Grid` stands in for Learn until ui-lib has a Learn icon (as
            // learn-pali's own rail does).
            NavEntry {
                label: t.nav_learn,
                icon: IconName::Grid,
                href: "/learn",
            },
            NavEntry {
                label: t.nav_search,
                icon: IconName::Search,
                href: "/search",
            },
            // The globe stands in for a map icon until ui-lib has one.
            NavEntry {
                label: t.nav_map,
                icon: IconName::Globe,
                href: "/map",
            },
        ],
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

    /// One rail, the three services in this order: Learn, Search, the Map
    /// (user, 2026-10-16: «Ajoute la carte»), each a route of the app.
    #[test]
    fn the_rail_lists_learn_search_and_the_map() {
        for language in Language::ALL {
            let sections = sections(language);
            assert_eq!(sections.len(), 1);
            let entries: Vec<(&str, IconName)> = sections[0]
                .entries
                .iter()
                .map(|e| (e.href, e.icon))
                .collect();
            assert_eq!(
                entries,
                [
                    ("/learn", IconName::Grid),
                    ("/search", IconName::Search),
                    ("/map", IconName::Globe),
                ]
            );
        }
    }

    #[test]
    fn the_entries_are_labelled_in_every_language() {
        for (language, labels) in [
            (Language::En, ["Learn", "Search", "Map"]),
            (Language::Fr, ["Apprendre", "Recherche", "Carte"]),
            (Language::Vi, ["Học", "Tìm kiếm", "Bản đồ"]),
        ] {
            let got: Vec<&str> = sections(language)[0]
                .entries
                .iter()
                .map(|e| e.label)
                .collect();
            assert_eq!(got, labels, "{language:?}");
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

//! The front's real links, run through the navigation rule: the two crates
//! agree on what stays in the window and what goes to the system browser.

use palicanon_app_desktop::navigation::{Navigation, decide};
use palicanon_app_front::nav;
use palicanon_ui_lib::Language;
use url::Url;

#[test]
fn every_footer_link_opens_in_the_system_browser() {
    for language in Language::ALL {
        let links = nav::footer_links(language);
        assert_ne!(links.len(), 0);
        for link in links {
            let url = Url::parse(link.href).expect("an absolute address");
            assert_eq!(decide(&url), Navigation::OpenOutside, "{}", link.href);
        }
    }
}

#[test]
fn every_rail_entry_stays_in_the_window() {
    let origin = Url::parse("tauri://localhost").unwrap();
    for language in Language::ALL {
        for section in nav::sections(language) {
            assert_ne!(section.entries.len(), 0);
            for entry in section.entries {
                let url = origin.join(entry.href).expect("a path");
                assert_eq!(decide(&url), Navigation::Stay, "{}", entry.href);
            }
        }
    }
}

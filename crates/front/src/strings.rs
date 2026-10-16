//! The app's own strings, by interface language. ui-lib carries its own
//! (the rail's Settings, the language names); these are the ones the app adds.

use palicanon_ui_lib::Language;

/// The app's strings in one language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Strings {
    /// The rail entry of Learn.
    pub nav_learn: &'static str,
    /// The rail entry of Search.
    pub nav_search: &'static str,
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
    /// The sign-in link's title.
    pub sync_link_title: &'static str,
    /// What the sign-in link is for.
    pub sync_link_explain: &'static str,
    /// The sign-in link's Copy.
    pub sync_link_copy: &'static str,
    /// The sign-in link's Copy, once done.
    pub sync_link_copied: &'static str,
    /// The sign-in link's Open in browser.
    pub sync_link_open: &'static str,
    /// The sign-in link's Cancel.
    pub sync_link_cancel: &'static str,
}

/// The strings of `language`.
pub fn of(language: Language) -> Strings {
    match language {
        Language::En => Strings {
            nav_learn: "Learn",
            nav_search: "Search",
            nav_map: "Map",
            not_found: "Not found",
            footer_acknowledgements: "Acknowledgements",
            footer_terms: "Terms",
            footer_privacy: "Privacy",
            footer_support: "Support",
            sync_link_title: "Sign in to Google",
            sync_link_explain: "Your browser opened Google's sign-in page. If it did not, open this link or copy it into your browser. PaliCanon waits here until you sign in.",
            sync_link_copy: "Copy link",
            sync_link_copied: "Copied",
            sync_link_open: "Open in browser",
            sync_link_cancel: "Cancel",
        },
        Language::Fr => Strings {
            nav_learn: "Apprendre",
            nav_search: "Recherche",
            nav_map: "Carte",
            not_found: "Introuvable",
            footer_acknowledgements: "Crédits",
            footer_terms: "Conditions",
            footer_privacy: "Confidentialité",
            footer_support: "Soutenir",
            sync_link_title: "Se connecter à Google",
            sync_link_explain: "Votre navigateur a ouvert la page de connexion de Google. Sinon, ouvrez ce lien ou copiez-le dans votre navigateur. PaliCanon attend ici que vous soyez connecté.",
            sync_link_copy: "Copier le lien",
            sync_link_copied: "Copié",
            sync_link_open: "Ouvrir dans le navigateur",
            sync_link_cancel: "Annuler",
        },
        Language::Vi => Strings {
            nav_learn: "Học",
            nav_search: "Tìm kiếm",
            nav_map: "Bản đồ",
            not_found: "Không tìm thấy",
            footer_acknowledgements: "Tri ân",
            footer_terms: "Điều khoản",
            footer_privacy: "Quyền riêng tư",
            footer_support: "Ủng hộ",
            sync_link_title: "Đăng nhập Google",
            sync_link_explain: "Trình duyệt đã mở trang đăng nhập của Google. Nếu chưa, hãy mở liên kết này hoặc sao chép vào trình duyệt. PaliCanon chờ ở đây cho đến khi bạn đăng nhập.",
            sync_link_copy: "Sao chép liên kết",
            sync_link_copied: "Đã sao chép",
            sync_link_open: "Mở trong trình duyệt",
            sync_link_cancel: "Hủy",
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
                s.nav_learn,
                s.nav_search,
                s.nav_map,
                s.not_found,
                s.footer_acknowledgements,
                s.footer_terms,
                s.footer_privacy,
                s.footer_support,
                s.sync_link_title,
                s.sync_link_explain,
                s.sync_link_copy,
                s.sync_link_copied,
                s.sync_link_open,
                s.sync_link_cancel,
            ] {
                assert!(!text.contains('\u{2014}'), "no em dash: {text}");
                assert_ne!(text.trim().len(), 0);
            }
        }
        assert_eq!(all[0].nav_map, "Map");
        assert_ne!(all[0], all[1]);
        assert_ne!(all[1], all[2]);
        assert_ne!(all[0], all[2]);
    }
}

//! What a second launch does: the single-instance plugin tells the running
//! app, which brings its window forward. The case where the window is not
//! there (not created yet, or closed) is an explicit branch: logged, then
//! ignored, never a panic and never a second window.

/// What the running app did about a second launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondLaunch {
    /// The main window was brought forward.
    Focused,
    /// There was no main window: logged and ignored.
    NoWindow,
}

/// Handles a second launch: `main` is the main window if there is one,
/// `bring_forward` unminimizes, shows and focuses it.
pub fn handle_second_launch<W>(main: Option<W>, bring_forward: impl FnOnce(W)) -> SecondLaunch {
    let Some(window) = main else {
        log::warn!("second launch ignored: there is no main window to bring forward");
        return SecondLaunch::NoWindow;
    };
    bring_forward(window);
    SecondLaunch::Focused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_present_window_is_brought_forward_once() {
        let mut calls = Vec::new();
        let outcome = handle_second_launch(Some("main"), |w| calls.push(w));
        assert_eq!(outcome, SecondLaunch::Focused);
        assert_eq!(calls, ["main"]);
    }

    #[test]
    fn an_absent_window_is_ignored_without_touching_anything() {
        let outcome = handle_second_launch(None::<&str>, |w| panic!("brought {w} forward"));
        assert_eq!(outcome, SecondLaunch::NoWindow);
    }
}

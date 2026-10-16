//! The xtask binary's exit code: Trunk's `pre_build` hooks run it, and a
//! build must stop when a step fails.

use std::process::{Command, Output};

fn xtask(task: &str, tailwind: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    command.arg(task);
    if let Some(tailwind) = tailwind {
        command.env("TAILWIND", tailwind);
    }
    command.output().expect("run xtask")
}

#[test]
fn a_failing_tailwind_makes_css_exit_non_zero() {
    let out = xtask("css", Some("/usr/bin/false"));
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("xtask css:"));
}

#[test]
fn a_missing_tailwind_makes_css_exit_non_zero() {
    let out = xtask("css", Some("/nonexistent/tailwindcss"));
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn an_unknown_task_exits_non_zero() {
    let out = xtask("nope", None);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown task"));
}

#[test]
fn pins_and_data_exit_zero_on_this_workspace() {
    for task in ["pins", "data"] {
        let out = xtask(task, None);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{task}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

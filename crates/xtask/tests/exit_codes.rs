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

/// The process environment cargo needs to find its checkouts, kept when a test
/// changes `HOME`.
fn cargo_environment() -> Vec<(&'static str, std::ffi::OsString)> {
    let home = std::env::var_os("HOME").unwrap_or_default();
    let cargo_home = std::env::var_os("CARGO_HOME")
        .unwrap_or_else(|| std::path::Path::new(&home).join(".cargo").into());
    let rustup_home = std::env::var_os("RUSTUP_HOME")
        .unwrap_or_else(|| std::path::Path::new(&home).join(".rustup").into());
    vec![("CARGO_HOME", cargo_home), ("RUSTUP_HOME", rustup_home)]
}

#[test]
fn no_tailwind_at_all_makes_css_exit_non_zero() {
    let empty_home = std::env::temp_dir().join(format!("xtask-no-home-{}", std::process::id()));
    std::fs::create_dir_all(&empty_home).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("css")
        .env_remove("TAILWIND")
        .env("HOME", &empty_home)
        .envs(cargo_environment())
        .output()
        .expect("run xtask");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no Tailwind CLI"));
}

#[test]
fn a_data_step_that_cannot_write_exits_non_zero() {
    // The staging directory's parent is a regular file.
    let blocker = std::env::temp_dir().join(format!("xtask-blocker-{}", std::process::id()));
    std::fs::write(&blocker, "x").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("data")
        .env("XTASK_DATA_OUT", blocker.join("map-data"))
        .output()
        .expect("run xtask");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("xtask data:"));
}

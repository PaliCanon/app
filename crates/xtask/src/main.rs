//! `cargo run -p xtask -- <task>`: the app's build tasks (see the library).
//! `pins` checks the lockstep and prints the checkout directories.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = match task.as_str() {
        "pins" => xtask::pins::of_workspace(&root).map(|pins| {
            println!("ui_lib={}", pins.ui_lib.display());
            if let Some(map) = pins.map {
                println!("map={}", map.display());
            }
        }),
        other => {
            eprintln!("xtask: unknown task {other:?} (pins)");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask {task}: {e}");
            ExitCode::FAILURE
        }
    }
}

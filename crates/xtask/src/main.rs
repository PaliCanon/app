//! `cargo xtask <task>` (alias in `.cargo/config.toml`): the app's build
//! tasks. `pins` checks the lockstep and prints the checkout directories;
//! `css` builds the Tailwind CSS into `target/css/app.css`; `data` stages the
//! map's data files into `target/map-data` (Trunk's `pre_build` hooks run the
//! last two).

use std::error::Error;
use std::path::Path;
use std::process::ExitCode;

use xtask::{css, data, pins};

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().unwrap_or(root);
    match run(&task, &root) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask {task}: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(task: &str, root: &Path) -> Result<(), Box<dyn Error>> {
    let pins = pins::of_workspace(root)?;
    match task {
        "pins" => {
            println!("ui_lib={}", pins.ui_lib.display());
            if let Some(map) = &pins.map {
                println!("map={}", map.display());
            }
        }
        "css" => println!("{}", css::build(root, &pins)?.display()),
        "data" => match data::stage(pins.map.as_deref(), &root.join("target/map-data"))? {
            data::Staged::Skipped => {
                println!("xtask data: skipped, the front does not depend on the map crate yet");
            }
            data::Staged::Copied(n) => println!("xtask data: {n} files in target/map-data"),
        },
        other => return Err(format!("unknown task {other:?} (pins, css, data)").into()),
    }
    Ok(())
}

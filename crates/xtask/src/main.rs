//! `cargo xtask <task>` (alias in `.cargo/config.toml`): the app's build
//! tasks, see the library. A task that fails exits non-zero with its reason
//! on stderr (Trunk's `pre_build` hooks run `css` and `data`, and stop the
//! build on a failure).

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().unwrap_or(root);
    let result = xtask::pins::of_workspace(&root)
        .map_err(Into::into)
        .and_then(|pins| xtask::run(&task, &root, &pins, xtask::css::tailwind_binary));
    match result {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("xtask {task}: {e}");
            ExitCode::FAILURE
        }
    }
}

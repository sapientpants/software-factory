//! Developer automation (SPEC 19.1). Run as `cargo xtask <task>`.

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1) {
        Some(task) => eprintln!("xtask: unknown task `{task}`; no tasks are defined yet"),
        None => eprintln!("usage: cargo xtask <task>; no tasks are defined yet"),
    }
    ExitCode::from(2)
}

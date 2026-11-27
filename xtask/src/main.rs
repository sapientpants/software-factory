//! Developer automation (SPEC 19.1). Run as `cargo xtask <task>`.

mod check;
mod fetch;
mod hook_skip;
mod pre_tool_use;
mod vm;

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("check") => check::main(),
        Some("fetch") => fetch::main(),
        Some("pre-tool-use") => pre_tool_use::main(),
        Some(task) => {
            eprintln!("xtask: unknown task `{task}`");
            ExitCode::FAILURE
        }
        None => {
            eprintln!("usage: cargo xtask <check | fetch | pre-tool-use>");
            ExitCode::FAILURE
        }
    }
}

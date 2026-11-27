//! The supervisor shim of AG-16, which `factory-cli` reaches here as the
//! hidden subcommand `sf __supervise` (19.1).

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use factory_proc::Supervisor;
use factory_proc::supervise::{FAILED, Supervise};

/// The name of the hidden subcommand.
pub const SUBCOMMAND: &str = "__supervise";

/// The shim of the binary `sf` at `sf`, which a command is started through.
pub fn supervisor(sf: impl Into<PathBuf>) -> Supervisor {
    Supervisor::new(sf, [SUBCOMMAND])
}

/// Runs the shim with the arguments that follow the subcommand.
pub fn run(args: Vec<OsString>) -> ExitCode {
    match Supervise::parse(args) {
        Ok(supervise) => supervise.run(),
        Err(message) => {
            eprintln!("sf {SUBCOMMAND}: {message}");
            ExitCode::from(FAILED)
        }
    }
}

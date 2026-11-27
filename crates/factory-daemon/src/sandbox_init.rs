//! The first process of a sandbox at level `process` (SEC-10a), which
//! `factory-cli` reaches here as the hidden subcommand `sf __sandbox-init`
//! (19.1). The bubblewrap backend is compiled for Linux alone (4.4), and so
//! is the first process.

use std::ffi::OsString;
use std::process::ExitCode;

use factory_proc::supervise::FAILED;

/// The name of the hidden subcommand, which the backend starts.
#[cfg(target_os = "linux")]
pub use factory_sandbox::bubblewrap::init::SUBCOMMAND;

/// The name of the hidden subcommand.
#[cfg(not(target_os = "linux"))]
pub const SUBCOMMAND: &str = "__sandbox-init";

/// Runs the first process with the arguments that follow the subcommand.
/// Off Linux there is none, and it fails.
pub fn run(args: Vec<OsString>) -> ExitCode {
    #[cfg(target_os = "linux")]
    let refused = match factory_sandbox::bubblewrap::init::Init::parse(args) {
        Ok(init) => return init.run(),
        Err(message) => message,
    };
    #[cfg(not(target_os = "linux"))]
    let refused = {
        let _ = args;
        "the first process of a sandbox runs on Linux alone".to_owned()
    };
    eprintln!("sf {SUBCOMMAND}: {refused}");
    ExitCode::from(FAILED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_command_it_starts_nothing_and_fails() {
        assert_eq!(run(Vec::new()), ExitCode::from(FAILED));
    }
}

//! The `sf` binary (SPEC 17, 19.1).

use std::ffi::OsString;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "sf", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// The supervisor shim (AG-16), which the daemon starts every sandboxed
    /// command through.
    #[command(name = "__supervise", hide = true, disable_help_flag = true)]
    Supervise {
        /// `--pipe-fd <fd> -- <program> [<arg>...]`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 0..)]
        args: Vec<OsString>,
    },
    /// The first process of a sandbox at level `process` (SEC-10a), which
    /// bubblewrap starts as PID 1 of the sandbox.
    #[command(name = "__sandbox-init", hide = true, disable_help_flag = true)]
    SandboxInit {
        /// `<program> [<arg>...]`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 0..)]
        args: Vec<OsString>,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        None => {
            println!("sf {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(Command::Supervise { args }) => factory_daemon::supervise::run(args),
        Some(Command::SandboxInit { args }) => factory_daemon::sandbox_init::run(args),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(words: &[&str]) -> Option<Command> {
        Cli::try_parse_from(words).ok().and_then(|cli| cli.command)
    }

    #[test]
    fn the_hidden_subcommands_get_their_arguments_whole() {
        let words = |words: &[&str]| words.iter().map(OsString::from).collect::<Vec<_>>();
        let init = ["/bin/sh", "-c", "exit 3", "--", "--help", "-V"];
        let supervise = ["--pipe-fd", "3", "--report-fd", "4", "--", "/bin/sh"];

        match parsed(&[&["sf", "__sandbox-init"][..], &init].concat()) {
            Some(Command::SandboxInit { args }) => assert_eq!(args, words(&init)),
            other => panic!("{other:?}"),
        }
        match parsed(&[&["sf", "__supervise"][..], &supervise].concat()) {
            Some(Command::Supervise { args }) => assert_eq!(args, words(&supervise)),
            other => panic!("{other:?}"),
        }
    }
}

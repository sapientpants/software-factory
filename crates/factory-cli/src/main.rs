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
}

fn main() -> ExitCode {
    match Cli::parse().command {
        None => {
            println!("sf {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(Command::Supervise { args }) => factory_daemon::supervise::run(args),
    }
}

//! The `sf` binary (SPEC 17, 19.1).

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use factory_daemon::sandbox_exec::{self, Request};

#[derive(Debug, Parser)]
#[command(name = "sf", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Sandbox instances outside any run (CLI-6).
    Sandbox {
        #[command(subcommand)]
        command: SandboxCommand,
    },
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

#[derive(Debug, Subcommand)]
enum SandboxCommand {
    /// Runs one command line in one sandbox instance at the configured level,
    /// as a setup or a verify command is run, and reports the wall-clock
    /// time, the size of the target directory, the proxy's denied-request log
    /// and the exit status, with which it exits (CLI-6).
    Exec(Exec),
}

#[derive(Debug, Args)]
struct Exec {
    /// The directory that the instance shows read-write as the working
    /// directory.
    #[arg(long, value_name = "path")]
    dir: PathBuf,
    /// A target directory to use in place of a new, empty one; it is left as
    /// it is.
    #[arg(long, value_name = "dir")]
    target: Option<PathBuf>,
    /// A check tool to put on the PATH, found in the tools directory (WS-7a).
    #[arg(long = "tool", value_name = "name>=<version", value_parser = sandbox_exec::parse_tool)]
    tools: Vec<sandbox_exec::CheckTool>,
    /// The report as JSON.
    #[arg(long)]
    json: bool,
    /// The command line: the program and its arguments.
    #[arg(last = true, required = true, value_name = "command")]
    command: Vec<OsString>,
}

impl Exec {
    fn request(self) -> Request {
        let mut command = self.command.into_iter();
        Request {
            dir: self.dir,
            target: self.target,
            tools: self.tools,
            program: command.next().unwrap_or_default(),
            args: command.collect(),
        }
    }
}

fn main() -> ExitCode {
    match Cli::parse().command {
        None => {
            println!("sf {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(Command::Sandbox {
            command: SandboxCommand::Exec(exec),
        }) => {
            let json = exec.json;
            sandbox_exec::main(&exec.request(), json)
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

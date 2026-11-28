//! The `sf` binary (SPEC 17, 19.1).

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use factory_daemon::sandbox_exec::{self, Form, Request};

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
    /// and the exit status, with which it exits. With `--model` and
    /// `--agent` it runs one agent session instead, given the prompt as its
    /// one message, and reports the subtype of the stream's `result` line and
    /// the uses of the Bash tool; it exits with 0 only where that subtype is
    /// `success` (CLI-6).
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
    #[arg(long, value_name = "dir", conflicts_with = "agent")]
    target: Option<PathBuf>,
    /// A check tool to put on the PATH, found in the tools directory (WS-7a).
    #[arg(
        long = "tool",
        value_name = "name>=<version",
        value_parser = sandbox_exec::parse_tool,
        conflicts_with = "agent"
    )]
    tools: Vec<sandbox_exec::CheckTool>,
    /// The model of the agent session.
    #[arg(long, value_name = "id", requires = "agent")]
    model: Option<String>,
    /// Runs one agent session, given this prompt as its one message, in
    /// place of a command line.
    #[arg(
        long,
        value_name = "prompt",
        requires = "model",
        conflicts_with = "command"
    )]
    agent: Option<String>,
    /// The report as JSON.
    #[arg(long)]
    json: bool,
    /// The command line: the program and its arguments.
    #[arg(last = true, required_unless_present = "agent", value_name = "command")]
    command: Vec<OsString>,
}

impl Exec {
    fn request(self) -> Request {
        let form = match (self.model, self.agent) {
            (Some(model), Some(prompt)) => Form::Agent { model, prompt },
            _ => {
                let mut command = self.command.into_iter();
                Form::Command {
                    program: command.next().unwrap_or_default(),
                    args: command.collect(),
                }
            }
        };
        Request {
            dir: self.dir,
            target: self.target,
            tools: self.tools,
            form,
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

    fn request(words: &[&str]) -> Option<Request> {
        match parsed(&[&["sf", "sandbox", "exec", "--dir", "/w"][..], words].concat()) {
            Some(Command::Sandbox {
                command: SandboxCommand::Exec(exec),
            }) => Some(exec.request()),
            _ => None,
        }
    }

    #[test]
    fn the_agent_form_takes_a_model_and_a_prompt_and_no_command_line() {
        let agent = request(&["--model", "claude-opus-5-5", "--agent", "run uname"]).unwrap();
        assert_eq!(
            agent.form,
            Form::Agent {
                model: "claude-opus-5-5".to_owned(),
                prompt: "run uname".to_owned()
            }
        );
        assert_eq!(agent.dir, PathBuf::from("/w"));

        let command = request(&["--", "make", "all"]).unwrap();
        assert_eq!(
            command.form,
            Form::Command {
                program: "make".into(),
                args: vec!["all".into()]
            }
        );

        for refused in [
            &["--agent", "p"][..],
            &["--model", "m"],
            &["--model", "m", "--agent", "p", "--", "true"],
            &["--model", "m", "--agent", "p", "--target", "/t"],
            &["--model", "m", "--agent", "p", "--tool", "deny=0.22.1"],
            &[],
        ] {
            assert!(request(refused).is_none(), "{refused:?}");
        }
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

//! The `PreToolUse` hook of agent sessions on this repository (SPEC DEV-10).
//!
//! Claude Code writes the event to stdin before a Bash tool call. Exit code 2
//! refuses the call and hands stderr to the session as the reason; exit code
//! 0 lets the call go on.

use std::io::Read;
use std::process::ExitCode;

use serde_json::Value;

use crate::hook_skip::hook_skip;

/// Exit code 2 is the one Claude Code reads as a refusal.
const REFUSE: u8 = 2;

pub fn main() -> ExitCode {
    let mut event = String::new();
    if let Err(error) = std::io::stdin().read_to_string(&mut event) {
        eprintln!("xtask pre-tool-use: refused, the event could not be read: {error}");
        return ExitCode::from(REFUSE);
    }
    match refusal(&event) {
        Some(reason) => {
            eprintln!("xtask pre-tool-use: refused. {reason}");
            ExitCode::from(REFUSE)
        }
        None => ExitCode::SUCCESS,
    }
}

/// Why the tool call of `event` is refused, if it is. An event without a
/// readable command is refused too: a hook that cannot see the command would
/// otherwise let every command through unseen.
fn refusal(event: &str) -> Option<String> {
    let event: Value = match serde_json::from_str(event) {
        Ok(event) => event,
        Err(error) => return Some(format!("The event is not JSON: {error}")),
    };
    let Some(command) = event["tool_input"]["command"].as_str() else {
        return Some("The event holds no `tool_input.command`.".to_owned());
    };
    let skip = hook_skip(command)?;
    Some(format!(
        "{skip}, which skips the git hooks of this repository (SPEC DEV-10). \
         Run `cargo xtask check`, fix what it reports, and run the command without that."
    ))
}

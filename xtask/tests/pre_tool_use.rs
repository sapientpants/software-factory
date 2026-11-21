//! `cargo xtask pre-tool-use` as Claude Code runs it (SPEC DEV-10): a
//! `PreToolUse` event on stdin, and exit code 2 with a reason on stderr to
//! refuse the tool call.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use serde_json::json;

/// Runs the task with `event` on its stdin.
fn pre_tool_use(event: &str) -> Output {
    let mut task = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("pre-tool-use")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start xtask");
    task.stdin
        .take()
        .expect("stdin of xtask")
        .write_all(event.as_bytes())
        .expect("write the event");
    task.wait_with_output().expect("wait for xtask")
}

/// The event Claude Code sends before a Bash tool call that runs `command`.
fn bash_event(command: &str) -> String {
    json!({
        "session_id": "00000000-0000-0000-0000-000000000000",
        "transcript_path": "/tmp/transcript.jsonl",
        "cwd": "/tmp",
        "permission_mode": "default",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": command, "description": "a command" },
        "tool_use_id": "toolu_0",
    })
    .to_string()
}

#[test]
fn a_commit_that_skips_the_hooks_is_refused_with_a_reason() {
    let output = pre_tool_use(&bash_event("git add . && git commit --no-verify -m x"));

    assert_eq!(output.status.code(), Some(2));
    let reason = String::from_utf8_lossy(&output.stderr);
    assert!(reason.contains("git commit --no-verify"), "{reason}");
    assert!(reason.contains("cargo xtask check"), "{reason}");
}

#[test]
fn a_command_that_skips_nothing_passes_in_silence() {
    let output = pre_tool_use(&bash_event("git commit -m 'never pass --no-verify'"));

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
}

#[test]
fn a_hooks_path_on_the_command_line_is_refused() {
    let output = pre_tool_use(&bash_event("git -c core.hooksPath=/dev/null commit -m x"));

    assert_eq!(output.status.code(), Some(2));
    let reason = String::from_utf8_lossy(&output.stderr);
    assert!(reason.contains("core.hooksPath"), "{reason}");
}

#[test]
fn an_event_without_a_readable_command_is_refused() {
    for event in [
        "",
        "not json",
        r#"{"tool_name":"Bash"}"#,
        r#"{"tool_name":"Bash","tool_input":{"command":7}}"#,
    ] {
        let output = pre_tool_use(event);

        assert_eq!(output.status.code(), Some(2), "{event}");
        assert!(!output.stderr.is_empty(), "{event}");
    }
}

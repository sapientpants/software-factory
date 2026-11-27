//! What the tests of `factory-proc` share.

#![allow(dead_code, reason = "each test file uses a part of it")]

use std::future::Future;
use std::time::Duration;

use factory_proc::{Command, Line, Process};
use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;
use tokio::time::Instant;

/// The longest that a test may take. A test that hangs fails at it, and so
/// fails in the mutation step (DEV-2a) rather than time out there.
pub const LIMIT: Duration = Duration::from_secs(30);

/// Runs `test` on a runtime of its own, and fails it after [`LIMIT`].
pub fn within(test: impl Future<Output = ()>) {
    let runtime = tokio::runtime::Runtime::new().unwrap_or_else(|error| panic!("{error}"));
    runtime.block_on(async {
        tokio::time::timeout(LIMIT, test)
            .await
            .unwrap_or_else(|_| panic!("the test took more than {LIMIT:?}"));
    });
}

/// A command for `/bin/sh -c script`.
pub fn sh(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(script);
    command
}

/// `command`, started with stderr discarded.
pub async fn spawn(command: &Command) -> Process {
    command
        .spawn(tokio::io::sink())
        .await
        .unwrap_or_else(|error| panic!("{error}"))
}

/// The next line of `process`, which is text.
pub async fn text(process: &mut Process) -> String {
    match process
        .next_line()
        .await
        .unwrap_or_else(|error| panic!("{error}"))
    {
        Some(Line::Text(text)) => text,
        other => panic!("not a line of text: {other:?}"),
    }
}

/// The pids on a line of text.
pub fn pids(line: &str) -> Vec<i32> {
    line.split_whitespace()
        .map(|pid| pid.parse().unwrap_or_else(|error| panic!("{pid}: {error}")))
        .collect()
}

/// Whether the process `pid` has gone within 5 s: a process whose parent
/// died is reaped by the system's init, which takes a moment.
pub async fn gone(pid: i32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if kill(Pid::from_raw(pid), None) == Err(Errno::ESRCH) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

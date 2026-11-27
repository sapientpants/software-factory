//! The stop sequence of SPEC AG-12.

mod common;

use std::time::Duration;

use common::within;

use factory_proc::{Command, Line, Process, StopTimes, Stopped, StoppedBy};
use tokio::time::Instant;

/// Short times, so that each step of the sequence shows in a test that takes
/// well under a second.
const SHORT: StopTimes = StopTimes {
    interrupt: Duration::from_millis(300),
    terminate: Duration::from_millis(300),
};

/// `/bin/sh -c script`, read up to its first line, which says that its traps
/// are set.
async fn started(script: &str) -> Process {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(script).stdin_piped();
    let mut process = command
        .spawn(tokio::io::sink())
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        process
            .next_line()
            .await
            .unwrap_or_else(|error| panic!("{error}")),
        Some(Line::Text("ready".to_owned()))
    );
    process
}

/// Stops `process` and says how long the sequence took.
async fn stop(
    mut process: Process,
    times: StopTimes,
    result: impl FnMut(&Line) -> bool,
) -> (Stopped, Duration) {
    let started = Instant::now();
    let stopped = process
        .stop(times, result)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    (stopped, started.elapsed())
}

const IGNORES_SIGINT: &str = "trap '' INT; echo ready; /bin/sleep 30";
const IGNORES_SIGINT_AND_SIGTERM: &str = "trap '' INT TERM; echo ready; /bin/sleep 30";

#[test]
fn the_times_are_those_of_ag_12() {
    assert_eq!(StopTimes::default().interrupt, Duration::from_secs(10));
    assert_eq!(StopTimes::default().terminate, Duration::from_secs(5));
}

#[test]
fn a_command_that_ends_on_sigint_is_stopped_by_it() {
    within(async {
        let process = started("echo ready; exec /bin/sleep 30").await;
        let (stopped, elapsed) = stop(process, SHORT, |_| false).await;
        assert_eq!(stopped.by, StoppedBy::Interrupt);
        assert!(elapsed < SHORT.interrupt, "{elapsed:?}");
    });
}

#[test]
fn a_command_that_ignores_sigint_is_stopped_by_sigterm() {
    within(async {
        let process = started(IGNORES_SIGINT).await;
        let (stopped, elapsed) = stop(process, SHORT, |_| false).await;
        assert_eq!(stopped.by, StoppedBy::Terminate);
        assert!(elapsed >= SHORT.interrupt, "{elapsed:?}");
        assert!(elapsed < SHORT.interrupt + SHORT.terminate, "{elapsed:?}");
    });
}

#[test]
fn a_command_that_ignores_sigterm_is_killed() {
    within(async {
        let process = started(IGNORES_SIGINT_AND_SIGTERM).await;
        let (stopped, elapsed) = stop(process, SHORT, |_| false).await;
        assert_eq!(stopped.by, StoppedBy::Kill);
        assert!(elapsed >= SHORT.interrupt + SHORT.terminate, "{elapsed:?}");
        assert!(
            elapsed < SHORT.interrupt + SHORT.terminate + Duration::from_millis(500),
            "{elapsed:?}"
        );
    });
}

#[test]
fn the_result_message_ends_the_wait_after_the_interrupt() {
    within(async {
        // The shell answers SIGINT with a result line and goes on, as the agent
        // CLI does; SIGTERM then ends it.
        let process =
            started("trap 'echo result' INT; echo ready; while :; do /bin/sleep 1; done").await;
        let times = StopTimes {
            interrupt: Duration::from_secs(5),
            terminate: Duration::from_secs(5),
        };
        let (stopped, elapsed) = stop(process, times, |line| {
            *line == Line::Text("result".to_owned())
        })
        .await;
        assert_eq!(stopped.by, StoppedBy::Terminate);
        assert_eq!(stopped.lines, vec![Line::Text("result".to_owned())]);
        // Well before the 5 s of the interrupt, though the drain after the exit
        // may take its 2 s where a `sleep` of the loop held stdout.
        assert!(elapsed < times.interrupt, "{elapsed:?}");
    });
}

#[test]
fn a_command_that_has_exited_is_stopped_by_itself() {
    within(async {
        let mut process = started("echo ready; exit 3").await;
        assert_eq!(process.wait().await.unwrap().code(), Some(3));
        let stopped = process.stop(SHORT, |_| false).await.unwrap();
        assert_eq!(stopped.by, StoppedBy::Itself);
        assert_eq!(stopped.status.code(), Some(3));
    });
}

#[test]
fn the_sequence_ends_both_commands_within_the_times_of_ag_12() {
    within(async {
        let times = StopTimes::AG_12;
        let (sigint, sigterm) = tokio::join!(
            async { stop(started(IGNORES_SIGINT).await, times, |_| false).await },
            async { stop(started(IGNORES_SIGINT_AND_SIGTERM).await, times, |_| false).await },
        );
        let margin = Duration::from_secs(1);
        assert_eq!(sigint.0.by, StoppedBy::Terminate);
        assert!(sigint.1 < times.interrupt + margin, "{:?}", sigint.1);
        assert_eq!(sigterm.0.by, StoppedBy::Kill);
        assert!(
            sigterm.1 < times.interrupt + times.terminate + margin,
            "{:?}",
            sigterm.1
        );
    });
}

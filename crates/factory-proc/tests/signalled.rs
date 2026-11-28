//! What the stop sequence of SPEC AG-12 signals: the command's group, or one
//! process that the program reports (SEC-10a).

mod common;

use std::time::Duration;

use common::{plain, sh, text, within};
use factory_proc::{Command, Line, Signalled, StopTimes, StoppedBy};

const SHORT: StopTimes = StopTimes {
    interrupt: Duration::from_millis(300),
    terminate: Duration::from_millis(300),
};

/// `/bin/sh -c script sh <fd>`, where `<fd>` is the descriptor on which the
/// script reports the process that the stop sequence signals.
fn reporting(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    let fd = command
        .signal_reported(plain)
        .unwrap_or_else(|error| panic!("{error}"));
    command.args(["-c", script, "sh", &fd.to_string()]);
    command
}

/// A shell that starts a child and reports it. The child prints `ready`
/// once its traps are set, so that no signal comes before them, and prints
/// `named` on SIGTERM and exits; it ignores SIGINT, as a background job does.
/// The shell, which is the leader of the group, prints `group` on SIGTERM,
/// and a SIGINT ends it.
const NAMED_CHILD: &str = r#"
    trap 'echo group' TERM
    /bin/sh -c 'trap "" INT; trap "echo named; exit 0" TERM; echo ready; while :; do /bin/sleep 0.05; done' &
    echo $! > "/dev/fd/$1"
    wait
"#;

#[test]
fn without_a_report_the_stop_sequence_signals_the_group() {
    within(async {
        let mut process = sh("echo ready; exec /bin/sleep 30")
            .spawn(tokio::io::sink())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(process.signalled(), Signalled::Group(process.group()));
        assert_eq!(text(&mut process).await, "ready");
        let stopped = process.stop(SHORT, |_| false).await.unwrap();
        assert_eq!(stopped.by, StoppedBy::Interrupt);
    });
}

#[test]
fn the_stop_sequence_signals_the_reported_process_and_not_the_group() {
    within(async {
        let mut process = reporting(NAMED_CHILD)
            .spawn(tokio::io::sink())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let Signalled::Process(named) = process.signalled() else {
            panic!("{:?}", process.signalled());
        };
        assert_ne!(named, process.group());
        assert_eq!(text(&mut process).await, "ready");

        let stopped = process.stop(SHORT, |_| false).await.unwrap();

        // The SIGINT ended neither: the leader was not signalled. The SIGTERM
        // reached the child alone, whose end ended the leader's wait.
        assert_eq!(stopped.by, StoppedBy::Terminate);
        assert_eq!(stopped.lines, [Line::Text("named".to_owned())]);
        assert_eq!(stopped.status.code(), Some(0));
    });
}

#[test]
fn a_program_that_ends_without_a_report_has_ended_by_itself() {
    within(async {
        let mut process = reporting("echo ready; exit 4")
            .spawn(tokio::io::sink())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let stopped = process.stop(SHORT, |_| false).await.unwrap();
        assert_eq!(stopped.by, StoppedBy::Itself);
        assert_eq!(stopped.status.code(), Some(4));
        assert_eq!(stopped.lines, [Line::Text("ready".to_owned())]);
    });
}

#[test]
fn a_report_that_names_no_process_starts_nothing() {
    within(async {
        for script in [
            "echo x > /dev/fd/$1; exec /bin/sleep 30",
            "echo 1 > /dev/fd/$1; exec /bin/sleep 30",
            "echo $PPID > /dev/fd/$1; exec /bin/sleep 30",
        ] {
            let spawned = reporting(script).spawn(tokio::io::sink()).await;
            assert!(spawned.is_err(), "{script}");
        }
    });
}

#[test]
fn the_report_stays_open_until_the_program_has_ended() {
    within(async {
        // A program that writes on the descriptor after its report, as
        // bubblewrap writes the exit code, is not ended by SIGPIPE.
        let mut process = reporting(
            "/bin/sleep 30 & echo $! > /dev/fd/$1; /bin/sleep 0.2; echo later > /dev/fd/$1 && echo wrote; kill $!",
        )
        .spawn(tokio::io::sink())
        .await
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(text(&mut process).await, "wrote");
        assert_eq!(process.wait().await.unwrap().code(), Some(0));
    });
}

#[test]
fn a_command_with_a_report_starts_once() {
    within(async {
        let mut command = reporting("/bin/sleep 30 & echo $! > /dev/fd/$1; wait");
        let process = command.spawn(tokio::io::sink()).await;
        assert!(process.is_ok());
        assert!(command.spawn(tokio::io::sink()).await.is_err());
    });
}

#[test]
fn a_command_with_a_report_keeps_its_stdin() {
    within(async {
        // The wait for the report must not close stdin, as a wait for the
        // leader does: the agent form of `sf sandbox exec` writes its
        // message there after the start (CLI-6).
        let mut command = reporting(
            "/bin/sleep 30 & echo $! > /dev/fd/$1; read -r line; echo \"got $line\"; kill $!",
        );
        command.stdin_piped();
        let mut process = command
            .spawn(tokio::io::sink())
            .await
            .unwrap_or_else(|error| panic!("{error}"));

        let stdin = process.stdin().expect("stdin is open");
        tokio::io::AsyncWriteExt::write_all(stdin, b"message\n")
            .await
            .unwrap();

        assert_eq!(text(&mut process).await, "got message");
    });
}

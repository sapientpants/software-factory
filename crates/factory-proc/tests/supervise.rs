//! The supervisor shim of SPEC AG-16.
//!
//! This test binary has a harness of its own, so that it can also play the
//! shim and the shim's parent: run with [`ROLE`] set, it does that and runs
//! no test. As the shim it prints nothing of its own, where libtest would
//! print its header onto the command's stdout. The harness answers `--list`
//! as nextest asks it, and runs the tests that its arguments name, or all.

#![expect(
    clippy::unwrap_used,
    reason = "a test binary whose harness is its own, which clippy does not see as test code (19.3)"
)]

mod common;

use std::ffi::OsString;
use std::process::ExitCode;
use std::time::Duration;

use common::{gone, pids, sh, spawn, text, within};
use factory_proc::supervise::{FAILED, Supervise};
use factory_proc::{Command, StopTimes, StoppedBy, Supervisor};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

/// The variable that makes this binary the shim (`shim`) or the shim's
/// parent (`parent`).
const ROLE: &str = "SF_TEST_SUPERVISE_ROLE";

/// A tree of two sleeping processes under a shell, which prints the pids of
/// the three.
const TREE: &str = "/bin/sleep 300 & a=$!; /bin/sleep 300 & b=$!; echo \"$$ $a $b\"; wait";

const SHORT: StopTimes = StopTimes {
    interrupt: Duration::from_millis(200),
    terminate: Duration::from_millis(200),
};

/// `/bin/sh -c script`, started through this binary as the shim.
fn supervised(script: &str) -> Command {
    let mut command = sh(script);
    command
        .supervised_by(Supervisor::new(
            std::env::current_exe().unwrap(),
            Vec::<OsString>::new(),
        ))
        .env(ROLE, "shim");
    command
}

/// The shim's part.
fn shim() -> ExitCode {
    match Supervise::parse(std::env::args_os().skip(1)) {
        Ok(supervise) => supervise.run(),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(FAILED)
        }
    }
}

/// The parent's part: starts the tree through the shim, prints the pids of
/// the shim and of the tree, and waits to be killed.
fn parent() -> ExitCode {
    within(async {
        let mut shim = spawn(&supervised(TREE)).await;
        let tree = text(&mut shim).await;
        println!("tree {} {tree}", shim.id());
        tokio::time::sleep(Duration::from_secs(300)).await;
    });
    ExitCode::SUCCESS
}

fn a_killed_parent_leaves_no_process_of_the_tree() {
    within(async {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.env(ROLE, "parent");
        let mut parent = spawn(&command).await;
        let tree = pids(text(&mut parent).await.strip_prefix("tree ").unwrap());
        assert_eq!(tree.len(), 4, "the shim, the shell and two sleeps");
        for pid in &tree {
            assert_eq!(kill(Pid::from_raw(*pid), None), Ok(()), "{pid} runs");
        }
        let parent = i32::try_from(parent.id()).unwrap();
        // A pid below 2 would signal this test's group or every process.
        assert!(parent > 1, "{parent}");
        kill(Pid::from_raw(parent), Signal::SIGKILL).unwrap();
        for pid in tree {
            assert!(gone(pid).await, "{pid} outlived its parent");
        }
    });
}

fn the_command_leads_a_group_that_the_shim_is_no_member_of() {
    within(async {
        let script = "/bin/ps -o pgid= -p $$; /bin/ps -o pgid= -p $PPID";
        let mut process = spawn(&supervised(script)).await;
        let command = text(&mut process).await;
        let shim = text(&mut process).await;
        assert_eq!(command.trim(), process.group().to_string());
        assert_eq!(shim.trim(), process.id().to_string());
        assert_ne!(process.group(), process.id());
    });
}

fn the_shim_exits_with_the_status_of_its_command() {
    within(async {
        let mut process = spawn(&supervised("echo out; exit 7")).await;
        assert_eq!(text(&mut process).await, "out");
        assert_eq!(process.wait().await.unwrap().code(), Some(7));
    });
}

fn sigterm_reaches_the_command_and_the_shim_exits_with_its_signal() {
    within(async {
        let mut process = spawn(&supervised("trap '' INT; echo ready; exec /bin/sleep 30")).await;
        assert_eq!(text(&mut process).await, "ready");
        let stopped = process.stop(SHORT, |_| false).await.unwrap();
        assert_eq!(stopped.by, StoppedBy::Terminate);
        assert_eq!(stopped.status.code(), Some(128 + 15));
    });
}

fn the_last_step_of_the_stop_sequence_leaves_no_process_of_the_tree() {
    within(async {
        let mut process = spawn(&supervised(&format!("trap '' INT TERM; {TREE}"))).await;
        let tree = pids(&text(&mut process).await);
        let shim = i32::try_from(process.id()).unwrap();
        let stopped = process.stop(SHORT, |_| false).await.unwrap();
        assert_eq!(stopped.by, StoppedBy::Kill);
        assert_eq!(stopped.status.code(), Some(128 + 9));
        for pid in tree.into_iter().chain([shim]) {
            assert!(gone(pid).await, "{pid} outlived the stop");
        }
    });
}

fn a_command_that_does_not_start_fails_the_shim() {
    within(async {
        let mut command = Command::new("/nonexistent/program");
        command
            .supervised_by(Supervisor::new(
                std::env::current_exe().unwrap(),
                Vec::<OsString>::new(),
            ))
            .env(ROLE, "shim");
        assert!(command.spawn(tokio::io::sink()).await.is_err());
    });
}

/// A stand-in for the shim, `/bin/sh -c script`, which reads the protocol's
/// arguments as `$1` and on.
fn fake_shim(script: &str) -> Command {
    let mut command = Command::new("/bin/true");
    command.supervised_by(Supervisor::new("/bin/sh", ["-c", script, "sh"]));
    command
}

fn a_shim_that_reports_no_pid_starts_nothing() {
    within(async {
        for script in [
            "exit 0",
            "printf '1\\n' > /dev/fd/$4",
            "printf 'x\\n' > /dev/fd/$4",
        ] {
            let spawned = fake_shim(script).spawn(tokio::io::sink()).await;
            assert!(spawned.is_err(), "{script}");
        }
        // The stand-in reports itself, the leader of a group of its own, so
        // that the sweep after its exit signals no other process.
        let mut process = fake_shim("printf '%s\\n' $$ > /dev/fd/$4")
            .spawn(tokio::io::sink())
            .await
            .unwrap();
        assert_eq!(process.group(), process.id());
        assert!(process.wait().await.unwrap().success());
    });
}

fn the_arguments_of_the_protocol() {
    let parse = |args: &[&str]| Supervise::parse(args.iter().map(OsString::from));
    assert!(
        parse(&[
            "--pipe-fd",
            "3",
            "--report-fd",
            "4",
            "--",
            "/bin/true",
            "-x"
        ])
        .is_ok()
    );
    for args in [
        &["--pipe-fd", "2", "--report-fd", "4", "--", "/bin/true"][..],
        &["--pipe-fd", "3", "--report-fd", "2", "--", "/bin/true"],
        &["--pipe-fd", "3", "--report-fd", "3", "--", "/bin/true"],
        &["--pipe-fd", "x", "--report-fd", "4", "--", "/bin/true"],
        &["--report-fd", "4", "--pipe-fd", "3", "--", "/bin/true"],
        &["--pipe-fd", "3", "--report-fd", "4", "/bin/true"],
        &["--pipe-fd", "3", "--report-fd", "4", "--"],
        &[],
    ] {
        assert!(parse(args).is_err(), "{args:?}");
    }
}

const TESTS: &[(&str, fn())] = &[
    (
        "a_killed_parent_leaves_no_process_of_the_tree",
        a_killed_parent_leaves_no_process_of_the_tree,
    ),
    (
        "the_command_leads_a_group_that_the_shim_is_no_member_of",
        the_command_leads_a_group_that_the_shim_is_no_member_of,
    ),
    (
        "the_shim_exits_with_the_status_of_its_command",
        the_shim_exits_with_the_status_of_its_command,
    ),
    (
        "sigterm_reaches_the_command_and_the_shim_exits_with_its_signal",
        sigterm_reaches_the_command_and_the_shim_exits_with_its_signal,
    ),
    (
        "the_last_step_of_the_stop_sequence_leaves_no_process_of_the_tree",
        the_last_step_of_the_stop_sequence_leaves_no_process_of_the_tree,
    ),
    (
        "a_command_that_does_not_start_fails_the_shim",
        a_command_that_does_not_start_fails_the_shim,
    ),
    (
        "a_shim_that_reports_no_pid_starts_nothing",
        a_shim_that_reports_no_pid_starts_nothing,
    ),
    (
        "the_arguments_of_the_protocol",
        the_arguments_of_the_protocol,
    ),
];

/// The flags of libtest that take a value, which is no filter.
const WITH_VALUE: &[&str] = &["--format", "--test-threads", "--skip", "--color", "-Z"];

fn main() -> ExitCode {
    match std::env::var(ROLE).as_deref() {
        Ok("shim") => return shim(),
        Ok("parent") => return parent(),
        _ => {}
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|arg| arg == name);
    if flag("--list") {
        // No test is ignored.
        if !flag("--ignored") {
            for (name, _) in TESTS {
                println!("{name}: test");
            }
        }
        return ExitCode::SUCCESS;
    }
    let mut filters = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if WITH_VALUE.contains(&arg.as_str()) {
            args.next();
        } else if !arg.starts_with('-') {
            filters.push(arg.as_str());
        }
    }
    let exact = flag("--exact");
    let mut failed = 0;
    for (name, test) in TESTS {
        let selected = filters.is_empty()
            || filters.iter().any(|filter| {
                if exact {
                    filter == name
                } else {
                    name.contains(filter)
                }
            });
        if selected {
            let passed = std::panic::catch_unwind(test).is_ok();
            println!("test {name} ... {}", if passed { "ok" } else { "FAILED" });
            failed += usize::from(!passed);
        }
    }
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

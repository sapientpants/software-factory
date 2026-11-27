//! `sf __supervise`, the supervisor shim of SPEC AG-16, as the binary `sf`
//! runs it. What the shim does is tested in `factory-proc`, whose tests play
//! the shim themselves; these show that `sf` reaches it (19.1).
//!
//! The parent of a shim is played by this test binary itself, run again with
//! [`PARENT`] set and the ignored test [`parent`] selected: a process that the
//! test can kill with SIGKILL.

use std::time::Duration;

use factory_daemon::process::{Command, Line, Process};
use factory_daemon::supervise::supervisor;
use nix::errno::Errno;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use tokio::time::Instant;

/// The variable that makes [`parent`] play its part.
const PARENT: &str = "SF_TEST_SUPERVISE_PARENT";

/// The binary under test.
const SF: &str = env!("CARGO_BIN_EXE_sf");

/// `/bin/sh -c script` through the shim of `sf`.
fn supervised(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(script).supervised_by(supervisor(SF));
    command
}

/// The pids of a tree of two sleeping processes under a shell, as the shell
/// prints them: the shell and its two children.
const TREE: &str = "/bin/sleep 300 & a=$!; /bin/sleep 300 & b=$!; echo \"$$ $a $b\"; wait";

async fn text(process: &mut Process) -> String {
    match process
        .next_line()
        .await
        .unwrap_or_else(|error| panic!("{error}"))
    {
        Some(Line::Text(text)) => text,
        other => panic!("not a line of text: {other:?}"),
    }
}

/// Whether the process `pid` has gone within 5 s, the time that the shim
/// and then init take to kill and reap it.
async fn gone(pid: i32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if kill(Pid::from_raw(pid), None) == Err(Errno::ESRCH) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

fn pids(line: &str) -> Vec<i32> {
    line.split_whitespace()
        .map(|pid| pid.parse().unwrap_or_else(|error| panic!("{pid}: {error}")))
        .collect()
}

/// The parent's part: starts the tree through the shim, prints the pids of
/// the shim and of the tree, and waits to be killed.
#[tokio::test]
#[ignore = "the parent of a_killed_parent_leaves_no_process_of_the_tree"]
async fn parent() {
    if std::env::var_os(PARENT).is_none() {
        return;
    }
    let mut shim = supervised(TREE).spawn(tokio::io::sink()).await.unwrap();
    let tree = text(&mut shim).await;
    println!("tree {} {tree}", shim.id());
    tokio::time::sleep(Duration::from_secs(300)).await;
}

#[tokio::test]
async fn a_killed_parent_leaves_no_process_of_the_tree() {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "parent", "--ignored", "--nocapture"])
        .env(PARENT, "1");
    let mut parent = command.spawn(tokio::io::sink()).await.unwrap();
    let tree = loop {
        let line = text(&mut parent).await;
        if let Some(tree) = line.strip_prefix("tree ") {
            break pids(tree);
        }
    };
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
}

#[tokio::test]
async fn the_shim_exits_with_the_status_of_its_command() {
    let mut process = supervised("echo out; exit 7")
        .spawn(tokio::io::sink())
        .await
        .unwrap();
    assert_eq!(text(&mut process).await, "out");
    assert_eq!(process.wait().await.unwrap().code(), Some(7));
}

#[test]
fn the_shim_refuses_arguments_outside_its_protocol() {
    let mut command = Command::new(SF);
    command.args(["__supervise", "--pipe-fd", "1", "--", "/bin/true"]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let status = runtime
        .block_on(async { command.spawn(tokio::io::sink()).await.unwrap().wait().await })
        .unwrap();
    assert_eq!(status.code(), Some(125));
}

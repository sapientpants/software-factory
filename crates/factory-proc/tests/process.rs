//! A command started through `factory-proc`: its environment, its group and
//! its output (SPEC AG-11, AG-13).

mod common;

use std::time::Duration;

use common::{gone, sh, spawn, text as read_text, within};
use factory_proc::{Command, DRAIN, Line};
use tokio::io::AsyncWriteExt as _;
use tokio::time::Instant;

/// The lines of stdout until the output ends.
async fn lines(mut command: Command) -> Vec<Line> {
    let mut process = command
        .spawn(tokio::io::sink())
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let mut lines = Vec::new();
    while let Some(line) = process
        .next_line()
        .await
        .unwrap_or_else(|error| panic!("{error}"))
    {
        lines.push(line);
    }
    lines
}

fn text(line: &str) -> Line {
    Line::Text(line.to_owned())
}

#[test]
fn the_environment_holds_the_allowed_variables_and_no_other() {
    within(async {
        // The test process has `HOME` and `PATH` at least, and the command sees
        // neither.
        let mut command = Command::new("/usr/bin/env");
        command.env("SF_SANDBOX", "none").env("LANG", "C.UTF-8");
        let mut environment = lines(command).await;
        environment.sort_by_key(|line| format!("{line:?}"));
        assert_eq!(
            environment,
            vec![text("LANG=C.UTF-8"), text("SF_SANDBOX=none")]
        );
    });
}

#[test]
fn the_command_leads_a_process_group_of_its_own() {
    within(async {
        let mut process = sh("/bin/ps -o pgid= -p $$")
            .spawn(tokio::io::sink())
            .await
            .unwrap();
        let Some(Line::Text(group)) = process.next_line().await.unwrap() else {
            panic!("no group");
        };
        assert_eq!(group.trim(), process.id().to_string());
        assert_eq!(process.group(), process.id());
        assert_ne!(process.id(), std::process::id());
    });
}

#[test]
fn a_line_over_the_cap_and_one_that_is_no_utf8_are_lines_and_reading_goes_on() {
    within(async {
        let mut command = sh(r"printf 'short\n%050d\n\377\nafter\nlast' 0");
        command.max_line_length(10);
        assert_eq!(
            lines(command).await,
            vec![
                text("short"),
                Line::TooLong,
                Line::NotUtf8,
                text("after"),
                text("last")
            ]
        );
    });
}

#[test]
fn stderr_is_written_to_its_sink_beside_stdout() {
    within(async {
        let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("stderr-sink");
        let file = tokio::fs::File::create(&path).await.unwrap();
        // More than a pipe holds, so that stdout would stall if stderr were not
        // drained meanwhile.
        let mut command = sh(
            "i=0; while [ $i -lt 4000 ]; do echo 'a line of stderr' >&2; i=$((i+1)); done; echo out",
        );
        let mut process = command.spawn(file).await.unwrap();
        assert_eq!(process.next_line().await.unwrap(), Some(text("out")));
        assert!(process.wait().await.unwrap().success());
        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written.lines().count(), 4000);
    });
}

#[test]
fn a_process_that_holds_stdout_after_the_leader_exits_is_drained_for_2_s_and_killed() {
    within(async {
        let started = Instant::now();
        let mut process = sh("/bin/sleep 30 & echo $!")
            .spawn(tokio::io::sink())
            .await
            .unwrap();
        let Some(Line::Text(pid)) = process.next_line().await.unwrap() else {
            panic!("no pid");
        };
        assert_eq!(process.next_line().await.unwrap(), None);
        let elapsed = started.elapsed();
        assert!(elapsed >= DRAIN, "{elapsed:?}");
        assert!(elapsed < DRAIN + Duration::from_secs(1), "{elapsed:?}");
        assert!(process.status().unwrap().success());
        assert!(gone(pid.parse().unwrap()).await);
    });
}

#[test]
fn a_dropped_process_leaves_no_process_of_its_group() {
    within(async {
        let mut process = sh("/bin/sleep 30 & echo $!; /bin/sleep 30")
            .spawn(tokio::io::sink())
            .await
            .unwrap();
        let Some(Line::Text(pid)) = process.next_line().await.unwrap() else {
            panic!("no pid");
        };
        let leader = i32::try_from(process.id()).unwrap();
        drop(process);
        assert!(gone(pid.parse().unwrap()).await);
        assert!(gone(leader).await);
    });
}

#[test]
fn stdin_is_written_until_it_is_closed() {
    within(async {
        let mut command = Command::new("/bin/cat");
        command.stdin_piped();
        let mut process = spawn(&mut command).await;
        let stdin = process.stdin().unwrap_or_else(|| panic!("no stdin"));
        stdin.write_all(b"to cat\n").await.unwrap();
        assert_eq!(read_text(&mut process).await, "to cat");
        assert_eq!(process.status(), None);
        process.close_stdin();
        assert!(process.stdin().is_none());
        assert_eq!(process.next_line().await.unwrap(), None);
        assert_eq!(process.status().and_then(|status| status.code()), Some(0));
    });
}

#[test]
fn the_status_is_that_of_the_leader() {
    within(async {
        let mut process = spawn(&mut sh("exit 4")).await;
        assert_eq!(process.wait().await.unwrap().code(), Some(4));
        assert_eq!(process.status().and_then(|status| status.code()), Some(4));
    });
}

#[test]
fn stdin_is_null_unless_it_is_piped() {
    within(async {
        let mut process = spawn(&mut Command::new("/bin/cat")).await;
        assert!(process.stdin().is_none());
        assert_eq!(process.next_line().await.unwrap(), None);
    });
}

#[test]
fn a_command_on_inherited_stdio_uses_the_three_of_this_process() {
    use nix::unistd::{dup, dup2_stderr, dup2_stdin, dup2_stdout};

    // nextest runs each test in a process of its own, so this one may put
    // files in place of its own stdio for the while.
    let dir = std::env::temp_dir().join(format!("sf-proc-inherit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("in"), "given\n").unwrap();
    let file = |name: &str| std::fs::File::create(dir.join(name)).unwrap();
    let (out, err) = (file("out"), file("err"));
    let saved = [
        dup(std::io::stdin()).unwrap(),
        dup(std::io::stdout()).unwrap(),
        dup(std::io::stderr()).unwrap(),
    ];
    dup2_stdin(std::fs::File::open(dir.join("in")).unwrap()).unwrap();
    dup2_stdout(&out).unwrap();
    dup2_stderr(&err).unwrap();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ran = runtime.block_on(async {
        tokio::time::timeout(common::LIMIT, async {
            let mut command = sh("read -r line; echo \"$line out\"; echo \"$line err\" >&2");
            command.inherit_stdio().stdin_piped();
            let mut process = command.spawn(tokio::io::sink()).await?;
            let piped = process.stdin().is_some();
            let line = process.next_line().await?;
            Ok::<_, factory_proc::Error>((process.wait().await?.code(), line, piped))
        })
        .await
    });
    let [stdin, stdout, stderr] = saved;
    dup2_stdin(stdin).unwrap();
    dup2_stdout(stdout).unwrap();
    dup2_stderr(stderr).unwrap();

    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let (out, err) = (read("out"), read("err"));
    std::fs::remove_dir_all(&dir).unwrap();
    let (code, line, piped) = ran.unwrap().unwrap();
    assert_eq!(code, Some(0));
    // Nothing reached a pipe: no stdin to write, no line, and the sink got
    // nothing.
    assert!(!piped);
    assert_eq!(line, None);
    assert_eq!((out.as_str(), err.as_str()), ("given out\n", "given err\n"));
}

#[test]
fn raw_stdout_is_read_as_the_bytes_it_holds() {
    within(async {
        // A NUL, a byte that is no UTF-8 and a line longer than the cap all
        // come back as they were written.
        let mut command = sh(r"printf 'a\000b\377\n%050d' 0");
        command.raw_stdout().max_line_length(10);
        let mut bytes = Vec::new();
        for line in lines(command).await {
            let Line::Bytes(chunk) = line else {
                panic!("{line:?}");
            };
            bytes.extend(chunk);
        }
        let mut expected = b"a\0b\xff\n".to_vec();
        expected.extend([b'0'; 50]);
        assert_eq!(bytes, expected);
    });
}

#[test]
fn a_taken_stdin_is_written_while_stdout_is_read() {
    within(async {
        let mut command = Command::new("/bin/cat");
        command.stdin_piped().raw_stdout();
        let mut process = spawn(&mut command).await;
        let mut stdin = process.take_stdin().unwrap_or_else(|| panic!("no stdin"));
        assert!(process.stdin().is_none());
        // More than a pipe holds in each direction, so that the write stalls
        // unless stdout is read beside it.
        let sent = vec![7u8; 1 << 20];
        let write = async {
            stdin.write_all(&sent).await.unwrap();
            drop(stdin);
        };
        let read = async {
            let mut received = Vec::new();
            while let Some(line) = process.next_line().await.unwrap() {
                let Line::Bytes(chunk) = line else {
                    panic!("{line:?}");
                };
                received.extend(chunk);
            }
            received
        };
        let ((), received) = tokio::join!(write, read);
        assert_eq!(received, sent);
    });
}

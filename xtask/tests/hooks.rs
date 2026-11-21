//! The committed hook files of SPEC DEV-9 and DEV-10, run as git and Claude
//! Code run them: `.githooks/pre-commit` through a real `git commit`, and the
//! commands of `.claude/settings.json` through `sh`. A stand-in `cargo` takes
//! the place of the real one, records how it was called and exits as told.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

/// What git exports to a hook about the repository it is working on.
const GIT_LOCAL: &[&str] = &["GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE", "GIT_PREFIX"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// A fresh directory for one test, holding the stand-in `cargo`.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(test: &str) -> Self {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("hooks-{test}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("bin")).expect("create the scratch directory");
        let scratch = Self { path };
        let cargo = scratch.path.join("bin/cargo");
        std::fs::write(
            &cargo,
            "#!/bin/sh\n\
             echo \"$*\" > \"$TRACE/arguments\"\n\
             cat > \"$TRACE/input\"\n\
             pwd -P > \"$TRACE/directory\"\n\
             env > \"$TRACE/environment\"\n\
             exit \"$CARGO_EXIT\"\n",
        )
        .expect("write the stand-in cargo");
        scratch.run(Command::new("chmod").arg("+x").arg(&cargo));
        scratch
    }

    /// Runs `command` with the stand-in `cargo` first on its `PATH`, cut off
    /// from the git repository and configuration of whoever runs the test.
    fn run(&self, command: &mut Command) -> Output {
        self.run_with_input(command, "")
    }

    /// As `run`, with `input` on the command's stdin.
    fn run_with_input(&self, command: &mut Command, input: &str) -> Output {
        let path = std::env::var("PATH").expect("PATH");
        for variable in GIT_LOCAL {
            command.env_remove(variable);
        }
        let mut child = command
            .env(
                "PATH",
                format!("{}:{path}", self.path.join("bin").display()),
            )
            .env("TRACE", &self.path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start the command");
        // A command that reads no input may have closed its end already.
        let _ = child
            .stdin
            .take()
            .expect("stdin of the command")
            .write_all(input.as_bytes());
        child.wait_with_output().expect("wait for the command")
    }

    /// A git repository with one staged file and nothing committed.
    fn repository(&self) -> PathBuf {
        let repository = self.path.join("repository");
        std::fs::create_dir(&repository).expect("create the repository directory");
        std::fs::write(repository.join("file"), "content\n").expect("write a file");
        for arguments in [
            &["init", "--quiet"][..],
            &["config", "user.name", "A Test"],
            &["config", "user.email", "test@example.invalid"],
            &["add", "file"],
        ] {
            let output = self.run(Command::new("git").args(arguments).current_dir(&repository));
            assert!(output.status.success(), "git {arguments:?}: {output:?}");
        }
        repository
    }

    /// What the stand-in `cargo` recorded under `name`, if it ran.
    fn trace(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.path.join(name)).ok()
    }
}

/// A `git commit` in `repository` with the hooks of this repository.
fn commit(scratch: &Scratch, repository: &Path, cargo_exit: &str) -> Output {
    let hooks = root().join(".githooks").canonicalize().expect(".githooks");
    scratch.run(
        Command::new("git")
            .arg("-c")
            .arg(format!("core.hooksPath={}", hooks.display()))
            .args(["commit", "--quiet", "--message", "a commit"])
            .current_dir(repository)
            .env("CARGO_EXIT", cargo_exit),
    )
}

fn has_a_commit(scratch: &Scratch, repository: &Path) -> bool {
    scratch
        .run(
            Command::new("git")
                .args(["rev-parse", "--quiet", "--verify", "HEAD"])
                .current_dir(repository),
        )
        .status
        .success()
}

#[test]
fn a_commit_is_refused_when_the_check_fails() {
    let scratch = Scratch::new("refused");
    let repository = scratch.repository();

    let output = commit(&scratch, &repository, "1");

    assert!(!output.status.success(), "{output:?}");
    assert!(!has_a_commit(&scratch, &repository));
    assert_eq!(scratch.trace("arguments").as_deref(), Some("xtask check\n"));
}

#[test]
fn a_commit_goes_through_when_the_check_passes() {
    let scratch = Scratch::new("passes");
    let repository = scratch.repository();

    let output = commit(&scratch, &repository, "0");

    assert!(output.status.success(), "{output:?}");
    assert!(has_a_commit(&scratch, &repository));
}

/// A test of the check that runs git in a directory of its own must work on
/// its own repository, not on the index of the commit being made.
#[test]
fn the_check_does_not_see_the_repository_git_is_committing_to() {
    let scratch = Scratch::new("environment");
    let repository = scratch.repository();

    commit(&scratch, &repository, "0");

    let environment = scratch.trace("environment").expect("the check ran");
    let exported: Vec<&str> = environment
        .lines()
        .filter(|line| {
            GIT_LOCAL
                .iter()
                .any(|variable| line.starts_with(&format!("{variable}=")))
        })
        .collect();
    assert!(exported.is_empty(), "{exported:?}");
}

/// The one command `.claude/settings.json` runs on `event`, for the tool
/// `matcher` where the event has one.
fn session_hook(event: &str, matcher: Option<&str>) -> String {
    let settings = std::fs::read_to_string(root().join(".claude/settings.json"))
        .expect("read .claude/settings.json");
    let settings: Value = serde_json::from_str(&settings).expect(".claude/settings.json is JSON");
    let commands: Vec<&str> = settings["hooks"][event]
        .as_array()
        .expect("hooks of the event")
        .iter()
        .filter(|group| group["matcher"].as_str() == matcher)
        .flat_map(|group| group["hooks"].as_array().expect("hooks of the group"))
        .map(|hook| hook["command"].as_str().expect("command of the hook"))
        .collect();
    assert_eq!(commands.len(), 1, "{event}: {commands:?}");
    commands[0].to_owned()
}

/// Runs a hook command of the settings file as Claude Code does: through the
/// shell, with the event on stdin, the project directory in the environment
/// and the session's own working directory elsewhere.
fn run_session_hook(
    scratch: &Scratch,
    command: &str,
    project: &Path,
    event: &str,
    cargo_exit: &str,
) -> Output {
    scratch.run_with_input(
        Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&scratch.path)
            .env("CLAUDE_PROJECT_DIR", project)
            .env("CARGO_EXIT", cargo_exit),
        event,
    )
}

/// A `PreToolUse` event for a Bash call, cut down to the command.
fn bash_event(command: &str) -> String {
    serde_json::json!({ "tool_name": "Bash", "tool_input": { "command": command } }).to_string()
}

#[test]
fn a_session_start_points_git_at_the_committed_hooks() {
    let scratch = Scratch::new("session-start");
    let repository = scratch.repository();
    let command = session_hook("SessionStart", None);

    let output = run_session_hook(&scratch, &command, &repository, "{}", "0");

    assert!(output.status.success(), "{output:?}");
    let hooks_path = scratch.run(
        Command::new("git")
            .args(["config", "--local", "core.hooksPath"])
            .current_dir(&repository),
    );
    assert_eq!(String::from_utf8_lossy(&hooks_path.stdout), ".githooks\n");
}

/// The wrappers are ones Claude Code's own `if` filter let through in a live
/// session (probe P-27), which is why the filter is the hook command's own.
#[test]
fn a_bash_call_that_names_git_is_put_to_the_task_in_the_project() {
    let scratch = Scratch::new("pre-tool-use");
    let project = scratch.repository();
    let command = session_hook("PreToolUse", Some("Bash"));

    for call in [
        "git status",
        "time git commit -n",
        "env FOO=1 /usr/bin/git commit -n",
    ] {
        let _ = std::fs::remove_file(scratch.path.join("arguments"));
        let event = bash_event(call);

        let output = run_session_hook(&scratch, &command, &project, &event, "0");

        assert_eq!(output.status.code(), Some(0), "{call}: {output:?}");
        assert_eq!(
            scratch.trace("arguments").as_deref(),
            Some("--quiet xtask pre-tool-use\n"),
            "{call}"
        );
        assert_eq!(scratch.trace("input").as_deref(), Some(event.as_str()));
        let directory = scratch.trace("directory").expect("the task ran");
        assert_eq!(
            Path::new(directory.trim_end()),
            project.canonicalize().expect("the project directory")
        );
    }
}

/// The task is built by cargo, so a call that cannot run git does not wait
/// for a build, and is not refused while the task does not build.
#[test]
fn a_bash_call_that_does_not_name_git_starts_no_task() {
    let scratch = Scratch::new("pre-tool-use-skipped");
    let project = scratch.repository();
    let command = session_hook("PreToolUse", Some("Bash"));

    let output = run_session_hook(&scratch, &command, &project, &bash_event("ls -n"), "2");

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(scratch.trace("arguments"), None);
}

/// Claude Code refuses a tool call on exit code 2 only, so a task that could
/// not be built or run must not look like one that let the call through.
#[test]
fn a_bash_call_that_names_git_is_refused_unless_the_task_passes() {
    let scratch = Scratch::new("pre-tool-use-refuses");
    let project = scratch.repository();
    let command = session_hook("PreToolUse", Some("Bash"));
    let event = bash_event("git status");

    for task_exit in ["2", "1", "101"] {
        let output = run_session_hook(&scratch, &command, &project, &event, task_exit);
        assert_eq!(output.status.code(), Some(2), "task exit {task_exit}");
    }
    let nowhere = scratch.path.join("no-such-project");
    let output = run_session_hook(&scratch, &command, &nowhere, &event, "0");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

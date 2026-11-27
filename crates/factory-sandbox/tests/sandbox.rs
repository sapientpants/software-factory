//! The sandbox tests of SPEC 19.4 for the bubblewrap backend, at level
//! `process`: those that need neither the proxy of SEC-10b nor the filter of
//! SEC-6e.
//!
//! The binary has a harness of its own. Run as `<binary> __supervise ...` it
//! is the supervisor shim of AG-16 that the backend starts, and runs no test.
//! Otherwise its tests are ignored unless [`GATE`] holds `process`: listed to
//! nextest as ignored, and run only where `--ignored` or `--include-ignored`
//! asks for them. So the plain test run runs none of them, and the sandbox
//! step of the check and the mutation step set the variable (DEV-2a). The
//! factory's other ignored tests are no sandbox tests, so `--run-ignored`
//! would not tell the two apart.
//! A test fails where bubblewrap is missing or cannot start: it is never
//! skipped (DEV-11).

use std::process::ExitCode;

#[cfg(target_os = "linux")]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test binary whose harness is its own, which clippy does not see as test code (19.3)"
)]
mod linux {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    use std::path::{Path, PathBuf};
    use std::process::ExitCode;
    use std::time::Duration;

    use factory_core::engine::RunId;
    use factory_proc::supervise::{FAILED, Supervise};
    use factory_proc::{Command, Line, Process, Supervisor};
    use factory_sandbox::bubblewrap::{Bubblewrap, parse_stat};
    use factory_sandbox::{
        Access, Error, ExecSpec, Mount, Sandbox as _, SandboxInstance, SandboxPolicy,
    };

    /// The first argument that makes this binary the shim.
    pub const SHIM: &str = "__supervise";

    /// Where the sandbox shows the work directory.
    const WORK: &str = "/work";

    /// The longest that a test may take.
    const LIMIT: Duration = Duration::from_secs(30);

    /// The shim's part.
    pub fn shim() -> ExitCode {
        match Supervise::parse(std::env::args_os().skip(2)) {
            Ok(supervise) => supervise.run(),
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(FAILED)
            }
        }
    }

    /// Runs `test` on a runtime of its own, and fails it after [`LIMIT`].
    fn within(test: impl Future<Output = ()>) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            tokio::time::timeout(LIMIT, test)
                .await
                .unwrap_or_else(|_| panic!("the test took more than {LIMIT:?}"));
        });
    }

    /// The `bwrap` that the `PATH` of the test finds.
    fn bwrap() -> PathBuf {
        std::env::var_os("PATH")
            .iter()
            .flat_map(std::env::split_paths)
            .map(|dir| dir.join("bwrap"))
            .find(|path| path.is_file())
            .expect("bubblewrap is installed and on the PATH (SPEC 4.4)")
    }

    fn backend() -> Bubblewrap {
        Bubblewrap::new(
            bwrap(),
            Supervisor::new(std::env::current_exe().unwrap(), [SHIM]),
        )
    }

    /// A fresh directory for one test, with a read-write `work` and a
    /// read-only `ro`, which the sandbox shows as `/work` and `/ro`, removed
    /// when the value is dropped. The host's paths are not shown: they lie
    /// below `/tmp`, where no mount is shown (SEC-6c), and wherever the
    /// copies of the mutation step lie.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("sf-sandbox-{}-{test}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            for dir in ["work", "ro", "exec"] {
                std::fs::create_dir_all(path.join(dir)).unwrap();
            }
            std::fs::write(path.join("ro/file"), "read-only\n").unwrap();
            Self(path)
        }

        fn work(&self) -> PathBuf {
            self.0.join("work")
        }

        fn tmp(&self) -> PathBuf {
            self.0.join("exec/tmp")
        }

        fn policy(&self) -> SandboxPolicy {
            SandboxPolicy {
                mounts: vec![
                    Mount {
                        host: self.work(),
                        sandbox: WORK.into(),
                        access: Access::ReadWrite,
                    },
                    Mount {
                        host: self.0.join("ro"),
                        sandbox: "/ro".into(),
                        access: Access::ReadOnly,
                    },
                ],
                tmp: self.tmp(),
            }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// `/bin/sh -c script` in the work directory, with `env` and a `PATH`.
    fn sh(script: &str, env: &[(&str, &str)]) -> ExecSpec {
        let mut spec =
            ExecSpec::new("/bin/sh", ["-c", script], WORK).with_env("PATH", "/usr/bin:/bin");
        for (key, value) in env {
            spec = spec.with_env(*key, *value);
        }
        spec
    }

    async fn spawn(instance: &dyn SandboxInstance, spec: &ExecSpec) -> Process {
        instance
            .command(spec)
            .unwrap()
            .spawn(tokio::io::stderr())
            .await
            .unwrap()
    }

    /// The lines of `process` to its end, and its exit code.
    async fn output(mut process: Process) -> (Option<i32>, Vec<String>) {
        let mut lines = Vec::new();
        while let Some(line) = process.next_line().await.unwrap() {
            match line {
                Line::Text(text) => lines.push(text),
                other => panic!("not a line of text: {other:?}"),
            }
        }
        (process.wait().await.unwrap().code(), lines)
    }

    /// Runs `script` in a new instance of the scratch's policy, and destroys
    /// the instance.
    async fn run(scratch: &Scratch, script: &str, env: &[(&str, &str)]) -> Vec<String> {
        let policy = scratch.policy();
        let instance = backend().create(RunId(1), &policy).await.unwrap();
        let process = spawn(instance.as_ref(), &sh(script, env)).await;
        let (code, lines) = output(process).await;
        instance.destroy().await.unwrap();
        assert_eq!(code, Some(0), "{lines:?}");
        lines
    }

    /// Whether the process `pid` has ended: gone, or a zombie.
    fn ended(pid: i32) -> bool {
        std::fs::read(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|stat| parse_stat(&stat))
            .is_none_or(|(_, _, zombie)| zombie)
    }

    /// `root` and every live process below it.
    fn tree(root: i32) -> Vec<i32> {
        let table: Vec<(i32, i32, bool)> = std::fs::read_dir("/proc")
            .unwrap()
            .flatten()
            .filter_map(|entry| std::fs::read(entry.path().join("stat")).ok())
            .filter_map(|stat| parse_stat(&stat))
            .collect();
        let mut tree = vec![root];
        let mut next = 0;
        while let Some(&parent) = tree.get(next) {
            let children: Vec<i32> = table
                .iter()
                .filter(|(pid, ppid, _)| *ppid == parent && !tree.contains(pid))
                .map(|(pid, _, _)| *pid)
                .collect();
            tree.extend(children);
            next += 1;
        }
        tree.retain(|pid| !ended(*pid));
        tree
    }

    fn a_command_runs_in_the_sandbox_through_the_shim() {
        within(async {
            let scratch = Scratch::new("shim");
            let policy = scratch.policy();
            let instance = backend().create(RunId(1), &policy).await.unwrap();
            // The command waits on its stdin until `bwrap` was looked at:
            // once it has ended, its command line reads empty.
            let script = "read -r _; echo $$; pwd; env | grep -v -e '^PWD=' | sort";
            let spec = sh(script, &[("SF_TEST_GIVEN", "yes")])
                .with_env("TMPDIR", "/elsewhere")
                .with_env("SF_SANDBOX", "none");
            let mut command = instance.command(&spec).unwrap();
            command.stdin_piped();
            let mut process = command.spawn(tokio::io::stderr()).await.unwrap();
            // The shim leads a group of its own, and the group it reports is
            // that of `bwrap`, which it started (AG-16).
            let (shim, group) = (process.id(), process.group());
            assert_ne!(shim, group);
            let started = std::fs::read(format!("/proc/{group}/cmdline")).unwrap();
            let program = started.split(|byte| *byte == 0).next().unwrap();
            assert_eq!(Path::new(std::str::from_utf8(program).unwrap()), bwrap());
            process.close_stdin();
            let (code, lines) = output(process).await;
            instance.destroy().await.unwrap();

            assert_eq!(code, Some(0), "{lines:?}");
            // PID 1 of the namespace is bubblewrap's, and the command the
            // next process (SEC-6a).
            assert_eq!(lines[0], "2");
            assert_eq!(lines[1], WORK);
            // The environment of the spec and of the instance, and nothing of
            // this process's (AG-11, AG-11a, SEC-6c).
            assert_eq!(
                lines[2..],
                [
                    "PATH=/usr/bin:/bin",
                    "SF_SANDBOX=process",
                    "SF_TEST_GIVEN=yes",
                    "TMPDIR=/tmp"
                ]
            );
        });
    }

    fn the_host_home_and_a_credential_file_cannot_be_read() {
        within(async {
            let scratch = Scratch::new("home");
            let credential = scratch.0.join("credential");
            std::fs::write(&credential, "a secret\n").unwrap();
            std::fs::set_permissions(&credential, std::fs::Permissions::from_mode(0o600)).unwrap();
            let home = std::env::var("HOME").expect("the test has a home");
            assert!(Path::new(&home).is_dir(), "{home}");

            let lines = run(
                &scratch,
                r#"
                cat "$CREDENTIAL" 2>/dev/null && echo READ-CREDENTIAL
                ls -A "$HOME_OF_THE_HOST" 2>/dev/null && echo LISTED-HOME
                for path in "$CREDENTIAL" "$SCRATCH" "$HOME_OF_THE_HOST" /root /home /etc/shadow /run /var /srv /opt /mnt; do
                    test -e "$path" && echo "SHOWN $path"
                done
                echo done
                "#,
                &[
                    ("CREDENTIAL", &credential.to_string_lossy()),
                    ("SCRATCH", &scratch.0.to_string_lossy()),
                    ("HOME_OF_THE_HOST", &home),
                ],
            )
            .await;

            assert_eq!(lines, ["done"]);
        });
    }

    fn a_host_process_cannot_be_seen_read_or_signalled() {
        within(async {
            let scratch = Scratch::new("processes");
            // The marker reaches the sandbox in a file alone, so that no
            // argument and no variable of a process of the sandbox holds it:
            // a `grep` given it as an argument would find itself.
            let marker = "sf-host-secret-31a7";
            std::fs::write(scratch.work().join("marker"), marker).unwrap();
            let mut host = Command::new("/bin/sh");
            host.args(["-c", "exec /bin/sleep 300", marker])
                .env("SF_TEST_MARKER", marker);
            let host = host.spawn(tokio::io::sink()).await.unwrap();
            let pid = host.id().to_string();
            assert!(!ended(host.id().try_into().unwrap()));

            let lines = run(
                &scratch,
                r#"
                cat /proc/[0-9]*/cmdline /proc/[0-9]*/environ 2>/dev/null \
                    | tr '\0' '\n' | grep -c -F -f /work/marker
                test -e "/proc/$HOST" && echo "SEES $HOST"
                kill -0 "$HOST" 2>/dev/null && echo "SIGNALS $HOST"
                kill -KILL "$HOST" 2>/dev/null && echo "KILLED $HOST"
                ls /proc | grep -c -E '^[0-9]+$'
                "#,
                &[("HOST", &pid)],
            )
            .await;

            // No process of the sandbox holds the marker; it sees bubblewrap's
            // PID 1, the shell and the commands of the pipe, and nothing of
            // the host.
            assert_eq!(lines[0], "0", "{lines:?}");
            assert_eq!(lines.len(), 2, "{lines:?}");
            let seen: usize = lines[1].parse().unwrap();
            assert!(seen <= 4, "{lines:?}");
            assert!(
                !ended(host.id().try_into().unwrap()),
                "the host process was killed"
            );
        });
    }

    fn writes_outside_the_allowed_paths_fail() {
        within(async {
            let scratch = Scratch::new("writes");
            let lines = run(
                &scratch,
                &r#"
                for dir in / /usr /usr/bin /bin /etc /proc /sys /dev /dev/shm /dev/pts /home "$RO" "$SCRATCH" "$SCRATCH/exec"; do
                    touch "$dir/sf-probe" 2>/dev/null && echo "WROTE $dir"
                    mkdir "$dir/sf-probe-dir" 2>/dev/null && echo "MADE $dir"
                done
                echo changed >> "$RO/file" 2>/dev/null && echo "CHANGED $RO/file"
                echo discarded > /dev/null && echo null
                touch ./ok && echo work
                touch /tmp/ok && echo tmp
                "#
                .replace("$RO", "/ro")
                .replace("$SCRATCH", &scratch.0.to_string_lossy()),
                &[],
            )
            .await;

            assert_eq!(lines, ["null", "work", "tmp"]);
            assert_eq!(
                std::fs::read_to_string(scratch.0.join("ro/file")).unwrap(),
                "read-only\n"
            );
            assert!(scratch.work().join("ok").exists());
            for dir in [&scratch.0, &scratch.0.join("ro"), &scratch.0.join("exec")] {
                assert!(!dir.join("sf-probe").exists(), "{}", dir.display());
            }
        });
    }

    fn a_setuid_binary_or_a_mount_gives_no_privilege() {
        within(async {
            let scratch = Scratch::new("privilege");
            let lines = run(
                &scratch,
                r#"
                grep -E '^(NoNewPrivs|CapEff|CapPrm|CapAmb):' /proc/self/status
                mount -t tmpfs none ./ 2>/dev/null && echo MOUNTED
                mount --bind /usr ./ 2>/dev/null && echo BOUND
                mount -o remount,rw / 2>/dev/null && echo REMOUNTED
                for setuid in /usr/bin/sudo /usr/bin/su /usr/bin/mount /usr/bin/passwd; do
                    test -u "$setuid" && echo "setuid $setuid"
                done
                test -u /usr/bin/sudo && /usr/bin/sudo -n /usr/bin/id -u 2>/dev/null
                test -u /usr/bin/su && /usr/bin/su -c '/usr/bin/id -u' root </dev/null 2>/dev/null
                echo uid $(id -u)
                "#,
                &[],
            )
            .await;

            assert_eq!(
                lines[..4],
                [
                    "CapPrm:\t0000000000000000",
                    "CapEff:\t0000000000000000",
                    "CapAmb:\t0000000000000000",
                    "NoNewPrivs:\t1",
                ],
                "{lines:?}"
            );
            // A setuid binary was there to try, and none ran as root.
            assert!(
                lines.iter().any(|line| line.starts_with("setuid ")),
                "{lines:?}"
            );
            let uid = std::fs::metadata(&scratch.0).unwrap().uid();
            assert!(uid != 0, "the tests run as root");
            assert!(!lines.contains(&"0".to_owned()), "{lines:?}");
            assert_eq!(lines.last().unwrap(), &format!("uid {uid}"));
            assert!(
                !lines
                    .iter()
                    .any(|line| ["MOUNTED", "BOUND", "REMOUNTED"].contains(&line.as_str())),
                "{lines:?}"
            );
        });
    }

    fn a_worker_creates_no_user_namespace() {
        within(async {
            let scratch = Scratch::new("userns");
            let lines = run(
                &scratch,
                r#"
                unshare --user true 2>/dev/null && echo UNSHARED
                unshare --user --map-root-user true 2>/dev/null && echo UNSHARED-ROOT
                bwrap --unshare-user --ro-bind / / true 2>/dev/null && echo NESTED
                test -x /usr/bin/unshare && test -x /usr/bin/bwrap && echo tried
                "#,
                &[],
            )
            .await;

            assert_eq!(lines, ["tried"]);
        });
    }

    fn no_etc_claude_code_is_shown() {
        within(async {
            let scratch = Scratch::new("claude-code");
            let lines = run(
                &scratch,
                "test -e /etc/claude-code || echo absent; ls -A /etc | sort",
                &[],
            )
            .await;

            assert_eq!(lines[0], "absent");
            for name in &lines[1..] {
                assert!(
                    factory_sandbox::bubblewrap::BASELINE
                        .contains(&format!("/etc/{name}").as_str()),
                    "/etc/{name} is no part of the baseline"
                );
            }
            // A policy that asks for it is refused.
            let mut policy = scratch.policy();
            policy.mounts.push(Mount {
                host: scratch.0.join("ro"),
                sandbox: "/etc/claude-code".into(),
                access: Access::ReadOnly,
            });
            let refused = backend().create(RunId(1), &policy).await.err();
            assert!(matches!(refused, Some(Error::Policy(_))), "{refused:?}");
            assert!(!policy.tmp.exists());
        });
    }

    fn tmp_is_the_temporary_directory_of_the_stage_execution() {
        within(async {
            let scratch = Scratch::new("tmp");
            let policy = scratch.policy();
            let instance = backend().create(RunId(1), &policy).await.unwrap();
            let metadata = std::fs::metadata(scratch.tmp()).unwrap();
            assert_eq!(metadata.permissions().mode() & 0o7777, 0o700);
            let script = "echo \"$TMPDIR\"; stat -c '%a %u' /tmp /; echo written > /tmp/probe";
            let process = spawn(instance.as_ref(), &sh(script, &[])).await;
            let (code, lines) = output(process).await;
            assert_eq!(code, Some(0), "{lines:?}");
            assert_eq!(
                std::fs::read_to_string(scratch.tmp().join("probe")).unwrap(),
                "written\n"
            );
            instance.destroy().await.unwrap();

            let uid = std::fs::metadata(&scratch.0).unwrap().uid();
            assert_eq!(lines[0], "/tmp");
            assert_eq!(lines[1], format!("700 {uid}"));
            // Above `/tmp` there is the root alone, which belongs to the
            // worker's user or to root and which neither the group nor
            // others can write without the sticky bit (SEC-6c).
            let (mode, owner) = lines[2].split_once(' ').unwrap();
            let mode = u32::from_str_radix(mode, 8).unwrap();
            let owner: u32 = owner.parse().unwrap();
            assert!(owner == uid || owner == 0, "{lines:?}");
            assert!(mode & 0o022 == 0 || mode & 0o1000 != 0, "{lines:?}");
            assert!(!scratch.tmp().exists(), "destroy removes it");
        });
    }

    fn the_sandbox_holds_no_network_interface_but_the_loopback() {
        within(async {
            let scratch = Scratch::new("network");
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port().to_string();
            let lines = run(
                &scratch,
                r#"
                tail -n +3 /proc/net/dev | cut -d: -f1 | tr -d ' '
                bash -c "echo hello > /dev/tcp/127.0.0.1/$PORT" 2>/dev/null && echo CONNECTED
                test -x /usr/bin/bash && echo tried
                "#,
                &[("PORT", &port)],
            )
            .await;

            assert_eq!(lines, ["lo", "tried"]);
            let accepted =
                tokio::time::timeout(Duration::from_millis(200), listener.accept()).await;
            assert!(accepted.is_err(), "the host's listener was reached");
        });
    }

    fn kill_all_leaves_no_process_of_the_instance() {
        within(async {
            let scratch = Scratch::new("kill");
            let policy = scratch.policy();
            let instance = backend().create(RunId(1), &policy).await.unwrap();
            let script = "/bin/sleep 300 & /bin/sleep 300 & echo ready; wait";
            let mut process = spawn(instance.as_ref(), &sh(script, &[])).await;
            assert_eq!(
                process.next_line().await.unwrap(),
                Some(Line::Text("ready".to_owned()))
            );
            let tree = tree(process.group().try_into().unwrap());
            // bwrap, bwrap's PID 1, the shell and two sleeps.
            assert_eq!(tree.len(), 5, "{tree:?}");

            instance.kill_all().await.unwrap();

            for pid in &tree {
                assert!(ended(*pid), "{pid} outlived the kill");
            }
            assert_eq!(process.wait().await.unwrap().code(), Some(128 + 9));
            instance.destroy().await.unwrap();
            assert!(!scratch.tmp().exists());
        });
    }

    fn reap_ends_what_an_instance_left_after_a_restart() {
        within(async {
            let scratch = Scratch::new("reap");
            let policy = scratch.policy();
            let instance = backend().create(RunId(7), &policy).await.unwrap();
            let record = instance.record();
            assert_eq!(record.run, RunId(7));
            assert_eq!(record.level, factory_sandbox::Level::Process);
            assert_eq!(record.tmp, scratch.tmp());
            let script = "/bin/sleep 300 & echo ready; wait";
            let mut process = spawn(instance.as_ref(), &sh(script, &[])).await;
            assert_eq!(
                process.next_line().await.unwrap(),
                Some(Line::Text("ready".to_owned()))
            );
            let tree = tree(process.group().try_into().unwrap());
            assert_eq!(tree.len(), 4, "{tree:?}");
            // The daemon that held the instance is gone; a new one reads the
            // record.
            drop(instance);

            backend().reap(&record).await.unwrap();

            for pid in &tree {
                assert!(ended(*pid), "{pid} outlived the reap");
            }
            assert!(!scratch.tmp().exists());
            let _ = process.wait().await;
        });
    }

    fn a_backend_without_bubblewrap_creates_no_instance() {
        within(async {
            let scratch = Scratch::new("no-bwrap");
            let backend = Bubblewrap::new(
                scratch.0.join("no-such-bwrap"),
                Supervisor::new(std::env::current_exe().unwrap(), [SHIM]),
            );
            let refused = backend.create(RunId(1), &scratch.policy()).await.err();
            assert!(matches!(refused, Some(Error::Host { .. })), "{refused:?}");
            assert!(!scratch.tmp().exists());
        });
    }

    fn an_existing_temporary_directory_is_refused() {
        within(async {
            let scratch = Scratch::new("tmp-exists");
            std::fs::create_dir(scratch.tmp()).unwrap();
            let refused = backend().create(RunId(1), &scratch.policy()).await.err();
            assert!(matches!(refused, Some(Error::Host { .. })), "{refused:?}");
        });
    }

    fn destroy_removes_a_temporary_directory_that_its_owner_cannot_write() {
        within(async {
            let scratch = Scratch::new("tmp-locked");
            let lines = run(
                &scratch,
                "mkdir -p /tmp/a/b && touch /tmp/a/b/c && chmod 0 /tmp/a/b /tmp/a && echo locked",
                &[],
            )
            .await;
            assert_eq!(lines, ["locked"]);
            assert!(!scratch.tmp().exists());
        });
    }

    /// The tests, each with its name.
    pub const TESTS: &[(&str, fn())] = &[
        (
            "a_command_runs_in_the_sandbox_through_the_shim",
            a_command_runs_in_the_sandbox_through_the_shim,
        ),
        (
            "the_host_home_and_a_credential_file_cannot_be_read",
            the_host_home_and_a_credential_file_cannot_be_read,
        ),
        (
            "a_host_process_cannot_be_seen_read_or_signalled",
            a_host_process_cannot_be_seen_read_or_signalled,
        ),
        (
            "writes_outside_the_allowed_paths_fail",
            writes_outside_the_allowed_paths_fail,
        ),
        (
            "a_setuid_binary_or_a_mount_gives_no_privilege",
            a_setuid_binary_or_a_mount_gives_no_privilege,
        ),
        (
            "a_worker_creates_no_user_namespace",
            a_worker_creates_no_user_namespace,
        ),
        ("no_etc_claude_code_is_shown", no_etc_claude_code_is_shown),
        (
            "tmp_is_the_temporary_directory_of_the_stage_execution",
            tmp_is_the_temporary_directory_of_the_stage_execution,
        ),
        (
            "the_sandbox_holds_no_network_interface_but_the_loopback",
            the_sandbox_holds_no_network_interface_but_the_loopback,
        ),
        (
            "kill_all_leaves_no_process_of_the_instance",
            kill_all_leaves_no_process_of_the_instance,
        ),
        (
            "reap_ends_what_an_instance_left_after_a_restart",
            reap_ends_what_an_instance_left_after_a_restart,
        ),
        (
            "a_backend_without_bubblewrap_creates_no_instance",
            a_backend_without_bubblewrap_creates_no_instance,
        ),
        (
            "an_existing_temporary_directory_is_refused",
            an_existing_temporary_directory_is_refused,
        ),
        (
            "destroy_removes_a_temporary_directory_that_its_owner_cannot_write",
            destroy_removes_a_temporary_directory_that_its_owner_cannot_write,
        ),
    ];
}

/// The variable that makes the tests of level `process` ordinary tests.
const GATE: &str = "SF_SANDBOX_TESTS";

/// The flags of libtest that take a value, which is no filter.
const WITH_VALUE: &[&str] = &["--format", "--test-threads", "--skip", "--color", "-Z"];

#[cfg(target_os = "linux")]
fn tests() -> &'static [(&'static str, fn())] {
    linux::TESTS
}

#[cfg(not(target_os = "linux"))]
fn tests() -> &'static [(&'static str, fn())] {
    // The backend is compiled out of a macOS build (SPEC 4.4).
    &[]
}

fn main() -> ExitCode {
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some(linux::SHIM) {
        return linux::shim();
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|arg| arg == name);
    // Without the gate every test is ignored: listed with `--ignored` alone,
    // and run where the ignored tests are asked for. With it no test is.
    let gated = std::env::var_os(GATE).is_some_and(|level| level == "process");
    let listed = if gated {
        !flag("--ignored")
    } else {
        flag("--ignored")
    };
    if flag("--list") {
        if listed {
            for (name, _) in tests() {
                println!("{name}: test");
            }
        }
        return ExitCode::SUCCESS;
    }
    let run = if gated {
        !flag("--ignored")
    } else {
        flag("--ignored") || flag("--include-ignored")
    };
    if !run {
        return ExitCode::SUCCESS;
    }
    let mut filters = Vec::new();
    let mut words = args.iter();
    while let Some(arg) = words.next() {
        if WITH_VALUE.contains(&arg.as_str()) {
            words.next();
        } else if !arg.starts_with('-') {
            filters.push(arg.as_str());
        }
    }
    let exact = flag("--exact");
    let mut failed = 0;
    for (name, test) in tests() {
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

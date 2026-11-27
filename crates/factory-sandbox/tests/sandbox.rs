//! The sandbox tests of SPEC 19.4 for the bubblewrap backend, at level
//! `process`: those that need no proxy of SEC-10b, which a listener on a Unix
//! socket stands in for.
//!
//! The binary has a harness of its own. Run as `<binary> __supervise ...` it
//! is the supervisor shim of AG-16 that the backend starts, and runs no test;
//! run as `<binary> __sandbox-init ...` it is the first process of SEC-10a,
//! which the backend shows as the `sf` binary.
//! Run as `<binary> __call <calls>` it makes the calls of the table of SEC-6e
//! and prints what each returned, which the tests of the filter have it do
//! inside an instance.
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
    use factory_sandbox::bubblewrap::init::{self, Init};
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

    /// The first argument that makes this binary the first process.
    pub const INIT: &str = init::SUBCOMMAND;

    /// The first process's part.
    pub fn first_process() -> ExitCode {
        match Init::parse(std::env::args_os().skip(2)) {
            Ok(init) => init.run(),
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(FAILED)
            }
        }
    }

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
        let this = std::env::current_exe().unwrap();
        Bubblewrap::new(bwrap(), Supervisor::new(&this, [SHIM]), this)
    }

    /// A fresh directory for one test, with a read-write `work` and a
    /// read-only `ro`, which the sandbox shows as `/work` and `/ro`, and the
    /// socket that stands in for the proxy's, removed when the value is
    /// dropped. The host's paths are not shown: they lie below `/tmp`, where
    /// no mount is shown (SEC-6c), and wherever the copies of the mutation
    /// step lie.
    struct Scratch(PathBuf, Option<std::os::unix::net::UnixListener>);

    impl Scratch {
        fn new(test: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("sf-sandbox-{}-{test}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            for dir in ["work", "ro", "exec"] {
                std::fs::create_dir_all(path.join(dir)).unwrap();
            }
            std::fs::write(path.join("ro/file"), "read-only\n").unwrap();
            let proxy =
                std::os::unix::net::UnixListener::bind(path.join("exec/proxy.sock")).unwrap();
            Self(path, Some(proxy))
        }

        /// The stand-in for the proxy's listener, which no connection
        /// reaches until it is taken.
        fn proxy(&mut self) -> tokio::net::UnixListener {
            let listener = self.1.take().expect("the stand-in is taken once");
            listener.set_nonblocking(true).unwrap();
            tokio::net::UnixListener::from_std(listener).unwrap()
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
                proxy: self.0.join("exec/proxy.sock"),
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
            // PID 1 of the namespace is the first process, and the command
            // the next process (SEC-6a, SEC-10a).
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

            // No process of the sandbox holds the marker; it sees the first
            // process, the shell and the commands of the pipe, and nothing of
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
            // bwrap, the first process, the shell and two sleeps.
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
            let this = std::env::current_exe().unwrap();
            let backend = Bubblewrap::new(
                scratch.0.join("no-such-bwrap"),
                Supervisor::new(&this, [SHIM]),
                this,
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

    /// The next line of `process`, which is text.
    async fn text(process: &mut Process) -> String {
        match process.next_line().await.unwrap() {
            Some(Line::Text(text)) => text,
            other => panic!("not a line of text: {other:?}"),
        }
    }

    /// The live children of `parent`.
    fn children(parent: i32) -> Vec<i32> {
        std::fs::read_dir("/proc")
            .unwrap()
            .flatten()
            .filter_map(|entry| std::fs::read(entry.path().join("stat")).ok())
            .filter_map(|stat| parse_stat(&stat))
            .filter(|(_, ppid, zombie)| *ppid == parent && !zombie)
            .map(|(pid, _, _)| pid)
            .collect()
    }

    /// The first process of the sandbox that `process` runs, as the host
    /// sees it: the one child of its `bwrap`, which is `sf __sandbox-init`.
    fn first_process_of(process: &Process) -> i32 {
        let [first] = children(process.group().try_into().unwrap())[..] else {
            panic!("bwrap has one child");
        };
        let cmdline = std::fs::read(format!("/proc/{first}/cmdline")).unwrap();
        let words: Vec<&[u8]> = cmdline.split(|byte| *byte == 0).take(2).collect();
        assert_eq!(words, [init::BINARY.as_bytes(), INIT.as_bytes()]);
        first
    }

    /// Waits up to 5 s for each of `pids` to end, and fails on any left.
    async fn all_end(pids: &[i32]) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while pids.iter().any(|pid| !ended(*pid)) && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let left: Vec<&i32> = pids.iter().filter(|pid| !ended(**pid)).collect();
        assert!(left.is_empty(), "{left:?} outlived the first process");
    }

    fn the_first_process_is_pid_1_and_the_binary_is_read_only() {
        within(async {
            let scratch = Scratch::new("pid-1");
            let lines = run(
                &scratch,
                r#"
                tr '\0' '\n' < /proc/1/cmdline | head -n 3
                grep -c -F "$SCRATCH" /proc/1/cmdline
                test -x /sf/sf && echo runs
                test -w /sf/sf || echo read-only
                test -S /sf/proxy.sock && echo socket
                ls -A /sf
                "#,
                &[("SCRATCH", &scratch.0.to_string_lossy())],
            )
            .await;

            // PID 1 holds no path of the host, so a worker reads none of
            // them there (SEC-6g).
            assert_eq!(
                lines,
                [
                    "/sf/sf",
                    "__sandbox-init",
                    "/bin/sh",
                    "0",
                    "runs",
                    "read-only",
                    "socket",
                    "proxy.sock",
                    "sf"
                ]
            );
        });
    }

    fn the_first_process_reaps_a_process_left_to_it() {
        within(async {
            let scratch = Scratch::new("orphan");
            // The subshell ends before its sleep does, which is then left to
            // PID 1 (SEC-10a).
            let lines = run(
                &scratch,
                r#"
                ( /bin/sleep 0.1 & echo $! > /tmp/orphan )
                orphan=$(cat /tmp/orphan)
                cut -d ' ' -f 4 "/proc/$orphan/stat"
                /bin/sleep 1
                test -e "/proc/$orphan" && cut -d ' ' -f 3 "/proc/$orphan/stat"
                echo done
                "#,
                &[],
            )
            .await;

            assert_eq!(lines, ["1", "done"]);
        });
    }

    fn a_connection_to_the_port_reaches_the_proxy_and_no_other() {
        within(async {
            let mut scratch = Scratch::new("forward");
            let proxy = scratch.proxy();
            let host = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = host.local_addr().unwrap().port().to_string();
            let policy = scratch.policy();
            let instance = backend().create(RunId(1), &policy).await.unwrap();
            // The connection is the command's first act: the port listens
            // before the command exists (SEC-10a).
            let script = r#"
                bash -c 'exec 3<>/dev/tcp/127.0.0.1/3128 && echo hello >&3 && read -r answer <&3 && echo "$answer"'
                bash -c "echo hello > /dev/tcp/127.0.0.1/$PORT" 2>/dev/null && echo HOST
                bash -c 'echo hello > /dev/tcp/127.0.0.1/3129' 2>/dev/null && echo OTHER
                echo done
                "#;
            let process = spawn(instance.as_ref(), &sh(script, &[("PORT", &port)])).await;
            let stand_in = async {
                use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};

                let (stream, _) = proxy.accept().await.unwrap();
                let mut stream = tokio::io::BufReader::new(stream);
                let mut line = String::new();
                stream.read_line(&mut line).await.unwrap();
                stream
                    .get_mut()
                    .write_all(b"from the proxy\n")
                    .await
                    .unwrap();
                line
            };
            let (heard, (code, lines)) = tokio::join!(stand_in, output(process));
            instance.destroy().await.unwrap();

            assert_eq!(code, Some(0), "{lines:?}");
            assert_eq!(heard, "hello\n");
            assert_eq!(lines, ["from the proxy", "done"]);
            // One connection reached the stand-in, and none the host.
            for accepted in [
                tokio::time::timeout(Duration::from_millis(200), proxy.accept())
                    .await
                    .is_ok(),
                tokio::time::timeout(Duration::from_millis(200), host.accept())
                    .await
                    .is_ok(),
            ] {
                assert!(!accepted);
            }
        });
    }

    fn a_sigint_and_a_sigterm_to_the_first_process_reach_the_command() {
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;

        within(async {
            let scratch = Scratch::new("sigterm");
            let policy = scratch.policy();
            let instance = backend().create(RunId(1), &policy).await.unwrap();
            let script = "trap 'echo int' INT; trap 'echo term; exit 7' TERM; \
                          echo ready; while :; do /bin/sleep 0.1; done";
            let mut process = spawn(instance.as_ref(), &sh(script, &[])).await;
            assert_eq!(text(&mut process).await, "ready");
            let first = Pid::from_raw(first_process_of(&process));

            kill(first, Signal::SIGINT).unwrap();
            assert_eq!(text(&mut process).await, "int");
            kill(first, Signal::SIGTERM).unwrap();
            assert_eq!(text(&mut process).await, "term");

            // The command ended by itself, and the first process and bwrap
            // with its status.
            assert_eq!(process.wait().await.unwrap().code(), Some(7));
            instance.destroy().await.unwrap();
        });
    }

    fn after_a_sigkill_to_the_first_process_no_process_of_the_sandbox_is_left() {
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;

        within(async {
            let scratch = Scratch::new("sigkill");
            let policy = scratch.policy();
            let instance = backend().create(RunId(1), &policy).await.unwrap();
            // A grandchild in a session of its own, which no signal to the
            // command's group reaches.
            let script = "/bin/sleep 300 & setsid /bin/sleep 300 & echo ready; wait";
            let mut process = spawn(instance.as_ref(), &sh(script, &[])).await;
            assert_eq!(
                process.next_line().await.unwrap(),
                Some(Line::Text("ready".to_owned()))
            );
            let tree = tree(process.group().try_into().unwrap());
            // bwrap, the first process, the shell and two sleeps.
            assert_eq!(tree.len(), 5, "{tree:?}");
            let first = first_process_of(&process);

            kill(Pid::from_raw(first), Signal::SIGKILL).unwrap();

            all_end(&tree).await;
            assert_eq!(process.wait().await.unwrap().code(), Some(128 + 9));
            instance.destroy().await.unwrap();
        });
    }

    fn a_proxy_that_is_no_socket_creates_no_instance() {
        within(async {
            let scratch = Scratch::new("no-socket");
            let mut policy = scratch.policy();
            policy.proxy = scratch.0.join("ro/file");
            let refused = backend().create(RunId(1), &policy).await.err();
            assert!(matches!(refused, Some(Error::Policy(_))), "{refused:?}");
            policy.proxy = scratch.0.join("no-such-socket");
            let refused = backend().create(RunId(1), &policy).await.err();
            assert!(matches!(refused, Some(Error::Host { .. })), "{refused:?}");
            assert!(!scratch.tmp().exists());
        });
    }

    /// The first argument that makes this binary the probe of the filter.
    pub const CALL: &str = "__call";

    /// Where an instance shows this binary to the tests of the filter.
    const PROBE: &str = "/probe";

    /// The probe of the filter: makes each call that `which` names and
    /// prints a line for each, its label and what it returned.
    pub fn call(which: &str) -> ExitCode {
        match which {
            "rows" => {
                for (label, outcome) in calls::rows() {
                    println!("{label} {outcome}");
                }
                ExitCode::SUCCESS
            }
            "descriptors" => {
                // What each descriptor of this process is: a file of memory
                // shows as `/memfd:<name> (deleted)`.
                for entry in std::fs::read_dir("/proc/self/fd").unwrap().flatten() {
                    if let Ok(target) = std::fs::read_link(entry.path()) {
                        println!("{}", target.display());
                    }
                }
                ExitCode::SUCCESS
            }
            "another-architecture" => {
                calls::another_architecture();
                println!("another-architecture returned");
                ExitCode::SUCCESS
            }
            other => {
                eprintln!("no such call: {other}");
                ExitCode::FAILURE
            }
        }
    }

    /// The calls of the probe. Each is made with arguments that the kernel
    /// would refuse or that would do nothing, so that a call which the
    /// filter lets through by mistake does no harm and, for most, returns
    /// something other than the code of its row.
    mod calls {
        use std::ffi::{CStr, c_long};

        use nix::libc;

        /// What a call returned: `ok` or the name of its error code.
        fn outcome(ret: c_long) -> String {
            if ret >= 0 {
                "ok".to_owned()
            } else {
                format!("{:?}", nix::errno::Errno::last())
            }
        }

        /// Makes the call `nr` with `args`.
        fn call(nr: c_long, args: [u64; 6]) -> String {
            // SAFETY: each call of the probe is given null pointers, a path
            // that does not exist, a descriptor that is not open, a buffer
            // of this frame, or flags that the kernel refuses, so that none
            // writes to memory of this process that it does not own.
            let ret =
                unsafe { libc::syscall(nr, args[0], args[1], args[2], args[3], args[4], args[5]) };
            outcome(ret)
        }

        fn path(text: &'static CStr) -> u64 {
            text.as_ptr() as u64
        }

        /// `CLONE_THREAD` without `CLONE_SIGHAND`, which the kernel refuses
        /// with `EINVAL` before it creates anything.
        const INVALID_CLONE: u64 = 0x0001_0000;

        /// The flags of `clone` that create a namespace, by name.
        const NAMESPACES: &[(&str, u64)] = &[
            ("CLONE_NEWNS", 0x0002_0000),
            ("CLONE_NEWCGROUP", 0x0200_0000),
            ("CLONE_NEWUTS", 0x0400_0000),
            ("CLONE_NEWIPC", 0x0800_0000),
            ("CLONE_NEWUSER", 0x1000_0000),
            ("CLONE_NEWPID", 0x2000_0000),
            ("CLONE_NEWNET", 0x4000_0000),
        ];

        /// Every call of the table that this architecture has, one or more
        /// for each row, with what it returned.
        pub fn rows() -> Vec<(String, String)> {
            let none = c"/sf-no-such-path";
            let empty = c"";
            let mut buffer = [0_u8; 64];
            let buffer = buffer.as_mut_ptr() as u64;
            let pid = u64::from(std::process::id());
            let bad = u64::MAX; // -1, a descriptor that is not open.
            let mut calls: Vec<(String, c_long, [u64; 6])> = vec![
                ("unshare".into(), libc::SYS_unshare, [0; 6]),
                ("setns".into(), libc::SYS_setns, [bad, 0, 0, 0, 0, 0]),
            ];
            for (name, flag) in NAMESPACES {
                calls.push((
                    format!("clone {name}"),
                    libc::SYS_clone,
                    [flag | INVALID_CLONE, 0, 0, 0, 0, 0],
                ));
            }
            calls.extend([
                (
                    "clone without a namespace".into(),
                    libc::SYS_clone,
                    [INVALID_CLONE, 0, 0, 0, 0, 0],
                ),
                ("clone3".into(), libc::SYS_clone3, [0; 6]),
                ("mount".into(), libc::SYS_mount, [0, path(none), 0, 0, 0, 0]),
                (
                    "umount2".into(),
                    libc::SYS_umount2,
                    [path(none), 0, 0, 0, 0, 0],
                ),
                (
                    "pivot_root".into(),
                    libc::SYS_pivot_root,
                    [path(none), path(none), 0, 0, 0, 0],
                ),
                (
                    "chroot".into(),
                    libc::SYS_chroot,
                    [path(none), 0, 0, 0, 0, 0],
                ),
                (
                    "open_tree".into(),
                    libc::SYS_open_tree,
                    [bad, path(empty), 0, 0, 0, 0],
                ),
                (
                    "move_mount".into(),
                    libc::SYS_move_mount,
                    [bad, path(empty), bad, path(empty), 0, 0],
                ),
                ("fsopen".into(), libc::SYS_fsopen, [0; 6]),
                ("fsconfig".into(), libc::SYS_fsconfig, [bad, 0, 0, 0, 0, 0]),
                ("fsmount".into(), libc::SYS_fsmount, [bad, 0, 0, 0, 0, 0]),
                (
                    "fspick".into(),
                    libc::SYS_fspick,
                    [bad, path(empty), 0, 0, 0, 0],
                ),
                (
                    "mount_setattr".into(),
                    libc::SYS_mount_setattr,
                    [bad, path(empty), 0, 0, 0, 0],
                ),
                ("add_key".into(), libc::SYS_add_key, [0; 6]),
                ("keyctl".into(), libc::SYS_keyctl, [bad, 0, 0, 0, 0, 0]),
                ("request_key".into(), libc::SYS_request_key, [0; 6]),
                // SYSLOG_ACTION_SIZE_BUFFER.
                ("syslog".into(), libc::SYS_syslog, [10, 0, 0, 0, 0, 0]),
                ("acct".into(), libc::SYS_acct, [0; 6]),
                ("quotactl".into(), libc::SYS_quotactl, [0; 6]),
                ("move_pages".into(), libc::SYS_move_pages, [0; 6]),
                ("mbind".into(), libc::SYS_mbind, [0; 6]),
                ("get_mempolicy".into(), libc::SYS_get_mempolicy, [0; 6]),
                ("set_mempolicy".into(), libc::SYS_set_mempolicy, [0; 6]),
                ("migrate_pages".into(), libc::SYS_migrate_pages, [0; 6]),
                (
                    "perf_event_open".into(),
                    libc::SYS_perf_event_open,
                    [0, 0, bad, bad, 0, 0],
                ),
                // PTRACE_PEEKDATA of a pid that does not exist.
                ("ptrace".into(), libc::SYS_ptrace, [2, bad, 0, 0, 0, 0]),
                // The query of the current domain.
                (
                    "personality 0xffffffff".into(),
                    libc::SYS_personality,
                    [0xFFFF_FFFF, 0, 0, 0, 0, 0],
                ),
                (
                    "personality PER_LINUX".into(),
                    libc::SYS_personality,
                    [0; 6],
                ),
                (
                    "process_vm_readv".into(),
                    libc::SYS_process_vm_readv,
                    [pid, 0, 0, 0, 0, 0],
                ),
                (
                    "process_vm_writev".into(),
                    libc::SYS_process_vm_writev,
                    [pid, 0, 0, 0, 0, 0],
                ),
                ("bpf".into(), libc::SYS_bpf, [0; 6]),
                // UFFD_USER_MODE_ONLY, which an unprivileged process may ask for.
                (
                    "userfaultfd".into(),
                    libc::SYS_userfaultfd,
                    [1, 0, 0, 0, 0, 0],
                ),
                ("io_uring_setup".into(), libc::SYS_io_uring_setup, [0; 6]),
                (
                    "io_uring_enter".into(),
                    libc::SYS_io_uring_enter,
                    [bad, 0, 0, 0, 0, 0],
                ),
                (
                    "io_uring_register".into(),
                    libc::SYS_io_uring_register,
                    [bad, 0, 0, 0, 0, 0],
                ),
                // On stdin, which is no terminal.
                (
                    "ioctl TIOCSTI".into(),
                    libc::SYS_ioctl,
                    [0, 0x5412, buffer, 0, 0, 0],
                ),
                (
                    "ioctl TIOCSTI with high bits".into(),
                    libc::SYS_ioctl,
                    [0, 0xFFFF_FFFF_0000_5412, buffer, 0, 0, 0],
                ),
                (
                    "ioctl TIOCLINUX".into(),
                    libc::SYS_ioctl,
                    [0, 0x541C, buffer, 0, 0, 0],
                ),
                (
                    "ioctl TCGETS".into(),
                    libc::SYS_ioctl,
                    [0, 0x5401, buffer, 0, 0, 0],
                ),
                // AF_ALG and AF_PACKET; AF_UNIX, with high bits too; AF_INET,
                // AF_INET6 and AF_NETLINK.
                (
                    "socket AF_ALG".into(),
                    libc::SYS_socket,
                    [38, 5, 0, 0, 0, 0],
                ),
                (
                    "socket AF_PACKET".into(),
                    libc::SYS_socket,
                    [17, 3, 0, 0, 0, 0],
                ),
                (
                    "socket AF_UNIX".into(),
                    libc::SYS_socket,
                    [1, 1, 0, 0, 0, 0],
                ),
                (
                    "socket AF_UNIX with high bits".into(),
                    libc::SYS_socket,
                    [0x1_0000_0001, 1, 0, 0, 0, 0],
                ),
                (
                    "socket AF_INET".into(),
                    libc::SYS_socket,
                    [2, 1, 0, 0, 0, 0],
                ),
                (
                    "socket AF_INET6".into(),
                    libc::SYS_socket,
                    [10, 1, 0, 0, 0, 0],
                ),
                (
                    "socket AF_NETLINK".into(),
                    libc::SYS_socket,
                    [16, 3, 0, 0, 0, 0],
                ),
                ("getpid".into(), libc::SYS_getpid, [0; 6]),
            ]);
            #[cfg(target_arch = "x86_64")]
            calls.extend([
                ("uselib".into(), libc::SYS_uselib, [0; 6]),
                // Read the LDT, into no buffer.
                ("modify_ldt".into(), libc::SYS_modify_ldt, [0; 6]),
                // `getpid` of the x32 ABI.
                ("x32 getpid".into(), libc::SYS_getpid | 0x4000_0000, [0; 6]),
            ]);
            calls
                .into_iter()
                .map(|(label, nr, args)| (label, call(nr, args)))
                .collect()
        }

        /// `getpid` of the 32-bit ABI of x86-64, through `int 0x80`. On
        /// arm64 there is none to make: a process of 64 bits cannot make a
        /// call of 32, and the processors of the VM and of the hosted runner
        /// have no 32-bit mode at all.
        pub fn another_architecture() {
            #[cfg(target_arch = "x86_64")]
            {
                let mut ret: i64 = 20;
                // SAFETY: `getpid`, number 20 of the i386 ABI, takes no
                // argument and touches no memory.
                unsafe {
                    std::arch::asm!("int 0x80", inout("rax") ret);
                }
                println!("returned {ret}");
            }
        }
    }

    fn probe_policy(scratch: &Scratch) -> SandboxPolicy {
        let mut policy = scratch.policy();
        policy.mounts.push(Mount {
            host: std::env::current_exe().unwrap(),
            sandbox: PROBE.into(),
            access: Access::ReadOnly,
        });
        policy
    }

    /// Runs `script` in an instance that shows this binary at [`PROBE`].
    async fn run_probe(scratch: &Scratch, script: &str) -> Vec<String> {
        let policy = probe_policy(scratch);
        let instance = backend().create(RunId(1), &policy).await.unwrap();
        let process = spawn(instance.as_ref(), &sh(script, &[])).await;
        let (code, lines) = output(process).await;
        instance.destroy().await.unwrap();
        assert_eq!(code, Some(0), "{lines:?}");
        lines
    }

    fn one_call_of_each_row_gets_its_code() {
        within(async {
            let scratch = Scratch::new("filter");
            let lines = run_probe(&scratch, "/probe __call rows").await;

            let mut expected: Vec<&str> = vec![
                "unshare EPERM",
                "setns EPERM",
                "clone CLONE_NEWNS EPERM",
                "clone CLONE_NEWCGROUP EPERM",
                "clone CLONE_NEWUTS EPERM",
                "clone CLONE_NEWIPC EPERM",
                "clone CLONE_NEWUSER EPERM",
                "clone CLONE_NEWPID EPERM",
                "clone CLONE_NEWNET EPERM",
                // The filter lets it through, and the kernel refuses the
                // flags.
                "clone without a namespace EINVAL",
                "clone3 ENOSYS",
                "mount EPERM",
                "umount2 EPERM",
                "pivot_root EPERM",
                "chroot EPERM",
                "open_tree ENOSYS",
                "move_mount ENOSYS",
                "fsopen ENOSYS",
                "fsconfig ENOSYS",
                "fsmount ENOSYS",
                "fspick ENOSYS",
                "mount_setattr ENOSYS",
                "add_key EPERM",
                "keyctl EPERM",
                "request_key EPERM",
                "syslog EPERM",
                "acct EPERM",
                "quotactl EPERM",
                "move_pages EPERM",
                "mbind EPERM",
                "get_mempolicy EPERM",
                "set_mempolicy EPERM",
                "migrate_pages EPERM",
                "perf_event_open EPERM",
                "ptrace EPERM",
                "personality 0xffffffff EPERM",
                "personality PER_LINUX ok",
                "process_vm_readv EPERM",
                "process_vm_writev EPERM",
                "bpf EPERM",
                "userfaultfd EPERM",
                "io_uring_setup ENOSYS",
                "io_uring_enter ENOSYS",
                "io_uring_register ENOSYS",
                "ioctl TIOCSTI EPERM",
                "ioctl TIOCSTI with high bits EPERM",
                "ioctl TIOCLINUX EPERM",
                "ioctl TCGETS ENOTTY",
                "socket AF_ALG EAFNOSUPPORT",
                "socket AF_PACKET EAFNOSUPPORT",
                "socket AF_UNIX ok",
                "socket AF_UNIX with high bits ok",
                "socket AF_INET ok",
                "socket AF_INET6 ok",
                "socket AF_NETLINK ok",
                "getpid ok",
            ];
            if cfg!(target_arch = "x86_64") {
                expected.extend(["uselib EPERM", "modify_ldt EPERM", "x32 getpid ENOSYS"]);
            }

            assert_eq!(lines, expected);
        });
    }

    fn a_call_of_another_architecture_ends_the_process() {
        within(async {
            let scratch = Scratch::new("another-arch");
            let lines = run_probe(
                &scratch,
                "/probe __call another-architecture; echo \"status $?\"",
            )
            .await;

            if cfg!(target_arch = "x86_64") {
                // Killed by SIGSYS, 31, before it printed anything.
                assert_eq!(lines, ["status 159"]);
            } else {
                // No call of another architecture can be made here.
                assert_eq!(lines, ["another-architecture returned", "status 0"]);
            }
        });
    }

    fn the_worker_inherits_no_descriptor_of_the_filter() {
        within(async {
            let scratch = Scratch::new("filter-fds");
            let lines = run_probe(&scratch, "/probe __call descriptors").await;

            // The worker may inherit descriptors that the process which
            // started the tests left open, so only those of the filter are
            // looked for.
            assert!(!lines.is_empty());
            assert!(
                !lines.iter().any(|line| line.contains("sf-seccomp")),
                "{lines:?}"
            );
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
        (
            "one_call_of_each_row_gets_its_code",
            one_call_of_each_row_gets_its_code,
        ),
        (
            "a_call_of_another_architecture_ends_the_process",
            a_call_of_another_architecture_ends_the_process,
        ),
        (
            "the_worker_inherits_no_descriptor_of_the_filter",
            the_worker_inherits_no_descriptor_of_the_filter,
        ),
        (
            "the_first_process_is_pid_1_and_the_binary_is_read_only",
            the_first_process_is_pid_1_and_the_binary_is_read_only,
        ),
        (
            "the_first_process_reaps_a_process_left_to_it",
            the_first_process_reaps_a_process_left_to_it,
        ),
        (
            "a_connection_to_the_port_reaches_the_proxy_and_no_other",
            a_connection_to_the_port_reaches_the_proxy_and_no_other,
        ),
        (
            "a_sigint_and_a_sigterm_to_the_first_process_reach_the_command",
            a_sigint_and_a_sigterm_to_the_first_process_reach_the_command,
        ),
        (
            "after_a_sigkill_to_the_first_process_no_process_of_the_sandbox_is_left",
            after_a_sigkill_to_the_first_process_no_process_of_the_sandbox_is_left,
        ),
        (
            "a_proxy_that_is_no_socket_creates_no_instance",
            a_proxy_that_is_no_socket_creates_no_instance,
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
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some(linux::INIT) {
        return linux::first_process();
    }
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some(linux::CALL) {
        return linux::call(std::env::args().nth(2).as_deref().unwrap_or(""));
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

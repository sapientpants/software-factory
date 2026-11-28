//! The sandbox tests of SPEC 19.4 for `sf sandbox exec` (CLI-6), at level
//! `process`: the bubblewrap backend, the egress proxy of SEC-10b and the
//! supervisor shim, joined as the command joins them. A listener on the
//! loopback interface stands in for a listed host, which the proxy is given
//! as the function that reaches a name (SEC-10b), so no test needs the
//! network (DEV-6).
//!
//! The binary has a harness of its own, as that of `factory-sandbox` has
//! (DEV-2a). Run as `<binary> __supervise ...` it is the supervisor shim of
//! AG-16, and as `<binary> __sandbox-init ...` the first process of SEC-10a,
//! each as `sf` runs it; the backend shows the binary as the `sf` binary.
//! Run as `<binary> __exec [--json] <dir> <program> [<arg>...]` it is
//! `sf sandbox exec --dir <dir> -- <program> [<arg>...]`, with the operator
//! configuration that `HOME` holds, and as `<binary> __doctor` it is
//! `sf doctor`.
//! Otherwise its tests are ignored unless [`GATE`] holds `process`, so the
//! plain test run runs none of them, and the sandbox step of the check and
//! the mutation step set the variable. A test fails where bubblewrap is
//! missing or cannot start: it is never skipped (DEV-11).

use std::process::ExitCode;

#[cfg(target_os = "linux")]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test binary whose harness is its own, which clippy does not see as test code (19.3)"
)]
mod linux {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use factory_core::config::OperatorConfig;
    use factory_core::ecosystem::{HostTriple, Rust};
    use factory_daemon::doctor;
    use factory_daemon::process::Supervisor;
    use factory_daemon::sandbox_exec::{
        self, DIRECTORY, Ended, Form, Host, Report, Request, Session,
    };
    use factory_net::proxy::Reason;
    use factory_sandbox::bubblewrap::Bubblewrap;
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};
    use tokio::net::{TcpListener, TcpStream};

    /// The first argument that makes this binary the shim.
    pub const SHIM: &str = factory_daemon::supervise::SUBCOMMAND;

    /// The first argument that makes this binary the first process.
    pub const INIT: &str = factory_daemon::sandbox_init::SUBCOMMAND;

    /// The first argument that makes this binary `sf sandbox exec`.
    pub const EXEC: &str = "__exec";

    /// The first argument that makes this binary `sf doctor`.
    pub const DOCTOR: &str = "__doctor";

    /// `sf sandbox exec` with the arguments that follow [`EXEC`].
    pub fn exec_main() -> std::process::ExitCode {
        let mut args = std::env::args_os().skip(2).peekable();
        let json = args.next_if(|arg| arg == "--json").is_some();
        let dir = args.next().expect("<dir>");
        let program = args.next().expect("<program>");
        let request = Request {
            dir: dir.into(),
            target: None,
            tools: Vec::new(),
            form: Form::Command {
                program,
                args: args.collect(),
            },
        };
        sandbox_exec::main(&request, json)
    }

    /// The listed name that the stand-in answers for.
    const LISTED: &str = "listed.test";

    /// A listed name that the proxy cannot reach.
    const DOWN: &str = "down.test";

    /// The longest that a test may take: less than the timeout that the
    /// mutation step gives a mutant's tests, so that a mutant under which a
    /// test hangs fails it and is caught.
    const LIMIT: Duration = Duration::from_secs(15);

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

    /// A fresh directory for one test, with the data directory and the
    /// directory of the request, removed when the value is dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Self {
            let path =
                Path::new("/tmp").join(format!("sf-exec-sandbox-{}-{test}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            for dir in ["data", "work"] {
                std::fs::create_dir_all(path.join(dir)).unwrap();
            }
            Self(path)
        }

        fn config(&self) -> OperatorConfig {
            let text = format!(
                "schema_version = 1\ndata_dir = {:?}\n[sandbox]\nlevel = \"process\"\n[egress]\nextra_hosts = [\"{LISTED}\", \"{DOWN}\"]\n",
                self.0.join("data")
            );
            OperatorConfig::parse(text.as_bytes()).unwrap()
        }

        fn request(&self, script: &str) -> Request {
            Request {
                dir: self.0.join("work"),
                target: None,
                tools: Vec::new(),
                form: Form::Command {
                    program: "/bin/bash".into(),
                    args: vec!["-c".into(), script.into()],
                },
            }
        }

        /// What is left below the data directory's `sandbox-exec`.
        fn left(&self) -> Vec<OsString> {
            std::fs::read_dir(self.0.join("data").join(DIRECTORY))
                .unwrap()
                .flatten()
                .map(|entry| entry.file_name())
                .collect()
        }
    }

    /// A directory that is removed when the value is dropped.
    struct Removed(PathBuf);

    impl Drop for Removed {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Runs `request` with `config` in an instance of the backend, with a
    /// proxy that reaches [`LISTED`] at `stand_in` and no other name, and the
    /// command's stdout as lines.
    async fn exec(
        config: &OperatorConfig,
        request: &Request,
        stand_in: u16,
    ) -> (Report, Vec<String>) {
        let backend = backend();
        let connect = move |name: String| async move {
            if name == LISTED {
                TcpStream::connect(("127.0.0.1", stand_in)).await
            } else {
                Err(std::io::Error::other(format!(
                    "{name} is reached by no test"
                )))
            }
        };
        let host = Host {
            sandbox: &backend,
            connect,
            ecosystem: &Rust,
            triple: HostTriple::THIS.unwrap(),
        };
        let mut stdout = Vec::new();
        let report = sandbox_exec::run(
            config,
            request,
            host,
            &mut stdout,
            tokio::io::stderr(),
            std::future::pending(),
        )
        .await
        .unwrap();
        let text = String::from_utf8(stdout).unwrap();
        (
            report,
            text.lines()
                .map(|line| line.trim_end().to_owned())
                .collect(),
        )
    }

    /// A listener on the loopback interface that stands in for the listed
    /// host: it answers each line it is sent with one of its own.
    async fn stand_in(listener: &TcpListener) -> String {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = tokio::io::BufReader::new(stream);
        let mut line = String::new();
        stream.read_line(&mut line).await.unwrap();
        stream
            .get_mut()
            .write_all(b"from the host\n")
            .await
            .unwrap();
        line
    }

    fn a_worker_reaches_a_listed_host_through_the_proxy_and_no_other() {
        within(async {
            let scratch = Scratch::new("egress");
            let host = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = host.local_addr().unwrap().port();
            // A CONNECT to the listed name, through the proxy variable; one to
            // a name that is not listed; and the stand-in's own port, which
            // the sandbox's loopback interface does not reach.
            let script = format!(
                r#"
                connect() {{
                    exec 3<>"/dev/tcp/127.0.0.1/${{HTTPS_PROXY##*:}}"
                    printf 'CONNECT %s HTTP/1.1\r\nHost: %s\r\n\r\n' "$1" "$1" >&3
                    read -r status <&3
                    echo "$1: $status"
                    while read -r line <&3 && [ "$line" != $'\r' ]; do :; done
                }}
                connect {LISTED}:443
                echo hello >&3
                read -r answer <&3
                echo "answer: $answer"
                exec 3<&-
                connect unlisted.test:443
                exec 3<&-
                connect {LISTED}:80
                exec 3<&-
                connect {DOWN}:443
                exec 3<&-
                (echo x > /dev/tcp/127.0.0.1/{port}) 2>/dev/null && echo HOST
                echo done
                "#
            );
            let request = scratch.request(&script);
            let config = scratch.config();
            // The stand-in is waited for while the command lives, and not
            // after it: a command that never reaches it fails the test at
            // once.
            let run = exec(&config, &request, port);
            let mut run = std::pin::pin!(run);
            let mut heard = None;
            let (report, lines) = loop {
                tokio::select! {
                    outcome = &mut run => break outcome,
                    line = stand_in(&host), if heard.is_none() => heard = Some(line),
                }
            };

            assert_eq!(report.code(), 0, "{lines:?}");
            assert_eq!(heard.as_deref(), Some("hello\n"), "{lines:?}");
            assert_eq!(
                lines,
                [
                    "listed.test:443: HTTP/1.1 200 OK",
                    "answer: from the host",
                    "unlisted.test:443: HTTP/1.1 403 Forbidden",
                    "listed.test:80: HTTP/1.1 403 Forbidden",
                    "down.test:443: HTTP/1.1 502 Bad Gateway",
                    "done",
                ]
            );
            let denied: Vec<(Reason, Option<String>)> = report
                .denials
                .iter()
                .map(|denial| {
                    (
                        denial.reason,
                        denial.sent.as_ref().map(|sent| sent.target.clone()),
                    )
                })
                .collect();
            assert_eq!(
                denied,
                [
                    (Reason::Host, Some("unlisted.test:443".to_owned())),
                    (Reason::Port, Some("listed.test:80".to_owned())),
                ]
            );
            // A listed host that cannot be reached is no denial (SEC-10b).
            let failed: Vec<&str> = report
                .failures
                .iter()
                .map(|failure| failure.name.as_str())
                .collect();
            assert_eq!(failed, [DOWN]);
            // One connection reached the stand-in, the proxy's.
            assert!(
                tokio::time::timeout(Duration::from_millis(200), host.accept())
                    .await
                    .is_err()
            );
            assert!(scratch.left().is_empty());
        });
    }

    fn the_command_gets_a_new_home_and_target_directory_which_are_removed() {
        within(async {
            let scratch = Scratch::new("home");
            let script = r#"
                echo "home=$HOME"
                echo "path=$PATH"
                echo "target=$CARGO_TARGET_DIR"
                echo "sandbox=$SF_SANDBOX"
                pwd
                echo "empty: [$(ls -A "$HOME")] [$(ls -A /target)]"
                printf 12345 > /target/five && touch "$HOME/h" /work/w && echo wrote
                env | cut -d= -f1 | sort | tr '\n' ' '; echo
                exit 3
                "#;
            let (report, lines) = exec(&scratch.config(), &scratch.request(script), 0).await;

            assert_eq!(report.code(), 3, "{lines:?}");
            assert_eq!(
                lines[..7],
                [
                    "home=/home/worker",
                    "path=/usr/bin:/bin",
                    "target=/target",
                    "sandbox=process",
                    "/work",
                    "empty: [] []",
                    "wrote",
                ]
            );
            // The allowlist of AG-11, and what the shell sets itself.
            assert_eq!(
                lines[7],
                "CARGO_HOME CARGO_TARGET_DIR HOME HTTPS_PROXY HTTP_PROXY LANG NO_PROXY PATH PWD SF_SANDBOX SHLVL TMPDIR _ http_proxy https_proxy no_proxy"
            );
            assert_eq!(report.target_bytes, 5);
            assert!(report.denials.is_empty());
            assert!(scratch.0.join("work/w").exists());
            assert!(scratch.left().is_empty(), "{:?}", scratch.left());
        });
    }

    fn a_named_target_directory_is_used_and_left_as_it_is() {
        within(async {
            let scratch = Scratch::new("target");
            let target = scratch.0.join("kept");
            std::fs::create_dir(&target).unwrap();
            std::fs::write(target.join("seed"), "seed").unwrap();
            let mut request = scratch.request("cat /target/seed && printf 12 > /target/more");
            request.target = Some(target.clone());

            let (report, lines) = exec(&scratch.config(), &request, 0).await;

            assert_eq!(report.code(), 0, "{lines:?}");
            assert_eq!(lines, ["seed"]);
            assert_eq!(report.target_bytes, 6);
            assert_eq!(std::fs::read_to_string(target.join("more")).unwrap(), "12");
            assert!(scratch.left().is_empty());
        });
    }

    /// A home directory whose operator configuration has the data directory
    /// of `scratch`, for `sf sandbox exec` as the binary runs it.
    fn home(scratch: &Scratch) -> PathBuf {
        let home = scratch.0.join("home");
        std::fs::create_dir_all(home.join(".config/sf")).unwrap();
        std::fs::write(
            home.join(".config/sf/config.toml"),
            format!(
                "schema_version = 1\ndata_dir = {:?}\n[sandbox]\nlevel = \"process\"\n",
                scratch.0.join("data")
            ),
        )
        .unwrap();
        home
    }

    /// This binary as `sf sandbox exec` in `scratch`, with `args` after
    /// [`EXEC`].
    async fn sf(
        scratch: &Scratch,
        args: &[&str],
    ) -> (factory_daemon::process::Process, tokio::io::DuplexStream) {
        let mut command = factory_daemon::process::Command::new(std::env::current_exe().unwrap());
        command
            .arg(EXEC)
            .args(args)
            .env("HOME", home(scratch))
            .env("PATH", std::env::var_os("PATH").unwrap());
        let (sink, read) = tokio::io::duplex(1 << 20);
        (command.spawn(sink).await.unwrap(), read)
    }

    /// The lines of text of `process` to its end.
    async fn lines(process: &mut factory_daemon::process::Process) -> Vec<String> {
        let mut lines = Vec::new();
        while let Some(line) = process.next_line().await.unwrap() {
            match line {
                factory_daemon::process::Line::Text(text) => lines.push(text),
                other => panic!("not a line of text: {other:?}"),
            }
        }
        lines
    }

    async fn text(mut read: tokio::io::DuplexStream) -> String {
        let mut text = String::new();
        tokio::io::AsyncReadExt::read_to_string(&mut read, &mut text)
            .await
            .unwrap();
        text
    }

    fn sf_reports_and_exits_with_the_status_of_the_command() {
        within(async {
            let scratch = Scratch::new("sf");
            let work = scratch.0.join("work");
            let work = work.to_str().unwrap();

            let (mut process, stderr) = sf(
                &scratch,
                &[work, "/bin/sh", "-c", "echo out; echo err >&2; exit 3"],
            )
            .await;
            let out = lines(&mut process).await;
            let code = process.wait().await.unwrap().code();
            drop(process);
            let stderr = text(stderr).await;

            assert_eq!(code, Some(3), "{stderr}");
            assert_eq!(out, ["out"]);
            assert!(
                stderr.starts_with(
                    "err\nsf sandbox exec: the command exited with 3\n  wall-clock time: "
                ),
                "{stderr}"
            );
            assert!(
                stderr.ends_with("  target directory: 0 bytes\n  denied requests: none\n"),
                "{stderr}"
            );

            let (mut process, stderr) = sf(&scratch, &["--json", work, "/bin/true"]).await;
            let code = process.wait().await.unwrap().code();
            drop(process);
            let stderr = text(stderr).await;
            assert_eq!(code, Some(0), "{stderr}");
            let report: serde_json::Value = serde_json::from_str(stderr.trim_end()).unwrap();
            assert_eq!(report["exit_status"], 0);
            assert_eq!(report["denied_requests"], serde_json::json!([]));
            assert!(scratch.left().is_empty());
        });
    }

    fn a_sigint_to_sf_stops_the_command_and_leaves_nothing_behind() {
        within(async {
            let scratch = Scratch::new("sigint");
            let work = scratch.0.join("work");
            let (mut process, stderr) = sf(
                &scratch,
                &[
                    work.to_str().unwrap(),
                    "/bin/sh",
                    "-c",
                    // The trap, and not the SIGINT's default action, ends
                    // the shell: a SIGINT that comes while the shell starts
                    // `sleep`, between its fork and the exec, is lost to the
                    // child, which then sleeps on. The SIGINT reaches the
                    // command through the first process (SEC-10a).
                    "trap 'exit 130' INT; echo started; while :; do sleep 0.1; done",
                ],
            )
            .await;
            let first = tokio::time::timeout(Duration::from_secs(10), process.next_line())
                .await
                .expect("the command starts within 10 s");
            assert_eq!(
                first.unwrap(),
                Some(factory_daemon::process::Line::Text("started".to_owned()))
            );

            // The SIGINT goes to `sf` alone, as a terminal's would, since the
            // shim leads a group of its own (AG-16).
            let times = factory_daemon::process::StopTimes {
                interrupt: Duration::from_secs(5),
                terminate: Duration::from_secs(2),
            };
            let stopped = process.stop(times, |_| false).await.unwrap();
            drop(process);
            let stderr = text(stderr).await;

            assert_eq!(
                stopped.by,
                factory_daemon::process::StoppedBy::Interrupt,
                "{stderr}"
            );
            assert_ne!(stopped.status.code(), Some(0), "{stderr}");
            assert!(
                stderr.contains("sf sandbox exec: the command exited with "),
                "{stderr}"
            );
            assert!(scratch.left().is_empty(), "{:?}", scratch.left());
        });
    }

    /// The agent form with a stand-in for the agent CLI, which the sandbox
    /// shows at its path: what the session sees of the files of AG-4 and
    /// AG-8, its token and its message, and that the host ends it at its
    /// `result` line. The stand-in lies in a directory of its own below
    /// `HOME`: no mount is shown below `/tmp` (SEC-6f), where the mutation
    /// step copies the tree and its build.
    fn the_agent_form_runs_a_session_with_its_configuration_read_only() {
        within(async {
            let scratch = Scratch::new("agent");
            let cli_dir = Removed(
                Path::new(&std::env::var_os("HOME").expect("HOME is set"))
                    .join(format!(".sf-exec-sandbox-{}-agent", std::process::id())),
            );
            let _ = std::fs::remove_dir_all(&cli_dir.0);
            std::fs::create_dir_all(&cli_dir.0).unwrap();
            let cli = cli_dir.0.join("claude");
            std::fs::write(
                &cli,
                r#"#!/bin/sh
trap 'exit 0' INT
IFS= read -r line
echo "message: $line"
touch /agent/x 2>/dev/null && echo agent-writable || echo agent-read-only
{ echo x >> "$HOME/.claude/settings.json"; } 2>/dev/null && echo settings-writable || echo settings-read-only
mkdir "$HOME/.claude/skills/s" 2>/dev/null && echo skills-writable || echo skills-read-only
touch "$HOME/.claude/.claude.json" && echo transcripts-writable
echo "settings: $(cat /agent/settings.json)"
echo "token: ${#CLAUDE_CODE_OAUTH_TOKEN}"
echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash"}]}}'
echo '{"type":"result","subtype":"success"}'
while :; do sleep 0.1; done
"#,
            )
            .unwrap();
            std::fs::set_permissions(&cli, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .unwrap();
            let token = scratch.0.join("token");
            std::fs::write(&token, format!("{}\n", "t".repeat(40))).unwrap();
            let config = OperatorConfig::parse(
                format!(
                    "schema_version = 1\ndata_dir = {:?}\n[credentials]\nsubscription_token_file = {token:?}\n[agent]\ncli = {cli:?}\n[sandbox]\nlevel = \"process\"\n",
                    scratch.0.join("data")
                )
                .as_bytes(),
            )
            .unwrap();
            let request = Request {
                dir: scratch.0.join("work"),
                target: None,
                tools: Vec::new(),
                form: Form::Agent {
                    model: "m".to_owned(),
                    prompt: "hello".to_owned(),
                },
            };

            let (report, lines) = exec(&config, &request, 0).await;
            drop(cli_dir);

            assert_eq!(
                report.ended,
                Ended::Session(Session {
                    result: Some("success".to_owned()),
                    bash_uses: 1
                }),
                "{lines:?}"
            );
            assert_eq!(report.code(), 0);
            let message = lines
                .first()
                .and_then(|line| line.strip_prefix("message: "))
                .unwrap_or_else(|| panic!("{lines:?}"));
            let message: serde_json::Value = serde_json::from_str(message).unwrap();
            assert_eq!(message["message"]["content"][0]["text"], "hello");
            assert_eq!(
                lines[1..7],
                [
                    "agent-read-only",
                    "settings-read-only",
                    "skills-read-only",
                    "transcripts-writable",
                    r#"settings: {"sandbox":{"enabled":false}}"#,
                    "token: 40",
                ]
            );
            assert!(report.denials.is_empty());
            assert!(scratch.left().is_empty(), "{:?}", scratch.left());
        });
    }

    /// The sandbox check of `sf doctor` (CLI-1): an instance at the
    /// configured level starts and runs a command, and its working
    /// directory is removed; at a level that the backend cannot provide the
    /// check fails and says which.
    fn the_sandbox_check_of_sf_doctor_starts_an_instance_at_the_configured_level() {
        within(async {
            let scratch = Scratch::new("doctor");
            let backend = backend();
            let host = || Host {
                sandbox: &backend,
                connect: factory_net::proxy::resolve,
                ecosystem: &Rust,
                triple: HostTriple::THIS.unwrap(),
            };

            let check = doctor::sandbox(&scratch.config(), host(), std::future::pending()).await;
            assert!(check.ok, "{check:?}");
            assert!(check.text.contains("`process`"), "{}", check.text);
            let work = scratch.0.join("data").join(doctor::DIRECTORY);
            assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);
            assert!(scratch.left().is_empty());

            let hardened = OperatorConfig::parse(
                format!(
                    "schema_version = 1\ndata_dir = {:?}\n[sandbox]\nlevel = \"hardened\"\n",
                    scratch.0.join("data")
                )
                .as_bytes(),
            )
            .unwrap();
            let check = doctor::sandbox(&hardened, host(), std::future::pending()).await;
            assert!(!check.ok, "{check:?}");
            assert!(check.text.contains("`hardened`"), "{}", check.text);
        });
    }

    /// This binary as `sf doctor`, with `HOME` at `home`: its exit code and
    /// its stdout.
    async fn doctor_main(home: &Path) -> (Option<i32>, String) {
        let mut command = factory_daemon::process::Command::new(std::env::current_exe().unwrap());
        command
            .arg(DOCTOR)
            .env("HOME", home)
            .env("PATH", std::env::var_os("PATH").unwrap());
        let mut process = command.spawn(tokio::io::sink()).await.unwrap();
        let out = lines(&mut process).await;
        let code = process.wait().await.unwrap().code();
        (code, out.join("\n"))
    }

    /// `sf doctor` as the binary runs it (CLI-1): every check of the M0 row
    /// passes on this host with a configuration whose files are in order,
    /// and it fails where a credential file has another mode than 0600. A
    /// stand-in for the agent CLI prints a version.
    fn sf_doctor_passes_where_every_check_passes_and_fails_otherwise() {
        within(async {
            let scratch = Scratch::new("doctor-main");
            let cli = scratch.0.join("claude");
            std::fs::write(&cli, "#!/bin/sh\necho '2.1.3 (Claude Code)'\n").unwrap();
            std::fs::set_permissions(&cli, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .unwrap();
            let token = scratch.0.join("token");
            std::fs::write(&token, "t".repeat(40)).unwrap();
            std::fs::set_permissions(&token, std::os::unix::fs::PermissionsExt::from_mode(0o600))
                .unwrap();
            let home = scratch.0.join("home");
            std::fs::create_dir_all(home.join(".config/sf")).unwrap();
            std::fs::write(
                home.join(".config/sf/config.toml"),
                format!(
                    "schema_version = 1\ndata_dir = {:?}\n[credentials]\nsubscription_token_file = {token:?}\n[sandbox]\nlevel = \"process\"\n[agent]\ncli = {cli:?}\n",
                    scratch.0.join("data")
                ),
            )
            .unwrap();

            let (code, out) = doctor_main(&home).await;
            assert_eq!(code, Some(0), "{out}");
            assert_eq!(out.lines().count(), 7, "{out}");
            assert!(out.lines().all(|line| line.starts_with("ok  ")), "{out}");

            std::fs::set_permissions(&token, std::os::unix::fs::PermissionsExt::from_mode(0o644))
                .unwrap();
            let (code, out) = doctor_main(&home).await;
            assert_eq!(code, Some(1), "{out}");
            assert!(out.contains("has mode 0644, not 0600"), "{out}");
            assert!(scratch.left().is_empty());
        });
    }

    /// The tests, each with its name.
    pub const TESTS: &[(&str, fn())] = &[
        (
            "a_worker_reaches_a_listed_host_through_the_proxy_and_no_other",
            a_worker_reaches_a_listed_host_through_the_proxy_and_no_other,
        ),
        (
            "the_command_gets_a_new_home_and_target_directory_which_are_removed",
            the_command_gets_a_new_home_and_target_directory_which_are_removed,
        ),
        (
            "a_named_target_directory_is_used_and_left_as_it_is",
            a_named_target_directory_is_used_and_left_as_it_is,
        ),
        (
            "sf_reports_and_exits_with_the_status_of_the_command",
            sf_reports_and_exits_with_the_status_of_the_command,
        ),
        (
            "a_sigint_to_sf_stops_the_command_and_leaves_nothing_behind",
            a_sigint_to_sf_stops_the_command_and_leaves_nothing_behind,
        ),
        (
            "the_agent_form_runs_a_session_with_its_configuration_read_only",
            the_agent_form_runs_a_session_with_its_configuration_read_only,
        ),
        (
            "the_sandbox_check_of_sf_doctor_starts_an_instance_at_the_configured_level",
            the_sandbox_check_of_sf_doctor_starts_an_instance_at_the_configured_level,
        ),
        (
            "sf_doctor_passes_where_every_check_passes_and_fails_otherwise",
            sf_doctor_passes_where_every_check_passes_and_fails_otherwise,
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
        return factory_daemon::supervise::run(std::env::args_os().skip(2).collect());
    }
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some(linux::EXEC) {
        return linux::exec_main();
    }
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some(linux::DOCTOR) {
        return factory_daemon::doctor::main(false);
    }
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some(linux::INIT) {
        return factory_daemon::sandbox_init::run(std::env::args_os().skip(2).collect());
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

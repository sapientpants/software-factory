//! `sf sandbox exec`, the command form (SPEC CLI-6), without a sandbox: a
//! backend of the test's own runs the command on the host, in the directory
//! that the sandbox would show as the working directory, so that what the
//! host does around an instance is tested on every host. What the bubblewrap
//! backend does with the same request is in the sandbox tests of this crate
//! (19.4).

#![expect(
    clippy::unwrap_used,
    reason = "the impls of the test's backend, which clippy does not see as test code (19.3)"
)]

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use factory_core::config::OperatorConfig;
use factory_core::ecosystem::{Ecosystem as _, HostTriple, Rust};
use factory_daemon::host::{Directories, Missing};
use factory_daemon::sandbox_exec::{
    self, Error, Given, HOME, Host, Layout, Report, Request, TARGET, WORK, given, parse_tool,
};
use factory_net::proxy::{Denial, Failure, Reason, Sent};
use factory_sandbox::{
    Access, BoxFuture, ExecSpec, Level, Mount, Sandbox, SandboxCaps, SandboxInstance,
    SandboxPolicy, SandboxRecord,
};

const TRIPLE: HostTriple = HostTriple::X86_64UnknownLinuxGnu;

/// A fresh directory for one test, removed when the value is dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Self {
        // `/tmp`, and not the long temporary directory of macOS: the path of
        // the proxy's socket lies below it (SEC-10b).
        let path = Path::new("/tmp").join(format!("sf-exec-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        for dir in ["data", "work", "tools"] {
            fs::create_dir_all(path.join(dir)).unwrap();
        }
        Self(path.canonicalize().unwrap())
    }

    fn data(&self) -> PathBuf {
        self.0.join("data")
    }

    fn work(&self) -> PathBuf {
        self.0.join("work")
    }

    fn config(&self, extra: &str) -> OperatorConfig {
        let text = format!(
            "schema_version = 1\ndata_dir = {:?}\ntools_dir = {:?}\n{extra}\n[sandbox]\nlevel = \"process\"\n",
            self.data(),
            self.0.join("tools"),
        );
        OperatorConfig::parse(text.as_bytes()).unwrap()
    }

    /// A check tool `name` at `version` in the tools directory.
    fn tool(&self, name: &str, version: &str) -> PathBuf {
        let bin = self.0.join("tools").join(name).join(version).join("bin");
        fs::create_dir_all(&bin).unwrap();
        let program = bin.join(name);
        fs::write(&program, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    /// What is left below the data directory's `sandbox-exec`.
    fn left(&self) -> Option<Vec<OsString>> {
        let entries = fs::read_dir(self.data().join(sandbox_exec::DIRECTORY)).ok()?;
        Some(entries.flatten().map(|entry| entry.file_name()).collect())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn request(dir: &Path, script: &str) -> Request {
    Request {
        dir: dir.to_owned(),
        target: None,
        tools: Vec::new(),
        program: "/bin/sh".into(),
        args: vec!["-c".into(), script.into()],
    }
}

/// What the backend of the test was asked for.
#[derive(Debug, Default)]
struct Asked {
    policies: Vec<SandboxPolicy>,
    specs: Vec<ExecSpec>,
    destroyed: usize,
}

/// A backend that runs the command on the host, in the host directory of the
/// mount at [`WORK`], with the spec's environment, and creates the temporary
/// directory as the bubblewrap backend does.
#[derive(Clone)]
struct OnTheHost {
    caps: SandboxCaps,
    asked: Arc<Mutex<Asked>>,
    refuse_command: bool,
}

impl OnTheHost {
    fn new() -> Self {
        Self {
            caps: Level::Process.requires(),
            asked: Arc::default(),
            refuse_command: false,
        }
    }
}

struct Instance {
    backend: OnTheHost,
    policy: SandboxPolicy,
}

impl Sandbox for OnTheHost {
    fn capabilities(&self) -> SandboxCaps {
        self.caps
    }

    fn create<'a>(
        &'a self,
        run: factory_core::engine::RunId,
        policy: &'a SandboxPolicy,
    ) -> BoxFuture<'a, Result<Box<dyn SandboxInstance>, factory_sandbox::Error>> {
        Box::pin(async move {
            let _ = run;
            // No check of the policy: the scratch lies below `/tmp`, which
            // the bubblewrap backend shows no mount at (SEC-6f).
            assert!(
                policy.proxy.exists(),
                "the proxy listens before the instance"
            );
            fs::create_dir(&policy.tmp).unwrap();
            self.asked.lock().unwrap().policies.push(policy.clone());
            let instance: Box<dyn SandboxInstance> = Box::new(Instance {
                backend: self.clone(),
                policy: policy.clone(),
            });
            Ok(instance)
        })
    }

    fn reap<'a>(
        &'a self,
        record: &'a SandboxRecord,
    ) -> BoxFuture<'a, Result<(), factory_sandbox::Error>> {
        Box::pin(async move {
            let _ = record;
            Ok(())
        })
    }
}

impl SandboxInstance for Instance {
    fn record(&self) -> SandboxRecord {
        SandboxRecord {
            run: factory_core::engine::RunId(0),
            level: Level::Process,
            tmp: self.policy.tmp.clone(),
        }
    }

    fn command(&self, spec: &ExecSpec) -> Result<factory_proc::Command, factory_sandbox::Error> {
        self.backend.asked.lock().unwrap().specs.push(spec.clone());
        if self.backend.refuse_command {
            return Err(factory_sandbox::Error::Policy(
                "refused by the test".to_owned(),
            ));
        }
        let work = self
            .policy
            .mounts
            .iter()
            .find(|mount| mount.sandbox == Path::new(WORK))
            .unwrap();
        let mut command = factory_proc::Command::new(&spec.program);
        command.args(&spec.args).current_dir(&work.host);
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        Ok(command)
    }

    fn kill_all(&self) -> BoxFuture<'_, Result<(), factory_sandbox::Error>> {
        Box::pin(async { Ok(()) })
    }

    fn destroy(self: Box<Self>) -> BoxFuture<'static, Result<(), factory_sandbox::Error>> {
        Box::pin(async move {
            self.backend.asked.lock().unwrap().destroyed += 1;
            let _ = fs::remove_dir_all(&self.policy.tmp);
            Ok(())
        })
    }
}

/// A proxy that reaches no host.
async fn nowhere(name: String) -> std::io::Result<tokio::net::TcpStream> {
    Err(std::io::Error::other(format!(
        "{name} is reached by no test"
    )))
}

/// Runs `request` on `backend` with `config`, and the command's stdout.
fn exec(
    config: &OperatorConfig,
    request: &Request,
    backend: &OnTheHost,
    stop: Option<Duration>,
) -> (Result<Report, Error>, String) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut stdout = Vec::new();
        let host = Host {
            sandbox: backend,
            connect: nowhere,
            ecosystem: &Rust,
            triple: TRIPLE,
        };
        let stop = async move {
            match stop {
                Some(after) => tokio::time::sleep(after).await,
                None => std::future::pending().await,
            }
        };
        let outcome =
            sandbox_exec::run(config, request, host, &mut stdout, tokio::io::sink(), stop).await;
        (outcome, String::from_utf8(stdout).unwrap())
    })
}

#[test]
fn the_command_runs_with_its_output_and_exit_status_and_leaves_nothing_behind() {
    let scratch = Scratch::new("runs");
    let backend = OnTheHost::new();
    let script = "pwd; printf 'no line break'; echo >&2 to-stderr; echo 12345 > w; exit 3";

    let (report, stdout) = exec(
        &scratch.config(""),
        &request(&scratch.work(), script),
        &backend,
        None,
    );
    let report = report.unwrap();

    assert_eq!(report.code(), 3);
    assert_eq!(
        stdout,
        format!("{}\nno line break", scratch.work().display())
    );
    assert!(report.denials.is_empty() && report.failures.is_empty());
    // The work directory is the operator's, and keeps what the command wrote.
    assert_eq!(
        fs::read_to_string(scratch.work().join("w")).unwrap(),
        "12345\n"
    );
    // The directory of the command is gone, its home and its target with it.
    assert_eq!(scratch.left(), Some(Vec::new()));
    let asked = backend.asked.lock().unwrap();
    assert_eq!(asked.destroyed, 1);
    assert_eq!(asked.specs[0].dir, Path::new(WORK));
    assert_eq!(asked.specs[0].program, Path::new("/bin/sh"));
}

#[test]
fn the_policy_shows_the_directory_a_new_home_and_a_new_target_directory() {
    let scratch = Scratch::new("policy");
    let backend = OnTheHost::new();
    let bin = scratch.tool("nextest", "0.9.152");
    let mut request = request(&scratch.work(), "true");
    request.tools = vec![parse_tool("nextest=0.9.152").unwrap()];

    let (report, _) = exec(&scratch.config(""), &request, &backend, None);
    report.unwrap();

    let asked = backend.asked.lock().unwrap();
    let policy = &asked.policies[0];
    let root = policy.tmp.parent().unwrap();
    assert_eq!(root.parent().unwrap(), scratch.data().join("sandbox-exec"));
    assert_eq!(policy.proxy, root.join("proxy.sock"));
    assert_eq!(
        policy.mounts,
        [
            Mount {
                host: scratch.work(),
                sandbox: WORK.into(),
                access: Access::ReadWrite
            },
            Mount {
                host: root.join("home"),
                sandbox: HOME.into(),
                access: Access::ReadWrite
            },
            Mount {
                host: root.join("target"),
                sandbox: TARGET.into(),
                access: Access::ReadWrite
            },
            Mount::at_its_path(bin.parent().unwrap(), Access::ReadOnly),
        ]
    );
    let env = &asked.specs[0].env;
    let path = env.iter().find(|(key, _)| key == "PATH").unwrap();
    assert_eq!(
        path.1,
        OsString::from(format!("{}:/usr/bin:/bin", bin.display()))
    );
}

#[test]
fn the_environment_holds_the_allowlist_of_ag_11_and_no_credential() {
    let given = Given {
        toolchain: None,
        tools: Vec::new(),
    };

    let env = sandbox_exec::environment(&Rust, &given).unwrap();
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (key.to_str().unwrap(), value.to_str().unwrap()))
        .collect();

    // The variables of build output are the ecosystem's (9.3, 9.5).
    let build_output = Rust.build_output(HOME, TARGET);
    assert_eq!(
        env[2..4],
        build_output
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        [&env[..2], &env[4..]].concat(),
        [
            ("PATH", "/usr/bin:/bin"),
            ("HOME", "/home/worker"),
            ("LANG", "C.UTF-8"),
            ("HTTPS_PROXY", "http://127.0.0.1:3128"),
            ("https_proxy", "http://127.0.0.1:3128"),
            ("HTTP_PROXY", "http://127.0.0.1:3128"),
            ("http_proxy", "http://127.0.0.1:3128"),
            ("NO_PROXY", "localhost,127.0.0.1,::1"),
            ("no_proxy", "localhost,127.0.0.1,::1"),
        ]
    );
}

#[test]
fn a_named_target_directory_is_used_and_left_as_it_is() {
    let scratch = Scratch::new("target");
    let backend = OnTheHost::new();
    let target = scratch.0.join("kept");
    fs::create_dir_all(target.join("deep")).unwrap();
    fs::write(target.join("deep/seed"), "1234567").unwrap();
    // A hard link is one file to the size.
    fs::hard_link(target.join("deep/seed"), target.join("link")).unwrap();
    let mut request = request(&scratch.work(), "true");
    request.target = Some(target.clone());

    let (report, _) = exec(&scratch.config(""), &request, &backend, None);
    let report = report.unwrap();

    assert_eq!(report.target_bytes, 7);
    assert_eq!(
        fs::read_to_string(target.join("deep/seed")).unwrap(),
        "1234567"
    );
    assert_eq!(
        backend.asked.lock().unwrap().policies[0].mounts[2].host,
        target
    );
    assert_eq!(scratch.left(), Some(Vec::new()));
}

#[test]
fn a_new_target_directory_is_measured_and_then_removed() {
    let scratch = Scratch::new("size");
    let backend = OnTheHost::new();
    // The backend of the test shows nothing, so the command finds the new
    // target directory beside the work directory's mount by its host path,
    // which the policy gives: the script reads it from its first argument.
    let script = "printf abcde > \"$0/five\"";
    let config = scratch.config("");
    let mut request = request(&scratch.work(), script);
    let data = scratch.data();
    // The id of the directory is not known before the run, so the script
    // finds the one directory below `sandbox-exec`.
    request.args = vec![
        "-c".into(),
        format!(
            "for d in {}/sandbox-exec/*/target; do {script}; done",
            data.display()
        )
        .replace("$0", "$d")
        .into(),
    ];

    let (report, _) = exec(&config, &request, &backend, None);

    assert_eq!(report.unwrap().target_bytes, 5);
    let target = backend.asked.lock().unwrap().policies[0].mounts[2]
        .host
        .clone();
    assert!(!target.exists(), "the new target directory is removed");
}

#[test]
fn the_size_counts_each_file_once_and_follows_no_link() {
    let scratch = Scratch::new("measure");
    let dir = scratch.0.join("measured");
    fs::create_dir_all(dir.join("d")).unwrap();
    fs::write(dir.join("a"), "abc").unwrap();
    fs::write(dir.join("d/b"), "defgh").unwrap();
    fs::hard_link(dir.join("d/b"), dir.join("c")).unwrap();
    std::os::unix::fs::symlink("/etc", dir.join("l")).unwrap();

    assert_eq!(sandbox_exec::size(&dir), 8);
    assert_eq!(sandbox_exec::size(&scratch.0.join("absent")), 0);
}

#[test]
fn a_stop_ends_the_command_by_the_stop_sequence() {
    let scratch = Scratch::new("stop");
    let backend = OnTheHost::new();
    let script = "trap 'echo interrupted; exit 9' INT; echo started; while :; do sleep 0.05; done";

    let (report, stdout) = exec(
        &scratch.config(""),
        &request(&scratch.work(), script),
        &backend,
        Some(Duration::from_millis(500)),
    );

    assert_eq!(report.unwrap().code(), 9);
    assert_eq!(stdout, "started\ninterrupted\n");
    assert_eq!(scratch.left(), Some(Vec::new()));
}

#[test]
fn what_the_host_lacks_creates_no_instance_and_no_directory() {
    let scratch = Scratch::new("missing");
    let backend = OnTheHost::new();
    let mut request = request(&scratch.work(), "true");
    request.tools = vec![parse_tool("typos=1.52.2").unwrap()];
    fs::write(
        scratch.work().join(Rust.toolchain_file()),
        "[toolchain]\nchannel = \"stable\"\n",
    )
    .unwrap();

    let (report, _) = exec(&scratch.config(""), &request, &backend, None);

    match report {
        Err(Error::Missing(missing)) => {
            assert!(
                matches!(missing[0], Missing::ToolchainFile(_)),
                "{missing:?}"
            );
            assert!(
                matches!(&missing[1], Missing::Tool { tool, .. } if tool.name() == "typos"),
                "{missing:?}"
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(scratch.left(), None);
    assert!(backend.asked.lock().unwrap().policies.is_empty());
}

#[test]
fn a_toolchain_that_is_named_and_missing_creates_no_instance() {
    let scratch = Scratch::new("toolchain");
    let backend = OnTheHost::new();
    fs::write(
        scratch.work().join(Rust.toolchain_file()),
        "[toolchain]\nchannel = \"1.100.0\"\n",
    )
    .unwrap();

    let (report, _) = exec(
        &scratch.config(""),
        &request(&scratch.work(), "true"),
        &backend,
        None,
    );

    let Err(error @ Error::Missing(_)) = report else {
        panic!("{report:?}");
    };
    assert!(
        error
            .to_string()
            .contains("the toolchain 1.100.0 is missing"),
        "{error}"
    );
    assert_eq!(scratch.left(), None);
}

#[test]
fn a_toolchain_file_that_is_no_regular_file_is_refused() {
    let scratch = Scratch::new("link");
    std::os::unix::fs::symlink("/etc/hosts", scratch.work().join(Rust.toolchain_file())).unwrap();

    let found = given(&Rust, TRIPLE, Directories::default(), &scratch.work(), &[]);

    assert!(
        matches!(&found, Err(missing) if matches!(missing[..], [Missing::ToolchainFile(_)])),
        "{found:?}"
    );
}

#[test]
fn without_a_toolchain_file_the_instance_is_given_no_toolchain() {
    let scratch = Scratch::new("none");
    let bin = scratch.tool("typos", "1.52.2");

    let found = given(
        &Rust,
        TRIPLE,
        Directories {
            toolchains: None,
            tools: Some(&scratch.0.join("tools")),
        },
        &scratch.work(),
        &[parse_tool("typos=1.52.2").unwrap()],
    )
    .unwrap();

    assert_eq!(found.toolchain, None);
    assert_eq!(found.tools.len(), 1);
    assert_eq!(found.read_only(), [bin.parent().unwrap()]);
}

#[test]
fn a_level_that_the_backend_cannot_provide_creates_nothing() {
    let scratch = Scratch::new("level");
    let mut backend = OnTheHost::new();
    backend.caps.egress_filtering = false;

    let (report, _) = exec(
        &scratch.config(""),
        &request(&scratch.work(), "true"),
        &backend,
        None,
    );
    assert!(matches!(report, Err(Error::Level(_))), "{report:?}");

    let hardened = OperatorConfig::parse(
        format!(
            "schema_version = 1\ndata_dir = {:?}\n[sandbox]\nlevel = \"hardened\"\n",
            scratch.data()
        )
        .as_bytes(),
    )
    .unwrap();
    let (report, _) = exec(
        &hardened,
        &request(&scratch.work(), "true"),
        &OnTheHost::new(),
        None,
    );
    assert!(matches!(report, Err(Error::Level(_))), "{report:?}");
    assert_eq!(scratch.left(), None);
}

#[test]
fn a_dir_that_is_no_directory_is_refused() {
    let scratch = Scratch::new("dir");
    let file = scratch.0.join("file");
    fs::write(&file, "").unwrap();

    for dir in [file, scratch.0.join("absent")] {
        let (report, _) = exec(
            &scratch.config(""),
            &request(&dir, "true"),
            &OnTheHost::new(),
            None,
        );
        assert!(matches!(report, Err(Error::Request(_))), "{report:?}");
    }
    assert_eq!(scratch.left(), None);
}

#[test]
fn a_failed_instance_leaves_no_directory_behind() {
    let scratch = Scratch::new("failed");
    let mut backend = OnTheHost::new();
    backend.refuse_command = true;

    let (report, _) = exec(
        &scratch.config(""),
        &request(&scratch.work(), "true"),
        &backend,
        None,
    );

    assert!(matches!(report, Err(Error::Sandbox(_))), "{report:?}");
    assert_eq!(backend.asked.lock().unwrap().destroyed, 1);
    assert_eq!(scratch.left(), Some(Vec::new()));
}

#[test]
fn the_allowlist_is_that_of_sec_11() {
    let scratch = Scratch::new("list");
    let list = sandbox_exec::allowlist(
        &Rust,
        &scratch.config("[egress]\nextra_hosts = [\"Mirror.Example\"]"),
    )
    .unwrap();

    for name in [
        "api.anthropic.com",
        "index.crates.io",
        "static.crates.io",
        "mirror.example",
    ] {
        assert!(list.allows(name), "{name}");
    }
    for name in ["github.com", "api.github.com", "crates.io"] {
        assert!(!list.allows(name), "{name}");
    }
    let refused = scratch.config("[egress]\nextra_hosts = [\"127.0.0.1\"]");
    assert!(sandbox_exec::allowlist(&Rust, &refused).is_err());
}

#[test]
fn a_tool_is_named_as_name_equals_version() {
    let tool = parse_tool("deny=0.22.1").unwrap();

    assert_eq!((tool.name(), tool.version()), ("deny", "0.22.1"));
    for refused in ["deny", "=0.22.1", "Deny=1", "deny=", "deny=/1"] {
        assert!(parse_tool(refused).is_err(), "{refused}");
    }
}

#[test]
fn the_layout_lies_below_the_data_directory() {
    let layout = Layout::new(Path::new("/srv/sf/data"), "7-1", None);

    assert_eq!(layout.root, Path::new("/srv/sf/data/sandbox-exec/7-1"));
    assert_eq!(layout.home, Path::new("/srv/sf/data/sandbox-exec/7-1/home"));
    assert_eq!(layout.tmp, Path::new("/srv/sf/data/sandbox-exec/7-1/tmp"));
    assert_eq!(
        layout.target,
        Path::new("/srv/sf/data/sandbox-exec/7-1/target")
    );
    assert_eq!(
        layout.proxy,
        Path::new("/srv/sf/data/sandbox-exec/7-1/proxy.sock")
    );
    assert!(layout.new_target);
    let named = Layout::new(Path::new("/srv/sf/data"), "7-1", Some(Path::new("/t")));
    assert_eq!(named.target, Path::new("/t"));
    assert!(!named.new_target);
}

#[test]
fn the_report_gives_the_time_the_size_the_denials_and_the_status() {
    use std::os::unix::process::ExitStatusExt as _;

    let started = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let report = Report {
        started,
        wall: Duration::from_millis(12_345),
        target_bytes: 678,
        denials: vec![
            Denial {
                time: started + Duration::from_millis(1_500),
                activity: None,
                sent: Some(Sent {
                    method: "CONNECT".to_owned(),
                    target: "evil.example:443\n".to_owned(),
                }),
                reason: Reason::Host,
            },
            Denial {
                time: started + Duration::from_secs(2),
                activity: None,
                sent: None,
                reason: Reason::Malformed,
            },
        ],
        failures: vec![Failure {
            time: started + Duration::from_secs(3),
            activity: None,
            name: "index.crates.io".to_owned(),
            error: "no answer".to_owned(),
        }],
        status: std::process::ExitStatus::from_raw(9),
    };

    assert_eq!(report.code(), 137);
    assert_eq!(
        report.text(),
        "sf sandbox exec: the command exited with 137\n  \
         wall-clock time: 12.345 s\n  \
         target directory: 678 bytes\n  \
         denied request at 1.500 s: host, \"CONNECT\" \"evil.example:443\\n\"\n  \
         denied request at 2.000 s: malformed\n  \
         unreachable at 3.000 s: \"index.crates.io\": no answer\n"
    );
    let json: serde_json::Value = serde_json::from_str(&report.json()).unwrap();
    assert_eq!(json["exit_status"], 137);
    assert_eq!(json["target_bytes"], 678);
    assert_eq!(json["denied_requests"][0]["target"], "evil.example:443\n");
    assert_eq!(
        json["denied_requests"][1]["method"],
        serde_json::Value::Null
    );
    assert_eq!(json["unreachable"][0]["name"], "index.crates.io");

    let clean = Report {
        denials: Vec::new(),
        failures: Vec::new(),
        status: std::process::ExitStatus::from_raw(0),
        ..report
    };
    assert!(clean.text().contains("  denied requests: none\n"));
    assert_eq!(clean.code(), 0);
}

#[test]
fn a_toolchain_file_that_cannot_be_looked_at_is_refused() {
    let scratch = Scratch::new("unreadable");
    let file = scratch.0.join("file");
    fs::write(&file, "").unwrap();
    fs::create_dir(scratch.work().join(Rust.toolchain_file())).unwrap();

    // A path below a file, which is no directory: no file is absent there.
    let below_a_file = given(&Rust, TRIPLE, Directories::default(), &file, &[]);
    let a_directory = given(&Rust, TRIPLE, Directories::default(), &scratch.work(), &[]);

    let problem = |found: Result<Given, Vec<Missing>>| match found {
        Err(missing) => match &missing[..] {
            [Missing::ToolchainFile(error)] => error.problem.clone(),
            other => panic!("{other:?}"),
        },
        Ok(given) => panic!("{given:?}"),
    };
    assert!(problem(below_a_file).starts_with("it cannot be read: "));
    assert_eq!(problem(a_directory), "it is no regular file");
}

#[test]
fn a_directory_that_another_command_holds_is_not_taken() {
    let scratch = Scratch::new("taken");
    let taken = scratch
        .data()
        .join(sandbox_exec::DIRECTORY)
        .join(format!("{}-0", std::process::id()));
    fs::create_dir_all(taken.join("home")).unwrap();

    let layout = Layout::create(&scratch.data(), None).unwrap();

    assert_eq!(
        layout.root.file_name().unwrap(),
        OsString::from(format!("{}-1", std::process::id()))
    );
    assert!(layout.home.is_dir() && layout.target.is_dir());
    // What the other command holds is left as it is.
    assert!(taken.join("home").is_dir());
    layout.remove().unwrap();
    assert!(!layout.root.exists());
    // A root that is gone already is removed.
    layout.remove().unwrap();
}

#[test]
fn a_directory_that_its_owner_cannot_write_is_removed() {
    let scratch = Scratch::new("readonly");
    let layout = Layout::create(&scratch.data(), None).unwrap();
    let locked = layout.home.join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("file"), "x").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();

    layout.remove().unwrap();

    assert!(!layout.root.exists());
}

#[test]
fn a_root_that_cannot_be_created_fails_by_name() {
    let scratch = Scratch::new("unwritable");
    let parent = scratch.data().join(sandbox_exec::DIRECTORY);
    fs::create_dir_all(&parent).unwrap();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o500)).unwrap();

    let created = Layout::create(&scratch.data(), None);
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();

    let error = created.unwrap_err().to_string();
    assert!(
        error.starts_with("cannot create the directory of the command "),
        "{error}"
    );
}

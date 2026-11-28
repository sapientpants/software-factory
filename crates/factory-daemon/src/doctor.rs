//! `sf doctor` (CLI-1): reports on the installation with the checks of the
//! M0 row of CLI-1, and with none of a later row.
//!
//! - The operator configuration parses as version 1 (CFG-2, 16.4), and where
//!   it does not, the check names the key and what is wrong with it (CFG-6).
//! - Each credential file that the configuration names exists with mode 0600
//!   (13.5).
//! - The `git` that the `PATH` finds is of a version that 4.4 accepts.
//! - The `bwrap` that the `PATH` finds is of a version that 4.4 accepts, and
//!   unprivileged user namespaces are enabled (4.4).
//! - A sandbox instance at the configured level starts and runs a command
//!   (13.3), as `sf sandbox exec` creates one (CLI-6).
//! - The agent CLI that the configuration names is present, and which
//!   version it is (10.3).
//!
//! A check that needs the configuration is not made where the configuration
//! is invalid, and says so. Each program is asked for its version on the
//! host, with this process's `PATH` and no other variable, and is stopped
//! where it does not answer within [`VERSION_TIMEOUT`].

use std::fmt::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use factory_core::config::OperatorConfig;
use factory_core::ecosystem::{HostTriple, Rust};
use factory_net::proxy::{self, Connect};
use factory_proc::{Command, Line, StopTimes};

use crate::sandbox_exec::{self, Form, Host, Interrupts, Request};

/// The oldest `git` that 4.4 accepts.
pub const MIN_GIT: Version = (2, 47, 3);

/// The oldest `bubblewrap` that 4.4 accepts.
pub const MIN_BWRAP: Version = (0, 9, 0);

/// How long a program has to print its version *(tunable)*.
pub const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// How long the command of the sandbox check has, from the start of the
/// instance *(tunable)*.
pub const SANDBOX_TIMEOUT: Duration = Duration::from_secs(30);

/// The directory below the data directory that holds the working directory
/// of the sandbox check's instance (16.4).
pub const DIRECTORY: &str = "doctor";

/// The command line that the sandbox check runs in the instance.
const PROBE: &str = "echo ok";

/// What [`PROBE`] prints.
const PROBE_OUTPUT: &str = "ok\n";

/// A version as major, minor and patch.
pub type Version = (u32, u32, u32);

/// One check and how it came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// What was checked.
    pub name: String,
    /// Whether it passed.
    pub ok: bool,
    /// What was found, or what is wrong.
    pub text: String,
}

impl Check {
    /// A check that passed.
    pub fn pass(name: impl Into<String>, text: String) -> Self {
        Self {
            name: name.into(),
            ok: true,
            text,
        }
    }

    /// A check that failed.
    pub fn fail(name: impl Into<String>, text: String) -> Self {
        Self {
            name: name.into(),
            ok: false,
            text,
        }
    }

    fn outcome(name: impl Into<String>, outcome: Result<String, String>) -> Self {
        match outcome {
            Ok(text) => Self::pass(name, text),
            Err(text) => Self::fail(name, text),
        }
    }
}

/// What `sf doctor` reports: every check, in the order made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The checks.
    pub checks: Vec<Check>,
}

impl Report {
    /// Whether every check passed.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|check| check.ok)
    }

    /// The report as text, one check to a line.
    #[must_use]
    pub fn text(&self) -> String {
        let mut text = String::new();
        for check in &self.checks {
            let mark = if check.ok { "ok  " } else { "FAIL" };
            let _ = writeln!(text, "{mark}  {}: {}", check.name, check.text);
        }
        text
    }

    /// The report as one JSON object (17).
    #[must_use]
    pub fn json(&self) -> String {
        let checks: Vec<serde_json::Value> = self
            .checks
            .iter()
            .map(|check| {
                serde_json::json!({ "name": check.name, "ok": check.ok, "text": check.text })
            })
            .collect();
        serde_json::json!({ "ok": self.ok(), "checks": checks }).to_string()
    }
}

/// The name of the configuration check.
const CONFIG: &str = "operator configuration";

/// Reads the operator configuration at `path` (CFG-2, CFG-6): the check,
/// and the configuration where it is valid.
#[must_use]
pub fn config(path: &Path) -> (Check, Option<OperatorConfig>) {
    let parsed = std::fs::read(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
        .and_then(|bytes| {
            OperatorConfig::parse(&bytes)
                .map_err(|error| format!("{} is invalid: {error}", path.display()))
        });
    match parsed {
        Ok(config) => (
            Check::pass(CONFIG, format!("{} parses as version 1", path.display())),
            Some(config),
        ),
        Err(text) => (Check::fail(CONFIG, text), None),
    }
}

/// The name of the credential check where it is not made.
const CREDENTIALS: &str = "credential files";

/// One check for each credential file that `config` names: it exists, is a
/// regular file and has mode 0600 (13.5). The length of the token's value is
/// a check of M1a (CLI-1).
#[must_use]
pub fn credentials(config: &OperatorConfig) -> Vec<Check> {
    let token = config
        .credentials
        .subscription_token_file
        .as_ref()
        .map(|file| ("[credentials] subscription_token_file", file.as_path()));
    let key = config.github.as_ref().map(|github| {
        (
            "[github] private_key_file",
            github.private_key_file.as_path(),
        )
    });
    token
        .into_iter()
        .chain(key)
        .map(|(key, file)| Check::outcome(format!("credential file `{key}`"), credential(file)))
        .collect()
}

fn credential(file: &Path) -> Result<String, String> {
    let metadata =
        std::fs::metadata(file).map_err(|error| format!("{}: {error}", file.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is no regular file", file.display()));
    }
    let mode = metadata.permissions().mode() & 0o7777;
    if mode == 0o600 {
        Ok(format!("{} has mode 0600", file.display()))
    } else {
        Err(format!("{} has mode {mode:04o}, not 0600", file.display()))
    }
}

/// The first three numbers of the version that follows `prefix` in `text`,
/// with 0 for a number left out.
fn version_after(text: &str, prefix: &str) -> Option<Version> {
    let rest = text.trim().strip_prefix(prefix)?;
    let word = rest.split_whitespace().next()?;
    let mut numbers = word.split('.').map(str::parse::<u32>);
    let major = numbers.next()?.ok()?;
    let minor = numbers.next().unwrap_or(Ok(0)).ok()?;
    let patch = numbers.next().and_then(Result::ok).unwrap_or(0);
    Some((major, minor, patch))
}

/// The version that `git --version` printed.
#[must_use]
pub fn git_version(text: &str) -> Option<Version> {
    version_after(text, "git version ")
}

/// The version that `bwrap --version` printed.
#[must_use]
pub fn bwrap_version(text: &str) -> Option<Version> {
    version_after(text, "bubblewrap ")
}

fn dotted((major, minor, patch): Version) -> String {
    format!("{major}.{minor}.{patch}")
}

/// Runs `program --version` on the host and gives the first line it
/// printed, where it exits with 0 within [`VERSION_TIMEOUT`].
async fn version_line(program: &Path) -> Result<String, String> {
    let shown = program.display();
    let mut command = Command::new(program);
    command.arg("--version");
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    let mut process = command
        .spawn(tokio::io::sink())
        .await
        .map_err(|error| format!("{shown} does not start: {error}"))?;
    let read = async {
        let mut first = None;
        while let Some(line) = process.next_line().await? {
            if let (None, Line::Text(text)) = (&first, line)
                && !text.trim().is_empty()
            {
                first = Some(text);
            }
        }
        process.wait().await.map(|status| (first, status))
    };
    let outcome = tokio::time::timeout(VERSION_TIMEOUT, read).await;
    match outcome {
        Err(_) => {
            let _ = process.stop(StopTimes::AG_12, |_| false).await;
            Err(format!(
                "{shown} --version did not end within {} s",
                VERSION_TIMEOUT.as_secs()
            ))
        }
        Ok(Err(error)) => Err(format!("{shown} --version failed: {error}")),
        Ok(Ok((_, status))) if !status.success() => {
            Err(format!("{shown} --version exited with {status}"))
        }
        Ok(Ok((None, _))) => Err(format!("{shown} --version printed nothing")),
        Ok(Ok((Some(line), _))) => Ok(line.trim().to_owned()),
    }
}

/// The version check of a program that 4.4 names a minimum for.
async fn versioned(
    program: Option<&Path>,
    name: &str,
    read: fn(&str) -> Option<Version>,
    min: Version,
) -> Result<String, String> {
    let program =
        program.ok_or_else(|| format!("is not installed: no `{name}` is on the PATH (4.4)"))?;
    let line = version_line(program).await?;
    let version = read(&line).ok_or_else(|| {
        format!(
            "{} printed no version that is read: {line:?}",
            program.display()
        )
    })?;
    if version >= min {
        Ok(format!("{} is {}", program.display(), dotted(version)))
    } else {
        Err(format!(
            "{} is {}, and 4.4 asks for {} or later",
            program.display(),
            dotted(version),
            dotted(min)
        ))
    }
}

/// `git` at `program`, the one that the `PATH` finds, is [`MIN_GIT`] or
/// later (4.4).
pub async fn git(program: Option<&Path>) -> Check {
    Check::outcome("git", versioned(program, "git", git_version, MIN_GIT).await)
}

/// `bubblewrap` at `program`, the one that the `PATH` finds, is
/// [`MIN_BWRAP`] or later (4.4).
pub async fn bubblewrap(program: Option<&Path>) -> Check {
    Check::outcome(
        "bubblewrap",
        versioned(program, "bwrap", bwrap_version, MIN_BWRAP).await,
    )
}

/// Unprivileged user namespaces are enabled, as the sysctls below
/// `proc_sys`, `/proc/sys` on the host, say (4.4): `user.max_user_namespaces`
/// is above 0, and where the kernel has them, Debian's
/// `kernel.unprivileged_userns_clone` is 1 and Ubuntu's
/// `kernel.apparmor_restrict_unprivileged_userns` is 0
/// (`docs/research/14-server-facts.md`).
#[must_use]
pub fn user_namespaces(proc_sys: &Path) -> Check {
    Check::outcome(
        "unprivileged user namespaces",
        read_user_namespaces(proc_sys),
    )
}

fn read_user_namespaces(proc_sys: &Path) -> Result<String, String> {
    let read = |name: &str| -> Result<Option<u64>, String> {
        let path = proc_sys.join(name.replace('.', "/"));
        match std::fs::read_to_string(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("cannot read `{name}`: {error}")),
            Ok(text) => text
                .trim()
                .parse()
                .map(Some)
                .map_err(|_| format!("`{name}` holds no number: {:?}", text.trim())),
        }
    };
    let max = "user.max_user_namespaces";
    match read(max)? {
        None => {
            return Err(format!(
                "`{max}` cannot be found, so they cannot be read on this host"
            ));
        }
        Some(0) => return Err(format!("`{max}` is 0")),
        Some(_) => {}
    }
    let clone = "kernel.unprivileged_userns_clone";
    if let Some(value) = read(clone)?
        && value != 1
    {
        return Err(format!("`{clone}` is {value}, not 1"));
    }
    let apparmor = "kernel.apparmor_restrict_unprivileged_userns";
    if let Some(value) = read(apparmor)?
        && value != 0
    {
        return Err(format!("`{apparmor}` is {value}, not 0"));
    }
    Ok("enabled".to_owned())
}

/// The name of the agent CLI check.
const AGENT: &str = "agent CLI";

/// The agent CLI that `config` names, `[agent] cli`, is an executable file,
/// and the version it prints (10.3). Whether that version is in the tested
/// range is a check of M1a (CLI-1).
pub async fn agent_cli(config: &OperatorConfig) -> Check {
    Check::outcome(AGENT, read_agent_cli(config).await)
}

async fn read_agent_cli(config: &OperatorConfig) -> Result<String, String> {
    let cli = config.agent.cli.as_ref().ok_or_else(|| {
        "the operator configuration names no `[agent] cli`, so no agent session starts (16.4)"
            .to_owned()
    })?;
    let path = cli.as_path();
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("{} cannot be found: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is no file", path.display()));
    }
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err(format!("{} is not executable", path.display()));
    }
    let line = version_line(path).await?;
    Ok(format!("{} is {line:?}", path.display()))
}

/// The name of the sandbox check.
const SANDBOX: &str = "sandbox";

/// A sandbox instance at the level that `config` asks for starts on `host`
/// and runs a command (13.3), as `sf sandbox exec` creates one (CLI-6). Its
/// working directory is a new, empty directory below
/// `<data directory>/doctor/`, which is removed afterwards. `stop` stops the
/// command, as does [`SANDBOX_TIMEOUT`].
pub async fn sandbox<C: Connect>(
    config: &OperatorConfig,
    host: Host<'_, C>,
    stop: impl Future<Output = ()> + Send,
) -> Check {
    Check::outcome(SANDBOX, run_sandbox(config, host, stop).await)
}

async fn run_sandbox<C: Connect>(
    config: &OperatorConfig,
    host: Host<'_, C>,
    stop: impl Future<Output = ()> + Send,
) -> Result<String, String> {
    let level = sandbox_exec::level(config).as_str();
    let work = Work::create(config.data_dir.as_path())?;
    let request = Request {
        dir: work.0.clone(),
        target: None,
        tools: Vec::new(),
        form: Form::Command {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), PROBE.into()],
        },
    };
    let mut stdout = Vec::new();
    let stop = async {
        tokio::select! {
            () = stop => {}
            () = tokio::time::sleep(SANDBOX_TIMEOUT) => {}
        }
    };
    let report = sandbox_exec::run(config, &request, host, &mut stdout, tokio::io::sink(), stop)
        .await
        .map_err(|error| format!("no instance at level `{level}` ran a command: {error}"))?;
    drop(work);
    let code = report.code();
    if code != 0 {
        return Err(format!(
            "the command `{}` in an instance at level `{level}` exited with {code}",
            PROBE
        ));
    }
    if stdout != PROBE_OUTPUT.as_bytes() {
        return Err(format!(
            "the command `{}` in an instance at level `{level}` printed {:?}",
            PROBE,
            String::from_utf8_lossy(&stdout)
        ));
    }
    Ok(format!(
        "an instance at level `{level}` started and ran `{}`",
        PROBE
    ))
}

/// The working directory of the sandbox check, removed when dropped.
struct Work(PathBuf);

impl Work {
    fn create(data_dir: &Path) -> Result<Self, String> {
        let path = data_dir
            .join(DIRECTORY)
            .join(std::process::id().to_string());
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
        Ok(Self(path))
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Every check of the M0 row of CLI-1, with the operator configuration of
/// this user and the programs that the `PATH` finds.
async fn checks() -> Report {
    let interrupts = Interrupts::install();
    let mut checks = Vec::new();
    let config = match sandbox_exec::config_path() {
        Ok(path) => {
            let (check, config) = config(&path);
            checks.push(check);
            config
        }
        Err(error) => {
            checks.push(Check::fail(CONFIG, error.to_string()));
            None
        }
    };
    let not_made = "not made, since no valid operator configuration was read";
    match &config {
        Some(config) => checks.extend(credentials(config)),
        None => checks.push(Check::fail(CREDENTIALS, not_made.to_owned())),
    }
    checks.push(git(sandbox_exec::on_the_path("git").as_deref()).await);
    checks.push(bubblewrap(sandbox_exec::on_the_path("bwrap").as_deref()).await);
    checks.push(user_namespaces(Path::new("/proc/sys")));
    let Some(config) = config else {
        checks.push(Check::fail(SANDBOX, not_made.to_owned()));
        checks.push(Check::fail(AGENT, not_made.to_owned()));
        return Report { checks };
    };
    checks.push(configured_sandbox(&config, interrupts).await);
    checks.push(agent_cli(&config).await);
    Report { checks }
}

/// The sandbox check with the backend of the configured level on this host.
async fn configured_sandbox(config: &OperatorConfig, interrupts: Interrupts) -> Check {
    let level = sandbox_exec::level(config);
    let backend = match sandbox_exec::backend(level) {
        Ok(backend) => backend,
        Err(error) => return Check::fail(SANDBOX, error.to_string()),
    };
    let Some(triple) = HostTriple::THIS else {
        return Check::fail(SANDBOX, "this build has no host triple of 4.4".to_owned());
    };
    let host = Host {
        sandbox: backend.as_ref(),
        connect: proxy::resolve,
        ecosystem: &Rust,
        triple,
    };
    sandbox(config, host, interrupts.received()).await
}

/// `sf doctor`: makes every check, writes the report to stdout, as JSON
/// where `json` holds, and exits with 0 where every check passed and 1
/// otherwise.
pub fn main(json: bool) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let error = format!("cannot start the runtime: {error}");
            if json {
                eprintln!("{}", serde_json::json!({ "error": error }));
            } else {
                eprintln!("sf doctor: {error}");
            }
            return ExitCode::FAILURE;
        }
    };
    let report = runtime.block_on(checks());
    if json {
        println!("{}", report.json());
    } else {
        print!("{}", report.text());
    }
    if report.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

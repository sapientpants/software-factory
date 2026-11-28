//! `sf sandbox exec`, the command form (CLI-6): one command line in one
//! sandbox instance at the configured level, outside any run.
//!
//! The host reads the toolchain file from the directory, as data, and looks up
//! the toolchain that it names and each check tool that the request names
//! (WS-7, WS-7a). Where one is missing it creates nothing. It then creates the
//! directory of the command below the data directory, `sandbox-exec/<id>/`
//! (9.1), with a new, empty home and, unless the request names one, a new
//! target directory; stands the egress proxy at the socket there, with the
//! allowlist of SEC-11 (SEC-10b); creates the instance; and runs the command
//! through the supervisor shim (AG-16). Once the command has ended it
//! destroys the instance, closes the proxy (AG-19), measures the target
//! directory and removes the directory of the command.
//!
//! It is no run: it reads no issue, takes no store lock, journals nothing and
//! writes nothing to GitHub. Nothing here opens the store or a client of
//! GitHub.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, ExitStatus};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

pub use factory_core::check_tool::CheckTool;
use factory_core::config::{CredentialMode, OperatorConfig, SandboxLevel};
use factory_core::ecosystem::{Ecosystem, HostTriple, Rust, ToolchainFileError};
use factory_core::engine::RunId;
use factory_net::proxy::{self, Allowlist, BindError, Connect, Denial, Failure, ListError, Proxy};
use factory_proc::{Line, StopTimes};
use factory_sandbox::{
    Access, ExecSpec, Level, Mount, PROXY_PORT, Sandbox, SandboxInstance, SandboxPolicy,
};
use tokio::io::{AsyncWrite, AsyncWriteExt as _};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::host::{self, Directories, FoundCheckTool, Missing, Toolchain};

/// Where the sandbox shows the directory that `--dir` names, which is the
/// command's working directory, whatever its path on the host (CLI-6).
pub const WORK: &str = "/work";

/// Where the sandbox shows the command's new home.
pub const HOME: &str = "/home/worker";

/// Where the sandbox shows the target directory, the new one or the one that
/// `--target` names.
pub const TARGET: &str = "/target";

/// The directories of the OS baseline that follow the toolchain and the
/// check tools on the worker `PATH` (WS-7, SEC-6).
pub const BASE_PATH: &[&str] = &["/usr/bin", "/bin"];

/// The locale of AG-11, one that the C library holds without a file of its
/// own.
pub const LOCALE: &str = "C.UTF-8";

/// The hosts of the model endpoint in `subscription` mode: `api.anthropic.com`
/// alone (SEC-11, P-22).
pub const MODEL_HOSTS: &[&str] = &["api.anthropic.com"];

/// What `NO_PROXY` holds (SEC-10b).
pub const NO_PROXY: &str = "localhost,127.0.0.1,::1";

/// The directory below the data directory that holds one directory for each
/// command (9.1).
pub const DIRECTORY: &str = "sandbox-exec";

/// The exit code of `sf sandbox exec` where it runs no command: the request
/// or the host lacks something, or the instance did not start. The shim
/// exits with the same where it starts no command.
pub const FAILED: u8 = factory_proc::supervise::FAILED;

/// The command line that the operator asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The directory that the instance shows read-write as the working
    /// directory.
    pub dir: PathBuf,
    /// The target directory to use in place of a new one, which the host
    /// leaves as it is.
    pub target: Option<PathBuf>,
    /// The check tools that the command is given (WS-7a).
    pub tools: Vec<CheckTool>,
    /// The program, as the worker sees it: a path, or a name that the worker
    /// `PATH` finds.
    pub program: OsString,
    /// Its arguments.
    pub args: Vec<OsString>,
}

/// Reads `<name>=<version>`, the value of `--tool` (WS-7a).
///
/// # Errors
///
/// Where there is no `=`, or the name or the version is of no form of WS-7a.
pub fn parse_tool(value: &str) -> Result<CheckTool, String> {
    let (name, version) = value
        .split_once('=')
        .ok_or_else(|| format!("`{value}` is no <name>=<version>"))?;
    CheckTool::new(name, version).map_err(|error| error.to_string())
}

/// What the host gives the instance of its installations (WS-7, WS-7a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Given {
    /// The toolchain, where the directory names one.
    pub toolchain: Option<Toolchain>,
    /// The check tools, in order of name.
    pub tools: Vec<FoundCheckTool>,
}

impl Given {
    /// The worker `PATH`: the toolchain's `bin` first, then each check tool's
    /// in order of name, then [`BASE_PATH`].
    ///
    /// # Errors
    ///
    /// Where a directory holds a `:`, which no `PATH` can hold.
    pub fn path(&self) -> Result<OsString, Error> {
        let bins = self
            .toolchain
            .iter()
            .map(|toolchain| toolchain.bin.clone())
            .chain(self.tools.iter().map(|tool| tool.bin.clone()))
            .chain(BASE_PATH.iter().map(PathBuf::from));
        std::env::join_paths(bins).map_err(|error| Error::Path(error.to_string()))
    }

    /// The directories that the sandbox shows read-only, each at its own path
    /// (SEC-6): the toolchain's and that of each check tool.
    #[must_use]
    pub fn read_only(&self) -> Vec<PathBuf> {
        self.toolchain
            .iter()
            .map(|toolchain| toolchain.directory.clone())
            .chain(self.tools.iter().map(|tool| tool.directory.clone()))
            .collect()
    }
}

/// Looks up what `dir` names and `tools` asks for: the toolchain that the
/// ecosystem's file in `dir` names, read as data, and each check tool. Where
/// `dir` holds no such file the instance is given no toolchain (CLI-6).
///
/// # Errors
///
/// Every thing that is missing, as [`host::resolve`] gives it.
pub fn given(
    ecosystem: &dyn Ecosystem,
    triple: HostTriple,
    directories: Directories<'_>,
    dir: &Path,
    tools: &[CheckTool],
) -> Result<Given, Vec<Missing>> {
    match toolchain_file(ecosystem, dir) {
        Ok(None) => Ok(Given {
            toolchain: None,
            tools: host::find_check_tools(directories.tools, tools)?,
        }),
        Ok(Some(file)) => {
            host::resolve(ecosystem, triple, directories, Some(&file), tools).map(|paths| Given {
                toolchain: Some(paths.toolchain),
                tools: paths.tools,
            })
        }
        Err(error) => {
            let mut missing = vec![Missing::ToolchainFile(error)];
            if let Err(tools) = host::find_check_tools(directories.tools, tools) {
                missing.extend(tools);
            }
            Err(missing)
        }
    }
}

/// The bytes of the ecosystem's toolchain file in `dir`, or `None` where it
/// has none. A file that is no regular file, a symbolic link among them,
/// names no toolchain that the host reads.
fn toolchain_file(
    ecosystem: &dyn Ecosystem,
    dir: &Path,
) -> Result<Option<Vec<u8>>, ToolchainFileError> {
    let file = ecosystem.toolchain_file();
    let path = dir.join(file);
    let refused = |problem: String| ToolchainFileError { file, problem };
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(refused(format!("it cannot be read: {error}"))),
        Ok(metadata) if !metadata.is_file() => Err(refused("it is no regular file".to_owned())),
        Ok(_) => std::fs::read(&path)
            .map(Some)
            .map_err(|error| refused(format!("it cannot be read: {error}"))),
    }
}

/// The allowlist of SEC-11 for `config`: the model endpoint of the credential
/// mode, the registry of `ecosystem` and the names of `[egress] extra_hosts`.
///
/// # Errors
///
/// Where an entry of `extra_hosts` cannot be on the list (SEC-10b).
pub fn allowlist(
    ecosystem: &dyn Ecosystem,
    config: &OperatorConfig,
) -> Result<Allowlist, ListError> {
    let model = match config.credentials.mode {
        CredentialMode::Subscription => MODEL_HOSTS,
    };
    Allowlist::new(
        model
            .iter()
            .chain(ecosystem.registry_hosts())
            .copied()
            .chain(config.egress.extra_hosts.iter().map(String::as_str)),
    )
}

/// The environment of a setup or a verify command (AG-11): the worker
/// `PATH`, the home, the variables of build output (9.3), the locale and
/// the proxy variables, each of the three under both its names (SEC-10b).
/// The instance sets `SF_SANDBOX` and `TMPDIR` itself. No model credential
/// is among them.
///
/// # Errors
///
/// Where the `PATH` cannot be written.
pub fn environment(
    ecosystem: &dyn Ecosystem,
    given: &Given,
) -> Result<Vec<(OsString, OsString)>, Error> {
    let proxy = format!("http://127.0.0.1:{PROXY_PORT}");
    let mut env: Vec<(OsString, OsString)> =
        vec![("PATH".into(), given.path()?), ("HOME".into(), HOME.into())];
    env.extend(
        ecosystem
            .build_output(HOME, TARGET)
            .into_iter()
            .map(|(key, value)| (key.into(), value.into())),
    );
    env.push(("LANG".into(), LOCALE.into()));
    for (key, value) in [
        ("HTTPS_PROXY", proxy.as_str()),
        ("HTTP_PROXY", proxy.as_str()),
        ("NO_PROXY", NO_PROXY),
    ] {
        env.push((key.into(), value.into()));
        env.push((key.to_ascii_lowercase().into(), value.into()));
    }
    Ok(env)
}

/// The directories of one command on the host, below
/// `<data directory>/sandbox-exec/<id>/` (9.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// `sandbox-exec/<id>/`, which the sandbox does not show.
    pub root: PathBuf,
    /// The new home.
    pub home: PathBuf,
    /// The temporary directory, which the backend creates (SEC-6c).
    pub tmp: PathBuf,
    /// The target directory: the new one below the root, or the one that
    /// `--target` names.
    pub target: PathBuf,
    /// Whether the target directory is the new one.
    pub new_target: bool,
    /// The proxy's socket (SEC-10b).
    pub proxy: PathBuf,
}

impl Layout {
    /// The directories of the command `id` below `data_dir`, with the target
    /// directory `target` where the request names one.
    #[must_use]
    pub fn new(data_dir: &Path, id: &str, target: Option<&Path>) -> Self {
        let root = data_dir.join(DIRECTORY).join(id);
        Self {
            home: root.join("home"),
            tmp: root.join("tmp"),
            target: target.map_or_else(|| root.join("target"), Path::to_owned),
            new_target: target.is_none(),
            proxy: root.join("proxy.sock"),
            root,
        }
    }

    /// Creates the root with mode 0700, and in it the home and the new
    /// target directory. The root must not exist yet.
    fn create_dirs(&self) -> Result<(), CreateError> {
        let host = |what: &str, path: &Path| {
            let what = format!("create {what} {}", path.display());
            move |source| Error::Host { what, source }
        };
        let parent = self.root.parent().unwrap_or(&self.root);
        std::fs::create_dir_all(parent).map_err(host("the directory", parent))?;
        match std::fs::DirBuilder::new().mode(0o700).create(&self.root) {
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(CreateError::Taken);
            }
            created => created.map_err(host("the directory of the command", &self.root))?,
        }
        std::fs::create_dir(&self.home).map_err(host("the home", &self.home))?;
        if self.new_target {
            std::fs::create_dir(&self.target)
                .map_err(host("the target directory", &self.target))?;
        }
        Ok(())
    }

    /// Creates the directories of a new command below `data_dir`, under the
    /// first id `<pid>-<n>` that no directory has. The id is short, since the
    /// path of the proxy's socket below it is limited in length (SEC-10b).
    ///
    /// # Errors
    ///
    /// Where a directory cannot be created.
    pub fn create(data_dir: &Path, target: Option<&Path>) -> Result<Self, Error> {
        let pid = std::process::id();
        for n in 0..IDS {
            let layout = Self::new(data_dir, &format!("{pid}-{n}"), target);
            match layout.create_dirs() {
                Ok(()) => return Ok(layout),
                Err(CreateError::Taken) => {}
                Err(CreateError::Failed(error)) => {
                    // The root was created and is this command's.
                    let _ = layout.remove();
                    return Err(error);
                }
            }
        }
        Err(Error::Host {
            what: format!(
                "find an id for the directory of the command below {}",
                data_dir.join(DIRECTORY).display()
            ),
            source: std::io::ErrorKind::AlreadyExists.into(),
        })
    }

    /// Removes the root and what it holds, the new home and the new target
    /// directory among them, also where a worker left a directory that its
    /// owner cannot write. The target directory that `--target` names lies
    /// elsewhere and is left as it is.
    ///
    /// # Errors
    ///
    /// Where the root is left.
    pub fn remove(&self) -> Result<(), Error> {
        match std::fs::remove_dir_all(&self.root) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => {
                writable(&self.root);
                std::fs::remove_dir_all(&self.root).map_err(|source| Error::Host {
                    what: format!("remove {}", self.root.display()),
                    source,
                })
            }
        }
    }
}

/// How many ids of one process [`Layout::create`] tries.
const IDS: u32 = 1000;

/// Why the directories of a command were not created.
enum CreateError {
    /// The root exists already, of another command.
    Taken,
    /// Another error.
    Failed(Error),
}

impl From<Error> for CreateError {
    fn from(error: Error) -> Self {
        Self::Failed(error)
    }
}

/// Gives the owner every right on `dir` and each directory below it.
fn writable(dir: &Path) {
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                writable(&entry.path());
            }
        }
    }
}

/// The sandbox policy of the command (SEC-6): the directory of the request,
/// the home and the target directory read-write at [`WORK`], [`HOME`] and
/// [`TARGET`], and what the host gives read-only, each at its own path.
#[must_use]
pub fn policy(dir: &Path, layout: &Layout, given: &Given) -> SandboxPolicy {
    let mut mounts = vec![
        Mount {
            host: dir.to_owned(),
            sandbox: WORK.into(),
            access: Access::ReadWrite,
        },
        Mount {
            host: layout.home.clone(),
            sandbox: HOME.into(),
            access: Access::ReadWrite,
        },
        Mount {
            host: layout.target.clone(),
            sandbox: TARGET.into(),
            access: Access::ReadWrite,
        },
    ];
    mounts.extend(
        given
            .read_only()
            .into_iter()
            .map(|path| Mount::at_its_path(path, Access::ReadOnly)),
    );
    SandboxPolicy {
        mounts,
        tmp: layout.tmp.clone(),
        proxy: layout.proxy.clone(),
    }
}

/// The size of the files below `dir`, in bytes: each regular file once,
/// however many hard links it has, and no symbolic link followed. An entry
/// that cannot be read counts for nothing.
#[must_use]
pub fn size(dir: &Path) -> u64 {
    fn walk(dir: &Path, seen: &mut BTreeSet<(u64, u64)>, total: &mut u64) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if metadata.is_dir() {
                walk(&entry.path(), seen, total);
            } else if metadata.is_file() && seen.insert((metadata.dev(), metadata.ino())) {
                *total += metadata.len();
            }
        }
    }
    let mut total = 0;
    walk(dir, &mut BTreeSet::new(), &mut total);
    total
}

/// What the proxy refused and what it could not reach, kept for the report.
#[derive(Debug, Default)]
struct Records {
    denials: Mutex<Vec<Denial>>,
    failures: Mutex<Vec<Failure>>,
}

impl proxy::Log for Records {
    fn denied(&self, denial: Denial) {
        self.denials
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(denial);
    }

    fn failed(&self, failure: Failure) {
        self.failures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(failure);
    }
}

impl Records {
    fn take(&self) -> (Vec<Denial>, Vec<Failure>) {
        (
            std::mem::take(&mut *self.denials.lock().unwrap_or_else(PoisonError::into_inner)),
            std::mem::take(&mut *self.failures.lock().unwrap_or_else(PoisonError::into_inner)),
        )
    }
}

/// What `sf sandbox exec` reports (CLI-6).
#[derive(Debug, Clone)]
pub struct Report {
    /// When the command started.
    pub started: SystemTime,
    /// The wall-clock time of the command, from its start to its end.
    pub wall: Duration,
    /// The size of the target directory at the end, in bytes ([`size`]).
    pub target_bytes: u64,
    /// The proxy's denied-request log.
    pub denials: Vec<Denial>,
    /// The listed hosts that the proxy could not reach, which are no denial
    /// (SEC-10b).
    pub failures: Vec<Failure>,
    /// The exit status of the command line.
    pub status: ExitStatus,
}

impl Report {
    /// The exit code of the command line, or 128 and the number of the
    /// signal that ended it.
    #[must_use]
    pub fn code(&self) -> i32 {
        use std::os::unix::process::ExitStatusExt as _;

        self.status
            .code()
            .or_else(|| self.status.signal().map(|signal| 128 + signal))
            .unwrap_or(i32::from(FAILED))
    }

    /// What `sf sandbox exec` exits with: the status of the command line.
    pub fn exit_code(&self) -> ExitCode {
        factory_proc::supervise::exit_code(self.status)
    }

    /// When `time` lay after the start, in seconds.
    fn after_start(&self, time: SystemTime) -> f64 {
        time.duration_since(self.started)
            .unwrap_or_default()
            .as_secs_f64()
    }

    /// The report as text, one item to a line. A worker's text is written
    /// escaped, as data (INV-9).
    #[must_use]
    pub fn text(&self) -> String {
        let mut text = format!(
            "sf sandbox exec: the command exited with {}\n  wall-clock time: {:.3} s\n  target directory: {} bytes\n",
            self.code(),
            self.wall.as_secs_f64(),
            self.target_bytes
        );
        if self.denials.is_empty() {
            text.push_str("  denied requests: none\n");
        }
        for denial in &self.denials {
            let _ = write!(
                text,
                "  denied request at {:.3} s: {}",
                self.after_start(denial.time),
                denial.reason
            );
            if let Some(sent) = &denial.sent {
                let _ = write!(text, ", {:?} {:?}", sent.method, sent.target);
            }
            text.push('\n');
        }
        for failure in &self.failures {
            let _ = writeln!(
                text,
                "  unreachable at {:.3} s: {:?}: {}",
                self.after_start(failure.time),
                failure.name,
                failure.error
            );
        }
        text
    }

    /// The report as one JSON object (17).
    #[must_use]
    pub fn json(&self) -> String {
        let denials: Vec<serde_json::Value> = self
            .denials
            .iter()
            .map(|denial| {
                serde_json::json!({
                    "at_secs": self.after_start(denial.time),
                    "reason": denial.reason.as_str(),
                    "method": denial.sent.as_ref().map(|sent| &sent.method),
                    "target": denial.sent.as_ref().map(|sent| &sent.target),
                })
            })
            .collect();
        let failures: Vec<serde_json::Value> = self
            .failures
            .iter()
            .map(|failure| {
                serde_json::json!({
                    "at_secs": self.after_start(failure.time),
                    "name": failure.name,
                    "error": failure.error,
                })
            })
            .collect();
        serde_json::json!({
            "exit_status": self.code(),
            "wall_clock_secs": self.wall.as_secs_f64(),
            "target_bytes": self.target_bytes,
            "denied_requests": denials,
            "unreachable": failures,
        })
        .to_string()
    }
}

/// What `sf sandbox exec` failed at, before or around the command.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The operator configuration cannot be read or is invalid (CFG-6).
    #[error("{0}")]
    Config(String),
    /// The configured level cannot be provided (SEC-7).
    #[error("{0}")]
    Level(String),
    /// What the request names is no directory.
    #[error("{0}")]
    Request(String),
    /// The host lacks the toolchain, a part of it or a check tool (WS-7,
    /// WS-7a).
    #[error("{}", missing(.0))]
    Missing(Vec<Missing>),
    /// The worker `PATH` cannot be written.
    #[error("the worker PATH cannot be written: {0}")]
    Path(String),
    /// An entry of the egress list is refused (SEC-10b).
    #[error("the egress list is refused: {0}")]
    List(#[from] ListError),
    /// The proxy's socket cannot be created (SEC-10b).
    #[error("{0}")]
    Proxy(#[from] BindError),
    /// The instance failed.
    #[error("{0}")]
    Sandbox(#[from] factory_sandbox::Error),
    /// Starting, reading or stopping the command failed.
    #[error("{0}")]
    Process(#[from] factory_proc::Error),
    /// A call on the host failed.
    #[error("cannot {what}: {source}")]
    Host {
        /// What was being done.
        what: String,
        /// Why it failed.
        source: std::io::Error,
    },
    /// The command's output could not be written.
    #[error("cannot write the command's output: {0}")]
    Output(std::io::Error),
}

/// Each missing thing on a line of its own.
fn missing(all: &[Missing]) -> String {
    all.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// What the command is run with on the host.
pub struct Host<'a, C> {
    /// The backend of the configured level.
    pub sandbox: &'a dyn Sandbox,
    /// How the proxy reaches a listed name (SEC-10b).
    pub connect: C,
    /// The ecosystem, `rust` in v1 (9.5).
    pub ecosystem: &'a dyn Ecosystem,
    /// The host triple of the build (WS-7).
    pub triple: HostTriple,
}

/// The level that `config` asks for: `process` or `hardened`, and never
/// `none` (SEC-7).
fn level(config: &OperatorConfig) -> Level {
    match config.sandbox.level {
        SandboxLevel::Process => Level::Process,
        SandboxLevel::Hardened => Level::Hardened,
    }
}

/// `path`, canonical, where it is a directory.
fn directory(path: &Path, what: &str) -> Result<PathBuf, Error> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|error| Error::Request(format!("{what}, {}: {error}", path.display())))?;
    if canonical.is_dir() {
        Ok(canonical)
    } else {
        Err(Error::Request(format!(
            "{what}, {}, is no directory",
            path.display()
        )))
    }
}

/// Runs `request` in one instance of `host.sandbox`, as CLI-6 says, writes
/// the command's stdout to `stdout` and its stderr to `stderr`, and stops the
/// command by the sequence of AG-12 where `stop` ends first.
///
/// # Errors
///
/// Where the level cannot be provided, what the request names is no
/// directory, the host lacks something, or the instance, the proxy or the
/// command fails. Nothing is created where the level, the request or the
/// host's installations fail.
pub async fn run<C: Connect>(
    config: &OperatorConfig,
    request: &Request,
    host: Host<'_, C>,
    stdout: &mut (impl AsyncWrite + Unpin + Send),
    stderr: impl AsyncWrite + Send + 'static,
    stop: impl Future<Output = ()> + Send,
) -> Result<Report, Error> {
    let level = level(config);
    if !host.sandbox.capabilities().provides(level) {
        return Err(Error::Level(format!(
            "the sandbox level `{}` of the operator configuration cannot be provided on this host (SEC-7)",
            level.as_str()
        )));
    }
    let dir = directory(&request.dir, "--dir")?;
    let target = request
        .target
        .as_deref()
        .map(|target| directory(target, "--target"))
        .transpose()?;
    let directories = Directories {
        toolchains: config
            .ecosystem
            .rust
            .toolchains_dir
            .as_ref()
            .map(|dir| dir.as_path()),
        tools: config.tools_dir.as_ref().map(|dir| dir.as_path()),
    };
    let given = given(
        host.ecosystem,
        host.triple,
        directories,
        &dir,
        &request.tools,
    )
    .map_err(Error::Missing)?;
    let list = allowlist(host.ecosystem, config)?;
    let env = environment(host.ecosystem, &given)?;

    let layout = Layout::create(config.data_dir.as_path(), target.as_deref())?;
    let records = Arc::new(Records::default());
    let outcome = in_the_instance(
        &layout,
        Proxy::new(
            list,
            host.connect,
            Arc::clone(&records) as Arc<dyn proxy::Log>,
        ),
        host.sandbox,
        &policy(&dir, &layout, &given),
        &ExecSpec {
            program: request.program.clone().into(),
            args: request.args.clone(),
            env,
            dir: WORK.into(),
        },
        stdout,
        stderr,
        stop,
    )
    .await;
    let target_bytes = size(&layout.target);
    let removed = layout.remove();
    let (started, wall, status) = outcome?;
    removed?;
    let (denials, failures) = records.take();
    Ok(Report {
        started,
        wall,
        target_bytes,
        denials,
        failures,
        status,
    })
}

/// Stands the proxy at the layout's socket, creates the instance, runs the
/// command in it and destroys it, and closes the proxy once the command has
/// ended (AG-19).
#[expect(
    clippy::too_many_arguments,
    reason = "each is one part of the command, and none groups with another"
)]
async fn in_the_instance<C: Connect>(
    layout: &Layout,
    proxy: Proxy<C>,
    sandbox: &dyn Sandbox,
    policy: &SandboxPolicy,
    spec: &ExecSpec,
    stdout: &mut (impl AsyncWrite + Unpin + Send),
    stderr: impl AsyncWrite + Send + 'static,
    stop: impl Future<Output = ()> + Send,
) -> Result<(SystemTime, Duration, ExitStatus), Error> {
    let bound = proxy.bind(&layout.proxy)?;
    let live = CancellationToken::new();
    // The command has no run; the id of the run is none of the store's.
    let instance = sandbox.create(RunId(0), policy).await?;
    let command = async {
        let outcome = command(instance.as_ref(), spec, stdout, stderr, stop).await;
        let destroyed = instance.destroy().await;
        live.cancel();
        outcome.and_then(|outcome| destroyed.map(|()| outcome).map_err(Error::from))
    };
    let ((), outcome) = tokio::join!(bound.serve(live.clone()), command);
    outcome
}

/// Starts `spec` in `instance` and copies its stdout to `stdout` until it
/// ends, or stops it where `stop` ends first.
async fn command(
    instance: &dyn SandboxInstance,
    spec: &ExecSpec,
    stdout: &mut (impl AsyncWrite + Unpin + Send),
    stderr: impl AsyncWrite + Send + 'static,
    stop: impl Future<Output = ()> + Send,
) -> Result<(SystemTime, Duration, ExitStatus), Error> {
    let mut command = instance.command(spec)?;
    // The bytes as they come, for a terminal: no line is cut or dropped.
    command.raw_stdout();
    let (started, clock) = (SystemTime::now(), Instant::now());
    let mut process = command.spawn(stderr).await?;
    let mut stop = std::pin::pin!(stop);
    let status = loop {
        tokio::select! {
            line = process.next_line() => match line? {
                Some(line) => write(stdout, &line).await?,
                None => break process.wait().await?,
            },
            () = &mut stop => {
                let stopped = process.stop(StopTimes::AG_12, |_| false).await?;
                for line in &stopped.lines {
                    write(stdout, line).await?;
                }
                break stopped.status;
            }
        }
    };
    stdout.flush().await.map_err(Error::Output)?;
    Ok((started, clock.elapsed(), status))
}

/// Writes the bytes of `line` to `stdout`.
async fn write(stdout: &mut (impl AsyncWrite + Unpin), line: &Line) -> Result<(), Error> {
    match line {
        Line::Bytes(bytes) => stdout.write_all(bytes).await.map_err(Error::Output),
        // A raw stdout gives bytes alone.
        Line::Text(_) | Line::TooLong | Line::NotUtf8 => Ok(()),
    }
}

/// The operator configuration of the user that `sf` runs as,
/// `~/.config/sf/config.toml` (16.1, CFG-4).
///
/// # Errors
///
/// Where `HOME` is not set, or the file cannot be read or is invalid.
pub fn operator_config() -> Result<OperatorConfig, Error> {
    let home = std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .ok_or_else(|| {
            Error::Config("HOME is not set, so no operator configuration is found".to_owned())
        })?;
    let path = Path::new(&home).join(".config/sf/config.toml");
    let bytes = std::fs::read(&path).map_err(|error| {
        Error::Config(format!(
            "cannot read the operator configuration {}: {error}",
            path.display()
        ))
    })?;
    OperatorConfig::parse(&bytes).map_err(|error| {
        Error::Config(format!(
            "the operator configuration {} is invalid: {error}",
            path.display()
        ))
    })
}

/// The program `name` on the `PATH` of this process.
fn on_the_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .iter()
        .flat_map(std::env::split_paths)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// The backend of `level` on this host: the bubblewrap that the `PATH`
/// finds, with this binary as the supervisor shim and the first process. Off
/// Linux there is none (4.4). One function for both, so that the mutation
/// step mutates the code that the build holds (DEV-2a).
fn backend(level: Level) -> Result<Box<dyn Sandbox>, Error> {
    if level != Level::Process {
        return Err(Error::Level(format!(
            "the sandbox level `{}` has no backend in this build (SEC-7)",
            level.as_str()
        )));
    }
    #[cfg(target_os = "linux")]
    {
        let sf = std::env::current_exe().map_err(|source| Error::Host {
            what: "find the sf binary".to_owned(),
            source,
        })?;
        let bwrap = on_the_path("bwrap").ok_or_else(|| {
            Error::Level("bubblewrap is not installed: no `bwrap` is on the PATH (4.4)".to_owned())
        })?;
        Ok(Box::new(factory_sandbox::bubblewrap::Bubblewrap::new(
            bwrap,
            crate::supervise::supervisor(&sf),
            sf,
        )))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = on_the_path;
        Err(Error::Level(format!(
            "the sandbox level `{}` has no backend off Linux (4.4)",
            level.as_str()
        )))
    }
}

/// The handlers of SIGINT and SIGTERM, installed before anything is created,
/// so that a signal that comes before the command starts ends no `sf` that
/// has left the directory of a command behind: the command then starts and
/// is stopped at once, and the directory is removed.
struct Interrupts(Option<(tokio::signal::unix::Signal, tokio::signal::unix::Signal)>);

impl Interrupts {
    /// Installs the handlers. Where one cannot be installed there are none.
    fn install() -> Self {
        use tokio::signal::unix::{SignalKind, signal};

        Self(
            signal(SignalKind::interrupt())
                .and_then(|interrupt| Ok((interrupt, signal(SignalKind::terminate())?)))
                .ok(),
        )
    }

    /// Ends at the first SIGINT or SIGTERM since [`Interrupts::install`].
    /// Without handlers it never ends.
    async fn received(self) {
        match self.0 {
            Some((mut interrupt, mut terminate)) => {
                tokio::select! {
                    _ = interrupt.recv() => {}
                    _ = terminate.recv() => {}
                }
            }
            None => std::future::pending().await,
        }
    }
}

/// `sf sandbox exec` in the command form: runs `request` with the operator
/// configuration of this user, writes the report or what failed to stderr,
/// as JSON where `json` holds, and exits with the command's status, or with [`FAILED`]
/// where it ran no command.
pub fn main(request: &Request, json: bool) -> ExitCode {
    let failed = |error: &Error| {
        if json {
            eprintln!("{}", serde_json::json!({ "error": error.to_string() }));
        } else {
            eprintln!("sf sandbox exec: {error}");
        }
        ExitCode::from(FAILED)
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(source) => {
            return failed(&Error::Host {
                what: "start the runtime".to_owned(),
                source,
            });
        }
    };
    let outcome = runtime.block_on(async {
        let interrupts = Interrupts::install();
        let config = operator_config()?;
        let sandbox = backend(level(&config))?;
        let triple = HostTriple::THIS
            .ok_or_else(|| Error::Level("this build has no host triple of 4.4".to_owned()))?;
        let host = Host {
            sandbox: sandbox.as_ref(),
            connect: proxy::resolve,
            ecosystem: &Rust,
            triple,
        };
        run(
            &config,
            request,
            host,
            &mut tokio::io::stdout(),
            tokio::io::stderr(),
            interrupts.received(),
        )
        .await
    });
    match outcome {
        Ok(report) => {
            if json {
                eprintln!("{}", report.json());
            } else {
                eprint!("{}", report.text());
            }
            report.exit_code()
        }
        Err(error) => failed(&error),
    }
}

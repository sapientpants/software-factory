//! The Sandbox traits, SandboxPolicy and backends (SPEC 19.1).
//!
//! A [`Sandbox`] is a backend, which [`Sandbox::create`]s one
//! [`SandboxInstance`] for each stage execution from that stage's
//! [`SandboxPolicy`] (13.3). The instance turns an [`ExecSpec`] into a
//! [`factory_proc::Command`] that runs inside it, through the supervisor shim
//! (AG-16). The bubblewrap backend of level `process` is compiled for Linux
//! alone (4.4): on macOS the crate holds the traits and the sandbox policy, and no
//! backend.

use std::ffi::OsString;
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;

use factory_core::engine::RunId;

#[cfg(target_os = "linux")]
pub mod bubblewrap;
pub mod seccomp;

/// The port of the loopback interface at which a worker at level `process`
/// reaches the egress proxy, one number for every instance (SEC-10a). The
/// host names it in the proxy variables of the environment (SEC-10b).
pub const PROXY_PORT: u16 = 3128;

/// A future that a trait method of this crate returns: the traits are used
/// as objects, which a method declared `async` cannot be.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The sandbox level of an instance (13.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Level {
    /// The passthrough, for the fake agent alone.
    None,
    /// bubblewrap.
    Process,
    /// A microVM.
    Hardened,
}

impl Level {
    /// The level as `SF_SANDBOX` holds it (AG-11a).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Process => "process",
            Self::Hardened => "hardened",
        }
    }

    /// What a backend must provide for the level (13.3).
    pub fn requires(self) -> SandboxCaps {
        let process = SandboxCaps {
            fs_isolation: true,
            process_isolation: true,
            egress_filtering: true,
            vm_boundary: false,
        };
        match self {
            Self::None => SandboxCaps::default(),
            Self::Process => process,
            Self::Hardened => SandboxCaps {
                vm_boundary: true,
                ..process
            },
        }
    }
}

/// What a backend provides (13.3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SandboxCaps {
    /// The filesystem policy of SEC-6.
    pub fs_isolation: bool,
    /// The process isolation of SEC-6a.
    pub process_isolation: bool,
    /// No network but what SEC-10 allows.
    pub egress_filtering: bool,
    /// A virtual machine between a worker and the host's kernel.
    pub vm_boundary: bool,
}

impl SandboxCaps {
    /// Whether these capabilities provide `level`: a run does not start where
    /// they do not (SEC-7).
    pub fn provides(self, level: Level) -> bool {
        let required = level.requires();
        (self.fs_isolation || !required.fs_isolation)
            && (self.process_isolation || !required.process_isolation)
            && (self.egress_filtering || !required.egress_filtering)
            && (self.vm_boundary || !required.vm_boundary)
    }
}

/// How a worker may use a path that a sandbox policy shows it (SEC-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Read and execute.
    ReadOnly,
    /// Read, execute and write.
    ReadWrite,
}

/// A path of the host that an instance shows a worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    /// Where it lies on the host.
    pub host: PathBuf,
    /// Where the worker sees it.
    pub sandbox: PathBuf,
    /// What the worker may do with it.
    pub access: Access,
}

impl Mount {
    /// `path`, shown at the path that it has on the host.
    pub fn at_its_path(path: impl Into<PathBuf>, access: Access) -> Self {
        let path = path.into();
        Self {
            host: path.clone(),
            sandbox: path,
            access,
        }
    }
}

/// What an instance shows a worker beside the backend's OS baseline (SEC-6),
/// which the caller does not choose. Everything else is denied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxPolicy {
    /// The paths of the host that the stage's role is given, in order: a
    /// later mount lies over an earlier one.
    pub mounts: Vec<Mount>,
    /// The temporary directory of the stage execution, `exec/<seq>/tmp/` on
    /// the host (SEC-6c, 9.1). [`Sandbox::create`] creates it with mode 0700,
    /// and the worker sees it as `/tmp`. It must not exist yet.
    pub tmp: PathBuf,
    /// The Unix socket of the egress proxy's listener for the stage
    /// execution, `exec/<seq>/proxy.sock` on the host (SEC-10b, 9.1). At
    /// level `process` the sandbox shows it at one fixed path, and the first
    /// process forwards a loopback port to it (SEC-10a). It must exist.
    pub proxy: PathBuf,
}

impl SandboxPolicy {
    /// Checks what the sandbox policy asks against what no instance shows (SEC-6,
    /// SEC-6c, SEC-6d).
    ///
    /// # Errors
    ///
    /// [`Error::Policy`] where a path is not absolute, holds `.` or `..`, or
    /// where a mount would be shown at the root, at or below `/proc`, `/dev`,
    /// `/tmp` or `/sf`, at `/etc`, or at or below `/etc/claude-code`.
    pub fn check(&self) -> Result<(), Error> {
        plain(&self.tmp, "the temporary directory")?;
        plain(&self.proxy, "the proxy's socket")?;
        for mount in &self.mounts {
            plain(&mount.host, "a mount's host path")?;
            plain(&mount.sandbox, "a mount's path in the sandbox")?;
            if let Some(reason) = refused(&mount.sandbox) {
                return Err(Error::Policy(format!(
                    "{} cannot be shown at {}: {reason}",
                    mount.host.display(),
                    mount.sandbox.display()
                )));
            }
        }
        Ok(())
    }
}

/// Fails where `path` is not absolute, holds `.` or `..`, or is not written
/// as its parts joined by single slashes.
fn plain(path: &Path, what: &str) -> Result<(), Error> {
    // `components` leaves out a `.` that is not at the start, and a doubled
    // or a trailing slash, so the path is compared with its parts joined.
    let joined: PathBuf = path.components().collect();
    let normal = path.is_absolute()
        && joined.as_os_str() == path.as_os_str()
        && path
            .components()
            .all(|part| matches!(part, Component::RootDir | Component::Normal(_)));
    if normal {
        Ok(())
    } else {
        Err(Error::Policy(format!(
            "{what}, {}, is no absolute path without `.` and `..`",
            path.display()
        )))
    }
}

/// Why no mount is shown at `sandbox`, where none is.
fn refused(sandbox: &Path) -> Option<&'static str> {
    if sandbox == Path::new("/") {
        return Some("the root is the backend's");
    }
    if sandbox.starts_with("/proc") || sandbox.starts_with("/dev") {
        return Some("the backend shows a private /proc and /dev (SEC-6a)");
    }
    if sandbox.starts_with("/tmp") {
        return Some("/tmp is the temporary directory of the sandbox policy (SEC-6c)");
    }
    if sandbox.starts_with("/sf") {
        return Some("/sf holds the first process and the proxy's socket (SEC-10a)");
    }
    if sandbox == Path::new("/etc") || sandbox.starts_with("/etc/claude-code") {
        return Some("no instance shows /etc/claude-code (SEC-6d)");
    }
    None
}

/// A command to run in an instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecSpec {
    /// The program, as the worker sees it: a path, or a name that the `PATH`
    /// of `env` finds.
    pub program: PathBuf,
    /// Its arguments.
    pub args: Vec<OsString>,
    /// Its environment, of the allowlist of AG-11. The instance sets
    /// `SF_SANDBOX` and `TMPDIR` itself, and a value of either here is
    /// replaced.
    pub env: Vec<(OsString, OsString)>,
    /// Its working directory, as the worker sees it.
    pub dir: PathBuf,
}

impl ExecSpec {
    /// `program` with `args`, in `dir`, with no variable.
    pub fn new(
        program: impl Into<PathBuf>,
        args: impl IntoIterator<Item = impl Into<OsString>>,
        dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            env: Vec::new(),
            dir: dir.into(),
        }
    }

    /// Adds a variable to the environment.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
}

/// What is journalled of an instance before its first exec (AG-15), and what
/// [`Sandbox::reap`] reads after a daemon restart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxRecord {
    /// The run that the instance was created for.
    pub run: RunId,
    /// Its level.
    pub level: Level,
    /// Its temporary directory on the host, which also names its processes
    /// to [`Sandbox::reap`].
    pub tmp: PathBuf,
}

/// A backend (13.3).
pub trait Sandbox: Send + Sync {
    /// What it provides.
    fn capabilities(&self) -> SandboxCaps;

    /// One instance for a stage execution of `run`, from `policy`.
    fn create<'a>(
        &'a self,
        run: RunId,
        policy: &'a SandboxPolicy,
    ) -> BoxFuture<'a, Result<Box<dyn SandboxInstance>, Error>>;

    /// Ends what is left of the instance of `record` after a daemon restart:
    /// its processes and its temporary directory.
    fn reap<'a>(&'a self, record: &'a SandboxRecord) -> BoxFuture<'a, Result<(), Error>>;
}

/// One instance, for one stage execution (13.3).
pub trait SandboxInstance: Send + Sync {
    /// What is journalled of it before the first exec (AG-15).
    fn record(&self) -> SandboxRecord;

    /// A command that runs `spec` inside the instance, through the supervisor
    /// shim (AG-16). The caller may set its stdin and stdout further, and
    /// starts it.
    ///
    /// # Errors
    ///
    /// Where the backend cannot run `spec`.
    fn command(&self, spec: &ExecSpec) -> Result<factory_proc::Command, Error>;

    /// Kills every process of the instance, and returns once none is left.
    fn kill_all(&self) -> BoxFuture<'_, Result<(), Error>>;

    /// Kills every process of the instance and removes what the instance made
    /// on the host.
    fn destroy(self: Box<Self>) -> BoxFuture<'static, Result<(), Error>>;
}

/// What a backend failed at.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The sandbox policy asks for what no instance shows.
    #[error("the sandbox policy is refused: {0}")]
    Policy(String),
    /// A call on the host failed.
    #[error("cannot {what}: {source}")]
    Host {
        /// What was being done.
        what: String,
        /// Why it failed.
        source: std::io::Error,
    },
    /// The seccomp filter of SEC-6e could not be compiled.
    #[error("cannot compile the seccomp filter: {0}")]
    Filter(String),
    /// A process of the instance outlived the kill.
    #[error("processes of the sandbox outlived the kill: {0:?}")]
    Survivors(Vec<i32>),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(mounts: Vec<Mount>) -> SandboxPolicy {
        SandboxPolicy {
            mounts,
            tmp: PathBuf::from("/srv/sf/runs/r/exec/1/tmp"),
            proxy: PathBuf::from("/srv/sf/runs/r/exec/1/proxy.sock"),
        }
    }

    fn at(sandbox: &str) -> Mount {
        Mount {
            host: PathBuf::from("/srv/sf/runs/r/workspace"),
            sandbox: PathBuf::from(sandbox),
            access: Access::ReadWrite,
        }
    }

    #[test]
    fn a_policy_of_plain_paths_outside_the_backends_own_passes() {
        let policy = policy(vec![
            at("/srv/sf/runs/r/workspace"),
            at("/opt/sf/current"),
            at("/etc/claude-code-not"),
            at("/tmpfoo"),
            at("/devices"),
            at("/sfx"),
        ]);

        assert!(policy.check().is_ok(), "{:?}", policy.check());
    }

    #[test]
    fn a_mount_at_the_backends_own_paths_is_refused() {
        for path in [
            "/",
            "/proc",
            "/proc/1",
            "/dev",
            "/dev/shm",
            "/tmp",
            "/tmp/x",
            "/sf",
            "/sf/sf",
            "/etc",
            "/etc/claude-code",
            "/etc/claude-code/managed-settings.json",
        ] {
            let outcome = policy(vec![at(path)]).check();
            assert!(
                matches!(outcome, Err(Error::Policy(_))),
                "{path}: {outcome:?}"
            );
        }
    }

    #[test]
    fn a_path_that_is_relative_or_holds_dots_is_refused() {
        let mut relative = at("/srv/x");
        relative.host = PathBuf::from("srv/x");
        let mut dotted = at("/srv/x");
        dotted.sandbox = PathBuf::from("/srv/../etc/claude-code");
        let mut tmp = policy(Vec::new());
        tmp.tmp = PathBuf::from("/srv/./tmp");
        let mut proxy = policy(Vec::new());
        proxy.proxy = PathBuf::from("proxy.sock");
        let mut doubled = at("/srv//x");
        doubled.host = PathBuf::from("/srv/x/");

        for policy in [
            policy(vec![relative]),
            policy(vec![dotted]),
            tmp,
            proxy,
            policy(vec![doubled]),
        ] {
            let outcome = policy.check();
            assert!(matches!(outcome, Err(Error::Policy(_))), "{outcome:?}");
        }
    }

    #[test]
    fn each_level_requires_what_13_3_gives_it() {
        let none = SandboxCaps::default();
        let process = Level::Process.requires();
        let hardened = Level::Hardened.requires();

        assert_eq!(Level::None.requires(), none);
        assert!(process.fs_isolation && process.process_isolation && process.egress_filtering);
        assert!(!process.vm_boundary && hardened.vm_boundary);
        assert!(none.provides(Level::None));
        assert!(!none.provides(Level::Process));
        assert!(process.provides(Level::Process) && !process.provides(Level::Hardened));
        assert!(hardened.provides(Level::Process) && hardened.provides(Level::Hardened));
        for missing in [
            SandboxCaps {
                fs_isolation: false,
                ..process
            },
            SandboxCaps {
                process_isolation: false,
                ..process
            },
            SandboxCaps {
                egress_filtering: false,
                ..process
            },
        ] {
            assert!(!missing.provides(Level::Process), "{missing:?}");
        }
    }

    #[test]
    fn the_level_reads_as_sf_sandbox_holds_it() {
        assert_eq!(
            [Level::None, Level::Process, Level::Hardened].map(Level::as_str),
            ["none", "process", "hardened"]
        );
    }

    #[test]
    fn a_spec_holds_its_variables_in_order() {
        let spec = ExecSpec::new("/bin/sh", ["-c", "true"], "/w")
            .with_env("PATH", "/usr/bin")
            .with_env("HOME", "/h");

        assert_eq!(spec.args, [OsString::from("-c"), OsString::from("true")]);
        assert_eq!(
            spec.env,
            [
                (OsString::from("PATH"), OsString::from("/usr/bin")),
                (OsString::from("HOME"), OsString::from("/h"))
            ]
        );
    }
}

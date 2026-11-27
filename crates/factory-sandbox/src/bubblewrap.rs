//! The bubblewrap backend of level `process` (13.3), compiled for Linux
//! alone (4.4).
//!
//! An instance is a set of arguments of `bwrap`: the namespaces of level
//! `process`, the OS baseline, the temporary directory as `/tmp` and the
//! mounts of the sandbox policy, over a root that is read-only (SEC-6, SEC-6a to
//! SEC-6d). [`SandboxInstance::command`] gives a [`Command`] that the
//! supervisor shim starts (AG-16): the shim starts `bwrap`, and `bwrap`
//! starts the command. The instance starts nothing itself. It knows its
//! processes on the host by the temporary directory in their arguments, which
//! is its own, and kills them by that, also after a daemon restart.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use factory_core::engine::RunId;
use factory_proc::{Command, Supervisor};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use tokio::time::Instant;

use crate::{
    Access, BoxFuture, Error, ExecSpec, Level, SandboxCaps, SandboxInstance, SandboxPolicy,
    SandboxRecord,
};

/// The OS baseline of SEC-6: the paths of the host's system that every
/// instance shows read-only, at the same path. A path that the host does not
/// have is left out, and one that is a symbolic link on the host is the same
/// link in the sandbox. `/etc` is not shown whole, so `/etc/claude-code` is
/// not shown, whatever the host holds there (SEC-6d).
pub const BASELINE: &[&str] = &[
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib32",
    "/lib64",
    "/libx32",
    // Where the dynamic loader looks.
    "/etc/ld.so.cache",
    "/etc/ld.so.conf",
    "/etc/ld.so.conf.d",
    // The links of Debian's alternatives, such as `cc`.
    "/etc/alternatives",
    // The certificate authorities, for TLS through the proxy (SEC-10b).
    "/etc/ssl",
    "/etc/ca-certificates",
    // Names: of `localhost`, of the worker's user and its groups, of
    // protocols and services, and the time zone. None holds a secret.
    "/etc/nsswitch.conf",
    "/etc/host.conf",
    "/etc/hosts",
    "/etc/passwd",
    "/etc/group",
    "/etc/protocols",
    "/etc/services",
    "/etc/localtime",
];

/// The options of level `process` that come before the mounts: PID, IPC,
/// network and user namespaces with a private `/proc` and `/dev` (SEC-6a);
/// no user namespace of the worker's own (SEC-6b); and no process of the
/// sandbox that outlives `bwrap` or keeps a controlling terminal (AG-16,
/// SEC-6e).
const NAMESPACES: &[&str] = &[
    "--unshare-user",
    "--unshare-pid",
    "--unshare-ipc",
    "--unshare-net",
    "--unshare-uts",
    "--unshare-cgroup-try",
    "--disable-userns",
    "--die-with-parent",
    "--new-session",
    "--proc",
    "/proc",
    "--dev",
    "/dev",
];

/// How long a kill waits for the processes of an instance to end.
const KILL_WAIT: Duration = Duration::from_secs(5);

/// How a path of [`BASELINE`] stands on the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnHost {
    /// It does not exist.
    Absent,
    /// A symbolic link, to this target.
    Link(PathBuf),
    /// A file or a directory.
    Present,
}

/// How `path` stands on this host.
fn on_this_host(path: &Path) -> OnHost {
    match std::fs::symlink_metadata(path) {
        Err(_) => OnHost::Absent,
        Ok(metadata) if metadata.file_type().is_symlink() => {
            std::fs::read_link(path).map_or(OnHost::Absent, OnHost::Link)
        }
        Ok(_) => OnHost::Present,
    }
}

/// The arguments of `bwrap` that show the baseline, where `host` says how
/// each of its paths stands.
pub fn baseline(host: impl Fn(&Path) -> OnHost) -> Vec<OsString> {
    let mut arguments = Vec::new();
    for path in BASELINE {
        match host(Path::new(path)) {
            OnHost::Absent => {}
            OnHost::Link(target) => {
                arguments.extend(["--symlink".into(), target.into_os_string(), path.into()]);
            }
            OnHost::Present => arguments.extend(["--ro-bind".into(), path.into(), path.into()]),
        }
    }
    arguments
}

/// Every argument of `bwrap` for `spec` in an instance of the sandbox policy `policy`, with the
/// baseline `baseline`.
pub fn arguments(policy: &SandboxPolicy, baseline: &[OsString], spec: &ExecSpec) -> Vec<OsString> {
    let mut arguments: Vec<OsString> = NAMESPACES.iter().map(OsString::from).collect();
    arguments.extend(baseline.iter().cloned());
    arguments.extend([
        "--bind".into(),
        policy.tmp.clone().into_os_string(),
        "/tmp".into(),
    ]);
    for mount in &policy.mounts {
        let option = match mount.access {
            Access::ReadOnly => "--ro-bind",
            Access::ReadWrite => "--bind",
        };
        arguments.extend([
            option.into(),
            mount.host.clone().into_os_string(),
            mount.sandbox.clone().into_os_string(),
        ]);
    }
    // The root that bubblewrap creates, once every mount point is in it, and
    // the `/dev` of `--dev`: each is a memory-backed file system that the
    // worker's user owns, so a write there would count against the host's
    // memory (SEC-6c). The devices in `/dev` are mounts of their own, and
    // stay writable.
    arguments.extend([
        "--remount-ro".into(),
        "/".into(),
        "--remount-ro".into(),
        "/dev".into(),
    ]);
    arguments.extend([
        "--chdir".into(),
        spec.dir.clone().into_os_string(),
        "--".into(),
        spec.program.clone().into_os_string(),
    ]);
    arguments.extend(spec.args.iter().cloned());
    arguments
}

/// The bubblewrap backend.
#[derive(Debug, Clone)]
pub struct Bubblewrap {
    program: PathBuf,
    supervisor: Supervisor,
}

impl Bubblewrap {
    /// The backend that runs `program`, the `bwrap` of the host, through
    /// `supervisor`, the shim of AG-16.
    pub fn new(program: impl Into<PathBuf>, supervisor: Supervisor) -> Self {
        Self {
            program: program.into(),
            supervisor,
        }
    }
}

impl crate::Sandbox for Bubblewrap {
    /// The isolation of the filesystem and of the processes. The network is
    /// denied whole, since the instance holds no interface but the loopback,
    /// but nothing filters it: until the egress proxy exists (SEC-10b) the
    /// backend does not provide level `process` (SEC-7).
    fn capabilities(&self) -> SandboxCaps {
        SandboxCaps {
            egress_filtering: false,
            ..Level::Process.requires()
        }
    }

    fn create<'a>(
        &'a self,
        run: RunId,
        policy: &'a SandboxPolicy,
    ) -> BoxFuture<'a, Result<Box<dyn SandboxInstance>, Error>> {
        Box::pin(async move {
            policy.check()?;
            std::fs::metadata(&self.program).map_err(|source| Error::Host {
                what: format!("find bubblewrap at {}", self.program.display()),
                source,
            })?;
            let tmp_error = |source| Error::Host {
                what: format!("create the temporary directory {}", policy.tmp.display()),
                source,
            };
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&policy.tmp)
                .map_err(tmp_error)?;
            // The mode is 0700 whatever the umask (SEC-6c).
            std::fs::set_permissions(&policy.tmp, std::fs::Permissions::from_mode(0o700))
                .map_err(tmp_error)?;
            let instance: Box<dyn SandboxInstance> = Box::new(Instance {
                program: self.program.clone(),
                supervisor: self.supervisor.clone(),
                baseline: baseline(on_this_host),
                record: SandboxRecord {
                    run,
                    level: Level::Process,
                    tmp: policy.tmp.clone(),
                },
                policy: policy.clone(),
            });
            Ok(instance)
        })
    }

    fn reap<'a>(&'a self, record: &'a SandboxRecord) -> BoxFuture<'a, Result<(), Error>> {
        Box::pin(async move {
            sweep(&record.tmp).await?;
            remove(&record.tmp)
        })
    }
}

/// An instance of the bubblewrap backend.
struct Instance {
    program: PathBuf,
    supervisor: Supervisor,
    baseline: Vec<OsString>,
    record: SandboxRecord,
    policy: SandboxPolicy,
}

impl SandboxInstance for Instance {
    fn record(&self) -> SandboxRecord {
        self.record.clone()
    }

    fn command(&self, spec: &ExecSpec) -> Result<Command, Error> {
        let mut command = Command::new(&self.program);
        command.args(arguments(&self.policy, &self.baseline, spec));
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        // Set after the spec's variables, so that each replaces a value of
        // the spec's (AG-11a, SEC-6c).
        command
            .env("SF_SANDBOX", Level::Process.as_str())
            .env("TMPDIR", "/tmp")
            .supervised_by(self.supervisor.clone());
        Ok(command)
    }

    fn kill_all(&self) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(sweep(&self.record.tmp))
    }

    fn destroy(self: Box<Self>) -> BoxFuture<'static, Result<(), Error>> {
        Box::pin(async move {
            sweep(&self.record.tmp).await?;
            remove(&self.record.tmp)
        })
    }
}

/// A process of the host, as `/proc` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    /// Its pid.
    pub pid: i32,
    /// Its parent's.
    pub ppid: i32,
    /// Whether it has ended and waits to be reaped.
    pub zombie: bool,
    /// Its arguments, the program's name first.
    pub argv: Vec<OsString>,
}

/// The live processes of the instance whose temporary directory is `tmp`,
/// among `table`: each `bwrap` whose arguments show `tmp` as `/tmp`, and
/// every process below one. A `bwrap` is a program of that name in any
/// directory, since that of a daemon that was restarted may lie at another
/// path. The supervisor shim, whose arguments hold those of its `bwrap`, is
/// none: it reports the command's end to the host (AG-16). A process that has
/// ended is none.
pub fn of_the_instance(table: &[Process], tmp: &Path) -> Vec<i32> {
    let shows_tmp = |argv: &[OsString]| {
        argv.windows(3).any(|window| {
            window[0] == "--bind" && Path::new(&window[1]) == tmp && window[2] == "/tmp"
        })
    };
    let mut pending: Vec<i32> = table
        .iter()
        .filter(|process| {
            let program = process.argv.first().map(Path::new);
            program.and_then(Path::file_name) == Some(OsStr::new("bwrap"))
                && shows_tmp(&process.argv)
        })
        .map(|process| process.pid)
        .collect();
    let mut tree = Vec::new();
    while let Some(parent) = pending.pop() {
        if tree.contains(&parent) {
            continue;
        }
        tree.push(parent);
        pending.extend(
            table
                .iter()
                .filter(|process| process.ppid == parent)
                .map(|process| process.pid),
        );
    }
    tree.retain(|pid| {
        table
            .iter()
            .any(|process| process.pid == *pid && !process.zombie)
    });
    tree
}

/// The pid, the parent's pid and whether the process has ended, from the
/// text of `/proc/<pid>/stat`.
pub fn parse_stat(stat: &[u8]) -> Option<(i32, i32, bool)> {
    let text = std::str::from_utf8(stat).ok()?;
    let (pid, rest) = text.split_once(" (")?;
    // The name may hold `)`: the fields go on after the last.
    let (_, fields) = rest.rsplit_once(") ")?;
    let mut fields = fields.split_whitespace();
    let state = fields.next()?;
    let ppid = fields.next()?.parse().ok()?;
    Some((pid.parse().ok()?, ppid, matches!(state, "Z" | "X")))
}

/// The processes of the host that this user can read. One that ends while
/// it is read is left out.
fn processes() -> Result<Vec<Process>, Error> {
    let entries = std::fs::read_dir("/proc").map_err(|source| Error::Host {
        what: "read /proc".to_owned(),
        source,
    })?;
    let mut table = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.as_bytes().iter().all(u8::is_ascii_digit) {
            continue;
        }
        let dir = entry.path();
        let (Ok(stat), Ok(cmdline)) = (
            std::fs::read(dir.join("stat")),
            std::fs::read(dir.join("cmdline")),
        ) else {
            continue;
        };
        let Some((pid, ppid, zombie)) = parse_stat(&stat) else {
            continue;
        };
        let argv = cmdline
            .split(|byte| *byte == 0)
            .filter(|word| !word.is_empty())
            .map(|word| OsStr::from_bytes(word).to_owned())
            .collect();
        table.push(Process {
            pid,
            ppid,
            zombie,
            argv,
        });
    }
    Ok(table)
}

/// Kills every process of the instance with SIGKILL, again until none is
/// left, for at most [`KILL_WAIT`]. With `--die-with-parent` the end of the
/// outer `bwrap` ends the sandbox too; the processes inside are killed as
/// well, so that the sweep does not rest on it.
async fn sweep(tmp: &Path) -> Result<(), Error> {
    let deadline = Instant::now() + KILL_WAIT;
    loop {
        let live = of_the_instance(&processes()?, tmp);
        if live.is_empty() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error::Survivors(live));
        }
        for pid in live {
            // One that has ended meanwhile is no failure.
            let _ = kill(Pid::from_raw(pid), Signal::SIGKILL);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Removes the temporary directory `tmp`, also where a worker left a
/// directory in it that its owner cannot write.
fn remove(tmp: &Path) -> Result<(), Error> {
    let failed = |source| Error::Host {
        what: format!("remove the temporary directory {}", tmp.display()),
        source,
    };
    match std::fs::remove_dir_all(tmp) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => {
            writable(tmp);
            std::fs::remove_dir_all(tmp).map_err(failed)
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mount;

    fn words(arguments: &[OsString]) -> Vec<&str> {
        arguments
            .iter()
            .map(|word| word.to_str().unwrap_or("<not UTF-8>"))
            .collect()
    }

    /// The baseline as a Debian host with a merged `/usr` gives it.
    fn debian(path: &Path) -> OnHost {
        match path.to_str() {
            Some("/bin") => OnHost::Link("usr/bin".into()),
            Some("/sbin") => OnHost::Link("usr/sbin".into()),
            Some("/lib") => OnHost::Link("usr/lib".into()),
            Some("/lib64") => OnHost::Link("usr/lib64".into()),
            Some("/etc/localtime") => OnHost::Link("/usr/share/zoneinfo/Etc/UTC".into()),
            Some("/lib32" | "/libx32" | "/etc/ca-certificates") => OnHost::Absent,
            _ => OnHost::Present,
        }
    }

    #[test]
    fn the_baseline_is_the_list_of_sec_6() {
        // The snapshot of the OS baseline (SEC-6): a change to it is a change
        // to this list.
        assert_eq!(
            BASELINE,
            [
                "/usr",
                "/bin",
                "/sbin",
                "/lib",
                "/lib32",
                "/lib64",
                "/libx32",
                "/etc/ld.so.cache",
                "/etc/ld.so.conf",
                "/etc/ld.so.conf.d",
                "/etc/alternatives",
                "/etc/ssl",
                "/etc/ca-certificates",
                "/etc/nsswitch.conf",
                "/etc/host.conf",
                "/etc/hosts",
                "/etc/passwd",
                "/etc/group",
                "/etc/protocols",
                "/etc/services",
                "/etc/localtime",
            ]
        );
    }

    #[test]
    fn the_baseline_shows_no_etc_claude_code() {
        for path in BASELINE.iter().map(Path::new) {
            assert_ne!(path, Path::new("/"), "the root");
            assert_ne!(path, Path::new("/etc"), "/etc whole (SEC-6d)");
            assert!(!path.starts_with("/etc/claude-code"), "{}", path.display());
        }
    }

    #[test]
    fn the_baseline_follows_how_the_host_holds_each_path() {
        let arguments = baseline(debian);

        assert_eq!(
            words(&arguments)[..15],
            [
                "--ro-bind",
                "/usr",
                "/usr",
                "--symlink",
                "usr/bin",
                "/bin",
                "--symlink",
                "usr/sbin",
                "/sbin",
                "--symlink",
                "usr/lib",
                "/lib",
                "--symlink",
                "usr/lib64",
                "/lib64",
            ]
        );
        let words = words(&arguments);
        assert!(!words.contains(&"/lib32") && !words.contains(&"/etc/ca-certificates"));
        assert!(
            words
                .windows(3)
                .any(|w| w == ["--symlink", "/usr/share/zoneinfo/Etc/UTC", "/etc/localtime"])
        );
        assert!(
            words
                .windows(3)
                .any(|w| w == ["--ro-bind", "/etc/passwd", "/etc/passwd"])
        );
    }

    #[test]
    fn this_hosts_baseline_reads_the_host() {
        assert_eq!(
            on_this_host(Path::new("/proc/self/no-such-entry")),
            OnHost::Absent
        );
        assert_eq!(on_this_host(Path::new("/proc")), OnHost::Present);
        assert_eq!(
            on_this_host(Path::new("/proc/self")),
            OnHost::Link(std::process::id().to_string().into())
        );
    }

    #[test]
    fn the_arguments_hold_the_namespaces_the_mounts_and_the_command_in_order() {
        let policy = SandboxPolicy {
            mounts: vec![
                Mount::at_its_path("/srv/run/workspace", Access::ReadWrite),
                Mount {
                    host: "/opt/sf/0.1.0/sf".into(),
                    sandbox: "/sf".into(),
                    access: Access::ReadOnly,
                },
            ],
            tmp: "/srv/run/exec/1/tmp".into(),
        };
        let baseline = baseline(|path| {
            if path == Path::new("/usr") {
                OnHost::Present
            } else {
                OnHost::Absent
            }
        });
        let spec = ExecSpec::new("/bin/sh", ["-c", "echo"], "/srv/run/workspace");

        assert_eq!(
            words(&arguments(&policy, &baseline, &spec)),
            [
                "--unshare-user",
                "--unshare-pid",
                "--unshare-ipc",
                "--unshare-net",
                "--unshare-uts",
                "--unshare-cgroup-try",
                "--disable-userns",
                "--die-with-parent",
                "--new-session",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--ro-bind",
                "/usr",
                "/usr",
                "--bind",
                "/srv/run/exec/1/tmp",
                "/tmp",
                "--bind",
                "/srv/run/workspace",
                "/srv/run/workspace",
                "--ro-bind",
                "/opt/sf/0.1.0/sf",
                "/sf",
                "--remount-ro",
                "/",
                "--remount-ro",
                "/dev",
                "--chdir",
                "/srv/run/workspace",
                "--",
                "/bin/sh",
                "-c",
                "echo",
            ]
        );
    }

    #[test]
    fn the_backend_does_not_provide_level_process_before_the_proxy() {
        use crate::Sandbox as _;

        let backend = Bubblewrap::new("/usr/bin/bwrap", Supervisor::new("/sf", ["__supervise"]));
        let caps = backend.capabilities();

        assert!(caps.fs_isolation && caps.process_isolation);
        assert!(!caps.egress_filtering && !caps.vm_boundary);
        assert!(!caps.provides(Level::Process));
    }

    #[test]
    fn removing_a_temporary_directory_that_is_gone_passes() {
        let gone = std::env::temp_dir().join(format!("sf-sandbox-gone-{}", std::process::id()));

        assert!(remove(&gone).is_ok());
    }

    #[test]
    fn a_stat_line_gives_the_pid_the_parent_and_whether_it_ended() {
        assert_eq!(
            parse_stat(b"42 (sleep) S 7 42 42 0 -1"),
            Some((42, 7, false))
        );
        assert_eq!(parse_stat(b"43 (a) b) (c) Z 1 0"), Some((43, 1, true)));
        assert_eq!(parse_stat(b"44 (x) X 2 0"), Some((44, 2, true)));
        for broken in [
            &b""[..],
            b"x (y) S 1",
            b"45 (y) S",
            b"45 y S 1",
            b"\xff (y) S 1",
        ] {
            assert_eq!(parse_stat(broken), None, "{broken:?}");
        }
    }

    fn process(pid: i32, ppid: i32, argv: &[&str]) -> Process {
        Process {
            pid,
            ppid,
            zombie: false,
            argv: argv.iter().map(OsString::from).collect(),
        }
    }

    #[test]
    fn the_instance_is_each_bwrap_that_shows_its_tmp_and_what_lies_below() {
        let ours = ["/usr/bin/bwrap", "--bind", "/run/a/tmp", "/tmp", "--", "sh"];
        let mut zombie = process(14, 12, &["sleep"]);
        zombie.zombie = true;
        let table = [
            process(
                10,
                1,
                &[
                    "/sf",
                    "__supervise",
                    "--",
                    "/usr/bin/bwrap",
                    "--bind",
                    "/run/a/tmp",
                    "/tmp",
                ],
            ),
            process(11, 10, &ours),
            process(12, 11, &ours),
            process(13, 12, &["sh"]),
            zombie,
            process(15, 13, &["sleep"]),
            // Another instance, and the bwrap of a restarted daemon.
            process(20, 1, &["/usr/bin/bwrap", "--bind", "/run/b/tmp", "/tmp"]),
            process(21, 20, &["sh"]),
            process(22, 1, &["/opt/bwrap", "--bind", "/run/a/tmp", "/tmp"]),
            // The words apart, or in another order.
            process(
                23,
                1,
                &["/usr/bin/bwrap", "--bind", "/run/a/tmp", "/x", "/tmp"],
            ),
            process(
                24,
                1,
                &["/usr/bin/bwrap", "--ro-bind", "/run/a/tmp", "/tmp"],
            ),
        ];

        let mut found = of_the_instance(&table, Path::new("/run/a/tmp"));
        found.sort_unstable();

        assert_eq!(found, [11, 12, 13, 15, 22]);
    }
}

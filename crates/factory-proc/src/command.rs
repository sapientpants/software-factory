//! A command to start: its program, arguments, environment and working
//! directory (SPEC AG-11).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use process_wrap::tokio::{CommandWrap, KillOnDrop, ProcessGroup};
use tokio::io::AsyncWrite;

use nix::unistd::Pid;

use crate::Error;
use crate::codec::LineCodec;
use crate::process::Process;
use crate::supervise::{Pipes, reported_group};

/// The cap of a line of stdout where the caller sets none, in bytes
/// *(tunable)*.
pub const MAX_LINE_LENGTH: usize = 1 << 20;

/// A command that [`Command::spawn`] starts as the leader of a process group
/// of its own, with a cleared environment that holds the variables set with
/// [`Command::env`] and no other (AG-11).
#[derive(Debug, Clone)]
pub struct Command {
    program: OsString,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    current_dir: Option<PathBuf>,
    stdin: bool,
    raw_stdout: bool,
    max_line_length: usize,
    supervisor: Option<Supervisor>,
    #[cfg(target_os = "linux")]
    files: Vec<std::sync::Arc<std::os::fd::OwnedFd>>,
}

/// The supervisor shim that a command is started through (AG-16): a program
/// and the arguments that come before those of the shim's protocol, which
/// [`crate::supervise::Supervise::parse`] reads.
#[derive(Debug, Clone)]
pub struct Supervisor {
    program: PathBuf,
    args: Vec<OsString>,
}

impl Supervisor {
    /// The shim `program`, started with `args` first.
    pub fn new(
        program: impl Into<PathBuf>,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

impl Command {
    /// A command that runs `program`, which is a path: the cleared
    /// environment holds no `PATH` to look it up in, unless one is set.
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_owned(),
            args: Vec::new(),
            env: Vec::new(),
            current_dir: None,
            stdin: false,
            raw_stdout: false,
            max_line_length: MAX_LINE_LENGTH,
            supervisor: None,
            #[cfg(target_os = "linux")]
            files: Vec::new(),
        }
    }

    /// Adds an argument.
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.args.push(arg.as_ref().to_owned());
        self
    }

    /// Adds arguments.
    pub fn args(&mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> &mut Self {
        self.args
            .extend(args.into_iter().map(|arg| arg.as_ref().to_owned()));
        self
    }

    /// Sets a variable of the environment, one that the caller allows.
    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.env
            .push((key.as_ref().to_owned(), value.as_ref().to_owned()));
        self
    }

    /// Sets the working directory.
    pub fn current_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.current_dir = Some(dir.as_ref().to_owned());
        self
    }

    /// Gives the command a pipe on its stdin, which [`Process::stdin`]
    /// writes to. Without it, stdin is `/dev/null`.
    pub fn stdin_piped(&mut self) -> &mut Self {
        self.stdin = true;
        self
    }

    /// Passes stdout on as the bytes it holds, in [`crate::Line::Bytes`],
    /// and not as lines: for output that is binary or NUL-delimited. The cap
    /// of a line does not apply to it.
    pub fn raw_stdout(&mut self) -> &mut Self {
        self.raw_stdout = true;
        self
    }

    /// Sets the cap of a line of stdout, in bytes.
    pub fn max_line_length(&mut self, max: usize) -> &mut Self {
        self.max_line_length = max;
        self
    }

    /// Starts the command through the supervisor shim (AG-16).
    pub fn supervised_by(&mut self, supervisor: Supervisor) -> &mut Self {
        self.supervisor = Some(supervisor);
        self
    }

    /// Gives the program the open file `file`, at the number of its
    /// descriptor, which this returns: through the shim too, where there is
    /// one. Each process started from the command gets a description of the
    /// file of its own, opened again for reading, so that each reads it from
    /// its start; `file` itself stays closed on exec, and no other program
    /// started meanwhile inherits it.
    #[cfg(target_os = "linux")]
    pub fn pass_file(&mut self, file: std::sync::Arc<std::os::fd::OwnedFd>) -> std::os::fd::RawFd {
        use std::os::fd::AsRawFd as _;

        let fd = file.as_raw_fd();
        self.files.push(file);
        fd
    }

    /// Starts the command. Its stderr is written to `stderr` while stdout is
    /// read (AG-13).
    ///
    /// # Errors
    ///
    /// [`Error::Spawn`] where the program does not start, or the shim
    /// reports no command, and [`Error::Pipe`] where the shim's pipes cannot
    /// be made.
    pub async fn spawn(&self, stderr: impl AsyncWrite + Send + 'static) -> Result<Process, Error> {
        let (program, args, pipes) = match &self.supervisor {
            None => (self.program.clone(), self.args.clone(), None),
            Some(supervisor) => {
                let pipes = Pipes::new()?;
                let mut args = supervisor.args.clone();
                args.extend(pipes.arguments());
                args.push(self.program.clone());
                args.extend(self.args.iter().cloned());
                (
                    supervisor.program.clone().into_os_string(),
                    args,
                    Some(pipes),
                )
            }
        };
        #[expect(
            clippy::disallowed_methods,
            reason = "factory-proc is the crate that starts processes (SPEC 19.3)"
        )]
        let mut command = tokio::process::Command::new(&program);
        command
            .args(&args)
            .env_clear()
            .envs(self.env.iter().map(|(key, value)| (key, value)))
            .stdin(if self.stdin {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = &self.current_dir {
            command.current_dir(dir);
        }
        if let Some(pipes) = &pipes {
            pipes.inherit(&mut command);
        }
        #[cfg(target_os = "linux")]
        pass_files(&mut command, &self.files);
        let mut wrap = CommandWrap::from(command);
        wrap.wrap(ProcessGroup::leader()).wrap(KillOnDrop);
        let spawn_error = |source| Error::Spawn {
            program: program.to_string_lossy().into_owned(),
            source,
        };
        let child = wrap.spawn().map_err(spawn_error)?;
        let leader = child
            .id()
            .and_then(|id| i32::try_from(id).ok())
            .map(Pid::from_raw)
            .ok_or_else(|| spawn_error(std::io::Error::other("the leader has no pid")))?;
        let (group, held) = match pipes {
            None => (leader, None),
            Some(pipes) => {
                // The shim holds its ends now, and this process no copy of
                // them.
                let ends = pipes.started();
                let group = reported_group(ends.report).await.map_err(spawn_error)?;
                (group, Some(ends.held))
            }
        };
        Ok(Process::new(
            child,
            leader,
            group,
            if self.raw_stdout {
                LineCodec::Raw
            } else {
                LineCodec::new(self.max_line_length)
            },
            Box::pin(stderr),
            held,
        ))
    }
}

/// Opens each of `files` again in the child, between its fork and its exec,
/// and puts the new description at the number of the old one, where the
/// exec does not close it.
#[cfg(target_os = "linux")]
fn pass_files(
    command: &mut tokio::process::Command,
    files: &[std::sync::Arc<std::os::fd::OwnedFd>],
) {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd as _, IntoRawFd as _};

    use nix::fcntl::{OFlag, open};
    use nix::sys::stat::Mode;

    if files.is_empty() {
        return;
    }
    // Made before the fork: the child allocates nothing.
    let paths: Vec<(CString, std::os::fd::RawFd)> = files
        .iter()
        .map(|file| {
            let fd = file.as_raw_fd();
            let path =
                CString::new(format!("/proc/self/fd/{fd}")).unwrap_or_else(|_| CString::default());
            (path, fd)
        })
        .collect();
    let reopen = move || {
        for (path, fd) in &paths {
            // Closed when it is dropped below, before the exec.
            let fresh = open(path.as_c_str(), OFlag::O_RDONLY, Mode::empty())?;
            // SAFETY: `fd` is a descriptor of the parent's that the child
            // inherited and that nothing in the child owns; `dup2` replaces
            // it, and the exec that follows is what uses it.
            let placed = unsafe { nix::unistd::dup2_raw(&fresh, *fd) }?;
            let _ = placed.into_raw_fd();
        }
        Ok(())
    };
    // SAFETY: the closure runs in the child before its exec, where it makes
    // calls of open(2), dup2(2) and close(2), each async-signal-safe, and
    // allocates nothing: the paths were made before the fork.
    unsafe {
        command.pre_exec(reopen);
    }
}

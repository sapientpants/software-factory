//! A command to start: its program, arguments, environment and working
//! directory (SPEC AG-11).

use std::ffi::{OsStr, OsString};
use std::io::{PipeReader, PipeWriter};
use std::os::fd::{AsRawFd as _, BorrowedFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};

use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::unistd::{Pid, getpid};
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop, ProcessGroup};
use tokio::io::{AsyncReadExt as _, AsyncWrite};
use tokio::net::unix::pipe::Receiver;

use crate::Error;
use crate::codec::LineCodec;
use crate::process::{Process, Signalled, Started};
use crate::supervise::{Pipes, reported_group};

/// The cap of a line of stdout where the caller sets none, in bytes
/// *(tunable)*.
pub const MAX_LINE_LENGTH: usize = 1 << 20;

/// The longest report of [`Command::signal_reported`], in bytes, its line
/// break among them *(tunable)*. bubblewrap's is about 200.
const MAX_REPORT_LENGTH: usize = 4096;

/// A command that [`Command::spawn`] starts as the leader of a process group
/// of its own, with a cleared environment that holds the variables set with
/// [`Command::env`] and no other (AG-11).
///
/// A command is started once where it holds a descriptor of its own, which
/// [`Command::signal_reported`] gives it; otherwise it may be started again.
#[derive(Debug)]
pub struct Command {
    program: OsString,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    current_dir: Option<PathBuf>,
    stdin: bool,
    inherit_stdio: bool,
    raw_stdout: bool,
    max_line_length: usize,
    supervisor: Option<Supervisor>,
    #[cfg(target_os = "linux")]
    files: Vec<std::sync::Arc<std::os::fd::OwnedFd>>,
    report: Report,
}

/// Whether the program reports the process that the stop sequence signals.
#[derive(Debug)]
enum Report {
    /// It does not: the stop sequence signals the command's group.
    None,
    /// On the writing end of this pipe, as a line that `parse` reads.
    Pending {
        reader: PipeReader,
        writer: PipeWriter,
        parse: fn(&[u8]) -> Option<u32>,
    },
    /// The command was started, and its pipe went with it.
    Spent,
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
            inherit_stdio: false,
            raw_stdout: false,
            max_line_length: MAX_LINE_LENGTH,
            supervisor: None,
            #[cfg(target_os = "linux")]
            files: Vec::new(),
            report: Report::None,
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

    /// Starts the command on the stdin, stdout and stderr of this process,
    /// in place of pipes: [`Process::next_line`] then reads no line, the sink
    /// of [`Command::spawn`] is given nothing, and [`Command::stdin_piped`]
    /// has no effect. For the first process of a sandbox, which reads and
    /// writes none of the three (SEC-10a).
    pub fn inherit_stdio(&mut self) -> &mut Self {
        self.inherit_stdio = true;
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

    /// Has the stop sequence signal one process in place of the command's
    /// group (AG-12): the one that the program names on the descriptor whose
    /// number this returns, which the caller puts in the program's arguments.
    /// The program gets the writing end of a pipe there, through the shim
    /// too, which hands it on unread. Once the program has started,
    /// [`Command::spawn`] reads the first line of the pipe, and `parse` gives
    /// the pid from it: `factory-proc` knows nothing of what the line holds.
    /// The pipe stays open while the [`Process`] lives, so that a program
    /// that writes more there is not ended by SIGPIPE. For the first process
    /// of a sandbox at level `process`, which passes SIGINT and SIGTERM on to
    /// the command's group (SEC-10a).
    ///
    /// # Errors
    ///
    /// Where the pipe cannot be made.
    pub fn signal_reported(&mut self, parse: fn(&[u8]) -> Option<u32>) -> std::io::Result<RawFd> {
        let (reader, writer) = std::io::pipe()?;
        let fd = writer.as_raw_fd();
        self.report = Report::Pending {
            reader,
            writer,
            parse,
        };
        Ok(fd)
    }

    /// Starts the command. Its stderr is written to `stderr` while stdout is
    /// read (AG-13).
    ///
    /// # Errors
    ///
    /// [`Error::Spawn`] where the program does not start, the shim reports
    /// no command, the program reports no process that may be signalled
    /// ([`Command::signal_reported`]), or the command, which holds a
    /// descriptor of its own, was started already; and [`Error::Pipe`] where
    /// the shim's pipes cannot be made.
    pub async fn spawn(
        &mut self,
        stderr: impl AsyncWrite + Send + 'static,
    ) -> Result<Process, Error> {
        let spawn_error = |program: &OsStr, source| Error::Spawn {
            program: program.to_string_lossy().into_owned(),
            source,
        };
        let report = match std::mem::replace(&mut self.report, Report::Spent) {
            Report::None => {
                self.report = Report::None;
                None
            }
            Report::Pending {
                reader,
                writer,
                parse,
            } => Some((reader, writer, parse)),
            Report::Spent => {
                return Err(spawn_error(
                    &self.program,
                    std::io::Error::other("the command was started already"),
                ));
            }
        };
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
            .envs(self.env.iter().map(|(key, value)| (key, value)));
        if self.inherit_stdio {
            command
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
        } else {
            command
                .stdin(if self.stdin {
                    Stdio::piped()
                } else {
                    Stdio::null()
                })
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
        }
        if let Some(dir) = &self.current_dir {
            command.current_dir(dir);
        }
        if let Some(pipes) = &pipes {
            pipes.inherit(&mut command);
        }
        #[cfg(target_os = "linux")]
        pass_files(&mut command, &self.files);
        if let Some((_, writer, _)) = &report {
            inherit(&mut command, writer.as_raw_fd());
        }
        let mut wrap = CommandWrap::from(command);
        wrap.wrap(ProcessGroup::leader()).wrap(KillOnDrop);
        let spawn_error = |source| spawn_error(&program, source);
        let mut child = wrap.spawn().map_err(spawn_error)?;
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
        let (signalled, status, report) = match report {
            None => (Signalled::Group(group.as_raw().unsigned_abs()), None, None),
            Some((reader, writer, parse)) => {
                // The program holds its copy now, and the pipe ends once
                // each holder of one has closed it.
                drop(writer);
                reported(child.as_mut(), group, reader, parse)
                    .await
                    .map_err(spawn_error)?
            }
        };
        Ok(Process::new(
            Started {
                child,
                leader,
                group,
                signalled,
                status,
                report,
            },
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

/// Makes the descriptor `fd` inheritable in the child alone, between its
/// fork and its exec.
fn inherit(command: &mut tokio::process::Command, fd: RawFd) {
    let inherit = move || {
        // SAFETY: the descriptor is the writing end of a pipe that the
        // command holds open until the child has started.
        let end = unsafe { BorrowedFd::borrow_raw(fd) };
        fcntl(end, FcntlArg::F_SETFD(FdFlag::empty()))?;
        Ok(())
    };
    // SAFETY: the closure runs in the child before its exec, where it makes
    // one call of fcntl(2), which is async-signal-safe, and allocates nothing.
    unsafe {
        command.pre_exec(inherit);
    }
}

/// What the stop sequence of a started program signals, which it reports on
/// `reader` (`Command::signal_reported`), with the leader's status where the
/// leader exited first, and the pipe, which the process keeps open. A
/// program that ends without a report has ended by itself, and its group is
/// what is left to signal.
async fn reported(
    child: &mut dyn ChildWrapper,
    group: Pid,
    reader: PipeReader,
    parse: fn(&[u8]) -> Option<u32>,
) -> std::io::Result<(Signalled, Option<ExitStatus>, Option<Receiver>)> {
    let mut receiver = Receiver::from_owned_fd(OwnedFd::from(reader))?;
    let own = getpid().as_raw().unsigned_abs();
    let ended = |status| {
        Ok((
            Signalled::Group(group.as_raw().unsigned_abs()),
            Some(status),
            None,
        ))
    };
    // A wait closes the child's stdin as soon as it is polled, even where
    // the report comes first, so the pipe is held aside meanwhile.
    let stdin = child.stdin().take();
    let outcome = tokio::select! {
        biased;
        line = first_line(&mut receiver) => match line? {
            Some(line) => match parse(&line) {
                // Never this process, nor init or a pid that is none, so
                // that no single fault signals the whole host.
                Some(pid) if pid > 1 && pid != own && i32::try_from(pid).is_ok() => {
                    Ok((Signalled::Process(pid), None, Some(receiver)))
                }
                _ => Err(std::io::Error::other(format!(
                    "the program reported no process to signal: {:?}",
                    String::from_utf8_lossy(&line)
                ))),
            },
            None => ended(child.wait().await?),
        },
        status = child.wait() => ended(status?),
    };
    *child.stdin() = stdin;
    outcome
}

/// The first line of `receiver`, without its line break, or `None` where it
/// ends before one. It reads at most [`MAX_REPORT_LENGTH`] bytes.
async fn first_line(receiver: &mut Receiver) -> std::io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    let mut buffer = [0; 256];
    loop {
        if let Some(end) = line.iter().position(|byte| *byte == b'\n') {
            line.truncate(end);
            return Ok(Some(line));
        }
        if line.len() >= MAX_REPORT_LENGTH {
            return Err(std::io::Error::other(format!(
                "the program's report is longer than {MAX_REPORT_LENGTH} bytes"
            )));
        }
        match receiver.read(&mut buffer).await? {
            0 => return Ok(None),
            n => line.extend_from_slice(buffer.get(..n).unwrap_or_default()),
        }
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

//! The supervisor shim of AG-16, which `sf` runs as `sf __supervise`.
//!
//! [`crate::Command::spawn`] starts the shim, through a [`crate::Supervisor`],
//! with two pipes: the reading end of one whose writing end only the caller
//! holds, and the writing end of one that the caller reads. The shim starts
//! the command as the leader of a process group of its own (AG-11), with the
//! shim's environment and stdio, reports the command's pid on the second
//! pipe and waits for the command. The caller signals that group, which the
//! shim is no member of: the stop sequence (AG-12) reaches the command and
//! not the shim, which exits after the command with its status. When the
//! first pipe closes, as it does when the caller dies, the shim kills the
//! group.
//!
//! The protocol is `--pipe-fd <fd> --report-fd <fd> -- <program> [<arg>...]`
//! after the supervisor's own arguments.

use std::ffi::OsString;
use std::io::{PipeReader, PipeWriter, Write as _};
use std::os::fd::{AsRawFd as _, BorrowedFd, FromRawFd as _, OwnedFd, RawFd};
use std::os::unix::process::ExitStatusExt as _;
use std::process::{ExitCode, ExitStatus};

use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use process_wrap::tokio::{CommandWrap, ProcessGroup};
use tokio::io::AsyncReadExt as _;
use tokio::net::unix::pipe::Receiver;

use crate::Error;

/// The exit code of a shim that could not start the command.
pub const FAILED: u8 = 125;

/// The longest report: a pid and its line break.
const REPORT: u64 = 16;

/// The two pipes between a caller and its shim.
pub(crate) struct Pipes {
    /// The pipe that the shim watches: it reads this end.
    watched: PipeReader,
    /// The caller's end of it, which closes when the caller dies.
    held: PipeWriter,
    /// The pipe that the shim reports the command's pid on: the caller reads
    /// this end.
    report: PipeReader,
    /// The shim's end of it.
    reported: PipeWriter,
}

/// The caller's ends of the pipes, once the shim has started.
pub(crate) struct Ends {
    pub(crate) held: PipeWriter,
    pub(crate) report: PipeReader,
}

impl Pipes {
    /// Two pipes, each of whose ends closes on exec. The shim inherits its
    /// two through [`Pipes::inherit`], and no other program does: one that
    /// another thread starts meanwhile would keep the watched pipe open past
    /// the caller's death, or delay the end of a report.
    pub(crate) fn new() -> Result<Self, Error> {
        let (watched, held) = std::io::pipe().map_err(Error::Pipe)?;
        let (report, reported) = std::io::pipe().map_err(Error::Pipe)?;
        Ok(Self {
            watched,
            held,
            report,
            reported,
        })
    }

    /// Makes the shim's two ends inheritable in the child alone, between its
    /// fork and its exec.
    pub(crate) fn inherit(&self, command: &mut tokio::process::Command) {
        let ends = [self.watched.as_raw_fd(), self.reported.as_raw_fd()];
        let inherit = move || {
            for end in ends {
                // SAFETY: the descriptor is an end of a pipe that `self`
                // holds open until the child has started.
                let end = unsafe { BorrowedFd::borrow_raw(end) };
                fcntl(end, FcntlArg::F_SETFD(FdFlag::empty()))?;
            }
            Ok(())
        };
        // SAFETY: the closure runs in the child before its exec, where it
        // makes one call of fcntl(2) for each end, which is async-signal-safe,
        // and allocates nothing.
        unsafe {
            command.pre_exec(inherit);
        }
    }

    /// The arguments of the protocol before the program.
    pub(crate) fn arguments(&self) -> [OsString; 5] {
        [
            "--pipe-fd".into(),
            self.watched.as_raw_fd().to_string().into(),
            "--report-fd".into(),
            self.reported.as_raw_fd().to_string().into(),
            "--".into(),
        ]
    }

    /// The caller's ends, once the shim has started; the shim's are closed
    /// here.
    pub(crate) fn started(self) -> Ends {
        Ends {
            held: self.held,
            report: self.report,
        }
    }
}

/// The group that the shim reports on `report`: the pid of the command it
/// started. The shim alone holds the other end, so the report ends when the
/// shim closes it, after the pid or at its exit without one.
pub(crate) async fn reported_group(report: PipeReader) -> std::io::Result<Pid> {
    let report = Receiver::from_owned_fd(OwnedFd::from(report))?;
    let mut line = Vec::new();
    report.take(REPORT).read_to_end(&mut line).await?;
    std::str::from_utf8(&line)
        .ok()
        .and_then(|line| line.strip_suffix('\n'))
        .and_then(|pid| pid.parse::<i32>().ok())
        .filter(|pid| *pid > 1)
        .map(Pid::from_raw)
        .ok_or_else(|| std::io::Error::other("the supervisor shim reported no pid"))
}

/// A shim's arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Supervise {
    pipe_fd: RawFd,
    report_fd: RawFd,
    program: OsString,
    args: Vec<OsString>,
}

impl Supervise {
    /// Reads the arguments of the protocol.
    ///
    /// # Errors
    ///
    /// A message where they are not `--pipe-fd <fd> --report-fd <fd> --
    /// <program> [<arg>...]` with two distinct descriptors above those of
    /// stdio.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        const USAGE: &str = "expected --pipe-fd <fd> --report-fd <fd> -- <program> [<arg>...]";
        let mut args = args.into_iter();
        let mut descriptor = |flag: &str| {
            if args.next().as_deref() != Some(flag.as_ref()) {
                return Err(USAGE.to_owned());
            }
            args.next()
                .and_then(|fd| fd.to_str()?.parse::<RawFd>().ok())
                .filter(|fd| *fd > 2)
                .ok_or(format!("{flag} takes a descriptor above 2"))
        };
        let pipe_fd = descriptor("--pipe-fd")?;
        let report_fd = descriptor("--report-fd")?;
        if pipe_fd == report_fd {
            return Err("--pipe-fd and --report-fd name one descriptor".to_owned());
        }
        if args.next().as_deref() != Some("--".as_ref()) {
            return Err(USAGE.to_owned());
        }
        let program = args.next().ok_or(USAGE)?;
        Ok(Self {
            pipe_fd,
            report_fd,
            program,
            args: args.collect(),
        })
    }

    /// Runs the shim. It returns the command's exit code, or 128 and the
    /// number of the signal that ended the command, and [`FAILED`] where the
    /// command did not start.
    pub fn run(self) -> ExitCode {
        match self.supervise() {
            Ok(status) => exit_code(status),
            Err(message) => {
                eprintln!("sf __supervise: {message}");
                ExitCode::from(FAILED)
            }
        }
    }

    fn supervise(self) -> Result<ExitStatus, String> {
        // SAFETY: each descriptor is an end of a pipe that the caller passed
        // on, which nothing of this process has opened or owns: nothing has
        // run yet but the parsing of the arguments, and the two are distinct.
        let (watched, reported) = unsafe {
            (
                OwnedFd::from_raw_fd(self.pipe_fd),
                OwnedFd::from_raw_fd(self.report_fd),
            )
        };
        // The command inherits neither.
        for (fd, end) in [(self.pipe_fd, &watched), (self.report_fd, &reported)] {
            fcntl(end, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))
                .map_err(|errno| format!("descriptor {fd}: {errno}"))?;
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("cannot start the runtime: {error}"))?;
        runtime.block_on(self.wait(watched, PipeWriter::from(reported)))
    }

    async fn wait(self, watched: OwnedFd, mut reported: PipeWriter) -> Result<ExitStatus, String> {
        let mut watched = Receiver::from_owned_fd(watched)
            .map_err(|error| format!("descriptor {}: {error}", self.pipe_fd))?;
        #[expect(
            clippy::disallowed_methods,
            reason = "factory-proc is the crate that starts processes (SPEC 19.3)"
        )]
        let mut command = tokio::process::Command::new(&self.program);
        command.args(&self.args);
        let mut wrap = CommandWrap::from(command);
        wrap.wrap(ProcessGroup::leader());
        let mut child = wrap
            .spawn()
            .map_err(|error| format!("cannot start {}: {error}", self.program.to_string_lossy()))?;
        let group = child
            .id()
            .and_then(|id| i32::try_from(id).ok())
            .map(Pid::from_raw)
            .ok_or("the command has no pid")?;
        // A caller that is gone has closed the watched pipe too, which ends
        // the command below.
        let _ = writeln!(reported, "{group}");
        drop(reported);
        let mut buffer = [0; 64];
        loop {
            tokio::select! {
                status = child.wait() => {
                    return status.map_err(|error| format!("cannot wait for the command: {error}"));
                }
                read = watched.read(&mut buffer) => {
                    if matches!(read, Ok(0) | Err(_)) {
                        // The caller is gone: the tree goes with it.
                        let _ = killpg(group, Signal::SIGKILL);
                        return child
                            .wait()
                            .await
                            .map_err(|error| format!("cannot wait for the command: {error}"));
                    }
                }
            }
        }
    }
}

/// The exit code that passes `status` on: its code, or 128 and the number of
/// the signal that ended the process (AG-16, SEC-10a).
pub fn exit_code(status: ExitStatus) -> ExitCode {
    let code = status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(i32::from(FAILED));
    ExitCode::from(u8::try_from(code & 0xff).unwrap_or(FAILED))
}

//! A started command: its output (AG-13) and its stop sequence (AG-12).

use std::io::PipeWriter;
use std::pin::Pin;
use std::process::ExitStatus;
use std::time::Duration;

use nix::errno::Errno;
use nix::sys::signal::{Signal, killpg};
use nix::unistd::{Pid, getpgrp};
use process_wrap::tokio::ChildWrapper;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::process::{ChildStderr, ChildStdin, ChildStdout};
use tokio::time::{Instant, sleep_until, timeout_at};
use tokio_util::bytes::BytesMut;
use tokio_util::codec::Decoder as _;

use crate::Error;
use crate::codec::{Line, LineCodec};

/// How long output is drained after the leader exits, before the pipes are
/// closed (AG-13).
pub const DRAIN: Duration = Duration::from_secs(2);

/// The waits of the stop sequence (AG-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StopTimes {
    /// After the interrupt, for the result message or the leader's exit.
    pub interrupt: Duration,
    /// After SIGTERM, for the leader's exit, before SIGKILL.
    pub terminate: Duration,
}

impl StopTimes {
    /// The times of AG-12: 10 s, then 5 s.
    pub const AG_12: Self = Self {
        interrupt: Duration::from_secs(10),
        terminate: Duration::from_secs(5),
    };
}

impl Default for StopTimes {
    fn default() -> Self {
        Self::AG_12
    }
}

/// The step of the stop sequence after which the leader had exited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoppedBy {
    /// It had exited before the sequence started.
    Itself,
    /// SIGINT.
    Interrupt,
    /// SIGTERM, after the interrupt and the close of stdin.
    Terminate,
    /// SIGKILL.
    Kill,
}

/// What a stop sequence ended with.
#[derive(Debug)]
pub struct Stopped {
    /// The step after which the leader had exited.
    pub by: StoppedBy,
    /// The leader's exit status.
    pub status: ExitStatus,
    /// The lines of stdout read during the sequence and the drain after it.
    pub lines: Vec<Line>,
}

/// What the pump of the output saw.
enum Event {
    Line(Line),
    Exited,
}

/// A command started by [`crate::Command::spawn`], the leader of a process
/// group of its own. The leader of this type is the process that was
/// started: the command, or the shim that started it (AG-16), whose exit the
/// output ends after; the group is the command's.
///
/// Stdout is read through [`Process::next_line`], and stderr is written to
/// its sink while it is. After the leader exits, output is drained for at
/// most [`DRAIN`], so a process that holds a pipe does not hang the reader;
/// then the pipes are closed and what is left of the group is killed. A
/// `Process` that is dropped first kills the group, and closes the shim's
/// pipe, so the shim kills it too.
pub struct Process {
    /// The process that was started, which the output ends after: the
    /// command, or the shim that started it.
    child: Box<dyn ChildWrapper>,
    leader: Pid,
    /// The command's group.
    group: Pid,
    stdin: Option<ChildStdin>,
    stdout: Option<Lines>,
    stderr: Option<ChildStderr>,
    sink: Pin<Box<dyn AsyncWrite + Send>>,
    buffer: Box<[u8]>,
    /// The bytes read from stderr and not yet written to the sink.
    pending: Vec<u8>,
    status: Option<ExitStatus>,
    drain_until: Option<Instant>,
    /// Whether the group was killed after the output ended.
    swept: bool,
    /// The writing end of the shim's pipe, where the command was started
    /// through it (AG-16). Closing it kills the tree.
    supervisor: Option<PipeWriter>,
}

impl Process {
    pub(crate) fn new(
        mut child: Box<dyn ChildWrapper>,
        leader: Pid,
        group: Pid,
        max_line_length: usize,
        sink: Pin<Box<dyn AsyncWrite + Send>>,
        supervisor: Option<PipeWriter>,
    ) -> Self {
        Self {
            leader,
            group,
            stdin: child.stdin().take(),
            stdout: child
                .stdout()
                .take()
                .map(|stdout| Lines::new(stdout, max_line_length)),
            stderr: child.stderr().take(),
            child,
            sink,
            buffer: vec![0; 8192].into_boxed_slice(),
            pending: Vec::new(),
            status: None,
            drain_until: None,
            swept: false,
            supervisor,
        }
    }

    /// The pid of the process that was started: the command, or the shim
    /// that started it.
    pub fn id(&self) -> u32 {
        self.leader.as_raw().unsigned_abs()
    }

    /// The process group that the stop sequence signals: the command's,
    /// which it leads (AG-11). Without a shim it is [`Process::id`].
    pub fn group(&self) -> u32 {
        self.group.as_raw().unsigned_abs()
    }

    /// The pipe on stdin, where the command was given one and it is open.
    pub fn stdin(&mut self) -> Option<&mut ChildStdin> {
        self.stdin.as_mut()
    }

    /// Closes stdin.
    pub fn close_stdin(&mut self) {
        self.stdin = None;
    }

    /// The leader's exit status, once it has exited.
    pub fn status(&self) -> Option<ExitStatus> {
        self.status
    }

    /// The next line of stdout, or `None` once the output has ended: the
    /// leader has exited and both pipes are at their end or were closed after
    /// [`DRAIN`].
    ///
    /// # Errors
    ///
    /// Where a pipe cannot be read, stderr cannot be written to its sink, or
    /// the leader cannot be waited for.
    pub async fn next_line(&mut self) -> Result<Option<Line>, Error> {
        loop {
            match self.pump().await? {
                Some(Event::Line(line)) => return Ok(Some(line)),
                Some(Event::Exited) => {}
                None => return Ok(None),
            }
        }
    }

    /// Reads the output to its end, discarding the lines of stdout, and
    /// returns the leader's exit status.
    ///
    /// # Errors
    ///
    /// As [`Process::next_line`].
    pub async fn wait(&mut self) -> Result<ExitStatus, Error> {
        while self.next_line().await?.is_some() {}
        self.status
            .ok_or_else(|| Error::Wait(std::io::Error::other("the output ended before the leader")))
    }

    /// Stops the command by the sequence of AG-12: SIGINT to the group, and a
    /// wait of `times.interrupt` for the leader to exit or for a line that
    /// `result` accepts; then stdin is closed, SIGTERM goes to the group, and
    /// after `times.terminate` SIGKILL. The output is read throughout and
    /// drained after the leader exits.
    ///
    /// # Errors
    ///
    /// As [`Process::next_line`], and where a signal cannot be sent.
    pub async fn stop(
        &mut self,
        times: StopTimes,
        mut result: impl FnMut(&Line) -> bool,
    ) -> Result<Stopped, Error> {
        let mut lines = Vec::new();
        let mut by = StoppedBy::Itself;
        if self.status.is_none() {
            by = StoppedBy::Interrupt;
            self.signal(Signal::SIGINT)?;
            let deadline = Instant::now() + times.interrupt;
            if !self.until(Some(deadline), &mut lines, &mut result).await? {
                by = StoppedBy::Terminate;
                self.close_stdin();
                self.signal(Signal::SIGTERM)?;
                let deadline = Instant::now() + times.terminate;
                if !self
                    .until(Some(deadline), &mut lines, &mut |_| false)
                    .await?
                {
                    by = StoppedBy::Kill;
                    self.signal(Signal::SIGKILL)?;
                    // The shim, which is no member of the group, then
                    // exits after the command; the close would make it kill
                    // the group too (AG-16).
                    self.supervisor = None;
                    self.until(None, &mut lines, &mut |_| false).await?;
                }
            }
        }
        while let Some(line) = self.next_line().await? {
            lines.push(line);
        }
        let status = self.wait().await?;
        Ok(Stopped { by, status, lines })
    }

    /// Reads the output until the leader exits (`true`), or until a line that
    /// `result` accepts or the deadline (`false`).
    async fn until(
        &mut self,
        deadline: Option<Instant>,
        lines: &mut Vec<Line>,
        result: &mut impl FnMut(&Line) -> bool,
    ) -> Result<bool, Error> {
        loop {
            let event = match deadline {
                Some(deadline) => match timeout_at(deadline, self.pump()).await {
                    Ok(event) => event?,
                    Err(_) => return Ok(false),
                },
                None => self.pump().await?,
            };
            match event {
                Some(Event::Line(line)) => {
                    let accepted = result(&line);
                    lines.push(line);
                    if accepted {
                        return Ok(false);
                    }
                }
                Some(Event::Exited) | None => return Ok(true),
            }
        }
    }

    /// Sends `signal` to the group. A group with no process left is no
    /// error.
    fn signal(&self, signal: Signal) -> Result<(), Error> {
        if !signalable(self.group, getpgrp()) {
            return Err(Error::Signal(std::io::Error::other(format!(
                "{} is no group that a command leads",
                self.group
            ))));
        }
        match killpg(self.group, signal) {
            Ok(()) | Err(Errno::ESRCH) => Ok(()),
            // macOS answers EPERM where each process left of the group is a
            // zombie that its parent has not reaped yet.
            #[cfg(target_os = "macos")]
            Err(Errno::EPERM) => Ok(()),
            Err(errno) => Err(Error::Signal(errno.into())),
        }
    }

    /// Reads the output until a line of stdout or the leader's exit, or
    /// `None` once the output has ended. Each branch is cancel-safe, so a
    /// caller may drop the future at a deadline.
    async fn pump(&mut self) -> Result<Option<Event>, Error> {
        loop {
            if self.status.is_some() && self.stdout.is_none() && self.stderr.is_none() {
                self.end().await?;
                return Ok(None);
            }
            let Self {
                child,
                stdout,
                stderr,
                sink,
                buffer,
                pending,
                status,
                drain_until,
                ..
            } = self;
            let drain = *drain_until;
            tokio::select! {
                line = next_line(stdout), if stdout.is_some() => match line {
                    Some(Ok(line)) => return Ok(Some(Event::Line(line))),
                    Some(Err(error)) => return Err(Error::Read(error)),
                    None => *stdout = None,
                },
                read = read(stderr, buffer), if stderr.is_some() && pending.is_empty() => {
                    match read.map_err(Error::Read)? {
                        0 => *stderr = None,
                        n => pending.extend_from_slice(buffer.get(..n).unwrap_or_default()),
                    }
                }
                written = sink.write(pending),
                    if !pending.is_empty() =>
                {
                    match written.map_err(Error::Sink)? {
                        0 => return Err(Error::Sink(std::io::ErrorKind::WriteZero.into())),
                        n => drop(pending.drain(..n.min(pending.len()))),
                    }
                }
                exited = child.wait(), if status.is_none() => {
                    *status = Some(exited.map_err(Error::Wait)?);
                    *drain_until = Some(Instant::now() + DRAIN);
                    return Ok(Some(Event::Exited));
                }
                () = sleep_until(drain.unwrap_or_else(Instant::now)), if drain.is_some() => {
                    // A process that holds a pipe does not hang the reader
                    // (AG-13).
                    *stdout = None;
                    *stderr = None;
                    pending.clear();
                }
            }
        }
    }

    /// Ends the output: flushes the sink once and kills what is left of the
    /// group.
    async fn end(&mut self) -> Result<(), Error> {
        if self.swept {
            return Ok(());
        }
        self.swept = true;
        self.signal(Signal::SIGKILL)?;
        self.sink.flush().await.map_err(Error::Sink)
    }
}

/// Whether `group` may be signalled by a process of the group `own`. Group 0
/// is the caller's own, group 1 is every process that the caller may signal,
/// and a negative one is no group; the caller's own group is refused too.
/// This is the second guard behind the report of a shim, which takes no pid
/// below 2, so that no single fault signals the whole host.
fn signalable(group: Pid, own: Pid) -> bool {
    group.as_raw() > 1 && group != own
}

impl Drop for Process {
    fn drop(&mut self) {
        if !self.swept {
            let _ = self.signal(Signal::SIGKILL);
        }
    }
}

/// Stdout, decoded by the codec as it is read. Unlike `FramedRead`, it
/// needs no `Stream` trait: [`Lines::next`] is one cancel-safe read at a time.
struct Lines {
    reader: ChildStdout,
    codec: LineCodec,
    buffer: BytesMut,
    eof: bool,
}

impl Lines {
    fn new(reader: ChildStdout, max_line_length: usize) -> Self {
        Self {
            reader,
            codec: LineCodec::new(max_line_length),
            buffer: BytesMut::new(),
            eof: false,
        }
    }

    /// The next line, or `None` at the end of the pipe.
    async fn next(&mut self) -> Option<std::io::Result<Line>> {
        loop {
            if self.eof {
                return self.codec.decode_eof(&mut self.buffer).transpose();
            }
            match self.codec.decode(&mut self.buffer) {
                Ok(Some(line)) => return Some(Ok(line)),
                Ok(None) => {}
                Err(error) => return Some(Err(error)),
            }
            self.buffer.reserve(8192);
            match self.reader.read_buf(&mut self.buffer).await {
                Ok(0) => self.eof = true,
                Ok(_) => {}
                Err(error) => return Some(Err(error)),
            }
        }
    }
}

async fn next_line(stdout: &mut Option<Lines>) -> Option<std::io::Result<Line>> {
    stdout.as_mut()?.next().await
}

async fn read(
    stderr: &mut Option<impl AsyncRead + Unpin>,
    buffer: &mut [u8],
) -> std::io::Result<usize> {
    match stderr {
        Some(stderr) => stderr.read(buffer).await,
        None => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use nix::unistd::Pid;

    use super::signalable;

    #[test]
    fn a_group_that_is_none_of_a_command_is_not_signalled() {
        let own = Pid::from_raw(500);
        for (group, expected) in [
            (-7, false),
            (0, false),
            (1, false),
            (500, false),
            (2, true),
            (501, true),
        ] {
            assert_eq!(signalable(Pid::from_raw(group), own), expected, "{group}");
        }
    }
}

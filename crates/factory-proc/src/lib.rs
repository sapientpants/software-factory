//! Process groups, supervisor shim, stop sequence, line codec and timeouts (SPEC 19.1).
//!
//! [`Command::spawn`] starts a command as the leader of a process group of
//! its own, with a cleared environment (AG-11). The [`Process`] it returns
//! reads stdout as capped lines and drains stderr beside it (AG-13), stops
//! the group by the sequence of AG-12, and leaves no process of the group
//! behind. [`supervise`] is the shim of AG-16, which the binary `sf` runs as
//! `sf __supervise`.

mod codec;
mod command;
mod process;
pub mod supervise;

pub use codec::Line;
pub use command::{Command, MAX_LINE_LENGTH, Supervisor};
pub use process::{DRAIN, Process, StopTimes, Stopped, StoppedBy};

/// What starting, reading or stopping a process failed at.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The program did not start.
    #[error("cannot start {program}: {source}")]
    Spawn {
        /// The program.
        program: String,
        /// Why.
        source: std::io::Error,
    },
    /// The pipe to the supervisor shim could not be made.
    #[error("cannot make the pipe of the supervisor shim: {0}")]
    Pipe(std::io::Error),
    /// Reading stdout or stderr failed.
    #[error("cannot read the output: {0}")]
    Read(std::io::Error),
    /// Writing stderr to its sink failed.
    #[error("cannot write stderr: {0}")]
    Sink(std::io::Error),
    /// Waiting for the leader failed.
    #[error("cannot wait for the process: {0}")]
    Wait(std::io::Error),
    /// A signal could not be sent to the group.
    #[error("cannot signal the process group: {0}")]
    Signal(std::io::Error),
}

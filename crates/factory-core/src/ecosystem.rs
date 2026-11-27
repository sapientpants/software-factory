//! `Ecosystem`, the interface behind which everything specific to a
//! language's toolchain sits (SPEC 9.5), with `rust` as its one
//! implementation.
//!
//! Each function is pure: it is given bytes that another crate read and
//! returns data. Where the answer lies on the host's disk, as for the
//! toolchain, it returns what to look for, and `factory-daemon` looks (WS-7,
//! 19.1). What this build holds of the interface is the toolchain; the other
//! concerns of 9.5 join it in the change that builds each.

mod rust;

pub use rust::Rust;

use std::fmt;

/// The ecosystem of one repository, which its policy names (9.5).
pub trait Ecosystem {
    /// The path, from the root of a tree, of the file that names the
    /// toolchain. A tree without it names no toolchain (WS-7).
    fn toolchain_file(&self) -> &'static str;

    /// What the host looks for, below the toolchains directory, to find the
    /// toolchain that `file` names on a host of `host` (WS-7).
    ///
    /// # Errors
    ///
    /// What is wrong with the file where it names no toolchain that the host
    /// can look for. It counts as a missing toolchain (WS-7).
    fn toolchain(
        &self,
        file: &[u8],
        host: HostTriple,
    ) -> Result<ToolchainLookup, ToolchainFileError>;
}

/// The host triple of the architecture that `sf` was built for (WS-7, 4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostTriple {
    /// `x86_64-unknown-linux-gnu`.
    X86_64UnknownLinuxGnu,
    /// `aarch64-unknown-linux-gnu`.
    Aarch64UnknownLinuxGnu,
}

impl HostTriple {
    /// The triple as it is written.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X86_64UnknownLinuxGnu => "x86_64-unknown-linux-gnu",
            Self::Aarch64UnknownLinuxGnu => "aarch64-unknown-linux-gnu",
        }
    }
}

impl fmt::Display for HostTriple {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the host looks for to find one toolchain (WS-7). Every path is
/// relative and uses `/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainLookup {
    /// The toolchain as the file names it, for a report.
    pub name: String,
    /// The toolchain's directory, below the toolchains directory.
    pub directory: String,
    /// The directory, below the toolchain's, that goes first on the worker
    /// `PATH`.
    pub bin: String,
    /// The programs, below the toolchain's directory, each of which is a
    /// regular file that the factory's user may execute.
    pub programs: Vec<String>,
    /// The file, below the toolchain's directory, that records what the
    /// toolchain holds, one name to a line.
    pub record: String,
    /// What the record holds: each part is present when the record holds one
    /// of its lines.
    pub parts: Vec<Part>,
}

/// One thing that the record of a toolchain holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// What it is to the toolchain.
    pub kind: PartKind,
    /// Its name, as a report gives it.
    pub name: String,
    /// The lines of the record, any one of which makes it present.
    pub lines: Vec<String>,
}

/// What a part of a toolchain is, by which a missing one is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    /// A part of the toolchain itself: without it the toolchain is missing.
    Toolchain,
    /// A component that the file names.
    Component,
    /// A target that the file names.
    Target,
}

/// What is wrong with the file that names the toolchain (WS-7).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{file}` names no toolchain that the host can look for: {problem}")]
pub struct ToolchainFileError {
    /// The file, as [`Ecosystem::toolchain_file`] names it.
    pub file: &'static str,
    /// What is wrong with it.
    pub problem: String,
}

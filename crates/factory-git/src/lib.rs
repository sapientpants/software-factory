//! The Git trait, git CLI wrapper, mirrors, workspaces, capture, export and publishing (SPEC 19.1).
//!
//! [`Git`] is the port of every git operation of the host, and [`GitCli`]
//! its one implementation: a wrapper over the `git` CLI that starts each
//! command through `factory-proc` and parses only porcelain or NUL-delimited
//! output (WS-8). Every command names its repository and none finds one from
//! its working directory, so each works under `safe.bareRepository =
//! explicit` (WS-15).

mod cli;
mod names;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

pub use cli::GitCli;
pub use names::{Branch, Oid, Remote};

/// The git operations of the host (9.2, WS-8).
pub trait Git: Send + Sync {
    /// Fetches every branch of `remote` into the bare mirror at `mirror`,
    /// pruning the branches that `remote` no longer has, and creates the
    /// mirror first where it does not exist. Fetches into one mirror run one
    /// at a time (WS-8).
    fn fetch_mirror(
        &self,
        mirror: &Path,
        remote: &Remote,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// The commit that `branch` names in the bare repository `repository`.
    fn branch_head(
        &self,
        repository: &Path,
        branch: &Branch,
    ) -> impl Future<Output = Result<Oid, Error>> + Send;

    /// Creates `host_git`, which does not exist, as a bare clone of
    /// `default` alone from `mirror`, by git's transport (`--no-local`),
    /// and removes its remote (WS-10). It then holds only the objects that
    /// `default` reaches.
    fn create_host_repository(
        &self,
        mirror: &Path,
        default: &Branch,
        host_git: &Path,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Fetches `branches` from `mirror` into `host_git`, each to the branch
    /// of its name (WS-10): the default branch before every test-merge, base
    /// update or rebuild, and the run's work branch on a rebuild.
    fn fetch_into_host(
        &self,
        host_git: &Path,
        mirror: &Path,
        branches: &[Branch],
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Creates `workspace`, which does not exist, as WS-1 says: a clone of
    /// `branch` alone from `source` (the mirror) by git's transport, with an
    /// object store of its own, no checkout and no remote, and `base`, which
    /// `branch` reaches, checked out detached with the empty tree as the
    /// attribute source (WS-11). Where it fails, what it left of `workspace`
    /// is the caller's to remove.
    fn create_workspace(
        &self,
        source: &Path,
        branch: &Branch,
        base: &Oid,
        workspace: &Path,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Creates `workspace` from `host_git` at `commit`, which need not be on
    /// the default branch (WS-10): as [`Git::create_workspace`] does, from a
    /// temporary branch at `commit` that is deleted afterwards, in `host_git`
    /// and in the workspace, also where the clone fails.
    fn create_workspace_at(
        &self,
        host_git: &Path,
        commit: &Oid,
        workspace: &Path,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

/// What a git operation failed at.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The process could not be started or read.
    #[error(transparent)]
    Process(#[from] factory_proc::Error),
    /// A git command exited with a failure.
    #[error("{command} failed with {status}: {stderr}")]
    Failed {
        /// The command, by its subcommand.
        command: String,
        /// Its exit status.
        status: ExitStatus,
        /// The start of what it wrote on stderr.
        stderr: String,
    },
    /// A git command printed what the wrapper does not read.
    #[error("{command} printed {reason}")]
    Output {
        /// The command, by its subcommand.
        command: String,
        /// What was wrong with it.
        reason: String,
    },
    /// A path that is not absolute: a command runs in `/`.
    #[error("{0:?} is not an absolute path")]
    NotAbsolute(PathBuf),
    /// Text that is no object name.
    #[error("{0:?} is no object name")]
    Oid(String),
    /// Text that is no branch name.
    #[error("{0:?} is no branch name")]
    Branch(String),
    /// Text that is no remote.
    #[error("{0:?} is no https URL")]
    Remote(String),
}

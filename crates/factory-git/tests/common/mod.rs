//! What the tests of `factory-git` share: scratch directories, the host's
//! `git`, and an upstream repository that stands in for GitHub.

#![allow(dead_code, reason = "each test file uses a part of it")]

use std::ffi::OsStr;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use factory_git::{Branch, GitCli, Oid};
use factory_proc::{Command, Line};

/// The longest that a test may take.
pub const LIMIT: Duration = Duration::from_secs(60);

/// Runs `test` on a runtime of its own, and fails it after [`LIMIT`].
pub fn within(test: impl Future<Output = ()>) {
    let runtime = tokio::runtime::Runtime::new().unwrap_or_else(|error| panic!("{error}"));
    runtime.block_on(async {
        tokio::time::timeout(LIMIT, test)
            .await
            .unwrap_or_else(|_| panic!("the test took more than {LIMIT:?}"));
    });
}

/// A fresh directory for one test, removed when the value is dropped.
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new(test: &str) -> Self {
        let path = std::env::temp_dir().join(format!("sf-git-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap_or_else(|error| panic!("create {path:?}: {error}"));
        Self(
            path.canonicalize()
                .unwrap_or_else(|error| panic!("the scratch directory has a path: {error}")),
        )
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The `git` of the test process's `PATH`: the host's.
pub fn program() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join("git"))
        .find(|git| git.is_file())
        .unwrap_or_else(|| panic!("no git on PATH"))
}

/// The wrapper over the host's `git`.
pub fn cli() -> GitCli {
    GitCli::new(program())
}

pub fn main() -> Branch {
    Branch::new("main").unwrap_or_else(|error| panic!("{error}"))
}

/// Runs the host's `git` with `args` and an identity, outside the wrapper,
/// for what a test arranges, and returns stdout's lines.
pub async fn git(args: &[&OsStr]) -> Vec<String> {
    let mut command = Command::new(program());
    command
        .args(args)
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_AUTHOR_DATE", "2026-11-27T12:00:00Z")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_DATE", "2026-11-27T12:00:00Z");
    let mut process = command
        .spawn(tokio::io::sink())
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let mut lines = Vec::new();
    while let Some(line) = process
        .next_line()
        .await
        .unwrap_or_else(|error| panic!("{error}"))
    {
        match line {
            Line::Text(text) => lines.push(text),
            other => panic!("git {args:?}: {other:?}"),
        }
    }
    let status = process
        .wait()
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(status.success(), "git {args:?}: {status}");
    lines
}

/// `--git-dir=<dir>`.
pub fn git_dir(dir: &Path) -> std::ffi::OsString {
    let mut arg = std::ffi::OsString::from("--git-dir=");
    arg.push(dir);
    arg
}

/// Whether the object `oid` can be read in the repository `dir`.
pub async fn has_object(dir: &Path, oid: &str) -> bool {
    let mut command = Command::new(program());
    command
        .arg(git_dir(dir))
        .args(["cat-file", "-e", oid])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    let mut process = command
        .spawn(tokio::io::sink())
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    process
        .wait()
        .await
        .unwrap_or_else(|error| panic!("{error}"))
        .success()
}

/// A repository that stands in for GitHub: a bare repository, filled from a
/// work tree of its own by [`Upstream::commit`].
pub struct Upstream {
    /// The bare repository that the host fetches from.
    pub bare: PathBuf,
    seed: PathBuf,
}

impl Upstream {
    /// An upstream with no commit, in `scratch`.
    pub async fn new(scratch: &Scratch) -> Self {
        let bare = scratch.join("upstream.git");
        let seed = scratch.join("seed");
        git(&[
            "init".as_ref(),
            "-q".as_ref(),
            "--bare".as_ref(),
            "-b".as_ref(),
            "main".as_ref(),
            bare.as_os_str(),
        ])
        .await;
        git(&[
            "init".as_ref(),
            "-q".as_ref(),
            "-b".as_ref(),
            "main".as_ref(),
            seed.as_os_str(),
        ])
        .await;
        Self { bare, seed }
    }

    /// Runs `git` in the seed, with its directory and work tree named.
    async fn seed(&self, args: &[&OsStr]) -> Vec<String> {
        let dir = git_dir(&self.seed.join(".git"));
        let mut tree = std::ffi::OsString::from("--work-tree=");
        tree.push(&self.seed);
        let mut all: Vec<&OsStr> = vec![dir.as_os_str(), tree.as_os_str()];
        all.extend_from_slice(args);
        git(&all).await
    }

    /// Commits `files` (path, bytes) on `branch` of the seed and pushes the
    /// branch to the upstream, and returns the commit. A file whose name ends
    /// in `.sh` is executable, and an ignore rule
    /// keeps no file out. A branch that the seed
    /// does not hold starts from `main`.
    pub async fn commit(&self, branch: &str, files: &[(&str, &[u8])]) -> String {
        let heads = self.seed.join(".git/refs/heads");
        if heads.join(branch).exists() {
            self.seed(&["checkout".as_ref(), "-q".as_ref(), branch.as_ref()])
                .await;
        } else if heads.join("main").exists() {
            self.seed(&[
                "checkout".as_ref(),
                "-q".as_ref(),
                "-b".as_ref(),
                branch.as_ref(),
                "main".as_ref(),
            ])
            .await;
        } else {
            self.seed(&[
                "checkout".as_ref(),
                "-q".as_ref(),
                "-B".as_ref(),
                branch.as_ref(),
            ])
            .await;
        }
        for (path, bytes) in files {
            let file = self.seed.join(path);
            if let Some(parent) = file.parent() {
                fs::create_dir_all(parent).unwrap_or_else(|error| panic!("{error}"));
            }
            fs::write(&file, bytes).unwrap_or_else(|error| panic!("{error}"));
            if path.ends_with(".sh") {
                use std::os::unix::fs::PermissionsExt as _;
                fs::set_permissions(&file, fs::Permissions::from_mode(0o755))
                    .unwrap_or_else(|error| panic!("{error}"));
            }
        }
        self.seed(&[
            "-c".as_ref(),
            "core.autocrlf=false".as_ref(),
            "add".as_ref(),
            "-A".as_ref(),
            "--force".as_ref(),
        ])
        .await;
        self.seed(&[
            "commit".as_ref(),
            "-q".as_ref(),
            "--allow-empty".as_ref(),
            "-m".as_ref(),
            "change".as_ref(),
        ])
        .await;
        let refspec = format!("+refs/heads/{branch}:refs/heads/{branch}");
        self.seed(&[
            "push".as_ref(),
            "-q".as_ref(),
            self.bare.as_os_str(),
            refspec.as_ref(),
        ])
        .await;
        self.seed(&["rev-parse".as_ref(), "HEAD".as_ref()])
            .await
            .remove(0)
    }
}

/// The commit `oid` as an [`Oid`].
pub fn oid(oid: &str) -> Oid {
    Oid::parse(oid).unwrap_or_else(|error| panic!("{error}"))
}

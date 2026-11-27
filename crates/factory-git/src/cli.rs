//! The wrapper over the `git` CLI (WS-8): every git command of the host runs
//! here, through `factory-proc`.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use factory_proc::{Command, Line};
use std::process::ExitStatus;

use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _};

use crate::capture::{Candidate, CaptureDirs, Captured, Limits, Rejection, Trees};
use crate::names::{Branch, Oid, Remote, absolute};
use crate::{Error, Git};

/// The empty tree, the attribute source of every command that moves content
/// between a tree and a directory (WS-11).
pub(crate) const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// The environment of every git command (AG-11), beside which a command of
/// capture, ingest, the commit or the export sets only the variables of P-7
/// that name its index, object directories and identity: no
/// system or global configuration and no system attributes, no prompt, no
/// optional lock, no replace object and no lazy fetch (P-7).
const ENVIRONMENT: [(&str, &str); 10] = [
    ("PATH", "/usr/bin:/bin"),
    ("HOME", "/nonexistent"),
    ("LC_ALL", "C"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_ATTR_NOSYSTEM", "1"),
    ("GIT_TERMINAL_PROMPT", "0"),
    ("GIT_OPTIONAL_LOCKS", "0"),
    ("GIT_NO_REPLACE_OBJECTS", "1"),
    ("GIT_NO_LAZY_FETCH", "1"),
];

/// The settings that every git command is given on its command line: a bare
/// repository is used only where the command names it (WS-15), and no hook
/// and no fsmonitor runs.
const SETTINGS: [&str; 3] = [
    "safe.bareRepository=explicit",
    "core.hooksPath=/dev/null",
    "core.fsmonitor=false",
];

/// The most bytes of a command's stderr that an error keeps.
const STDERR_CAP: usize = 16 * 1024;

/// The `git` CLI of the host, at the path of its program.
///
/// Every command names its repository, with `--git-dir` or with the path that
/// `init` and `clone` take, and runs in `/`, so none finds a repository from
/// its working directory (WS-8). Every path it is given is absolute.
///
/// Fetches into a mirror are serialised by the mirror's path as it is given,
/// across this value and its clones: the daemon holds one, and derives each
/// mirror's path from ids (WS-2), so one mirror has one path.
#[derive(Debug, Clone)]
pub struct GitCli {
    program: PathBuf,
    mirrors: Locks,
}

impl GitCli {
    /// The `git` at `program`, an absolute path.
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            mirrors: Locks::default(),
        }
    }

    /// Runs `git` with `args` after the common settings, and returns the
    /// lines of its stdout.
    pub(crate) async fn run(&self, args: &[&OsStr]) -> Result<Vec<String>, Error> {
        let mut command = Command::new(&self.program);
        for setting in SETTINGS {
            command.arg("-c").arg(setting);
        }
        command.args(args).current_dir("/");
        for (key, value) in ENVIRONMENT {
            command.env(key, value);
        }
        let name = describe(args);
        // Stderr is read beside the output, so that a full pipe never holds
        // the process up. The process holds the other end of the pipe, and
        // the read ends when the process is dropped.
        let (reader, writer) = tokio::io::duplex(8192);
        let output = async move {
            let mut process = command.spawn(writer).await?;
            let mut lines = Vec::new();
            let mut unreadable = None;
            while let Some(line) = process.next_line().await? {
                match line {
                    Line::Text(text) => lines.push(text),
                    Line::TooLong => unreadable = Some("a line longer than the cap"),
                    Line::NotUtf8 | Line::Bytes(_) => {
                        unreadable = Some("a line that is no UTF-8");
                    }
                }
            }
            let status = process.wait().await?;
            Ok::<_, Error>((lines, unreadable, status))
        };
        let (output, stderr) = tokio::join!(output, capture(reader));
        let (lines, unreadable, status) = output?;
        if !status.success() {
            return Err(Error::Failed {
                command: name,
                status,
                stderr,
            });
        }
        match unreadable {
            Some(reason) => Err(Error::Output {
                command: name,
                reason: reason.to_owned(),
            }),
            None => Ok(lines),
        }
    }

    /// Runs `git --git-dir=<repository>` with `args`.
    pub(crate) async fn run_in(
        &self,
        repository: &Path,
        args: &[&OsStr],
    ) -> Result<Vec<String>, Error> {
        let dir = option("--git-dir=", absolute(repository)?);
        let mut all = vec![dir.as_os_str()];
        all.extend_from_slice(args);
        self.run(&all).await
    }

    /// Runs `git` with `args` after the common settings, with the variables
    /// of `env` beside those of every command, `stdin` written to its stdin,
    /// and its stdout read as bytes, at most `cap` of them. It fails only
    /// where the process cannot be started or read: the caller reads the
    /// status.
    pub(crate) async fn raw(
        &self,
        args: &[&OsStr],
        env: &[(&str, &OsStr)],
        stdin: Option<&[u8]>,
        cap: usize,
    ) -> Result<Output, Error> {
        let mut command = Command::new(&self.program);
        for setting in SETTINGS {
            command.arg("-c").arg(setting);
        }
        command.args(args).current_dir("/").raw_stdout();
        for (key, value) in ENVIRONMENT {
            command.env(key, value);
        }
        for (key, value) in env {
            command.env(key, value);
        }
        if stdin.is_some() {
            command.stdin_piped();
        }
        let name = describe(args);
        let (reader, writer) = tokio::io::duplex(8192);
        let output = async move {
            let mut process = command.spawn(writer).await?;
            let pipe = process.take_stdin();
            let feed = async move {
                if let (Some(mut pipe), Some(bytes)) = (pipe, stdin) {
                    // A command may exit before it reads all of stdin: its
                    // status says what came of it.
                    let _ = pipe.write_all(bytes).await;
                }
            };
            let read = async {
                let mut stdout = Vec::new();
                let mut overflow = false;
                while let Some(line) = process.next_line().await? {
                    if let Line::Bytes(chunk) = line {
                        // The first `cap` bytes are kept, and the rest read
                        // and discarded.
                        let room = cap.saturating_sub(stdout.len());
                        if chunk.len() > room {
                            overflow = true;
                        }
                        stdout.extend_from_slice(chunk.get(..room).unwrap_or(&chunk));
                    }
                }
                let status = process.wait().await?;
                Ok::<_, Error>((stdout, overflow, status))
            };
            let ((), read) = tokio::join!(feed, read);
            read
        };
        let (output, stderr) = tokio::join!(output, capture(reader));
        let (stdout, overflow, status) = output?;
        Ok(Output {
            command: name,
            status,
            stdout,
            overflow,
            stderr,
        })
    }
}

impl Git for GitCli {
    async fn fetch_mirror(&self, mirror: &Path, remote: &Remote) -> Result<(), Error> {
        absolute(mirror)?;
        let lock = self.mirrors.of(mirror);
        let _serialised = lock.lock().await;
        if !mirror.join("HEAD").exists() {
            self.run(&[
                "init".as_ref(),
                "-q".as_ref(),
                "--bare".as_ref(),
                "--template=".as_ref(),
                mirror.as_os_str(),
            ])
            .await?;
        }
        self.run_in(
            mirror,
            &[
                "fetch".as_ref(),
                "-q".as_ref(),
                "--prune".as_ref(),
                "--no-tags".as_ref(),
                "--".as_ref(),
                remote.as_os_str(),
                "+refs/heads/*:refs/heads/*".as_ref(),
            ],
        )
        .await?;
        Ok(())
    }

    async fn branch_head(&self, repository: &Path, branch: &Branch) -> Result<Oid, Error> {
        let revision = format!("{}^{{commit}}", branch.full());
        let lines = self
            .run_in(
                repository,
                &[
                    "rev-parse".as_ref(),
                    "--verify".as_ref(),
                    "--end-of-options".as_ref(),
                    revision.as_ref(),
                ],
            )
            .await?;
        one_oid("git rev-parse", &lines)
    }

    async fn create_host_repository(
        &self,
        mirror: &Path,
        default: &Branch,
        host_git: &Path,
    ) -> Result<(), Error> {
        self.clone_branch("--bare", mirror, default, host_git)
            .await?;
        self.remove_origin(host_git).await
    }

    async fn fetch_into_host(
        &self,
        host_git: &Path,
        mirror: &Path,
        branches: &[Branch],
    ) -> Result<(), Error> {
        let refspecs: Vec<String> = branches
            .iter()
            .map(|branch| format!("+{0}:{0}", branch.full()))
            .collect();
        let mut args: Vec<&OsStr> = vec![
            "fetch".as_ref(),
            "-q".as_ref(),
            "--no-tags".as_ref(),
            "--".as_ref(),
            absolute(mirror)?.as_os_str(),
        ];
        args.extend(refspecs.iter().map(OsStr::new));
        self.run_in(host_git, &args).await?;
        Ok(())
    }

    async fn create_workspace(
        &self,
        source: &Path,
        branch: &Branch,
        base: &Oid,
        workspace: &Path,
    ) -> Result<(), Error> {
        self.clone_branch("--no-checkout", source, branch, workspace)
            .await?;
        let git_dir = option("--git-dir=", workspace.join(".git"));
        let work_tree = option("--work-tree=", workspace);
        let attributes = format!("--attr-source={EMPTY_TREE}");
        self.run(&[
            git_dir.as_os_str(),
            work_tree.as_os_str(),
            attributes.as_ref(),
            "checkout".as_ref(),
            "-q".as_ref(),
            "--detach".as_ref(),
            base.as_str().as_ref(),
        ])
        .await?;
        self.remove_origin(&workspace.join(".git")).await
    }

    async fn create_workspace_at(
        &self,
        host_git: &Path,
        commit: &Oid,
        workspace: &Path,
    ) -> Result<(), Error> {
        absolute(workspace)?;
        let branch = Branch::new(&format!("sf/workspace/{commit}"))?;
        let full = branch.full();
        // A ref of this name that an interrupted creation left behind names
        // the same commit, and is written over.
        self.run_in(
            host_git,
            &[
                "update-ref".as_ref(),
                "--no-deref".as_ref(),
                full.as_ref(),
                commit.as_str().as_ref(),
            ],
        )
        .await?;
        let created = self
            .create_workspace(host_git, &branch, commit, workspace)
            .await;
        let deleted = self.delete_ref(host_git, &full).await;
        created?;
        deleted?;
        self.delete_ref(&workspace.join(".git"), &full).await
    }

    async fn tree_of(&self, repository: &Path, commit: &Oid) -> Result<Oid, Error> {
        GitCli::tree_of(self, repository, commit).await
    }

    async fn capture(
        &self,
        dirs: CaptureDirs<'_>,
        trees: Trees<'_>,
        limits: Limits,
    ) -> Result<Captured, Error> {
        GitCli::capture(self, dirs, trees, limits).await
    }

    async fn ingest(
        &self,
        host_git: &Path,
        ingest: &Path,
        trees: Trees<'_>,
        tree: &[u8],
        pack: &[u8],
        limits: Limits,
    ) -> Result<Captured, Error> {
        GitCli::ingest(self, host_git, ingest, trees, tree, pack, limits).await
    }

    async fn commit_candidate(
        &self,
        host_git: &Path,
        candidate: &Candidate<'_>,
    ) -> Result<Oid, Error> {
        GitCli::commit_candidate(self, host_git, candidate).await
    }

    async fn export(&self, host_git: &Path, candidate: &Oid, export: &Path) -> Result<(), Error> {
        GitCli::export(self, host_git, candidate, export).await
    }
}

impl GitCli {
    /// Clones `branch` alone of `source` into `target` by git's transport,
    /// with an object store of its own, no tag and no template: as a bare
    /// repository (`--bare`, WS-10) or with no checkout (`--no-checkout`,
    /// WS-1).
    async fn clone_branch(
        &self,
        kind: &str,
        source: &Path,
        branch: &Branch,
        target: &Path,
    ) -> Result<(), Error> {
        self.run(&[
            "clone".as_ref(),
            "-q".as_ref(),
            kind.as_ref(),
            "--no-local".as_ref(),
            "--single-branch".as_ref(),
            "--no-tags".as_ref(),
            "--template=".as_ref(),
            "--branch".as_ref(),
            branch.as_str().as_ref(),
            "--".as_ref(),
            absolute(source)?.as_os_str(),
            absolute(target)?.as_os_str(),
        ])
        .await?;
        Ok(())
    }

    /// Deletes the ref `full` of `repository`.
    async fn delete_ref(&self, repository: &Path, full: &str) -> Result<(), Error> {
        self.run_in(
            repository,
            &[
                "update-ref".as_ref(),
                "--no-deref".as_ref(),
                "-d".as_ref(),
                full.as_ref(),
            ],
        )
        .await?;
        Ok(())
    }

    /// Removes the remote `origin` that a clone left in `repository`.
    async fn remove_origin(&self, repository: &Path) -> Result<(), Error> {
        self.run_in(
            repository,
            &["remote".as_ref(), "remove".as_ref(), "origin".as_ref()],
        )
        .await?;
        Ok(())
    }
}

/// What a command of [`GitCli::raw`] left.
#[derive(Debug)]
pub(crate) struct Output {
    /// The command, by its subcommand.
    pub(crate) command: String,
    pub(crate) status: ExitStatus,
    /// Its stdout, up to the cap.
    pub(crate) stdout: Vec<u8>,
    /// Whether stdout held more than the cap, of which it keeps the first.
    pub(crate) overflow: bool,
    /// The start of its stderr.
    pub(crate) stderr: String,
}

impl Output {
    /// The output of a command that succeeded, or its failure as an error.
    pub(crate) fn checked(self) -> Result<Self, Error> {
        if self.status.success() && !self.overflow {
            Ok(self)
        } else if self.overflow {
            Err(Error::Output {
                command: self.command,
                reason: "more than the cap".to_owned(),
            })
        } else {
            Err(Error::Failed {
                command: self.command,
                status: self.status,
                stderr: self.stderr,
            })
        }
    }

    /// The output of a command of the staging git that succeeded, or its
    /// failure as a rejection (9.4 step 2).
    pub(crate) fn staged(self) -> Result<Self, Rejection> {
        if self.status.success() && !self.overflow {
            Ok(self)
        } else {
            Err(Rejection::Staging {
                command: self.command,
                stderr: if self.overflow {
                    "more than the cap".to_owned()
                } else {
                    self.stderr
                },
            })
        }
    }
}

/// The one object name of a command's output.
pub(crate) fn one_oid(command: &str, lines: &[String]) -> Result<Oid, Error> {
    match lines {
        [line] => Oid::parse(line),
        _ => Err(Error::Output {
            command: command.to_owned(),
            reason: format!("{} lines where one object name was expected", lines.len()),
        }),
    }
}

/// `<name><value>`, as one argument.
pub(crate) fn option(name: &str, value: impl AsRef<OsStr>) -> OsString {
    let mut arg = OsString::from(name);
    arg.push(value);
    arg
}

/// `git <subcommand>`, the name of a command in an error: its first argument
/// that is no option. A URL or a path is never in it.
fn describe(args: &[&OsStr]) -> String {
    let subcommand = args
        .iter()
        .find(|arg| !arg.to_string_lossy().starts_with('-'))
        .map(|arg| arg.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("git {subcommand}")
}

/// One lock for each mirror, by its path, so that fetches into one mirror
/// run one at a time and fetches into two run side by side (WS-8).
#[derive(Debug, Clone, Default)]
struct Locks(Arc<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>>);

impl Locks {
    fn of(&self, path: &Path) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(locks.entry(path.to_owned()).or_default())
    }
}

/// The first [`STDERR_CAP`] bytes that `reader` gives, as text without the
/// white space at its end. What comes after them is read and discarded, and
/// a failed read ends the text where it failed.
async fn capture(reader: impl AsyncRead + Unpin) -> String {
    let mut kept = Vec::new();
    let mut limited = reader.take(STDERR_CAP as u64);
    let _ = limited.read_to_end(&mut kept).await;
    let _ = tokio::io::copy(&mut limited.into_inner(), &mut tokio::io::sink()).await;
    String::from_utf8_lossy(&kept).trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::PathBuf;

    use super::{GitCli, describe};
    use crate::Error;

    fn cli() -> GitCli {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let program: PathBuf = std::env::split_paths(&path)
            .map(|dir| dir.join("git"))
            .find(|git| git.is_file())
            .unwrap();
        GitCli::new(program)
    }

    /// Runs `test` on a runtime of its own, and fails it after 30 s.
    fn within(test: impl Future<Output = ()>) {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(30), test)
                .await
                .unwrap();
        });
    }

    #[test]
    fn every_command_uses_a_bare_repository_only_where_it_names_it() {
        within(async {
            for (key, value) in [
                ("safe.bareRepository", "explicit"),
                ("core.hooksPath", "/dev/null"),
                ("core.fsmonitor", "false"),
            ] {
                let lines = cli()
                    .run(&["config".as_ref(), "--get".as_ref(), key.as_ref()])
                    .await
                    .unwrap();
                assert_eq!(lines, [value], "{key}");
            }
        });
    }

    #[test]
    fn a_command_reads_no_configuration_of_the_host_and_finds_no_repository() {
        within(async {
            // No system or global file: `--show-origin` lists only the
            // command line.
            let lines = cli()
                .run(&[
                    "config".as_ref(),
                    "--list".as_ref(),
                    "--show-origin".as_ref(),
                ])
                .await
                .unwrap();
            assert!(
                lines.iter().all(|line| line.starts_with("command line:")),
                "{lines:?}"
            );
            let error = cli()
                .run(&["rev-parse".as_ref(), "--git-dir".as_ref()])
                .await;
            assert!(error.is_err());
        });
    }

    #[test]
    fn a_failed_command_carries_the_start_of_its_stderr() {
        within(async {
            let error = cli()
                .run(&["rev-parse".as_ref(), "--git-dir".as_ref()])
                .await
                .unwrap_err();
            let Error::Failed { stderr, .. } = error else {
                panic!("{error}");
            };
            assert!(
                stderr.starts_with("fatal: not a git repository"),
                "{stderr}"
            );

            // git names an unknown command in its message, so a long name
            // makes a long stderr.
            let name = "x".repeat(40_000);
            let error = cli().run(&[name.as_ref()]).await.unwrap_err();
            let Error::Failed { stderr, .. } = error else {
                panic!("{error}");
            };
            // The cap is 16 KiB.
            assert_eq!(stderr.len(), 16_384);
            assert!(stderr.ends_with("xxxx"), "{}", &stderr[..80]);
        });
    }

    #[test]
    fn raw_output_keeps_the_bytes_up_to_its_cap() {
        within(async {
            let args: [&OsStr; 4] = [
                "-c".as_ref(),
                "x.y=abc".as_ref(),
                "config".as_ref(),
                "x.y".as_ref(),
            ];
            let at_cap = cli().raw(&args, &[], None, 4).await.unwrap();
            assert_eq!(at_cap.stdout, b"abc\n");
            assert!(!at_cap.overflow);
            assert!(at_cap.checked().is_ok());
            let over = cli().raw(&args, &[], None, 3).await.unwrap();
            assert_eq!(over.stdout, b"abc");
            assert!(over.overflow);
            // A command that succeeded with more than the cap is no output to
            // read.
            assert!(over.status.success());
            assert!(matches!(over.checked(), Err(Error::Output { .. })));
        });
    }

    #[test]
    fn a_failed_command_is_named_by_its_subcommand_and_nothing_else() {
        let args: Vec<&OsStr> = vec![
            "--git-dir=/srv/mirrors/o/r.git".as_ref(),
            "fetch".as_ref(),
            "https://x-access-token:secret@github.com/o/r".as_ref(),
        ];
        assert_eq!(describe(&args), "git fetch");
    }
}

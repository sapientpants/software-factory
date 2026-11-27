//! Capture, ingest, the commit of a candidate and the export (SPEC 9.4).
//!
//! Capture builds a candidate's tree from a work tree in four steps: the
//! scan, by `lstat` alone; the staging git, which lists and stages the change
//! set in `quarantine/` and hands back a tree name and a pack; the ingest of
//! that pack into `host.git`, through `ingest/`; and, apart, the commit. The
//! export writes a candidate into a new, empty directory. Nothing moves bytes
//! through an in-tree attribute (WS-11).

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};
use std::os::unix::fs::FileTypeExt as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::{EMPTY_TREE, GitCli, Output, option};
use crate::names::{Oid, absolute};
use crate::tree::{self, Entry, Tree};
use crate::{Error, dotgit};

/// `quarantine/config`, written by the host: the only configuration that the
/// staging git reads (9.4 step 2, P-7). No hook, filter, fsmonitor, excludes
/// file or attributes file, and no system or global configuration, which the
/// environment of every command leaves out.
const STAGE_CONFIG: &str = "[core]
\trepositoryformatversion = 0
\tbare = false
\tfileMode = true
\tsymlinks = true
\tignoreCase = false
\tprecomposeUnicode = false
\tprotectHFS = true
\tprotectNTFS = true
\tautocrlf = false
\tsafecrlf = false
\tfsmonitor = false
\tuntrackedCache = false
\thooksPath = /dev/null
\tattributesFile = /dev/null
\texcludesFile = /dev/null
\tlogAllRefUpdates = false
[gc]
\tauto = 0
[maintenance]
\tauto = false
[submodule]
\trecurse = false
";

/// What a pack may hold beyond the change set's bytes: its trees and its
/// headers *(tunable)*.
const PACK_HEADROOM: u64 = 1_048_576;

/// The most bytes of a listing or a tree that the host reads *(tunable)*.
const LISTING_CAP: usize = 67_108_864;

/// The capture limits of 9.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Bytes of one file.
    pub file_bytes: u64,
    /// New or changed bytes of a candidate.
    pub candidate_bytes: u64,
    /// The time that the staging step may take.
    pub staging: Duration,
}

impl From<&factory_core::config::CaptureLimits> for Limits {
    fn from(limits: &factory_core::config::CaptureLimits) -> Self {
        Self {
            file_bytes: limits.file_bytes.get(),
            candidate_bytes: limits.candidate_bytes.get(),
            staging: Duration::from_secs(limits.timeout_secs.get()),
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::from(&factory_core::config::CaptureLimits::default())
    }
}

impl Limits {
    /// The most bytes of a pack that ingest reads.
    fn pack_bytes(self) -> u64 {
        self.candidate_bytes.saturating_add(PACK_HEADROOM)
    }
}

/// The directories of one capture, all absolute (9.1).
#[derive(Debug, Clone, Copy)]
pub struct CaptureDirs<'a> {
    /// The work tree: `runs/<run_id>/workspace`.
    pub workspace: &'a Path,
    /// The bare `host.git` of the run.
    pub host_git: &'a Path,
    /// The repository of the staging git, made anew for each capture.
    pub quarantine: &'a Path,
    /// The object directory that a pack is indexed into before it enters
    /// `host.git`.
    pub ingest: &'a Path,
}

/// What a capture compares against (9.4 step 2, WS-12).
#[derive(Debug, Clone, Copy)]
pub struct Trees<'a> {
    /// The tree the workspace was last created or captured from: what the
    /// change set is counted from.
    pub previous: &'a Oid,
    /// The tree of the run's `base_sha`, for WS-12 and WS-13.
    pub base: &'a Oid,
}

/// A path of a tree or of the workspace, relative to its root, as the bytes
/// that git and the file system hold: it may be no UTF-8.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TreePath(Vec<u8>);

impl TreePath {
    /// The bytes of the path.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl From<&[u8]> for TreePath {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }
}

impl fmt::Debug for TreePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", String::from_utf8_lossy(&self.0))
    }
}

impl fmt::Display for TreePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.0))
    }
}

/// Why a capture built no candidate. Each returns the run to `implement`
/// with the rejection in its feedback (7.3).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, thiserror::Error)]
pub enum Rejection {
    /// An entry whose name git treats as `.git` (step 1).
    #[error("{0}: a name that git treats as .git")]
    DotGit(TreePath),
    /// An entry that is no regular file, directory or symbolic link (step 1).
    #[error("{0}: not a regular file, directory or symbolic link")]
    SpecialFile(TreePath),
    /// A directory that the scan could not list (step 1).
    #[error("{0}: a directory that cannot be listed")]
    Unlistable(TreePath),
    /// A listed path that is no regular file or symbolic link of the scan,
    /// such as a nested repository, which git lists as its directory (step 2).
    #[error("{0}: not a regular file or symbolic link")]
    NotAFile(TreePath),
    /// A file over the limit of one file (steps 2 and 3).
    #[error("{path}: {bytes} bytes, over the limit of a file")]
    FileTooLarge {
        /// The file.
        path: TreePath,
        /// Its size.
        bytes: u64,
    },
    /// A change set over the limit of a candidate (steps 2 and 3).
    #[error("a change set of {bytes} bytes, over the limit of a candidate")]
    ChangeSetTooLarge {
        /// Its new or changed bytes, or more where it was cut off.
        bytes: u64,
    },
    /// A git command of the staging step failed (step 2).
    #[error("{command} failed: {stderr}")]
    Staging {
        /// The command, by its subcommand.
        command: String,
        /// The start of its stderr.
        stderr: String,
    },
    /// Listed paths that staging left as they were (step 2).
    #[error("not staged: {0:?}")]
    NotStaged(Vec<TreePath>),
    /// What the staging step handed back is no tree name and pack that
    /// ingest takes: `index-pack --strict` refused the pack, or the name is
    /// no tree of it (step 3).
    #[error("the staging output was refused: {0}")]
    Pack(String),
    /// An entry of a mode that no candidate holds, such as a submodule
    /// entry (step 3).
    #[error("{path}: mode {mode}")]
    Mode {
        /// The entry.
        path: TreePath,
        /// Its mode.
        mode: String,
    },
    /// Two paths of one fold that the base tree does not both hold (WS-12).
    #[error("{first} and {second} have one fold")]
    SameFold {
        /// The first of the two, in the order of the tree.
        first: TreePath,
        /// The second.
        second: TreePath,
    },
    /// A path that differs from the base tree below a directory with the
    /// layout of a repository (WS-13).
    #[error("{path} lies below {layout}, which has the layout of a git repository")]
    BelowLayout {
        /// The path.
        path: TreePath,
        /// The directory, as one of its spellings; empty for the root.
        layout: TreePath,
    },
}

/// What a capture built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Captured {
    /// The candidate's tree, now in `host.git`.
    Tree(Oid),
    /// No candidate, and `host.git` as it was.
    Rejected(Vec<Rejection>),
}

/// The commit of a candidate (9.4 step 4).
#[derive(Debug, Clone)]
pub struct Candidate<'a> {
    /// Its tree.
    pub tree: &'a Oid,
    /// Its parents: the previous published head or `base_sha`, or, after a
    /// conflicting base update, the two parents of W.
    pub parents: &'a [Oid],
    /// The subject, a blank line, the body and the trailers.
    pub message: &'a str,
    /// The factory identity, which is author and committer.
    pub identity: &'a Identity,
    /// The journalled date, in seconds since the epoch, in UTC.
    pub seconds: i64,
}

/// A name and an address for a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The name.
    pub name: String,
    /// The address.
    pub email: String,
}

impl Identity {
    fn valid(&self) -> bool {
        let fine = |text: &str| {
            !text.trim().is_empty() && !text.chars().any(|c| c.is_control() || c == '<' || c == '>')
        };
        fine(&self.name) && fine(&self.email)
    }
}

/// An entry of the scan's inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    File,
    Symlink,
    Directory,
}

/// What the scan saw of each entry: its kind and its `lstat` size.
type Inventory = BTreeMap<Vec<u8>, (Kind, u64)>;

/// Step 1: every entry of `workspace` but the root `.git`, by `lstat` alone,
/// with the rejections of the scan.
fn scan(workspace: &Path) -> (Inventory, Vec<Rejection>) {
    let mut inventory = Inventory::new();
    let mut rejections = Vec::new();
    let mut pending: Vec<(PathBuf, Vec<u8>)> = vec![(workspace.to_owned(), Vec::new())];
    while let Some((dir, relative)) = pending.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => {
                rejections.push(Rejection::Unlistable(TreePath(relative)));
                continue;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                rejections.push(Rejection::Unlistable(TreePath(relative.clone())));
                break;
            };
            let name = entry.file_name();
            let name = name.as_bytes();
            if relative.is_empty() && name == b".git" {
                continue;
            }
            let mut path = relative.clone();
            if !path.is_empty() {
                path.push(b'/');
            }
            path.extend_from_slice(name);
            if dotgit::is_dot_git(name) {
                rejections.push(Rejection::DotGit(TreePath(path)));
                continue;
            }
            let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
                rejections.push(Rejection::Unlistable(TreePath(path)));
                continue;
            };
            let kind = metadata.file_type();
            if kind.is_file() {
                inventory.insert(path, (Kind::File, metadata.len()));
            } else if kind.is_symlink() {
                inventory.insert(path, (Kind::Symlink, metadata.len()));
            } else if kind.is_dir() {
                inventory.insert(path.clone(), (Kind::Directory, 0));
                pending.push((entry.path(), path));
            } else {
                debug_assert!(
                    kind.is_fifo()
                        || kind.is_socket()
                        || kind.is_block_device()
                        || kind.is_char_device()
                );
                rejections.push(Rejection::SpecialFile(TreePath(path)));
            }
        }
    }
    (inventory, rejections)
}

/// The change set that the staging git listed: the paths to remove from the
/// index and those to add.
#[derive(Debug, Default)]
struct Listing {
    deleted: Vec<Vec<u8>>,
    other: Vec<Vec<u8>>,
}

impl Listing {
    fn is_empty(&self) -> bool {
        self.deleted.is_empty() && self.other.is_empty()
    }

    fn paths(&self) -> Vec<TreePath> {
        self.deleted
            .iter()
            .chain(&self.other)
            .map(|path| TreePath(path.clone()))
            .collect()
    }

    /// The host's check of the listing against the scan's inventory, which
    /// touches the workspace no more (step 2).
    fn check(&self, inventory: &Inventory, limits: Limits) -> Vec<Rejection> {
        let mut rejections = Vec::new();
        let mut total: u64 = 0;
        for path in &self.other {
            match inventory.get(path) {
                // git lists a nested repository as its directory, with a `/`
                // at its end, which the inventory never holds.
                Some((Kind::File | Kind::Symlink, size)) => {
                    total = total.saturating_add(*size);
                    if *size > limits.file_bytes {
                        rejections.push(Rejection::FileTooLarge {
                            path: TreePath(path.clone()),
                            bytes: *size,
                        });
                    }
                }
                _ => rejections.push(Rejection::NotAFile(TreePath(path.clone()))),
            }
        }
        if total > limits.candidate_bytes {
            rejections.push(Rejection::ChangeSetTooLarge { bytes: total });
        }
        rejections
    }
}

/// The records of NUL-delimited output, without the empty one at its end.
fn records(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes
        .strip_suffix(b"\0")
        .unwrap_or(bytes)
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
}

/// The output of `diff-files -z --raw`: a line of modes, names and status,
/// then the path, for each entry.
fn parse_diff_files(bytes: &[u8], command: &str) -> Result<Listing, Error> {
    let mut listing = Listing::default();
    let mut fields = records(bytes);
    while let Some(meta) = fields.next() {
        let status = meta
            .split(|byte| *byte == b' ')
            .nth(4)
            .and_then(|s| s.first());
        let (Some(status), Some(path)) = (status, fields.next()) else {
            return Err(Error::Output {
                command: command.to_owned(),
                reason: "a record that is no raw diff entry".to_owned(),
            });
        };
        if *status == b'D' {
            listing.deleted.push(path.to_vec());
        } else {
            listing.other.push(path.to_vec());
        }
    }
    Ok(listing)
}

/// The output of `ls-tree -r -t -z -l --full-tree`: the entries of a tree.
fn parse_tree(bytes: &[u8]) -> Result<Tree, Error> {
    let unread = || Error::Output {
        command: "git ls-tree".to_owned(),
        reason: "a record that is no tree entry".to_owned(),
    };
    let mut tree = Tree::new();
    for record in records(bytes) {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(unread)?;
        let meta =
            std::str::from_utf8(record.get(..tab).ok_or_else(unread)?).map_err(|_| unread())?;
        let path = record.get(tab + 1..).ok_or_else(unread)?;
        let mut fields = meta.split_ascii_whitespace();
        let (Some(mode), Some(_kind), Some(oid), Some(size), None) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            return Err(unread());
        };
        let size = if size == "-" {
            0
        } else {
            size.parse().map_err(|_| unread())?
        };
        tree.insert(
            path.to_vec(),
            Entry {
                mode: mode.to_owned(),
                oid: oid.to_owned(),
                size,
            },
        );
    }
    Ok(tree)
}

/// The bytes of `paths` with a NUL after each, for `--stdin` under `-z`.
fn nul_terminated(paths: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for path in paths {
        bytes.extend_from_slice(path);
        bytes.push(0);
    }
    bytes
}

/// The attribute source of every command that moves content between a tree
/// and a directory (WS-11).
fn attr_source() -> OsString {
    option("--attr-source=", EMPTY_TREE)
}

/// The options of every command of the staging git: `quarantine` as its
/// repository, the workspace as its work tree, and the attribute source of
/// WS-11.
struct Staging([OsString; 3]);

impl Staging {
    fn new(dirs: CaptureDirs<'_>) -> Self {
        Self([
            option("--git-dir=", dirs.quarantine),
            option("--work-tree=", dirs.workspace),
            attr_source(),
        ])
    }

    /// The options, then `args`.
    fn args<'a>(&'a self, args: &[&'a OsStr]) -> Vec<&'a OsStr> {
        let mut all: Vec<&OsStr> = self.0.iter().map(OsString::as_os_str).collect();
        all.extend_from_slice(args);
        all
    }
}

/// What the staging step handed back, or why it built nothing.
enum Staged {
    /// The tree name and the pack, as untrusted bytes.
    Output {
        tree: Vec<u8>,
        pack: Vec<u8>,
    },
    Rejected(Vec<Rejection>),
}

impl GitCli {
    /// The tree of `commit` in `repository`.
    pub(crate) async fn tree_of(&self, repository: &Path, commit: &Oid) -> Result<Oid, Error> {
        let revision = format!("{commit}^{{tree}}");
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
        crate::cli::one_oid("git rev-parse", &lines)
    }

    /// Steps 1 to 3 of 9.4.
    pub(crate) async fn capture(
        &self,
        dirs: CaptureDirs<'_>,
        trees: Trees<'_>,
        limits: Limits,
    ) -> Result<Captured, Error> {
        for dir in [dirs.workspace, dirs.host_git, dirs.quarantine, dirs.ingest] {
            absolute(dir)?;
        }
        let workspace = dirs.workspace.to_owned();
        // A panic of the scan is one of this activity (19.3). A blocking task
        // is cancelled only as the runtime shuts down, and its error is then
        // a panic as well.
        let (inventory, rejections) = tokio::task::spawn_blocking(move || scan(&workspace))
            .await
            .unwrap_or_else(|error| std::panic::resume_unwind(error.into_panic()));
        if !rejections.is_empty() {
            return Ok(Captured::Rejected(rejections));
        }
        // The quarantine is the host's to make, before the staging step and
        // outside its time.
        self.make_quarantine(dirs.host_git, dirs.quarantine).await?;
        let staged = tokio::time::timeout(
            limits.staging,
            self.stage(dirs, trees.previous, &inventory, limits),
        )
        .await
        .map_err(|_| Error::TimedOut(limits.staging))??;
        match staged {
            Staged::Rejected(rejections) => Ok(Captured::Rejected(rejections)),
            Staged::Output { tree, pack } => {
                self.ingest(dirs.host_git, dirs.ingest, trees, &tree, &pack, limits)
                    .await
            }
        }
    }

    /// Step 2: the staging git, with `quarantine` as its repository and the
    /// workspace as its work tree.
    ///
    /// In M0 the host's git runs it outside any sandbox. The capture stage of
    /// M1a runs these commands in one sandbox instance with no network and
    /// no credentials (9.4, 13.3), and takes back what [`Staged::Output`]
    /// holds alone.
    async fn stage(
        &self,
        dirs: CaptureDirs<'_>,
        previous: &Oid,
        inventory: &Inventory,
        limits: Limits,
    ) -> Result<Staged, Error> {
        let staging = Staging::new(dirs);
        // A failed command of the staging git is the workspace's doing, and a
        // rejection: it reads nothing else.
        macro_rules! staged {
            ($args:expr, $stdin:expr, $cap:expr) => {{
                let output = self.raw(&staging.args($args), &[], $stdin, $cap).await?;
                match output.staged() {
                    Ok(output) => output,
                    Err(failed) => return Ok(Staged::Rejected(vec![failed])),
                }
            }};
        }

        staged!(&["read-tree".as_ref(), previous.as_str().as_ref()], None, 0);
        staged!(
            &["update-index".as_ref(), "-q".as_ref(), "--refresh".as_ref()],
            None,
            LISTING_CAP
        );
        let listing = self.list(&staging).await?;
        let listing = match listing {
            Ok(listing) => listing,
            Err(failed) => return Ok(Staged::Rejected(vec![failed])),
        };
        let rejections = listing.check(inventory, limits);
        if !rejections.is_empty() {
            return Ok(Staged::Rejected(rejections));
        }
        // Deletions first: a deleted path may now lie behind a symbolic link
        // or below a file of its name, where `--remove` refuses it (P-7).
        for (option, paths) in [
            ("--force-remove", &listing.deleted),
            ("--add", &listing.other),
        ] {
            if !paths.is_empty() {
                let input = nul_terminated(paths);
                staged!(
                    &[
                        "update-index".as_ref(),
                        "-z".as_ref(),
                        option.as_ref(),
                        "--stdin".as_ref()
                    ],
                    Some(&input),
                    LISTING_CAP
                );
            }
        }
        // `update-index` skips a path it refuses and exits 0, so what is still
        // listed was not staged.
        let left = match self.list(&staging).await? {
            Ok(left) => left,
            Err(failed) => return Ok(Staged::Rejected(vec![failed])),
        };
        if !left.is_empty() {
            return Ok(Staged::Rejected(vec![Rejection::NotStaged(left.paths())]));
        }
        let tree = staged!(&["write-tree".as_ref()], None, 1024).stdout;
        let tree = tree.strip_suffix(b"\n").unwrap_or(&tree).to_vec();
        let mut input = tree.clone();
        input.push(b'\n');
        let cap = usize::try_from(limits.pack_bytes()).unwrap_or(usize::MAX);
        let output = self
            .raw(
                &staging.args(&[
                    "pack-objects".as_ref(),
                    "--revs".as_ref(),
                    "--stdout".as_ref(),
                    "--local".as_ref(),
                    "-q".as_ref(),
                ]),
                &[],
                Some(&input),
                cap,
            )
            .await?;
        if output.overflow {
            return Ok(Staged::Rejected(vec![Rejection::ChangeSetTooLarge {
                bytes: limits.pack_bytes(),
            }]));
        }
        match output.staged() {
            Ok(output) => Ok(Staged::Output {
                tree,
                pack: output.stdout,
            }),
            Err(failed) => Ok(Staged::Rejected(vec![failed])),
        }
    }

    /// The change set, by the staging git: changed and deleted tracked paths,
    /// submodules ignored, and untracked paths less those that the
    /// workspace's `.gitignore` files ignore.
    async fn list(&self, staging: &Staging) -> Result<Result<Listing, Rejection>, Error> {
        let command = "git diff-files";
        let diff = self
            .raw(
                &staging.args(&[
                    "diff-files".as_ref(),
                    "-z".as_ref(),
                    "--raw".as_ref(),
                    "--no-renames".as_ref(),
                    "--ignore-submodules=all".as_ref(),
                ]),
                &[],
                None,
                LISTING_CAP,
            )
            .await?;
        let diff = match diff.staged() {
            Ok(diff) => diff,
            Err(failed) => return Ok(Err(failed)),
        };
        let mut listing = parse_diff_files(&diff.stdout, command)?;
        let others = self
            .raw(
                &staging.args(&[
                    "ls-files".as_ref(),
                    "-z".as_ref(),
                    "--others".as_ref(),
                    "--exclude-standard".as_ref(),
                ]),
                &[],
                None,
                LISTING_CAP,
            )
            .await?;
        let others = match others.staged() {
            Ok(others) => others,
            Err(failed) => return Ok(Err(failed)),
        };
        listing
            .other
            .extend(records(&others.stdout).map(<[u8]>::to_vec));
        Ok(Ok(listing))
    }

    /// Creates `quarantine` anew: an empty object directory with `host.git`'s
    /// objects as an alternate, and the host-written configuration.
    async fn make_quarantine(&self, host_git: &Path, quarantine: &Path) -> Result<(), Error> {
        remove(quarantine)?;
        self.run(&[
            "init".as_ref(),
            "-q".as_ref(),
            "--bare".as_ref(),
            "--template=".as_ref(),
            quarantine.as_os_str(),
        ])
        .await?;
        fs::write(quarantine.join("config"), STAGE_CONFIG)?;
        let mut alternate = host_git.join("objects").into_os_string().into_vec();
        alternate.push(b'\n');
        fs::create_dir_all(quarantine.join("objects/info"))?;
        fs::write(quarantine.join("objects/info/alternates"), alternate)?;
        Ok(())
    }

    /// Step 3: indexes `pack` into `ingest`, walks the tree that `tree` names,
    /// and moves the pack into `host_git` where nothing is rejected. `tree`
    /// and `pack` are what the staging step handed back, and untrusted.
    pub(crate) async fn ingest(
        &self,
        host_git: &Path,
        ingest: &Path,
        trees: Trees<'_>,
        tree: &[u8],
        pack: &[u8],
        limits: Limits,
    ) -> Result<Captured, Error> {
        absolute(host_git)?;
        absolute(ingest)?;
        let Some(name) = std::str::from_utf8(tree)
            .ok()
            .and_then(|text| Oid::parse(text).ok())
        else {
            return Ok(Captured::Rejected(vec![Rejection::Pack(format!(
                "{:?} is no object name",
                String::from_utf8_lossy(tree)
            ))]));
        };
        remove(ingest)?;
        fs::create_dir_all(ingest.join("pack"))?;
        let walked = self
            .walk(host_git, ingest, trees, &name, pack, limits)
            .await;
        let moved = match &walked {
            Ok(Captured::Tree(_)) => move_pack(ingest, &host_git.join("objects/pack")),
            _ => Ok(()),
        };
        let removed = remove(ingest);
        let captured = walked?;
        moved?;
        removed?;
        Ok(captured)
    }

    /// The checks of ingest, against `ingest` with `host_git`'s objects
    /// behind it.
    async fn walk(
        &self,
        host_git: &Path,
        ingest: &Path,
        trees: Trees<'_>,
        name: &Oid,
        pack: &[u8],
        limits: Limits,
    ) -> Result<Captured, Error> {
        let objects = host_git.join("objects");
        let env: [(&str, &OsStr); 2] = [
            ("GIT_OBJECT_DIRECTORY", ingest.as_os_str()),
            ("GIT_ALTERNATE_OBJECT_DIRECTORIES", objects.as_os_str()),
        ];
        let dir = option("--git-dir=", host_git);
        let max = format!("--max-input-size={}", limits.pack_bytes());
        let indexed = self
            .raw(
                &[
                    dir.as_os_str(),
                    "index-pack".as_ref(),
                    "--stdin".as_ref(),
                    "--strict".as_ref(),
                    max.as_ref(),
                ],
                &env,
                Some(pack),
                1024,
            )
            .await?;
        if !indexed.status.success() {
            return Ok(Captured::Rejected(vec![Rejection::Pack(indexed.stderr)]));
        }
        let kind = self
            .raw(
                &[
                    dir.as_os_str(),
                    "cat-file".as_ref(),
                    "-t".as_ref(),
                    name.as_str().as_ref(),
                ],
                &env,
                None,
                64,
            )
            .await?;
        if !kind.status.success() || kind.stdout != b"tree\n" {
            return Ok(Captured::Rejected(vec![Rejection::Pack(format!(
                "{name} is no tree"
            ))]));
        }
        let new = self.entries(&dir, &env, name).await?;
        let previous = self.entries(&dir, &env, trees.previous).await?;
        let base = self.entries(&dir, &env, trees.base).await?;
        let mut rejections = tree::modes(&new);
        rejections.extend(tree::same_folds(&new, &base));
        rejections.extend(tree::below_layouts(&new, &base));
        rejections.extend(tree::sizes(
            &new,
            &previous,
            limits.file_bytes,
            limits.candidate_bytes,
        ));
        if rejections.is_empty() {
            Ok(Captured::Tree(name.clone()))
        } else {
            Ok(Captured::Rejected(rejections))
        }
    }

    /// The entries of the tree `name` at every depth.
    async fn entries(
        &self,
        dir: &OsStr,
        env: &[(&str, &OsStr)],
        name: &Oid,
    ) -> Result<Tree, Error> {
        // Not `--format`: with it git quotes a path even under `-z` (P-7).
        let output = self
            .raw(
                &[
                    dir,
                    "ls-tree".as_ref(),
                    "-r".as_ref(),
                    "-t".as_ref(),
                    "-z".as_ref(),
                    "-l".as_ref(),
                    "--full-tree".as_ref(),
                    name.as_str().as_ref(),
                ],
                env,
                None,
                LISTING_CAP,
            )
            .await?
            .checked()?;
        parse_tree(&output.stdout)
    }

    /// Step 4: the candidate's commit in `host_git`.
    pub(crate) async fn commit_candidate(
        &self,
        host_git: &Path,
        candidate: &Candidate<'_>,
    ) -> Result<Oid, Error> {
        let identity = candidate.identity;
        if !identity.valid() {
            return Err(Error::Identity);
        }
        let date = format!("{} +0000", candidate.seconds);
        let env: [(&str, &OsStr); 6] = [
            ("GIT_AUTHOR_NAME", identity.name.as_ref()),
            ("GIT_AUTHOR_EMAIL", identity.email.as_ref()),
            ("GIT_AUTHOR_DATE", date.as_ref()),
            ("GIT_COMMITTER_NAME", identity.name.as_ref()),
            ("GIT_COMMITTER_EMAIL", identity.email.as_ref()),
            ("GIT_COMMITTER_DATE", date.as_ref()),
        ];
        let dir = option("--git-dir=", absolute(host_git)?);
        let mut args: Vec<&OsStr> = vec![
            dir.as_os_str(),
            "commit-tree".as_ref(),
            "--no-gpg-sign".as_ref(),
            candidate.tree.as_str().as_ref(),
        ];
        for parent in candidate.parents {
            args.push("-p".as_ref());
            args.push(parent.as_str().as_ref());
        }
        args.extend::<[&OsStr; 2]>(["-F".as_ref(), "-".as_ref()]);
        let output = self
            .raw(&args, &env, Some(candidate.message.as_bytes()), 1024)
            .await?
            .checked()?;
        let text = String::from_utf8_lossy(&output.stdout);
        Oid::parse(text.trim_end())
    }

    /// The export: `candidate` of `host_git` checked out into `export`, which
    /// this creates and which must not exist, through a temporary index
    /// beside it, with no attribute applied (9.4, WS-11, WS-14).
    pub(crate) async fn export(
        &self,
        host_git: &Path,
        candidate: &Oid,
        export: &Path,
    ) -> Result<(), Error> {
        absolute(export)?;
        fs::create_dir(export)?;
        let mut index = export.as_os_str().to_owned();
        index.push(".index");
        let index = PathBuf::from(index);
        remove(&index)?;
        let env: [(&str, &OsStr); 1] = [("GIT_INDEX_FILE", index.as_os_str())];
        let dir = option("--git-dir=", absolute(host_git)?);
        let work_tree = option("--work-tree=", export);
        let attributes = attr_source();
        let read = self
            .raw(
                &[
                    dir.as_os_str(),
                    attributes.as_os_str(),
                    "read-tree".as_ref(),
                    candidate.as_str().as_ref(),
                ],
                &env,
                None,
                0,
            )
            .await
            .and_then(Output::checked);
        let checked_out = match read {
            Ok(_) => self
                .raw(
                    &[
                        dir.as_os_str(),
                        work_tree.as_os_str(),
                        attributes.as_os_str(),
                        "checkout-index".as_ref(),
                        "-a".as_ref(),
                    ],
                    &env,
                    None,
                    LISTING_CAP,
                )
                .await
                .and_then(Output::checked),
            Err(error) => Err(error),
        };
        let removed = remove(&index);
        checked_out?;
        removed
    }
}

/// Moves the files of `ingest/pack` into `pack_dir`, the index last: a pack
/// is used once its index is there.
fn move_pack(ingest: &Path, pack_dir: &Path) -> Result<(), Error> {
    let files: Vec<PathBuf> = fs::read_dir(ingest.join("pack"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()?;
    for file in index_last(files) {
        if let Some(name) = file.file_name() {
            fs::rename(&file, pack_dir.join(name))?;
        }
    }
    Ok(())
}

/// `files` with every `.idx` file after the others.
fn index_last(mut files: Vec<PathBuf>) -> Vec<PathBuf> {
    files.sort_by_key(|file| file.extension() == Some(OsStr::new("idx")));
    files
}

/// Removes `path`, a directory or a file, where it exists.
fn remove(path: &Path) -> Result<(), Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path)?,
        Ok(_) => fs::remove_file(path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::{
        Identity, Inventory, Kind, Limits, Listing, Rejection, TreePath, index_last,
        parse_diff_files, parse_tree, remove,
    };

    #[test]
    fn the_default_limits_are_those_of_9_4() {
        assert_eq!(
            Limits::default(),
            Limits {
                file_bytes: 5 * 1024 * 1024,
                candidate_bytes: 50 * 1024 * 1024,
                staging: Duration::from_secs(300),
            }
        );
    }

    #[test]
    fn a_path_is_its_bytes_and_shows_as_text() {
        let path = TreePath::from(b"src/\xff.rs".as_slice());
        assert_eq!(path.as_bytes(), b"src/\xff.rs");
        assert_eq!(path.to_string(), "src/\u{FFFD}.rs");
        assert_eq!(
            Rejection::DotGit(path).to_string(),
            "src/\u{FFFD}.rs: a name that git treats as .git"
        );
    }

    #[test]
    fn an_identity_holds_no_angle_bracket_or_control_character() {
        let identity = |name: &str, email: &str| Identity {
            name: name.to_owned(),
            email: email.to_owned(),
        };
        assert!(identity("sf-factory[bot]", "1+bot@example.invalid").valid());
        for (name, email) in [
            ("a\nb", "c@d"),
            ("a", "c\0d"),
            ("a <b", "c@d"),
            ("a", "c>d"),
            (" ", "c@d"),
            ("a", ""),
        ] {
            assert!(!identity(name, email).valid(), "{name:?} {email:?}");
        }
    }

    #[test]
    fn a_listing_is_empty_only_without_deletions_and_other_paths() {
        let deleted = Listing {
            deleted: vec![b"gone".to_vec()],
            other: Vec::new(),
        };
        let other = Listing {
            deleted: Vec::new(),
            other: vec![b"new".to_vec()],
        };
        assert!(Listing::default().is_empty());
        assert!(!deleted.is_empty());
        assert!(!other.is_empty());
        let both = Listing {
            deleted: deleted.deleted,
            other: other.other,
        };
        assert_eq!(
            both.paths(),
            [
                TreePath::from(b"gone".as_slice()),
                TreePath::from(b"new".as_slice())
            ]
        );
    }

    #[test]
    fn the_listing_check_admits_a_file_and_a_change_set_at_their_limits() {
        let mut inventory = Inventory::new();
        inventory.insert(b"a".to_vec(), (Kind::File, 10));
        inventory.insert(b"b".to_vec(), (Kind::Symlink, 5));
        inventory.insert(b"dir".to_vec(), (Kind::Directory, 0));
        let listing = Listing {
            deleted: vec![b"gone".to_vec()],
            other: vec![b"a".to_vec(), b"b".to_vec()],
        };
        let limits = |file_bytes, candidate_bytes| Limits {
            file_bytes,
            candidate_bytes,
            staging: Duration::from_secs(1),
        };
        assert!(listing.check(&inventory, limits(10, 15)).is_empty());
        assert_eq!(
            listing.check(&inventory, limits(9, 15)),
            [Rejection::FileTooLarge {
                path: TreePath::from(b"a".as_slice()),
                bytes: 10
            }]
        );
        assert_eq!(
            listing.check(&inventory, limits(10, 14)),
            [Rejection::ChangeSetTooLarge { bytes: 15 }]
        );
        // A directory, and a nested repository as git lists it.
        let nested = Listing {
            deleted: Vec::new(),
            other: vec![b"dir".to_vec(), b"dir/".to_vec()],
        };
        assert_eq!(nested.check(&inventory, limits(10, 15)).len(), 2);
    }

    #[test]
    fn the_index_of_a_pack_moves_last() {
        let files = ["p.idx", "p.pack", "p.rev"].map(PathBuf::from).to_vec();
        assert_eq!(
            index_last(files),
            ["p.pack", "p.rev", "p.idx"].map(PathBuf::from).to_vec()
        );
    }

    #[test]
    fn removing_what_is_not_there_succeeds_and_any_other_failure_does_not() {
        let dir = std::env::temp_dir().join(format!("sf-remove-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("file");
        std::fs::write(&file, b"x").unwrap();
        assert!(remove(&dir.join("absent")).is_ok());
        // A path below a file is no directory entry, and no absent one.
        assert!(remove(&file.join("below")).is_err());
        remove(&dir).unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn a_raw_diff_is_read_as_deletions_and_other_paths() {
        let z = "0".repeat(40);
        let output = format!(
            ":100644 100644 {z} {z} M\0a\tb\0:100644 000000 {z} {z} D\0gone\0:120000 100644 {z} {z} T\0new\nline\0"
        );
        let listing = parse_diff_files(output.as_bytes(), "git diff-files").unwrap();
        assert_eq!(listing.deleted, [b"gone".to_vec()]);
        assert_eq!(listing.other, [b"a\tb".to_vec(), b"new\nline".to_vec()]);
        assert!(parse_diff_files(b":100644 M\0", "git diff-files").is_err());
    }

    #[test]
    fn a_tree_listing_is_read_with_raw_paths_and_sizes() {
        let a = "a".repeat(40);
        let output = format!("100644 blob {a}      12\ttab\there\0040000 tree {a}       -\tdir\0");
        let tree = parse_tree(output.as_bytes()).unwrap();
        assert_eq!(tree[b"tab\there".as_slice()].size, 12);
        assert_eq!(tree[b"dir".as_slice()].mode, "040000");
        assert!(parse_tree(b"100644 blob\0").is_err());
    }
}

//! The hostile-tree suite of WS-9: capture, ingest and export (SPEC 9.4)
//! with real `git`, against workspaces made hostile in one way each, and the
//! cases that probe P-7 added. A canary script that hostile configuration
//! and hooks name leaves a file behind if anything from a workspace runs.
//!
//! In M0 the staging git runs outside any sandbox; the capture stage of M1a
//! runs it in one sandbox instance (9.4). The tables of probe P-40 for WS-12
//! and WS-13 are M1a's (19.4): this suite holds one case of each rule.

mod common;

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{Scratch, Upstream, cli, git, git_dir, has_object, main, oid, program};
use factory_git::{
    Candidate, CaptureDirs, Captured, Error, Git as _, Identity, Limits, Oid, Rejection, Remote,
    TreePath, Trees,
};
use factory_proc::Command;

/// The base commit: a small crate with an ignore file, a tracked file that
/// an ignore rule matches, and an executable script.
const BASE: &[(&str, &[u8])] = &[
    ("README.md", b"# fixture\n"),
    ("src/lib.rs", b"pub fn f() {}\n"),
    ("src/main.rs", b"fn main() {}\n"),
    ("scripts/run.sh", b"#!/bin/sh\n"),
    (".gitignore", b"target/\n*.log\n"),
    ("build/keep.log", b"tracked although *.log is ignored\n"),
    ("docs/guide.md", b"guide\n"),
];

/// Limits small enough that a test writes no large file.
const LIMITS: Limits = Limits {
    file_bytes: 64 * 1024,
    candidate_bytes: 256 * 1024,
    staging: Duration::from_secs(30),
};

/// One run's directories (9.1), a workspace at the base, and a canary.
struct Run {
    scratch: Scratch,
    host: PathBuf,
    workspace: PathBuf,
    base: Oid,
    base_tree: Oid,
    /// The blob of another run's unmerged branch, which is in the mirror.
    other_blob: String,
    outside: PathBuf,
    canary: PathBuf,
}

impl Run {
    async fn new(name: &str, base: &[(&str, &[u8])]) -> Self {
        let scratch = Scratch::new(name);
        let upstream = Upstream::new(&scratch).await;
        let base_commit = upstream.commit("main", base).await;
        upstream
            .commit("factory/issue-9", &[("other.rs", b"another run's\n")])
            .await;
        let other_blob = git(&[
            git_dir(&upstream.bare).as_os_str(),
            "rev-parse".as_ref(),
            "factory/issue-9:other.rs".as_ref(),
        ])
        .await
        .remove(0);
        let mirror = scratch.join("mirror.git");
        let git = cli();
        git.fetch_mirror(
            &mirror,
            &Remote::path(&upstream.bare).unwrap_or_else(|error| panic!("{error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("{error}"));
        let host = scratch.join("host.git");
        git.create_host_repository(&mirror, &main(), &host)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let workspace = scratch.join("w");
        let base = oid(&base_commit);
        git.create_workspace(&mirror, &main(), &base, &workspace)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let base_tree = git
            .tree_of(&host, &base)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let outside = scratch.join("outside");
        let canary = outside.join("FIRED");
        write(
            &outside.join("canary.sh"),
            format!("#!/bin/sh\necho fired >> '{}'\n", canary.display()).as_bytes(),
        );
        chmod(&outside.join("canary.sh"), 0o755);
        write(&outside.join("dir/lib.rs"), b"OUTSIDE CONTENT\n");
        write(&outside.join("ignore-all"), b"*\n");
        Self {
            scratch,
            host,
            workspace,
            base,
            base_tree,
            other_blob,
            outside,
            canary,
        }
    }

    /// A run on [`BASE`].
    async fn standard(name: &str) -> Self {
        Self::new(name, BASE).await
    }

    fn p(&self, relative: &str) -> PathBuf {
        self.workspace.join(relative)
    }

    fn dirs(&self) -> (PathBuf, PathBuf) {
        (self.scratch.join("q"), self.scratch.join("i"))
    }

    fn trees(&self) -> Trees<'_> {
        Trees {
            previous: &self.base_tree,
            base: &self.base_tree,
        }
    }

    /// Captures the workspace from the base, with [`LIMITS`].
    async fn capture(&self) -> Result<Captured, Error> {
        self.capture_with(LIMITS).await
    }

    async fn capture_with(&self, limits: Limits) -> Result<Captured, Error> {
        let (quarantine, ingest) = self.dirs();
        cli()
            .capture(
                CaptureDirs {
                    workspace: &self.workspace,
                    host_git: &self.host,
                    quarantine: &quarantine,
                    ingest: &ingest,
                },
                self.trees(),
                limits,
            )
            .await
    }

    /// The tree that the capture built; fails the test on a rejection.
    async fn tree(&self) -> Oid {
        match self
            .capture()
            .await
            .unwrap_or_else(|error| panic!("{error}"))
        {
            Captured::Tree(tree) => tree,
            Captured::Rejected(rejections) => panic!("rejected: {rejections:?}"),
        }
    }

    /// The rejections of the capture; fails the test where it built a tree.
    async fn rejected(&self) -> Vec<Rejection> {
        match self
            .capture()
            .await
            .unwrap_or_else(|error| panic!("{error}"))
        {
            Captured::Tree(tree) => panic!("captured {tree}"),
            Captured::Rejected(rejections) => rejections,
        }
    }

    /// Commits `tree` on the base and exports the commit into a new
    /// directory, whose path it returns.
    async fn export(&self, tree: &Oid, name: &str) -> PathBuf {
        let commit = self.commit(tree).await;
        let export = self.scratch.join(name);
        cli()
            .export(&self.host, &commit, &export)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        export
    }

    async fn commit(&self, tree: &Oid) -> Oid {
        cli()
            .commit_candidate(
                &self.host,
                &Candidate {
                    tree,
                    parents: std::slice::from_ref(&self.base),
                    message: "fix: a candidate\n\nFactory-Issue: 130\n",
                    identity: &identity(),
                    seconds: 1_795_000_000,
                },
            )
            .await
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Hostile configuration that names the canary for every command git
    /// could run from it.
    fn hostile_config(&self) -> String {
        let canary = self.outside.join("canary.sh");
        let c = canary.display();
        let o = self.outside.display();
        format!(
            "[core]\n\tfsmonitor = {c}\n\thooksPath = {o}/hooks\n\tpager = {c}\n\
             \tattributesFile = {o}/attrs\n\texcludesFile = {o}/ignore-all\n\
             \tsshCommand = {c}\n\teditor = {c}\n\
             [filter \"evil\"]\n\tclean = {c}\n\tsmudge = {c}\n\tprocess = {c}\n\trequired = true\n\
             [diff]\n\texternal = {c}\n[alias]\n\tstatus = !{c}\n\tread-tree = !{c}\n"
        )
    }

    /// A repository at `dir` with a commit, whose configuration and hooks
    /// name the canary.
    async fn nested_repository(&self, dir: &Path) {
        fs::create_dir_all(dir).unwrap_or_else(|error| panic!("{error}"));
        git(&[
            "init".as_ref(),
            "-q".as_ref(),
            "--template=".as_ref(),
            dir.as_os_str(),
        ])
        .await;
        write(&dir.join("inner.txt"), b"inner\n");
        let inner_dir = git_dir(&dir.join(".git"));
        let mut tree = OsString::from("--work-tree=");
        tree.push(dir);
        git(&[
            inner_dir.as_os_str(),
            tree.as_os_str(),
            "add".as_ref(),
            "inner.txt".as_ref(),
        ])
        .await;
        git(&[
            inner_dir.as_os_str(),
            tree.as_os_str(),
            "commit".as_ref(),
            "-q".as_ref(),
            "-m".as_ref(),
            "inner".as_ref(),
        ])
        .await;
        self.make_hostile(&dir.join(".git"));
    }

    /// Appends the hostile configuration to `git_dir/config` and writes hooks
    /// that run the canary.
    fn make_hostile(&self, git_dir: &Path) {
        let mut config = fs::read(git_dir.join("config")).unwrap_or_default();
        config.extend_from_slice(self.hostile_config().as_bytes());
        write(&git_dir.join("config"), &config);
        for hook in [
            "post-index-change",
            "pre-commit",
            "reference-transaction",
            "post-checkout",
            "fsmonitor-watchman",
        ] {
            let path = git_dir.join("hooks").join(hook);
            write(
                &path,
                format!(
                    "#!/bin/sh\nexec '{}'\n",
                    self.outside.join("canary.sh").display()
                )
                .as_bytes(),
            );
            chmod(&path, 0o755);
        }
    }

    /// Whether anything ran the canary.
    fn fired(&self) -> bool {
        self.canary.exists()
    }

    /// The entries of `tree` in `host.git`, by path, as (mode, object).
    async fn entries(&self, tree: &Oid) -> BTreeMap<String, (String, String)> {
        entries(&self.host, tree).await
    }

    /// The bytes of `path` in `tree`.
    async fn blob(&self, tree: &Oid, path: &str) -> Vec<u8> {
        let spec = format!("{tree}:{path}");
        let lines = git(&[
            git_dir(&self.host).as_os_str(),
            "cat-file".as_ref(),
            "blob".as_ref(),
            spec.as_ref(),
        ])
        .await;
        let mut bytes = lines.join("\n").into_bytes();
        bytes.push(b'\n');
        bytes
    }

    /// The files of `host.git/objects/pack`.
    fn packs(&self) -> Vec<OsString> {
        let mut names: Vec<_> = fs::read_dir(self.host.join("objects/pack"))
            .unwrap_or_else(|error| panic!("{error}"))
            .map(|entry| entry.unwrap_or_else(|error| panic!("{error}")).file_name())
            .collect();
        names.sort();
        names
    }
}

fn identity() -> Identity {
    Identity {
        name: "sf-factory[bot]".to_owned(),
        email: "1+sf-factory[bot]@users.noreply.github.com".to_owned(),
    }
}

fn write(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|error| panic!("{error}"));
    }
    fs::write(path, bytes).unwrap_or_else(|error| panic!("{error}"));
}

fn chmod(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .unwrap_or_else(|error| panic!("{error}"));
}

/// Makes a FIFO at `path`.
async fn mkfifo(path: &Path) {
    let mut command = Command::new("/usr/bin/mkfifo");
    command.arg(path);
    let status = command
        .spawn(tokio::io::sink())
        .await
        .unwrap_or_else(|error| panic!("{error}"))
        .wait()
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(status.success());
}

/// The entries of `tree` in `repository`, by path, as (mode, object): paths
/// that are UTF-8 and hold no line break.
async fn entries(repository: &Path, tree: &Oid) -> BTreeMap<String, (String, String)> {
    let lines = git(&[
        git_dir(repository).as_os_str(),
        "ls-tree".as_ref(),
        "-r".as_ref(),
        "-z".as_ref(),
        "--full-tree".as_ref(),
        tree.as_str().as_ref(),
    ])
    .await;
    let output = lines.join("\n");
    output
        .split('\0')
        .filter(|record| !record.is_empty())
        .map(|record| {
            let (meta, path) = record
                .split_once('\t')
                .unwrap_or_else(|| panic!("{record:?}"));
            let mut fields = meta.split(' ').map(str::to_owned);
            let (Some(mode), Some(_), Some(object)) = (fields.next(), fields.next(), fields.next())
            else {
                panic!("{record:?}");
            };
            (path.to_owned(), (mode, object))
        })
        .collect()
}

/// Whether `path`, as bytes, is in `tree` of `repository`.
async fn in_tree(repository: &Path, tree: &Oid, path: &[u8]) -> bool {
    let mut spec = format!("{tree}:").into_bytes();
    spec.extend_from_slice(path);
    let mut command = Command::new(program());
    command
        .arg(git_dir(repository))
        .args(["cat-file".as_ref(), "-e".as_ref(), OsStr::from_bytes(&spec)])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    command
        .spawn(tokio::io::sink())
        .await
        .unwrap_or_else(|error| panic!("{error}"))
        .wait()
        .await
        .unwrap_or_else(|error| panic!("{error}"))
        .success()
}

/// Every file and symbolic link below `root` but `.git`, by its path, with
/// its bytes or its target and whether it is executable.
fn contents(root: &Path) -> BTreeMap<Vec<u8>, (Vec<u8>, bool)> {
    let mut found = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap_or_else(|error| panic!("{error}")) {
            let path = entry.unwrap_or_else(|error| panic!("{error}")).path();
            let relative = path
                .strip_prefix(root)
                .unwrap_or_else(|error| panic!("{error}"))
                .as_os_str()
                .as_bytes()
                .to_vec();
            if relative == b".git" {
                continue;
            }
            let metadata = fs::symlink_metadata(&path).unwrap_or_else(|error| panic!("{error}"));
            if metadata.is_symlink() {
                let target = fs::read_link(&path).unwrap_or_else(|error| panic!("{error}"));
                found.insert(relative, (target.as_os_str().as_bytes().to_vec(), false));
            } else if metadata.is_dir() {
                pending.push(path);
            } else {
                let executable = metadata.permissions().mode() & 0o100 != 0;
                found.insert(
                    relative,
                    (
                        fs::read(&path).unwrap_or_else(|error| panic!("{error}")),
                        executable,
                    ),
                );
            }
        }
    }
    found
}

/// Asserts that `export` holds every path of `tree` with the bytes, target
/// and executable bit that it has in the workspace, and nothing else (WS-11).
async fn assert_export_is_workspace(run: &Run, tree: &Oid, export: &Path) {
    let exported = contents(export);
    let workspace = contents(&run.workspace);
    let mut paths = Vec::new();
    for path in exported.keys() {
        assert!(
            in_tree(&run.host, tree, path).await,
            "{path:?} is no path of the tree"
        );
        paths.push(path);
    }
    // Every file and link of the tree is there.
    for path in run.entries(tree).await.keys() {
        assert!(
            exported.contains_key(path.as_bytes()),
            "{path} was not exported"
        );
    }
    for path in paths {
        assert_eq!(
            exported.get(path),
            workspace.get(path),
            "{}",
            String::from_utf8_lossy(path)
        );
    }
}

fn path(text: &str) -> TreePath {
    TreePath::from(text.as_bytes())
}

// ---------------------------------------------------------------------------
// What a candidate holds
// ---------------------------------------------------------------------------

#[test]
fn an_untouched_workspace_is_an_empty_change_set() {
    common::within(async {
        let run = Run::standard("untouched").await;
        let packs = run.packs();
        let tree = run.tree().await;
        assert_eq!(tree, run.base_tree);
        assert!(!run.fired());
        // The empty pack that ingest moved holds no object; the pack of the
        // clone is still there.
        assert!(packs.iter().all(|pack| run.packs().contains(pack)));
    });
}

#[test]
fn a_base_whose_attributes_change_bytes_is_an_empty_change_set_and_exports_its_blobs() {
    common::within(async {
        // `eol`, `ident`, `working-tree-encoding` and a filter: the workspace
        // checkout, the staging and the export apply none of them (WS-11).
        // The seed commits `utf16.txt` through its attribute, so the blob
        // holds UTF-8.
        let base: &[(&str, &[u8])] = &[
            (
                ".gitattributes",
                b"crlf.txt text eol=crlf\nid.txt ident\nfilter.txt filter=hostile\n\
                  utf16.txt working-tree-encoding=UTF-16\n",
            ),
            ("crlf.txt", b"one\ntwo\n"),
            ("id.txt", b"$Id$\n"),
            ("filter.txt", b"plain\n"),
            ("utf16.txt", b"\xff\xfeh\x00i\x00\n\x00"),
        ];
        let run = Run::new("base-attributes", base).await;
        let tree = run.tree().await;
        assert_eq!(tree, run.base_tree);
        let export = run.export(&tree, "e").await;
        let mut expected: BTreeMap<Vec<u8>, (Vec<u8>, bool)> = base
            .iter()
            .map(|(path, bytes)| (path.as_bytes().to_vec(), (bytes.to_vec(), false)))
            .collect();
        expected.insert(b"utf16.txt".to_vec(), (b"hi\n".to_vec(), false));
        assert_eq!(contents(&export), expected);
        assert_eq!(fs::read(run.p("utf16.txt")).unwrap(), b"hi\n");
    });
}

#[test]
fn a_change_set_of_edits_additions_deletions_links_and_swaps_is_captured() {
    common::within(async {
        let run = Run::standard("benign").await;
        write(&run.p("src/lib.rs"), b"pub fn changed() {}\n");
        write(&run.p("src/new.rs"), b"pub fn new() {}\n");
        fs::remove_file(run.p("docs/guide.md")).unwrap();
        symlink("lib.rs", run.p("src/link.rs")).unwrap();
        write(&run.p("target/debug/app"), b"ignored build output\n");
        write(
            &run.p("build/keep.log"),
            b"tracked, matches *.log, modified\n",
        );
        write(&run.p("build/new.log"), b"untracked, matches *.log\n");
        fs::create_dir(run.p("emptydir")).unwrap();
        // A tracked file becomes a directory, and a tracked directory a file.
        fs::remove_file(run.p("README.md")).unwrap();
        write(&run.p("README.md/index.md"), b"# now a directory\n");
        fs::remove_dir_all(run.p("scripts")).unwrap();
        write(&run.p("scripts"), b"now a file\n");

        let tree = run.tree().await;

        let entries = run.entries(&tree).await;
        for (path, present) in [
            ("src/new.rs", true),
            ("src/link.rs", true),
            ("README.md/index.md", true),
            ("scripts", true),
            ("docs/guide.md", false),
            ("target/debug/app", false),
            ("build/new.log", false),
            ("scripts/run.sh", false),
        ] {
            assert_eq!(entries.contains_key(path), present, "{path}");
        }
        assert_eq!(entries["src/link.rs"].0, "120000");
        // An ignore rule never drops a tracked path.
        assert_eq!(
            run.blob(&tree, "build/keep.log").await,
            b"tracked, matches *.log, modified\n"
        );
        let export = run.export(&tree, "e").await;
        assert_export_is_workspace(&run, &tree, &export).await;
        assert!(!export.join("emptydir").exists());
    });
}

#[test]
fn executable_bits_are_captured_in_both_directions() {
    common::within(async {
        let run = Run::standard("modes").await;
        chmod(&run.p("README.md"), 0o755);
        chmod(&run.p("scripts/run.sh"), 0o644);
        write(&run.p("scripts/new.sh"), b"#!/bin/sh\n");
        chmod(&run.p("scripts/new.sh"), 0o755);
        write(&run.p("scripts/setuid.sh"), b"#!/bin/sh\n");
        chmod(&run.p("scripts/setuid.sh"), 0o4755);
        write(&run.p("scripts/world.sh"), b"#!/bin/sh\n");
        chmod(&run.p("scripts/world.sh"), 0o666);

        let tree = run.tree().await;

        let entries = run.entries(&tree).await;
        for (path, mode) in [
            ("README.md", "100755"),
            ("scripts/run.sh", "100644"),
            ("scripts/new.sh", "100755"),
            ("scripts/setuid.sh", "100755"),
            ("scripts/world.sh", "100644"),
        ] {
            assert_eq!(entries[path].0, mode, "{path}");
        }
        let export = run.export(&tree, "e").await;
        assert_export_is_workspace(&run, &tree, &export).await;
    });
}

#[test]
fn a_hostile_root_dot_git_is_never_read() {
    common::within(async {
        let run = Run::standard("root-git").await;
        let git_dir = run.p(".git");
        run.make_hostile(&git_dir);
        write(&git_dir.join("info/exclude"), b"*\n");
        write(&git_dir.join("info/attributes"), b"* filter=evil\n");
        mkfifo(&git_dir.join("fifo")).await;
        write(&run.p("src/new.rs"), b"pub fn new() {}\n");

        let tree = run.tree().await;

        // The workspace's `info/exclude` ignores everything, and is not read.
        assert!(run.entries(&tree).await.contains_key("src/new.rs"));
        assert!(!run.fired());
    });
}

#[test]
fn a_root_dot_git_replaced_by_a_gitfile_is_never_read() {
    common::within(async {
        let run = Run::standard("root-gitfile").await;
        let elsewhere = run.outside.join("elsewhere");
        run.nested_repository(&elsewhere).await;
        fs::remove_dir_all(run.p(".git")).unwrap();
        write(
            &run.p(".git"),
            format!("gitdir: {}\n", elsewhere.join(".git").display()).as_bytes(),
        );
        write(&run.p("src/new.rs"), b"pub fn new() {}\n");

        let tree = run.tree().await;

        assert!(run.entries(&tree).await.contains_key("src/new.rs"));
        assert!(!run.fired());
    });
}

#[test]
fn symbolic_links_to_directories_are_links_and_nothing_behind_them_is_read() {
    common::within(async {
        let run = Run::standard("symlinks").await;
        symlink("/etc", run.p("etc-link")).unwrap();
        symlink(run.outside.join("dir"), run.p("outside-link")).unwrap();
        // A tracked directory becomes a link to a directory that holds a file
        // of the same name as the one that was tracked there.
        fs::remove_dir_all(run.p("src")).unwrap();
        symlink(run.outside.join("dir"), run.p("src")).unwrap();
        symlink("../docs", run.p("scripts/docs-link")).unwrap();
        symlink("nowhere", run.p("dangling")).unwrap();

        let tree = run.tree().await;

        let entries = run.entries(&tree).await;
        for link in [
            "etc-link",
            "outside-link",
            "src",
            "scripts/docs-link",
            "dangling",
        ] {
            assert_eq!(entries[link].0, "120000", "{link}");
        }
        assert!(
            !entries
                .keys()
                .any(|path| path.starts_with("src/") || path.starts_with("etc-link/"))
        );
        // The content behind the links entered no object.
        let behind = git(&[
            "hash-object".as_ref(),
            run.outside.join("dir/lib.rs").as_os_str(),
        ])
        .await
        .remove(0);
        assert!(!has_object(&run.host, &behind).await);
        let export = run.export(&tree, "e").await;
        assert_export_is_workspace(&run, &tree, &export).await;
    });
}

#[test]
fn hard_links_are_two_files_with_one_blob() {
    common::within(async {
        let run = Run::standard("hardlinks").await;
        write(&run.p("src/a.rs"), b"linked\n");
        fs::hard_link(run.p("src/a.rs"), run.p("src/b.rs")).unwrap();
        fs::hard_link(run.outside.join("dir/lib.rs"), run.p("src/outside.rs")).unwrap();

        let tree = run.tree().await;

        let entries = run.entries(&tree).await;
        assert_eq!(entries["src/a.rs"].1, entries["src/b.rs"].1);
        assert!(entries.contains_key("src/outside.rs"));
        let export = run.export(&tree, "e").await;
        let metadata = fs::symlink_metadata(export.join("src/b.rs")).unwrap();
        assert_eq!(std::os::unix::fs::MetadataExt::nlink(&metadata), 1);
        assert_export_is_workspace(&run, &tree, &export).await;
    });
}

#[test]
fn attributes_that_a_round_writes_apply_to_neither_staging_nor_export() {
    common::within(async {
        let run = Run::standard("attributes").await;
        write(
            &run.p(".gitattributes"),
            b"*.txt filter=evil\ncrlf.txt text eol=crlf\nutf16.txt working-tree-encoding=UTF-16\n\
              id.txt ident\nsecret.txt export-ignore\n*.rs -diff\n\
              *.bin filter=lfs diff=lfs merge=lfs -text\n",
        );
        // The filter driver is defined in the workspace's own configuration.
        run.make_hostile(&run.p(".git"));
        write(&run.p("crlf.txt"), b"one\r\ntwo\r\n");
        write(&run.p("utf16.txt"), b"\xff\xfeh\x00\xe9\x00\n\x00");
        write(
            &run.p("id.txt"),
            b"$Id: 0123456789012345678901234567890123456789 $\n",
        );
        write(&run.p("secret.txt"), b"must reach the export\n");
        write(&run.p("src/lib.rs"), b"pub fn changed() {}\n");
        let pointer = format!(
            "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize 12345\n",
            "0".repeat(64)
        );
        write(&run.p("asset.bin"), pointer.as_bytes());

        let tree = run.tree().await;

        // Each file has its workspace bytes in the blob and in the export:
        // CRLF, UTF-16, the expanded `$Id$` and the LFS pointer as written,
        // and the file marked `export-ignore` is there.
        let export = run.export(&tree, "e").await;
        assert_export_is_workspace(&run, &tree, &export).await;
        assert_eq!(
            fs::read(export.join("crlf.txt")).unwrap(),
            b"one\r\ntwo\r\n"
        );
        assert!(export.join("secret.txt").exists());
        assert_eq!(
            fs::read(export.join("asset.bin")).unwrap(),
            pointer.as_bytes()
        );
        assert!(!run.fired());
    });
}

#[test]
fn an_agent_edited_gitignore_is_the_only_source_of_ignore_rules() {
    common::within(async {
        let run = Run::standard("gitignore").await;
        // Drops `target/`, adds `hidden/` and un-ignores one log.
        write(&run.p(".gitignore"), b"*.log\nhidden/\n!build/shown.log\n");
        write(&run.p("hidden/tool.rs"), b"exists in the workspace only\n");
        write(&run.p("target/debug/app"), b"no longer ignored\n");
        write(&run.p("build/shown.log"), b"un-ignored\n");
        // A directory that hides itself, its own ignore file included.
        write(&run.p("vendor/self/.gitignore"), b"*\n");
        write(&run.p("vendor/self/blob.rs"), b"hidden\n");
        write(
            &run.p("build/keep.log"),
            b"tracked, still matches, modified\n",
        );

        let tree = run.tree().await;

        let entries = run.entries(&tree).await;
        for (path, present) in [
            (".gitignore", true),
            ("target/debug/app", true),
            ("build/shown.log", true),
            ("build/keep.log", true),
            ("hidden/tool.rs", false),
            ("vendor/self/.gitignore", false),
            ("vendor/self/blob.rs", false),
        ] {
            assert_eq!(entries.contains_key(path), present, "{path}");
        }
        assert_eq!(
            run.blob(&tree, "build/keep.log").await,
            b"tracked, still matches, modified\n"
        );
    });
}

#[test]
fn a_gitignore_that_links_outside_is_not_followed() {
    common::within(async {
        let run = Run::standard("gitignore-link").await;
        fs::remove_file(run.p(".gitignore")).unwrap();
        symlink(run.outside.join("ignore-all"), run.p(".gitignore")).unwrap();
        write(&run.p("src/new.rs"), b"pub fn new() {}\n");

        let tree = run.tree().await;

        let entries = run.entries(&tree).await;
        assert!(entries.contains_key("src/new.rs"));
        assert_eq!(entries[".gitignore"].0, "120000");
    });
}

#[test]
fn names_with_newlines_dashes_tabs_and_quotes_are_captured_and_exported() {
    common::within(async {
        let run = Run::standard("names").await;
        let mut names: Vec<Vec<u8>> = [
            "new\nline.txt",
            "-rf",
            "--stdin",
            "tab\tname",
            "quo\"te",
            " lead",
        ]
        .iter()
        .map(|name| name.as_bytes().to_vec())
        .collect();
        names.push(vec![b'x'; 255]);
        // A name that is no UTF-8, where the file system takes one: APFS does
        // not.
        let raw = b"\xff\xfe.bin".to_vec();
        let odd = run.p("odd");
        fs::create_dir_all(&odd).unwrap();
        if fs::write(odd.join(OsStr::from_bytes(&raw)), b"x\n").is_ok() {
            names.push(raw);
        }
        for name in &names {
            fs::write(odd.join(OsStr::from_bytes(name)), b"x\n").unwrap();
        }

        let tree = run.tree().await;

        for name in &names {
            let mut path = b"odd/".to_vec();
            path.extend_from_slice(name);
            assert!(in_tree(&run.host, &tree, &path).await, "{name:?}");
        }
        let export = run.export(&tree, "e").await;
        assert_eq!(contents(&export.join("odd")).len(), names.len());
    });
}

#[test]
fn a_file_in_an_ignored_directory_over_the_limit_is_not_listed() {
    common::within(async {
        let run = Run::standard("large-ignored").await;
        write(&run.p("target/big.bin"), &vec![1; 100 * 1024]);
        assert_eq!(run.tree().await, run.base_tree);
    });
}

#[test]
fn a_capture_builds_on_the_previous_candidate() {
    common::within(async {
        let run = Run::standard("rounds").await;
        write(&run.p("src/new.rs"), b"first round\n");
        let first = run.tree().await;
        write(&run.p("src/new.rs"), b"second round\n");
        let (quarantine, ingest) = run.dirs();

        let second = cli()
            .capture(
                CaptureDirs {
                    workspace: &run.workspace,
                    host_git: &run.host,
                    quarantine: &quarantine,
                    ingest: &ingest,
                },
                Trees {
                    previous: &first,
                    base: &run.base_tree,
                },
                LIMITS,
            )
            .await
            .unwrap();

        let Captured::Tree(second) = second else {
            panic!("{second:?}");
        };
        assert_eq!(run.blob(&second, "src/new.rs").await, b"second round\n");
        assert!(!ingest.exists());
    });
}

// ---------------------------------------------------------------------------
// The scan (step 1)
// ---------------------------------------------------------------------------

#[test]
fn a_nested_repository_is_rejected_by_the_scan_and_runs_nothing() {
    common::within(async {
        let run = Run::standard("nested").await;
        run.nested_repository(&run.p("vendor/nested")).await;
        assert_eq!(
            run.rejected().await,
            [Rejection::DotGit(path("vendor/nested/.git"))]
        );
        assert!(!run.fired());
    });
}

#[test]
fn a_nested_gitfile_and_a_dot_git_link_are_rejected_by_the_scan() {
    common::within(async {
        let run = Run::standard("nested-gitfile").await;
        let elsewhere = run.outside.join("inner");
        run.nested_repository(&elsewhere).await;
        write(
            &run.p("vendor/a/.git"),
            format!("gitdir: {}\n", elsewhere.join(".git").display()).as_bytes(),
        );
        fs::create_dir_all(run.p("vendor/b")).unwrap();
        symlink(elsewhere.join(".git"), run.p("vendor/b/.git")).unwrap();
        // A `.git` file that names no repository, which git would leave out
        // of its listing without a message.
        write(&run.p("vendor/c/.git"), b"gitdir: /nonexistent\n");
        write(&run.p("vendor/c/inner.txt"), b"inner\n");
        let mut rejections = run.rejected().await;
        rejections.sort();
        assert_eq!(
            rejections,
            [
                Rejection::DotGit(path("vendor/a/.git")),
                Rejection::DotGit(path("vendor/b/.git")),
                Rejection::DotGit(path("vendor/c/.git")),
            ]
        );
        assert!(!run.fired());
    });
}

#[test]
fn every_spelling_of_dot_git_is_rejected_by_the_scan() {
    common::within(async {
        let run = Run::standard("spellings").await;
        for (dir, name) in [
            ("a", ".GIT"),
            ("b", ".git "),
            ("c", "git~1"),
            ("d", ".g\u{200C}it"),
        ] {
            let repository = run.p(&format!("vendor/{dir}"));
            run.nested_repository(&repository).await;
            fs::rename(repository.join(".git"), repository.join(name)).unwrap();
        }
        write(&run.p("vendor/x\\.git\\config"), b"[core]\n");
        let mut rejections = run.rejected().await;
        rejections.sort();
        assert_eq!(
            rejections,
            [
                Rejection::DotGit(path("vendor/a/.GIT")),
                Rejection::DotGit(path("vendor/b/.git ")),
                Rejection::DotGit(path("vendor/c/git~1")),
                Rejection::DotGit(path("vendor/d/.g\u{200C}it")),
                Rejection::DotGit(path("vendor/x\\.git\\config")),
            ]
        );
        assert!(!run.fired());
    });
}

#[test]
fn a_nested_repository_in_an_ignored_directory_is_rejected_by_the_scan() {
    common::within(async {
        let run = Run::standard("nested-ignored").await;
        run.nested_repository(&run.p("target/nested")).await;
        assert_eq!(
            run.rejected().await,
            [Rejection::DotGit(path("target/nested/.git"))]
        );
    });
}

#[test]
fn fifos_and_sockets_are_rejected_by_the_scan_and_nothing_blocks() {
    common::within(async {
        let run = Run::standard("fifos").await;
        mkfifo(&run.p("pipe")).await;
        fs::create_dir_all(run.p("target")).unwrap();
        mkfifo(&run.p("target/pipe")).await;
        // git blocks on a `.gitignore` that is a FIFO (P-7).
        fs::remove_file(run.p(".gitignore")).unwrap();
        mkfifo(&run.p(".gitignore")).await;
        fs::remove_file(run.p("src/lib.rs")).unwrap();
        mkfifo(&run.p("src/lib.rs")).await;
        let _socket = std::os::unix::net::UnixListener::bind(run.p("s")).unwrap();
        let mut rejections = run.rejected().await;
        rejections.sort();
        assert_eq!(
            rejections,
            [
                Rejection::SpecialFile(path(".gitignore")),
                Rejection::SpecialFile(path("pipe")),
                Rejection::SpecialFile(path("s")),
                Rejection::SpecialFile(path("src/lib.rs")),
                Rejection::SpecialFile(path("target/pipe")),
            ]
        );
    });
}

// ---------------------------------------------------------------------------
// The listing and the staging (step 2)
// ---------------------------------------------------------------------------

#[test]
fn a_file_over_the_limit_is_rejected_by_its_lstat_size() {
    common::within(async {
        let run = Run::standard("large").await;
        write(&run.p("big.bin"), &vec![1; 100 * 1024]);
        // A sparse file of 1 TiB, which nothing reads.
        let huge = fs::File::create(run.p("huge.bin")).unwrap();
        huge.set_len(1 << 40).unwrap();
        let rejections = run.rejected().await;
        assert!(rejections.contains(&Rejection::FileTooLarge {
            path: path("big.bin"),
            bytes: 100 * 1024
        }));
        assert!(rejections.contains(&Rejection::FileTooLarge {
            path: path("huge.bin"),
            bytes: 1 << 40
        }));
    });
}

#[test]
fn a_change_set_over_the_limit_is_rejected() {
    common::within(async {
        let run = Run::standard("total").await;
        for i in 0..5 {
            write(&run.p(&format!("data/{i}.bin")), &vec![1; 60 * 1024]);
        }
        assert_eq!(
            run.rejected().await,
            [Rejection::ChangeSetTooLarge {
                bytes: 5 * 60 * 1024
            }]
        );
    });
}

#[test]
fn a_file_that_the_staging_git_cannot_read_fails_the_capture() {
    common::within(async {
        let run = Run::standard("unreadable").await;
        let locked = run.p("src/locked.rs");
        write(&locked, b"cannot read\n");
        chmod(&locked, 0o000);
        if fs::read(&locked).is_ok() {
            // A user that reads every file, as root does, cannot test it.
            return;
        }
        let rejections = run.rejected().await;
        assert!(
            matches!(rejections.as_slice(), [Rejection::Staging { command, .. }] if command == "git update-index"),
            "{rejections:?}"
        );
        chmod(&locked, 0o644);
    });
}

#[test]
fn a_staging_step_over_its_time_is_an_error() {
    common::within(async {
        let run = Run::standard("timeout").await;
        let result = run
            .capture_with(Limits {
                staging: Duration::ZERO,
                ..LIMITS
            })
            .await;
        assert!(matches!(result, Err(Error::TimedOut(_))), "{result:?}");
    });
}

// ---------------------------------------------------------------------------
// The ingest (step 3): WS-12, WS-13 and a subverted staging git
// ---------------------------------------------------------------------------

#[test]
fn a_repository_layout_that_a_round_brings_in_is_rejected() {
    common::within(async {
        // The layout of a bare repository under another name, with a
        // configuration that names the canary (P-7, P-40).
        let run = Run::standard("layout").await;
        write(&run.p("tests/fixture/HEAD"), b"ref: refs/heads/main\n");
        write(
            &run.p("tests/fixture/config"),
            run.hostile_config().as_bytes(),
        );
        write(&run.p("tests/fixture/objects/keep"), b"");
        write(&run.p("tests/fixture/refs/keep"), b"");
        write(&run.p("src/lib.rs"), b"pub fn changed() {}\n");
        let packs = run.packs();

        let mut rejections = run.rejected().await;
        rejections.sort();

        let below = |file: &str| Rejection::BelowLayout {
            path: path(file),
            layout: path("tests/fixture"),
        };
        assert_eq!(
            rejections,
            [
                below("tests/fixture/HEAD"),
                below("tests/fixture/config"),
                below("tests/fixture/objects/keep"),
                below("tests/fixture/refs/keep"),
            ]
        );
        assert_eq!(run.packs(), packs);
        assert!(!run.scratch.join("i").exists());
        assert!(!run.fired());
    });
}

#[test]
fn a_layout_of_the_base_tree_admits_no_change_below_it() {
    common::within(async {
        let mut base = BASE.to_vec();
        base.extend_from_slice(&[
            ("fixture/HEAD", b"ref: refs/heads/main\n".as_slice()),
            ("fixture/config", b"[core]\n"),
            ("fixture/objects/keep", b""),
            ("fixture/refs/keep", b""),
        ]);
        let run = Run::new("layout-base", &base).await;
        write(&run.p("src/lib.rs"), b"pub fn changed() {}\n");
        // A change elsewhere passes.
        run.tree().await;
        // A change to its configuration does not.
        write(&run.p("fixture/config"), run.hostile_config().as_bytes());
        assert_eq!(
            run.rejected().await,
            [Rejection::BelowLayout {
                path: path("fixture/config"),
                layout: path("fixture"),
            }]
        );
        // The directory removed whole passes.
        fs::remove_dir_all(run.p("fixture")).unwrap();
        let tree = run.tree().await;
        assert!(!run.entries(&tree).await.contains_key("fixture/HEAD"));
    });
}

/// A bare repository whose objects are written apart from `host.git`, with
/// `host.git`'s objects behind it: what a subverted staging git could make.
struct Forge {
    dir: PathBuf,
}

impl Forge {
    async fn new(run: &Run) -> Self {
        let dir = run.scratch.join("forge.git");
        git(&[
            "init".as_ref(),
            "-q".as_ref(),
            "--bare".as_ref(),
            "--template=".as_ref(),
            dir.as_os_str(),
        ])
        .await;
        write(
            &dir.join("objects/info/alternates"),
            format!("{}\n", run.host.join("objects").display()).as_bytes(),
        );
        Self { dir }
    }

    async fn blob(&self, bytes: &[u8]) -> String {
        let file = self.dir.join("blob-input");
        fs::write(&file, bytes).unwrap_or_else(|error| panic!("{error}"));
        git(&[
            git_dir(&self.dir).as_os_str(),
            "hash-object".as_ref(),
            "-w".as_ref(),
            file.as_os_str(),
        ])
        .await
        .remove(0)
    }

    /// A tree of `entries` (mode, name, object), objects that it lacks
    /// allowed.
    async fn tree(&self, entries: &[(&str, &str, &str)]) -> String {
        let spec: String = entries
            .iter()
            .map(|(mode, name, object)| {
                let kind = match *mode {
                    "040000" => "tree",
                    "160000" => "commit",
                    _ => "blob",
                };
                format!("{mode} {kind} {object}\t{name}\n")
            })
            .collect();
        let file = self.dir.join("tree-input");
        fs::write(&file, spec).unwrap_or_else(|error| panic!("{error}"));
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("\"$0\" --git-dir=\"$1\" mktree --missing < \"$2\"")
            .arg(program())
            .arg(&self.dir)
            .arg(&file)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null");
        let mut process = command
            .spawn(tokio::io::sink())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let Some(factory_proc::Line::Text(tree)) = process
            .next_line()
            .await
            .unwrap_or_else(|error| panic!("{error}"))
        else {
            panic!("mktree printed no tree");
        };
        assert!(
            process
                .wait()
                .await
                .unwrap_or_else(|error| panic!("{error}"))
                .success()
        );
        tree
    }

    /// A pack of exactly `objects`.
    async fn pack(&self, objects: &[&str]) -> Vec<u8> {
        let list = self.dir.join("pack-input");
        fs::write(
            &list,
            objects.iter().map(|o| format!("{o}\n")).collect::<String>(),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let base = self.dir.join("forged");
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("\"$0\" --git-dir=\"$1\" pack-objects -q \"$2\" < \"$3\"")
            .arg(program())
            .arg(&self.dir)
            .arg(&base)
            .arg(&list)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null");
        let mut process = command
            .spawn(tokio::io::sink())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let Some(factory_proc::Line::Text(hash)) = process
            .next_line()
            .await
            .unwrap_or_else(|error| panic!("{error}"))
        else {
            panic!("pack-objects printed no name");
        };
        assert!(
            process
                .wait()
                .await
                .unwrap_or_else(|error| panic!("{error}"))
                .success()
        );
        fs::read(self.dir.join(format!("forged-{hash}.pack")))
            .unwrap_or_else(|error| panic!("{error}"))
    }
}

/// Ingests `tree` and `pack` into `run`'s `host.git`, against `base` as the
/// previous and the base tree.
async fn ingest(run: &Run, base: &Oid, tree: &[u8], pack: &[u8], limits: Limits) -> Captured {
    cli()
        .ingest(
            &run.host,
            &run.scratch.join("i"),
            Trees {
                previous: base,
                base,
            },
            tree,
            pack,
            limits,
        )
        .await
        .unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn hostile_packs_and_tree_names_are_rejected_and_leave_host_git_as_it_was() {
    common::within(async {
        let run = Run::standard("forged").await;
        let forge = Forge::new(&run).await;
        let packs = run.packs();
        let blob = forge.blob(b"x\n").await;
        let missing = "1".repeat(40);
        let mut cases: Vec<(&str, String, Vec<u8>)> = Vec::new();
        for (label, entries) in [
            (
                "an entry named .git",
                vec![("100644", ".git", blob.as_str())],
            ),
            (
                "an entry named .GIT",
                vec![("100644", ".GIT", blob.as_str())],
            ),
            (
                "an entry named git~1",
                vec![("100644", "git~1", blob.as_str())],
            ),
            (
                "a .gitmodules link",
                vec![("120000", ".gitmodules", blob.as_str())],
            ),
            (
                "a submodule entry",
                vec![("160000", "sub", run.base.as_str())],
            ),
            (
                "a blob in no pack",
                vec![("100644", "missing", missing.as_str())],
            ),
            (
                "another run's blob",
                vec![("100644", "stolen", run.other_blob.as_str())],
            ),
        ] {
            let tree = forge.tree(&entries).await;
            let pack = forge.pack(&[&tree, &blob]).await;
            cases.push((label, tree, pack));
        }
        let gitlink = cases[4].2.clone();
        cases.push((
            "a truncated pack",
            run.base_tree.to_string(),
            gitlink[..gitlink.len() - 5].to_vec(),
        ));
        cases.push((
            "bytes that are no pack",
            run.base_tree.to_string(),
            b"not a pack".to_vec(),
        ));
        cases.push((
            "a tree name that is no object name",
            "HEAD; rm -rf /".to_owned(),
            Vec::new(),
        ));
        cases.push((
            "a blob named as the tree",
            blob.clone(),
            forge.pack(&[&blob]).await,
        ));

        for (label, tree, pack) in cases {
            let captured = ingest(&run, &run.base_tree, tree.as_bytes(), &pack, LIMITS).await;
            if label == "a submodule entry" {
                // It passes `index-pack`; the walk of the modes rejects it.
                assert_eq!(
                    captured,
                    Captured::Rejected(vec![Rejection::Mode {
                        path: path("sub"),
                        mode: "160000".to_owned()
                    }])
                );
            } else {
                // `index-pack --strict`, the tree name or its type refuses
                // every other one before the walk.
                assert!(
                    matches!(captured.clone(), Captured::Rejected(r) if matches!(r.as_slice(), [Rejection::Pack(_)])),
                    "{label}: {captured:?}"
                );
            }
        }
        assert_eq!(run.packs(), packs);
        assert!(!run.scratch.join("i").exists());
        // Another run's blob is in the mirror, and in neither host.git nor
        // the workspace (WS-9).
        assert!(!has_object(&run.host, &run.other_blob).await);
        assert!(!has_object(&run.workspace.join(".git"), &run.other_blob).await);
    });
}

#[test]
fn a_blob_over_the_limit_is_rejected_by_its_size_in_the_pack() {
    common::within(async {
        // What the listing check saw cannot be trusted: ingest reads the
        // sizes from the blobs.
        let run = Run::standard("forged-size").await;
        let forge = Forge::new(&run).await;
        let blob = forge.blob(&vec![1; 100 * 1024]).await;
        let entries = run.entries(&run.base_tree).await;
        let (_, readme) = &entries["README.md"];
        let tree = forge
            .tree(&[
                ("100644", "README.md", readme),
                ("100644", "big.bin", &blob),
            ])
            .await;
        let pack = forge.pack(&[&tree, &blob]).await;
        assert_eq!(
            ingest(&run, &run.base_tree, tree.as_bytes(), &pack, LIMITS).await,
            Captured::Rejected(vec![Rejection::FileTooLarge {
                path: path("big.bin"),
                bytes: 100 * 1024
            }])
        );
    });
}

#[test]
fn two_paths_of_one_fold_are_rejected_unless_the_base_tree_holds_both() {
    common::within(async {
        // Built as trees, so that no file system has to hold both paths.
        let run = Run::standard("folds").await;
        let forge = Forge::new(&run).await;
        let one = forge.blob(b"one\n").await;
        let two = forge.blob(b"two\n").await;
        let three = forge.blob(b"three\n").await;

        // A pair that a round adds is rejected.
        let added = forge
            .tree(&[("100644", "README.md", &one), ("100644", "readme.md", &two)])
            .await;
        let pack = forge.pack(&[&added, &one, &two]).await;
        assert_eq!(
            ingest(&run, &run.base_tree, added.as_bytes(), &pack, LIMITS).await,
            Captured::Rejected(vec![Rejection::SameFold {
                first: path("README.md"),
                second: path("readme.md"),
            }])
        );

        // A pair that the base tree holds is admitted, and a round may change
        // both of its files. The base tree is put in host.git directly.
        let base = forge
            .tree(&[("100644", "README.md", &one), ("100644", "readme.md", &two)])
            .await;
        let base_pack = forge.pack(&[&base, &one, &two]).await;
        let Captured::Tree(base) =
            ingest(&run, &oid(&base), base.as_bytes(), &base_pack, LIMITS).await
        else {
            panic!("the base tree was not ingested");
        };
        let changed = forge
            .tree(&[
                ("100644", "README.md", &three),
                ("100644", "readme.md", &one),
            ])
            .await;
        let pack = forge.pack(&[&changed, &three]).await;
        assert_eq!(
            ingest(&run, &base, changed.as_bytes(), &pack, LIMITS).await,
            Captured::Tree(oid(&changed))
        );
    });
}

#[test]
fn a_layout_made_of_two_directories_with_one_fold_is_rejected() {
    common::within(async {
        // `fix/HEAD` beside `FIX/config`, `FIX/objects` and `FIX/refs`, which
        // the Mac's file system makes one repository (P-40).
        let run = Run::standard("fold-layout").await;
        let forge = Forge::new(&run).await;
        let blob = forge.blob(b"ref: refs/heads/main\n").await;
        let lower = forge.tree(&[("100644", "HEAD", &blob)]).await;
        let upper = forge
            .tree(&[
                ("100644", "config", &blob),
                ("100755", "objects", &blob),
                ("100755", "refs", &blob),
            ])
            .await;
        let tree = forge
            .tree(&[("040000", "FIX", &upper), ("040000", "fix", &lower)])
            .await;
        let pack = forge.pack(&[&tree, &lower, &upper, &blob]).await;
        let Captured::Rejected(rejections) =
            ingest(&run, &run.base_tree, tree.as_bytes(), &pack, LIMITS).await
        else {
            panic!("ingested");
        };
        let below = rejections
            .iter()
            .filter(|rejection| matches!(rejection, Rejection::BelowLayout { .. }))
            .count();
        assert_eq!(below, 4, "{rejections:?}");
        // The two directories are a pair of one fold as well.
        assert!(rejections.contains(&Rejection::SameFold {
            first: path("FIX"),
            second: path("fix"),
        }));
    });
}

// ---------------------------------------------------------------------------
// The commit (step 4) and the export
// ---------------------------------------------------------------------------

#[test]
fn the_same_candidate_gives_the_same_commit() {
    common::within(async {
        let run = Run::standard("commit").await;
        write(&run.p("src/new.rs"), b"new\n");
        let tree = run.tree().await;
        let first = run.commit(&tree).await;
        assert_eq!(run.commit(&tree).await, first);
        let header = git(&[
            git_dir(&run.host).as_os_str(),
            "cat-file".as_ref(),
            "commit".as_ref(),
            first.as_str().as_ref(),
        ])
        .await;
        assert_eq!(header[0], format!("tree {tree}"));
        assert_eq!(header[1], format!("parent {}", run.base));
        assert_eq!(
            header[2],
            "author sf-factory[bot] <1+sf-factory[bot]@users.noreply.github.com> 1795000000 +0000"
        );
        assert_eq!(&header[5..], ["fix: a candidate", "", "Factory-Issue: 130"]);
    });
}

#[test]
fn an_identity_that_git_would_change_is_refused() {
    common::within(async {
        let run = Run::standard("identity").await;
        let result = cli()
            .commit_candidate(
                &run.host,
                &Candidate {
                    tree: &run.base_tree,
                    parents: &[],
                    message: "x\n",
                    identity: &Identity {
                        name: "a <b>".to_owned(),
                        email: "c@d".to_owned(),
                    },
                    seconds: 0,
                },
            )
            .await;
        assert!(matches!(result, Err(Error::Identity)), "{result:?}");
    });
}

#[test]
fn an_export_is_a_new_directory_and_never_one_that_exists() {
    common::within(async {
        let run = Run::standard("export").await;
        let commit = run.commit(&run.base_tree).await;
        let export = run.scratch.join("e");
        fs::create_dir(&export).unwrap();
        assert!(cli().export(&run.host, &commit, &export).await.is_err());

        // A second export, after a stand-in for verify wrote to the first,
        // holds the candidate's bytes and nothing else (WS-14).
        let first = run.scratch.join("e1");
        cli().export(&run.host, &commit, &first).await.unwrap();
        write(&first.join("src/lib.rs"), b"rewritten\n");
        write(&first.join("planted.rs"), b"mod planted;\n");
        let second = run.scratch.join("e2");
        cli().export(&run.host, &commit, &second).await.unwrap();
        let expected: BTreeMap<Vec<u8>, (Vec<u8>, bool)> = BASE
            .iter()
            .map(|(path, bytes)| {
                (
                    path.as_bytes().to_vec(),
                    (bytes.to_vec(), path.ends_with(".sh")),
                )
            })
            .collect();
        assert_eq!(contents(&second), expected);
        assert!(!run.scratch.join("e2.index").exists());
    });
}

#[test]
fn a_round_that_writes_two_names_of_one_fold_is_rejected_where_the_file_system_holds_both() {
    common::within(async {
        let run = Run::standard("case").await;
        write(&run.p("readme.md"), b"collides with README.md\n");
        write(&run.p("src/Lib.rs"), b"collides with lib.rs\n");
        if fs::read(run.p("README.md")).unwrap() != b"# fixture\n" {
            // A file system that is not case-sensitive, as the Mac's, holds
            // one of the two; the trees of the ingest cases stand in there.
            return;
        }
        let mut rejections = run.rejected().await;
        rejections.sort();
        assert_eq!(
            rejections,
            [
                Rejection::SameFold {
                    first: path("README.md"),
                    second: path("readme.md"),
                },
                Rejection::SameFold {
                    first: path("src/Lib.rs"),
                    second: path("src/lib.rs"),
                },
            ]
        );
    });
}

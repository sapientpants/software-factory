//! The workspace of a run: a clone of its own with the base checked out
//! detached, no remote, and no in-tree attribute applied (SPEC WS-1, WS-10,
//! WS-11).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{Scratch, Upstream, cli, git as run, git_dir, has_object, main, oid, within};
use factory_git::{Git as _, Remote};

/// What the base commit holds: attributes that would change the bytes of a
/// checkout, and the files they would change (P-7).
const ATTRIBUTES: &[(&str, &[u8])] = &[
    (
        ".gitattributes",
        b"crlf.txt text eol=crlf\nid.txt ident\nfilter.txt filter=hostile\n",
    ),
    ("crlf.txt", b"one\ntwo\n"),
    ("id.txt", b"$Id$\n"),
    ("filter.txt", b"plain\n"),
    ("src/lib.rs", b"pub fn f() {}\n"),
];

/// A mirror whose `main` holds [`ATTRIBUTES`] and which also holds another
/// run's branch; the mirror, the upstream, and the commits of the two.
async fn mirrored(scratch: &Scratch) -> (PathBuf, Upstream, String, String) {
    let upstream = Upstream::new(scratch).await;
    let base = upstream.commit("main", ATTRIBUTES).await;
    let other = upstream
        .commit("factory/issue-9", &[("other.rs", b"another run's\n")])
        .await;
    let mirror = scratch.join("mirror.git");
    cli()
        .fetch_mirror(
            &mirror,
            &Remote::path(&upstream.bare).unwrap_or_else(|error| panic!("{error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    (mirror, upstream, base, other)
}

/// `git --git-dir=<workspace>/.git <args>`, outside the wrapper.
async fn in_workspace(workspace: &Path, args: &[&str]) -> Vec<String> {
    let dir = git_dir(&workspace.join(".git"));
    let mut all: Vec<&std::ffi::OsStr> = vec![dir.as_os_str()];
    all.extend(args.iter().map(std::ffi::OsStr::new));
    run(&all).await
}

/// The files of `workspace` with their bytes, but `.git`.
fn files(workspace: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found = Vec::new();
    let mut pending = vec![workspace.to_owned()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap_or_else(|error| panic!("{error}")) {
            let path = entry.unwrap_or_else(|error| panic!("{error}")).path();
            let relative = path
                .strip_prefix(workspace)
                .unwrap_or_else(|error| panic!("{error}"))
                .to_string_lossy()
                .into_owned();
            if relative == ".git" {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push((
                    relative,
                    fs::read(&path).unwrap_or_else(|error| panic!("{error}")),
                ));
            }
        }
    }
    found.sort();
    found
}

/// [`ATTRIBUTES`], as [`files`] gives them.
fn expected() -> Vec<(String, Vec<u8>)> {
    let mut expected: Vec<_> = ATTRIBUTES
        .iter()
        .map(|(path, bytes)| ((*path).to_owned(), bytes.to_vec()))
        .collect();
    expected.sort();
    expected
}

#[test]
fn a_workspace_holds_the_base_detached_with_no_remote() {
    within(async {
        let scratch = Scratch::new("workspace-created");
        let (mirror, _upstream, base, _other) = mirrored(&scratch).await;
        let workspace = scratch.join("runs/1/workspace");

        cli()
            .create_workspace(&mirror, &main(), &oid(&base), &workspace)
            .await
            .unwrap();

        // A detached HEAD holds the commit and names no branch.
        assert_eq!(
            fs::read_to_string(workspace.join(".git/HEAD")).unwrap(),
            format!("{base}\n")
        );
        assert!(in_workspace(&workspace, &["remote"]).await.is_empty());
    });
}

#[test]
fn a_workspace_is_checked_out_with_no_in_tree_attribute() {
    within(async {
        let scratch = Scratch::new("workspace-attributes");
        let (mirror, _upstream, base, _other) = mirrored(&scratch).await;
        let workspace = scratch.join("workspace");

        cli()
            .create_workspace(&mirror, &main(), &oid(&base), &workspace)
            .await
            .unwrap();

        // Each file has the bytes of its blob: no CRLF, no expanded `$Id$`.
        assert_eq!(files(&workspace), expected());
    });
}

#[test]
fn a_workspace_holds_its_own_objects_and_none_of_another_run() {
    within(async {
        let scratch = Scratch::new("workspace-objects");
        let (mirror, _upstream, base, other) = mirrored(&scratch).await;
        let workspace = scratch.join("workspace");

        cli()
            .create_workspace(&mirror, &main(), &oid(&base), &workspace)
            .await
            .unwrap();

        let git = workspace.join(".git");
        assert!(has_object(&git, &base).await);
        assert!(!has_object(&git, &other).await);
        // No alternate points at the mirror's objects.
        assert!(!git.join("objects/info/alternates").exists());
    });
}

#[test]
fn a_workspace_from_host_git_at_a_commit_off_the_default_branch() {
    within(async {
        let scratch = Scratch::new("workspace-at");
        let (mirror, upstream, _base, other) = mirrored(&scratch).await;
        let work = upstream
            .commit("factory/issue-1", &[("work.rs", b"this run\n")])
            .await;
        let git = cli();
        git.fetch_mirror(&mirror, &Remote::path(&upstream.bare).unwrap())
            .await
            .unwrap();
        let host = scratch.join("host.git");
        git.create_host_repository(&mirror, &main(), &host)
            .await
            .unwrap();
        let branch = factory_git::Branch::new("factory/issue-1").unwrap();
        git.fetch_into_host(&host, &mirror, &[branch])
            .await
            .unwrap();
        let refs_before = run(&[git_dir(&host).as_os_str(), "for-each-ref".as_ref()]).await;
        let workspace = scratch.join("workspace");

        git.create_workspace_at(&host, &oid(&work), &workspace)
            .await
            .unwrap();

        assert_eq!(
            in_workspace(&workspace, &["rev-parse", "HEAD"]).await,
            [work]
        );
        assert!(in_workspace(&workspace, &["remote"]).await.is_empty());
        let mut expected = expected();
        expected.push(("work.rs".to_owned(), b"this run\n".to_vec()));
        expected.sort();
        assert_eq!(files(&workspace), expected);
        assert!(!has_object(&workspace.join(".git"), &other).await);
        // The temporary ref is gone from host.git, and the workspace names
        // no branch but the detached HEAD.
        let refs_after = run(&[git_dir(&host).as_os_str(), "for-each-ref".as_ref()]).await;
        assert_eq!(refs_after, refs_before);
        assert!(in_workspace(&workspace, &["for-each-ref"]).await.is_empty());
    });
}

#[test]
fn a_workspace_at_a_commit_that_host_git_lacks_fails_and_leaves_no_ref() {
    within(async {
        let scratch = Scratch::new("workspace-at-missing");
        let (mirror, _upstream, _base, other) = mirrored(&scratch).await;
        let git = cli();
        let host = scratch.join("host.git");
        git.create_host_repository(&mirror, &main(), &host)
            .await
            .unwrap();
        let refs_before = run(&[git_dir(&host).as_os_str(), "for-each-ref".as_ref()]).await;

        let result = git
            .create_workspace_at(&host, &oid(&other), &scratch.join("workspace"))
            .await;

        assert!(result.is_err());
        let refs_after = run(&[git_dir(&host).as_os_str(), "for-each-ref".as_ref()]).await;
        assert_eq!(refs_after, refs_before);
    });
}

#[test]
fn a_temporary_ref_that_an_interrupted_creation_left_is_written_over_and_deleted() {
    within(async {
        let scratch = Scratch::new("workspace-at-stale");
        let (mirror, _upstream, base, _other) = mirrored(&scratch).await;
        let git = cli();
        let host = scratch.join("host.git");
        git.create_host_repository(&mirror, &main(), &host)
            .await
            .unwrap();
        let refs_before = run(&[git_dir(&host).as_os_str(), "for-each-ref".as_ref()]).await;
        let stale = format!("refs/heads/sf/workspace/{base}");
        run(&[
            git_dir(&host).as_os_str(),
            "update-ref".as_ref(),
            stale.as_ref(),
            base.as_ref(),
        ])
        .await;
        let workspace = scratch.join("workspace");

        git.create_workspace_at(&host, &oid(&base), &workspace)
            .await
            .unwrap();

        assert_eq!(
            in_workspace(&workspace, &["rev-parse", "HEAD"]).await,
            [base]
        );
        let refs_after = run(&[git_dir(&host).as_os_str(), "for-each-ref".as_ref()]).await;
        assert_eq!(refs_after, refs_before);
    });
}

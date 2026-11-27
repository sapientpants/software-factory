//! `host.git` of a run: a bare, single-branch clone of the mirror's default
//! branch with no remote, fetched into from the mirror (SPEC WS-10).

mod common;

use common::{Scratch, Upstream, cli, git as run, git_dir, has_object, main, oid, within};
use factory_git::{Branch, Git as _, Remote};

/// An upstream with a commit on `main` and an unmerged branch of another
/// run, fetched into a mirror; the commit of `main` and that of the other
/// branch.
async fn mirrored(scratch: &Scratch) -> (std::path::PathBuf, Upstream, String, String) {
    let upstream = Upstream::new(scratch).await;
    let head = upstream.commit("main", &[("README.md", b"hello\n")]).await;
    let other = upstream
        .commit(
            "factory/issue-9",
            &[("other.rs", b"another run's secret\n")],
        )
        .await;
    let mirror = scratch.join("mirror.git");
    cli()
        .fetch_mirror(
            &mirror,
            &Remote::path(&upstream.bare).unwrap_or_else(|error| panic!("{error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    (mirror, upstream, head, other)
}

#[test]
fn host_git_holds_the_default_branch_and_nothing_of_another_run() {
    within(async {
        let scratch = Scratch::new("host-created");
        let (mirror, _upstream, head, other) = mirrored(&scratch).await;
        let host = scratch.join("runs/1/host.git");
        let git = cli();

        git.create_host_repository(&mirror, &main(), &host)
            .await
            .unwrap();

        assert_eq!(git.branch_head(&host, &main()).await.unwrap(), oid(&head));
        assert!(has_object(&mirror, &other).await);
        assert!(!has_object(&host, &other).await);
        let branches = run(&[git_dir(&host).as_os_str(), "for-each-ref".as_ref()]).await;
        assert_eq!(branches.len(), 1, "{branches:?}");
        let remotes = run(&[git_dir(&host).as_os_str(), "remote".as_ref()]).await;
        assert!(remotes.is_empty(), "{remotes:?}");
        let bare = run(&[
            git_dir(&host).as_os_str(),
            "rev-parse".as_ref(),
            "--is-bare-repository".as_ref(),
        ])
        .await;
        assert_eq!(bare, ["true"]);
    });
}

#[test]
fn a_fetch_into_host_git_brings_the_branches_it_names_from_the_mirror() {
    within(async {
        let scratch = Scratch::new("host-fetch");
        let (mirror, upstream, _head, other) = mirrored(&scratch).await;
        let host = scratch.join("host.git");
        let git = cli();
        git.create_host_repository(&mirror, &main(), &host)
            .await
            .unwrap();
        let head = upstream.commit("main", &[("README.md", b"moved\n")]).await;
        let work = upstream
            .commit("factory/issue-1", &[("work.rs", b"this run\n")])
            .await;
        git.fetch_mirror(&mirror, &Remote::path(&upstream.bare).unwrap())
            .await
            .unwrap();
        let branch = Branch::new("factory/issue-1").unwrap();

        git.fetch_into_host(&host, &mirror, &[main(), branch.clone()])
            .await
            .unwrap();

        assert_eq!(git.branch_head(&host, &main()).await.unwrap(), oid(&head));
        assert_eq!(git.branch_head(&host, &branch).await.unwrap(), oid(&work));
        assert!(!has_object(&host, &other).await);
    });
}

#[test]
fn a_relative_path_is_refused_before_git_runs() {
    within(async {
        let scratch = Scratch::new("host-relative");
        let (mirror, ..) = mirrored(&scratch).await;
        let error = cli()
            .create_host_repository(&mirror, &main(), "runs/1/host.git".as_ref())
            .await
            .unwrap_err();
        assert!(
            matches!(error, factory_git::Error::NotAbsolute(_)),
            "{error}"
        );
    });
}

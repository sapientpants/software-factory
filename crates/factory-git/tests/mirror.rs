//! The mirror of a repository: created on its first fetch, fetched by the
//! host only (SPEC 9.1, WS-8).

mod common;

use common::{Scratch, Upstream, cli, main, oid, within};
use factory_git::{Branch, Git as _, GitCli, Remote};

#[test]
fn a_first_fetch_creates_the_mirror_with_the_upstream_branches() {
    within(async {
        let scratch = Scratch::new("first-fetch");
        let upstream = Upstream::new(&scratch).await;
        let head = upstream.commit("main", &[("README.md", b"hello\n")]).await;
        let mirror = scratch.join("mirrors/owner/repo.git");
        let git = cli();

        git.fetch_mirror(&mirror, &Remote::path(&upstream.bare).unwrap())
            .await
            .unwrap();

        assert_eq!(git.branch_head(&mirror, &main()).await.unwrap(), oid(&head));
    });
}

#[test]
fn a_later_fetch_moves_the_branches_and_prunes_those_upstream_removed() {
    within(async {
        let scratch = Scratch::new("later-fetch");
        let upstream = Upstream::new(&scratch).await;
        upstream.commit("main", &[("README.md", b"one\n")]).await;
        upstream
            .commit("factory/issue-7", &[("a.rs", b"a\n")])
            .await;
        let mirror = scratch.join("mirror.git");
        let remote = Remote::path(&upstream.bare).unwrap();
        let git = cli();
        git.fetch_mirror(&mirror, &remote).await.unwrap();
        let work = Branch::new("factory/issue-7").unwrap();
        assert!(git.branch_head(&mirror, &work).await.is_ok());

        let head = upstream.commit("main", &[("README.md", b"two\n")]).await;
        common::git(&[
            common::git_dir(&upstream.bare).as_os_str(),
            "branch".as_ref(),
            "-D".as_ref(),
            "factory/issue-7".as_ref(),
        ])
        .await;
        git.fetch_mirror(&mirror, &remote).await.unwrap();

        assert_eq!(git.branch_head(&mirror, &main()).await.unwrap(), oid(&head));
        assert!(git.branch_head(&mirror, &work).await.is_err());
    });
}

/// A stand-in for `git` that appends `start` and `end` to `log` around a
/// sleep on `fetch`, and does nothing else.
fn stand_in(scratch: &Scratch) -> (std::path::PathBuf, std::path::PathBuf) {
    let program = scratch.join("git");
    let log = scratch.join("log");
    let script = format!(
        "#!/bin/sh\nfor arg in \"$@\"; do\n  if [ \"$arg\" = fetch ]; then\n    echo start >> '{log}'\n    /bin/sleep 0.3\n    echo end >> '{log}'\n  fi\ndone\n",
        log = log.display()
    );
    std::fs::write(&program, script).unwrap_or_else(|error| panic!("{error}"));
    std::fs::set_permissions(
        &program,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    (program, log)
}

/// Two fetches at once into `first` and `second`, and the log they left.
async fn two_fetches(test: &str, same: bool) -> Vec<String> {
    let scratch = Scratch::new(test);
    let (program, log) = stand_in(&scratch);
    let first = scratch.join("first.git");
    let second = if same {
        first.clone()
    } else {
        scratch.join("second.git")
    };
    for mirror in [&first, &second] {
        std::fs::create_dir_all(mirror).unwrap_or_else(|error| panic!("{error}"));
        std::fs::write(mirror.join("HEAD"), "ref: refs/heads/main\n")
            .unwrap_or_else(|error| panic!("{error}"));
    }
    let git = GitCli::new(program);
    let remote =
        Remote::path(&scratch.join("upstream.git")).unwrap_or_else(|error| panic!("{error}"));
    let (a, b) = tokio::join!(
        git.fetch_mirror(&first, &remote),
        git.fetch_mirror(&second, &remote)
    );
    a.unwrap_or_else(|error| panic!("{error}"));
    b.unwrap_or_else(|error| panic!("{error}"));
    std::fs::read_to_string(log)
        .unwrap_or_else(|error| panic!("{error}"))
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn fetches_into_one_mirror_run_one_at_a_time() {
    within(async {
        assert_eq!(
            two_fetches("serialised", true).await,
            ["start", "end", "start", "end"]
        );
    });
}

#[test]
fn fetches_into_two_mirrors_run_side_by_side() {
    within(async {
        assert_eq!(
            two_fetches("side-by-side", false).await,
            ["start", "start", "end", "end"]
        );
    });
}

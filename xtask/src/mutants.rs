//! `cargo xtask mutants`: the mutation step of `cargo xtask check` (SPEC
//! DEV-2, DEV-2a). `cargo mutants --in-diff` is given the diff from a base
//! commit to the work tree as it stands, new files included.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{check, vm};

/// The files whose code the sandbox tests of 19.4 test, and the plain test
/// run does not, or only in part: in a worker's sandbox, where those tests do not run (DEV-11),
/// the step does not mutate them.
pub const SANDBOX_CODE: &[&str] = &[
    "crates/factory-sandbox/src/bubblewrap.rs",
    "crates/factory-sandbox/src/bubblewrap/init.rs",
    "crates/factory-daemon/src/sandbox_exec.rs",
];

/// The variable through which the development VM is given the diff that the
/// Mac wrote: the copy of the work tree in the VM holds no `.git` (DEV-3).
pub const DIFF_VARIABLE: &str = "XTASK_MUTANTS_DIFF";

/// The file below the build output `target` that holds the diff.
pub fn diff_file(target: &Path) -> PathBuf {
    target.join("xtask").join("mutants.diff")
}

pub fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    if let Err(error) = std::env::set_current_dir(&root) {
        eprintln!("xtask mutants: cannot enter the repository's root: {error}");
        return ExitCode::FAILURE;
    }
    match run(&root) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("xtask mutants: {failure}");
            ExitCode::FAILURE
        }
    }
}

fn run(root: &Path) -> Result<(), String> {
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let file = match std::env::var_os(DIFF_VARIABLE) {
        Some(file) => PathBuf::from(file),
        None => {
            let diff = diff_of_the_work_tree(root, std::env::var_os("SF_BASE_SHA"))?;
            let file = diff_file(&target);
            write(&file, &diff)?;
            file
        }
    };
    let worker = check::is_a_worker(std::env::var_os("SF_SANDBOX").as_deref());
    mutate("cargo", &file, &target, worker)
}

/// The diff from the base to the work tree at `root`, each found as DEV-2a
/// says. `sf_base_sha` is the value of `SF_BASE_SHA`, where it is set.
pub fn diff_of_the_work_tree(
    root: &Path,
    sf_base_sha: Option<OsString>,
) -> Result<Vec<u8>, String> {
    let base = base(root, sf_base_sha)?;
    eprintln!("xtask mutants: the diff runs from {base} to the work tree");
    diff(root, &base)
}

/// The base commit (DEV-2a): the commit that `sf_base_sha` names where it is
/// set, and otherwise the merge base of `HEAD` and `origin/main`. Where
/// neither gives a commit the step fails by name.
fn base(root: &Path, sf_base_sha: Option<OsString>) -> Result<String, String> {
    if let Some(named) = sf_base_sha {
        let named = named.to_string_lossy().into_owned();
        return git(
            root,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{named}^{{commit}}"),
            ],
            None,
        )
        .map(|output| output.trim().to_owned())
        .map_err(|error| {
            format!(
                "the mutation step has no base: `SF_BASE_SHA` holds `{named}`, which names \
                     no commit of the repository ({error}) (DEV-2a)"
            )
        });
    }
    git(root, &["merge-base", "HEAD", "origin/main"], None)
        .map(|output| output.trim().to_owned())
        .map_err(|error| {
            format!(
                "the mutation step has no base: `SF_BASE_SHA` is not set, and `HEAD` and \
                 `origin/main` have no merge base ({error}); fetch `main` from `origin` \
                 (DEV-2a)"
            )
        })
}

/// The diff from `base` to the work tree at `root`, new files included and
/// the files that git ignores left out. The repository's index is left as
/// it is: a copy of it is given every file of the work tree.
///
/// Git takes a tracked file whose stat data match its index entry for
/// unchanged, and the copy's fresh mtime hides the racy entries whose stat
/// data it would not trust. `--renormalize` adds every tracked file again
/// from its content, so a file rewritten at the same size within a tick of
/// the last write of the index is in the diff too (DEV-2a).
fn diff(root: &Path, base: &str) -> Result<Vec<u8>, String> {
    let index = git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        None,
    )?;
    static COPIES: AtomicUsize = AtomicUsize::new(0);
    let copy = std::env::temp_dir().join(format!(
        "xtask-mutants-index-{}-{}",
        std::process::id(),
        COPIES.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&copy);
    if Path::new(index.trim()).exists() {
        std::fs::copy(index.trim(), &copy)
            .map_err(|error| format!("cannot copy the index of the repository: {error}"))?;
    }
    let diff = git(root, &["add", "--all"], Some(&copy))
        .and_then(|_| git(root, &["add", "--renormalize", "--", ":/"], Some(&copy)))
        .and_then(|_| {
            git_bytes(
                root,
                &[
                    "diff",
                    "--cached",
                    "--no-color",
                    "--no-ext-diff",
                    "--no-renames",
                    "--src-prefix=a/",
                    "--dst-prefix=b/",
                    base,
                ],
                Some(&copy),
            )
        });
    let _ = std::fs::remove_file(&copy);
    diff
}

/// Runs `cargo mutants` on the diff in `file`, with `cargo` as the program,
/// and fails where a mutant survives. An empty diff runs nothing, and the step
/// says that it had nothing to mutate. `mutants.out` is written below
/// `target`, the build output, and not in the work tree. The mutated copies
/// are built without `CARGO_TARGET_DIR`, each in a build output of its own.
///
/// The tests of each mutant include the sandbox tests of 19.4, which nextest
/// sees as ignored unless [`check::SANDBOX_TESTS`] is set: without them every
/// mutant of the sandbox's code survives.
/// In a worker's sandbox (`worker`) they cannot run, and the files of
/// [`SANDBOX_CODE`] are left out of the mutation, which the step says.
pub fn mutate(cargo: &str, file: &Path, target: &Path, worker: bool) -> Result<(), String> {
    let diff = std::fs::read(file)
        .map_err(|error| format!("cannot read the diff {}: {error}", file.display()))?;
    if diff.is_empty() {
        eprintln!("xtask mutants: the diff is empty, so there is nothing to mutate");
        return Ok(());
    }
    let mut step: Vec<String> = [
        "mutants",
        "--in-diff",
        &file.to_string_lossy(),
        "--test-tool=nextest",
        "--cargo-arg=--locked",
        "--output",
        &target.to_string_lossy(),
    ]
    .map(str::to_owned)
    .into();
    if worker {
        eprintln!(
            "xtask mutants: in a worker's sandbox the sandbox tests do not run, \
             so {} is not mutated (DEV-11)",
            SANDBOX_CODE.join(", ")
        );
        step.extend(SANDBOX_CODE.iter().map(|path| format!("--exclude={path}")));
    }
    eprintln!("xtask mutants: {cargo} {}", step.join(" "));
    // The copies that `cargo mutants` builds share no build output with the
    // check: with the check's `CARGO_TARGET_DIR` a mutated build would take
    // the place of the check's own (DEV-2a).
    let mut command = Command::new(cargo);
    command.args(&step).env_remove("CARGO_TARGET_DIR");
    if worker {
        command.env_remove(check::SANDBOX_TESTS.0);
    } else {
        command.env(check::SANDBOX_TESTS.0, check::SANDBOX_TESTS.1);
    }
    let status = command
        .status()
        .map_err(|error| format!("`{cargo} mutants` could not be started: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "`{cargo} mutants` {}: a mutant survived, timed out or the tests failed without one; \
             its report is in {}",
            vm::ended(status),
            target.join("mutants.out").display()
        ))
    }
}

/// Writes `contents` to `file`, creating its directory.
fn write(file: &Path, contents: &[u8]) -> Result<(), String> {
    if let Some(directory) = file.parent() {
        std::fs::create_dir_all(directory)
            .map_err(|error| format!("cannot create {}: {error}", directory.display()))?;
    }
    std::fs::write(file, contents)
        .map_err(|error| format!("cannot write {}: {error}", file.display()))
}

fn git(root: &Path, arguments: &[&str], index: Option<&Path>) -> Result<String, String> {
    git_bytes(root, arguments, index).map(|output| String::from_utf8_lossy(&output).into_owned())
}

/// Runs git in `root` with `index` as its index where it is given, and
/// returns what it printed.
fn git_bytes(root: &Path, arguments: &[&str], index: Option<&Path>) -> Result<Vec<u8>, String> {
    let call = format!("git {}", arguments.join(" "));
    let mut command = Command::new("git");
    command
        .args(arguments)
        .current_dir(root)
        .stderr(Stdio::piped());
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let output = command
        .output()
        .map_err(|error| format!("`{call}` could not be started: {error}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        let said = String::from_utf8_lossy(&output.stderr);
        let said = said.trim();
        Err(match said {
            "" => format!("`{call}` {}", vm::ended(output.status)),
            said => format!("`{call}` {}: {said}", vm::ended(output.status)),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use super::*;

    /// A fresh directory for one test, removed when the value is dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("xtask-mutants-{}-{test}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create the scratch directory");
            Self(path)
        }

        fn write(&self, file: &str, contents: &str) {
            let path = self.0.join(file);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("create a directory");
            std::fs::write(path, contents).expect("write a file");
        }

        /// A repository with one commit, which holds `.gitignore` and `kept`.
        fn repository(test: &str) -> Self {
            let scratch = Self::new(test);
            git_in(&scratch.0, &["init", "--quiet"]);
            scratch.write(".gitignore", "/target\n");
            scratch.write("kept", "one\n");
            git_in(&scratch.0, &["add", "--all"]);
            git_in(&scratch.0, &["commit", "--quiet", "-m", "first"]);
            scratch
        }

        fn head(&self) -> String {
            let output = Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&self.0)
                .output()
                .expect("run git");
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }

        /// A stand-in `cargo` that records its arguments and exits with `exit`.
        fn cargo(&self, exit: i32) -> String {
            let cargo = self.0.join("cargo");
            std::fs::write(
                &cargo,
                format!(
                    "#!/bin/sh\necho \"$* with SF_SANDBOX_TESTS=$SF_SANDBOX_TESTS\" >> '{runs}'\nexit {exit}\n",
                    runs = self.0.join("runs").display()
                ),
            )
            .expect("write the stand-in cargo");
            let status = Command::new("chmod")
                .arg("+x")
                .arg(&cargo)
                .status()
                .expect("run chmod");
            assert!(status.success());
            cargo.to_string_lossy().into_owned()
        }

        fn runs(&self) -> Option<String> {
            std::fs::read_to_string(self.0.join("runs")).ok()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Runs git in `directory`, cut off from the configuration of whoever
    /// runs the test.
    fn git_in(directory: &Path, arguments: &[&str]) {
        let status = Command::new("git")
            .args(arguments)
            .current_dir(directory)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .env_remove("GIT_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_WORK_TREE")
            .status()
            .expect("run git");
        assert!(status.success(), "git {arguments:?}");
    }

    fn text(diff: &[u8]) -> String {
        String::from_utf8_lossy(diff).into_owned()
    }

    #[test]
    fn the_diff_runs_to_the_work_tree_with_its_new_files_and_without_what_git_ignores() {
        let scratch = Scratch::repository("work-tree");
        let base = scratch.head();
        scratch.write("kept", "two\n");
        scratch.write("src/new.rs", "fn new() {}\n");
        scratch.write("target/built", "built\n");

        let diff = text(&diff(&scratch.0, &base).expect("the diff"));

        assert!(diff.contains("diff --git a/kept b/kept"), "{diff}");
        assert!(diff.contains("+two"), "{diff}");
        assert!(
            diff.contains("diff --git a/src/new.rs b/src/new.rs"),
            "{diff}"
        );
        assert!(diff.contains("+fn new() {}"), "{diff}");
        assert!(!diff.contains("target/built"), "{diff}");
    }

    #[test]
    fn the_diff_holds_a_file_rewritten_with_the_stat_data_of_its_index_entry() {
        let scratch = Scratch::repository("stat");
        let base = scratch.head();
        // Git compares no ctime, and the index records an mtime well before
        // its own, so the entry is not racy and git trusts its stat data.
        git_in(&scratch.0, &["config", "core.trustctime", "false"]);
        let kept = scratch.0.join("kept");
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        let set_mtime = || {
            std::fs::File::options()
                .write(true)
                .open(&kept)
                .and_then(|file| file.set_modified(old))
                .expect("set the mtime of kept");
        };
        set_mtime();
        git_in(&scratch.0, &["update-index", "--refresh"]);
        // The same size, the same inode and, set back, the same mtime.
        scratch.write("kept", "two\n");
        set_mtime();

        let diff = text(&diff(&scratch.0, &base).expect("the diff"));

        assert!(diff.contains("diff --git a/kept b/kept"), "{diff}");
        assert!(diff.contains("+two"), "{diff}");
    }

    #[test]
    fn the_diff_leaves_the_index_of_the_repository_as_it_is() {
        let scratch = Scratch::repository("index");
        let base = scratch.head();
        scratch.write("untracked", "new\n");
        let index = scratch.0.join(".git/index");
        let before = std::fs::read(&index).expect("read the index");

        diff(&scratch.0, &base).expect("the diff");

        assert_eq!(std::fs::read(&index).expect("read the index"), before);
    }

    #[test]
    fn the_base_is_the_commit_that_sf_base_sha_names() {
        let scratch = Scratch::repository("named");
        let first = scratch.head();
        scratch.write("second", "two\n");
        git_in(&scratch.0, &["add", "second"]);
        git_in(&scratch.0, &["commit", "--quiet", "-m", "second"]);
        let second = scratch.head();
        // A remote-tracking `main` that would give another base.
        git_in(
            &scratch.0,
            &["update-ref", "refs/remotes/origin/main", &second],
        );

        let base = base(&scratch.0, Some(first.clone().into())).expect("a base");
        let diff =
            text(&diff_of_the_work_tree(&scratch.0, Some(first.clone().into())).expect("the diff"));

        assert_eq!(base, first);
        assert!(diff.contains("b/second"), "{diff}");
    }

    #[test]
    fn without_sf_base_sha_the_base_is_the_merge_base_of_head_and_origin_main() {
        let scratch = Scratch::repository("merge-base");
        let first = scratch.head();
        git_in(
            &scratch.0,
            &["update-ref", "refs/remotes/origin/main", &first],
        );
        scratch.write("committed", "on the branch\n");
        git_in(&scratch.0, &["add", "committed"]);
        git_in(&scratch.0, &["commit", "--quiet", "-m", "on the branch"]);

        let base = base(&scratch.0, None).expect("a base");
        let diff = text(&diff_of_the_work_tree(&scratch.0, None).expect("the diff"));

        assert_eq!(base, first);
        assert!(diff.contains("+on the branch"), "{diff}");
    }

    #[test]
    fn a_sf_base_sha_that_names_no_commit_fails_by_name() {
        let scratch = Scratch::repository("no-commit");
        git_in(
            &scratch.0,
            &["update-ref", "refs/remotes/origin/main", &scratch.head()],
        );

        let failure = base(&scratch.0, Some("0123456789abcdef".into())).expect_err("no base");

        assert!(failure.contains("mutation step"), "{failure}");
        assert!(failure.contains("`SF_BASE_SHA`"), "{failure}");
        assert!(failure.contains("0123456789abcdef"), "{failure}");
    }

    #[test]
    fn without_sf_base_sha_and_origin_main_the_step_fails_by_name() {
        let scratch = Scratch::repository("no-origin");

        let failure = base(&scratch.0, None).expect_err("no base");

        assert!(failure.contains("mutation step"), "{failure}");
        assert!(failure.contains("`origin/main`"), "{failure}");
    }

    #[test]
    fn the_diff_is_written_to_its_file_with_its_directory() {
        let scratch = Scratch::new("write");
        let file = scratch.0.join("target/xtask/mutants.diff");

        write(&file, b"the diff\n").expect("write the diff");

        assert_eq!(std::fs::read(&file).expect("read the diff"), b"the diff\n");
    }

    #[test]
    fn an_empty_diff_says_that_it_had_nothing_to_mutate_and_runs_nothing() {
        let scratch = Scratch::new("empty");
        let cargo = scratch.cargo(2);
        let file = scratch.0.join("mutants.diff");
        std::fs::write(&file, "").expect("write the diff");

        let outcome = mutate(&cargo, &file, &scratch.0.join("target"), false);

        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(scratch.runs(), None, "cargo ran");
    }

    #[test]
    fn a_diff_is_given_to_cargo_mutants_and_its_passing_run_passes() {
        let scratch = Scratch::new("passes");
        let cargo = scratch.cargo(0);
        let file = scratch.0.join("mutants.diff");
        std::fs::write(&file, "diff --git a/x b/x\n").expect("write the diff");
        let target = scratch.0.join("target");

        let outcome = mutate(&cargo, &file, &target, false);

        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(
            scratch.runs().as_deref(),
            Some(
                format!(
                    "mutants --in-diff {} --test-tool=nextest --cargo-arg=--locked --output {} \
                     with SF_SANDBOX_TESTS=process\n",
                    file.display(),
                    target.display()
                )
                .as_str()
            )
        );
    }

    #[test]
    fn in_a_workers_sandbox_the_sandboxs_code_is_not_mutated() {
        let scratch = Scratch::new("worker");
        let cargo = scratch.cargo(0);
        let file = scratch.0.join("mutants.diff");
        std::fs::write(&file, "diff --git a/x b/x\n").expect("write the diff");
        let target = scratch.0.join("target");

        let outcome = mutate(&cargo, &file, &target, true);

        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(
            scratch.runs().as_deref(),
            Some(
                format!(
                    "mutants --in-diff {} --test-tool=nextest --cargo-arg=--locked --output {} \
                     --exclude=crates/factory-sandbox/src/bubblewrap.rs \
                     --exclude=crates/factory-sandbox/src/bubblewrap/init.rs \
                     --exclude=crates/factory-daemon/src/sandbox_exec.rs with SF_SANDBOX_TESTS=\n",
                    file.display(),
                    target.display()
                )
                .as_str()
            )
        );
    }

    #[test]
    fn the_sandboxs_code_is_a_file_of_the_tree() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        for path in SANDBOX_CODE {
            assert!(root.join(path).is_file(), "{path}");
        }
    }

    #[test]
    fn a_surviving_mutant_fails_the_step_by_name() {
        let scratch = Scratch::new("survives");
        // `cargo mutants` exits with 2 where a mutant was missed.
        let cargo = scratch.cargo(2);
        let file = scratch.0.join("mutants.diff");
        std::fs::write(&file, "diff --git a/x b/x\n").expect("write the diff");

        let failure =
            mutate(&cargo, &file, &scratch.0.join("target"), false).expect_err("it fails");

        assert!(failure.contains("mutants"), "{failure}");
        assert!(failure.contains("exit code 2"), "{failure}");
        assert!(failure.contains("survived"), "{failure}");
    }
}

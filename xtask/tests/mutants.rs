//! `cargo xtask mutants` as `cargo xtask check` runs it (SPEC DEV-2a): its
//! exit code and what it says.

use std::path::PathBuf;
use std::process::{Command, Output};

/// A fresh directory for one test, removed when the value is dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("xtask-mutants-task-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create the scratch directory");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs the task with the variables given, and neither `SF_BASE_SHA` nor the
/// diff variable of the VM where they are not given.
fn mutants(variables: &[(&str, &str)], target: &Scratch) -> Output {
    let mut task = Command::new(env!("CARGO_BIN_EXE_xtask"));
    task.arg("mutants")
        .env_remove("SF_BASE_SHA")
        .env_remove("XTASK_MUTANTS_DIFF")
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .env("CARGO_TARGET_DIR", &target.0);
    for (name, value) in variables {
        task.env(name, value);
    }
    task.output().expect("run xtask")
}

#[test]
fn a_sf_base_sha_that_names_no_commit_fails_the_task_by_name() {
    let target = Scratch::new("no-commit");

    let output = mutants(&[("SF_BASE_SHA", "no-such-commit")], &target);

    let said = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{said}");
    assert!(said.contains("`SF_BASE_SHA`"), "{said}");
    assert!(said.contains("no-such-commit"), "{said}");
}

#[test]
fn an_empty_diff_from_the_vm_passes_with_nothing_to_mutate() {
    let target = Scratch::new("empty");
    let diff = target.0.join("mutants.diff");
    std::fs::write(&diff, "").expect("write the diff");

    let output = mutants(&[("XTASK_MUTANTS_DIFF", &diff.to_string_lossy())], &target);

    let said = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{said}");
    assert!(said.contains("nothing to mutate"), "{said}");
}

#[test]
fn a_diff_from_the_vm_that_is_missing_fails_the_task() {
    let target = Scratch::new("missing");
    let diff = target.0.join("no-such.diff");

    let output = mutants(&[("XTASK_MUTANTS_DIFF", &diff.to_string_lossy())], &target);

    let said = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{said}");
    assert!(said.contains("no-such.diff"), "{said}");
}

#[test]
fn cargo_mutants_is_given_the_diff_and_not_the_build_output_of_the_check() {
    use std::os::unix::fs::PermissionsExt;

    let target = Scratch::new("target");
    let bin = target.0.join("bin");
    std::fs::create_dir_all(&bin).expect("create the directory of the stand-in");
    let runs = target.0.join("runs");
    std::fs::write(
        bin.join("cargo"),
        format!(
            "#!/bin/sh\necho \"$1 $2 $3 ${{CARGO_TARGET_DIR-unset}}\" >> '{}'\n",
            runs.display()
        ),
    )
    .expect("write the stand-in cargo");
    std::fs::set_permissions(bin.join("cargo"), std::fs::Permissions::from_mode(0o755))
        .expect("make the stand-in executable");
    let diff = target.0.join("mutants.diff");
    std::fs::write(&diff, "diff --git a/x b/x\n").expect("write the diff");
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = mutants(
        &[
            ("XTASK_MUTANTS_DIFF", &diff.to_string_lossy()),
            ("PATH", &path),
        ],
        &target,
    );

    let said = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{said}");
    assert_eq!(
        std::fs::read_to_string(&runs).expect("cargo ran"),
        format!("mutants --in-diff {} unset\n", diff.display())
    );
}

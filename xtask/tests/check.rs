//! `cargo xtask check` as a process (SPEC DEV-1): a check that fails exits
//! with a failure and says so.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

/// A fresh directory for one test, removed when the value is dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("xtask-check-task-{}-{test}", std::process::id()));
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

#[test]
fn a_check_that_fails_before_its_steps_exits_with_a_failure() {
    let scratch = Scratch::new("fails");
    // A `cargo` that reports no version, so the version check fails before a
    // step runs on Linux. On macOS the base that `SF_BASE_SHA` names is no
    // commit, so the command fails before the VM runs.
    let cargo = scratch.0.join("cargo");
    std::fs::write(&cargo, "#!/bin/sh\nexit 1\n").expect("write the stand-in cargo");
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755))
        .expect("make the stand-in executable");
    let path = format!(
        "{}:{}",
        scratch.0.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("check")
        .env("PATH", path)
        .env("SF_BASE_SHA", "no-such-commit")
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("run xtask");

    let said = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{said}");
    assert!(said.contains("xtask check:"), "{said}");
    assert!(!said.contains("xtask check: passed"), "{said}");
}

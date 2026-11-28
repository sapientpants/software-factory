//! `sf sandbox exec` as the binary `sf` runs it (SPEC CLI-6): the operator
//! configuration that it reads, and what it does where it runs no command.
//! None of these creates a sandbox; the sandbox tests of `factory-daemon` run
//! the command in one (19.4).

#![expect(
    clippy::unwrap_used,
    reason = "the helpers of a test file, which clippy does not see as test code (19.3)"
)]

use std::path::{Path, PathBuf};

use factory_daemon::process::Command;

/// The binary under test.
const SF: &str = env!("CARGO_BIN_EXE_sf");

/// A home directory for one test, removed when the value is dropped.
struct Home(PathBuf);

impl Home {
    fn new(test: &str) -> Self {
        let path = Path::new("/tmp").join(format!("sf-cli-exec-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join(".config/sf")).unwrap();
        Self(path)
    }

    fn config(&self, text: &str) {
        std::fs::write(self.0.join(".config/sf/config.toml"), text).unwrap();
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs `sf` with `args` and `HOME` at `home`, and its exit code and stderr.
fn sf(home: &Path, args: &[&str]) -> (Option<i32>, String) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut command = Command::new(SF);
        command
            .args(args)
            .env("HOME", home)
            .env("PATH", std::env::var_os("PATH").unwrap_or_default());
        let (sink, mut read) = tokio::io::duplex(1 << 16);
        let mut process = command.spawn(sink).await.unwrap();
        let code = process.wait().await.unwrap().code();
        drop(process);
        let mut stderr = String::new();
        tokio::io::AsyncReadExt::read_to_string(&mut read, &mut stderr)
            .await
            .unwrap();
        (code, stderr)
    })
}

#[test]
fn without_an_operator_configuration_no_command_runs() {
    let home = Home::new("none");

    let (code, stderr) = sf(&home.0, &["sandbox", "exec", "--dir", "/", "--", "true"]);

    assert_eq!(code, Some(125));
    assert!(
        stderr.starts_with("sf sandbox exec: cannot read the operator configuration ")
            && stderr.contains("/.config/sf/config.toml"),
        "{stderr}"
    );
}

#[test]
fn an_invalid_operator_configuration_names_its_key() {
    let home = Home::new("invalid");
    home.config("schema_version = 1\ndata_dir = \"relative\"\n[sandbox]\nlevel = \"process\"\n");

    let (code, stderr) = sf(&home.0, &["sandbox", "exec", "--dir", "/", "--", "true"]);

    assert_eq!(code, Some(125));
    assert!(stderr.contains("is invalid: `data_dir`"), "{stderr}");
}

#[test]
fn a_dir_that_does_not_exist_runs_no_command() {
    let home = Home::new("dir");
    home.config(&format!(
        "schema_version = 1\ndata_dir = {:?}\n[sandbox]\nlevel = \"process\"\n",
        home.0.join("data")
    ));
    let absent = home.0.join("absent");

    let (code, stderr) = sf(
        &home.0,
        &[
            "sandbox",
            "exec",
            "--dir",
            absent.to_str().unwrap(),
            "--",
            "true",
        ],
    );

    assert_eq!(code, Some(125));
    // Off Linux there is no backend, which is found before the directory.
    let expected = if cfg!(target_os = "linux") {
        "sf sandbox exec: --dir, "
    } else {
        "sf sandbox exec: the sandbox level `process` has no backend off Linux"
    };
    assert!(stderr.starts_with(expected), "{stderr}");
    assert!(!home.0.join("data").exists());
}

#[test]
fn the_command_line_is_required_and_a_tool_is_held_to_its_form() {
    let home = Home::new("args");

    let (code, stderr) = sf(&home.0, &["sandbox", "exec", "--dir", "/"]);
    assert_eq!(code, Some(2), "{stderr}");

    let (code, stderr) = sf(
        &home.0,
        &[
            "sandbox", "exec", "--dir", "/", "--tool", "Deny", "--", "true",
        ],
    );
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("is no <name>=<version>"), "{stderr}");
}

#[test]
fn with_json_what_failed_is_json() {
    let home = Home::new("json");

    let (code, stderr) = sf(
        &home.0,
        &["sandbox", "exec", "--json", "--dir", "/", "--", "true"],
    );

    assert_eq!(code, Some(125));
    let error: serde_json::Value = serde_json::from_str(stderr.trim_end()).unwrap();
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .starts_with("cannot read the operator configuration "),
        "{stderr}"
    );
}

#[test]
fn level_hardened_has_no_backend_yet() {
    let home = Home::new("hardened");
    home.config(&format!(
        "schema_version = 1\ndata_dir = {:?}\n[sandbox]\nlevel = \"hardened\"\n",
        home.0.join("data")
    ));

    let (code, stderr) = sf(&home.0, &["sandbox", "exec", "--dir", "/", "--", "true"]);

    assert_eq!(code, Some(125));
    assert!(
        stderr.starts_with(
            "sf sandbox exec: the sandbox level `hardened` has no backend in this build"
        ),
        "{stderr}"
    );
    assert!(!home.0.join("data").exists());
}

//! `sf doctor` as the binary `sf` runs it (SPEC CLI-1), with an operator
//! configuration that is invalid, so that no check starts a sandbox
//! instance: the report names the key (CFG-6, CLI-7) and `sf` fails.

#![expect(
    clippy::unwrap_used,
    reason = "the helpers of a test file, which clippy does not see as test code (19.3)"
)]

use std::path::{Path, PathBuf};

use factory_daemon::process::{Command, Line};

/// The binary under test.
const SF: &str = env!("CARGO_BIN_EXE_sf");

/// A home directory for one test, removed when the value is dropped.
struct Home(PathBuf);

impl Home {
    fn new(test: &str) -> Self {
        let path = Path::new("/tmp").join(format!("sf-cli-doctor-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join(".config/sf")).unwrap();
        Self(path)
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs `sf` with `args` and `HOME` at `home`, and its exit code and stdout.
fn sf(home: &Path, args: &[&str]) -> (Option<i32>, String) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut command = Command::new(SF);
        command
            .args(args)
            .env("HOME", home)
            .env("PATH", std::env::var_os("PATH").unwrap_or_default());
        let mut process = command.spawn(tokio::io::sink()).await.unwrap();
        let mut stdout = String::new();
        while let Some(line) = process.next_line().await.unwrap() {
            if let Line::Text(text) = line {
                stdout.push_str(&text);
                stdout.push('\n');
            }
        }
        (process.wait().await.unwrap().code(), stdout)
    })
}

#[test]
fn an_invalid_configuration_is_reported_by_its_key_and_fails() {
    let home = Home::new("invalid");
    std::fs::write(
        home.0.join(".config/sf/config.toml"),
        "schema_version = 1\ndata_dir = \"relative\"\n[sandbox]\nlevel = \"process\"\n",
    )
    .unwrap();

    let (code, stdout) = sf(&home.0, &["doctor"]);
    assert_eq!(code, Some(1), "{stdout}");
    let first = stdout.lines().next().unwrap_or_default();
    assert!(
        first.starts_with("FAIL  operator configuration: ") && first.contains("`data_dir`"),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "FAIL  credential files: not made, since no valid operator configuration was read\n"
        ) && stdout
            .contains("FAIL  sandbox: not made, since no valid operator configuration was read\n"),
        "{stdout}"
    );

    let (code, stdout) = sf(&home.0, &["doctor", "--json"]);
    assert_eq!(code, Some(1), "{stdout}");
    let report: serde_json::Value = serde_json::from_str(stdout.trim_end()).unwrap();
    assert_eq!(report["ok"], false);
    assert_eq!(report["checks"][0]["name"], "operator configuration");
    assert_eq!(report["checks"][0]["ok"], false);
}

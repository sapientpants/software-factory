//! The checks of `sf doctor` that start no sandbox instance (CLI-1), in the
//! plain test run (DEV-2a). The check that starts one is a sandbox test, in
//! `tests/sandbox.rs`.

#![expect(
    clippy::unwrap_used,
    reason = "the helpers of the tests, which clippy does not see as test code (19.3)"
)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use factory_daemon::doctor::{self, Check};

/// A fresh directory for one test, removed when the value is dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Self {
        let path = std::env::temp_dir().join(format!("sf-doctor-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, text: &str, mode: u32) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn passed(check: &Check) -> bool {
    check.ok
}

const MINIMAL: &str =
    "schema_version = 1\ndata_dir = \"/srv/sf/data\"\n[sandbox]\nlevel = \"process\"\n";

#[test]
fn a_configuration_of_version_1_passes_and_an_invalid_one_names_the_key() {
    let scratch = Scratch::new("config");
    let good = scratch.write("good.toml", MINIMAL, 0o644);
    let (check, config) = doctor::config(&good);
    assert!(passed(&check), "{check:?}");
    assert!(config.is_some());

    let bad = scratch.write(
        "bad.toml",
        "schema_version = 1\ndata_dir = \"srv\"\n[sandbox]\nlevel = \"process\"\n",
        0o644,
    );
    let (check, config) = doctor::config(&bad);
    assert!(!passed(&check));
    assert!(config.is_none());
    assert!(check.text.contains("`data_dir`"), "{}", check.text);

    let (check, config) = doctor::config(&scratch.0.join("missing.toml"));
    assert!(!passed(&check));
    assert!(config.is_none());
    assert!(check.text.contains("missing.toml"), "{}", check.text);
}

#[test]
fn each_credential_file_that_the_configuration_names_has_mode_0600() {
    let scratch = Scratch::new("credentials");
    let token = scratch.write("token", "t", 0o600);
    let key = scratch.write("key.pem", "k", 0o640);
    let text = format!(
        "{MINIMAL}[credentials]\nsubscription_token_file = {token:?}\n[github]\napp_id = 1\nprivate_key_file = {key:?}\n"
    );
    let file = scratch.write("config.toml", &text, 0o644);
    let config = doctor::config(&file).1.unwrap();

    let checks = doctor::credentials(&config);
    assert_eq!(checks.len(), 2, "{checks:?}");
    assert!(passed(&checks[0]), "{checks:?}");
    assert!(checks[0].name.contains("subscription_token_file"));
    assert!(!passed(&checks[1]));
    assert!(checks[1].name.contains("private_key_file"));
    assert!(checks[1].text.contains("0640"), "{}", checks[1].text);

    std::fs::remove_file(&token).unwrap();
    let checks = doctor::credentials(&config);
    assert!(!passed(&checks[0]), "{checks:?}");

    // A configuration that names no file has nothing to check.
    let none = doctor::config(&scratch.write("none.toml", MINIMAL, 0o644))
        .1
        .unwrap();
    assert!(doctor::credentials(&none).is_empty());
}

#[test]
fn versions_are_read_from_what_git_and_bubblewrap_print() {
    assert_eq!(
        doctor::git_version("git version 2.47.3\n"),
        Some((2, 47, 3))
    );
    assert_eq!(
        doctor::git_version("git version 2.39.5 (Apple Git-154)"),
        Some((2, 39, 5))
    );
    assert_eq!(
        doctor::git_version("git version 2.50.1.windows.1"),
        Some((2, 50, 1))
    );
    assert_eq!(doctor::git_version("git version 2.48"), Some((2, 48, 0)));
    assert_eq!(doctor::git_version("bubblewrap 0.11.0"), None);
    assert_eq!(
        doctor::bwrap_version("bubblewrap 0.11.0\n"),
        Some((0, 11, 0))
    );
    assert_eq!(doctor::bwrap_version("git version 2.47.3"), None);
}

/// An executable shell script in `scratch` that prints `out` and exits
/// with `code`.
fn program(scratch: &Scratch, name: &str, out: &str, code: i32) -> PathBuf {
    scratch.write(
        name,
        &format!("#!/bin/sh\nprintf '%s\\n' '{out}'\nexit {code}\n"),
        0o755,
    )
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

#[test]
fn git_passes_from_the_version_that_4_4_names() {
    let scratch = Scratch::new("git");
    let check = |out: &str, code| block_on(doctor::git(Some(&program(&scratch, "git", out, code))));
    assert!(passed(&check("git version 2.47.3", 0)));
    assert!(passed(&check("git version 2.51.0", 0)));
    let old = check("git version 2.47.2", 0);
    assert!(!passed(&old));
    assert!(old.text.contains("2.47.3"), "{}", old.text);
    assert!(!passed(&check("nonsense", 0)));
    assert!(!passed(&check("git version 2.50.0", 1)));
    assert!(!passed(&block_on(doctor::git(None))));
}

#[test]
fn bubblewrap_passes_from_the_version_that_4_4_names() {
    let scratch = Scratch::new("bwrap");
    let check = |out: &str| {
        block_on(doctor::bubblewrap(Some(&program(
            &scratch, "bwrap", out, 0,
        ))))
    };
    assert!(passed(&check("bubblewrap 0.9.0")));
    assert!(passed(&check("bubblewrap 0.11.0")));
    let old = check("bubblewrap 0.8.0");
    assert!(!passed(&old));
    assert!(old.text.contains("0.9.0"), "{}", old.text);
    let missing = block_on(doctor::bubblewrap(None));
    assert!(!passed(&missing));
    assert!(missing.text.contains("bwrap"), "{}", missing.text);
}

/// A stand-in for `/proc/sys` that holds `values`, each a sysctl as a path
/// below it and its value.
fn sysctls(scratch: &Scratch, values: &[(&str, &str)]) -> PathBuf {
    let root = scratch.0.join("sys");
    let _ = std::fs::remove_dir_all(&root);
    for (name, value) in values {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("{value}\n")).unwrap();
    }
    root
}

#[test]
fn unprivileged_user_namespaces_are_read_from_the_sysctls() {
    let scratch = Scratch::new("userns");
    let check = |values: &[(&str, &str)]| doctor::user_namespaces(&sysctls(&scratch, values));
    let max = "user/max_user_namespaces";
    let clone = "kernel/unprivileged_userns_clone";
    let apparmor = "kernel/apparmor_restrict_unprivileged_userns";

    // The server's (P-24, docs/research/14-server-facts.md).
    assert!(passed(&check(&[(max, "60236"), (clone, "1")])));
    // A kernel without Debian's switch.
    assert!(passed(&check(&[(max, "60236")])));
    assert!(passed(&check(&[(max, "60236"), (apparmor, "0")])));

    for (values, named) in [
        (&[(max, "0"), (clone, "1")][..], "max_user_namespaces"),
        (&[(max, "60236"), (clone, "0")], "unprivileged_userns_clone"),
        (
            &[(max, "60236"), (apparmor, "1")],
            "apparmor_restrict_unprivileged_userns",
        ),
        (&[], "max_user_namespaces"),
        (&[(max, "many")], "max_user_namespaces"),
    ] {
        let failed = check(values);
        assert!(!passed(&failed), "{values:?}");
        assert!(failed.text.contains(named), "{}", failed.text);
    }

    // A sysctl that is there and cannot be read is no sysctl that is absent.
    let root = sysctls(&scratch, &[(max, "60236")]);
    std::fs::create_dir_all(root.join(clone)).unwrap();
    let unread = doctor::user_namespaces(&root);
    assert!(!passed(&unread));
    assert!(unread.text.contains("cannot read"), "{}", unread.text);
}

#[test]
fn the_agent_cli_is_present_and_gives_its_version() {
    let scratch = Scratch::new("agent");
    let cli = program(&scratch, "claude", "2.1.3 (Claude Code)", 0);
    let config = |cli: &Path| {
        let text = format!("{MINIMAL}[agent]\ncli = {cli:?}\n");
        doctor::config(&scratch.write("config.toml", &text, 0o644))
            .1
            .unwrap()
    };

    let check = block_on(doctor::agent_cli(&config(&cli)));
    assert!(passed(&check), "{check:?}");
    assert!(check.text.contains("2.1.3 (Claude Code)"), "{}", check.text);

    let missing = block_on(doctor::agent_cli(&config(&scratch.0.join("absent"))));
    assert!(!passed(&missing));
    assert!(missing.text.contains("absent"), "{}", missing.text);

    let plain = scratch.write("plain", "#!/bin/sh\necho 1.0\n", 0o644);
    let plain = block_on(doctor::agent_cli(&config(&plain)));
    assert!(!passed(&plain));
    assert!(plain.text.contains("not executable"), "{}", plain.text);

    let failing = program(&scratch, "broken", "oops", 2);
    assert!(!passed(&block_on(doctor::agent_cli(&config(&failing)))));

    let unnamed = doctor::config(&scratch.write("unnamed.toml", MINIMAL, 0o644))
        .1
        .unwrap();
    let unnamed = block_on(doctor::agent_cli(&unnamed));
    assert!(!passed(&unnamed));
    assert!(unnamed.text.contains("`[agent] cli`"), "{}", unnamed.text);
}

#[test]
fn the_report_passes_where_every_check_passes() {
    let pass = Check::pass("a", "fine".to_owned());
    let fail = Check::fail("b", "wrong".to_owned());
    let report = doctor::Report {
        checks: vec![pass.clone(), fail],
    };
    assert!(!report.ok());
    let text = report.text();
    assert!(text.contains("ok    a: fine\n"), "{text}");
    assert!(text.contains("FAIL  b: wrong\n"), "{text}");
    let json: serde_json::Value = serde_json::from_str(&report.json()).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["checks"][1]["name"], "b");
    assert_eq!(json["checks"][1]["ok"], false);
    assert_eq!(json["checks"][1]["text"], "wrong");
    assert!(doctor::Report { checks: vec![pass] }.ok());
}

//! The structural test of SPEC 19.1: the workspace holds exactly the crates in
//! the spec's table, each one's dependencies stay within its row, and each one
//! carries the repository's licence (D-17). It also holds the tree to one
//! configuration file for each tool of its checks (DEV-2b, D-49).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// Each crate and the workspace crates it may take as a normal dependency.
const ALLOWED: &[(&str, &[&str])] = &[
    ("factory-core", &[]),
    ("factory-store", &["factory-core"]),
    ("factory-proc", &[]),
    ("factory-sandbox", &["factory-proc", "factory-core"]),
    (
        "factory-git",
        &["factory-proc", "factory-sandbox", "factory-core"],
    ),
    ("factory-github", &["factory-core"]),
    (
        "factory-agent",
        &["factory-proc", "factory-sandbox", "factory-core"],
    ),
    ("factory-net", &["factory-core"]),
    (
        "factory-daemon",
        &[
            "factory-core",
            "factory-store",
            "factory-proc",
            "factory-sandbox",
            "factory-git",
            "factory-github",
            "factory-agent",
            "factory-net",
        ],
    ),
    ("factory-cli", &["factory-daemon"]),
    (
        "factory-testkit",
        &[
            "factory-core",
            "factory-proc",
            "factory-store",
            "factory-git",
            "factory-agent",
            "factory-github",
        ],
    ),
    ("xtask", &[]),
];

/// Dev-dependencies may additionally include this crate.
const TESTKIT: &str = "factory-testkit";

/// The I/O crates of SPEC 19.2, which `factory-core` must not depend on.
const CORE_FORBIDDEN: &[&str] = &[
    "tokio",
    "tokio-util",
    "process-wrap",
    "octocrab",
    "reqwest",
    "axum",
    "rusqlite",
];

/// The licence of every workspace crate (SPEC D-17).
const LICENSE: &str = "MIT OR Apache-2.0";

struct Dependency {
    name: String,
    dev: bool,
}

/// The output of `cargo metadata` for the workspace, without dependencies.
fn metadata() -> Value {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml");
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .expect("run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse cargo metadata")
}

/// Workspace members and their declared dependencies, from `cargo metadata`.
fn workspace() -> BTreeMap<String, Vec<Dependency>> {
    let metadata = metadata();
    let mut members = BTreeMap::new();
    for package in metadata["packages"].as_array().expect("packages array") {
        let name = package["name"].as_str().expect("package name").to_owned();
        let dependencies = package["dependencies"]
            .as_array()
            .expect("dependencies array")
            .iter()
            .map(|dependency| Dependency {
                name: dependency["name"]
                    .as_str()
                    .expect("dependency name")
                    .to_owned(),
                dev: dependency["kind"].as_str() == Some("dev"),
            })
            .collect();
        members.insert(name, dependencies);
    }
    members
}

#[test]
fn members_match_the_spec_table() {
    let actual: BTreeSet<String> = workspace().into_keys().collect();
    let expected: BTreeSet<String> = ALLOWED.iter().map(|(name, _)| (*name).to_owned()).collect();
    assert_eq!(
        actual, expected,
        "workspace members differ from the table in SPEC 19.1"
    );
}

#[test]
fn dependencies_follow_the_spec_table() {
    let workspace = workspace();
    let allowed: BTreeMap<&str, &[&str]> = ALLOWED.iter().copied().collect();

    let mut violations = Vec::new();
    for (name, dependencies) in &workspace {
        let permitted = allowed.get(name.as_str()).copied().unwrap_or_default();
        for dependency in dependencies {
            if !workspace.contains_key(&dependency.name) {
                continue;
            }
            let ok = permitted.contains(&dependency.name.as_str())
                || (dependency.dev && dependency.name == TESTKIT);
            if !ok {
                violations.push(format!("{name} -> {}", dependency.name));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "dependencies outside the table in SPEC 19.1: {violations:?}"
    );
}

#[test]
fn core_has_no_io_crates() {
    let workspace = workspace();
    let io: Vec<&str> = workspace["factory-core"]
        .iter()
        .filter(|dependency| !dependency.dev)
        .map(|dependency| dependency.name.as_str())
        .filter(|name| CORE_FORBIDDEN.contains(name))
        .collect();
    assert!(io.is_empty(), "factory-core depends on I/O crates: {io:?}");
}

#[test]
fn every_crate_carries_the_licence() {
    let metadata = metadata();
    let unlicensed: Vec<&str> = metadata["packages"]
        .as_array()
        .expect("packages array")
        .iter()
        .filter(|package| package["license"].as_str() != Some(LICENSE))
        .map(|package| package["name"].as_str().expect("package name"))
        .collect();
    assert!(
        unlicensed.is_empty(),
        "crates without the licence {LICENSE} (SPEC D-17): {unlicensed:?}"
    );
}

/// The names under which a tool reads configuration unasked beside its one
/// file, each with the tool: the plain names of SPEC DEV-2b, which fail at any
/// depth. Probe P-43 found them (D-49).
const SECOND_NAMES: &[(&str, &str)] = &[
    (".clippy.toml", "clippy"),
    (".rustfmt.toml", "rustfmt"),
    ("rust-toolchain", "rustup"),
    (".deny.toml", "cargo-deny"),
    ("deny.exceptions.toml", "cargo-deny"),
    (".deny.exceptions.toml", "cargo-deny"),
    ("_typos.toml", "typos"),
    (".typos.toml", "typos"),
    ("pyproject.toml", "typos"),
    // No configuration: it takes files out of what typos reads.
    (".ignore", "typos"),
    (".tool-versions", "mise"),
    (".miserc.toml", "mise"),
    (".miserc.local.toml", "mise"),
];

/// The tool that reads `path` unasked beside its one configuration file, if
/// any does (SPEC DEV-2b). `path` is relative to the repository's root. A name
/// is compared without regard to the case of ASCII letters, as the file system
/// of a Mac reads it.
fn tool_that_reads(path: &Path) -> Option<&'static str> {
    let components: Vec<String> = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();
    let (name, directories) = components.split_last()?;
    let name = name.as_str();
    let directories: Vec<&str> = directories.iter().map(String::as_str).collect();

    if let Some((_, tool)) = SECOND_NAMES.iter().find(|(second, _)| *second == name) {
        return Some(tool);
    }
    // An entry that is no directory where a tool looks for one. The walk hands
    // over files and symbolic links alone, and a tool reads through a link.
    match (directories.as_slice(), name) {
        ([], "mise" | ".mise") | ([".config"], "mise") => return Some("mise"),
        ([], ".config") => return Some("cargo-nextest"),
        (_, ".cargo") => return Some("cargo"),
        _ => {}
    }
    // The file that cargo read before `config.toml`, and still prefers.
    if name == "config" && directories.last() == Some(&".cargo") {
        return Some("cargo");
    }
    // `.mise.toml`, `mise.local.toml`, `mise.<environment>.toml` and the like.
    let named_for_mise = name != "mise.toml"
        && (name.starts_with("mise.") || name.starts_with(".mise."))
        && name.ends_with(".toml");
    // The directories and the second places that mise reads at the root.
    let in_a_place_of_mise = match directories.as_slice() {
        ["mise" | ".mise", ..] | [".config", "mise", ..] => true,
        [".config"] => matches!(name, "mise.toml" | "miserc.toml"),
        _ => false,
    };
    (named_for_mise || in_a_place_of_mise).then_some("mise")
}

/// Every file and symbolic link of the repository, relative to its root,
/// without what git and cargo keep there: `.git` and `target`.
fn tree() -> Vec<PathBuf> {
    fn walk(root: &Path, directory: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(directory).expect("read a directory of the tree") {
            let entry = entry.expect("read an entry of the tree");
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .expect("an entry lies below the root");
            if relative == Path::new(".git") || relative == Path::new("target") {
                continue;
            }
            // `file_type` does not follow a symbolic link, so a link is an entry
            // of its own and the walk never leaves the tree.
            if entry.file_type().expect("read an entry's type").is_dir() {
                walk(root, &path, found);
            } else {
                found.push(relative.to_owned());
            }
        }
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    walk(&root, &root, &mut found);
    found
}

#[test]
fn the_tree_holds_one_configuration_file_for_each_tool() {
    let found: Vec<String> = tree()
        .iter()
        .filter_map(|path| tool_that_reads(path).map(|tool| format!("{} ({tool})", path.display())))
        .collect();
    assert!(
        found.is_empty(),
        "files that a tool reads beside its configuration (SPEC DEV-2b): {found:?}"
    );
}

#[test]
fn a_file_that_a_tool_reads_beside_its_own_is_known_by_its_path() {
    let read_beside: &[(&str, &str)] = &[
        (".clippy.toml", "clippy"),
        ("crates/factory-core/.clippy.toml", "clippy"),
        (".Clippy.TOML", "clippy"),
        ("rust-toolchain", "rustup"),
        ("xtask/rust-toolchain", "rustup"),
        (".rustfmt.toml", "rustfmt"),
        ("crates/factory-git/src/.rustfmt.toml", "rustfmt"),
        (".deny.toml", "cargo-deny"),
        ("deny.exceptions.toml", "cargo-deny"),
        (".deny.exceptions.toml", "cargo-deny"),
        (".cargo/deny.exceptions.toml", "cargo-deny"),
        (".config/deny.exceptions.toml", "cargo-deny"),
        ("_typos.toml", "typos"),
        (".typos.toml", "typos"),
        ("pyproject.toml", "typos"),
        (".cargo/config", "cargo"),
        ("crates/factory-core/.cargo/config", "cargo"),
        (".mise.toml", "mise"),
        ("mise.local.toml", "mise"),
        (".mise.local.toml", "mise"),
        ("mise.ci.toml", "mise"),
        (".mise.ci.local.toml", "mise"),
        (".tool-versions", "mise"),
        (".miserc.toml", "mise"),
        (".miserc.local.toml", "mise"),
        (".config/miserc.toml", "mise"),
        (".config/mise.toml", "mise"),
        (".config/mise.local.toml", "mise"),
        (".config/mise/config.toml", "mise"),
        (".config/mise/conf.d/a.toml", "mise"),
        ("mise/config.toml", "mise"),
        ("mise/conf.d/a/mise.toml", "mise"),
        (".mise/config.local.toml", "mise"),
        // A symbolic link, or a file, where a tool looks for a directory.
        ("mise", "mise"),
        (".mise", "mise"),
        (".config/mise", "mise"),
        (".config", "cargo-nextest"),
        (".cargo", "cargo"),
        ("crates/factory-core/.cargo", "cargo"),
        (".ignore", "typos"),
        ("docs/.ignore", "typos"),
    ];
    for (path, tool) in read_beside {
        assert_eq!(tool_that_reads(Path::new(path)), Some(*tool), "{path}");
    }

    let first_or_unrelated = [
        "clippy.toml",
        "rust-toolchain.toml",
        "rustfmt.toml",
        "deny.toml",
        "typos.toml",
        "mise.toml",
        "xtask/mise.toml",
        ".cargo/config.toml",
        ".cargo/mutants.toml",
        ".config/nextest.toml",
        "docs/config",
        "crates/factory-core/src/mise/config.toml",
        "crates/factory-core/src/mise",
        "docs/.config",
        "crates/factory-core/src/config.toml",
        "docs/rust-toolchain.md",
    ];
    for path in first_or_unrelated {
        assert_eq!(tool_that_reads(Path::new(path)), None, "{path}");
    }
}

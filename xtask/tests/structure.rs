//! The structural test of SPEC 19.1: the workspace holds exactly the crates in
//! the spec's table, each one's dependencies stay within its row, and each one
//! carries the repository's licence (D-17).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
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

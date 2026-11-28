//! The `rust` implementation of `Ecosystem`: what `rust-toolchain.toml` names
//! and what the host looks for (SPEC WS-7, 9.5).

use factory_core::ecosystem::{Ecosystem, HostTriple, Part, PartKind, Rust, ToolchainLookup};

const LINUX_X86: HostTriple = HostTriple::X86_64UnknownLinuxGnu;

fn lookup(file: &str) -> ToolchainLookup {
    Rust.toolchain(file.as_bytes(), LINUX_X86)
        .unwrap_or_else(|error| panic!("{file:?} is refused: {error}"))
}

#[test]
fn a_full_version_names_rustups_directory_for_the_host() {
    let found = lookup("[toolchain]\nchannel = \"1.100.0\"\n");

    assert_eq!(found.name, "1.100.0");
    assert_eq!(found.directory, "1.100.0-x86_64-unknown-linux-gnu");
    assert_eq!(found.bin, "bin");
    assert_eq!(found.programs, ["bin/rustc", "bin/cargo"]);
    assert_eq!(found.record, "lib/rustlib/components");
    assert_eq!(
        found.parts,
        [Part {
            kind: PartKind::Toolchain,
            name: "rust-std-x86_64-unknown-linux-gnu".to_owned(),
            lines: vec!["rust-std-x86_64-unknown-linux-gnu".to_owned()],
        }]
    );
}

#[test]
fn cargo_keeps_its_home_below_the_home_and_builds_into_the_target_directory() {
    assert_eq!(
        Rust.build_output("/home/worker", "/target"),
        [
            ("CARGO_HOME", "/home/worker/.cargo".to_owned()),
            ("CARGO_TARGET_DIR", "/target".to_owned()),
        ]
    );
}

#[test]
fn the_registry_is_crates_io() {
    assert_eq!(
        Rust.registry_hosts(),
        ["index.crates.io", "static.crates.io"]
    );
}

#[test]
fn the_file_is_rust_toolchain_toml_at_the_root() {
    assert_eq!(Rust.toolchain_file(), "rust-toolchain.toml");
}

#[test]
fn the_host_triple_names_the_directory_and_the_standard_library() {
    let found = Rust
        .toolchain(
            b"[toolchain]\nchannel = \"1.97.1\"\n",
            HostTriple::Aarch64UnknownLinuxGnu,
        )
        .expect("a valid file");

    assert_eq!(found.directory, "1.97.1-aarch64-unknown-linux-gnu");
    assert_eq!(found.parts[0].lines, ["rust-std-aarch64-unknown-linux-gnu"]);
    assert_eq!(
        HostTriple::Aarch64UnknownLinuxGnu.to_string(),
        "aarch64-unknown-linux-gnu"
    );
}

#[test]
fn a_component_is_found_under_its_four_names_and_a_target_by_its_standard_library() {
    let found = lookup(
        "[toolchain]\nchannel = \"1.100.0\"\nprofile = \"minimal\"\n\
         components = [\"clippy\", \"rust-src\", \"llvm-tools-preview\"]\n\
         targets = [\"wasm32-wasip1\", \"thumbv7em-none-eabihf\", \"x86_64-unknown-linux-musl\"]\n",
    );

    let component = |name: &str, lines: [&str; 4]| Part {
        kind: PartKind::Component,
        name: name.to_owned(),
        lines: lines.map(str::to_owned).to_vec(),
    };
    let target = |name: &str| Part {
        kind: PartKind::Target,
        name: name.to_owned(),
        lines: vec![format!("rust-std-{name}")],
    };
    assert_eq!(
        found.parts[1..],
        [
            component(
                "clippy",
                [
                    "clippy",
                    "clippy-x86_64-unknown-linux-gnu",
                    "clippy-preview",
                    "clippy-preview-x86_64-unknown-linux-gnu",
                ]
            ),
            component(
                "rust-src",
                [
                    "rust-src",
                    "rust-src-x86_64-unknown-linux-gnu",
                    "rust-src-preview",
                    "rust-src-preview-x86_64-unknown-linux-gnu",
                ]
            ),
            component(
                "llvm-tools-preview",
                [
                    "llvm-tools-preview",
                    "llvm-tools-preview-x86_64-unknown-linux-gnu",
                    "llvm-tools-preview-preview",
                    "llvm-tools-preview-preview-x86_64-unknown-linux-gnu",
                ]
            ),
            target("wasm32-wasip1"),
            target("thumbv7em-none-eabihf"),
            target("x86_64-unknown-linux-musl"),
        ]
    );
}

#[test]
fn a_profile_is_tested_for_nothing_and_the_lists_may_be_empty() {
    for (file, directory, parts) in [
        (
            "[toolchain]\nchannel = \"1.100.0\"\nprofile = \"default\"\n",
            "1.100.0",
            1,
        ),
        (
            "[toolchain]\nchannel = \"1.100.0\"\nprofile = 3\n",
            "1.100.0",
            1,
        ),
        (
            "[toolchain]\nchannel = \"1.100.0\"\ncomponents = []\ntargets = []\n",
            "1.100.0",
            1,
        ),
        ("toolchain = { channel = \"1.100.0\" }\n", "1.100.0", 1),
        ("[toolchain]\nchannel = \"0.0.0\"\n", "0.0.0", 1),
        (
            "[toolchain]\nchannel = \"10.200.30\"\ncomponents = [\"a\", \"a1-b2\"]\ntargets = [\"a._-9\"]\n",
            "10.200.30",
            4,
        ),
    ] {
        let found = lookup(file);
        assert_eq!(
            found.directory,
            format!("{directory}-x86_64-unknown-linux-gnu"),
            "{file:?}"
        );
        assert_eq!(found.parts.len(), parts, "{file:?}");
    }
}

/// The problem that the host reports for `file`, which it cannot resolve.
fn refused(file: &[u8]) -> String {
    let Err(error) = Rust.toolchain(file, LINUX_X86) else {
        panic!("{:?} is resolved", String::from_utf8_lossy(file));
    };
    assert_eq!(error.file, "rust-toolchain.toml");
    assert!(
        error
            .to_string()
            .starts_with("`rust-toolchain.toml` names no toolchain that the host can look for: "),
        "{error}"
    );
    error.problem
}

#[test]
fn a_file_that_the_host_cannot_resolve_says_what_is_wrong_with_it() {
    let cases: &[(&[u8], &str)] = &[
        (b"[toolchain\n", "TOML"),
        (b"\xFF", "UTF-8"),
        (b"", "`toolchain`"),
        (b"channel = \"1.100.0\"\n", "`channel`"),
        (b"[toolchain]\n", "`channel`"),
        (b"[toolchain]\ncomponents = [\"clippy\"]\n", "`channel`"),
        (
            b"[toolchain]\nchannel = \"1.100.0\"\npath = \"/opt/rust\"\n",
            "`path`",
        ),
        (b"[toolchain]\nchannel = \"1.100.0\"\n[other]\n", "`other`"),
        (
            b"[toolchain]\nchannel = \"1.100.0\"\nextra = 1\n",
            "`extra`",
        ),
        (b"[toolchain]\nchannel = 1\n", "integer"),
        (
            b"[toolchain]\nchannel = \"1.100.0\"\ncomponents = \"clippy\"\n",
            "sequence",
        ),
        (
            b"[toolchain]\nchannel = \"1.100.0\"\ntargets = [1]\n",
            "integer",
        ),
    ];
    for (file, said) in cases {
        let problem = refused(file);
        assert!(
            problem.contains(said),
            "{:?}: {problem}",
            String::from_utf8_lossy(file)
        );
    }
}

#[test]
fn a_channel_that_is_no_full_version_is_refused() {
    for channel in [
        "stable",
        "beta",
        "nightly",
        "1.100",
        "1",
        "1.100.0.1",
        "1..0",
        ".1.0",
        "1.0.",
        "1.100.x",
        "v1.100.0",
        "1.100.0-x86_64-unknown-linux-gnu",
        "nightly-2026-11-20",
        "1.100.0 ",
        "１.100.0",
        "",
    ] {
        let problem = refused(format!("[toolchain]\nchannel = {channel:?}\n").as_bytes());
        assert!(problem.contains("a full version"), "{channel:?}: {problem}");
        assert!(
            problem.contains(&format!("{channel:?}")),
            "{channel:?}: {problem}"
        );
    }
}

#[test]
fn a_component_or_a_target_of_another_form_is_refused() {
    for component in [
        "", "Clippy", "1clippy", "-clippy", "clip_py", "clip.py", "clip py", "../x", "é",
    ] {
        let problem = refused(
            format!(
                "[toolchain]\nchannel = \"1.100.0\"\ncomponents = [\"clippy\", {component:?}]\n"
            )
            .as_bytes(),
        );
        assert!(problem.contains("a component"), "{component:?}: {problem}");
    }
    for target in ["", "X86_64", "1x", ".x", "_x", "-x", "x/y", "x y", "../x"] {
        let problem = refused(
            format!("[toolchain]\nchannel = \"1.100.0\"\ntargets = [{target:?}]\n").as_bytes(),
        );
        assert!(problem.contains("a target"), "{target:?}: {problem}");
    }
}

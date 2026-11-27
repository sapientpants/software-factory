//! The lookup of a toolchain and the check tools on the host's disk (SPEC
//! WS-7, WS-7a, 19.1). Each test lays out the directories it reads, and none
//! needs an installed toolchain.

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use factory_core::check_tool::CheckTool;
use factory_core::ecosystem::{Ecosystem, HostTriple, Rust, ToolchainLookup};
use factory_daemon::host::{Directories, Missing, resolve};

const HOST: HostTriple = HostTriple::X86_64UnknownLinuxGnu;
const FILE: &[u8] =
    b"[toolchain]\nchannel = \"1.100.0\"\ncomponents = [\"clippy\", \"rust-src\"]\ntargets = [\"wasm32-wasip1\"]\n";

/// A fresh directory for one test, removed when the value is dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Self {
        let path = std::env::temp_dir().join(format!("sf-host-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path)
            .unwrap_or_else(|error| panic!("create the scratch directory: {error}"));
        Self(
            path.canonicalize()
                .unwrap_or_else(|error| panic!("the scratch directory has a path: {error}")),
        )
    }

    fn toolchains(&self) -> PathBuf {
        self.0.join("toolchains")
    }

    fn tools(&self) -> PathBuf {
        self.0.join("tools")
    }

    fn directories(&self) -> (PathBuf, PathBuf) {
        (self.toolchains(), self.tools())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn lookup(file: &[u8]) -> ToolchainLookup {
    Rust.toolchain(file, HOST)
        .unwrap_or_else(|error| panic!("a valid file: {error}"))
}

/// Writes an executable file, and the directories above it.
fn executable(path: &Path) {
    fs::create_dir_all(path.parent().unwrap_or_else(|| panic!("a parent")))
        .unwrap_or_else(|error| panic!("create its directory: {error}"));
    fs::write(path, "#!/bin/sh\n").unwrap_or_else(|error| panic!("write the program: {error}"));
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("make it executable: {error}"));
}

/// Lays out at `directory` the toolchain that `file` names, as rustup leaves
/// it, with its record holding `lines`.
fn lay_out_toolchain(directory: &Path, file: &[u8], lines: &[&str]) {
    let lookup = lookup(file);
    for program in &lookup.programs {
        executable(&directory.join(program));
    }
    let record = directory.join(&lookup.record);
    fs::create_dir_all(record.parent().unwrap_or_else(|| panic!("a parent")))
        .unwrap_or_else(|error| panic!("create its directory: {error}"));
    fs::write(record, lines.join("\n") + "\n")
        .unwrap_or_else(|error| panic!("write the record: {error}"));
}

/// Every line that rustup records for the toolchain of [`FILE`].
const FULL: &[&str] = &[
    "clippy-preview-x86_64-unknown-linux-gnu",
    "rust-std-x86_64-unknown-linux-gnu",
    "rust-std-wasm32-wasip1",
    "rust-src",
];

fn tool(name: &str, version: &str) -> CheckTool {
    CheckTool::new(name, version).unwrap_or_else(|error| panic!("a valid check tool: {error}"))
}

fn lay_out_tool(tools: &Path, name: &str, version: &str) -> PathBuf {
    let directory = tools.join(name).join(version);
    executable(&directory.join("bin").join(name));
    directory
}

fn run(
    toolchains: Option<&Path>,
    tools_dir: Option<&Path>,
    file: Option<&[u8]>,
    tools: &[CheckTool],
) -> Result<factory_daemon::host::HostPaths, Vec<Missing>> {
    resolve(
        &Rust,
        HOST,
        Directories {
            toolchains,
            tools: tools_dir,
        },
        file,
        tools,
    )
}

/// The reports of what is missing, each as it is said.
fn said(missing: &[Missing]) -> Vec<String> {
    missing.iter().map(ToString::to_string).collect()
}

#[test]
fn a_present_toolchain_and_check_tools_give_the_paths_of_the_sandbox() {
    let scratch = Scratch::new("present");
    let (toolchains, tools_dir) = scratch.directories();
    let toolchain = toolchains.join("1.100.0-x86_64-unknown-linux-gnu");
    lay_out_toolchain(&toolchain, FILE, FULL);
    let typos = lay_out_tool(&tools_dir, "typos", "1.52.2");
    let deny = lay_out_tool(&tools_dir, "deny", "0.22.1");
    // Another toolchain and another version beside them, which nothing names.
    lay_out_toolchain(
        &toolchains.join("1.99.0-x86_64-unknown-linux-gnu"),
        FILE,
        FULL,
    );
    lay_out_tool(&tools_dir, "typos", "1.0.0");

    let paths = run(
        Some(&toolchains),
        Some(&tools_dir),
        Some(FILE),
        &[tool("typos", "1.52.2"), tool("deny", "0.22.1")],
    )
    .expect("everything is present");

    assert_eq!(paths.toolchain.directory, toolchain);
    assert_eq!(paths.toolchain.bin, toolchain.join("bin"));
    let tools: Vec<(String, &Path, &Path)> = paths
        .tools
        .iter()
        .map(|found| {
            (
                found.tool.to_string(),
                found.directory.as_path(),
                found.bin.as_path(),
            )
        })
        .collect();
    assert_eq!(
        tools,
        [
            (
                "deny 0.22.1".to_owned(),
                deny.as_path(),
                deny.join("bin").as_path()
            ),
            (
                "typos 1.52.2".to_owned(),
                typos.as_path(),
                typos.join("bin").as_path()
            ),
        ]
    );
    // The toolchain's `bin` first, then each tool's in order of name.
    assert_eq!(
        paths.worker_path(),
        [toolchain.join("bin"), deny.join("bin"), typos.join("bin")]
    );
    assert_eq!(paths.read_only(), [toolchain.clone(), deny, typos]);
}

#[test]
fn no_shim_of_rustup_or_mise_is_among_the_paths() {
    let scratch = Scratch::new("no-shim");
    let (toolchains, tools_dir) = scratch.directories();
    lay_out_toolchain(
        &toolchains.join("1.100.0-x86_64-unknown-linux-gnu"),
        FILE,
        FULL,
    );
    lay_out_tool(&tools_dir, "typos", "1.52.2");
    // A rustup home and a mise installation around the two directories, with
    // their shims, as an operator's account holds them.
    for shim in [
        "bin/rustup",
        "bin/rustc",
        "shims/rustc",
        "shims/typos",
        "bin/mise",
    ] {
        executable(&scratch.0.join(shim));
    }

    let paths = run(
        Some(&toolchains),
        Some(&tools_dir),
        Some(FILE),
        &[tool("typos", "1.52.2")],
    )
    .expect("everything is present");

    for path in paths.worker_path().iter().chain(&paths.read_only()) {
        assert!(
            path.starts_with(toolchains.join("1.100.0-x86_64-unknown-linux-gnu"))
                || path.starts_with(tools_dir.join("typos/1.52.2")),
            "{} lies outside the toolchain and the tool",
            path.display()
        );
        for entry in fs::read_dir(path).expect("a directory") {
            let name = entry.expect("an entry").file_name();
            assert!(
                !matches!(name.to_str(), Some("rustup" | "mise")),
                "{}",
                path.display()
            );
        }
    }
}

#[test]
fn a_symbolic_link_in_the_toolchains_directory_is_followed_to_the_directory_it_leads_to() {
    let scratch = Scratch::new("link");
    let toolchains = scratch.toolchains();
    let real = scratch.0.join("elsewhere/rust-1.100.0");
    lay_out_toolchain(&real, FILE, FULL);
    // A program of the toolchain that is itself a link.
    let rustc = real.join("bin/rustc");
    fs::rename(&rustc, real.join("bin/rustc-1.100.0")).expect("move the program");
    symlink("rustc-1.100.0", &rustc).expect("link the program");
    fs::create_dir_all(&toolchains).expect("create the toolchains directory");
    symlink(&real, toolchains.join("1.100.0-x86_64-unknown-linux-gnu"))
        .expect("link the toolchain");

    let paths = run(Some(&toolchains), None, Some(FILE), &[]).expect("the toolchain is present");

    assert_eq!(paths.toolchain.directory, real);
    assert_eq!(paths.worker_path(), [real.join("bin")]);
    assert_eq!(paths.read_only(), [real]);
}

#[test]
fn a_missing_component_and_target_are_each_reported_by_name() {
    let scratch = Scratch::new("component");
    let toolchains = scratch.toolchains();
    lay_out_toolchain(
        &toolchains.join("1.100.0-x86_64-unknown-linux-gnu"),
        FILE,
        &[
            "rust-std-x86_64-unknown-linux-gnu",
            "rustfmt-preview-x86_64-unknown-linux-gnu",
        ],
    );

    let missing = run(Some(&toolchains), None, Some(FILE), &[]).expect_err("parts are missing");

    assert_eq!(
        said(&missing),
        [
            "the component `clippy` of the toolchain 1.100.0 is missing",
            "the component `rust-src` of the toolchain 1.100.0 is missing",
            "the target `wasm32-wasip1` of the toolchain 1.100.0 is missing",
        ]
    );
    assert_eq!(
        missing[0],
        Missing::Component {
            toolchain: "1.100.0".to_owned(),
            component: "clippy".to_owned()
        }
    );
    assert_eq!(
        missing[2],
        Missing::Target {
            toolchain: "1.100.0".to_owned(),
            target: "wasm32-wasip1".to_owned()
        }
    );
}

#[test]
fn a_component_is_present_under_each_of_its_names() {
    let file = b"[toolchain]\nchannel = \"1.100.0\"\ncomponents = [\"clippy\"]\n";
    for line in [
        "clippy",
        "clippy-x86_64-unknown-linux-gnu",
        "clippy-preview",
        "clippy-preview-x86_64-unknown-linux-gnu",
    ] {
        let scratch = Scratch::new(&format!("names-{line}"));
        let toolchains = scratch.toolchains();
        lay_out_toolchain(
            &toolchains.join("1.100.0-x86_64-unknown-linux-gnu"),
            file,
            &["rust-std-x86_64-unknown-linux-gnu", line],
        );
        assert!(
            run(Some(&toolchains), None, Some(file), &[]).is_ok(),
            "{line}"
        );
    }
    let scratch = Scratch::new("names-other");
    let toolchains = scratch.toolchains();
    lay_out_toolchain(
        &toolchains.join("1.100.0-x86_64-unknown-linux-gnu"),
        file,
        &[
            "rust-std-x86_64-unknown-linux-gnu",
            "clippy-aarch64-unknown-linux-gnu",
            " clippy",
            "clippy-preview-",
        ],
    );
    assert_eq!(
        said(&run(Some(&toolchains), None, Some(file), &[]).expect_err("no line names it")),
        ["the component `clippy` of the toolchain 1.100.0 is missing"]
    );
}

#[test]
fn a_missing_toolchain_is_reported_by_name_with_what_is_wrong() {
    let scratch = Scratch::new("toolchain");
    let toolchains = scratch.toolchains();
    let directory = toolchains.join("1.100.0-x86_64-unknown-linux-gnu");
    let plain = b"[toolchain]\nchannel = \"1.100.0\"\n";
    let reported = |test: &str| -> Vec<String> {
        said(&run(Some(&toolchains), None, Some(plain), &[]).expect_err(test))
    };

    fs::create_dir_all(&toolchains).expect("create the toolchains directory");
    let absent = reported("no directory");
    assert_eq!(absent.len(), 1);
    assert!(
        absent[0].starts_with(&format!(
            "the toolchain 1.100.0 is missing: `{}` cannot be found: ",
            directory.display()
        )),
        "{absent:?}"
    );

    fs::write(&directory, "").expect("write a file in its place");
    assert_eq!(
        reported("a file"),
        [format!(
            "the toolchain 1.100.0 is missing: `{}` is no directory",
            directory.display()
        )]
    );
    fs::remove_file(&directory).expect("remove the file");

    // The other toolchain's name, and that of a host of another architecture.
    lay_out_toolchain(
        &toolchains.join("1.100-x86_64-unknown-linux-gnu"),
        plain,
        FULL,
    );
    lay_out_toolchain(
        &toolchains.join("1.100.0-aarch64-unknown-linux-gnu"),
        plain,
        FULL,
    );
    assert_eq!(reported("other names").len(), 1);

    fs::create_dir_all(&directory).expect("create the directory");
    let empty = reported("an empty directory");
    assert_eq!(empty.len(), 3, "{empty:?}");
    let programs = lookup(plain).programs;
    for (report, program) in empty.iter().zip(&programs) {
        assert_eq!(
            report,
            &format!(
                "the toolchain 1.100.0 is missing: `{program}` is no regular file in the toolchain's directory that the factory's user may execute"
            )
        );
    }
    assert!(
        empty[2].starts_with(
            "the toolchain 1.100.0 is missing: `lib/rustlib/components` cannot be read: "
        ),
        "{empty:?}"
    );

    lay_out_toolchain(
        &directory,
        plain,
        &["rust-std-aarch64-unknown-linux-gnu", "rust-src"],
    );
    assert_eq!(
        reported("no standard library of the host"),
        [
            "the toolchain 1.100.0 is missing: `lib/rustlib/components` lacks the line `rust-std-x86_64-unknown-linux-gnu`"
        ]
    );

    lay_out_toolchain(&directory, plain, FULL);
    let program = directory.join(&programs[1]);
    fs::set_permissions(&program, fs::Permissions::from_mode(0o644)).expect("take its mode");
    assert_eq!(
        reported("a program that nobody may execute"),
        [format!(
            "the toolchain 1.100.0 is missing: `{}` is no regular file in the toolchain's directory that the factory's user may execute",
            programs[1]
        )]
    );
    fs::remove_file(&program).expect("remove the program");
    fs::create_dir(&program).expect("a directory in its place");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).expect("an executable mode");
    assert_eq!(reported("a directory in place of a program").len(), 1);
    fs::remove_dir(&program).expect("remove the directory");
    symlink("nowhere", &program).expect("a dangling link");
    assert_eq!(reported("a dangling link in place of a program").len(), 1);
    // A link to a program outside the toolchain's directory, which the
    // sandbox does not show.
    fs::remove_file(&program).expect("remove the link");
    let outside = scratch.0.join("outside/program");
    executable(&outside);
    symlink(&outside, &program).expect("a link that leads out");
    assert_eq!(
        reported("a link that leads out of the toolchain"),
        [format!(
            "the toolchain 1.100.0 is missing: `{}` is no regular file in the toolchain's directory that the factory's user may execute",
            programs[1]
        )]
    );
}

#[test]
fn without_a_toolchains_directory_or_a_file_no_toolchain_is_present() {
    assert_eq!(
        said(&run(None, None, Some(FILE), &[]).expect_err("no directory")),
        [
            "the toolchain 1.100.0 is missing: the operator configuration names no toolchains directory"
        ]
    );
    let scratch = Scratch::new("no-file");
    let toolchains = scratch.toolchains();
    let missing = run(Some(&toolchains), None, None, &[]).expect_err("no file");
    let file = Rust.toolchain_file();
    assert_eq!(missing, [Missing::NoToolchainFile { file }]);
    assert_eq!(
        said(&missing),
        [format!(
            "the toolchain is missing: the tree holds no `{file}`, so it names none"
        )]
    );
    let bad = b"[toolchain]\nchannel = \"stable\"\n";
    let missing = run(Some(&toolchains), None, Some(bad), &[]).expect_err("a bad file");
    assert_eq!(
        missing,
        [Missing::ToolchainFile(
            Rust.toolchain(bad, HOST).expect_err("a bad file")
        )]
    );
    assert!(
        said(&missing)[0].starts_with(&format!(
            "the toolchain is missing: `{file}` names no toolchain that the host can look for: "
        )),
        "{missing:?}"
    );
}

#[test]
fn a_missing_check_tool_is_reported_by_name_and_version() {
    let scratch = Scratch::new("tools");
    let (toolchains, tools_dir) = scratch.directories();
    lay_out_toolchain(
        &toolchains.join("1.100.0-x86_64-unknown-linux-gnu"),
        FILE,
        FULL,
    );
    lay_out_tool(&tools_dir, "present", "1.0.0");
    // Another version of the tool alone.
    lay_out_tool(&tools_dir, "other-version", "1.0.0");
    // A `bin` with no executable file in it: a plain file, a directory, and a
    // link that leads nowhere.
    let unexecutable = tools_dir.join("unexecutable/1.0.0/bin");
    fs::create_dir_all(unexecutable.join("subdirectory")).expect("create its bin");
    fs::write(unexecutable.join("plain"), "").expect("write a plain file");
    symlink("nowhere", unexecutable.join("dangling")).expect("a dangling link");
    // A directory with no `bin`.
    fs::create_dir_all(tools_dir.join("no-bin/1.0.0/lib")).expect("create its directory");
    // An executable linked into `bin` from outside the tool's directory,
    // which the sandbox does not show.
    executable(&scratch.0.join("elsewhere/linked"));
    fs::create_dir_all(tools_dir.join("linked/1.0.0/bin")).expect("create its bin");
    symlink(
        scratch.0.join("elsewhere/linked"),
        tools_dir.join("linked/1.0.0/bin/linked"),
    )
    .expect("link the tool");
    // An executable linked into `bin` from inside the tool's directory.
    executable(&tools_dir.join("inside/1.0.0/libexec/inside"));
    fs::create_dir_all(tools_dir.join("inside/1.0.0/bin")).expect("create its bin");
    symlink(
        "../libexec/inside",
        tools_dir.join("inside/1.0.0/bin/inside"),
    )
    .expect("link the tool");

    let missing = run(
        Some(&toolchains),
        Some(&tools_dir),
        Some(FILE),
        &[
            tool("present", "1.0.0"),
            tool("unexecutable", "1.0.0"),
            tool("absent", "2.0.0"),
            tool("other-version", "2.0.0"),
            tool("no-bin", "1.0.0"),
            tool("linked", "1.0.0"),
            tool("inside", "1.0.0"),
        ],
    )
    .expect_err("tools are missing");

    let bin = |name: &str, version: &str| tools_dir.join(name).join(version).join("bin");
    let said = said(&missing);
    assert_eq!(said.len(), 5, "{said:?}");
    // In order of name.
    assert!(
        said[0].starts_with(&format!(
            "the check tool absent 2.0.0 is missing: `{}` cannot be found: ",
            tools_dir.join("absent/2.0.0").display()
        )),
        "{said:?}"
    );
    assert_eq!(
        said[1],
        format!(
            "the check tool linked 1.0.0 is missing: `{}` holds no executable file that lies in `{}`",
            bin("linked", "1.0.0").display(),
            tools_dir.join("linked/1.0.0").display()
        )
    );
    assert!(
        said[2].starts_with(&format!(
            "the check tool no-bin 1.0.0 is missing: `{}` cannot be read: ",
            bin("no-bin", "1.0.0").display()
        )),
        "{said:?}"
    );
    assert!(
        said[3].starts_with("the check tool other-version 2.0.0 is missing: "),
        "{said:?}"
    );
    assert_eq!(
        said[4],
        format!(
            "the check tool unexecutable 1.0.0 is missing: `{}` holds no executable file that lies in `{}`",
            unexecutable.display(),
            tools_dir.join("unexecutable/1.0.0").display()
        )
    );
    assert!(
        matches!(&missing[4], Missing::Tool { tool: found, .. } if *found == tool("unexecutable", "1.0.0"))
    );
}

#[test]
fn without_a_tools_directory_no_check_tool_is_present() {
    let scratch = Scratch::new("no-tools");
    let toolchains = scratch.toolchains();
    lay_out_toolchain(
        &toolchains.join("1.100.0-x86_64-unknown-linux-gnu"),
        FILE,
        FULL,
    );

    let missing = run(
        Some(&toolchains),
        None,
        Some(FILE),
        &[tool("b", "1"), tool("a", "2")],
    )
    .expect_err("no tools directory");

    assert_eq!(
        said(&missing),
        [
            "the check tool a 2 is missing: the operator configuration names no tools directory",
            "the check tool b 1 is missing: the operator configuration names no tools directory",
        ]
    );
    assert!(run(Some(&toolchains), None, Some(FILE), &[]).is_ok());
}

#[test]
fn what_is_missing_of_the_toolchain_and_of_the_tools_is_reported_together() {
    let missing = run(None, None, Some(FILE), &[tool("typos", "1.52.2")]).expect_err("all missing");

    assert_eq!(
        said(&missing),
        [
            "the toolchain 1.100.0 is missing: the operator configuration names no toolchains directory",
            "the check tool typos 1.52.2 is missing: the operator configuration names no tools directory",
        ]
    );
}

#[test]
fn a_symbolic_link_in_the_tools_directory_is_followed_to_the_directory_it_leads_to() {
    let scratch = Scratch::new("tool-link");
    let (toolchains, tools_dir) = scratch.directories();
    lay_out_toolchain(
        &toolchains.join("1.100.0-x86_64-unknown-linux-gnu"),
        FILE,
        FULL,
    );
    let real = lay_out_tool(&scratch.0.join("opt"), "typos", "1.52.2");
    fs::create_dir_all(tools_dir.join("typos")).expect("create the tool's directory");
    symlink(&real, tools_dir.join("typos/1.52.2")).expect("link the version");

    let paths = run(
        Some(&toolchains),
        Some(&tools_dir),
        Some(FILE),
        &[tool("typos", "1.52.2")],
    )
    .expect("the tool is present");

    assert_eq!(paths.tools[0].directory, real);
    assert_eq!(paths.tools[0].bin, real.join("bin"));
}

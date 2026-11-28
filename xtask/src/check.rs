//! `cargo xtask check`: every check of the factory repository behind one
//! command (SPEC DEV-1).

use std::ffi::OsString;
use std::fmt;
use std::path::Path;
use std::process::{Command, ExitCode};

use crate::mutants;
use crate::vm::{self, Vm};

/// The checks of SPEC DEV-2 that exist so far. The rest of that set is added
/// here as it is built, and nowhere else (DEV-1).
fn steps() -> Vec<Step> {
    vec![
        Step::new("cargo", &["fmt", "--all", "--check"]),
        // Over code and documents, with `typos.toml`, which it finds by
        // itself; no argument names it (DEV-2b).
        Step::new("typos", &[]),
        // The check of local Markdown links (DEV-2, 19.2).
        Step::new("cargo", &["xtask", "links"]),
        // The cross-reference check of ids and section numbers (DEV-2c).
        Step::new("cargo", &["xtask", "references"]),
        // DEV-2a. The advisories read the database that `cargo xtask fetch`
        // fetched, and fail where it is missing or too old.
        Step::new("cargo", &["deny", "--locked", "--offline", "check"]).on_failure(
            "where `cargo deny` says that the advisory database is missing or stale, \
             run `cargo xtask fetch` and then the check again (DEV-14)",
        ),
        // The unused-dependency check (19.2).
        Step::new("cargo", &["shear", "--locked", "--offline"]),
        Step::new(
            "cargo",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
        )
        .also_outside_the_vm(),
        // The tests and the doc tests, each word for word a verify command of
        // 11.2 (BOOT-6). nextest runs no doc test.
        Step::new("cargo", &["nextest", "run", "--workspace", "--locked"]).also_outside_the_vm(),
        // The sandbox tests of 19.4 at level `process`, a step of their own:
        // those of the backend, and those of `sf sandbox exec`, which joins
        // it with the egress proxy (CLI-6).
        // They are ignored unless the variable says the level, so the run
        // above runs none of them (DEV-2a). A check that creates a sandbox
        // does not run in a worker's (SEC-6b), so there the step is not run
        // (DEV-11).
        Step::new(
            "cargo",
            &[
                "nextest",
                "run",
                "--locked",
                "--package",
                "factory-sandbox",
                "--package",
                "factory-daemon",
                "--test",
                "sandbox",
            ],
        )
        .with_variable(SANDBOX_TESTS.0, SANDBOX_TESTS.1)
        .not_run_in_a_worker(
            "a sandbox test creates a sandbox, which a worker's sandbox does not allow (SEC-6b)",
        ),
        Step::new("cargo", &["test", "--doc", "--workspace", "--locked"]).also_outside_the_vm(),
        // Warnings denied, the lint of broken intra-doc links among them (DEV-2).
        Step::new("cargo", &["doc", "--workspace", "--no-deps", "--locked"])
            .with_variable("RUSTDOCFLAGS", "-D warnings"),
        // `cargo mutants --in-diff` on the diff from the base to the work tree
        // (DEV-2a). The task finds the base and runs the tool, so the version
        // check asks the tool and not the task.
        Step::new("cargo", &["xtask", "mutants"]).running("mutants"),
        // Coverage, reported without a floor (DEV-2).
        Step::new("cargo", &["llvm-cov", "nextest", "--workspace", "--locked"]),
    ]
}

/// The variable that makes the sandbox tests of level `process` ordinary
/// tests, with its value. Without it nextest sees them as ignored.
pub const SANDBOX_TESTS: (&str, &str) = ("SF_SANDBOX_TESTS", "process");

/// The steps that run on macOS outside the development VM, after the whole set
/// has run in it: clippy and the tests a second time, for the macOS build
/// (DEV-3).
fn macos_steps() -> Vec<Step> {
    steps()
        .into_iter()
        .filter(|step| step.outside_the_vm)
        .collect()
}

pub fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    if let Err(error) = std::env::set_current_dir(&root) {
        eprintln!("xtask check: cannot enter the repository's root: {error}");
        return ExitCode::FAILURE;
    }
    let outcome = if cfg!(target_os = "macos") {
        // Every step has its run in the VM, so none is not run (DEV-3).
        let vm = Vm::new("limactl", vm::NAME);
        in_the_vm(&root, &vm, std::env::var_os("SF_BASE_SHA"))
            .and_then(|()| at_the_pins(&macos_steps(), false))
    } else {
        at_the_pins(
            &steps(),
            is_a_worker(std::env::var_os("SF_SANDBOX").as_deref()),
        )
    };
    match outcome {
        Ok(not_run) => {
            eprintln!("xtask check: {}", passed(&not_run));
            ExitCode::SUCCESS
        }
        Err(failure) => {
            eprintln!("xtask check: {failure}");
            ExitCode::FAILURE
        }
    }
}

/// Whether `sf_sandbox`, the value of `SF_SANDBOX`, names the level of a
/// worker's sandbox: the command knows a worker's sandbox by that variable
/// alone (AG-11a, DEV-11).
pub fn is_a_worker(sf_sandbox: Option<&std::ffi::OsStr>) -> bool {
    sf_sandbox.is_some_and(|level| level == "process" || level == "hardened")
}

/// The last line of a check that passed: it names every step that was not
/// run once more (DEV-11).
fn passed(not_run: &[String]) -> String {
    if not_run.is_empty() {
        "passed".to_owned()
    } else {
        format!("passed; not run: {}", not_run.join(", "))
    }
}

/// Runs the steps once each check tool among them reports the version that
/// `mise.toml` pins (DEV-2). In a worker's sandbox the steps that do not run
/// there are not run. It returns the steps that were not run.
fn at_the_pins(steps: &[Step], worker: bool) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string("mise.toml")
        .map_err(|error| format!("cannot read mise.toml: {error}"))?;
    check_versions(steps, &pins(&text))?;
    run(steps, worker)
}

/// On macOS, the whole set in the development VM (DEV-3): the VM is created
/// and started where it has to be, and shown the work tree at `root`.
/// `sf_base_sha` is the value of `SF_BASE_SHA`, where it is set.
fn in_the_vm(root: &Path, vm: &Vm, sf_base_sha: Option<OsString>) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve the repository's root: {error}"))?;
    // The copy in the VM holds no history, so the Mac finds the base of the
    // mutation step and gives the VM the diff (DEV-2a).
    let diff = mutants::diff_of_the_work_tree(&root, sf_base_sha)?;
    vm.ensure(&root.join("lima.yaml"))?;
    vm.check(&root, &diff)
}

/// The tools that `mise.toml` pins, each with its version: the lines
/// `name = "version"` of its table `[tools]`, a quoted name unquoted.
fn pins(text: &str) -> Vec<(String, String)> {
    let mut in_tools = false;
    let mut pins = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_tools = line == "[tools]";
        } else if let (true, false, Some((name, version))) =
            (in_tools, line.starts_with('#'), line.split_once('='))
        {
            let unquote = |word: &str| word.trim().trim_matches('"').to_owned();
            pins.push((unquote(name), unquote(version)));
        }
    }
    pins
}

/// Fails by name where a step runs a cargo subcommand that `pins` holds, as
/// `cargo-<subcommand>`, and that subcommand reports another version (DEV-2).
/// A pin decides nothing by itself: cargo looks for a subcommand in
/// `$CARGO_HOME/bin` before `PATH`, `mise exec` or not.
pub fn check_versions(steps: &[Step], pins: &[(String, String)]) -> Result<(), String> {
    for step in steps {
        let Some(subcommand) = step.tool.as_ref().or(step.arguments.first()) else {
            continue;
        };
        let tool = format!("cargo-{subcommand}");
        let Some((_, pinned)) = pins.iter().find(|(name, _)| {
            // A name of mise's registry, or one with a backend: `cargo:`,
            // or `github:` and `aqua:` with the repository's owner.
            *name == tool
                || name.ends_with(&format!("/{tool}"))
                || name.ends_with(&format!(":{tool}"))
        }) else {
            continue;
        };
        let query = format!("{} {subcommand} --version", step.program);
        let output = Command::new(&step.program)
            .args([subcommand, "--version"])
            .output()
            .map_err(|error| format!("`{query}` could not be started: {error}"))?;
        let reported = String::from_utf8_lossy(&output.stdout);
        let first = reported.lines().next().unwrap_or_default().trim();
        if !output.status.success() || !first.split_whitespace().any(|word| word == pinned) {
            return Err(format!(
                "`{query}` reports `{first}`, and mise.toml pins {tool} {pinned}; \
                 run `mise install`, and remove any other {tool} that cargo finds first, \
                 in `$CARGO_HOME/bin` (DEV-2)"
            ));
        }
    }
    Ok(())
}

/// One check: a program and its arguments, run from the repository's root.
#[derive(Debug)]
pub struct Step {
    program: String,
    arguments: Vec<String>,
    /// Variables set for the program, beside those that it inherits.
    variables: Vec<(String, String)>,
    /// Whether macOS runs the step outside the VM too, for its own build.
    outside_the_vm: bool,
    /// What the failure of the step adds to its message.
    on_failure: Option<&'static str>,
    /// The cargo subcommand whose version the step is checked for, where it
    /// is not the first argument: a task of `xtask` that runs a check tool.
    tool: Option<String>,
    /// Why the step is not run in a worker's sandbox, where it is not
    /// (DEV-7, DEV-11).
    not_in_a_worker: Option<&'static str>,
}

impl Step {
    pub fn new(program: &str, arguments: &[&str]) -> Self {
        Self {
            program: program.to_owned(),
            arguments: arguments
                .iter()
                .map(|&argument| argument.to_owned())
                .collect(),
            variables: Vec::new(),
            outside_the_vm: false,
            on_failure: None,
            tool: None,
            not_in_a_worker: None,
        }
    }

    /// Sets the variable `name` to `value` for the program.
    fn with_variable(mut self, name: &str, value: &str) -> Self {
        self.variables.push((name.to_owned(), value.to_owned()));
        self
    }

    /// Gives the failure of the step a sentence of its own, after the exit
    /// code.
    fn on_failure(mut self, advice: &'static str) -> Self {
        self.on_failure = Some(advice);
        self
    }

    /// Names `subcommand` as the cargo subcommand that the step runs, for the
    /// version check (DEV-2).
    fn running(mut self, subcommand: &str) -> Self {
        self.tool = Some(subcommand.to_owned());
        self
    }

    /// Marks the step as one that is not run in a worker's sandbox, for
    /// `reason` (DEV-11).
    fn not_run_in_a_worker(mut self, reason: &'static str) -> Self {
        self.not_in_a_worker = Some(reason);
        self
    }

    /// Marks the step as one that macOS runs outside the VM too (DEV-3).
    fn also_outside_the_vm(mut self) -> Self {
        self.outside_the_vm = true;
        self
    }
}

impl fmt::Display for Step {
    /// The step for its messages: its variables first, each value in single
    /// quotes, then the program and its arguments.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, value) in &self.variables {
            write!(formatter, "{name}='{value}' ")?;
        }
        write!(formatter, "{}", self.program)?;
        for argument in &self.arguments {
            write!(formatter, " {argument}")?;
        }
        Ok(())
    }
}

/// Runs the steps in order and stops at the first that fails. The error names
/// that step and says how it failed. Where `worker` is set, a step that does
/// not run in a worker's sandbox is not run: the command names it and the
/// reason, and goes on. It returns the steps that were not run, which a
/// failure names too.
pub fn run(steps: &[Step], worker: bool) -> Result<Vec<String>, String> {
    let mut not_run = Vec::new();
    for step in steps {
        if let (true, Some(reason)) = (worker, step.not_in_a_worker) {
            eprintln!("xtask check: not run, in a worker's sandbox: `{step}`: {reason}");
            not_run.push(format!("`{step}`"));
            continue;
        }
        eprintln!("xtask check: {step}");
        let status = Command::new(&step.program)
            .args(&step.arguments)
            .envs(step.variables.iter().map(|(name, value)| (name, value)))
            .status()
            .map_err(|error| format!("`{step}` could not be started: {error}"))?;
        if !status.success() {
            let failure = format!("`{step}` {}", vm::ended(status));
            let failure = match step.on_failure {
                Some(advice) => format!("{failure}; {advice}"),
                None => failure,
            };
            return Err(if not_run.is_empty() {
                failure
            } else {
                format!("{failure}; not run: {}", not_run.join(", "))
            });
        }
    }
    Ok(not_run)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    /// A fresh directory for one test, removed when the value is dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("xtask-check-{}-{test}", std::process::id()));
            std::fs::create_dir_all(&path).expect("create the scratch directory");
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A step that passes and leaves `file` behind as the trace of its run.
    fn leaves(file: &Path) -> Step {
        Step::new("touch", &[&file.to_string_lossy()])
    }

    #[test]
    fn a_check_whose_steps_all_pass_passes_and_has_run_each() {
        let scratch = Scratch::new("pass");
        let (first, second) = (scratch.0.join("first"), scratch.0.join("second"));

        let outcome = run(&[leaves(&first), leaves(&second)], false);

        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(first.exists() && second.exists());
    }

    #[test]
    fn a_failing_step_fails_the_check_by_name_and_stops_it() {
        let scratch = Scratch::new("fail");
        let (before, after) = (scratch.0.join("before"), scratch.0.join("after"));

        let outcome = run(
            &[
                leaves(&before),
                Step::new("sh", &["-c", "exit 3"]),
                leaves(&after),
            ],
            false,
        );

        let failure = outcome.expect_err("the check fails").to_string();
        assert!(failure.contains("sh -c exit 3"), "{failure}");
        assert!(failure.contains("exit code 3"), "{failure}");
        assert!(before.exists());
        assert!(!after.exists(), "a step ran after the failure");
    }

    #[test]
    fn outside_the_vm_the_mac_runs_clippy_and_the_tests_alone() {
        let steps: Vec<String> = macos_steps().iter().map(ToString::to_string).collect();

        assert_eq!(
            steps,
            [
                "cargo clippy --workspace --all-targets --locked -- -D warnings",
                "cargo nextest run --workspace --locked",
                "cargo test --doc --workspace --locked",
            ]
        );
    }

    #[test]
    fn the_set_checks_the_supply_chain_without_the_network() {
        let steps: Vec<String> = steps().iter().map(ToString::to_string).collect();

        assert!(steps.contains(&"cargo deny --locked --offline check".to_owned()));
        assert!(steps.contains(&"cargo shear --locked --offline".to_owned()));
    }

    #[test]
    fn the_set_runs_the_tests_with_nextest_the_doc_tests_and_rustdoc() {
        let steps: Vec<String> = steps().iter().map(ToString::to_string).collect();

        // Word for word the nextest and doc-test verify commands of 11.2
        // (BOOT-6).
        assert!(steps.contains(&"cargo nextest run --workspace --locked".to_owned()));
        assert!(steps.contains(&"cargo test --doc --workspace --locked".to_owned()));
        assert!(steps.contains(
            &"RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked".to_owned()
        ));
        assert!(
            !steps.contains(&"cargo test --workspace --locked".to_owned()),
            "`cargo test` no longer stands in for the test runs (DEV-1)"
        );
    }

    #[test]
    fn the_set_checks_the_spelling_and_the_local_markdown_links_in_the_vm_alone() {
        let steps: Vec<String> = steps().iter().map(ToString::to_string).collect();
        let on_the_mac: Vec<String> = macos_steps().iter().map(ToString::to_string).collect();

        // typos reads typos.toml, its one file, without being told (DEV-2b).
        assert!(steps.contains(&"typos".to_owned()), "{steps:?}");
        assert!(steps.contains(&"cargo xtask links".to_owned()), "{steps:?}");
        assert!(
            !on_the_mac
                .iter()
                .any(|step| step == "typos" || step == "cargo xtask links")
        );
    }

    #[test]
    fn the_set_checks_the_cross_references_in_the_vm_alone() {
        let steps: Vec<String> = steps().iter().map(ToString::to_string).collect();
        let on_the_mac: Vec<String> = macos_steps().iter().map(ToString::to_string).collect();

        assert!(
            steps.contains(&"cargo xtask references".to_owned()),
            "{steps:?}"
        );
        assert!(!on_the_mac.contains(&"cargo xtask references".to_owned()));
    }

    #[test]
    fn a_failing_deny_step_names_the_task_that_fetches() {
        let deny = steps()
            .into_iter()
            .find(|step| step.arguments.first().is_some_and(|word| word == "deny"))
            .expect("the set has the deny step");
        let failing = Step {
            program: "sh".to_owned(),
            arguments: vec!["-c".to_owned(), "exit 1".to_owned()],
            ..deny
        };

        let failure = run(&[failing], false).expect_err("the check fails");

        assert!(failure.contains("exit code 1"), "{failure}");
        assert!(failure.contains("`cargo xtask fetch`"), "{failure}");
    }

    #[test]
    fn a_step_runs_its_program_with_its_variables() {
        let scratch = Scratch::new("variables");
        let trace = scratch.0.join("trace");
        let step = Step::new(
            "sh",
            &[
                "-c",
                r#"printf %s "$XTASK_CHECK_VALUE" > "$0""#,
                &trace.to_string_lossy(),
            ],
        )
        .with_variable("XTASK_CHECK_VALUE", "-D warnings");

        let outcome = run(&[step], false);

        assert!(outcome.is_ok(), "{outcome:?}");
        let value = std::fs::read_to_string(&trace).expect("the step left its trace");
        assert_eq!(value, "-D warnings");
    }

    #[test]
    fn the_pins_are_the_tools_of_mise_toml_each_at_its_version() {
        let text = "[tools]\nrust = \"1.100.0\"\n# A comment.\ncargo-nextest = \"0.9.152\"\n\
                    \"github:Boshen/cargo-shear\" = \"1.14.1\"\n\n[env]\nFOO = \"bar\"\n";

        assert_eq!(
            pins(text),
            [
                ("rust".to_owned(), "1.100.0".to_owned()),
                ("cargo-nextest".to_owned(), "0.9.152".to_owned()),
                ("github:Boshen/cargo-shear".to_owned(), "1.14.1".to_owned()),
            ]
        );
    }

    /// A stand-in `cargo` in `scratch` that prints `version` for any
    /// subcommand's `--version`, and records each run in `runs`.
    fn cargo_that_reports(scratch: &Scratch, version: &str) -> String {
        use std::os::unix::fs::PermissionsExt;

        let cargo = scratch.0.join("cargo");
        std::fs::write(
            &cargo,
            format!(
                "#!/bin/sh\necho \"$*\" >> '{runs}'\necho \"cargo-$1 {version} (abc 2026-11-13)\"\necho 'release: x'\n",
                runs = scratch.0.join("runs").display(),
            ),
        )
        .expect("write the stand-in cargo");
        std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755))
            .expect("make the stand-in cargo executable");
        cargo.to_string_lossy().into_owned()
    }

    #[test]
    fn a_check_tool_at_another_version_than_its_pin_fails_the_check_by_name() {
        let scratch = Scratch::new("version-other");
        let cargo = cargo_that_reports(&scratch, "0.9.150");
        let pins = [("cargo-nextest".to_owned(), "0.9.152".to_owned())];

        let outcome = check_versions(&[Step::new(&cargo, &["nextest", "run"])], &pins);

        let failure = outcome.expect_err("the check fails");
        assert!(failure.contains("nextest"), "{failure}");
        assert!(failure.contains("0.9.150"), "{failure}");
        assert!(failure.contains("0.9.152"), "{failure}");
    }

    #[test]
    fn check_tools_at_their_pins_pass_and_unpinned_subcommands_are_not_asked() {
        let scratch = Scratch::new("version-pinned");
        let cargo = cargo_that_reports(&scratch, "1.14.1");
        let pins = [
            ("rust".to_owned(), "1.100.0".to_owned()),
            ("github:Boshen/cargo-shear".to_owned(), "1.14.1".to_owned()),
        ];
        let steps = [
            Step::new(&cargo, &["fmt", "--all", "--check"]),
            Step::new(&cargo, &["shear", "--locked", "--offline"]),
        ];

        let outcome = check_versions(&steps, &pins);

        assert!(outcome.is_ok(), "{outcome:?}");
        let runs = std::fs::read_to_string(scratch.0.join("runs")).expect("cargo ran");
        assert_eq!(runs, "shear --version\n");
    }

    #[test]
    fn the_set_mutates_the_diff_and_reports_coverage_in_the_vm_alone() {
        let steps: Vec<String> = steps().iter().map(ToString::to_string).collect();
        let on_the_mac: Vec<String> = macos_steps().iter().map(ToString::to_string).collect();
        let (mutation, coverage) = (
            "cargo xtask mutants".to_owned(),
            "cargo llvm-cov nextest --workspace --locked".to_owned(),
        );

        assert!(steps.contains(&mutation), "{steps:?}");
        assert!(steps.contains(&coverage), "{steps:?}");
        // `cargo mutants` on the Mac misses every mutant of Linux-only code
        // (DEV-3, P-38).
        assert!(!on_the_mac.contains(&mutation) && !on_the_mac.contains(&coverage));
    }

    #[test]
    fn the_mutation_task_is_checked_for_the_version_of_cargo_mutants() {
        let scratch = Scratch::new("version-mutants");
        let cargo = cargo_that_reports(&scratch, "27.1.0");
        let pins = [
            ("cargo:cargo-mutants".to_owned(), "27.1.0".to_owned()),
            ("aqua:taiki-e/cargo-llvm-cov".to_owned(), "0.9.2".to_owned()),
        ];
        let steps = [
            Step::new(&cargo, &["xtask", "mutants"]).running("mutants"),
            Step::new(&cargo, &["llvm-cov", "nextest"]),
        ];

        let outcome = check_versions(&steps, &pins);

        let failure = outcome.expect_err("llvm-cov is not at its pin");
        assert!(failure.contains("cargo-llvm-cov 0.9.2"), "{failure}");
        let runs = std::fs::read_to_string(scratch.0.join("runs")).expect("cargo ran");
        assert_eq!(runs, "mutants --version\nllvm-cov --version\n");
    }

    #[test]
    fn on_the_mac_a_work_tree_without_a_base_fails_before_the_vm_runs() {
        let scratch = Scratch::new("no-base");
        let status = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&scratch.0)
            .env_remove("GIT_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_WORK_TREE")
            .status()
            .expect("run git");
        assert!(status.success());
        let vm = Vm::new("xtask-no-such-limactl", "sf-test");

        let failure = in_the_vm(&scratch.0, &vm, None).expect_err("there is no base");

        assert!(failure.contains("mutation step"), "{failure}");
        assert!(!failure.contains("sf-test"), "the VM was asked: {failure}");
    }

    #[test]
    fn in_a_workers_sandbox_a_step_that_does_not_run_there_is_named_and_the_rest_run() {
        let scratch = Scratch::new("worker");
        let (before, skipped, after) = (
            scratch.0.join("before"),
            scratch.0.join("skipped"),
            scratch.0.join("after"),
        );
        let steps = [
            leaves(&before),
            leaves(&skipped).not_run_in_a_worker("it makes a sandbox"),
            leaves(&after),
        ];

        let not_run = run(&steps, true).expect("the check passes");

        assert_eq!(not_run, [format!("`touch {}`", skipped.display())]);
        assert!(before.exists() && after.exists());
        assert!(!skipped.exists(), "the step ran in a worker");
        assert_eq!(
            passed(&not_run),
            format!("passed; not run: `touch {}`", skipped.display())
        );
    }

    #[test]
    fn outside_a_workers_sandbox_every_step_runs() {
        let scratch = Scratch::new("not-worker");
        let marked = scratch.0.join("marked");

        let not_run = run(&[leaves(&marked).not_run_in_a_worker("a reason")], false)
            .expect("the check passes");

        assert!(not_run.is_empty(), "{not_run:?}");
        assert!(marked.exists());
        assert_eq!(passed(&not_run), "passed");
    }

    #[test]
    fn a_failure_after_a_step_that_was_not_run_names_it_too() {
        let scratch = Scratch::new("worker-fails");
        let skipped = scratch.0.join("skipped");
        let steps = [
            leaves(&skipped).not_run_in_a_worker("a reason"),
            Step::new("sh", &["-c", "exit 4"]),
        ];

        let failure = run(&steps, true).expect_err("the check fails");

        assert!(failure.contains("exit code 4"), "{failure}");
        assert!(
            failure.ends_with(&format!("; not run: `touch {}`", skipped.display())),
            "{failure}"
        );
    }

    #[test]
    fn a_worker_is_known_by_sf_sandbox_alone() {
        use std::ffi::OsStr;

        assert!(is_a_worker(Some(OsStr::new("process"))));
        assert!(is_a_worker(Some(OsStr::new("hardened"))));
        for other in ["none", "", "Process", "yes"] {
            assert!(!is_a_worker(Some(OsStr::new(other))), "{other}");
        }
        assert!(!is_a_worker(None));
    }

    #[test]
    fn the_sandbox_tests_are_a_step_of_their_own_that_no_worker_runs() {
        let set = steps();
        let sandbox: Vec<&Step> = set
            .iter()
            .filter(|step| step.not_in_a_worker.is_some())
            .collect();
        let on_the_mac: Vec<String> = macos_steps().iter().map(ToString::to_string).collect();

        let [sandbox] = sandbox[..] else {
            panic!("one step is not run in a worker: {sandbox:?}");
        };
        let sandbox = sandbox.to_string();
        assert_eq!(
            sandbox,
            "SF_SANDBOX_TESTS='process' cargo nextest run --locked --package factory-sandbox \
             --package factory-daemon --test sandbox"
        );
        assert!(!on_the_mac.contains(&sandbox), "{on_the_mac:?}");
        // The plain test run sets no variable, so it runs none of the
        // sandbox tests (DEV-2a).
        let plain = set
            .iter()
            .map(ToString::to_string)
            .find(|step| step.contains("cargo nextest run --workspace"))
            .expect("the plain test run");
        assert_eq!(plain, "cargo nextest run --workspace --locked");
    }

    #[test]
    fn a_step_that_cannot_be_started_fails_the_check_by_name() {
        let outcome = run(&[Step::new("xtask-no-such-program", &["--flag"])], false);

        let failure = outcome.expect_err("the check fails").to_string();
        assert!(
            failure.contains("xtask-no-such-program --flag"),
            "{failure}"
        );
    }
}

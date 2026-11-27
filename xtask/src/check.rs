//! `cargo xtask check`: every check of the factory repository behind one
//! command (SPEC DEV-1).

use std::fmt;
use std::path::Path;
use std::process::{Command, ExitCode};

use crate::vm::{self, Vm};

/// The checks of SPEC DEV-2 that exist so far. The rest of that set is added
/// here as it is built, and nowhere else (DEV-1).
fn steps() -> Vec<Step> {
    vec![
        Step::new("cargo", &["fmt", "--all", "--check"]),
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
        Step::new("cargo", &["test", "--workspace", "--locked"]).also_outside_the_vm(),
    ]
}

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
        in_the_vm(&root).and_then(|()| run(&macos_steps()))
    } else {
        run(&steps())
    };
    match outcome {
        Ok(()) => {
            eprintln!("xtask check: passed");
            ExitCode::SUCCESS
        }
        Err(failure) => {
            eprintln!("xtask check: {failure}");
            ExitCode::FAILURE
        }
    }
}

/// On macOS, the whole set in the development VM (DEV-3): the VM is created
/// and started where it has to be, and shown the work tree at `root`.
fn in_the_vm(root: &Path) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve the repository's root: {error}"))?;
    let vm = Vm::new("limactl", vm::NAME);
    vm.ensure(&root.join("lima.yaml"))?;
    vm.check(&root)
}

/// One check: a program and its arguments, run from the repository's root.
#[derive(Debug)]
pub struct Step {
    program: String,
    arguments: Vec<String>,
    /// Whether macOS runs the step outside the VM too, for its own build.
    outside_the_vm: bool,
    /// What the failure of the step adds to its message.
    on_failure: Option<&'static str>,
}

impl Step {
    pub fn new(program: &str, arguments: &[&str]) -> Self {
        Self {
            program: program.to_owned(),
            arguments: arguments
                .iter()
                .map(|&argument| argument.to_owned())
                .collect(),
            outside_the_vm: false,
            on_failure: None,
        }
    }

    /// Gives the failure of the step a sentence of its own, after the exit
    /// code.
    fn on_failure(mut self, advice: &'static str) -> Self {
        self.on_failure = Some(advice);
        self
    }

    /// Marks the step as one that macOS runs outside the VM too (DEV-3).
    fn also_outside_the_vm(mut self) -> Self {
        self.outside_the_vm = true;
        self
    }
}

impl fmt::Display for Step {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.program)?;
        for argument in &self.arguments {
            write!(formatter, " {argument}")?;
        }
        Ok(())
    }
}

/// Runs the steps in order and stops at the first that fails. The error names
/// that step and says how it failed.
pub fn run(steps: &[Step]) -> Result<(), String> {
    for step in steps {
        eprintln!("xtask check: {step}");
        let status = Command::new(&step.program)
            .args(&step.arguments)
            .status()
            .map_err(|error| format!("`{step}` could not be started: {error}"))?;
        if !status.success() {
            let failure = format!("`{step}` {}", vm::ended(status));
            return Err(match step.on_failure {
                Some(advice) => format!("{failure}; {advice}"),
                None => failure,
            });
        }
    }
    Ok(())
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

        let outcome = run(&[leaves(&first), leaves(&second)]);

        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(first.exists() && second.exists());
    }

    #[test]
    fn a_failing_step_fails_the_check_by_name_and_stops_it() {
        let scratch = Scratch::new("fail");
        let (before, after) = (scratch.0.join("before"), scratch.0.join("after"));

        let outcome = run(&[
            leaves(&before),
            Step::new("sh", &["-c", "exit 3"]),
            leaves(&after),
        ]);

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
                "cargo test --workspace --locked",
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

        let failure = run(&[failing]).expect_err("the check fails");

        assert!(failure.contains("exit code 1"), "{failure}");
        assert!(failure.contains("`cargo xtask fetch`"), "{failure}");
    }

    #[test]
    fn a_step_that_cannot_be_started_fails_the_check_by_name() {
        let outcome = run(&[Step::new("xtask-no-such-program", &["--flag"])]);

        let failure = outcome.expect_err("the check fails").to_string();
        assert!(
            failure.contains("xtask-no-such-program --flag"),
            "{failure}"
        );
    }
}

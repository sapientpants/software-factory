//! `cargo xtask check`: every check of the factory repository behind one
//! command (SPEC DEV-1).

use std::fmt;
use std::path::Path;
use std::process::{Command, ExitCode};

/// The checks of SPEC DEV-2 that exist so far. The rest of that set is added
/// here as it is built, and nowhere else (DEV-1).
fn steps() -> Vec<Step> {
    vec![
        Step::new("cargo", &["fmt", "--all", "--check"]),
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
        ),
        Step::new("cargo", &["test", "--workspace", "--locked"]),
    ]
}

pub fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    if let Err(error) = std::env::set_current_dir(&root) {
        eprintln!("xtask check: cannot enter the repository's root: {error}");
        return ExitCode::FAILURE;
    }
    match run(&steps()) {
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

/// One check: a program and its arguments, run from the repository's root.
#[derive(Debug)]
pub struct Step {
    program: String,
    arguments: Vec<String>,
}

impl Step {
    pub fn new(program: &str, arguments: &[&str]) -> Self {
        Self {
            program: program.to_owned(),
            arguments: arguments
                .iter()
                .map(|&argument| argument.to_owned())
                .collect(),
        }
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
            return Err(match status.code() {
                Some(code) => format!("`{step}` failed with exit code {code}"),
                None => format!("`{step}` was ended by a signal"),
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
    fn a_step_that_cannot_be_started_fails_the_check_by_name() {
        let outcome = run(&[Step::new("xtask-no-such-program", &["--flag"])]);

        let failure = outcome.expect_err("the check fails").to_string();
        assert!(
            failure.contains("xtask-no-such-program --flag"),
            "{failure}"
        );
    }
}

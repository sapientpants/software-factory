//! `cargo xtask fetch`: what `cargo xtask check` reads from the network, so
//! that the check itself fetches nothing (SPEC DEV-14).

use std::fs::FileTimes;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::vm::{self, Vm};

pub fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    if let Err(error) = std::env::set_current_dir(&root) {
        eprintln!("xtask fetch: cannot enter the repository's root: {error}");
        return ExitCode::FAILURE;
    }
    let outcome = if cfg!(target_os = "macos") {
        // The dependencies outside the VM, for the clippy and the tests that
        // run there, and both inside it, where the set runs (DEV-14, DEV-3).
        let outside = Fetch::new("cargo", PathBuf::new()).dependencies();
        let inside = in_the_vm(&root);
        outside.and(inside)
    } else {
        match advisory_dbs() {
            Some(databases) => Fetch::new("cargo", databases).all(),
            None => Err("neither CARGO_HOME nor HOME is set".to_owned()),
        }
    };
    match outcome {
        Ok(()) => {
            eprintln!("xtask fetch: done");
            ExitCode::SUCCESS
        }
        Err(failure) => {
            eprintln!("xtask fetch: {failure}");
            ExitCode::FAILURE
        }
    }
}

/// On macOS, the task in the development VM, which is created and started
/// where it has to be.
fn in_the_vm(root: &Path) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve the repository's root: {error}"))?;
    let vm = Vm::new("limactl", vm::NAME);
    vm.ensure(&root.join("lima.yaml"))?;
    vm.fetch(&root)
}

/// The directory under which `cargo deny` keeps its clones of the advisory
/// database when `deny.toml` names no `db-path` (DEV-2a, P-38).
fn advisory_dbs() -> Option<PathBuf> {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".cargo")))?;
    Some(cargo_home.join("advisory-dbs"))
}

/// The two fetches of DEV-14, run with `cargo`, against the clones of the
/// advisory database under `databases`.
struct Fetch {
    cargo: String,
    databases: PathBuf,
}

impl Fetch {
    fn new(cargo: &str, databases: PathBuf) -> Self {
        Self {
            cargo: cargo.to_owned(),
            databases,
        }
    }

    /// Both fetches. The second runs where the first fails, and the error
    /// names each that failed.
    fn all(&self) -> Result<(), String> {
        let failures: Vec<String> = [self.dependencies(), self.database()]
            .into_iter()
            .filter_map(Result::err)
            .collect();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    fn dependencies(&self) -> Result<(), String> {
        self.cargo(&["fetch", "--locked"])
    }

    /// `cargo deny fetch db`. A fetch that fails leaves `FETCH_HEAD` with the
    /// time of the attempt, which `cargo deny` takes for the time of the last
    /// fetch (P-38), so where it fails the time of each clone's file is put
    /// back, and a file that was not there is removed.
    fn database(&self) -> Result<(), String> {
        let before = fetch_times(&self.databases);
        let failure = match self.cargo(&["deny", "fetch", "db"]) {
            Ok(()) => return Ok(()),
            Err(failure) => failure,
        };
        Err(match restore(&self.databases, &before) {
            Ok(()) => failure,
            Err(error) => {
                format!("{failure}; and the time of the last fetch could not be put back: {error}")
            }
        })
    }

    fn cargo(&self, arguments: &[&str]) -> Result<(), String> {
        let call = format!("{} {}", self.cargo, arguments.join(" "));
        eprintln!("xtask fetch: {call}");
        let status = Command::new(&self.cargo)
            .args(arguments)
            .status()
            .map_err(|error| format!("`{call}` could not be started: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("`{call}` {}", vm::ended(status)))
        }
    }
}

/// The file of a clone from whose time `cargo deny` reads the last fetch.
fn fetch_head(clone: &Path) -> PathBuf {
    clone.join(".git").join("FETCH_HEAD")
}

/// The times of the `FETCH_HEAD` of each clone under `databases`, and `None`
/// for a clone that has none.
fn fetch_times(databases: &Path) -> Vec<(PathBuf, Option<FileTimes>)> {
    clones(databases)
        .into_iter()
        .map(|clone| {
            // The modified time alone: it is what `cargo deny` reads.
            let times = std::fs::metadata(fetch_head(&clone))
                .and_then(|metadata| metadata.modified())
                .ok()
                .map(|modified| FileTimes::new().set_modified(modified));
            (clone, times)
        })
        .collect()
}

/// Puts back the times of `before` on the `FETCH_HEAD` of each clone under
/// `databases`, and removes the file from a clone that had none, a clone that
/// the failed fetch made among them.
fn restore(databases: &Path, before: &[(PathBuf, Option<FileTimes>)]) -> Result<(), String> {
    for clone in clones(databases) {
        let file = fetch_head(&clone);
        let times = before
            .iter()
            .find(|(earlier, _)| *earlier == clone)
            .and_then(|(_, times)| *times);
        let outcome = match times {
            Some(times) => std::fs::File::options()
                .write(true)
                .open(&file)
                .and_then(|handle| handle.set_times(times)),
            None => match std::fs::remove_file(&file) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            },
        };
        outcome.map_err(|error| format!("{}: {error}", file.display()))?;
    }
    Ok(())
}

/// The directories under `databases`, each a clone of a database.
fn clones(databases: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(databases) else {
        return Vec::new();
    };
    let mut clones: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect();
    clones.sort();
    clones
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;

    /// A fresh directory for one test, removed when the value is dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("xtask-fetch-{}-{test}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create the scratch directory");
            Self(path)
        }

        fn databases(&self) -> PathBuf {
            self.0.join("advisory-dbs")
        }

        /// A stand-in `cargo` that appends its arguments to `calls`. For
        /// `deny fetch db` it writes an empty `FETCH_HEAD` into every clone
        /// and into a new one, as a failed fetch leaves it, and exits with
        /// `deny`; for `fetch` it exits with `fetch`.
        fn cargo(&self, fetch: i32, deny: i32) -> String {
            let script = self.0.join("cargo");
            std::fs::write(
                &script,
                format!(
                    "#!/bin/sh\n\
                     echo \"$*\" >> '{calls}'\n\
                     [ \"$1\" = fetch ] && exit {fetch}\n\
                     mkdir -p '{databases}/new/.git'\n\
                     for clone in '{databases}'/*; do : > \"$clone/.git/FETCH_HEAD\"; done\n\
                     exit {deny}\n",
                    calls = self.0.join("calls").display(),
                    databases = self.databases().display(),
                ),
            )
            .expect("write the stand-in cargo");
            let status = Command::new("chmod")
                .arg("+x")
                .arg(&script)
                .status()
                .expect("run chmod");
            assert!(status.success());
            script.to_string_lossy().into_owned()
        }

        fn calls(&self) -> Vec<String> {
            std::fs::read_to_string(self.0.join("calls"))
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect()
        }

        /// A clone named `name`, with a `FETCH_HEAD` that `age` old where
        /// `age` is given.
        fn clone(&self, name: &str, age: Option<Duration>) -> PathBuf {
            let clone = self.databases().join(name);
            std::fs::create_dir_all(clone.join(".git")).expect("create a clone");
            if let Some(age) = age {
                let file = std::fs::File::create(fetch_head(&clone)).expect("write FETCH_HEAD");
                let then = SystemTime::now() - age;
                file.set_times(FileTimes::new().set_accessed(then).set_modified(then))
                    .expect("age FETCH_HEAD");
            }
            clone
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn modified(clone: &Path) -> SystemTime {
        std::fs::metadata(fetch_head(clone))
            .and_then(|metadata| metadata.modified())
            .expect("FETCH_HEAD has a time")
    }

    const EIGHT_DAYS: Duration = Duration::from_secs(8 * 24 * 60 * 60);

    #[test]
    fn the_task_fetches_the_dependencies_and_the_database() {
        let scratch = Scratch::new("both");
        let fetch = Fetch::new(&scratch.cargo(0, 0), scratch.databases());

        let outcome = fetch.all();

        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(scratch.calls(), ["fetch --locked", "deny fetch db"]);
    }

    #[test]
    fn a_fetch_that_works_leaves_the_time_that_it_gave() {
        let scratch = Scratch::new("works");
        let clone = scratch.clone("advisory-db", Some(EIGHT_DAYS));
        let fetch = Fetch::new(&scratch.cargo(0, 0), scratch.databases());

        fetch.all().expect("the fetch works");

        assert!(modified(&clone) > SystemTime::now() - Duration::from_secs(60));
    }

    #[test]
    fn a_fetch_of_the_database_that_fails_puts_its_time_back_and_fails() {
        let scratch = Scratch::new("fails");
        let clone = scratch.clone("advisory-db", Some(EIGHT_DAYS));
        let before = modified(&clone);
        let fetch = Fetch::new(&scratch.cargo(0, 1), scratch.databases());

        let failure = fetch.all().expect_err("the fetch fails");

        assert!(failure.contains("deny fetch db"), "{failure}");
        assert!(failure.contains("exit code 1"), "{failure}");
        assert_eq!(modified(&clone), before);
    }

    #[test]
    fn a_fetch_head_that_a_failed_fetch_made_is_removed() {
        let scratch = Scratch::new("absent");
        let clone = scratch.clone("advisory-db", None);
        let fetch = Fetch::new(&scratch.cargo(0, 1), scratch.databases());

        fetch.all().expect_err("the fetch fails");

        assert!(!fetch_head(&clone).exists());
        assert!(!fetch_head(&scratch.databases().join("new")).exists());
    }

    #[test]
    fn a_failed_fetch_of_the_dependencies_still_fetches_the_database() {
        let scratch = Scratch::new("dependencies");
        let fetch = Fetch::new(&scratch.cargo(4, 0), scratch.databases());

        let failure = fetch.all().expect_err("the fetch fails");

        assert!(failure.contains("fetch --locked"), "{failure}");
        assert!(failure.contains("exit code 4"), "{failure}");
        assert_eq!(scratch.calls(), ["fetch --locked", "deny fetch db"]);
    }

    #[test]
    fn a_cargo_that_cannot_be_started_fails_by_name() {
        let scratch = Scratch::new("missing");
        let fetch = Fetch::new("xtask-no-such-cargo", scratch.databases());

        let failure = fetch.all().expect_err("no cargo");

        assert!(
            failure.contains("xtask-no-such-cargo fetch --locked"),
            "{failure}"
        );
        assert!(
            failure.contains("xtask-no-such-cargo deny fetch db"),
            "{failure}"
        );
    }
}

//! The development VM of SPEC DEV-3: a Lima instance that `cargo xtask check`
//! on macOS creates from `lima.yaml`, starts, and runs the check set in.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

/// The name of the Lima instance.
pub const NAME: &str = "sf-dev";

/// The development VM, as `limactl` sees it.
pub struct Vm {
    limactl: String,
    name: String,
}

impl Vm {
    pub fn new(limactl: &str, name: &str) -> Self {
        Self {
            limactl: limactl.to_owned(),
            name: name.to_owned(),
        }
    }

    /// Creates the VM from `config` where it does not exist, and starts it
    /// where it does not run. The error names the VM and says what failed.
    pub fn ensure(&self, config: &Path) -> Result<(), String> {
        match self.instance()? {
            None => {
                self.limactl(&[
                    "create",
                    "--tty=false",
                    "--name",
                    &self.name,
                    &config.to_string_lossy(),
                ])
                .map_err(|error| self.failure("created", &error))?;
                let instance = self.instance()?.ok_or_else(|| {
                    self.failure(
                        "created",
                        "`limactl list` does not name it after `limactl create`",
                    )
                })?;
                self.record(config, &instance.directory)?;
                // The provisioning adds Lima's user to group `kvm` after the
                // first login has begun, and a login keeps the groups it began
                // with. Lima logs in again after a restart (D-53).
                for subcommand in ["start", "stop", "start"] {
                    self.limactl(&[subcommand, "--tty=false", &self.name])
                        .map_err(|error| self.failure("started", &error))?;
                }
                Ok(())
            }
            Some(instance) => {
                self.compare(config, &instance.directory);
                if instance.status == "Running" {
                    return Ok(());
                }
                // Every other status, `Broken` among them, is one that
                // `limactl start` is asked to end.
                self.limactl(&["start", "--tty=false", &self.name])
                    .map_err(|error| self.failure("started", &error))
            }
        }
    }

    /// Runs `cargo xtask check` in the VM on a copy of the work tree at
    /// `root`, an absolute path, and fails by name where the copy or the run
    /// fails. The copy lies at `~/sf/tree` and the build output at
    /// `~/sf/target` in the VM, each below the path of the work tree on the
    /// Mac. The copy is made anew on every run, with the files' times, so that
    /// cargo builds again only what changed. The build output stays.
    pub fn check(&self, root: &Path) -> Result<(), String> {
        let guest = root.strip_prefix("/").map_err(|_| {
            self.failure(
                "shown the work tree",
                &format!("{} is no absolute path", root.display()),
            )
        })?;
        let tree = Path::new("sf/tree").join(guest);
        let target = Path::new("sf/target").join(guest);
        self.copy(root, &tree)
            .map_err(|error| self.failure("shown the work tree", &error))?;
        eprintln!(
            "xtask check: the set, in the development VM `{}`",
            self.name
        );
        self.limactl(&self.shell(
            "cd \"$HOME/$1\" && CARGO_TARGET_DIR=\"$HOME/$2\" && export CARGO_TARGET_DIR \
             && \"$HOME/.local/bin/mise\" install --yes \
             && exec \"$HOME/.local/bin/mise\" exec -- cargo xtask check",
            &[&tree.to_string_lossy(), &target.to_string_lossy()],
        ))
        .map_err(|error| {
            format!(
                "the check set in the development VM `{}` failed: {error}",
                self.name
            )
        })
    }

    /// The arguments of `limactl` that run `script` with `sh` in the VM, from
    /// its root, with `arguments` as `$1` and on.
    fn shell<'a>(&'a self, script: &'a str, arguments: &[&'a str]) -> Vec<&'a str> {
        let mut call = vec![
            "shell",
            "--workdir=/",
            &self.name,
            "--",
            "sh",
            "-c",
            script,
            "sh",
        ];
        call.extend_from_slice(arguments);
        call
    }

    /// Replaces the directory `tree`, relative to the home directory in the
    /// VM, with the files of the work tree at `root`.
    fn copy(&self, root: &Path, tree: &Path) -> Result<(), String> {
        let files = work_tree_files(root)?;
        let mut archive = Command::new("tar");
        archive.args(["--null", "-T", "-", "--no-xattrs", "-cf", "-"]);
        if cfg!(target_os = "macos") {
            // bsdtar would otherwise add the Mac's metadata as `._` files.
            archive
                .arg("--no-mac-metadata")
                .env("COPYFILE_DISABLE", "1");
        }
        let mut archive = archive
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|error| format!("`tar` could not be started: {error}"))?;
        let mut list = archive.stdin.take().ok_or("`tar` has no stdin")?;
        let listing = std::thread::spawn(move || {
            for file in files {
                list.write_all(file.as_bytes())?;
                list.write_all(b"\0")?;
            }
            Ok::<(), std::io::Error>(())
        });
        let stdout = archive.stdout.take().ok_or("`tar` has no stdout")?;
        let extract = Command::new(&self.limactl)
            .args(self.shell(
                "cd && rm -rf -- \"$1\" && mkdir -p -- \"$1\" && tar -xf - -C \"$1\"",
                &[&tree.to_string_lossy()],
            ))
            .stdin(stdout)
            .status()
            .map_err(|error| format!("`{} shell` could not be started: {error}", self.limactl))?;
        let listed = listing
            .join()
            .map_err(|_| "the list of files was not written")?;
        let archived = archive
            .wait()
            .map_err(|error| format!("`tar` was lost: {error}"))?;
        listed.map_err(|error| format!("the list of files was not written: {error}"))?;
        // Where the VM's side ends first, the Mac's `tar` loses its reader and
        // ends too, so the VM's failure is the one to name.
        if !extract.success() {
            return Err(format!("`tar` in the VM {}", ended(extract)));
        }
        if !archived.success() {
            return Err(format!("`tar` on the Mac {}", ended(archived)));
        }
        Ok(())
    }

    /// Keeps a copy of the configuration that the VM was created from in
    /// Lima's directory of the instance. Lima's own copy there has its base
    /// templates merged in and cannot be compared with `lima.yaml`.
    fn record(&self, config: &Path, directory: &Path) -> Result<(), String> {
        std::fs::copy(config, directory.join(RECORD))
            .map(|_| ())
            .map_err(|error| {
                self.failure(
                    "created",
                    &format!("cannot keep a copy of {}: {error}", config.display()),
                )
            })
    }

    /// Warns where the VM was created from another configuration than
    /// `config`, and goes on: every work tree shares the one VM, so a branch
    /// that changes `lima.yaml` would otherwise stop the check on every other.
    /// The VM is not deleted: it holds what DEV-4 puts in it.
    fn compare(&self, config: &Path, directory: &Path) {
        let current = std::fs::read(config).ok();
        if current.is_some() && std::fs::read(directory.join(RECORD)).ok() == current {
            return;
        }
        eprintln!(
            "xtask check: warning: the development VM `{name}` was not created from the {config} \
             of this work tree. Delete it with `limactl delete --force {name}` where the check \
             needs what that file gives it, and the next check creates it anew",
            name = self.name,
            config = config.display(),
        );
    }

    fn instance(&self) -> Result<Option<Instance>, String> {
        let output = Command::new(&self.limactl)
            .args(["list", "--format={{.Name}}\t{{.Status}}\t{{.Dir}}"])
            .stderr(Stdio::inherit())
            .output()
            .map_err(|error| {
                self.failure(
                    "looked up",
                    &format!("`{} list` could not be started: {error}", self.limactl),
                )
            })?;
        if !output.status.success() {
            return Err(self.failure(
                "looked up",
                &format!("`{} list` {}", self.limactl, ended(output.status)),
            ));
        }
        let listing = String::from_utf8_lossy(&output.stdout);
        Ok(listing.lines().find_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let (name, status, directory) = (fields.next()?, fields.next()?, fields.next()?);
            (name == self.name).then(|| Instance {
                status: status.to_owned(),
                directory: PathBuf::from(directory),
            })
        }))
    }

    fn limactl(&self, arguments: &[&str]) -> Result<(), String> {
        let call = format!("{} {}", self.limactl, arguments.join(" "));
        let status = Command::new(&self.limactl)
            .args(arguments)
            .status()
            .map_err(|error| format!("`{call}` could not be started: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("`{call}` {}", ended(status)))
        }
    }

    fn failure(&self, what: &str, error: &str) -> String {
        format!(
            "the development VM `{}` could not be {what}: {error}",
            self.name
        )
    }
}

/// The files of the work tree at `root` that the VM is shown, relative to it
/// and sorted: those that git tracks and those it neither tracks nor ignores,
/// as they stand, without a tracked file that is gone from the work tree.
/// `.git`, `target` and the files of another work tree below this one are
/// not among them.
pub fn work_tree_files(root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("`git ls-files` could not be started: {error}"))?;
    if !output.status.success() {
        return Err(format!("`git ls-files` {}", ended(output.status)));
    }
    let mut files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|file| !file.is_empty() && root.join(file).symlink_metadata().is_ok())
        .map(str::to_owned)
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

/// The file in Lima's directory of the instance that holds the configuration
/// the VM was created from.
const RECORD: &str = "sf-lima.yaml";

/// What `limactl list` says of the VM.
struct Instance {
    status: String,
    directory: PathBuf,
}

fn ended(status: ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("failed with exit code {code}"),
        None => "was ended by a signal".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    /// A fresh directory for one test, removed when the value is dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Self {
            let path = std::env::temp_dir().join(format!("xtask-vm-{}-{test}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create the scratch directory");
            Self(path)
        }

        /// A stand-in `limactl` that appends its arguments to `calls`, prints
        /// `listing` for `list` until `create` makes the VM, and exits with
        /// `failing`'s code for the subcommand that `failing` names.
        fn limactl(&self, listing: &str, failing: Option<(&str, i32)>) -> String {
            let (subcommand, code) = failing.unwrap_or(("none", 0));
            let script = self.0.join("limactl");
            std::fs::write(
                self.0.join("listing"),
                listing.replace("\\t", "\t").replace("\\n", "\n"),
            )
            .expect("write the listing");
            std::fs::write(
                &script,
                format!(
                    "#!/bin/sh\n\
                     cd '{scratch}'\n\
                     echo \"$*\" >> calls\n\
                     [ \"$1\" = {subcommand} ] && exit {code}\n\
                     [ \"$1\" = list ] && cat listing\n\
                     [ \"$1\" = create ] && printf 'sf-test\\tStopped\\t%s\\n' \"$PWD\" >> listing\n\
                     exit 0\n",
                    scratch = self.0.display(),
                ),
            )
            .expect("write the stand-in limactl");
            let status = Command::new("chmod")
                .arg("+x")
                .arg(&script)
                .status()
                .expect("run chmod");
            assert!(status.success());
            script.to_string_lossy().into_owned()
        }

        fn calls_file(&self) -> PathBuf {
            self.0.join("calls")
        }

        /// The subcommand and instance of each call, `list` without its format.
        fn calls(&self) -> Vec<String> {
            std::fs::read_to_string(self.calls_file())
                .unwrap_or_default()
                .lines()
                .map(|call| {
                    let words: Vec<&str> = call.split(' ').collect();
                    match words[0] {
                        "list" => "list".to_owned(),
                        subcommand => format!("{subcommand} {}", words.contains(&"sf-test")),
                    }
                })
                .collect()
        }

        /// What `limactl list` prints of the VM, with the scratch directory as
        /// its instance directory.
        fn listing(&self, status: &str) -> String {
            format!("sf-test\\t{status}\\t{}\\n", self.0.display())
        }

        fn config(&self) -> PathBuf {
            let config = self.0.join("lima.yaml");
            std::fs::write(&config, "vmType: vz\n").expect("write the configuration");
            config
        }
    }

    /// Runs git in `directory`, cut off from the repository and the
    /// configuration of whoever runs the test.
    fn git(directory: &Path, arguments: &[&str]) {
        let status = Command::new("git")
            .args(arguments)
            .current_dir(directory)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GIT_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_WORK_TREE")
            .status()
            .expect("run git");
        assert!(status.success(), "git {arguments:?}");
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_vm_that_does_not_exist_is_created_started_and_started_again() {
        let scratch = Scratch::new("absent");
        let vm = Vm::new(&scratch.limactl("other\\tRunning\\t/x\\n", None), "sf-test");

        let outcome = vm.ensure(&scratch.config());

        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(
            scratch.calls(),
            [
                "list",
                "create true",
                "list",
                "start true",
                "stop true",
                "start true"
            ]
        );
    }

    #[test]
    fn a_vm_once_created_is_not_created_again() {
        let scratch = Scratch::new("again");
        let config = scratch.config();
        let vm = Vm::new(&scratch.limactl("", None), "sf-test");

        vm.ensure(&config).expect("the VM is created");
        let outcome = vm.ensure(&config);

        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(
            scratch
                .calls()
                .iter()
                .filter(|call| call.starts_with("create"))
                .count(),
            1
        );
    }

    #[test]
    fn a_stopped_vm_is_started_and_a_running_one_is_left_as_it_is() {
        for (status, expected) in [
            ("Stopped", &["list", "start true"][..]),
            ("Running", &["list"][..]),
        ] {
            let scratch = Scratch::new(status);
            let config = scratch.config();
            let vm = Vm::new(&scratch.limactl(&scratch.listing(status), None), "sf-test");
            vm.record(&config, &scratch.0)
                .expect("record the configuration");

            let outcome = vm.ensure(&config);

            assert!(outcome.is_ok(), "{status}: {outcome:?}");
            assert_eq!(scratch.calls(), expected, "{status}");
        }
    }

    #[test]
    fn a_vm_made_from_another_configuration_is_used_as_it_is_and_kept() {
        let scratch = Scratch::new("changed");
        let config = scratch.config();
        let vm = Vm::new(
            &scratch.limactl(&scratch.listing("Running"), None),
            "sf-test",
        );
        vm.record(&config, &scratch.0)
            .expect("record the configuration");
        std::fs::write(&config, "vmType: qemu\n").expect("change the configuration");

        let outcome = vm.ensure(&config);

        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(scratch.calls(), ["list"], "the VM was changed");
    }

    #[test]
    fn a_vm_that_cannot_be_created_or_started_fails_by_name() {
        for (status, failing, what) in [
            (None, "create", "could not be created"),
            (None, "start", "could not be started"),
            (Some("Stopped"), "start", "could not be started"),
            (Some("Running"), "list", "could not be looked up"),
        ] {
            let scratch = Scratch::new(failing);
            let config = scratch.config();
            let listing = status.map_or(String::new(), |status| scratch.listing(status));
            let vm = Vm::new(&scratch.limactl(&listing, Some((failing, 7))), "sf-test");
            vm.record(&config, &scratch.0)
                .expect("record the configuration");

            let failure = vm.ensure(&config).expect_err("the VM is not there");

            assert!(failure.contains("`sf-test`"), "{failure}");
            assert!(failure.contains(what), "{failure}");
            assert!(failure.contains("exit code 7"), "{failure}");
        }
    }

    #[test]
    fn the_work_tree_is_its_files_as_they_stand_without_what_git_ignores() {
        let scratch = Scratch::new("tree");
        git(&scratch.0, &["init", "--quiet"]);
        for file in [
            ".gitignore",
            "kept",
            "removed",
            "src/staged",
            "untracked",
            "target/built",
        ] {
            let path = scratch.0.join(file);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("create a directory");
            std::fs::write(&path, file).expect("write a file");
        }
        std::fs::write(scratch.0.join(".gitignore"), "/target\n").expect("write .gitignore");
        git(
            &scratch.0,
            &["add", ".gitignore", "kept", "removed", "src/staged"],
        );
        std::fs::remove_file(scratch.0.join("removed")).expect("remove a tracked file");

        let files = work_tree_files(&scratch.0).expect("list the work tree");

        assert_eq!(files, [".gitignore", "kept", "src/staged", "untracked"]);
    }

    /// A stand-in `limactl` whose `shell` runs the command on this machine,
    /// with `home` as `HOME` and a stand-in `mise` there that records how it
    /// was run, and exits with `exit` for `mise exec`.
    fn local_limactl(scratch: &Scratch, home: &Path, exit: i32) -> String {
        let mise = home.join(".local/bin/mise");
        std::fs::create_dir_all(mise.parent().expect("a parent")).expect("create a directory");
        std::fs::write(
            &mise,
            format!(
                "#!/bin/sh\n\
                 echo \"$* in $PWD with $CARGO_TARGET_DIR\" >> '{calls}'\n\
                 [ \"$1\" = exec ] && exit {exit}\n\
                 exit 0\n",
                calls = scratch.0.join("mise-calls").display(),
            ),
        )
        .expect("write the stand-in mise");
        let limactl = scratch.0.join("limactl");
        std::fs::write(
            &limactl,
            format!(
                "#!/bin/sh\n\
                 [ \"$1\" = shell ] || exit 9\n\
                 while [ \"$1\" != -- ]; do shift; done\n\
                 shift\n\
                 HOME='{home}' exec \"$@\"\n",
                home = home.display(),
            ),
        )
        .expect("write the stand-in limactl");
        for script in [&mise, &limactl] {
            let status = Command::new("chmod")
                .arg("+x")
                .arg(script)
                .status()
                .expect("run chmod");
            assert!(status.success());
        }
        limactl.to_string_lossy().into_owned()
    }

    #[test]
    fn the_set_runs_in_the_vm_on_a_fresh_copy_of_the_work_tree() {
        let scratch = Scratch::new("copy");
        let (root, home) = (scratch.0.join("work"), scratch.0.join("home"));
        std::fs::create_dir_all(root.join("src")).expect("create the work tree");
        std::fs::write(root.join("src/lib.rs"), "fn main() {}\n").expect("write a file");
        git(&root, &["init", "--quiet"]);
        let vm = Vm::new(&local_limactl(&scratch, &home, 0), "sf-test");
        let guest = root.strip_prefix("/").expect("an absolute path");
        let (tree, target) = (
            home.join("sf/tree").join(guest),
            home.join("sf/target").join(guest),
        );
        std::fs::create_dir_all(&tree).expect("create an earlier copy");
        std::fs::write(tree.join("stale"), "").expect("leave a file of an earlier copy");

        let outcome = vm.check(&root);

        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(
            std::fs::read_to_string(tree.join("src/lib.rs"))
                .ok()
                .as_deref(),
            Some("fn main() {}\n")
        );
        assert!(!tree.join("stale").exists(), "the earlier copy was kept");
        let calls = std::fs::read_to_string(scratch.0.join("mise-calls")).expect("mise ran");
        let with = format!("in {} with {}", tree.display(), target.display());
        assert_eq!(
            calls.lines().collect::<Vec<_>>(),
            [
                format!("install --yes {with}"),
                format!("exec -- cargo xtask check {with}")
            ]
        );
    }

    #[test]
    fn a_set_that_fails_in_the_vm_fails_the_check_by_name() {
        let scratch = Scratch::new("fails");
        let root = scratch.0.join("work");
        std::fs::create_dir_all(&root).expect("create the work tree");
        git(&root, &["init", "--quiet"]);
        let vm = Vm::new(
            &local_limactl(&scratch, &scratch.0.join("home"), 5),
            "sf-test",
        );

        let failure = vm.check(&root).expect_err("the set fails");

        assert!(failure.contains("`sf-test`"), "{failure}");
        assert!(failure.contains("exit code 5"), "{failure}");
    }

    #[test]
    fn a_missing_limactl_fails_by_name() {
        let scratch = Scratch::new("missing");
        let vm = Vm::new("xtask-no-such-limactl", "sf-test");

        let failure = vm.ensure(&scratch.config()).expect_err("no limactl");

        assert!(failure.contains("`sf-test`"), "{failure}");
        assert!(failure.contains("xtask-no-such-limactl"), "{failure}");
    }
}

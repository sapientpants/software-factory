//! Which Bash command lines tell git to skip its hooks (SPEC DEV-10).

mod shell;

use std::fmt;

/// How a command line skips git's hooks.
#[derive(Debug, PartialEq, Eq)]
pub enum Skip {
    /// The named git subcommand was given `--no-verify` in some spelling.
    NoVerify(&'static str),
    /// A git command was given its own `core.hooksPath`.
    HooksPath,
}

impl fmt::Display for Skip {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoVerify(subcommand) => {
                write!(formatter, "The command runs `git {subcommand} --no-verify`")
            }
            Self::HooksPath => {
                write!(formatter, "The command gives git its own `core.hooksPath`")
            }
        }
    }
}

/// The first way `command` tells git to skip its hooks, if it does.
///
/// Every command of the line is read, with the shell's quoting undone, and
/// each word `git` in one is taken as the start of a git command, so that a
/// wrapper such as `env` or `time` hides nothing.
pub fn hook_skip(command: &str) -> Option<Skip> {
    shell::commands(command).iter().find_map(|words| {
        (0..words.len())
            .filter(|&index| is_git(&words[index]))
            .find_map(|index| git_skip(&words[index + 1..]))
    })
}

/// Whether a word names the git program, by a path or without one.
fn is_git(word: &str) -> bool {
    word.rsplit('/').next() == Some("git")
}

/// The options of `git` itself whose value is the next word, besides `-c`
/// and `--config-env`.
const GIT_WITH_VALUE: &[&str] = &[
    "-C",
    "--attr-source",
    "--git-dir",
    "--namespace",
    "--work-tree",
];

/// How the words after `git` skip the hooks: git's own options first, then
/// the subcommand's.
fn git_skip(words: &[String]) -> Option<Skip> {
    let mut words = words.iter().map(String::as_str);
    while let Some(word) = words.next() {
        if word == "-c" || word == "--config-env" {
            if sets_hooks_path(words.next()?) {
                return Some(Skip::HooksPath);
            }
        } else if let Some(setting) = word.strip_prefix("--config-env=") {
            if sets_hooks_path(setting) {
                return Some(Skip::HooksPath);
            }
        } else if GIT_WITH_VALUE.contains(&word) {
            words.next();
        } else if !word.starts_with('-') {
            let arguments: Vec<&str> = words.collect();
            return match word {
                "commit" if COMMIT.no_verify_in_force(&arguments) => Some(Skip::NoVerify("commit")),
                "push" if PUSH.no_verify_in_force(&arguments) => Some(Skip::NoVerify("push")),
                _ => None,
            };
        }
    }
    None
}

/// Whether a `name=value` setting names `core.hooksPath`. Git compares the
/// section and the variable without regard to letter case.
fn sets_hooks_path(setting: &str) -> bool {
    let name = setting.split('=').next().unwrap_or(setting);
    name.eq_ignore_ascii_case("core.hooksPath")
}

/// What has to be known of a subcommand's options to tell a flag from the
/// value of another option.
struct Options {
    /// The long options whose value may be the next word.
    long_with_value: &'static [&'static str],
    /// The short options whose value is the rest of the word or, when nothing
    /// follows the letter, the next word.
    short_with_value: &'static [char],
    /// The short options whose value, if any, is the rest of the word and
    /// never the next one.
    short_with_attached_value: &'static [char],
    /// Whether `-n` is the short form of `--no-verify`.
    n_is_no_verify: bool,
}

const COMMIT: Options = Options {
    long_with_value: &[
        "--author",
        "--cleanup",
        "--date",
        "--file",
        "--fixup",
        "--message",
        "--pathspec-from-file",
        "--reedit-message",
        "--reuse-message",
        "--squash",
        "--template",
        "--trailer",
    ],
    short_with_value: &['C', 'c', 'F', 'm', 't'],
    short_with_attached_value: &['S', 'u'],
    n_is_no_verify: true,
};

/// On a push `-n` is `--dry-run`.
const PUSH: Options = Options {
    long_with_value: &["--exec", "--push-option", "--receive-pack", "--repo"],
    short_with_value: &['o'],
    short_with_attached_value: &[],
    n_is_no_verify: false,
};

impl Options {
    /// Whether the arguments leave `--no-verify` in force. As in git, the
    /// last of `--verify` and `--no-verify` wins, and a word that is the
    /// value of an option is no flag and does not end the options.
    fn no_verify_in_force(&self, arguments: &[&str]) -> bool {
        let mut in_force = false;
        let mut arguments = arguments.iter().copied();
        while let Some(argument) = arguments.next() {
            if argument == "--" {
                break;
            }
            if let Some(verify) = verify_option(argument) {
                in_force = !verify;
            } else if self.is_long_with_value(argument) {
                arguments.next();
            } else if let Some(letters) = short_options(argument) {
                for (index, letter) in letters.char_indices() {
                    if letter == 'n' && self.n_is_no_verify {
                        in_force = true;
                    } else if self.short_with_value.contains(&letter) {
                        if letters[index + letter.len_utf8()..].is_empty() {
                            arguments.next();
                        }
                        break;
                    } else if self.short_with_attached_value.contains(&letter) {
                        break;
                    }
                }
            }
        }
        in_force
    }

    /// Whether `argument` is a long option that takes the next word as its
    /// value, written out or abbreviated. Where git would call an
    /// abbreviation ambiguous it runs nothing, so the answer does not matter.
    fn is_long_with_value(&self, argument: &str) -> bool {
        argument.len() > 2
            && self
                .long_with_value
                .iter()
                .any(|option| option.starts_with(argument))
    }
}

/// `Some(true)` for `--verify`, `Some(false)` for `--no-verify`, in any
/// abbreviation git accepts. A shorter one is ambiguous with `--verbose` or
/// `--no-verbose`, and git refuses it.
fn verify_option(argument: &str) -> Option<bool> {
    let abbreviates = |option: &str, shortest: &str| {
        option.starts_with(argument) && argument.starts_with(shortest)
    };
    if abbreviates("--no-verify", "--no-veri") {
        Some(false)
    } else if abbreviates("--verify", "--veri") {
        Some(true)
    } else {
        None
    }
}

/// The letters of a word of short options, such as `anm` for `-anm`.
fn short_options(argument: &str) -> Option<&str> {
    argument
        .strip_prefix('-')
        .filter(|letters| !letters.is_empty() && !letters.starts_with('-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_with_no_verify_skips_the_hooks() {
        assert_eq!(
            hook_skip("git commit --no-verify -m x"),
            Some(Skip::NoVerify("commit"))
        );
    }

    #[test]
    fn a_plain_commit_skips_nothing() {
        assert_eq!(hook_skip("git commit -m x"), None);
    }

    // The expectations of the tables below are what git 2.56.1 did with each
    // command in a repository whose every hook fails.

    #[test]
    fn every_spelling_of_no_verify_on_a_commit_is_a_skip() {
        for command in [
            "git commit -n -m x",
            "git commit -nm x",
            "git commit -anm x",
            "git commit -qn -m x",
            "git commit -m x -n",
            "git commit -F message.txt -n",
            "git commit -nF message.txt",
            "git commit --no-verif -m x",
            "git commit --no-veri -m x",
            "git commit -m x --verify --no-verify",
            "git commit --mess -- -n",
            "git commit --mes x -n",
        ] {
            assert_eq!(
                hook_skip(command),
                Some(Skip::NoVerify("commit")),
                "{command}"
            );
        }
    }

    #[test]
    fn a_commit_that_runs_its_hooks_is_not_a_skip() {
        for command in [
            "git commit -m -n",
            "git commit -mnaming",
            "git commit -Fnotes.txt",
            "git commit --message -n",
            "git commit --message=-n",
            "git commit --mess -n",
            "git commit --no-verify --verify -m x",
            "git commit -m x --veri",
            "git commit -m x -- -n",
            "git commit",
        ] {
            assert_eq!(hook_skip(command), None, "{command}");
        }
    }

    #[test]
    fn no_verify_on_a_push_is_a_skip() {
        for command in [
            "git push --no-verify",
            "git push --no-veri origin main",
            "git push origin main --no-verify",
            "git push -o -- --no-verify origin main",
            "git push --push-opt -- --no-verify origin main",
        ] {
            assert_eq!(
                hook_skip(command),
                Some(Skip::NoVerify("push")),
                "{command}"
            );
        }
    }

    #[test]
    fn a_push_that_runs_its_hooks_is_not_a_skip() {
        for command in [
            "git push",
            "git push -n origin main",
            "git push --no-verify --verify origin main",
            "git push -o --no-verify origin main",
        ] {
            assert_eq!(hook_skip(command), None, "{command}");
        }
    }

    #[test]
    fn the_subcommand_is_found_behind_gits_own_options() {
        for command in [
            "git -C . commit -n -m x",
            "git --no-pager -c user.name=n commit -n",
            "/opt/homebrew/bin/git commit -n -m x",
        ] {
            assert_eq!(
                hook_skip(command),
                Some(Skip::NoVerify("commit")),
                "{command}"
            );
        }
        assert_eq!(hook_skip("git --no-pager -c user.name=n commit -m x"), None);
    }

    #[test]
    fn a_hooks_path_given_on_the_command_line_is_a_skip() {
        for command in [
            "git -c core.hooksPath=/dev/null commit -m x",
            "git -c CORE.HooksPath=/dev/null commit -m x",
            "git --config-env core.hooksPath=HOME commit -m x",
            "git --config-env=core.hooksPath=HOME commit -m x",
            "git -C . -c core.hooksPath=/dev/null push",
        ] {
            assert_eq!(hook_skip(command), Some(Skip::HooksPath), "{command}");
        }
    }

    #[test]
    fn a_flag_inside_a_quoted_message_is_not_a_skip() {
        for command in [
            r#"git commit -m "fix -n handling""#,
            "git commit -m 'never pass --no-verify'",
            r#"git commit -m "say \"git commit -n\" is refused""#,
            "git commit -m \"$(cat <<'EOF'\nRefuse --no-verify\n\ngit commit -n is refused.\nEOF\n)\"",
            r"git commit -m fix\ -n",
            r#"git commit -m "$(printf "%s -n" x)""#,
            "git commit -m x # -n would skip the hook",
            r#"git commit -F - <<< "-n""#,
            "git commit -m x > -n",
        ] {
            assert_eq!(hook_skip(command), None, "{command}");
        }
    }

    #[test]
    fn a_quoted_flag_is_still_a_flag() {
        for command in [
            r#"git commit -m x "--no-verify""#,
            "git commit '-n' -m x",
            r#"git commit -m"x" -n"#,
        ] {
            assert_eq!(
                hook_skip(command),
                Some(Skip::NoVerify("commit")),
                "{command}"
            );
        }
    }

    #[test]
    fn a_skip_is_found_in_any_command_of_the_line() {
        for command in [
            "cd crates && git commit -n -m x",
            "git add . ; git commit -nm x",
            "git add .;git commit -nm x",
            "cargo test || git commit -n",
            "git add .\ngit commit -n -m x",
            "(git commit -n)",
            "echo $(git commit -n)",
            "echo `git commit -n`",
            "FOO=1 git commit -n",
            "time git commit -n",
            "env GIT_EDITOR=true git commit -n",
            "git status | git commit -n -F -",
            "mise exec -- git commit -n",
            "git 2>/dev/null commit -n",
            "git commit -F <(echo message) -n",
            "git commit -m x $'-n'",
            "echo $((1<<2))\ngit commit -n",
            "env -C git git commit -n",
            "git commit \\\n  -n -m x",
            "cat <<EOF\ntext\nEOF\ngit commit -n",
            r#"echo "$(git commit -n)""#,
            "git commit -m x 2>&1 -n",
            "git commit -m x &> log -n",
        ] {
            assert_eq!(
                hook_skip(command),
                Some(Skip::NoVerify("commit")),
                "{command}"
            );
        }
    }

    #[test]
    fn a_flag_of_another_command_of_the_line_is_not_a_skip() {
        for command in [
            "git commit -m x && ls -n",
            "git commit -m x; cargo test --no-verify",
            "git commit -m x | grep -n foo",
            "git commit -m x\nsort -n numbers",
            "git commit -m x & wait -n",
            "git commit -F - <<'EOF'\nRefuse --no-verify\n\ngit commit -n is refused.\nEOF",
            "git commit -F - <<-EOF\n\tgit commit -n\n\tEOF",
        ] {
            assert_eq!(hook_skip(command), None, "{command}");
        }
    }

    #[test]
    fn other_subcommands_keep_their_own_n() {
        for command in [
            "git log -n 3",
            "git clean -n",
            "git add -n .",
            "git status",
            "git",
            "cargo test -n",
        ] {
            assert_eq!(hook_skip(command), None, "{command}");
        }
    }
}

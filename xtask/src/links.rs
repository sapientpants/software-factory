//! `cargo xtask links`: the check of local Markdown links (SPEC DEV-2). A link
//! from a Markdown file of the repository to a path of the repository that
//! does not exist fails it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pulldown_cmark::{Event, Options, Parser, Tag};

pub fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let broken = broken(&root);
    for link in &broken {
        eprintln!("xtask links: {link}");
    }
    if broken.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Every local link of a Markdown file below `root` whose path does not exist,
/// each as `file:line: destination` and a reason.
pub fn broken(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    markdown_files(root, root, &mut files);
    files.sort();
    let mut broken = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(root.join(&file)) else {
            broken.push(format!("{}: cannot be read as UTF-8", file.display()));
            continue;
        };
        for (offset, destination) in destinations(&text) {
            let Some(path) = local_path(&destination) else {
                continue;
            };
            let line = 1 + text[..offset].matches('\n').count();
            let directory = file.parent().unwrap_or(Path::new(""));
            let Some(target) = resolve(directory, &path) else {
                broken.push(format!(
                    "{}:{line}: {destination} leaves the repository",
                    file.display()
                ));
                continue;
            };
            if !root.join(target).exists() {
                broken.push(format!(
                    "{}:{line}: {destination} does not exist",
                    file.display()
                ));
            }
        }
    }
    broken
}

/// The path that `destination` names in the repository, or `None` where it
/// names none: where it has a scheme, as `https:` and `mailto:` do, starts
/// with `//`, or is a fragment alone, a heading of its own file.
fn local_path(destination: &str) -> Option<String> {
    let scheme = destination.split_once(':').is_some_and(|(scheme, _)| {
        scheme.starts_with(|first: char| first.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|letter| letter.is_ascii_alphanumeric() || "+-.".contains(letter))
    });
    if scheme || destination.starts_with("//") {
        return None;
    }
    let path = destination
        .split(['#', '?'])
        .next()
        .filter(|path| !path.is_empty())?;
    Some(percent_decoded(path))
}

/// `path` with each `%` and two hexadecimal digits replaced by the byte they
/// name; as written where the bytes are no UTF-8.
fn percent_decoded(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = path
            .get(index + 1..index + 3)
            .filter(|_| bytes[index] == b'%')
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match byte {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| path.to_owned())
}

/// The path relative to the root that `destination` names from a file in
/// `directory`, as GitHub reads it: from the root where it starts with `/`,
/// and otherwise from the file's directory. `None` where `..` leaves the root.
fn resolve(directory: &Path, destination: &str) -> Option<PathBuf> {
    let (start, path) = match destination.strip_prefix('/') {
        Some(path) => (Path::new(""), path),
        None => (directory, destination),
    };
    let mut target: Vec<&str> = start.iter().filter_map(|part| part.to_str()).collect();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                target.pop()?;
            }
            name => target.push(name),
        }
    }
    Some(target.iter().collect())
}

/// The Markdown files below `directory`, as paths relative to `root`. The walk
/// follows no symbolic link and leaves out `.git` and `target` at the root, as
/// the walk of the structural test does (DEV-2b).
fn markdown_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            let name = entry.file_name();
            let outside = directory == root && (name == ".git" || name == "target");
            if !outside {
                markdown_files(root, &path, files);
            }
        } else if kind.is_file()
            && path.extension().is_some_and(|extension| extension == "md")
            && let Ok(relative) = path.strip_prefix(root)
        {
            files.push(relative.to_path_buf());
        }
    }
}

/// The destination of each link and image of `text`, with the byte offset at
/// which its element starts.
fn destinations(text: &str) -> Vec<(usize, String)> {
    Parser::new_ext(text, Options::ENABLE_TABLES)
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
                Some((range.start, dest_url.into_string()))
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A fresh tree for one test, removed when the value is dropped.
    struct Tree(PathBuf);

    impl Tree {
        fn new(test: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("xtask-links-{}-{test}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create the tree");
            Self(path)
        }

        /// Writes `text` to `file`, a path relative to the tree.
        fn with(self, file: &str, text: &str) -> Self {
            let path = self.0.join(file);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("create a directory");
            std::fs::write(path, text).expect("write a file");
            self
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_link_to_a_missing_file_is_broken_and_one_to_a_file_that_exists_is_not() {
        let tree = Tree::new("missing").with("SPEC.md", "# Spec\n").with(
            "README.md",
            "See [the spec](SPEC.md).\n\nAnd [gone](GONE.md).\n",
        );

        let broken = broken(&tree.0);

        assert_eq!(broken.len(), 1, "{broken:?}");
        assert!(broken[0].starts_with("README.md:3: GONE.md"), "{broken:?}");
    }

    #[test]
    fn a_link_is_read_from_the_directory_of_its_file_and_a_leading_slash_from_the_root() {
        let tree = Tree::new("relative")
            .with("GLOSSARY.md", "# Terms\n")
            .with("docs/research/01-a.md", "# A\n")
            .with(
                "docs/research/02-b.md",
                "[a](01-a.md) [up](../../GLOSSARY.md) [root](/GLOSSARY.md) [dir](../research)\n\
                 \n\
                 [wrong](GLOSSARY.md)\n",
            );

        let broken = broken(&tree.0);

        assert_eq!(broken.len(), 1, "{broken:?}");
        assert!(
            broken[0].starts_with("docs/research/02-b.md:3: GLOSSARY.md"),
            "{broken:?}"
        );
    }

    #[test]
    fn a_link_with_a_scheme_and_one_to_a_heading_of_its_own_file_are_no_local_links() {
        let tree = Tree::new("remote").with(
            "README.md",
            "[web](https://example.com/GONE.md) [mail](mailto:someone@example.com) \
             [net](//example.com/GONE.md) [here](#a-heading) <https://example.com/x.md>\n",
        );

        let broken = broken(&tree.0);

        assert!(broken.is_empty(), "{broken:?}");
    }

    #[test]
    fn the_path_of_a_link_is_read_without_its_fragment_and_query_and_percent_decoded() {
        let tree = Tree::new("forms")
            .with("docs/my file.md", "# Mine\n")
            .with("docs/img.png", "")
            .with(
                "README.md",
                "[frag](docs/my%20file.md#a-heading) [query](docs/my%20file.md?x=1) \
                 [angle](<docs/my file.md>) ![alt](docs/img.png) [ref][r] [gone ref][g]\n\
                 \n\
                 [r]: docs/my%20file.md\n\
                 [g]: docs/gone%20file.md#top\n",
            );

        let broken = broken(&tree.0);

        assert_eq!(broken.len(), 1, "{broken:?}");
        assert!(
            broken[0].contains("docs/gone%20file.md#top does not exist"),
            "{broken:?}"
        );
    }

    #[test]
    fn the_files_of_git_and_of_the_build_output_at_the_root_are_not_read() {
        let tree = Tree::new("walk")
            .with(".git/notes.md", "[gone](GONE.md)\n")
            .with("target/doc/notes.md", "[gone](GONE.md)\n")
            .with(".github/notes.md", "[gone](GONE.md)\n")
            .with("crates/target/notes.md", "[gone](GONE.md)\n");

        let broken = broken(&tree.0);

        assert_eq!(broken.len(), 2, "{broken:?}");
        assert!(broken[0].starts_with(".github/notes.md:1:"), "{broken:?}");
        assert!(
            broken[1].starts_with("crates/target/notes.md:1:"),
            "{broken:?}"
        );
    }

    #[test]
    fn a_link_that_leaves_the_repository_is_broken() {
        let tree = Tree::new("leaves").with("docs/a.md", "[out](../../outside.md)\n");

        let broken = broken(&tree.0);

        assert_eq!(
            broken,
            ["docs/a.md:1: ../../outside.md leaves the repository"]
        );
    }

    #[test]
    fn a_link_in_code_or_in_a_comment_is_no_link() {
        let tree = Tree::new("code").with(
            "README.md",
            "A span `[s](GONE.md)`.\n\
             \n\
             <!-- [h](GONE.md) -->\n\
             \n\
             ````markdown\n\
             [f](GONE.md)\n\
             ````\n\
             \n    [i](GONE.md)\n",
        );

        let broken = broken(&tree.0);

        assert!(broken.is_empty(), "{broken:?}");
    }
}

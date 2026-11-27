//! The files of the work tree that the checks of xtask read (SPEC DEV-2).

use std::path::{Path, PathBuf};

/// The files below `root`, as sorted paths relative to it. The walk follows no
/// symbolic link and leaves out `.git` and `target` at the root, as the walk
/// of the structural test does (DEV-2b).
pub fn files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk(root, root, &mut files);
    files.sort();
    files
}

fn walk(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) {
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
                walk(root, &path, files);
            }
        } else if kind.is_file()
            && let Ok(relative) = path.strip_prefix(root)
        {
            files.push(relative.to_path_buf());
        }
    }
}

/// A fresh tree for one test, removed when the value is dropped.
#[cfg(test)]
pub struct Tree(pub PathBuf);

#[cfg(test)]
impl Tree {
    /// An empty tree named after `test`, which is unique among the tests.
    pub fn new(test: &str) -> Self {
        let path = std::env::temp_dir().join(format!("xtask-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create the tree");
        Self(path)
    }

    /// Writes `contents` to `file`, a path relative to the tree.
    pub fn with(self, file: &str, contents: impl AsRef<[u8]>) -> Self {
        let path = self.0.join(file);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("create a directory");
        std::fs::write(path, contents).expect("write a file");
        self
    }
}

#[cfg(test)]
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

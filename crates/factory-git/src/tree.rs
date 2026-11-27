//! The walk of a tree at ingest (SPEC 9.4 step 3): its modes, its paths of
//! one fold (WS-12), the layouts of a repository in it (WS-13) and the sizes
//! of what changed. Pure functions over the entries that `ls-tree` listed.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use factory_core::fold::fold_bytes;

use crate::capture::{Rejection, TreePath};

/// An entry of a tree, as `ls-tree -r -t -l` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The mode, as git prints it: `100644`, `040000` and so on.
    pub(crate) mode: String,
    /// The object name.
    pub(crate) oid: String,
    /// The size of a blob; 0 for a tree or a commit.
    pub(crate) size: u64,
}

impl Entry {
    fn is_tree(&self) -> bool {
        self.mode == "040000"
    }
}

/// The entries of a tree at every depth, by path.
pub(crate) type Tree = BTreeMap<Vec<u8>, Entry>;

/// Every entry whose mode is not one that a candidate may hold: a regular
/// file, an executable file or a symbolic link, beside the trees that hold
/// them. A submodule entry is one.
pub(crate) fn modes(tree: &Tree) -> Vec<Rejection> {
    tree.iter()
        .filter(|(_, entry)| {
            !matches!(
                entry.mode.as_str(),
                "040000" | "100644" | "100755" | "120000"
            )
        })
        .map(|(path, entry)| Rejection::Mode {
            path: TreePath::from(path.as_slice()),
            mode: entry.mode.clone(),
        })
        .collect()
}

/// The paths other than trees in which `tree` and `other` differ: added,
/// changed in mode or content, or removed.
pub(crate) fn differing<'a>(tree: &'a Tree, other: &'a Tree) -> BTreeSet<&'a [u8]> {
    let files = |t: &'a Tree| t.iter().filter(|(_, entry)| !entry.is_tree());
    let mut paths = BTreeSet::new();
    for (path, entry) in files(tree) {
        if other.get(path) != Some(entry) {
            paths.insert(path.as_slice());
        }
    }
    for (path, entry) in files(other) {
        if tree.get(path) != Some(entry) {
            paths.insert(path.as_slice());
        }
    }
    paths
}

/// The rejections of WS-12: each two paths of `tree` that have the same fold
/// (VAL-5d), unless `base` has an entry at both.
pub(crate) fn same_folds(tree: &Tree, base: &Tree) -> Vec<Rejection> {
    let mut by_fold: HashMap<String, Vec<&[u8]>> = HashMap::new();
    for path in tree.keys() {
        by_fold.entry(fold_bytes(path)).or_default().push(path);
    }
    let mut rejections = Vec::new();
    // A group of one path makes no pair.
    let mut groups: Vec<_> = by_fold.into_values().collect();
    groups.sort();
    for paths in groups {
        for (i, first) in paths.iter().enumerate() {
            for second in paths.iter().skip(i + 1) {
                if !(base.contains_key(*first) && base.contains_key(*second)) {
                    rejections.push(Rejection::SameFold {
                        first: TreePath::from(*first),
                        second: TreePath::from(*second),
                    });
                }
            }
        }
    }
    rejections
}

/// The rejections of WS-13: each path in which `tree` differs from `base`
/// that lies below a directory with the layout of a repository in `tree`,
/// or with it in `base` where the directory still holds an entry in `tree`.
pub(crate) fn below_layouts(tree: &Tree, base: &Tree) -> Vec<Rejection> {
    let in_tree = layouts(tree);
    let in_base = layouts(base);
    let occupied = occupied(tree);
    let mut rejections = Vec::new();
    for path in differing(tree, base) {
        let layout = ancestors(path).find_map(|dir| {
            let folded = fold_bytes(dir);
            let named = in_tree
                .get(&folded)
                .or_else(|| in_base.get(&folded).filter(|_| occupied.contains(&folded)))?;
            Some(named.clone())
        });
        if let Some(layout) = layout {
            rejections.push(Rejection::BelowLayout {
                path: TreePath::from(path),
                layout: TreePath::from(layout.as_slice()),
            });
        }
    }
    rejections
}

/// The directories of `tree` that have the layout of a repository, by their
/// fold, each with a path that spells it: those that hold an entry `HEAD`
/// with entries `objects` and `refs`, or with an entry `commondir`. Entries
/// count whatever they are, and the entries of two directories with one fold
/// count as those of one. The root is the empty path.
fn layouts(tree: &Tree) -> HashMap<String, Vec<u8>> {
    let mut names: HashMap<String, (Vec<u8>, HashSet<String>)> = HashMap::new();
    for path in tree.keys() {
        let (dir, name) = split(path);
        names
            .entry(fold_bytes(dir))
            .or_insert_with(|| (dir.to_vec(), HashSet::new()))
            .1
            .insert(fold_bytes(name));
    }
    names
        .into_iter()
        .filter(|(_, (_, names))| {
            names.contains("head")
                && ((names.contains("objects") && names.contains("refs"))
                    || names.contains("commondir"))
        })
        .map(|(folded, (dir, _))| (folded, dir))
        .collect()
}

/// The folds of every directory of `tree` that holds an entry, the root
/// among them where the tree is not empty.
fn occupied(tree: &Tree) -> HashSet<String> {
    tree.keys()
        .flat_map(|path| ancestors(path))
        .map(fold_bytes)
        .collect()
}

/// The directories that `path` lies below: the root, as the empty path, and
/// each proper prefix that ends before a `/`.
fn ancestors(path: &[u8]) -> impl Iterator<Item = &[u8]> {
    std::iter::once(&path[..0]).chain(
        path.iter()
            .enumerate()
            .filter(|(_, byte)| **byte == b'/')
            .filter_map(|(i, _)| path.get(..i)),
    )
}

/// The directory of `path` and its last name.
fn split(path: &[u8]) -> (&[u8], &[u8]) {
    match path.iter().rposition(|byte| *byte == b'/') {
        Some(i) => (
            path.get(..i).unwrap_or_default(),
            path.get(i + 1..).unwrap_or_default(),
        ),
        None => (&path[..0], path),
    }
}

/// The rejections of the capture limits, by the sizes of the blobs of
/// `tree` that differ from `previous`: each file over `file_bytes`, and the
/// change set where its total is over `total_bytes`.
pub(crate) fn sizes(
    tree: &Tree,
    previous: &Tree,
    file_bytes: u64,
    total_bytes: u64,
) -> Vec<Rejection> {
    let mut rejections = Vec::new();
    let mut total: u64 = 0;
    for path in differing(tree, previous) {
        let Some(entry) = tree.get(path) else {
            continue;
        };
        total = total.saturating_add(entry.size);
        if entry.size > file_bytes {
            rejections.push(Rejection::FileTooLarge {
                path: TreePath::from(path),
                bytes: entry.size,
            });
        }
    }
    if total > total_bytes {
        rejections.push(Rejection::ChangeSetTooLarge { bytes: total });
    }
    rejections
}

#[cfg(test)]
mod tests {
    use super::{Entry, Tree, below_layouts, differing, modes, same_folds, sizes};
    use crate::capture::{Rejection, TreePath};

    fn blob(oid: char, size: u64) -> Entry {
        Entry {
            mode: "100644".to_owned(),
            oid: oid.to_string().repeat(40),
            size,
        }
    }

    /// A tree of `files`, each a blob of its own content, with the trees that
    /// hold them.
    fn tree(files: &[(&str, char)]) -> Tree {
        let mut tree = Tree::new();
        for (path, oid) in files {
            tree.insert(path.as_bytes().to_vec(), blob(*oid, 1));
            let mut dir = *path;
            while let Some(i) = dir.rfind('/') {
                dir = &dir[..i];
                tree.insert(
                    dir.as_bytes().to_vec(),
                    Entry {
                        mode: "040000".to_owned(),
                        oid: "d".repeat(40),
                        size: 0,
                    },
                );
            }
        }
        tree
    }

    fn path(text: &str) -> TreePath {
        TreePath::from(text.as_bytes())
    }

    fn pair(first: &str, second: &str) -> Rejection {
        Rejection::SameFold {
            first: path(first),
            second: path(second),
        }
    }

    fn below(file: &str, layout: &str) -> Rejection {
        Rejection::BelowLayout {
            path: path(file),
            layout: path(layout),
        }
    }

    #[test]
    fn a_submodule_entry_is_a_mode_that_no_candidate_holds() {
        let mut t = tree(&[("a", '1')]);
        t.insert(
            b"sub".to_vec(),
            Entry {
                mode: "160000".to_owned(),
                oid: "2".repeat(40),
                size: 0,
            },
        );
        assert_eq!(
            modes(&t),
            [Rejection::Mode {
                path: path("sub"),
                mode: "160000".to_owned()
            }]
        );
    }

    #[test]
    fn the_paths_that_differ_are_those_added_changed_and_removed() {
        let base = tree(&[("kept", '1'), ("changed", '1'), ("removed", '1')]);
        let new = tree(&[("kept", '1'), ("changed", '2'), ("added/file", '1')]);
        let got: Vec<&[u8]> = differing(&new, &base).into_iter().collect();
        assert_eq!(got, [b"added/file".as_slice(), b"changed", b"removed"]);
    }

    #[test]
    fn two_paths_of_one_fold_are_rejected_unless_the_base_tree_holds_both() {
        let base = tree(&[("README.md", '1')]);
        // Pairs of probe P-40 that only the full fold makes one.
        let new = tree(&[
            ("README.md", '1'),
            ("readme.md", '2'),
            ("STRASSE.txt", '1'),
            ("stra\u{DF}e.txt", '2'),
            ("\u{E9}.md", '1'),
            ("e\u{301}.md", '2'),
            ("src/a.rs", '1'),
            ("SRC/a.rs", '2'),
        ]);
        let mut got = same_folds(&new, &base);
        got.sort_by_key(|rejection| format!("{rejection:?}"));
        let mut expected = vec![
            pair("README.md", "readme.md"),
            pair("STRASSE.txt", "stra\u{DF}e.txt"),
            pair("e\u{301}.md", "\u{E9}.md"),
            pair("SRC", "src"),
            pair("SRC/a.rs", "src/a.rs"),
        ];
        expected.sort_by_key(|rejection| format!("{rejection:?}"));
        assert_eq!(got, expected);

        // A pair that the base tree holds is admitted, whatever the round did
        // to either path.
        let base = tree(&[("README.md", '1'), ("readme.md", '2')]);
        let new = tree(&[("README.md", '3'), ("readme.md", '4')]);
        assert!(same_folds(&new, &base).is_empty());
        // A name that differs by a fold that the Mac does not make, the
        // fullwidth `a`, is no pair.
        assert!(same_folds(&tree(&[("a", '1'), ("\u{FF41}", '2')]), &Tree::new()).is_empty());
    }

    #[test]
    fn a_path_that_a_round_brings_below_a_layout_is_rejected() {
        let base = tree(&[("src/lib.rs", '1')]);
        // `HEAD` with `objects` and `refs`, as files or directories.
        let new = tree(&[
            ("src/lib.rs", '1'),
            ("fix/HEAD", '1'),
            ("fix/config", '1'),
            ("fix/objects", '1'),
            ("fix/refs/keep", '1'),
        ]);
        let mut got = below_layouts(&new, &base);
        got.sort_by_key(|rejection| format!("{rejection:?}"));
        assert_eq!(
            got,
            [
                below("fix/HEAD", "fix"),
                below("fix/config", "fix"),
                below("fix/objects", "fix"),
                below("fix/refs/keep", "fix"),
            ]
        );
        // `HEAD` with `commondir`, in letter cases that fold to the names.
        let new = tree(&[("src/lib.rs", '1'), ("x/head", '1'), ("x/CommonDir", '1')]);
        assert_eq!(below_layouts(&new, &base).len(), 2);
        // `HEAD` without `refs` is no layout.
        let new = tree(&[("src/lib.rs", '1'), ("x/HEAD", '1'), ("x/objects", '1')]);
        assert!(below_layouts(&new, &base).is_empty());
    }

    #[test]
    fn the_entries_of_two_directories_with_one_fold_make_one_layout() {
        let new = tree(&[
            ("fix/HEAD", '1'),
            ("FIX/config", '1'),
            ("FIX/objects", '1'),
            ("FIX/refs", '1'),
        ]);
        assert_eq!(below_layouts(&new, &Tree::new()).len(), 4);
    }

    #[test]
    fn a_layout_of_the_base_tree_admits_no_change_below_it_unless_it_is_removed_whole() {
        let base = tree(&[
            ("src/lib.rs", '1'),
            ("fix/HEAD", '1'),
            ("fix/config", '1'),
            ("fix/objects/keep", '1'),
            ("fix/refs/keep", '1'),
        ]);
        // A change elsewhere passes.
        let mut new = base.clone();
        new.insert(b"src/lib.rs".to_vec(), blob('2', 1));
        assert!(below_layouts(&new, &base).is_empty());
        // A change to its `config` does not, nor the removal of one entry of
        // the layout.
        let mut new = base.clone();
        new.insert(b"fix/config".to_vec(), blob('2', 1));
        new.remove(b"fix/refs/keep".as_slice());
        new.remove(b"fix/refs".as_slice());
        let mut got = below_layouts(&new, &base);
        got.sort_by_key(|rejection| format!("{rejection:?}"));
        assert_eq!(
            got,
            [below("fix/config", "fix"), below("fix/refs/keep", "fix")]
        );
        // The directory removed whole passes.
        let new = tree(&[("src/lib.rs", '1')]);
        assert!(below_layouts(&new, &base).is_empty());
    }

    #[test]
    fn a_layout_at_the_root_rejects_every_path_that_differs() {
        let base = tree(&[("src/lib.rs", '1')]);
        let new = tree(&[
            ("src/lib.rs", '2'),
            ("HEAD", '1'),
            ("objects", '1'),
            ("refs", '1'),
        ]);
        assert_eq!(below_layouts(&new, &base).len(), 4);
    }

    #[test]
    fn the_limits_count_the_blobs_that_differ_from_the_previous_tree() {
        let previous = tree(&[("big", '1')]);
        let mut new = previous.clone();
        // An unchanged file over the limit counts for nothing.
        new.get_mut(b"big".as_slice()).unwrap().size = 100;
        let mut previous = new.clone();
        previous.insert(b"gone".to_vec(), blob('9', 50));
        new.insert(b"a".to_vec(), blob('2', 10));
        new.insert(b"b".to_vec(), blob('3', 11));
        assert_eq!(
            sizes(&new, &previous, 10, 21),
            [Rejection::FileTooLarge {
                path: path("b"),
                bytes: 11
            }]
        );
        assert_eq!(
            sizes(&new, &previous, 11, 20),
            [Rejection::ChangeSetTooLarge { bytes: 21 }]
        );
        assert!(sizes(&new, &previous, 11, 21).is_empty());
    }
}

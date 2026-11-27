//! Which names git treats as `.git` (SPEC 9.4 step 1): git's own rule for
//! that one name, ported from `is_hfs_dotgit` and `is_ntfs_dotgit`, which it
//! applies under `core.protectHFS` and `core.protectNTFS`. It is not the fold
//! of VAL-5d, which knows no spelling of HFS+ or NTFS (P-29).

/// Whether git would treat the name of one directory entry as `.git`, on any
/// file system: in any letter case, with the code points that HFS+ ignores
/// inside it, or with what NTFS strips or reads in place of it.
pub(crate) fn is_dot_git(name: &[u8]) -> bool {
    is_hfs_dot_git(name) || is_ntfs_dot_git(name)
}

/// git's `is_hfs_dotgit`: `.git` in ASCII letters of any case, once the code
/// points that HFS+ ignores are left out, followed by the end of the name. A
/// sequence that is no UTF-8 reads as the end of the name, as in git.
fn is_hfs_dot_git(name: &[u8]) -> bool {
    let mut chars = HfsChars::new(name);
    if chars.next() != Some('.') {
        return false;
    }
    for wanted in ['g', 'i', 't'] {
        match chars.next() {
            Some(c) if c.is_ascii() && c.to_ascii_lowercase() == wanted => {}
            _ => return false,
        }
    }
    matches!(chars.next(), None | Some('/'))
}

/// The characters of a name, without those that HFS+ ignores, up to the
/// first sequence that is no UTF-8.
struct HfsChars<'a> {
    rest: &'a [u8],
}

impl<'a> HfsChars<'a> {
    fn new(name: &'a [u8]) -> Self {
        Self { rest: name }
    }
}

impl Iterator for HfsChars<'_> {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        loop {
            let c = first_char(self.rest)?;
            self.rest = self.rest.get(c.len_utf8()..).unwrap_or_default();
            if !matches!(
                c,
                '\u{200C}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{206A}'..='\u{206F}' | '\u{FEFF}'
            ) {
                return Some(c);
            }
        }
    }
}

/// The first character of `bytes`, or `None` at their end or where they do
/// not start with UTF-8.
fn first_char(bytes: &[u8]) -> Option<char> {
    let len = bytes.len().min(4);
    let start = bytes.get(..len)?;
    let valid = match std::str::from_utf8(start) {
        Ok(text) => text,
        Err(error) => std::str::from_utf8(start.get(..error.valid_up_to())?).ok()?,
    };
    valid.chars().next()
}

/// git's `is_ntfs_dotgit`: a component of the name, between backslashes,
/// that is `.git` or `git~1` in any letter case followed only by spaces and
/// periods, up to the end of the name or a `:`, which starts a stream name.
fn is_ntfs_dot_git(name: &[u8]) -> bool {
    for piece in name.split(|&byte| byte == b'\\') {
        // git reads a component up to a `/` or a `:`, and goes on to the next
        // one only after a backslash.
        let end = piece.iter().position(|&byte| matches!(byte, b'/' | b':'));
        let component = piece.get(..end.unwrap_or(piece.len())).unwrap_or_default();
        if matches_with_trailing(component, b".git") || matches_with_trailing(component, b"git~1") {
            return true;
        }
        if end.is_some() {
            return false;
        }
    }
    false
}

/// Whether `component` is `word` in any ASCII letter case, followed by
/// nothing but spaces and periods.
fn matches_with_trailing(component: &[u8], word: &[u8]) -> bool {
    component.len() >= word.len()
        && component
            .get(..word.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(word))
        && component
            .get(word.len()..)
            .unwrap_or_default()
            .iter()
            .all(|&byte| matches!(byte, b' ' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::is_dot_git;

    #[test]
    fn the_names_that_git_treats_as_dot_git() {
        for name in [
            ".git",
            ".GIT",
            ".Git",
            ".git ",
            ".git.",
            ".git. .",
            "git~1",
            "GIT~1",
            "git~1 .",
            ".g\u{200C}it",
            "\u{FEFF}.git",
            ".git\u{206A}",
            ".git:stream",
            "x\\.git",
            "x\\.git\\config",
            "a\\b\\GIT~1",
        ] {
            assert!(is_dot_git(name.as_bytes()), "{name:?}");
        }
        // A sequence that is no UTF-8 after `.git` reads as its end.
        assert!(is_dot_git(b".git\xff"));
    }

    #[test]
    fn the_names_that_git_does_not_treat_as_dot_git() {
        for name in [
            "git",
            ".gitignore",
            ".gitattributes",
            ".github",
            ".git-blame-ignore-revs",
            "..git",
            "x.git",
            "git~2",
            "git~10",
            ".gi",
            ".g\u{00ED}t",
            "x:.git",
            ".gut",
            ".abc",
            ".gix",
            "x:y\\.git",
            "x/y\\git~1",
            ".git x",
            "",
        ] {
            assert!(!is_dot_git(name.as_bytes()), "{name:?}");
        }
        assert!(!is_dot_git(b".g\xffit"));
    }
}

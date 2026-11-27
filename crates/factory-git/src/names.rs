//! The values that the wrapper puts on a command line: an object name, a
//! branch name and a remote. Each is checked before it reaches `git`, so none
//! can be read as an option (WS-8).

use std::ffi::OsString;
use std::fmt;
use std::path::Path;

use crate::Error;

/// The name of an object: 40 lowercase hexadecimal digits, a SHA-1.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Oid(String);

impl Oid {
    /// `text` as an object name.
    ///
    /// # Errors
    ///
    /// [`Error::Oid`] where it is not 40 lowercase hexadecimal digits.
    pub fn parse(text: &str) -> Result<Self, Error> {
        if text.len() == 40
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Ok(Self(text.to_owned()))
        } else {
            Err(Error::Oid(text.to_owned()))
        }
    }

    /// The name as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The name of a branch, without `refs/heads/`, that `git check-ref-format
/// --branch` would accept and that does not start with `-`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Branch(String);

impl Branch {
    /// `name` as a branch name.
    ///
    /// # Errors
    ///
    /// [`Error::Branch`] where git would refuse it as a branch name, or it
    /// starts with `-`.
    pub fn new(name: &str) -> Result<Self, Error> {
        if valid_branch(name) {
            Ok(Self(name.to_owned()))
        } else {
            Err(Error::Branch(name.to_owned()))
        }
    }

    /// The name, without `refs/heads/`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The full name of the ref, `refs/heads/<name>`.
    pub fn full(&self) -> String {
        format!("refs/heads/{}", self.0)
    }
}

/// The rules of `git check-ref-format` for `refs/heads/<name>`, and no
/// leading `-`.
fn valid_branch(name: &str) -> bool {
    !name.is_empty()
        && name != "@"
        && !name.starts_with('-')
        && !name.ends_with('/')
        && !name.ends_with('.')
        && !name.contains("..")
        && !name.contains("@{")
        && !name.bytes().any(|byte| {
            byte.is_ascii_control()
                || matches!(byte, b' ' | b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
        })
        && name.split('/').all(|component| {
            !component.is_empty() && !component.starts_with('.') && !component.ends_with(".lock")
        })
}

/// Where a mirror is fetched from: an `https://` URL or the absolute path of
/// a local repository, which stands in for GitHub in a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote(OsString);

impl Remote {
    /// An `https://` URL with no white space or control character in it.
    ///
    /// # Errors
    ///
    /// [`Error::Remote`] where `url` is not such a URL.
    pub fn https(url: &str) -> Result<Self, Error> {
        let rest = url.strip_prefix("https://").unwrap_or_default();
        if rest.is_empty()
            || url
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte == b' ')
        {
            Err(Error::Remote(url.to_owned()))
        } else {
            Ok(Self(url.into()))
        }
    }

    /// The local repository at `path`, which is absolute.
    ///
    /// # Errors
    ///
    /// [`Error::NotAbsolute`] where `path` is relative.
    pub fn path(path: &Path) -> Result<Self, Error> {
        absolute(path)?;
        Ok(Self(path.as_os_str().to_owned()))
    }

    pub(crate) fn as_os_str(&self) -> &std::ffi::OsStr {
        &self.0
    }
}

/// `path`, where it is absolute: every command runs in `/`.
pub(crate) fn absolute(path: &Path) -> Result<&Path, Error> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(Error::NotAbsolute(path.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::{Branch, Oid, Remote};

    #[test]
    fn an_object_name_is_forty_lowercase_hexadecimal_digits() {
        let forty = "0123456789abcdef0123456789abcdef01234567";
        for (text, valid) in [
            (forty, true),
            ("4b825dc642cb6eb9a060e54bf8d69288fbee4904", true),
            ("0123456789ABCDEF0123456789abcdef01234567", false),
            (&forty[..39], false),
            ("0123456789abcdef0123456789abcdef012345678", false),
            ("--upload-pack=/bin/sh-0123456789abcdef0123", false),
            ("0123456789abcdef0123456789abcdef0123456g", false),
            ("", false),
        ] {
            assert_eq!(Oid::parse(text).is_ok(), valid, "{text}");
        }
    }

    #[test]
    fn a_branch_name_is_one_that_git_takes_and_no_option() {
        for (name, valid) in [
            ("main", true),
            ("factory/issue-42", true),
            ("release-1.2", true),
            ("-main", false),
            ("--upload-pack=x", false),
            ("", false),
            ("@", false),
            ("a..b", false),
            ("a@{1}", false),
            ("a b", false),
            ("a\tb", false),
            ("a~1", false),
            ("a^", false),
            ("a:b", false),
            ("a?", false),
            ("a*", false),
            ("a[", false),
            ("a\\b", false),
            ("a/", false),
            ("a.", false),
            ("/a", false),
            ("a//b", false),
            (".a", false),
            ("a/.b", false),
            ("a.lock", false),
            ("a.lock/b", false),
        ] {
            assert_eq!(Branch::new(name).is_ok(), valid, "{name:?}");
        }
    }

    #[test]
    fn a_branch_has_its_full_name() {
        assert_eq!(
            Branch::new("factory/issue-42").unwrap().full(),
            "refs/heads/factory/issue-42"
        );
    }

    #[test]
    fn a_remote_is_an_https_url_or_an_absolute_path() {
        assert!(Remote::https("https://github.com/owner/repo.git").is_ok());
        for url in [
            "http://github.com/owner/repo.git",
            "https://",
            "https://github.com/a b",
            "https://github.com/a\nb",
            "ext::sh -c x",
            "-u",
        ] {
            assert!(Remote::https(url).is_err(), "{url:?}");
        }
        assert!(Remote::path("/srv/repo.git".as_ref()).is_ok());
        assert!(Remote::path("repo.git".as_ref()).is_err());
    }
}

//! A check tool at one exact version (SPEC WS-7a): a program that a
//! repository's setup commands or checks run and that is no part of its
//! toolchain. Its name and its version both become directory names on the
//! host, below the tools directory, so each is held to its form here, by one
//! rule for every ecosystem.

use std::fmt;

/// A check tool that a policy names, with its one exact version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CheckTool {
    name: String,
    version: String,
}

/// Why a name or a version is of no form of WS-7a. A policy with one is
/// invalid (16.3).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CheckToolError {
    /// The name is of another form.
    #[error(
        "the check tool `{0}` has no name of WS-7a's form: lowercase letters, digits and hyphens, beginning with a letter"
    )]
    Name(String),
    /// The version is of another form.
    #[error(
        "the check tool `{name}` has a version `{version}` of no form of WS-7a: letters, digits, dots, hyphens and `+`, beginning with a letter or a digit"
    )]
    Version {
        /// The tool's name.
        name: String,
        /// The version.
        version: String,
    },
}

impl CheckTool {
    /// A check tool, where `name` is made of lowercase letters, digits and
    /// hyphens and begins with a letter, and `version` is made of letters,
    /// digits, dots, hyphens and `+` and begins with a letter or a digit
    /// (WS-7a).
    ///
    /// # Errors
    ///
    /// The name, or else the version, where it is of another form.
    pub fn new(name: &str, version: &str) -> Result<Self, CheckToolError> {
        let name_ok = name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
            && name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        if !name_ok {
            return Err(CheckToolError::Name(name.to_owned()));
        }
        let version_ok = version
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
            && version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'));
        if !version_ok {
            return Err(CheckToolError::Version {
                name: name.to_owned(),
                version: version.to_owned(),
            });
        }
        Ok(Self {
            name: name.to_owned(),
            version: version.to_owned(),
        })
    }

    /// The name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The version.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
}

impl fmt::Display for CheckTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.name, self.version)
    }
}

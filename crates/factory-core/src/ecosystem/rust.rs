//! The `rust` implementation of [`Ecosystem`] (9.5). This file and its tests
//! are the one place in the `factory-*` crates that names a Cargo file or a
//! cargo command; the structural test holds the others to that (19.1).

use super::{Ecosystem, HostTriple, Part, PartKind, ToolchainFileError, ToolchainLookup};
use serde::Deserialize;
use serde::de::{self, Deserializer, IgnoredAny, Unexpected};
use toml::de::DeTable;

/// The ecosystem of a repository that Cargo builds, with rustup's layout of a
/// toolchain (WS-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rust;

/// The file that names the toolchain. The file `rust-toolchain`, which rustup
/// also reads, is not read (WS-7).
const TOOLCHAIN_FILE: &str = "rust-toolchain.toml";

impl Ecosystem for Rust {
    fn toolchain_file(&self) -> &'static str {
        TOOLCHAIN_FILE
    }

    /// Reads the table `[toolchain]` and its keys `channel`, `components` and
    /// `targets`; `profile` may stand there and is tested for nothing. The
    /// toolchain lies at `<channel>-<host triple>`, as rustup names its
    /// directory, and is present when it holds `bin/rustc`, `bin/cargo` and
    /// the line `rust-std-<host triple>` in `lib/rustlib/components`. A
    /// component is present when that file holds its name, or its name with
    /// `-preview` after it, each with `-<host triple>` at the end or without;
    /// a target when it holds `rust-std-<target>` (WS-7, P-39).
    ///
    /// ```
    /// use factory_core::ecosystem::{Ecosystem, HostTriple, Rust};
    ///
    /// let file = b"[toolchain]\nchannel = \"1.100.0\"\ncomponents = [\"clippy\"]\n";
    /// let lookup = Rust.toolchain(file, HostTriple::Aarch64UnknownLinuxGnu)?;
    /// assert_eq!(lookup.directory, "1.100.0-aarch64-unknown-linux-gnu");
    ///
    /// assert!(Rust.toolchain(b"[toolchain]\nchannel = \"stable\"\n", HostTriple::Aarch64UnknownLinuxGnu).is_err());
    /// # Ok::<(), factory_core::ecosystem::ToolchainFileError>(())
    /// ```
    fn toolchain(
        &self,
        file: &[u8],
        host: HostTriple,
    ) -> Result<ToolchainLookup, ToolchainFileError> {
        let refused = |problem: String| ToolchainFileError {
            file: TOOLCHAIN_FILE,
            problem,
        };
        let text = std::str::from_utf8(file).map_err(|error| {
            refused(format!(
                "it is not UTF-8: byte {} starts no character",
                error.valid_up_to()
            ))
        })?;
        let table = DeTable::parse(text)
            .map_err(|error| refused(format!("it is not TOML: {}", error.message())))?;
        let File { toolchain } = File::deserialize(toml::de::Deserializer::from(table))
            .map_err(|error| refused(error.message().to_owned()))?;
        let host = host.as_str();
        let std_of = |target: &str| format!("rust-std-{target}");

        let mut parts = vec![Part {
            kind: PartKind::Toolchain,
            name: std_of(host),
            lines: vec![std_of(host)],
        }];
        parts.extend(toolchain.components.into_iter().map(|Name(name)| Part {
            kind: PartKind::Component,
            lines: vec![
                name.clone(),
                format!("{name}-{host}"),
                format!("{name}-preview"),
                format!("{name}-preview-{host}"),
            ],
            name,
        }));
        parts.extend(toolchain.targets.into_iter().map(|Name(name)| Part {
            kind: PartKind::Target,
            lines: vec![std_of(&name)],
            name,
        }));

        let Name(channel) = toolchain.channel;
        Ok(ToolchainLookup {
            directory: format!("{channel}-{host}"),
            name: channel,
            bin: "bin".to_owned(),
            programs: vec!["bin/rustc".to_owned(), "bin/cargo".to_owned()],
            record: "lib/rustlib/components".to_owned(),
            parts,
        })
    }

    /// `CARGO_HOME` at `.cargo` below the home, where Cargo would put it by
    /// itself, and `CARGO_TARGET_DIR` at the target directory, which lies
    /// outside the workspace (9.3).
    fn build_output(&self, home: &str, target: &str) -> Vec<(&'static str, String)> {
        vec![
            ("CARGO_HOME", format!("{home}/.cargo")),
            ("CARGO_TARGET_DIR", target.to_owned()),
        ]
    }

    /// The index and the downloads of crates.io (SEC-11).
    fn registry_hosts(&self) -> &'static [&'static str] {
        &["index.crates.io", "static.crates.io"]
    }
}

/// The file: the table `[toolchain]` and nothing else.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    toolchain: Table,
}

/// `[toolchain]`. A `path`, or any key that is not one of these four, makes
/// the file one that the host cannot resolve (WS-7).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    #[serde(deserialize_with = "channel")]
    channel: Name,
    #[serde(default, deserialize_with = "components")]
    components: Vec<Name>,
    #[serde(default, deserialize_with = "targets")]
    targets: Vec<Name>,
    // It names what rustup would install, and the host installs nothing.
    #[serde(default, rename = "profile")]
    _profile: Option<IgnoredAny>,
}

/// A name that has been held to its form.
struct Name(String);

/// A full version: three numbers written in digits, with a dot between them.
fn is_full_version(channel: &str) -> bool {
    let numbers: Vec<&str> = channel.split('.').collect();
    numbers.len() == 3
        && numbers
            .iter()
            .all(|number| !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Lowercase letters, digits and hyphens, beginning with a letter.
fn is_component(name: &str) -> bool {
    starts_with_a_letter(name)
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// As a component, with a dot and an underscore as well.
fn is_target(name: &str) -> bool {
    starts_with_a_letter(name)
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.' | b'_')
        })
}

fn starts_with_a_letter(name: &str) -> bool {
    name.bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase())
}

fn of_form<E: de::Error>(value: String, form: fn(&str) -> bool, expected: &str) -> Result<Name, E> {
    if form(&value) {
        Ok(Name(value))
    } else {
        Err(E::invalid_value(Unexpected::Str(&value), &expected))
    }
}

fn channel<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Name, D::Error> {
    of_form(
        String::deserialize(deserializer)?,
        is_full_version,
        "a full version, three numbers with a dot between them, as `1.100.0`",
    )
}

fn components<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Name>, D::Error> {
    names(
        deserializer,
        is_component,
        "a component: lowercase letters, digits and hyphens, beginning with a letter",
    )
}

fn targets<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Name>, D::Error> {
    names(
        deserializer,
        is_target,
        "a target: lowercase letters, digits, hyphens, dots and underscores, beginning with a letter",
    )
}

/// A list of names, each of which is held to `form`.
fn names<'de, D: Deserializer<'de>>(
    deserializer: D,
    form: fn(&str) -> bool,
    expected: &str,
) -> Result<Vec<Name>, D::Error> {
    Vec::<String>::deserialize(deserializer)?
        .into_iter()
        .map(|name| of_form(name, form, expected))
        .collect()
}

//! The operator configuration of version 1 (SPEC 16.4, CFG-2, CFG-5, CFG-6).
//!
//! The types hold every key of 16.4, and [`OperatorConfig::parse`] reads a
//! file of them from its bytes. Nothing here opens a file: the crate that
//! reads it hands the bytes over (19.1). A file is invalid where it holds a key
//! that version 1 does not have, where its `schema_version` is not one this
//! build knows (CFG-2), and in each case of CFG-6; the error names the key.
//! [`json_schema`] generates the JSON Schema of the file for editor support
//! (CFG-2).

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema as schema};
use serde::de::{self, Deserializer, IgnoredAny, Unexpected, Visitor};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU64;
use std::ops::Range;
use std::path::{Path, PathBuf};
use toml::de::{DeTable, DeValue};

/// The one `schema_version` that this build knows (CFG-2).
pub const SCHEMA_VERSION: i64 = 1;

/// The operator configuration of version 1, every key of 16.4.
///
/// A key that the file may leave out holds what the table of 16.4 says for
/// it: `None`, an empty collection or the default of 9.4 and section 14.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "Operator configuration, version 1 (SPEC 16.4)")]
pub struct OperatorConfig {
    /// The version of the file; this build knows 1 alone (CFG-2).
    schema_version: SchemaVersion,
    /// The data directory (9.1).
    pub data_dir: AbsolutePath,
    /// The tools directory (WS-7a). Without it no check tool is present.
    pub tools_dir: Option<AbsolutePath>,
    /// The credential mode and the subscription token's file (13.5).
    #[serde(default)]
    pub credentials: Credentials,
    /// The GitHub App that this build authenticates as (5.7). Without it a
    /// command that needs GitHub refuses.
    #[serde(default, deserialize_with = "github")]
    #[schemars(with = "Option<GitHubTable>")]
    pub github: Option<GitHubApp>,
    /// The sandbox level (13.3).
    pub sandbox: Sandbox,
    /// The agent CLI (10.3).
    #[serde(default)]
    pub agent: Agent,
    /// The directories of each ecosystem (WS-7).
    #[serde(default)]
    pub ecosystem: Ecosystems,
    /// The names that join the egress list (SEC-11).
    #[serde(default)]
    pub egress: Egress,
    /// The session and capture limits (section 14, 9.4).
    #[serde(default)]
    pub limits: Limits,
    /// The price table, one row for each model id (BUD-3). A model without a
    /// row starts no session.
    #[serde(default)]
    pub prices: BTreeMap<String, PriceRow>,
    /// The repository allowlist, the table `[[repository]]`. Without an entry
    /// the factory works on no repository.
    #[serde(default, rename = "repository")]
    pub repositories: Vec<Repository>,
}

// The tables with a default derive `Serialize` only so that `schemars` can
// write that default into the JSON Schema; nothing writes the file.

/// `[credentials]` (13.5).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    /// The credential mode; `subscription` where the file leaves it out.
    #[serde(default)]
    pub mode: CredentialMode,
    /// The file that holds the subscription token and nothing else, with mode
    /// 0600. Without it no agent session starts.
    pub subscription_token_file: Option<AbsolutePath>,
}

/// `[credentials] mode`. Version 1 has one value: the API-key modes of 13.5
/// are not scheduled (22).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CredentialMode {
    /// The agent CLI's subscription login, the default of 13.5.
    #[default]
    Subscription,
}

/// `[github]` with both of its keys (5.7, 13.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubApp {
    /// The id of the GitHub App.
    pub app_id: u64,
    /// The file of its private key, with mode 0600.
    pub private_key_file: AbsolutePath,
}

/// `[github]` as the file writes it: both keys or neither (CFG-6).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("dependentRequired" = {
    "app_id": ["private_key_file"],
    "private_key_file": ["app_id"]
}))]
struct GitHubTable {
    /// The id of the GitHub App that this build authenticates as (5.7).
    app_id: Option<u64>,
    /// The file of the App's private key, with mode 0600 (13.5).
    private_key_file: Option<AbsolutePath>,
}

fn github<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<GitHubApp>, D::Error> {
    let table = GitHubTable::deserialize(deserializer)?;
    match (table.app_id, table.private_key_file) {
        (Some(app_id), Some(private_key_file)) => Ok(Some(GitHubApp {
            app_id,
            private_key_file,
        })),
        (None, None) => Ok(None),
        (Some(_), None) => Err(de::Error::missing_field("private_key_file")),
        (None, Some(_)) => Err(de::Error::missing_field("app_id")),
    }
}

/// `[sandbox]` (13.3).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Sandbox {
    /// The level; SEC-7 says which command accepts which.
    pub level: SandboxLevel,
}

/// `[sandbox] level`. `none` is no value of the file: a test sets it in code,
/// with the fake agent (SEC-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SandboxLevel {
    /// The bubblewrap backend.
    Process,
    /// The microVM backend (M2).
    Hardened,
}

/// `[agent]` (10.3).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    /// The executable of the agent CLI. Without it no agent session starts.
    pub cli: Option<AbsolutePath>,
}

/// `[ecosystem]`: one table for each ecosystem, of which version 1 knows
/// `rust` alone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ecosystems {
    /// `[ecosystem.rust]`.
    #[serde(default)]
    pub rust: RustEcosystem,
}

/// `[ecosystem.rust]` (WS-7).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RustEcosystem {
    /// The toolchains directory. Without it no toolchain is present.
    pub toolchains_dir: Option<AbsolutePath>,
}

/// `[egress]` (SEC-11).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Egress {
    /// Names that join the list of SEC-11 for every sandbox instance, each
    /// compared as SEC-10b says.
    #[serde(default)]
    pub extra_hosts: Vec<String>,
}

/// `[limits]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// `[limits.session]` (section 14).
    #[serde(default)]
    pub session: SessionLimits,
    /// `[limits.capture]` (9.4).
    #[serde(default)]
    pub capture: CaptureLimits,
}

/// `[limits.session]`: the session limits of section 14, whose numbers are
/// the defaults (D-22).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SessionLimits {
    /// Turns of a session.
    pub turns: Limit,
    /// Minutes of wall clock of a session.
    pub minutes: Limit,
    /// Dollars of an `implement` session.
    pub implement_usd: PositiveAmount,
}

impl Default for SessionLimits {
    fn default() -> Self {
        Self {
            turns: Limit::new(200),
            minutes: Limit::new(90),
            implement_usd: PositiveAmount(20.0),
        }
    }
}

/// `[limits.capture]`: the capture limits of 9.4, whose numbers are the
/// defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CaptureLimits {
    /// Bytes of one file.
    pub file_bytes: Limit,
    /// New or changed bytes of a candidate.
    pub candidate_bytes: Limit,
    /// Seconds of the sandboxed staging step.
    pub timeout_secs: Limit,
}

impl Default for CaptureLimits {
    fn default() -> Self {
        Self {
            file_bytes: Limit::new(5 * 1024 * 1024),
            candidate_bytes: Limit::new(50 * 1024 * 1024),
            timeout_secs: Limit::new(300),
        }
    }
}

/// `[prices."<model id>"]`: one row of the price table (BUD-3).
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PriceRow {
    /// Input tokens.
    pub input: Price,
    /// Output tokens.
    pub output: Price,
    /// Tokens read from the cache.
    pub cache_read: Price,
    /// Tokens written to the cache for five minutes.
    pub cache_write_5m: Price,
    /// Tokens written to the cache for one hour.
    pub cache_write_1h: Price,
}

/// One entry of `[[repository]]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Repository {
    /// The repository, as `owner/repo`.
    pub name: String,
    /// The required CI checks of the repository (VAL-8).
    #[serde(default)]
    pub required_checks: Vec<RequiredCheck>,
}

/// One required CI check (VAL-8).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequiredCheck {
    /// The check-run name, as the API returns it.
    pub name: String,
    /// The App that runs the check.
    pub app_id: u64,
}

/// A path that the file names. It is absolute: the file expands neither `~`
/// nor a variable (CFG-6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AbsolutePath(PathBuf);

impl AbsolutePath {
    /// The path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl<'de> Deserialize<'de> for AbsolutePath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let path = String::deserialize(deserializer)?;
        if path.starts_with('/') {
            Ok(Self(PathBuf::from(path)))
        } else {
            Err(de::Error::invalid_value(
                Unexpected::Str(&path),
                &"an absolute path, since the file expands neither `~` nor a variable",
            ))
        }
    }
}

impl JsonSchema for AbsolutePath {
    fn schema_name() -> Cow<'static, str> {
        "AbsolutePath".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schema!({
            "type": "string",
            "pattern": "^/",
            "description": "An absolute path; the file expands neither `~` nor a variable"
        })
    }
}

/// A limit: a whole number greater than zero (CFG-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Limit(NonZeroU64);

impl Limit {
    /// A limit of the build's defaults, which is never zero.
    const fn new(value: u64) -> Self {
        match NonZeroU64::new(value) {
            Some(value) => Self(value),
            None => panic!("a default limit is zero"),
        }
    }

    /// The number.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl<'de> Deserialize<'de> for Limit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct WholeNumber;

        impl Visitor<'_> for WholeNumber {
            type Value = Limit;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a whole number greater than zero")
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Limit, E> {
                u64::try_from(value)
                    .ok()
                    .and_then(NonZeroU64::new)
                    .map(Limit)
                    .ok_or_else(|| E::invalid_value(Unexpected::Signed(value), &self))
            }
        }

        deserializer.deserialize_i64(WholeNumber)
    }
}

impl JsonSchema for Limit {
    fn schema_name() -> Cow<'static, str> {
        "Limit".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schema!({ "type": "integer", "minimum": 1 })
    }
}

/// A number of dollars greater than zero, as `implement_usd` is (CFG-6).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PositiveAmount(f64);

impl PositiveAmount {
    /// The number.
    #[must_use]
    pub fn get(self) -> f64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for PositiveAmount {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_f64(Amount { zero: false })
            .map(Self)
    }
}

impl JsonSchema for PositiveAmount {
    fn schema_name() -> Cow<'static, str> {
        "PositiveAmount".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schema!({ "type": "number", "exclusiveMinimum": 0 })
    }
}

/// A price: a number that is not negative (CFG-6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price(f64);

impl Price {
    /// The number.
    #[must_use]
    pub fn get(self) -> f64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Price {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_f64(Amount { zero: true })
            .map(Self)
    }
}

impl JsonSchema for Price {
    fn schema_name() -> Cow<'static, str> {
        "Price".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schema!({ "type": "number", "minimum": 0 })
    }
}

/// Reads a finite number, written as an integer or a float, that is greater
/// than zero, or not negative where `zero` is allowed.
struct Amount {
    zero: bool,
}

impl Visitor<'_> for Amount {
    type Value = f64;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.zero {
            "a number that is not negative"
        } else {
            "a number greater than zero"
        })
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<f64, E> {
        if self.accepts(value) {
            Ok(value)
        } else {
            Err(E::invalid_value(Unexpected::Float(value), &self))
        }
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<f64, E> {
        // A price or a limit in dollars is far below 2^53, where an integer
        // would lose precision as a float.
        #[allow(clippy::cast_precision_loss)]
        let float = value as f64;
        if self.accepts(float) {
            Ok(float)
        } else {
            Err(E::invalid_value(Unexpected::Signed(value), &self))
        }
    }
}

impl Amount {
    fn accepts(&self, value: f64) -> bool {
        value.is_finite() && if self.zero { value >= 0.0 } else { value > 0.0 }
    }
}

/// `schema_version`, which [`OperatorConfig::parse`] checks before it reads
/// the rest of the file, so that a file of a later version is refused for
/// its version and not for a key it holds (CFG-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SchemaVersion;

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        IgnoredAny::deserialize(deserializer).map(|_| Self)
    }
}

impl JsonSchema for SchemaVersion {
    fn schema_name() -> Cow<'static, str> {
        "SchemaVersion".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schema!({ "type": "integer", "const": SCHEMA_VERSION })
    }
}

/// The JSON Schema of the operator configuration of version 1 (CFG-2).
#[must_use]
pub fn json_schema() -> Schema {
    schemars::schema_for!(OperatorConfig)
}

/// Where in the file an error lies, counted from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// The line.
    pub line: usize,
    /// The column, in characters.
    pub column: usize,
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

/// Why a file is not a valid operator configuration of version 1.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// The bytes are not UTF-8, as a TOML file is.
    #[error("the operator configuration is not UTF-8: byte {offset} starts no character")]
    NotUtf8 {
        /// The offset of the first byte that starts no character.
        offset: usize,
    },
    /// The text is not TOML.
    #[error("the operator configuration is not TOML{}: {message}", at_position(*.at))]
    Syntax {
        /// What the TOML parser reports.
        message: String,
        /// Where it lies.
        at: Option<Position>,
    },
    /// A key is wrong: unknown, missing, or with a value that it may not have.
    #[error("`{key}`{}: {problem}", at_position(*.at))]
    Key {
        /// The key, as a dotted path from the top of the file, with `[i]` for
        /// the entry of an array.
        key: String,
        /// What is wrong with it.
        problem: String,
        /// Where it lies, where the file holds it.
        at: Option<Position>,
    },
}

impl ConfigError {
    /// The key that the error names, where it names one.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        match self {
            Self::Key { key, .. } => Some(key),
            Self::NotUtf8 { .. } | Self::Syntax { .. } => None,
        }
    }
}

fn at_position(at: Option<Position>) -> String {
    at.map(|at| format!(" at {at}")).unwrap_or_default()
}

const MISSING: &str = "is missing, and the file may not leave it out";

impl OperatorConfig {
    /// Parses an operator configuration of version 1 from the bytes of its
    /// file (16.4). The file is invalid where it is not TOML, where its
    /// `schema_version` is missing or not [`SCHEMA_VERSION`], where it holds a
    /// key that version 1 does not have (CFG-2), and in each case of CFG-6.
    ///
    /// # Errors
    ///
    /// The first thing wrong with the file, which names the key where there
    /// is one.
    ///
    /// ```
    /// use factory_core::config::OperatorConfig;
    ///
    /// let file = b"schema_version = 1\ndata_dir = \"/srv/sf/data\"\n[sandbox]\nlevel = \"process\"\n";
    /// let config = OperatorConfig::parse(file)?;
    /// assert_eq!(config.limits.session.turns.get(), 200);
    ///
    /// let error = OperatorConfig::parse(b"schema_version = 2\n").unwrap_err();
    /// assert_eq!(error.key(), Some("schema_version"));
    /// # Ok::<(), factory_core::config::ConfigError>(())
    /// ```
    pub fn parse(bytes: &[u8]) -> Result<Self, ConfigError> {
        let source = std::str::from_utf8(bytes).map_err(|error| ConfigError::NotUtf8 {
            offset: error.valid_up_to(),
        })?;
        let root = DeTable::parse(source).map_err(|error| ConfigError::Syntax {
            message: error.message().to_owned(),
            at: error.span().map(|span| position(source, span.start)),
        })?;
        check_version(source, root.get_ref())?;
        Self::deserialize(toml::de::Deserializer::from(root.clone()))
            .map_err(|error| key_error(source, root.get_ref(), &error))
    }
}

fn check_version(source: &str, root: &DeTable<'_>) -> Result<(), ConfigError> {
    let key = |problem: String, at| ConfigError::Key {
        key: "schema_version".to_owned(),
        problem,
        at,
    };
    let Some(version) = root.get("schema_version") else {
        return Err(key(MISSING.to_owned(), None));
    };
    let span = version.span();
    let problem = match version.get_ref() {
        DeValue::Integer(integer) => {
            if i64::from_str_radix(integer.as_str(), integer.radix()) == Ok(SCHEMA_VERSION) {
                return Ok(());
            }
            format!(
                "is {}, a version that this build does not know; it knows {SCHEMA_VERSION}",
                source.get(span.clone()).unwrap_or_default()
            )
        }
        _ => format!("is not a whole number; this build knows version {SCHEMA_VERSION}"),
    };
    Err(key(problem, Some(position(source, span.start))))
}

/// Turns an error of the typed read into one that names its key, found by
/// the span of the error in the parsed file.
fn key_error(source: &str, root: &DeTable<'_>, error: &toml::de::Error) -> ConfigError {
    let message = error.message();
    // A key missing from the top of the file comes with an empty span.
    let span = error.span().filter(|span| !span.is_empty());
    let mut path = Vec::new();
    if let Some(span) = &span {
        locate_in_table(root, span, &mut path);
    }
    let problem = if let Some(field) = message
        .strip_prefix("missing field `")
        .and_then(|rest| rest.strip_suffix('`'))
    {
        path.push(Segment::Key(field.to_owned()));
        MISSING.to_owned()
    } else if message.starts_with("unknown field `") {
        format!("is no key of version {SCHEMA_VERSION} of the operator configuration")
    } else {
        message.to_owned()
    };
    ConfigError::Key {
        key: dotted(&path),
        problem,
        at: span.map(|span| position(source, span.start)),
    }
}

/// One step of a key's path: a key of a table or the entry of an array.
enum Segment {
    Key(String),
    Index(usize),
}

/// Pushes onto `path` the deepest key under `table` whose key or value holds
/// `span`, and says whether it found one.
fn locate_in_table(table: &DeTable<'_>, span: &Range<usize>, path: &mut Vec<Segment>) -> bool {
    for (key, value) in table {
        path.push(Segment::Key(key.get_ref().to_string()));
        if locate_in_value(value.get_ref(), span, path)
            || holds(&key.span(), span)
            || holds(&value.span(), span)
        {
            return true;
        }
        path.pop();
    }
    false
}

fn locate_in_value(value: &DeValue<'_>, span: &Range<usize>, path: &mut Vec<Segment>) -> bool {
    match value {
        DeValue::Table(table) => locate_in_table(table, span, path),
        DeValue::Array(array) => {
            for (index, item) in array.iter().enumerate() {
                path.push(Segment::Index(index));
                if locate_in_value(item.get_ref(), span, path) || holds(&item.span(), span) {
                    return true;
                }
                path.pop();
            }
            false
        }
        _ => false,
    }
}

fn holds(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// Writes a path as TOML writes a dotted key, a key quoted where it is not
/// bare, with `[i]` for the entry of an array.
fn dotted(path: &[Segment]) -> String {
    let mut out = String::new();
    for segment in path {
        match segment {
            Segment::Key(key) => {
                if !out.is_empty() {
                    out.push('.');
                }
                let bare = !key.is_empty()
                    && key
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
                if bare {
                    out.push_str(key);
                } else {
                    out.push('"');
                    for c in key.chars() {
                        if matches!(c, '"' | '\\') {
                            out.push('\\');
                        }
                        out.push(c);
                    }
                    out.push('"');
                }
            }
            Segment::Index(index) => out.push_str(&format!("[{index}]")),
        }
    }
    out
}

/// The line and column, from 1, of the byte at `offset`.
fn position(source: &str, offset: usize) -> Position {
    let before = source.get(..offset).unwrap_or(source);
    Position {
        line: before.matches('\n').count() + 1,
        column: before
            .rsplit('\n')
            .next()
            .map_or(0, |line| line.chars().count())
            + 1,
    }
}

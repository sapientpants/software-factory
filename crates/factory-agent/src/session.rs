//! What the host prepares for an agent session, before the sandbox starts
//! the CLI (CLI-6): the configuration directory of AG-4, the host-written
//! `--settings` and MCP files (AG-1, AG-8), the argument list of AG-1 and the
//! variables of AG-11 that an agent session alone is given, the subscription
//! token among them (13.5).

use std::ffi::OsString;
use std::fmt;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use factory_core::config::CredentialMode;

/// The configuration directory below the session's home: the CLI's
/// user-level configuration where no variable names another (AG-4).
pub const CONFIG_DIR: &str = ".claude";

/// The entries of the configuration directory that the CLI reads as
/// configuration, which the host creates and the sandbox shows read-only
/// (AG-4): each a file with its content, or a directory where there is none.
const CONFIGURATION: &[(&str, Option<&str>)] = &[
    ("settings.json", Some("{}\n")),
    ("CLAUDE.md", Some("")),
    ("skills", None),
    ("rules", None),
    ("commands", None),
    ("agents", None),
];

/// The name of the host's `--settings` file (AG-1, AG-8).
pub const SETTINGS_FILE: &str = "settings.json";

/// The name of the host's `--mcp-config` file (AG-1, AG-3a).
pub const MCP_FILE: &str = "mcp.json";

/// The least length of the token, in bytes *(tunable)* (SEC-13).
pub const MIN_TOKEN_BYTES: usize = 32;

/// Creates the configuration directory below `home`, with each entry that
/// the CLI reads as configuration, empty (AG-4). It gives the paths of those
/// entries relative to `home`, for the sandbox to show read-only; the rest of
/// the directory stays writable for transcripts.
///
/// # Errors
///
/// Where an entry cannot be created, or the directory exists already.
pub fn create_config_dir(home: &Path) -> std::io::Result<Vec<PathBuf>> {
    let dir = home.join(CONFIG_DIR);
    std::fs::create_dir(&dir)?;
    let mut read_only = Vec::with_capacity(CONFIGURATION.len());
    for (name, content) in CONFIGURATION {
        let path = dir.join(name);
        match content {
            Some(content) => std::fs::write(&path, content)?,
            None => std::fs::create_dir(&path)?,
        }
        read_only.push(Path::new(CONFIG_DIR).join(name));
    }
    Ok(read_only)
}

/// Writes the host's `--settings` file, which turns the CLI's own Bash
/// sandbox off (AG-8, SEC-8), and its MCP file, which names no server
/// (AG-3a), into `dir`, which lies outside every path that a worker can
/// write (AG-4). The shell-tool timeout stays as it is: it is a key of
/// policy, which the driver reads in M1a (AG-8, CLI-6).
///
/// # Errors
///
/// Where a file cannot be written.
pub fn write_host_files(dir: &Path) -> std::io::Result<()> {
    let settings = serde_json::json!({ "sandbox": { "enabled": false } });
    let mcp = serde_json::json!({ "mcpServers": {} });
    std::fs::write(dir.join(SETTINGS_FILE), format!("{settings}\n"))?;
    std::fs::write(dir.join(MCP_FILE), format!("{mcp}\n"))
}

/// The id of a session, a random UUID of version 4 (AG-1).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SessionId([u8; 16]);

impl SessionId {
    /// A new id, from the random bytes of the system.
    ///
    /// # Errors
    ///
    /// Where `/dev/urandom` cannot be read.
    pub fn random() -> std::io::Result<Self> {
        let mut bytes = [0; 16];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        Ok(Self::from_bytes(bytes))
    }

    /// The id of `bytes`, with the bits of the version and the variant set.
    #[must_use]
    pub const fn from_bytes(mut bytes: [u8; 16]) -> Self {
        // The high bits of byte 6 hold the version, 4, and those of byte 8
        // the variant, 0b10.
        bytes[6] = 0x40 + bytes[6] % 0x10;
        bytes[8] = 0x80 + bytes[8] % 0x40;
        Self(bytes)
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (at, byte) in self.0.iter().enumerate() {
            if matches!(at, 4 | 6 | 8 | 10) {
                f.write_str("-")?;
            }
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SessionId({self})")
    }
}

/// One invocation of the CLI (AG-1). Flag names are those of Claude Code
/// 2.1.311, and `--forward-subagent-text` was observed on 2.1.323 (P-26).
#[derive(Debug, Clone)]
pub struct Invocation<'a> {
    /// The credential mode, which says whether the CLI runs bare (AG-3a).
    pub mode: CredentialMode,
    /// The session's id.
    pub session_id: SessionId,
    /// The model.
    pub model: &'a str,
    /// The permission mode.
    pub permission_mode: &'a str,
    /// The tool allowlist (AG-22).
    pub tools: &'a [&'a str],
    /// The host's `--settings` file, as the worker sees it.
    pub settings: &'a Path,
    /// The host's MCP file, as the worker sees it.
    pub mcp_config: &'a Path,
    /// The result schema, inline, where the session has one.
    pub json_schema: Option<&'a str>,
    /// The CLI's own cap of turns (AG-14).
    pub max_turns: u32,
    /// The CLI's own cap of dollars (AG-14).
    pub max_budget_usd: f64,
    /// The system prompt file, as the worker sees it, where the session has
    /// one (10.4).
    pub system_prompt_file: Option<&'a Path>,
}

impl Invocation<'_> {
    /// The arguments of the CLI, in the order of AG-1.
    #[must_use]
    pub fn args(&self) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec!["-p".into()];
        match self.mode {
            // Bare mode does not read the subscription token (AG-3a).
            CredentialMode::Subscription => {}
        }
        let tools = self.tools.join(",");
        for arg in [
            "--setting-sources",
            "user",
            "--output-format",
            "stream-json",
            "--verbose",
            "--input-format",
            "stream-json",
            "--session-id",
        ] {
            args.push(arg.into());
        }
        args.push(self.session_id.to_string().into());
        for (flag, value) in [
            ("--model", self.model),
            ("--permission-mode", self.permission_mode),
            ("--permission-prompts", "none"),
            ("--tools", &tools),
            ("--allowedTools", &tools),
        ] {
            args.extend([flag.into(), value.into()]);
        }
        args.extend([
            "--settings".into(),
            self.settings.into(),
            "--mcp-config".into(),
            self.mcp_config.into(),
            "--strict-mcp-config".into(),
        ]);
        if let Some(schema) = self.json_schema {
            args.extend(["--json-schema".into(), schema.into()]);
        }
        args.extend([
            "--max-turns".into(),
            self.max_turns.to_string().into(),
            "--max-budget-usd".into(),
            self.max_budget_usd.to_string().into(),
        ]);
        if let Some(file) = self.system_prompt_file {
            args.extend(["--append-system-prompt-file".into(), file.into()]);
        }
        args.extend([
            "--include-partial-messages".into(),
            "--forward-subagent-text".into(),
        ]);
        args
    }
}

/// The subscription token (13.5). Its debug form does not show it.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    /// The content of `file` without the white space at its end, read once
    /// (SEC-13).
    ///
    /// # Errors
    ///
    /// Where the file cannot be read, is no UTF-8, or its value is shorter
    /// than [`MIN_TOKEN_BYTES`]. No error holds the value.
    pub fn read(file: &Path) -> Result<Self, TokenError> {
        let bytes = std::fs::read(file).map_err(|source| TokenError::Read {
            file: file.to_owned(),
            source,
        })?;
        let text = String::from_utf8(bytes).map_err(|_| TokenError::NotUtf8 {
            file: file.to_owned(),
        })?;
        let value = text.trim_end();
        if value.len() < MIN_TOKEN_BYTES {
            return Err(TokenError::Short {
                file: file.to_owned(),
                bytes: value.len(),
            });
        }
        Ok(Self(value.to_owned()))
    }

    /// The value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Token({} bytes)", self.0.len())
    }
}

/// Why the token was not read. No variant holds the value.
#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    /// The file cannot be read.
    #[error("cannot read the subscription token file {}: {source}", .file.display())]
    Read {
        /// The file.
        file: PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// The file is no UTF-8.
    #[error("the subscription token file {} is no UTF-8", .file.display())]
    NotUtf8 {
        /// The file.
        file: PathBuf,
    },
    /// The value is shorter than [`MIN_TOKEN_BYTES`].
    #[error(
        "the subscription token in {} is {bytes} bytes, shorter than {MIN_TOKEN_BYTES} bytes (SEC-13)",
        .file.display()
    )]
    Short {
        /// The file.
        file: PathBuf,
        /// The length of the value.
        bytes: usize,
    },
}

/// The variables of AG-11 that an agent session alone is given: those of
/// SEC-11 and BUD-10, and the subscription token (13.5).
#[must_use]
pub fn environment(token: &Token) -> Vec<(OsString, OsString)> {
    vec![
        (
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(),
            "1".into(),
        ),
        ("CLAUDE_CODE_DISABLE_BACKGROUND_TASKS".into(), "1".into()),
        ("CLAUDE_CODE_OAUTH_TOKEN".into(), token.value().into()),
    ]
}

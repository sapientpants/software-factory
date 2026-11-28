//! What the host prepares for an agent session (SPEC CLI-6): the
//! configuration directory of AG-4, the host's files of AG-1 and AG-8, the
//! argument list of AG-1 and the environment of AG-11 with the token (13.5).

#![expect(
    clippy::unwrap_used,
    reason = "the helpers of a test file, which clippy does not see as test code (19.3)"
)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use factory_agent::session::{
    self, CONFIG_DIR, Invocation, MCP_FILE, SETTINGS_FILE, SessionId, Token, TokenError,
};
use factory_core::config::CredentialMode;

/// A fresh directory for one test, removed when the value is dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Self {
        let path = std::env::temp_dir().join(format!("sf-agent-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn json(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn the_config_directory_holds_each_entry_that_the_cli_reads_as_configuration() {
    let scratch = Scratch::new("config");

    let read_only = session::create_config_dir(&scratch.0).unwrap();

    let dir = scratch.0.join(CONFIG_DIR);
    assert_eq!(CONFIG_DIR, ".claude");
    assert_eq!(
        read_only,
        [
            "settings.json",
            "CLAUDE.md",
            "skills",
            "rules",
            "commands",
            "agents"
        ]
        .map(|name| Path::new(".claude").join(name))
    );
    assert_eq!(json(&dir.join("settings.json")), serde_json::json!({}));
    assert_eq!(fs::read(dir.join("CLAUDE.md")).unwrap(), b"");
    for name in ["skills", "rules", "commands", "agents"] {
        assert_eq!(fs::read_dir(dir.join(name)).unwrap().count(), 0, "{name}");
    }
    // A second directory is no fresh one.
    assert!(session::create_config_dir(&scratch.0).is_err());
}

#[test]
fn the_host_files_turn_the_clis_sandbox_off_and_name_no_mcp_server() {
    let scratch = Scratch::new("host");

    session::write_host_files(&scratch.0).unwrap();

    assert_eq!(
        json(&scratch.0.join(SETTINGS_FILE)),
        serde_json::json!({ "sandbox": { "enabled": false } })
    );
    assert_eq!(
        json(&scratch.0.join(MCP_FILE)),
        serde_json::json!({ "mcpServers": {} })
    );
}

#[test]
fn the_invocation_is_that_of_ag_1_in_subscription_mode() {
    let id = SessionId::from_bytes([0xab; 16]);
    let invocation = Invocation {
        mode: CredentialMode::Subscription,
        session_id: id,
        model: "claude-opus-5-5",
        permission_mode: "dontAsk",
        tools: &["Bash"],
        settings: Path::new("/agent/settings.json"),
        mcp_config: Path::new("/agent/mcp.json"),
        json_schema: None,
        max_turns: 10,
        max_budget_usd: 1.0,
        system_prompt_file: None,
    };

    let args: Vec<String> = invocation
        .args()
        .into_iter()
        .map(|arg| arg.into_string().unwrap())
        .collect();

    assert_eq!(
        args.join(" "),
        "-p --setting-sources user --output-format stream-json --verbose --input-format stream-json \
         --session-id abababab-abab-4bab-abab-abababababab --model claude-opus-5-5 \
         --permission-mode dontAsk --permission-prompts none --tools Bash --allowedTools Bash \
         --settings /agent/settings.json --mcp-config /agent/mcp.json --strict-mcp-config \
         --max-turns 10 --max-budget-usd 1 --include-partial-messages --forward-subagent-text"
    );
    assert!(!args.iter().any(|arg| arg == "--bare"), "AG-3a");
}

#[test]
fn a_schema_and_a_system_prompt_file_join_the_invocation_where_given() {
    let invocation = Invocation {
        mode: CredentialMode::Subscription,
        session_id: SessionId::from_bytes([0; 16]),
        model: "m",
        permission_mode: "dontAsk",
        tools: &["Read", "Bash"],
        settings: Path::new("/s"),
        mcp_config: Path::new("/m"),
        json_schema: Some(r#"{"type":"object"}"#),
        max_turns: 200,
        max_budget_usd: 20.5,
        system_prompt_file: Some(Path::new("/p.md")),
    };

    let args = invocation.args();

    let after = |flag: &str| {
        let at = args.iter().position(|arg| arg == flag).unwrap();
        args[at + 1].clone()
    };
    assert_eq!(after("--tools"), "Read,Bash");
    assert_eq!(after("--allowedTools"), "Read,Bash");
    assert_eq!(after("--json-schema"), r#"{"type":"object"}"#);
    assert_eq!(after("--max-turns"), "200");
    assert_eq!(after("--max-budget-usd"), "20.5");
    assert_eq!(after("--append-system-prompt-file"), "/p.md");
}

#[test]
fn a_session_id_is_a_random_uuid_of_version_4() {
    let id = SessionId::from_bytes([0; 16]).to_string();
    assert_eq!(id, "00000000-0000-4000-8000-000000000000");
    assert_eq!(
        format!("{:?}", SessionId::from_bytes([0xff; 16])),
        "SessionId(ffffffff-ffff-4fff-bfff-ffffffffffff)"
    );

    let (a, b) = (SessionId::random().unwrap(), SessionId::random().unwrap());
    assert_ne!(a, b);
    let text = a.to_string();
    assert_eq!(text.len(), 36);
    assert_eq!(&text[14..15], "4");
    assert!(matches!(&text[19..20], "8" | "9" | "a" | "b"), "{text}");
}

#[test]
fn the_token_is_the_file_without_the_white_space_at_its_end() {
    let scratch = Scratch::new("token");
    let file = scratch.0.join("token");
    let value = "sk-ant-oat01-".to_owned() + &"x".repeat(40);
    fs::write(&file, format!("{value}\r\n \n")).unwrap();

    let token = Token::read(&file).unwrap();

    assert_eq!(token.value(), value);
    // Its debug form gives its length alone.
    assert_eq!(format!("{token:?}"), "Token(53 bytes)");
    let env = session::environment(&token);
    assert_eq!(
        env,
        [
            ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
            ("CLAUDE_CODE_DISABLE_BACKGROUND_TASKS", "1"),
            ("CLAUDE_CODE_OAUTH_TOKEN", value.as_str()),
        ]
        .map(|(key, value)| (OsString::from(key), OsString::from(value)))
    );
}

#[test]
fn a_short_or_unreadable_token_is_refused_without_its_value() {
    let scratch = Scratch::new("short");
    let file = scratch.0.join("token");
    fs::write(&file, format!("{}\n", "y".repeat(31))).unwrap();

    let short = Token::read(&file).unwrap_err();
    let absent = Token::read(&scratch.0.join("absent")).unwrap_err();

    assert!(
        matches!(short, TokenError::Short { bytes: 31, .. }),
        "{short:?}"
    );
    fs::write(&file, "z".repeat(32)).unwrap();
    assert_eq!(Token::read(&file).unwrap().value(), "z".repeat(32));
    assert!(!short.to_string().contains("yyy"), "{short}");
    assert!(
        short.to_string().contains("shorter than 32 bytes"),
        "{short}"
    );
    assert!(matches!(absent, TokenError::Read { .. }), "{absent:?}");
    fs::write(&file, b"\xff".repeat(40)).unwrap();
    assert!(matches!(
        Token::read(&file),
        Err(TokenError::NotUtf8 { .. })
    ));
}

//! The operator configuration of version 1 (SPEC 16.4, CFG-2, CFG-6): the
//! example of 16.4, the defaults of its table, and each case that makes a file
//! invalid, with the key that the error names.

use factory_core::config::{
    ConfigError, CredentialMode, OperatorConfig, SandboxLevel, json_schema,
};
use std::path::Path;

/// The example of 16.4, read from the specification itself so that the two
/// cannot drift apart.
fn spec_example() -> Option<&'static str> {
    let spec = include_str!("../../../SPEC.md");
    let (_, section) = spec.split_once("\n### 16.4 ")?;
    let (_, block) = section.split_once("```toml\n")?;
    let (toml, _) = block.split_once("```")?;
    Some(toml)
}

#[test]
fn the_example_of_16_4_parses_with_its_values() {
    let config = OperatorConfig::parse(
        spec_example()
            .expect("16.4 holds the example as a TOML block")
            .as_bytes(),
    )
    .expect("the example is valid");

    assert_eq!(config.data_dir.as_path(), Path::new("/srv/sf/data"));
    assert_eq!(
        config.tools_dir.as_ref().map(|p| p.as_path()),
        Some(Path::new("/srv/sf/tools"))
    );
    assert_eq!(config.credentials.mode, CredentialMode::Subscription);
    assert_eq!(
        config
            .credentials
            .subscription_token_file
            .as_ref()
            .map(|p| p.as_path()),
        Some(Path::new("/etc/sf/subscription-token"))
    );
    let github = config.github.as_ref().expect("the example names an App");
    assert_eq!(github.app_id, 123_456);
    assert_eq!(
        github.private_key_file.as_path(),
        Path::new("/etc/sf/app-key.pem")
    );
    assert_eq!(config.sandbox.level, SandboxLevel::Process);
    assert_eq!(
        config.agent.cli.as_ref().map(|p| p.as_path()),
        Some(Path::new("/opt/claude-code/claude"))
    );
    assert_eq!(
        config
            .ecosystem
            .rust
            .toolchains_dir
            .as_ref()
            .map(|p| p.as_path()),
        Some(Path::new("/srv/sf/rustup/toolchains"))
    );
    assert!(config.egress.extra_hosts.is_empty());

    assert_eq!(config.limits.session.turns.get(), 200);
    assert_eq!(config.limits.session.minutes.get(), 90);
    assert_eq!(config.limits.session.implement_usd.get(), 20.0);
    assert_eq!(config.limits.capture.file_bytes.get(), 5_242_880);
    assert_eq!(config.limits.capture.candidate_bytes.get(), 52_428_800);
    assert_eq!(config.limits.capture.timeout_secs.get(), 300);

    assert_eq!(config.prices.len(), 1);
    let row = &config.prices["claude-opus-5-5"];
    assert_eq!(row.input.get(), 4.0);
    assert_eq!(row.output.get(), 20.0);
    assert_eq!(row.cache_read.get(), 0.2);
    assert_eq!(row.cache_write_5m.get(), 5.0);
    assert_eq!(row.cache_write_1h.get(), 8.0);

    assert_eq!(config.repositories.len(), 1);
    assert_eq!(config.repositories[0].name, "sapientpants/sf-scratch");
    assert!(config.repositories[0].required_checks.is_empty());
}

/// The keys that the file may not leave out, and nothing else (16.4).
const MINIMAL: &str = "schema_version = 1\ndata_dir = \"/d\"\n[sandbox]\nlevel = \"hardened\"\n";

#[test]
fn a_key_left_out_gets_what_the_table_of_16_4_says() {
    let config = OperatorConfig::parse(MINIMAL.as_bytes()).expect("the minimal file is valid");

    assert_eq!(config.data_dir.as_path(), Path::new("/d"));
    assert_eq!(config.sandbox.level, SandboxLevel::Hardened);
    assert_eq!(config.tools_dir, None, "no check tool is present");
    assert_eq!(config.credentials.mode, CredentialMode::Subscription);
    assert_eq!(config.credentials.subscription_token_file, None);
    assert_eq!(config.github, None);
    assert_eq!(config.agent.cli, None);
    assert_eq!(config.ecosystem.rust.toolchains_dir, None);
    assert!(config.egress.extra_hosts.is_empty());
    // The defaults of section 14 (D-22) and of 9.4.
    assert_eq!(config.limits.session.turns.get(), 200);
    assert_eq!(config.limits.session.minutes.get(), 90);
    assert_eq!(config.limits.session.implement_usd.get(), 20.0);
    assert_eq!(config.limits.capture.file_bytes.get(), 5_242_880);
    assert_eq!(config.limits.capture.candidate_bytes.get(), 52_428_800);
    assert_eq!(config.limits.capture.timeout_secs.get(), 300);
    assert!(config.prices.is_empty());
    assert!(config.repositories.is_empty());
}

#[test]
fn a_table_that_leaves_out_some_keys_keeps_the_defaults_of_the_others() {
    let file = format!(
        "{MINIMAL}[credentials]\n[limits.session]\nturns = 7\n[limits.capture]\ntimeout_secs = 9\n\
         [[repository]]\nname = \"o/r\"\n[[repository]]\nname = \"o/s\"\n\
         required_checks = [{{ name = \"check\", app_id = 15368 }}]\n"
    );
    let config = OperatorConfig::parse(file.as_bytes()).expect("valid");

    assert_eq!(config.credentials.mode, CredentialMode::Subscription);
    assert_eq!(config.limits.session.turns.get(), 7);
    assert_eq!(config.limits.session.minutes.get(), 90);
    assert_eq!(config.limits.session.implement_usd.get(), 20.0);
    assert_eq!(config.limits.capture.file_bytes.get(), 5_242_880);
    assert_eq!(config.limits.capture.timeout_secs.get(), 9);
    assert_eq!(config.repositories.len(), 2);
    assert!(config.repositories[0].required_checks.is_empty());
    assert_eq!(config.repositories[1].name, "o/s");
    assert_eq!(config.repositories[1].required_checks.len(), 1);
    assert_eq!(config.repositories[1].required_checks[0].name, "check");
    assert_eq!(config.repositories[1].required_checks[0].app_id, 15368);
}

#[test]
fn a_version_of_1_may_be_written_in_another_base() {
    let file = MINIMAL.replace("= 1", "= 0x1");
    assert!(OperatorConfig::parse(file.as_bytes()).is_ok());
}

#[test]
fn an_empty_github_table_holds_neither_key() {
    let file = format!("{MINIMAL}[github]\n");
    let config = OperatorConfig::parse(file.as_bytes()).expect("valid");
    assert_eq!(config.github, None);
}

#[test]
fn an_amount_may_be_written_as_an_integer_and_a_price_may_be_zero() {
    let file = format!(
        "{MINIMAL}[limits.session]\nimplement_usd = 3\n[prices.m]\ninput = 0\noutput = 0.0\n\
         cache_read = 1\ncache_write_5m = 0.25\ncache_write_1h = 0\n"
    );
    let config = OperatorConfig::parse(file.as_bytes()).expect("valid");
    assert_eq!(config.limits.session.implement_usd.get(), 3.0);
    let row = &config.prices["m"];
    assert_eq!(row.input.get(), 0.0);
    assert_eq!(row.output.get(), 0.0);
    assert_eq!(row.cache_read.get(), 1.0);
    assert_eq!(row.cache_write_5m.get(), 0.25);
}

/// A file that holds `MINIMAL` and then `rest`.
fn with(rest: &str) -> String {
    format!("{MINIMAL}{rest}")
}

/// A file that holds `MINIMAL` with `keys` at its top level.
fn top(keys: &str) -> String {
    MINIMAL.replace("[sandbox]", &format!("{keys}[sandbox]"))
}

/// Each case that makes a file invalid (CFG-2, CFG-6): the file, the key
/// that the error names and a part of what it says is wrong.
fn invalid_cases() -> Vec<(&'static str, String, &'static str, &'static str)> {
    const MISSING: &str = "is missing";
    const UNKNOWN: &str = "is no key of version 1";
    const RELATIVE: &str = "expected an absolute path";
    const LIMIT: &str = "expected a whole number greater than zero";
    const POSITIVE: &str = "expected a number greater than zero";
    const PRICE: &str = "expected a number that is not negative";
    let price_row = |input: &str| {
        with(&format!(
            "[prices.m]\ninput = {input}\noutput = 1\ncache_read = 1\ncache_write_5m = 1\ncache_write_1h = 1\n"
        ))
    };
    vec![
        // CFG-2: the version.
        (
            "no version",
            "data_dir = \"/d\"\n[sandbox]\nlevel = \"process\"\n".into(),
            "schema_version",
            MISSING,
        ),
        (
            "version 2",
            MINIMAL.replace("= 1", "= 2"),
            "schema_version",
            "is 2, a version that this build does not know",
        ),
        (
            "version 2 in hexadecimal",
            MINIMAL.replace("= 1", "= 0x2"),
            "schema_version",
            "is 0x2, a version",
        ),
        (
            "version 0",
            MINIMAL.replace("= 1", "= 0"),
            "schema_version",
            "is 0",
        ),
        (
            "version as a string",
            MINIMAL.replace("= 1", "= \"1\""),
            "schema_version",
            "is not a whole number",
        ),
        (
            "version 2 with a key of its own",
            "schema_version = 2\nlater = true\n".into(),
            "schema_version",
            "is 2",
        ),
        // CFG-2: unknown fields, at each depth.
        (
            "unknown top-level key",
            top("later = 1\n"),
            "later",
            UNKNOWN,
        ),
        (
            "unknown key of a table",
            with("[agent]\nflags = []\n"),
            "agent.flags",
            UNKNOWN,
        ),
        (
            "unknown key beside a known one",
            MINIMAL.replace("level", "mode = 1\nlevel"),
            "sandbox.mode",
            UNKNOWN,
        ),
        ("unknown table", with("[classes]\n"), "classes", UNKNOWN),
        (
            "unknown ecosystem",
            with("[ecosystem.python]\nvenv = \"/v\"\n"),
            "ecosystem.python",
            UNKNOWN,
        ),
        (
            "unknown key of a dotted table",
            with("[limits.session]\nhours = 1\n"),
            "limits.session.hours",
            UNKNOWN,
        ),
        (
            "unknown limits table",
            with("[limits.disk]\n"),
            "limits.disk",
            UNKNOWN,
        ),
        (
            "unknown key of a price row with a quoted id",
            with("[prices.\"m.1\"]\nper_call = 1\n"),
            "prices.\"m.1\".per_call",
            UNKNOWN,
        ),
        (
            "unknown key of a second repository",
            with(
                "[[repository]]\nname = \"o/r\"\n[[repository]]\nname = \"o/s\"\nmirror = \"x\"\n",
            ),
            "repository[1].mirror",
            UNKNOWN,
        ),
        (
            "unknown key of a required check",
            with(
                "[[repository]]\nname = \"o/r\"\nrequired_checks = [{ name = \"c\", app_id = 1, kind = \"x\" }]\n",
            ),
            "repository[0].required_checks[0].kind",
            UNKNOWN,
        ),
        (
            "unknown key of an inline table",
            top("egress = { extra_hosts = [], mirror = \"m\" }\n"),
            "egress.mirror",
            UNKNOWN,
        ),
        // CFG-6: the keys that may not be left out.
        (
            "no data_dir",
            MINIMAL.replace("data_dir = \"/d\"\n", ""),
            "data_dir",
            MISSING,
        ),
        (
            "no sandbox table",
            MINIMAL.replace("[sandbox]\nlevel = \"hardened\"\n", ""),
            "sandbox",
            MISSING,
        ),
        (
            "no sandbox level",
            MINIMAL.replace("level = \"hardened\"\n", ""),
            "sandbox.level",
            MISSING,
        ),
        (
            "no sandbox level, inline",
            MINIMAL.replace("[sandbox]\nlevel = \"hardened\"\n", "sandbox = {}\n"),
            "sandbox.level",
            MISSING,
        ),
        (
            "github without its key file",
            with("[github]\napp_id = 1\n"),
            "github.private_key_file",
            MISSING,
        ),
        (
            "github without its App id",
            with("[github]\nprivate_key_file = \"/k\"\n"),
            "github.app_id",
            MISSING,
        ),
        (
            "a price row without a price",
            with("[prices.m]\ninput = 1\noutput = 1\ncache_write_5m = 1\ncache_write_1h = 1\n"),
            "prices.m.cache_read",
            MISSING,
        ),
        (
            "a repository without a name",
            with("[[repository]]\nrequired_checks = []\n"),
            "repository[0].name",
            MISSING,
        ),
        (
            "a required check without its App",
            with("[[repository]]\nname = \"o/r\"\nrequired_checks = [{ name = \"c\" }]\n"),
            "repository[0].required_checks[0].app_id",
            MISSING,
        ),
        // CFG-6: every path is absolute, and nothing is expanded.
        (
            "relative data_dir",
            MINIMAL.replace("\"/d\"", "\"data\""),
            "data_dir",
            RELATIVE,
        ),
        (
            "data_dir under ~",
            MINIMAL.replace("\"/d\"", "\"~/sf\""),
            "data_dir",
            RELATIVE,
        ),
        (
            "data_dir in a variable",
            MINIMAL.replace("\"/d\"", "\"$HOME/sf\""),
            "data_dir",
            RELATIVE,
        ),
        (
            "empty data_dir",
            MINIMAL.replace("\"/d\"", "\"\""),
            "data_dir",
            RELATIVE,
        ),
        (
            "relative tools_dir",
            top("tools_dir = \"tools\"\n"),
            "tools_dir",
            RELATIVE,
        ),
        (
            "relative token file",
            with("[credentials]\nsubscription_token_file = \"./token\"\n"),
            "credentials.subscription_token_file",
            RELATIVE,
        ),
        (
            "relative key file",
            with("[github]\napp_id = 1\nprivate_key_file = \"key.pem\"\n"),
            "github.private_key_file",
            RELATIVE,
        ),
        (
            "relative agent CLI",
            with("[agent]\ncli = \"claude\"\n"),
            "agent.cli",
            RELATIVE,
        ),
        (
            "relative toolchains_dir",
            with("[ecosystem.rust]\ntoolchains_dir = \"~/.rustup/toolchains\"\n"),
            "ecosystem.rust.toolchains_dir",
            RELATIVE,
        ),
        // CFG-6: a limit is a whole number greater than zero.
        (
            "zero turns",
            with("[limits.session]\nturns = 0\n"),
            "limits.session.turns",
            LIMIT,
        ),
        (
            "negative minutes",
            with("[limits.session]\nminutes = -1\n"),
            "limits.session.minutes",
            LIMIT,
        ),
        (
            "fractional file_bytes",
            with("[limits.capture]\nfile_bytes = 1.5\n"),
            "limits.capture.file_bytes",
            LIMIT,
        ),
        (
            "zero candidate_bytes",
            with("[limits.capture]\ncandidate_bytes = 0\n"),
            "limits.capture.candidate_bytes",
            LIMIT,
        ),
        (
            "timeout as a string",
            with("[limits.capture]\ntimeout_secs = \"300\"\n"),
            "limits.capture.timeout_secs",
            LIMIT,
        ),
        // CFG-6: implement_usd is a number greater than zero.
        (
            "zero implement_usd",
            with("[limits.session]\nimplement_usd = 0\n"),
            "limits.session.implement_usd",
            POSITIVE,
        ),
        (
            "zero implement_usd as a float",
            with("[limits.session]\nimplement_usd = 0.0\n"),
            "limits.session.implement_usd",
            POSITIVE,
        ),
        (
            "negative implement_usd",
            with("[limits.session]\nimplement_usd = -20.0\n"),
            "limits.session.implement_usd",
            POSITIVE,
        ),
        (
            "implement_usd not a number",
            with("[limits.session]\nimplement_usd = nan\n"),
            "limits.session.implement_usd",
            POSITIVE,
        ),
        (
            "infinite implement_usd",
            with("[limits.session]\nimplement_usd = inf\n"),
            "limits.session.implement_usd",
            POSITIVE,
        ),
        // CFG-6: a price is a number that is not negative.
        ("negative price", price_row("-0.5"), "prices.m.input", PRICE),
        (
            "negative integer price",
            price_row("-1"),
            "prices.m.input",
            PRICE,
        ),
        (
            "price not a number",
            price_row("nan"),
            "prices.m.input",
            PRICE,
        ),
        ("infinite price", price_row("inf"), "prices.m.input", PRICE),
        // The values of version 1.
        (
            "sandbox level none",
            MINIMAL.replace("hardened", "none"),
            "sandbox.level",
            "unknown variant `none`",
        ),
        (
            "credential mode gateway",
            with("[credentials]\nmode = \"gateway\"\n"),
            "credentials.mode",
            "unknown variant `gateway`",
        ),
        (
            "extra hosts as a string",
            with("[egress]\nextra_hosts = \"a.example\"\n"),
            "egress.extra_hosts",
            "invalid type",
        ),
    ]
}

#[test]
fn each_invalid_case_names_its_key() {
    let mut wrong = Vec::new();
    for (case, file, key, problem) in invalid_cases() {
        match OperatorConfig::parse(file.as_bytes()) {
            Ok(_) => wrong.push(format!("{case}: parsed")),
            Err(error) => {
                let text = error.to_string();
                if error.key() != Some(key)
                    || !text.contains(problem)
                    || !text.contains(&format!("`{key}`"))
                {
                    wrong.push(format!("{case}: {error:?}"));
                }
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "cases that went wrong:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn an_error_says_where_the_key_lies() {
    let cases: &[(&str, String, &str)] = &[
        (
            "a value",
            MINIMAL.replace("hardened", "none"),
            "`sandbox.level` at line 4, column 9: unknown variant `none`, expected `process` or `hardened`",
        ),
        (
            "a key missing from a table",
            MINIMAL.replace("level = \"hardened\"\n", ""),
            "`sandbox.level` at line 3, column 1: is missing, and the file may not leave it out",
        ),
        (
            "a key missing from the top of the file",
            MINIMAL.replace("data_dir = \"/d\"\n", ""),
            "`data_dir`: is missing, and the file may not leave it out",
        ),
        (
            "an unknown key",
            with("[agent]\nflags = []\n"),
            "`agent.flags` at line 6, column 1: is no key of version 1 of the operator configuration",
        ),
        (
            "a version this build does not know",
            MINIMAL.replace("= 1", "= 2"),
            "`schema_version` at line 1, column 18: is 2, a version that this build does not know; it knows 1",
        ),
        (
            "a version that is no number",
            MINIMAL.replace("= 1", "= true"),
            "`schema_version` at line 1, column 18: is not a whole number; this build knows version 1",
        ),
        (
            "no version",
            "data_dir = \"/d\"\n".to_owned(),
            "`schema_version`: is missing, and the file may not leave it out",
        ),
        (
            "a column counted in characters",
            top("prices = { \"\u{E9}\" = { bad = 1 } }\n"),
            "`prices.\"\u{E9}\".bad` at line 3, column 20: is no key of version 1 of the operator configuration",
        ),
    ];
    for (case, file, text) in cases {
        let error = OperatorConfig::parse(file.as_bytes()).expect_err(case);
        assert_eq!(error.to_string(), *text, "{case}");
    }
}

#[test]
fn a_key_that_is_not_bare_is_quoted() {
    let cases: &[(&str, &str)] = &[
        ("[prices.\"a\\\"b\"]\nx = 1\n", "prices.\"a\\\"b\".x"),
        ("[prices.\"a\\\\b\"]\nx = 1\n", "prices.\"a\\\\b\".x"),
        ("[prices.\"\"]\nx = 1\n", "prices.\"\".x"),
        ("[prices.\"a b\"]\nx = 1\n", "prices.\"a b\".x"),
        ("[prices.A-z_09]\nx = 1\n", "prices.A-z_09.x"),
    ];
    for (rest, key) in cases {
        let error = OperatorConfig::parse(with(rest).as_bytes()).expect_err(rest);
        assert_eq!(error.key(), Some(*key), "{rest}");
    }
}

#[test]
fn a_file_that_is_not_utf8_or_not_toml_names_no_key() {
    let not_utf8 = OperatorConfig::parse(b"schema_version = 1\n\xFF\n").expect_err("not UTF-8");
    assert_eq!(not_utf8, ConfigError::NotUtf8 { offset: 19 });
    assert_eq!(not_utf8.key(), None);
    assert_eq!(
        not_utf8.to_string(),
        "the operator configuration is not UTF-8: byte 19 starts no character"
    );

    let not_toml =
        OperatorConfig::parse(b"schema_version = 1\n\n[sandbox\n").expect_err("not TOML");
    assert_eq!(not_toml.key(), None);
    let ConfigError::Syntax { at, .. } = &not_toml else {
        panic!("not a syntax error: {not_toml:?}");
    };
    assert_eq!(at.map(|at| at.line), Some(3));
    assert!(
        not_toml
            .to_string()
            .starts_with("the operator configuration is not TOML at line 3, column "),
        "{not_toml}"
    );
}

/// The committed JSON Schema, for editor support (CFG-2).
const SCHEMA_FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/schema/operator-config-v1.json"
);

#[test]
fn the_committed_json_schema_is_the_generated_one() {
    let generated =
        serde_json::to_string_pretty(&json_schema()).expect("a schema serialises") + "\n";
    if std::env::var_os("SF_WRITE_SCHEMA").is_some() {
        std::fs::write(SCHEMA_FILE, &generated).expect("the schema is written");
    }
    let committed = std::fs::read_to_string(SCHEMA_FILE).unwrap_or_default();
    assert!(
        committed == generated,
        "{SCHEMA_FILE} is not the schema of the types; run the test with SF_WRITE_SCHEMA=1 to write it"
    );
}

#[test]
fn the_json_schema_refuses_an_unknown_key_and_requires_what_16_4_requires() {
    let schema = serde_json::to_value(json_schema()).expect("a schema serialises");
    assert_eq!(schema["additionalProperties"], false);
    let mut required: Vec<&str> = schema["required"]
        .as_array()
        .expect("a list of required keys")
        .iter()
        .filter_map(|key| key.as_str())
        .collect();
    required.sort_unstable();
    assert_eq!(required, ["data_dir", "sandbox", "schema_version"]);
    assert_eq!(
        schema["properties"]["schema_version"]["$ref"],
        "#/$defs/SchemaVersion"
    );
    assert_eq!(schema["$defs"]["SchemaVersion"]["const"], 1);
}

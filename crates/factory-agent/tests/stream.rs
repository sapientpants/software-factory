//! The part of the stream protocol that `sf sandbox exec` reads in the agent
//! form (SPEC CLI-6): the one message it writes, the uses of the Bash tool
//! and the `result` line.

use factory_agent::stream::{Stream, is_result, user_message};

/// An `assistant` line of one content block, as the CLI writes them.
fn tool_use(id: &str, name: &str) -> String {
    format!(
        r#"{{"type":"assistant","message":{{"id":"msg_1","role":"assistant","content":[{{"type":"tool_use","id":"{id}","name":"{name}","input":{{"command":"echo hi"}}}}]}},"parent_tool_use_id":null,"session_id":"s"}}"#
    )
}

#[test]
fn the_message_is_one_user_line_of_stream_json() {
    let message = user_message("run \"echo\"\nand report");

    assert!(message.ends_with('\n'));
    assert_eq!(message.matches('\n').count(), 1, "{message}");
    let value: serde_json::Value = serde_json::from_str(&message).unwrap();
    assert_eq!(value["type"], "user");
    assert_eq!(value["message"]["role"], "user");
    assert_eq!(value["message"]["content"][0]["type"], "text");
    assert_eq!(
        value["message"]["content"][0]["text"],
        "run \"echo\"\nand report"
    );
}

#[test]
fn uses_of_the_bash_tool_are_counted_once_each() {
    let mut stream = Stream::default();

    for line in [
        r#"{"type":"system","subtype":"init","tools":["Bash"]}"#.to_owned(),
        tool_use("toolu_1", "Bash"),
        // A partial message of the same call is no second use.
        r#"{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","id":"toolu_1","name":"Bash"}}}"#.to_owned(),
        tool_use("toolu_1", "Bash"),
        tool_use("toolu_2", "Read"),
        tool_use("toolu_3", "Bash"),
        r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1"}]}}"#.to_owned(),
        "not json".to_owned(),
        r#"[1,2]"#.to_owned(),
    ] {
        assert_eq!(stream.read(&line), None, "{line}");
    }

    assert_eq!(stream.bash_uses(), 2);
    assert_eq!(stream.result(), None);
}

#[test]
fn a_use_of_the_bash_tool_without_an_id_counts_each_time() {
    let mut stream = Stream::default();
    let unnamed =
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash"}]}}"#;

    for line in [unnamed, unnamed, &tool_use("toolu_1", "Bash")] {
        stream.read(line);
    }

    assert_eq!(stream.bash_uses(), 3);
}

#[test]
fn the_first_result_line_ends_the_reading() {
    let mut stream = Stream::default();
    stream.read(&tool_use("toolu_1", "Bash"));

    let first = stream.read(r#"{"type":"result","subtype":"error_max_turns","is_error":true}"#);

    assert_eq!(first, Some("error_max_turns"));
    // What follows the first result is not read.
    assert_eq!(stream.read(&tool_use("toolu_2", "Bash")), None);
    assert_eq!(
        stream.read(r#"{"type":"result","subtype":"success"}"#),
        None
    );
    assert_eq!(stream.bash_uses(), 1);
    assert_eq!(stream.result(), Some("error_max_turns"));
}

#[test]
fn a_result_line_without_a_subtype_has_an_empty_one() {
    let mut stream = Stream::default();

    assert_eq!(stream.read(r#"{"type":"result"}"#), Some(""));
    assert!(is_result(r#"{"type":"result","subtype":"success"}"#));
    assert!(!is_result(r#"{"type":"assistant"}"#));
    assert!(!is_result("result"));
}

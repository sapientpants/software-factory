//! What M0 builds of the stream-JSON protocol, for the agent form of
//! `sf sandbox exec` (CLI-6): the one message that a session is given, and a
//! reading of the lines as far as the first `result` line, which counts the
//! uses of the Bash tool. Event normalisation, the limits and the structured
//! result are the driver's, and arrive with it in M1a (AG-5, 22).

use std::collections::BTreeSet;

use serde_json::Value;

/// The name of the Bash tool, in a `tool_use` block of an `assistant` line.
pub const BASH: &str = "Bash";

/// `prompt` as the one user message of a session on stream-JSON input: one
/// line, with its line break.
#[must_use]
pub fn user_message(prompt: &str) -> String {
    let message = serde_json::json!({
        "type": "user",
        "message": {
            "role": "user",
            "content": [{ "type": "text", "text": prompt }],
        },
    });
    format!("{message}\n")
}

/// Whether `line` is a `result` line.
#[must_use]
pub fn is_result(line: &str) -> bool {
    parsed(line).is_some_and(|value| kind(&value) == Some("result"))
}

/// The uses of the Bash tool and the subtype of the first `result` line, read
/// from the lines of a stream in order. A line that is no JSON object is no
/// event and is passed over.
#[derive(Debug, Default, Clone)]
pub struct Stream {
    /// The ids of the Bash tool's `tool_use` blocks: the CLI writes the
    /// blocks of one model call on lines of their own, and a block may stand
    /// on more than one.
    bash: BTreeSet<String>,
    /// The Bash tool's `tool_use` blocks that carry no id.
    unnamed: usize,
    /// The subtype of the first `result` line.
    result: Option<String>,
}

impl Stream {
    /// Reads `line`, and gives the subtype of the `result` line where it is
    /// the first. Once a `result` line was read, no line is read any more.
    /// A `result` line without a subtype has an empty one.
    pub fn read(&mut self, line: &str) -> Option<&str> {
        if self.result.is_some() {
            return None;
        }
        let value = parsed(line)?;
        match kind(&value)? {
            "assistant" => {
                let blocks = value
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten();
                for block in blocks {
                    if kind(block) == Some("tool_use")
                        && block.get("name").and_then(Value::as_str) == Some(BASH)
                    {
                        match block.get("id").and_then(Value::as_str) {
                            Some(id) => {
                                self.bash.insert(id.to_owned());
                            }
                            None => self.unnamed += 1,
                        }
                    }
                }
                None
            }
            "result" => {
                let subtype = value.get("subtype").and_then(Value::as_str).unwrap_or("");
                self.result = Some(subtype.to_owned());
                self.result.as_deref()
            }
            _ => None,
        }
    }

    /// How many uses of the Bash tool the lines read held.
    #[must_use]
    pub fn bash_uses(&self) -> usize {
        self.bash.len() + self.unnamed
    }

    /// The subtype of the first `result` line, where one was read.
    #[must_use]
    pub fn result(&self) -> Option<&str> {
        self.result.as_deref()
    }
}

/// `line` as a JSON object.
fn parsed(line: &str) -> Option<Value> {
    serde_json::from_str::<Value>(line)
        .ok()
        .filter(Value::is_object)
}

/// The `type` of an event or a block.
fn kind(value: &Value) -> Option<&str> {
    value.get("type").and_then(Value::as_str)
}

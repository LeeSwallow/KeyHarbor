//! CDP message framing on the pipe: one JSON object followed by `\0` per message.

use serde_json::{Value, json};

/// A command's outcome: its `result`, or the error message.
pub type Reply = std::result::Result<Value, String>;

/// A message from the browser.
pub enum Incoming {
    /// The answer to the command with this id.
    Reply { id: u64, reply: Reply },
    /// Anything without an id.
    Event(Value),
}

/// `{"id", "method", "params"[, "sessionId"]}` and the terminating NUL.
pub fn encode(id: u64, method: &str, params: Value, session: Option<&str>) -> Vec<u8> {
    let mut message = json!({"id": id, "method": method, "params": params});
    if let Some(session) = session {
        message["sessionId"] = json!(session);
    }
    let mut bytes = serde_json::to_vec(&message).expect("JSON value serializes");
    bytes.push(0);
    bytes
}

/// Parses one frame read up to (and including) its NUL; `None` if it is not JSON.
pub fn decode(frame: &mut Vec<u8>) -> Option<Incoming> {
    if frame.last() == Some(&0) {
        frame.pop();
    }
    let message = serde_json::from_slice::<Value>(frame).ok()?;
    Some(match message.get("id").and_then(Value::as_u64) {
        Some(id) => Incoming::Reply {
            id,
            reply: match message.get("error") {
                Some(error) => Err(error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("CDP error")
                    .to_owned()),
                None => Ok(message.get("result").cloned().unwrap_or(json!({}))),
            },
        },
        None => Incoming::Event(message),
    })
}

//! The control-socket protocol, the one place it is written down (check-layers K): one JSON
//! request per line, one JSON response per line.
//!
//! Request `{"op":"...", "agent":"...", ...}` → `{"ok":true,"result":...}` or
//! `{"ok":false,"error":"..."}`. The server (`ops`) and every client (MCP, `ProcessRuntime`)
//! build and read these types only; defaults such as `login`'s `submit` live here once.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One request: who asks (the local user when absent) and the operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlEnvelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(flatten)]
    pub request: ControlRequest,
}

impl ControlEnvelope {
    /// A request from the local user (`cli`).
    pub fn local(request: ControlRequest) -> Self {
        Self {
            agent: None,
            request,
        }
    }

    /// A request on behalf of `agent`.
    pub fn from_agent(agent: &str, request: ControlRequest) -> Self {
        Self {
            agent: Some(agent.to_owned()),
            request,
        }
    }
}

/// The operations a supervisor serves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ControlRequest {
    Navigate {
        url: String,
    },
    Snapshot,
    Screenshot,
    Click {
        target: TargetWire,
    },
    Fill {
        target: TargetWire,
        /// May hold placeholders; the guard resolves them.
        text: String,
    },
    Type {
        text: String,
    },
    Press {
        key: String,
    },
    Tabs,
    NewTab {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    SwitchTab {
        id: String,
    },
    /// `selector` wins over `ms` when both are given.
    Wait {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ms: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selector: Option<String>,
    },
    /// Fill a login item into the page's sign-in form (R-39).
    Login {
        item: String,
        /// Absent means [`LOGIN_SUBMITS_BY_DEFAULT`]; read it with [`login_submits`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        submit: Option<bool>,
    },
    Close,
    Status,
}

/// `login` presses Enter in the password input unless asked not to.
pub const LOGIN_SUBMITS_BY_DEFAULT: bool = true;

/// Whether a `login` request submits the form.
pub fn login_submits(submit: Option<bool>) -> bool {
    submit.unwrap_or(LOGIN_SUBMITS_BY_DEFAULT)
}

/// The element a request acts on: `{"ref": N}` (from a snapshot) or `{"selector": "css"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetWire {
    Ref(u64),
    Selector(String),
}

/// One response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(into = "ResponseWire", from = "ResponseWire")]
pub enum ControlResponse {
    Ok(Value),
    Err(String),
}

#[derive(Serialize, Deserialize)]
struct ResponseWire {
    ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl From<ControlResponse> for ResponseWire {
    fn from(response: ControlResponse) -> Self {
        match response {
            ControlResponse::Ok(result) => Self {
                ok: true,
                result: Some(result),
                error: None,
            },
            ControlResponse::Err(error) => Self {
                ok: false,
                result: None,
                error: Some(error),
            },
        }
    }
}

impl From<ResponseWire> for ControlResponse {
    fn from(wire: ResponseWire) -> Self {
        if wire.ok {
            Self::Ok(wire.result.unwrap_or(Value::Null))
        } else {
            Self::Err(wire.error.unwrap_or_else(|| "supervisor error".to_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wire(request: ControlRequest) -> Value {
        serde_json::to_value(ControlEnvelope::local(request)).unwrap()
    }

    #[test]
    fn requests_keep_the_protocol_shape() {
        assert_eq!(wire(ControlRequest::Snapshot), json!({"op": "snapshot"}));
        assert_eq!(
            serde_json::to_value(ControlEnvelope::from_agent(
                "claude",
                ControlRequest::Click {
                    target: TargetWire::Ref(7)
                }
            ))
            .unwrap(),
            json!({"op": "click", "agent": "claude", "target": {"ref": 7}})
        );
        assert_eq!(
            wire(ControlRequest::Fill {
                target: TargetWire::Selector("#user".into()),
                text: "x".into()
            }),
            json!({"op": "fill", "target": {"selector": "#user"}, "text": "x"})
        );
        assert_eq!(
            wire(ControlRequest::NewTab { url: None }),
            json!({"op": "new_tab"})
        );
    }

    #[test]
    fn requests_read_what_clients_send() {
        let read = |v: Value| serde_json::from_value::<ControlEnvelope>(v).unwrap();
        assert_eq!(
            read(json!({"op": "new_tab", "url": null})).request,
            ControlRequest::NewTab { url: None }
        );
        assert_eq!(
            read(json!({"op": "wait", "ms": 10})).request,
            ControlRequest::Wait {
                ms: Some(10),
                selector: None
            }
        );
        let login = read(json!({"op": "login", "item": "gh"})).request;
        let ControlRequest::Login { submit, .. } = login else {
            panic!("not a login")
        };
        assert!(login_submits(submit));
        assert!(!login_submits(Some(false)));
        for bad in [
            json!({"op": "fly"}),
            json!({}),
            json!({"op": "click"}),
            json!({"op": "login", "item": "gh", "submit": "yes"}),
        ] {
            assert!(
                serde_json::from_value::<ControlEnvelope>(bad.clone()).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn responses_are_ok_with_a_result_or_an_error() {
        let ok = serde_json::to_value(ControlResponse::Ok(json!({"a": 1}))).unwrap();
        assert_eq!(ok, json!({"ok": true, "result": {"a": 1}}));
        let err = serde_json::to_value(ControlResponse::Err("no".into())).unwrap();
        assert_eq!(err, json!({"ok": false, "error": "no"}));
        assert_eq!(
            serde_json::from_value::<ControlResponse>(json!({"ok": true})).unwrap(),
            ControlResponse::Ok(Value::Null)
        );
    }
}

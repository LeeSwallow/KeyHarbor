//! The control-socket protocol, the one place it is written down (check-layers K): one JSON
//! request per line, one JSON response per line.
//!
//! Request `{"op":"...", "agent":"...", ...}` → `{"ok":true,"result":...}` or
//! `{"ok":false,"error":"..."}`. The server (`ops`) and every client (MCP, `ProcessRuntime`)
//! build and read these types only; defaults such as `login`'s `submit` live here once.

use keyharbor_domain::{
    handoff_site::SiteCookie,
    ports::browser::{FillOutcome, Target},
};
use keyharbor_guard::LoginOutcome;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use zeroize::Zeroizing;

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
    /// A site handed over from the user's browser by the native host (R-42). Values travel
    /// only in this request; the response is a count.
    ReceiveSite {
        origin: String,
        cookies: Vec<CookieWire>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        local_storage: Option<BTreeMap<String, String>>,
        ttl_seconds: u64,
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

impl From<TargetWire> for Target {
    fn from(target: TargetWire) -> Self {
        match target {
            TargetWire::Ref(reference) => Self::Ref(reference),
            TargetWire::Selector(selector) => Self::Selector(selector),
        }
    }
}

/// The `result` of `fill` and `type`: `{}`, or a warning when the frame's origin changed
/// while the text went in.
#[derive(Serialize)]
pub struct FillResultWire {
    #[serde(skip_serializing_if = "Option::is_none")]
    warning: Option<&'static str>,
}

impl From<FillOutcome> for FillResultWire {
    fn from(outcome: FillOutcome) -> Self {
        Self {
            warning: outcome
                .origin_changed
                .then_some("origin_changed_after_fill"),
        }
    }
}

/// The `result` of `login`: the fields filled, whether the form was submitted, the URL after.
#[derive(Serialize)]
pub struct LoginResultWire {
    filled: Vec<&'static str>,
    submitted: bool,
    url: String,
}

impl From<LoginOutcome> for LoginResultWire {
    fn from(outcome: LoginOutcome) -> Self {
        Self {
            filled: outcome.filled,
            submitted: outcome.submitted,
            url: outcome.url,
        }
    }
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
    fn guard_outcomes_keep_their_result_shape() {
        let fill = |changed| {
            serde_json::to_value(FillResultWire::from(FillOutcome {
                origin_changed: changed,
            }))
            .unwrap()
        };
        assert_eq!(fill(false), json!({}));
        assert_eq!(fill(true), json!({"warning": "origin_changed_after_fill"}));
        let login = LoginResultWire::from(LoginOutcome {
            filled: vec!["username", "password"],
            submitted: true,
            url: "https://github.com/".into(),
        });
        assert_eq!(
            serde_json::to_value(login).unwrap(),
            json!({"filled": ["username", "password"], "submitted": true, "url": "https://github.com/"})
        );
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

/// A cookie as the extension and CDP shape it (`Network.CookieParam`). `Debug` leaves the
/// value out.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieWire {
    pub name: String,
    pub value: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub http_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub same_site: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<i64>,
}

impl std::fmt::Debug for CookieWire {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CookieWire")
            .field("name", &self.name)
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl From<CookieWire> for SiteCookie {
    fn from(wire: CookieWire) -> Self {
        Self {
            name: wire.name,
            value: Zeroizing::new(wire.value),
            url: wire.url,
            domain: wire.domain,
            path: wire.path,
            secure: wire.secure,
            http_only: wire.http_only,
            same_site: wire.same_site,
            expires: wire.expires,
        }
    }
}

impl From<&SiteCookie> for CookieWire {
    fn from(cookie: &SiteCookie) -> Self {
        Self {
            name: cookie.name.clone(),
            value: cookie.value.to_string(),
            url: cookie.url.clone(),
            domain: cookie.domain.clone(),
            path: cookie.path.clone(),
            secure: cookie.secure,
            http_only: cookie.http_only,
            same_site: cookie.same_site.clone(),
            expires: cookie.expires,
        }
    }
}

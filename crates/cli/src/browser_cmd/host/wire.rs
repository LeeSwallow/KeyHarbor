//! The Native Messaging protocol v1, the one place it is written down (#198): what the
//! extension sends ([`HostRequestWire`], and [`ExtensionMessageWire`] while the bridge is open)
//! and what this host answers ([`HostReplyWire`], and [`filled`] for the one reply that carries
//! a value). `protocol.ts` in integrations/browser-extension is the other side. The handlers
//! read and build these types only; no other host file names a `"type"`.
//!
//! The bytes are the ones the host wrote before these types existed: a reply was a
//! `serde_json::Value`, whose keys come out in alphabetical order, so [`encode`] still goes
//! through a `Value` and [`FilledWire`] lists its fields in that order (J-250).

use keyharbor_supervisor::{
    bridge::SiteAnswer,
    control::wire::{CookieWire, LocalStorageWire},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::Write;
use zeroize::Zeroizing;

/// The protocol version this host speaks.
pub(super) const VERSION: u64 = 1;

/// Why a request was not done; the extension shows the code only (`error {code}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Refusal {
    BadRequest,
    NoSession,
    NotRunning,
    Refused,
    NotConfirmed,
    NotBrowser,
    NoVault,
    Failed,
}

/// One request: the version and, by `type`, what is asked.
#[derive(Deserialize)]
struct RequestEnvelope {
    v: u64,
    #[serde(flatten)]
    request: HostRequestWire,
}

/// What the extension asks, by `type`. No `Debug`: `receive_site` and `save_login` hold values.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum HostRequestWire {
    ReceiveSite(ReceiveSiteWire),
    AutofillList {
        origin: String,
    },
    AutofillFill {
        origin: String,
        item: String,
    },
    SaveLogin(SaveLoginWire),
    /// Opens the lasting connection the admin app's asks come through.
    Bridge,
}

/// `receive_site` (`ReceiveSiteWire` in protocol.ts): a site handed over to a session.
#[derive(Deserialize)]
pub(super) struct ReceiveSiteWire {
    pub origin: String,
    pub cookies: Vec<CookieWire>,
    #[serde(default)]
    pub local_storage: Option<LocalStorageWire>,
    pub ttl_seconds: u64,
    pub session: String,
}

/// `save_login`: what the page held when the person pressed save. No `Debug`: it holds values.
#[derive(Deserialize)]
pub(super) struct SaveLoginWire {
    pub origin: String,
    #[serde(default, deserialize_with = "maybe_secret")]
    pub username: Option<Zeroizing<String>>,
    #[serde(deserialize_with = "secret")]
    pub password: Zeroizing<String>,
}

/// The string read moves into `Zeroizing` without a copy, so it is wiped when dropped.
fn secret<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Zeroizing<String>, D::Error> {
    String::deserialize(d).map(Zeroizing::new)
}
fn maybe_secret<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Zeroizing<String>>, D::Error> {
    Ok(Option::<String>::deserialize(d)?.map(Zeroizing::new))
}

/// Reads one request; anything that is not protocol v1 is `bad_request`.
pub(super) fn parse_request(body: &[u8]) -> Result<HostRequestWire, Refusal> {
    let envelope: RequestEnvelope =
        serde_json::from_slice(body).map_err(|_| Refusal::BadRequest)?;
    if envelope.v != VERSION {
        return Err(Refusal::BadRequest);
    }
    Ok(envelope.request)
}

/// The host's replies that carry no value, by `type`. `filled` is not here: it never becomes
/// a `Value` ([`filled`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum HostReplyWire {
    /// `receive_site` done: the session and how many cookies it took.
    Received {
        session: String,
        cookies: u64,
    },
    /// The logins that may fill an origin, by name.
    Logins {
        items: Vec<String>,
    },
    /// `save_login` found the login unchanged.
    Known {
        item: String,
    },
    /// `save_login` stored a draft for the person to confirm.
    Saved {
        item: String,
    },
    Error {
        code: Refusal,
    },
    /// The bridge is listening.
    BridgeReady,
    /// The app asks, through the bridge, to hand `origin` to `session`.
    RequestSite {
        id: u64,
        session: String,
        origin: String,
    },
}

#[derive(Serialize)]
struct ReplyEnvelope<'a> {
    v: u64,
    #[serde(flatten)]
    reply: &'a HostReplyWire,
}

/// `reply` as the bytes the extension reads, `v` included. Through a `Value` so the keys keep
/// the alphabetical order they always had; no reply here holds a value.
pub(super) fn encode(reply: &HostReplyWire) -> serde_json::Result<String> {
    let envelope = ReplyEnvelope { v: VERSION, reply };
    Ok(serde_json::to_value(envelope)?.to_string())
}

/// The `filled` reply, borrowing the login so the only copy written is the wiped buffer
/// (#157). Fields in key order, the order a `Value` writes them in.
#[derive(Serialize)]
struct FilledWire<'a> {
    password: &'a str,
    r#type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<&'a str>,
    v: u64,
}

/// The `filled` reply in a wiped buffer, `v` included (J-160).
pub(super) fn filled(
    username: Option<&str>,
    password: &str,
) -> Result<Zeroizing<Vec<u8>>, Refusal> {
    seal(&FilledWire {
        password,
        r#type: "filled",
        username,
        v: VERSION,
    })
}

/// Serializes `reply` straight into a wiped buffer. It is measured first so the buffer never
/// grows: a reallocation would leave the old bytes behind unwiped.
fn seal(reply: &impl Serialize) -> Result<Zeroizing<Vec<u8>>, Refusal> {
    let mut counter = ByteCount(0);
    serde_json::to_writer(&mut counter, reply).map_err(|_| Refusal::Failed)?;
    let mut body = Zeroizing::new(Vec::with_capacity(counter.0));
    serde_json::to_writer(&mut *body, reply).map_err(|_| Refusal::Failed)?;
    Ok(body)
}

/// A writer that only counts, for [`seal`].
struct ByteCount(usize);

impl Write for ByteCount {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A message from the extension while the bridge is open. Anything but `request_answer` is
/// ignored.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ExtensionMessageWire {
    /// How the ask `id` ended (`RequestAnswerWire` in protocol.ts): `outcome`, `cookies`.
    RequestAnswer {
        id: u64,
        #[serde(flatten)]
        rest: Map<String, Value>,
    },
    #[serde(other)]
    Other,
}

impl ExtensionMessageWire {
    /// Reads one message; `None` when it is not JSON (the bridge treats that as closed).
    pub(super) fn parse(body: &[u8]) -> Option<Self> {
        let value: Value = serde_json::from_slice(body).ok()?;
        Some(Self::deserialize(value).unwrap_or(Self::Other))
    }

    /// The answer to the ask `id`, if this is it. An answer whose outcome this host does not
    /// know is `failed`.
    pub(super) fn answer_to(&self, id: u64) -> Option<SiteAnswer> {
        let Self::RequestAnswer { id: answered, rest } = self else {
            return None;
        };
        if *answered != id {
            return None;
        }
        let mut answer = rest.clone();
        answer.retain(|k, _| k == "outcome" || k == "cookies");
        Some(serde_json::from_value(Value::Object(answer)).unwrap_or(SiteAnswer::Failed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A reply as the host built it before #198: a `Value` with `v` added by `serve`.
    fn as_before(mut reply: Value) -> String {
        reply["v"] = json!(VERSION);
        reply.to_string()
    }

    #[test]
    fn replies_keep_the_bytes_they_had_as_values() {
        let cases = [
            (
                HostReplyWire::Received {
                    session: "work".into(),
                    cookies: 3,
                },
                json!({"type": "received", "session": "work", "cookies": 3}),
            ),
            (
                HostReplyWire::Logins {
                    items: vec!["github".into(), "github-2".into()],
                },
                json!({"type": "logins", "items": ["github", "github-2"]}),
            ),
            (
                HostReplyWire::Logins { items: vec![] },
                json!({"type": "logins", "items": []}),
            ),
            (
                HostReplyWire::Known {
                    item: "github".into(),
                },
                json!({"type": "known", "item": "github"}),
            ),
            (
                HostReplyWire::Saved {
                    item: "github~draft-1".into(),
                },
                json!({"type": "saved", "item": "github~draft-1"}),
            ),
            (
                HostReplyWire::BridgeReady,
                json!({"v": VERSION, "type": "bridge_ready"}),
            ),
            (
                HostReplyWire::RequestSite {
                    id: 7,
                    session: "work".into(),
                    origin: "https://github.com".into(),
                },
                json!({"v": VERSION, "type": "request_site", "id": 7, "session": "work", "origin": "https://github.com"}),
            ),
        ];
        for (reply, before) in cases {
            assert_eq!(encode(&reply).unwrap(), as_before(before), "{reply:?}");
        }
    }

    #[test]
    fn every_refusal_keeps_its_code() {
        let codes = [
            (Refusal::BadRequest, "bad_request"),
            (Refusal::NoSession, "no_session"),
            (Refusal::NotRunning, "not_running"),
            (Refusal::Refused, "refused"),
            (Refusal::NotConfirmed, "not_confirmed"),
            (Refusal::NotBrowser, "not_browser"),
            (Refusal::NoVault, "no_vault"),
            (Refusal::Failed, "failed"),
        ];
        for (code, text) in codes {
            assert_eq!(
                encode(&HostReplyWire::Error { code }).unwrap(),
                as_before(json!({"type": "error", "code": text}))
            );
        }
    }

    #[test]
    fn filled_keeps_its_bytes_in_a_wiped_buffer() {
        for (username, password) in [(Some("me@example.com"), "p\"w\\d"), (None, "pw")] {
            let mut before = json!({"type": "filled", "password": password});
            if let Some(username) = username {
                before["username"] = json!(username);
            }
            let body: Zeroizing<Vec<u8>> = filled(username, password).unwrap();
            assert_eq!(body.as_slice(), as_before(before).as_bytes());
            // Measured before writing: the buffer never grew, so no unwiped copy was left.
            assert_eq!(body.capacity(), body.len());
        }
    }

    /// The requests as protocol.ts builds them.
    #[test]
    fn the_extensions_requests_are_read_by_type() {
        let parse = |message: Value| parse_request(message.to_string().as_bytes());
        let receive = json!({
            "v": 1, "type": "receive_site", "origin": "https://github.com", "session": "work",
            "ttl_seconds": 3600, "local_storage": {"k": "v"},
            "cookies": [{"name": "sid", "value": "x", "url": "https://github.com/", "path": "/",
                         "secure": true, "httpOnly": true, "sameSite": "Lax"}],
        });
        let Ok(HostRequestWire::ReceiveSite(site)) = parse(receive) else {
            panic!("receive_site")
        };
        assert_eq!(
            (
                site.origin.as_str(),
                site.session.as_str(),
                site.ttl_seconds
            ),
            ("https://github.com", "work", 3600)
        );
        assert_eq!(site.cookies.len(), 1);
        assert!(site.local_storage.is_some());

        assert!(matches!(
            parse(json!({"v": 1, "type": "autofill_list", "origin": "https://github.com"})),
            Ok(HostRequestWire::AutofillList { origin }) if origin == "https://github.com"
        ));
        assert!(matches!(
            parse(json!({"v": 1, "type": "autofill_fill", "origin": "https://github.com", "item": "github"})),
            Ok(HostRequestWire::AutofillFill { origin, item }) if origin == "https://github.com" && item == "github"
        ));
        let Ok(HostRequestWire::SaveLogin(save)) = parse(
            json!({"v": 1, "type": "save_login", "origin": "https://github.com", "username": "amy", "password": "pw"}),
        ) else {
            panic!("save_login")
        };
        assert_eq!(
            (
                save.username.as_deref().map(String::as_str),
                save.password.as_str()
            ),
            (Some("amy"), "pw")
        );
        let Ok(HostRequestWire::SaveLogin(save)) = parse(
            json!({"v": 1, "type": "save_login", "origin": "https://github.com", "password": "pw"}),
        ) else {
            panic!("save_login without a username")
        };
        assert!(save.username.is_none());
        assert!(matches!(
            parse(json!({"v": 1, "type": "bridge"})),
            Ok(HostRequestWire::Bridge)
        ));
    }

    #[test]
    fn anything_else_is_a_bad_request() {
        for body in [
            &b"not json"[..],
            br#"{"v":2,"type":"receive_site"}"#,
            br#"{"v":2,"type":"bridge"}"#,
            br#"{"type":"bridge"}"#,
            br#"{"v":1}"#,
            br#"{"v":1,"type":"show"}"#,
            br#"{"v":1,"type":"autofill_list"}"#,
            br#"{"v":1,"type":"save_login","origin":"https://a.example"}"#,
        ] {
            assert_eq!(
                parse_request(body).err(),
                Some(Refusal::BadRequest),
                "{}",
                String::from_utf8_lossy(body)
            );
        }
    }

    #[test]
    fn the_bridge_takes_only_the_answer_to_its_ask() {
        let parse = |message: Value| ExtensionMessageWire::parse(message.to_string().as_bytes());
        let handed = parse(
            json!({"v": 1, "type": "request_answer", "id": 2, "outcome": "handed_off", "cookies": 3}),
        )
        .unwrap();
        assert_eq!(
            handed.answer_to(2),
            Some(SiteAnswer::HandedOff { cookies: 3 })
        );
        assert_eq!(handed.answer_to(1), None, "an answer to another ask");
        let unknown =
            parse(json!({"v": 1, "type": "request_answer", "id": 2, "outcome": "maybe"})).unwrap();
        assert_eq!(unknown.answer_to(2), Some(SiteAnswer::Failed));
        for other in [
            json!({"v": 1, "type": "something_else", "id": 2}),
            json!({"v": 1}),
            json!({"v": 1, "type": "request_answer", "id": "2", "outcome": "declined"}),
        ] {
            assert_eq!(parse(other.clone()).unwrap().answer_to(2), None, "{other}");
        }
        assert!(ExtensionMessageWire::parse(b"not json").is_none());
    }
}

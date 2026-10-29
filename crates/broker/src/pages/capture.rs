//! Capturing sign-ins the user types (08 §8.2 ②): a script in an isolated world reports
//! trusted form submits with a password through a binding only that world can call.

use crate::cdp::Cdp;
use keyharbor_domain::ports::browser::CapturedLogin;
use serde::Deserialize;
use serde_json::{Value, json};
use zeroize::Zeroizing;

/// The isolated world the script runs in; the binding exists only there.
const WORLD: &str = "keyharbor-capture";
pub(super) const BINDING: &str = "__khCapture";
const SCRIPT: &str = include_str!("capture.js");

/// The binding's payload as the script sends it.
#[derive(Deserialize)]
struct CaptureWire {
    origin: String,
    username: Option<String>,
    password: String,
}

impl From<CaptureWire> for CapturedLogin {
    fn from(wire: CaptureWire) -> Self {
        Self {
            origin: wire.origin,
            username: wire.username.map(Zeroizing::new),
            password: Zeroizing::new(wire.password),
        }
    }
}

pub(super) fn parse(payload: &Value) -> Option<CapturedLogin> {
    let text = Zeroizing::new(payload.as_str()?.to_owned());
    serde_json::from_str::<CaptureWire>(&text)
        .ok()
        .map(Into::into)
}

/// Adds the binding and the script to one page session.
pub(super) async fn instrument(cdp: &Cdp, session: &str) -> anyhow::Result<()> {
    let session = Some(session);
    cdp.call(
        "Runtime.addBinding",
        json!({"name": BINDING, "executionContextName": WORLD}),
        session,
    )
    .await?;
    cdp.call(
        "Page.addScriptToEvaluateOnNewDocument",
        json!({"source": SCRIPT, "worldName": WORLD, "runImmediately": true}),
        session,
    )
    .await?;
    cdp.call("Runtime.enable", json!({}), session).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payloads_parse_into_logins_and_others_are_ignored() {
        let login = parse(&json!(
            r#"{"origin":"https://a.example","username":"me","password":"pw"}"#
        ))
        .unwrap();
        assert_eq!(login.origin, "https://a.example");
        assert_eq!(login.username.as_deref().map(String::as_str), Some("me"));
        assert_eq!(login.password.as_str(), "pw");
        let anonymous = parse(&json!(r#"{"origin":"x","username":null,"password":"p"}"#));
        assert!(anonymous.unwrap().username.is_none());
        for bad in [json!(3), json!("not json"), json!(r#"{"origin":"x"}"#)] {
            assert!(parse(&bad).is_none(), "{bad}");
        }
    }
}

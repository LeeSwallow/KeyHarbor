//! Capturing sign-ins the user types (08 §8.2 ②): every page gets a script in an isolated
//! world that reports trusted form submits with a password through a binding only that world
//! can call. Reports go to the supervisor, never to a control response.

use crate::{cdp::Cdp, chrome::CdpLink};
use keyharbor_domain::ports::browser::CapturedLogin;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::{broadcast, mpsc};
use zeroize::Zeroizing;

/// The isolated world the script runs in; the binding exists only there.
const WORLD: &str = "keyharbor-capture";
const BINDING: &str = "__khCapture";
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

impl CdpLink {
    /// Sends every captured sign-in to `captured` until the browser closes or the receiver
    /// is dropped. Pages are attached as they appear (and those already open).
    pub async fn capture_logins(&self, captured: mpsc::Sender<CapturedLogin>) {
        let cdp = self.cdp();
        let mut events = cdp.events();
        let attach = json!({"autoAttach": true, "waitForDebuggerOnStart": false, "flatten": true});
        if cdp
            .call("Target.setAutoAttach", attach, None)
            .await
            .is_err()
        {
            return;
        }
        let mut ours = HashSet::new();
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            };
            match event["method"].as_str() {
                Some("Target.attachedToTarget") => {
                    let params = &event["params"];
                    if params["targetInfo"]["type"] != "page" {
                        continue;
                    }
                    if let Some(session) = params["sessionId"].as_str() {
                        ours.insert(session.to_owned());
                        tokio::spawn(instrument(cdp.clone(), session.to_owned()));
                    }
                }
                Some("Runtime.bindingCalled") => {
                    let from_ours = event["sessionId"]
                        .as_str()
                        .is_some_and(|s| ours.contains(s));
                    if !from_ours || event["params"]["name"] != BINDING {
                        continue;
                    }
                    if let Some(login) = parse(&event["params"]["payload"])
                        && captured.send(login).await.is_err()
                    {
                        return;
                    }
                }
                _ => {}
            }
        }
    }
}

fn parse(payload: &Value) -> Option<CapturedLogin> {
    let text = Zeroizing::new(payload.as_str()?.to_owned());
    serde_json::from_str::<CaptureWire>(&text)
        .ok()
        .map(Into::into)
}

/// Adds the binding and the script to one page session (best effort: a page that closes
/// first is simply not watched).
async fn instrument(cdp: Arc<Cdp>, session: String) {
    let session = Some(session.as_str());
    let steps = [
        (
            "Runtime.addBinding",
            json!({"name": BINDING, "executionContextName": WORLD}),
        ),
        (
            "Page.addScriptToEvaluateOnNewDocument",
            json!({"source": SCRIPT, "worldName": WORLD, "runImmediately": true}),
        ),
        ("Runtime.enable", json!({})),
        ("Runtime.runIfWaitingForDebugger", json!({})),
    ];
    for (method, params) in steps {
        if cdp.call(method, params, session).await.is_err() {
            return;
        }
    }
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
        assert!(
            parse(&json!(r#"{"origin":"x","username":null,"password":"p"}"#))
                .unwrap()
                .username
                .is_none()
        );
        for bad in [json!(3), json!("not json"), json!(r#"{"origin":"x"}"#)] {
            assert!(parse(&bad).is_none(), "{bad}");
        }
    }
}

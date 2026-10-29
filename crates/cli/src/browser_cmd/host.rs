//! The Native Messaging host (protocol v1, integrations/browser-extension): one
//! length-prefixed JSON request on stdin, one reply on stdout. stdout carries only the
//! protocol, and neither the reply nor an error ever holds a cookie value.

use crate::compose::App;
use anyhow::Result;
use keyharbor_core::session as sessions;
use keyharbor_domain::{
    SessionName,
    handoff_site::{self, SiteCookie},
    vault::Origin,
};
use keyharbor_supervisor::control::{
    client,
    wire::{ControlEnvelope, ControlRequest, CookieWire},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
};
use zeroize::Zeroizing;

/// The protocol version this host speaks.
const VERSION: u64 = 1;
/// Larger than any real site's cookies and storage; Chrome allows up to 64 MiB.
const MAX_MESSAGE: u32 = 4 * 1024 * 1024;

/// `receive_site` as the extension sends it (`ReceiveSiteWire` in protocol.ts).
#[derive(Deserialize)]
struct ReceiveSiteWire {
    v: u64,
    r#type: String,
    origin: String,
    cookies: Vec<CookieWire>,
    #[serde(default)]
    local_storage: Option<BTreeMap<String, String>>,
    ttl_seconds: u64,
    session: String,
}

/// Why a request was not done; the extension shows the code only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    BadRequest,
    NoSession,
    NotRunning,
    Refused,
    Failed,
}

impl Refusal {
    fn code(self) -> &'static str {
        match self {
            Self::BadRequest => "bad_request",
            Self::NoSession => "no_session",
            Self::NotRunning => "not_running",
            Self::Refused => "refused",
            Self::Failed => "failed",
        }
    }
}

fn read_message(input: &mut impl Read) -> Result<Zeroizing<Vec<u8>>, Refusal> {
    let mut length = [0u8; 4];
    input
        .read_exact(&mut length)
        .map_err(|_| Refusal::BadRequest)?;
    let length = u32::from_ne_bytes(length);
    if length == 0 || length > MAX_MESSAGE {
        return Err(Refusal::BadRequest);
    }
    let mut body = Zeroizing::new(vec![0u8; length as usize]);
    input
        .read_exact(&mut body)
        .map_err(|_| Refusal::BadRequest)?;
    Ok(body)
}

fn write_message(output: &mut impl Write, reply: &Value) -> Result<()> {
    let body = reply.to_string();
    output.write_all(&u32::try_from(body.len())?.to_ne_bytes())?;
    output.write_all(body.as_bytes())?;
    output.flush()?;
    Ok(())
}

/// Checks the request and hands it to the named session's supervisor; returns the session
/// name and how many cookies it took.
fn receive(app: &App, body: &[u8]) -> Result<(String, u64), Refusal> {
    let wire: ReceiveSiteWire = serde_json::from_slice(body).map_err(|_| Refusal::BadRequest)?;
    if wire.v != VERSION || wire.r#type != "receive_site" {
        return Err(Refusal::BadRequest);
    }
    let origin = Origin::parse(&wire.origin).map_err(|_| Refusal::BadRequest)?;
    let name = SessionName::parse(&wire.session).map_err(|_| Refusal::BadRequest)?;
    let cookies: Vec<SiteCookie> = wire.cookies.into_iter().map(Into::into).collect();
    handoff_site::check(&origin, &cookies, wire.ttl_seconds).map_err(|_| Refusal::Refused)?;

    let store = app.store().map_err(|_| Refusal::Failed)?;
    let runtime = app.runtime().map_err(|_| Refusal::Failed)?;
    let session = sessions::list(&*store, &*runtime)
        .map_err(|_| Refusal::Failed)?
        .into_iter()
        .find(|view| view.row.name == name)
        .ok_or(Refusal::NoSession)?;
    if !runtime.is_alive(session.row.id) {
        return Err(Refusal::NotRunning);
    }
    let request = ControlRequest::ReceiveSite {
        origin: origin.as_str().to_owned(),
        cookies: cookies.iter().map(CookieWire::from).collect(),
        local_storage: wire.local_storage,
        ttl_seconds: wire.ttl_seconds,
    };
    let result = client::request(
        app.root(),
        &session.row.id,
        &ControlEnvelope::local(request),
    )
    .map_err(|_| Refusal::Failed)?;
    Ok((
        name.as_str().to_owned(),
        result["cookies"].as_u64().unwrap_or(0),
    ))
}

/// Answers one request from the extension.
pub fn serve(app: &App) -> Result<()> {
    let outcome = read_message(&mut std::io::stdin().lock()).and_then(|body| receive(app, &body));
    let reply = match outcome {
        Ok((session, cookies)) => {
            json!({"v": VERSION, "type": "received", "session": session, "cookies": cookies})
        }
        Err(refusal) => json!({"v": VERSION, "type": "error", "code": refusal.code()}),
    };
    write_message(&mut std::io::stdout().lock(), &reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn framed(body: &[u8]) -> Vec<u8> {
        let mut out = (body.len() as u32).to_ne_bytes().to_vec();
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn messages_are_length_prefixed_and_bounded() {
        let body = read_message(&mut framed(b"{}").as_slice()).unwrap();
        assert_eq!(body.as_slice(), b"{}");
        assert_eq!(
            read_message(&mut [0u8, 0, 0].as_slice()).unwrap_err(),
            Refusal::BadRequest
        );
        assert_eq!(
            read_message(&mut framed(b"").as_slice()).unwrap_err(),
            Refusal::BadRequest
        );
        let huge = (MAX_MESSAGE + 1).to_ne_bytes();
        assert_eq!(
            read_message(&mut huge.as_slice()).unwrap_err(),
            Refusal::BadRequest
        );
        let mut short = framed(b"{}");
        short.pop();
        assert_eq!(
            read_message(&mut short.as_slice()).unwrap_err(),
            Refusal::BadRequest
        );
        let mut out = Vec::new();
        write_message(&mut out, &json!({"a": 1})).unwrap();
        assert_eq!(out, framed(br#"{"a":1}"#));
    }
}

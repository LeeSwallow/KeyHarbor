//! The Native Messaging host (protocol v1, integrations/browser-extension): one
//! length-prefixed JSON request on stdin, one reply on stdout. stdout carries only the
//! protocol. A reply carries a value in one case only, `filled` after Touch ID
//! ([`autofill`], ADR-0017); no reply or error ever holds a cookie value.

mod autofill;
mod caller;
mod receive;

use crate::compose::App;
use anyhow::Result;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{Read, Write};
use zeroize::Zeroizing;

/// The protocol version this host speaks.
const VERSION: u64 = 1;
/// Larger than any real site's cookies and storage; Chrome allows up to 64 MiB.
const MAX_MESSAGE: u32 = 4 * 1024 * 1024;

/// Why a request was not done; the extension shows the code only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    BadRequest,
    NoSession,
    NotRunning,
    Refused,
    NotConfirmed,
    NotBrowser,
    NoVault,
    Failed,
}

impl Refusal {
    fn code(self) -> &'static str {
        match self {
            Self::BadRequest => "bad_request",
            Self::NoSession => "no_session",
            Self::NotRunning => "not_running",
            Self::Refused => "refused",
            Self::NotConfirmed => "not_confirmed",
            Self::NotBrowser => "not_browser",
            Self::NoVault => "no_vault",
            Self::Failed => "failed",
        }
    }
}

/// The two fields every request has, read first to pick the handler.
#[derive(Deserialize)]
struct Kind {
    v: u64,
    r#type: String,
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
    let body = Zeroizing::new(reply.to_string());
    output.write_all(&u32::try_from(body.len())?.to_ne_bytes())?;
    output.write_all(body.as_bytes())?;
    output.flush()?;
    Ok(())
}

/// The reply to one request, by its `type`.
fn answer(app: &App, body: &[u8]) -> Result<Value, Refusal> {
    let kind: Kind = serde_json::from_slice(body).map_err(|_| Refusal::BadRequest)?;
    if kind.v != VERSION {
        return Err(Refusal::BadRequest);
    }
    match kind.r#type.as_str() {
        "receive_site" => {
            let (session, cookies) = receive::receive(app, body)?;
            Ok(json!({"type": "received", "session": session, "cookies": cookies}))
        }
        "autofill_list" => autofill::list(app, body),
        "autofill_fill" => autofill::fill(app, body),
        "save_login" => autofill::save(app, body),
        _ => Err(Refusal::BadRequest),
    }
}

/// Answers one request from the extension.
pub fn serve(app: &App) -> Result<()> {
    let mut reply =
        match read_message(&mut std::io::stdin().lock()).and_then(|body| answer(app, &body)) {
            Ok(reply) => reply,
            Err(refusal) => json!({"type": "error", "code": refusal.code()}),
        };
    reply["v"] = json!(VERSION);
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

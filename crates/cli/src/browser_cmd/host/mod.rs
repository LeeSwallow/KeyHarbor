//! The Native Messaging host (protocol v1, integrations/browser-extension): one
//! length-prefixed JSON request on stdin, one reply on stdout. stdout carries only the
//! protocol. A reply carries a value in one case only, `filled` after Touch ID
//! ([`autofill`], ADR-0017); no reply or error ever holds a cookie value.

mod autofill;
mod bridge;
mod caller;
mod receive;

use crate::compose::App;
use anyhow::Result;
use serde::{Deserialize, Serialize};
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
    write_frame(output, Zeroizing::new(reply.to_string()).as_bytes())
}

fn write_frame(output: &mut impl Write, body: &[u8]) -> Result<()> {
    output.write_all(&u32::try_from(body.len())?.to_ne_bytes())?;
    output.write_all(body)?;
    output.flush()?;
    Ok(())
}

/// A reply without values is a `Value` that `serve` stamps with `v`. The one that carries a
/// value (`filled`) is already written into a wiped buffer, `v` included: a `Value` would
/// hold an unwiped `String` copy of it (#157, J-160).
enum Reply {
    Plain(Value),
    Sealed(Zeroizing<Vec<u8>>),
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

/// The reply to one request, by its `type`.
fn answer(app: &App, body: &[u8]) -> Result<Reply, Refusal> {
    let kind: Kind = serde_json::from_slice(body).map_err(|_| Refusal::BadRequest)?;
    if kind.v != VERSION {
        return Err(Refusal::BadRequest);
    }
    match kind.r#type.as_str() {
        "receive_site" => {
            let (session, cookies) = receive::receive(app, body)?;
            Ok(Reply::Plain(
                json!({"type": "received", "session": session, "cookies": cookies}),
            ))
        }
        "autofill_list" => autofill::list(app, body).map(Reply::Plain),
        "autofill_fill" => autofill::fill(app, body).map(Reply::Sealed),
        "save_login" => autofill::save(app, body).map(Reply::Plain),
        _ => Err(Refusal::BadRequest),
    }
}

/// Answers one request from the extension, or serves the bridge while it stays connected.
pub fn serve(app: &App) -> Result<()> {
    let first = read_message(&mut std::io::stdin().lock());
    if let Ok(body) = &first
        && bridge::is_bridge(body)
    {
        // Only a browser may open the bridge: another process must not sit on the socket.
        if app.trusts_any_caller() || caller::parent_path().is_some_and(|p| caller::is_browser(&p))
        {
            return bridge::run(app);
        }
        let refused = json!({"v": VERSION, "type": "error", "code": Refusal::NotBrowser.code()});
        return write_message(&mut std::io::stdout().lock(), &refused);
    }
    let mut reply = match first.and_then(|body| answer(app, &body)) {
        Ok(Reply::Sealed(body)) => return write_frame(&mut std::io::stdout().lock(), &body),
        Ok(Reply::Plain(reply)) => reply,
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

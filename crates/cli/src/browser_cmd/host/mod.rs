//! The Native Messaging host (protocol v1, integrations/browser-extension): one
//! length-prefixed JSON request on stdin, one reply on stdout. stdout carries only the
//! protocol, and its messages are the types in [`wire`]. A reply carries a value in one case
//! only, `filled` after Touch ID ([`autofill`], ADR-0017); no reply or error ever holds a
//! cookie value.

mod autofill;
mod bridge;
mod caller;
mod receive;
mod wire;

use crate::compose::App;
use anyhow::Result;
use std::io::{Read, Write};
use wire::{HostReplyWire, HostRequestWire, Refusal};
use zeroize::Zeroizing;

/// Larger than any real site's cookies and storage; Chrome allows up to 64 MiB.
const MAX_MESSAGE: u32 = 4 * 1024 * 1024;

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

fn write_reply(output: &mut impl Write, reply: &HostReplyWire) -> Result<()> {
    write_frame(output, wire::encode(reply)?.as_bytes())
}

fn write_frame(output: &mut impl Write, body: &[u8]) -> Result<()> {
    output.write_all(&u32::try_from(body.len())?.to_ne_bytes())?;
    output.write_all(body)?;
    output.flush()?;
    Ok(())
}

/// A reply without values, or the one that carries a value (`filled`), already written into
/// a wiped buffer with `v`: a `Value` would hold an unwiped `String` copy of it (#157, J-160).
enum Reply {
    Plain(HostReplyWire),
    Sealed(Zeroizing<Vec<u8>>),
}

/// The reply to one request. [`serve`] opens the bridge before this is reached.
fn answer(app: &App, request: HostRequestWire) -> Result<Reply, Refusal> {
    match request {
        HostRequestWire::ReceiveSite(site) => receive::receive(app, site).map(Reply::Plain),
        HostRequestWire::AutofillList { origin } => autofill::list(app, &origin).map(Reply::Plain),
        HostRequestWire::AutofillFill { origin, item } => {
            autofill::fill(app, &origin, &item).map(Reply::Sealed)
        }
        HostRequestWire::SaveLogin(login) => autofill::save(app, login).map(Reply::Plain),
        HostRequestWire::Bridge => Err(Refusal::BadRequest),
    }
}

/// Answers one request from the extension, or serves the bridge while it stays connected.
pub fn serve(app: &App) -> Result<()> {
    let request =
        read_message(&mut std::io::stdin().lock()).and_then(|body| wire::parse_request(&body));
    let reply = match request {
        // Only a browser may open the bridge: another process must not sit on the socket.
        Ok(HostRequestWire::Bridge) if caller::is_trusted(app) => return bridge::run(app),
        Ok(HostRequestWire::Bridge) => Err(Refusal::NotBrowser),
        Ok(request) => answer(app, request),
        Err(refusal) => Err(refusal),
    };
    let mut out = std::io::stdout().lock();
    match reply {
        Ok(Reply::Sealed(body)) => write_frame(&mut out, &body),
        Ok(Reply::Plain(reply)) => write_reply(&mut out, &reply),
        Err(code) => write_reply(&mut out, &HostReplyWire::Error { code }),
    }
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
        write_reply(&mut out, &HostReplyWire::BridgeReady).unwrap();
        assert_eq!(out, framed(br#"{"type":"bridge_ready","v":1}"#));
    }
}

//! The bridge (drag to hand off, spec 2026-10-31): the extension opens a lasting native
//! connection with `bridge`; while it lasts this host listens on `run/extension.sock`, passes
//! each ask of the admin app to the extension as `request_site`, and returns its answer. The
//! extension asks the person in the browser before it hands anything over, so a process that
//! writes to the socket (an agent) cannot take a site without them. No value crosses here.

use super::{VERSION, read_message, write_message};
use crate::compose::App;
use anyhow::Result;
use keyharbor_domain::{SessionName, vault::Origin};
use keyharbor_supervisor::bridge::{BRIDGE_WAIT, SiteAnswer, SiteAsk, bridge_socket, listen};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    sync::mpsc,
};

enum Event {
    /// The app connected with one ask.
    Ask(UnixStream),
    /// A message from the extension, or `None` when it closed the connection.
    Extension(Option<Value>),
}

/// The `request_site` for `ask`, or the answer when it is not a site and a session name.
fn request(id: u64, ask: &SiteAsk) -> Result<Value, SiteAnswer> {
    let origin = Origin::of_page(&ask.url).map_err(|_| SiteAnswer::BadSite)?;
    let session = SessionName::parse(&ask.session).map_err(|_| SiteAnswer::NoSession)?;
    Ok(
        json!({"v": VERSION, "type": "request_site", "id": id, "session": session.as_str(), "origin": origin.as_str()}),
    )
}

/// The extension's answer to `id`, if `message` is it.
fn answer_to(id: u64, message: &Value) -> Option<SiteAnswer> {
    if message["type"] != "request_answer" || message["id"].as_u64() != Some(id) {
        return None;
    }
    let mut answer = message.clone();
    answer
        .as_object_mut()?
        .retain(|k, _| k == "outcome" || k == "cookies");
    Some(serde_json::from_value(answer).unwrap_or(SiteAnswer::Failed))
}

fn reply(stream: &UnixStream, answer: SiteAnswer) {
    if let Ok(mut line) = serde_json::to_string(&answer) {
        line.push('\n');
        let _ = (&*stream).write_all(line.as_bytes());
    }
}

/// Serves the bridge until the extension disconnects; then removes the socket.
pub(super) fn run(app: &App) -> Result<()> {
    let socket = bridge_socket(app.root());
    let listener = listen(app.root())?;
    let mut out = std::io::stdout().lock();
    write_message(&mut out, &json!({"v": VERSION, "type": "bridge_ready"}))?;

    let (tx, rx) = mpsc::channel();
    let asks = tx.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if asks.send(Event::Ask(stream)).is_err() {
                break;
            }
        }
    });
    std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        loop {
            let message = read_message(&mut input)
                .ok()
                .and_then(|body| serde_json::from_slice::<Value>(&body).ok());
            let closed = message.is_none();
            if tx.send(Event::Extension(message)).is_err() || closed {
                break;
            }
        }
    });

    let mut waiting: VecDeque<UnixStream> = VecDeque::new();
    let mut next_id = 0u64;
    let result = loop {
        let stream = match waiting.pop_front() {
            Some(stream) => stream,
            None => match rx.recv() {
                Ok(Event::Ask(stream)) => stream,
                Ok(Event::Extension(Some(_))) => continue,
                Ok(Event::Extension(None)) | Err(_) => break Ok(()),
            },
        };
        let mut line = String::new();
        let ask = BufReader::new(&stream)
            .read_line(&mut line)
            .ok()
            .and_then(|_| serde_json::from_str::<SiteAsk>(line.trim()).ok());
        let Some(ask) = ask else {
            reply(&stream, SiteAnswer::Failed);
            continue;
        };
        next_id += 1;
        let message = match request(next_id, &ask) {
            Ok(message) => message,
            Err(answer) => {
                reply(&stream, answer);
                continue;
            }
        };
        if write_message(&mut out, &message).is_err() {
            reply(&stream, SiteAnswer::Failed);
            break Ok(());
        }
        // One ask at a time: others wait their turn; the extension's answer ends this one.
        let answer = loop {
            match rx.recv_timeout(BRIDGE_WAIT) {
                Ok(Event::Ask(other)) => waiting.push_back(other),
                Ok(Event::Extension(Some(message))) => {
                    if let Some(answer) = answer_to(next_id, &message) {
                        break Some(answer);
                    }
                }
                Ok(Event::Extension(None)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break None;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => break Some(SiteAnswer::Timeout),
            }
        };
        match answer {
            Some(answer) => reply(&stream, answer),
            None => {
                reply(&stream, SiteAnswer::Failed);
                break Ok(());
            }
        }
    };
    for stream in waiting {
        reply(&stream, SiteAnswer::Failed);
    }
    let _ = std::fs::remove_file(&socket);
    result
}

/// Whether the first message opens a bridge rather than asking one thing.
pub(super) fn is_bridge(message: &[u8]) -> bool {
    serde_json::from_slice::<Value>(message)
        .is_ok_and(|m| m["v"] == VERSION && m["type"] == "bridge")
}

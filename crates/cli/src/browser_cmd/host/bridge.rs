//! The bridge (drag to hand off, spec 2026-10-31): the extension opens a lasting native
//! connection with `bridge`; while it lasts this host listens on `run/extension.sock`, passes
//! each ask of the admin app to the extension as `request_site`, and returns its answer. The
//! extension asks the person in the browser before it hands anything over, so a process that
//! writes to the socket (an agent) cannot take a site without them. No value crosses here.

use super::{
    read_message,
    wire::{ExtensionMessageWire, HostReplyWire},
    write_reply,
};
use crate::compose::App;
use anyhow::Result;
use keyharbor_domain::{SessionName, vault::Origin};
use keyharbor_supervisor::bridge::{BRIDGE_WAIT, SiteAnswer, SiteAsk, bridge_socket, listen};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, StdoutLock, Write},
    os::unix::net::{UnixListener, UnixStream},
    sync::mpsc,
};

enum Event {
    /// The app connected with one ask.
    Ask(UnixStream),
    /// A message from the extension, or `None` when it closed the connection.
    Extension(Option<ExtensionMessageWire>),
}

/// Whether the bridge goes on after an ask.
enum Served {
    Next,
    /// The extension is gone: the bridge ends.
    Closed,
}

/// The `request_site` for `ask`, or the answer when it is not a site and a session name.
fn request(id: u64, ask: &SiteAsk) -> Result<HostReplyWire, SiteAnswer> {
    let origin = Origin::of_page(&ask.url).map_err(|_| SiteAnswer::BadSite)?;
    let session = SessionName::parse(&ask.session).map_err(|_| SiteAnswer::NoSession)?;
    Ok(HostReplyWire::RequestSite {
        id,
        session: session.as_str().to_owned(),
        origin: origin.as_str().to_owned(),
    })
}

fn reply(stream: &UnixStream, answer: SiteAnswer) {
    if let Ok(mut line) = serde_json::to_string(&answer) {
        line.push('\n');
        let _ = (&*stream).write_all(line.as_bytes());
    }
}

/// The one ask an app connection carries, if it sent one.
fn read_ask(stream: &UnixStream) -> Option<SiteAsk> {
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    serde_json::from_str(line.trim()).ok()
}

/// Feeds app connections and extension messages into one channel, each from its own thread.
fn events(listener: UnixListener) -> mpsc::Receiver<Event> {
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
                .and_then(|body| ExtensionMessageWire::parse(&body));
            let closed = message.is_none();
            if tx.send(Event::Extension(message)).is_err() || closed {
                break;
            }
        }
    });
    rx
}

/// One bridge: the extension's stdout and the asks waiting their turn.
struct Bridge<'a> {
    out: StdoutLock<'a>,
    events: mpsc::Receiver<Event>,
    waiting: VecDeque<UnixStream>,
    next_id: u64,
}

impl Bridge<'_> {
    /// The next ask to serve, or `None` once the extension is gone.
    fn next_ask(&mut self) -> Option<UnixStream> {
        if let Some(stream) = self.waiting.pop_front() {
            return Some(stream);
        }
        loop {
            match self.events.recv() {
                Ok(Event::Ask(stream)) => return Some(stream),
                Ok(Event::Extension(Some(_))) => continue,
                Ok(Event::Extension(None)) | Err(_) => return None,
            }
        }
    }

    /// Passes one ask to the extension and its answer back to the app.
    fn serve_ask(&mut self, stream: &UnixStream) -> Served {
        let Some(ask) = read_ask(stream) else {
            reply(stream, SiteAnswer::Failed);
            return Served::Next;
        };
        self.next_id += 1;
        let message = match request(self.next_id, &ask) {
            Ok(message) => message,
            Err(answer) => {
                reply(stream, answer);
                return Served::Next;
            }
        };
        if write_reply(&mut self.out, &message).is_err() {
            reply(stream, SiteAnswer::Failed);
            return Served::Closed;
        }
        match self.await_answer(self.next_id) {
            Some(answer) => {
                reply(stream, answer);
                Served::Next
            }
            None => {
                reply(stream, SiteAnswer::Failed);
                Served::Closed
            }
        }
    }

    /// The extension's answer to `id`; `None` if it closed first. One ask at a time: others
    /// wait their turn.
    fn await_answer(&mut self, id: u64) -> Option<SiteAnswer> {
        loop {
            match self.events.recv_timeout(BRIDGE_WAIT) {
                Ok(Event::Ask(other)) => self.waiting.push_back(other),
                Ok(Event::Extension(Some(message))) => {
                    if let Some(answer) = message.answer_to(id) {
                        return Some(answer);
                    }
                }
                Ok(Event::Extension(None)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return None;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => return Some(SiteAnswer::Timeout),
            }
        }
    }
}

/// Serves the bridge until the extension disconnects; then removes the socket.
pub(super) fn run(app: &App) -> Result<()> {
    let socket = bridge_socket(app.root());
    let listener = listen(app.root())?;
    let mut out = std::io::stdout().lock();
    write_reply(&mut out, &HostReplyWire::BridgeReady)?;
    let mut bridge = Bridge {
        out,
        events: events(listener),
        waiting: VecDeque::new(),
        next_id: 0,
    };
    while let Some(stream) = bridge.next_ask() {
        if let Served::Closed = bridge.serve_ask(&stream) {
            break;
        }
    }
    for stream in bridge.waiting {
        reply(&stream, SiteAnswer::Failed);
    }
    let _ = std::fs::remove_file(&socket);
    Ok(())
}

//! The person at the browser, for tests: a raw CDP connection (a session opened with
//! `cdp: true`) whose input Chrome treats as trusted, like a person's.

use serde_json::{Value, json};
use std::net::TcpStream;
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};

pub struct Person {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    session: String,
    next: u64,
}

impl Person {
    /// Connects to the browser endpoint and attaches to its first page.
    pub fn connect(endpoint: &str) -> Self {
        let (socket, _) = tungstenite::connect(endpoint).unwrap();
        let mut person = Self {
            socket,
            session: String::new(),
            next: 0,
        };
        let targets = person.browser("Target.getTargets", json!({}));
        let page = targets["targetInfos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["type"] == "page")
            .unwrap()["targetId"]
            .clone();
        let attached = person.browser(
            "Target.attachToTarget",
            json!({"targetId": page, "flatten": true}),
        );
        person.session = attached["sessionId"].as_str().unwrap().to_owned();
        person
    }
    /// A browser-level command.
    pub fn browser(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params, None)
    }
    /// A command to the attached page.
    pub fn page(&mut self, method: &str, params: Value) -> Value {
        let session = self.session.clone();
        self.call(method, params, Some(&session))
    }
    fn call(&mut self, method: &str, params: Value, session: Option<&str>) -> Value {
        self.next += 1;
        let mut request = json!({"id": self.next, "method": method, "params": params});
        if let Some(session) = session {
            request["sessionId"] = json!(session);
        }
        self.socket
            .send(Message::text(request.to_string()))
            .unwrap();
        loop {
            let Message::Text(text) = self.socket.read().unwrap() else {
                continue;
            };
            let reply: Value = serde_json::from_str(&text).unwrap();
            if reply["id"] == self.next {
                assert!(reply.get("error").is_none(), "{method}: {reply}");
                return reply["result"].clone();
            }
        }
    }
    /// Focuses `selector` and types `text` as keyboard input.
    pub fn type_into(&mut self, selector: &str, text: &str) {
        let focus = format!("document.querySelector('{selector}').focus()");
        self.page("Runtime.evaluate", json!({"expression": focus}));
        self.page("Input.insertText", json!({"text": text}));
    }
    pub fn press_enter(&mut self) {
        for kind in ["keyDown", "keyUp"] {
            let key = json!({"type": kind, "key": "Enter", "code": "Enter", "windowsVirtualKeyCode": 13, "text": "\r"});
            self.page("Input.dispatchKeyEvent", key);
        }
    }
}

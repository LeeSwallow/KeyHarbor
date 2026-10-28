//! A CDP client over Chrome's `--remote-debugging-pipe` (framing in [`super::protocol`]).
//!
//! Requests get increasing integer ids; a reader task matches responses to waiting callers and
//! broadcasts everything else (events). A call that gets no answer fails after the timeout.

use super::protocol::{self, Incoming, Reply};
use anyhow::{Result, anyhow, bail};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    sync::{broadcast, mpsc, oneshot},
};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Reply>>>>;

pub struct Cdp {
    outgoing: Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>,
    pending: Pending,
    /// Set once the browser side of the pipe has closed.
    closed: Arc<AtomicBool>,
    events: broadcast::Sender<Value>,
    next_id: AtomicU64,
    timeout: Duration,
}

impl Cdp {
    /// Starts the reader and writer tasks; must be called inside a Tokio runtime.
    pub fn new<W, R>(writer: W, reader: R) -> Self
    where
        W: AsyncWrite + Unpin + Send + 'static,
        R: AsyncRead + Unpin + Send + 'static,
    {
        let (outgoing, mut queue) = mpsc::unbounded_channel::<Vec<u8>>();
        tokio::spawn(async move {
            let mut writer = writer;
            while let Some(message) = queue.recv().await {
                if writer.write_all(&message).await.is_err() || writer.flush().await.is_err() {
                    break;
                }
            }
            let _ = writer.shutdown().await;
        });
        let pending: Pending = Arc::default();
        let (events, _) = broadcast::channel(1024);
        let closed = Arc::new(AtomicBool::new(false));
        let (pending_reader, events_reader, closed_reader) =
            (pending.clone(), events.clone(), closed.clone());
        tokio::spawn(async move {
            let mut reader = BufReader::new(reader);
            let mut buffer = Vec::new();
            loop {
                buffer.clear();
                match reader.read_until(0, &mut buffer).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => (),
                }
                let Some(message) = protocol::decode(&mut buffer) else {
                    continue;
                };
                match message {
                    Incoming::Reply { id, reply } => {
                        let waiter = pending_reader.lock().ok().and_then(|mut p| p.remove(&id));
                        if let Some(waiter) = waiter {
                            let _ = waiter.send(reply);
                        }
                    }
                    Incoming::Event(event) => {
                        let _ = events_reader.send(event);
                    }
                }
            }
            // The browser is gone: wake every caller with an error.
            closed_reader.store(true, Ordering::SeqCst);
            if let Ok(mut pending) = pending_reader.lock() {
                pending.clear();
            }
        });
        Self {
            outgoing: Mutex::new(Some(outgoing)),
            pending,
            closed,
            events,
            next_id: AtomicU64::new(1),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    #[cfg(test)]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn enqueue(&self, bytes: Vec<u8>) -> Result<()> {
        let outgoing = self
            .outgoing
            .lock()
            .map_err(|_| anyhow!("CDP lock poisoned"))?;
        match outgoing.as_ref() {
            Some(sender) if sender.send(bytes).is_ok() => Ok(()),
            _ => bail!("browser connection closed"),
        }
    }

    /// Sends `method` (to the page `session`, if given) and waits for its result.
    pub async fn call(&self, method: &str, params: Value, session: Option<&str>) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply, response) = oneshot::channel();
        self.pending
            .lock()
            .map_err(|_| anyhow!("CDP lock poisoned"))?
            .insert(id, reply);
        if self.closed.load(Ordering::SeqCst) {
            self.forget(id);
            bail!("browser connection closed");
        }
        if let Err(error) = self.enqueue(protocol::encode(id, method, params, session)) {
            self.forget(id);
            return Err(error);
        }
        match tokio::time::timeout(self.timeout, response).await {
            Ok(Ok(Ok(result))) => Ok(result),
            Ok(Ok(Err(message))) => bail!("{method}: {message}"),
            Ok(Err(_)) => bail!("browser connection closed"),
            Err(_) => {
                self.forget(id);
                bail!(
                    "{method} timed out after {} seconds",
                    self.timeout.as_secs_f32()
                )
            }
        }
    }

    fn forget(&self, id: u64) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&id);
        }
    }

    /// Sends `method` without waiting for a reply, then closes the pipe to the browser.
    pub fn send_and_close(&self, method: &str, params: Value) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let _ = self.enqueue(protocol::encode(id, method, params, None));
        if let Ok(mut outgoing) = self.outgoing.lock() {
            outgoing.take();
        }
    }

    pub fn events(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, DuplexStream, duplex};

    /// The browser's side of the pipe pair.
    struct FakeBrowser {
        from_client: BufReader<DuplexStream>,
        to_client: DuplexStream,
    }
    impl FakeBrowser {
        async fn read(&mut self) -> Value {
            let mut buffer = Vec::new();
            self.from_client.read_until(0, &mut buffer).await.unwrap();
            assert_eq!(buffer.pop(), Some(0), "messages end with NUL");
            serde_json::from_slice(&buffer).unwrap()
        }
        async fn send(&mut self, message: Value) {
            let mut bytes = serde_json::to_vec(&message).unwrap();
            bytes.push(0);
            self.to_client.write_all(&bytes).await.unwrap();
        }
    }
    fn pair() -> (Arc<Cdp>, FakeBrowser) {
        let (client_write, browser_read) = duplex(1 << 16);
        let (browser_write, client_read) = duplex(1 << 16);
        (
            Arc::new(Cdp::new(client_write, client_read)),
            FakeBrowser {
                from_client: BufReader::new(browser_read),
                to_client: browser_write,
            },
        )
    }

    #[tokio::test]
    async fn requests_are_nul_terminated_and_matched() {
        let (cdp, mut browser) = pair();
        let call = tokio::spawn({
            let cdp = cdp.clone();
            async move {
                cdp.call("Page.navigate", json!({"url": "about:blank"}), Some("S1"))
                    .await
            }
        });
        let request = browser.read().await;
        assert_eq!(request["method"], "Page.navigate");
        assert_eq!(request["params"]["url"], "about:blank");
        assert_eq!(request["sessionId"], "S1");
        browser
            .send(json!({"id": request["id"], "result": {"frameId": "F"}}))
            .await;
        assert_eq!(call.await.unwrap().unwrap(), json!({"frameId": "F"}));
        let call = tokio::spawn({
            let cdp = cdp.clone();
            async move { cdp.call("Browser.getVersion", json!({}), None).await }
        });
        let request = browser.read().await;
        assert!(request.get("sessionId").is_none());
        assert!(request["id"].as_u64().unwrap() > 1);
        browser
            .send(json!({"id": request["id"], "error": {"code": -32000, "message": "nope"}}))
            .await;
        let error = call.await.unwrap().unwrap_err().to_string();
        assert!(error.contains("nope"), "{error}");
    }

    #[tokio::test]
    async fn out_of_order_responses_reach_their_callers() {
        let (cdp, mut browser) = pair();
        let first = tokio::spawn({
            let cdp = cdp.clone();
            async move { cdp.call("A", json!({}), None).await }
        });
        let a = browser.read().await;
        let second = tokio::spawn({
            let cdp = cdp.clone();
            async move { cdp.call("B", json!({}), None).await }
        });
        let b = browser.read().await;
        browser
            .send(json!({"id": b["id"], "result": {"name": "B"}}))
            .await;
        browser
            .send(json!({"id": a["id"], "result": {"name": "A"}}))
            .await;
        assert_eq!(second.await.unwrap().unwrap()["name"], "B");
        assert_eq!(first.await.unwrap().unwrap()["name"], "A");
    }

    #[tokio::test]
    async fn events_are_broadcast() {
        let (cdp, mut browser) = pair();
        let mut one = cdp.events();
        let mut two = cdp.events();
        browser
            .send(json!({"method": "Page.loadEventFired", "params": {}, "sessionId": "S"}))
            .await;
        assert_eq!(one.recv().await.unwrap()["method"], "Page.loadEventFired");
        assert_eq!(two.recv().await.unwrap()["sessionId"], "S");
    }

    #[tokio::test]
    async fn unanswered_calls_time_out_and_closed_pipe_fails_calls() {
        let (client_write, browser_read) = duplex(1 << 16);
        let (browser_write, client_read) = duplex(1 << 16);
        let cdp =
            Arc::new(Cdp::new(client_write, client_read).with_timeout(Duration::from_millis(100)));
        let error = cdp.call("Slow", json!({}), None).await.unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error}");
        drop(browser_write);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let error = cdp.call("Gone", json!({}), None).await.unwrap_err();
        assert!(error.to_string().contains("closed"), "{error}");
        drop(browser_read);
    }

    #[tokio::test]
    async fn send_and_close_flushes_then_closes() {
        let (cdp, mut browser) = pair();
        cdp.send_and_close("Browser.close", json!({}));
        assert_eq!(browser.read().await["method"], "Browser.close");
        let mut rest = Vec::new();
        browser.from_client.read_to_end(&mut rest).await.unwrap();
        assert!(rest.is_empty());
        assert!(cdp.call("After", json!({}), None).await.is_err());
    }
}

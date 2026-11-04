//! The guarded browser against a fake page: the shared `Fake` and helpers here, one file
//! per group of behaviour.

use super::*;
use crate::{
    Guard,
    test_support::{env, init, login, open_vault},
};
use keyharbor_domain::ports::browser::{Choice, Element, Focus, Page};
use keyharbor_domain::vault::Origin;
use keyharbor_domain::{
    SessionId,
    audit::AuditKind,
    policy::{Effect, LOCAL_AGENT, Resource, Rule},
    ports::{PolicyStore, Store},
    session::substitution_lock::{LockReason, UNLOCKED},
};
use keyharbor_domain::{
    ports::GrantStore,
    upload::{UploadFile, UploadPolicy},
};
use keyharbor_domain::{ports::browser::LoginFields, vault::ItemName};
use keyharbor_sqlite::SqliteStore;
use serde_json::json;
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

mod fill;
mod items;
mod locks;
mod login;
mod redaction;
mod select;
mod upload;

/// A browser whose page is `https://github.com` with one focused input (node 7), a
/// password input unless a test changes `element`.
struct Fake {
    typed: Vec<String>,
    echo: String,
    element: Element,
    /// What the last redacted snapshot was asked to hide.
    hidden: BTreeSet<u64>,
    /// The focused node; inserting text moves the focus to `focus_after_insert`.
    focused: u64,
    focus_after_insert: Option<u64>,
    /// Whether navigations have settled, and the page's navigation count; clearing an
    /// element adds `navigate_on_clear` to it (page code that navigates).
    settled: bool,
    epoch: u64,
    navigate_on_clear: u64,
    /// The sign-in inputs `login_fields` reports.
    login: LoginFields,
    pressed: Vec<String>,
    /// The `<select>` options `(value, label)` and what `select_option` was asked for.
    options: Vec<(String, String)>,
    asked: Vec<Choice>,
    /// What `set_files` put into the page.
    uploaded: Vec<std::path::PathBuf>,
    /// What the last screenshot was asked to hide, and whether hiding fails.
    shot_hidden: Option<BTreeSet<u64>>,
    conceal_fails: bool,
}
impl Default for Fake {
    fn default() -> Self {
        Self {
            typed: Vec::new(),
            echo: String::new(),
            element: Element {
                local_name: "input".into(),
                input_type: Some("password".into()),
                ..Element::default()
            },
            hidden: BTreeSet::new(),
            focused: 7,
            focus_after_insert: None,
            settled: true,
            epoch: 0,
            navigate_on_clear: 0,
            login: LoginFields {
                username: Some(7),
                password: Some(7),
            },
            pressed: Vec::new(),
            options: Vec::new(),
            asked: Vec::new(),
            uploaded: Vec::new(),
            shot_hidden: None,
            conceal_fails: false,
        }
    }
}
impl BrowserPort for Fake {
    async fn navigate(&mut self, _: &str) -> Result<Value> {
        Ok(json!({"title": self.echo}))
    }
    async fn snapshot(&mut self) -> Result<Value> {
        Ok(json!({"text": self.typed.join("|")}))
    }
    async fn screenshot(&mut self) -> Result<Value> {
        Ok(json!({}))
    }
    async fn click(&mut self, _: &Target) -> Result<Value> {
        anyhow::bail!("no element {}", self.echo)
    }
    async fn fill(&mut self, _: &Target, text: &str) -> Result<FillOutcome> {
        self.typed.push(text.to_owned());
        Ok(FillOutcome::default())
    }
    async fn type_text(&mut self, text: &str) -> Result<FillOutcome> {
        self.typed.push(text.to_owned());
        Ok(FillOutcome::default())
    }
    async fn press(&mut self, key: &str) -> Result<Value> {
        self.pressed.push(key.to_owned());
        Ok(json!({}))
    }
    async fn tabs(&mut self) -> Result<Value> {
        Ok(json!([]))
    }
    async fn new_tab(&mut self, _: Option<&str>) -> Result<Value> {
        Ok(json!({}))
    }
    async fn switch_tab(&mut self, _: &str) -> Result<Value> {
        Ok(json!({}))
    }
    async fn wait(&mut self, _: &WaitFor) -> Result<Value> {
        Ok(json!({}))
    }
    fn close(&mut self) -> Result<()> {
        Ok(())
    }
}
impl PageInput for Fake {
    async fn page(&mut self) -> Result<Page> {
        Ok(Page("p".into()))
    }
    async fn focus_element(&mut self, _: &Page, _: &Target) -> Result<u64> {
        Ok(7)
    }
    async fn clear_element(&mut self, _: &Page, _: u64) -> Result<()> {
        self.epoch += self.navigate_on_clear;
        Ok(())
    }
    async fn insert_text(&mut self, _: &Page, text: &str) -> Result<()> {
        self.typed.push(text.to_owned());
        if let Some(node) = self.focus_after_insert {
            self.focused = node;
        }
        Ok(())
    }
    async fn redacted_snapshot(&mut self, hidden: &BTreeSet<u64>) -> Result<Value> {
        self.hidden = hidden.clone();
        self.snapshot().await
    }
    async fn redacted_screenshot(&mut self, hidden: &BTreeSet<u64>) -> Result<Value> {
        self.shot_hidden = Some(hidden.clone());
        if self.conceal_fails && !hidden.is_empty() {
            anyhow::bail!("filled inputs could not be hidden; no screenshot taken");
        }
        self.screenshot().await
    }
    async fn login_fields(&mut self, _: &Page) -> Result<LoginFields> {
        Ok(self.login.clone())
    }
    async fn set_files(&mut self, _: &Page, _: u64, paths: &[std::path::PathBuf]) -> Result<()> {
        self.uploaded.extend_from_slice(paths);
        Ok(())
    }
    async fn location(&mut self, _: &Page) -> Result<String> {
        Ok(format!("https://github.com/{}", self.echo))
    }
    async fn select_option(&mut self, _: &Page, _: u64, choice: &Choice) -> Result<Option<u32>> {
        self.asked.push(choice.clone());
        let found = self.options.iter().position(|(value, label)| match choice {
            Choice::Value(want) => value == want,
            Choice::Label(want) => label == want,
            Choice::Index(_) => false,
        });
        Ok(match choice {
            Choice::Index(index) => (*index as usize) < self.options.len(),
            _ => found.is_some(),
        }
        .then(|| match choice {
            Choice::Index(index) => *index,
            _ => found.unwrap() as u32,
        }))
    }
}
impl FrameInspector for Fake {
    async fn focus(&mut self, _: &Page) -> Result<Focus> {
        Ok(Focus {
            frame: "main".into(),
            node: self.focused,
        })
    }
    async fn frame_origin(&mut self, _: &Page, _: &str) -> Option<Origin> {
        Origin::parse("https://github.com").ok()
    }
    async fn element(&mut self, _: &Page, _: u64) -> Result<Element> {
        Ok(self.element.clone())
    }
    async fn settle(&mut self, _: &Page, _: Option<&str>) -> Result<bool> {
        Ok(self.settled)
    }
    async fn navigation_epoch(&mut self, _: &Page) -> u64 {
        self.epoch
    }
}

fn guard(e: &crate::test_support::Env) -> (Guard, SharedStoreProbe) {
    let store: Box<dyn Store> = Box::new(SqliteStore::open(&e.data).unwrap());
    let store = Arc::new(Mutex::new(store));
    let (data, keys) = (e.data.clone(), e.keys.clone());
    let opener: crate::VaultOpener = Box::new(move || {
        keyharbor_core::Vault::open(
            SqliteStore::open(&data)?,
            Box::new(keyharbor_keystore::FileKeyStore::new(keys.clone())),
            crate::test_support::clock(),
        )
    });
    (
        Guard::new(
            keyharbor_crypto::random::id::<SessionId>(),
            store.clone(),
            opener,
            Arc::new(ClockHandle),
        ),
        SharedStoreProbe(store),
    )
}

/// One clock for every guard in these tests, so a test can move it.
static CLOCK: std::sync::LazyLock<keyharbor_domain::ports::ManualClock> =
    std::sync::LazyLock::new(|| keyharbor_domain::ports::ManualClock::at_unix_seconds(1000));
struct ClockHandle;
impl keyharbor_domain::ports::Clock for ClockHandle {
    fn now(&self) -> std::time::Duration {
        CLOCK.now()
    }
    fn sleep(&self, duration: std::time::Duration) {
        CLOCK.sleep(duration)
    }
}
struct SharedStoreProbe(crate::SharedStore);
impl SharedStoreProbe {
    fn kinds(&self) -> Vec<(AuditKind, String)> {
        let store = self.0.lock().unwrap();
        store
            .list_audit(None)
            .unwrap()
            .into_iter()
            .filter(|r| r.event.kind != AuditKind::VaultItemChanged)
            .map(|r| (r.event.kind, r.event.result))
            .collect()
    }
}

fn block<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

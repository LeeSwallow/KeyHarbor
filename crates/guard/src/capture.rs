//! Storing a sign-in the user typed in a session's browser (08 §8.2 ②). It is kept only when
//! capture is on and the browser is with the user (a handoff), so text an agent typed is
//! never captured (J-80). It never overwrites: a new account or a changed password becomes a
//! draft, an unchanged one is left alone.

use crate::Guard;
use anyhow::{Result, anyhow, bail};
use keyharbor_core::{DraftItem, ItemSource, ItemStatus, PlaintextAccess, Vault};
use keyharbor_domain::{
    State,
    audit::{AuditEvent, AuditKind},
    ports::browser::CapturedLogin,
    settings::{CAPTURE_ENABLED, capture_enabled},
    vault::{FieldName, ItemKind, ItemName, Origin, PlaceholderRef},
};
use zeroize::Zeroizing;

/// Who the usage record names for a capture: the person at the browser.
pub const CAPTURE_AGENT: &str = "user";

/// What became of one captured sign-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureOutcome {
    /// `capture_enabled` is not `true`.
    Off,
    /// The session was not handed to the user, so the input may be the agent's.
    NotHandedOff,
    /// The vault already has this username and password for the origin.
    Known(ItemName),
    /// Stored as a draft (a new account, or a new password for an existing one).
    Draft(ItemName),
}

impl Guard {
    pub fn capture(&self, login: CapturedLogin) -> Result<CaptureOutcome> {
        let (enabled, state) = {
            let store = self
                .store()
                .lock()
                .map_err(|_| anyhow!("store lock poisoned"))?;
            let enabled = capture_enabled(store.setting_get(CAPTURE_ENABLED)?.as_deref());
            (enabled, store.get(self.session())?.map(|row| row.state))
        };
        if !enabled {
            return Ok(CaptureOutcome::Off);
        }
        if state != Some(State::AwaitingUser) {
            return Ok(CaptureOutcome::NotHandedOff);
        }
        let origin = Origin::parse(&login.origin).map_err(|e| anyhow!("{e}"))?;
        let outcome = self.with_vault(|loaded| match loaded {
            Some(loaded) => store_login(&loaded.vault, &origin, login),
            None => bail!("no vault to capture into"),
        })??;
        let (result, item) = match &outcome {
            CaptureOutcome::Known(item) => ("known", item),
            CaptureOutcome::Draft(item) => ("draft", item),
            _ => return Ok(outcome),
        };
        self.record(
            &AuditEvent::new(CAPTURE_AGENT, AuditKind::CredentialCaptured, result)
                .session(self.session())
                .item(item.as_str())
                .origin(Some(origin.as_str())),
        );
        Ok(outcome)
    }
}

fn field(name: &str) -> FieldName {
    FieldName::parse(name).expect("login field names are valid")
}

/// The stored value of `item.field`, if it has one.
fn stored(
    vault: &Vault,
    access: &PlaintextAccess,
    item: &ItemName,
    name: &str,
) -> Option<Zeroizing<String>> {
    let reference = PlaceholderRef {
        item: item.clone(),
        field: field(name),
    };
    vault.resolve(access, &reference).ok()
}

/// Compares with the logins already kept for `origin` and stores a draft if this one is new.
pub(crate) fn store_login(
    vault: &Vault,
    origin: &Origin,
    login: CapturedLogin,
) -> Result<CaptureOutcome> {
    let access = PlaintextAccess::issue();
    let mut base = None;
    for item in vault.list()? {
        let usable = !item.tampered
            && item.kind == ItemKind::Login
            && item.status != ItemStatus::Discarded
            && item.origins.iter().any(|o| o == origin.as_str());
        if !usable {
            continue;
        }
        let name = ItemName::parse(&item.name).map_err(|e| anyhow!("{e}"))?;
        let username = stored(vault, &access, &name, "username");
        if username.as_deref().map(String::as_str) != login.username.as_deref().map(String::as_str)
        {
            continue;
        }
        if stored(vault, &access, &name, "password").as_deref() == Some(&login.password) {
            return Ok(CaptureOutcome::Known(name));
        }
        base.get_or_insert(name.base());
    }
    let name = match base {
        Some(name) => name,
        None => name_for(origin)?,
    };
    let mut fields = vec![(field("password"), login.password)];
    if let Some(username) = login.username {
        fields.push((field("username"), username));
    }
    let draft = DraftItem {
        name,
        kind: ItemKind::Login,
        fields,
        origins: vec![origin.clone()],
        source: ItemSource::Captured,
    };
    Ok(CaptureOutcome::Draft(
        vault.add_draft(draft, CAPTURE_AGENT)?,
    ))
}

/// An item name from the origin's host: `https://accounts.example.com` → `accounts-example-com`.
fn name_for(origin: &Origin) -> Result<ItemName> {
    let host = origin
        .as_str()
        .split_once("://")
        .map_or(origin.as_str(), |(_, rest)| rest);
    let name: String = host
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(64)
        .collect();
    ItemName::parse_new(name.trim_matches('-')).map_err(|e| anyhow!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_come_from_the_host() {
        let name = |o: &str| name_for(&Origin::parse(o).unwrap()).unwrap();
        assert_eq!(
            name("https://accounts.example.com").as_str(),
            "accounts-example-com"
        );
        assert_eq!(name("http://localhost:8080").as_str(), "localhost-8080");
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;
    use crate::test_support::{clock, env, init, login, open_vault, pref};
    use keyharbor_domain::{
        SessionName,
        ports::{SessionStore, SettingsStore, Store},
    };
    use keyharbor_sqlite::SqliteStore;
    use std::sync::{Arc, Mutex};

    struct Setup {
        e: crate::test_support::Env,
        guard: Guard,
        store: crate::SharedStore,
        generation: keyharbor_domain::Generation,
        id: keyharbor_domain::SessionId,
    }

    /// A running session, a vault with `github` (me@x.com / old) and capture `on`.
    fn setup(on: bool) -> Setup {
        let e = env();
        init(&e).add(login("github", "old")).unwrap();
        let store = SqliteStore::open(&e.data).unwrap();
        let id = store
            .create(SessionName::parse("work").unwrap())
            .unwrap()
            .id;
        let generation = store.reserve(id, false).unwrap().unwrap();
        assert!(
            store
                .transition(id, generation, State::Starting, State::Running)
                .unwrap()
        );
        if on {
            store.setting_set(CAPTURE_ENABLED, "true").unwrap();
        }
        let store: crate::SharedStore = Arc::new(Mutex::new(Box::new(store) as Box<dyn Store>));
        let data = e.data.clone();
        let keys = e.keys.clone();
        let opener = move || {
            open_vault(&crate::test_support::Env {
                _tmp: tempfile::tempdir().unwrap(),
                data: data.clone(),
                keys: keys.clone(),
            })
        };
        let guard = Guard::new(id, store.clone(), Box::new(opener), clock());
        Setup {
            e,
            guard,
            store,
            generation,
            id,
        }
    }
    fn hand_off(s: &Setup) {
        s.store
            .lock()
            .unwrap()
            .request_handoff(s.id, s.generation, None)
            .unwrap();
    }
    fn typed(origin: &str, username: Option<&str>, password: &str) -> CapturedLogin {
        CapturedLogin {
            origin: origin.into(),
            username: username.map(|u| Zeroizing::new(u.into())),
            password: Zeroizing::new(password.into()),
        }
    }
    fn draft(name: &str) -> CaptureOutcome {
        CaptureOutcome::Draft(ItemName::parse(name).unwrap())
    }

    #[test]
    fn nothing_is_captured_while_off_or_while_the_agent_has_the_browser() {
        let off = setup(false);
        hand_off(&off);
        let login = || typed("https://github.com", Some("me@x.com"), "new");
        assert_eq!(off.guard.capture(login()).unwrap(), CaptureOutcome::Off);
        let agent_has_it = setup(true);
        assert_eq!(
            agent_has_it.guard.capture(login()).unwrap(),
            CaptureOutcome::NotHandedOff
        );
    }

    #[test]
    fn a_changed_password_becomes_a_draft_and_the_stored_one_stays() {
        let s = setup(true);
        hand_off(&s);
        let changed = s
            .guard
            .capture(typed("https://github.com", Some("me@x.com"), "new"))
            .unwrap();
        assert_eq!(changed, draft("github~draft-1"));
        let vault = open_vault(&s.e).unwrap();
        let access = PlaintextAccess::issue();
        assert_eq!(
            *vault.resolve(&access, &pref("github", "password")).unwrap(),
            "old"
        );
        assert_eq!(
            *vault
                .resolve(&access, &pref("github~draft-1", "password"))
                .unwrap(),
            "new"
        );
        let again = s
            .guard
            .capture(typed("https://github.com", Some("me@x.com"), "new"))
            .unwrap();
        assert_eq!(
            again,
            CaptureOutcome::Known(ItemName::parse("github~draft-1").unwrap())
        );
        let same = s
            .guard
            .capture(typed("https://github.com", Some("me@x.com"), "old"))
            .unwrap();
        assert_eq!(
            same,
            CaptureOutcome::Known(ItemName::parse("github").unwrap())
        );
    }

    #[test]
    fn a_new_account_is_named_after_its_host_and_bad_origins_are_refused() {
        let s = setup(true);
        hand_off(&s);
        let other_user = s
            .guard
            .capture(typed("https://github.com", Some("you@x.com"), "pw"))
            .unwrap();
        assert_eq!(other_user, draft("github-com"));
        let new_site = s
            .guard
            .capture(typed("https://shop.example", None, "pw"))
            .unwrap();
        assert_eq!(new_site, draft("shop-example"));
        let item = open_vault(&s.e)
            .unwrap()
            .list()
            .unwrap()
            .into_iter()
            .find(|i| i.name == "shop-example")
            .unwrap();
        assert_eq!(
            (item.status, item.source),
            (ItemStatus::Draft, ItemSource::Captured)
        );
        assert!(s.guard.capture(typed("null", None, "pw")).is_err());
        assert!(
            s.guard
                .capture(typed("http://evil.example", None, "pw"))
                .is_err()
        );
        let events = s.store.lock().unwrap().list_audit(None).unwrap();
        let captured: Vec<_> = events
            .iter()
            .filter(|r| r.event.kind == AuditKind::CredentialCaptured)
            .map(|r| (r.event.agent.as_str(), r.event.result.as_str()))
            .collect();
        assert_eq!(captured, [("user", "draft"), ("user", "draft")]);
        assert!(!format!("{events:?}").contains("pw\""));
    }
}

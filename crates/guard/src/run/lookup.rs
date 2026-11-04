//! Channel ④ (developer secrets spec §3): a program inside `keyharbor run` asks for a value by
//! name while it runs, e.g. `keyring.get_password("myapp/dev/db", "password")`. The CLI
//! carries the request over a socket only the child knows; deciding is here.
//!
//! In order: the run's token, then the run's scope (the items it was given, widened by
//! `--allow`), then the same chain as the other channels (`checks::PROCESS`). Every refusal
//! after the token check names the item and the reason only, and is recorded; a value is
//! resolved only once everything passed (J-122).

use super::Checker;
use crate::audit;
use keyharbor_core::{AccessToken, PlaintextAccess, Vault};
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    denial::DenialReason,
    item_pattern::ItemPattern,
    ports::Store,
    vault::{FieldName, ItemName, PlaceholderRef},
};
use zeroize::Zeroizing;

/// The usage-record result for a value handed over by lookup (kind `env_injected`; the
/// values handed over when the child starts are `ok`).
pub const LOOKUP_RESULT: &str = "lookup";

/// One request, as the CLI read it off the socket.
pub struct Ask<'a> {
    pub token: &'a str,
    pub item: &'a str,
    pub field: &'a str,
}

/// Why a request got no value. Its text names at most the item and the reason.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The request did not carry the run's token.
    Token,
    /// The item or field name is not a valid name.
    BadName,
    /// The item is outside the run's scope, or a check refused it.
    Denied(ItemName, DenialReason),
    /// The value could not be read.
    Vault,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Token => write!(f, "denied ({})", DenialReason::BadToken),
            Self::BadName => f.write_str("bad item or field name"),
            Self::Denied(item, reason) => write!(f, "denied: {} ({reason})", item.as_str()),
            Self::Vault => write!(f, "denied ({})", DenialReason::Vault),
        }
    }
}

/// What one run's child may ask for, and the token it must show.
pub struct Lookup {
    agent: String,
    given: Vec<ItemName>,
    allow: Vec<ItemPattern>,
    token: AccessToken,
}

impl Lookup {
    pub(super) fn new(agent: &str, given: Vec<ItemName>, allow: Vec<ItemPattern>) -> Self {
        Self {
            agent: agent.to_owned(),
            given,
            allow,
            token: AccessToken::new(),
        }
    }

    /// The token to hand to the child, and to no one else.
    pub fn token(&self) -> &str {
        self.token.as_str()
    }

    fn covers(&self, item: &ItemName) -> bool {
        self.given.contains(item) || self.allow.iter().any(|p| p.matches(item))
    }

    /// The value `ask` names, if the token is right, the item is in scope and the checks pass.
    /// Policy rules are read again for every request, so a rule added during a long run
    /// applies to its next request.
    pub fn answer(
        &self,
        store: &dyn Store,
        vault: &Vault,
        ask: &Ask<'_>,
    ) -> Result<Zeroizing<String>, Refusal> {
        let item = ItemName::parse(ask.item);
        if !self.token.matches(ask.token) {
            let mut event = AuditEvent::new(
                &self.agent,
                AuditKind::SecretDenied,
                DenialReason::BadToken.as_str(),
            );
            if let Ok(item) = &item {
                event = event.item(item.as_str());
            }
            audit::record(store, &event);
            return Err(Refusal::Token);
        }
        let item = item.map_err(|_| Refusal::BadName)?;
        let field = FieldName::parse(ask.field).map_err(|_| Refusal::BadName)?;
        let deny = |reason| {
            audit::record(
                store,
                &audit::denied(&self.agent, None, &item, None, reason),
            );
            Refusal::Denied(item.clone(), reason)
        };
        if !self.covers(&item) {
            return Err(deny(DenialReason::OutsideRun));
        }
        let mut checker =
            Checker::new(store, vault, &self.agent).map_err(|_| deny(DenialReason::Vault))?;
        // The checker records its refusals; only the reason goes back.
        checker
            .item_use(&item)
            .and_then(|info| checker.pass(&item, &info, &[field.as_str()]))
            .map_err(|reason| Refusal::Denied(item.clone(), reason))?;
        // Every check passed: this is where the guard grants plaintext.
        let reference = PlaceholderRef { item, field };
        let value = vault
            .resolve(&PlaintextAccess::issue(), &reference)
            .map_err(|_| Refusal::Vault)?;
        audit::record(
            store,
            &AuditEvent::new(&self.agent, AuditKind::EnvInjected, LOOKUP_RESULT)
                .item(reference.item.as_str()),
        );
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{env, field, init, login};
    use keyharbor_core::{ItemSource, ItemStatus, NewItem};
    use keyharbor_domain::{
        policy::{Effect, Resource, Rule},
        ports::{AuditStore, PolicyStore},
        vault::ItemKind,
    };
    use keyharbor_sqlite::SqliteStore;

    fn env_item(name: &str, value: &str) -> NewItem {
        NewItem {
            name: ItemName::parse(name).unwrap(),
            kind: ItemKind::Env,
            fields: vec![field("PASSWORD", value)],
            origins: vec![],
            origin_binding: None,
            status: ItemStatus::Active,
            source: ItemSource::User,
            details: Default::default(),
        }
    }
    fn name(text: &str) -> ItemName {
        ItemName::parse(text).unwrap()
    }
    fn lookup(given: &[&str], allow: &[&str]) -> Lookup {
        Lookup::new(
            "claude",
            given.iter().map(|g| name(g)).collect(),
            allow
                .iter()
                .map(|a| ItemPattern::parse(a).unwrap())
                .collect(),
        )
    }
    fn ask<'a>(token: &'a str, item: &'a str, field: &'a str) -> Ask<'a> {
        Ask { token, item, field }
    }
    fn last_result(store: &SqliteStore) -> (String, String) {
        let last = store.list_audit(None).unwrap().pop().unwrap().event;
        (last.kind.as_str().to_owned(), last.result)
    }

    #[test]
    fn the_run_s_items_are_answered_and_recorded() {
        let e = env();
        let vault = init(&e);
        vault.add(env_item("myapp/dev/db", "KH-SYNTH-dev")).unwrap();
        let store = SqliteStore::open(&e.data).unwrap();
        let l = lookup(&["myapp/dev/db"], &[]);
        let value = l
            .answer(&store, &vault, &ask(l.token(), "myapp/dev/db", "PASSWORD"))
            .unwrap();
        assert_eq!(&*value, "KH-SYNTH-dev");
        assert_eq!(
            last_result(&store),
            ("env_injected".into(), LOOKUP_RESULT.into())
        );
    }

    #[test]
    fn a_request_without_the_token_gets_nothing() {
        let e = env();
        let vault = init(&e);
        vault.add(env_item("app", "KH-SYNTH-app")).unwrap();
        let store = SqliteStore::open(&e.data).unwrap();
        let l = lookup(&["app"], &[]);
        let other = lookup(&["app"], &[]);
        for token in ["", "0000", other.token()] {
            let refusal = l
                .answer(&store, &vault, &ask(token, "app", "PASSWORD"))
                .unwrap_err();
            assert_eq!(refusal, Refusal::Token);
            assert!(!refusal.to_string().contains("KH-SYNTH"));
        }
        assert_eq!(
            last_result(&store),
            ("secret_denied".into(), "bad-token".into())
        );
    }

    #[test]
    fn names_outside_the_run_are_refused_unless_allowed() {
        let e = env();
        let vault = init(&e);
        for item in ["app", "myapp/dev/cache", "myapp/prod/db"] {
            vault.add(env_item(item, "KH-SYNTH-x")).unwrap();
        }
        let store = SqliteStore::open(&e.data).unwrap();
        let l = lookup(&["app"], &["myapp/dev/*"]);
        let answer = |item| l.answer(&store, &vault, &ask(l.token(), item, "PASSWORD"));
        assert!(answer("myapp/dev/cache").is_ok());
        assert_eq!(
            answer("myapp/prod/db").unwrap_err(),
            Refusal::Denied(name("myapp/prod/db"), DenialReason::OutsideRun)
        );
        assert_eq!(
            last_result(&store),
            ("secret_denied".into(), "outside-run".into())
        );
        assert_eq!(answer("bad name!").unwrap_err(), Refusal::BadName);
        assert_eq!(
            l.answer(&store, &vault, &ask(l.token(), "app", "bad field!"))
                .unwrap_err(),
            Refusal::BadName
        );
    }

    #[test]
    fn the_process_chain_still_decides() {
        let e = env();
        let vault = init(&e);
        vault.add(env_item("app", "KH-SYNTH-app")).unwrap();
        vault.add(login("github", "KH-SYNTH-pw")).unwrap();
        let store = SqliteStore::open(&e.data).unwrap();
        let l = lookup(&[], &["*"]);
        let answer = |item, field| l.answer(&store, &vault, &ask(l.token(), item, field));
        assert_eq!(
            answer("github", "password").unwrap_err(),
            Refusal::Denied(name("github"), DenialReason::Origin)
        );
        assert_eq!(
            answer("app", "MISSING").unwrap_err(),
            Refusal::Denied(name("app"), DenialReason::NotFound)
        );
        assert_eq!(
            answer("nope", "PASSWORD").unwrap_err(),
            Refusal::Denied(name("nope"), DenialReason::NotFound)
        );
        // A rule added while the run goes on applies to the next request.
        assert!(answer("app", "PASSWORD").is_ok());
        store
            .policy_set(&Rule::new("claude", Resource::Item, "app", Effect::Deny).unwrap())
            .unwrap();
        let refusal = answer("app", "PASSWORD").unwrap_err();
        assert_eq!(refusal, Refusal::Denied(name("app"), DenialReason::Policy));
        assert_eq!(refusal.to_string(), "denied: app (policy)");
        assert_eq!(
            last_result(&store),
            ("secret_denied".into(), "policy".into())
        );
    }
}

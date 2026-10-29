//! Asking the user for information only they have (08 §8.2 ③): the agent asks and waits,
//! the user answers outside the agent's reach. One implementation for the CLI and MCP.

use crate::{ItemSource, ItemStatus, LOCAL_AGENT, NewItem, Vault};
use anyhow::{Context, Result, bail};
use keyharbor_crypto::random;
use keyharbor_domain::{
    RequestId,
    audit::{AuditEvent, AuditKind},
    ports::{Clock, Store},
    request::{VaultRequest, VaultRequestStatus},
    vault::{Description, FieldName, ItemKind, ItemName, Origin},
};
use zeroize::Zeroizing;

/// What an agent asks for.
#[derive(Clone, Debug)]
pub struct Ask {
    pub kind: ItemKind,
    pub name: ItemName,
    pub reason: Description,
    pub origin: Option<Origin>,
}

/// How the user answers.
pub enum Answer {
    /// New values, stored as an active item under the requested name (the user typed them,
    /// so it is not a draft).
    Values(Vec<(FieldName, Zeroizing<String>)>),
    /// An item already in the vault, of the requested kind.
    Existing(ItemName),
}

fn note(store: &dyn Store, event: AuditEvent) {
    let _ = store.record(&event);
}

/// Records the request and returns its id.
pub fn ask(store: &dyn Store, clock: &dyn Clock, agent: &str, ask: Ask) -> Result<RequestId> {
    let request = VaultRequest {
        id: random::id(),
        agent: agent.to_owned(),
        kind: ask.kind,
        name: ask.name,
        reason: ask.reason,
        origin: ask.origin,
        status: VaultRequestStatus::Pending,
        created_at: clock.unix_seconds(),
        item: None,
    };
    store.vault_request_add(&request)?;
    note(
        store,
        AuditEvent::new(
            agent,
            AuditKind::VaultRequested,
            VaultRequestStatus::Pending.as_str(),
        )
        .item(request.name.as_str())
        .origin(request.origin.as_ref().map(Origin::as_str)),
    );
    Ok(request.id)
}

/// Requests waiting for an answer, oldest first.
pub fn pending(store: &dyn Store, clock: &dyn Clock) -> Result<Vec<VaultRequest>> {
    let now = clock.unix_seconds();
    Ok(store
        .vault_requests_pending()?
        .into_iter()
        .filter(|r| r.status_at(now) == VaultRequestStatus::Pending)
        .collect())
}

/// The request as of now, its status counting expiry.
pub fn status(store: &dyn Store, clock: &dyn Clock, id: RequestId) -> Result<VaultRequest> {
    let mut request = store.vault_request_get(id)?.context("request not found")?;
    request.status = request.status_at(clock.unix_seconds());
    Ok(request)
}

fn open_request(store: &dyn Store, clock: &dyn Clock, id: RequestId) -> Result<VaultRequest> {
    let request = status(store, clock, id)?;
    if request.status != VaultRequestStatus::Pending {
        bail!("request is {}", request.status.as_str());
    }
    Ok(request)
}

/// Answers a pending request and returns the item that answers it.
pub fn fulfill(
    store: &dyn Store,
    vault: &Vault,
    clock: &dyn Clock,
    id: RequestId,
    answer: Answer,
) -> Result<ItemName> {
    let request = open_request(store, clock, id)?;
    let item = match answer {
        Answer::Existing(name) => {
            let found = vault
                .list()?
                .into_iter()
                .find(|item| item.name == name.as_str() && !item.tampered)
                .context("item not found")?;
            if found.kind != request.kind {
                bail!("the request is for a {} item", request.kind.as_str());
            }
            name
        }
        Answer::Values(fields) => {
            if fields.is_empty() {
                bail!("no values given");
            }
            vault
                .add(NewItem {
                    name: request.name.clone(),
                    kind: request.kind,
                    fields,
                    origins: request.origin.clone().into_iter().collect(),
                    origin_binding: None,
                    status: ItemStatus::Active,
                    source: ItemSource::Requested,
                    details: Default::default(),
                })
                .with_context(|| {
                    format!(
                        "could not store {} (to answer with an existing item, name it)",
                        request.name.as_str()
                    )
                })?;
            request.name.clone()
        }
    };
    if !store.vault_request_finish(id, VaultRequestStatus::Fulfilled, Some(&item))? {
        bail!("request was answered meanwhile");
    }
    note(
        store,
        AuditEvent::new(LOCAL_AGENT, AuditKind::VaultRequestAnswered, "fulfilled")
            .item(item.as_str()),
    );
    Ok(item)
}

pub fn decline(store: &dyn Store, clock: &dyn Clock, id: RequestId) -> Result<()> {
    let request = open_request(store, clock, id)?;
    if !store.vault_request_finish(id, VaultRequestStatus::Declined, None)? {
        bail!("request was answered meanwhile");
    }
    note(
        store,
        AuditEvent::new(LOCAL_AGENT, AuditKind::VaultRequestAnswered, "declined")
            .item(request.name.as_str()),
    );
    Ok(())
}

/// One look for [`crate::session::poll`]: `None` while the request is pending, else the
/// answered (or expired) request.
pub fn answered(
    store: &dyn Store,
    clock: &dyn Clock,
    id: RequestId,
) -> Result<Option<VaultRequest>> {
    let request = status(store, clock, id)?;
    Ok((request.status != VaultRequestStatus::Pending).then_some(request))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{env, field, init, login, open_store, pref};
    use keyharbor_domain::ports::ManualClock;
    use std::time::Duration;

    fn card_ask() -> Ask {
        Ask {
            kind: ItemKind::Card,
            name: ItemName::parse("my-visa").unwrap(),
            reason: Description::parse("pay at the shop").unwrap(),
            origin: Some(Origin::parse("https://shop.example").unwrap()),
        }
    }

    #[test]
    fn a_fulfilled_request_stores_an_active_item_under_the_requested_name() {
        let e = env();
        let vault = init(&e);
        let store = open_store(&e.data);
        let clock = ManualClock::at_unix_seconds(1_000);
        let id = ask(&store, &clock, "codex", card_ask()).unwrap();
        assert_eq!(pending(&store, &clock).unwrap()[0].id, id);
        assert_eq!(answered(&store, &clock, id).unwrap(), None);

        let item = fulfill(
            &store,
            &vault,
            &clock,
            id,
            Answer::Values(vec![field("number", "4111111111111111")]),
        )
        .unwrap();
        assert_eq!(item.as_str(), "my-visa");
        assert_eq!(
            *vault.resolve(&pref("my-visa", "number")).unwrap(),
            "4111111111111111"
        );
        let stored = vault.list().unwrap().into_iter().next().unwrap();
        assert_eq!(
            (stored.status, stored.source),
            (ItemStatus::Active, ItemSource::Requested)
        );
        assert_eq!(stored.origins, ["https://shop.example"]);
        let done = answered(&store, &clock, id).unwrap().unwrap();
        assert_eq!(
            (done.status, done.item),
            (VaultRequestStatus::Fulfilled, Some(item))
        );
        assert!(pending(&store, &clock).unwrap().is_empty());
        assert!(decline(&store, &clock, id).is_err());
    }

    #[test]
    fn an_existing_item_of_the_requested_kind_can_answer() {
        let e = env();
        let vault = init(&e);
        vault.add(login("github", "pw")).unwrap();
        let store = open_store(&e.data);
        let clock = ManualClock::at_unix_seconds(1_000);
        let id = ask(&store, &clock, "codex", card_ask()).unwrap();
        let github = ItemName::parse("github").unwrap();
        assert!(fulfill(&store, &vault, &clock, id, Answer::Existing(github)).is_err());
        let mut login_ask = card_ask();
        login_ask.kind = ItemKind::Login;
        let id = ask(&store, &clock, "codex", login_ask).unwrap();
        let github = ItemName::parse("github").unwrap();
        assert_eq!(
            fulfill(&store, &vault, &clock, id, Answer::Existing(github.clone())).unwrap(),
            github
        );
    }

    #[test]
    fn a_declined_or_old_request_cannot_be_answered() {
        let e = env();
        let vault = init(&e);
        let store = open_store(&e.data);
        let clock = ManualClock::at_unix_seconds(1_000);
        let declined = ask(&store, &clock, "codex", card_ask()).unwrap();
        decline(&store, &clock, declined).unwrap();
        assert_eq!(
            answered(&store, &clock, declined).unwrap().unwrap().status,
            VaultRequestStatus::Declined
        );
        let old = ask(&store, &clock, "codex", card_ask()).unwrap();
        clock.advance(Duration::from_secs(VaultRequest::TTL_SECONDS));
        assert_eq!(
            answered(&store, &clock, old).unwrap().unwrap().status,
            VaultRequestStatus::Expired
        );
        assert!(pending(&store, &clock).unwrap().is_empty());
        let late = fulfill(
            &store,
            &vault,
            &clock,
            old,
            Answer::Values(vec![field("number", "1")]),
        );
        assert_eq!(late.unwrap_err().to_string(), "request is expired");
        assert!(vault.list().unwrap().is_empty());
    }
}

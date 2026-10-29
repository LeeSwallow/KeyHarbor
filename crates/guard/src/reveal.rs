//! A person looking at a value in the admin app (09 §9.4: "보기", "복사"). This is the one
//! plaintext path that ends on a screen, so it is here, where plaintext is granted: the item
//! must exist and verify (a tampered item shows nothing), and every reveal is recorded.
//! Agents never reach it: neither the MCP server nor the control socket calls it.

use crate::audit;
use anyhow::{Context, Result};
use keyharbor_core::{PlaintextAccess, Vault};
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    ports::Store,
    vault::{FieldName, ItemName, PlaceholderRef},
};
use zeroize::Zeroizing;

/// Who the usage record names for a reveal: the person at the app.
pub const REVEALER: &str = "user";

/// The value of `item.field`, for a person to see; records `value_revealed`.
pub fn reveal(
    store: &dyn Store,
    vault: &Vault,
    item: &ItemName,
    field: &FieldName,
) -> Result<Zeroizing<String>> {
    vault
        .item_use(item)
        .context("the item failed verification; it is not shown")?
        .context("item not found")?;
    let reference = PlaceholderRef {
        item: item.clone(),
        field: field.clone(),
    };
    let value = vault.resolve(&PlaintextAccess::issue(), &reference)?;
    audit::record(
        store,
        &AuditEvent::new(REVEALER, AuditKind::ValueRevealed, "shown").item(item.as_str()),
    );
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{env, init, login};
    use keyharbor_domain::ports::AuditStore;
    use keyharbor_sqlite::SqliteStore;

    #[test]
    fn a_reveal_shows_the_value_and_is_recorded() {
        let e = env();
        let vault = init(&e);
        vault.add(login("github", "pw-1234")).unwrap();
        let store = SqliteStore::open(&e.data).unwrap();
        let name = ItemName::parse("github").unwrap();
        let field = FieldName::parse("password").unwrap();
        assert_eq!(*reveal(&store, &vault, &name, &field).unwrap(), "pw-1234");
        let events = store.list_audit(None).unwrap();
        let last = &events.last().unwrap().event;
        assert_eq!(
            (last.kind, last.agent.as_str()),
            (AuditKind::ValueRevealed, "user")
        );
        assert!(!format!("{events:?}").contains("pw-1234"));
        let missing = ItemName::parse("nope").unwrap();
        assert!(reveal(&store, &vault, &missing, &field).is_err());
    }

    #[test]
    fn a_tampered_item_shows_nothing() {
        let e = env();
        let vault = init(&e);
        vault.add(login("github", "pw-1234")).unwrap();
        let store = SqliteStore::open(&e.data).unwrap();
        store.execute_for_tests("UPDATE vault_items SET origin_binding = 0");
        let name = ItemName::parse("github").unwrap();
        let field = FieldName::parse("password").unwrap();
        assert!(reveal(&store, &vault, &name, &field).is_err());
    }
}

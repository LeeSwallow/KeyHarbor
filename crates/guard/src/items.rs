//! Adding and changing items from an interface, in the one safe order: descriptions are
//! checked for vault values first (`describe`), then what the change weakens is confirmed
//! by a person (R-35), and only then is anything stored.

use crate::describe;
use anyhow::Result;
use keyharbor_core::{ItemChange, NewItem, Vault, weaken};
use keyharbor_domain::{ports::Confirm, vault::ItemName};

/// Adds `item`; `texts` are the descriptions given with it, as typed.
pub fn add(vault: &Vault, item: NewItem, texts: &[&str]) -> Result<()> {
    describe::refuse_values(vault, &item.fields, texts)?;
    vault.add(item)
}

/// Changes the item `name`; `texts` are the descriptions given with the change, as typed.
pub fn edit(
    vault: &Vault,
    confirm: &dyn Confirm,
    name: &ItemName,
    change: ItemChange,
    texts: &[&str],
) -> Result<()> {
    describe::refuse_values(vault, &change.set_fields, texts)?;
    weaken::require(confirm, &vault.weakenings(name, &change)?)?;
    vault.edit(name, change)
}

//! Adding and changing items from an interface, in the one safe order: descriptions are
//! checked for vault values first (`describe`), then what the change weakens is confirmed
//! by a person (R-35), and only then is anything stored. What is stored comes back as the
//! warnings for secret values masking handles badly (J-230); they never carry a value.

use crate::describe;
use anyhow::Result;
use keyharbor_core::{ItemChange, NewItem, Vault, weaken};
use keyharbor_domain::{
    ports::Confirm,
    vault::{CommonValueWarning, FieldName, ItemName, common_value_warnings},
};

/// Adds `item`; `texts` are the descriptions given with it, as typed.
pub fn add(vault: &Vault, item: NewItem, texts: &[&str]) -> Result<Vec<CommonValueWarning>> {
    describe::refuse_values(vault, &item.fields, texts)?;
    let warnings = common_value_warnings(
        &item.name,
        item.kind,
        &item.fields,
        &item.details.plain_fields,
    );
    vault.add(item)?;
    Ok(warnings)
}

/// Changes the item `name`; `texts` are the descriptions given with the change, as typed.
pub fn edit(
    vault: &Vault,
    confirm: &dyn Confirm,
    name: &ItemName,
    change: ItemChange,
    texts: &[&str],
) -> Result<Vec<CommonValueWarning>> {
    describe::refuse_values(vault, &change.set_fields, texts)?;
    weaken::require(confirm, &vault.weakenings(name, &change)?)?;
    let warnings = edit_warnings(vault, name, &change)?;
    vault.edit(name, change)?;
    Ok(warnings)
}

/// The warnings for the values `change` sets, against the fields that stay secret after it:
/// the item's plain fields plus those it marks plain, less those it marks secret again.
fn edit_warnings(
    vault: &Vault,
    name: &ItemName,
    change: &ItemChange,
) -> Result<Vec<CommonValueWarning>> {
    if change.set_fields.is_empty() {
        return Ok(Vec::new());
    }
    let Some(item) = vault.list()?.into_iter().find(|i| i.name == name.as_str()) else {
        return Ok(Vec::new());
    };
    let plain: Vec<FieldName> = item
        .plain_values
        .keys()
        .filter_map(|f| FieldName::parse(f).ok())
        .chain(change.make_plain.iter().cloned())
        .filter(|f| !change.make_secret.contains(f))
        .collect();
    Ok(common_value_warnings(
        name,
        item.kind,
        &change.set_fields,
        &plain,
    ))
}

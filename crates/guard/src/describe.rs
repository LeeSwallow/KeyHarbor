//! Descriptions are plaintext that agents read, so one that holds a vault value — typed by
//! mistake — would leak it (R-38). Before a description is stored, the masker of the whole
//! vault (plus the values being stored with it) must find nothing in it.

use crate::pep::mask::{Masker, masker};
use anyhow::{Result, bail};
use keyharbor_core::Vault;
use keyharbor_domain::vault::{FieldName, ItemName, PlaceholderRef};
use zeroize::Zeroizing;

/// Fails if any of `texts` contains a value of the vault or one of `new_values` (values not
/// stored yet), in any form the masker recognizes. The error never names the value.
pub fn refuse_values(
    vault: &Vault,
    new_values: &[(FieldName, Zeroizing<String>)],
    texts: &[&str],
) -> Result<()> {
    if texts.is_empty() {
        return Ok(());
    }
    let stored = masker(vault)?;
    let unstored = Masker::build(new_values.iter().map(|(field, value)| {
        let reference = PlaceholderRef {
            item: ItemName::parse("new").expect("valid name"),
            field: field.clone(),
        };
        (reference, value.clone())
    }));
    let holds_value =
        |text: &&str| stored.redact(text) != **text || unstored.redact(text) != **text;
    if texts.iter().any(holds_value) {
        bail!("a description must not contain a vault value");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{env, field, init, login};

    #[test]
    fn descriptions_holding_a_value_are_refused() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "KH-SYNTH-pw-88")).unwrap();
        refuse_values(&v, &[], &["Work account for CI"]).unwrap();
        let err = refuse_values(&v, &[], &["pw is KH-SYNTH-pw-88"]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "a description must not contain a vault value"
        );
        let new = [field("value", "KH-SYNTH-new-88")];
        assert!(refuse_values(&v, &new, &["key KH-SYNTH-new-88"]).is_err());
        assert!(refuse_values(&v, &new, &["KH%2DSYNTH"]).is_ok());
    }
}

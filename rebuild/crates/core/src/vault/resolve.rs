//! The plaintext paths. Both demand a [`PlaintextAccess`], which only the guard (after its
//! checks) and core's export obtain; values only ever land in `Zeroizing` buffers.

use super::{MaskValues, Vault, vault_err};
use crate::PlaintextAccess;
use anyhow::{Context, Result, anyhow};
use keyharbor_domain::vault::{
    Domain, FieldName, ItemKind, ItemName, PlaceholderRef, parse_placeholders,
};
use keyharbor_vault::record::{FieldAd, open_field};
use zeroize::Zeroizing;

impl Vault {
    /// Decrypts one field after verifying the item's metadata. Only the guard (and export)
    /// holds the capability this needs.
    pub fn resolve(
        &self,
        _access: &PlaintextAccess,
        reference: &PlaceholderRef,
    ) -> Result<Zeroizing<String>> {
        let (row, fields) = self
            .store
            .item_get(reference.item.as_str())?
            .context("item not found")?;
        let (kind, _, _) = self.verified(&row)?;
        let field = fields
            .iter()
            .find(|f| f.field == reference.field.as_str())
            .context("field not found")?;
        let domain = kind.domain();
        let ad = FieldAd {
            vault: &self.vault_id,
            domain,
            item: &row.id,
            field: &field.field,
        };
        let bytes =
            open_field(&self.domain_keys[&domain].1, &ad, &field.sealed).map_err(vault_err)?;
        let text = std::str::from_utf8(&bytes).context("stored field is not UTF-8")?;
        Ok(Zeroizing::new(text.to_owned()))
    }

    /// Every field value in the vault, whatever its status or policy (an item an agent may not
    /// use must not leak either), with the version they were read at — for building the
    /// guard's masker. Fails if any value cannot be decrypted, so callers can withhold
    /// responses rather than send them unmasked.
    pub fn mask_values(&self, _access: &PlaintextAccess) -> Result<MaskValues> {
        let version = self.version()?;
        let mut values = Vec::new();
        for row in self.store.item_list()? {
            let item = ItemName::parse(&row.name).map_err(|e| anyhow!("{e}"))?;
            let domain = Domain::from_byte(row.domain).context("unknown domain in database")?;
            let (_, fields) = self
                .store
                .item_get(&row.name)?
                .context("item changed while masking")?;
            // An unknown kind is masked in full: masking errs on the side of hiding.
            let kind = ItemKind::parse(&row.kind);
            for field in fields {
                if kind
                    .and_then(|k| k.field(&field.field))
                    .is_some_and(|spec| !spec.maskable)
                {
                    continue;
                }
                let ad = FieldAd {
                    vault: &self.vault_id,
                    domain,
                    item: &row.id,
                    field: &field.field,
                };
                let bytes = open_field(&self.domain_keys[&domain].1, &ad, &field.sealed)
                    .map_err(|e| anyhow!("{e}"))
                    .context("could not decrypt a value for masking")?;
                let text = Zeroizing::new(
                    std::str::from_utf8(&bytes)
                        .context("stored field is not UTF-8")?
                        .to_owned(),
                );
                values.push((
                    PlaceholderRef {
                        item: item.clone(),
                        field: FieldName::parse(&field.field).map_err(|e| anyhow!("{e}"))?,
                    },
                    text,
                ));
            }
        }
        Ok(MaskValues { version, values })
    }
}

/// Replaces every `{{secret:item.field}}` in `template` with `resolve(reference)`.
/// The result is allocated once at its final size, so no partial copy is left behind.
pub fn fill_template(
    template: &str,
    mut resolve: impl FnMut(&PlaceholderRef) -> Result<Zeroizing<String>>,
) -> Result<Zeroizing<String>> {
    let refs = parse_placeholders(template).map_err(|e| anyhow!("{e}"))?;
    let values = refs.iter().map(&mut resolve).collect::<Result<Vec<_>>>()?;
    let size = template.len() + values.iter().map(|v| v.len()).sum::<usize>();
    let mut out = Zeroizing::new(String::with_capacity(size));
    let mut rest = template;
    for value in &values {
        let start = rest.find("{{secret:").expect("parsed placeholder");
        let end = start + rest[start..].find("}}").expect("parsed placeholder") + 2;
        out.push_str(&rest[..start]);
        out.push_str(value);
        rest = &rest[end..];
    }
    out.push_str(rest);
    debug_assert_eq!(out.capacity(), size);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{env, init, login, pref};

    #[test]
    fn templates_substitute_in_order() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "KH-SYNTH-pw")).unwrap();
        let out = fill_template(
            "u={{secret:github.username}}&p={{secret:github.password}}!",
            |r| v.resolve(r),
        )
        .unwrap();
        assert_eq!(&*out, "u=me@x.com&p=KH-SYNTH-pw!");
        assert_eq!(&*fill_template("plain", |r| v.resolve(r)).unwrap(), "plain");
        let mut seen = Vec::new();
        fill_template("{{secret:a.b}}{{secret:c.d}}", |r| {
            seen.push(r.clone());
            Ok(Zeroizing::new(String::new()))
        })
        .unwrap();
        assert_eq!(seen, vec![pref("a", "b"), pref("c", "d")]);
        assert!(fill_template("{{secret:github.note}}", |r| v.resolve(r)).is_err());
        assert!(fill_template("{{secret:github}}", |r| v.resolve(r)).is_err());
    }
}

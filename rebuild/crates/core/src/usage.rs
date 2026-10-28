//! What the broker and `keyharbor run` need to use an item: its verified metadata, and
//! placeholder substitution into a template. Values only ever land in `Zeroizing` buffers.

use crate::{ItemStatus, Vault};
use anyhow::{Context, Result, anyhow};
use keyharbor_domain::vault::{
    Domain, ItemKind, ItemName, Origin, PlaceholderRef, parse_placeholders,
};
use zeroize::Zeroizing;

/// Verified, value-free facts about an item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemUse {
    pub kind: ItemKind,
    pub domain: Domain,
    pub origin_binding: bool,
    pub origins: Vec<Origin>,
    pub status: ItemStatus,
    pub fields: Vec<String>,
}

impl Vault {
    /// `Ok(None)` if there is no such item; an error if its metadata fails verification.
    pub fn item_use(&self, name: &ItemName) -> Result<Option<ItemUse>> {
        let Some((row, fields)) = self.store.item_get(name.as_str())? else {
            return Ok(None);
        };
        let (kind, _, origins) = self.verified(&row)?;
        Ok(Some(ItemUse {
            kind,
            domain: kind.domain(),
            origin_binding: row.origin_binding,
            origins,
            status: ItemStatus::parse(&row.status).context("unknown status in database")?,
            fields: fields.into_iter().map(|f| f.field).collect(),
        }))
    }

    /// The vault's change counter; it moves on every write and key rotation.
    pub fn version(&self) -> Result<i64> {
        self.store.vault_version()
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
    fn item_use_reports_verified_metadata() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "pw")).unwrap();
        let used = v
            .item_use(&ItemName::parse("github").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(used.kind, ItemKind::Login);
        assert_eq!(used.domain, Domain::Credentials);
        assert!(used.origin_binding);
        assert_eq!(
            used.origins,
            vec![Origin::parse("https://github.com").unwrap()]
        );
        assert_eq!(used.status, ItemStatus::Active);
        assert_eq!(used.fields, vec!["password", "username"]);
        assert!(
            v.item_use(&ItemName::parse("nope").unwrap())
                .unwrap()
                .is_none()
        );
        let before = v.version().unwrap();
        v.remove(&ItemName::parse("github").unwrap()).unwrap();
        assert_eq!(v.version().unwrap(), before + 1);
    }

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

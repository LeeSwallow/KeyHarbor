//! Items an agent adds without seeing a value (08 §8.2): a generated password or values it
//! put. They are drafts until a person or a sign-in confirms them, and they never replace an
//! existing item: a taken name gets the next free `<name>~draft-N` (08 §8.3).

use super::{ItemDetails, ItemSource, ItemStatus, NewItem, Vault};
use anyhow::{Result, bail};
use keyharbor_crypto::random;
use keyharbor_domain::{
    password::PasswordRule,
    vault::{FieldName, ItemKind, ItemName, Origin},
};
use zeroize::Zeroizing;

/// Drafts of one name an agent may pile up before a person sorts them out.
pub const MAX_DRAFTS: u32 = 20;

/// An item an agent puts: chosen name, kind, values and origins.
pub struct DraftItem {
    pub name: ItemName,
    pub kind: ItemKind,
    pub fields: Vec<(FieldName, Zeroizing<String>)>,
    pub origins: Vec<Origin>,
    pub source: ItemSource,
}

/// A login whose password KeyHarbor makes up.
pub struct GeneratedLogin {
    pub name: ItemName,
    pub origin: Origin,
    pub username: Option<Zeroizing<String>>,
    pub rule: PasswordRule,
}

impl Vault {
    /// Stores `item` as a draft added by `agent` and returns the name it got: `item.name`
    /// if free, else the first free `~draft-N`. Nothing existing is changed.
    pub fn add_draft(&self, item: DraftItem, agent: &str) -> Result<ItemName> {
        if item.name.is_draft() {
            bail!("a name with ~draft- is reserved for drafts");
        }
        let name = self.free_draft_name(&item.name)?;
        let domain = item.kind.domain();
        let new = NewItem {
            name: name.clone(),
            kind: item.kind,
            fields: item.fields,
            origins: item.origins,
            origin_binding: None,
            status: ItemStatus::Draft,
            source: item.source,
            details: ItemDetails::default(),
        };
        self.insert(new, domain, agent)?;
        Ok(name)
    }

    /// A draft login with a new password following `login.rule`; the value never leaves.
    pub fn generate_login(&self, login: GeneratedLogin, agent: &str) -> Result<ItemName> {
        let mut fields = vec![(FieldName::parse("password")?, random::password(&login.rule))];
        if let Some(username) = login.username {
            fields.push((FieldName::parse("username")?, username));
        }
        self.add_draft(
            DraftItem {
                name: login.name,
                kind: ItemKind::Login,
                fields,
                origins: vec![login.origin],
                source: ItemSource::Generated,
            },
            agent,
        )
    }

    fn free_draft_name(&self, name: &ItemName) -> Result<ItemName> {
        let taken = |candidate: &ItemName| -> Result<bool> {
            Ok(self.store.item_get(candidate.as_str())?.is_some())
        };
        if !taken(name)? {
            return Ok(name.clone());
        }
        for n in 1..=MAX_DRAFTS {
            let draft = name.draft(n);
            if !taken(&draft)? {
                return Ok(draft);
            }
        }
        bail!(
            "{} already has {MAX_DRAFTS} drafts; confirm or discard some",
            name.as_str()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{env, field, init, login, pref};

    fn put(name: &str, password: &str) -> DraftItem {
        DraftItem {
            name: ItemName::parse(name).unwrap(),
            kind: ItemKind::Login,
            fields: vec![field("username", "me@x.com"), field("password", password)],
            origins: vec![Origin::parse("https://github.com").unwrap()],
            source: ItemSource::Agent,
        }
    }

    #[test]
    fn a_put_with_a_taken_name_becomes_a_draft_and_keeps_the_original() {
        let e = env();
        let vault = init(&e);
        vault.add(login("github", "original")).unwrap();
        let first = vault
            .add_draft(put("github", "injected"), "claude")
            .unwrap();
        let second = vault.add_draft(put("github", "again"), "claude").unwrap();
        assert_eq!(first.as_str(), "github~draft-1");
        assert_eq!(second.as_str(), "github~draft-2");
        assert_eq!(
            *vault.resolve(&pref("github", "password")).unwrap(),
            "original"
        );
        assert_eq!(
            *vault.resolve(&pref("github~draft-1", "password")).unwrap(),
            "injected"
        );
        let draft = vault
            .list()
            .unwrap()
            .into_iter()
            .find(|item| item.name == "github~draft-2")
            .unwrap();
        assert_eq!(
            (draft.status, draft.source),
            (ItemStatus::Draft, ItemSource::Agent)
        );
    }

    #[test]
    fn a_free_name_is_used_as_is_and_draft_names_cannot_be_put() {
        let e = env();
        let vault = init(&e);
        assert_eq!(
            vault
                .add_draft(put("blog", "v"), "claude")
                .unwrap()
                .as_str(),
            "blog"
        );
        let mut forged = put("blog", "v");
        forged.name = ItemName::parse("blog~draft-1").unwrap();
        assert!(vault.add_draft(forged, "claude").is_err());
        let mut active = login("x", "v");
        active.name = ItemName::parse("x~draft-1").unwrap();
        assert!(vault.add(active).is_err());
    }

    #[test]
    fn drafts_of_one_name_are_capped() {
        let e = env();
        let vault = init(&e);
        vault.add_draft(put("a", "v"), "claude").unwrap();
        for _ in 0..MAX_DRAFTS {
            vault.add_draft(put("a", "v"), "claude").unwrap();
        }
        assert!(vault.add_draft(put("a", "v"), "claude").is_err());
    }

    #[test]
    fn a_generated_login_follows_the_rule_and_is_usable_by_placeholder() {
        let e = env();
        let vault = init(&e);
        let name = vault
            .generate_login(
                GeneratedLogin {
                    name: ItemName::parse("shop").unwrap(),
                    origin: Origin::parse("https://shop.example").unwrap(),
                    username: Some(Zeroizing::new("me".into())),
                    rule: PasswordRule::new(24, false, "").unwrap(),
                },
                "claude",
            )
            .unwrap();
        let password = vault.resolve(&pref(name.as_str(), "password")).unwrap();
        assert_eq!(password.len(), 24);
        assert!(password.chars().all(|c| c.is_ascii_alphanumeric()));
        let item = vault.list().unwrap().into_iter().next().unwrap();
        assert_eq!(
            (item.status, item.source),
            (ItemStatus::Draft, ItemSource::Generated)
        );
        assert_eq!(item.origins, ["https://shop.example"]);
    }
}

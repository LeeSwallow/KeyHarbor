//! Items an agent adds without seeing a value (08 §8.2): a generated password or values it
//! put. They are drafts until a person or a sign-in confirms them, and they never replace an
//! existing item: a taken name gets the next free `<name>~draft-N` (08 §8.3).

use crate::vault::{ItemDetails, ItemSource, ItemStatus, NewItem, Vault};
use anyhow::{Result, bail};
use keyharbor_crypto::random;
use keyharbor_domain::{
    password::PasswordRule,
    settings::MAX_DRAFTS,
    vault::{DRAFT_MARK, FieldName, ItemKind, ItemName, Origin},
};
use zeroize::Zeroizing;

/// A new password following `rule`, for an interface that makes one for a person (the admin
/// app's "비밀번호 생성"). Interfaces get randomness here, not from the crypto crate.
pub fn generate_password(rule: &PasswordRule) -> Zeroizing<String> {
    random::password(rule)
}

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

/// A generated login draft: the name it got and its fields, in the order an agent types them
/// (`username` when one was given, then `password`). Never a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedDraft {
    pub name: ItemName,
    pub fields: Vec<FieldName>,
}

impl Vault {
    /// Stores `item` as a draft added by `agent` and returns the name it got: `item.name`
    /// if free, else the first free `~draft-N`. Nothing existing is changed.
    pub fn add_draft(&self, item: DraftItem, agent: &str) -> Result<ItemName> {
        if item.name.is_draft() {
            bail!("a name with {DRAFT_MARK} is reserved for drafts");
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
    pub fn generate_login(&self, login: GeneratedLogin, agent: &str) -> Result<GeneratedDraft> {
        let password = FieldName::parse("password")?;
        let username = FieldName::parse("username")?;
        let mut fields = vec![(password.clone(), random::password(&login.rule))];
        let mut typed = vec![password];
        if let Some(value) = login.username {
            fields.push((username.clone(), value));
            typed.insert(0, username);
        }
        let name = self.add_draft(
            DraftItem {
                name: login.name,
                kind: ItemKind::Login,
                fields,
                origins: vec![login.origin],
                source: ItemSource::Generated,
            },
            agent,
        )?;
        Ok(GeneratedDraft {
            name,
            fields: typed,
        })
    }

    fn free_draft_name(&self, name: &ItemName) -> Result<ItemName> {
        let taken = |candidate: &ItemName| -> Result<bool> {
            Ok(self.store.item_get(candidate.as_str())?.is_some())
        };
        if !taken(name)? {
            return Ok(name.clone());
        }
        let max = self.max_drafts()?;
        for n in 1..=max {
            let draft = name.draft(n);
            if !taken(&draft)? {
                return Ok(draft);
            }
        }
        bail!(
            "{} already has {max} drafts; confirm or discard some",
            name.as_str()
        )
    }

    /// The `max_drafts` setting: drafts of one name an agent may pile up.
    fn max_drafts(&self) -> Result<u32> {
        let max = crate::settings::number(&*self.store, MAX_DRAFTS)?;
        // The setting's range (1..=100) fits; a wider one would be a domain change.
        Ok(u32::try_from(max)?)
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
        for _ in 0..20 {
            vault.add_draft(put("a", "v"), "claude").unwrap();
        }
        assert_eq!(
            vault
                .add_draft(put("a", "v"), "claude")
                .unwrap_err()
                .to_string(),
            "a already has 20 drafts; confirm or discard some",
            "20 by default"
        );
    }

    #[test]
    fn the_draft_cap_follows_the_setting() {
        use keyharbor_domain::ports::SettingsStore;
        let e = env();
        let vault = init(&e);
        crate::tests::open_store(&e.data)
            .setting_set(MAX_DRAFTS, "2")
            .unwrap();
        for _ in 0..3 {
            vault.add_draft(put("a", "v"), "claude").unwrap();
        }
        assert!(vault.add_draft(put("a", "v"), "claude").is_err());
    }

    #[test]
    fn a_generated_login_follows_the_rule_and_is_usable_by_placeholder() {
        let e = env();
        let vault = init(&e);
        let draft = vault
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
        let typed: Vec<&str> = draft.fields.iter().map(FieldName::as_str).collect();
        assert_eq!(
            typed,
            ["username", "password"],
            "the order an agent types them"
        );
        let password = vault
            .resolve(&pref(draft.name.as_str(), "password"))
            .unwrap();
        assert_eq!(password.len(), 24);
        assert!(password.chars().all(|c| c.is_ascii_alphanumeric()));
        let item = vault.list().unwrap().into_iter().next().unwrap();
        assert_eq!(
            (item.status, item.source),
            (ItemStatus::Draft, ItemSource::Generated)
        );
        assert_eq!(item.origins, ["https://shop.example"]);
    }

    #[test]
    fn a_generated_login_without_a_username_has_only_a_password_field() {
        let e = env();
        let vault = init(&e);
        let draft = vault
            .generate_login(
                GeneratedLogin {
                    name: ItemName::parse("shop").unwrap(),
                    origin: Origin::parse("https://shop.example").unwrap(),
                    username: None,
                    rule: PasswordRule::default(),
                },
                "claude",
            )
            .unwrap();
        let typed: Vec<&str> = draft.fields.iter().map(FieldName::as_str).collect();
        assert_eq!(typed, ["password"]);
    }
}

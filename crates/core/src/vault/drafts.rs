//! Settling drafts (08 §8.3): confirm one (a numbered draft replaces the item it was made
//! for), discard one, prune old ones, and pick the login draft a sign-in confirms.

use super::{ItemSource, ItemStatus, Vault};
use crate::LOCAL_AGENT;
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::{
    date::{Date, Ttl},
    settings::{DRAFT_TTL, NO_TTL, SettingSpec},
    vault::{ItemKind, ItemName, Origin},
};

/// Draft sources a sign-in may confirm on its own: values KeyHarbor made or the user typed.
/// A draft the agent put is left for a person, so an agent cannot swap a password in by
/// putting one and then saying it signed in (J-79).
const SELF_CONFIRMING: [ItemSource; 2] = [ItemSource::Generated, ItemSource::Captured];

impl Vault {
    /// Makes the draft `name` active and returns the name it now has: a numbered draft
    /// (`github~draft-2`) replaces `github` in one write; a draft with its own name keeps it.
    pub fn confirm(&self, name: &ItemName, agent: &str) -> Result<ItemName> {
        let (mut row, fields) = self
            .store
            .item_get(name.as_str())?
            .context("item not found")?;
        let (kind, name, origins) = self.verified(&row)?;
        if row.status != ItemStatus::Draft.as_str() {
            bail!("{} is not a draft", name.as_str());
        }
        let target = name.base();
        row.name = target.as_str().to_owned();
        row.status = ItemStatus::Active.as_str().to_owned();
        row.updated_at = self.clock.now_rfc3339();
        row.meta_mac = self.mac_for(&row, kind, &target, &origins);
        let guard = self.guard(kind.domain());
        if name.is_draft() {
            self.store
                .item_replace(&guard, target.as_str(), &row, &fields)?;
        } else {
            self.store.item_update(&guard, &row, &fields)?;
        }
        self.note_change_by(agent, &target, "confirmed");
        Ok(target)
    }

    /// Marks a draft discarded: it can no longer be used and `prune_drafts` removes it.
    pub fn discard(&self, name: &ItemName) -> Result<()> {
        let summary = self
            .list()?
            .into_iter()
            .find(|item| item.name == name.as_str())
            .context("item not found")?;
        if summary.status != ItemStatus::Draft {
            bail!("{} is not a draft", name.as_str());
        }
        self.edit(
            name,
            super::ItemChange {
                status: Some(ItemStatus::Discarded),
                ..Default::default()
            },
        )
    }

    /// Removes discarded items and drafts last changed more than `draft_ttl` ago; returns
    /// their names.
    pub fn prune_drafts(&self) -> Result<Vec<ItemName>> {
        let ttl = self.draft_ttl()?;
        let today = self.clock.today();
        let mut removed = Vec::new();
        for item in self.list()? {
            let stale = item.status == ItemStatus::Draft
                && ttl.is_some_and(|ttl| {
                    Date::parse(item.updated_at.get(..10).unwrap_or(""))
                        .is_ok_and(|changed| ttl.from(changed).is_expired(today))
                });
            if stale || item.status == ItemStatus::Discarded {
                let name = ItemName::parse(&item.name).map_err(|e| anyhow!("{e}"))?;
                if self.store.item_delete(&item.name)? {
                    self.note_change_by(LOCAL_AGENT, &name, "pruned");
                    removed.push(name);
                }
            }
        }
        Ok(removed)
    }

    fn draft_ttl(&self) -> Result<Option<Ttl>> {
        let spec = SettingSpec::find(DRAFT_TTL).expect("draft_ttl is registered");
        let value = self.store.setting_get(DRAFT_TTL)?;
        match value.as_deref().unwrap_or(spec.default) {
            NO_TTL => Ok(None),
            ttl => Ttl::parse(ttl)
                .map(Some)
                .map_err(|e| anyhow!("{DRAFT_TTL}: {e}")),
        }
    }

    /// The login draft a sign-in at `origin` confirms: `named` if it is one, else the most
    /// recently changed one for that origin. Only generated or captured drafts qualify.
    pub fn login_draft_for(
        &self,
        origin: &Origin,
        named: Option<&ItemName>,
    ) -> Result<Option<ItemName>> {
        let mut candidates: Vec<_> = self
            .list()?
            .into_iter()
            .filter(|item| {
                !item.tampered
                    && item.kind == ItemKind::Login
                    && item.status == ItemStatus::Draft
                    && SELF_CONFIRMING.contains(&item.source)
                    && item.origins.iter().any(|o| o == origin.as_str())
            })
            .collect();
        if let Some(named) = named {
            candidates.retain(|item| item.name == named.as_str());
        }
        candidates.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
        candidates
            .pop()
            .map(|item| ItemName::parse(&item.name).map_err(|e| anyhow!("{e}")))
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DraftItem, GeneratedLogin,
        tests::{env, field, init, login, open_store, pref},
    };
    use keyharbor_domain::{
        password::PasswordRule,
        ports::{ManualClock, SettingsStore},
    };
    use keyharbor_keystore::FileKeyStore;
    use std::{sync::Arc, time::Duration};

    fn name(value: &str) -> ItemName {
        ItemName::parse(value).unwrap()
    }
    fn github() -> Origin {
        Origin::parse("https://github.com").unwrap()
    }
    fn put(vault: &Vault, value: &str, source: ItemSource) -> ItemName {
        vault
            .add_draft(
                DraftItem {
                    name: name("github"),
                    kind: ItemKind::Login,
                    fields: vec![field("password", value)],
                    origins: vec![github()],
                    source,
                },
                "claude",
            )
            .unwrap()
    }

    #[test]
    fn confirming_a_numbered_draft_replaces_the_item_it_was_made_for() {
        let e = env();
        let vault = init(&e);
        vault.add(login("github", "old")).unwrap();
        let draft = put(&vault, "new", ItemSource::Captured);
        assert_eq!(vault.confirm(&draft, "cli").unwrap(), name("github"));
        assert_eq!(*vault.resolve(&pref("github", "password")).unwrap(), "new");
        let items = vault.list().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].status, ItemStatus::Active);
        assert!(!items[0].tampered);
        assert!(vault.confirm(&name("github"), "cli").is_err());
    }

    #[test]
    fn a_draft_with_its_own_name_just_becomes_active() {
        let e = env();
        let vault = init(&e);
        let draft = put(&vault, "v", ItemSource::Agent);
        assert_eq!(draft, name("github"));
        vault.confirm(&draft, "cli").unwrap();
        assert_eq!(vault.list().unwrap()[0].status, ItemStatus::Active);
    }

    #[test]
    fn discarded_and_stale_drafts_are_pruned_and_active_items_are_not() {
        let e = env();
        let vault = init(&e);
        vault.add(login("github", "old")).unwrap();
        let discarded = put(&vault, "a", ItemSource::Agent);
        let kept = put(&vault, "b", ItemSource::Agent);
        vault.discard(&discarded).unwrap();
        assert!(vault.resolve(&pref(discarded.as_str(), "password")).is_ok());
        assert!(vault.discard(&name("github")).is_err());
        assert_eq!(vault.prune_drafts().unwrap(), [discarded]);
        let names: Vec<_> = vault.list().unwrap().into_iter().map(|i| i.name).collect();
        assert_eq!(names, ["github", kept.as_str()]);
    }

    #[test]
    fn drafts_older_than_draft_ttl_are_pruned_unless_it_is_none() {
        let e = env();
        let clock = Arc::new(ManualClock::at_unix_seconds(1_793_203_129));
        let vault = Vault::init(
            open_store(&e.data),
            Box::new(FileKeyStore::new(e.keys.clone())),
            clock.clone(),
        )
        .unwrap();
        let draft = put(&vault, "v", ItemSource::Agent);
        let store = open_store(&e.data);
        store.setting_set(DRAFT_TTL, "2d").unwrap();
        clock.advance(Duration::from_secs(86_400));
        assert!(vault.prune_drafts().unwrap().is_empty());
        clock.advance(Duration::from_secs(2 * 86_400));
        store.setting_set(DRAFT_TTL, NO_TTL).unwrap();
        assert!(vault.prune_drafts().unwrap().is_empty());
        store.setting_set(DRAFT_TTL, "2d").unwrap();
        assert_eq!(vault.prune_drafts().unwrap(), [draft]);
    }

    #[test]
    fn a_sign_in_picks_only_drafts_the_agent_did_not_write() {
        let e = env();
        let vault = init(&e);
        vault.add(login("github", "old")).unwrap();
        let agent_put = put(&vault, "injected", ItemSource::Agent);
        assert_eq!(vault.login_draft_for(&github(), None).unwrap(), None);
        assert_eq!(
            vault.login_draft_for(&github(), Some(&agent_put)).unwrap(),
            None
        );
        let generated = vault
            .generate_login(
                GeneratedLogin {
                    name: name("github"),
                    origin: github(),
                    username: None,
                    rule: PasswordRule::default(),
                },
                "claude",
            )
            .unwrap();
        assert_eq!(
            vault.login_draft_for(&github(), None).unwrap(),
            Some(generated.clone())
        );
        let other = Origin::parse("https://gitlab.com").unwrap();
        assert_eq!(vault.login_draft_for(&other, None).unwrap(), None);
    }
}

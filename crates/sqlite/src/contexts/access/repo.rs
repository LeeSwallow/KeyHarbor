//! `PolicyStore` for the access context. One rule per (agent, resource, target).

use super::{
    mappers::{rule, rule_model},
    models::RuleModel,
    schema::{origin_uses, policy_rules},
};
use crate::SqliteStore;
use anyhow::Result;
use diesel::{prelude::*, upsert::excluded};
use keyharbor_domain::{
    policy::{Resource, Rule},
    ports::PolicyStore,
};

impl PolicyStore for SqliteStore {
    fn policy_rules(&self) -> Result<Vec<Rule>> {
        self.read(|conn| {
            policy_rules::table
                .select(RuleModel::as_select())
                .order((
                    policy_rules::agent,
                    policy_rules::resource,
                    policy_rules::target,
                ))
                .load(conn)?
                .into_iter()
                .map(rule)
                .collect()
        })
    }

    fn policy_set(&self, rule: &Rule) -> Result<()> {
        self.read(|conn| {
            diesel::insert_into(policy_rules::table)
                .values(rule_model(rule))
                .on_conflict((
                    policy_rules::agent,
                    policy_rules::resource,
                    policy_rules::target,
                ))
                .do_update()
                .set(policy_rules::effect.eq(excluded(policy_rules::effect)))
                .execute(conn)?;
            Ok(())
        })
    }

    fn note_origin_use(&self, item: &str, origin: &str) -> Result<bool> {
        self.read(|conn| {
            Ok(diesel::insert_or_ignore_into(origin_uses::table)
                .values((
                    origin_uses::item_name.eq(item),
                    origin_uses::origin.eq(origin),
                ))
                .execute(conn)?
                == 1)
        })
    }

    fn policy_remove(&self, agent: &str, resource: Resource, target: &str) -> Result<bool> {
        self.read(|conn| {
            Ok(
                diesel::delete(policy_rules::table.find((agent, resource.as_str(), target)))
                    .execute(conn)?
                    == 1,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::SqliteStore;
    use keyharbor_domain::policy::{Effect, Resource, Rule};
    use keyharbor_domain::ports::PolicyStore;

    #[test]
    fn rules_upsert_list_and_remove() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
        assert!(store.policy_rules().unwrap().is_empty());
        let deny = Rule::new("codex", Resource::Domain, "payment", Effect::Deny).unwrap();
        store.policy_set(&deny).unwrap();
        store
            .policy_set(&Rule::new("*", Resource::RawCdp, "*", Effect::Deny).unwrap())
            .unwrap();
        let allow = Rule {
            effect: Effect::Allow,
            ..deny.clone()
        };
        store.policy_set(&allow).unwrap();
        let rules = store.policy_rules().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[1], allow);
        assert!(
            store
                .policy_remove("codex", Resource::Domain, "payment")
                .unwrap()
        );
        assert!(
            !store
                .policy_remove("codex", Resource::Domain, "payment")
                .unwrap()
        );
        assert_eq!(store.policy_rules().unwrap().len(), 1);
    }

    #[test]
    fn origin_uses_are_first_only_once() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
        assert!(store.note_origin_use("me", "https://a.example").unwrap());
        assert!(!store.note_origin_use("me", "https://a.example").unwrap());
        assert!(store.note_origin_use("me", "https://b.example").unwrap());
        assert!(store.note_origin_use("home", "https://a.example").unwrap());
    }
}

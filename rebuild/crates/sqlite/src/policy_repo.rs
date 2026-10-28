//! Agent policy rules (02 §2.7, §2.8). One rule per (agent, resource, target).

use crate::SqliteStore;
use anyhow::{Context, Result};
use keyharbor_domain::policy::{Effect, Resource, Rule};
use rusqlite::{Connection, params};

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS policy_rules (agent TEXT NOT NULL, resource TEXT NOT NULL CHECK(resource IN ('session','item','domain','create_session','raw_cdp')), target TEXT NOT NULL, effect TEXT NOT NULL CHECK(effect IN ('allow','deny')), PRIMARY KEY(agent, resource, target));",
    )?;
    Ok(())
}

impl SqliteStore {
    pub fn policy_rules(&self) -> Result<Vec<Rule>> {
        let mut query = self.conn.prepare(
            "SELECT agent, resource, target, effect FROM policy_rules ORDER BY agent, resource, target",
        )?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        rows.map(|raw| {
            let (agent, resource, target, effect) = raw?;
            Ok(Rule {
                agent,
                resource: Resource::parse(&resource).context("invalid stored policy resource")?,
                target,
                effect: Effect::parse(&effect).context("invalid stored policy effect")?,
            })
        })
        .collect()
    }

    /// Adds the rule or changes the effect of the rule with the same agent, resource and target.
    pub fn policy_set(&self, rule: &Rule) -> Result<()> {
        self.conn.execute(
            "INSERT INTO policy_rules (agent, resource, target, effect) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(agent, resource, target) DO UPDATE SET effect=excluded.effect",
            params![
                rule.agent,
                rule.resource.as_str(),
                rule.target,
                rule.effect.as_str()
            ],
        )?;
        Ok(())
    }

    /// False if no such rule existed.
    pub fn policy_remove(&self, agent: &str, resource: Resource, target: &str) -> Result<bool> {
        Ok(self.conn.execute(
            "DELETE FROM policy_rules WHERE agent=?1 AND resource=?2 AND target=?3",
            params![agent, resource.as_str(), target],
        )? == 1)
    }
}

#[cfg(test)]
mod tests {
    use crate::SqliteStore;
    use keyharbor_domain::policy::{Effect, Resource, Rule};

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
}

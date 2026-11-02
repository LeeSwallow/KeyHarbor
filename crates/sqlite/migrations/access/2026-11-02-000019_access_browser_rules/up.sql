-- #120: rules may name a browser kind (`policy deny --resource browser --target ego-lite`).
-- SQLite cannot change a CHECK constraint in place, so the table is rebuilt.
CREATE TABLE policy_rules_next (agent TEXT NOT NULL, resource TEXT NOT NULL CHECK(resource IN ('session','item','domain','create_session','raw_cdp','browser')), target TEXT NOT NULL, effect TEXT NOT NULL CHECK(effect IN ('allow','deny')), PRIMARY KEY(agent, resource, target));
INSERT INTO policy_rules_next SELECT agent, resource, target, effect FROM policy_rules;
DROP TABLE policy_rules;
ALTER TABLE policy_rules_next RENAME TO policy_rules;

CREATE TABLE policy_rules_prev (agent TEXT NOT NULL, resource TEXT NOT NULL CHECK(resource IN ('session','item','domain','create_session','raw_cdp')), target TEXT NOT NULL, effect TEXT NOT NULL CHECK(effect IN ('allow','deny')), PRIMARY KEY(agent, resource, target));
INSERT INTO policy_rules_prev SELECT agent, resource, target, effect FROM policy_rules WHERE resource != 'browser';
DROP TABLE policy_rules;
ALTER TABLE policy_rules_prev RENAME TO policy_rules;

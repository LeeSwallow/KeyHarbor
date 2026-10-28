-- The schema as it was before migrations existed. IF NOT EXISTS lets databases created by
-- earlier builds (which already have these tables) record this migration without change.
CREATE TABLE IF NOT EXISTS policy_rules (agent TEXT NOT NULL, resource TEXT NOT NULL CHECK(resource IN ('session','item','domain','create_session','raw_cdp')), target TEXT NOT NULL, effect TEXT NOT NULL CHECK(effect IN ('allow','deny')), PRIMARY KEY(agent, resource, target));

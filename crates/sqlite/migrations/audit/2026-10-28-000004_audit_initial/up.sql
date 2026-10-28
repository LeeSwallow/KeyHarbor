-- The schema as it was before migrations existed. IF NOT EXISTS lets databases created by
-- earlier builds (which already have these tables) record this migration without change.
-- No column holds a value, by design (02 §2.7.2).
CREATE TABLE IF NOT EXISTS audit (seq INTEGER PRIMARY KEY, at TEXT NOT NULL, agent TEXT NOT NULL, session_id TEXT, kind TEXT NOT NULL, item_name TEXT, origin TEXT, result TEXT NOT NULL);

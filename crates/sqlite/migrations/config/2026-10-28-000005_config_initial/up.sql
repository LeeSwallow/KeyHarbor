-- The schema as it was before migrations existed. IF NOT EXISTS lets databases created by
-- earlier builds (which already have these tables) record this migration without change.
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);

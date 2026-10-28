-- The schema as it was before migrations existed. IF NOT EXISTS lets databases created by
-- earlier builds (which already have these tables) record this migration without change.
-- Builds before the raw CDP option lack generations.cdp; the session context adds it after
-- this migration (SQLite has no ADD COLUMN IF NOT EXISTS).
CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, state TEXT NOT NULL CHECK(state IN ('stopped','starting','running','awaiting_user','stopping','failed')));
CREATE TABLE IF NOT EXISTS generations (session_id TEXT PRIMARY KEY, generation TEXT NOT NULL, cdp INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS handoff_requests (request_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE, generation TEXT NOT NULL, origin TEXT, status TEXT NOT NULL CHECK(status IN ('pending','resumed','superseded','cancelled')));
CREATE INDEX IF NOT EXISTS handoff_requests_session ON handoff_requests(session_id, status);

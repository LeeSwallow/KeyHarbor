-- 02 §2.7 SITE_STATE: what an agent last recorded about a session's sign-in at one origin.
-- Names only (item name, agent name), never a value. Removed with its session.
CREATE TABLE site_states (
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  origin TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('logged_in','expired','unknown')),
  item_name TEXT,
  checked_at TEXT NOT NULL,
  recorded_by TEXT NOT NULL,
  PRIMARY KEY(session_id, origin)
);

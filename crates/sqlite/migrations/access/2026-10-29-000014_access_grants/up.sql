-- R-43: grants. A subject (now: a session) may act within a scope (now: one origin and its
-- subdomains) until a time. Rows stay after a grant ends: a session that had one stays limited.
CREATE TABLE grants (
  id TEXT PRIMARY KEY,
  subject_kind TEXT NOT NULL CHECK(subject_kind IN ('session')),
  subject TEXT NOT NULL,
  scope_kind TEXT NOT NULL CHECK(scope_kind IN ('origin')),
  scope TEXT NOT NULL,
  until BIGINT,
  created_by TEXT NOT NULL,
  reason TEXT,
  created_at BIGINT NOT NULL,
  revoked_at BIGINT,
  cleared BOOLEAN NOT NULL DEFAULT 0
);
CREATE INDEX grants_by_subject ON grants(subject_kind, subject);

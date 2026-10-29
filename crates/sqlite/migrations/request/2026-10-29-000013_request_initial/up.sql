-- 08 §8.2 ③: an agent's request for information only the user has. Names, the agent's
-- reason and the outcome only; the answer's values go to the vault, never here.
CREATE TABLE vault_requests (
  id TEXT PRIMARY KEY,
  agent TEXT NOT NULL,
  kind TEXT NOT NULL,
  name TEXT NOT NULL,
  reason TEXT NOT NULL,
  origin TEXT,
  status TEXT NOT NULL CHECK(status IN ('pending','fulfilled','declined','expired')),
  created_at BIGINT NOT NULL,
  item_name TEXT
);

-- The schema as it was before migrations existed. IF NOT EXISTS lets databases created by
-- earlier builds (which already have these tables) record this migration without change.
CREATE TABLE IF NOT EXISTS vault (id BLOB PRIMARY KEY CHECK(length(id)=16), kek_id BLOB NOT NULL CHECK(length(kek_id)=16), version INTEGER NOT NULL DEFAULT 0, singleton INTEGER NOT NULL DEFAULT 1 UNIQUE CHECK(singleton=1));
CREATE TABLE IF NOT EXISTS domain_keys (domain INTEGER PRIMARY KEY CHECK(domain IN (1,2,3)), dk_id BLOB NOT NULL CHECK(length(dk_id)=16), wrapped BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS vault_items (id BLOB PRIMARY KEY CHECK(length(id)=16), name TEXT NOT NULL UNIQUE, domain INTEGER NOT NULL CHECK(domain IN (1,2,3)), kind TEXT NOT NULL, origin_binding INTEGER NOT NULL, status TEXT NOT NULL CHECK(status IN ('draft','active','discarded')), source TEXT NOT NULL, meta_mac BLOB NOT NULL, updated_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS vault_fields (item_id BLOB NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE, field TEXT NOT NULL, sealed BLOB NOT NULL, PRIMARY KEY(item_id, field));
CREATE TABLE IF NOT EXISTS item_origins (item_id BLOB NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE, origin TEXT NOT NULL, PRIMARY KEY(item_id, origin));

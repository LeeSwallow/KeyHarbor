-- R-40: a fourth domain, developer (4), for secret and env items. SQLite cannot change a
-- CHECK constraint, so domain_keys and vault_items are rebuilt with domain IN (1,2,3,4).
-- vault_fields and item_origins reference vault_items with ON DELETE CASCADE; they are copied
-- aside and dropped first, so dropping vault_items deletes nothing. The migration runs in
-- one transaction. Moving existing secret and env items into the new domain needs the
-- domain keys and happens when the vault is next opened (keyharbor-core).
CREATE TEMP TABLE kh_fields AS SELECT item_id, field, sealed FROM vault_fields;
CREATE TEMP TABLE kh_origins AS SELECT item_id, origin FROM item_origins;
CREATE TEMP TABLE kh_items AS SELECT id, name, domain, kind, origin_binding, status, source, meta_mac, updated_at, run_allowed FROM vault_items;
CREATE TEMP TABLE kh_keys AS SELECT domain, dk_id, wrapped FROM domain_keys;
DROP TABLE vault_fields;
DROP TABLE item_origins;
DROP TABLE vault_items;
DROP TABLE domain_keys;
CREATE TABLE domain_keys (domain INTEGER PRIMARY KEY CHECK(domain IN (1,2,3,4)), dk_id BLOB NOT NULL CHECK(length(dk_id)=16), wrapped BLOB NOT NULL);
CREATE TABLE vault_items (id BLOB PRIMARY KEY CHECK(length(id)=16), name TEXT NOT NULL UNIQUE, domain INTEGER NOT NULL CHECK(domain IN (1,2,3,4)), kind TEXT NOT NULL, origin_binding INTEGER NOT NULL, status TEXT NOT NULL CHECK(status IN ('draft','active','discarded')), source TEXT NOT NULL, meta_mac BLOB NOT NULL, updated_at TEXT NOT NULL, run_allowed INTEGER NOT NULL DEFAULT 0);
CREATE TABLE vault_fields (item_id BLOB NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE, field TEXT NOT NULL, sealed BLOB NOT NULL, PRIMARY KEY(item_id, field));
CREATE TABLE item_origins (item_id BLOB NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE, origin TEXT NOT NULL, PRIMARY KEY(item_id, origin));
INSERT INTO domain_keys (domain, dk_id, wrapped) SELECT domain, dk_id, wrapped FROM kh_keys;
INSERT INTO vault_items (id, name, domain, kind, origin_binding, status, source, meta_mac, updated_at, run_allowed) SELECT id, name, domain, kind, origin_binding, status, source, meta_mac, updated_at, run_allowed FROM kh_items;
INSERT INTO vault_fields (item_id, field, sealed) SELECT item_id, field, sealed FROM kh_fields;
INSERT INTO item_origins (item_id, origin) SELECT item_id, origin FROM kh_origins;
DROP TABLE kh_fields;
DROP TABLE kh_origins;
DROP TABLE kh_items;
DROP TABLE kh_keys;

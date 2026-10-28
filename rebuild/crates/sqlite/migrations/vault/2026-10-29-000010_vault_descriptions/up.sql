-- R-38: what an item and each of its fields are for. Plaintext (not secret), signed in the
-- metadata MAC v2 (docs/design/07 §7.6.3).
ALTER TABLE vault_items ADD COLUMN description TEXT NULL;
CREATE TABLE item_field_descriptions (item_id BLOB NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE, field TEXT NOT NULL, description TEXT NOT NULL, PRIMARY KEY(item_id, field));

-- R-37: the last day agents may use an item (UTC, YYYY-MM-DD); NULL never expires. Signed in
-- the metadata MAC v2 (docs/design/07 §7.6.3).
ALTER TABLE vault_items ADD COLUMN expires_at TEXT NULL;

-- Developer secrets spec §2: tags to find items by. Plaintext (not secret), space-joined in
-- sorted order (a tag has no spaces), signed in the metadata MAC v3 (docs/design/07 §7.6.3).
ALTER TABLE vault_items ADD COLUMN tags TEXT NOT NULL DEFAULT '';

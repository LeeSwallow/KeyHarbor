-- Developer secrets spec §2: fields marked not secret (shown in listings, not masked), space-
-- joined in sorted order. Signed in the metadata MAC v4 (docs/design/07 §7.6.3).
ALTER TABLE vault_items ADD COLUMN plain_fields TEXT NOT NULL DEFAULT '';

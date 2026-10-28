-- Phase 3 (R-35, R-36): whether `keyharbor run` may inject an item's values. Existing items
-- get their kind's default (secret and env items yes, the others no); that is also the value
-- their v1 metadata MAC vouches for (docs/design/07 §7.6.3).
ALTER TABLE vault_items ADD COLUMN run_allowed INTEGER NOT NULL DEFAULT 0;
UPDATE vault_items SET run_allowed = 1 WHERE kind IN ('secret', 'env');

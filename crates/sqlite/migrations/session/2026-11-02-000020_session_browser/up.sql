-- #120: which browser the current run uses (chrome or ego-lite).
ALTER TABLE generations ADD COLUMN browser TEXT NOT NULL DEFAULT 'chrome';

-- #206 (R-48(3)): agents may type the item's one-time codes when agent_otp is on. On for every
-- existing item, the value metadata MACs before v5 vouch for (docs/design/07 §7.6.3).
ALTER TABLE vault_items ADD COLUMN otp_allowed INTEGER NOT NULL DEFAULT 1;

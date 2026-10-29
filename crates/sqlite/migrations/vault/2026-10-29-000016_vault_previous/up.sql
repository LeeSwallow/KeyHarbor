-- Developer secrets spec §2: the value a field had before its last change, as a sealed record
-- under the same key and AD as the current one (so reverting swaps records, nothing is opened).
ALTER TABLE vault_fields ADD COLUMN previous BLOB;

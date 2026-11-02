-- J-110: why the last run did not start, shown by open and list. A short reason, never a value.
ALTER TABLE sessions ADD COLUMN failure TEXT NULL;

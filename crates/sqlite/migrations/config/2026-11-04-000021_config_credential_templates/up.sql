-- Credential templates (R-48): a name and the fields a new credential item starts with, each
-- `name=secret` or `name=plain`, space separated in order. No value is ever stored here.
CREATE TABLE credential_templates (
    name TEXT PRIMARY KEY NOT NULL,
    fields TEXT NOT NULL
);

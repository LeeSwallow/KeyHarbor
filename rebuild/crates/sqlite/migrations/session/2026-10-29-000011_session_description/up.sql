-- R-38: what a session is for, shown to agents. Not a secret.
ALTER TABLE sessions ADD COLUMN description TEXT NULL;

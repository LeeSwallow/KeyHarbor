---
name: signing-up-for-a-site
description: Use when a task needs a new account on a website. KeyHarbor makes up the password and keeps it as a draft; you type it by placeholder and the draft is confirmed when you record the sign-in.
---

# Signing up for a site

1. Make the login: `vault_generate {name, origin, username?, password_rule?}`.
   Pass `password_rule` (`length`, `symbols`, `forbid`) when the site states its rules.
   It returns the item name (a taken name becomes `<name>~draft-N`) and its placeholders.
2. Fill the sign-up form with `browser_fill`, using those placeholders for the password
   (and its confirmation field).
3. Terms to accept, an email to verify, a CAPTCHA: handing-off-to-the-user.
4. Once signed in, record it: `site_state_set {session, origin, state: "logged_in", item}`.
   That confirms the generated draft; without it the draft stays a draft.

- Never choose a password yourself and type it: it would be outside the vault.
- If the site showed you a value you must keep (a recovery code, an API key), save it with
  `vault_put {name, kind, fields}`. It is stored as a draft for the user to confirm.

---
name: logging-in-to-a-site
description: Use when a task needs the browser signed in to a website (GitHub, a dashboard, a shop) and the vault has a login for it. Checks the recorded state, signs in with placeholders, and records the result.
---

# Logging in to a site

1. Find the login: `vault_list` (look at `origins`, `kind: login`, `tags`).
2. Open a session: `session_list`, else `session_create`, then `session_open`.
3. Check what is known: `site_state_get {session, origin}`. `logged_in` is a note from an
   earlier run, not proof: confirm in the page.
4. Go to the sign-in page: `browser_navigate`, then `browser_snapshot`.
5. Fill and submit in one step: `browser_login {session, item}`. Or by hand:
   `browser_fill` with `{{secret:<item>.username}}` and `{{secret:<item>.password}}`, then
   `browser_click` on the sign-in button.
6. `browser_wait`, `browser_snapshot`, and check you are signed in.
7. Record it: `site_state_set {session, origin, state: "logged_in", item}`.
   If the site asked to sign in again, record `expired` instead.

- A CAPTCHA, 2FA code or "is this you?" page: handing-off-to-the-user.
- `denied: <item> (origin)`: the page is not the item's site. Do not retry elsewhere.
- No login in the vault: asking-for-a-secret, or signing-up-for-a-site for a new account.

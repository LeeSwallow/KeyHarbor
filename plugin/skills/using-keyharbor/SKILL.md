---
name: using-keyharbor
description: Use when a task needs a website login, a password, an API key, an .env file, a card or address, or a browser session the user is signed in to. Maps the KeyHarbor MCP tools and CLI commands to the other keyharbor skills, and sets the rules for secret values.
---

# Using KeyHarbor

KeyHarbor keeps the user's secrets in a local vault and drives a dedicated browser profile.
You never see a value: you see names and `{{secret:item.field}}` placeholders.

| You need to | Use |
|---|---|
| see what the vault holds (names, kinds, sites, placeholders, tags) | `vault_list` |
| a browser signed in to a site | logging-in-to-a-site |
| a new account | signing-up-for-a-site |
| an API key or `.env` for a shell command | using-env-secrets (`keyharbor run`) |
| a CAPTCHA, 2FA, terms or payment confirmed | handing-off-to-the-user |
| a card, address or password the vault lacks | asking-for-a-secret |

Sessions: `session_list`, `session_create`, `session_open`, `session_stop`. Pages:
`browser_navigate`, `browser_snapshot`, `browser_click`, `browser_fill`, `browser_select`,
`browser_upload`, `browser_type`, `browser_press`, `browser_wait`, `browser_login`, `browser_tabs`, `browser_new_tab`,
`browser_switch_tab`, `browser_screenshot`.

Rules for every value: `${CLAUDE_PLUGIN_ROOT}/rules/secrets.md`.

A `<select>` (school, service status) takes `browser_select` with `value`, `label` or `index`;
a placeholder works for `value` and `label`, and the result then names only the index.
A file input takes `browser_upload` with absolute paths. Only files inside the folders the
person allowed (`upload_dirs`) go; if it says `upload_dirs is not set`, ask the person to allow
the folder, do not look for another way.

A session may be held to some sites only (a grant). If a page refuses with
`denied: navigation (out-of-grant)`, stay on the granted sites; only the user can add one.

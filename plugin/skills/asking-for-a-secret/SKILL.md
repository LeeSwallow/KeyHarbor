---
name: asking-for-a-secret
description: Use when the task needs information only the user has and the vault lacks - a card for a purchase, an address, a password or key you were not given. Asks through KeyHarbor so the value goes from the user to the vault, never through the chat.
---

# Asking for a secret

1. `vault_request {kind, name, reason, origin?}`; `kind` is login, secret, env, card,
   identity or address. Say plainly why you need it in `reason`; no secret in it.
2. Tell the user to answer in a terminal: `keyharbor vault requests`, then
   `keyharbor vault fulfill <id>` (or `keyharbor vault decline <id>`).
3. `vault_wait {request_id, timeout_seconds}` returns `fulfilled` with the item name and
   placeholders, `declined`, `expired`, or `pending` if the wait ran out (wait again).
4. Use the placeholders as usual.

- Never ask the user to paste the value into the chat.
- A value you already have in context (an API key a site just showed): `vault_put` instead.

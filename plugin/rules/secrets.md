# Secrets with KeyHarbor

- Never print, summarize, log or repeat a secret value. Refer to it by its placeholder,
  `{{secret:item.field}}`, which `vault_list` gives for every field.
- Type placeholders, not values, into `browser_fill` and `browser_type`. KeyHarbor fills the
  value only on the item's site and masks it in every result you read back.
- When KeyHarbor refuses (`denied: <item> (<reason>)`), do not work around it: no other
  field, no other site, no copying the value elsewhere. Tell the user the reason.
- Do not solve CAPTCHAs, approve terms or confirm payments yourself. Hand the browser to the
  user (handing-off-to-the-user).
- Do not ask the user to paste a password or key into the chat. Ask through KeyHarbor
  (asking-for-a-secret), where the value goes straight to the vault.
- Do not write secrets into files, URLs or command arguments yourself; use `keyharbor run`.

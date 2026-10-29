---
name: using-env-secrets
description: Use when a shell command, test run, script or MCP server needs API keys, a .env file, a key file or a config with secrets. Runs it through keyharbor run so values reach only that process and its output comes back masked.
---

# Using env secrets

```sh
keyharbor run --env myapp -- npm test                      # every variable of an env item
keyharbor run --env-file .env.keyharbor -- ./serve         # KEY={{secret:item.field}} lines
keyharbor run --file GOOGLE_APPLICATION_CREDENTIALS=gcp.key -- ./deploy.sh
keyharbor run --template CONF=app.yml.tmpl -- ./serve --config "$CONF"
```

- Find items with `keyharbor vault list --prefix myapp/` or `--tag prod` (or `vault_list`).
- `--file` and `--template` write a private file, put its path in the variable, and remove
  it when the command ends.
- A placeholder in an argument works but other processes can read arguments; prefer `--env`
  or `--file`.
- Commit `.env.keyharbor` (placeholders only); never write a plaintext `.env`.
- Output is masked: a value shows as its placeholder. Do not try to decode or reprint it.
- `denied: <item> (run-not-allowed)`: the user has not allowed `run` for that item.

---
description: Check that KeyHarbor is installed and its vault is ready
allowed-tools: Bash(keyharbor:*)
---

Run `keyharbor status` and report the line it prints.

- If the command is not found, KeyHarbor is not installed: tell the user to build it from
  https://github.com/LeeSwallow/KeyHarbor (`cargo install --path crates/cli`) and stop.
- If it says `vault not initialized`, tell the user to run `keyharbor vault init`
  themselves in a terminal (macOS asks to store the root key in the keychain). Do not run it.
- The MCP server comes with this plugin. For Codex, tell the user to run
  `codex mcp add keyharbor -- keyharbor mcp --agent codex`.

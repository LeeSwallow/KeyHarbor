#!/usr/bin/env sh
# At the start of a session, tells the agent that KeyHarbor is here and what waits for the
# user: counts only, never a name or a value. Says nothing when keyharbor is not installed,
# so a machine without it costs no context.
set -u
command -v keyharbor >/dev/null 2>&1 || exit 0
line="$(keyharbor status 2>/dev/null)" || exit 0
printf 'KeyHarbor is installed (%s). For sign-ins, sign-ups, API keys and handoffs use the keyharbor:using-keyharbor skill; type {{secret:item.field}} placeholders, never values.\n' "$(printf '%s' "$line" | tr '\t' ',')"

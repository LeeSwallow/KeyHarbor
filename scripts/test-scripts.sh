#!/bin/sh
# test-scripts.sh: tests for the commit-msg hook and the flow scripts.
set -u
cd "$(dirname "$0")/.."
root="$(pwd)"
fail=0
ok() { echo "✓ $*"; }
no() { echo "✗ $*"; fail=1; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# 1–2. The hook removes AI trailers and keeps human co-authors.
msg="$tmp/msg"
printf 'feat: x\n\nbody\n\nCo-Authored-By: Claude <noreply@anthropic.com>\nCo-authored-by: Kim <kim@example.com>\n🤖 Generated with [Claude Code](https://claude.com/claude-code)\n' > "$msg"
(cd "$tmp" && git init -q && sh "$root/.githooks/commit-msg" "$msg")
grep -qi 'claude' "$msg" && no "hook left an AI trailer" || ok "hook removes AI trailers"
grep -q 'Co-authored-by: Kim' "$msg" && ok "hook keeps human co-authors" || no "hook removed a human co-author"

# 4. On an issue branch the hook adds Refs.
(cd "$tmp" && git -c user.name=t -c user.email=t@t commit -q --allow-empty --no-verify -m init && git checkout -q -b fix/12-x && printf 'fix: y\n' > "$msg" && sh "$root/.githooks/commit-msg" "$msg")
grep -q 'Refs: #12' "$msg" && ok "hook adds Refs on an issue branch" || no "hook did not add Refs: #12"

# 3. check-pr.sh
pr() { PR_TITLE="$1" PR_BODY="$2" PR_LABELS="$3" PR_AUTHOR=someone sh scripts/check-pr.sh >/dev/null 2>&1; }
pr "feat: add x" "Closes #1" "feature" && ok "check-pr accepts a valid PR" || no "check-pr rejected a valid PR"
pr "update stuff" "Closes #1" "feature" && no "check-pr accepted a bad title" || ok "check-pr rejects a bad title"
pr "feat: add x" "Closes #1" "" && no "check-pr accepted no kind label" || ok "check-pr requires a kind label"
pr "docs: typo" "" "docs
no-issue" && ok "check-pr allows no-issue" || no "check-pr rejected a no-issue PR"

# Review Focus 1: a tag that does not match the crate version stops the release.
sh scripts/check-version.sh v0.1.0 >/dev/null 2>&1 && ok "check-version accepts the crate version" || no "check-version rejected v0.1.0"
out="$(sh scripts/check-version.sh v9.9.9 2>&1)" && no "check-version accepted a wrong tag" || {
	printf '%s' "$out" | grep -q 'does not match' && ok "check-version rejects a wrong tag" || no "check-version gave no reason"
}

[ "$fail" -eq 0 ] && echo "all script tests passed"
exit "$fail"

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

# The shipped binary has no test-only features (dev-dependencies excluded).
sh scripts/check-release-features.sh >/dev/null 2>&1 && ok "release build has no test-only features" || no "check-release-features failed"

# The layer rules hold for crates/ (ADR-0014).
sh scripts/check-layers.sh >/dev/null 2>&1 && ok "layer rules hold" || no "check-layers failed"

# Each layer rule fails on a planted violation (a copy of crates/).
# $1 = name, $2 = file to append to (relative to the copy), $3 = line, $4 = expected message.
planted() {
	fx="$tmp/layers-$1"
	cp -R crates "$fx"
	printf '%s\n' "$3" >> "$fx/$2"
	out="$(CHECK_LAYERS_ROOT="$fx" sh scripts/check-layers.sh 2>&1)" && no "check-layers accepted $1" || {
		printf '%s' "$out" | grep -qF "$4" && ok "check-layers rejects $1" || no "check-layers gave no reason for $1: $out"
	}
	rm -rf "$fx"
}
planted "mcp-depends-on-sqlite" mcp/Cargo.toml 'keyharbor-sqlite = { path = "../sqlite" }' \
	"mcp must not depend on keyharbor-sqlite"
planted "a-cli-command-opening-the-store" cli/src/policy_cmd.rs 'fn x() { let _ = keyharbor_sqlite::SqliteStore::open; }' \
	"interfaces build adapters outside the composition root"
planted "an-mcp-tool-calling-a-storage-port" mcp/src/tools/vault.rs 'fn x(s: &dyn S) { let _ = s.policy_rules(); }' \
	"interfaces call storage ports directly"
planted "a-placeholder-literal-outside-domain" guard/src/login.rs 'fn x() -> String { format!("{{{{secret:{}.{}}}}}", "a", "b") }' \
	"placeholder syntax outside keyharbor-domain"
planted "a-requester-check-outside-domain" supervisor/src/ops.rs 'fn x(a: &str) -> bool { a == ANY }' \
	"requester-name rule outside keyharbor-domain"
planted "an-interface-choosing-a-session-action" mcp/src/server.rs 'fn x() { let _ = Action::RawCdp; }' \
	"interfaces choose session-level policy actions"
planted "broker-depending-on-the-guard" broker/Cargo.toml 'keyharbor-guard = { path = "../guard" }' \
	"broker must not depend on keyharbor-guard"
planted "mcp-depending-on-the-cdp-adapter" mcp/Cargo.toml 'keyharbor-broker = { path = "../broker" }' \
	"only keyharbor-supervisor wraps the CDP adapter"
planted "a-control-request-built-by-hand" mcp/src/server.rs 'fn x() -> serde_json::Value { serde_json::json!({"op": "close"}) }' \
	"control requests built or parsed by hand"
planted "file-io-in-core" core/src/policy.rs 'fn x() { let _ = std::fs::read("a"); }' \
	"I/O in an inner crate"
planted "the-clock-read-in-the-guard" guard/src/audit.rs 'fn x() { let _ = std::time::SystemTime::now(); }' \
	"I/O in an inner crate"
planted "a-driver-crate-in-the-domain" domain/Cargo.toml 'tokio.workspace = true' \
	"domain depends on tokio; inner crates use no driver crate"
planted "serde-json-in-a-domain-value" domain/src/policy.rs 'fn x() -> serde_json::Value { serde_json::Value::Null }' \
	"keyharbor-domain uses serde_json outside ports/browser.rs"
planted "randomness-outside-crypto" sqlite/src/lib.rs 'fn x() { let _ = uuid::Uuid::new_v4(); }' \
	"randomness outside keyharbor-crypto"
planted "the-uuid-v4-feature" domain/Cargo.toml 'uuid = { workspace = true, features = ["v4"] }' \
	"uuid's v4 feature is on"
planted "clap-outside-the-cli" mcp/Cargo.toml 'clap = "4"' \
	"mcp depends on clap; it belongs to one adapter crate only"

# The design-system rules hold for design-system/ (ADR-0015).
sh scripts/check-design.sh >/dev/null 2>&1 && ok "design rules hold" || no "check-design failed"

[ "$fail" -eq 0 ] && echo "all script tests passed"
exit "$fail"

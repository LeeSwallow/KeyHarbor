#!/bin/sh
# check-layers.sh: fail when crates/ breaks the hexagonal layering of
# docs/design/02 §2.4 and ADR-0014.
#
# 1. Normal dependencies (dev-dependencies excluded) between keyharbor crates
#    follow the allowed table below.
# 2. Only keyharbor-crypto depends on or names a cryptographic primitive crate
#    (chacha20poly1305, hkdf, hmac, sha2, age, getrandom; ADR-0016).
# 3. rusqlite is not used anywhere. Production code runs no raw SQL: schema
#    SQL lives in crates/sqlite/migrations/, and `sql_query(`/`batch_execute(`
#    appear only in test code (`#[cfg(test)]` items, the `test-support`
#    feature, tests/ directories) — plus the connection PRAGMAs.
# 4. `PlaintextAccess::issue` is called only by the guard, by core's export,
#    and by tests.
# 5. The raw browser adapter (CDP) stays inside keyharbor-broker: interfaces
#    reach browser operations only through the guarded control path.
# 6. A persistence context does not reach into another context's models,
#    schema or mappers.
# 7. (A) Interfaces call use cases only: the CLI's command modules and the MCP
#    server neither open the store nor call storage ports; the composition
#    root (cli/src/main.rs, cli/src/compose.rs) builds the adapters.
# 8. (J) Which session operation needs which policy action is the guard's
#    table (`keyharbor_guard::session`); interfaces never name a session-level
#    `Action`.
# 9. (H) The placeholder syntax `{{secret:item.field}}` is written and found
#    only in keyharbor-domain (`PLACEHOLDER_OPEN`, `Display for PlaceholderRef`);
#    comments, tool descriptions and tests may show it.
# 10. (I) The rule "a requester is never `*`" lives in keyharbor-domain
#    (`AgentName`); nobody else compares a requester with `ANY`.
# 11. (B) keyharbor-broker is the CDP adapter only: it depends on the domain
#    alone, and only keyharbor-supervisor (which wraps it in the guard)
#    depends on it.
# 12. (K) The control-socket protocol has one wire module
#    (supervisor/src/control/wire.rs): no other production code writes an
#    `"op"` key by hand.
# 13. (C) The inner crates (domain, crypto, vault, core, guard) do no I/O:
#    no files, processes, sockets, environment, clock, sleeping or printing
#    outside tests. Ports bring those in (Clock, RotationLock, KeyStore, ...).
# 14. (D) The inner crates depend on no driver crate (tokio, rmcp, clap,
#    diesel, libsqlite3-sys, security-framework, libc, rpassword, base64).
# 15. (E) keyharbor-domain uses serde_json only where a port result is the
#    page's JSON (ports/browser.rs, J-43).
# 16. (F) Randomness comes from keyharbor-crypto only (ADR-0016): nobody else
#    calls `new_v4`, `OsRng`, `rand::` or `getrandom`, and no crate turns on
#    uuid's `v4` feature (crypto builds v4 ids from its own random bytes).
# 17. (G) A driver technology stays in its adapter crate: security-framework
#    in keyharbor-keystore, rmcp in keyharbor-mcp, clap in keyharbor.
#
# CHECK_LAYERS_ROOT replaces crates/ (scripts/test-scripts.sh runs the
# rules against copies with planted violations).
set -u
cd "${CHECK_LAYERS_ROOT:-$(dirname "$0")/../crates}" || exit 1
bad=0
no() { echo "✗ $*"; bad=1; }

# --- 1. allowed dependencies -------------------------------------------------
allowed() {
	case "$1" in
	domain) echo "" ;;
	crypto) echo "domain" ;;
	vault) echo "domain crypto" ;;
	core) echo "domain vault crypto" ;;
	guard) echo "domain core" ;;
	# Row ids and run generations come from crypto's random source (ADR-0016, J-75).
	sqlite) echo "domain crypto" ;;
	# (B) the CDP adapter knows only the ports it implements.
	broker) echo "domain" ;;
	# The supervisor process composes the CDP adapter with the guard (#11).
	supervisor) echo "domain core guard broker" ;;
	mcp) echo "domain core guard supervisor" ;;
	cli) echo "domain core guard sqlite keystore supervisor mcp" ;;
	# The key-store adapter (macOS Keychain, test file store; #19).
	keystore) echo "domain" ;;
	# A dev tool (ADR-0015): uses no keyharbor crate, and no crate may depend on it.
	design-check) echo "" ;;
	*) echo "?" ;;
	esac
}

# Prints the names of the normal (non-dev, non-build) dependencies of a manifest.
normal_deps() {
	awk '
		/^\[/ {
			section = $0
			normal = (section ~ /^\[(target\..*\.)?dependencies\]$/)
			next
		}
		normal && /^[A-Za-z0-9_-]+[[:space:]]*(=|\.)/ {
			name = $0
			sub(/[[:space:]]*[=.].*/, "", name)
			print name
		}
	' "$1"
}

for manifest in */Cargo.toml; do
	crate="${manifest%/Cargo.toml}"
	ok="$(allowed "$crate")"
	if [ "$ok" = "?" ]; then
		no "$crate: not in the layer table (add it to scripts/check-layers.sh and ADR-0014)"
		continue
	fi
	for dep in $(normal_deps "$manifest"); do
		case "$dep" in
		keyharbor-*)
			short="${dep#keyharbor-}"
			case " $ok " in
			*" $short "*) ;;
			*) no "$crate must not depend on $dep (allowed: ${ok:-nothing})" ;;
			esac
			;;
		rusqlite)
			no "$crate depends on rusqlite; persistence goes through Diesel in keyharbor-sqlite"
			;;
		chacha20poly1305 | hkdf | hmac | sha2 | age | getrandom)
			[ "$crate" = crypto ] || no "$crate depends on $dep; only keyharbor-crypto uses cryptographic primitives"
			;;
		diesel | diesel_migrations | libsqlite3-sys)
			[ "$crate" = sqlite ] || no "$crate depends on $dep; only keyharbor-sqlite talks to the database"
			;;
		esac
	done
done
if grep -q '^rusqlite' ../Cargo.toml 2>/dev/null; then
	no "the workspace still declares rusqlite"
fi

# --- 2. cryptographic primitives ----------------------------------------------
hits="$(grep -rnE '\b(chacha20poly1305|hkdf|hmac|sha2|age|getrandom)(::|;|\b *as\b)' --include='*.rs' . |
	grep -E 'use |::' | grep -v '^\./crypto/' || true)"
[ -n "$hits" ] && no "cryptographic primitives used outside keyharbor-crypto:
$hits"

# --- 3. raw SQL ------------------------------------------------------------------
hits="$(grep -rn 'rusqlite' --include='*.rs' . || true)"
[ -n "$hits" ] && no "rusqlite referenced:
$hits"
# Prints file:line for matches of $1 outside items marked #[cfg(test)] or
# #[cfg(feature = "test-support")] (the marked item's brace block is skipped).
outside_tests() {
	find . -name '*.rs' -not -path '*/tests/*' -not -path '*/target/*' | while read -r f; do
		PAT="$1" awk -v file="$f" '
			BEGIN { pat = ENVIRON["PAT"] }
			/#\[cfg\((test|feature = "test-support")\)\]/ { pending = 1 }
			{
				line = $0
				if (depth == 0 && !pending && line ~ pat) print file ":" NR ": " line
				if (pending || depth > 0) {
					opens = gsub(/\{/, "{", line); closes = gsub(/\}/, "}", line)
					if (pending && opens > 0) { pending = 0; started = 1 }
					if (started) { depth += opens - closes; if (depth <= 0) { depth = 0; started = 0 } }
				}
			}
		' "$f"
	done
}
hits="$(outside_tests 'sql_query\(|batch_execute\(' | grep -v '^\./sqlite/src/connection\.rs:.*batch_execute(PRAGMAS)' || true)"
[ -n "$hits" ] && no "raw SQL in production code (put schema changes in a migration):
$hits"

# --- 4. the plaintext capability -------------------------------------------------
hits="$(grep -rn 'PlaintextAccess::issue' --include='*.rs' . |
	grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' |
	grep -vE '^\./guard/' |
	grep -vE '^\./core/src/transfer\.rs:' |
	grep -vE '^\./core/src/test_support\.rs:' |
	grep -vE '^\./[a-z]+/tests/' || true)"
[ -n "$hits" ] && no "PlaintextAccess::issue called outside the guard, core's export and tests:
$hits"

# --- 5. the raw browser adapter stays in the broker -------------------------------
if [ -f broker/src/lib.rs ] && grep -nE '^pub mod (cdp|browser)\b' broker/src/lib.rs >/dev/null; then
	no "keyharbor-broker exposes its raw CDP adapter (pub mod cdp/browser)"
fi
hits="$(grep -rnE 'keyharbor_broker\b|\bCdpBrowser\b' --include='*.rs' mcp cli 2>/dev/null || true)"
[ -n "$hits" ] && no "interfaces reach the raw browser adapter:
$hits"
if [ ! -d guard ] || ! grep -rq 'struct GuardedBrowser' guard/src 2>/dev/null; then
	no "keyharbor-guard with GuardedBrowser is missing"
fi

# --- 6. persistence contexts stay apart ---------------------------------------------
if [ -d sqlite/src/contexts ]; then
	for dir in sqlite/src/contexts/*/; do
		ctx="$(basename "$dir")"
		hits="$(grep -rnE 'contexts::([a-z_]+)::(models|mappers|repo)\b|super::super::([a-z_]+)::(models|mappers|repo)\b' "$dir" |
			grep -vE "contexts::$ctx::|super::super::$ctx::" || true)"
		[ -n "$hits" ] && no "context $ctx uses another context's models, mappers or repository:
$hits"
	done
else
	no "keyharbor-sqlite has no src/contexts/ (persistence contexts)"
fi

# --- 7. (A) interfaces call use cases, the composition root builds adapters --------
surfaces="$(ls cli/src/*_cmd.rs cli/src/*_cmd/*.rs 2>/dev/null; find mcp/src -name '*.rs' 2>/dev/null)"
if [ -n "$surfaces" ]; then
	# shellcheck disable=SC2086 # one path per word
	hits="$(grep -nE 'SqliteStores?\b|keystore_from_env|ProcessRuntime::new' $surfaces || true)"
	[ -n "$hits" ] && no "interfaces build adapters outside the composition root (cli/src/main.rs, cli/src/compose.rs):
$hits"
	# shellcheck disable=SC2086
	hits="$(grep -nE '\.(item_list|item_get|set_description|policy_rules|policy_set|policy_remove|list_audit|setting_get|setting_set|handoff_status|request_handoff|reserve|transition|force_stopped)\(|\bstore\(?\)?\.(list|create|get|delete|record)\(' $surfaces || true)"
	[ -n "$hits" ] && no "interfaces call storage ports directly (add a use case to core or guard):
$hits"
fi

# --- 8. (J) the session policy table lives in the guard -----------------------------
hits="$(grep -rnE 'Action::(OpenSession|CreateSession|RawCdp)' --include='*.rs' mcp cli 2>/dev/null || true)"
[ -n "$hits" ] && no "interfaces choose session-level policy actions (use keyharbor_guard::session):
$hits"

# --- 9. (H) placeholder syntax in one place ------------------------------------------
hits="$(outside_tests '[{][{]secret:' |
	grep -v '^\./domain/src/vault\.rs:' |
	grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' |
	grep -vE 'description = "' || true)"
[ -n "$hits" ] && no "placeholder syntax outside keyharbor-domain (use PlaceholderRef or vault::placeholder):
$hits"

# --- 10. (I) requester names in one place --------------------------------------------
hits="$(grep -rnE 'reserved for rules that apply to every agent|[!=]= *(policy::)?ANY\b' --include='*.rs' . |
	grep -vE '^\./(domain/src/|guard/src/pdp/)' | grep -v '/tests/' || true)"
[ -n "$hits" ] && no "requester-name rule outside keyharbor-domain (use policy::AgentName):
$hits"

# --- 11. (B) only the supervisor wraps the CDP adapter ------------------------------
for manifest in */Cargo.toml; do
	crate="${manifest%/Cargo.toml}"
	[ "$crate" = supervisor ] && continue
	if normal_deps "$manifest" | grep -qx 'keyharbor-broker'; then
		no "$crate depends on keyharbor-broker; only keyharbor-supervisor wraps the CDP adapter in the guard"
	fi
done

# --- 12. (K) one wire module for the control protocol -------------------------------
hits="$(outside_tests '"op"[[:space:]]*[]:)]' | grep -E '^\./(mcp|cli|broker|supervisor)/' |
	grep -v '^\./supervisor/src/control/wire\.rs:' |
	grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true)"
[ -n "$hits" ] && no "control requests built or parsed by hand (use keyharbor_supervisor::control::wire):
$hits"

inner="domain crypto vault core guard"

# --- 13. (C) the inner crates do no I/O ----------------------------------------------
# (awk has no \b: a word start is "line start or a non-identifier character".)
hits="$(outside_tests 'std::(fs|process|net|env)([^A-Za-z0-9_]|$)|(^|[^A-Za-z0-9_])(File::|tokio::)|Command::new|SystemTime::now|Instant::now|thread::sleep|println!|eprintln!' |
	grep -E "^\./($(echo "$inner" | tr ' ' '|'))/src/" |
	grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true)"
[ -n "$hits" ] && no "I/O in an inner crate (bring it in through a port; the adapter lives outside):
$hits"

# --- 14. (D) the inner crates use no driver crate --------------------------------------
for crate in $inner; do
	[ -f "$crate/Cargo.toml" ] || continue
	for dep in $(normal_deps "$crate/Cargo.toml"); do
		case "$dep" in
		tokio | rmcp | clap | diesel | diesel_migrations | libsqlite3-sys | security-framework | libc | rpassword | base64)
			no "$crate depends on $dep; inner crates use no driver crate"
			;;
		esac
	done
done

# --- 15. (E) serde_json in the domain only for page results -----------------------------
hits="$(grep -rln 'serde_json' domain/src 2>/dev/null | grep -v '^domain/src/ports/browser\.rs$' || true)"
[ -n "$hits" ] && no "keyharbor-domain uses serde_json outside ports/browser.rs:
$hits"

# --- 16. (F) randomness only in keyharbor-crypto -----------------------------------------
hits="$(grep -rnE 'new_v4\(|OsRng|\brand::|getrandom' --include='*.rs' . |
	grep -v '^\./crypto/' | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true)"
[ -n "$hits" ] && no "randomness outside keyharbor-crypto (use keyharbor_crypto::random):
$hits"
hits="$(grep -nE '^uuid.*"v4"' */Cargo.toml ../Cargo.toml 2>/dev/null || true)"
[ -n "$hits" ] && no "uuid's v4 feature is on (v4 ids come from keyharbor_crypto::random):
$hits"

# --- 17. (G) a driver technology stays in its adapter --------------------------------------
for manifest in */Cargo.toml; do
	crate="${manifest%/Cargo.toml}"
	for dep in $(normal_deps "$manifest"); do
		case "$dep:$crate" in
		security-framework:keystore | rmcp:mcp | clap:cli) ;;
		security-framework:* | rmcp:* | clap:*)
			no "$crate depends on $dep; it belongs to one adapter crate only"
			;;
		esac
	done
done

[ "$bad" -eq 0 ] && echo "layer rules hold"
exit "$bad"

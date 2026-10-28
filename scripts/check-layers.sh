#!/bin/sh
# check-layers.sh: fail when rebuild/ breaks the hexagonal layering of
# docs/design/02 §2.4 and ADR-0014.
#
# 1. Normal dependencies (dev-dependencies excluded) between keyharbor crates
#    follow the allowed table below.
# 2. Only keyharbor-vault depends on or names the AEAD/KDF/MAC crates.
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
set -u
cd "$(dirname "$0")/../rebuild/crates" || exit 1
bad=0
no() { echo "✗ $*"; bad=1; }

# --- 1. allowed dependencies -------------------------------------------------
allowed() {
	case "$1" in
	domain) echo "" ;;
	vault) echo "domain" ;;
	core) echo "domain vault" ;;
	guard) echo "domain core" ;;
	sqlite) echo "domain" ;;
	broker) echo "domain guard" ;;
	mcp) echo "domain core guard broker sqlite vault" ;;
	cli) echo "domain vault core guard sqlite broker mcp" ;;
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
		chacha20poly1305 | hkdf | hmac)
			[ "$crate" = vault ] || no "$crate depends on $dep; only keyharbor-vault uses cryptographic primitives"
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
hits="$(grep -rnE '\b(chacha20poly1305|hkdf|hmac)::' --include='*.rs' . | grep -v '^\./vault/' || true)"
[ -n "$hits" ] && no "cryptographic primitives used outside keyharbor-vault:
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
hits="$(grep -rnE 'keyharbor_broker::(cdp|browser|ops)\b|\bCdpBrowser\b' --include='*.rs' mcp cli 2>/dev/null || true)"
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

[ "$bad" -eq 0 ] && echo "layer rules hold"
exit "$bad"

#!/bin/sh
# smoke-test.sh [binary]: run a keyharbor binary end to end against the REAL
# login keychain, in a throwaway data folder, then remove what it created.
# Run it by hand before tagging a release (it may show a macOS keychain prompt).
#   sh scripts/smoke-test.sh                      # builds rebuild/target/release/keyharbor
#   sh scripts/smoke-test.sh ./dist/…/keyharbor    # a GoReleaser artifact
set -u
cd "$(dirname "$0")/.."
bin="${1:-}"
if [ -z "$bin" ]; then
	(cd rebuild && cargo build --release --locked -p keyharbor -q) || exit 1
	bin="rebuild/target/release/keyharbor"
fi
unset KEYHARBOR_TEST_KEYSTORE_DIR KEYHARBOR_HOME
tmp="$(mktemp -d)"
d1="$tmp/vault-a" d2="$tmp/vault-b"
secret="KH-SMOKE-$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
ids=""
fail=0
log="$tmp/out.log"
step() { echo "· $*"; }
run() { "$bin" "$@" >>"$log" 2>&1 || { echo "✗ keyharbor $*"; fail=1; }; }
cleanup() {
	for id in $ids; do
		security delete-generic-password -s io.github.leeswallow.keyharbor -a "$id" >/dev/null 2>&1
		security delete-generic-password -s io.github.leeswallow.keyharbor -a "$id#next" >/dev/null 2>&1
	done
	rm -rf "$tmp"
}
trap cleanup EXIT

step "$("$bin" --version)"
for d in "$d1" "$d2"; do
	out="$("$bin" --data-dir "$d" vault init 2>&1)" || { echo "✗ init: $out"; exit 1; }
	ids="$ids $(printf '%s' "$out" | cut -f2)"
done
step "init ok (keychain items:$ids)"
printf '{"password":"%s-pw"}' "$secret" | run --data-dir "$d1" vault add blog --kind login --origin https://blog.example.com --set username=me --stdin
printf '{"number":"%s-card","cvc":"9876"}' "$secret" | run --data-dir "$d1" vault add visa --kind card --stdin
printf 'API_KEY=%s-env\n' "$secret" > "$tmp/app.env"
run --data-dir "$d1" vault import-env app "$tmp/app.env"
run --data-dir "$d1" vault list
run --data-dir "$d1" vault rotate-domain payment
run --data-dir "$d1" vault rotate-root
printf 'smoke passphrase\n' | run --data-dir "$d1" vault export "$tmp/backup.age" --passphrase-stdin
printf 'smoke passphrase\n' | run --data-dir "$d2" vault import "$tmp/backup.age" --passphrase-stdin
run --data-dir "$d2" vault list
step "add · import-env · rotate · export · import done"

[ "$("$bin" --data-dir "$d2" vault list 2>/dev/null | wc -l | tr -d ' ')" = 3 ] && step "imported vault has 3 items" || { echo "✗ imported vault does not have 3 items"; fail=1; }
if grep -q "$secret" "$log" || grep -q 9876 "$log"; then echo "✗ a secret value appeared in the output"; fail=1; else step "no secret value in any output"; fi
grep -aq "$secret" "$d1/keyharbor.sqlite3" "$d1/keyharbor.sqlite3-wal" 2>/dev/null && { echo "✗ plaintext in the database"; fail=1; } || step "no plaintext in the database"
head -c 22 "$tmp/backup.age" | grep -q 'age-encryption.org/v1' && step "export is an age file" || { echo "✗ export is not an age file"; fail=1; }

[ "$fail" -eq 0 ] && echo "smoke test passed" || { echo "--- output"; cat "$log"; }
exit "$fail"

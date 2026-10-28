#!/bin/sh
# licenses.sh: write build/THIRD_PARTY_LICENSES.html, the licenses of every crate
# linked into the keyharbor binary. The release archive carries it; GoReleaser
# runs this before building. Needs cargo-about (cargo install cargo-about --locked --features cli).
set -eu
cd "$(dirname "$0")/.."
mkdir -p build
cargo about generate --manifest-path crates/cli/Cargo.toml --config about.toml \
	about.hbs -o build/THIRD_PARTY_LICENSES.html
echo "wrote build/THIRD_PARTY_LICENSES.html"

#!/bin/sh
# check-design.sh: the design-system rules of ADR-0015 (layers, theme parity, contrast, icons,
# classes). The work is in the Rust crate crates/design-check; this only runs it.
# Set KEYHARBOR_TEST_CHROME to also render the showcase in headless Chrome.
set -eu
cd "$(dirname "$0")/.."
cargo test -q -p keyharbor-design-check

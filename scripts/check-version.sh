#!/bin/sh
# check-version.sh <tag>: fail unless the tag is v<version of the keyharbor crate>,
# so a release never ships a binary that reports another version.
set -eu
tag="${1:?usage: check-version.sh v<version>}"
cd "$(dirname "$0")/.."
version="$(cargo pkgid --manifest-path rebuild/Cargo.toml -p keyharbor | sed -E 's/.*[#@]//')"
if [ "$tag" != "v$version" ]; then
	echo "tag $tag does not match crate version $version (rebuild/crates/cli/Cargo.toml)" >&2
	exit 1
fi
echo "tag $tag matches crate version $version"

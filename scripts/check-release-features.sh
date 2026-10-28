#!/bin/sh
# check-release-features.sh: fail when the release build of keyharbor would
# enable a test-only feature. Dev-dependencies are left out on purpose: the CLI
# tests turn on insecure-test-keystore, the shipped binary must not.
set -eu
cd "$(dirname "$0")/../rebuild"
if cargo tree -p keyharbor -e normal,build,features | grep -q insecure-test-keystore; then
	echo "insecure-test-keystore is enabled in the release build" >&2
	exit 1
fi
echo "release build has no test-only features"

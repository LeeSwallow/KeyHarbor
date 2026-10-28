#!/bin/sh
# check-release-features.sh: fail when the release build of keyharbor would
# enable a test-only feature (insecure-test-keystore, test-support). Dev-dependencies
# are left out on purpose: tests turn them on, the shipped binary must not.
set -eu
cd "$(dirname "$0")/../rebuild"
tree="$(cargo tree -p keyharbor -e normal,build,features)"
for feature in insecure-test-keystore test-support; do
	if printf '%s\n' "$tree" | grep -q "feature \"$feature\""; then
		echo "$feature is enabled in the release build" >&2
		exit 1
	fi
done
echo "release build has no test-only features"

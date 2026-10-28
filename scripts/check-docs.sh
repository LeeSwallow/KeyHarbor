#!/bin/sh
# check-docs.sh: fail when a relative Markdown link points to a file that does
# not exist. Checks the root *.md, docs/ and .github/; web links, mailto and
# in-page anchors are skipped.
set -u
cd "$(dirname "$0")/.."
bad=0
files="$(ls ./*.md 2>/dev/null; find docs .github rebuild -name '*.md' -not -path '*/target/*' 2>/dev/null)"
for f in $files; do
	dir="$(dirname "$f")"
	# Links inside fenced code blocks and inline code are examples, not links.
	for link in $(awk '/^[[:space:]]*```/ { fence = !fence; next } !fence' "$f" | sed -E 's/`[^`]*`//g' | grep -oE '\]\([^)[:space:]]+\)' | sed -E 's/^\]\(//; s/\)$//'); do
		case "$link" in
		http://* | https://* | mailto:* | \#*) continue ;;
		esac
		path="${link%%#*}"
		[ -z "$path" ] && continue
		if [ ! -e "$dir/$path" ]; then
			echo "$f: broken link → $link"
			bad=1
		fi
	done
done
[ "$bad" -eq 0 ] && echo "all links resolve"
exit "$bad"

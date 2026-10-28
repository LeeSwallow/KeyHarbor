#!/bin/sh
# check-site.sh: the download page works under the Pages sub-path (/KeyHarbor/):
# every local href/src is relative and points to a file in site/.
set -u
cd "$(dirname "$0")/../site" 2>/dev/null || { echo "no site/ folder"; exit 1; }
bad=0
for f in *.html; do
	for ref in $(grep -oE '(href|src)="[^"]+"' "$f" | sed -E 's/^(href|src)="//; s/"$//'); do
		case "$ref" in
		http://* | https://* | mailto:* | \#*) continue ;;
		/*) echo "$f: absolute path breaks under /KeyHarbor/ → $ref"; bad=1 ;;
		*) [ -e "${ref%%#*}" ] || { echo "$f: missing file → $ref"; bad=1; } ;;
		esac
	done
done
[ "$bad" -eq 0 ] && echo "site ok"
exit "$bad"

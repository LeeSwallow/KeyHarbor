#!/usr/bin/env sh
# Repository settings that cannot live in files: run once after the first
# push, by the maintainer, with the GitHub CLI logged in. Usage:
#   scripts/repo-setup.sh OWNER/REPO
set -eu
repo="${1:?usage: repo-setup.sh OWNER/REPO}"
owner="${repo%%/*}"
name="${repo#*/}"

gh repo edit "$repo" \
  --description "Local vault and browser-session foundation for AI agents" \
  --homepage "https://$(printf '%s' "$owner" | tr '[:upper:]' '[:lower:]').github.io/$name/" \
  --enable-discussions \
  --enable-wiki=false \
  --delete-branch-on-merge \
  --enable-squash-merge --enable-merge-commit=false --enable-rebase-merge=false \
  --add-topic rust --add-topic cli --add-topic macos --add-topic password-manager \
  --add-topic secrets --add-topic keychain --add-topic ai-agents --add-topic mcp \
  --add-topic claude-code --add-topic codex

# The labels: one list, in .github/labels.txt.
sh "$(dirname "$0")/sync-labels.sh" "$repo"
# Security reports reach the maintainer privately, from the Security tab.
gh api -X PUT "repos/$repo/private-vulnerability-reporting" >/dev/null && echo "private vulnerability reporting: on"
# The download page is built by .github/workflows/pages.yml.
gh api -X POST "repos/$repo/pages" -f build_type=workflow >/dev/null 2>&1 && echo "pages: on (workflow)" || echo "pages: already on"
# main takes changes only when CI is green. The maintainer may still push directly.
gh api -X PUT "repos/$repo/branches/main/protection" --input - >/dev/null <<'JSON'
{
  "required_status_checks": { "strict": false, "contexts": ["test (macos-latest)", "test (ubuntu-latest)"] },
  "enforce_admins": false,
  "required_pull_request_reviews": null,
  "restrictions": null
}
JSON
echo "branch protection: main requires CI (admins exempt)"

printf '\nDone. Still by hand: gh secret set HOMEBREW_TAP_TOKEN -R %s\n' "$repo"

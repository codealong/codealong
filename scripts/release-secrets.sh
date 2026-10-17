#!/usr/bin/env bash
# Gives GitHub what it builds releases with (.github/workflows/release.yml) and moves the Homebrew
# cask with (homebrew.yml), once, after the repository is public: environments need a paid plan
# on a private one. Run it again to replace a key.
#
#   scripts/release-secrets.sh            both environments and every secret
#   scripts/release-secrets.sh --check    says what's set, changing nothing
#
# `release` holds the signing keys: only v* tags may use it, and each build waits for the person
# who ran this to approve it on GitHub, so neither a workflow changed on a branch nor a tag pushed
# by an agent signs anything unseen. `homebrew` holds a deploy key that can push to
# codealong/homebrew-tap alone, made here; v* tags only, no approval, so publishing a release
# moves the cask by itself.
#
# The keys come from 1Password's Private vault, as design/plans/releases.md lists them, through
# `op` (which asks you to allow it), and go to GitHub on gh's standard input: nothing is written
# to disk but the .p12 and the new deploy key, in a temporary folder removed on exit.
set -euo pipefail
cd "$(dirname "$0")/.."

repo="${CODEALONG_REPO:-codealong/codealong}"
tap="${CODEALONG_TAP:-codealong/homebrew-tap}"
vault="${CODEALONG_VAULT:-Private}"
certificate_item="Codealong Developer ID signing key (Gordon Hempton, N3347S68VA)"
notary_item="Codealong notarization key (App Store Connect API)"
update_item="Codealong update signing key"

fail() {
  echo "$*" >&2
  exit 1
}

check=0
case "${1:-}" in
  "") ;;
  --check) check=1 ;;
  -h | --help)
    sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'
    exit 0
    ;;
  *) fail "Unknown option $1; see scripts/release-secrets.sh --help." ;;
esac

gh auth status >/dev/null 2>&1 || fail "gh isn't signed in (gh auth login)."
[ "$(gh api "repos/$repo" --jq .private)" = false ] || fail "$repo is private, where environments need a paid plan: make it public first, then run this."

if [ "$check" = 1 ]; then
  for env in release homebrew; do
    if gh api "repos/$repo/environments/$env" >/dev/null 2>&1; then
      echo "$env: $(gh secret list --repo "$repo" --env "$env" --json name --jq 'map(.name) | join(", ")')"
      echo "  may deploy: $(gh api "repos/$repo/environments/$env/deployment-branch-policies" --jq '[.branch_policies[] | .type + " " + .name] | join(", ")')"
    else
      echo "$env: not made yet"
    fi
  done
  exit 0
fi

command -v op >/dev/null || fail "This reads the keys from 1Password with its command line, op: brew install 1password-cli."
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# An environment that only v* tags may deploy to, waiting on the given reviewers.
environment() {
  local name="$1" reviewers="$2"
  gh api -X PUT "repos/$repo/environments/$name" --input - >/dev/null <<JSON
{"reviewers": $reviewers, "deployment_branch_policy": {"protected_branches": false, "custom_branch_policies": true}}
JSON
  if ! gh api "repos/$repo/environments/$name/deployment-branch-policies" --jq '.branch_policies[] | select(.type == "tag") | .name' | grep -qx 'v\*'; then
    gh api -X POST "repos/$repo/environments/$name/deployment-branch-policies" -f name='v*' -f type=tag >/dev/null
  fi
}

# An item's ID (its name has characters op:// references don't allow), and the first of its
# attached files whose name ends as given.
item_id() { op item get "$1" --vault "$vault" --format json | node -p 'JSON.parse(require("fs").readFileSync(0, "utf8")).id'; }
attachment() {
  op item get "$1" --vault "$vault" --format json |
    node -e 'const i = JSON.parse(require("fs").readFileSync(0, "utf8")); const f = (i.files || []).find((f) => f.name.endsWith(process.argv[1])); if (!f) process.exit(1); console.log(f.name);' "$2" ||
    fail "\"$1\" in 1Password has no attached $2 file."
}
# A field's value, by the first label that matches.
field() {
  op item get "$1" --vault "$vault" --format json --reveal |
    node -e 'const i = JSON.parse(require("fs").readFileSync(0, "utf8")); const re = new RegExp(process.argv[1], "i"); const f = (i.fields || []).find((f) => re.test(f.label || "") && f.value); if (!f) process.exit(1); process.stdout.write(f.value);' "$2" ||
    fail "\"$1\" in 1Password has no field labelled like /$2/."
}
secret() { gh secret set "$1" --repo "$repo" --env "$2" >/dev/null; }

me="$(gh api user --jq .id)"
environment release "[{\"type\": \"User\", \"id\": $me}]"
environment homebrew "[]"
echo "Environments: release (v* tags, waits for $(gh api user --jq .login) to approve), homebrew (v* tags)."

echo "Reading the Developer ID certificate."
id="$(item_id "$certificate_item")"
p12="$(attachment "$certificate_item" .p12)"
op read --out-file "$work/certificate.p12" "op://$vault/$id/$p12" >/dev/null
base64 -i "$work/certificate.p12" | secret APPLE_CERTIFICATE release
op read "op://$vault/$id/password" | secret APPLE_CERTIFICATE_PASSWORD release

echo "Reading the notary's key."
id="$(item_id "$notary_item")"
p8="$(attachment "$notary_item" .p8)"
op read "op://$vault/$id/$p8" | secret APPLE_API_KEY_P8 release
# Apple names the file after the key: AuthKey_<key ID>.p8.
key_id="${p8#AuthKey_}"
key_id="${key_id%.p8}"
printf '%s' "$key_id" | secret APPLE_API_KEY release
field "$notary_item" 'issuer' | secret APPLE_API_ISSUER release

echo "Reading the update key."
id="$(item_id "$update_item")"
key="$(attachment "$update_item" .key)"
op read "op://$vault/$id/$key" | secret TAURI_SIGNING_PRIVATE_KEY release
op read "op://$vault/$id/password" | secret TAURI_SIGNING_PRIVATE_KEY_PASSWORD release

# A deploy key of the tap's own, replacing the one made before: it can push to the tap and
# nowhere else, and only GitHub keeps its private half.
echo "Making the tap's deploy key."
title="Codealong releases ($repo homebrew.yml)"
for old in $(gh api "repos/$tap/keys" --jq ".[] | select(.title == \"$title\") | .id"); do
  gh api -X DELETE "repos/$tap/keys/$old" >/dev/null
done
ssh-keygen -q -t ed25519 -N "" -C "$title" -f "$work/tap"
gh api -X POST "repos/$tap/keys" -f title="$title" -f key="$(cat "$work/tap.pub")" -F read_only=false >/dev/null
secret HOMEBREW_TAP_KEY homebrew <"$work/tap"

echo
echo "Done. release: $(gh secret list --repo "$repo" --env release --json name --jq 'map(.name) | join(", ")')"
echo "      homebrew: $(gh secret list --repo "$repo" --env homebrew --json name --jq 'map(.name) | join(", ")')"

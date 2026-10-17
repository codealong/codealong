#!/usr/bin/env bash
# Starts a release: sets the version of the app, the command and the Claude Code plugin, which
# are one number, commits "Release <version>" and tags it v<version> with the release notes as
# the tag's message. Pushing the tag has GitHub build the release (.github/workflows/release.yml);
# scripts/release.sh builds the same on a Mac with the keys (design/plans/releases.md).
#
#   scripts/version.sh 0.3.0                    notes: the commit subjects since the last tag
#   scripts/version.sh 0.3.0 --notes notes.md   notes from a file (the first release needs one)
#   scripts/version.sh 0.3.0 --push             pushes the commit to main and the tag after
#
# The version is x.y.z, or x.y.z-beta.n for a prerelease, and newer than the app's: the updater
# only offers a newer version, and Claude Code updates the plugin whenever its version changes.
# It runs where HEAD is origin/main with nothing uncommitted (the main checkout, or a worktree
# made for it: git worktree add --detach ../codealong-release origin/main), so the release commit
# is the next one on main and the tag names what main has.
set -euo pipefail
cd "$(dirname "$0")/.."

fail() {
  echo "$*" >&2
  exit 1
}

version=""
notes_file=""
push=0
while [ $# -gt 0 ]; do
  case "$1" in
    --notes)
      [ $# -ge 2 ] || fail "--notes takes the notes' file."
      notes_file="$2"
      shift 2
      ;;
    --push) push=1 && shift ;;
    -h | --help)
      sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'
      exit 0
      ;;
    -*) fail "Unknown option $1; see scripts/version.sh --help." ;;
    *)
      [ -z "$version" ] || fail "One version at a time; see scripts/version.sh --help."
      version="$1" && shift
      ;;
  esac
done
[ -n "$version" ] || fail "Which version? scripts/version.sh <x.y.z>; see --help."
# Numbers without leading zeros, as Cargo and the updater read them, and a prerelease only as
# alpha.n, beta.n or rc.n, whose order is then the order of their names and numbers.
[[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-(alpha|beta|rc)\.(0|[1-9][0-9]*))?$ ]] ||
  fail "$version isn't a version: x.y.z, or x.y.z-beta.1 (alpha, beta or rc) for a prerelease."
[ -z "$notes_file" ] || [ -f "$notes_file" ] || fail "No release notes at $notes_file."

[ -z "$(git status --porcelain)" ] || fail "This checkout has uncommitted changes: a release is a commit of its own."
git fetch -q origin main --tags
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || fail "This checkout isn't at origin/main: a release is the next commit on main, so start from origin/main (see --help)."
! git rev-parse -q --verify "refs/tags/v$version" >/dev/null || fail "v$version is tagged already: a release's files never change, so pick a new version."

current="$(node -p "require('./src-tauri/tauri.conf.json').version")"
# Newer by semver's order: the numbers in turn, a prerelease before its release, and between
# prereleases alpha, beta, rc, then their numbers.
newer="$(node -e '
const parse = (v) => {
  const [core, pre] = v.split("-");
  const [label, n] = pre ? pre.split(".") : ["~", "0"];
  return [...core.split(".").map(Number), label, Number(n)];
};
const [a, b] = process.argv.slice(1).map(parse);
const i = a.findIndex((x, i) => x !== b[i]);
console.log(i >= 0 && a[i] > b[i]);
' "$version" "$current")"
[ "$newer" = true ] || fail "$version isn't newer than the app's $current: installed copies would never take it."

notes="$(mktemp)"
trap 'rm -f "$notes"' EXIT
if [ -n "$notes_file" ]; then
  cp "$notes_file" "$notes"
elif last="$(git describe --tags --abbrev=0 --match 'v*' 2>/dev/null)"; then
  git log --no-merges --format='- %s' "$last..HEAD" >"$notes"
else
  fail "There's no release before this one to list the commits since: write the notes in a file and pass --notes <file>."
fi
[ -s "$notes" ] || fail "The release notes are empty."

# The app's, the engine's (and its lockfile entry), the UI's, and the plugin's in both places
# Claude Code reads it. Only the version's own line changes in each, so the files keep their
# layout.
node -e '
const fs = require("fs");
const v = process.argv[1];
const edit = (file, from, to) => {
  const text = fs.readFileSync(file, "utf8");
  const next = text.replace(from, to);
  if (next === text && !text.match(from)) throw new Error(`No version found in ${file}`);
  fs.writeFileSync(file, next);
};
const json = /("version": ")[^"]*(")/;
edit("src-tauri/tauri.conf.json", json, `$1${v}$2`);
edit("plugin/.claude-plugin/plugin.json", json, `$1${v}$2`);
edit(".claude-plugin/marketplace.json", json, `$1${v}$2`);
edit("package.json", json, `$1${v}$2`);
edit("package-lock.json", /^(\{\s*"name": "codealong",\s*"version": ")[^"]*(")/, `$1${v}$2`);
edit("package-lock.json", /("packages": \{\s*"": \{\s*"name": "codealong",\s*"version": ")[^"]*(")/, `$1${v}$2`);
edit("src-tauri/Cargo.toml", /^(\[package\]\nname = "codealong"\nversion = ")[^"]*(")/m, `$1${v}$2`);
edit("src-tauri/Cargo.lock", /(\nname = "codealong"\nversion = ")[^"]*(")/, `$1${v}$2`);
' "$version"

git add src-tauri/tauri.conf.json plugin/.claude-plugin/plugin.json .claude-plugin/marketplace.json \
  package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -q -m "Release $version"
# Kept as written: git would otherwise drop a notes line starting with # (a heading).
git tag -a --cleanup=whitespace "v$version" -F "$notes"
echo "Committed \"Release $version\" ($(git rev-parse --short HEAD)) and tagged it v$version."

if [ "$push" = 1 ]; then
  git push -q origin HEAD:main
  git push -q origin "v$version"
  echo "Pushed main and v$version: GitHub builds the release as a draft, to publish once it's tried."
else
  echo "Next: git push origin HEAD:main && git push origin v$version"
  echo "  The tag has GitHub build the release as a draft; or scripts/release.sh --draft builds it here."
fi

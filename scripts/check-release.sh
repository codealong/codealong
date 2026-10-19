#!/usr/bin/env bash
# shellcheck disable=SC2016 # the node programs below are single-quoted on purpose
# Checks a release on GitHub, draft or published, as someone who downloads it gets it, before it's
# published: every file it should have, byte for byte what GitHub serves; the disk image and the
# app in it accepted by Gatekeeper once marked as downloaded (a browser's quarantine), notarized
# with the ticket stapled, signed by the Developer ID with the app's entitlements, and the version
# it says; the license files inside; the update archive the same build, its signature good against
# the key in tauri.conf.json, and latest.json pointing at it. The /release skill
# (.claude/skills/release) runs it after GitHub builds the draft.
#
#   scripts/check-release.sh 0.3.0
#
# It downloads the release (about 70 MB) into a temporary folder it removes, mounts nothing it
# leaves mounted, and runs nothing it downloaded but `--version` of the app's command.
set -euo pipefail
cd "$(dirname "$0")/.."

version="${1:-}"
[ -n "$version" ] || { echo "Usage: scripts/check-release.sh <version>" >&2; exit 2; }
repo="${CODEALONG_REPO:-codealong/codealong}"
tag="v$version"
work="$(mktemp -d)"
# shellcheck disable=SC2329 # it runs from the trap below
cleanup() {
  hdiutil detach -quiet "$work/mnt" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

failed=0
check() {
  if [ "$2" = 0 ]; then
    echo "  ok    $1"
  else
    echo "  FAIL  $1${3:+: $3}"
    failed=1
  fi
}
# Checks a test (`holds <name> <why it failed> test …`), saying why when it fails.
holds() {
  local name="$1" why="$2"
  shift 2
  if "$@"; then check "$name" 0; else check "$name" 1 "$why"; fi
}
# Runs a command and checks it succeeded, saying its last line when it didn't.
runs() {
  local name="$1" out
  shift
  if out="$("$@" 2>&1)"; then check "$name" 0; else check "$name" 1 "$(echo "$out" | tail -1)"; fi
}

echo "$repo $tag"
gh release view "$tag" -R "$repo" --json isDraft,isPrerelease,name,assets,body >"$work/release.json"
node -e '
  const r = require(process.argv[1]);
  console.log(`  ${r.name}: ${r.isDraft ? "draft" : "published"}${r.isPrerelease ? ", prerelease" : ""}, ${r.assets.length} files, notes ${r.body.length} characters`);
' "$work/release.json"

echo "The files, as GitHub serves them:"
gh release download "$tag" -R "$repo" -D "$work/dl"
dmg="Codealong_${version}_universal.dmg"
archive="Codealong_${version}_universal.app.tar.gz"
for f in Codealong.dmg "$dmg" "$dmg.sha256" "$archive" "$archive.sig" latest.json; do
  holds "has $f" "" test -f "$work/dl/$f"
done
# GitHub's own digest of each file, against what came down.
node -e '
  const r = require(process.argv[1]);
  for (const a of r.assets) if (a.digest) console.log(a.digest.replace(/^sha256:/, "") + "  " + a.name);
' "$work/release.json" >"$work/digests"
(cd "$work/dl" && shasum -a 256 -c --status "$work/digests"); check "each file matches GitHub's digest" $?
(cd "$work/dl" && shasum -a 256 -c --status "$dmg.sha256"); check "$dmg.sha256 matches" $?
cmp -s "$work/dl/Codealong.dmg" "$work/dl/$dmg"; check "Codealong.dmg is $dmg" $?

echo "The disk image, downloaded with a browser:"
xattr -w com.apple.quarantine "0083;$(printf %x "$(date +%s)");Safari;" "$work/dl/Codealong.dmg"
gk="$(spctl -a -t open --context context:primary-signature -vv "$work/dl/Codealong.dmg" 2>&1 || true)"
echo "$gk" | grep -q 'accepted' && echo "$gk" | grep -q 'Notarized Developer ID'; check "Gatekeeper accepts it as notarized" $? "$(echo "$gk" | tr '\n' ' ')"
runs "its notarization ticket is stapled" xcrun stapler validate "$work/dl/Codealong.dmg"
hdiutil attach -quiet -nobrowse -readonly -noautoopen -mountpoint "$work/mnt" "$work/dl/Codealong.dmg"
holds "it has the Applications link to drag to" "" test -L "$work/mnt/Applications"
ditto "$work/mnt/Codealong.app" "$work/Codealong.app"
hdiutil detach -quiet "$work/mnt"
app="$work/Codealong.app"

echo "The app, copied out of it:"
gk="$(spctl -a -t exec -vv "$app" 2>&1 || true)"
echo "$gk" | grep -q 'accepted' && echo "$gk" | grep -q 'Notarized Developer ID'; check "Gatekeeper accepts it as notarized" $? "$(echo "$gk" | tr '\n' ' ')"
runs "its notarization ticket is stapled" xcrun stapler validate "$app"
runs "its signature holds, everything in it too" codesign --verify --deep --strict "$app"
runs "Apple's distribution checks pass" syspolicy_check distribution "$app"
signed="$(codesign -dvv "$app" 2>&1 || true)"
echo "$signed" | grep -q '^Authority=Developer ID Application:'; check "signed with a Developer ID" $?
echo "$signed" | grep -q 'flags=.*runtime'; check "with the hardened runtime" $?
codesign -d --entitlements - --xml "$app" 2>/dev/null | grep -q 'com.apple.security.automation.apple-events'; check "and may drive terminals (Apple Events)" $?
said="$(plutil -extract CFBundleShortVersionString raw "$app/Contents/Info.plist" 2>/dev/null || true)"
holds "it says it's $version" "it says $said" test "$said" = "$version"
said="$("$app/Contents/MacOS/codealong" --version 2>&1 || true)"
holds "its command says so too" "$said" test "$said" = "codealong $version"
for f in LICENSE NOTICE THIRD_PARTY_LICENSES.txt; do
  holds "has $f" "" test -s "$app/Contents/Resources/$f"
done

echo "The update installed copies get:"
mkdir "$work/update"
tar -xzf "$work/dl/$archive" -C "$work/update"
cdhash() { codesign -dvvv "$1" 2>&1 | grep '^CDHash=' || true; }
[ -n "$(cdhash "$app")" ] && [ "$(cdhash "$work/update/Codealong.app")" = "$(cdhash "$app")" ]; check "the archive holds the same build" $?
# The updater's check: a minisign signature (prehashed with BLAKE2b, "ED") by the key whose public
# half is tauri.conf.json's, over the archive, and over its trusted comment.
node -e '
  const fs = require("fs"), crypto = require("crypto");
  const [conf, file, sigFile, latest, version, archive] = process.argv.slice(1);
  const b64 = (s) => Buffer.from(s.trim(), "base64");
  const pub = b64(b64(JSON.parse(fs.readFileSync(conf, "utf8")).plugins.updater.pubkey).toString().split("\n")[1]);
  const sigText = fs.readFileSync(sigFile, "utf8").trim();
  const lines = b64(sigText).toString().split("\n");
  const sig = b64(lines[1]), trusted = lines[2].replace(/^trusted comment: /, ""), global = b64(lines[3]);
  const key = crypto.createPublicKey({ key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), pub.subarray(10)]), format: "der", type: "spki" });
  const data = fs.readFileSync(file);
  const msg = sig.subarray(0, 2).toString() === "ED" ? crypto.createHash("blake2b512").update(data).digest() : data;
  const say = (name, ok, why) => console.log(`  ${ok ? "ok  " : "FAIL"}  ${name}${ok || !why ? "" : ": " + why}`);
  const results = [];
  const check = (name, ok, why) => { say(name, ok, why); results.push(ok); };
  check("its signature is by the app'"'"'s update key", pub.subarray(2, 10).equals(sig.subarray(2, 10)) && crypto.verify(null, msg, key, sig.subarray(10)));
  check("and so is its trusted comment", crypto.verify(null, Buffer.concat([sig.subarray(10), Buffer.from(trusted)]), key, global), trusted);
  const m = JSON.parse(fs.readFileSync(latest, "utf8"));
  check(`latest.json says ${version}`, m.version === version, m.version);
  check("with notes", typeof m.notes === "string" && m.notes.trim().length > 0);
  for (const p of ["darwin-aarch64", "darwin-x86_64"]) {
    const e = m.platforms && m.platforms[p];
    check(`for ${p}, this archive and its signature`, !!e && e.url.endsWith(`/v${version}/${archive}`) && e.signature.trim() === sigText, e && e.url);
  }
  process.exit(results.every(Boolean) ? 0 : 1);
' src-tauri/tauri.conf.json "$work/dl/$archive" "$work/dl/$archive.sig" "$work/dl/latest.json" "$version" "$archive" || failed=1

echo
if [ "$failed" = 0 ]; then
  echo "All good: $tag is as a download should be."
else
  echo "Something's wrong with $tag: see FAIL above."
fi
exit "$failed"

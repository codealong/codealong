#!/usr/bin/env bash
# Builds Codealong's installer for other Macs: the app for Apple silicon and Intel, signed with a
# Developer ID and notarized by Apple, in a disk image whose window says to drag it into
# Applications. The app relaunch.sh builds is for this Mac only: signed with an Apple Development
# certificate or ad hoc, one architecture, never notarized, and other Macs refuse to open it.
#
#   scripts/release.sh                  the installer, from a clean checkout
#   scripts/release.sh --arch arm64     one architecture (arm64, x86_64, or the default universal)
#   scripts/release.sh --no-notarize    signed but not notarized: opens on this Mac only
#   scripts/release.sh --allow-dirty    builds uncommitted changes too; never ship that
#   scripts/release.sh --notes <file>   release notes (default: commit subjects since the last tag)
#   scripts/release.sh --draft          also makes a draft GitHub release with every file
#
# Signing uses the release team's "Developer ID Application" certificate from the keychain
# (below; APPLE_SIGNING_IDENTITY names it exactly). Notarizing needs one of, for that team:
#   CODEALONG_NOTARY_PROFILE  credentials saved in the keychain, once per Mac, with
#                             `xcrun notarytool store-credentials <name> --key <AuthKey_ID.p8>
#                             --key-id <ID> --issuer <issuer>` (or --apple-id and --team-id,
#                             which asks for an app-specific password)
#   APPLE_API_KEY and APPLE_API_ISSUER, an App Store Connect API key's ID and issuer, with the
#                             key at APPLE_API_KEY_PATH or ~/.appstoreconnect/private_keys/
# Passwords never go on a command line, where other processes could read them.
#
# Each release carries its update, which installed copies download (updates.rs): the app as a
# .app.tar.gz signed with the update key, and latest.json, the manifest they ask for. The key
# comes in TAURI_SIGNING_PRIVATE_KEY (the key, or a path to it) and
# TAURI_SIGNING_PRIVATE_KEY_PASSWORD; kept in a password manager, `op run` hands them over without
# writing them anywhere (design/plans/releases.md has the command). Without it, only a
# --no-notarize build goes ahead, with no update.
#
# The tests run before the build (npm test, cargo test): a failing one stops the release with
# nothing built, signed or sent to Apple.
#
# The app is notarized and stapled before it goes into the disk image, then the disk image is
# signed, notarized and stapled too, so it opens without a warning even offline. It all builds
# in a folder of its own beside the shared one (see target-dir.sh): a release compiles
# everything again, for two architectures and with link-time optimization, and doing that in the
# shared folder would hold up every other build on this Mac and throw away its cache. The
# installer lands in <that folder>/dist/<version>/, printed at the end.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

arch=universal
notarize=1
dirty=0
draft=0
notes_file=""
while [ $# -gt 0 ]; do
  case "$1" in
    --arch)
      arch="${2:-}"
      shift 2 || true
      ;;
    --no-notarize) notarize=0 && shift ;;
    --allow-dirty) dirty=1 && shift ;;
    --notes)
      notes_file="${2:-}"
      shift 2 || true
      ;;
    --draft) draft=1 && shift ;;
    -h | --help)
      sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "Unknown option $1; see scripts/release.sh --help." >&2
      exit 2
      ;;
  esac
done
# The names follow Tauri's own bundles: Codealong_<version>_<aarch64|x64|universal>.dmg. The
# platforms are the updater's names for the Macs an update is for.
case "$arch" in
  universal) triples=(aarch64-apple-darwin x86_64-apple-darwin) target=universal-apple-darwin archs="arm64 x86_64" platforms="darwin-aarch64 darwin-x86_64" ;;
  arm64 | aarch64) arch=aarch64 triples=(aarch64-apple-darwin) target=aarch64-apple-darwin archs=arm64 platforms=darwin-aarch64 ;;
  x86_64 | x64) arch=x64 triples=(x86_64-apple-darwin) target=x86_64-apple-darwin archs=x86_64 platforms=darwin-x86_64 ;;
  *)
    echo "--arch takes arm64, x86_64 or universal." >&2
    exit 2
    ;;
esac

fail() {
  echo "$*" >&2
  exit 1
}

# Everything that can stop a release is checked before the long build starts.
if [ "$dirty" = 0 ] && [ -n "$(git status --porcelain)" ]; then
  fail "This checkout has uncommitted changes. An installer is built from a commit: commit them, or pass --allow-dirty to try one anyway."
fi
[ -d node_modules ] || fail "Run npm ci first: the build compiles the UI."

version="$(node -p "require('./src-tauri/tauri.conf.json').version")"
cargo_version="$(sed -n 's/^version = "\(.*\)"$/\1/p' src-tauri/Cargo.toml | head -1)"
npm_version="$(node -p "require('./package.json').version")"
if [ "$cargo_version" != "$version" ] || [ "$npm_version" != "$version" ]; then
  fail "The version differs between tauri.conf.json ($version), Cargo.toml ($cargo_version) and package.json ($npm_version)."
fi

# Releases are signed by one Apple developer team, Gordon Hempton's own, and never by another
# certificate that happens to be in the keychain: the team is what macOS remembers people's
# permissions by, and the name it shows. CODEALONG_TEAM_ID names another team, for a fork.
team="${CODEALONG_TEAM_ID:-N3347S68VA}"
identity="${APPLE_SIGNING_IDENTITY:-}"
if [ -z "$identity" ]; then
  identity="$(security find-identity -v -p codesigning | sed -n "s/.*\"\(Developer ID Application: .* ($team)\)\"$/\1/p" | head -1)"
  [ -n "$identity" ] || fail "No Developer ID Application certificate for team $team in the keychain. Its account holder makes one at developer.apple.com (Certificates, +, Developer ID Application) and opens the downloaded file on this Mac."
fi
case "$identity" in
  "Developer ID Application: "*"($team)") ;;
  *) fail "APPLE_SIGNING_IDENTITY is \"$identity\", which isn't a Developer ID of team $team (CODEALONG_TEAM_ID)." ;;
esac

notary=()
if [ "$notarize" = 1 ]; then
  if [ -n "${CODEALONG_NOTARY_PROFILE:-}" ]; then
    notary=(--keychain-profile "$CODEALONG_NOTARY_PROFILE")
  elif [ -n "${APPLE_API_KEY:-}" ] && [ -n "${APPLE_API_ISSUER:-}" ]; then
    key="${APPLE_API_KEY_PATH:-}"
    if [ -z "$key" ]; then
      for d in ./private_keys ~/private_keys ~/.private_keys ~/.appstoreconnect/private_keys; do
        if [ -f "$d/AuthKey_$APPLE_API_KEY.p8" ]; then
          key="$d/AuthKey_$APPLE_API_KEY.p8"
          break
        fi
      done
    fi
    [ -f "$key" ] || fail "No key file for APPLE_API_KEY $APPLE_API_KEY: set APPLE_API_KEY_PATH, or put AuthKey_$APPLE_API_KEY.p8 in ~/.appstoreconnect/private_keys."
    notary=(--key "$key" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER")
  else
    fail "Notarizing needs CODEALONG_NOTARY_PROFILE, or APPLE_API_KEY and APPLE_API_ISSUER (see scripts/release.sh --help). --no-notarize builds an installer that opens on this Mac only."
  fi
  xcrun notarytool history "${notary[@]}" --output-format json >/dev/null ||
    fail "Apple's notary service didn't accept those credentials (xcrun notarytool history failed)."
fi

updates=0
[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && updates=1
if [ "$updates" = 0 ] && [ "$notarize" = 1 ]; then
  fail "A release carries its update, signed with the update key: set TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD (see scripts/release.sh --help)."
fi
[ -z "$notes_file" ] || [ -f "$notes_file" ] || fail "No release notes at $notes_file."
if [ "$draft" = 1 ]; then
  [ "$notarize" = 1 ] || fail "A draft release is for handing out, so it's notarized: drop --no-notarize."
  gh auth status >/dev/null 2>&1 || fail "--draft makes the release with gh, which isn't signed in (gh auth login)."
  ! git rev-parse -q --verify "refs/tags/v$version" >/dev/null || fail "v$version is tagged already: a release's files never change, so set a new version first."
  ! gh release view "v$version" >/dev/null 2>&1 || fail "GitHub has a v$version release already: set a new version first."
fi

# The tests come last of these checks: the ones above take a moment and these a few minutes,
# still nothing beside the build and Apple's notary, which a failure here saves. The engine's
# build where tests always do, in the shared folder (CODEALONG_CHECKOUT has cargo build this
# checkout's own crates there, not link another checkout's: lib.rs CHECKOUT), and after the UI,
# which Tauri reads from dist/ as the engine compiles.
shared="$(scripts/target-dir.sh)"
echo "Running the tests before anything is built."
npm test || fail "npm test failed: nothing was built."
npm run build >/dev/null || fail "The UI doesn't build (npm run build): nothing was built."
(cd src-tauri && CARGO_TARGET_DIR="$shared" cargo test) || fail "cargo test failed: nothing was built."

for triple in "${triples[@]}"; do
  rustup target list --installed | grep -qx "$triple" || rustup target add "$triple"
done

build="$(dirname "$shared")/codealong-release"
dist="$build/dist/$version"
mkdir -p "$build" "$dist"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# dmgbuild draws the disk image's window; it lives in a virtualenv of the release folder's own.
# 1.6.7 is the first whose background Finder shows on macOS 26.2 and later (earlier ones write a
# bookmark Finder there can't resolve, and the window comes up blank), and it needs Python 3.10,
# newer than the 3.9 macOS comes with. A new version or a Python upgrade gets a fresh virtualenv.
dmgbuild_version=1.6.7
venv="$build/tools/dmgbuild-$dmgbuild_version"
if ! "$venv/bin/python" -c 'import dmgbuild' 2>/dev/null; then
  python=""
  for p in python3 /opt/homebrew/bin/python3 /usr/local/bin/python3; do
    if command -v "$p" >/dev/null && "$p" -c 'import sys; sys.exit(sys.version_info < (3, 10))' 2>/dev/null; then
      python="$(command -v "$p")"
      break
    fi
  done
  [ -n "$python" ] || fail "The disk image's window is made with dmgbuild, which needs Python 3.10 or later (macOS's own is 3.9): brew install python."
  rm -rf "$venv"
  "$python" -m venv "$venv"
  "$venv/bin/pip" install --quiet --disable-pip-version-check "dmgbuild==$dmgbuild_version"
fi

echo "Building Codealong $version ($archs), signed as $identity."
# Tauri signs the app with the hardened runtime and Entitlements.plist. It would notarize it too
# when it sees Apple's variables, so they're kept from it: this script notarizes, with the
# keychain profile as well.
env -u APPLE_API_KEY -u APPLE_API_ISSUER -u APPLE_API_KEY_PATH -u APPLE_ID -u APPLE_PASSWORD -u APPLE_TEAM_ID \
  APPLE_SIGNING_IDENTITY="$identity" CARGO_TARGET_DIR="$build" CODEALONG_RELEASE=1 \
  CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_INCREMENTAL=false \
  npx tauri build --ci --target "$target" --bundles app

app="$build/$target/release/bundle/macos/Codealong.app"
[ -d "$app" ] || fail "The build finished without $app."

# The checks Gatekeeper makes, before anything goes to Apple.
codesign --verify --deep --strict "$app" || fail "The app's signature doesn't verify."
signature="$(codesign -dvv "$app" 2>&1)"
grep -q "^Authority=Developer ID Application" <<<"$signature" || fail "The app isn't signed with a Developer ID."
grep -q "flags=.*runtime" <<<"$signature" || fail "The app isn't signed with the hardened runtime."
grep -q "^Timestamp=" <<<"$signature" || fail "The app's signature has no secure timestamp, which notarizing requires."
codesign -d --entitlements - --xml "$app" 2>/dev/null | grep -q "com.apple.security.automation.apple-events" ||
  fail "The app's signature is missing its entitlements (Entitlements.plist): it couldn't script Ghostty."
built_archs="$(lipo -archs "$app/Contents/MacOS/codealong")"
for a in $archs; do
  grep -qw "$a" <<<"$built_archs" || fail "The app is built for $built_archs, not $archs."
done

# Submits a file to Apple's notary service and waits; on a refusal, prints Apple's log of why.
notarize() {
  local file="$1" what="$2" out id status
  echo "Notarizing $what (Apple usually takes a few minutes)…"
  out="$(xcrun notarytool submit "$file" "${notary[@]}" --wait --output-format json)" || true
  id="$(node -p 'JSON.parse(require("fs").readFileSync(0, "utf8")).id || ""' <<<"$out" 2>/dev/null || true)"
  status="$(node -p 'JSON.parse(require("fs").readFileSync(0, "utf8")).status || ""' <<<"$out" 2>/dev/null || true)"
  if [ "$status" != Accepted ]; then
    echo "Apple didn't notarize $what (${status:-no answer}): $out" >&2
    [ -n "$id" ] && xcrun notarytool log "$id" "${notary[@]}" >&2
    exit 1
  fi
}

if [ "$notarize" = 1 ]; then
  ditto -c -k --keepParent "$app" "$work/Codealong.zip"
  notarize "$work/Codealong.zip" "the app"
  xcrun stapler staple "$app"
fi

# The update: the stapled app as an archive, signed with the update key. The updater unpacks
# every entry as a file, so the archive has none of macOS's own (AppleDouble files, attributes).
archive="$dist/Codealong_${version}_${arch}.app.tar.gz"
rm -f "$archive" "$archive.sig" "$dist/latest.json" "$dist/notes.md"
if [ "$updates" = 1 ]; then
  COPYFILE_DISABLE=1 tar --no-mac-metadata --no-xattrs -czf "$archive" -C "$(dirname "$app")" Codealong.app
  npx tauri signer sign --app-version "$version" "$archive" >/dev/null
  [ -f "$archive.sig" ] || fail "Signing the update didn't write $archive.sig."
  # Installed copies trust only the public key in tauri.conf.json, so a signature by any other
  # key is refused on every Mac: the key IDs have to match.
  key_id='const t=Buffer.from(process.argv[1],"base64").toString().split("\n")[1]; console.log(Buffer.from(t,"base64").subarray(2,10).toString("hex"))'
  signed_by="$(node -e "$key_id" "$(cat "$archive.sig")")"
  trusted="$(node -e "$key_id" "$(node -p "require('./src-tauri/tauri.conf.json').plugins.updater.pubkey")")"
  [ "$signed_by" = "$trusted" ] || fail "The update is signed with key $signed_by, but the app trusts only $trusted (plugins.updater.pubkey)."
  mkdir "$work/unpacked"
  tar -xzf "$archive" -C "$work/unpacked"
  codesign --verify --deep --strict "$work/unpacked/Codealong.app" || fail "The app unpacked from the update doesn't pass codesign --verify --strict."
  [ "$(plutil -extract CFBundleShortVersionString raw "$work/unpacked/Codealong.app/Contents/Info.plist")" = "$version" ] ||
    fail "The app in the update isn't version $version."

  # The notes go in the GitHub release and in latest.json, which the app shows beside Restart.
  if [ -n "$notes_file" ]; then
    cp "$notes_file" "$dist/notes.md"
  elif last="$(git describe --tags --abbrev=0 --match 'v*' 2>/dev/null)"; then
    git log --no-merges --format='- %s' "$last..HEAD" >"$dist/notes.md"
  else
    : >"$dist/notes.md"
  fi
  url="${CODEALONG_RELEASE_URL:-https://github.com/codealong/codealong/releases/download/v$version}/$(basename "$archive")"
  node -e '
const fs = require("fs");
const [version, sig, url, notes, ...platforms] = process.argv.slice(1);
const entry = { signature: fs.readFileSync(sig, "utf8").trim(), url };
const manifest = {
  version,
  notes: fs.readFileSync(notes, "utf8").trim(),
  pub_date: new Date().toISOString().replace(/\.\d+Z$/, "Z"),
  platforms: Object.fromEntries(platforms.map((p) => [p, entry])),
};
process.stdout.write(JSON.stringify(manifest, null, 2) + "\n");
' "$version" "$archive.sig" "$url" "$dist/notes.md" $platforms >"$dist/latest.json"
fi

dmg="$dist/Codealong_${version}_${arch}.dmg"
rm -f "$dmg"
"$venv/bin/dmgbuild" -s src-tauri/dmg/settings.py -D app="$app" -D dir="$PWD/src-tauri/dmg" Codealong "$dmg"
# The app people drag out is the copy in the image, so that's the one checked: copying can add
# attributes that break its signature.
mnt="$work/mnt"
mkdir -p "$mnt"
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$mnt" "$dmg"
copied=ok
codesign --verify --deep --strict "$mnt/Codealong.app" || copied=broken
hdiutil detach -quiet "$mnt" || hdiutil detach -quiet -force "$mnt"
[ "$copied" = ok ] || fail "The app inside the disk image doesn't pass codesign --verify --strict."
codesign --force --sign "$identity" --timestamp "$dmg"
if [ "$notarize" = 1 ]; then
  notarize "$dmg" "the disk image"
  xcrun stapler staple "$dmg"
  spctl -a -t exec -vv "$app" 2>&1 | grep -q "source=Notarized Developer ID" || fail "Gatekeeper doesn't accept the app."
  spctl -a -t open --context context:primary-signature -vv "$dmg" 2>&1 | grep -q "source=Notarized Developer ID" ||
    fail "Gatekeeper doesn't accept the disk image."
fi
(cd "$dist" && shasum -a 256 "$(basename "$dmg")" >"$(basename "$dmg").sha256")

# A draft: nobody sees it, and latest.json only reaches installed copies once it's published.
# The disk image goes up under a name of its own too, so a link to the latest one never changes.
if [ "$draft" = 1 ]; then
  mkdir "$work/stable"
  cp "$dmg" "$work/stable/Codealong.dmg"
  gh release create "v$version" --draft --title "Codealong $version" --notes-file "$dist/notes.md" --target "$(git rev-parse HEAD)" \
    "$dmg" "$dmg.sha256" "$work/stable/Codealong.dmg" "$archive" "$archive.sig" "$dist/latest.json"
fi

size="$(du -h "$dmg" | cut -f1 | tr -d ' ')"
echo
echo "Built $dmg ($size)"
echo "  Codealong $version for $archs, from $(git rev-parse --short HEAD)$([ -z "$(git status --porcelain)" ] || echo ', with uncommitted changes')"
echo "  Signed as $identity"
if [ "$notarize" = 1 ]; then
  echo "  Notarized and stapled: it opens on any Mac with macOS 13 or later."
else
  echo "  Not notarized: other Macs refuse to open it. Build it without --no-notarize to hand it out."
fi
if [ "$updates" = 1 ]; then
  echo "  Update: $(basename "$archive") and latest.json, signed with key $signed_by"
else
  echo "  No update: set the update key to make one."
fi
[ "$draft" = 0 ] || echo "  Draft release v$version made on GitHub; publishing it hands the update to installed copies."

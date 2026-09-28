#!/usr/bin/env bash
# Builds Codealong's installer for other Macs: the app for Apple silicon and Intel, signed with a
# Developer ID and notarized by Apple, in a disk image whose window says to drag it into
# Applications. The app relaunch.sh builds is for this Mac only: ad-hoc signed, one architecture,
# never notarized, and other Macs refuse to open it.
#
#   scripts/release.sh                  the installer, from a clean checkout
#   scripts/release.sh --arch arm64     one architecture (arm64, x86_64, or the default universal)
#   scripts/release.sh --no-notarize    signed but not notarized: opens on this Mac only
#   scripts/release.sh --allow-dirty    builds uncommitted changes too; never ship that
#
# Signing uses the "Developer ID Application" certificate in the keychain;
# APPLE_SIGNING_IDENTITY picks one when there are several. Notarizing needs one of:
#   CODEALONG_NOTARY_PROFILE  credentials saved in the keychain, once per Mac, with
#                             `xcrun notarytool store-credentials <name> --key <AuthKey_ID.p8>
#                             --key-id <ID> --issuer <issuer>` (or --apple-id and --team-id,
#                             which asks for an app-specific password)
#   APPLE_API_KEY and APPLE_API_ISSUER, an App Store Connect API key's ID and issuer, with the
#                             key at APPLE_API_KEY_PATH or ~/.appstoreconnect/private_keys/
# Passwords never go on a command line, where other processes could read them.
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
while [ $# -gt 0 ]; do
  case "$1" in
    --arch)
      arch="${2:-}"
      shift 2 || true
      ;;
    --no-notarize) notarize=0 && shift ;;
    --allow-dirty) dirty=1 && shift ;;
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
# The names follow Tauri's own bundles: Codealong_<version>_<aarch64|x64|universal>.dmg.
case "$arch" in
  universal) triples=(aarch64-apple-darwin x86_64-apple-darwin) target=universal-apple-darwin archs="arm64 x86_64" ;;
  arm64 | aarch64) arch=aarch64 triples=(aarch64-apple-darwin) target=aarch64-apple-darwin archs=arm64 ;;
  x86_64 | x64) arch=x64 triples=(x86_64-apple-darwin) target=x86_64-apple-darwin archs=x86_64 ;;
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

identity="${APPLE_SIGNING_IDENTITY:-}"
if [ -z "$identity" ]; then
  identities="$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application: .*\)"$/\1/p' | sort -u)"
  case "$(printf '%s' "$identities" | grep -c .)" in
    0) fail "No Developer ID Application certificate in the keychain. Make one at developer.apple.com (Certificates, type Developer ID Application) and open the downloaded file, or set APPLE_SIGNING_IDENTITY." ;;
    1) identity="$identities" ;;
    *) fail "$(printf 'Several Developer ID certificates are in the keychain; pick one with APPLE_SIGNING_IDENTITY:\n%s' "$identities")" ;;
  esac
fi

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

for triple in "${triples[@]}"; do
  rustup target list --installed | grep -qx "$triple" || rustup target add "$triple"
done

shared="$(scripts/target-dir.sh)"
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
  APPLE_SIGNING_IDENTITY="$identity" CARGO_TARGET_DIR="$build" \
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

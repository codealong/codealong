#!/usr/bin/env bash
# The installers for Linux and Windows, as release.sh makes the Mac's: built on each OS by
# .github/workflows/release.yml, then put in the draft release the Mac's job made.
#
#   scripts/installers.sh build [dir]           this OS's installers for this machine's processor,
#                                               into dir (default dist/installers)
#   scripts/installers.sh publish <tag> <dir>   every installer in dir into that draft release, and
#                                               their updates into its latest.json
#
# Linux: an AppImage, which runs on any distribution, a .deb and an .rpm; Windows: an NSIS setup
# .exe, which installs for you alone, with no administrator's say. Each installs the app that
# keeps itself up to date: with the update key (TAURI_SIGNING_PRIVATE_KEY and its password, as
# release.sh takes them) each is signed, and its platform's entries for latest.json written
# beside it; the updater asks for `<os>-<arch>-<installer>` first, then `<os>-<arch>`, which is
# the AppImage's or the setup's. Without the key it builds them unsigned, to try, and publish
# refuses those. Each is kept under the name Tauri gives it, with the version, and a copy without
# the version that a link to the latest release can name (Codealong_amd64.AppImage).
set -euo pipefail
cd "$(dirname "$0")/.."

fail() {
  echo "installers.sh: $*" >&2
  exit 1
}

version="$(node -p "require('./src-tauri/tauri.conf.json').version")"

# A path as the OS's own programs take it: on Windows, Git Bash's /tmp/… as C:\…, which Tauri and
# Node want in a file they're given, where Git Bash doesn't turn it around for them.
native() {
  if command -v cygpath >/dev/null; then cygpath -w "$1"; else echo "$1"; fi
}

# The updater's name for a key's ID, from a signature or the public key (both base64 minisign).
key_id() {
  node -e 'const t=Buffer.from(process.argv[1],"base64").toString().split("\n")[1]; console.log(Buffer.from(t,"base64").subarray(2,10).toString("hex"))' "$1"
}

build() {
  local out="${1:-dist/installers}"
  local host os arch bundles
  host="$(rustc -vV | sed -n 's/^host: //p')"
  case "$host" in
    *-linux-*) os=linux bundles=appimage,deb,rpm ;;
    *-windows-*) os=windows bundles=nsis ;;
    *) fail "installers are built on Linux and Windows; release.sh builds the Mac's" ;;
  esac
  arch="${host%%-*}"
  local target="${CARGO_TARGET_DIR:-$(scripts/target-dir.sh)}"
  # Cargo takes the folder as given; Git Bash's own programs (find) want it as a POSIX path.
  local bundle="$target/release/bundle"
  if command -v cygpath >/dev/null; then bundle="$(cygpath -u "$target")/release/bundle"; fi
  local work
  work="$(mktemp -d)"
  scripts/licenses.sh "$work/THIRD_PARTY_LICENSES.txt"
  node -e 'process.stdout.write(JSON.stringify({ bundle: { resources: { [process.argv[1]]: "THIRD_PARTY_LICENSES.txt" } } }))' "$(native "$work/THIRD_PARTY_LICENSES.txt")" >"$work/release.conf.json"
  rm -rf "$bundle"
  echo "Building Codealong $version for $os on $arch: $bundles."
  CARGO_TARGET_DIR="$target" CODEALONG_RELEASE=1 CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_INCREMENTAL=false \
    npx tauri build --ci --bundles "$bundles" --config "$(native "$work/release.conf.json")"

  mkdir -p "$out"
  local signed=0 trusted
  if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
    signed=1
    trusted="$(key_id "$(node -p "require('./src-tauri/tauri.conf.json').plugins.updater.pubkey")")"
  fi
  # Each bundle's own folder, the installer's name for the updater, and the file.
  local entries=() kind file name stable
  for kind in appimage deb rpm nsis; do
    case "$kind" in
      appimage) file="$(find "$bundle/appimage" -maxdepth 1 -name '*.AppImage' 2>/dev/null | head -1)" ;;
      deb) file="$(find "$bundle/deb" -maxdepth 1 -name '*.deb' 2>/dev/null | head -1)" ;;
      rpm) file="$(find "$bundle/rpm" -maxdepth 1 -name '*.rpm' 2>/dev/null | head -1)" ;;
      nsis) file="$(find "$bundle/nsis" -maxdepth 1 -name '*-setup.exe' 2>/dev/null | head -1)" ;;
    esac
    [ -n "$file" ] || continue
    name="$(basename "$file")"
    cp "$file" "$out/$name"
    # Tauri's name without the version: Codealong_0.21.0_amd64.deb, Codealong-0.21.0-1.x86_64.rpm.
    stable="${name/_${version}_/_}"
    stable="${stable/-${version}-1./.}"
    [ "$stable" != "$name" ] || fail "$name doesn't have the version $version in it to leave out"
    cp "$file" "$out/$stable"
    for f in "$name" "$stable"; do
      (cd "$out" && sha256sum "$f" >"$f.sha256")
    done
    if [ "$signed" = 1 ]; then
      npx tauri signer sign "$out/$name" >/dev/null
      [ -s "$out/$name.sig" ] || fail "signing $name wrote no $name.sig"
      [ "$(key_id "$(cat "$out/$name.sig")")" = "$trusted" ] ||
        fail "$name is signed with another key than the one the app trusts (plugins.updater.pubkey)"
      entries+=("$kind" "$name")
    fi
  done
  [ -n "$(find "$out" -maxdepth 1 -type f | head -1)" ] || fail "the build made none of $bundles"
  if [ "$signed" = 1 ]; then
    # This platform's entries for latest.json, its URLs given at publish: the AppImage's or the
    # setup's under the platform's own name too, for a copy that doesn't say how it was installed.
    node -e '
const fs = require("fs");
const [out, os, arch, ...pairs] = process.argv.slice(1);
const platforms = {};
for (let i = 0; i < pairs.length; i += 2) {
  const [kind, file] = [pairs[i], pairs[i + 1]];
  const entry = { signature: fs.readFileSync(`${out}/${file}.sig`, "utf8").trim(), file };
  platforms[`${os}-${arch}-${kind}`] = entry;
  if (kind === "appimage" || kind === "nsis") platforms[`${os}-${arch}`] = entry;
}
fs.writeFileSync(`${out}/platforms-${os}-${arch}.json`, JSON.stringify(platforms, null, 2) + "\n");
' "$out" "$os" "$arch" "${entries[@]}"
  else
    echo "No update key (TAURI_SIGNING_PRIVATE_KEY): the installers are unsigned, to try, and publish won't take them."
  fi
  rm -rf "$work"
  ls -l "$out"
}

publish() {
  local tag="$1" dir="$2"
  [ "$tag" = "v$version" ] || fail "the tag is $tag but the app's version is $version"
  [ "$(gh release view "$tag" --json isDraft -q .isDraft)" = true ] || fail "$tag isn't a draft release: a published one's files never change"
  local fragments=("$dir"/platforms-*.json)
  [ -e "${fragments[0]}" ] || fail "no signed installers in $dir"
  local work
  work="$(mktemp -d)"
  gh release download "$tag" -p latest.json -D "$work" || fail "the draft $tag has no latest.json: release.sh makes it with the Mac's update"
  node -e '
const fs = require("fs");
const [manifest, base, ...fragments] = process.argv.slice(1);
const m = JSON.parse(fs.readFileSync(manifest, "utf8"));
for (const f of fragments) {
  for (const [platform, { signature, file }] of Object.entries(JSON.parse(fs.readFileSync(f, "utf8")))) {
    m.platforms[platform] = { signature, url: `${base}/${file}` };
  }
}
process.stdout.write(JSON.stringify(m, null, 2) + "\n");
' "$work/latest.json" "https://github.com/codealong/codealong/releases/download/$tag" "${fragments[@]}" >"$work/latest.merged.json"
  mv "$work/latest.merged.json" "$work/latest.json"
  local files=()
  while IFS= read -r f; do files+=("$f"); done < <(find "$dir" -maxdepth 1 -type f ! -name 'platforms-*.json' | sort)
  gh release upload "$tag" --clobber "${files[@]}" "$work/latest.json"
  echo "Put ${#files[@]} files in the draft $tag, and latest.json now has: $(node -p "Object.keys(require('$work/latest.json').platforms).join(', ')")"
  rm -rf "$work"
}

case "${1:-}" in
  build) shift && build "$@" ;;
  publish)
    [ $# -eq 3 ] || fail "publish <tag> <dir>"
    publish "$2" "$3"
    ;;
  *)
    sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac

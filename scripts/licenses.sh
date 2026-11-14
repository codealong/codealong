#!/usr/bin/env bash
# licenses.sh <file>: writes what the app is built from that others wrote, with their licenses,
# which their licenses ask to go with every copy: THIRD_PARTY_NOTICES.md, the fonts' licenses, and
# every crate the app links (cargo-about, from src-tauri/about.toml; a license it doesn't accept
# fails, with what it said). Every installer carries it as THIRD_PARTY_LICENSES.txt, beside
# LICENSE and NOTICE: release.sh's for macOS, installers.sh's for Linux and Windows.
set -euo pipefail
cd "$(dirname "$0")/.."
out="$1"
command -v cargo-about >/dev/null ||
  { echo "The app carries the licenses of the crates it's built with, which cargo-about writes: cargo install cargo-about --locked --features cli." >&2; exit 1; }
log="$(mktemp)"
{
  cat THIRD_PARTY_NOTICES.md
  for f in src/fonts/OFL-*.txt; do
    printf '\n\n## %s\n\n' "$(basename "$f")"
    cat "$f"
  done
  printf '\n\n'
  cargo about generate --manifest-path src-tauri/Cargo.toml --locked --fail -c src-tauri/about.toml src-tauri/about.hbs 2>"$log"
} >"$out" || {
  cat "$log" >&2
  rm -f "$log"
  echo "cargo-about couldn't list the crates' licenses (above): a new crate's license may need looking at, and adding to src-tauri/about.toml." >&2
  exit 1
}
rm -f "$log"

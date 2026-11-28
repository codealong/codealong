#!/usr/bin/env bash
# licenses.sh <file> [target…]: writes what the app is built from that others wrote, with their
# licenses, which their licenses ask to go with every copy: THIRD_PARTY_NOTICES.md, the fonts'
# licenses, and every crate the app links (cargo-about, from src-tauri/about.toml; a license it
# doesn't accept fails, with what it said). Every installer carries it as
# THIRD_PARTY_LICENSES.txt, beside LICENSE and NOTICE: release.sh's for macOS, installers.sh's for
# Linux and Windows. The crates are those built for about.toml's targets, the Macs', or for the
# targets given (installers.sh's).
set -euo pipefail
cd "$(dirname "$0")/.."
out="$1"
shift
command -v cargo-about >/dev/null ||
  { echo "The app carries the licenses of the crates it's built with, which cargo-about writes: cargo install cargo-about --version 0.9.2 --locked --features cli (release.yml says why that one)." >&2; exit 1; }
log="$(mktemp)"
config=src-tauri/about.toml
if [ $# -gt 0 ]; then
  config="$(mktemp)"
  trap 'rm -f "$config"' EXIT
  targets="$(printf '"%s", ' "$@")"
  sed "s/^targets = .*/targets = [${targets%, }]/" src-tauri/about.toml >"$config"
  grep -q "^targets = \[${targets%, }\]" "$config" || { echo "about.toml has no targets line to put $* in" >&2; exit 1; }
fi
{
  cat THIRD_PARTY_NOTICES.md
  for f in src/fonts/OFL-*.txt; do
    printf '\n\n## %s\n\n' "$(basename "$f")"
    cat "$f"
  done
  printf '\n\n'
  cargo about generate --manifest-path src-tauri/Cargo.toml --locked --fail -c "$config" src-tauri/about.hbs 2>"$log"
} >"$out" || {
  cat "$log" >&2
  rm -f "$log"
  echo "cargo-about couldn't list the crates' licenses (above): a new crate's license may need looking at, and adding to src-tauri/about.toml." >&2
  exit 1
}
rm -f "$log"

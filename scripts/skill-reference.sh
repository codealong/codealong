#!/usr/bin/env bash
# Writes the skill's command reference from the engine binary's --help texts. Run after changing
# src-tauri/src/cli.rs; a cargo test fails while the file is out of date.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
target="$(scripts/target-dir.sh)"
(cd src-tauri && cargo build --quiet)
mkdir -p skills/codealong/references
"$target/debug/codealong" reference > skills/codealong/references/reference.md
echo "Wrote skills/codealong/references/reference.md"

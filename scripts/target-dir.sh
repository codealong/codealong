#!/usr/bin/env bash
# Prints cargo's build directory for this checkout: shared by every worktree (.cargo/config.toml),
# so the engine binary is <dir>/release/codealong and the bundle <dir>/release/bundle/macos.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
cargo metadata --format-version 1 --no-deps --manifest-path src-tauri/Cargo.toml |
  python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'

#!/usr/bin/env bash
# Prints the build directory every checkout of Codealong shares: target-dir in .cargo/config.toml
# as it resolves in the main checkout, the one holding the repository's .git. The engine binary is
# <dir>/release/codealong and the bundle <dir>/release/bundle/macos.
#
# Cargo resolves target-dir against the checkout it runs in, so for a worktree inside another
# (Claude Code makes its own in .claude/worktrees/<name>) it names a folder beside that worktree.
# The scripts that build pass the shared directory to cargo as CARGO_TARGET_DIR, and this makes
# that folder a link to it, so cargo run there by hand builds in it too. A real folder already in
# its place is only reported: something may be building in it.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
here="$PWD"
# A bare repository has no main checkout, and a build directory set in the environment holds
# wherever cargo runs: then this checkout's own answer is the one.
common="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
main="${common%/.git}"
if [ "$main" = "$common" ] || [ -n "${CARGO_TARGET_DIR:-}${CARGO_BUILD_TARGET_DIR:-}" ]; then
  main="$here"
fi
# Cargo reads its config from the folder it runs in, not the manifest's.
target_in() {
  (cd "$1" && cargo metadata --format-version 1 --no-deps --manifest-path "$here/src-tauri/Cargo.toml") |
    python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
}
shared="$(target_in "$main")"
own="$shared"
[ "$main" = "$here" ] || own="$(target_in "$here")"
python3 - "$shared" "$own" <<'EOF'
import os, sys

shared = os.path.realpath(sys.argv[1])
own = os.path.join(os.path.realpath(os.path.dirname(sys.argv[2])), os.path.basename(sys.argv[2]))
if own != shared:
    try:
        # Also brings the shared directory back if it was removed, which would leave a link to it
        # pointing nowhere and cargo failing with "Not a directory".
        os.makedirs(shared, exist_ok=True)
        os.symlink(os.path.relpath(shared, os.path.dirname(own)), own)
        print(f"Linked {own} to {shared}, so cargo run in this checkout builds there.", file=sys.stderr)
    except OSError:
        pass
    if os.path.lexists(own) and os.path.realpath(own) != shared:
        print(
            f"Cargo run by hand in this checkout builds into {own}, not {shared}. Once nothing "
            "builds in it, remove it, and the next run of scripts/target-dir.sh links it to the "
            "shared one.",
            file=sys.stderr,
        )
print(shared)
EOF

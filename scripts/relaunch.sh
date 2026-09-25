#!/usr/bin/env bash
# Hands a change to the user: builds the bundle, quits the running Codealong and opens the new
# one. One bundle build at a time on this Mac; a build started from another worktree waits.
# The running app is matched by its own path, anchored, so a shell whose command line mentions
# that path (another session's build, say) is never killed. The app stays up during the build.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
target="$(scripts/target-dir.sh)"
app="$target/release/bundle/macos/Codealong.app"
lock="$target/relaunch.lock"
mkdir -p "$target"

# The lock is a folder holding its owner's pid; one whose owner is gone is taken over.
while ! mkdir "$lock" 2>/dev/null; do
  holder="$(cat "$lock/pid" 2>/dev/null || true)"
  if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
    rm -rf "$lock"
    continue
  fi
  echo "Waiting for the build in progress (pid ${holder:-?})…"
  sleep 10
done
echo $$ >"$lock/pid"
trap 'rm -rf "$lock"' EXIT

npx tauri build
pkill -f 'Codealong\.app/Contents/MacOS/codeal[o]ng$' || true
# macOS won't open an app while its last copy is still quitting (error -600), so wait for it to
# go, then open, trying once more if the first open is turned away.
for _ in $(seq 1 50); do
  pgrep -f 'Codealong\.app/Contents/MacOS/codeal[o]ng$' >/dev/null || break
  sleep 0.2
done
open "$app" || { sleep 2; open "$app"; }
echo "Relaunched $app"

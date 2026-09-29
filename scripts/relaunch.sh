#!/usr/bin/env bash
# Hands a change to the user: builds Codealong Dev, quits the running one and opens the new one.
# Codealong Dev (src-tauri/tauri.dev.conf.json) is this repo's build: an id, name and light icon of
# its own beside the Codealong installed in Applications, the same settings and todos, and the
# last of the two opened wins (server::take_over). One bundle build at a time on this Mac; a build
# started from another worktree waits. The running app is matched by its own path, anchored, so a
# shell whose command line mentions that path (another session's build, say) is never killed. The
# app stays up during the build.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
target="$(scripts/target-dir.sh)"
# From any checkout, cargo and tauri build in the shared directory, which holds the lock below
# and the app that gets opened.
export CARGO_TARGET_DIR="$target"
bundles="$target/release/bundle/macos"
app="$bundles/Codealong Dev.app"
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
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

# Builds take turns in no particular order, so origin/main may have moved on while this one
# waited, and a newer app been launched meanwhile. A clean checkout that origin/main has only
# moved past is brought up to it, so an older build never replaces a newer one.
fetch() { GIT_TERMINAL_PROMPT=0 git fetch -q origin main 2>/dev/null; }
clean() { [ -z "$(git status --porcelain)" ]; }
if fetch && clean && git merge-base --is-ancestor HEAD origin/main &&
  [ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]; then
  echo "origin/main has moved past this checkout; fast-forwarding to it."
  git merge -q --ff-only origin/main || echo "Couldn't fast-forward; building the checkout as it is."
fi

# The workspace's own crates build to the same files from every checkout, and cargo goes by file
# times: a build another checkout made since this one's files last changed would pass for this
# checkout's. Touched, they compile from here.
touch src-tauri/src/lib.rs src-tauri/peer/src/lib.rs
npx tauri build --config src-tauri/tauri.dev.conf.json
# What runs from the shared bundle folder is a build of this repo, and relaunch's own to quit. The
# Codealong in Applications is left alone: the new build asks it to quit as it opens.
running="^$(printf '%s' "$bundles" | sed 's/[][\.*^$()+?{}|]/\\&/g')/[^/]+\.app/Contents/MacOS/codeal[o]ng$"
pkill -f "$running" || true
# macOS won't open an app while its last copy is still quitting (error -600), so wait for it to
# go, then open, trying once more if the first open is turned away.
for _ in $(seq 1 50); do
  pgrep -f "$running" >/dev/null || break
  sleep 0.2
done
# Builds before Codealong Dev were Codealong.app, here. Gone, it can't be opened from Spotlight or
# a link as if it were the app in Applications, and the command's link moves to Codealong Dev.
old="$bundles/Codealong.app"
if [ -d "$old" ]; then
  for link in /usr/local/bin/codealong "$HOME/.local/bin/codealong"; do
    if [ "$(readlink "$link" 2>/dev/null)" = "$old/Contents/MacOS/codealong" ]; then
      ln -sfn "$app/Contents/MacOS/codealong" "$link" 2>/dev/null || true
    fi
  done
  "$lsregister" -u "$old" 2>/dev/null || true
  rm -rf "$old"
fi
open "$app" || { sleep 2; open "$app"; }

# Names what was launched, for the agent reporting it: the commit, where it stands against
# origin/main, and whether uncommitted changes went into the build.
if ! fetch; then
  where="couldn't fetch origin/main to compare"
elif [ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ]; then
  where="origin/main"
elif git merge-base --is-ancestor HEAD origin/main; then
  n="$(git rev-list --count HEAD..origin/main)"
  where="$n commit$([ "$n" = 1 ] || echo s) behind origin/main"
else
  where="not in origin/main"
fi
clean || where="$where, with uncommitted changes"
echo "Relaunched Codealong Dev from $(git rev-parse --short HEAD) ($where)"

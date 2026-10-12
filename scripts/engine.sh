#!/usr/bin/env bash
# Prints the path of this checkout's own engine binary, building it first when src-tauri has
# changed since the last one. The browser checks snapshot your data with it (preview.sh,
# style-diff.mjs), and so can you:
#
#   "$(scripts/engine.sh)" --snapshot --table
#
# It's a debug build. `cargo test` has already compiled the library it links, so after a test run
# it's a link away, and it never waits behind the release builds that relaunch.sh makes for the
# app. Dependencies are optimized in debug builds (src-tauri/Cargo.toml), which keeps a snapshot
# within about twice release's time.
#
# Every checkout builds into one directory (scripts/target-dir.sh), so the binary there is whoever
# built last. The build is copied out of it, to .preview/engine/codealong, beside a note of what it
# was built from: src-tauri as committed, its uncommitted changes and its new files. While those
# are the same the copy is printed and nothing is built.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
own=.preview/engine
mkdir -p "$own"
# One at a time in a checkout (two previews at once, say): the second finds the first's build. The
# lock is a folder holding its owner's pid; one whose owner is gone is taken over, as is one a
# minute old that never got its pid.
lock="$own/lock"
while ! mkdir "$lock" 2>/dev/null; do
  holder="$(cat "$lock/pid" 2>/dev/null || true)"
  if { [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; } ||
    { [ -z "$holder" ] && [ -n "$(find "$lock" -maxdepth 0 -mmin +1 2>/dev/null)" ]; }; then
    rm -rf "$lock"
    continue
  fi
  sleep 1
done
echo $$ >"$lock/pid"
trap 'rm -rf "$lock" "$own/codealong.new"' EXIT
state="$({
  git rev-parse HEAD:src-tauri
  git diff HEAD --binary -- src-tauri
  git ls-files -o --exclude-standard -z -- src-tauri | xargs -0 shasum 2>/dev/null
} | shasum | cut -d' ' -f1)"
if [ -x "$own/codealong" ] && [ "$(cat "$own/state" 2>/dev/null)" = "$state" ]; then
  echo "$PWD/$own/codealong"
  exit 0
fi

target="$(scripts/target-dir.sh)"
export CARGO_TARGET_DIR="$target"
built="$target/debug/codealong"
for _ in 1 2 3; do
  (cd src-tauri && cargo build --quiet --bin codealong) >&2
  rm -f "$own/codealong.new"
  cp -c "$built" "$own/codealong.new" 2>/dev/null || cp "$built" "$own/codealong.new"
  # Once cargo lets go of its lock another checkout's build can replace the binary, even with an
  # older one of its own it only links again. The copy says which checkout built it
  # (lib.rs CHECKOUT); one that isn't this one is built again.
  if [ "$("$own/codealong.new" --snapshot --checkout 2>/dev/null)" -ef . ]; then
    mv "$own/codealong.new" "$own/codealong"
    echo "$state" >"$own/state"
    echo "$PWD/$own/codealong"
    exit 0
  fi
done
echo "Another checkout's build kept replacing $built before it could be copied; try again." >&2
exit 1

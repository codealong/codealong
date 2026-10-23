#!/usr/bin/env bash
# Hands a change to the user: builds Codealong Dev, quits the running one, puts the new one in
# Applications and opens it there, so Spotlight or any launcher opens the latest build.
# Codealong Dev (src-tauri/tauri.dev.conf.json) is this repo's build: an id, name and light icon of
# its own beside the Codealong installed in Applications, the same settings and todos, and the
# last of the two opened wins (server::take_over). One bundle build at a time on this Mac; a build
# started from another worktree waits. The running app is matched by its own path, anchored, so a
# shell whose command line mentions that path (another session's build, say) is never killed. The
# app stays up during the build. One whose commit the running app already has (another session's
# relaunch built a later origin/main while this one waited) builds nothing and leaves the app be.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
target="$(scripts/target-dir.sh)"
# From any checkout, cargo and tauri build in the shared directory, which holds the lock below
# and the app each build makes.
export CARGO_TARGET_DIR="$target"
bundles="$target/release/bundle/macos"
app="$bundles/Codealong Dev.app"
# Where the app is opened from: /Applications, or ~/Applications on a Mac where you can't write it.
apps=/Applications
[ -w "$apps" ] || apps="$HOME/Applications"
installed="$apps/Codealong Dev.app"
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

# Codealong Dev in Applications, or any app in the shared bundle folder (where builds ran from
# before), is a build of this repo and relaunch's own to quit. Codealong itself is left alone: the
# new build asks it to quit as it opens.
re() { printf '%s' "$1" | sed 's/[][\.*^$()+?{}|]/\\&/g'; }
running="^($(re "$installed")|$(re "$bundles")/[^/]+\.app)/Contents/MacOS/codeal[o]ng$"

# The commit the running app was built from, when the build had no uncommitted changes. Agents land
# one after another, and a relaunch waiting its turn often finds the app already built from a later
# origin/main that has its commit: built and reopened again, the app would drop its windows and
# the phones and Macs linked to it for the same code, as it did several times an hour.
launched="$target/launched"
was="$(cat "$launched" 2>/dev/null || true)"
if [ -n "$was" ] && clean && git merge-base --is-ancestor HEAD "$was" 2>/dev/null && pgrep -f "$running" >/dev/null; then
  scripts/catch-up-main.sh
  at=""
  [ "$was" != "$(git rev-parse origin/main 2>/dev/null)" ] || at=" (origin/main)"
  echo "Relaunched already: Codealong Dev runs $(git rev-parse --short "$was")$at, which has this checkout's $(git rev-parse --short HEAD), so nothing was built or reopened"
  exit 0
fi
built=""
clean && built="$(git rev-parse HEAD)"

# The UI is built with vite alone, in a few seconds: checking its types is npm run typecheck's job,
# and the tsc in npm run build took half a minute of every relaunch. The commit goes into the app,
# which looks at origin/main for commits it hasn't (updates.rs `built_commit`).
CODEALONG_COMMIT="$built" npx tauri build --config src-tauri/tauri.dev.conf.json --config '{"build": {"beforeBuildCommand": "npx vite build"}}'
# macOS remembers what the app was allowed (controlling Ghostty, reading Documents or an external
# drive) by its signature. The linker's ad-hoc one changes with every build, so macOS takes each
# build for a new app and asks again; one signed with an Apple Development certificate is the same
# app as the last. CODEALONG_SIGNING_IDENTITY names the certificate, else it's the keychain's first. No
# hardened runtime or timestamp: those are for a build other Macs open (release.sh's), and a
# timestamp needs Apple's server. Over SSH the login keychain is out of reach, so the build stays
# ad hoc.
identity="${CODEALONG_SIGNING_IDENTITY:-}"
[ -n "$identity" ] ||
  identity="$(security find-identity -v -p codesigning | sed -n 's/.*"\(Apple Development: .*\)"$/\1/p' | head -1)"
if [ -z "$identity" ]; then
  echo "No Apple Development certificate in the keychain: the build keeps its ad-hoc signature, and macOS asks again for what it had allowed."
elif signed="$(codesign --force --timestamp=none --sign "$identity" "$app" 2>&1)" &&
  codesign --verify --strict "$app" 2>/dev/null; then
  echo "Signed with $identity."
else
  printf '%s\n' "$signed"
  # Whatever a failed signing left behind, an ad-hoc signature is one the app opens with.
  codesign --force --sign - "$app" 2>/dev/null
  echo "Couldn't sign with $identity, so the build is signed ad hoc and macOS asks again for what it had allowed."
fi
pkill -f "$running" || true
# macOS won't open an app while its last copy is still quitting (error -600), so wait for it to
# go before opening (the binary tries once more if the first open is turned away).
for _ in $(seq 1 50); do
  pgrep -f "$running" >/dev/null || break
  sleep 0.2
done
# The build replaces the copy in Applications. It's moved, not copied, so LaunchServices knows one
# Codealong Dev to open a link with, the one in Applications.
mkdir -p "$apps"
"$lsregister" -u "$app" 2>/dev/null || true
rm -rf "$installed"
mv "$app" "$installed"
# Builds before Codealong Dev were Codealong.app, in the bundle folder. Gone, it can't be opened
# from Spotlight or a link as if it were the app in Applications.
old="$bundles/Codealong.app"
if [ -d "$old" ]; then
  "$lsregister" -u "$old" 2>/dev/null || true
  rm -rf "$old"
fi
# The `codealong` command's link follows the app once it opens (install::follow_command_link); one
# into the bundle folder is pointed at Applications now, so it never points at nothing meanwhile.
for link in /usr/local/bin/codealong "$HOME/.local/bin/codealong"; do
  case "$(readlink "$link" 2>/dev/null)" in
  "$bundles"/*.app/Contents/MacOS/codealong) ln -sfn "$installed/Contents/MacOS/codealong" "$link" 2>/dev/null || true ;;
  esac
done
# Opened as typing `codealong` opens it: the binary hands its bundle to macOS, which opens a new
# copy with the environment the Dock gives an app, not this shell's (install::open_as_the_dock_does).
"$installed/Contents/MacOS/codealong"
# Opened, the new copy asks whichever copy answers on the socket to quit and takes the socket from
# it (server::take_over): a copy the pkill above didn't match, too, one an earlier build started
# from a terminal, whose command line is the word `codealong` alone. It's relaunched once the new
# copy holds the socket, and not before.
sock="$HOME/Library/Application Support/Codealong/codealong.sock"
new_copy="^$(re "$installed")/Contents/MacOS/codeal[o]ng$"
took=""
holder=""
for _ in $(seq 1 300); do
  holder="$(lsof -t "$sock" 2>/dev/null | head -1 || true)"
  case " $(pgrep -f "$new_copy" | tr '\n' ' ' || true) " in
  *" ${holder:-none} "*)
    took=1
    break
    ;;
  esac
  sleep 0.2
done
# A Dock pin made while builds ran from the bundle folder points at nothing once they're moved to
# Applications, and the Dock shows the running app under a question mark. It's pointed at
# Applications, and the Dock restarted to read it, once. Nothing here may stop the script.
repin() {
  local prefs pinned i key repinned=""
  prefs="$(mktemp)" || return 0
  if defaults export com.apple.dock "$prefs" 2>/dev/null; then
    pinned="$(plutil -extract persistent-apps raw -o - "$prefs" 2>/dev/null || echo 0)"
    for ((i = 0; i < pinned; i++)); do
      key="persistent-apps.$i.tile-data"
      [ "$(plutil -extract "$key.file-data._CFURLString" raw -o - "$prefs" 2>/dev/null || true)" = "file://${app// /%20}/" ] || continue
      plutil -replace "$key.file-data._CFURLString" -string "file://${installed// /%20}/" "$prefs" 2>/dev/null || continue
      # The bookmark beside it leads to the old place too; without one the Dock goes by the URL.
      plutil -remove "$key.book" "$prefs" 2>/dev/null || true
      repinned=1
    done
    if [ -n "$repinned" ] && defaults import com.apple.dock "$prefs" 2>/dev/null; then
      killall Dock 2>/dev/null || true
      echo "Pointed the Dock's Codealong Dev at $installed."
    fi
  fi
  rm -f "$prefs"
}
repin
# For the next relaunch to know what runs (above). A build with uncommitted changes names no commit,
# and one that didn't take over runs none of it.
if [ -n "$took" ] && [ -n "$built" ]; then echo "$built" >"$launched"; else rm -f "$launched"; fi

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
# The shared build directory grows by gigabytes a day, so once a day a relaunch prunes what no
# build has used for days, in the background (prune-target.sh; its last run's line is in prune.log).
if [ -z "$(find "$target/pruned" -mtime -1 2>/dev/null)" ]; then
  touch "$target/pruned"
  nohup scripts/prune-target.sh >"$target/prune.log" 2>&1 &
fi
# The main checkout is brought up to origin/main too (landing's step 3), and says how that went.
scripts/catch-up-main.sh
if [ -z "$took" ]; then
  running_now="nothing"
  # It may have gone since; the line is printed all the same.
  [ -z "$holder" ] || running_now="pid $holder ($(ps -o command= -p "$holder" 2>/dev/null | cut -c1-120 || true))"
  echo "Not relaunched: Codealong Dev from $(git rev-parse --short HEAD) was opened, but after waiting for it $running_now answered as Codealong. lsappinfo list shows what macOS has open."
  exit 1
fi
echo "Relaunched Codealong Dev from $(git rev-parse --short HEAD) ($where)"

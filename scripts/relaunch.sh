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

# The workspace's own crates build to the same files from every checkout, and cargo goes by file
# times: a build another checkout made since this one's files last changed would pass for this
# checkout's. Touched, they compile from here.
touch src-tauri/src/lib.rs src-tauri/peer/src/lib.rs
npx tauri build --config src-tauri/tauri.dev.conf.json
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
# go, then open, trying once more if the first open is turned away.
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
# The app gets the environment the Dock gives it, not this shell's: `open` hands its own
# environment to the app it starts, and an agent's shell holds its session's ID, message socket
# and token (CLAUDE_CODE_*), its editor's variables (VSCODE_*, ELECTRON_RUN_AS_NODE) and a PATH of
# its own, which the app passed on to every process it started, long after that session had ended.
me="$(id -un)"
dock=(HOME="$HOME" USER="$me" LOGNAME="$me" SHELL="${SHELL:-/bin/zsh}" PATH=/usr/bin:/bin:/usr/sbin:/sbin TMPDIR="$(getconf DARWIN_USER_TEMP_DIR)")
agent="$(launchctl getenv SSH_AUTH_SOCK || true)"
[ -z "$agent" ] || dock+=(SSH_AUTH_SOCK="$agent")
[ -z "${__CF_USER_TEXT_ENCODING:-}" ] || dock+=(__CF_USER_TEXT_ENCODING="$__CF_USER_TEXT_ENCODING")
open_app() { env -i "${dock[@]}" /usr/bin/open "$installed"; }
open_app || { sleep 2; open_app; }
# For the next relaunch to know what runs (above). A build with uncommitted changes names no commit.
if [ -n "$built" ]; then echo "$built" >"$launched"; else rm -f "$launched"; fi

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
# The main checkout is brought up to origin/main too (landing's step 3), and says how that went.
scripts/catch-up-main.sh
echo "Relaunched Codealong Dev from $(git rev-parse --short HEAD) ($where)"

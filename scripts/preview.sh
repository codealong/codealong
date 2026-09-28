#!/usr/bin/env bash
# Renders the UI in headless Chrome against a snapshot of your real data, with Tauri's IPC
# replaced by scripts/preview/fake-tauri.js (which records every command the UI sends).
#
#   scripts/preview.sh [view] [project] [check.js]
#
#   view     worktrees | sessions | prs | todo | canvas, bar for the floating bar, peek for the
#            bar's peek, or quick for the quick panel (default worktrees)
#   project  the project to show (default: the first one)
#   check.js optional script run in the page; whatever it puts in document.title is printed
#            (use it to click things and read window.__calls). Without it, a screenshot is
#            written to $PREVIEW_DIR/<view>.png.
#
# PREVIEW_INIT=<file.js> runs a script in the page before either, for a screenshot of a state
# a click gets to (a dialog, say); PREVIEW_NAME names that screenshot instead of the view.
# PREVIEW_WAIT=<ms> runs a check on the real clock, for one that times work
# (scripts/preview/push-bench.js): its title is printed once it starts with "done: ", or when
# the wait runs out. By default time is virtual and stands still while scripts run.
#
# PREVIEW_SCHEME=dark renders as if macOS were in dark mode (the screenshot is <view>-dark.png).
# Under theme = "system" the page follows it; "light" and "dark" in config.toml win over it.
# PREVIEW_SIZE sets the window size (default 1440,900). The bar renders on a stand-in desktop
# of that size (default 1100,64, which is also the screen width it fits itself to), and so does
# the quick panel (default 760,640: its window's width). The panel opens as the shortcut opens
# it, with nothing typed; PREVIEW_INIT can type or click from there. The peek (default 520,640)
# opens on PREVIEW_TRANSCRIPT's session when there is one, else on the first that needs you.
#
# PREVIEW_TODOS=<file> shows the todos in it (scripts/preview/todos-sample.json has some) in
# place of yours; "$lane:N" and "$session:N" in it stand for the project's Nth most recent
# worktree and session. Images come from $PREVIEW_IMAGES (default: Codealong's own images).
# scripts/preview/todos-jev-sample.json has the same todos with Jev's todo features on: one
# looks done, and the panel gets suggestions.
#
# PREVIEW_CANVAS=<file> opens the Canvas view on the document in it (one project's, as
# canvas.json keeps it under canvases.<project>) in place of a fresh tidy.
# PREVIEW_USAGE=<file> shows the plan usage in it (scripts/preview/usage-sample.json has some) in
# place of the snapshot's, which has no Claude numbers: reading them rate-limits.
#
# PREVIEW_INSIGHTS=<file> merges Jev's read of sessions and worktrees into the board
# (scripts/preview/insights-sample.json has some: landing chips, a risk label, a question).
#
# PREVIEW_ROUTE=<file> turns routing on and answers the quick panel as Jev might, from the
# board's own sessions and worktrees (scripts/preview/route-sample.json sends to a session;
# route-start-sample.json starts new work, with a session that may already be on it).
#
# PREVIEW_TRANSCRIPT=<session key> shows that session's real conversation in its panel (its
# newest page, from --snapshot --transcript) and what it waits on you to allow (--pending);
# other sessions show their title and excerpt there.
# PREVIEW_DEVICES=<file> shows your other devices from it (scripts/preview/devices-sample.json
# has some): Settings › Devices, the sidebar's Devices, and a few of the board's own worktrees
# shown as if they were on those devices.
# PREVIEW_FOUND=<file> lists the accounts in it under Settings › Accounts › Found on this Mac
# (scripts/preview/found-sample.json has some) in place of the ones this Mac has but config.toml
# doesn't list, which are often none.
#
# Needs a release build of the engine (cargo build --release in src-tauri) for the snapshot;
# it's found through scripts/target-dir.sh.
# PR details come from $PREVIEW_DIR/pr-detail.json if you put one there:
#   "$(scripts/target-dir.sh)/release/codealong" --snapshot --pr owner/name 123 > .preview/pr-detail.json
set -euo pipefail
cd "$(dirname "$0")/.."
view="${1:-worktrees}"
project="${2:-}"
check="${3:-}"
dir="${PREVIEW_DIR:-.preview}"
# A free port unless one is given, so a stray server can't answer for this one.
port="${PREVIEW_PORT:-$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')}"
chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
scheme=""
case "${PREVIEW_SCHEME:-}" in
  dark) scheme="--blink-settings=preferredColorScheme=0" ;;
  light) scheme="--blink-settings=preferredColorScheme=1" ;;
esac
shot="$dir/${PREVIEW_NAME:-$view}${PREVIEW_SCHEME:+-$PREVIEW_SCHEME}.png"
# Every checkout builds into one directory, so the engine there may be another session's build:
# PREVIEW_BIN points at a copy of your own.
bin="${PREVIEW_BIN:-$(scripts/target-dir.sh)/release/codealong}"

npm run build >/dev/null
mkdir -p "$dir"
rm -rf "$dir/assets"
cp -R dist/. "$dir/"
cp scripts/preview/fake-tauri.js "$dir/"
rm -f "$dir/todos-fixture.json"
if [ -n "${PREVIEW_TODOS:-}" ]; then
  cp "$PREVIEW_TODOS" "$dir/todos-fixture.json"
fi
rm -f "$dir/insights-fixture.json"
if [ -n "${PREVIEW_INSIGHTS:-}" ]; then
  cp "$PREVIEW_INSIGHTS" "$dir/insights-fixture.json"
fi
rm -f "$dir/route-fixture.json"
if [ -n "${PREVIEW_ROUTE:-}" ]; then
  cp "$PREVIEW_ROUTE" "$dir/route-fixture.json"
fi
rm -f "$dir/canvas-fixture.json"
if [ -n "${PREVIEW_CANVAS:-}" ]; then
  cp "$PREVIEW_CANVAS" "$dir/canvas-fixture.json"
fi
rm -f "$dir/usage-fixture.json"
rm -f "$dir/devices-fixture.json"
if [ -n "${PREVIEW_DEVICES:-}" ]; then
  cp "$PREVIEW_DEVICES" "$dir/devices-fixture.json"
fi
if [ -n "${PREVIEW_USAGE:-}" ]; then
  cp "$PREVIEW_USAGE" "$dir/usage-fixture.json"
fi
images="${PREVIEW_IMAGES:-$HOME/Library/Application Support/Codealong/images}"
rm -rf "$dir/images"
if [ -d "$images" ]; then
  cp -R "$images" "$dir/images"
fi
"$bin" --snapshot >"$dir/board.json" 2>/dev/null
# The floating bar gets its own small feed from the engine, not the board.
[ "$view" = bar ] && "$bin" --snapshot --bar >"$dir/bar-feed.json" 2>/dev/null
"$bin" --snapshot --accounts >"$dir/accounts.json" 2>/dev/null
"$bin" --snapshot --start-options >"$dir/start-options.json" 2>/dev/null
"$bin" --snapshot --config >"$dir/config.json" 2>/dev/null
rm -f "$dir/transcript.json" "$dir/transcript-key.json" "$dir/pending.json"
if [ -n "${PREVIEW_TRANSCRIPT:-}" ]; then
  "$bin" --snapshot --transcript "$PREVIEW_TRANSCRIPT" >"$dir/transcript.json" 2>/dev/null
  "$bin" --snapshot --pending "$PREVIEW_TRANSCRIPT" >"$dir/pending.json" 2>/dev/null
  echo "{\"key\": \"$PREVIEW_TRANSCRIPT\"}" >"$dir/transcript-key.json"
fi
if [ -n "${PREVIEW_FOUND:-}" ]; then
  cp "$PREVIEW_FOUND" "$dir/found.json"
else
  "$bin" --snapshot --found >"$dir/found.json" 2>/dev/null
fi

setup="localStorage.setItem('codealong.view','$view');localStorage.setItem('codealong.age','all');"
[ -n "$project" ] && setup="$setup localStorage.setItem('codealong.project','$project');"
size="${PREVIEW_SIZE:-1440,900}"
if [ "$view" = bar ]; then
  size="${PREVIEW_SIZE:-1100,64}"
  # Headless Chrome's screen is 800 wide whatever the window; the bar fits itself to the screen.
  setup="$setup window.__CODEALONG_BAR__=true;Object.defineProperty(screen,'availWidth',{value:${size%%,*}});</script><style>html.is-bar{background:linear-gradient(135deg,rgb(93,111,134),rgb(169,182,196))!important}</style><script>"
fi
if [ "$view" = quick ]; then
  size="${PREVIEW_SIZE:-760,640}"
  setup="$setup window.__CODEALONG_QUICK__=true;</script><style>html.is-quick{background:linear-gradient(135deg,rgb(93,111,134),rgb(169,182,196))!important}</style><script>"
fi
if [ "$view" = peek ]; then
  size="${PREVIEW_SIZE:-520,640}"
  setup="$setup localStorage.setItem('codealong.preview.peek','${PREVIEW_TRANSCRIPT:-}'); window.__CODEALONG_PEEK__=true;</script><style>html.is-peek{background:linear-gradient(135deg,rgb(93,111,134),rgb(169,182,196))!important}</style><script>"
fi
tail=""
if [ -n "${PREVIEW_INIT:-}" ]; then
  cp "$PREVIEW_INIT" "$dir/init.js"
  tail='<script src="./init.js"></script>'
fi
if [ -n "$check" ]; then
  cp "$check" "$dir/check.js"
  tail="$tail"'<script src="./check.js"></script>'
fi
sed -e "s#<head>#<head><script src=\"./fake-tauri.js\"></script><script>$setup</script>#" -e "s#</body>#$tail</body>#" "$dir/index.html" >"$dir/preview.html"

(cd "$dir" && exec python3 -m http.server "$port" --bind 127.0.0.1 >/dev/null 2>&1) &
server=$!
trap 'kill $server 2>/dev/null || true' EXIT
sleep 1
url="http://127.0.0.1:$port/preview.html"
if [ -n "$check" ] && [ -n "${PREVIEW_WAIT:-}" ]; then
  # Virtual time stands still while scripts run, so a check that times work runs on the real
  # clock, and its title is read from Chrome's debugging endpoint once it says "done: " (or the
  # wait runs out): --dump-dom would read the page as soon as it loads.
  debug_port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
  profile="$(mktemp -d)"
  "$chrome" --headless=new --disable-gpu --enable-unsafe-swiftshader ${scheme:+"$scheme"} --window-size="$size" --remote-debugging-port="$debug_port" --user-data-dir="$profile" "$url" >/dev/null 2>&1 &
  browser=$!
  trap 'kill $server $browser 2>/dev/null || true; rm -rf "$profile"' EXIT
  title=""
  deadline=$((SECONDS + PREVIEW_WAIT / 1000))
  while [ $SECONDS -lt $deadline ]; do
    sleep 1
    title="$(curl -s "http://127.0.0.1:$debug_port/json/list" | python3 -c 'import json, sys; print(next((t["title"] for t in json.load(sys.stdin) if t.get("type") == "page"), ""))' 2>/dev/null || true)"
    case "$title" in done:*) break ;; esac
  done
  echo "${title#done: }"
elif [ -n "$check" ]; then
  "$chrome" --headless=new --disable-gpu --enable-unsafe-swiftshader ${scheme:+"$scheme"} --window-size="$size" --virtual-time-budget=8000 --dump-dom "$url" 2>/dev/null | sed -n 's#.*<title>\(.*\)</title>.*#\1#p'
else
  "$chrome" --headless=new --disable-gpu --enable-unsafe-swiftshader ${scheme:+"$scheme"} --hide-scrollbars --force-device-scale-factor=1 --window-size="$size" --virtual-time-budget=6000 --screenshot="$shot" "$url" 2>/dev/null
  echo "$shot"
fi

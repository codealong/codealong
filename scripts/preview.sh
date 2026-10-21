#!/usr/bin/env bash
# Renders the UI in headless Chrome against a snapshot of your real data, with Tauri's IPC
# replaced by scripts/preview/fake-tauri.js (which records every command the UI sends).
#
#   scripts/preview.sh [view] [project] [check.js]
#
#   view     sessions | prs | todo | schedules | kanban | canvas, bar for the floating bar, notch for the
#            bar docked in the notch, peek for the bar's peek, kept for a session kept on the
#            desktop (the peek's session, in a window of its size), side for the peek's
#            session's side conversation popped out into a window of its own, quick for the
#            quick panel, waiting for the window that works through what waits on you, desktop for a card
#            pinned to the desktop, or lineage for the lines between desktop cards over
#            stand-ins for them (default sessions)
#   project  the project to show (default: the first one)
#   check.js optional script run in the page, after scripts/preview/prelude.js, which gives it
#            check(), until(), calls() and the rest, and done(), which puts its result in
#            document.title: "done: PASS 7/7", or "done: FAIL 5/7 <name>: <why>; …". The result
#            is printed, with the commands the stub had no answer for and what the check noted
#            on lines after it, and the run exits 0 only for a PASS: a check that fails, or
#            never says done, exits 1, and 3 says Chrome never showed the page at all (it can
#            hang as it starts on a busy Mac), which says nothing of the check. Without a
#            check, a screenshot is written to $PREVIEW_DIR/<view>.png. scripts/checks.sh runs
#            every check there is.
#
# PREVIEW_INIT=<file.js> runs a script in the page before either, for a screenshot of a state
# a click gets to (a dialog, say); PREVIEW_NAME names that screenshot instead of the view.
# PREVIEW_WAIT=<ms> runs a check on the real clock, for one that times work or waits on frames:
# its result is read once its title starts with "done: ", or when the wait runs out. By default
# time is virtual and stands still while scripts run. A bench (scripts/preview/*-bench.js) says
# what it measured in place of a result, which is printed as it is.
# PREVIEW_PREPARE=only builds the UI and takes the snapshots into $PREVIEW_DIR and stops there;
# PREVIEW_PREPARE=skip renders from what such a run left, so many runs share one build and one
# board (scripts/checks.sh does both).
#
# PREVIEW_SCHEME=dark renders as if macOS were in dark mode (the screenshot is <view>-dark.png).
# Under theme = "system" the page follows it; "light" and "dark" in config.toml win over it.
# PREVIEW_PLATFORM=linux draws the windows as Linux has them (src/platform.ts): its keys in
# words (Ctrl+Alt+W for ⌃⌘W) and pressed as Linux presses them, and no room for macOS's window
# buttons. The screenshot is <view>-linux.png (<view>-linux-dark.png with PREVIEW_SCHEME=dark).
# The notch view renders the bar docked in a 14-inch MacBook Pro's notch (185 by 32 points),
# closed, on a stand-in screen top with its menu bar (default 1512,320); PREVIEW_INIT can point
# at it to open it, and click a pill to show its session under the pills, which wants a taller
# window (scripts/preview/notch-session-init.js, with PREVIEW_SIZE=1512,720).
# PREVIEW_NOTCH=drawn gives it an external display's drawn notch instead.
# PREVIEW_SIZE sets the window size (default 1440,900). The bar renders on a stand-in desktop
# of that size (default 1100,64, which is also the screen width it fits itself to), and so does
# the quick panel (default 760,640: its window's width). The panel opens as the shortcut opens
# it, with nothing typed; PREVIEW_INIT can type or click from there. The peek (default 520,640),
# a side conversation's window (520,640) and the waiting window (default 820,760, an ordinary window, so on no desktop) open on
# PREVIEW_TRANSCRIPT's session when there is one, else on the first that needs you.
# A desktop card (default 360,300) shows PREVIEW_PIN: `session`, `worktree`, `todo` or `pr` for the
# latest session, the latest worktree with sessions, the first todo or a PR with sessions, or `<ref>:<key>` for one.
#
# PREVIEW_TODOS=<file> shows the todos in it (scripts/preview/todos-sample.json has some) in
# place of yours; "$lane:N" and "$session:N" in it stand for the project's Nth most recent
# worktree and session. Images come from $PREVIEW_IMAGES (default: Codealong's own images).
# scripts/preview/todos-jev-sample.json has the same todos with Jev's todo features on: one
# looks done, and the panel gets suggestions.
#
# PREVIEW_SCHEDULES=<file> shows the schedules in it (scripts/preview/schedules-sample.json has
# some) in place of yours, "$lane:N" and "$session:N" as PREVIEW_TODOS has them, and times in
# minutes from now (`nextIn`, a run's `atAgo`).
#
# PREVIEW_CANVAS=<file> opens the Canvas view on the document in it (one board's, as
# boards.json keeps it under a board's canvas) in place of a fresh tidy. PREVIEW_BOARDS=<file>
# lists the boards in it (scripts/preview/boards-sample.json has some) beside each project's own.
# Boards are on whatever config.toml's `[experimental] boards` says; PREVIEW_BOARDS_OFF=1 renders
# them off, as a new config.toml has them.
# PREVIEW_USAGE=<file> shows the plan usage in it (scripts/preview/usage-sample.json has some) in
# place of the snapshot's, which has no Claude numbers: reading them rate-limits.
#
# PREVIEW_SKILLS=<file> lists the skills in it (scripts/preview/skills-sample.json has some) in
# place of this Mac's: `every` in every folder asked about, `first` in the first of them only,
# so one skill is in several repos and one in a single repo, `account` for every account, and
# `firstAccount` for the first account asked about only.
#
# PREVIEW_INSIGHTS=<file> merges Jev's read of sessions and worktrees into the board
# (scripts/preview/insights-sample.json has some: landing chips, a risk label, a question).
#
# PREVIEW_ROUTE=<file> turns routing on and answers the quick panel as Jev might, from the
# board's own sessions and worktrees (scripts/preview/route-sample.json sends to a session;
# route-start-sample.json starts new work, with a session that may already be on it;
# route-find-sample.json takes it for a search for a session, opening it, sending to it next;
# route-late-sample.json sends as route-sample.json does, a second and a half late).
# scripts/preview/quick-search-init.js types a search for a screenshot of the suggestions.
#
# PREVIEW_TRANSCRIPT=<session key> shows that session's real conversation in its panel (its
# newest page, from --snapshot --transcript) and what it waits on you to allow (--pending);
# other sessions show their title and excerpt there. PREVIEW_BEFORE=<cursor> shows the page
# before that place in its record (a byte offset at a line's start) in place of the newest.
# PREVIEW_TOKENS=<file> answers the Usage window with the token report in it (`codealong --snapshot
# --tokens [days] > file` prints this Mac's); without one it gets a report made up from the board.
# PREVIEW_NOTICES=<file> shows the background failures in it (scripts/preview/notices-sample.json
# has two) in the accounts bar, in place of the snapshot's, which usually has none.
# PREVIEW_TRAINING=<file> says which accounts' providers may train on them (training-sample.json
# has some), for the accounts bar's flags and Settings › Accounts: the snapshot reads none, since
# that asks every provider (`codealong --snapshot --training` does).
# PREVIEW_MCP=<file> lists the MCP servers each account's agents have (mcp-sample.json has some),
# for Settings › Accounts: the snapshot reads none, since that asks each provider
# (`codealong --snapshot --mcp` does).
# PREVIEW_DEVICES=<file> shows your other devices from it (scripts/preview/devices-sample.json
# has some): Settings › Devices, the sidebar's Devices, and a few of the board's own worktrees
# shown as if they were on those devices.
# PREVIEW_FOUND=<file> lists the accounts in it under Settings › Accounts › Found on this Mac
# (scripts/preview/found-sample.json has some) in place of the ones this Mac has but config.toml
# doesn't list, which are often none.
# PREVIEW_SETUP=<file> gives the setup wizard what's in it (scripts/preview/setup-sample.json is a
# new Mac's: accounts listed and not, folders searched and not) in place of this Mac's
# (--snapshot --setup); scripts/preview/setup-init.js opens the wizard.
# PREVIEW_EMBLEMS=1 gives every session an emblem made up from its key, as `[jev] emblems` would
# once Jev had picked their icons: some with a harness's color too, for the two-tone gradient.
# PREVIEW_GROUP=worktree|repo shows Sessions, PRs and Todo grouped by worktree or by repo (Group
# by, over each list); without it, as the window was left: not grouped.
# PREVIEW_LOADING=1 shows every window as it is at launch, before the engine's first scan has
# finished: nothing read yet, and other devices' worktrees (PREVIEW_DEVICES) already in.
#
# The snapshots come from this checkout's own engine, which scripts/engine.sh builds when
# src-tauri has changed (a debug build), or from PREVIEW_BIN. The UI is built with vite alone:
# npm run typecheck is what checks its types.
# PR details come from $PREVIEW_DIR/pr-detail.json if you put one there:
#   "$(scripts/engine.sh)" --snapshot --pr owner/name 123 > .preview/pr-detail.json
set -euo pipefail
cd "$(dirname "$0")/.."
view="${1:-sessions}"
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
shot="$dir/${PREVIEW_NAME:-$view}${PREVIEW_PLATFORM:+-$PREVIEW_PLATFORM}${PREVIEW_SCHEME:+-$PREVIEW_SCHEME}.png"
prepare="${PREVIEW_PREPARE:-}"
bin="${PREVIEW_BIN:-}"
if [ -z "$bin" ] && { [ "$prepare" != skip ] || [ -n "${PREVIEW_TRANSCRIPT:-}" ]; }; then
  bin="$(scripts/engine.sh)"
fi
if [ -n "$bin" ] && [ ! -x "$bin" ]; then
  echo "No engine at $bin (PREVIEW_BIN)." >&2
  exit 1
fi

# The build and the snapshots, which PREVIEW_PREPARE=skip takes from an earlier run.
if [ "$prepare" != skip ]; then
  npx vite build >/dev/null
  mkdir -p "$dir"
  rm -rf "$dir/assets"
  cp -R dist/. "$dir/"
  "$bin" --snapshot >"$dir/board.json" 2>/dev/null
  # The floating bar gets its own small feed from the engine, not the board.
  { [ "$view" = bar ] || [ "$view" = notch ] || [ "$prepare" = only ]; } && "$bin" --snapshot --bar >"$dir/bar-feed.json" 2>/dev/null
  "$bin" --snapshot --accounts >"$dir/accounts.json" 2>/dev/null
  "$bin" --snapshot --start-options >"$dir/start-options.json" 2>/dev/null
  "$bin" --snapshot --editor-icons >"$dir/editor-icons.json" 2>/dev/null
  "$bin" --snapshot --shell-commands >"$dir/shell-commands.json" 2>/dev/null
  # The skills of every worktree on the board, for the `/` lists.
  python3 -c 'import json, sys; sys.stdout.write("\0".join(sorted({l["path"] for l in json.load(open(sys.argv[1]))["lanes"] if not l.get("device")})))' "$dir/board.json" 2>/dev/null |
    xargs -0 "$bin" --snapshot --skills >"$dir/skills.json" 2>/dev/null
  "$bin" --snapshot --config >"$dir/config.json" 2>/dev/null
  if [ -z "${PREVIEW_FOUND:-}" ] || [ "$prepare" = only ]; then
    "$bin" --snapshot --found >"$dir/found-here.json" 2>/dev/null
  fi
  if [ -z "${PREVIEW_SETUP:-}" ] || [ "$prepare" = only ]; then
    "$bin" --snapshot --setup >"$dir/setup-here.json" 2>/dev/null
  fi
elif [ ! -f "$dir/board.json" ]; then
  echo "PREVIEW_PREPARE=skip, but $dir has no build and snapshots: run with PREVIEW_PREPARE=only first." >&2
  exit 1
fi
[ "$prepare" = only ] && exit 0

cp scripts/preview/fake-tauri.js "$dir/"
# What a PREVIEW_* variable names goes beside the page under the name fake-tauri.js reads, and
# the last run's is taken away.
fixture() {
  rm -f "$dir/$1-fixture.json"
  if [ -n "$2" ]; then
    cp "$2" "$dir/$1-fixture.json"
  fi
}
fixture todos "${PREVIEW_TODOS:-}"
fixture schedules "${PREVIEW_SCHEDULES:-}"
fixture insights "${PREVIEW_INSIGHTS:-}"
fixture route "${PREVIEW_ROUTE:-}"
fixture canvas "${PREVIEW_CANVAS:-}"
fixture boards "${PREVIEW_BOARDS:-}"
fixture usage "${PREVIEW_USAGE:-}"
fixture devices "${PREVIEW_DEVICES:-}"
fixture notices "${PREVIEW_NOTICES:-}"
fixture training "${PREVIEW_TRAINING:-}"
fixture mcp "${PREVIEW_MCP:-}"
fixture skills "${PREVIEW_SKILLS:-}"
fixture tokens "${PREVIEW_TOKENS:-}"
rm -f "$dir/transcript.json" "$dir/transcript-key.json" "$dir/pending.json"
if [ -n "${PREVIEW_TRANSCRIPT:-}" ]; then
  "$bin" --snapshot --transcript "$PREVIEW_TRANSCRIPT" ${PREVIEW_BEFORE:+--before "$PREVIEW_BEFORE"} >"$dir/transcript.json" 2>/dev/null
  "$bin" --snapshot --pending "$PREVIEW_TRANSCRIPT" >"$dir/pending.json" 2>/dev/null
  echo "{\"key\": \"$PREVIEW_TRANSCRIPT\"}" >"$dir/transcript-key.json"
fi
# After the conversation is read: reading it keeps the images its prompts came with.
images="${PREVIEW_IMAGES:-$HOME/Library/Application Support/Codealong/images}"
rm -rf "$dir/images"
if [ -d "$images" ]; then
  cp -R "$images" "$dir/images"
fi
cp "${PREVIEW_FOUND:-$dir/found-here.json}" "$dir/found.json"
cp "${PREVIEW_SETUP:-$dir/setup-here.json}" "$dir/setup.json"

# The sidebar opens, though it starts folded in the app: the checks find the views and filters
# by their labels there (sidebar-check.js folds it).
setup="localStorage.setItem('codealong.view','$view');localStorage.setItem('codealong.age','all');localStorage.setItem('codealong.sidebarCollapsed','0');"
[ -n "$project" ] && setup="$setup localStorage.setItem('codealong.project','$project');"
[ -n "${PREVIEW_GROUP:-}" ] && setup="$setup localStorage.setItem('codealong.sessionGroup','$PREVIEW_GROUP'); localStorage.setItem('codealong.prGroup','$PREVIEW_GROUP'); localStorage.setItem('codealong.todoGroup','$PREVIEW_GROUP');"
[ -n "${PREVIEW_LOADING:-}" ] && setup="$setup window.__PREVIEW_LOADING__=true;"
[ -n "${PREVIEW_EMBLEMS:-}" ] && setup="$setup window.__PREVIEW_EMBLEMS__=true;"
# The window remembers whether boards were on, for before the board comes: as the stub has them.
if [ -n "${PREVIEW_BOARDS_OFF:-}" ]; then
  setup="$setup window.__PREVIEW_BOARDS_OFF__=true; localStorage.setItem('codealong.boardsOn','0');"
else
  setup="$setup localStorage.setItem('codealong.boardsOn','1');"
fi
[ -n "${PREVIEW_PLATFORM:-}" ] && setup="$setup window.__CODEALONG_PLATFORM__='$PREVIEW_PLATFORM';"
size="${PREVIEW_SIZE:-1440,900}"
# The windows that float over the desktop render on a stand-in for it: `on_desktop <the page's
# class> [a layer over it]`.
backdrop="linear-gradient(135deg,rgb(93,111,134),rgb(169,182,196))"
on_desktop() {
  setup="$setup</script><style>html.$1{background:${2:-}$backdrop!important}</style><script>"
}
if [ "$view" = bar ]; then
  size="${PREVIEW_SIZE:-1100,64}"
  # Headless Chrome's screen is 800 wide whatever the window; the bar fits itself to the screen.
  setup="$setup window.__CODEALONG_BAR__=true;Object.defineProperty(screen,'availWidth',{value:${size%%,*}});"
  on_desktop is-bar
fi
if [ "$view" = notch ]; then
  size="${PREVIEW_SIZE:-1512,320}"
  width="${size%%,*}"
  if [ "${PREVIEW_NOTCH:-}" = drawn ]; then
    notch="{x:$((width / 2)),top:1440,width:180,height:25,real:false}"
    menu=25
    screen=1440
  else
    notch="{x:$((width / 2)),top:982,width:185,height:32,real:true}"
    menu=32
    screen=982
  fi
  # Headless Chrome's screen is 800 by 600 whatever the window; the bar fits itself to the screen.
  setup="$setup window.__CODEALONG_BAR__=true; window.__BAR_MODE__={mode:'notch',notch:$notch};Object.defineProperty(screen,'availWidth',{value:$width});Object.defineProperty(screen,'height',{value:$screen});"
  on_desktop is-bar "linear-gradient(rgba(236,236,238,0.82) 0 ${menu}px,transparent ${menu}px),"
fi
if [ "$view" = quick ]; then
  size="${PREVIEW_SIZE:-760,640}"
  setup="$setup window.__CODEALONG_QUICK__=true;"
  on_desktop is-quick
fi
if [ "$view" = waiting ]; then
  size="${PREVIEW_SIZE:-820,760}"
  setup="$setup window.__CODEALONG_WAITING__=true;"
  [ -n "${PREVIEW_TRANSCRIPT:-}" ] && setup="$setup window.__WAITING_ASKED__={key:'$PREVIEW_TRANSCRIPT'};"
fi
if [ "$view" = desktop ]; then
  size="${PREVIEW_SIZE:-360,300}"
  setup="$setup window.__CODEALONG_DESKTOP__=true; window.__CODEALONG_PIN__='${PREVIEW_PIN:-session}';"
  on_desktop is-desktop
fi
if [ "$view" = lineage ]; then
  size="${PREVIEW_SIZE:-1440,420}"
  cp scripts/preview/lineage-desk.js "$dir/"
  setup="$setup</script><script src=\"./lineage-desk.js\"></script><script>"
fi
if [ "$view" = peek ]; then
  size="${PREVIEW_SIZE:-520,640}"
  setup="$setup localStorage.setItem('codealong.preview.peek','${PREVIEW_TRANSCRIPT:-}'); window.__CODEALONG_PEEK__=true;"
  on_desktop is-peek
fi
if [ "$view" = kept ]; then
  size="${PREVIEW_SIZE:-640,720}"
  setup="$setup localStorage.setItem('codealong.preview.peek','${PREVIEW_TRANSCRIPT:-}'); window.__CODEALONG_KEPT__=true;"
  on_desktop is-peek
fi
if [ "$view" = side ]; then
  size="${PREVIEW_SIZE:-520,640}"
  setup="$setup localStorage.setItem('codealong.preview.peek','${PREVIEW_TRANSCRIPT:-}'); window.__CODEALONG_KEPT__=true; window.__CODEALONG_SIDE__=true;"
  on_desktop is-peek
fi
tail=""
# A check starts from prelude.js, and so may a PREVIEW_INIT script (a check can be one).
if [ -n "${PREVIEW_INIT:-}" ] || [ -n "$check" ]; then
  cp scripts/preview/prelude.js "$dir/"
  tail='<script src="./prelude.js"></script>'
fi
if [ -n "${PREVIEW_INIT:-}" ]; then
  cp "$PREVIEW_INIT" "$dir/init.js"
  tail="$tail"'<script src="./init.js"></script>'
fi
if [ -n "$check" ]; then
  cp "$check" "$dir/check.js"
  tail="$tail"'<script src="./check.js"></script>'
  setup="$setup window.__PREVIEW_SIZE__='$size';"
fi
sed -e "s#<head>#<head><script src=\"./fake-tauri.js\"></script><script>$setup</script>#" -e "s#</body>#$tail</body>#" "$dir/index.html" >"$dir/preview.html"

(cd "$dir" && exec python3 -m http.server "$port" --bind 127.0.0.1 >/dev/null 2>&1) &
server=$!
browser=""
work=""
# A Chrome that hung may not go when asked: it gets a few seconds, since its profile can't be
# removed from under it, and then it's made to.
cleanup() {
  kill $server $browser 2>/dev/null || true
  if [ -n "$browser" ]; then
    local n=0
    while kill -0 "$browser" 2>/dev/null && [ $n -lt 20 ]; do
      sleep 0.2
      n=$((n + 1))
    done
    kill -9 "$browser" 2>/dev/null || true
  fi
  [ -z "$work" ] || rm -rf "$work" 2>/dev/null || true
}
trap cleanup EXIT
sleep 1
url="http://127.0.0.1:$port/preview.html"

# Prints what a check's title says after "done: ", its result first and then, on lines of their
# own, what prelude.js put after it. A check passes only by saying PASS; a bench says what it
# measured, and that's all that's asked of it. No title at all is a page Chrome never showed.
report() {
  local title="$1" result
  case "$check" in
    *-bench.js)
      echo "${title#done: }"
      return 0
      ;;
  esac
  case "$title" in
    done:*) result="${title#done: }" ;;
    "") result="FAIL 0/0 no result: Chrome never showed the page" ;;
    *) result="FAIL 0/0 no result: the check never said done (the page's title is \"$title\")" ;;
  esac
  printf '%s\n' "$result" | awk '{ n = split($0, part, / ## /); for (i = 1; i <= n; i++) print part[i] }'
  [ -n "$title" ] || return 3
  case "$result" in
    PASS\ *) return 0 ;;
    *) return 1 ;;
  esac
}

if [ -n "$check" ]; then
  # Chrome runs beside this script, not under it, so a run that's stopped (checks.sh gives each
  # one so long) takes its browser and its server with it.
  trap 'exit 130' INT TERM HUP
  work="$(mktemp -d)"
fi
# A check's window is a browser window, whose size takes in its frame: as an app window (the
# real clock) a title bar, 32 points on Chrome 154, and on virtual time, where --dump-dom wants
# a browser window, its tabs and toolbar, 87. The page gets that much more, so it's the size
# asked for, as the screenshot is; the bar, 64 points high, had no page at all before, and a
# hidden page's timers barely run. A browser window is 500 points wide at least. prelude.js
# notes the size the page got when it isn't the one asked for.
width="${size%%,*}"
height="${size##*,}"
if [ -n "$check" ] && [ -n "${PREVIEW_WAIT:-}" ]; then
  # Virtual time stands still while scripts run, so a check that times work runs on the real
  # clock, and its title is read from Chrome's debugging endpoint once it says "done: " (or the
  # wait runs out): --dump-dom would read the page as soon as it loads. The endpoint gives the
  # title as HTML.
  debug_port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
  "$chrome" --headless=new --disable-gpu --enable-unsafe-swiftshader ${scheme:+"$scheme"} --window-size="$width,$((height + 32))" --remote-debugging-port="$debug_port" --user-data-dir="$work/profile" --app="$url" >/dev/null 2>&1 &
  browser=$!
  title=""
  started=$SECONDS
  deadline=$((SECONDS + PREVIEW_WAIT / 1000))
  while [ $SECONDS -lt $deadline ]; do
    sleep 1
    # Half a minute with no page to ask about is a Chrome that hung as it started.
    if [ -z "$title" ] && [ $((SECONDS - started)) -gt 30 ]; then
      break
    fi
    title="$(curl -s --max-time 5 "http://127.0.0.1:$debug_port/json/list" | python3 -c 'import html, json, sys; print(html.unescape(next((t["title"] for t in json.load(sys.stdin) if t.get("type") == "page"), "")))' 2>/dev/null || true)"
    case "$title" in done:*) break ;; esac
  done
  report "$title" || exit $?
elif [ -n "$check" ]; then
  "$chrome" --headless=new --disable-gpu --enable-unsafe-swiftshader ${scheme:+"$scheme"} --window-size="$width,$((height + 87))" --virtual-time-budget=8000 --user-data-dir="$work/profile" --dump-dom "$url" >"$work/dom.html" 2>/dev/null &
  browser=$!
  # Under a heavy load Chrome can print the page and then never exit: once the page is out it
  # gets a few seconds more, then it's stopped.
  printed=0
  while kill -0 "$browser" 2>/dev/null; do
    if grep -q '</html>' "$work/dom.html"; then
      printed=$((printed + 1))
      if [ $printed -gt 25 ]; then
        kill "$browser" 2>/dev/null || true
        break
      fi
    fi
    sleep 0.2
  done
  wait "$browser" 2>/dev/null || true
  title="$(python3 -c 'import html, re, sys; m = re.search(r"<title>(.*?)</title>", sys.stdin.read(), re.S); print(" ".join(html.unescape(m.group(1)).split()) if m else "")' <"$work/dom.html")"
  report "$title" || exit $?
else
  "$chrome" --headless=new --disable-gpu --enable-unsafe-swiftshader ${scheme:+"$scheme"} --hide-scrollbars --force-device-scale-factor=1 --window-size="$size" --virtual-time-budget=6000 --screenshot="$shot" "$url" 2>/dev/null
  echo "$shot"
fi

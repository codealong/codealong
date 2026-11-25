#!/usr/bin/env bash
# Renders the UI in headless Chrome against a snapshot of your real data, with Tauri's IPC
# replaced by scripts/preview/fake-tauri.js (which records every command the UI sends).
#
#   scripts/preview.sh [view] [project] [check.js]
#
#   view     sessions | prs | todo | schedules | kanban | canvas, bar for the floating bar, notch for the
#            bar docked in the notch, peek for the bar's peek, kept for a session kept on the
#            desktop (the peek's session, in a window of its size), side for the peek's
#            session's side conversation popped out into a window of its own, terminal for a
#            terminal of Codealong's own popped out of its session's drawer (the stub's sample
#            screen, for the board's first session), quick for the
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
# PREVIEW_FIXTURE=1 renders the engine's made-up Mac in place of this one (`codealong --snapshot
# --fixture`, src-tauri/src/fixture.rs): three projects (Northwind, Atlas, Kestrel) under
# /Users/me, worktrees in every bay, sessions in every status on five harnesses, PRs in every
# state with some that wait on your review, todos and usage, the same on every Mac and every
# day, with its times counted back from now. Everything else is as it is without it: the
# PREVIEW_* files' "$lane:N" and "$session:N" stand for its worktrees and sessions, and
# PREVIEW_TRANSCRIPT names one of its sessions (their keys read as what they are,
# `claude:claude:checkout-approval`; `"$(scripts/engine.sh)" --snapshot --fixture` lists them),
# whose conversation and what it waits on are made up too. Nothing of this Mac's may be in its
# page, so:
# - its build and snapshots have a folder of their own, .preview/fixture, where the screenshot is
#   written too (PREVIEW_DIR names another, for either Mac);
# - the engine is asked to name the made-up Mac before anything it prints is believed
#   (`--snapshot --fixture name`): one built before it had one, which PREVIEW_BIN may name,
#   prints this Mac's board whatever it's asked, and a release build has none;
# - a folder says which Mac its snapshots are of ($PREVIEW_DIR/snapshot-of) only once it has
#   all of them, the other Mac's are taken away before the first is written, and a run with
#   PREVIEW_PREPARE=skip stops at a folder that holds the other Mac's, or one a run left half
#   made, rather than render it;
# - none of this Mac's kept images are copied unless PREVIEW_IMAGES names a folder of them, and
#   a pr-detail.json left in the folder is taken away.
# scripts/checks.sh --fixture runs the checks this way.
#
# PREVIEW_DATE=<YYYY-MM-DD> has the page take that day for today, from noon: its clock (Date)
# starts there and runs on (scripts/preview/clock.js), and window.__PREVIEW_DATE__ says the day,
# for a check to hold the page to. For what a window says differently on another day, a year's
# end above all (a todo due next year says the year): scripts/preview/due-check.js has a run on
# December 28. Only the page's clock moves, so it's for what the page dates by itself (the
# sample todos' due dates, PREVIEW_TODOS); a board's own times stay as they were snapshotted.
# PREVIEW_SCHEME=dark renders as if macOS were in dark mode (the screenshot is <view>-dark.png).
# Under theme = "system" the page follows it; "light" and "dark" in config.toml win over it.
# PREVIEW_PLATFORM=linux draws the windows as Linux has them (src/platform.ts): its keys in
# words (Ctrl+Alt+W for ⌃⌘W) and pressed as Linux presses them, and no room for macOS's window
# buttons. The screenshot is <view>-linux.png (<view>-linux-dark.png with PREVIEW_SCHEME=dark).
# PREVIEW_PLATFORM=windows draws them as Windows has them, which is Linux's way, <view>-windows.png.
# The notch view renders the bar docked in a 14-inch MacBook Pro's notch (185 by 32 points),
# closed, on a stand-in screen top with its menu bar (default 1512,320); PREVIEW_INIT can point
# at it to open it, and click a pill to show its session under the pills, which wants a taller
# window (scripts/preview/notch-session-init.js, with PREVIEW_SIZE=1512,720).
# PREVIEW_NOTCH=drawn gives it an external display's drawn notch instead.
# PREVIEW_SIZE sets the window size (default 1440,900). The bar renders on a stand-in desktop
# of that size (default 1200,64, which is also the screen width it fits itself to), and so does
# the quick panel (default 760,640: its window's width). The panel opens as the shortcut opens
# it, with nothing typed; PREVIEW_INIT can type or click from there. The peek (default 520,640),
# a side conversation's window (520,640) and the waiting window (default 820,760, an ordinary window, so on no desktop) open on
# PREVIEW_TRANSCRIPT's session when there is one, else on the first that needs you.
# A desktop card (default 360,300) shows PREVIEW_PIN: `session`, `worktree`, `todo` or `pr` for the
# latest session, the latest worktree with sessions, the first todo or a PR with sessions, or `<ref>:<key>` for one;
# with PREVIEW_DEVICES, `session:@` or `worktree:@` for a connected device's (`@<short>` for that device's).
#
# PREVIEW_TODOS=<file> shows the todos in it (scripts/preview/todos-sample.json has some) in
# place of yours; "$lane:N" and "$session:N" in it stand for the project's Nth most recent
# worktree and session. Images come from $PREVIEW_IMAGES (default: Codealong's own images).
# scripts/preview/todos-jev-sample.json has the same todos with Jev's todo features on: one
# looks done, and the panel gets suggestions.
#
# PREVIEW_SCHEDULES=<file> shows the schedules in it (scripts/preview/schedules-sample.json has
# some) in place of yours, "$lane:N" and "$session:N" as PREVIEW_TODOS has them, and times in
# minutes from now (`nextIn`, a run's `atAgo`, a finished one's `finishedAgo`); its `harness`
# list is the schedules the harnesses keep themselves.
#
# PREVIEW_CANVAS=<file> opens the Canvas view on the document in it (one board's, as
# boards.json keeps it under a board's canvas) in place of a fresh tidy. PREVIEW_BOARDS=<file>
# lists the boards in it (scripts/preview/boards-sample.json has some, boards-everything.json one
# in each project that takes in everything); without one there are none, as on a new machine,
# and the Board view says how to make one.
# Boards are on whatever config.toml's `[experimental] boards` says; PREVIEW_BOARDS_OFF=1 renders
# them off, as a new config.toml has them.
# PREVIEW_VOICE=1 renders with voice on (`[experimental] voice`), which is off otherwise as a new
# config.toml has it; PREVIEW_VOICE=<session key> also has that session listening, a coach a few
# minutes into a call, its conversation the meeting it heard. scripts/preview/voice-init.js has the
# first session listed listen instead, and shows it.
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
# route-search-sample.json for a search none of the sessions fits;
# route-late-sample.json sends as route-sample.json does, a second and a half late).
# scripts/preview/quick-search-init.js types a search for a screenshot of the suggestions.
#
# PREVIEW_TRANSCRIPT=<session key> shows that session's real conversation in its panel (its
# newest page, from --snapshot --transcript) and what it waits on you to allow (--pending);
# other sessions show their title and excerpt there. PREVIEW_BEFORE=<cursor> shows the page
# before that place in its record (a byte offset at a line's start) in place of the newest.
# The pictures, films and recordings its calls sent you show from where they are on this Mac.
# PREVIEW_TOKENS=<file> answers the Analytics window with the token report in it (`codealong --snapshot
# --tokens [days] > file` prints this Mac's); without one it gets a report made up from the board.
# PREVIEW_NOTICES=<file> shows the background failures in it (scripts/preview/notices-sample.json
# has two) in the accounts bar, in place of the snapshot's, which usually has none.
# PREVIEW_TRAINING=<file> says which accounts' providers may train on them (training-sample.json
# has some), for the accounts bar's flags and Settings › Accounts: the snapshot reads none, since
# that asks every provider (`codealong --snapshot --training` does).
# PREVIEW_RESETS=<file> says which accounts hold limit resets (resets-sample.json has a Codex
# account with three), for the accounts bar's marks and cards: the snapshot reads none, since that
# asks each provider (`codealong --snapshot --resets` does).
# PREVIEW_MCP=<file> lists the MCP servers each account's agents have (mcp-sample.json has some),
# for Settings › Accounts: the snapshot reads none, since that asks each provider
# (`codealong --snapshot --mcp` does).
# PREVIEW_DEVICES=<file> shows your other devices from it (scripts/preview/devices-sample.json
# has some): Settings › Devices, the sidebar's Devices, and a few of the board's own worktrees
# shown as if they were on those devices.
# PREVIEW_FOUND=<file> lists the accounts in it under Settings › Accounts › Found on this machine
# (scripts/preview/found-sample.json has some) in place of the ones this Mac has but config.toml
# doesn't list, which are often none.
# PREVIEW_SETUP=<file> gives the setup wizard what's in it (scripts/preview/setup-sample.json is a
# new Mac's: accounts listed and not, folders searched and not) in place of this Mac's
# (--snapshot --setup); scripts/preview/setup-init.js opens the wizard, setup-emblems-init.js at its
# session emblems step with Emoji picked.
# PREVIEW_EMBLEMS=1 gives every session an emblem made up from its key, as `[emblems] kind = "emoji"`
# would once Jev had picked them; PREVIEW_EMBLEMS=drawn a picture of scripts/preview/emblems/, as
# `"drawn"` would once they were drawn.
# PREVIEW_ANALYTICS=1 turns `[experimental] analytics` on, with a made-up report for the Analytics
# window's Your part and Sessions (scripts/preview/usage-part-init.js opens them).
# PREVIEW_KEYS=held turns `[appearance] show_keys` off: keyboard shortcuts show only while ⌘ or ⌃
# is held (src/keysShown.ts). Otherwise they show all the time, as a new config.toml has them.
# PREVIEW_GLASS=1 turns `[appearance] glass` on. It's off otherwise, though macOS has it on by
# default, so the checks see the windows as they're drawn without it. With it the main window's chrome
# and board are see-through, over a stand-in for the glass macOS draws behind them: the theme's
# surface tinted olive to blue, as macOS 26's Liquid Glass is nearly white in light and a dark gray
# in dark, with the wallpaper's hue, strong enough that a gray with a hue of its own shows against
# it. The quick panel has it only
# under its card, behind the page as the app puts it, so it shows only where the page lets it.
# PREVIEW_GROUP=project|repo|worktree shows Sessions, PRs, Todo and Schedules grouped (Group
# by, over each list); without it, as the window was left: not grouped.
# PREVIEW_LOADING=1 shows every window as it is at launch, before the engine's first scan has
# finished: nothing read yet, and other devices' worktrees (PREVIEW_DEVICES) already in.
# PREVIEW_CHROME, PREVIEW_POLL and PREVIEW_WAIT_POLL are test knobs (src/checks.test.ts): another
# Chrome binary, and shorter sleep intervals for cleanup/title polls (defaults 0.2 s and 1 s).
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
# The made-up Mac's build and snapshots are kept apart from this Mac's.
dir="${PREVIEW_DIR:-.preview${PREVIEW_FIXTURE:+/fixture}}"
chrome="${PREVIEW_CHROME:-}"
if [ -z "$chrome" ]; then
  if [ -x "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" ]; then
    chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
  else
    chrome="$(command -v google-chrome || command -v google-chrome-stable || command -v chromium || command -v chromium-browser || echo "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")"
  fi
fi
extra_chrome=()
if [ "$(uname)" = Linux ]; then
  extra_chrome+=(--no-sandbox --disable-dev-shm-usage)
  if [ -z "${PREVIEW_PLATFORM:-}" ] || [ "${PREVIEW_PLATFORM:-}" = mac ]; then
    extra_chrome+=(--user-agent="Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
  fi
fi
# As it starts, Chrome links its own program into a folder of its own, so an update can't change
# it under the running browser. macOS counts that as changing an app, and refuses it for a
# program it can't ask about, such as the bare Claude Code binary that hosts a background
# session, with a notification that the program "was prevented from modifying apps". A headless
# run needs no such copy, so every launch here turns it off (MacAppCodeSignClone).
scheme=""
case "${PREVIEW_SCHEME:-}" in
  dark) scheme="--blink-settings=preferredColorScheme=0" ;;
  light) scheme="--blink-settings=preferredColorScheme=1" ;;
esac
# A day, and nothing else: it's written into the page.
case "${PREVIEW_DATE:-}" in
  "" | [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]) ;;
  *)
    echo "PREVIEW_DATE is a day, written 2026-12-28; it's $PREVIEW_DATE." >&2
    exit 1
    ;;
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
# What the engine is asked for: this Mac's, each by its mode (--snapshot --accounts), or with
# PREVIEW_FIXTURE its made-up Mac's, each by the mode's name (--snapshot --fixture accounts).
# $dir/snapshot-of says which of the two its snapshots are of, once it has them all ($marked).
# The made-up Mac's says more than its name: a folder marked before the engine was asked for
# its name (a commit of the branch this came in on, never landed) could hold this Mac's board
# under the bare name, and no such folder says this.
snap=(--snapshot)
mode="--"
of="this Mac"
marked="$of"
if [ -n "${PREVIEW_FIXTURE:-}" ]; then
  snap=(--snapshot --fixture)
  mode=""
  of="the made-up Mac"
  marked="$of, as its engine named it"
  # An engine from before it had a made-up Mac takes --fixture for nothing and prints this Mac's
  # board, which would be shown, and kept, as the made-up one. So it's asked for the made-up
  # Mac's name first, which only an engine that has one answers with (fixture.rs `NAME`).
  if [ -n "$bin" ] && [ "$("$bin" --snapshot --fixture name 2>/dev/null)" != "$of" ]; then
    echo "The engine at $bin has no made-up Mac: it was built before it had one, or isn't a debug build. scripts/engine.sh builds this checkout's, which PREVIEW_BIN is used in place of." >&2
    exit 1
  fi
fi
# What a folder says its snapshots are of while they're being taken, and so of a run that stopped
# before it had them all.
unfinished="a run that stopped before it had them all"

# The build and the snapshots, which PREVIEW_PREPARE=skip takes from an earlier run.
if [ "$prepare" != skip ]; then
  npx vite build >/dev/null
  mkdir -p "$dir"
  # What an earlier run left of the other Mac, or of a run that stopped part way, is taken away
  # before anything is written: some of these are only written for some views (the bar's feed),
  # and would stay to be shown as this run's. (What it left of the same Mac stays, as it did.)
  was="$(cat "$dir/snapshot-of" 2>/dev/null || echo "this Mac")"
  echo "$unfinished" >"$dir/snapshot-of"
  if [ "$was" != "$marked" ]; then
    rm -f "$dir/board.json" "$dir/bar-feed.json" "$dir/accounts.json" "$dir/start-options.json" "$dir/editor-icons.json" \
      "$dir/shell-commands.json" "$dir/skills.json" "$dir/config.json" "$dir/found-here.json" "$dir/setup-here.json"
  fi
  rm -rf "$dir/assets"
  cp -R dist/. "$dir/"
  # Why the first snapshot failed is worth saying: a build with no made-up Mac, for one.
  if ! "$bin" "${snap[@]}" >"$dir/board.json" 2>"$dir/snapshot.err"; then
    cat "$dir/snapshot.err" >&2
    echo "The engine at $bin couldn't snapshot $of." >&2
    exit 1
  fi
  rm -f "$dir/snapshot.err"
  # The floating bar gets its own small feed from the engine, not the board.
  { [ "$view" = bar ] || [ "$view" = notch ] || [ "$prepare" = only ]; } && "$bin" "${snap[@]}" "${mode}bar" >"$dir/bar-feed.json" 2>/dev/null
  "$bin" "${snap[@]}" "${mode}accounts" >"$dir/accounts.json" 2>/dev/null
  "$bin" "${snap[@]}" "${mode}start-options" >"$dir/start-options.json" 2>/dev/null
  "$bin" "${snap[@]}" "${mode}editor-icons" >"$dir/editor-icons.json" 2>/dev/null
  "$bin" "${snap[@]}" "${mode}shell-commands" >"$dir/shell-commands.json" 2>/dev/null
  # The skills of every worktree on the board, for the `/` lists.
  python3 -c 'import json, sys; sys.stdout.write("\0".join(sorted({l["path"] for l in json.load(open(sys.argv[1]))["lanes"] if not l.get("device")})))' "$dir/board.json" 2>/dev/null |
    xargs -0 "$bin" "${snap[@]}" "${mode}skills" >"$dir/skills.json" 2>/dev/null
  "$bin" "${snap[@]}" "${mode}config" >"$dir/config.json" 2>/dev/null
  if [ -z "${PREVIEW_FOUND:-}" ] || [ "$prepare" = only ]; then
    "$bin" "${snap[@]}" "${mode}found" >"$dir/found-here.json" 2>/dev/null
  fi
  if [ -z "${PREVIEW_SETUP:-}" ] || [ "$prepare" = only ]; then
    "$bin" "${snap[@]}" "${mode}setup" >"$dir/setup-here.json" 2>/dev/null
  fi
  # A real PR's details left here would be the one thing the made-up Mac's page isn't.
  [ -z "${PREVIEW_FIXTURE:-}" ] || rm -f "$dir/pr-detail.json"
  # Every snapshot is in it, a failed one having stopped the script, so now it says whose.
  echo "$marked" >"$dir/snapshot-of"
elif [ ! -f "$dir/board.json" ]; then
  echo "PREVIEW_PREPARE=skip, but $dir has no build and snapshots: run with PREVIEW_PREPARE=only first." >&2
  exit 1
elif [ "$(cat "$dir/snapshot-of" 2>/dev/null || echo "this Mac")" != "$marked" ]; then
  # A folder an earlier version prepared says nothing, and holds this Mac's.
  echo "PREVIEW_PREPARE=skip, but $dir holds the snapshots of $(cat "$dir/snapshot-of" 2>/dev/null || echo "this Mac"), not of $marked: run with PREVIEW_PREPARE=only first, with PREVIEW_FIXTURE=1 for the made-up Mac and without it for this one." >&2
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
fixture resets "${PREVIEW_RESETS:-}"
fixture mcp "${PREVIEW_MCP:-}"
fixture skills "${PREVIEW_SKILLS:-}"
fixture tokens "${PREVIEW_TOKENS:-}"
rm -f "$dir/transcript.json" "$dir/transcript-key.json" "$dir/pending.json"
if [ -n "${PREVIEW_TRANSCRIPT:-}" ]; then
  "$bin" "${snap[@]}" "${mode}transcript" "$PREVIEW_TRANSCRIPT" ${PREVIEW_BEFORE:+--before "$PREVIEW_BEFORE"} >"$dir/transcript.json" 2>/dev/null
  "$bin" "${snap[@]}" "${mode}pending" "$PREVIEW_TRANSCRIPT" >"$dir/pending.json" 2>/dev/null
  echo "{\"key\": \"$PREVIEW_TRANSCRIPT\"}" >"$dir/transcript-key.json"
fi
# The files its calls sent you, which the page loads from ./files as the app loads them from
# where they are (api.ts `sentFileUrl`): linked there by their full paths, not copied, as a film
# can be large.
rm -rf "$dir/files"
if [ -s "$dir/transcript.json" ]; then
  python3 -c 'import json, sys; sys.stdout.write("".join(f + "\0" for f in {f for i in json.load(open(sys.argv[1]))["items"] for f in (i.get("sent") or {}).get("files", [])}))' "$dir/transcript.json" 2>/dev/null |
    while IFS= read -r -d '' f; do
      if [ -f "$f" ]; then
        mkdir -p "$dir/files$(dirname "$f")"
        ln -s "$f" "$dir/files$f"
      fi
    done
fi
# After the conversation is read: reading it keeps the images its prompts came with. The made-up
# Mac has kept none, so this Mac's stay out of its page.
images="${PREVIEW_IMAGES:-}"
if [ -z "$images" ] && [ -z "${PREVIEW_FIXTURE:-}" ]; then
  images="$HOME/Library/Application Support/Codealong/images"
fi
rm -rf "$dir/images"
if [ -n "$images" ] && [ -d "$images" ]; then
  # One the app removes while they're copied is no reason to stop.
  cp -R "$images" "$dir/images" 2>/dev/null || true
fi
if [ "${PREVIEW_EMBLEMS:-}" = drawn ]; then
  mkdir -p "$dir/images"
  cp scripts/preview/emblems/*.png "$dir/images/"
fi
cp "${PREVIEW_FOUND:-$dir/found-here.json}" "$dir/found.json"
cp "${PREVIEW_SETUP:-$dir/setup-here.json}" "$dir/setup.json"

# The sidebar opens, though it starts folded in the app: the checks find the views and filters
# by their labels there (sidebar-check.js folds it).
setup="localStorage.setItem('codealong.view','$view');localStorage.setItem('codealong.age','all');localStorage.setItem('codealong.sidebarCollapsed','0');"
[ -n "$project" ] && setup="$setup localStorage.setItem('codealong.project','$project');"
[ -n "${PREVIEW_GROUP:-}" ] && setup="$setup localStorage.setItem('codealong.sessionGroup','$PREVIEW_GROUP'); localStorage.setItem('codealong.prGroup','$PREVIEW_GROUP'); localStorage.setItem('codealong.todoGroup','$PREVIEW_GROUP'); localStorage.setItem('codealong.scheduleGroup','$PREVIEW_GROUP');"
[ -n "${PREVIEW_LOADING:-}" ] && setup="$setup window.__PREVIEW_LOADING__=true;"
[ -n "${PREVIEW_EMBLEMS:-}" ] && setup="$setup window.__PREVIEW_EMBLEMS__='$PREVIEW_EMBLEMS';"
[ -n "${PREVIEW_ANALYTICS:-}" ] && setup="$setup window.__PREVIEW_ANALYTICS__=true;"
[ -n "${PREVIEW_EXPERIMENTS:-}" ] && setup="$setup window.__PREVIEW_EXPERIMENTS__=true;"
[ -n "${PREVIEW_GLASS:-}" ] && setup="$setup window.__PREVIEW_GLASS__=true;"
[ "${PREVIEW_KEYS:-}" = held ] && setup="$setup window.__PREVIEW_KEYS_HELD__=true;"
# The window remembers whether boards were on, for before the board comes: as the stub has them.
if [ -n "${PREVIEW_BOARDS_OFF:-}" ]; then
  setup="$setup window.__PREVIEW_BOARDS_OFF__=true; localStorage.setItem('codealong.boardsOn','0');"
else
  setup="$setup localStorage.setItem('codealong.boardsOn','1');"
fi
[ -n "${PREVIEW_PLATFORM:-}" ] && setup="$setup window.__CODEALONG_PLATFORM__='$PREVIEW_PLATFORM';"
if [ -n "${PREVIEW_VOICE:-}" ]; then
  setup="$setup window.__PREVIEW_VOICE__=true;"
  [ "$PREVIEW_VOICE" = 1 ] || setup="$setup window.__LISTENS__='$PREVIEW_VOICE';"
fi
size="${PREVIEW_SIZE:-1440,900}"
# The windows that float over the desktop render on a stand-in for it: `on_desktop <the page's
# class> [a layer over it]`.
backdrop="linear-gradient(135deg,rgb(93,111,134),rgb(169,182,196))"
on_desktop() {
  setup="$setup</script><style>html.$1{background:${2:-}$backdrop!important}</style><script>"
}
[ -n "${PREVIEW_GLASS:-}" ] && setup="$setup</script><style>html.is-glass{background:linear-gradient(135deg,color-mix(in srgb,var(--surface) 82%,rgb(120,150,60)),color-mix(in srgb,var(--surface) 82%,rgb(60,120,200)))!important}html.is-glass.is-quick .ca-quick::before,html.is-glass.is-bar .ca-hud::before{content:'';position:absolute;inset:0;z-index:-1;border-radius:inherit;background:linear-gradient(135deg,color-mix(in srgb,var(--surface) 82%,rgb(120,150,60)),color-mix(in srgb,var(--surface) 82%,rgb(60,120,200)))}</style><script>"
if [ "$view" = bar ]; then
  size="${PREVIEW_SIZE:-1200,64}"
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
if [ "$view" = terminal ]; then
  size="${PREVIEW_SIZE:-720,460}"
  setup="$setup window.__CODEALONG_KEPT__=true; window.__CODEALONG_TERMINAL__=true;"
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
# The page's clock on another day (PREVIEW_DATE), before anything in it reads the time
# (scripts/preview/clock.js). The page is told the day, for a check to hold it to.
clock=""
if [ -n "${PREVIEW_DATE:-}" ]; then
  cp scripts/preview/clock.js "$dir/"
  clock="<script>window.__PREVIEW_DATE__='$PREVIEW_DATE';</script><script src=\"./clock.js\"></script>"
fi
sed -e "s#<head>#<head>$clock<script src=\"./fake-tauri.js\"></script><script>$setup</script>#" -e "s#</body>#$tail</body>#" "$dir/index.html" >"$dir/preview.html"

server=""
browser=""
work=""
# What only this run's server has to give: a file by a name of this run's own.
token="served-$$-$RANDOM$RANDOM"
# Chrome is more than the process started here: each run has ten or so helpers beside it
# (renderers, the GPU's, utilities). Ended alone, the browser leaves them to end by themselves,
# which on a Mac busy enough they aren't given the time to do: on 2026-11-20 over a thousand
# stood about from checks long over, with a load average near a thousand to match. Every one
# of them is a program of Chrome's own and carries the run's profile in its arguments, as the
# browser does, and the profile's folder is this run's own (mktemp), so no other run's Chrome
# has it, nor yours. `ours` prints the pids of the processes that are both: started from where
# Chrome is (its app, or the program itself when it isn't one), with that argument whole (not a
# longer path, not part of another argument). Something else that names the profile, to look
# for it say, isn't one. None, where the process table can't be asked (an agent's sandbox):
# there they're left as they were.
ours() {
  [ -n "$work" ] || return 0
  ps -Awwo pid=,command= 2>/dev/null | CHROME="${chrome%%/Contents/*}" PROFILE=" --user-data-dir=$work/profile" awk '
    {
      command = $0
      sub(/^[ \t]*[0-9]+[ \t]+/, "", command)
      at = index(command, ENVIRON["PROFILE"])
      after = substr(command, at + length(ENVIRON["PROFILE"]), 1)
    }
    index(command, ENVIRON["CHROME"]) == 1 && at && (after == "" || after == " ") { print $1 }' || true
}
# A Chrome that hung may not go when asked: it gets a few seconds, since its profile can't be
# removed from under it, and then it's made to. So do its helpers, after it. Tests can shorten
# the poll step (PREVIEW_POLL / PREVIEW_WAIT_POLL) without changing the retry counts.
poll="${PREVIEW_POLL:-0.2}"
wait_poll="${PREVIEW_WAIT_POLL:-1}"
cleanup() {
  # To the end, whatever comes: a second stop (gtimeout signals this script and its group
  # each) would leave the helpers and the profile behind.
  trap '' INT TERM HUP
  kill $server $browser 2>/dev/null || true
  local n=0 left
  if [ -n "$browser" ]; then
    while kill -0 "$browser" 2>/dev/null && [ $n -lt 20 ]; do
      sleep "$poll"
      n=$((n + 1))
    done
    kill -9 "$browser" 2>/dev/null || true
  fi
  left="$(ours)"
  if [ -n "$left" ]; then
    # shellcheck disable=SC2086
    kill $left 2>/dev/null || true
    n=0
    while [ -n "$(ours)" ] && [ $n -lt 10 ]; do
      sleep "$poll"
      n=$((n + 1))
    done
    # Listed again: only what's still there, and still Chrome with this profile.
    left="$(ours)"
    # shellcheck disable=SC2086
    [ -z "$left" ] || kill -9 $left 2>/dev/null || true
  fi
  rm -f "$dir/$token"
  [ -z "$work" ] || rm -rf "$work" 2>/dev/null || true
}
trap cleanup EXIT
# The page is served from $dir on a port: a free one unless one is given, found here and not
# before, since a prepare, and a folder or an engine refused, open no socket and so run where
# none may be opened (an agent's sandbox, where src/checks.test.ts runs them).
#
# Another server may answer on that port: one already on the port PREVIEW_PORT names, or one
# that took a free port between its being found and this run's server binding it (two runs of
# checks.sh --jobs at once). This run's server then fails to start, and the page opened would be
# the other's: this Mac's under the made-up Mac's name, or another check's with its result. So
# before anything is opened the port is asked for what only this run's server has, and one that
# answers otherwise isn't used: `serve <port>` is true once this run's own server answers there.
echo "$token" >"$dir/$token"
serve() {
  local n=0
  port="$1"
  (cd "$dir" && exec python3 -m http.server "$port" --bind 127.0.0.1 >/dev/null 2>&1) &
  server=$!
  # For as long as it's starting, which on a busy Mac is a while: half a minute at most.
  while [ $n -lt 300 ]; do
    [ "$(curl -s --max-time 2 "http://127.0.0.1:$port/$token" 2>/dev/null)" != "$token" ] || return 0
    # It couldn't have the port and is gone: whoever answers there isn't it.
    kill -0 "$server" 2>/dev/null || break
    sleep 0.1
    n=$((n + 1))
  done
  kill "$server" 2>/dev/null || true
  server=""
  return 1
}
if [ -n "${PREVIEW_PORT:-}" ]; then
  if ! serve "$PREVIEW_PORT"; then
    echo "Port $PREVIEW_PORT (PREVIEW_PORT) isn't this run's to serve its page on: its own server doesn't answer there, so another's page would be taken for this one. Leave PREVIEW_PORT out for a free port." >&2
    exit 1
  fi
else
  tries=0
  until serve "$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"; do
    tries=$((tries + 1))
    if [ $tries -ge 5 ]; then
      echo "No port for this run to serve its page on: its own server answered on none of five free ones (the last, $port), so another's page would be taken for this one." >&2
      exit 1
    fi
  done
fi
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
  # one so long) takes its browser, its helpers and its server with it. Not for a screenshot,
  # whose Chrome runs in front: bash would hold a stop until that Chrome returned.
  trap 'exit 130' INT TERM HUP
fi
# Chrome's profile is a folder of this run's, for a screenshot too: what its helpers are known
# by afterwards.
work="$(mktemp -d)"
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
  "$chrome" --headless=new --disable-features=MacAppCodeSignClone --disable-gpu --enable-unsafe-swiftshader ${extra_chrome[@]+"${extra_chrome[@]}"} ${scheme:+"$scheme"} --window-size="$width,$((height + 32))" --remote-debugging-port="$debug_port" --user-data-dir="$work/profile" --app="$url" >/dev/null 2>&1 &
  browser=$!
  title=""
  started=$SECONDS
  deadline=$((SECONDS + PREVIEW_WAIT / 1000))
  while [ $SECONDS -lt $deadline ]; do
    sleep "$wait_poll"
    # Half a minute with no page to ask about is a Chrome that hung as it started.
    if [ -z "$title" ] && [ $((SECONDS - started)) -gt 30 ]; then
      break
    fi
    title="$(curl -s --max-time 5 "http://127.0.0.1:$debug_port/json/list" | python3 -c 'import html, json, sys; print(html.unescape(next((t["title"] for t in json.load(sys.stdin) if t.get("type") == "page"), "")))' 2>/dev/null || true)"
    case "$title" in done:*) break ;; esac
  done
  report "$title" || exit $?
elif [ -n "$check" ]; then
  "$chrome" --headless=new --disable-features=MacAppCodeSignClone --disable-gpu --enable-unsafe-swiftshader ${extra_chrome[@]+"${extra_chrome[@]}"} ${scheme:+"$scheme"} --window-size="$width,$((height + 87))" --virtual-time-budget=8000 --user-data-dir="$work/profile" --dump-dom "$url" >"$work/dom.html" 2>/dev/null &
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
    sleep "$poll"
  done
  wait "$browser" 2>/dev/null || true
  # Waited for, so its number is no longer its own: nothing here signals it again.
  browser=""
  title="$(python3 -c 'import html, re, sys; m = re.search(r"<title>(.*?)</title>", sys.stdin.read(), re.S); print(" ".join(html.unescape(m.group(1)).split()) if m else "")' <"$work/dom.html")"
  report "$title" || exit $?
else
  "$chrome" --headless=new --disable-features=MacAppCodeSignClone --disable-gpu --enable-unsafe-swiftshader ${extra_chrome[@]+"${extra_chrome[@]}"} ${scheme:+"$scheme"} --hide-scrollbars --force-device-scale-factor=1 --window-size="$size" --virtual-time-budget=6000 --user-data-dir="$work/profile" --screenshot="$shot" "$url" 2>/dev/null
  echo "$shot"
fi

#!/usr/bin/env bash
# Runs in the VM, for `scripts/linux-vm.sh run`: the app built there (`build`), on a display of
# its own (Xvfb :99, with openbox to place windows) and with a home of its own (~/run-home, kept
# from run to run, so what you set up there stays), for `secs` seconds; then a screenshot of the
# display at ~/linux.png, and the app's output. Everything it started is stopped by its process
# group, never by a name a pattern could match elsewhere.
set -euo pipefail
secs="${1:-20}"
app="$CARGO_TARGET_DIR/release/codealong"
[ -x "$app" ] || { echo "No build yet: scripts/linux-vm.sh build" >&2; exit 1; }
export DISPLAY=:99
started=()
trap 'for g in "${started[@]}"; do kill -- "-$g" 2>/dev/null || true; done' EXIT
setsid Xvfb :99 -screen 0 1600x1000x24 >/dev/null 2>&1 &
started+=($!)
sleep 1
setsid openbox >/dev/null 2>&1 &
started+=($!)
# A compositor, which draws the windows that float with their clear parts clear.
setsid xcompmgr >/dev/null 2>&1 &
started+=($!)
mkdir -p ~/run-home
HOME=~/run-home setsid dbus-run-session -- "$app" >~/run.log 2>&1 &
started+=($!)
sleep "$secs"
import -window root ~/linux.png
tail -40 ~/run.log

#!/usr/bin/env bash
# Renders the UI in headless Chrome against a snapshot of your real data, with Tauri's IPC
# replaced by scripts/preview/fake-tauri.js (which records every command the UI sends).
#
#   scripts/preview.sh [view] [project] [check.js]
#
#   view     worktrees | sessions | prs, or bar for the floating bar (default worktrees)
#   project  the project to show (default: the first one)
#   check.js optional script run in the page; whatever it puts in document.title is printed
#            (use it to click things and read window.__calls). Without it, a screenshot is
#            written to $PREVIEW_DIR/<view>.png.
#
# PREVIEW_SCHEME=dark renders as if macOS were in dark mode (the screenshot is <view>-dark.png).
# Under theme = "system" the page follows it; "light" and "dark" in config.toml win over it.
# The bar renders on a stand-in desktop at $PREVIEW_SIZE (default 1100,64, which is also the
# screen width it fits itself to).
#
# Needs a release build of the engine (cargo build --release in src-tauri) for the snapshot.
# PR details come from $PREVIEW_DIR/pr-detail.json if you put one there:
#   src-tauri/target/release/codealong --snapshot --pr owner/name 123 > .preview/pr-detail.json
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
shot="$dir/$view${PREVIEW_SCHEME:+-$PREVIEW_SCHEME}.png"
bin="src-tauri/target/release/codealong"

npm run build >/dev/null
mkdir -p "$dir"
rm -rf "$dir/assets"
cp -R dist/. "$dir/"
cp scripts/preview/fake-tauri.js "$dir/"
"$bin" --snapshot >"$dir/board.json" 2>/dev/null
"$bin" --snapshot --accounts >"$dir/accounts.json" 2>/dev/null
"$bin" --snapshot --start-options >"$dir/start-options.json" 2>/dev/null

setup="localStorage.setItem('codealong.view','$view');localStorage.setItem('codealong.age','all');"
[ -n "$project" ] && setup="$setup localStorage.setItem('codealong.project','$project');"
size="1440,900"
if [ "$view" = bar ]; then
  size="${PREVIEW_SIZE:-1100,64}"
  # Headless Chrome's screen is 800 wide whatever the window; the bar fits itself to the screen.
  setup="$setup window.__CODEALONG_BAR__=true;Object.defineProperty(screen,'availWidth',{value:${size%%,*}});</script><style>html.is-bar{background:linear-gradient(135deg,rgb(93,111,134),rgb(169,182,196))!important}</style><script>"
fi
tail=""
if [ -n "$check" ]; then
  cp "$check" "$dir/check.js"
  tail='<script src="./check.js"></script>'
fi
sed -e "s#<head>#<head><script src=\"./fake-tauri.js\"></script><script>$setup</script>#" -e "s#</body>#$tail</body>#" "$dir/index.html" >"$dir/preview.html"

(cd "$dir" && exec python3 -m http.server "$port" --bind 127.0.0.1 >/dev/null 2>&1) &
server=$!
trap 'kill $server 2>/dev/null || true' EXIT
sleep 1
url="http://127.0.0.1:$port/preview.html"
if [ -n "$check" ]; then
  "$chrome" --headless=new --disable-gpu ${scheme:+"$scheme"} --window-size="$size" --virtual-time-budget=8000 --dump-dom "$url" 2>/dev/null | sed -n 's#.*<title>\(.*\)</title>.*#\1#p'
else
  "$chrome" --headless=new --disable-gpu ${scheme:+"$scheme"} --hide-scrollbars --force-device-scale-factor=1 --window-size="$size" --virtual-time-budget=6000 --screenshot="$shot" "$url" 2>/dev/null
  echo "$shot"
fi

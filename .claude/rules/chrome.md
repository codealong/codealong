---
paths:
  - "src-tauri/src/chrome.rs"
  - "src-tauri/src/chrometabs.rs"
  - "src-tauri/src/previews.rs"
---

# Chrome

What Google Chrome and the Claude in Chrome extension keep and answer, as chrome.rs and
chrometabs.rs read them, moved from CLAUDE.md's Harness facts: Chrome 154 (Chromium's source and
this Mac, 2026-09-29) and Claude in Chrome 1.0.97, `fcoeoabgfenejglbffodgkkbkcdhcgfn`
(2026-09-30). Paths are under `src-tauri/src/`.

## Google Chrome

- Profiles in `Local State`, and opening a page in one with `open -n -b com.google.Chrome --args
  --profile-directory=<folder> <url>`: `chrome.rs` (module header). Not said there: Chrome
  rewrites `Local State` while it runs, not at once, so a profile made a moment ago can be
  missing from it.
- A tab's AppleScript `id` is the id extensions get for it, unique across all profiles'
  windows, and windows are held by id: `chrometabs.rs` (module header, `SHOW_TAB`). Not said
  there: that comes from Chromium's `tab_applescript.mm` and `extension_tab_util.cc`, and one
  Chrome process runs every profile's windows, which one `every window` lists. By
  `window_applescript.mm`, `set active tab index of w to i` activates the tab through the tab
  strip and `set index of w to 1` orders the window front. Windows have no profile property.
- A profile's History (`visit_time` in microseconds since 1601, read as immutable while Chrome
  holds it locked): `chrometabs.rs` (`by_history`, `open_history`).

## Claude in Chrome

- Chrome starts the native host (`Claude.app/Contents/Helpers/chrome-native-host`, or Claude
  Code's `~/.claude/chrome/chrome-native-host`) once per profile with the extension; each
  listens on `/tmp/claude-mcp-browser-bridge-$USER/<pid>.sock`, and nothing in its arguments
  names the profile.
- The "Tab Context" block that ends each result of its tools (`mcp__claude-in-chrome__*`), and
  `tabs_context_mcp`'s JSON: `chrometabs.rs` (module header, `result`). Not said there: a call
  that closed the group's last tab says "Group is now empty (auto-removed)", other tools'
  results can carry the JSON as "Tab context (from front-loaded tabs_context_mcp)", and a group
  closed under a session answers "This session's tab group no longer exists".
- `list_connected_browsers` answers `[{deviceId, name, osPlatform, connectedAt, isLocal,
  inUse}]`: a device is one profile's extension, and names like "Browser 2" change with the
  list. How `select_browser {deviceId}` and `switch_browser` answer: `chrometabs.rs` (`result`).
- The extension's storage (LevelDB: `bridgeDeviceId`, `tabGroups`, how soon a gone group's
  record is dropped): `chrometabs.rs` (module header). Not said there: it keeps `displayName`
  too, and rewrites `tabGroups` whole on each change.

## Unchecked

- Chrome, 2026-09-29: `set active tab index` and `set index of w to 1` (in `SHOW_TAB`) not yet
  watched on a window in use.

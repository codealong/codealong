---
paths:
  - "src-tauri/src/launch/rex.rs"
  - "src-tauri/src/launch/stage.rs"
---

# Rex

What Rex's server and app take from a script, and how Codealong starts, shows, types into and
closes a session there, moved from `terminals.md`, which has the other terminals and what they
all share; read it before changing `launch/rex.rs`. Paths are under `src-tauri/src/`.

Rex Beta 0.1.0 (1497), its `rex` 0.0.1793047680 at API revision 7, on macOS 26.6 (2026-10-26).
Its docs are markdown at `https://www.superlogical.com/rex/docs.md` (every page in one file at
`/rex/docs/llms-full.txt`), and `rex help <command>`, `rex api ls` and `rex api describe <method>`
say the rest.

- `/Applications/Rex Beta.app`, bundle id `com.superlogical.rex`, no AppleScript dictionary; its
  only URL route found is `rex://settings/server`. A server of its own (`rex server run`, named
  `rex` in the kernel, its parent launchd) runs every terminal and outlives the app, which is a
  client of it like `rex` is. `rex` is the server's own copy under `~/Library/Application
  Support/com.superlogical.rex/RexServer/Versions/Current/Rex Server.app/Contents/MacOS/`, and the
  app holds one at `Contents/Helpers/Rex Server.rex/Contents/MacOS/rex`: `launch/rex.rs` (`cli`).
  `--autostart=false` keeps it from starting a server (exit 4, "rex server is not running").
- Sessions hold windows (the app's tabs), windows a layout of blocks, each block a terminal on a
  TTY of its own under the server: rex → login → zsh, `login` there even for a block whose
  program is a command (`shell = "none"`). Ids are `session:<uuid>`, `window:…`, `block:…`; each
  terminal has `REX_SESSION`, `REX_BLOCK`, `REX_SERVER` and `REX_BIN_DIR` set.
- `rex do -e <lua> --args <json> --json` runs a script against the server and prints what it
  returned (an empty table as `{}`); `error(msg, 0)` exits 1 with `Error: msg`. A script hears a
  session only after `rex.session.attach`, else "stream is not attached" (409). What Codealong
  asks, each in one call: `launch/rex.rs` (`LIST`, `NEW`, `SPLIT`, `SHOW`, `WRITE`, `CLOSE`). Not said
  there: the terminal's `pwd` method is a `file://` URL when the shell reported OSC 7 and absent
  until it does, so the folder is `process`'s `foreground.cwd`.
- A terminal block takes `cwd`, `command`, `shell`, `initial_input` (typed once its program
  starts, as given) and `exit`, but no environment, so a start with one runs the launcher as its
  `command`. `session.create` with `initial_windows` doesn't move the app to the new session, and
  `new_window` with `focus = false` leaves the session's active window alone.
- The terminal's `write` method types raw bytes as if typed (nothing bracketed), so a send is a
  bracketed paste, then Return a moment later, as iTerm2's. `block.close` ends what runs there
  without asking; closing a window's last block closes the window.
- The app shows one session at a time, and the server doesn't say which: a client's
  `session_ids` are every session it's attached to. The app notes it in its own settings,
  `Workspace.lastFocusedSession` in `com.superlogical.rex`, `{"sessionID": …}` held as data, which
  `defaults export com.superlogical.rex -` prints in full (`defaults read` cuts data short); it
  followed a switch made by `session.select` (2026-10-27): `launch/rex.rs` (`shown_session`).
- `client.perform` of `session.select` (`session_id`, `window_id`) moves the app, refused (403,
  "Rex.app is not accepting actions. Turn on Remote Control in its Settings under Rex Server.")
  until Remote Control is on; `session.focus_block` on the server needs nothing. Remote Control is
  the app's preference `rex.client.remoteControl` (a bool, absent when off), which the running app
  takes at once when written with `defaults write`. With it on, `session.select` of a session
  made a moment before answered 200 and the app took the new block (the server's log, `rex server
  logs --all`, has each request), but Rex stayed behind the app that was clicked in: `open -b`,
  run by Codealong's engine from a click in its main window, didn't bring it forward, so the
  windows bring the terminal's app forward themselves (`lib.rs` `open_target`, `start_session`).
- The app can run with every window closed, and then refuses every action, `session.select` and
  its own `client.window.new` alike (400, "Rex Beta has no window to act in.", `code=not_supported`;
  0.1.0 (1541), 2026-10-28). `open -b` makes it open a window, on the session it showed last,
  within half a second, and `session.select` then works: `launch/rex.rs` (`show`). Before that,
  Attach in Rex opened its block and brought Rex forward on another session.
- App Nap: behind other apps' windows macOS naps the Rex app as it does Ghostty (every thread at
  priority 4 within about 10 s of being covered), and a napped app on a busy Mac answered
  `session.select` in 6 to 8 s, or not within `client.perform`'s default `timeout_ms` of 10 s
  (load near 300, 2026-11-03). The server and its terminals don't nap, so only what the app
  answers is slow. A client answers its requests one at a time in the order they came, and a
  request's `timeout_ms` starts only once it's at the front of that line (Rex's API reference).
  Past it, the call fails with `code=timeout` (504), the app isn't told, and it still carried
  the request out once it got to it, its answer logged as a `client.reply` with 404. An `open
  -g -b` doesn't wake it; coming forward does. So Codealong waits a second for the app, lets it
  go there by itself as the window brings Rex forward, and asks again once Rex is the active
  app, for a session its list didn't have yet: `launch/rex.rs` (`ANSWER_WAIT`, `show`). Waiting
  the 10 s, Attach in Rex looked like it did nothing, and a second click opened a second tab
  attached to the same job. Settings › General › Keep Rex awake stops the naps once the app
  restarts (`os/naps.rs`).
- The server makes a block's pty with no display to draw in, so it should start one while the
  screen is locked; unchecked with the screen locked. Starts go to Rex then rather than tmux
  (`Terminal::holds_sessions`, `launch.rs` `host_rule`).
- Each of the app's windows shows one session, and two can show the same one. Checked on 0.1.0
  (1541), 2026-10-29: `client.window.new` (⌘N) makes a session for its new window, named as Rex
  names one (two words, a shell in `~`), and its request timed out at 10 s though the window
  opened. `session.select` acts on the app's current window, the one last in front while Rex was
  the active app: with Rex behind, a window opened since didn't take it, and it never went to the
  window already showing the session. So a window per session can't be reached through Rex;
  raising one takes Accessibility (its title ends ` - <session label>`). Right after a busy spell
  (5 s answers), `session.select` of a session made a minute before failed with
  `SessionNotFoundOnServerError` (500) until `session.refresh`; on 2026-10-27 one made a moment
  before worked at once.
- Keys, by its menus read through Accessibility (the docs list none, and `rex keymap` shows only
  the config file's): ⌘K Change Session…, ⌃⇥ and ⌃⇧⇥ the tabs, ⌘N a window, ⇧⌘N a session;
  Next Session and Previous Session have none. With no config file, ⌘` goes through the app's
  windows as macOS does it. `rex.bind("cmd+[Backquote]", "session.next")` in
  `~/.config/rex/init.lua`, then `rex config reload`, takes it from macOS (a press posted to the
  HID stream stepped the sessions in the front window and left the windows as they were), and
  `shift+cmd+[Backquote]` with `session.previous` goes back. Next Session follows `session.list`'s
  order, which `session.move` (`before_session_id`) sets once the script is attached to the
  session moved (else 409).
- `session.new_split` with a block as `anchor_id` puts a new terminal beside it in the same tab
  (`direction` `vertical` is one above the other), from the server, with Rex behind.
- `cargo test rex_round_trip -- --ignored` opens a terminal in a session of its own behind your
  windows, checks its environment and typing, presses a key and closes it;
  `rex_notes_the_session_it_shows` checks the app's note names one of the server's sessions.

## Unchecked

- Rex, 2026-11-03: asking a napped Rex again once it's in front (`launch/rex.rs` `show`,
  `later`). The stage plays the ask again on the test's thread, every app being the active one
  there, and a newer show dropping it; not the real thread waiting for Rex to come forward, nor
  one ask waiting its turn behind another (`ask`'s lock). On this Mac's Rex a 1 ms wait answered
  late and a 1 s one showed the session; the order of a late select, the window bringing Rex
  forward, then the ask again, wasn't seen. The turns and the newest show are each process's
  own, so the server's ask again and the app's show don't wait for each other (`SHOWS` says
  when that matters).

## Elsewhere

- Reading what a block shows (`SCREEN`, Rex's `format` and `blob_read`) is in `terminals.md`,
  under Reading what a tab printed.

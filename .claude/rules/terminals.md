---
paths:
  - "src-tauri/src/launch.rs"
  - "src-tauri/src/launch/**"
  - "src-tauri/src/send.rs"
  - "src-tauri/src/runcode.rs"
  - "src-tauri/src/front.rs"
---

# Terminals

What Ghostty, iTerm2, Terminal and Rex take from a script, and how Codealong starts a session in
them, moved from CLAUDE.md's Harness facts: Ghostty 1.3 (its AppleScript dictionary and 1.3.1's
source), iTerm2 3.7.3 and Terminal 2.15 on macOS 26.6 (their dictionaries and a program
recording its input, 2026-09-29), and launch.rs's starts in iTerm2 and Terminal (checked
2026-09-29). Paths are under `src-tauri/src/`.

## Ghostty

- Windows, tabs and terminals with `id`, `name`, `working directory`; `focus`, `select tab`,
  `activate window`; `new window with configuration {initial working directory, initial input,
  environment variables}`, the last a list of `KEY=VALUE` set in the window's shell. `initial
  input` is written to the new terminal's pty as it starts, before the shell reads it, byte for
  byte (it goes through a Zig string literal, so control characters pass): each newline is a
  Return to the shell, which reads it a line at a time (1.3.1's `embedded.zig`).
- Typing into a session (`input text` pastes, bracketed when the program asked for that, then
  `send key "enter"`), only into a tab found by its mark, never by title: `launch/ghostty.rs`
  (`GHOSTTY_TYPE`, `terminal_of`). Not said there: to a program that didn't ask for bracketed
  pastes, each newline of `input text` is a Return, and neither needs the terminal in front.
- Finding and focusing a session's tab (an OSC 7 mark on its TTY, the 8 s wait, titles only as
  a fallback, focus requests taking turns and a stale one dropped): `launch.rs`
  (`terminal_focus_session`, `Turns`, `kind_by`), `launch/ghostty.rs` (`focus_session`,
  `GHOSTTY_MARK_WAIT_SECS`). iTerm2 and Terminal find the tab by its TTY instead. A tmux
  session's tab is its client's, the `tmux attach` process (named `tmux` too, so its terminal is
  the one above it), found by the client's TTY those same ways; a tmux session no client is
  attached to (`session_attached` 0, as `tmux new-session -d` leaves it) has no tab, and Show in
  opens a new one attaching to it: `launch/tmux.rs` (`focus_session`, `attach_command`). Not
  said there: `list-panes -a` lists a window once per session it's in (linked, or a group made
  with `new-session -t`), each line with that session's `session_attached`; `attach -t %<pane>`
  shows that pane's window and selects the pane, where `attach -t <session>` shows the window
  the session was on; a pane alone as a target (`select-window -t %3`) names the session with it
  that was active last, attached or not, so a window is selected as `=<session>:@<window>` in the
  session the client shows (tmux 3.7c, 2026-10-13). Only terminals Codealong doesn't script, and
  a tmux client in one, go by title.
- Holding windows, tabs and terminals by id, one question for an id with its own folder, and
  `current date` outside the tell: `launch/ghostty.rs` (`GHOSTTY_FOCUS`, `GHOSTTY_LIST`). Not
  said there: store a query's result before taking an item: `item 1 of (id of every terminal
  whose …)` is sent to Ghostty whole and fails.
- macOS doesn't let a background process bring an app forward: `activate` from a shell leaves
  Ghostty behind; from a click in Codealong it works.
- A new window or tab asks for Ghostty to be the active app whatever the script does after, so
  a Background start leaves out only the script's own asks: `launch/ghostty.rs`
  (`GHOSTTY_NEW`), `launch.rs` (`Placement`). `new tab` also makes the new tab the selected one.
- A tab on another desktop (a Space), and why a focus raises it again once its app is active:
  `launch.rs` (`follow`). `select tab` is `makeKeyAndOrderFront` too; it and `activate window`
  act at once, and `focus` does the same on Ghostty's next turn (1.3.1's ScriptTab.swift,
  ScriptWindow.swift, BaseTerminalController.swift). Nothing public says which desktop a window
  is on, or goes to one. Found, and not used: the window server's private
  `CGSCopySpacesForWindows` (a tab that isn't selected is a window on no desktop) and
  `_SLPSSetFrontProcessWithOptions` with a window's number (how AltTab focuses a window on
  another desktop); Ghostty's script ids are object addresses, not window numbers, and a
  window's number through Accessibility needs that permission, asked again for every build of
  Codealong Dev.
- Pressing a key (`send key` gets only keys with codes of their own through, so a digit goes as
  `perform action "text:…"`, a Zig string literal): `launch/ghostty.rs` (`press`,
  `text_action`). Not said there: `send key "<name>"` takes the Swift names of Ghostty's keys
  (`digit1`, `escape`, `enter`; `digit_1` and `1` error with "Unknown key name"). Checked on
  1.3.1 (2026-09-27); `cargo test ghostty_presses_keys -- --ignored` repeats it.
- While the screen is locked Ghostty doesn't start new terminals: `new window` answers a window
  whose terminal never started (`ghostty_surface_new` returned NULL; this Mac's log has
  `[embedded_window] error initializing surface err=error.OutOfMemory` at 2026-10-12 12:55:44,
  during a lock, the error Ghostty's macOS bindings give for a NULL from a CoreText, CoreGraphics or
  IOSurface call), and `input text` into it says "Terminal surface model is not available". So
  starts go to tmux then (below). By 1.3.1's source, `input text` and `send key` into a terminal
  made before the lock reach its pty without the display; not checked live.
- A terminal a script closed (`close terminal`) stays in `every terminal`, its window in
  `windows`, ordered out with nothing drawn, and `exists terminal id` says true; `close` on it
  again fails with "Terminal is not in a window". One Ghostty closed itself, its shell having
  exited, leaves the lists (1.3.1, 2026-10-22). So whether a tab closed is told by its shell
  having ended: `stale.rs` (`ghostty_closes_a_stale_tab`).
- Where a new tab goes and its name (`window-new-tab-position`, `move_tab`, `set_tab_title`):
  `launch/ghostty.rs` (`GHOSTTY_NEW`), the name from `board::tab_label`. Not said there: `new
  tab in window id <w>` answers the tab, whose `id` and `index` can be read and which `tab id
  <id> of window id <w>` finds again; `move_tab:<n>` moves it `n` places (negative is back,
  wrapping at the ends); `set_tab_title:` with nothing clears the name, its docs say (`ghostty
  +list-actions --docs`). Checked on 1.3.1 (2026-10-05); `cargo test
  ghostty_groups_and_names_tabs -- --ignored` repeats it.
- App Nap: behind other apps' windows macOS naps Ghostty, every thread at priority 4, and a
  napped Ghostty on a busy Mac took 21 s over one question; marks are timed from its first
  answer, and what a send says when Ghostty was slow: `launch/ghostty.rs` (`naps`,
  `GHOSTTY_MARK_WAIT_SECS`, `untyped`). Not said there: `ps -M -p <pid>` shows the priorities,
  and `log show --predicate 'process == "runningboardd" AND eventMessage CONTAINS "Set AppNap
  state"'` when a nap began and ended (`active:Y`, `active:N`). An Apple Event doesn't wake a
  napped app at once: it's taken in at the napped priority, and the nap ends after.
- Why a script gets a minute (Ghostty slow to answer, macOS asking again after each rebuild,
  Ghostty carrying out an event after `osascript` is killed): `launch.rs` (`OSASCRIPT_TIMEOUT`).
  It asks again when the bundle is ad-hoc signed, with a per-build identifier
  (`codealong-<hash>`); `relaunch.sh` signs with an Apple Development certificate when the Mac
  has one, and macOS keeps the answer across those builds.

## iTerm2

- `/Applications/iTerm.app`, bundle id `com.googlecode.iterm2`, scripted as `application
  "iTerm"`. Windows (front to back, `id`), tabs (no id; `select`), sessions (`id` a UUID, `tty`,
  `name`, `variable named "session.path"` its folder, `"session.jobPid"`). A tab's shell runs
  under a server that carries on when iTerm2 restarts: `launch/iterm.rs` (`runs_as`).
- `tty of every session of every tab of window id <w>` answers in one event (lists of lists);
  `whose` on nested elements doesn't. Why lists are walked by index: `launch/iterm.rs`
  (`SESSION_ON`).
- A new window or tab runs the launcher in place of the profile's shell, since a script can't
  give it a folder or environment: `launch/iterm.rs` (`open`). Not said there: `create window
  with default profile command <cmd>` and `create tab …` (in a `tell window id`) put no `login`
  between iTerm2 and the command, and return at once with the session's `tty` known.
- `write text` (exactly what it's given, then `\r` unless `newline no`; nothing is bracketed, so
  Codealong wraps a paste, and `1` or Escape arrive as typed keys): `launch/iterm.rs` (`TYPE`,
  `press`).
- `select` and `close` (a script closes a session without asking, even with a job running):
  `launch/iterm.rs` (`FOCUS`, `CLOSE`).
- A closed window stays in `windows`, invisible, for good (`visible` false, not minimized):
  scripts take `every window whose visible is true or miniaturized is true`, and a new tab can
  get a closed one's TTY.

## Terminal

- `/System/Applications/Utilities/Terminal.app`, `com.apple.Terminal`. Tabs have `tty`,
  `processes` (names), `busy`, `selected`, `custom title`; no folder. A tab's shell runs under
  `login` → `Terminal`.
- `do script` (a new window, or typing into a tab, always with `\r` after and nothing bracketed,
  so no single keypress and no answering a permission prompt there): `launch/terminal_app.rs`
  (module header, `NEW`, `type_text`), `launch.rs` (`Terminal::presses_keys`). Not said there:
  `\n` inside the text stays `\n`.
- No tabs from a script (`make new tab` fails, a tab doesn't take `close`), so new terminals are
  windows and a tab that shares its window is closed by typing `exit`: `launch/terminal_app.rs`
  (`run`, `CLOSE`).
- Focus: `launch/terminal_app.rs` (`FOCUS`).
- Closed windows and tabs whose shell exited stay listed, so only tabs with processes in windows
  you can see count: `launch/terminal_app.rs` (`TAB_ON`). The profile's setting is
  `shellExitAction = 2` ("When the shell exits: Don't close the window"). In AppleScript
  `contents of <tab>` is the tab itself (the keyword wins); `history of <tab>` is its text.

## Rex

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
  asks, each in one call: `launch/rex.rs` (`LIST`, `NEW`, `SHOW`, `WRITE`, `CLOSE`). Not said
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
- The server makes a block's pty with no display to draw in, so it should start one while the
  screen is locked; unchecked with the screen locked. Starts go to Rex then rather than tmux
  (`Terminal::holds_sessions`, `launch.rs` `host_rule`).
- `cargo test rex_round_trip -- --ignored` opens a terminal in a session of its own behind your
  windows, checks its environment and typing, presses a key and closes it;
  `rex_notes_the_session_it_shows` checks the app's note names one of the server's sessions.

## Codealong's tmux server

- When a start runs in tmux (`[apps] tmux`, the screen locked, or a Background start out of
  sight with no tab at all), and how a tab shows it and ends it: `launch.rs` (`host_for`,
  `out_of_sight_in_tmux`, `session_run`, `TabLooks`, `show_unlocked`), `launch/tmux.rs`
  (module header, `hosted`, `CONFIG`, `start`, `attach_line`, `WAITS_FOR_TAB`). A session option
  (`@codealong-tab`) marks what waits for a tab, read back with `#{@codealong-tab}` in `list-panes`
  and cleared with `set-option -u`. The lock is the window server's
  `CGSSessionScreenIsLocked`, there only while locked: `os/system/macos.rs` (`screen_locked`); it
  read true with the screen locked and the display asleep (2026-10-18), and `ioreg -n Root -d1 -a`
  shows it too. iTerm2's and Terminal's new windows while locked are unchecked, and taken to fail
  as Ghostty's do.
- Not said there (tmux 3.7c, 2026-10-18): `destroy-unattached on` set on a session no client has
  attached to ends it at once (its server exited with it), so it's set as a tab attaches; a client
  hung up after that ends the session, and a plain attach's leaves it running.
  `set-option -t '=<name>'` refuses ("no such session") where `display-message -t '=<name>:'`
  takes it, so sessions are held by the ids `new-session -P -F '#{session_id} #{pane_id}'` prints.
  `TMUX` is `<socket>,<server's pid>,<session>`. `default-size` sizes a session started detached,
  and `default-terminal` is `tmux-256color`, which macOS 26's terminfo has.
- macOS shows no other process the environment of its own programs: a pane's `/bin/zsh` read
  empty through `KERN_PROCARGS2`, where a `claude`'s reads, so a pane whose `TMUX` can't be read is
  looked for in Codealong's server and then yours (`launch/tmux.rs` `Server::of`, `pane_of`).
- `paste-buffer` without `-r` turns each newline into a Return, as terminals do with a paste; a
  program that asked for bracketed pastes gets the text whole (left so).
- `cargo test a_session_runs_in_codealongs_own_tmux_server -- --ignored` starts a real session in a
  scratch server of its own, types into it, attaches and hangs up a client, with nothing on screen.

## Starting a session in iTerm2 or Terminal

- The launcher (a `/bin/sh` script in `$TMPDIR`, mode 600, that deletes itself, sets the folder
  and environment and `exec`s the login shell), and the leading space on the line Terminal
  types to run it: `launch.rs` (`launcher`), `launch/terminal_app.rs` (`open`).
- Typing the next line only once the shell reads a key at a time, each line led by Ctrl-U:
  `launch.rs` (`wait_for_shell`, `fresh_line`).
- Run on a code block runs a block's commands without their comments, since zsh takes `#` as a
  comment only with `interactive_comments` set: `runcode.rs` (module header). bash's interactive
  shells take comments. They run in a tab of their own as `codealong terminal run`'s do (below).

## Reading what a tab printed

- Ghostty 1.3.1 gives a script no way to read a terminal's text (its dictionary has none; its
  `write_screen_file` and `write_scrollback_file` actions go through the clipboard or a file it
  opens). iTerm2's session `contents` and `text` are its visible screen only; Terminal's tab
  `history` is its whole scrollback (`contents` is the tab itself). Checked 2026-10-12.
- So a command an agent runs in your terminal (`codealong terminal run`) runs under
  `/usr/bin/script -q -F <file>` in its tab, which keeps what it prints in any terminal:
  `terminal.rs` (module header, `wrapper`). macOS's `script` exits with the command's status, and
  `-F` writes each output as it comes. Without `-k` the file has no input, so a password typed at
  a prompt that doesn't echo it isn't kept (checked in tmux with `read -s`, 2026-10-12).
- Ctrl-C reaches the process group in the foreground of the tab's terminal, which the kernel's
  process table has (`e_tpgid`): `procs.rs` (`foreground_group`). The command's shell runs with
  `-l -i`, as a new tab's does, so job control puts a foreground command in a group of its own;
  zsh ends with 130 when it's interrupted.
- Nothing outside a shell can tell its prompt from a `read` of yours: the terminal's canonical
  input is off at zsh's prompt, but also in zsh's `read -k` and bash's `read -e` (Codex checked
  both on new ptys, 2026-10-12), and neither starts a process. Nor whether you typed there: a
  terminal device's access time moves when what was typed is read and not when something is
  printed (`w`'s idle time), but also as zsh sets the terminal up again when a command it ran
  ends, with nobody typing (zsh in tmux, 2026-10-12). So the line typed into a run's tab `exec`s
  the wrapper, which is the tab's shell until you take one there with Return, and `terminal stop
  --close` hangs up only a tab that's still the run's, each claiming the tab with one `mkdir` so
  a close and your Return at once never both have it: `terminal.rs` (`TYPED`, `wrapper`,
  `close`, `CLAIM`). The terminal closes a tab whose shell ended as its settings say (Ghostty does;
  Terminal's `shellExitAction` above may keep it). A run's tab closes itself once
  `[apps] close_command_tabs_after` is up, its wrapper claiming it as `close` does and hanging
  itself up, unless you took a shell there first: `terminal.rs` (`wrapper`). It sleeps rather than
  `read -t`, which macOS's `/bin/sh` has as bash 3.2 but not as dash, which
  `/private/var/select/sh` may name. Run on a code block runs this way too (`runcode.rs`), so a
  block's commands don't land in your shell's history.
- A napped Ghostty opened a run's tab and ran the command, then didn't answer within the minute
  (2026-10-15): the run is started once its wrapper has noted itself, whatever the script said,
  and keeps its folder, which the wrapper needs to note the end and to close the tab: `terminal.rs`
  (`start`). With the folder removed, that tab waited for Return for good.
- A redraw touches every tab's terminal device at once: the lid opened at 09:10:00 (`pmset -g
  log`), and twenty Ghostty tabs at their prompts had both their device's access and modification
  times moved between 09:10:05 and 09:10:07 (2026-10-27), as each window was resized and its shell
  redrew its prompt and read again. A device keeps only its last time, so what a tab was used for
  before is gone from it; Close stale terminals keeps it itself (stale.rs `Uses`).
- macOS's `/bin/sh` runs as the shell `/private/var/select/sh` names (`bash` here), and that's the
  name the kernel gives the process (`ps -o ucomm`), not `sh`.
- An agent's sandbox may keep its command from looking at your processes or signalling them (a
  Codex sandbox refused `ps`), so the command asks the app for a run's state, `stop` and
  `--close` when it's up (server.rs `TerminalRuns`, `TerminalStop`), and without the app a
  process it may not look at counts as there, not gone (`Noted::Unknown`). The app takes from its
  socket only ids that name a run's own folder (`run_folder`), so a sandboxed agent can't have it
  signal from files of its own making.
- `cargo test a_command_runs_in_a_tab_under_script -- --ignored` runs a command in a tmux session
  nobody sees, with your login shell: kept output, Ctrl-C, and closing a tab or not.

## Testing

- `cargo test ghostty_hands_over -- --ignored` opens a real Ghostty window that echoes a tricky
  prompt back through its environment, then closes. `cargo test tty_apps -- --ignored
  --test-threads=1` does the same in iTerm2 and Terminal, and types, presses keys, focuses and
  closes there, on windows of its own; don't type on the Mac while it runs. `cargo test
  ghostty_closes_a_stale_tab -- --ignored` opens two Ghostty windows and closes the one behind
  as Close stale terminals does (stale.rs), hanging up the `sleep` in front of its shell.

## Unchecked

- tmux, 2026-10-18: a start while locked going to tmux and its Ghostty tab opening once unlocked,
  seen end to end (each step was, apart); and which app macOS's privacy prompts name for an agent
  under Codealong's tmux server, which Codealong starts (Codealong, as for a background agent whose
  daemon it started, would be the guess), and so whether access granted to the terminal app
  (Full Disk Access, say) reaches it.

- Ghostty, 2026-10-03: whether macOS lets Ghostty's own ask to be the active app through while
  another app is active, which would bring it forward on a Background start anyway.
- Ghostty, 2026-10-01: that `launch::follow`'s second focus takes you to a tab on another
  desktop (every Ghostty window was on one desktop, and the check moves your screen); with
  Mission Control's "When switching to an application, switch to a Space with open windows for
  the application" off, macOS may stay where you are.

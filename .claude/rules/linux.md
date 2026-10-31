---
paths:
  - "src-tauri/src/**/linux.rs"
  - "src-tauri/src/os/**"
  - "scripts/linux-vm.sh"
  - "scripts/linux/**"
---

# Linux

What Linux, its desktops and the harnesses there turned out to need, found in the VM
`scripts/linux-vm.sh` makes (Ubuntu 24.04 arm64, WebKitGTK 2.52, tmux 3.4, Claude Code 2.1.301,
2026-10-16). Paths are under `src-tauri/src/`.

- tmux renames its processes there: the kernel names the server `tmux: server` and a client
  `tmux: client` (macOS keeps `tmux`), so a session under tmux is found by that name
  (`launch/tmux.rs` `runs_as`). A process's name is cut to 15 characters: GNOME Terminal's server
  is `gnome-terminal-` (`launch/linux.rs` `EMULATORS`).
- Right after `spawn` returns, `/proc/<pid>/environ` can read empty or the starter's for a moment,
  and its `stat` name be the starting thread's: the kernel says where the new program's
  environment is, and names it, only after the close-on-exec pipe that `spawn` waits on has closed (`procs.rs` test
  `reads_a_running_process_environment…`, `launch.rs` test `reads_a_process_line…`; a GitHub
  runner, 2026-10-16).
- util-linux's `script` (2.39) takes the command as one line (`-c`), runs it with `$SHELL -c`,
  writes each output as it comes only with `-f` and exits with the command's status only with
  `-e`; it has no `-F` and takes no command words (`terminal.rs` `under_script`). bash 5 ends a
  `bash -i -c` with 1 when its foreground job gets Ctrl-C, with or without `script`; zsh and
  macOS's bash 3.2 end with 130. `cargo test a_command_runs_in_a_tab_under_script -- --ignored`
  passed in the VM with bash as the login shell (2026-10-16).
- One emulator process can have every window: GNOME Terminal's server does, and `xdotool search
  --pid` lists them all, so a window is raised only when it's the process's one
  (`launch/linux.rs` `raise`).
- A tmux session's pane typed into from a shell inside another tmux session would nest tmux, which
  it refuses unless `$TMUX` is unset, so a terminal opened only to attach to one is a window
  attached to it directly (`launch/linux.rs` `run`).
- Claude Code there keeps its sign-in in `<config dir>/.credentials.json` (the same JSON the
  Mac's keychain item holds, `claudeAiOauth`), and writes `sessions/<pid>.<hash>.key` as it
  starts, before its `<pid>.json`; an unsigned one stops at the theme and login screens and
  writes no `<pid>.json` (`claude/usage.rs`).
- WebKitGTK's user agent says `X11; Linux`, which is how the page knows it's there
  (`src/platform.ts` `platform`, which `src/main.tsx`'s `is-linux` asks). It reports no Super to
  a page at all: no `metaKey`, and `getModifierState` false for `Meta`, `OS` and `Super` (2.52,
  Super on mod4 under openbox in the VM, 2026-10-16), so the windows' keys use Ctrl, Alt and
  Shift alone there (platform.ts's header). A transparent window is drawn black without a compositor: the VM
  runs `xcompmgr` (`scripts/linux/run.sh`).
- GDK's coordinates, the pointer's and a monitor's, are the whole desktop's in application pixels
  on X11, y growing down from the top left of the desktop; os/windows/linux.rs turns them into AppKit's.
  Under Wayland a window can't learn where it or the pointer is, and `xdotool` raises nothing.
- A board window gets a native title bar from the window manager; the overlay title bar and its
  traffic lights are macOS's (`lib.rs` project windows, `waiting.rs`).
- The board hides a worktree outside your projects until it holds a session you haven't archived,
  so a fresh home in the VM shows none of its repos until config.toml has a project for them
  (`board.rs` `hidden`).

## Trying it

- `scripts/linux-vm.sh run` starts Xvfb, openbox, xcompmgr and the app with `HOME=~/run-home`,
  and screenshots the display. To try a live session without signing in, start `claude` in tmux
  with that home and write its `sessions/<pid>.json` (`pid`, `sessionId`, `cwd`, `status`,
  `startedAt` from `/proc/<pid>/stat`) and a transcript under `projects/`: Show in, sends and
  keys then go through the real tmux and emulator.
- `--snapshot --open <lane> 's|<key>|focus'` clicks Show in without the app's window, and
  `--snapshot --send` types into the session, both with `DISPLAY=:99`.

## Unchecked

- 2026-10-16: Wayland (XWayland apps keep X11's ways; native Wayland gets no window places), a
  real desktop's Secret Service for `os/secrets.rs`, the global shortcuts (Tauri maps `Cmd` to Super
  there), a signed-in Claude Code's own sessions and usage, and every emulator but xterm.
- The main window isn't put back there: as the first window up its border isn't known, so it
  would grow by its title bar at each launch (`frames.rs`). The waiting window's place
  (2026-10-16, openbox in the VM): its size is kept when it's put away
  and opened again, its frame read with the title bar and the border taken off as it's put back
  (os/windows/linux.rs `border`), but openbox places it as it's shown, wherever it was put before, and
  after a relaunch it opened where Tauri first put it. Putting the frame back once it's shown
  moved it up by the title bar each time: openbox took the place asked for as the inside's, where
  Tauri reads a window's place as its outside's. Its shortcut works there (⌥⌘. as Alt+Super+.).

# Developing Codealong

How to build Codealong from source, how it's checked, and where things are in the code.
[CLAUDE.md](../CLAUDE.md) has the working notes that every change follows, for people and agents
alike: the commands, the conventions, how a change is checked and how it lands. What Codealong
does, feature by feature, is in the [guide](guide.md).

Codealong is a [Tauri 2](https://tauri.app) app: an engine in Rust (`src-tauri/`) that reads what
every harness records, watches git and GitHub, and publishes a board, and a React UI (`src/`) that
draws it in the main window, the floating bar, the quick panel and the rest. The engine runs in
Codealong's server, a process of its own that the app is a view of (*The server and the app*,
below). What goes between
devices is its own crate, codealong-peer (`src-tauri/peer/`), which the
[iPhone app](https://github.com/codealong/codealong-ios) builds on too.

## Requirements

- macOS 13 or later, with Xcode's command line tools (`xcode-select --install`)
- Rust 1.96 or later (`rustup`) and Node 22.13 or later
- Zig 0.17 (`brew install zig`), which builds Ghostty's terminal library for Codealong's own
  terminals (*Ghostty's terminal library*, below)
- `gh` signed in (`gh auth login`) for PRs
- For voice (an experiment): macOS 26 and Xcode 26, whose Swift compiler builds `codealong-ear`
  (`scripts/ear.sh`). Without them everything else builds, and Codealong says it can't hear
- A terminal: Ghostty 1.3+, iTerm2, Terminal or Rex (a new config.toml picks the first of those installed)

## Build and run

```bash
npm install
npm run tauri dev         # the app with hot reload
scripts/relaunch.sh       # build Codealong Dev, then quit it and reopen it from Applications
scripts/catch-up-main.sh  # fast-forward the main checkout to origin/main (relaunch.sh does too)
scripts/version.sh 0.3.0  # start a release: the version everywhere, committed and tagged
scripts/release.sh        # the installer for other Macs (design/plans/releases.md)
scripts/check-release.sh 0.3.0  # a release on GitHub as a download gets it: notarized, signed, its update
npm run snapshot          # print the board as a table without opening a window
```

Builds from `relaunch.sh` are Codealong Dev: a light icon and an id of their own, so they never
pass for the Codealong installed in Applications. Each one replaces `Codealong Dev.app` in
/Applications (~/Applications when that isn't writable), so Spotlight opens the latest build.
Both use your settings and todos; whichever you open last quits the other, and the `codealong`
command follows it. Both say they open Codealong's links (`codealong://session/<key>`, the
`CFBundleURLTypes` in `src-tauri/Info.plist`), and Codealong Dev tells macOS at each launch that
it's the one, so where both are installed a link clicked in a terminal or another app opens
the build of the repo and never the other, which would quit it (`lib.rs` setup, os::apps
`claim_links`). relaunch.sh opens the new build as `codealong` typed alone does, through macOS
with the Dock's environment (`install::open_as_the_dock_does`). A Dock pin from when builds ran from the build folder is pointed at
Applications, so the Dock doesn't show the app as a question mark.

Builds go to `codealong-target` next to the main checkout, one directory shared by every
checkout and worktree (`.cargo/config.toml`); `scripts/target-dir.sh` prints it and has
worktrees inside the checkout build there too. Release builds take minutes and queue on one lock,
so only `relaunch.sh` and `release.sh` make them; the engine the checks run is a debug build of
your checkout, with its dependencies optimized (`scripts/engine.sh`).

## Tests and checks

```bash
npm run typecheck                        # tsc
npm run lint                             # oxlint
npm test                                 # vitest
(cd src-tauri && cargo test)             # the engine and codealong-peer
scripts/engine.sh                        # build this checkout's engine when it changed; print its path
scripts/preview.sh [view] [project]      # render the UI in headless Chrome against your real data
scripts/checks.sh                        # every browser check in scripts/preview/
```

`cargo test` covers the model's rules, the harnesses' parsers, git and GitHub parsing, usage,
config writes, deep links and terminal scripts. It passes on Linux too, less the tests of what
only macOS has (`#[cfg(target_os = "macos")]`); see Linux below. `scripts/preview.sh` renders the UI with Tauri's
IPC stubbed, for screenshots and scripted checks; its header lists the views and options.
`codealong --snapshot` checks the engine's behavior without a window (CLAUDE.md lists its modes).

## Linux

The engine and the app build and run on Linux (X11, WebKitGTK 4.1), where some of what macOS
gives the app has no counterpart. `scripts/linux-vm.sh` makes and drives a VM to build and try it
in from a Mac (Ubuntu 24.04 under [Lima](https://lima-vm.io), `brew install lima`; the VM's
packages are in `scripts/linux/lima.yaml`):

```bash
scripts/linux-vm.sh                # create the VM (the first time) and start it
scripts/linux-vm.sh test           # cargo test in it, on a copy of this checkout
scripts/linux-vm.sh build          # tauri build: the binary and a .deb
scripts/linux-vm.sh run [secs]     # the app on the VM's own headless display; .preview/linux.png
scripts/linux-vm.sh shell [cmd…]   # a shell in the VM's copy of the checkout
```

The VM mounts nothing of the Mac's: it works on a copy of the checkout the script runs from, one
for each checkout (under `~/checkouts`, brought up to date over ssh by every command but `start`
and `shell`), so sessions in different worktrees can use it at once, and its builds and
`node_modules` never mix with the Mac's. A run has a
home of its own there (`~/run-home`, kept between runs): its own config.toml, sessions and
socket, and nothing of the Mac's.

What Codealong asks of the OS is in `src-tauri/src/os/`. What only one OS can call (AppKit and
WebKit, GTK, Win32, the kernel's own calls) is an area there with a `macos.rs`, a `linux.rs` and a
`windows.rs` (Windows below) that give the same functions, picked as it's built; that's the only place `cfg(target_os)` decides what's
compiled, which a test checks. What merely differs, a path or a program to run, goes by
`os::current()`, so both OSes' ways compile and are tested on either, a test asking as if on the
other with `os::as_if`:

| | macOS | Linux |
|---|---|---|
| `os/windows/` | non-activating NSPanels at their levels | borderless windows kept on top through Tauri, screens and the pointer from GDK |
| `os/procs/` | `sysctl` and `proc_pidinfo` | `/proc` |
| `os/apps/`, `os/outline/` | other apps through NSWorkspace, their icons, the Dock icon, the outline round a window | nothing to do: no app is asked to come forward, and editors are listed by name |
| `os/system/`, `os/exits/` | Mach's memory figures, QoS classes, App Nap, kqueue | `/proc/meminfo`, nice, pidfds and eventfd |
| `os/sound/` | NSSound, by name | the freedesktop sound theme, through `paplay` |
| `os/login/` | a login item, through ServiceManagement's `SMAppService` | an XDG autostart entry in `~/.config/autostart` |
| `os/dirs.rs` | `~/Library/Application Support/Codealong`, `Logs`, `Caches` | `~/.config/codealong`, `~/.local/state/codealong`, `~/.cache/codealong` |
| `os/open.rs` | `open` | `xdg-open`, and an app's own command started by itself |
| `os/secrets.rs` | the login keychain (`security`) | the desktop's Secret Service (`secret-tool`) |
| `launch::terminals()` | Ghostty, iTerm2 and Terminal through AppleScript; Rex through its command | `launch/linux.rs`: each new terminal a tmux session, shown in the desktop's terminal emulator |

CI (`.github/workflows/linux.yml`) builds and tests the engine on Linux every four hours, on
`main`, with warnings as errors; `gh workflow run linux.yml` runs it now (`--ref <branch>` for a
pushed branch), and `gh run list --workflow linux.yml` says how the last ones went. It isn't run
on every push, which would spend the organization's plan's Actions minutes within days.

What Linux goes without: the notch, the focus outline round a terminal on another screen
(flash.rs), app icons in the editor menus, pictures of the windows for Improve Codealong, the
Dock icon's status, the Claude and Codex apps (macOS's), Antigravity's and Cursor's sign-ins
for their usage, and the menu bar (Tauri makes none on Linux), so what only it has: New window
(⇧⌘N), Analytics… (⇧⌘U; the usage bar's Analytics button opens it too), Set up Codealong… and Close idle
sessions…. The windows' keys are written for macOS, and `src/platform.ts` presses and says
them as Linux has them (⌘ is Ctrl, ⌥ and ⌃ are Alt); `PREVIEW_PLATFORM=linux` previews them.
`.claude/rules/linux.md` has what was found there.

## Windows

The engine and the app build for Windows 10 (1803) and later, x64 and Arm64, with WebView2. It's
a first cut: it compiles, and its tests compile, but it hasn't run there yet, and some of what
Linux has isn't there either (below). Two scripts check it from a Mac:

```bash
scripts/windows-check.sh [--tests]   # cargo check for Windows, cross-compiled here (cargo-xwin)
scripts/windows-vm.sh                # a Windows 11 Arm64 VM under QEMU, made the first time
scripts/windows-vm.sh test           # cargo test in it, on a copy of this checkout
scripts/windows-vm.sh build          # tauri build --no-bundle: codealong.exe
scripts/windows-vm.sh run [secs]     # the app on the VM's desktop; .preview/windows.png
scripts/windows-vm.sh shell [cmd…]   # PowerShell in the VM's copy of the checkout
```

`windows-check.sh` needs cargo-xwin, the `x86_64-pc-windows-msvc` target and Homebrew's LLVM (its
header says how); it takes a minute or two, where the VM's first build takes many. The VM installs
itself from Microsoft's ISO with no clicks (`scripts/windows/`: the answer file, OpenSSH at first
logon, then Visual Studio's build tools, Rust, Node, Git for Windows and Zig over SSH), which takes
an hour or two the first time. Microsoft's download page sometimes refuses a script; then download
the Arm64 ISO in a browser and give it as `WINDOWS_ISO=<file>`.

Each area of `src-tauri/src/os/` has a `windows.rs` beside its `macos.rs` and `linux.rs`, through
Win32 (`windows-sys`). What macOS and Linux share, POSIX's, is a `posix.rs` where an area has
both (`os/procs`, `os/pty`), and the flat modules pick by `cfg(unix)` and `cfg(windows)` inside:

| | Windows |
|---|---|
| `os/windows/` | borderless windows kept on top through Tauri; screens, the pointer and opacity through Win32 |
| `os/procs/` | a Toolhelp snapshot, `GetProcessTimes`, the TCP table, and each process's PEB for its folder, environment and command line; no `ps`, so `procs::list` reads that table |
| `os/sock.rs` | the server's socket is AF_UNIX there too (uds_windows) |
| `os/pty/` | Codealong's own terminals aren't on Windows yet (no ConPTY), and say so |
| `os/system/`, `os/exits/` | `GlobalMemoryStatusEx`, thread priorities, process handles waited on |
| `os/sound/` | `PlaySound`, the sounds in `C:\Windows\Media` |
| `os/login/` | a value under the registry's `Run` key, which Task Manager's Startup apps lists |
| `os/dirs.rs` | `AppData\Roaming\Codealong`; logs and caches in `AppData\Local\Codealong` |
| `os/open.rs` | the shell's own handler (`rundll32 url.dll,FileProtocolHandler`) |
| `os/secrets.rs` | Credential Manager |
| `launch::terminals()` | `launch/windows.rs`: Windows Terminal, running a new terminal's command in Git Bash |

What Windows goes without, besides what Linux does: Codealong's own terminals, typing into a
session's terminal, finding its tab and bringing it forward (Windows Terminal takes no scripts),
tmux, Claude Code's and the Codex app's own sockets, a server handing its terminals to a new build,
and the menu bar. The windows' keys are Linux's (⌘ is Ctrl, ⌥ and ⌃ are Alt);
`PREVIEW_PLATFORM=windows` previews them. `.claude/rules/microsoft-windows.md` has what was found
there.

## Ghostty's terminal library

Codealong's own terminals (`src-tauri/src/terminals.rs`) run on libghostty-vt, Ghostty's terminal
emulator as a library: the server parses what a terminal's shell prints with it, encodes keys,
the mouse and pastes for the program's modes, and sends the windows the screen to draw
(`src/ui/terminal/`, which draws with WebGL2, or Canvas 2D where there's none). It comes through
the `libghostty-vt` crate, whose build script fetches Ghostty's source at the commit the crate
pins and builds it with Zig, so a first build takes a few minutes and needs the network; it
compiles once into the shared build directory after that. `GHOSTTY_SOURCE_DIR` builds it from a
checkout of Ghostty instead, for building offline.

The library is at 1.0 (Ghostty's `vt-1.0.0` tag), whose C API keeps its promises across 1.x, but
the Rust crate has no release on it yet, so it's pinned to a git revision in `src-tauri/Cargo.toml`.
That revision fixes the Ghostty commit and so the Zig it needs (its `build.zig.zon`'s
`minimum_zig_version`): 0.17 today, which CI installs too (`.github/workflows/`,
`scripts/linux/lima.yaml`). Taking a newer one is a change of its own: move the revision, match
the Zig version everywhere it's named, and run `cargo test terminals`, whose tests feed the
emulator escape sequences and check the screens, keys, mouse and pastes that come of them.
`.claude/rules/libghostty.md` has what the library does and doesn't do that the code relies on.

The snapshot format isn't among 1.0's promises: it's versioned apart from the library, and its
version 1 has no binary-compatibility guarantee yet. So a newer revision may write snapshots
(`Terminal::encode_snapshot`) its older builds can't read:
a server handing its terminals to a new build (`serve.rs` `hand_over`) writes them with the old
library and the new build reads them with its own. One it can't read leaves its program to draw
the screen again (`terminals.rs` `run`), its scrollback lost, so a revision that changes the
format is worth a word in the release notes. `cargo test terminals` checks a snapshot round trip
within one build; across two builds, a server in a home of its own handed over to the next is the
check (`.claude/rules/terminals.md`, *Codealong's own terminals*).

## Icons and the mark

The [logo master and brand notes](../design/brand/README.md) live in `design/brand`.
Run `npm run icons` to regenerate the UI mark, browser icon and native bundle icons.
The mark's inner node shows the app-wide status: red for session errors, amber when
you are needed (including PR review requests), blue for unread results, green while
agents work, and light gray when idle, in that priority order. The project switcher,
floating bar, browser icon and running Dock icon stay in sync across projects.

The screenshots in the README (`docs/images/`) are the real app's UI drawn against a made-up
board, by `npm run shots` in the codealong-website repo; its `studio/readme.mjs` says which shots
they are and copies them here, light and dark. The film the README opens on (`notch-demo.gif`) is
its `studio/reel.mjs`: the same UI, driven through what waits on you and recorded frame by frame.

## How the agent skill and plugin are packaged

`skills/codealong/` is one skill in the Agent Skills layout (`SKILL.md` plus
`references/reference.md`, generated from `codealong --help` by `scripts/skill-reference.sh`),
read as-is by Claude Code, Codex, OpenCode and Antigravity. The app ships it in its bundle and
Settings › Agents (or `codealong install`, or adding an account in Settings) copies it into the
skills folder of each account that isn't Claude Code, with a `.codealong-version` marker; at
launch the app refreshes copies it made when the skill changed, including ones in folders of
accounts you've since removed, and never touches a folder it didn't make. A copy someone edited
is left alone: Settings › Agents marks it Edited, and replaces it only when asked (so does
`codealong install --account <name>`).

The orchestrator is part of the same skill, `references/orchestrate.md`, which SKILL.md points
an agent at when it's asked to run as one, so every harness has it from the one copy. That file
has a skill's frontmatter too, and `plugin/skills/orchestrate/SKILL.md` links to it, so the plugin
offers it in Claude Code as `/codealong:orchestrate` (tauri.conf.json bundles the link's file as
the plugin's). `references/visualize.md`, laying the work in progress out on a board's canvas, is
packaged the same way, as `/codealong:visualize`, and so is `references/meetings.md`, working from
a meeting Codealong heard, as `/codealong:meetings`. A new one is a file there with a skill's
frontmatter, a link to it at `plugin/skills/<name>/SKILL.md`, that link's line in tauri.conf.json,
and its name in skills.rs's test of the limits.

`plugin/` wraps the same skill as a Claude Code plugin (`.claude-plugin/plugin.json`, a
`bin/codealong` shim that finds the app, a `SessionStart` hook that runs `codealong whoami
--context`, and a `PermissionRequest` hook that runs the hidden `codealong session hook
permission-request`), and `.claude-plugin/marketplace.json` makes the repo a marketplace. The app
ships the marketplace too (`Resources/plugin-marketplace`, with the skill from
`Resources/skills`) and installs the plugin in each Claude Code account with `claude plugin`,
from a copy it keeps in `~/Library/Application Support/Codealong/plugin-marketplace`: the plugin
is the one the app answers, and it needs no network or access to the repo. The copy's version is
plugin.json's with a hash of the plugin's files after a `+`, and Claude Code updates a plugin
whenever that string changes, so at launch the app updates the plugin it installed whenever its
files, the skill included, changed. An account whose plugin comes from another marketplace (the
repo's, added by hand) keeps it until you pick Use Codealong's copy. A Claude Code account with
the plugin gets no skill copy, and one Codealong made there before is taken away, so sessions
don't see the skill twice. The permission hook hands the request to the server over its socket
and waits; the server holds it (`claude/asks.rs`) until it's answered in one of Codealong's windows or on
another device, or answered or gone where the session runs, and the hook prints the decision,
or nothing. It prints nothing and exits 0 when no server answers, and one older than the
hook fails its unknown subcommand the same quiet way, so Claude Code's own prompt is always
there to answer. A question (`AskUserQuestion`) is answered with the allow's `updatedInput`,
its questions with your answers beside them; a plan (`ExitPlanMode`) is left to the prompt,
since Claude Code drops a plain allow for it. With `[sessions] live_replies` on, the staged copy's
hooks.json also has an async `MessageDisplay` hook (`install.rs` `LIVE_HOOK`, which the version's
hash takes in), running the hidden `codealong session hook message-display`: it hands the server
each batch of a reply as the terminal shows it, which `claude/live.rs` keeps until the record has
the message. Changing the setting brings the plugin up to date in every account that has
Codealong's copy (`install::follow_live_replies`).

A Codex account gets the skill copy and, beside it, a `PermissionRequest` hook of Codealong's in
its `hooks.json` (`codex/hook.rs`), which runs the hidden `codealong session hook
codex-permission-request` through the staged plugin's `bin/codealong` shim. Codex runs it as a
call needs approving, before its own prompt, and it writes the request down as a note in
`~/Library/Application Support/Codealong/codex-asks` and returns at once with nothing printed,
so Codex asks as it always does; the reader takes a note as the prompt that's up while its turn
is open and its call has no result (`codex/asks.rs`). Codex runs only hooks you've trusted, so
the hook is trusted as it goes in, through Codex's own app-server as its terminal UI's review
does it (`appserver::trust_hook`), and taken out again where that fails.

The command talks to the server over `codealong.sock` next to the config (JSON lines, one
request and reply per line, mode 0600), and the server answers with the board the windows have
and applies todo changes through its engine, so nothing writes over anything. A command that
needs it starts it when none answers. Where it can't (an agent's sandbox may not let a command
reach launchd) or `CODEALONG_NO_SERVER` is set, todo changes go to `todos.json` under an advisory
lock the server takes too, and sessions come from a light scan (no git, PRs or visitors).

## The server and the app

Codealong is two processes of one binary. The server, `codealong serve` (`serve.rs`), runs the
engine and everything that goes on whether a window is open or not: your other devices, the
sessions Codealong runs itself over ACP and their side conversations, the schedules, the
automatic reviews and the rest of what acts on a published board, the token ledger, and the
socket. The app is a view of it (`view.rs`): it keeps a copy of each board the server publishes
(`view::Mirror`), hands it to its windows as they've always heard it (`windowfeed.rs`), and cuts
the floating bar's feed, the desktop cards and the Dock icon from it. A window's command that
reads or changes what the server holds is a call of the server's (`calls.rs`), which the app's
invoke handler hands it over the socket (lib.rs `forward`), its arguments and answer as a Tauri
command's; the windows' code is the same either way. What only a window can do (the panels, menus,
opening a file or a link on your screen) is the app's own, in lib.rs. The server keeps a
connection open to each app it has (`Request::Subscribe`, `views.rs`) and writes it every board,
a reply as it streams, your devices and the rest, one JSON line an event, a ping every few
seconds; it asks the latest app to show a session (`codealong session open`) or to work a board
out, which the main window does.

The server is a launchd job (`os/background.rs`, Linux: a process in a session of its own), so
it's nobody's child: not the app's, nor that of a command run in an agent's sandbox. Its label
is named for the folder of Codealong's files, so a copy tried with a home of its own has a server
of its own; its output goes to `server.log` in Codealong's logs folder, and launchd starts it
again if it crashes. The app starts it as it opens, and connects again whenever it goes; the
command starts it when none answers; `codealong server stop` stops it and the app with it.
Quitting the app leaves it, and the agents it runs, going.

A build's engine is a hash of the Rust it's built from (build.rs `CODEALONG_ENGINE`), not of the
windows' pages it carries too. An app keeps the server it finds when it's its own build, the same
binary and engine (serve.rs `ours`), and replaces it otherwise, so a relaunch that changed only the
windows leaves the sessions Codealong runs going (relaunch.sh replaces the bundle where it was);
one that changed the engine cuts their turns off as quitting the app did, and they carry on once
the new server is up (`[sessions] continue_after_restart`). Codealong beside Codealong Dev, or a
debug build in a worktree, is another binary, so what the server installs and starts always
comes from the app you opened; on a Mac only an installed app's command starts a server by
itself. One server runs for a folder of files, holding a lock beside them for its life
(`server.run.lock`), however it was started. Of two apps, the one opened last stays: one opening
asks those opened before it to quit, the server tells an older one that connects after a newer to
quit, and an older one finding a newer one's server of another build gives way rather than
replacing it (`views::Info::since`, `serve::GIVE_WAY`). The server tells an app of another build
that connects that it's another's, and that app starts its own. A window's command waits for the
app to be connected to its own server before it's sent, and is never sent twice. The app counts
each board's `rev` itself, so a new server, whose count starts again, is never taken for older
than what the windows have. When launchd won't take the job, the app runs the server in itself,
as the engine always was.

## Code map

| Path | What |
| --- | --- |
| `src-tauri/src/engine.rs`, `throttle.rs` | Caches, file watching (FSEvents), git workers (spaced per worktree, results published in batches), GitHub, publishing the board |
| `src-tauri/src/windowfeed.rs`, `src/boardPush.ts` | What the windows are sent of each board: the first whole, then the fields, lanes, sessions and PRs that changed, put back together in each window, which reads the board whole when it misses one (`api.ts` `onBoard`) |
| `src-tauri/src/board.rs` | The model's rules: status, bays, headlines, open targets (pure, tested) |
| `src-tauri/src/claude/` | Claude Code and the Claude app's Code sessions, a file a subject: the adapter and reader (`mod.rs`), the registry and the background service's records, transcripts and the conversation one holds, permission modes, the requests the plugin's hook hands the app (`asks.rs`) and the replies its `MessageDisplay` hook does (`live.rs`), the app's records, the ways a prompt gets in (the background service's socket, a session's own, the app), starting and forking, names, side conversations, what its Bash tool is running, folder trust, plan usage, and a repo's `WorktreeRemove` hooks and a job's record as a worktree goes (`teardown.rs`) |
| `src-tauri/src/codex/` | Codex, in its app and its terminal UI: the adapter and reader (`mod.rs`), rollouts and the thread index, the conversation a rollout holds, its app-server (archiving, renaming, new threads), the socket the app's windows talk on (starting, steering and stopping a turn), its terminal UIs, side conversations and plan usage |
| `src-tauri/src/opencode.rs`, `antigravity.rs`, `cursor.rs` | Harness adapters: OpenCode's database and background service; Antigravity's app and command line (`agy`), their conversation summaries, transcripts and processes; Cursor's app (its chats in its state database) and command line (`cursor-agent`), their transcripts and processes |
| `src-tauri/src/acp.rs` | Sessions Codealong runs itself: an Agent Client Protocol adapter process per account, turns, permission requests, the streamed reply |
| `src-tauri/ear/main.swift`, `src-tauri/src/ear.rs`, `scripts/ear.sh` | Hearing: `codealong-ear`, a helper in Swift (Apple's speech recognition has no other interface) that captures the microphone, what the Mac plays or what some apps play, and says what it heard as JSON lines; the engine's way to it; and the script that builds it, since cargo doesn't. relaunch.sh, release.sh and engine.sh put it beside the engine |
| `src-tauri/src/listen.rs`, `src/listening.ts`, `src/ui/Listening.tsx` | Sessions that listen to a meeting: the transcript, when a session is told what's new and in what words, the roles (notes, coach, requests), taking one up again after a restart; in the windows, the meeting drawn in the session's conversation, the strip over its box, its chip and the dialog that sets one up |
| `src-tauri/src/echo.rs` | Speakers or headphones, worked out from the sound: whether a listening's microphone hears what the Mac plays (how quiet it gets while the Mac is loud against while it's silent), from what the Mac plays through as a first guess, for the helper to take the Mac's audio out of the microphone |
| `src-tauri/src/meetings.rs`, `src/meetings.ts`, `src/ui/Meeting.tsx` | Meetings: the calendar read through the helper (EventKit), the rule for which meeting is on offer and when, taking one (a session started as a chat, which listens as its note-taker) or passing on it, and the folder a meeting's transcript, notes and `meeting.json` go in; the offer as the bar, the notch and the main window draw it |
| `src-tauri/src/dictation.rs`, `src/talk.ts`, `src/said.ts`, `src/ui/Said.tsx` | A key held to talk: hearing while it's down, then what was said typed where the caret is, into a session's box, or sent to the orchestrator; what the bar shows of it, and of a listening session's cue |
| `src-tauri/src/side.rs`, `src/side.ts`, `src/ui/SideStrip.tsx` | Side conversations: a throwaway copy of a session's conversation, run by its harness (`Adapter::side`), asked from the reply box with `/side` and shown above it, or popped out into a window of its own (kept.rs); what was asked and answered, kept to read later (`sides.json`) |
| `src-tauri/src/suggestions.rs`, `src/ui/SuggestedReplies.tsx` | Suggested replies: what another session suggested a session's user send it (`codealong session suggest`, `suggestions.json`), kept until the session moves on (`board::suggestions_hold`), and the list above the reply box that sends one with a click; for questions a session asks, choices of their own picked in its question box (`suggestions::picked`) |
| `src-tauri/src/repos.rs`, `git.rs`, `github.rs`, `ghpoll.rs` | Repo discovery, callsigns, worktrees, git status, PRs, and when GitHub is asked for them |
| `src-tauri/src/held.rs` | The repo scan's reads of folders it doesn't own, each on a thread of its own, so one macOS holds for a privacy prompt holds up only that folder |
| `src-tauri/src/launch.rs`, `launch/iterm.rs`, `launch/terminal_app.rs`, `launch/rex.rs`, `launch/linux.rs` | Terminal profiles: Ghostty, iTerm2 and Terminal through AppleScript, Rex through its command (a session's tab found exactly, typing, closing), on Linux tmux shown in the desktop's emulator, deep links |
| `src-tauri/src/editors.rs`, `src/editors.ts` | The editors a folder opens in: the ones Codealong knows, found where Mac apps are installed, and the Open in menus |
| `src-tauri/src/flash.rs` | Outlines a focused terminal that came forward on another screen |
| `src-tauri/src/caret.rs`, `src/caret.ts` | A text cursor WebKit stopped drawing in a window you type in: typing into a page that says it isn't focused hands it its focus again, and the perf log says what macOS said of the window |
| `src/typingTrace.ts` | With the perf log on, what moved the text cursor in a session's reply box, in each window: typing, drafts from another window, focus, and the page's own calls that place the caret |
| `src-tauri/src/previews.rs`, `src-tauri/src/chrome.rs`, `src/ui/PreviewBits.tsx` | Previews: each worktree's URL and Chrome profile (`previews.json`), the servers listening in worktrees' folders (`procs::listeners`, the kernel's socket lists), their front pages read once, a kqueue for their exits; Chrome's profiles and opening a page in one; the row's chip, the panel's section and the URL sheet |
| `claude/transcript.rs` (`Background`), `codex/background.rs`, `src/background.ts`, `src/ui/BackgroundBits.tsx` | What an agent left running in the background (`Session::tasks`): Claude Code's shells and Monitors from its transcript, Codex's background terminals from its rollout, what each printed and Stop (`Adapter::task_output`, `stop_task`); the header's chip and list |
| `src/ui/ContextMeter.tsx` | How full a session's context window is, in its reply box's foot (`Session.context`, from each harness's latest call), and on hover what the session has cost |
| `src/ui/UsageDialog.tsx`, `src/spend.ts` | The Analytics window (View › Analytics…, the usage bar): what sessions used and cost from the token ledger, its tiles, charts and table, and the words and sums for them (pure, tested) |
| `src/ui/UsageAnalytics.tsx`, `src/analytics.ts`, `src/ui/UsageChart.tsx` | The Analytics window's Your part and Sessions, with `[experimental] analytics` on: who began each turn and what your prompts did, how sessions began and how you left them (analytics.rs), their series, words and sums (pure, tested), and what every chart of the window shares |
| `src/ui/JevUsage.tsx`, `src/jevUsage.ts` | Settings › Jev's usage: what Codealong's requests to Jev used and cost today or over the last days, by the feature that asked, and the sums and words for it (pure, tested) |
| `src-tauri/src/idle.rs`, `src/ui/CloseIdleDialog.tsx` | Close idle sessions: which sessions can close or only be archived, and why (pure, tested), and closing and archiving them |
| `src-tauri/src/stale.rs`, `src/ui/CloseStaleDialog.tsx` | Close stale terminals: every terminal tab from the process table, when its device was last used (a redraw of every tab not counted: `Uses`, which the engine updates every tick), what it shows and whether closing it loses anything (pure, tested), and closing the ones picked |
| `src-tauri/src/cleanup.rs`, `claude/teardown.rs`, `src/ui/CleanUpDialog.tsx` | Clean up worktree: whether one can go with nothing in it lost (`board::cleanup`, pure, tested, on each lane), checked again against git as it goes; the repo's own `WorktreeRemove` hook or git removing it, then `claude rm` for a background job that held it; Mark as done on such a job; the dialog that asks first |
| `src-tauri/src/start.rs`, `src/start.ts` | New sessions: where each account can start one, the launch it makes, the agent menu (every account on every model, and the effort levels) and Auto's pick; and going on with one that isn't running, by the same rules (`resume`) |
| `src-tauri/src/quick.rs`, `src/quick.ts`, `src/ui/Quick.tsx` | The quick panel: its window (a panel that takes the keyboard over any app), its shortcut and the waiting window's, and its page (what needs you, finding sessions, starting one) |
| `src-tauri/src/waiting.rs`, `src/ui/Waiting.tsx` | The waiting window: an ordinary window, made when first asked for and hidden on close, that keeps where it was, comes from ⌥⌘., Window › Waiting on you and ⇥ in the quick panel, and works through what waits on you one card at a time (`quick.ts` `queueRows`, `queue.ts`) |
| `src-tauri/src/frames.rs` | Where an ordinary window was, for the next launch: the main window's `window.json` and the waiting window's `waiting.json`, noted as it moves and written once it stops, and where one opens (`start_frame`) |
| `src/ui/StartControls.tsx`, `src/startAccounts.ts` | Where a session goes and on what, as the start row and a stopped session's box both draw it (the places, a model menu's lines, the effort row), and this Mac's accounts as they offer them, read once a minute |
| `src-tauri/src/harness.rs` | The interface each harness implements: an `Adapter` for a session (its record and conversation, what it waits on you to allow, opening, sending, starting, searching, renaming with `src/ui/RenameDialog.tsx`) and for an account, and a `Reader` that reads its sessions for the board; `.claude/rules/adding-a-harness.md` lists it all |
| `src-tauri/src/transcript.rs`, `src/ui/SessionPanel.tsx`, `src/ui/SessionHeader.tsx`, `src/sessionContext.ts`, `src/ui/SessionThread.tsx`, `src/ui/SessionReply.tsx`, `src/ui/SessionAgent.tsx`, `src/ui/ModelMenu.tsx` | A session's conversation, read a page at a time from its own record, the panel that shows it, and the box that sends it messages or answers what it waits on, with its account, model and effort as one chip, and for one that isn't running, where it goes on and on what |
| `src-tauri/src/peek.rs`, `src/ui/Peek.tsx` | The floating bar's peek: its window (a panel under the bar that takes the keyboard only on a click), where it opens for a pill and when it closes. What it shows is the session's card (`ui/SessionCard.tsx`), as the waiting window does |
| `src/ui/Notch.tsx`, `src/ui/NotchSession.tsx`, `src/ui/NotchReview.tsx` | The bar docked in the notch: its three states (closed, the pills, a session, a review or the end of what waits under them) shown through one clip, and what it shows in place of a peek: which session or PR, the queue of both it works through and its keys |
| `src/ui/QueueEnd.tsx` | The end of what waits, where the notch and the waiting window work through it: nothing (else) waits, and how the last one went |
| `src/ui/PrCard.tsx`, `src/ui/PrBox.tsx`, `src/review.ts` | A PR waiting on your review as a card of its own, in the notch and the waiting window: the PR panel's details with the strip over them, the box that approves, comments or asks for changes (PrBox.tsx, fixed at the foot of the panel too, with the line over it saying where the PR's review stands), the PR's menu and its chords; and Review with…, which starts a review session on an account or opens the Codex app's review page |
| `src/ui/SessionCard.tsx`, `src/ui/cardHost.ts` | A session as a card of its own, for the peek, a session kept on the desktop, the session in the notch and the waiting window: the session's panel with its menus, what's around it and its shortcuts wired once; and, for the windows that aren't the board's, the session kept up with the board while its card is up |
| `src-tauri/src/kept.rs` | Sessions kept on the desktop from the peek or the notch, and side conversations popped out: a window each, made as the peek's is, `kept.json`, where a popped-out one opens, sizing and closing. `Peek.tsx` draws them too |
| `src-tauri/src/search.rs` | Searching what was said in sessions: an in-memory index of prompts and replies, read once per file and then from where it stopped |
| `src-tauri/src/skills.rs`, `src/skills.ts`, `src/useSkills.ts`, `src/ui/SlashList.tsx` | Skills by name: the folders each harness reads them from (`Adapter::worktree_skills`, `account_skills`) listed once by name with how each harness is told to run one, the `/` lists of the quick panel and the reply box, the repo a skill picks and the accounts it leaves out |
| `src-tauri/src/route.rs`, `src-tauri/src/starts.rs`, `src/route.ts` | Suggestions in the quick panel: the request to Jev and reading its answer back, the key in the keychain, how your past starts weigh a new worktree, and which suggestions the panel shows |
| `src-tauri/src/todojev.rs` | Jev on todos: which todo a new session is for, whether a todo's work looks done, and a todo's suggested project, labels and dependencies |
| `src-tauri/src/send.rs` | Sending a prompt to a session that's already going: its harness's ways in tried in order, typing into its terminal and resuming it (`start::resume`), a way of the harness's own by its module (`send::Own`) |
| `src-tauri/src/terminal.rs` | Commands run in your terminal: an agent's (`codealong terminal run`), its request held on the command's connection while its session waits on you for it, and yours (`!` in the box, Run on a block in the conversation; actions.rs `run_command`), whose session is told what it printed once it ends (`report_ended`); the run's folder, the wrapper its tab runs under `script(1)`, what it printed rendered as a screen ends up showing it, and stopping it as Ctrl-C does |
| `src-tauri/src/runcode.rs`, `src/ui/RunButton.tsx` | Run on a code block and `!` in a session's box: the lines of a shell block to run for the session in a new terminal (comments out, here-documents and quoted text whole), the commands on the PATH that make a block with no language one, and the button |
| `src-tauri/src/peers.rs`, `peerfeed.rs` | Your other devices: links, connections, and the feed of worktrees and sessions sent a change at a time; `design/plans/devices.md` has the reasoning |
| `src-tauri/src/shared.rs` | The automations your machines share, so one of them runs each: where each stands (`[devices] automations`) and whether it's around; `autoreview.rs` `reviewer_of` picks which reviews a PR, `schedules/machines.rs` `line` which runs a schedule |
| `src-tauri/peer/` | codealong-peer, what goes between devices: the protocol (each side proves its device key), link codes and iroh, feeds, the requests and their answers, the worktree and session model, the schedules your machines share, and a device that only watches. The iPhone app ([codealong-ios](https://github.com/codealong/codealong-ios)) builds on it; `design/plans/ios.md` has the plan |
| `src-tauri/src/tailnet.rs`, `devicekey.rs` | Reaching them on your tailnet through Tailscale's command (iroh is in codealong-peer), and this device's key in the keychain |
| `src/menus.ts`, `src/contextMenu.ts` | Right-click menus of sessions, worktrees, PRs and todos, each defined once and the same in every window (each says what it can do); native menus through Tauri |
| `src/devices.ts`, `src/useDevices.ts`, `src/ui/DevicesSection.tsx` | Other devices' worktrees in the views, followed beside the board by every window that shows them, and Settings › Devices |
| `src-tauri/src/shown.rs` | What the windows show: the board and your other Macs' worktrees beside it, which the bar's feed, desktop cards, the lines between them and a session for the peek are cut from; the board alone is what the engine acts on |
| `src/notices.ts`, `src/ui/Notices.tsx` | Notices: what isn't working in the background, the engine's and a window's own, as a chip that can be dismissed |
| `src/tips.ts`, `src/ui/Tip.tsx` | Tips on using Codealong: the one list, each tip with the places it shows and its keys from the chord tables, and the box that shows them where a window waits on what it reads |
| `src/ui/Sidebar.tsx`, `src/savedViews.ts` | The sidebar: the view and what narrows it, open or folded to a rail, and saved views |
| `src-tauri/src/cli.rs`, `server.rs`, `install.rs` | The `codealong` command (clap; JSON or tables; the server over its socket, started when none answers, else the files), the server's socket, and putting the skill and the command where agents find them |
| `src-tauri/src/serve.rs`, `views.rs`, `os/background.rs`, `build.rs` | Codealong's server (`codealong serve`): what it runs, starting, replacing and stopping it as a launchd job, the apps connected to it and what it tells and asks them, and the engine's build id an app keeps a server by |
| `src-tauri/src/calls.rs`, `view.rs` | What the windows ask the server, as their commands (`calls!`); and the app's side of it: taking over from an app that's open, starting and following the server, the board's copy (`Mirror`), and handing a call to it |
| `src-tauri/src/changes.rs`, `hygiene.rs` | For an orchestrator's loop: what changed on the board since an earlier look, told by difference from what that look saw (`codealong changes`, `changes.json`); and what's left running of old sessions, with the Mac's load, memory and swap, and ending it (`codealong hygiene`) |
| `src-tauri/src/actions.rs` | What Codealong does to a session or a worktree, or asks of a device, one table of requests: done here, or asked of the device it's on, for a window, another device and the `codealong` command alike, with what each may ask for and what each carries |
| `src-tauri/src/snapshot.rs` | `codealong --snapshot`: the engine without a window, its modes in one table, for checking what it shows and does |
| `src-tauri/src/todos.rs`, `src/todos.ts`, `src/ui/LabelPicker.tsx` | Todos: the store and its changes, linking sessions started from a todo; the list's sections, filters and prompt; the labels todos and sessions wear, and their picker |
| `src-tauri/src/schedules/`, `src/schedules.ts`, `src/ui/SchedulesView.tsx`, `src/ui/SchedulePanel.tsx`, `src/ui/ScheduleDialog.tsx` | Schedules: what each is and what happened to it (`schedules.json`, `schedule-runs.json`, `mod.rs`), the times a schedule's words name (`rule.rs`, tested on made-up clocks with summer time), and the scheduler, which works out what's due every few seconds and starts or sends for a run (`runner.rs`); the schedules your machines share, each keeping a copy from the others' feeds, and which of them runs one (`machines.rs`, with codealong-peer's `schedules`); the view, its panel and the dialog that makes one |
| `src-tauri/src/boards.rs`, `src/boards/`, `src/ui/BoardBar.tsx`, `src/ui/BoardDialog.tsx`, `src/ui/RuleFields.tsx` | Boards: named sets of work in a project, each shown as Kanban or Canvas. `boards.json` and every change to it (`BoardOp`, which the engine makes for the window and for `codealong board`), what a rule takes in, who's on a board and which column a card goes in (`rules.ts`), the window's boards with its changes shown at once (`useBoards.ts`), and the main window's answers to `codealong board show\|move\|arrange`, which the engine hands it (`useBoardAsks.ts`); the strip's toggle, and the board, column and rule dialogs |
| `src/canvas/`, `src/ui/CanvasView.tsx`, `src/ui/CanvasCard.tsx` | A board's canvas: its documents (what's on it, where; kept in sync with what the board shows, and between windows; Tidy's layouts, and `codealong board arrange`'s), the WebGL layer (grid, links, zoomed-out blocks) and the HTML cards, notes and frames on top |
| `src-tauri/src/attachments.rs`, `src/attachInput.ts` | Files in descriptions and prompts: pasted, dropped or picked, kept by content, images shown in place, all handed to agents as paths |
| `src-tauri/src/turns.rs` | Analytics (`[experimental] analytics`): what came in to each session and who from, read from each harness's records as the ledger reads calls (`Adapter::turn_files`, `turn_step`; `claude/turns.rs`, `codex/turns.rs`, and any other harness through its `prose`), kept as a row a turn in `ledger.sqlite`; what each turn counts as (`counted`: one the session began itself with the turn it follows, a started session's opening as its starter's); the turn each call was made in; and the report of it |
| `src-tauri/src/stance.rs` | What each prompt of yours did with the reply before it: by rule where the record is sure (`by_rule`), by Jev (`[jev] prompts`, `request`, `pass`) or by an agent's tag |
| `src-tauri/src/began.rs` | Where each session came from (`began_of`: you, a fork, an agent's helper, an agent, a review, a schedule, a headless run), handed over by the engine as it publishes and kept in the ledger |
| `src-tauri/src/dismissed.rs` | How the sessions you marked done, archived or closed idle stood as you left them (`board::standing_of`), noted in the ledger; the report of them, and the open sessions left hanging |
| `src-tauri/src/analytics.rs` | The analytics report the Analytics window's Your part and Sessions and `codealong analytics` show, and an agent's tag of a prompt |
| `src-tauri/src/ledger.rs` | The token ledger behind the Analytics window: every model call each harness recorded (`Adapter::token_files`, `token_use`, `token_uses_since`), read once per file and then from where it stopped into `ledger.sqlite`, one row per call however many records copy it; and reports of it by day, account, model, folder and session, priced as they're asked, with what was paid (plans a day's share at a time, Claude's extra usage readings, recorded and by-the-token costs); and each request to Jev with its tokens, by the feature that asked, for Settings › Jev |
| `src-tauri/src/prices.rs`, `prices.json` | What a model's tokens cost at API prices, by kind of token, with fast mode, priority and long-prompt prices; `prices.json` is written by `scripts/prices.mjs` from LiteLLM's list, and `[prices]` in config.toml goes over it |
| `src-tauri/src/usage.rs` | Plan usage: what every harness's numbers go into, and each account's state for the Accounts dialog; the numbers are the harnesses' own (Claude through its sign-in and usage endpoint, Codex from its rollouts, none for OpenCode and Antigravity) |
| `src-tauri/src/config.rs`, `state.rs` | `config.toml` (accounts saved in place) and `state.json` (seen, pinned, archived, sessions' labels, account folders already read) |
| `src-tauri/src/zoom.rs`, `src/zoom.ts` | Zooming: the steps, the View menu's items, the webview's page zoom on every board window, the CSS zoom a card takes in the windows that float, and the keys that ask for it |
| `src-tauri/src/store.rs` | The small JSON files next to the config, read, locked and written one way: a temporary file of the write's own and a rename, a read-modify-write under the file's `.lock`, and a file that doesn't parse left as it is, with an error |
| `src-tauri/src/reusable.rs`, `src/reusable.ts` | Reusable worktrees: the marks made by hand (`worktrees.json`); `board::reusable`, `reuse_of`, `earlier` and `looks_reusable` have the rules, `git::new_work` reads the reflog, and `src/route.ts` sends Jev's new work to them |
| `src-tauri/src/prlinks.rs` | Sessions tied to PRs other than by branch: what their records show they did to a PR, links made by hand, and the PRs whose primary session is nudged with what happens on them, and who that is when picked (`pr-links.json`) |
| `src/ui/AgentReview.tsx`, `src/prDraft.ts`, `src/agentReview.ts` | The top of a PR's Reviews tab, your review: where the agent's review stands, your draft review on GitHub (read once for every part of a window, its text and comments changed and taken out there, submitted by any verdict, github.rs `your_review`, `review`), what the agent wrote, and the panel's box for a verdict; which tab a PR's details open on |
| `src/ui/PrReviewers.tsx`, `src/prReviews.ts` | The rest of a PR's Reviews tab: everyone on the PR once, by who asked for changes first, each with their verdict and what they wrote (reviews' text, threads on the code with the lines around them, comments), what's still open first; what the tab counts |
| `src-tauri/src/autoreview.rs` | Automatic reviews (`[reviews]`, `[reviews.own]`): which PRs are due a review and when, the session it runs in (for a PR of yours, on another harness than its primary session), what it's told (Codealong's instructions or yours), reading the review from its last message, and putting it in a draft review of yours or a comment, or handing it back to a PR of yours' primary session (`reviews.json`) |
| `src-tauri/src/teamfile.rs`, `src/repoSettings.ts` | Settings for one repo's pull requests: a repo's team settings (`.codealong/config.toml`, read from the branch its PRs merge into as fetched, parsed with what's each person's own refused), your `[repos."github.com/…"]` tables, and how they go together over `[reviews]` and `[pull_requests]` (config.rs `reviews_for`, `pull_requests_for`; mirrored for Settings and the PR windows, with where each value comes from and the team's offer) |
| `src/prLinks.ts`, `src/ui/PrLink.tsx` | Links to PRs in conversations and prompts: the board's PRs by link, and the card a link shows on hover (`pr_card` for a PR the board doesn't list) |
| `src/codealongLinks.ts`, `src/ui/SessionLink.tsx` | Links to a session or a todo in a conversation, as the skill has agents write them (`codealong://session/<key>`): which one a link names, and the link with a session's card on hover. The window says what it lists through the PR links' host |
| `src-tauri/src/chrometabs.rs` | The Chrome tabs a Claude Code session drove through Claude in Chrome, read from its transcript; which profile they're in (the extension's storage, then History); bringing its tab forward by AppleScript, or its last page again in that profile |
| `src-tauri/src/detect.rs` | The harness accounts on this device: folders, sign-ins, and the wrappers and aliases that run them |
| `src-tauri/src/os/` | Everything asked of macOS, Linux or Windows, each area with each OS's side (its `mod.rs` header; the Linux and Windows tables above) |
| `src-tauri/src/panel.rs`, `os/windows/` | The one AppKit (and on Linux GTK) layer for the windows that float (the bar, its peek, the quick panel, the cards and sessions kept on the desktop, the lines between cards): turning a Tauri window into a non-activating panel and back, what each kind answers about the keyboard, every window's level (`Level`), showing, hiding, taking and handing back the keyboard, frames, screens and the pointer |
| `src-tauri/src/geometry.rs` | Rectangles on the screen (`Rect`, AppKit's y up) and in a window's page (`PageRect`, y down from its window's top) with the flip between them, the screen a thing is on, and keeping a window on it (pure, tested) |
| `src-tauri/src/front.rs` | Bringing the main window forward, asking another app to come forward after a panel opened something in it, and whether an app is the active one |
| `src-tauri/src/bar.rs`, `barfeed.rs` | The bar's window (a panel over every app, floating where you put it or docked in the notch, found from the screen's sides of the menu bar, where it takes clicks only on the shape it shows and has the keyboard only while it shows a session; Window › Floating bar and Bar in the notch) and the small feed of what it shows, sent only when that changes |
| `src-tauri/src/menubar.rs` | The bar's third place: an item in the menu bar (Tauri's tray) in place of its window, whose menu lists what the bar shows and has the server's state, Restart server and quitting with the server or without it; its mark dims while the app isn't connected to the server |
| `src-tauri/src/origins.rs`, `src/lineage.ts`, `src-tauri/src/lineage.rs`, `src/ui/DesktopLines.tsx` | Where sessions came from: Codealong's own record of starts and forks (`origins.json`), the line drawn between them on the canvas and between desktop cards, and the clear window under the cards that draws it there |
| `src/ui/card/` | The card the Canvas view, the Kanban view and the desktop share: what it shows, looked up from the board (`data.ts`), its face (`CardFace.tsx`), and the popover a canvas card opens (`CardPopover.tsx`). Each adds only where a card goes and what a click does |
| `src/ui/KanbanView.tsx`, `src/kanban.ts` | A board's Kanban: its columns (boardLists.ts `kanbanColumns`: the stages, or its own) of todo and session cards, and what a card dropped on a column or another card does (`kanbanDrop`, worked out by where the change would land it, `applyKanbanDrop`) |
| `src-tauri/src/desktop.rs`, `src/ui/DesktopPin.tsx` | Cards pinned to the desktop: a window each, made as the bar's is, where a new one goes, `desktop.json`, and what each shows, cut from the board and sent only when it changes; the page draws the card the Canvas view draws (`ui/card/`). `drag_out` follows the pointer for a thing dragged out of a window |
| `src/mark.ts`, `src/ui/MarkLive.tsx` | The mark's motion on the bar: what happened between two feeds (a new session working, a turn ended, something needs you, a message sent), which moment plays for it, and the mark in parts that move |
| `src/ui/useRowDrag.tsx` | Dragging rows in the lists and the quick panel: among the pinned ones (pin, reorder), down to the list (unpin), out of the window (the desktop) |
| `src/` | The React UI, built from the design system's components and tokens; `lanes.ts` groups the board and mirrors the engine's status rules for the age filter; `pins.ts` keeps the pinned lists; `history.ts` the sessions you looked at, for going back and forward; `ahead.ts` shows sessions marked seen, archived, renamed or sent a message at once, in every window, until a board shows it; `prStatus.ts` and `ui/PrPills.tsx` say where a PR stands and which PRs are your move; `bar.ts`, `ui/Bar.tsx` and `ui/Notch.tsx` lay out the bar, floating or in the notch; `ui/Picker.tsx` is the quick panel's filterable menus, `ui/FieldPicker.tsx` one opened from a form's field, and `ui/pickerOptions.tsx` worktrees and sessions as their options; `ui/SettingsDialog.tsx` and `ui/AccountsSection.tsx` are Settings |
| `scripts/preview.sh` | Headless preview against real data, for screenshots and scripted checks |
| `src/styles/` | The stylesheets: `app.css` is the index, only `@import`s in cascade order with `tokens.css` first; the files several components share sit beside it (base elements, glyphs, cards, controls, chips, rows, nav, menus, trees, the dialog and panel shells), and each component's own styles next to it (`src/ui/Sidebar.css`). `styles.test.ts` keeps each class in one file, every styled class one the UI renders, and every file in the index once |
| `scripts/preview/style-diff.mjs` | Records every scene's computed styles before and after a change to the stylesheets and compares them |
| `design/tokens.json` | Design tokens; `npm run tokens` regenerates `src/styles/tokens.css` |

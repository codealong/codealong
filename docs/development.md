# Developing Codealong

How to build Codealong from source, how it's checked, and where things are in the code.
[CLAUDE.md](../CLAUDE.md) has the working notes that every change follows, for people and agents
alike: the commands, the conventions, how a change is checked and how it lands. What Codealong
does, feature by feature, is in the [guide](guide.md).

Codealong is a [Tauri 2](https://tauri.app) app: an engine in Rust (`src-tauri/`) that reads what
every harness records, watches git and GitHub, and publishes a board, and a React UI (`src/`) that
draws it in the main window, the floating bar, the quick panel and the rest. What goes between
devices is its own crate, codealong-peer (`src-tauri/peer/`), which the
[iPhone app](https://github.com/codealong/codealong-ios) builds on too.

## Requirements

- macOS 13 or later, with Xcode's command line tools (`xcode-select --install`)
- Rust 1.91 or later (`rustup`) and Node 22.13 or later
- `gh` signed in (`gh auth login`) for PRs
- A terminal: Ghostty 1.3+, iTerm2 or Terminal (a new config.toml picks the first of those installed)

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
command follows it. relaunch.sh opens the new build as `codealong` typed alone does, through macOS
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
npm run lint                             # eslint
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
WebKit, GTK, the kernel's own calls) is an area there with a `macos.rs` and a `linux.rs` that give
the same functions, picked as it's built; that's the only place `cfg(target_os)` decides what's
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
| `launch::terminals()` | Ghostty, iTerm2 and Terminal through AppleScript | `launch/linux.rs`: each new terminal a tmux session, shown in the desktop's terminal emulator |

CI (`.github/workflows/linux.yml`) builds and tests the engine on Linux every four hours, on
`main`, with warnings as errors; `gh workflow run linux.yml` runs it now (`--ref <branch>` for a
pushed branch), and `gh run list --workflow linux.yml` says how the last ones went. It isn't run
on every push, which would spend the organization's plan's Actions minutes within days.

What Linux goes without: the notch, the focus outline round a terminal on another screen
(flash.rs), app icons in the editor menus, pictures of the windows for Improve Codealong, the
Dock icon's status, the Claude and Codex apps (macOS's), and Antigravity's and Cursor's sign-ins
for their usage. The windows' keys are written for macOS, and `src/platform.ts` presses and says
them as Linux has them (⌘ is Ctrl, ⌥ and ⌃ are Alt); `PREVIEW_PLATFORM=linux` previews them.
`.claude/rules/linux.md` has what was found there.

## Icons and the mark

The [logo master and brand notes](../design/brand/README.md) live in `design/brand`.
Run `npm run icons` to regenerate the UI mark, browser icon and native bundle icons.
The mark's inner node shows the app-wide status: red for session errors, amber when
you are needed (including PR review requests), blue for unread results, green while
agents work, and light gray when idle, in that priority order. The project switcher,
floating bar, browser icon and running Dock icon stay in sync across projects.

The screenshots in the README (`docs/images/`) are the real app's UI drawn against a made-up
board, by `npm run shots` in the codealong-website repo.

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
packaged the same way, as `/codealong:visualize`.

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
don't see the skill twice. The permission hook hands the request to the app over its socket
and waits; the app holds it (`claude/asks.rs`) until it's answered in one of Codealong's windows or on
another device, or answered or gone where the session runs, and the hook prints the decision,
or nothing. It prints nothing and exits 0 when the app isn't running, and an app older than the
hook fails its unknown subcommand the same quiet way, so Claude Code's own prompt is always
there to answer. A question (`AskUserQuestion`) is answered with the allow's `updatedInput`,
its questions with your answers beside them; a plan (`ExitPlanMode`) is left to the prompt,
since Claude Code drops a plain allow for it. With `[sessions] live_replies` on, the staged copy's
hooks.json also has an async `MessageDisplay` hook (`install.rs` `LIVE_HOOK`, which the version's
hash takes in), running the hidden `codealong session hook message-display`: it hands the app
each batch of a reply as the terminal shows it, which `claude/live.rs` keeps until the record has
the message. Changing the setting brings the plugin up to date in every account that has
Codealong's copy (`install::follow_live_replies`).

The command talks to the running app over `codealong.sock` next to the config (JSON lines, one
request and reply per line, mode 0600), and the app answers with the board its windows have and
applies todo changes through its engine, so nothing writes over anything. Without the app, todo
changes go to `todos.json` under an advisory lock the app takes too, and sessions come from a
light scan (no git, PRs or visitors).

## Code map

| Path | What |
| --- | --- |
| `src-tauri/src/engine.rs`, `throttle.rs` | Caches, file watching (FSEvents), git workers (spaced per worktree, results published in batches), GitHub, publishing the board |
| `src-tauri/src/board.rs` | The model's rules: status, bays, headlines, open targets (pure, tested) |
| `src-tauri/src/claude/` | Claude Code and the Claude app's Code sessions, a file a subject: the adapter and reader (`mod.rs`), the registry and the background service's records, transcripts and the conversation one holds, permission modes, the requests the plugin's hook hands the app (`asks.rs`) and the replies its `MessageDisplay` hook does (`live.rs`), the app's records, the ways a prompt gets in (the background service's socket, a session's own, the app), starting and forking, names, side conversations, what its Bash tool is running, folder trust, plan usage, and a repo's `WorktreeRemove` hooks and a job's record as a worktree goes (`teardown.rs`) |
| `src-tauri/src/codex/` | Codex, in its app and its terminal UI: the adapter and reader (`mod.rs`), rollouts and the thread index, the conversation a rollout holds, its app-server (archiving, renaming, new threads), the socket the app's windows talk on (starting, steering and stopping a turn), its terminal UIs, side conversations and plan usage |
| `src-tauri/src/opencode.rs`, `antigravity.rs`, `cursor.rs` | Harness adapters: OpenCode's database and background service; Antigravity's app and command line (`agy`), their conversation summaries, transcripts and processes; Cursor's app (its chats in its state database) and command line (`cursor-agent`), their transcripts and processes |
| `src-tauri/src/acp.rs` | Sessions Codealong runs itself: an Agent Client Protocol adapter process per account, turns, permission requests, the streamed reply |
| `src-tauri/src/side.rs`, `src/side.ts`, `src/ui/SideStrip.tsx` | Side conversations: a throwaway copy of a session's conversation, run by its harness (`Adapter::side`), asked from the reply box with `/side` and shown above it, or popped out into a window of its own (kept.rs); what was asked and answered, kept to read later (`sides.json`) |
| `src-tauri/src/suggestions.rs`, `src/ui/SuggestedReplies.tsx` | Suggested replies: what another session suggested a session's user send it (`codealong session suggest`, `suggestions.json`), kept until the session moves on (`board::suggestions_hold`), and the list above the reply box that sends one with a click; for questions a session asks, choices of their own picked in its question box (`suggestions::picked`) |
| `src-tauri/src/repos.rs`, `git.rs`, `github.rs`, `ghpoll.rs` | Repo discovery, callsigns, worktrees, git status, PRs, and when GitHub is asked for them |
| `src-tauri/src/held.rs` | The repo scan's reads of folders it doesn't own, each on a thread of its own, so one macOS holds for a privacy prompt holds up only that folder |
| `src-tauri/src/launch.rs`, `launch/iterm.rs`, `launch/terminal_app.rs`, `launch/linux.rs` | Terminal profiles: Ghostty, iTerm2 and Terminal through AppleScript (a session's tab found exactly, typing, closing), on Linux tmux shown in the desktop's emulator, deep links |
| `src-tauri/src/editors.rs`, `src/editors.ts` | The editors a folder opens in: the ones Codealong knows, found where Mac apps are installed, and the Open in menus |
| `src-tauri/src/flash.rs` | Outlines a focused terminal that came forward on another screen |
| `src-tauri/src/caret.rs`, `src/caret.ts` | A text cursor WebKit stopped drawing in a window you type in: typing into a page that says it isn't focused hands it its focus again, and the perf log says what macOS said of the window |
| `src-tauri/src/previews.rs`, `src-tauri/src/chrome.rs`, `src/ui/PreviewBits.tsx` | Previews: each worktree's URL and Chrome profile (`previews.json`), the servers listening in worktrees' folders (`procs::listeners`, the kernel's socket lists), their front pages read once, a kqueue for their exits; Chrome's profiles and opening a page in one; the row's chip, the panel's section and the URL sheet |
| `src/ui/ContextMeter.tsx` | How full a session's context window is, in its reply box's foot (`Session.context`, from each harness's latest call), and on hover what the session has cost |
| `src/ui/UsageDialog.tsx`, `src/spend.ts` | The Usage window (View › Usage…, the usage bar): what sessions used and cost from the token ledger, its tiles, charts and table, and the words and sums for them (pure, tested) |
| `src/ui/JevUsage.tsx`, `src/jevUsage.ts` | Settings › Jev's usage: what Codealong's requests to Jev used and cost today or over the last days, by the feature that asked, and the sums and words for it (pure, tested) |
| `src-tauri/src/idle.rs`, `src/ui/CloseIdleDialog.tsx` | Close idle sessions: which sessions can close or only be archived, and why (pure, tested), and closing and archiving them |
| `src-tauri/src/cleanup.rs`, `claude/teardown.rs`, `src/ui/CleanUpDialog.tsx` | Clean up worktree: whether one can go with nothing in it lost (`board::cleanup`, pure, tested, on each lane), checked again against git as it goes; the repo's own `WorktreeRemove` hook or git removing it, then `claude rm` for a background job that held it; Mark as done on such a job; the dialog that asks first |
| `src-tauri/src/start.rs`, `src/start.ts` | New sessions: where each account can start one, the launch it makes, the agent menu (every account on every model, and the effort levels) and Auto's pick; and going on with one that isn't running, by the same rules (`resume`) |
| `src-tauri/src/quick.rs`, `src/quick.ts`, `src/ui/Quick.tsx` | The quick panel: its window (a panel that takes the keyboard over any app), its shortcut and the waiting window's, and its page (what needs you, finding sessions, starting one) |
| `src-tauri/src/waiting.rs`, `src/ui/Waiting.tsx` | The waiting window: an ordinary window, made when first asked for and hidden on close, that keeps where it was, comes from ⌥⌘., Window › Waiting on you and ⇥ in the quick panel, and works through what waits on you one card at a time (`quick.ts` `queueRows`, `queue.ts`) |
| `src/ui/StartControls.tsx`, `src/startAccounts.ts` | Where a session goes and on what, as the start row and a stopped session's box both draw it (the places, a model menu's lines, the effort row), and this Mac's accounts as they offer them, read once a minute |
| `src-tauri/src/harness.rs` | The interface each harness implements: an `Adapter` for a session (its record and conversation, what it waits on you to allow, opening, sending, starting, searching, renaming with `src/ui/RenameDialog.tsx`) and for an account, and a `Reader` that reads its sessions for the board; `.claude/rules/adding-a-harness.md` lists it all |
| `src-tauri/src/transcript.rs`, `src/ui/SessionPanel.tsx`, `src/ui/SessionHeader.tsx`, `src/sessionContext.ts`, `src/ui/SessionThread.tsx`, `src/ui/SessionReply.tsx`, `src/ui/SessionAgent.tsx`, `src/ui/ModelMenu.tsx` | A session's conversation, read a page at a time from its own record, the panel that shows it, and the box that sends it messages or answers what it waits on, with its account, model and effort as one chip, and for one that isn't running, where it goes on and on what |
| `src-tauri/src/peek.rs`, `src/ui/Peek.tsx` | The floating bar's peek: its window (a panel under the bar that takes the keyboard only on a click), where it opens for a pill and when it closes. What it shows is the session's card (`ui/SessionCard.tsx`), as the waiting window does |
| `src/ui/Notch.tsx`, `src/ui/NotchSession.tsx`, `src/ui/NotchReview.tsx` | The bar docked in the notch: its three states (closed, the pills, a session or a review under them) shown through one clip, and what it shows in place of a peek: which session or PR, the queue of both it works through and its keys |
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
| `src-tauri/peer/` | codealong-peer, what goes between devices: the protocol (each side proves its device key), link codes and iroh, feeds, the requests and their answers, the worktree and session model, and a device that only watches. The iPhone app ([codealong-ios](https://github.com/codealong/codealong-ios)) builds on it; `design/plans/ios.md` has the plan |
| `src-tauri/src/tailnet.rs`, `devicekey.rs` | Reaching them on your tailnet through Tailscale's command (iroh is in codealong-peer), and this device's key in the keychain |
| `src/menus.ts`, `src/contextMenu.ts` | Right-click menus of sessions, worktrees, PRs and todos, each defined once and the same in every window (each says what it can do); native menus through Tauri |
| `src/devices.ts`, `src/ui/DevicesSection.tsx` | Other devices' worktrees in the views, and Settings › Devices |
| `src/notices.ts`, `src/ui/Notices.tsx` | Notices: what isn't working in the background, the engine's and a window's own, as a chip that can be dismissed |
| `src/tips.ts`, `src/ui/Tip.tsx` | Tips on using Codealong: the one list, each tip with the places it shows and its keys from the chord tables, and the box that shows them where a window waits on what it reads |
| `src/ui/Sidebar.tsx`, `src/savedViews.ts` | The sidebar: the view and what narrows it, open or folded to a rail, and saved views |
| `src-tauri/src/cli.rs`, `server.rs`, `install.rs` | The `codealong` command (clap; JSON or tables; the app over its socket, else the files), the app's socket, and putting the skill and the command where agents find them |
| `src-tauri/src/changes.rs`, `hygiene.rs` | For an orchestrator's loop: what changed on the board since an earlier look, told by difference from what that look saw (`codealong changes`, `changes.json`); and what's left running of old sessions, with the Mac's load, memory and swap, and ending it (`codealong hygiene`) |
| `src-tauri/src/actions.rs` | What Codealong does to a session or a worktree, or asks of a device, one table of requests: done here, or asked of the device it's on, for a window, another device and the `codealong` command alike, with what each may ask for and what each carries |
| `src-tauri/src/snapshot.rs` | `codealong --snapshot`: the engine without a window, its modes in one table, for checking what it shows and does |
| `src-tauri/src/todos.rs`, `src/todos.ts` | Todos: the store and its changes, linking sessions started from a todo; the list's sections, filters and prompt |
| `src-tauri/src/boards.rs`, `src/boards/`, `src/ui/BoardBar.tsx`, `src/ui/BoardDialog.tsx`, `src/ui/RuleFields.tsx` | Boards: named sets of work in a project, each shown as Kanban or Canvas. `boards.json` and every change to it (`BoardOp`, which the engine makes for the window and for `codealong board`), what a rule takes in, who's on a board and which column a card goes in (`rules.ts`), the window's boards with its changes shown at once (`useBoards.ts`), and the main window's answers to `codealong board show\|move\|arrange`, which the engine hands it (`useBoardAsks.ts`); the strip's toggle, and the board, column and rule dialogs |
| `src/canvas/`, `src/ui/CanvasView.tsx`, `src/ui/CanvasCard.tsx` | A board's canvas: its documents (what's on it, where; kept in sync with what the board holds, and between windows; Tidy's layouts, and `codealong board arrange`'s), the WebGL layer (grid, links, zoomed-out blocks) and the HTML cards, notes and frames on top |
| `src-tauri/src/attachments.rs`, `src/attachInput.ts` | Files in descriptions and prompts: pasted, dropped or picked, kept by content, images shown in place, all handed to agents as paths |
| `src-tauri/src/ledger.rs` | The token ledger behind the Usage window: every model call each harness recorded (`Adapter::token_files`, `token_use`, `token_uses_since`), read once per file and then from where it stopped into `ledger.sqlite`, one row per call however many records copy it; and reports of it by day, account, model, folder and session, priced as they're asked, with what was paid (plans a day's share at a time, Claude's extra usage readings, recorded and by-the-token costs); and each request to Jev with its tokens, by the feature that asked, for Settings › Jev |
| `src-tauri/src/prices.rs`, `prices.json` | What a model's tokens cost at API prices, by kind of token, with fast mode, priority and long-prompt prices; `prices.json` is written by `scripts/prices.mjs` from LiteLLM's list, and `[prices]` in config.toml goes over it |
| `src-tauri/src/usage.rs` | Plan usage: what every harness's numbers go into, and each account's state for the Accounts dialog; the numbers are the harnesses' own (Claude through its sign-in and usage endpoint, Codex from its rollouts, none for OpenCode and Antigravity) |
| `src-tauri/src/config.rs`, `state.rs` | `config.toml` (accounts saved in place) and `state.json` (seen, pinned, archived, account folders already read) |
| `src-tauri/src/zoom.rs`, `src/zoom.ts` | Zooming: the steps, the View menu's items, the webview's page zoom on every board window, the CSS zoom a card takes in the windows that float, and the keys that ask for it |
| `src-tauri/src/store.rs` | The small JSON files next to the config, read, locked and written one way: a temporary file of the write's own and a rename, a read-modify-write under the file's `.lock`, and a file that doesn't parse left as it is, with an error |
| `src-tauri/src/reusable.rs`, `src/reusable.ts` | Reusable worktrees: the marks made by hand (`worktrees.json`); `board::reusable`, `reuse_of`, `earlier` and `looks_reusable` have the rules, `git::new_work` reads the reflog, and `src/route.ts` sends Jev's new work to them |
| `src-tauri/src/prlinks.rs` | Sessions tied to PRs other than by branch: what their records show they did to a PR, links made by hand, and the PRs whose primary session is nudged with what happens on them, and who that is when picked (`pr-links.json`) |
| `src/ui/AgentReview.tsx`, `src/prDraft.ts`, `src/agentReview.ts` | The top of a PR's Reviews tab, your review: where the agent's review stands, your draft review on GitHub (read once for every part of a window, its text and comments changed and taken out there, submitted by any verdict, github.rs `your_review`, `review`), what the agent wrote, and the panel's box for a verdict; which tab a PR's details open on |
| `src/ui/PrReviewers.tsx`, `src/prReviews.ts` | The rest of a PR's Reviews tab: everyone on the PR once, by who asked for changes first, each with their verdict and what they wrote (reviews' text, threads on the code with the lines around them, comments), what's still open first; what the tab counts |
| `src-tauri/src/autoreview.rs` | Automatic reviews (`[reviews]`, `[reviews.own]`): which PRs are due a review and when, the session it runs in (for a PR of yours, on another harness than the session that opened it), what it's told (Codealong's instructions or yours), reading the review from its last message, and putting it in a draft review of yours or a comment, or handing it back to the session that opened a PR of yours (`reviews.json`) |
| `src-tauri/src/teamfile.rs`, `src/repoSettings.ts` | Settings for one repo's pull requests: a repo's team settings (`.codealong/config.toml`, read from the branch its PRs merge into as fetched, parsed with what's each person's own refused), your `[repos."github.com/…"]` tables, and how they go together over `[reviews]` and `[pull_requests]` (config.rs `reviews_for`, `pull_requests_for`; mirrored for Settings and the PR windows, with where each value comes from and the team's offer) |
| `src/prLinks.ts`, `src/ui/PrLink.tsx` | Links to PRs in conversations and prompts: the board's PRs by link, and the card a link shows on hover (`pr_card` for a PR the board doesn't list) |
| `src-tauri/src/chrometabs.rs` | The Chrome tabs a Claude Code session drove through Claude in Chrome, read from its transcript; which profile they're in (the extension's storage, then History); bringing its tab forward by AppleScript, or its last page again in that profile |
| `src-tauri/src/detect.rs` | The harness accounts on this device: folders, sign-ins, and the wrappers and aliases that run them |
| `src-tauri/src/os/` | Everything asked of macOS or Linux, each area with both OSes' sides (its `mod.rs` header; the Linux table above) |
| `src-tauri/src/panel.rs`, `os/windows/` | The one AppKit (and on Linux GTK) layer for the windows that float (the bar, its peek, the quick panel, the cards and sessions kept on the desktop, the lines between cards): turning a Tauri window into a non-activating panel and back, what each kind answers about the keyboard, every window's level (`Level`), showing, hiding, taking and handing back the keyboard, frames, screens and the pointer |
| `src-tauri/src/geometry.rs` | Rectangles on the screen (`Rect`, AppKit's y up) and in a window's page (`PageRect`, y down from its window's top) with the flip between them, the screen a thing is on, and keeping a window on it (pure, tested) |
| `src-tauri/src/front.rs` | Bringing the main window forward, asking another app to come forward after a panel opened something in it, and whether an app is the active one |
| `src-tauri/src/bar.rs`, `barfeed.rs` | The bar's window (a panel over every app, floating where you put it or docked in the notch, found from the screen's sides of the menu bar, where it takes clicks only on the shape it shows and has the keyboard only while it shows a session; Window › Floating bar and Bar in the notch) and the small feed of what it shows, sent only when that changes |
| `src-tauri/src/origins.rs`, `src/lineage.ts`, `src-tauri/src/lineage.rs`, `src/ui/DesktopLines.tsx` | Where sessions came from: Codealong's own record of starts and forks (`origins.json`), the line drawn between them on the canvas and between desktop cards, and the clear window under the cards that draws it there |
| `src/ui/card/` | The card the Canvas view, the Kanban view and the desktop share: what it shows, looked up from the board (`data.ts`), its face (`CardFace.tsx`), and the popover a canvas card opens (`CardPopover.tsx`). Each adds only where a card goes and what a click does |
| `src/ui/KanbanView.tsx`, `src/kanban.ts` | A board's Kanban: its columns (boardLists.ts `kanbanColumns`: the stages, or its own) of todo and session cards, and what a card dropped on a column or another card does (`kanbanDrop`, worked out by where the change would land it, `applyKanbanDrop`) |
| `src-tauri/src/desktop.rs`, `src/ui/DesktopPin.tsx` | Cards pinned to the desktop: a window each, made as the bar's is, where a new one goes, `desktop.json`, and what each shows, cut from the board and sent only when it changes; the page draws the card the Canvas view draws (`ui/card/`). `drag_out` follows the pointer for a thing dragged out of a window |
| `src/mark.ts`, `src/ui/MarkLive.tsx` | The mark's motion on the bar: what happened between two feeds (a new session working, a turn ended, something needs you, a message sent), which moment plays for it, and the mark in parts that move |
| `src/ui/useRowDrag.tsx` | Dragging rows in the lists and the quick panel: among the pinned ones (pin, reorder), down to the list (unpin), out of the window (the desktop) |
| `src/` | The React UI, built from the design system's components and tokens; `lanes.ts` groups the board and mirrors the engine's status rules for the age filter; `pins.ts` keeps the pinned lists; `history.ts` the sessions you looked at, for going back and forward; `ahead.ts` shows sessions marked seen, archived, renamed or sent a message at once, in every window, until a board shows it; `prStatus.ts` and `ui/PrPills.tsx` say where a PR stands and which PRs are your move; `bar.ts`, `ui/Bar.tsx` and `ui/Notch.tsx` lay out the bar, floating or in the notch; `ui/Picker.tsx` is the quick panel's filterable menus; `ui/SettingsDialog.tsx` and `ui/AccountsSection.tsx` are Settings |
| `scripts/preview.sh` | Headless preview against real data, for screenshots and scripted checks |
| `src/styles/` | The stylesheets: `app.css` is the index, only `@import`s in cascade order with `tokens.css` first; the files several components share sit beside it (base elements, glyphs, cards, controls, chips, rows, nav, menus, trees, the dialog and panel shells), and each component's own styles next to it (`src/ui/Sidebar.css`). `styles.test.ts` keeps each class in one file, every styled class one the UI renders, and every file in the index once |
| `scripts/preview/style-diff.mjs` | Records every scene's computed styles before and after a change to the stylesheets and compares them |
| `design/tokens.json` | Design tokens; `npm run tokens` regenerates `src/styles/tokens.css` |

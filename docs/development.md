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
- Rust (`rustup`) and Node 20 or later
- `gh` signed in (`gh auth login`) for PRs
- A terminal: Ghostty 1.3+, iTerm2 or Terminal (a new config.toml picks the first of those installed)

## Build and run

```bash
npm install
npm run tauri dev         # the app with hot reload
scripts/relaunch.sh       # build Codealong Dev, then quit it and reopen it from Applications
scripts/catch-up-main.sh  # fast-forward the main checkout to origin/main (relaunch.sh does too)
scripts/release.sh        # the installer for other Macs (design/plans/releases.md)
npm run snapshot          # print the board as a table without opening a window
```

Builds from `relaunch.sh` are Codealong Dev: a light icon and an id of their own, so they never
pass for the Codealong installed in Applications. Each one replaces `Codealong Dev.app` in
/Applications (~/Applications when that isn't writable), so Spotlight opens the latest build.
Both use your settings and todos; whichever you open last quits the other, and the `codealong`
command follows it. A Dock pin from when builds ran from the build folder is pointed at
Applications, so the Dock doesn't show the app as a question mark.

Builds go to `codealong-target` next to the main checkout, one directory shared by every
checkout and worktree (`.cargo/config.toml`); `scripts/target-dir.sh` prints it and has
worktrees inside the checkout build there too.

## Tests and checks

```bash
npm run typecheck                        # tsc
npm run lint                             # eslint
npm test                                 # vitest
(cd src-tauri && cargo test)             # the engine and codealong-peer
scripts/preview.sh [view] [project]      # render the UI in headless Chrome against your real data
scripts/checks.sh                        # every browser check in scripts/preview/
```

`cargo test` covers the model's rules, the harnesses' parsers, git and GitHub parsing, usage,
config writes, deep links and terminal scripts. `scripts/preview.sh` renders the UI with Tauri's
IPC stubbed, for screenshots and scripted checks; its header lists the views and options.
`codealong --snapshot` checks the engine's behavior without a window (CLAUDE.md lists its modes).

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
since Claude Code drops a plain allow for it.

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
| `src-tauri/src/claude/` | Claude Code and the Claude app's Code sessions, a file a subject: the adapter and reader (`mod.rs`), the registry and the background service's records, transcripts and the conversation one holds, permission modes, the requests the plugin's hook hands the app (`asks.rs`), the app's records, the ways a prompt gets in (the background service's socket, a session's own, the app), starting and forking, names, side conversations, what its Bash tool is running, folder trust and plan usage |
| `src-tauri/src/codex/` | Codex, in its app and its terminal UI: the adapter and reader (`mod.rs`), rollouts and the thread index, the conversation a rollout holds, its app-server (archiving, renaming, new threads), the socket the app's windows talk on (starting, steering and stopping a turn), its terminal UIs, side conversations and plan usage |
| `src-tauri/src/opencode.rs`, `antigravity.rs`, `cursor.rs` | Harness adapters: OpenCode's database and background service; Antigravity's app and command line (`agy`), their conversation summaries, transcripts and processes; Cursor's app (its chats in its state database) and command line (`cursor-agent`), their transcripts and processes |
| `src-tauri/src/acp.rs` | Sessions Codealong runs itself: an Agent Client Protocol adapter process per account, turns, permission requests, the streamed reply |
| `src-tauri/src/side.rs`, `src/side.ts`, `src/ui/SideStrip.tsx` | Side conversations: a throwaway copy of a session's conversation, run by its harness (`Adapter::side`), asked from the reply box with `/side` and shown above it, or popped out into a window of its own (kept.rs); what was asked and answered, kept to read later (`sides.json`) |
| `src-tauri/src/suggestions.rs`, `src/ui/SuggestedReplies.tsx` | Suggested replies: what another session suggested a session's user send it (`codealong session suggest`, `suggestions.json`), kept until the session moves on (`board::suggestions_hold`), and the list above the reply box that sends one with a click; for questions a session asks, choices of their own picked in its question box (`suggestions::picked`) |
| `src-tauri/src/repos.rs`, `git.rs`, `github.rs`, `ghpoll.rs` | Repo discovery, callsigns, worktrees, git status, PRs, and when GitHub is asked for them |
| `src-tauri/src/held.rs` | The repo scan's reads of folders it doesn't own, each on a thread of its own, so one macOS holds for a privacy prompt holds up only that folder |
| `src-tauri/src/launch.rs`, `launch/iterm.rs`, `launch/terminal_app.rs` | Terminal profiles: Ghostty, iTerm2 and Terminal through AppleScript (a session's tab found exactly, typing, closing), deep links |
| `src-tauri/src/editors.rs`, `src/editors.ts` | The editors a folder opens in: the ones Codealong knows, found where Mac apps are installed, and the Open in menus |
| `src-tauri/src/flash.rs` | Outlines a focused terminal that came forward on another screen |
| `src-tauri/src/caret.rs`, `src/caret.ts` | A text cursor WebKit stopped drawing in a window you type in: typing into a page that says it isn't focused hands it its focus again, and the perf log says what macOS said of the window |
| `src-tauri/src/previews.rs`, `src-tauri/src/chrome.rs`, `src/ui/PreviewBits.tsx` | Previews: each worktree's URL and Chrome profile (`previews.json`), the servers listening in worktrees' folders (`procs::listeners`, the kernel's socket lists), their front pages read once, a kqueue for their exits; Chrome's profiles and opening a page in one; the row's chip, the panel's section and the URL sheet |
| `src-tauri/src/idle.rs`, `src/ui/CloseIdleDialog.tsx` | Close idle sessions: which sessions can close or only be archived, and why (pure, tested), and closing and archiving them |
| `src-tauri/src/start.rs`, `src/start.ts` | New sessions: where each account can start one, the launch it makes, the agent menu (every account on every model, and the effort levels) and Auto's pick; and going on with one that isn't running, by the same rules (`resume`) |
| `src-tauri/src/quick.rs`, `src/quick.ts`, `src/ui/Quick.tsx` | The quick panel: its window (a panel that takes the keyboard over any app), its shortcut and the waiting window's, and its page (what needs you, finding sessions, starting one) |
| `src-tauri/src/waiting.rs`, `src/ui/Waiting.tsx` | The waiting window: an ordinary window, made when first asked for and hidden on close, that keeps where it was, comes from ⌥⌘., Window › Waiting on you and ⇥ in the quick panel, and works through what waits on you one card at a time (`quick.ts` `queueRows`, `queue.ts`) |
| `src/ui/StartControls.tsx`, `src/startAccounts.ts` | Where a session goes and on what, as the start row and a stopped session's box both draw it (the places, a model menu's lines, the effort row), and this Mac's accounts as they offer them, read once a minute |
| `src-tauri/src/harness.rs` | The interface each harness implements: an `Adapter` for a session (its record and conversation, what it waits on you to allow, opening, sending, starting, searching, renaming with `src/ui/RenameDialog.tsx`) and for an account, and a `Reader` that reads its sessions for the board; `.claude/rules/adding-a-harness.md` lists it all |
| `src-tauri/src/transcript.rs`, `src/ui/SessionPanel.tsx`, `src/ui/SessionHeader.tsx`, `src/sessionContext.ts`, `src/ui/SessionThread.tsx`, `src/ui/SessionReply.tsx`, `src/ui/ResumeBar.tsx`, `src/ui/SessionModel.tsx`, `src/ui/ModelMenu.tsx` | A session's conversation, read a page at a time from its own record, the panel that shows it, and the box that sends it messages or answers what it waits on, with the model it's on, and for one that isn't running, where it goes on and on what |
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
| `src-tauri/src/runcode.rs`, `src/ui/RunButton.tsx` | Run on a code block: the lines of a shell block to type into a new terminal (comments out, here-documents and quoted text whole), the commands on the PATH that make a block with no language one, and the button |
| `src-tauri/src/peers.rs`, `peerfeed.rs` | Your other devices: links, connections, and the feed of worktrees and sessions sent a change at a time; `design/plans/devices.md` has the reasoning |
| `src-tauri/peer/` | codealong-peer, what goes between devices: the protocol (each side proves its device key), link codes and iroh, feeds, the requests and their answers, the worktree and session model, and a device that only watches. The iPhone app ([codealong-ios](https://github.com/codealong/codealong-ios)) builds on it; `design/plans/ios.md` has the plan |
| `src-tauri/src/tailnet.rs`, `devicekey.rs` | Reaching them on your tailnet through Tailscale's command (iroh is in codealong-peer), and this device's key in the keychain |
| `src/menus.ts`, `src/contextMenu.ts` | Right-click menus of sessions, worktrees, PRs and todos, each defined once and the same in every window (each says what it can do); native menus through Tauri |
| `src/devices.ts`, `src/ui/DevicesSection.tsx` | Other devices' worktrees in the views, and Settings › Devices |
| `src/notices.ts`, `src/ui/Notices.tsx` | Notices: what isn't working in the background, the engine's and a window's own, as a chip that can be dismissed |
| `src/tips.ts`, `src/ui/Tip.tsx` | Tips on using Codealong: the one list, each tip with the places it shows and its keys from the chord tables, and the box that shows them where a window waits on what it reads |
| `src/ui/Sidebar.tsx`, `src/savedViews.ts` | The sidebar: the view and what narrows it, open or folded to a rail, and saved views |
| `src-tauri/src/cli.rs`, `server.rs`, `install.rs` | The `codealong` command (clap; JSON or tables; the app over its socket, else the files), the app's socket, and putting the skill and the command where agents find them |
| `src-tauri/src/actions.rs` | What Codealong does to a session or a worktree, or asks of a device, one table of requests: done here, or asked of the device it's on, for a window, another device and the `codealong` command alike, with what each may ask for and what each carries |
| `src-tauri/src/snapshot.rs` | `codealong --snapshot`: the engine without a window, its modes in one table, for checking what it shows and does |
| `src-tauri/src/todos.rs`, `src/todos.ts` | Todos: the store and its changes, linking sessions started from a todo; the list's sections, filters and prompt |
| `src-tauri/src/canvas.rs`, `src/canvas/`, `src/ui/CanvasView.tsx`, `src/ui/CanvasCard.tsx` | The Canvas view: its documents (what's on it, where; kept in sync with the board; Tidy's layouts), the WebGL layer (grid, links, zoomed-out blocks) and the HTML cards, notes and frames on top |
| `src-tauri/src/attachments.rs`, `src/attachInput.ts` | Files in descriptions and prompts: pasted, dropped or picked, kept by content, images shown in place, all handed to agents as paths |
| `src-tauri/src/usage.rs` | Plan usage: what every harness's numbers go into, and each account's state for the Accounts dialog; the numbers are the harnesses' own (Claude through its sign-in and usage endpoint, Codex from its rollouts, none for OpenCode and Antigravity) |
| `src-tauri/src/config.rs`, `state.rs` | `config.toml` (accounts saved in place) and `state.json` (seen, pinned, archived, account folders already read) |
| `src-tauri/src/zoom.rs`, `src/zoom.ts` | Zooming: the steps, the View menu's items, the webview's page zoom on every board window, the CSS zoom a card takes in the windows that float, and the keys that ask for it |
| `src-tauri/src/store.rs` | The small JSON files next to the config, read, locked and written one way: a temporary file of the write's own and a rename, a read-modify-write under the file's `.lock`, and a file that doesn't parse left as it is, with an error |
| `src-tauri/src/reusable.rs`, `src/reusable.ts` | Reusable worktrees: the marks made by hand (`worktrees.json`); `board::reusable`, `reuse_of`, `earlier` and `looks_reusable` have the rules, `git::new_work` reads the reflog, and `src/route.ts` sends Jev's new work to them |
| `src-tauri/src/prlinks.rs` | Sessions tied to PRs other than by branch: what their records show they did to a PR, links made by hand, and the PRs whose primary session is nudged with what happens on them, and who that is when picked (`pr-links.json`) |
| `src/ui/AgentReview.tsx`, `src/prDraft.ts`, `src/agentReview.ts` | A PR's Review tab: where the agent's review stands, your draft review on GitHub (read once for every part of a window, its text and comments changed and taken out there, submitted by any verdict, github.rs `your_review`, `review`), what the agent wrote, and the panel's box for a verdict; which tab a PR's details open on |
| `src-tauri/src/autoreview.rs` | Automatic reviews (`[reviews]`, `[reviews.own]`): which PRs are due a review and when, the session it runs in (for a PR of yours, on another harness than the session that opened it), what it's told (Codealong's instructions or yours), reading the review from its last message, and putting it in a draft review of yours or a comment, or handing it back to the session that opened a PR of yours (`reviews.json`) |
| `src-tauri/src/teamfile.rs`, `src/repoSettings.ts` | Settings for one repo's pull requests: a repo's team settings (`.codealong/config.toml`, read from the branch its PRs merge into as fetched, parsed with what's each person's own refused), your `[repos."github.com/…"]` tables, and how they go together over `[reviews]` and `[pull_requests]` (config.rs `reviews_for`, `pull_requests_for`; mirrored for Settings and the PR windows, with where each value comes from and the team's offer) |
| `src/prLinks.ts`, `src/ui/PrLink.tsx` | Links to PRs in conversations and prompts: the board's PRs by link, and the card a link shows on hover (`pr_card` for a PR the board doesn't list) |
| `src-tauri/src/chrometabs.rs` | The Chrome tabs a Claude Code session drove through Claude in Chrome, read from its transcript; which profile they're in (the extension's storage, then History); bringing its tab forward by AppleScript, or its last page again in that profile |
| `src-tauri/src/detect.rs` | The harness accounts on this device: folders, sign-ins, and the wrappers and aliases that run them |
| `src-tauri/src/panel.rs` | The one AppKit layer for the windows that float (the bar, its peek, the quick panel, the cards and sessions kept on the desktop, the lines between cards): turning a Tauri window into a non-activating panel and back, what each kind answers about the keyboard, every window's level (`Level`), showing, hiding, taking and handing back the keyboard, frames, screens and the pointer |
| `src-tauri/src/geometry.rs` | Rectangles on the screen (`Rect`, AppKit's y up) and in a window's page (`PageRect`, y down from its window's top) with the flip between them, the screen a thing is on, and keeping a window on it (pure, tested) |
| `src-tauri/src/front.rs` | Bringing the main window forward, asking another app to come forward after a panel opened something in it, and whether an app is the active one |
| `src-tauri/src/bar.rs`, `barfeed.rs` | The bar's window (a panel over every app, floating where you put it or docked in the notch, found from the screen's sides of the menu bar, where it takes clicks only on the shape it shows and has the keyboard only while it shows a session; Window › Floating bar and Bar in the notch) and the small feed of what it shows, sent only when that changes |
| `src-tauri/src/origins.rs`, `src/lineage.ts`, `src-tauri/src/lineage.rs`, `src/ui/DesktopLines.tsx` | Where sessions came from: Codealong's own record of starts and forks (`origins.json`), the line drawn between them on the canvas and between desktop cards, and the clear window under the cards that draws it there |
| `src/ui/card/` | The card the Canvas view and the desktop share: what it shows, looked up from the board (`data.ts`), its face (`CardFace.tsx`), and the popover a canvas card opens (`CardPopover.tsx`). The canvas and the desktop add only where a card goes and what a click does |
| `src-tauri/src/desktop.rs`, `src/ui/DesktopPin.tsx` | Cards pinned to the desktop: a window each, made as the bar's is, where a new one goes, `desktop.json`, and what each shows, cut from the board and sent only when it changes; the page draws the card the Canvas view draws (`ui/card/`). `drag_out` follows the pointer for a thing dragged out of a window |
| `src/mark.ts`, `src/ui/MarkLive.tsx` | The mark's motion on the bar: what happened between two feeds (a new session working, a turn ended, something needs you, a message sent), which moment plays for it, and the mark in parts that move |
| `src/ui/useRowDrag.tsx` | Dragging rows in the lists and the quick panel: among the pinned ones (pin, reorder), down to the list (unpin), out of the window (the desktop) |
| `src/` | The React UI, built from the design system's components and tokens; `lanes.ts` groups the board and mirrors the engine's status rules for the age filter; `pins.ts` keeps the pinned lists; `history.ts` the sessions you looked at, for going back and forward; `ahead.ts` shows sessions marked seen, archived, renamed or sent a message at once, in every window, until a board shows it; `prStatus.ts` and `ui/PrPills.tsx` say where a PR stands and which PRs are your move; `bar.ts`, `ui/Bar.tsx` and `ui/Notch.tsx` lay out the bar, floating or in the notch; `ui/Picker.tsx` is the quick panel's filterable menus; `ui/SettingsDialog.tsx` and `ui/AccountsSection.tsx` are Settings |
| `scripts/preview.sh` | Headless preview against real data, for screenshots and scripted checks |
| `src/styles/` | The stylesheets: `app.css` is the index, only `@import`s in cascade order with `tokens.css` first; the files several components share sit beside it (base elements, glyphs, cards, controls, chips, rows, nav, menus, trees, the dialog and panel shells), and each component's own styles next to it (`src/ui/Sidebar.css`). `styles.test.ts` keeps each class in one file, every styled class one the UI renders, and every file in the index once |
| `scripts/preview/style-diff.mjs` | Records every scene's computed styles before and after a change to the stylesheets and compares them |
| `design/tokens.json` | Design tokens; `npm run tokens` regenerates `src/styles/tokens.css` |

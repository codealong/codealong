# Codealong

A Mac app (Tauri 2: Rust engine, React UI) that tracks every coding agent running on this Mac,
across Claude Code accounts (`~/.claude`, `~/.claude2`), Codex, OpenCode and Antigravity, organized by project →
repo → worktree → branch → PR. It shows what each agent is doing, opens any session where it
lives, and shows plan usage. README.md has the feature list, settings and code map. The design
system (tokens, components, the model and build plan) is the artifact
https://claude.ai/artifact/Eg7p4HkWgTVUR54RfYgUPc; the app has moved past it in places.

Codealong is meant for other people too: nothing in it may assume the user's own projects,
repos or naming. Behavior comes from what the harnesses record and from `config.toml`.

## Commands

```bash
npm run typecheck                       # tsc
npm test                                # vitest (format, lanes, pins, sections)
(cd src-tauri && cargo test)            # engine rules, parsers, usage, config, launch scripts; codealong-peer's too
(cd src-tauri && cargo build --release) # the engine binary; cargo test does NOT rebuild it
scripts/relaunch.sh                     # builds Codealong Dev, then quits and reopens it
scripts/release.sh                      # the installer for other Macs: universal, Developer ID, notarized
```

Every checkout and worktree builds into one directory, `codealong-target` next to the main
checkout (`.cargo/config.toml`), so dependencies compile once and concurrent builds queue on
cargo's lock instead of fighting for cores. `scripts/target-dir.sh` prints it; the engine binary
is `<dir>/release/codealong` and the bundle `<dir>/release/bundle/macos/Codealong Dev.app`. Cargo
resolves `../codealong-target` against the checkout it runs in, so a worktree inside another
(Claude Code's `.claude/worktrees/<name>`) gets `.claude/worktrees/codealong-target`: the
scripts that build pass the shared directory to cargo as `CARGO_TARGET_DIR`, and
`scripts/target-dir.sh` makes that folder a link to it, for cargo run by hand. It only reports
a real folder already there; remove that once nothing builds in it. The workspace's own crates
(codealong, codealong-peer) build to the same files from every checkout too, and cargo goes by
file times: once another session has built, a `cargo test` in your worktree can link its copy of
them instead of yours (errors about what you just added are the sign). `touch
src-tauri/src/lib.rs src-tauri/peer/src/lib.rs` before cargo then; `relaunch.sh` always does. Tests
run in the debug profile: after the first build they rebuild in seconds, and the release
profile is tuned for rebuild speed too (no LTO). Hand a change to the user with
`scripts/relaunch.sh`: it waits for any bundle build in progress, builds, and only then quits
the running app and opens the new one. Builds take turns in no set order, so when its turn
comes it fast-forwards a clean checkout that `origin/main` has moved past: an older build never
replaces a newer one. Its last line says which commit it launched and how that stands against
`origin/main`. Don't `pkill` the app by hand; a pattern that contains
the app's path also matches other sessions' shells, and kills their builds.

What `relaunch.sh` builds is Codealong Dev (`src-tauri/tauri.dev.conf.json`: the id
`app.codealong.desktop.dev`, its own name and a light icon, in the Dock too), so a build of this
repo never passes for Codealong, the app `scripts/release.sh` builds and people install in
Applications. They use the same data folder (config.toml, todos, the socket), so one runs at a
time: a copy opened the usual way asks the running one to quit (`server::take_over`), and the
`codealong` command's link follows the copy that opened (`install::follow_command_link`).

Don't open a second copy of the app where the user can see it. Opened the usual way it quits
theirs; a bare binary from a build folder shows up beside it as a second Codealong. Check UI with
`scripts/preview.sh` and behavior with `--snapshot`. A copy you must run to debug gets a home of
its own (`HOME=<your scratchpad>/home …`: its own settings, todos and socket, so it leaves the
user's app alone), you say so, and you quit it when you're done. Its own home doesn't keep it off
the screen: its bar and any cards it pins float over every app as the real ones do, and the user
takes them for their own app misbehaving (a drag test's cards "one" and "three", "No longer on
the board", 2026-09-29).

The engine binary is also the `codealong` command for agents and people (`cli.rs`; `main.rs`
runs it for any first argument that's one of its subcommands, and starts the app otherwise):
`status`, `accounts [--usage]`, `whoami`, `todo list|show|add|update|move|start|done|reopen|
wait|ready|link|unlink|delete|labels`, `sessions [find <words>]`, `session show|open|send`, `worktrees`, `reuse`, `start`,
`config show|check`, `install`. Settings are changed by editing config.toml, not through the
command: agents edit the file well and keep its comments, and `config check` runs
`config::parse`'s checks on the result. JSON when piped, tables in a terminal (`--table` forces them). With the app up it
asks the app over `codealong.sock` (`server.rs`: JSON lines, `status`, `board`, `todo-apply`,
`todo-started`, `start`, `show`); otherwise `engine::light_snapshot` and `todos::change` (under
`todos.json.lock`, which the engine takes for its own todo changes too). After changing
`cli.rs`, run `scripts/skill-reference.sh`: it rewrites
`skills/codealong/references/reference.md` from `--help`, and a cargo test fails while that
file is stale. `codealong reference` (hidden) prints the same text. A debug binary run with a
subcommand `main.rs` doesn't know starts the app headless and takes the socket; kill it by pid.

The engine binary doubles as a CLI for checking behavior without a window:

- `codealong --snapshot [--table] [--no-github] [--usage]`: the board as JSON (or a table).
  Claude usage is read only with `--usage`; the usage endpoint rate-limits (see below).
- `codealong --snapshot --accounts`: configured accounts and sign-in state.
- `codealong --snapshot --found`: accounts on this Mac that config.toml doesn't list
  (`detect.rs`); `cargo test this_macs_accounts -- --ignored --nocapture` prints all of them.
- `codealong --snapshot --config`: the settings and config.toml as Settings gets them (parsed
  config, the file's text, its parse error if any, the app profiles).
- `codealong --snapshot --open <lane id> <target id>`: runs a click's open action.
- `codealong --snapshot --pr <owner/name> <number>`: a PR's detail, as the PR panel gets it.
- `codealong --snapshot --pr-card <owner/name> <number>`: a PR the board doesn't list, as a link
  to it in a conversation gets it on hover (`pr_card`: GitHub's facts, and the worktrees and
  sessions here that worked on it).
- `codealong --snapshot --route <text> [--project <name>]`: what Jev suggests for text typed in
  the quick panel (one TypeSafe call; needs the key).
- `codealong --snapshot --transcript <session key> [--before <cursor>]`: a page of a session's
  conversation (prompts, replies, tool calls and their output) read from its own record, as its
  panel gets it (`transcript.rs`).
- `codealong --snapshot --rename <session key> <name>`: names a session where its harness keeps
  names, as Rename… in its menu does (`Adapter::rename`). It renames it for real: use a scratch
  session, or a `HOME` of your own with scratch accounts.
- `codealong --snapshot --stop <session key>`: stops the turn a session is taking, as its Stop
  does (`Adapter::interrupt`). Real: use it only on a session of your own.
- `codealong --snapshot --fork <session key> [prompt]`: forks a session into the background, as
  `/fork` in its box does (`Adapter::fork_background`). Real: it starts a copy (a Claude Code
  background job, `claude rm <job>` after; an OpenCode session). Codex's copy runs over ACP,
  which needs the app.
- `codealong --snapshot --side <session key> <question> [a follow-up…]`: asks a throwaway copy
  of a session, as `/side` in its box does (side.rs, `Adapter::side`), and prints the questions
  with their answers. Real: the copy may do what its session may without asking, so a question
  that asks for a change gets it made. Nothing of it is kept: a Claude Code copy writes no
  transcript, a Codex one no rollout, and only the app keeps what was said (`sides.json`). It
  spends the account's usage, so use a scratch session.
- `codealong --snapshot --editor-icons`: the installed editors' app icons, as the Open in menus
  get them (`editors.rs`).
- `codealong --snapshot --start-options`: where each account can start a session.
- `codealong --snapshot --start-plan <folder> <account> <terminal|background|claude-app|codex-app> [prompt]
  [--model <model>] [--effort <level>] [--worktree]`: the command or link starting a session would use, without
  starting one.

## Checking UI changes

`scripts/preview.sh [view] [project] [check.js]` builds the UI, snapshots real data, stubs
Tauri's IPC with `scripts/preview/fake-tauri.js`, and renders in headless Chrome: a screenshot
in `.preview/<view>.png`, or with `check.js`, whatever the script puts in `document.title`.
The stub records every command in `window.__calls`, so a check can click, double-click or
right-click and assert what the app would have sent. `PREVIEW_SCHEME=dark` renders as if macOS
were in dark mode; check UI changes in both. `PREVIEW_INIT=<file.js>` runs a script before the
screenshot, for a state a click gets to (Settings opens on a synthetic ⌘, keydown), and
`PREVIEW_NAME` names that screenshot. The app rewrites `document.title` as counts change, so a
check that runs for a while should set its title on an interval. Native menus, window creation and real opens
can't be seen this way; say so rather than claiming they were checked.
`scripts/preview.sh quick` renders the quick panel as its shortcut opens it (a `PREVIEW_INIT`
can type into it or click its menus open).
`scripts/preview.sh lineage` renders the lines between desktop cards (lineage.rs) over stand-ins
for the cards, from `scripts/preview/lineage-desk.js`. `scripts/preview.sh bar` renders the floating bar on a stand-in desktop (`PREVIEW_SIZE=1600,64` for
a wider screen), from the feed `--snapshot --bar` prints, and `scripts/preview.sh notch` the bar docked
in the notch, closed (`PREVIEW_SIZE=1512,720 PREVIEW_INIT=scripts/preview/notch-session-init.js` opens it
on a session; `notch-session-check.js` checks its clicks and keys). The engine the preview runs is whatever
was last built into the shared directory, possibly another session's: `PREVIEW_BIN=<a copy of
your build>` uses yours. `PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh todo` shows
sample todos linked to your real worktrees and sessions in place of your own, and
`PREVIEW_USAGE=scripts/preview/usage-sample.json` sample plan usage in place of the snapshot's,
which has no Claude numbers, and `PREVIEW_TRANSCRIPT=<session key>` that
session's real conversation in its panel (others get placeholders), and `PREVIEW_FOUND=scripts/preview/found-sample.json` sample accounts
under Settings › Accounts › Found on this Mac, and `PREVIEW_DEVICES=scripts/preview/devices-sample.json` other devices
(Settings › Devices, the sidebar's devices, a link ask, and a few of your worktrees shown as if on those devices), and `PREVIEW_CANVAS=<file>` opens the Canvas view on a saved
document (one project's from canvas.json) in place of a fresh tidy, and `PREVIEW_LOADING=1` shows the windows as
they are at launch, before the engine's first scan has finished. In check mode Chrome runs on virtual time, which stands still while scripts run (`performance.now()` too), and draws no frames, so scroll events never
arrive; and it resizes the page just before a screenshot, which closes whatever closes on resize.
`PREVIEW_WAIT=<ms>` runs a check on the real clock with frames instead, and prints its title once
it starts with `done: ` (or the wait runs out).

## Checking performance

`[debug] perf_log = true` in config.toml (no restart needed) makes the engine and every window
append timings to `~/Library/Logs/Codealong/perf.log`, each line stamped with the time and the
load average: every publish (size, how many lanes, sessions and PRs changed, compose and emit
time, file-watcher batches and git runs since the last one), git runs, engine and main-thread
lag, and per window the push-to-paint time, how long pointer events waited in the queue, and
event-loop stalls. `perf.rs` has the details. Set it back to false when done; the log grows.

A main thread still stuck a second after the watcher's probe is sampled while it's stuck
(`/usr/bin/sample`, two seconds, at most every five minutes, and again at ten seconds): the log
says where the main thread was, innermost frame first ("waiting in its event loop" means none
of Codealong's, AppKit's or WebKit's code was running), and `stalls/` beside the log keeps the
last 20 reports. The release build is stripped, so Codealong's own frames are offsets; a build
of the same commit with `CARGO_PROFILE_RELEASE_STRIP=false` names them (`atos -o <binary> -l
<load address> <address>`, the load address from the report's header).

`node scripts/perf-summary.mjs` sums up the app's latest run (`--since HH:MM`, `--runs N`):
publishes, engine lag, git runs, file-watcher batches (big ones name the folders they came
from) and each window's push times. A window that can't be seen holds pushes and renders the
latest when it's shown, so its push times are for boards someone saw.

A push's cost in the window is measured with `PREVIEW_WAIT=90000 scripts/preview.sh <view> ''
scripts/preview/push-bench.js`: forty pushes of the real board with one worktree changed, as
most publishes are. Rows of the board and the sidebar skip rendering when nothing they show
changed (`reuse.ts` keeps unchanged parts of a push as the same objects, `ui/memoRow.ts`
compares rows). `PREVIEW_WAIT=120000 scripts/preview.sh worktrees '' scripts/preview/rows-check.js`
(and `sessions`) checks that a session's row still shows what a push changes while the other
worktrees' rows skip rendering (it counts renders through React's devtools hook).

The quick panel's open, keys and pushes are timed with `PREVIEW_WAIT=120000 scripts/preview.sh
quick '' scripts/preview/quick-bench.js`. Chrome doesn't drop a hidden page to background
priority as WebKit does (Tauri and macOS, below), so an open slowed by that looks fast there.

Git is the engine's biggest cost and starts during load by construction, so `throttle.rs`
spaces runs in one worktree two seconds apart, publishes git-only changes in two-second
batches (session changes still go out at once), runs git at nice 10, and puts the engine and
git threads in the utility QoS class. At launch, worktrees with the latest activity go first.

Done means: typecheck and tests pass, the engine builds without warnings, UI changes are
previewed, and the work has landed (next section).

## Landing work

Work has landed when it's in `origin/main` (github.com/codealong/codealong, private), the main
checkout has it, and the app that's running was built from it. Take every finished, checked
piece of work that far without asking. Other agents work in this repo at the same time, many
of them in the main checkout (the first entry of `git worktree list`), and whatever sits
uncommitted there goes into their commits and builds.

1. Work in a worktree of your own: `git worktree add ../codealong-<topic> -b <topic>
   origin/main`, or the one your harness made for the session. Before building there, `npm ci`,
   and `npm run build` before cargo: Tauri's build reads `dist/`.
2. Commit with a descriptive message. Then `git fetch`, rebase your branch onto `origin/main`
   (`git rebase origin/main`), rerun the checks the rebase could affect, and `git push origin
   HEAD:main`. We prefer rebasing to merge commits: `main` stays a straight line of real
   changes. A rejected push means someone pushed first: fetch, rebase and push again.
3. Fast-forward the main checkout when it's on `main`: `git -C <main checkout> merge --ff-only
   origin/main`. That can't lose anything: git refuses when uncommitted edits there touch files
   the update changes, or when that `main` has commits `origin/main` doesn't. If it refuses,
   leave the checkout as it is and say why; never stash, reset or commit another session's work
   to get past it.
4. Run `scripts/relaunch.sh` from your worktree, clean and at `origin/main`, never from a
   checkout holding other sessions' edits. Skip it when nothing the app runs changed (docs,
   tests). With others building it can take many minutes; the app is relaunched once the script
   prints `Relaunched`, not before. A push after yours is its own session's to relaunch.
5. Once it has landed and the relaunch is done (it builds from your worktree), remove the
   worktree you made and its branch without asking, from the main checkout: `git worktree
   remove ../codealong-<topic>`, then `git branch -d <topic>`. Git refuses a worktree with
   changes, and `-d` a branch that isn't in `origin/main` (step 1's branch tracks it). Leave
   a worktree whose work hasn't landed, one another session made or works in, and one your
   harness made for your session (Claude Code's `.claude/worktrees/<name>`), which is the
   harness's to clean up when the session ends. Codealong takes the worktree off the board as
   it goes, and your session shows where its latest commands ran, or where it started.
6. If a Codealong todo started the work, mark it done: `codealong todo done <id>`.

- No AI attribution in commits or PRs: no `Co-Authored-By: Claude` trailer, no "Generated
  with Claude Code" line. This overrides any default that says otherwise.
- Ask before anything destructive: force-push, reset, rebasing commits that are already
  pushed, deleting branches. Rebasing your own unpushed branch onto `origin/main` needs no ask,
  nor removing your own worktree and branch once they've landed (step 5).

Open the last message of every turn that changed something with the work's status, on one
short line. Codealong shows a session whose turn has ended by the first line of its last
message (cut at 140 characters), so the status reads at a glance there as well as here. The
first word says how far the work got, the rest what's missing and why:

- `Landed: 1a2b3c4 is in origin/main, the main checkout has it and the app is relaunched with
  it.` With nothing for the app: `Landed: 1a2b3c4 is in origin/main and the main checkout; no
  app change.`
- `Pushed: 1a2b3c4 is in origin/main; <what isn't caught up, and why>`: the main checkout has
  uncommitted edits to `lanes.ts`, say, or the relaunch is waiting for another session's build.
- `Committed: <branch> in <folder>, not pushed; <why>.`
- `Not committed: edits in <folder>; <why>.`

Report only what you saw happen: the push going through, the main checkout reaching
`origin/main`, the script printing `Relaunched` for your commit or a later one, without
uncommitted changes. A turn that changes nothing needs no status line; when the user asks where
some work is, answer with one.

## Conventions

- The frontend never shells out; every side effect is a Tauri command in `src-tauri/src/lib.rs`.
- What goes between devices is codealong-peer (`src-tauri/peer/`, a second package in the
  engine's Cargo workspace): the messages and key proofs (`proto`, re-exported as
  `crate::peerproto`), link codes and iroh (`net`, as `crate::irohnet`), feeds (`feed`), the
  connection (`wire`), what devices ask each other for and get back (`api`: the request names,
  `Pending`, `Answer` and `Sent`, which harness.rs and send.rs re-export), a device that only
  watches (`client`), and the worktree and session model their feeds carry (`model`; the
  engine's `model.rs` re-exports it and keeps the board). So a new field on a session or lane
  goes in `peer/src/model.rs`. Phones and tablets are linked devices that only watch
  (`peerproto::watches`): Macs list them apart (`DevicesView.phones`), don't dial them and don't
  ask them to search. The iPhone app (codealong-ios) builds on the crate at a pinned
  `origin/main` commit and talks to Macs of other builds: a message a peer doesn't know is
  skipped now, but ended the connection on builds before 2026-09-28, so new messages go only to
  peers that say they know them.
- `src-tauri/src/board.rs` holds the model's rules as pure functions with tests; each harness's
  reader gathers a session's inputs and calls them, and the engine the rest. `src/lanes.ts` mirrors `bay_of`, the agent bays'
  headlines and the order of sessions and lanes (`sort_sessions`, `lane_prompt_at`) for the
  filters; keep them in step.
- What Codealong does to a session, whichever device it's on, is a request in
  `src-tauri/src/actions.rs` (`REQUESTS`: its name and what it does here), and who may ask for it
  from another device is its `Reach` in codealong-peer's `api::REQUESTS` (read, organize, or
  control, which needs remote control on). A window's command goes through `actions::run`, which
  asks the session's device when it's elsewhere; a device's request through `actions::answer`.
  A new session action is a line in each, not a command with its own remote branch.
- A card is one design wherever it is: the Canvas view's cards and the cards pinned to the
  desktop draw the same face, from `src/ui/card/` (`data.ts`, `CardFace.tsx`; `CardPopover.tsx` is
  the popover a canvas card opens, and the desktop has none: a panel beside a card was tried and
  taken out, 2026-09-30). `CanvasCard.tsx` and `DesktopPin.tsx` add only where a card goes and
  what a click does; a change to what a card shows goes in `card/`, never into one of them.
- A session shown with its conversation is one component wherever it is: `ui/SessionPanel.tsx`
  (the header in `ui/SessionHeader.tsx`, the conversation, the reply box), as a panel in the main
  window and as a card in the bar's peek, under the pills of the bar docked in the notch and in
  the quick panel's reply view. A window says what it can do by the props it passes; a change to
  the header goes in `SessionHeader.tsx` and shows in all of them. `sessionContext.ts` looks up
  what the header says around a session (its PRs, todos and lineage) for the windows without the
  main window's indexes. The windows that show one session as a card (the peek, a session kept on
  the desktop, the notch, the quick panel's reply view) all go through `ui/SessionCard.tsx`, which
  builds the session's menu, the menus of what its header names and its shortcuts once; a window
  passes only what differs (how it reports and opens, what it does once the session has left).
  What a card can do to its session goes there, not into `Peek.tsx`, `NotchSession.tsx` or
  `Quick.tsx`. The ones that aren't the board's (the peek, a kept session, the notch) read their
  session and what's around it through `ui/cardHost.ts`.
- The bar docked in the notch shows one of three things, in one window through one clip
  (`Notch.tsx`, sized by `bar.ts` `notchSizes`): the notch alone, the bar's pills under it (the
  pointer on it, or pinned), and a session under the pills (a click on its pill;
  `NotchSession.tsx`). It has no peek. The session works through what waits on you with the keys
  a session's panel has in Sessions (⌘⏎, ⌘E, ⌘], ⌥⇧↑ ⌥⇧↓, ⌥↑ ⌥↓, ⌃- ⌃⇧-), over the bar's own
  list (`bar.ts` `waitingAfter`, the rule the quick panel and Sessions go by, `quick.ts`
  `leftAfter`); a key for getting about added to one goes in all three.
- A side conversation (side.rs) is a harness's throwaway copy of a session, described by its
  adapter (`Adapter::side`: the process, and a `Dialect` that turns its lines into what was
  said, the tool it runs and the end of a turn, and questions into lines). The dialects are
  tested without a process; side.rs runs them, one copy a session, tells the windows (`side`
  events), and keeps each turn as it ends (`sides.json`, read back by `side::past`). A copy
  may do what its session may without asking and no more, is told to change nothing it isn't
  asked to (`side::RULES`), has nobody to ask, and leaves no record in its harness.
- A session's, worktree's, PR's or todo's right-click menu is defined once, in `src/menus.ts`,
  and every window (main, bar, peek, quick panel, desktop cards) builds it from there, saying
  what it can do. A new item goes there, not into one window's menu.
- Chips are one shape and size wherever they show (`--chip-radius`, 20 high, 11px text; the
  Chips section of app.css says what each fill means), and one component a kind of thing in
  `ui/primitives.tsx`: `HarnessChip` an agent, `WorktreeChip` a worktree, `PrChip` a PR
  (`TodoChip` and `LabelChip` in `TodoBits.tsx`). A row's chips go in a `.ca-chips` lane, which
  never cuts one off: each shows whole or wraps out of sight, the last first, and a long name
  shortens inside its chip. `scripts/preview/chip-lanes-check.js` checks every list for it. A
  session's account is said last, never before its title (asked 2026-10-01).
- A right-click (or ⌃-click) on a list's row opens its menu and doesn't select it
  (`primitives.tsx`'s `selects`); contextMenu.ts rings the row while the menu is up. A menu is
  built from the row it's for, never from the selection.
- Styles: tokens in `design/tokens.json` (`npm run tokens`), colors only through CSS
  variables. Dark values apply under `prefers-color-scheme` unless `data-theme` on the root
  says otherwise; it comes from `theme` in config.toml, which also sets the app's native
  appearance (window chrome, menus) in `lib.rs`. `src/styles/components.css` is the design
  system's bundle; app styles go in `src/styles/app.css`. Icons are Tabler outline paths in
  `src/ui/icons.ts`.
- UI copy is sentence case and plain words. Comments explain why, in plain sentences.
- `rustfmt` isn't installed here, so Rust formatting is unchecked; match the surrounding code.

## Adding a harness

A harness is a module (`claude.rs`, `codex.rs`, `opencode.rs`, `antigravity.rs`) that knows its
files and commands, with two things in it that the rest of the app goes through (`harness.rs`):
an `Adapter`, which answers for a session and for an account, and a `Reader`, which reads its
sessions for the board. Nothing outside a harness's module branches on which harness it is: it
asks `harness::adapter(session.harness)` or `harness::of(account)`. Adding one:

1. A variant of `model::Harness` with its name in `Harness::word` and its config string in
   `Harness::config` and `from_config` (codealong-peer's `model.rs`), in `config::HARNESSES`, and
   in `src/types.ts` and `src/harness.ts`.
2. A module with a struct implementing `harness::Adapter`, listed in `harness::ADAPTERS`. It must
   say `harness`, `spec` (its folder, the variable that moves it and its command, which is how
   detect.rs finds its accounts), `records`, `targets` (the ways to open a session) and `reader`;
   everything else defaults to saying it can't. What it can answer, by what asks:
   - a session's panel: `conversation` and `conversation_after` (`transcript.rs` pages a file of
     JSON lines given a line parser), `pending`, `answer`, `answer_questions`, `open_calls`;
   - opening: `targets`, and `open` for a way of its own (its app); the engine does `focus`,
     `resume` (`Session::resume_command`) and `attach` for every harness;
   - sending (send.rs): `routes`, best first, `resume` for what a new terminal types to go on
     with one that isn't running, `typed_attachments`, `clears_before_typing`, `takes_messages`;
   - acting on it: `fork`, `fork_background`, `side`, `stop`, `rename`, `interrupts` and
     `interrupt`, `archive`, `skips_permission_prompts`, `runs_its_commands`;
   - starting (start.rs): `places`, `start_in_terminal`, `background_command` and
     `start_in_background`, `app_link` and `start_in_app`, `models`, `takes_model`,
     `offers_model`, `efforts`, `default_model`, `makes_worktrees`, `before_start`,
     `app_projects`;
   - search (search.rs): `search_files` and `prose` for records that are files of lines, `said`
     for messages kept in a database;
   - its accounts: `installed`, `signed_in`, `sign_in_command`, `folder_use`, `skills_dir`,
     `acp_command`, `session_cwds` (first run's projects), `chats` (the folder of sessions it
     starts without a folder of yours, which get one lane), `no_limits`, and `polls_usage` with
     `read_usage` for usage asked from somewhere slow.
3. A `harness::Reader` in the module, which `Adapter::reader` makes and the engine keeps without
   looking inside: what the harness's files said at the last read, the folders to watch, taking
   in the files that change (`owns`, `apply`), what the watcher never says (`tick` every ten
   seconds, `glance` every second or two, `poll` for what's asked off the engine's thread), and
   its sessions, put together with a `board::*_session` constructor for its status rules, with
   where each came from (`Session::origin`: what it's a fork of, or which session started it)
   when its records say. Plan usage that comes with the sessions is `Reader::usage`. A database
   is opened with `harness::open_database`, which reads a WAL database its writer has closed.
4. What's still said harness by harness, and so needs a line for a new one: a new way a prompt
   gets in is a `send::Route` with its transport in send.rs (and a `Way` in codealong-peer, for
   what the reply box says); a new place a session starts is a `Place` there too;
   `board::session_headline`'s words for an approval, mirrored in `src/lanes.ts`; and the UI's
   own lists, which the engine doesn't send: names and apps in `src/harness.ts`, which sessions
   take `/fork` and `/side` in `src/slash.ts`, a new account's defaults and help in
   `src/ui/AccountsSection.tsx`, the reply box's words in `src/ui/SessionReply.tsx`.

## Harness facts

What each harness records and accepts, checked on this Mac. Read these before changing an
adapter; most were found by reading the apps' own bundles and files.

Claude Code

- Live sessions: `<home>/sessions/<pid>.json` with `status` (busy, shell, idle, waiting,
  blocked), `kind` (interactive, bg), `jobId`, `name`/`nameSource`, `statusUpdatedAt`. The
  timestamp isn't refreshed while busy: a session that says busy with nothing new in its
  transcript or subagent logs for 30 minutes is stuck, not working. `shell` is a turn that
  ended with a command left running in the background (a poll for a review, say, for hours): it
  runs detached, can't wait for typing, and wakes the session when it finishes, so it's never
  stuck. It can sit there forever, though (a dev server, a sign-in waiting on a browser): the
  command's processes are the shells under the session's process with `eval '…'` in their
  arguments (its MCP servers aren't), and `procs::command_trees` reads what they use.
- Transcripts: `<home>/projects/<encoded cwd>/<session>.jsonl`; subagents log to
  `<session>/subagents/agent-*.jsonl`. `ai-title` lines (the terminal tab's title) and
  `gitBranch` fields repeat through the file, so the tail has the current ones. Claude Code
  deletes transcripts after about 30 days.
- Prompts you typed, pasted or ran as a slash command are `user` lines with `origin: {kind:
  "human"}`; background tasks' notifications are `task-notification`, other sessions'
  messages `peer`, compaction summaries `isCompactSummary`. Older transcripts have no
  `origin`. `last-prompt` lines repeat the prompt's text but carry no time. One turn often
  writes more than the last 256KB, so the prompt can be far behind the tail. A message sent
  while a session is busy doesn't change the registry's `statusUpdatedAt`, nor is it a `user`
  line: it's written when the turn takes it in, as a `type: "attachment"` line whose
  `attachment` is `{type: "queued_command", prompt}` (background tasks' notifications come the
  same way, as `<task-notification>…`).
- Escape before the agent has written anything back, with the input box empty, doesn't
  interrupt the turn (2.1.285's `auto_restore_cancel`, checked in tmux 2026-09-29): it winds the
  conversation back to before the prompt and puts the prompt's text back in the input box. The
  agent never sees it, and nothing in the transcript says so: the prompt and its attachments
  stay, the registry goes idle, and the next line written has the line before the prompt as its
  `parentUuid` (after a session's first prompt, with nothing before it, the next prompt goes on
  from the prompt's last attachment instead). With text in the box, Escape interrupts as it
  does later, and writes `[Request interrupted by user]`. `claude.rs` marks such a prompt
  `withdrawn` (`fate_of`).
- Clearing the input box from outside (2.1.285, in tmux): Ctrl+C with text there clears it, and
  the text goes to the prompt history; in an empty box it only says to press it again to exit,
  which a second within the next second does. Ctrl+L is bound to `chat:clearInput` but runs the
  clear-screen handler and leaves the text; Esc Esc clears it but opens the rewind list in an
  empty box. A key and a paste that arrive in one read are taken as one paste, control
  characters and all (`^L^[[200~…`), so a key goes a moment before the paste. Typed into a box
  holding a prompt Claude Code put back, a prompt is added to it and both go as one, so
  send.rs presses Ctrl+C first then, while the session is idle.
- Another session's message (checked on 2.1.28x) is a `user` line marked `isMeta` when it starts
  a turn, or a `queued_command` attachment when a busy turn takes it in, either with `origin:
  {kind: "peer", name, body}`: `name` is the envelope's `from-name` (older lines put it in
  `from`), `body` the text inside the envelope. What you send a Claude app session from Codealong
  is one of these, named `Codealong` and starting with `send::FROM_YOU`; the panel shows it as
  yours and it counts as your prompt. A subagent's report back comes the same way, with
  `handback: true`.
- A line's `cwd` is the shell's folder. After a command that `cd`s outside the session's
  folder the shell goes back to it, so an agent working in another worktree starts each
  command there with `cd <folder> &&`, and its other commands (temp files, `osascript`) run
  where it started. `board::session_seed` lists an agent where most of its last five
  commands and edits worked; `util::command_dir` has the rule for commands.
- Background jobs (`kind: bg`) have no terminal; `<account command> attach <jobId>` opens one.
- Each background job has a record, `<home>/jobs/<job>/state.json` (`sessionId`, `state`,
  `intent`, `name`…), kept after the job ends. A job that copied a conversation (`/fork`,
  `/background`, `--bg --resume <id> --fork-session`) names the session it copied in
  `forkParentSessionId`, with `forkBoundaryAt` and `forkSourceAlive` (checked on 2.1.286,
  2026-09-30). A fork into a terminal (`--resume --fork-session` without `--bg`) records nothing
  of the kind: its transcript's lines are copies (the same uuids) with no mark, so Codealong notes
  the ones it makes (origins.rs). A `claude` started from another session's Bash tool inherits
  `CLAUDE_CODE_SESSION_ID` and `CLAUDE_CODE_CHILD_SESSION=1`, but nothing on disk says so.
- From 2.1.286 the account's background service keeps a warm spare: a background session started
  ahead of time (roster `dispatch.source: "spare"`, empty `seed.intent`), which the next
  background start takes over. Until then its registry entry says `"spare": true`, `kind: bg`,
  `status: idle`, `name` its job ID, and it has no transcript; Claude Code clears the mark when
  its first turn starts (`busy`). Claude Code's own session lookups skip spares, and so does
  `claude::read_registry` (checked 2026-09-30, one spare per account on this Mac).
- A Bash call's `tool_use` is in the transcript before its command runs, and the command's
  shell, a background one too, is a child of the session's process: the `codealong` command
  finds the session that runs it by its parent processes. A terminal keeps
  `CODEALONG_SESSION` after its session ends, and whatever starts from it later inherits it.
- A command that acted on git or GitHub gets a `toolUseResult.gitOperation` on its result line:
  `pr` (`number`, `url`, `action` created, edited, commented or closed; closed has no url),
  `push`, `commit`, `branch`. `prlinks.rs` links a session to each PR its `pr` names, subagents'
  logs (`<session>/subagents/agent-*.jsonl`, same format) included.
- `--session-id <uuid>` gives a new session the ID you choose, so Codealong knows it before it
  appears. The prompt argument goes through the same path as typed text: `@"<path>"` mentions
  attach files of up to 256 KB (262,144 bytes), an image as an image and a PDF whatever its
  size; a bigger one is silently not attached and is left for the Read tool. Image paths without `@` are only picked up when
  pasted. Reading outside the working folder asks unless the folder is given with `--add-dir`,
  which takes several folders: put it before the other options (or `--add-dir=<dir>`), or it
  swallows the prompt. There's no image flag. The prompt is a positional argument parsed like any other: one that starts with `-`
  reads as an option, and one that is only a subcommand's name (`update`, `doctor`) runs that
  command. `start.rs` pads such prompts with a space.
- A session's name (`/rename`, or the Claude app's titles) is a `custom-title` line in its
  transcript, `{"type":"custom-title","customTitle":…,"sessionId":…}`, and
  `<session>/custom-title.json` beside it (`{"customTitle":…}`, mode 600, its folder 700), which
  `/resume` lists sessions by. `/rename` also writes an `agent-name` line and the registry's
  `name` with `nameSource: "user"`, the name other sessions message it by. Another process may
  write the line (2.1.285's SDK docs say so): a running session re-stamps its metadata every
  32 KB of its own writes, at a compaction and as it exits, and first reads the last 64 KB of its
  transcript and takes the latest `custom-title` there as its own, terminal title included; a
  re-stamp skips a line equal to the last of its type within 32 KB. Checked on 2.1.285
  (2026-09-29): a session `/rename`d in tmux, renamed from outside, then exited, kept the new
  name, and `claude --resume` listed it by it. `claude.rs`'s `rename` writes both, never the
  registry, which the running process owns.
- `--resume <id> --fork-session` opens a copy of the conversation as far as it's written (a
  turn still running shows as interrupted in the copy) while the original carries on.
  `--session-id <uuid>` names the copy; with `--resume` it's taken only alongside
  `--fork-session` ("--session-id can only be used with --continue or --resume if
  --fork-session is also specified"). The registry has the new ID at once, the transcript from
  the first prompt, as with a start (checked on 2.1.284, 2026-09-28). With `--bg` the copy is a
  background job (checked on 2.1.286, 2026-09-30): the roster's `workers.<job>.dispatch.launch`
  says `{mode: "resume", sessionId: <id>, fork: true}`. With a prompt it writes its transcript,
  the conversation so far included, at once; without one it prints `(idle — send a prompt to
  start)` and has none until its first prompt, so the board shows it with the conversation it
  copies (`claude::resumed_from`).
- `/btw <question>` in a session asks a separate call over the conversation's context that has
  no tools and answers once; the answer shows in that terminal only and isn't written to the
  transcript, so typing it into a session from outside is no use. The same thing is the
  stream-json control request `side_question` (`{"type":"control_request","request_id",
  "request":{"subtype":"side_question","question","history":[{question,response}]}}`, answered
  whole in a `control_response`), which only a process's own stdin takes: not the session's
  socket, nor the daemon's.
- A headless fork that keeps no record is how Codealong asks on the side (side.rs; checked on
  2.1.286, 2026-09-30): `claude -p --input-format stream-json --output-format stream-json
  --verbose --include-partial-messages --resume <id> --fork-session --no-session-persistence
  --permission-mode <mode>`, run in the session's folder with its stdin kept open. A question
  is a line `{"type":"user","message":{"role":"user","content":…}}`; the answer streams as
  `stream_event` lines (the API's events; `content_block_delta` with a `text_delta`), each tool
  call comes whole in an `assistant` line, and a `result` line ends the turn (`is_error`,
  `result`, `usage`). No transcript is written and the original is untouched. The flag wins
  over the account's `defaultMode`, and with nobody to ask, a call that would ask is refused
  (`system` / `permission_denied`: in `default`, a Write or a `touch`), while reads and
  read-only commands run; in `bypassPermissions` a Write went through (2026-10-01). The help
  lists the modes as acceptEdits, auto, bypassPermissions, manual, dontAsk and plan; `default`,
  which transcripts write, is still taken. The copy gets its session's own mode where that
  acts without asking (`claude::side_mode`), else `default`. `--permission-prompts host` would
  hand a `-p` run's prompts to whoever runs it; side.rs doesn't use it yet.
  It reads the session's prompt cache when it's on the session's model: forking an interactive
  haiku session of 35,000 tokens, the first question read 35,120 from the cache and wrote 594,
  and answered in 2.9 s. A resumed conversation sends the system prompt it recorded
  (`--system-prompt-snapshot`, on by default), whatever `--append-system-prompt` a later launch
  passes, so what the copy is told goes in its first message. While it runs it's in the
  registry as any session is (`kind: "interactive"`, `entrypoint: "sdk-cli"`, a messaging
  socket, a derived name), and its `SessionStart` (matcher `fork`) and `PermissionRequest`
  hooks run: `claude::read_registry` and the app's permission hook skip a copy's process.
  A transcript's `message.model` has no context size (`claude-opus-5-5`, where the session ran
  `opus[1m]`), so the copy runs on the account's usual model, not one named from the transcript.
- `--model` takes a full name or an alias for the newest model of a family: `sonnet`, `opus`,
  `haiku`, `fable`, their `[1m]` (1M context) variants, `best` and `opusplan` (2.1.282's list).
  Quote it when typing it: zsh reads `fable[1m]` as a file pattern ("no matches found"). Without
  it a session starts on `model` from the account's `settings.json` (a repo's
  `.claude/settings.json` can override that).
- `--effort <level>` takes `low`, `medium`, `high`, `xhigh` (its picker calls it Extra) and `max`
  (read from 2.1.284's code). Without it a session starts on `settings.json`'s
  `modelSettings.<model>.effortLevel`, else its `effortLevel`, else the model's own default
  (the environment's `CLAUDE_CODE_EFFORT_LEVEL` wins over those settings). A model that doesn't
  take a level gets a lower one without a word (`max` and `xhigh` become `high`), and Haiku 4.5
  takes none.
  claude-agent-acp offers it as the config option `effort` (category `thought_level`), listed
  only once the session's model takes one, so it's set after the model.
- `-w/--worktree [name]` makes a worktree for the session (read from 2.1.282's code): folder
  `.claude/worktrees/<name>` under the checkout, branch `worktree-<name>`, a word-pair name when
  none is given, based on a freshly fetched `origin/<default branch>` (settings
  `"worktree": {"baseRef": "head"}` bases it on HEAD). A `WorktreeCreate` hook in settings.json
  replaces all of that, even in a git repo, and prints the path. It refuses folders that haven't
  been trusted yet. Its name is optional, so it must come before the prompt, which it would
  otherwise take as the name: `start.rs` types `--worktree` first.
- `--bg` starts a session under the account's background service (checked on 2.1.283) and
  prints `backgrounded · <job>`, or `(idle — send a prompt to start)` without a prompt. It
  ignores `--session-id` (with a warning): the session's ID begins with the job ID, and
  `<home>/daemon/roster.json` has the whole one (`workers.<job>.sessionId`) as soon as it
  returns. `--worktree`, `--model` and `--add-dir` work with it. It refuses a folder that hasn't
  been trusted ("Workspace not trusted. Run `claude` in … once"). The session runs under the
  service's processes, not the calling shell, and gets the environment of whichever shell
  started the service, so variables set for one start don't reach it (and a stale
  `CODEALONG_SESSION` can): `start.rs` quotes everything in the command instead. `stop <job>`
  keeps the conversation; `rm <job>` removes it and its worktree once nothing holds the lock.
- `/background` (`/bg`, optional prompt) moves a terminal's conversation to a background job
  with a session of its own (checked on 2.1.284, 2026-09-29): the roster's
  `workers.<job>.dispatch` says `source: "slash"`, `launch: {mode: "resume", fork: true,
  sessionId: <the old transcript's path>}`. The old transcript gets a last line
  `{"type":"continued-in","continuedInSessionId":<the job's session>}`, and the window's
  registry entry `parkedJobId: <job>` while it shows Claude Code's list of agents ("Your
  conversation moved to the background — enter opens it · esc returns to it"); leaving the list
  clears it. A conversation taken back to its window goes on in the old transcript after that
  line (one on this Mac did). The job's transcript has only its `ai-title` and `agent-name`
  lines until its next prompt, which writes a copy of the conversation (the same message
  uuids) ahead of it. Claude Code's `/resume` hides a transcript whose continuation has
  messages. A window that hadn't been told anything moves as a fresh job (`mode: "prompt"`, no
  transcript at all). Codealong shows the conversation once, as the job, reading the old
  transcript until the job has its own (`board::claude_moves`).
- Every live session (terminal, Claude app, background) listens on a Unix socket, the registry's
  `messagingSocketPath` (`/tmp/cc-socks/<pid>.sock`), for other sessions' messages
  (`peerProtocol: 1`, from 2.1.282's code). One JSON line per connection, nothing comes back on
  it: `{"msgV":1,"msg_id":<uuid>,"type":"user","session_id":<registry sessionId>,
  "message":{"role":"user","content":…},"priority":"next"}`. The auth line is optional on macOS.
  The receiver starts a turn when idle and folds it into the running one when busy, tells the
  model it came from another session and isn't the user's approval, and holds it for you when
  its permission class differs from the one the sender claims (none claimed: a session that
  bypasses permissions holds it, `cause: "no-mode-asserted"`). The claim is an attribute of the
  envelope, `<cross-session-message from-name="…" from-mode="bypass|prompting">`, in Claude
  Code's order (`from`, `from-session`, `hop-chain`, `from-name`, `from-mode`, `from-plugin`):
  it reads the envelope only if writing it again gives the same text. `bypass` is
  `bypassPermissions` (and `plan` entered from it). `send.rs` uses the socket for Claude app
  sessions and claims `bypass` for what you type in Codealong's windows to a session in that
  mode, never for the command line or another device (checked on 2.1.281, 2026-09-27).
- A Claude app session's mode is in the app's record of it (`local_<id>.json`,
  `permissionMode`; the registry's `hostSessionId` names the record); a terminal session's is
  on its transcript's prompt lines (`permissionMode`).
- Background jobs take input through the account's daemon: `/tmp/cc-daemon-<uid>/<first 8 hex of
  sha256(config dir)>/control.sock`, one JSON line each way, `{"proto":1,"op":"reply","short":
  <jobId>,"text":…,"auth":<contents of <home>/daemon/control.key>}`. It's pasted into the job as
  if typed.
- Nothing on that socket stops a job's turn (2.1.285's ops: attach, await-ack, dispatch, has,
  kill, lease, leases, list, nudge, ping, reply, resize, respawn-stale, shutdown, yield), and
  `reply` pastes and presses Return. `attach` (`{"proto":1,"op":"attach","short","auth","cols",
  "rows","attachId"}`; `caps`, if given, must be an object: null is refused as malformed) answers
  one JSON line and hands whatever comes after the request line to the job's terminal as typed,
  so an Escape written there stops the turn as in a terminal (`send::daemon_press_escape`,
  checked 2026-09-29). Attaching sizes the job's terminal to the attacher's and redraws it;
  detaching gives a terminal still attached its own size back. The daemon drops an attached
  connection whose unread output piles up.
- Nothing from outside stops a Claude app session's turn: the app sends a stream-json
  `control_request` `{subtype: "interrupt"}` down the pipe it runs Claude Code through, and the
  session's socket takes only messages.
- A permission prompt (checked on 2.1.283, in tmux): the registry says `"status": "waiting"`,
  `"waitingFor": "permission prompt"`, and the transcript's last `tool_use` has no result yet.
  The prompt lists `1. Yes`, `2. Yes, and always allow…`, `3. No`; the digit alone answers it
  (no Return), and Escape cancels it, which denies the call and ends the turn ("Interrupted ·
  What should Claude do instead?"). A stray digit with no prompt up lands in the input box, so
  `claude.rs`'s adapter presses a key only while the registry and the transcript both agree the
  prompt is up for the call you saw. Read-only commands (`git status`, `ls`) aren't asked about.
  Only typed keys answer it: a bracketed paste goes to Claude Code's paste handler, never to its
  keys, so a pasted `1` does nothing. Where the terminal has the kitty keyboard protocol it pushes
  flags 5 (1 in its legacy mode), never "report all keys", so a typed digit still arrives as the
  plain character.
- The folder-trust prompt a new folder gets has "No, exit" as its default: Return quits.
- Trust lives in the account's state file (`~/.claude.json` for `~/.claude`, `<home>/.claude.json`
  for any other, or an old install's `<home>/.config.json`): `projects.<path>.hasTrustDialogAccepted`
  (read from 2.1.285's code, checked in tmux 2026-09-29). Yes at the prompt writes it for the
  folder's git root, or the folder outside a repo. A folder counts as trusted when the repo's main
  checkout has it (so every worktree of a trusted repo is), or the folder itself or any folder
  above it up to its worktree's root does (up to `/` outside a repo); `--bg`'s "Workspace not
  trusted" goes by the same entries, with the path's links resolved and in NFC. Home trust is
  never saved. Claude Code rewrites the file all the time: under a proper-lockfile lock (a folder,
  `<file>.lock`, made with mkdir, its time freshened every 5 s, stale after 10), re-reading the
  file under it and applying only its own change, and running sessions reread it every second. So
  a write under that lock, in one step (temp file and rename), survives a running session's next
  write: `claude::trust_folder` does this when Codealong starts or forks a Claude Code session in
  a folder, adding only that entry. It strips `hasCompletedProjectOnboarding`, `history`,
  `projectOnboardingSeenCount` and `devIntentsDetected` from project entries whenever it saves.
- A `PermissionRequest` hook (checked on 2.1.284, from its code and a session in tmux with a
  stub hook) gets `session_id`, `transcript_path` (a subagent's calls are in
  `<session>/subagents/agent-<agent_id>.jsonl`), `cwd`, `permission_mode`, `agent_id` in a
  subagent, `tool_name`, `tool_input` and `permission_suggestions` (permission updates:
  `addRules`, `addDirectories`, `setMode`… with a `destination`), but no tool-use id. Its
  process is a child of the session's. In a terminal (and for the Claude app) the prompt
  shows while the hook runs and the first answer wins; answered at the prompt, the hook's
  process is left running until its `timeout` (default 600 s). A background subagent's prompt
  waits for its hooks instead (`awaitAutomatedChecksBeforeDialog`), and there the registry
  doesn't say waiting until they return. The hook answers by printing
  `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":…}}`: `{"behavior":
  "allow"}` ("Allowed by PermissionRequest hook"), with `updatedPermissions` (the suggestions)
  to apply them as the prompt's "don't ask again" does (a `setMode` one switches the session's
  mode), or `{"behavior":"deny","message":…,"interrupt":true}`, which ends the turn as the
  prompt's No does. A plain allow for `AskUserQuestion` or `ExitPlanMode` is dropped and the
  prompt stays (`requiresUserInteraction`); a question takes `updatedInput` with `answers`.
  The registry's `waiting` / `permission prompt` comes from the session's `requires_action`
  state, in a terminal and under the Claude app alike (an `AskUserQuestion` says `input
  needed`). On 2.1.284 the call's own transcript line can be missing while its prompt is up: a
  session's first call was written as its prompt showed, but a later one only with its result,
  four minutes after, when it was answered. So a prompt's call can't be read from the
  transcript until then (`claude.rs`'s keypress path looks for it there); the hook has it.
- An `AskUserQuestion` call's result line has `toolUseResult: {questions, answers, annotations}`:
  `answers` keyed by each question's text, a choice's label or your own words, several as a list
  (older versions joined them into one string with commas). Declined, the result is an error,
  "The user doesn't want to proceed…", with `toolUseResult` the string "User rejected tool use".
- A peer message to a session that bypasses permissions is held ("Held message from another
  session", Deny first) unless its settings say `"crossSessionInbound": "accept"`; typing into
  its terminal isn't held. Under the Claude app (stream-json) a hold is only a `{"type":
  "system","subtype":"peer_message_hold","state":"held"}` line to the app, which (2.9939.2) has
  no code for it: the message is never shown or delivered, and the sender never hears.
- Sign-in lives in the login keychain: service `Claude Code-credentials` for `~/.claude`,
  `Claude Code-credentials-<first 8 hex of sha256(config dir)>` for any other folder, account
  `$USER`. Read it with `/usr/bin/security find-generic-password -a $USER -s <service> -w`
  (no prompt). Never refresh the token (refreshing rotates it and signs Claude Code out), never
  log it, and pass it to curl on stdin (`-K -`), not in arguments.
- Usage: `GET https://api.anthropic.com/api/oauth/usage` with `anthropic-beta:
  oauth-2025-04-20`. Its `limits` list has `session`, `weekly_all` and `weekly_scoped` entries
  (per model, e.g. Fable) with `percent`, `severity`, `resets_at`. It returns 429 when called
  often: the app polls every 2 minutes, keeps the last numbers on failure and backs off 10
  minutes; don't loop `--usage` snapshots.

Skills (one folder in the Agent Skills layout, `SKILL.md` with `name` and `description`,
optional `references/`, read by all four harnesses; from their docs, 2026-09-24)

- Personal skills folders: Claude Code `<config dir>/skills` (a second account runs with
  `CLAUDE_CONFIG_DIR=<home>`, so `~/.claude2/skills`; `CLAUDE_CONFIG_DIR=~/.claude2 claude
  plugin install …` installs a plugin there), Codex `<CODEX_HOME>/skills`, OpenCode
  `~/.config/opencode/skills` (it also reads Claude's folders), Antigravity
  `~/.gemini/config/skills` (workspace ones in `.agents/skills`). `install.rs` copies rather
  than links: not every harness follows links, and the bundle moves on rebuild.
- A Claude Code plugin's `bin/` goes on the Bash tool's PATH while it's enabled, so
  `plugin/bin/codealong` is a shim that finds the app; `${CLAUDE_PLUGIN_ROOT}` resolves in
  hooks.
- `claude plugin` (2.1.284, with `CLAUDE_CONFIG_DIR` choosing the account) is how install.rs puts
  the plugin in a Claude Code account: `marketplace add <folder>` records `{source: "directory",
  path}` in `plugins/known_marketplaces.json` and `extraKnownMarketplaces` in settings.json;
  `install codealong@codealong` copies the plugin, links followed, into
  `plugins/cache/<marketplace>/<plugin>/<version>` and lists it in
  `plugins/installed_plugins.json` (`plugins."codealong@codealong"[].{scope, version,
  installPath}`), with `enabledPlugins` in settings.json (false once disabled; `list --json` says
  the same). `update` rereads a folder marketplace by itself and updates on any change to the
  version string (`0.2.1+aaaa` to `0.2.1+bbbb` updated), succeeding when there's nothing to do;
  new sessions get it. `enable` fails on an enabled plugin. `marketplace remove` uninstalls the
  marketplace's plugins with it. A GitHub marketplace (`owner/repo`) is cloned over SSH, then
  HTTPS, with git's prompts off: a private repo works wherever git can already clone it. A `SessionStart` or `UserPromptSubmit` command hook's stdout (exit 0) is added to
  the model's context, which is how `plugin/hooks/hooks.json` hands a session its todo.
  `allowed-tools` and `disable-model-invocation` in SKILL.md are Claude Code's own.

Claude app

- Keeps a record per Code session in `~/Library/Application Support/Claude/claude-code-sessions/<account>/<org>/local_<id>.json` (`sessionId`, `cliSessionId`, `isArchived`, `title`).
- `claude://code/continue?session=` accepts only its own `local_…` IDs (or `last`);
  `claude://resume?session=<uuid>` imports a CLI session and opens it. Rejected links are
  logged as `claudeURLHandler: code entry link invalid ?session` in `~/Library/Logs/Claude/main.log`.
- Offers no way for another program to archive a session.
- `claude://code/new?folder=<path>&q=<prompt>` (`prompt=` works too) opens the new-session page
  with the folder and prompt filled in; you send it there. It takes no model or worktree: the
  page has its own choices. `q` is capped at 14,336 characters; `folder` can repeat;
  `ssh_host`, `ssh_port`, `ssh_folder` and `source` are taken too. `file` is only logged, so a
  new Code session can't be handed images (`claude://cowork/new?file=` does take files). The
  Code tab takes images only from its own input box.
- A Code session started without a folder runs in a scratch workspace the app makes for it,
  `~/Library/Application Support/Claude/scratch-workspaces/<account>/<org>/scratch-<date>-<6
  hex>` (`userData` in 2.9939's code), and the app removes it once the session is gone. It's
  no repo, so the engine lists every such session in one lane, Claude app chats
  (`engine::chat_roots`); a session with no lane would be on no list at all. Its first prompt
  line has two text parts: the app's `<system-reminder>` saying so, then what you typed
  (`claude::typed_text` skips the reminder).

Codex

- Rollouts: `<CODEX_HOME>/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl`; archived ones move to
  `archived_sessions/`; names in `session_index.jsonl`; shared state in `state_5.sqlite`.
- A turn is `event_msg` `task_started` … `task_complete` (with `last_agent_message`) or
  `turn_aborted`. One turn can write megabytes of tool output, so the event that opened it can
  be far behind the tail: `parse_rollout` reads back for it. Newer versions record prompts as
  `item_completed` with a `UserMessage` item, not `user_message`, and wrap context around them
  (`## My request:`, `<heartbeat>`, `<realtime_delegation>`; answers to the agent's questions
  are `<send_user_message_question_reply>`).
- Approvals aren't recorded anywhere on disk: not in the rollout, `logs_2.sqlite` or the app's
  logs. While the app shows "Awaiting approval" (e.g. a computer-use app to allow) the rollout
  just ends at a `response_item` `function_call`/`custom_tool_call` with no `*_output` for its
  `call_id`. Of 22,000 calls in a month, commands answered within 168 s and other tools nearly
  always within 30 s, so `board::codex_unanswered` takes a call silent past its tool's limit
  (4 min for commands, 1 min otherwise; none for sleep and wait tools) as Needs you.
- Commands are `item_completed` `CommandExecution` items with the folder the agent chose
  (`cwd`, a `file://` URL) and `[shell, "-lc", script]`. Scripts don't `cd` (none of 3,096
  recent ones did), and 45% ran outside the thread's own folder. File edits are `FileChange`
  items, `changes` keyed by path. The item carries the command's `stdout` too, so a `gh pr
  create` is followed by the URL it printed; the app's `attach_artifact` tool (`McpToolCall`,
  `artifact_type: pull_request`) shows a PR on the thread.
- The app runs every thread's commands under its one `codex app-server` process, with no
  thread id in their environment. A call is in the rollout before it runs: a `response_item`
  `custom_tool_call` (the app's `exec` tool, whose `input` is JavaScript holding the command in
  a string literal, `tools.exec_command({cmd:\"…\"})`) or `function_call` (`arguments` as
  JSON), answered by a `*_output` with its `call_id`. `codealong` finds the thread that runs it
  from these (`Adapter::open_calls`); a terminal's thread runs its commands under its `codex`.
- A forked thread's `session_meta` has `forked_from_id` (the thread it copied). A thread the app
  made for an agent (its `create_thread` tool, `codex_app` namespace) says `thread_source:
  "agent_created_thread"`, and its first turn opens with that tool's output, `<codex_delegation>
  <source_thread_id>…</source_thread_id><input>…</input></codex_delegation>`, a few lines in
  (after the app's instructions, tens of KB). Subagents (`source.subagent.thread_spawn.
  parent_thread_id`) aren't threads of their own on the board.
- `/side` in the TUI (its tui/src/app/side.rs; checked over an app-server on 0.157.1,
  2026-09-30) is `thread/fork {threadId, ephemeral: true, developerInstructions}`, then
  `thread/inject_items` with a "Side conversation boundary" user message, then ordinary
  `turn/start`s. Over the protocol the fork needs `excludeTurns: true` ("ephemeral paginated
  thread/fork requires `excludeTurns: true`") and a parent that has a rollout (an ephemeral
  parent: "no rollout found for thread id"). It took 0.04 to 0.13 s; the first answer 4.5 s
  (the thread's MCP servers start first), a follow-up 2.8 s, as `item/agentMessage/delta`
  notifications, each tool an `item/started` (`commandExecution` with `command`,
  `mcpToolCall`…), ended by `turn/completed` (`turn.status`, `turn.error.message`). No rollout
  is written for the copy. `thread/start {ephemeral: true}` runs turns with nothing written
  too, and `thread/delete {threadId}` removes a thread and its rollout. Each turn's
  `turn_context` line in a rollout names its `model` and `effort`, its `approval_policy`
  (untrusted, on-request, never, or a granular object) and its `sandbox_policy` (`type`
  read-only, workspace-write or danger-full-access, with its roots); side.rs forks on the
  thread's latest model, sandbox and approval policy (`codex::last_turn`), so the copy may do
  what the thread may, and declines an approval that comes to it. Codex's own `/side` gives
  its copy the thread's permissions too, and tells it to change nothing unless asked.
- Each turn's `token_count` event carries `rate_limits` (`primary`/`secondary` windows with
  `used_percent`, `window_minutes`, `resets_at`; `limit_id` `codex` or per model; `plan_type`).
- The desktop app is `/Applications/ChatGPT.app` (bundle id `com.openai.codex`; an older
  `/Applications/Codex.app` copy may exist). Installing it doesn't put `codex` on the PATH; its
  CLI is `Contents/Resources/codex-cli/bin/codex` (`Contents/Resources/codex` before 26.924). The ChatGPT chat app has the same name, so Codealong knows
  the Codex app by its bundle id. `codex://threads/<uuid>` opens a thread (the app
  reads it first and ignores ones it can't), `codex://review?pr=<url>` opens a PR review.
- The app's main process routes between its windows on `<CODEX_HOME>/ipc/ipc.sock` (4-byte
  little-endian length, then JSON). A client sends `initialize`, then `thread-owner-discovery`
  (version 1) finds the window that has a thread, and `thread-follower-start-turn` (version 2)
  starts a turn there, as if typed. Answer `client-discovery-request`s with `canHandle: false`
  or the router waits 10 s on you. A mismatched version is dropped. Starting a turn from an
  app-server of your own would make a second copy of the thread the app doesn't see.
- A window owns only the threads it has loaded (4 of 40 recent ones, checked 2026-09-27); a
  discovery for any other goes unanswered until the router gives up after 10 s. Opening
  `codex://threads/<id>` makes the window take it within about 2 s, but it takes it as it starts
  loading: a turn sent in the next second went out with an empty model ("The '' model is not
  supported") and the app's default reviewer, so `send::codex_start_turn` waits 2 s more.
- `thread-follower-interrupt-turn` to the owning window stops a thread's turn as the app's Stop
  does from a window that doesn't own it: `params: {conversationId, mode: "user-stop"}`, version
  3 (4 with `expectedTurnId`, which stops the turn only while it's still the one running; 26.924's
  code). It sends `turn/interrupt`, kills the exec running the call, pauses an active goal and
  stops its subagents; the rollout gets `turn_aborted`. A command already handed to a background
  terminal keeps running, as after Escape in the CLI ("1 background terminal running · /stop to
  close"); `thread/backgroundTerminals/clean {threadId}` ends those (checked 2026-09-29).
- A new thread can be started for the app to run: `thread/start {cwd}` from an app-server of
  your own, then `thread/inject_items` with one item (an empty list is refused), since the
  rollout isn't written until the history has something in it and the app can't open a thread
  without its rollout ("rollout_not_found"). Opened in the app and sent its first turn through
  the window socket, it runs in the app like one typed there (`start::codex_app_start`). A turn
  in a thread with no turns needs `model` (and `effort`) in its request, or the window sends an
  empty model. Such a thread says `originator: "codealong"`; the app's thread list doesn't
  filter on originator for local threads.
- The CLI inside the app bundle (v0.152.1 on 2026-09-26, 0.158.0-alpha.2 in 26.924) can be older
  than the account's default model, which the API then refuses ("requires a newer version of
  Codex"); `-m` picks another. A new folder asks to be trusted, then about new or changed hooks.
- The CLI (Homebrew cask `codex`, 0.157.1 on 2026-09-27; `/opt/homebrew/bin/codex`) runs its
  TUI against a shared app-server daemon it spawns (`~/.codex/packages/app-server-daemon/…/codex
  app-server --managed-daemon`); the daemon, not the TUI, holds the rollout open, so a TUI's
  thread isn't in its open files. A thread the TUI made says `originator: "codex-tui"` in its
  `session_meta` (`codex_cli_rs` before; the app's say `Codex Desktop`), with `source` "vscode"
  through the daemon. There's no way to choose a new thread's id. `codex -- <prompt>` starts on
  a prompt (`-i` takes several images, so `--` ends them), `codex resume -- <id> <prompt>`
  continues one, `codex fork <id> [prompt]` opens a copy of one (an app's thread too) under a new
  id Codex picks (the app-server's `thread/fork {threadId, cwd, …}` does the same), and a typed-in prompt pasted into the TUI is sent with Return, queued while a
  turn runs. `codex::terminals` finds TUIs by `ps` (the daemon, `exec` and bundled copies aren't
  terminals) and `pair_terminals` gives each its `resume` id or its folder's newest CLI thread
  active since it started. Escape in the TUI stops a running turn ("Conversation interrupted", `turn_aborted`),
  leaving its background terminals running.
- `codex://threads/new?path=<folder>&prompt=<text>` opens a new thread there with the prompt
  filled in (it also takes `originUrl`, `projectId`, `mode` and `browserUrl`, and no images;
  nothing in 26.924's link parser sends it);
  `codex://threads/<uuid>?prompt=` fills in a reply. `mode` is chat, work or codex, not a
  worktree choice: the new-thread page has its own Local or Worktree. The app sends images as
  `{type: "localImage", path}`, and so does the app-server's `turn/start` input; the CLI's
  `-i/--image <FILE>...` attaches images to its first prompt (prompt first, or after `--`). A folder the app has no project for becomes a new project. Its projects are
  in `<CODEX_HOME>/.codex-global-state.json` (`local-projects[].rootPaths`,
  `electron-saved-workspace-roots`), a file of several megabytes.
- A thread started outside a project (a "projectless chat") runs in a folder the app makes for
  it, `~/Documents/Codex/<date>/<slug from its first words>` (26.924's code; the same file lists
  them in `projectless-thread-ids`). The engine lists them all in one lane, Codex chats.
- The app's unread dots are in that file too, `electron-thread-read-state-v1.unreadByIdentity.
  <account hash>.<host key>`: the ids of threads with a turn you haven't looked at, rewritten as
  one changes. Only the app's own threads and their subagents were ever in it (none from the
  CLI, `exec` or Codealong), and every thread on the board was read there while Codealong still
  called ten of them Your turn (checked 2026-09-28). Codealong doesn't read it yet.
- Actions go through its app-server: `<the app's CLI> app-server --listen stdio://` with `CODEX_HOME` set, JSON lines: `initialize {clientInfo}`,
  then `{"method":"initialized"}`, then e.g. `thread/archive {threadId}`. No shared daemon runs,
  so `codex app-server proxy` fails. `codex app-server generate-json-schema --out <dir>` dumps
  the protocol. Logs: `~/Library/Logs/com.openai.codex/`. Your own app-server can't archive a
  thread the app has loaded ("thread … already has an active writer").
- `thread/name/set {threadId, name}` names a thread, loaded or not: a line in
  `session_index.jsonl` and `threads.name` in `state_5.sqlite`. It refuses an archived thread
  ("no rollout found for thread id …"; 0.158.0-alpha.2 on a scratch `CODEX_HOME`, 2026-09-29).
  The window socket has no rename (its `thread-follower-*` messages), so the app shows a new
  name when it next reads its threads.

OpenCode (2.0.16, from its source at that tag and its files here)

- Data folder `~/.local/share/opencode` (`opencode debug paths` prints every folder;
  `XDG_DATA_HOME` moves it). Sessions are rows in `opencode.db` (SQLite, WAL mode):
  `session_v2` (`id` is `ses_…`, `directory`, `title`, `parent_id` on subagents, `time_created`,
  `time_updated`, `time_idle` and `idle_outcome` written when a run ends, `time_viewed`,
  `time_archived`) and `session_message` (`type` user, assistant…; `seq`; `data` JSON: a user
  message's `text`, an assistant's `content` parts of type text, reasoning or tool with `name`
  and `state.input`). A run streams writes many times a second, all into the WAL; Codealong
  rereads both tables when the database or its WAL changes (see below). The `event` table stayed empty when a session was made through
  the API, so a run in progress is a user message newer than `time_idle`.
- Permission and question requests never reach the database. The background service
  (`opencode serve --service`, started by the TUI) knows them: `~/.local/state/opencode/service.json`
  has its `url`, `pid` and `password` (mode 600; HTTP Basic auth as user `opencode`).
  `GET /api/session/active` gives `{data: {ses_…: {type: "running"}}}`, `GET
  /api/session/<id>/permission` and `/form` what it waits on, `GET /api/session` the list
  (takes `directory`), `/openapi.json` the rest. Codealong polls every 10 seconds and after
  each database change, and without a service goes by the database with a 15-minute stale
  cap. The password goes to curl on stdin, never on the command line.
- The TUI (`opencode [dir]`) is a client of the service: `--session <id>` (`-s`) continues a
  session, `--prompt <text>` starts on a prompt, `--continue` the last one. Nothing lets
  Codealong choose a session's id, so a started session shows once OpenCode writes it.
  `opencode run` is non-interactive and `mini` a small interactive one; both count as
  terminals only for `mini`. The TUI titles its tab `OC | <title>` (can be turned off), and two
  terminals can show one session. Codealong pairs a `--session` terminal with its session and
  any other with its folder's latest session.
- Tools run in the service, not the TUI, and the service keeps the environment of the shell
  that first started it, `CODEALONG_SESSION` included. A tool part is in `session_message`
  with its `state.input` while its command runs (`state.status` not yet completed or error), so
  `codealong` finds the session that runs it there (checked with `opencode run`, 2026-09-27).
- `POST /api/session/<id>/prompt` with `{text, delivery: "queue"|"steer"}` sends a session a
  prompt; any TUI on that session shows it.
- `PATCH /api/session/<id>` with `{title}` renames a session (204; `time_updated` moves), as it
  takes `metadata` and `permissions` (checked against a service of its own, 2026-09-29).
- `POST /api/session/<id>/interrupt` (body `{}` is fine) stops a session's run and answers
  `{interrupted}`, false for one that was idle; the row gets `idle_outcome` `interrupted`.
  `?resume=true`, which the TUI's double Escape sends, goes straight on with a message steering
  it (checked 2026-09-29).
- A fork's row says what it copied: `fork_session_id` and `fork_boundary` in `session_v2`, from
  2.0.16's schema; older databases have no such columns.
- `POST /api/session/<id>/fork` with `{}` copies a session's history into a new session (`{before:
  <message id>}` copies what came before it) and answers `{data: {id, title, fork: {sessionID,
  boundary}}}`. The copy has no `parent_id`, so it lists as a session of its own, titled `<title>
  (fork #1)`; an empty session is refused (`kind: "empty_session"`). The command line can't
  fork (checked against a service of its own with scratch `XDG_*` folders, 2026-09-28).
- Permission requests: `GET /api/session/<id>/permission` lists them (`id` `per_…`, `action`
  such as `shell`, `resources` such as the command, `source.id` the tool call);
  `POST /api/session/<id>/permission/<request>/reply` with `{"decision": "once"|"always"|"reject"}`
  answers one, and the TUI moves on as if you'd answered there. Shell commands in the project
  aren't asked about unless a config says so (`opencode.json`: `"permission": {"bash": "ask"}`);
  `--auto` approves everything not denied. `opencode --prompt` fills the prompt in without
  sending it. The free models (`opencode/…`) need no sign-in.
- `opencode.db` runs in WAL mode: while a session runs only `opencode.db-wal` changes, and
  FSEvents doesn't report those writes (SQLite writes through a file it holds open; a watcher
  saw OpenCode's other files and a probe file change, never the WAL). So the engine stats the
  database and its WAL every 1.5 s (`engine::check_databases`) and rereads on a change.
- Sign-in is `opencode auth login`; credentials live in the database (`credential`, and
  `account` for OpenCode Go) or a legacy `auth.json`. There are no plan limits to read.
- Creating a session through the API in a folder made OpenCode point its `global` project
  (its project for folders without version control) at that folder.

Antigravity (the hub app 2.17.0 and its command line, `agy` 1.2.14; from the app bundle, the
command line's binary, and runs on this Mac, 2026-10-01)

- Two programs on one sign-in. The hub app is Electron (`/Applications/Antigravity.app`, bundle
  id `com.google.antigravity`), not the earlier VS Code fork; its agent is
  `Contents/Resources/bin/language_server` (Go; "gemini_coder" protos), which serves the UI on
  random local ports (logged in `~/Library/Logs/Antigravity/language_server.log`) behind a CSRF
  token and has none of the command line's flags. The command line is `agy`, a Go binary signed
  by Google that updates itself: `curl -fsSL https://antigravity.google/cli/install.sh | bash`
  puts it in `~/.local/bin` and adds that to the PATH in `~/.zshrc`.
- Each keeps its conversations in a folder of its own under `~/.gemini`, laid out alike:
  `antigravity` for the app (a separately installed IDE uses `antigravity-ide`) and
  `antigravity-cli` for the command line. They share the sign-in, a token file
  `~/.gemini/jetski-standalone-oauth-token` (mode 600; only its presence is checked), and
  `~/.gemini/config` (skills, plugins, MCP servers). Codealong reads both folders as one
  account (`antigravity::folders`). The command line can't go on with one of the app's
  conversations ("trajectory not found").
- `conversation_summaries.db` (SQLite in WAL mode, written by gorm) has a row a conversation:
  `conversation_id`, `title` (empty in every row seen), `preview` (the first prompt),
  `step_count`, `last_modified_time` and `last_user_input_time` (text such as
  `2026-09-24 14:16:07.434615-07:00`; the year 1 for never), `workspace_uris` (JSON list of
  `file://` URIs), `status`, `source`, `parent_conversation_id` and `nesting_depth` (subagents),
  `not_fully_idle`, `killed`, `app_data_dir`, `raw_summary` (a protobuf). `status` is
  `CASCADE_RUN_STATUS_IDLE`, `_RUNNING`, `_BUSY` or `_CANCELING`; none says error or waiting.
  The command line's table lists the app's conversations too, without a status: `app_data_dir`
  says whose folder a row is.
- SQLite takes a WAL database's `-shm` and `-wal` away when its last connection closes, and a
  read-only connection can't make them, so a plain read-only open fails once the harness has
  quit ("unable to open database file"). `harness::open_database` reads it as immutable then.
- `conversations/<id>.db` holds a conversation's steps (`steps`: `idx`, `step_type`, `status`,
  and protobuf blobs). The numbers are in the binary's own definitions: status 1 pending,
  2 running, 3 done, 4 invalid, 5 cleared, 6 canceled, 7 error, 8 generating, 9 waiting,
  11 queued, 12 interrupted. A step whose permission prompt is up is waiting (9), with the
  summary running; a call a print run couldn't ask about is canceled (6).
- `brain/<id>/.system_generated/logs/transcript_full.jsonl` is the same steps as JSON lines,
  written as they finish: `step_index`, `source` (`USER_EXPLICIT`, `MODEL`, `SYSTEM`), `type`
  (`USER_INPUT`, `PLANNER_RESPONSE`, `GENERIC`, `SYSTEM_MESSAGE`…), `status`, `created_at` (to
  the second), `content`, and on a planner's step `tool_calls` (`name`, `args`).
  `transcript.jsonl` beside it is the same with each argument as a string of JSON. Your prompt
  is wrapped in `<USER_REQUEST>`, followed by the time and what settings changed. A call has no
  id: its result is a step of its own after the planner's, in the calls' order (two calls at
  step 9 are answered by steps 10 and 11), and lines aren't always in step order (10 was
  written before 9). A result starts with `Created At:` and `Completed At:` lines; a command's
  goes on `The command exited with code N.` and `Output:`. A step that waits on you isn't in
  the transcript until it's answered: the planner's call is its last line.
- Its tools take `toolAction` and `toolSummary` besides their own arguments: `run_command`
  (`CommandLine`, `Cwd`), `view_file` (`AbsolutePath`), `replace_file_content` and
  `write_to_file` (`TargetFile`).
- `agy` opens its terminal UI; `-i <prompt>` starts it on a prompt, `--conversation <id>` goes
  on with a conversation (with `-i` too), `-c` with the folder's last
  (`cache/last_conversations.json`). `--model <slug>` takes what `agy models` lists
  (`slug<TAB>name` a line; it asks Google, a second or two), `--add-dir` repeats, `--effort`,
  `--mode accept-edits|plan`, `--sandbox`. A prompt that starts with `-` is taken as the
  option's value. There's no way to choose a conversation's id.
- `-p <prompt>` is a print run: one turn, then it exits. `--output-format json` prints
  `{conversation_id, status, response, num_turns, usage}`; `stream-json` prints an `init`
  event, `step_update`s (`step_type` user_input, agent_response with `text_delta`, tool with
  `tool_info`) and a `result`. `--input-format stream-json` takes `{"event":"user","message":
  {"content":…}}` lines, a turn each, and stays up while its stdin is open. A print run can't
  ask: a call that needs permission is denied, the turn ends with `denied_actions` in its
  result and exit 0. `--dangerously-skip-permissions` allows everything, and
  `~/.gemini/antigravity-cli/settings.json` (`permissions.allow`) what it lists; reads and
  writes in the workspace aren't asked about.
- A print run writes its summary row only as it ends, though its transcript is written as it
  goes; the terminal UI writes the row as it goes (running, mid-turn). So while a process has a
  conversation, Codealong goes by its transcript (`antigravity::started`, `Tail::turn_open`).
- A process keeps `presence/<id>.lock` open while it has a conversation (`lsof -p` shows it);
  the files stay behind. That's how Codealong tells which `agy` has which conversation.
- The terminal UI's first run asks for a color scheme and then shows Terms of Service and a
  data-use choice, ticked by default; print runs go ahead without it
  (`cache/onboarding.json`). A prompt given with `-i` runs behind those screens, in
  `~/.gemini/antigravity-cli/scratch` with no workspace on its row, which is the folder
  Codealong lists as Antigravity chats.
- It has no Agent Client Protocol mode (its issue 31 is open), so Codealong can't run it
  itself. Google's staff say on their forum that running the binary as a child process on the
  user's sign-in is supported and counts against the same limits.
- Not checked, because the terminal UI wouldn't go past its first-run screens here: the keys
  its permission prompt takes, whether Escape stops a turn, and how it takes a paste. So
  Codealong doesn't answer its prompts or stop its turns. Nor: what the app writes mid-turn,
  and its hooks (`~/.gemini/config/hooks.json`; the binary names `PreToolUse` and
  `PostToolUse`).
- Deep links: the app registers `antigravity://`, but its UI handles only
  `antigravity://teleport/ai_studio`; nothing opens a conversation or starts one, so Codealong
  brings the app forward. Quotas show in the app only.

Ghostty (1.3, AppleScript dictionary in the app bundle)

- Windows, tabs and terminals with `id`, `name`, `working directory`; `focus`, `select tab`,
  `activate window`; `new window with configuration {initial working directory, initial input,
  environment variables}`, the last a list of `KEY=VALUE` set in the window's shell. `initial
  input` is written to the new terminal's pty as it starts, before the shell reads it, byte for
  byte (it goes through a Zig string literal, so control characters pass): each newline is a
  Return to the shell, which reads it a line at a time (1.3.1's `embedded.zig`).
- `input text <text> to terminal id <id>` pastes (bracketed when the program asked for that,
  as Claude Code and Codex do; otherwise each newline is a Return), and `send key "enter" to
  terminal id <id>` presses Return. Neither needs the terminal in front. `send.rs` types only
  into a tab found by its OSC 7 mark, never by title.
- No TTY or PID property. To focus a session's own tab, Codealong writes an OSC 7 folder report
  with a unique path to the session's TTY (`ps -o tty= -p <pid>`), finds the terminal reporting
  it, selects that tab, then reports the real folder again. Claude Code titles tabs
  `<glyph> <title>` and animates the glyph while working, so titles are only a fallback, used
  when they (or the folder) pick out a single tab.
  The mark normally shows within a fraction of a second, but on a busy Mac (load in the
  hundreds) Ghostty takes seconds, so the wait is 8 s, and only for sessions whose process
  descends from Ghostty (claude → zsh → login → ghostty); tmux and other terminals go straight to
  titles. Once the tab is selected, the other focus steps are best effort. Focus requests take
  turns, and one still waiting when a newer click arrives is dropped.
- `windows` run front to back (`NSApp.orderedWindows`), and a loop variable over app objects
  (`repeat with w in windows`) is `item i of every window`, asked afresh on every use: after
  `select tab` brings a window forward, `w` is another window. Hold windows, tabs and terminals
  by id (`every window whose id of every terminal contains <id>`, then `every tab of w whose
  …`); `id of every terminal whose working directory contains <mark>` is one question, where
  separate `id` and `working directory` lists can fall out of step. Store a query's result
  before taking an item: `item 1 of (id of every terminal whose …)` is sent to Ghostty whole and
  fails. `select tab` and `activate window` act at once; `focus` raises the window on Ghostty's
  next turn. `current date` inside `tell application "Ghostty"` goes to Ghostty first.
- macOS doesn't let a background process bring an app forward: `activate` from a shell leaves
  Ghostty behind; from a click in Codealong it works.
- A tab on another desktop (a Space): `select tab` and `activate window` are
  `makeKeyAndOrderFront`, the second with `NSApp.activate` after it, and `focus` does the same on
  Ghostty's next turn (1.3.1's ScriptTab.swift, ScriptWindow.swift, BaseTerminalController.swift),
  so the window is raised before Ghostty is the active app, and after a click in the bar Ghostty
  is active only once Codealong asks (`bar::bring_forward`). macOS switches desktops for a window
  the active app brings forward (AppKit's notes on `moveToActiveSpace`), so `launch::follow`
  focuses the terminal a second time once its app is active, in iTerm2 and Terminal too. Nothing
  public says which desktop a window is on, or goes to one. The window server's own calls do
  (`CGSCopySpacesForWindows`, read here: a tab that isn't selected is a window on no desktop; and
  `_SLPSSetFrontProcessWithOptions` with a window's number, which AltTab focuses a window on
  another desktop with), but Ghostty's script ids are object addresses, not window numbers, and
  getting a window's number through Accessibility needs that permission, asked again for every
  build of Codealong Dev. Not yet checked by hand (2026-10-01): that the second focus takes you
  to the other desktop, since every Ghostty window was on one desktop and the check moves your
  screen. With Mission Control's "When switching to an application, switch to a Space with open
  windows for the application" off, macOS may stay where you are.
- `send key "<name>" to terminal id <id>` takes the Swift names of Ghostty's keys (`digit1`,
  `escape`, `enter`; `digit_1` and `1` error with "Unknown key name") and sends a key event
  without text, and Ghostty writes nothing for a character key without text: only keys with
  codes of their own (Return, Escape, Tab, arrows) arrive. `perform action "text:<chars>" on
  terminal id <id>` writes characters to the program as typing them does (a Zig string literal,
  so `\x1b` works) and answers true. Checked on 1.3.1 with a program recording its input,
  2026-09-27; `cargo test ghostty_presses_keys -- --ignored` repeats it. While the screen is
  locked Ghostty doesn't start new terminals ("Terminal surface model is not available"), so
  nothing can be typed into one opened then.
- Scripts can take many seconds: while Ghostty launches or is busy, and while macOS asks whether
  Codealong may control it. The bundle is ad-hoc signed with a per-build identifier
  (`codealong-<hash>`), so macOS treats each rebuild as a new app and asks again. Ghostty still
  carries out an Apple event after `osascript` is killed, so `launch.rs` waits a minute rather
  than report a failure for a window that then opens.

Google Chrome (154, for previews' profiles, chrome.rs; checked 2026-09-29)

- Profiles are listed in `~/Library/Application Support/Google/Chrome/Local State` (JSON):
  `profile.info_cache` keyed by folder (`Default`, `Profile 12`), each with `name` (what the
  profile picker shows) and `user_name` (the Google account, empty when none), and
  `profile.profiles_order` for the picker's order. It's rewritten while Chrome runs, not at
  once: a profile made a moment ago can be missing from it.
- `open -a`/`-b` with `--args` gives a running app nothing, so a page opens in a profile with
  `open -n -b com.google.Chrome --args --profile-directory=<folder> <url>`: the new copy hands
  the URL to the running Chrome, which opens it in that profile's window, and quits (checked
  with a Chrome on a scratch `--user-data-dir`: one main process after, the page in that
  profile's session). Started from the binary instead, a Chrome that wasn't running would be
  Codealong's child.
- A tab's AppleScript `id` (text) is the id extensions get for it (both are
  `SessionTabHelper`'s, per Chromium's `tab_applescript.mm` and `extension_tab_util.cc`), unique
  across all profiles' windows, which one Chrome process runs and one `every window` lists.
  `every window` answers `window id N` references. By `window_applescript.mm`, `set active tab
  index of w to i` activates the tab through the tab strip and `set index of w to 1` orders the
  window front (not yet watched on a window in use). Windows have no profile property.
- A profile's History (SQLite: `urls`, `visits` with `visit_time` in microseconds since 1601) is
  locked while Chrome runs; a `file:…?immutable=1` URI reads it in place.

Claude in Chrome (extension 1.0.97, `fcoeoabgfenejglbffodgkkbkcdhcgfn`; chrometabs.rs, checked
2026-09-30)

- Chrome starts the native host (`Claude.app/Contents/Helpers/chrome-native-host`, or Claude
  Code's `~/.claude/chrome/chrome-native-host`) once per profile with the extension; each listens
  on `/tmp/claude-mcp-browser-bridge-$USER/<pid>.sock`, and nothing in its arguments names the
  profile.
- Its tools are `mcp__claude-in-chrome__*`. Each result ends with `Tab Context:` then
  `- Executed on tabId: N` (when the call acted on a tab), `- Available tabs:` and one `  •
  tabId N: "title" ("url")` a line for the session's tab group, none once they're closed ("Group
  is now empty (auto-removed)"). `tabs_context_mcp` answers JSON first,
  `{"availableTabs":[{tabId,title,url}],"selectedTabId":N,"tabGroupId":G}`, and other tools'
  results can carry it as "Tab context (from front-loaded tabs_context_mcp)". A group closed
  under a session answers "This session's tab group no longer exists".
- `list_connected_browsers` answers `[{deviceId, name, osPlatform, connectedAt, isLocal,
  inUse}]`: a device is one profile's extension, and names like "Browser 2" change with the
  list. `select_browser {deviceId}` answers `Connected to browser "<name>"` or `Selected
  browser …`, as `switch_browser` does.
- The extension's storage (LevelDB, `<profile>/Local Extension Settings/<extension id>/`) keeps
  `bridgeDeviceId`, `displayName`, and `tabGroups`, `{"<main tab id>":{"chromeGroupId":G,
  "mainTabId",…}}`, rewritten whole on each change. The log is plain; compaction into tables
  drops a gone group's record within days.

iTerm2 (3.7.3, from its AppleScript dictionary and a program recording its input, 2026-09-29)

- `/Applications/iTerm.app`, bundle id `com.googlecode.iterm2`, scripted as `application
  "iTerm"`. Windows (front to back, `id`), tabs (no id; `select`), sessions (`id` a UUID, `tty`,
  `name`, `variable named "session.path"` its folder, `"session.jobPid"`). A tab's shell runs
  under `iTermServer-<version>` (zsh → login → iTermServer-3.7. → iTerm2 in `ps`), which carries
  on under launchd when iTerm2 restarts: `launch::kind_by` takes either name.
- `tty of every session of every tab of window id <w>` answers in one event (lists of lists);
  `whose` on nested elements doesn't. Inside the tell `contents of x` asks iTerm2 for a
  session's contents, so lists are walked by index.
- `create window with default profile command <cmd>` / `create tab …` (in a `tell window id`)
  run the command in place of the profile's shell, with no `login` in between, and return at
  once with the session's `tty` known. There's no folder or environment option:
  `launch::launcher` is that command.
- `write text <t>` sends exactly `<t>` then `\r`; `newline no` drops the `\r`. Nothing is
  bracketed, so a paste is wrapped in `ESC[200~ … ESC[201~` by Codealong, and `1` or Escape
  alone arrive as typed keys.
- `select` on a window brings it to the front (with `activate`); on a tab or session selects it.
  `close` on a session closes it without asking, even with a job running.
- A closed window stays in `windows`, invisible, for good (`visible` false, not minimized): scripts
  take `every window whose visible is true or miniaturized is true`, and a new tab can get a
  closed one's TTY.

Terminal (2.15 on macOS 26.6, from its AppleScript dictionary and a program recording its input,
2026-09-29)

- `/System/Applications/Utilities/Terminal.app`, `com.apple.Terminal`. Tabs have `tty`,
  `processes` (names), `busy`, `selected`, `custom title`; no folder. A tab's shell runs under
  `login` → `Terminal`.
- `do script <t>` opens a new window whose shell gets `<t>` typed; `do script <t> in <tab>` types
  into that tab. It always sends `\r` after `<t>` (`\n` inside stays `\n`), and nothing is
  bracketed: no single keypress, so Codealong can't answer a permission prompt there (a `1` with
  its `\r` reaches Claude Code as text). A bracketed paste followed at once by `\r` reached
  Claude Code 2.1.284 as one prompt, sent.
- No tabs from a script: `make new tab` fails ("AppleEvent handler failed"), and a tab doesn't
  take `close`, only a window does. New terminals are always windows; closing a tab that shares
  its window types `exit` in it.
- Focus: `set selected of tab i of window id w to true`, `set index of window id w to 1`,
  `activate`.
- A closed window stays in `windows` (invisible), like iTerm2's. A tab whose shell exited stays
  when its profile says so ("When the shell exits: Don't close the window",
  `shellExitAction = 2`), with its TTY and no `processes`; a new tab can get that TTY, so only
  tabs with processes count. `contents of <tab>` in AppleScript is the tab itself (the
  keyword wins); `history of <tab>` is its text.

Starting a session in iTerm2 or Terminal (launch.rs, checked 2026-09-29)

- The launcher is a `/bin/sh` script in `$TMPDIR` (mode 600) that deletes itself, `cd`s, exports
  the environment and `exec`s the login shell (`getpwuid`) with `-l`. iTerm2 runs it as the
  session's command; Terminal types ` exec /bin/sh '<launcher>'` into the new window's shell
  (the leading space keeps it out of history where `HIST_IGNORE_SPACE` or fish's rule applies).
- Codealong types the next line only once the shell reads a key at a time: the launcher gone and
  the TTY out of canonical mode (`tcgetattr`, `ICANON` off), which zsh's line editor, readline
  and fish set while they wait for a line. A window that comes forward takes the keyboard, so
  keys typed on the Mac meanwhile land in its shell ahead of the line (`ree exec /bin/sh …`, while
  someone typed during the opt-in `tty_apps_*` tests): each line typed into a new shell starts
  with Ctrl-U, which clears the line in zsh, bash, fish and a terminal with no line editor.
- zsh takes `#` on a typed line as a comment only with `interactive_comments` set, which is off
  unless a startup file sets it: `echo b > b.txt # note` typed into `zsh -f` wrote `b # note`.
  bash's interactive shells take comments. So Run on a code block (runcode.rs) types a block's
  commands without their comments, one line each; lines typed before the shell is up (Ghostty's
  initial input) or after (iTerm2, Terminal) ran alike in zsh 5.9 and bash 3.2 on a pty.

Agent Client Protocol (hosted sessions, acp.rs; schema v1, adapters checked 2026-09-27)

- JSON-RPC 2.0, one message a line, on the adapter's stdin and stdout. `initialize`
  (`protocolVersion: 1`; Codealong offers no `fs` or `terminal`, so the agents use their own
  tools), `session/new {cwd, mcpServers}`, `session/prompt` (answers with `stopReason` when the
  turn ends: turns take as long as they take), `session/cancel`, `session/resume` or
  `session/load` (replays the history as updates) for a session the process hasn't got.
  Updates are `session/update` notifications (`agent_message_chunk`, `tool_call`,
  `tool_call_update`, `usage_update`, `session_info_update`…); asking is a request,
  `session/request_permission {toolCall, options: [{optionId, kind}]}`, answered `{outcome:
  {outcome: "selected", optionId}}` (kinds `allow_once`, `allow_always`, `reject_once`) or
  `cancelled`.
- `@agentclientprotocol/claude-agent-acp` 0.81.2 (the Claude Agent SDK and its own `claude`)
  honors `CLAUDE_CONFIG_DIR`: sign-in, settings (its mode starts at the account's
  `defaultMode`, e.g. bypassPermissions) and skills are the account's. The ACP session id is the
  Claude session id, and the transcript is the usual `projects/<cwd>/<id>.jsonl` with
  `entrypoint: "sdk-cli"`; there's no `sessions/<pid>.json`, so the board only knows it's live
  from acp.rs. Modes `default` (asks), `acceptEdits`, `plan`, `auto`, `bypassPermissions`; its
  `model` option takes `opus`, `sonnet`, `haiku`, `opus[1m]`… A denied call doesn't end the
  turn: the agent carries on and says so.
- `@agentclientprotocol/codex-acp` 1.13.1 runs its own bundled `codex app-server` (0.156.1)
  with `CODEX_HOME`; the session id is the thread id, and the rollout is an ordinary one with
  `originator: "codealong"`. With `approvals_reviewer = "auto_review"` in config.toml it doesn't
  ask: its reviewer model decides.
- `opencode acp` (built in) is a client of OpenCode's service; ids are its `ses_…` rows. It's
  installed in `~/.opencode/bin`, which only an interactive shell puts on PATH.
- The adapter processes are the app's children, one per account, with stderr in
  `~/Library/Logs/Codealong/acp-<account>.log`; the sessions Codealong started are in
  `hosted.json` in its folder.

Tailscale (1.102, through its command; for your other devices, tailnet.rs)

- The command is `/usr/local/bin/tailscale` (the standalone app's link), Homebrew's, or
  `/Applications/Tailscale.app/Contents/MacOS/Tailscale`. `status --json` has `BackendState`
  ("Running"), `Self`, and `Peer` keyed by node key, each with `ID`, `HostName`, `DNSName`
  (with a trailing dot), `OS` ("macOS", "iOS"), `Online`, `UserID` and `TailscaleIPs`; `User`
  maps the id, as a string, to `LoginName`; `CurrentTailnet.Name`. `whois --json <ip>` gives
  `Node.ComputedName` and `UserProfile.LoginName` for any tailnet address, this device's
  included.
- A TCP listener binds to this device's 100.x address without asking anything. A stream accepted
  from a listener that doesn't block doesn't block either on macOS, so `Wire::tcp` sets it back.

iroh (1.2; codealong-peer's net.rs)

- `Endpoint::builder(presets::N0)` uses number 0's public relays (rate-limited; their Pro plan
  or a self-hosted `iroh-relay` for real use) and its address lookup (pkarr and DNS), so a device
  dials another by key alone. The `EndpointId` is the Ed25519 public key: `PublicKey` prints as
  hex, parses hex or base32, and `fmt_short` is its first five bytes. A connection proves the
  other side's key (`remote_id`). A stream from `open_bi` reaches the other side's `accept_bi`
  only once something is written on it.
- It's async on tokio: net.rs runs a runtime of its own, and each connection's reader and
  writer `block_on` it from plain threads.

Tauri and macOS (from the Tauri 2.11 and tao 0.35 sources)

- Tauri doesn't say whether a window can be seen; AppKit's `NSWindow.occlusionState` (through
  objc2) does: visible means some part is on screen, so not minimized, hidden, on another Space
  or fully covered.
- `always_on_top` is AppKit's floating level, where other apps' panels can cover a window; the
  floating bar sets the status item level (still under menus) through objc2. Docked in the notch
  it's at `NSMainMenuWindowLevel + 2` (26), above the menu bar (24) and status items (25), as
  CodeIsland does.
- The notch (read on this Mac, a 16-inch MacBook Pro, 2026-09-28): `NSScreen.safeAreaInsets.top`
  32, and `auxiliaryTopLeftArea` / `…RightArea` the menu bar on either side of the camera, 771
  and 772 points wide on a 1728-point screen, so the notch is 185 wide and half a point right of
  the middle; the menu bar is 33 tall (`frame` top less `visibleFrame` top). A screen without one
  has a safe area of 0 and empty sides. A web view takes the clicks on its clear parts, and a
  window resized as its page animates shows, for a frame, its old picture pinned to the new
  window's corner (WebKit draws in a process of its own): opening the docked bar that way looked
  glitchy. So its window keeps the size it opens to at its biggest, with a session under its pills
  (about 940 by 620 points, clear but for the shape), the page shows the shape through an animated
  clip, and bar.rs looks at the pointer every 40 ms while it's docked, letting clicks through
  (`setIgnoresMouseEvents`) off the shape the page reports, and telling the page when the pointer
  comes onto the shape and leaves it, which a window letting clicks through doesn't. A press that
  began on the shape stays the shape's until it's let go, for a selection dragged past its edge.
- The docked bar has the keyboard only while it shows a session (`bar::set_keys`). Its panel
  answers `canBecomeKeyWindow` and `isKeyWindow` truthfully while it's asked to have it and as
  the floating bar does otherwise (no, and yes), as the peek's panel does, so the cards on the
  desktop, which share the class, never take it. A key panel hands the keyboard back to the app
  you're in when it's ordered out and at no other time AppKit offers, and the bar has to stay up,
  so `drop_keys` orders it out and in again in one turn of the run loop; AppKit has sent a turn's
  window changes to the screen together since 10.11, so nothing should show. Not yet checked by
  hand (2026-10-01): that the app you were in gets its keys back at once, and that the notch
  doesn't blink as it does. If it blinks, a second, unseen panel can take the keyboard from the
  bar and be the one ordered out.
- Only a non-activating NSPanel goes onto another app's full-screen Space; an ordinary window
  doesn't, whatever its level or `fullScreenAuxiliary`. Tauri makes NSWindows, so `bar.rs` swaps
  the bar's class to an NSPanel subclass (NSPanel adds no storage) before it's first shown: a
  window once shown as a window stays off full-screen Spaces after the swap.
- WebKit key-value observes its web view's window (`contentLayoutRect`), and KVO works by
  swapping the observed object's class (`NSKVONotifying_TaoWindow`). The panel swap replaces that
  class, so when the web view leaves the window (the window is destroyed) WebKit's
  `removeObserver:` raises "Cannot remove an observer … because it is not registered", an
  Objective-C exception Rust can't unwind through: the app aborts ("Rust cannot catch foreign
  exceptions"). A panel that goes away gets its old class back first (`bar::native::unmake_panel`,
  used by the cards pinned to the desktop; checked 2026-09-28). The bar, the peek and the quick
  panel are never destroyed.
- A click in a non-activating panel leaves the app inactive, and macOS may refuse an inactive
  app's request to bring another app forward, so the bar asks after opening (`bring_forward`).
- WebKit treats a page as active only while its window answers yes to `isKeyWindow` (re-read
  when the window is ordered on screen). An inactive page gets mouse moves only for its
  scrollbars: no DOM mouse events, no tooltips, and `:hover` stays where the last click put it
  (WebKit's WebFrame.cpp, checked in an offscreen WKWebView on 2026-09-26). The bar's panel refuses
  key, except docked while it shows a session (below), but answers yes, and allows tooltips
  while Codealong is in the background. A WKWebView takes
  mouse moves from its own tracking area, active always with classic scroll bars and in the key
  window with overlay ones, and has no `mouseMoved:` to call: a test sends them with
  `_simulateMouseMove:`.
- `proc_pidinfo` answers only for your own processes: it refuses the root `login` between a
  Ghostty tab's shell and Ghostty. `sysctl` KERN_PROC_PID, which `ps` reads, answers for any
  (`procs::parent_and_name`, `procs::tty`). On a Mac that had stalled (load in the hundreds after
  a restart, 2026-09-28) `ps` took minutes, and a killed process isn't reaped until the kernel
  lets it go, so `procs::run` reaps a timed-out one on a thread of its own.
- `pkill -f <pattern>` also matches any shell whose command line contains the pattern, such as
  another session's build; `scripts/relaunch.sh` anchors it (`codeal[o]ng$`) so only the app
  matches.
- A plain JS `listen()` hears events sent to any window with `emit_to`; listen on the window
  (`getCurrentWebviewWindow().listen`) for the ones meant for it. A window's own `emit`
  (`w.emit`) goes to every window, however it listens: send to one with `emit_to(label, …)`.
- A window's label must be listed in `src-tauri/capabilities/default.json` (`windows`, a `*` at
  the end matches the rest). One that isn't still runs Codealong's own commands, but its page's
  `listen` is refused, so it never hears an event and shows only what it read as it loaded: the
  lines between desktop cards drew one stale line that way, well off its cards (2026-09-30). The
  preview's stub allows everything, so it can't show this; a cargo test checks the labels.
- A window ordered front is scaled in by AppKit over a fifth of a second unless its
  `animationBehavior` is none (`CGWindowListCopyWindowInfo` reports its bounds smaller meanwhile).
  What another process draws can be seen without the screen: `screencapture -x -o -l <window id>`
  captures one window, its clear parts clear, and its id comes from the window list (the app's
  windows by owner name, the panels at layers 24 to 26).
- A menu item's icon can be raw RGBA, `{rgba, width, height}` (`JsImage::Rgba`), which needs
  neither the `image-png` nor the `image-ico` feature; Tauri's JS types don't list that form, but
  its `transformImage` passes it through. muda shows an item's image 18 points high.
- `show()` orders a window front without activating the app; `set_focus()` activates it.
  Commands without `async` run on the main thread.
- Transparent windows need `macOSPrivateApi` in tauri.conf.json and the `macos-private-api`
  feature. `data-tauri-drag-region="deep"` drags from anywhere inside except controls.
- WebKit holds back ResizeObserver and animation frames while a window is hidden.
- WebKit runs a hidden page's web process at background priority (`ps -o pri` 4, where a
  visible one is 47), `background_throttling` off or not: that setting only keeps it from being
  suspended. While a script the app sends runs in the page (`evaluateJavaScript`: Tauri's `eval`
  and so every event it emits) the process is at foreground priority, and the process log says
  so (`log show --predicate 'subsystem == "com.apple.WebKit" AND category ==
  "ProcessSuspension"'`). Work the page does after the script, such as awaiting a command's
  reply, can wait seconds on a busy Mac: the quick panel's open took 2.5 to 25 s at a load of 300
  in a WKWebView set up like the app's (checked 2026-09-27), and 7 to 37 ms once it did it all
  inside its opening script.
- WebKit hands an element's transform animation to the compositor but repaints a rotating path
  inside an SVG every frame: the working glyph turns its whole `<svg>` instead (app.css). With the
  bar's small feed, that took the bar's web process from 7.6% of a core to 0.7% (2026-09-25).
- WebKit also slows a visible page's timers while its app isn't frontmost: the bar's 1 s timer
  ran a second late around the clock until the bar opted out (`background_throttling`).
- The bar listens for `bar-feed` (barfeed.rs), never `board`, so Tauri doesn't send it the board.
- The quick panel (quick.rs) is a non-activating panel that takes the keyboard: its NSPanel
  subclass says yes to `canBecomeKeyWindow`, and showing it is `orderFrontRegardless`,
  `makeKeyWindow`, then making the WKWebView first responder, which a window made with
  `focused(false)` doesn't do by itself. Codealong stays inactive, so the app you were in keeps
  the menu bar. The panel listens for `board` only while it's up; an open brings the board in the
  script that opens the page (`eval_with_callback`, whose result is the page's height).
- The bar's peek (peek.rs) is a third such panel. A hover shows it with `orderFrontRegardless`
  alone, so it never takes the keyboard; a pointerdown in its page asks for `makeKeyWindow`.
  Until then its `isKeyWindow` says yes, as the bar's does, so the page gets hover and tooltips;
  taking the keyboard turns that off first, so AppKit sees a window that isn't key yet. The
  peek's page gets no pointer moves off itself, so peek.rs watches `NSEvent.mouseLocation` to
  close it. A window takes clicks on its clear parts too, so the peek's keeps only a thin margin
  on the bar's side and never covers a pill. Not yet checked by hand (2026-09-27): a click in
  the peek making it key under that override.
- A session kept on the desktop from the peek (kept.rs) is a window of its own with the peek's
  panel class, so which panels answer truthfully about being key is a list of windows
  (`peek::native`'s `ENGAGED`), not one flag. A kept window is destroyed when it's closed, so it
  gets its class back first, as a desktop card does. One that loses the keyboard is ordered in
  again saying it's key, as the peek is up from a hover, so its page still gets the pointer.
  Not yet checked by hand (2026-10-01): keeping, sizing, dragging and closing one in the real
  app; the preview only shows the page and the commands it sends.
- tauri-plugin-global-shortcut's `register` and `unregister` hand the work to the main thread and
  wait for it, so calling them on the main thread (in `setup`, or a command without `async`)
  deadlocks. Carbon's `RegisterEventHotKey` succeeds for keys another app already holds
  (⌘Space and ChatGPT's ⌥Space registered fine from a test program), so clashes can't be found
  by registering. Apps keep theirs in their preferences: the KeyboardShortcuts library as
  `KeyboardShortcuts_<name>` = `{carbonKeyCode, carbonModifiers}` (ChatGPT in `com.openai.chat`,
  Maccy in its sandbox container), macOS in `com.apple.symbolichotkeys`, which lists only the
  ones changed from their defaults.
- A context menu runs its own event loop on the main thread until it closes, and the page's IPC
  goes on being served inside it. Tauri's `plugin:menu|popup` (2.11.6, the same in 2.12.0)
  holds the webview's resource table lock all that time, and `Menu.new` and a resource's
  `close`, commands without `async`, take it on the main thread: a burst of right-clicks hung
  the app for good (2026-09-28; a `sample` showed the main thread in `popUpMenuPositioningItem`
  waiting on a mutex). `popup_menu` in lib.rs lets go of the lock before popping up.
- The background computer-use tools click through raw input: the app comes forward for a moment
  and the one before is put back, which takes the keyboard from the quick panel and hides it.
  `screencapture -l <window id>` captures a window of Codealong's alone, even ordered out.

macOS privacy prompts (TCC; macOS 26.6, checked 2026-09-27)

- In a folder macOS protects, `open()` and `opendir()` wait until its prompt ("Codealong would
  like to access…") is answered, hours on a Mac nobody sits at, while stat, access, realpath,
  chdir and an FSEvents watch there return at once. Checked in `~/Documents` (Desktop and
  Downloads are protected the same way); a USB drive held the engine in `open()` on a Mac mini.
  `git worktree list` only looks worktree paths up, so it doesn't wait. Each build is signed
  afresh, so macOS asks again after every update. The repo scan reads such folders through
  `held.rs`.
- A process started from a terminal or an agent's shell gets that app's permissions. To see a
  prompt, start it as its own responsible process: `posix_spawn` with
  `responsibility_spawnattrs_setdisclaim(&attr, 1)` (looked up with `dlsym`), as Claude.app's
  `disclaimer` helper starts Claude Code. A file symlinked into `~/Documents` makes a read
  wait on the prompt, which gives a test a held folder anywhere. A read-only disk image mounted
  with `hdiutil` asked for nothing, though `diskutil` calls it removable.
- tccd logs each request: `log show --predicate 'process == "tccd"'` has `AUTHREQ_PROMPTING`
  and `AUTHREQ_RESULT` (authValue 2 allowed, 0 denied; authReason 2 is a person's answer).

TypeSafe (Jev, from docs.typesafe.ai, 2026-09-25)

- `POST https://api.typesafe.ai/v1/systemone`, `Authorization: Bearer`, body `{model, state,
  questions}`. Questions are `choice` (up to 255 options, each described by a string or an
  object), `score` (2 to 10 levels) or `noul` (yes/no); answers carry `probabilities` and a
  `confidence`. `jev-latest` is `jev-1.13.0`: 64k tokens per request, 32k for the state plus
  the longest question; 1,200 requests a minute; $0.042 per million input tokens, output free.
  Errors are 401, 422, 429 and 529.
- It reads instructions literally and is weak at dates, counting and arithmetic, so `route.rs`
  hands it times as words and keeps each question to one plain thing.

GitHub

- Through `gh` only: one GraphQL query per repo (viewer, open and recent PRs, a
  `review-requested:@me` search), asked when a glance says the repo changed (below); PR detail from GraphQL `bodyHTML` plus the REST files
  endpoint; approving with `gh pr review --approve`. Only repos with a worktree off the default
  branch are asked that way (`board::polled_slugs`); one more query each poll searches all of
  GitHub for open PRs that wait on your review (`github::fetch_requested`), and the board lists
  its finds for the other repos checked out here. Who asked for a review is the `actor` of the
  PR's `ReviewRequestedEvent` (`timelineItems`), with the `requestedReviewer` it named; it's
  often not the author (a teammate asks for a review of a bot's PR).
- GraphQL is charged in points, 5,000 an hour for an account, shared with everything else that
  uses `gh` (you, the agents, your other Macs). A query costs the connections it could return
  over a hundred, at least 1, whatever comes back: a repo's PRs (100 open ones with four
  connections each, 40 closed with two) are 5 points for a repo with none, the search 2.
  `rateLimit { cost remaining resetAt }` in a query says, and is free. Every minute for seven
  repos that was 2,200 points an hour, and each `--snapshot` (a preview takes one) 37 more, which
  ran the account out by half past every hour (2026-10-01). So a poll is a glance first
  (github.rs, "Asking only for what changed"): one query for every repo, a point for eight of
  them, with each open PR's `updatedAt`, `headRefOid`, `mergeStateStatus` and `statusCheckRollup
  { state }` (a field of the PR itself, no connection) and the latest closed PR. A repo is read
  in full when its glance changed, each minute while its checks run, and every ten minutes (only
  the first with under 500 points left);
  answers are kept in `~/Library/Caches/Codealong/github`, so a snapshot or a relaunch reads
  from GitHub only what changed. A PR's `updatedAt` doesn't move when a check finishes, nor
  when its base moves; the checks' state and the merge state do. `mergeStateStatus` is what
  takes the time, 40 ms a PR.
- `gh api graphql` exits 1 when any part of a query failed (a repo you can't see is `null` under
  its alias, with a `NOT_FOUND` error) and still prints the rest. `gh api rate_limit` isn't to
  go by: it showed 126 GraphQL points used while the queries' own `rateLimit` said 2,073, a
  second counter that one query in ten lands on.
- REST `repos/<owner>/<name>` answers to a repo's old names (after a rename of the repo or its
  owner, or a transfer) with the repo as it is now; `.full_name` is its name now. The engine
  asks once a launch per repo, so other devices line a checkout with an old remote up with the
  repo's other worktrees (`repo_slug_now`).
- `mergeStateStatus` (no preview header needed) is GitHub's own answer to whether a PR can
  merge: `CLEAN` (or `HAS_HOOKS`) means ready, `DIRTY` is conflicts, `BEHIND` an out-of-date
  branch the repo won't merge, `BLOCKED` a required review or check (also drafts), `UNSTABLE`
  failing checks that aren't required. Approved with passing checks can still be `DIRTY`. It's
  `UNKNOWN` until GitHub has worked it out, which asking starts, so the next poll has it.

## Testing against real data

Don't use real data to test actions that reach outside the Mac or can't be undone: approving
PRs, archiving threads, typing into sessions. For Codex archiving there's an opt-in round-trip
test that unarchives and re-archives a thread that's already archived:
`CODEALONG_TEST_THREAD=<id> cargo test appserver -- --ignored`. Starting sessions
is checked without starting one: `--snapshot --start-plan` prints the launch, and
`cargo test ghostty_hands_over -- --ignored` opens a real Ghostty window that echoes
a tricky prompt back through its environment, then closes. `cargo test tty_apps -- --ignored
--test-threads=1` does the same in iTerm2 and Terminal, and types, presses keys, focuses and
closes there, on windows of its own; don't type on the Mac while it runs.

Your other devices are tested with two in the same process over loopback (`cargo test peers`):
linking, feeds, requests and remote control's refusal. By hand, `cargo test iroh_connects --
--ignored` connects two iroh endpoints on this device through the relays (it needs the
internet), and `cargo test tailnet_here -- --ignored --nocapture` reads this device's tailnet
and listens on it. Don't turn remote control on to test sends against real sessions;
`CODEALONG_DEVICE_KEY=<hex>` gives a second copy of the app a device key of its own.

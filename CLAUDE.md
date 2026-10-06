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
npm run lint                            # eslint: only the rules of hooks and their dependency lists
npm test                                # vitest (format, lanes, pins, sections)
(cd src-tauri && cargo test)            # engine rules, parsers, usage, config, launch scripts; codealong-peer's too
(cd src-tauri && cargo build --release) # the engine binary; cargo test does NOT rebuild it
scripts/checks.sh                       # every browser check (scripts/preview/*-check.js) on your real data; names run a few
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
wait|ready|link|unlink|delete|labels`, `sessions [find <words>]`, `session show|open|send|pending|answer|stop|rename|fork`, `worktrees`, `reuse`, `start`,
`config show|check`, `install`. Settings are changed by editing config.toml, not through the
command: agents edit the file well and keep its comments, and `config check` runs
`config::parse`'s checks on the result. JSON when piped, tables in a terminal (`--table` forces them). With the app up it
asks the app over `codealong.sock` (`server.rs`: JSON lines, `status`, `board`, `todo-apply`,
`start`, `run`, `show`); otherwise `engine::light_snapshot` and `todos::change` (under
`todos.json.lock`, which the engine takes for its own todo changes too). What `session` does to
a session (`send`, `stop`, `fork`…) is the app's own request in actions.rs, through `run` or,
with the app closed, `actions::local` over the light board, asked as the command
(`actions::From::Command`): what a window may, less what each request's `command` rule keeps
for the session's user (allowing a call or answering a question, a side conversation, a move),
and a message says it's an agent's. After changing
`cli.rs`, run `scripts/skill-reference.sh`: it rewrites
`skills/codealong/references/reference.md` from `--help`, and a cargo test fails while that
file is stale. `codealong reference` (hidden) prints the same text. A debug binary run with a
subcommand `main.rs` doesn't know starts the app headless and takes the socket; kill it by pid.

The engine binary doubles as a CLI for checking behavior without a window (snapshot.rs, whose
`MODES` lists them with their arguments). A session's modes are the app's own actions
(actions.rs) over a board read once, asked as another device asks them: they do what a window's
do, for a session Codealong runs and behind the same settings, and a send isn't yours.

- `codealong --snapshot [--table] [--no-github] [--usage]`: the board as JSON (or a table).
  Claude usage is read only with `--usage`; the usage endpoint rate-limits (see below).
- `codealong --snapshot --bar`: what the floating bar would show (`barfeed.rs`).
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
  the quick panel (one TypeSafe call; needs the key, and `[jev] quick_panel` on).
- `codealong --snapshot --transcript <session key> [--before <cursor>]`: a page of a session's
  conversation (prompts, replies, tool calls and their output) read from its own record, as its
  panel gets it (`transcript.rs`).
- `codealong --snapshot --pending <session key>`: what a session waits on you to allow, as its
  panel shows it (`Adapter::pending`).
- `codealong --snapshot --answer <session key> <id> <allow|deny|always> [answers]`: answers it,
  as the panel's buttons do; `answers` are its questions' answers, as JSON. Real: use it only
  on a session of your own.
- `codealong --snapshot --send <session key> <text> [--now]`: sends a session a message where it
  runs, as another device does (send.rs), into the turn it's taking with `--now`: one that would
  only open the session for someone to send is an error. Real.
- `codealong --snapshot --rename <session key> <name>`: names a session where its harness keeps
  names, as Rename… in its menu does (`Adapter::rename`). It renames it for real: use a scratch
  session, or a `HOME` of your own with scratch accounts.
- `codealong --snapshot --stop <session key>`: stops the turn a session is taking, as its Stop
  does (`Adapter::interrupt`). Real: use it only on a session of your own.
- `codealong --snapshot --fork <session key> [prompt]`: forks a session into the background, as
  `/fork` in its box does (`Adapter::fork_background`). Real: it starts a copy (a Claude Code
  background job, `claude rm <job>` after; an OpenCode session). Codex's copy runs over ACP,
  which needs the app.
- `codealong --snapshot --move <session key> <account | worktree folder>`: moves a session to
  another account of its harness, or to a worktree (Codealong's chats folder for a chat of its
  own), as Move to in its menu does. Real: it closes the session where it runs and opens it
  again elsewhere, so use a scratch session. Archiving what a move leaves behind needs the app's
  engine; this mode's light one only says it couldn't.
- `codealong --snapshot --side <session key> <question> [a follow-up…]`: asks a throwaway copy
  of a session, as `/side` in its box does (side.rs, `Adapter::side`), and prints the questions
  with their answers. Real: the copy may do what its session may without asking, so a question
  that asks for a change gets it made. Nothing of it is kept: a Claude Code copy writes no
  transcript, a Codex one no rollout, and only the app keeps what was said (`sides.json`). It
  spends the account's usage, so use a scratch session.
- `codealong --snapshot --editor-icons`: the installed editors' app icons, as the Open in menus
  get them (`editors.rs`).
- `codealong --snapshot --start-options`: where each account can start a session.
- `codealong --snapshot --skills [folder…]`: the skills a session in each folder could be asked to
  run by name, and each account's own, as the `/` lists get them (skills.rs).
- `codealong --snapshot --start-plan <folder> <account> <terminal|background|claude-app|codex-app> [prompt]
  [--model <model>] [--effort <level>] [--worktree]`: the command or link starting a session would use, without
  starting one.

## Checking UI changes

`scripts/preview.sh [view] [project] [check.js]` builds the UI, snapshots real data, stubs
Tauri's IPC with `scripts/preview/fake-tauri.js`, and renders in headless Chrome: a screenshot
in `.preview/<view>.png`, or with `check.js`, that check's result.
The stub records every command in `window.__calls`, so a check can click, double-click or
right-click and assert what the app would have sent; a command it has no answer for resolves
null and is named in `window.__unknown`. A check starts from `scripts/preview/prelude.js`, loaded
ahead of it: `say`, `wait`, `until`, `check(name, ok, why)`, `note`, `calls(cmd)`, `q` and `all`,
`rightClick` with `menuLabels` and `menuItem` for the menu it asked for, and `done()` (or
`stopped(e)` from a catch), which puts the one result in `document.title`: `done: PASS 7/7`, or
`done: FAIL 5/7 <name>: <why>; …`, then ` ## unanswered: …` (the commands that got null) and
` ## note: …` (what the check wanted said, a step skipped for want of data). `preview.sh` prints
those on a line each and exits 0 only for a PASS; a check that never says done fails, and 3
says Chrome never showed the page (it can hang as it starts on a busy Mac).
`scripts/checks.sh [name…]` builds and snapshots once, runs every `*-check.js` (or the ones
named: `scripts/checks.sh queue rename`) and prints a line for each run, exiting 1 if any
failed; `-v` adds what went unanswered. Each run is stopped after its wait and a minute and a
half more, and one whose page never showed is tried again. How a check runs is in its header: each comment line
there that is a whole command, `//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/queue-check.js`,
is one run, and a line with something to fill in (`<key>`) is for running by hand. The checks
read this Mac's board, so one can fail for want of data (no session that used Chrome) and says
what it missed. `PREVIEW_SCHEME=dark` renders as if macOS
were in dark mode; check UI changes in both. `PREVIEW_INIT=<file.js>` runs a script before the
screenshot, for a state a click gets to (Settings opens on a synthetic ⌘, keydown), and
`PREVIEW_NAME` names that screenshot. The app rewrites `document.title` as counts change; the
prelude puts back what a check said. Native menus, window creation and real opens
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
session's real conversation in its panel (others get placeholders; `PREVIEW_BEFORE=<cursor>` shows the page before that byte offset in its record), and `PREVIEW_NOTICES=scripts/preview/notices-sample.json`
sample notices in the accounts bar (a snapshot usually has none), and `PREVIEW_SKILLS=scripts/preview/skills-sample.json` sample skills in the `/` lists (one in every repo, some in the first only, one of the account's own) in place of this Mac's, and `PREVIEW_FOUND=scripts/preview/found-sample.json` sample accounts
under Settings › Accounts › Found on this Mac, and `PREVIEW_DEVICES=scripts/preview/devices-sample.json` other devices
(Settings › Devices, the sidebar's devices, a link ask, and a few of your worktrees shown as if on those devices), and `PREVIEW_CANVAS=<file>` opens the Canvas view on a saved
document (one project's from canvas.json) in place of a fresh tidy, and `PREVIEW_LOADING=1` shows the windows as
they are at launch, before the engine's first scan has finished. In check mode Chrome runs on virtual time, which stands still while scripts run (`performance.now()` too), and draws no frames, so scroll events never
arrive; and it resizes the page just before a screenshot, which closes whatever closes on resize.
`PREVIEW_WAIT=<ms>` runs a check on the real clock with frames instead, and reads its result once
its title starts with `done: `; a wait that runs out fails it. A bench (`*-bench.js`) says what it
measured in place of a result, and its title is printed as it is. A check's page is the size its
view asks for (the bar's 1100 by 64, say), the window taller by Chrome's own frame; the prelude
notes one that isn't (on virtual time a page is 500 wide at least).

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

Done means: typecheck, lint and tests pass, the engine builds without warnings, UI changes are
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
  what each carries (`api::params`), `Pending`, `Answer` and `Sent`, which harness.rs and send.rs
  re-export), a device that only
  watches (`client`), and the worktree and session model their feeds carry (`model`; the
  engine's `model.rs` re-exports it and keeps the board). So a new field on a session or lane
  goes in `peer/src/model.rs`, with its neutral value in `Session::new` or `Lane::new` there,
  which everything that builds one starts from (a session's key is written only by
  `Session::key_for`). Tests build sessions and lanes from the shared fixtures
  (`peer/src/fixtures.rs`: `session`, `lane`, and `scratch` for a folder to write in), which the
  engine's tests compile too, as `crate::fixtures`. Phones and tablets are linked devices that
  only watch
  (`peerproto::watches`): Macs list them apart (`DevicesView.phones`), don't dial them and don't
  ask them to search. The iPhone app (codealong-ios) builds on the crate at a pinned
  `origin/main` commit and talks to Macs of other builds: a message a peer doesn't know is
  skipped now, but ended the connection on builds before 2026-09-28, so new messages go only to
  peers that say they know them.
- `src-tauri/src/board.rs` holds the model's rules as pure functions with tests; each harness's
  reader gathers a session's inputs and calls them, and the engine the rest. `src/lanes.ts` mirrors `bay_of`, the agent bays'
  headlines and the order of sessions and lanes (`sort_sessions`, `lane_prompt_at`) for the
  filters; keep them in step.
- What the main window's views list (the bays, the sessions with the pinned first, the PRs and
  todos in their sections, the sidebar's and switcher's counts) is worked out from the board,
  the filters and the search in `src/boardLists.ts`, as pure functions with tests that
  `useBoardLists` keeps between pushes; a new list or filter goes there, not into `App.tsx`.
  The quick panel's words for a start and for Jev's suggestions are pure functions of the
  choice in `start.ts`, `route.ts` and `quick.ts`, with tests that pin them.
- The engine's subsystems are structs of their own with `on_*` methods, GitHub's being the first
  (`ghpoll.rs`, `GhPoll`): the engine keeps one field for it, hands it the worktrees and the
  time with each call and hears whether the board changed, `compose` reads the rest, what it
  asks in the background comes back as one `Msg` for its `on_event`, and its rules are tested
  with made-up clocks, as throttle.rs's are.
- Work the engine waits on from another thread (a flag that says it's on its way) is sent with
  `throttle::background`, whose `ended` runs however the work ends, a panic and a thread that
  never started included, so the flag always comes off; a git run that panics still answers
  (`engine::git_worker`).
- What Codealong does to a session or a worktree, or asks of a device, whichever device it's on,
  is a request in `src-tauri/src/actions.rs` (`REQUESTS`: its name, what it does here, how it
  crosses to another device when that isn't the default, and, as `only_here`, why it can't when
  it needs someone at that Mac), and who may ask for it from another device is its `Reach` in
  codealong-peer's `api::REQUESTS` (read, organize, or control, which needs remote control on).
  A window's command goes through `actions::run` (a session's key or a worktree's lane id names
  the device), `run_on` (a device by its short id) or `run_keys` (sessions on several devices,
  one request each), which ask the other device when it's elsewhere; a device's request through
  `actions::answer`; the `codealong` command's through the socket's `run` (`actions::run_as`,
  `From::Command`), which each request's `command` rule narrows. A new session action is a line
  in each, with its params a struct (`api::params`, or actions.rs's `params` for one that stays
  on its Mac), not a command with its own remote branch or its own "that's on another device".
- A terminal is a module under `src-tauri/src/launch/` (`ghostty.rs`, `iterm.rs`,
  `terminal_app.rs`, `tmux.rs`) that knows its scripts and commands, with a struct implementing
  `launch::Terminal`, listed in `launch::TERMINALS`: its name, profile and bundle ID, opening a
  tab on a command with an environment, focusing a session's tab (and again once its app is the
  active one), typing into it, pressing a key there (Terminal can't press one alone) and closing
  it; what one can't do has a default that says so. Nothing outside a terminal's module branches
  on which terminal it is: it asks `launch::terminal_app(profile)` for the one config.toml names,
  or `launch::terminal_kind(pid)` for the one a session's process runs under. `launch.rs` keeps
  what they share (where a new tab goes, the launcher, waiting for a shell, TTYs and their marks,
  taking turns). `launch/stage.rs` stands in for the Mac in tests that pin what each terminal is
  sent, script by script: a change to a script, its arguments or their order shows there.
- Everything Codealong knows of a harness's files, sockets and commands is in its module
  (`claude/`, `codex/`, `opencode.rs`, `antigravity.rs`; "Adding a harness" has how), as a
  terminal's is in its own. send.rs, usage.rs, transcript.rs and search.rs keep what every
  harness shares (choosing a way in, typing into a terminal and resuming; a usage meter; paging
  a file of lines; the index) and ask the adapter for the rest.
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
  a session's panel has in Sessions and the quick panel's reply view (⌘⏎, ⌘E, ⌘], ⌥⇧↑ ⌥⇧↓,
  ⌥↑ ⌥↓, ⌃- ⌃⇧-), over the bar's own list (`bar.ts` `waitingItems`). All three go by one set of
  rules, `src/queue.ts`: which key asks for what (`QUEUE_CHORDS`, `queueKey`), and what a key, an
  answer or a session leaving comes to given the window's list, the session shown and what's
  been dealt with (`keyStep`, `answered`, `left`; ⌘E is `seenStep`). `useWaitingQueue`
  (`src/useWaitingQueue.ts`) runs them for a window and keeps what's been dealt with and what
  the strip says; a window passes its rows, how it shows a session, and what's its own (the
  main window marks seen itself, keeps a row that left without having waited, and has ⌥↑ ⌥↓ and
  ⌃- ⌃⇧- in every view). A key for getting about goes in `queue.ts`, never into one window. The
  session's bottom corners and bottom edge drag it to a size the page keeps (`notchDragged`,
  localStorage), in the same window.
- The strip over a session's header (`SessionPanel.tsx` `QueueStrip`) says where the queue
  stands; with `hotkeys` (Sessions' panel, the notch) it's always there and has what's commonly
  done to a session as buttons that say their keys. The buttons run the session's own actions
  (`menus.ts` `sessionActions`) and read their keys from `CHORDS`, so an action given a chord
  there gets its key on the strip, in the menus and in every window at once. Snoozing has a
  chord for each of its menu's times instead (`snooze.ts` `SNOOZE_CHORDS`, ⌃⌘1 to ⌃⌘4), which
  `menus.chorded` and `keyed` run. Back, Next and × say the queue's keys, from `queue.ts`
  `QUEUE_CHORDS` and `AWAY_CHORD`, written as `CHORDS` are (`menus.chordSigns` says any of
  them); `CHORDS`' ⌘E is the queue's too. No key is written into the strip itself.
- Skills are listed by the engine (skills.rs), each once by name with what each harness that
  reads it writes in a prompt to run it (`Skill::written`: `/ship` for Claude Code, `$ship` for
  Codex). An adapter says which folders of a worktree and of an account it reads
  (`worktree_skills`, `account_skills`), how a prompt names a skill (`skill_mention`, None when
  text can't) and how often each ran (`skill_uses`); nothing is cached, the files are small. The
  windows write `/name` whatever the harness and swap it as they send or start (`src/skills.ts`
  `writtenFor`): the quick panel lists a project's repos' skills on a slash, the skill picked
  says which repo the session starts in (`skillStart`, as new work goes anywhere), and only
  accounts whose harness is in `written` are offered (asked 2026-10-04: no plain-words
  fallback); a session's reply box lists its worktree's and its account's (`SlashList.tsx`),
  under Codealong's own commands. Other devices' folders aren't read.
- A side conversation (side.rs) is a harness's throwaway copy of a session, described by its
  adapter (`Adapter::side`: the process, and a `Dialect` that turns its lines into what was
  said, the tool it runs and the end of a turn, and questions into lines). The dialects are
  tested without a process; side.rs runs them, one copy a session, tells the windows (`side`
  events), and keeps each turn as it ends (`sides.json`, read back by `side::past`). A copy
  may do what its session may without asking and no more, is told to change nothing it isn't
  asked to (`side::RULES`), has nobody to ask, and leaves no record in its harness.
- A child process that speaks JSON lines on its stdin and stdout (Codex's app-server, an ACP
  adapter, a side conversation's copy) goes through `rpc.rs`, which starts it, reads its lines
  on a thread, gives each answer to the request that waits for it and every other line to the
  caller, keeps what stderr last said for the error, and kills and reaps it when its client
  is dropped: a new one doesn't spawn and read for itself.
- What goes wrong in the background (a file that doesn't read, GitHub not answering) is a
  notice: the engine's are `Board.notices` (`Engine::notices`, GitHub's from `GhPoll::notice`:
  a key, a few words, the whole of it, and when it ends where that's known), a window's own go
  through `reportNotice` in `src/notices.ts`, with `clearNotice` once it works again, and
  `ui/Notices.tsx` draws them all as one chip at the end of the accounts bar, with a card for
  the whole of it and a dismiss that lasts while the notice stands. A window without that bar
  (the bar, the peek, the quick panel, a card) hands its notices to the board's windows
  (`drawNoticesHere`). What goes wrong with something you just did is a toast in the window you
  did it in, not a notice, and a failure left unsaid (`.catch(() => {})`) says why it doesn't
  matter.
- Codealong's small JSON files next to the config (todos.json, state.json, origins.json…) are
  read, locked and written through `src-tauri/src/store.rs`: a write is a temporary file of its
  own and a rename, and a read-modify-write is `store::change`, under the file's `.lock`. A file
  that's there but doesn't parse is an error and is never overwritten, since another build may
  have written a shape this one doesn't know; only a cache or a window's place may read as
  empty instead (`store::read_or_default`).
- What a window remembers in localStorage (a view, a filter, a width, a project picked) is read
  and written through `src/stored.ts`: `useStored` with a codec for the kind of value, or
  `readStored`/`writeStored` where a change isn't always kept. A key and the way its value is
  written never change, or people's choices are lost. A value that a listener, timer or
  callback reads after its render is `useLatest` (`latest.ts`).
- Settings are read with `config::current()`: in the app, the ones its engine runs on (the last
  good ones while config.toml doesn't parse, which a notice says); elsewhere, config.toml itself,
  or an error that names it. Nothing reads the file again for itself or falls back to the
  defaults, which have no accounts.
- What AppKit is told about the windows that float (the bar, its peek, the quick panel, the
  cards and sessions kept on the desktop, the lines between cards) is in `src-tauri/src/panel.rs`
  and nowhere else: a window's module says what kind of panel it is (`panel::Config`) and when
  it's shown, given the keyboard or hidden, and never reaches into another window's module for
  it. Rectangles, the flip between AppKit's y (up from the bottom of the main screen) and a
  page's (down from the top of its window) and which screen a thing is on are
  `src-tauri/src/geometry.rs` (`Rect`, `PageRect`, `screen_for`), not arithmetic where they're used.
- A session's, worktree's, PR's or todo's right-click menu is defined once, in `src/menus.ts`,
  and every window (main, bar, peek, quick panel, desktop cards) builds it from there, saying
  what it can do. A new item goes there, not into one window's menu. A todo's (and several
  todos', and the todo panel's ⋯) takes how the window changes todos (`TodoEdit`): an item shows
  where the window has what it needs, so a card on the desktop has a todo's status and due date
  but not its labels.
- Chips are one shape and size wherever they show (`--chip-radius`, 20 high, 11px text;
  `src/styles/chips.css` says what each fill means), and one component a kind of thing in
  `ui/primitives.tsx`: `HarnessChip` an agent, `WorktreeChip` a worktree, `PrChip` a PR
  (`TodoChip` and `LabelChip` in `TodoBits.tsx`). A row's chips go in a `.ca-chips` lane, which
  never cuts one off: each shows whole or wraps out of sight, the last first, and a long name
  shortens inside its chip. `scripts/preview/chip-lanes-check.js` checks every list for it. A
  session's account is said last, never before its title (asked 2026-10-01).
- A dialog, a popover's dismissal, a list row, a segmented control and a chip are each one
  component or hook: `Dialog` (scrim, Escape, its name, the keyboard kept and handed back),
  `Row`, `Seg` and `Chip` (with `MoreChip`, and `chipHandlers` for how a chip answers clicks) in
  `ui/primitives.tsx`, and `useDismiss` (a press elsewhere, Escape for the one opened last). A
  new one is drawn by them, not by hand, and a class list goes through `cx`.
- A right-click (or ⌃-click) on a list's row opens its menu and doesn't select it
  (`primitives.tsx`'s `selects`); contextMenu.ts rings the row while the menu is up. A menu is
  built from the row it's for, never from the selection.
- Styles: plain global CSS, every class named `ca-…` (not modules: names stay as they are, and
  `scripts/preview/` selects by them). A component's styles go in a file beside it
  (`src/ui/Sidebar.css` next to `Sidebar.tsx`; `src/App.css` is the main window's, and each
  window has its own: Bar, Notch, Peek, Quick, DesktopPin, DesktopLines, CanvasView), and what
  several share in `src/styles/`: `base.css` (elements, inline code, meta text, labels),
  `glyphs.css`, `cards.css`, `controls.css` (buttons, fields, switches), `chips.css`, `rows.css`
  (list rows, bays), `nav.css`, `menus.css`, `tree.css`, `dialog.css` and `panel.css` (the
  dialog's and the right-hand panel's shells). Read those before styling something new.
  `src/styles/app.css` is the one stylesheet code imports (in `main.tsx`), and holds only
  `@import`s, `tokens.css` first: their order is the cascade (of two equally specific rules the
  later wins), so a new file goes in at its place, not at the end, and moving one changes what
  wins. `src/styles/styles.test.ts` fails when a class is defined (named alone) in two files,
  when a rule in a component's file names none of its own classes (another file's class is
  styled there only beside one of its own: `.ca-peek .ca-thread` lives in Peek.css), when a
  `ca-` class styled is one nothing in `src/` spells (names built at run time are listed in its
  `BUILT`), when a stylesheet isn't in the index exactly once or other code imports one, and when
  `tokens.css` isn't what `npm run tokens` writes from `design/tokens.json`. Type, spacing,
  durations and stacking come from tokens too (`font: var(--type-meta)`, `gap: var(--space-1)`,
  `var(--dur-fast)`, `z-index: var(--z-popover)`; a new layer is a token in `stacking`), and the
  test fails on a `font` shorthand or a one-length `gap` that equals a token without naming it;
  a size between the steps stays literal until a designer gives it one. After moving rules
  or reordering the index, `scripts/preview/style-diff.mjs` (its header says how) records every
  scene's computed styles from the build before and the build after and compares them, element
  by element, light and dark. Colors only through CSS variables; dark values apply under
  `prefers-color-scheme` unless `data-theme` on the root says otherwise; it comes from `theme` in
  config.toml, which also sets the app's native appearance (window chrome, menus) in `lib.rs`.
  Icons are Tabler outline paths in `src/ui/icons.ts`.
- UI copy is sentence case and plain words. Comments explain why, in plain sentences.
- `rustfmt` isn't installed here, so Rust formatting is unchecked; match the surrounding code.

## Adding a harness

A harness is a module that knows its files, sockets and commands: a folder split by subject
for the big ones (`claude/`, `codex/`, the adapter and the reader in `mod.rs` and each file's
first lines saying what it holds), a file for the others (`opencode.rs`, `antigravity.rs`). Two
things in it are what the rest of the app goes through (`harness.rs`): an `Adapter`, which
answers for a session and for an account, and a `Reader`, which reads its sessions for the
board. Nothing outside a harness's module branches on which harness it is: it asks
`harness::adapter(session.harness)` or `harness::of(account)`, or every adapter
(`harness::all()`) for what any of them may have, such as the PR a line of a record says a
session made. What a harness's own sockets and windows are written is pinned byte for byte by
tests beside the code that writes it (`the_daemon_is_written_these_lines`,
`a_sessions_socket_is_written_these_lines`, `the_codex_apps_windows_are_written_these_frames`):
a change to what's sent shows there. Adding one:

1. A variant of `model::Harness` with its name in `Harness::word` and its config string in
   `Harness::config` and `from_config` (codealong-peer's `model.rs`), in `config::HARNESSES`, and
   in `src/types.ts` and `src/harness.ts`.
2. A module with a struct implementing `harness::Adapter`, listed in `harness::ADAPTERS`. It must
   say `harness`, `spec` (its folder, the variable that moves it and its command, which is how
   detect.rs finds its accounts), `records`, `targets` (the ways to open a session) and `reader`;
   everything else defaults to saying it can't. What it can answer, by what asks:
   - a session's panel: `conversation` and `conversation_after` (`transcript.rs` pages a file of
     JSON lines given a line parser), `pending`, `answer`, `deny_saying`, `answer_questions`,
     `open_calls`;
   - opening: `targets`, and `open` for a way of its own (its app); the engine does `focus`,
     `resume` (`Session::resume_command`) and `attach` for every harness;
   - sending (send.rs): `routes`, best first: typing into its terminal and resuming it in a new
     one (`resume` for what that types) are send.rs's, and a way in of its own (its service, a
     socket, its app) is a type in its module that implements `send::Own` and sends by it, told
     whose the message is and whether it goes in now (`send::Delivery`); `typed_attachments`,
     `clears_before_typing`, `takes_messages`, and for Send now `steers` and `sends_now_with`
     (the keys typed in place of Return);
   - acting on it: `fork`, `fork_background`, `move_to`, `side`, `stop`, `rename`,
     `interrupts` and `interrupt`, `archive`, `skips_permission_prompts`, `runs_its_commands`;
   - starting (start.rs): `places`, `start_in_terminal`, `runs_in_background` with
     `background_command` and `start_in_background`, `app_link` and `start_in_app`, `models`, `takes_model`,
     `offers_model`, `efforts`, `default_model`, `makes_worktrees`, `before_start`,
     `app_projects`;
   - search (search.rs): `search_files` and `prose` for records that are files of lines, `said`
     for messages kept in a database;
   - skills (skills.rs): `worktree_skills` and `account_skills` (the folders it reads them
     from), `skill_mention` (how a prompt runs one by name) and `skill_uses`;
   - its accounts: `installed`, `signed_in`, `sign_in_command`, `folder_use`, `skills_dir`,
     `installs_plugin` (Codealong goes in as a plugin, not a skill copy), `acp_command`,
     `session_cwds` (first run's projects), `chats` (the folder of sessions it starts without a
     folder of yours, which get one lane), `no_limits`, and `polls_usage` with `read_usage` for
     usage asked from somewhere slow;
   - the engine: `scratch_owner` (the session a scratch folder of its own is for, whose worktrees
     show it), `databases` (the ones in WAL mode, whose `-wal` the watcher keeps), and, for its
     records read as text without parsing, `output_marks` and `folder_marks` (a tool's output,
     and what comes before the folder a command ran in) and `pr_needles` with `pr_made` (a PR
     the session made, for prlinks.rs).
3. A `harness::Reader` in the module, which `Adapter::reader` makes and the engine keeps without
   looking inside: what the harness's files said at the last read, the folders to watch, taking
   in the files that change (`owns`, `apply`), what the watcher never says (`tick` every ten
   seconds, `glance` every second or two, `poll` for what's asked off the engine's thread), and
   its sessions, put together with a `board::*_session` constructor for its status rules, with
   where each came from (`Session::origin`: what it's a fork of, or which session started it)
   when its records say. Plan usage that comes with the sessions is `Reader::usage`. A database
   is opened with `harness::open_database`, which reads a WAL database its writer has closed.
4. What's still said harness by harness, and so needs a line for a new one: a new way in of its
   own needs a `Way` in codealong-peer, for what the reply box says (`send::Own::way`); a new
   place a session starts is a `Place` there too; a worktree folder the harness makes for itself
   is known by its path in repos.rs (`callsign`, `origin`, with an `Origin` of its own in
   codealong-peer); `board::session_headline`'s words for an approval, mirrored in
   `src/lanes.ts`; and the UI's own lists, which the engine doesn't send: names and apps in
   `src/harness.ts`, which sessions take `/fork` and `/side` in `src/slash.ts`, a new account's
   defaults and help in `src/ui/AccountsSection.tsx`, the reply box's words in
   `src/ui/SessionReply.tsx`.

## Where facts live

What other programs record and accept (most of it found by reading their code and testing on
this Mac) is written beside the code that relies on it, and what's more than that code needs is
in `.claude/rules/`. Each file there loads when a session reads or edits a file its `paths`
name; read one yourself when you need it elsewhere. Read the one for a harness before changing
its adapter, and add what you find to it, not here.

- `claude-code.md`: Claude Code and the Claude app. `codex.md`, `opencode.md`,
  `antigravity.md`, `acp.md` (the Agent Client Protocol adapters). `skills.md`: skills and the
  plugin, in every harness.
- `terminals.md`: Ghostty, iTerm2, Terminal, and starting a session in them. `chrome.md`:
  Chrome's profiles and tabs, Claude in Chrome.
- `windows.md`: Tauri, AppKit and WebKit (panels and their levels, the notch, the keyboard,
  events). `macos-privacy.md`: macOS's privacy prompts.
- `github.md`: `gh`, GraphQL points and polling. `jev.md`: TypeSafe. `devices.md`: Tailscale,
  iroh and testing linked devices.

## Testing against real data

Don't use real data to test actions that reach outside the Mac or can't be undone: approving
PRs, archiving threads, typing into sessions. Starting sessions is checked without starting one:
`--snapshot --start-plan` prints the launch. The opt-in tests that open real windows or reach
real accounts say so in their own docs and in their area's rules file (terminals, devices;
Codex's archive round trip is in `codex/appserver.rs`); don't type on the Mac while one types
into a terminal.

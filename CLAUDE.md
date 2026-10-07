# Codealong

A Mac app (Tauri 2: Rust engine, React UI) that tracks every coding agent running on this Mac,
across Claude Code accounts, Codex, OpenCode and Antigravity, organized by project → repo →
worktree → branch → PR. It shows what each agent is doing, opens any session where it lives,
and shows plan usage. README.md has the feature list, settings and code map. The design
system (tokens, components, the model and build plan) is the artifact
https://claude.ai/artifact/Eg7p4HkWgTVUR54RfYgUPc; the app has moved past it in places.

Codealong is meant for other people too: nothing in it may assume the user's own projects,
repos or naming. Behavior comes from what the harnesses record and from `config.toml`.

This file is what every change needs, and Codex reads only its first 32 KiB (`AGENTS.md` is a
link to it), so a test keeps it under that. What another program records and accepts, and how
an area of the code works, goes beside the code that relies on it, and in the `.claude/rules/`
file for that area when it's more than that code needs (see *Where facts live*).

## Commands

```bash
npm run typecheck                       # tsc
npm run lint                            # eslint: only the rules of hooks and their dependency lists
npm test                                # vitest
(cd src-tauri && cargo test)            # engine rules, parsers, usage, config, launch scripts; codealong-peer's too
(cd src-tauri && cargo build --release) # the engine binary; cargo test does NOT rebuild it
scripts/checks.sh                       # every browser check (scripts/preview/*-check.js) on your real data; names run a few
scripts/relaunch.sh                     # builds Codealong Dev, then quits it and reopens it from Applications
scripts/release.sh                      # the installer for other Macs: universal, Developer ID, notarized
```

Every checkout and worktree builds into one directory, `codealong-target` next to the main
checkout, so dependencies compile once and builds queue on cargo's lock; `scripts/target-dir.sh`
prints it (its header says how a worktree inside another gets it). The engine binary is
`<dir>/release/codealong`. The workspace's own crates (codealong, codealong-peer) build to the
same files from every checkout, and cargo goes by file times: once another session has built, a
`cargo test` in your worktree can link its copy of them instead of yours (errors about what you
just added are the sign). `touch src-tauri/src/lib.rs src-tauri/peer/src/lib.rs` before cargo
then; `relaunch.sh` always does.

Hand a change to the user with `scripts/relaunch.sh`: it waits its turn behind other relaunches,
builds, and only then quits the running app and opens the new one, which it puts in Applications
so a launcher always opens the latest build; its last line says which commit it launched and how
that stands against `origin/main`. What it builds is Codealong Dev
(`src-tauri/tauri.dev.conf.json`, its own id, name and icon), never Codealong, the app
`scripts/release.sh` builds and people install. The two share their data folder (config.toml,
todos, the socket), so one runs at a time: a copy opened the usual way asks the running one to
quit (`server::take_over`). Don't `pkill` the app by hand; a pattern that contains the app's
path also matches other sessions' shells, and kills their builds.

Don't open a second copy of the app where the user can see it. Opened the usual way it quits
theirs; a bare binary from a build folder shows up beside it as a second Codealong, and its bar
and pinned cards float over every app as the real ones do, which the user takes for their own
app misbehaving. Check UI with `scripts/preview.sh` and behavior with `--snapshot`. A copy you
must run to debug gets a home of its own (`HOME=<your scratchpad>/home …`: its own settings,
todos and socket), you say so, and you quit it when you're done. A debug binary run with a
subcommand `main.rs` doesn't know starts the app headless and takes the socket; kill it by pid.

The engine binary is also the `codealong` command for agents and people (`cli.rs`;
`codealong --help`, or `skills/codealong/references/reference.md`, lists what it does). JSON
when piped, tables in a terminal. With the app up it asks the app over `codealong.sock`
(`server.rs`); otherwise it reads a light board itself. What it does to a session is the app's
own request in actions.rs, asked as the command (`actions::From::Command`). Settings are changed
by editing config.toml, not through the command; `codealong config check` checks the result.
After changing `cli.rs`, run `scripts/skill-reference.sh`, which rewrites `reference.md` from
`--help`: a cargo test fails while it's stale.

`codealong --snapshot` checks behavior without a window (snapshot.rs, whose `MODES` has each
mode's arguments). A session's modes are the app's own actions over a board read once, asked as
another device asks them. The ones marked real act for real: use them only on a scratch session
of your own. Claude usage is read only with `--usage`, and its endpoint rate-limits: don't loop
it.

- `--snapshot [--table] [--no-github] [--usage]`: the board; `--snapshot --bar`: the floating bar's feed.
- `--snapshot --accounts`, `--snapshot --found`, `--snapshot --config`: accounts and sign-in;
  accounts on this Mac config.toml doesn't list; the settings as Settings gets them.
- `--snapshot --open <lane id> <target id>`: runs a click's open action; `--snapshot --link
  <url>`: where a click on a web link would open it (a worktree's app opens in its preview's
  Chrome profile).
- `--snapshot --pr <owner/name> <number>`: a PR's detail; `--snapshot --pr-card …`: a PR the
  board doesn't list, as a link to it gets it on hover.
- `--snapshot --route <text> [--project <name>]`: what Jev suggests in the quick panel.
- `--snapshot --transcript <session key> [--before <cursor>]`: a page of a session's
  conversation; `--snapshot --pending <session key>`: what it waits on you to allow.
- Real: `--snapshot --answer <key> <id> <allow|deny|always> [answers]`, `--snapshot --send <key>
  <text> [--now]`, `--snapshot --rename <key> <name>`, `--snapshot --stop <key>`,
  `--snapshot --done <key> [--stop]` (ends it as Mark as done does, without archiving),
  `--snapshot --fork <key> [prompt]` (a copy to remove after), `--snapshot --move <key>
  <account | worktree folder>`, `--snapshot --resume <key> <place> [prompt] [--model <model>]`
  (one that isn't running), `--snapshot --switch-model <key> <model>` (one that is),
  `--snapshot --side <key> <question> [follow-ups]` and `--snapshot --pr-side <owner/name>
  <number> <account> <question> [follow-ups]` (spend usage).
- `--snapshot --editor-icons`, `--snapshot --skills [folder…]`, `--snapshot --start-options`.
- `--snapshot --start-plan <folder> <account> <terminal|background|claude-app|codex-app>
  [prompt] [--model <model>] [--effort <level>] [--worktree]`: what a start would run, without
  starting.

## Checking UI changes

`scripts/preview.sh [view] [project] [check.js]` builds the UI, snapshots real data, stubs
Tauri's IPC and renders in headless Chrome: a screenshot in `.preview/`, or with a check
(`scripts/preview/*-check.js`), its result. Its header lists the views and every `PREVIEW_*`
variable (sample todos, usage, devices, a real transcript, dark mode…); `scripts/checks.sh` runs
every check, or the ones named. Check UI changes in light and dark (`PREVIEW_SCHEME=dark`).
Native menus, window creation and real opens can't be seen this way: say so rather than claiming
they were checked. `.claude/rules/preview.md` has what writing a check needs.

Performance work starts from `[debug] perf_log = true` in config.toml: see
`.claude/rules/performance.md`.

## Landing work

Done means: typecheck, lint and tests pass, the engine builds without warnings, UI changes are
previewed, and the work has landed. Work has landed when it's in `origin/main`
(github.com/codealong/codealong, private), the main checkout has it, and the app that's running
was built from it. Take every finished, checked piece of work that far without asking. Other
agents work in this repo at the same time, many of them in the main checkout (the first entry of
`git worktree list`), and whatever sits uncommitted there goes into their commits and builds.

1. Work in a worktree of your own: `git worktree add ../codealong-<topic> -b <topic>
   origin/main`, or the one your harness made for the session. Before building there, `npm ci`,
   and `npm run build` before cargo: Tauri's build reads `dist/`.
2. Commit with a descriptive message. Then `git fetch`, rebase your branch onto `origin/main`
   (`git rebase origin/main`), rerun the checks the rebase could affect, and `git push origin
   HEAD:main`. We prefer rebasing to merge commits: `main` stays a straight line of real
   changes. A rejected push means someone pushed first: fetch, rebase and push again.
3. Bring the main checkout up to `origin/main`: step 4's `relaunch.sh` does it after its build,
   and with nothing to relaunch, `scripts/catch-up-main.sh` does it alone. It only fast-forwards
   a main checkout on `main`, so it can't lose anything: git refuses when uncommitted edits there
   touch files the update changes, and a `main` with commits `origin/main` doesn't is left alone.
   It prints where the main checkout stands; when it couldn't catch up, leave the checkout as it
   is and say why; never stash, reset or commit another session's work to get past it.
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
   harness's to clean up when the session ends.
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

Each rule names the file whose header has the rest.

- The frontend never shells out; every side effect is a Tauri command in `src-tauri/src/lib.rs`.
  Commands without `async` run on the main thread; one that can wait on a script, another
  device, `gh` or curl goes through lib.rs's `blocking`.
- What goes between devices is codealong-peer (`src-tauri/peer/`, its `lib.rs` says what each
  module holds). A new field on a session or lane goes in `peer/src/model.rs`, with its neutral
  value in `Session::new` or `Lane::new`, which everything that builds one starts from; tests
  build them from the shared fixtures (`peer/src/fixtures.rs`). The iPhone app builds on the
  crate at a pinned commit and talks to Macs of other builds, so a new message goes only to
  peers that say they know it.
- `src-tauri/src/board.rs` holds the model's rules as pure functions with tests; each harness's
  reader gathers a session's inputs and calls them, and the engine the rest. `src/lanes.ts`
  mirrors `bay_of`, the agent bays' headlines and the order of sessions and lanes
  (`sort_sessions`, `lane_prompt_at`) for the filters; keep them in step.
- What the main window's views list is worked out from the board, the filters and the search in
  `src/boardLists.ts`, as pure functions with tests; a new list or filter goes there, not into
  `App.tsx`. The quick panel's words are pure functions in `start.ts`, `route.ts` and
  `quick.ts`, with tests that pin them.
- The engine's subsystems are structs of their own with `on_*` methods, as `GhPoll`
  (`ghpoll.rs`) is, with rules tested on made-up clocks. Work the engine waits on from another
  thread goes through `throttle::background`, so its flag comes off however the work ends.
- What Codealong does to a session or a worktree, or asks of a device, is a request in
  `src-tauri/src/actions.rs` (`REQUESTS`), and who may ask for it from another device is its
  `Reach` in codealong-peer's `api::REQUESTS`. A new session action is a line in each, with its
  params a struct, not a command with its own remote branch or its own "that's on another
  device".
- Nothing outside a harness's module branches on which harness it is (ask
  `harness::adapter(session.harness)` or `harness::of(account)`), and nothing outside a
  terminal's module on which terminal it is (`launch::terminal_app`, `launch::terminal_kind`).
  A terminal is a module under `src-tauri/src/launch/`, a variant of `HostKind` and a line in
  `launch::TERMINALS`; `launch/stage.rs` pins what each is sent. send.rs, usage.rs,
  transcript.rs and search.rs keep what every harness shares and ask the adapter for the rest.
  `.claude/rules/adding-a-harness.md` says how to add a harness.
- One thing, one component, wherever it shows: a card's face is `src/ui/card/` (`CanvasCard.tsx`
  and `DesktopPin.tsx` add where it goes and what a click does); a session with its conversation
  is `ui/SessionPanel.tsx`, its header `ui/SessionHeader.tsx`; and every window that shows one
  session as a card (the peek, a kept session, the notch, the waiting window) goes
  through `ui/SessionCard.tsx`, the ones that aren't the board's reading through
  `ui/cardHost.ts`; a PR waiting on your review, shown with a box to review it in the notch and
  the waiting window, is `ui/PrCard.tsx` the same way (the PR panel's `PrDetails`, the strip, the
  box, its chords in `menus.ts` `PR_CHORDS`; a review is one engine command, `review_pr`). A
  change goes there, never into one window's file.
- Keys for getting about what waits on you are `src/queue.ts`'s (`QUEUE_CHORDS`, `keyStep`),
  which `useWaitingQueue` runs for each window over its rows: sessions and the PRs waiting on
  your review as one list (`bar.ts` `notchRows` for the notch, `quick.ts` `queueRows` for
  the waiting window), a review leaving it through `left` as a snoozed session does. An action's key
  is in `menus.ts` `CHORDS` and shows on the queue strip, in the menus and in every window at
  once. No key is written into one window or into the strip.
- Skills are listed by the engine (skills.rs) and named by each harness's adapter; the windows
  write `/name` whatever the harness, anywhere in the text, and swap each as they send or
  start (`src/skills.ts` `writtenFor`). Only accounts whose harness can run a skill by name are offered one (asked
  2026-10-04: no plain-words fallback).
- A side conversation (side.rs, `Adapter::side`) is a harness's throwaway copy of a session: it
  may do what its session may without asking and no more, and leaves no record in its harness.
- A child process that speaks JSON lines on its stdin and stdout goes through `rpc.rs`.
- What goes wrong in the background is a notice (`Board.notices`; a window's own through
  `reportNotice` in `src/notices.ts`, and `clearNotice` once it works again); what goes wrong
  with something you just did is a toast in the window you did it in. A failure left unsaid
  (`.catch(() => {})`) says why it doesn't matter.
- Codealong's JSON files next to the config are read and written through
  `src-tauri/src/store.rs`: a file that doesn't parse is an error and is never overwritten. What
  a window remembers in localStorage goes through `src/stored.ts`, and a key and the way its
  value is written never change. A value that a listener, timer or callback reads after its
  render is `useLatest` (`latest.ts`).
- Settings are read with `config::current()`; nothing reads config.toml again for itself.
  Codealong writes it only from config.rs, under its lock (`config::update`).
- What AppKit is told about the windows that float is in `src-tauri/src/panel.rs` and nowhere
  else; rectangles, the flip between AppKit's y and a page's, and which screen a thing is on are
  `src-tauri/src/geometry.rs`, not arithmetic where they're used.
- A session's, worktree's, PR's or todo's right-click menu is defined once, in `src/menus.ts`,
  and every window builds it from there, from the row it's for, never from the selection. A
  right-click on a row opens its menu and doesn't select it.
- A dialog, a dismissal, a list row, a segmented control and a chip are each one component or
  hook in `ui/primitives.tsx` (`Dialog`, `useDismiss`, `Row`, `Seg`, `Chip`; `HarnessChip`,
  `WorktreeChip`, `PrChip`), and a class list goes through `cx`. A row's chips go in a
  `.ca-chips` lane, which never cuts one off. A session's account is said last, never before its
  title (asked 2026-10-01).
- Styles are plain global CSS, every class named `ca-…`; a component's styles go in a file
  beside it and what several share in `src/styles/` (read those before styling something new).
  `src/styles/app.css` is the one stylesheet code imports, and its order is the cascade (its
  header says where a new file goes). Colors only through CSS variables; type, spacing,
  durations and stacking from tokens (`design/tokens.json`, `npm run tokens`); a new type style
  is a new token, not a hand-written `font:`. `styles.test.ts` says what else it holds. Icons
  are Tabler outline paths in `src/ui/icons.ts`.
- UI copy is sentence case and plain words. Comments explain why, in plain sentences.
- `rustfmt` isn't installed here, so Rust formatting is unchecked; match the surrounding code.

## Where facts live

What other programs record and accept (most of it found by reading their code and testing on
this Mac) is written beside the code that relies on it, and what's more than that code needs is
in `.claude/rules/`. Each file there loads when a session reads or edits a file its `paths`
name; read one yourself when you need it elsewhere. Read the one for a harness before changing
its adapter, and add what you find to it, not here.

- `claude-code.md`: Claude Code and the Claude app. `codex.md`, `opencode.md`,
  `antigravity.md`, `acp.md` (the Agent Client Protocol adapters). `skills.md`: skills and the
  plugin, in every harness. `adding-a-harness.md`.
- `terminals.md`: Ghostty, iTerm2, Terminal, and starting a session in them. `chrome.md`:
  Chrome's profiles and tabs, Claude in Chrome.
- `windows.md`: Tauri, AppKit and WebKit (panels and their levels, the notch, the keyboard,
  events). `macos-privacy.md`: macOS's privacy prompts.
- `github.md`: `gh`, GraphQL points and polling. `jev.md`: TypeSafe. `devices.md`: Tailscale,
  iroh and testing linked devices.
- `preview.md`: writing browser checks. `performance.md`: the timing log and benches.

## Testing against real data

Don't use real data to test actions that reach outside the Mac or can't be undone: approving
PRs, archiving threads, typing into sessions. Starting sessions is checked without starting one:
`--snapshot --start-plan` prints the launch. The opt-in tests that open real windows or reach
real accounts say so in their own docs and in their area's rules file (terminals, devices;
Codex's archive round trip is in `codex/appserver.rs`); don't type on the Mac while one types
into a terminal.

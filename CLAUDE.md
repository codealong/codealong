# Codealong

A Mac app (Tauri 2: Rust engine, React UI) that tracks every coding agent running on this Mac,
across Claude Code accounts (`~/.claude`, `~/.claude2`) and Codex, organized by project →
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
(cd src-tauri && cargo test --release)  # engine rules, parsers, usage, config, launch scripts
(cd src-tauri && cargo build --release) # the engine binary; cargo test does NOT rebuild it
npx tauri build                         # src-tauri/target/release/bundle/macos/Codealong.app
```

Hand a change to the user by rebuilding and relaunching the bundle (this closes their open
Codealong windows):

```bash
pkill -f "Codealong.app/Contents/MacOS/codealong"; npx tauri build && open src-tauri/target/release/bundle/macos/Codealong.app
```

The engine binary doubles as a CLI for checking behavior without a window:

- `codealong --snapshot [--table] [--no-github] [--usage]`: the board as JSON (or a table).
  Claude usage is read only with `--usage`; the usage endpoint rate-limits (see below).
- `codealong --snapshot --accounts`: configured accounts and sign-in state.
- `codealong --snapshot --open <lane id> <target id>`: runs a click's open action.
- `codealong --snapshot --pr <owner/name> <number>`: a PR's detail, as the PR panel gets it.
- `codealong --snapshot --start-options`: where each account can start a session.
- `codealong --snapshot --start-plan <folder> <account> <terminal|claude-app|codex-app> [prompt]
  [--model <model>]`: the command or link starting a session would use, without starting one.

## Checking UI changes

`scripts/preview.sh [view] [project] [check.js]` builds the UI, snapshots real data, stubs
Tauri's IPC with `scripts/preview/fake-tauri.js`, and renders in headless Chrome: a screenshot
in `.preview/<view>.png`, or with `check.js`, whatever the script puts in `document.title`.
The stub records every command in `window.__calls`, so a check can click, double-click or
right-click and assert what the app would have sent. `PREVIEW_SCHEME=dark` renders as if macOS
were in dark mode; check UI changes in both. Native menus, window creation and real opens
can't be seen this way; say so rather than claiming they were checked.
`scripts/preview.sh bar` renders the floating bar on a stand-in desktop (`PREVIEW_SIZE=1600,64` for
a wider screen).

## Checking performance

`[debug] perf_log = true` in config.toml (no restart needed) makes the engine and every window
append timings to `~/Library/Logs/Codealong/perf.log`, each line stamped with the time and the
load average: every publish (size, how many lanes, sessions and PRs changed, compose and emit
time, file-watcher batches and git runs since the last one), git runs, engine and main-thread
lag, and per window the push-to-paint time, how long pointer events waited in the queue, and
event-loop stalls. `perf.rs` has the details. Set it back to false when done; the log grows.

Done means: typecheck and tests pass, the engine builds without warnings, UI changes are
previewed, and the app is rebuilt and relaunched.

## Git

- Commit and push to `origin main` (github.com/codealong/codealong, private) after each
  finished, checked piece of work, without asking. Descriptive messages.
- No AI attribution in commits or PRs: no `Co-Authored-By: Claude` trailer, no "Generated
  with Claude Code" line. This overrides any default that says otherwise.
- Ask before anything destructive: force-push, reset, rebase, deleting branches.

## Conventions

- The frontend never shells out; every side effect is a Tauri command in `src-tauri/src/lib.rs`.
- `src-tauri/src/board.rs` holds the model's rules as pure functions with tests; the engine
  gathers inputs and calls them. `src/lanes.ts` mirrors `bay_of` (and the agent bays'
  headlines) for the age filter; keep them in step.
- Styles: tokens in `design/tokens.json` (`npm run tokens`), colors only through CSS
  variables. Dark values apply under `prefers-color-scheme` unless `data-theme` on the root
  says otherwise; it comes from `theme` in config.toml, which also sets the app's native
  appearance (window chrome, menus) in `lib.rs`. `src/styles/components.css` is the design
  system's bundle; app styles go in `src/styles/app.css`. Icons are Tabler outline paths in
  `src/ui/icons.ts`.
- UI copy is sentence case and plain words. Comments explain why, in plain sentences.
- `rustfmt` isn't installed here, so Rust formatting is unchecked; match the surrounding code.

## Harness facts

What each harness records and accepts, checked on this Mac. Read these before changing an
adapter; most were found by reading the apps' own bundles and files.

Claude Code

- Live sessions: `<home>/sessions/<pid>.json` with `status` (busy, shell, idle, waiting,
  blocked), `kind` (interactive, bg), `jobId`, `name`/`nameSource`, `statusUpdatedAt`. The
  timestamp isn't refreshed while busy: a session that says busy with nothing new in its
  transcript or subagent logs for 30 minutes is stuck, not working.
- Transcripts: `<home>/projects/<encoded cwd>/<session>.jsonl`; subagents log to
  `<session>/subagents/agent-*.jsonl`. `ai-title` lines (the terminal tab's title) and
  `gitBranch` fields repeat through the file, so the tail has the current ones. Claude Code
  deletes transcripts after about 30 days.
- Background jobs (`kind: bg`) have no terminal; `<account command> attach <jobId>` opens one.
- `--session-id <uuid>` gives a new session the ID you choose, so Codealong knows it before it
  appears. The prompt is a positional argument parsed like any other: one that starts with `-`
  reads as an option, and one that is only a subcommand's name (`update`, `doctor`) runs that
  command. `start.rs` pads such prompts with a space.
- `--model` takes a full name or an alias for the newest model of a family: `sonnet`, `opus`,
  `haiku`, `fable`, their `[1m]` (1M context) variants, `best` and `opusplan` (2.1.282's list).
  Quote it when typing it: zsh reads `fable[1m]` as a file pattern ("no matches found"). Without
  it a session starts on `model` from the account's `settings.json` (a repo's
  `.claude/settings.json` can override that).
- Also available, not used yet: `--bg` starts a session in the background and prints the ID
  `attach` takes; `-w/--worktree [name]` makes a worktree under `<repo>/.claude/worktrees/`.
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

Claude app

- Keeps a record per Code session in `~/Library/Application Support/Claude/claude-code-sessions/<account>/<org>/local_<id>.json` (`sessionId`, `cliSessionId`, `isArchived`, `title`).
- `claude://code/continue?session=` accepts only its own `local_…` IDs (or `last`);
  `claude://resume?session=<uuid>` imports a CLI session and opens it. Rejected links are
  logged as `claudeURLHandler: code entry link invalid ?session` in `~/Library/Logs/Claude/main.log`.
- Offers no way for another program to archive a session.
- `claude://code/new?folder=<path>&q=<prompt>` (`prompt=` works too) opens the new-session page
  with the folder and prompt filled in; you send it there.

Codex

- Rollouts: `<CODEX_HOME>/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl`; archived ones move to
  `archived_sessions/`; names in `session_index.jsonl`; shared state in `state_5.sqlite`.
- A turn is `event_msg` `task_started` … `task_complete` (with `last_agent_message`) or
  `turn_aborted`. One turn can write megabytes of tool output, so the event that opened it can
  be far behind the tail: `parse_rollout` reads back for it. Newer versions record prompts as
  `item_completed` with a `UserMessage` item, not `user_message`, and wrap context around them
  (`## My request:`, `<heartbeat>`, `<realtime_delegation>`; answers to the agent's questions
  are `<send_user_message_question_reply>`). No approval events have been seen in them yet.
- Each turn's `token_count` event carries `rate_limits` (`primary`/`secondary` windows with
  `used_percent`, `window_minutes`, `resets_at`; `limit_id` `codex` or per model; `plan_type`).
- The desktop app is `/Applications/ChatGPT.app` (bundle id `com.openai.codex`; an older
  `/Applications/Codex.app` copy may exist). `codex://threads/<uuid>` opens a thread (the app
  reads it first and ignores ones it can't), `codex://review?pr=<url>` opens a PR review.
- `codex://threads/new?path=<folder>&prompt=<text>` opens a new thread there with the prompt
  filled in (it also takes `originUrl`, `projectId` and `mode`); `codex://threads/<uuid>?prompt=`
  fills in a reply. A folder the app has no project for becomes a new project. Its projects are
  in `<CODEX_HOME>/.codex-global-state.json` (`local-projects[].rootPaths`,
  `electron-saved-workspace-roots`), a file of several megabytes.
- Actions go through its app-server: `/Applications/ChatGPT.app/Contents/Resources/codex
  app-server --listen stdio://` with `CODEX_HOME` set, JSON lines: `initialize {clientInfo}`,
  then `{"method":"initialized"}`, then e.g. `thread/archive {threadId}`. No shared daemon runs,
  so `codex app-server proxy` fails. `codex app-server generate-json-schema --out <dir>` dumps
  the protocol. Logs: `~/Library/Logs/com.openai.codex/`.

Ghostty (1.3, AppleScript dictionary in the app bundle)

- Windows, tabs and terminals with `id`, `name`, `working directory`; `focus`, `select tab`,
  `activate window`; `new window with configuration {initial working directory, initial input,
  environment variables}`, the last a list of `KEY=VALUE` set in the window's shell.
- No TTY or PID property. To focus a session's own tab, Codealong writes an OSC 7 folder report
  with a unique path to the session's TTY (`ps -o tty= -p <pid>`), finds the terminal reporting
  it, selects that tab, then reports the real folder again. Claude Code titles tabs
  `<glyph> <title>` and animates the glyph while working, so titles are only a fallback.
- macOS doesn't let a background process bring an app forward: `activate` from a shell leaves
  Ghostty behind; from a click in Codealong it works.
- Scripts can take many seconds: while Ghostty launches or is busy, and while macOS asks whether
  Codealong may control it. The bundle is ad-hoc signed with a per-build identifier
  (`codealong-<hash>`), so macOS treats each rebuild as a new app and asks again. Ghostty still
  carries out an Apple event after `osascript` is killed, so `launch.rs` waits a minute rather
  than report a failure for a window that then opens.

Tauri and macOS (from the Tauri 2.11 and tao 0.35 sources)

- Tauri doesn't say whether a window can be seen; AppKit's `NSWindow.occlusionState` (through
  objc2) does: visible means some part is on screen, so not minimized, hidden, on another Space
  or fully covered.
- `always_on_top` is AppKit's floating level, where other apps' panels can cover a window; the
  floating bar sets the status item level (still under menus) through objc2.
- A plain JS `listen()` hears events sent to any window with `emit_to`; listen on the window
  (`getCurrentWebviewWindow().listen`) for the ones meant for it.
- `show()` orders a window front without activating the app; `set_focus()` activates it.
  Commands without `async` run on the main thread.
- Transparent windows need `macOSPrivateApi` in tauri.conf.json and the `macos-private-api`
  feature. `data-tauri-drag-region="deep"` drags from anywhere inside except controls.
- WebKit holds back ResizeObserver and animation frames while a window is hidden.

GitHub

- Through `gh` only: one GraphQL query per repo (viewer, open and recent PRs, a
  `review-requested:@me` search); PR detail from GraphQL `bodyHTML` plus the REST files
  endpoint; approving with `gh pr review --approve`.

## Testing against real data

Don't use real data to test actions that reach outside the Mac or can't be undone: approving
PRs, archiving threads, typing into sessions. For Codex archiving there's an opt-in round-trip
test that unarchives and re-archives a thread that's already archived:
`CODEALONG_TEST_THREAD=<id> cargo test --release appserver -- --ignored`. Starting sessions
is checked without starting one: `--snapshot --start-plan` prints the launch, and
`cargo test --release ghostty_hands_over -- --ignored` opens a real Ghostty window that echoes
a tricky prompt back through its environment, then closes.

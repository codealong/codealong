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
(cd src-tauri && cargo test)            # engine rules, parsers, usage, config, launch scripts
(cd src-tauri && cargo build --release) # the engine binary; cargo test does NOT rebuild it
scripts/relaunch.sh                     # builds the bundle, then quits and reopens the app
```

Every checkout and worktree builds into one directory, `../codealong-target` next to them
(`.cargo/config.toml`), so dependencies compile once and concurrent builds queue on cargo's lock
instead of fighting for cores. `scripts/target-dir.sh` prints it; the engine binary is
`<dir>/release/codealong` and the bundle `<dir>/release/bundle/macos/Codealong.app`. Tests
run in the debug profile: after the first build they rebuild in seconds, and the release
profile is tuned for rebuild speed too (no LTO). Hand a change to the user with
`scripts/relaunch.sh`: it waits for any bundle build in progress, builds, and only then quits
the running app and opens the new one. Builds take turns in no set order, so when its turn
comes it fast-forwards a clean checkout that `origin/main` has moved past: an older build never
replaces a newer one. Its last line says which commit it launched and how that stands against
`origin/main`. Don't `pkill` the app by hand; a pattern that contains
the app's path also matches other sessions' shells, and kills their builds.

The engine binary is also the `codealong` command for agents and people (`cli.rs`; `main.rs`
runs it for any first argument that's one of its subcommands, and starts the app otherwise):
`status`, `accounts [--usage]`, `whoami`, `todo list|show|add|update|move|start|done|reopen|
wait|ready|link|unlink|delete|labels`, `sessions [find <words>]`, `session show`, `worktrees`, `start`,
`config show|check`, `install`. Settings are changed by editing config.toml, not through the
command: agents edit the file well and keep its comments, and `config check` runs
`config::parse`'s checks on the result. JSON when piped, tables in a terminal (`--table` forces them). With the app up it
asks the app over `codealong.sock` (`server.rs`: JSON lines, `status`, `board`, `todo-apply`,
`todo-started`, `start`); otherwise `engine::light_snapshot` and `todos::change` (under
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
- `codealong --snapshot --route <text> [--project <name>]`: what Jev suggests for text typed in
  the quick panel (one TypeSafe call; needs the key).
- `codealong --snapshot --transcript <session key> [--before <cursor>]`: a page of a session's
  conversation (prompts, replies, tool calls and their output) read from its own record, as its
  panel gets it (`transcript.rs`).
- `codealong --snapshot --start-options`: where each account can start a session.
- `codealong --snapshot --start-plan <folder> <account> <terminal|background|claude-app|codex-app> [prompt]
  [--model <model>] [--worktree]`: the command or link starting a session would use, without
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
`scripts/preview.sh bar` renders the floating bar on a stand-in desktop (`PREVIEW_SIZE=1600,64` for
a wider screen), from the feed `--snapshot --bar` prints. The engine the preview runs is whatever
was last built into the shared directory, possibly another session's: `PREVIEW_BIN=<a copy of
your build>` uses yours. `PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh todo` shows
sample todos linked to your real worktrees and sessions in place of your own, and
`PREVIEW_USAGE=scripts/preview/usage-sample.json` sample plan usage in place of the snapshot's,
which has no Claude numbers, and `PREVIEW_TRANSCRIPT=<session key>` that
session's real conversation in its panel (others get placeholders), and `PREVIEW_FOUND=scripts/preview/found-sample.json` sample accounts
under Settings › Accounts › Found on this Mac. In check mode Chrome draws no frames, so scroll events never
arrive; and it resizes the page just before a screenshot, which closes whatever closes on resize.

## Checking performance

`[debug] perf_log = true` in config.toml (no restart needed) makes the engine and every window
append timings to `~/Library/Logs/Codealong/perf.log`, each line stamped with the time and the
load average: every publish (size, how many lanes, sessions and PRs changed, compose and emit
time, file-watcher batches and git runs since the last one), git runs, engine and main-thread
lag, and per window the push-to-paint time, how long pointer events waited in the queue, and
event-loop stalls. `perf.rs` has the details. Set it back to false when done; the log grows.

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
5. If a Codealong todo started the work, mark it done: `codealong todo done <id>`.

- No AI attribution in commits or PRs: no `Co-Authored-By: Claude` trailer, no "Generated
  with Claude Code" line. This overrides any default that says otherwise.
- Ask before anything destructive: force-push, reset, rebasing commits that are already
  pushed, deleting branches. Rebasing your own unpushed branch onto `origin/main` needs no ask.

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
- `src-tauri/src/board.rs` holds the model's rules as pure functions with tests; the engine
  gathers inputs and calls them. `src/lanes.ts` mirrors `bay_of`, the agent bays'
  headlines and the order of sessions and lanes (`sort_sessions`, `lane_prompt_at`) for the
  filters; keep them in step.
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
files, and an `Adapter` in it (`harness.rs`) that the rest of the app asks about a session:
where its record is, its conversation (`transcript.rs` pages a file of JSON lines given a line
parser), the ways a prompt reaches it (`send::Route`s, tried in order by `send.rs`), and what
it waits on you to allow and how to answer. Adding one:

1. A variant of `model::Harness` (and `src/types.ts`), its name in `Harness::word` and
   `src/harness.ts`, and its string in `config::HARNESSES`.
2. A module with a struct implementing `harness::Adapter`, listed in `harness::ADAPTERS`. Only
   `harness` and `records` are required; the rest default to saying it can't.
3. Reading its sessions for the board, which still lives outside the adapter: the engine's
   fields and `scan_sessions`, `watch`, `apply_fs`, `build_sessions`, `session_cwds`; a
   `board::*_session` constructor for its status rules; `board::targets` and
   `engine::open_on` for opening; `start.rs` for starting; a new `send::Route` if it takes
   prompts some new way; `usage.rs` and `detect.rs` for limits, sign-in and finding it on the Mac; `install.rs` for its
   skills folder; `search.rs` for text search. Most of these match on the harness's config
   string with a fallback, so the compiler won't list them: search for an existing harness's
   name. They move into the adapter as they're next changed.

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
  stuck.
- Transcripts: `<home>/projects/<encoded cwd>/<session>.jsonl`; subagents log to
  `<session>/subagents/agent-*.jsonl`. `ai-title` lines (the terminal tab's title) and
  `gitBranch` fields repeat through the file, so the tail has the current ones. Claude Code
  deletes transcripts after about 30 days.
- Prompts you typed, pasted or ran as a slash command are `user` lines with `origin: {kind:
  "human"}`; background tasks' notifications are `task-notification`, other sessions'
  messages `peer`, compaction summaries `isCompactSummary`. Older transcripts have no
  `origin`. `last-prompt` lines repeat the prompt's text but carry no time. One turn often
  writes more than the last 256KB, so the prompt can be far behind the tail. A message sent
  while a session is busy doesn't change the registry's `statusUpdatedAt`.
- A line's `cwd` is the shell's folder. After a command that `cd`s outside the session's
  folder the shell goes back to it, so an agent working in another worktree starts each
  command there with `cd <folder> &&`, and its other commands (temp files, `osascript`) run
  where it started. `board::session_seed` lists an agent where most of its last five
  commands and edits worked; `util::command_dir` has the rule for commands.
- Background jobs (`kind: bg`) have no terminal; `<account command> attach <jobId>` opens one.
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
- `--model` takes a full name or an alias for the newest model of a family: `sonnet`, `opus`,
  `haiku`, `fable`, their `[1m]` (1M context) variants, `best` and `opusplan` (2.1.282's list).
  Quote it when typing it: zsh reads `fable[1m]` as a file pattern ("no matches found"). Without
  it a session starts on `model` from the account's `settings.json` (a repo's
  `.claude/settings.json` can override that).
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
- Every live session (terminal, Claude app, background) listens on a Unix socket, the registry's
  `messagingSocketPath` (`/tmp/cc-socks/<pid>.sock`), for other sessions' messages
  (`peerProtocol: 1`, from 2.1.282's code). One JSON line per connection, nothing comes back on
  it: `{"msgV":1,"msg_id":<uuid>,"type":"user","session_id":<registry sessionId>,
  "message":{"role":"user","content":…},"priority":"next"}`. The auth line is optional on macOS.
  The receiver starts a turn when idle and folds it into the running one when busy, tells the
  model it came from another session and isn't the user's approval, and holds it for you when
  its permission mode differs from the sender's claimed `from-mode` (send none: a
  bypass-permissions session asks first). `send.rs` uses it for Claude app sessions.
- Background jobs take input through the account's daemon: `/tmp/cc-daemon-<uid>/<first 8 hex of
  sha256(config dir)>/control.sock`, one JSON line each way, `{"proto":1,"op":"reply","short":
  <jobId>,"text":…,"auth":<contents of <home>/daemon/control.key>}`. It's pasted into the job as
  if typed.
- A permission prompt (checked on 2.1.283, in tmux): the registry says `"status": "waiting"`,
  `"waitingFor": "permission prompt"`, and the transcript's last `tool_use` has no result yet.
  The prompt lists `1. Yes`, `2. Yes, and always allow…`, `3. No`; the digit alone answers it
  (no Return), and Escape cancels it, which denies the call and ends the turn ("Interrupted ·
  What should Claude do instead?"). A stray digit with no prompt up lands in the input box, so
  `claude.rs`'s adapter presses a key only while the registry and the transcript both agree the
  prompt is up for the call you saw. Read-only commands (`git status`, `ls`) aren't asked about.
- The folder-trust prompt a new folder gets has "No, exit" as its default: Return quits.
- A peer message to a session that bypasses permissions is held ("Held message from another
  session", Deny first) unless its settings say `"crossSessionInbound": "accept"`; typing into
  its terminal isn't held.
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
  hooks. A `SessionStart` or `UserPromptSubmit` command hook's stdout (exit 0) is added to
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
- Each turn's `token_count` event carries `rate_limits` (`primary`/`secondary` windows with
  `used_percent`, `window_minutes`, `resets_at`; `limit_id` `codex` or per model; `plan_type`).
- The desktop app is `/Applications/ChatGPT.app` (bundle id `com.openai.codex`; an older
  `/Applications/Codex.app` copy may exist). Installing it doesn't put `codex` on the PATH; its
  CLI is `Contents/Resources/codex`. The ChatGPT chat app has the same name, so Codealong knows
  the Codex app by its bundle id. `codex://threads/<uuid>` opens a thread (the app
  reads it first and ignores ones it can't), `codex://review?pr=<url>` opens a PR review.
- The app's main process routes between its windows on `<CODEX_HOME>/ipc/ipc.sock` (4-byte
  little-endian length, then JSON). A client sends `initialize`, then `thread-owner-discovery`
  (version 1) finds the window that has a thread, and `thread-follower-start-turn` (version 2)
  starts a turn there, as if typed. Answer `client-discovery-request`s with `canHandle: false`
  or the router waits 10 s on you. A mismatched version is dropped. Starting a turn from an
  app-server of your own would make a second copy of the thread the app doesn't see.
- The CLI inside the app bundle (`Contents/Resources/codex`, v0.152.1 on 2026-09-26) can be older
  than the account's default model, which the API then refuses ("requires a newer version of
  Codex"); `-m` picks another. A new folder asks to be trusted, then about new or changed hooks.
  Codealong doesn't detect Codex CLI processes: every Codex thread counts as the app's.
- `codex://threads/new?path=<folder>&prompt=<text>` opens a new thread there with the prompt
  filled in (it also takes `originUrl`, `projectId`, `mode` and `browserUrl`, and no images);
  `codex://threads/<uuid>?prompt=` fills in a reply. `mode` is chat, work or codex, not a
  worktree choice: the new-thread page has its own Local or Worktree. The app sends images as
  `{type: "localImage", path}`, and so does the app-server's `turn/start` input; the CLI's
  `-i/--image <FILE>...` attaches images to its first prompt (prompt first, or after `--`). A folder the app has no project for becomes a new project. Its projects are
  in `<CODEX_HOME>/.codex-global-state.json` (`local-projects[].rootPaths`,
  `electron-saved-workspace-roots`), a file of several megabytes.
- Actions go through its app-server: `/Applications/ChatGPT.app/Contents/Resources/codex
  app-server --listen stdio://` with `CODEX_HOME` set, JSON lines: `initialize {clientInfo}`,
  then `{"method":"initialized"}`, then e.g. `thread/archive {threadId}`. No shared daemon runs,
  so `codex app-server proxy` fails. `codex app-server generate-json-schema --out <dir>` dumps
  the protocol. Logs: `~/Library/Logs/com.openai.codex/`.

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
- `POST /api/session/<id>/prompt` with `{text, delivery: "queue"|"steer"}` sends a session a
  prompt; any TUI on that session shows it.
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

Antigravity (2.17.0, from its app bundle and files here)

- Now an Electron hub app (`/Applications/Antigravity.app`, bundle id `com.google.antigravity`),
  not the earlier VS Code fork. Its agent is `Contents/Resources/bin/language_server` (Go;
  "gemini_coder" protos), which serves the UI on random local ports (logged in
  `~/Library/Logs/Antigravity/language_server.log`) behind a CSRF token.
- App data: `~/.gemini/antigravity` (`--app_data_dir antigravity`; a separately installed IDE
  uses `~/.gemini/antigravity-ide`). `conversation_summaries.db` (SQLite, written by gorm) has
  a row per conversation: `conversation_id`, `title`, `preview`, `step_count`,
  `last_modified_time` and `last_user_input_time` (text such as
  `2026-09-24 14:16:07.434615-07:00`), `workspace_uris` (JSON list of `file://` URIs),
  `status`, `source`, `parent_conversation_id` and `nesting_depth` (subagents), `not_fully_idle`,
  `killed`, `raw_summary` (a `CascadeTrajectorySummary` protobuf). The conversations themselves
  are protobufs in `conversations/`, artifacts in `brain/`. No conversation had run on this Mac
  when the adapter was written, so `status` values are unseen: the hub's own rules are unread
  when `lastModifiedTime` passes the last view and working when `notFullyIdle`, and Codealong
  goes by those, taking a status containing waiting, blocked or paused as Needs you.
- Deep links: the app registers `antigravity://`, but its UI handles only
  `antigravity://teleport/ai_studio`; nothing opens a conversation or starts one, so Codealong
  brings the app forward. Sign-in is the token file `~/.gemini/jetski-standalone-oauth-token`
  (mode 600; only its presence is checked). Quotas show in the app only.

Ghostty (1.3, AppleScript dictionary in the app bundle)

- Windows, tabs and terminals with `id`, `name`, `working directory`; `focus`, `select tab`,
  `activate window`; `new window with configuration {initial working directory, initial input,
  environment variables}`, the last a list of `KEY=VALUE` set in the window's shell.
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
- `send key "<name>" to terminal id <id>` takes Ghostty's key names (`digit_1`, `escape`, `enter`)
  and errors with "Unknown key name" for others. While the screen is locked Ghostty doesn't
  start new terminals ("Terminal surface model is not available"), so nothing can be typed
  into one opened then.
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
- Only a non-activating NSPanel goes onto another app's full-screen Space; an ordinary window
  doesn't, whatever its level or `fullScreenAuxiliary`. Tauri makes NSWindows, so `bar.rs` swaps
  the bar's class to an NSPanel subclass (NSPanel adds no storage) before it's first shown: a
  window once shown as a window stays off full-screen Spaces after the swap.
- A click in a non-activating panel leaves the app inactive, and macOS may refuse an inactive
  app's request to bring another app forward, so the bar asks after opening (`bring_forward`).
- `pkill -f <pattern>` also matches any shell whose command line contains the pattern, such as
  another session's build; `scripts/relaunch.sh` anchors it (`codeal[o]ng$`) so only the app
  matches.
- A plain JS `listen()` hears events sent to any window with `emit_to`; listen on the window
  (`getCurrentWebviewWindow().listen`) for the ones meant for it. A window's own `emit`
  (`w.emit`) goes to every window, however it listens: send to one with `emit_to(label, …)`.
- `show()` orders a window front without activating the app; `set_focus()` activates it.
  Commands without `async` run on the main thread.
- Transparent windows need `macOSPrivateApi` in tauri.conf.json and the `macos-private-api`
  feature. `data-tauri-drag-region="deep"` drags from anywhere inside except controls.
- WebKit holds back ResizeObserver and animation frames while a window is hidden.
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
  the menu bar. The panel listens for `board` only while it's up.
- tauri-plugin-global-shortcut's `register` and `unregister` hand the work to the main thread and
  wait for it, so calling them on the main thread (in `setup`, or a command without `async`)
  deadlocks. Carbon's `RegisterEventHotKey` succeeds for keys another app already holds
  (⌘Space and ChatGPT's ⌥Space registered fine from a test program), so clashes can't be found
  by registering. Apps keep theirs in their preferences: the KeyboardShortcuts library as
  `KeyboardShortcuts_<name>` = `{carbonKeyCode, carbonModifiers}` (ChatGPT in `com.openai.chat`,
  Maccy in its sandbox container), macOS in `com.apple.symbolichotkeys`, which lists only the
  ones changed from their defaults.
- The background computer-use tools click through raw input: the app comes forward for a moment
  and the one before is put back, which takes the keyboard from the quick panel and hides it.
  `screencapture -l <window id>` captures a window of Codealong's alone, even ordered out.

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
  `review-requested:@me` search); PR detail from GraphQL `bodyHTML` plus the REST files
  endpoint; approving with `gh pr review --approve`.
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
a tricky prompt back through its environment, then closes.

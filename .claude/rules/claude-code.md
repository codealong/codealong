---
paths:
  - "src-tauri/src/claude/mod.rs"
  - "src-tauri/src/claude/registry.rs"
  - "src-tauri/src/claude/shells.rs"
  - "src-tauri/src/claude/usage.rs"
  - "src-tauri/src/claude/signin.rs"
  - "src-tauri/src/claude/mcp.rs"
  - "src-tauri/src/claude/seed.rs"
  - "src-tauri/src/claude/training.rs"
  - "src-tauri/src/claude/app.rs"
  - "src-tauri/src/claude/applog.rs"
---

# Claude Code and the Claude app

What Claude Code and the Claude app record and accept, found by reading their code and testing
on this Mac; read it before changing the Claude adapter (paths are under `src-tauri/src/`).

This file has Claude Code's live sessions and its background service, a Claude account, and the
Claude app's records. The rest is a file a subject, each loading for the code that relies on it
(a new file under `claude/` is added to the `paths` of its subject's).

The adapter, `claude/mod.rs`, loads this file alone, and what its functions rely on is spread
over all seven. Before changing one of them, read the file that names it below; this file has
`leftover`, `runs_its_commands` and `app_link`.

- `claude-transcript.md`: a transcript's lines: which are prompts, how a turn begins and ends,
  the errors and limits it stops at, usage, compaction, names and wakeups. In the adapter:
  `subagent_logs`, `subagent_logs_under`, `rename`, `sign_in_command`.
- `claude-conversation.md`: what a prompt and a call's result record, as a conversation shows
  them (images, answers, artifacts, files, the PR a call made). Nothing in the adapter.
- `claude-start.md`: starting, resuming, copying and moving a session, the folders an account
  trusts, and what records where a session came from (a job's `state.json`, `Forks`,
  `/background`'s `parked_job`). In the adapter: `start_in_terminal`, `background_command`,
  `before_start`, `start_trusts`, `offers_model`, `fork_from`, `move_folder`,
  `move_folder_background`, `move_to`, `move_to_background`, `resume_args`, `session_var`, and
  the reader's `moved_copy`, `closed_copies` and `parent_key`.
- `claude-send.md`: the input box, the socket for other sessions' messages and what it holds,
  the background service's socket, the queue, `/model` and `/effort`. In the adapter:
  `clears_before_typing`, `typed_shell_commands`, `SEND_NOW`, `steer_queued`, `steers`,
  `model_command`, `switches_mid_turn`, `live_mode`.
- `claude-asks.md`: permission prompts (`PERMISSION_PROMPT`), questions (a blocked job's
  `job_questions`), the permission mode a session runs in (`launched_with`, the Claude app's
  `Records::modes`), the plugin's hooks, and the worktree a session made, as it ends. In the
  adapter: `answer`, `waiting_call`, `waiting_on`, `helper_logs`, `helper_waiting`,
  `keyed_questions`, `job_pending`, `answer_job_questions`, `press_in_job`,
  `clears_context_first`.
- `claude-side.md`: side conversations, and why `read_registry` skips a copy's entry. In the
  adapter: `side`, `fresh_side`.

## Live sessions

- Live sessions, `<home>/sessions/<pid>.json`, their fields, and when a busy one is stuck or a
  `shell` one has left a command sitting there: `claude/registry.rs` (`RegistryEntry`),
  `board/sessions.rs` (`CLAUDE_STALL_MS`, `claude_busy_since`, `BACKGROUND_QUIET_MS`),
  `claude/shells.rs`. `statusUpdatedAt` isn't refreshed while busy. A `shell` session is never
  stuck by the 30-minute rule, but Jev can still call its background command stuck (`board/overlays.rs`
  `BACKGROUND_STALLED`). While its command is active it's counted as working but drawn waiting,
  a clock and "Waiting on `<command>`" (`Session::waits_in_background`, `lanes.ts`
  `waitsInBackground`): a CI watch looked like a stuck turn behind the spinner (2026-10-07).
  Only the terminal UI writes `shell` (2.1.312's code: idle while any `local_bash` task runs, a
  Monitor's included); without one (the Claude app, Codealong's ACP sessions, headless runs) the
  registry says only busy, waiting or idle, so the transcript says which background shells a
  session started (a Bash result's `toolUseResult.backgroundTaskId`, a Monitor's `taskId`, ids
  `b` and eight of `0-9a-z`) and which ended (a `<task-notification>` with a `<status>`; a
  Monitor's events come without one, its expiry too, an `<event>` that says "[Monitor expired
  after 30m with 2 events delivered. Re-arm it…]", which ends it; TaskStop's result `task_id`,
  which no notification follows),
  and the process table whether one still runs: `claude/transcript.rs` (`background_step`),
  `board/sessions.rs` (`untold_shell`). Read as a turn that ended, an ACP session's reply saying it waited
  on what its Monitor would report was taken by Jev for asking you (2026-10-29). What a session's
  header lists of them (`Session::tasks`): each shell named by its call's `command` and
  `description` (a Bash call's, or a Monitor's with `timeout_ms`), the file its output goes to
  from the result's words ("Output is being written to: /private/tmp/claude-<uid>/<encoded
  folder>/<session>/tasks/<id>.output"; the folder is the one the session started in, not always
  its transcript's project; a call moved there past its timeout has `timedOutAfterMs` and what it
  printed instead) or the notification's `<output-file>`, and how it ended from `<status>` and the
  summary's "(exit code N)" / "failed with exit code N" (2.1.323). The file ends `[exited with code
  N]` once it's done. A notification is in a `queue-operation` `enqueue` as the shell ends, before
  the turn takes it in: `claude/transcript.rs` (`Background`, `notifications`, `output_file`),
  `board/sessions.rs` (`claude_tasks`). Stop finds its shell as a direct child of the session's process
  (`harness::task_shell`). Not by `CLAUDE_CODE_SESSION_ID`: macOS gives no other process the
  environment of its own programs, and `procs::environment` read 0 variables for the session's
  `/bin/zsh` shells, `/bin/sleep` and `caffeinate`, even for one it started with a variable set,
  while it read Claude Code's own process and itself whole (2026-11-12, inside and outside the
  sandbox). A shell's script is a compound command, so its shell stays in the table.
- Agents and workflows run in the background keep the registry `busy` with the session's turn over
  (2.1.322's code: its status is `isLoading || delegatedActive ? "busy" : "idle"`, `delegatedActive`
  any running `local_agent`, `remote_agent`, `in_process_teammate` or `local_workflow` task, and
  `shell` only `idle` with a `local_bash` one), and `statusUpdatedAt` stays at the prompt until
  the last of them ends. The transcript says which a session started (an Agent or Workflow call's
  result, `toolUseResult.status: "async_launched"` with an Agent's `agentId` and `description`, a
  Workflow's `taskId` and `summary`) and which ended (a
  `<task-notification>` with a `<status>`, its `<task-id>` the agent's id, or one held for the turn
  as a `queued_command`; TaskStop's `task_id`), and `system` / `turn_duration` that the turn did:
  `claude/transcript.rs` (`agent_started`, `background_step`), `board/sessions.rs` (`agents_left_running`).
  Read as a turn, a session that ended a turn on a question while six agents ran showed working for
  40 minutes, its last command for an excerpt, and the question never reached Needs you
  (2026-11-10). Not followed: a foreground agent moved to the background, a finished agent resumed
  by SendMessage (both busy by the same rule), and remote agents and teammates.
- Background jobs (`kind: bg`) have no terminal; `<account command> attach <jobId>` opens one:
  `claude/registry.rs` (`RegistryEntry::job_id`).
- Warm spares (from 2.1.286), which Claude Code's own lookups and `read_registry` skip:
  `claude/registry.rs` (`RegistryEntry::spare`, `read_registry`). A spare's registry entry says
  `"spare": true`, `kind: bg`, `status: idle` and `name` its job ID, and its roster entry
  `dispatch.source: "spare"` with an empty `seed.intent`. Each spare runs in a pty host of its
  own under the service, and a job runs in the spare it claimed (the session's pid is that
  `claude bg-spare` process); with the service gone they're launchd's, and nothing claims them
  again: `claude/mod.rs` (`leftover`), which `codealong hygiene` ends.
- A Bash call's shell, a background one too, is a child of the session's process, which is how
  the `codealong` command finds its session; a terminal keeps `CODEALONG_SESSION` after its
  session ends: `claude/mod.rs` (`runs_its_commands`), `cli/whoami.rs` (`find_me`). The call is
  usually in the transcript before its command runs (`open_calls`), but not always: in a long
  session Codealong ran over ACP (2.1.280, a 20 MB transcript), commands that ran within about
  130 ms of their call's stamp read the transcript before the call was there, while one that
  ran `sleep 5` first found it. Fresh 2.1.280 runs on Haiku, Sonnet and Opus always had it
  written by the time their command started, 44 ms after the stamp (2026-10-16). Lines go
  through a write queue drained every 100 ms (`FLUSH_INTERVAL_MS`), and while a reply streams
  its lines are held back until its `stop_reason` is known. Such a session gets no process on
  the board, so the command finds it by `CLAUDE_CODE_SESSION_ID` in its shells instead
  (`cli/whoami.rs` `hosted_above`).
- Who's at a session, by `entrypoint` (registry and transcript lines): Claude Code sets
  `CLAUDE_CODE_ENTRYPOINT` to `sdk-cli` when it runs non-interactively (`-p`, or its output not a
  terminal) and nothing set it already, `cli` otherwise; the Agent SDKs set `sdk-ts` and `sdk-py`.
  Its own session picker leaves out those three and `sessionKind` `daemon`/`daemon-worker`
  (`isHiddenFromSessionPicker`, 2.1.296). A `claude -p … --no-session-persistence` run writes no
  transcript, only its `sessions/<pid>.json` while it runs, named from its folder (`judge-cwd-xx`,
  `nameSource: derived`): an eval's judge made about 1,100 of them in one folder (2026-10-13).
  Codealong calls only `sdk-cli` headless (`board/sessions.rs` `claude_session`, `board/overlays.rs` `apply_headless`): an SDK
  program is often a front end someone types into, as Codealong's own sessions are.
- The background service retires a job left idle for an hour, whether its turn ended
  (`settled`) or it sits at its prompt (`idle-prompt`): `<home>/daemon.log` says `[bg] bg retire
  <job>: idle-prompt, idle 61m`, then `bg settled <job> (done)`, and the job leaves the roster's
  `workers`. Its `state.json` keeps `state: "done"` (`lastTerminalAt` the retirement), and
  `--resume <id> --bg` brings it back under the same job ID (`bg claimed-spare <job> (fleet)`).
  On 2.1.329 (by its code), a fresh `--bg --resume <id>` job starts with no `name` in `state.json`
  and runs `agent_namer` on its next turn unless `--name <title>` sets `nameSource: "user"`
  (`claude/start.rs` `resume_job_command`).
  Seen across a day's jobs on 2.1.296 (2026-10-12); whether a setting changes the hour wasn't
  looked into. Why another session's message may resume it: send.rs (`wakes`). It doesn't
  retire one a tab is attached to (`claude attach` running): jobs attached for a day and more,
  idle for hours, ran on (2.1.317, 2026-11-05), so hibernation stops them (hibernate.rs). A job's
  worker is a process on a terminal of its own under the service's pty host (`Session::pid`),
  which what its attached tabs type is read from; prompts the service hands it (`reply`) didn't
  move that terminal's read time, and four jobs' terminals were all reread within the same
  second with nobody typing.

## Accounts

- Sign-in in the login keychain (its services, the account, reading it without a prompt) and
  never refreshing, logging or passing the token in arguments: `claude/usage.rs` (module header,
  `keychain_services`, `read_sign_in`, `fetch`).
- An account's MCP servers (2.1.306, 2026-10-20): the claude.ai connectors from `GET
  https://api.anthropic.com/v1/mcp_servers?limit=1000&include_additional_installs=true` with the
  sign-in's token, `anthropic-beta: mcp-servers-2025-12-04` and `anthropic-version: 2023-06-01`
  (the sign-in needs the `user:mcp_servers` scope), the state file's `mcpServers` and the
  plugins' (`plugin:<plugin>:<name>`): `claude/mcp.rs` (module header, `connected`),
  `claude/skills.rs` (`plugins`). Claude Code also keeps `claudeAiMcpEverConnected` in the
  state file, every connector that ever connected, which kept two the account no longer loads;
  it skips a connector whose URL a plugin's server has too (Figma here). `claude mcp list` starts
  every server to say it's connected.
- What a new account takes from another as Settings › Accounts adds it (2.1.327's code,
  2026-11-17): `claude/seed.rs` (module header). The folder's own of yours are `settings.json`,
  `CLAUDE.md`, `keybindings.json` and the folders `skills`, `commands`, `agents`,
  `output-styles`, `rules` (user-level rules, `<config>/rules`) and `themes`; the state file's
  `mcpServers` are the servers added for every folder. `skills/synced/<sign-in>` and
  `plugins/synced/<sign-in>` belong to the sign-in, and settings that sign in or pay
  (`apiKeyHelper`, `awsAuthRefresh`, `awsCredentialExport`, `gcpAuthRefresh`,
  `forceLoginMethod`, `forceLoginOrgUUID`, and in `env` the keys and tokens, the cloud
  providers' switches and credentials, which Bedrock's setup saves there, and
  `ANTHROPIC_BASE_URL`: `SIGN_IN_ENV`) to the account. A new account whose `settings.json` turns
  on plugins of marketplaces it lists (`enabledPlugins`, `extraKnownMarketplaces`) gets them
  added and installed by Claude Code itself once it runs (seen with a copied settings.json,
  2026-11-17), so the plugins themselves aren't copied.
- Help improve Claude (whether Anthropic trains on the account), read and changed through the
  account's settings: `claude/training.rs`. The settings answer holds much else (dismissed
  banners, feature flags) and `grove_updated_at`, `grove_notice_viewed_at`. Claude Code's notice
  of the new terms also reads `GET /api/claude_code_grove` (`grove_enabled` there is whether the
  notice applies, with `domain_excluded` for emails whose domain is left out of training,
  `notice_is_grace_period`, `notice_reminder_frequency`); it answered 403 to both accounts'
  OAuth tokens here (2026-10-13), so Codealong goes by the account settings alone.
- Who an account is signed in as and on what plan, for `codealong account check` and boundary
  groups: the state file's `oauthAccount` (`organizationType` `claude_team` for a Team seat here),
  and `GET https://api.anthropic.com/api/oauth/profile` with the sign-in's token, as Claude Code
  2.1.327 asks it (`account.email`, `organization.uuid`, `organization.organization_type`, which it
  maps to max, pro, team and enterprise): `claude/signin.rs` (module header).
- Plan usage, `GET https://api.anthropic.com/api/oauth/usage`: the request and its `limits`
  list in `claude/usage.rs` (`fetch`, `claude_meters`), how often the app asks and how it backs
  off in `engine.rs` (`USAGE_EVERY`, `USAGE_BACKOFF`) and `engine/polls.rs` (`keep_last_good`).
  It returns 429 when called often, so don't loop `--usage` snapshots. The same answer has
  `extra_usage`, this billing period's extra-usage spend in minor units (`claude/usage.rs`
  `extra_spend`; Claude Code 2.1.296's schema for it describes the units), and `spend`, the same
  as `{used, limit}` of `{amount_minor, currency, exponent}` with `balance` and `auto_reload`, and
  `seven_day_breakdown`, the week's share by surface (Claude Code, Chats, Cowork; read
  2026-10-12). Nothing in a transcript says which calls extra usage paid for: overage fields
  (`quotaLimits.isUsingOverage`) are only on refused turns. Claude took away its 5-hour limits
  on 2026-10-31: `five_hour` answers null and `limits` has no `session` row, on both accounts
  here. That day both were answered 429 at once right after a relaunch, and both read again 10
  minutes later; what set it off isn't known. Claude Code 2.1.316's own read (`fetchUtilization`) asks with `/api/oauth/usage` alone, keeps a
  429 for its `Retry-After`, or 5 minutes without one, an hour at most, and sends
  `cedar_ember=1`, `at_wall=1` or `starry_waterfall=<surface>` only for its limit-reached offers.
- Limit resets (2.1.327's code, 2026-11-18), which Codealong doesn't read or use (resets.rs): a
  hidden `/limit-reset` (`local-jsx`, so interactive only; with neither program on it isn't
  enabled) and `/rate-limit-options`' menu use one of two programs, each claimed with `POST
  /api/organizations/<org uuid>/reset_rate_limits`. `juniper_tide` (flag `tengu_nifty_lemur`,
  "Reset your session limit now · uses weekly limit · 1/week") resets the 5-hour limit once a
  week, `{program}` alone, answering `{result, next_available_at}`; its status is the usage
  answer's `juniper_tide` (`eligible`, `ineligible_reason`, `available`, `next_available_at`,
  `resets_per_week`), read with `?at_wall=1&skip_spend=1`, and a bare `/limit-reset` claims it
  at once, unasked. `cedar_ember` (flag `tengu_cedar_ember`) is granted resets with use-by dates,
  of kinds fable, full, 5-hour and weekly, picked in a dialog (`/limit-reset fable|full|5-hour|
  weekly`); `{program, grant_id, request_id}`, answering `{result, reason, resets_left, cleared,
  weekly_resets_at}`, its status `cedar_ember` (`eligible`, `grants[] {id, label, resets_total,
  resets_left, starts_at, ends_at, clears, paused, usable_now, use_requires_limit…}`,
  `next_grant_id`), read with `?cedar_ember=1&skip_spend=1`. Asked with Codealong's own
  User-Agent, all three accounts here answered both `eligible: false, ineligible_reason:
  "surface"`: the server offers them only to Claude Code and claude.ai (`event_props.surface`
  `claude_code_cli`, `claude_ai`). Neither would have helped: the two Max accounts have no 5-hour
  limit since 2026-10-31, the Team seat's answer had no `juniper_tide` block, and none of the
  three state files' 1,452 cached flags had `tengu_cedar_ember`; `tengu_nifty_lemur` was on only
  in `~/.claude.json`.
  Codealong's numbers outlast a relaunch in `usage.json` (`usage.rs` `keep`, `kept`), so a read
  turned away at launch keeps them too.

## Claude app

- Its record of each Code session (`local_<id>.json` under
  `~/Library/Application Support/Claude/claude-code-sessions/<account>/<org>/`), and no way for
  another program to archive a session: `claude/app.rs` (module header, `parse_record`). A
  record also has `sessionId` and `title`.
- The app's Fork, and its `btw` (a side question asked in a fork of the conversation, titled
  "btw: <question>", that goes on as a session of its own), name the session they copied in
  their record (`forkedFromSessionId`, the app's ID; a `btw`'s `spawnedFrom` also says `btw`,
  `sideSession` and `userFork`). Their transcripts start with that session's lines as they
  were, its `sessionId` on them included, where Claude Code's own forks rewrite it, so a
  transcript's session is the one its file is named for: `claude/app.rs` (`came_from`),
  `claude/transcript.rs` (`HeadInfo::copied_from`). A session started from a task another
  session's agent offered (the `spawn_task` tool's chip) has `spawnedFrom` alone, with that
  session and the `taskId` (app 2.31226, Claude Code 2.1.293 to 2.1.295, 2026-10-09).
- Opening a session: `claude://code/continue?session=` takes only the app's own `local_…` IDs,
  and `claude://resume?session=<uuid>` imports a CLI session and opens it: `claude/app.rs`
  (`claude_app_url`). `continue` also takes `last`. Rejected links are logged as
  `claudeURLHandler: code entry link invalid ?session` in `~/Library/Logs/Claude/main.log`.
- A session running in a terminal or as a background job shows in the app through its Remote
  Control, as it runs on there: `/remote-control` (`/rc`) turns it on, settings.json's
  `remoteControlAtStartup: true` for every session, and the registry entry then has
  `bridgeSessionId` (`session_…`). `claude://code/<that id>` opens it (the app's `/code/<id>`
  route takes `session_` and `cse_` ids, finding a session of its own with that Remote Control
  id, else showing the Remote Control one), and the log says `LocalSessions.setFocusedSession:
  sessionId=session_…` (2.42304, 2026-10-16): `claude/registry.rs` (`remote_control_id`),
  `claude/app.rs` (`remote_control_url`). The app's `CliSessionDiscovery` also lists its own
  account's recent terminal transcripts (its config folder's) in its sidebar, with whether each runs (from the
  registry), and tails them; it resumes one only once nothing runs it ("Running" refusal), and
  its handles for them are made up afresh each launch, so no link reaches one without Remote
  Control. Claude Code's own `/desktop` (`/app`) hands a session over instead: it saves it, opens
  `claude://resume?session=<id>` and exits; `claude --desktop --resume <id>` refuses one open in
  a terminal ("Run /desktop there instead"), 2.1.302.
- `claude://code/new?folder=<path>&q=<prompt>` opens the new-session page with the folder and
  prompt filled in, for you to send there, and takes no model or worktree: `claude/mod.rs`
  (`app_link`). `prompt=` works too. `q` is capped at 14,336 characters; `folder` can repeat;
  `ssh_host`, `ssh_port`, `ssh_folder` and `source` are taken too. `file` is only logged, so a
  new Code session can't be handed images (`claude://cowork/new?file=` does take files). The
  Code tab takes images only from its own input box.
- The approvals the app asks for itself (its built-in browser's sites, computer use), which leave
  Claude Code's registry saying `busy` while they're up, and how its log, `main.log`, records them
  as they're asked, answered and withdrawn: `claude/applog.rs` (module header, `Log::up_for`).
  Before it, a session waiting on one read as stuck after 30 minutes (2026-10-27).
- A Code session started without a folder runs in a scratch workspace the app makes for it and
  removes once the session is gone (2.9939's code): `claude/app.rs` (`scratch_dir`). The engine
  lists every such session in one lane (`engine/reposcan.rs` `chat_roots`), and the app's
  `<system-reminder>` ahead of the first prompt is skipped, as is the `<artifact-view-context>`
  ahead of a prompt sent while you look at an artifact, in the app or a terminal
  (`claude/transcript.rs` `AHEAD_OF_PROMPT`). A comment sent to a session from an artifact's
  comment thread comes as a prompt of its own, the comment quoted in what the app wraps it in
  (`claude/transcript.rs` `COMMENT_HEAD`). Both name the artifact by its uuid
  (`claude.ai/code/artifact/<uuid>`), where Claude Code's `Artifact` results link to
  `claude.ai/artifact/<id>`, the same uuid in base58: `claude/conversation.rs` (`artifact_about`,
  `artifact_id`).

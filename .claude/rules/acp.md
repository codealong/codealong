---
paths:
  - "src-tauri/src/acp.rs"
  - "src-tauri/src/rpc.rs"
---

# Agent Client Protocol

What the Agent Client Protocol (schema v1) and its adapters take and do, for the sessions
Codealong runs itself (acp.rs), moved from CLAUDE.md's Harness facts; the adapters were checked
on 2026-09-27. Paths are under `src-tauri/src/`.

- The protocol as Codealong speaks it: `initialize` (no `fs` or `terminal` offered),
  `session/new`, `session/prompt` and its `stopReason`, `session/cancel`, `session/resume` or
  `session/load`, `session/fork {sessionId, cwd, mcpServers}` where the agent offers
  `sessionCapabilities.fork`, `session/set_config_option`, the `session/update`s it reads and
  `session/request_permission` with its answers: `acp.rs` (header, `Agent::initialize`,
  `start`, `adopt`, `fork`, `load`, `run_turn`, `cancel`, `on_update`, `on_permission`, `answer`). Other
  updates it passes over include `usage_update` and `session_info_update`. ACP has no word for
  where a fork ends; both adapters take JetBrains AIR's `_meta` for it (`acp.rs` `fork_point`).
- `@agentclientprotocol/claude-agent-acp` 0.81.2 (the Claude Agent SDK and its own `claude`),
  run with the account's folder as `CLAUDE_CONFIG_DIR`, but none for `~/.claude`, whose sign-in
  a named folder's keychain item doesn't hold (`claude/mod.rs` `usual_folder`, `acp_launch`;
  named, every turn ended "Not logged in"): sign-in, settings (its mode
  starts at the account's `defaultMode`, e.g. bypassPermissions) and skills are the account's.
  The ACP session id is the Claude session id, and the transcript is the usual
  `projects/<cwd>/<id>.jsonl`. Its `entrypoint` was `sdk-cli` in early runs; on 2026-10-13 a
  0.81.2 session's transcript and its claude's (2.1.280) `sessions/<pid>.json` both said
  `sdk-ts`, `kind: interactive`. acp.rs (`overlay`) marks it hosted, and never headless. Modes `default` (asks),
  `acceptEdits`, `plan`, `auto`, `bypassPermissions`; its `model` option takes `opus`,
  `sonnet`, `haiku`, `opus[1m]`… A denied call doesn't end the turn: the agent carries on and
  says so. A session its command line started (an `sdk-cli` one from `claude -p` among them)
  is taken on by its ID with `session/resume`, its conversation and all, and its model set after
  (`acp.rs` `adopt`, `acp_takes_on_a_session`; checked 2026-10-06): the transcript gets a
  `/model` line for the settings' model as it's resumed and another for the one asked for.
- claude-agent-acp's `claude` is the adapter's child, started with `--permission-mode <mode>`
  and `--setting-sources=user,project,local`, so it runs the account's hooks, the Codealong
  plugin's `PermissionRequest` hook among them; its `sessions/<pid>.json` (2.1.280) lets that
  hook find its process. The board gives a hosted session no pid (`acp.rs` `overlay`); the
  `claude` itself isn't started with `CLAUDE_CODE_SESSION_ID`, but the shells it runs commands
  in are given the session's ID, which is how the `codealong` command run there finds its
  session (`cli.rs` `hosted_above`; the app's own environment had none, 2026-10-16). A call
  it asks about comes both ways: as the adapter's `session/request_permission` and through the
  hook. The app holds only the first (server.rs
  `hold_permission_ask`, `acp::runs`): one held through the hook as well showed the session
  waiting on a call the panel couldn't answer (2026-10-14). Claude Code reads files in
  parallel, and each read outside the folder asked at once, three requests waiting together
  (2026-10-14, Opus 5.5 reading three pasted images in `default` mode): each waits for its own
  answer (`Hosted::permissions`). The ACP SDK withdraws one whose call was aborted with a
  `$/cancel_request {requestId}` notification and still waits for its answer (`on_withdrawn`).
- A turn the agent takes by itself once a prompt's turn has ended (a background command's or
  `Monitor`'s `<task-notification>`, another session's message: the origins in claude-agent-acp
  0.81.2's `AUTONOMOUS_RESULT_ORIGINS`) comes with no `session/prompt` to answer, but the adapter
  goes on streaming it as out-of-turn `session/update`s, and asks its permissions out of turn
  too. Its `claude`'s (2.1.280) `sessions/<pid>.json` says `busy` from the turn's start
  (`statusUpdatedAt` then) to `idle` at its end, so the board takes the records' word for a turn
  that began, or wrote something, after Codealong's last ended (`acp.rs` `overlay`: a
  notification that comes as a prompt's turn ends may follow it without an `idle` between, by the
  adapter's notes on 2.1.270, and Codealong hears the prompt's answer only after the turn it
  answers has ended); before, such a session showed as
  your turn while it worked (2026-10-29, a Haiku session woken by its `sleep 20` in the
  background). That `claude` never says `shell`: between turns, while what it left running in
  the background goes on, it says `idle`, so the transcript says which shells it started and
  the process table whether one runs (board.rs `untold_shell`, `claude-code.md`), and `overlay`
  leaves a session waiting on one waiting. The adapter publishes background tasks
  (`dist/async-tasks.js`) only to a client that says it takes AIR's async tasks, which Codealong
  doesn't yet.
- claude-agent-acp's modes are its config option `mode` (category `mode`), each `{value, name,
  description}` (`dist/session-mode.js` `buildAvailableModes`: default "Manual", acceptEdits,
  plan, auto, and bypassPermissions when bypass is allowed, as Codealong's launch allows it). A
  session starts at `permissions.defaultMode` from the account's settings, else `default`, so
  without that setting it asks, even where Claude Code's terminal starts in auto
  (`hasSeenAutoDefaultNotice` in `~/.claude.json`, 2026-10-14). `session/set_config_option`
  with `configId: "mode"` switches it at once, mid-turn too (`query.setPermissionMode`), and the
  agent says of switches it makes itself (a plan approved "and use auto mode", a model that
  doesn't take auto) with `current_mode_update {currentModeId}` and `config_option_update
  {configOptions}`, which also comes before its answer to a `set_config_option`: `acp.rs`
  (`switch_mode`, `on_update`, `modes_offered`).
- A plan to approve is a `session/request_permission` whose `toolCall.kind` is `switch_mode`,
  the plan in `rawInput.plan` (`acp.rs` `is_plan`). claude-agent-acp 0.81.2 asks so for
  `ExitPlanMode` (`toolCall.name` too, title "Approve Plan", `_meta.permission.title` "Ready to
  code?"), with these options (`dist/permissions/options/tools.js`
  `buildExitPlanModePermissionOptions`, sorted allow_once, allow_always, reject):
  `exit-plan-default` "Yes, manually approve edits" (allow_once, the mode then `default`); with
  a plan, `exit-plan-clear-auto` / `-bypass` / `-accept-edits` "Yes, clear context (12% used)
  and use auto mode" (allow_always: the call is denied with `interrupt`, and the same ACP turn
  goes on in a fresh query on "Implement the following plan:" and the plan, the conversation
  gone); `exit-plan-auto` "Yes, and use auto mode" and `exit-plan-bypass` "Yes, and bypass
  permissions" for the modes the session offers, bypass first when it bypassed before it
  planned (`prePlanMode`), else `exit-plan-accept-edits` "Yes, auto-accept edits"; `reject` "No,
  keep planning" (reject_once: a deny with `interrupt`, the turn ending as cancelled).
  `dist/permissions/effects.js` says what each does. codex-acp 1.13.1 asks "Implement this
  plan?" with `implement_plan` (allow_once) and `revise_plan` (reject_once). Which option each
  of Codealong's answers gives: `acp.rs` (`pick`, `clears_context`).
- `@agentclientprotocol/codex-acp` 1.13.1 runs its own bundled `codex app-server` (0.156.1)
  with `CODEX_HOME` (`codex/mod.rs`, `acp_command`); the session id is the thread id, and the
  rollout is an ordinary one with `originator: "codealong"`. The board gives such a session no
  process; its commands carry the thread's ID in `CODEX_THREAD_ID`, which is how the `codealong`
  command run there finds its session however it was run (`cli.rs` `hosted_above`). With `approvals_reviewer =
  "auto_review"` in config.toml it doesn't ask: its reviewer model decides. It can put a prompt
  into the running turn, which Codealong doesn't use yet: the extension request
  `_session/steering {sessionId, prompt}` (`turn/steer` underneath), offered in `initialize`'s
  answer as `_meta.steering.supported` (checked 2026-10-13). Its `outcome` is `injected`,
  `failed`, or `startedNewTurn` when the turn had ended, a turn it starts itself whose end no
  answer reports, so `acp.rs` would show it done while it runs, and a queued prompt sent then
  would start a turn over it. Its `thread/status/changed` comes as a `session_info_update` with
  `_meta.codex.threadStatus`, which might mark that end (not checked). A `session/prompt` during a
  turn starts a new turn rather than steering.
- `opencode acp` (built in) is a client of OpenCode's service; its ids are the `ses_…` rows.
  Where it's installed and the PATH it gets: `acp.rs` (`adapter_path`).
- The adapter processes, one per account and children of the app, their stderr logs and
  `hosted.json`: `acp.rs` (header, `agent`, `log_path`, `remembered_path`).
- When the app quits, each adapter's input closes and codex-acp 1.13.1 interrupts the turns it
  runs: the rollout ends the turn with `turn_aborted`, `reason: "interrupted"`, as a Stop does, and
  its log has `write_stdin failed: Unknown process id …` for the commands it was running and
  `failed to record rollout items: thread … not found` (2026-10-12: a Codex review cut off by a
  relaunch, its `turn_aborted` 1.5 s before the next app's start). So the record can't tell that
  from a Stop: hosted.json notes each turn while it runs, and one still noted at launch was cut off
  (`acp.rs` `note_turn`, `Hosted::cut_off`, `overlay`). claude-agent-acp 0.81.2's `claude`
  (2.1.280) ends the call it was making with the result "The user doesn't want to proceed with
  this tool use. The tool use was rejected…" and then "[Request interrupted by user for tool
  use]", as if you had said no (2026-10-15: three relaunches cut one session off, each about 5 s
  before the next app's perf log began). Codealong tells such a session to carry on as it comes
  back, saying the rejection was the cut (reconnect.rs, `start::CARRY_ON`).

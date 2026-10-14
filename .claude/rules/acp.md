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
  updates it passes over include `usage_update` and `session_info_update`.
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
  rollout is an ordinary one with `originator: "codealong"`. With `approvals_reviewer =
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
  (`acp.rs` `note_turn`, `Hosted::cut_off`, `overlay`).

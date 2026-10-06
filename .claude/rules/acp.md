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
  run with `CLAUDE_CONFIG_DIR` (`claude/mod.rs`, `acp_command`): sign-in, settings (its mode
  starts at the account's `defaultMode`, e.g. bypassPermissions) and skills are the account's.
  The ACP session id is the Claude session id, and the transcript is the usual
  `projects/<cwd>/<id>.jsonl` with `entrypoint: "sdk-cli"`; there's no `sessions/<pid>.json`,
  so the board only knows it's live from acp.rs (`overlay`). Modes `default` (asks),
  `acceptEdits`, `plan`, `auto`, `bypassPermissions`; its `model` option takes `opus`,
  `sonnet`, `haiku`, `opus[1m]`… A denied call doesn't end the turn: the agent carries on and
  says so. A session its command line started (an `sdk-cli` one from `claude -p` among them)
  is taken on by its ID with `session/resume`, its conversation and all, and its model set after
  (`acp.rs` `adopt`, `acp_takes_on_a_session`; checked 2026-10-06): the transcript gets a
  `/model` line for the settings' model as it's resumed and another for the one asked for.
- `@agentclientprotocol/codex-acp` 1.13.1 runs its own bundled `codex app-server` (0.156.1)
  with `CODEX_HOME` (`codex/mod.rs`, `acp_command`); the session id is the thread id, and the
  rollout is an ordinary one with `originator: "codealong"`. With `approvals_reviewer =
  "auto_review"` in config.toml it doesn't ask: its reviewer model decides.
- `opencode acp` (built in) is a client of OpenCode's service; its ids are the `ses_…` rows.
  Where it's installed and the PATH it gets: `acp.rs` (`adapter_path`).
- The adapter processes, one per account and children of the app, their stderr logs and
  `hosted.json`: `acp.rs` (header, `agent`, `log_path`, `remembered_path`).

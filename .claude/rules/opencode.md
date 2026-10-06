---
paths:
  - "src-tauri/src/opencode.rs"
---

# OpenCode

What OpenCode 2.0.16 records and accepts, moved from CLAUDE.md's Harness facts, which came from
its source at that tag, its compiled bundle and its files on this Mac. Paths are under
`src-tauri/src/`.

- Data folder `~/.local/share/opencode` (`opencode debug paths` prints every folder;
  `XDG_DATA_HOME` moves it). The tables and the columns read: `opencode.rs` (header, `ROW`,
  `SessionRow`). In `session_message`'s `data`, a user message has `text`, and an assistant's
  `content` has parts of type text, reasoning or tool, a tool part with `name` and
  `state.input`. An assistant message's `time` has `created`, `streamed` and `completed`; the
  reply is written into the same row as it streams, and every finished one on this Mac had
  `completed` (`opencode.rs`, `streaming`, `transcript_newer`; not yet watched mid-reply). The
  `event` table stayed empty when a session was made through the API, so a
  run in progress is a user message newer than `time_idle` (`SessionRow::turn_open`).
- The background service is `opencode serve --service`, which the TUI starts.
  `~/.local/state/opencode/service.json` is mode 600. `GET /api/session/active` answers `{data:
  {ses_…: {type: "running"}}}`, `GET /api/session` lists sessions (it takes `directory`), and
  `/openapi.json` has the rest. What Codealong reads and asks, signed in how, how often, and
  the 15-minute stale cap without a service: `opencode.rs` (`service_at`, `poll`, `get`,
  `Reader::poll`), `board.rs` (`opencode_session`, `OPENCODE_STALE_MS`).
- The TUI (`opencode [dir]`) is a client of the service: `--session <id>` (`-s`) continues a
  session, `--continue` (`-c`) the folder's last, and `--prompt <text>` sends a prompt. With or
  without `--session`, the TUI puts the text in its box and submits it once the model is ready,
  if the box still holds it unchanged (2.0.16's bundle, its home and session routes). Nothing
  lets Codealong choose a session's id, so a started session shows once OpenCode writes it. The
  TUI titles its tab `OC | <title>` (can be turned off), and two terminals can show one
  session. `mini` is a small interactive TUI and counts as a terminal; `opencode run` is
  non-interactive and doesn't. Which processes are terminals and which session each shows:
  `opencode.rs` (`terminal_args`, `terminal_for`).
- Tools run in the service, not the TUI, and the service keeps the environment of the shell
  that first started it, `CODEALONG_SESSION` included. How `codealong` finds the session a
  command runs in, from its tool part not yet completed: `opencode.rs` (`open_calls_in`;
  checked with `opencode run`, 2026-09-27).
- Sending a prompt, queued or into the running turn: `opencode.rs` (`Posted`, `prompt_body`).
- Renaming, `PATCH /api/session/<id>` with `{title}`: `opencode.rs` (`OpenCode::rename`). It
  answers 204, `time_updated` moves, and it takes `metadata` and `permissions` too.
- Stopping a run: `opencode.rs` (`OpenCode::interrupt`, `interrupted`). The row gets
  `idle_outcome` `interrupted`; `?resume=true` is what the TUI's double Escape sends.
- A fork's row (`fork_session_id`, `fork_boundary`, from 2.0.16's schema; older databases have
  neither): `opencode.rs` (`read_sessions_from`).
- Forking through the service, the copy's answer and the refusal of an empty session:
  `opencode.rs` (`OpenCode::fork`, `forked_id`, `opens_the_fork_its_service_made`). `{before:
  <message id>}` copies only what came before that message. The command line can't fork.
- Permission requests: `GET /api/session/<id>/permission` lists them (`id` `per_…`, `action`
  such as `shell`, `resources` such as the command, `source.id` the tool call), and a reply
  (`opencode.rs`, `permission_request`, `OpenCode::answer`) moves the TUI on as if you'd
  answered there. A reject takes a `message` beside it, what to do instead (the service's
  schema, 2026-10-05; not yet watched on a run). Shell commands in the project aren't asked about unless a config says so
  (`opencode.json`: `"permission": {"bash": "ask"}`); `--auto` approves everything not denied.
  The free models (`opencode/…`) need no sign-in.
- While a session runs only `opencode.db-wal` changes, which FSEvents doesn't report:
  `harness.rs` (`DatabaseMarks`), `opencode.rs` (`Reader::glance`), `engine.rs`
  (`DATABASES_EVERY`).
- Sign-in and plan limits: `opencode.rs` (`signed_in`, `OpenCode::sign_in_command`,
  `OpenCode::no_limits`).
- Creating a session through the API in a folder made OpenCode point its `global` project
  (its project for folders without version control) at that folder.

---
paths:
  - "src-tauri/src/antigravity.rs"
---

# Antigravity

What Antigravity's hub app 2.17.0 and its command line, `agy` 1.2.14, record and accept, moved
from CLAUDE.md's Harness facts, which came from the app bundle, the command line's binary and
runs on this Mac (2026-10-01). Paths are under `src-tauri/src/`.

- Two programs on one sign-in: `antigravity.rs` (header, `APP_BUNDLE_ID`, `is_app_agent`,
  `cli_installed`). The hub app is Electron (`/Applications/Antigravity.app`), not the earlier
  VS Code fork; its agent, `Contents/Resources/bin/language_server` (Go; "gemini_coder"
  protos), serves the UI on random local ports (logged in
  `~/Library/Logs/Antigravity/language_server.log`) behind a CSRF token and has none of the
  command line's flags. `agy` is a Go binary signed by Google that updates itself; its
  installer puts it in `~/.local/bin` and adds that to the PATH in `~/.zshrc`.
- The two folders under `~/.gemini`, read as one account, and the shared sign-in token:
  `antigravity.rs` (`folders`, `Antigravity::signed_in`). A separately installed IDE uses
  `antigravity-ide`. The token file is mode 600, and `~/.gemini/config` holds the skills,
  plugins and MCP servers both share. The command line can't go on with one of the app's
  conversations ("trajectory not found").
- `conversation_summaries.db` (SQLite in WAL mode, written by gorm), its columns, statuses and
  times, and each folder's table listing the other's conversations: `antigravity.rs`
  (`read_from`, `Conversation`, `timestamp`). `title` was empty in every row seen, and
  `raw_summary` is a protobuf. No status says error or waiting.
- Opening a WAL database whose harness has quit: `harness.rs` (`open_database`).
- `conversations/<id>.db` holds a conversation's steps (`steps`: `idx`, `step_type`, `status`,
  and protobuf blobs). The numbers are in the binary's own definitions: status 1 pending,
  2 running, 3 done, 4 invalid, 5 cleared, 6 canceled, 7 error, 8 generating, 9 waiting,
  11 queued, 12 interrupted. A step whose permission prompt is up is waiting (9, `STEP_WAITING`),
  with the summary running; a call a print run couldn't ask about is canceled (6).
- The transcript (`brain/<id>/.system_generated/logs/transcript_full.jsonl`, and
  `transcript.jsonl` with each argument a string of JSON), its fields, the `<USER_REQUEST>`
  wrapping, results as steps of their own in the calls' order, lines out of step order, and a
  result's text: `antigravity.rs` (`transcript_file`, `step`, `request_of`, `Call`, `tail_of`,
  `result_of`). Steps are written as they finish, `created_at` to the second, so a step that
  waits on you isn't there until it's answered: the planner's call is the last line.
- Tool arguments (`toolAction`, `toolSummary`, `CommandLine`, `Cwd`, `AbsolutePath`,
  `TargetFile`): `antigravity.rs` (`call`).
- `agy` opens its terminal UI: `-i <prompt>`, `--conversation <id>`, `--model` with what `agy
  models` lists, and `--add-dir` are in `antigravity.rs` (`Antigravity::start_in_terminal`,
  `Antigravity::resume`, `models`, `listed_models`, `cli_args`). `-c` goes on with the folder's
  last conversation (`cache/last_conversations.json`), `--add-dir` repeats, and it takes
  `--effort`, `--mode accept-edits|plan` and `--sandbox`. A prompt that starts with `-` is taken
  as the option's value. There's no way to choose a conversation's id.
- `-p <prompt>` is a print run: one turn, then it exits. `--output-format json` prints
  `{conversation_id, status, response, num_turns, usage}`; `stream-json` prints an `init`
  event, `step_update`s (`step_type` user_input, agent_response with `text_delta`, tool with
  `tool_info`) and a `result`. `--input-format stream-json` takes `{"event":"user","message":
  {"content":…}}` lines, a turn each, and stays up while its stdin is open. A print run can't
  ask: a call that needs permission is denied, the turn ends with `denied_actions` in its
  result and exit 0. `--dangerously-skip-permissions` allows everything, and
  `~/.gemini/antigravity-cli/settings.json` (`permissions.allow`) what it lists; reads and
  writes in the workspace aren't asked about.
- A print run writes its summary row only as it ends; the terminal UI writes it as it goes:
  `antigravity.rs` (`started`), `board.rs` (`antigravity_session`).
- Which `agy` has which conversation, by the `presence/<id>.lock` it holds open: `antigravity.rs`
  (header, `held_conversation`, `processes`). The lock files stay behind after.
- The terminal UI's first run asks for a color scheme and then shows Terms of Service and a
  data-use choice, ticked by default; print runs go ahead without it
  (`cache/onboarding.json`). A prompt given with `-i` runs behind those screens, in the
  command line's `scratch` folder with no workspace on its row (`Antigravity::chats`).
- It has no Agent Client Protocol mode, so Codealong can't run it itself. Google says running
  the binary as a child process on the user's sign-in is supported and counts against the same
  limits.
- No deep link opens or starts a conversation, and quotas show only in the app: `launch.rs`
  (`open_antigravity`), `antigravity.rs` (`Antigravity::no_limits`).

## Unchecked

- The terminal UI wouldn't go past its first-run screens here, so these weren't checked: the
  keys its permission prompt takes and whether Escape stops a turn (so Codealong doesn't answer
  its prompts or stop its turns), and how it takes a paste, though Codealong types into a
  command line conversation's terminal (`Antigravity::routes`, `Route::Type`).
- What the app writes mid-turn.
- Its hooks (`~/.gemini/config/hooks.json`; the binary names `PreToolUse` and `PostToolUse`).

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
- The two folders under `~/.gemini`, read as one account, and the shared sign-in:
  `antigravity.rs` (`folders`, `signed_in`). A separately installed IDE uses
  `antigravity-ide`. The sign-in was a token file (mode 600) until `agy` 1.3.4 moved it into the
  login keychain (service `gemini`, account `antigravity`; go-keyring, whose item `security
  find-generic-password` without `-w` finds without a prompt) and deleted the file, on a sign-in
  on 2026-10-13. `~/.gemini/config` holds the skills, plugins and MCP servers both share. The
  command line can't go on with one of the app's conversations ("trajectory not found").
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
  writes in the workspace aren't asked about. With `--input-format stream-json` the prompt
  still has to be given, empty, as `-p ''` and first: `-p --input-format …` takes the flag as
  the prompt. Its `init` event names the tools and `permission_mode` (`request-review`), but
  1.3.1 has no event to ask a permission over stdin or answer one.
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
- Every `agy`, print runs too, runs its agent as a language server inside its own process, on
  two random ports on localhost: HTTPS for gRPC and plain HTTP, both ConnectRPC, named in its
  log (`log/cli-<time>.log`, "Language server listening on random port"). It serves the hub's
  `exa.language_server_pb.LanguageServerService` (`StartCascade`, `SendUserCascadeMessage`,
  `HandleCascadeUserInteraction`, `CancelCascadeInvocation`…), but every call without the
  `x-codeium-csrf-token` header is refused ("missing CSRF token", 401), and the token is never
  in its arguments, its environment or a file (checked on 1.3.1, 2026-10-08). So Codealong
  can't drive a running `agy` through it.
- Remote Control (`agy --remote-control`, `/remote-control`, or the `agy remote-control
  start|status|stop` daemon, which runs the hidden `agy remote-control serve` from a
  LaunchAgent) carries that same server to Google's own clients, the dashboard on
  antigravity.google.com and the hub app, over WebRTC and a reverse tunnel signed in as the
  user (antigravity.google/docs/remote-control). From there you can follow a terminal
  session, send it prompts, answer its permission prompts and, through the daemon, start
  conversations on the Mac. Nothing local comes of it: no port, socket or token for another
  program. The daemon signs in with the command line's own sign-in, kept in the keychain
  (`serve` says "not signed in: run `agy`, sign in" before `start` has run); the daemon name
  is `cliRemoteControlHostname` in `~/.gemini/config/config.json`; an account or org can have
  it turned off ("Remote control is not enabled for your account.").
- No deep link opens or starts a conversation: `launch.rs` (`open_antigravity`).
- Plan usage is the command line's `/usage` (or `/quota`, the same) in a print run, `agy -p
  /usage --output-format json`, which since 1.1.11 answers without a turn, spends no quota and
  leaves no conversation (its changelog, `~/.gemini/antigravity-cli/cache/CHANGELOG.md`; the
  conversation count held): `antigravity.rs` (`read_usage`, `usage_answer`, `usage_meters`).
  The answer's `command.data.groups` are groups of models that share a limit, each with
  `buckets` of `{id, name, window, remaining_fraction, reset_time}`; on 1.3.4 (2026-10-13) two
  weekly ones, "Gemini Models" and "Claude and GPT models", "tied directly to your individual
  tier", with no tier's name. An untouched bucket's `reset_time` is a week from the asking, a
  little later each time; one in use has a fixed one. A read takes about two seconds and needs
  only the sign-in, not the app. Each run, print runs too, writes a log of about 18 KB in
  `log/cli-<time>.log` (a thousand there by then) unless `--log-file` names a file, which it
  writes over. Signed out, it prints a Google sign-in URL on stderr and waits for the browser
  instead of exiting, so the sign-in is checked first. `/credits` (AI credits) said "no credits
  info found" for this account. The app reads the same quotas from
  `cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels` and its language server has
  `GetUserStatus` and `RetrieveUserQuotaSummary`, both behind its CSRF token, so without the
  command line there's nothing Codealong reads (`Antigravity::no_limits`).

## Unchecked

- The terminal UI wouldn't go past its first-run screens here, so these weren't checked: the
  keys its permission prompt takes and whether Escape stops a turn (so Codealong doesn't answer
  its prompts or stop its turns), and how it takes a paste, though Codealong types into a
  command line conversation's terminal (`Antigravity::routes`, `Route::Type`).
- What the app writes mid-turn.
- Where the conversations that Remote Control's daemon starts are written, and how they'd
  show on the board: starting the daemon registers the Mac with Google, so it wasn't run.
- Its hooks (`~/.gemini/config/hooks.json`; the binary names `PreToolUse` and `PostToolUse`).

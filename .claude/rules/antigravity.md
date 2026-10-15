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
  11 queued, 12 interrupted. A step whose permission prompt is up is waiting (9, `STEP_WAITING`);
  a call a print run couldn't ask about is canceled (6). A reply is generating (8) from its first
  words, so while the model thinks before them no step is under way (`STEP_UNDER_WAY`,
  `steps_now`). Escape in the terminal UI cancels a running call (6), which never reaches the
  transcript, or keeps a reply cut short as done; Escape while the model thinks writes no step,
  your prompt staying the latest, and only the row says the turn ended. The terminal UI closes
  the database about 30 seconds after its last write, its WAL and `-shm` going with it.
- Step types seen (1.3.5, 2026-10-14): 14 your prompt, 15 the model's reply, 17 an error's
  note, 101 a system message (a background command's result, which starts the agent again), 132
  a command, and an `ask_question` too. The note a failed turn ends with ("Agent execution
  terminated due to error", an `agent executor error` after it) has words for you in its payload (field 24.3.1, and 24.5 is
  1); the notes the agent goes on after carry only the error (24.3.2): a request it tries again,
  "API error (attempt N): …", one a try (eight in a row while the network was down), and a
  stream cut off that it picks up, "The stream was interrupted. Please continue…" (every note on
  this Mac; `ERROR_FOR_YOU`, `ends_turn`). In the transcript a note is `ERROR_MESSAGE`, its
  content empty. A reply that makes calls turns done in
  the same write that adds its first call's step (watched every 4 ms), so a done reply or a
  turn's error note as the latest step is a turn's end (`STEP_ENDS_TURN`, `steps_now`). A call that fails
  (7) hands its error to the model, which goes on (19 of 19 here); a canceled one (6) ended its
  turn every time (`STEP_CANCELED`). No step was ever interrupted (12).
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
  `Antigravity::resume`, `models`, `listed_models`, `cli_args`). `agy models` asks Google and
  answered in 4 s at a load near 900 (1.3.5, 2026-10-14), so its list is read in the background,
  kept an hour, asked again a minute after a read that got none, and kept in
  `~/Library/Caches/Codealong/agy-models.json` for the next launch (`KeptModels`,
  `kept_models_path`). `-c` goes on with the folder's
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
- A print run writes its summary row only as it ends. The terminal UI writes its row as a turn
  starts (running, the time and step count as they were), when a step waits on you, as a
  command's step starts (running, counting that step; 1.3.5) and as the turn ends (idle, with
  the time and count of its latest step), not at each step: `antigravity.rs` (`started`),
  `board.rs` (`antigravity_session`). A command that runs long goes to the background on its
  own: the turn ends with the agent saying it waits, the row idle with `not_fully_idle` 1, the
  command's step running (2); its result comes back as a `SYSTEM_MESSAGE` step (type 101) that
  starts the agent again, the row written running (1.3.5, in tmux, 2026-10-14).
- Every `agy` that starts, print runs too (Codealong's usage read every couple of minutes among
  them), writes every conversation it doesn't run itself back to the command line's table as
  idle (`IDLE`, `not_fully_idle` 0; "summary store: starting background reconciliation
  (trigger=startup)" in its log), the rest of the row from its own copy,
  `jetbox_summaries_proto.pb` beside the table, which can be far older: on 2026-10-14 a turn
  the UI had written at 182 steps went back to 114 steps and a time 30 minutes earlier, exactly
  the file's copy, and a finished conversation's row from 93 steps to 78. Its log counts them
  ("[Summaries] reconcile checked 33 conversations, restored 1", at 17:28, 17:34 and 17:38
  during that turn). So a turn in a
  terminal UI reads idle, with an older time and count, until that UI writes its row again,
  which can be minutes: `antigravity.rs` (`idled_by_another`), `board.rs`
  (`antigravity_session`). A row's step count matches its database's steps after its own UI's
  turn end, but a finished one's can be put back too, so a count short of the steps doesn't by
  itself say a turn is under way. Neither `ANTIGRAVITY_APP_DATA_DIR` nor `JETSKI_APP_DATA_DIR`
  moves a run to another folder, and another `HOME` loses the sign-in, so a usage read can't
  keep out of the table (1.3.4, 2026-10-13).
- Which `agy` has which conversation, by the `presence/<id>.lock` it holds open: `antigravity.rs`
  (header, `held_conversation`, `processes`). The lock files stay behind after.
- Each `agy` writes a log, `log/cli-<YYYYMMDD_HHMMSS>.log` by its local start time, keeps it open,
  and names itself in its third line (`Starting language server process with pid N`); a UI was
  seen holding another `agy`'s log open beside its own (`open_logs`, `own_log`). Its lines are
  glog's, `I1014 17:45:37.928030 <thread> <file>:<line>] <text>`, this Mac's time without the
  year. What a terminal UI writes of its conversation (every log on this Mac, 1.3.4 and 1.3.5,
  2026-10-14): `HandleUserInput called with text: "<text>"` as Return sends what's typed,
  Codealong's typing too; `Forwarding user message to conversation <id>` as a prompt goes in,
  milliseconds after the input when no turn runs, and for `-i`'s prompt with no input line before
  it; `Cancelling in-progress response for conversation <id>` at Escape while a reply is on its
  way. A message typed while a turn runs is held until the turn ends (`Queued Messages` in
  `/config`: Queue by default, or Send Immediately, `"queuedMessages": "send-immediately"` in
  `settings.json`), and then `Auto-flush: sending N queued input(s)` and one forwarding line send
  them all as one prompt. From the binary's strings, unseen: `Auto-flush SendUserMessage failed,
  restoring N pending input(s)` holds them again, and `Pending inputs cancelled (N regular + M
  shell dropped)` drops them. Nothing else says a held message came: on 2026-10-14 one typed at
  17:45 waited 31 minutes, behind a request that hung for 18 and failed, and reached the
  transcript only then. No line marks a model request's start, only its response
  (`streamGenerateContent … ResponseID`), so a request in flight can't be seen; a turn its UI
  started and no step has ended can (`Said`, `LogRead`, `Conversation::ui_turn_open`).
- The terminal UI's first run asks for a color scheme and then shows Terms of Service and a
  data-use choice, ticked by default; print runs go ahead without it
  (`cache/onboarding.json`). A prompt given with `-i` runs behind those screens, in the
  command line's `scratch` folder with no workspace on its row (`Antigravity::chats`).
- In the terminal UI (1.3.4, 2026-10-13, in tmux), a folder not in `settings.json`'s
  `trustedWorkspaces` is asked about first (↑/↓ and Return, no digits), and a yes adds it there.
  A permission prompt is a menu: for a command 1 run it, 2 always in this conversation for
  commands that start the same way, 3 always (written to `settings.json`), 4 no, cancel; for a
  file outside the workspace 1 allow, 2 always allow non-workspace access (not always there), then
  no. ↑/↓ and Enter pick, the digit alone answers, Tab amends a command and Ctrl+G edits it,
  Escape cancels. The call's step is waiting (9) while it's up. Escape stops a turn
  ("Interrupted · What should Antigravity CLI do instead?"). Codealong answers a prompt with 1 or
  Escape (`Antigravity::answer`) and doesn't stop its turns.
- Questions (`ask_question`, 1.3.5, in tmux, 2026-10-14): the call's arguments, the dialog's keys
  and the result's `A1: …` lines are in `antigravity.rs` (`questions_of`, `question_keys`,
  `answers_of`). Its step waits (9) while the dialog is up, under `always-proceed` too, and the
  transcript has the call but no result until it's answered, so its questions show at once.
  Not said there: the dialog asks one question at a time ("Question 1/3"), ↑/↓ and → move,
  Space toggles too, a skipped question shows `(skipped)` in the terminal, and Ctrl+C only says
  "press ctrl+c again to exit". Its keys go only to 9 (`KeyChoice1` to `KeyChoice9` in the
  binary), and none clears a question's ticks except skipping it. Keys arriving together as fast as tmux sends them, a paste
  among them, are taken in order. The binary also names "auto-answer ask_question" (`skipped=true`)
  and "user cancelled ask_question", which nothing seen did.
- Workspace trust: which folders count, how the path is taken, and trusting one before a start:
  `antigravity.rs` (`TRUSTED_WORKSPACES`, `trust_folder`, `Antigravity::before_start`). The
  settings are `settings.json` in the command line's folder, written by agy indented by two
  spaces with a newline at the end, mode 600. A signed-out agy shows its sign-in before the trust
  question; a hub project's `folderUri` (`~/.gemini/config/projects/<id>.json`) is trusted too.
  Behind the trust question a `-i` prompt ran with no workspace at all, and asked before reading
  each file of the folder it was started in (two helpers' reviews on 2026-10-13); one started
  in a folder trusted from outside went straight in (1.3.4, in tmux). In a home of its own agy
  is signed out: it reads the keychain through `/usr/bin/security`, which follows `HOME`.
- The permissions a turn ran with, in its prompt's step (`steps.step_payload`, a
  `gemini_coder.Step`; step type 14 is a prompt): `antigravity.rs` (`turn_mode`, `TOOL_CONFIG`,
  `policy_mode`, `proto_at`). The settings' keys (`/config` writes them, a default removes its
  key): `agentMode` (`accept-edits`, `plan`), `toolPermission` (`strict`, `always-proceed`,
  `proceed-in-sandbox`; `request-review` when absent), `enableTerminalSandbox`,
  `allowNonWorkspaceAccess`, `artifactReviewPolicy`, `permissions {allow, deny, ask}`. Shift+Tab
  cycles default, accept-edits, plan and isn't saved; `--mode`, `--sandbox` and
  `--dangerously-skip-permissions` are never saved. The command line's log names its process
  (`Starting language server process with pid N`) and says `toolPermission` as it starts, each
  override a flag made, and `SetCycleMode called: <mode>` at every switch; nothing writes the
  agent mode to the conversation (plan puts `/plan ` before the prompt, by its code, unseen).
  The hub app's own settings are `userSettings` in `~/.gemini/config/config.json`, per project
  under `settings` in `projects/<id>.json`; its conversations seen had only the model per turn.
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
- Enable Telemetry (whether Google collects interactions to improve its models) is the
  account's on Google's servers, which the app and `agy` read as they start and change in their
  settings: `antigravity.rs` (Enable Telemetry section). `agy` never writes it to its
  `settings.json`, whatever the docs list there (its field is `json:"-"`), and reads it as off
  when the read fails. Google turns away a call whose User-Agent doesn't say Antigravity. Gemini
  Code Assist for individuals has an opt-out of its own on the same backend
  (`getCodeAssistGlobalUserSetting`, `freeTierDataCollectionOptin`), which nothing in Antigravity
  calls; it read false here (2026-10-13).

## Unchecked

- How the terminal UI takes a paste of several lines, though Codealong types into a command line
  conversation's terminal (`Antigravity::routes`, `Route::Type`); a message of one line it typed
  arrived whole, as one input (2026-10-14).
- What taking queued messages back into the editor (↑) writes to the log, and whether Escape
  drops the queue or sends it.
- A plan-mode turn's record, and the `strict`, `proceed-in-sandbox` and `auto` policies on a real
  turn: read from its code only.
- What the app writes mid-turn.
- Where the conversations that Remote Control's daemon starts are written, and how they'd
  show on the board: starting the daemon registers the Mac with Google, so it wasn't run.
- Its hooks (`~/.gemini/config/hooks.json`; the binary names `PreToolUse` and `PostToolUse`).

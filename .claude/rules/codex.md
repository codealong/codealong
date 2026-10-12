---
paths:
  - "src-tauri/src/codex/**"
---

# Codex

What Codex's app and CLI record and accept, moved from CLAUDE.md's Harness facts, which were
checked on this Mac, most of them by reading the apps' own bundles and files. Paths are under
`src-tauri/src/`.

- Rollouts, `archived_sessions/` and the thread index (`session_index.jsonl`):
  `codex/rollout.rs` (header, `archived_rollout_files`). Codex's shared state is in
  `state_5.sqlite`.
- Turns (`task_started` … `task_complete` or `turn_aborted`), reading back for the event that
  opened one, and how prompts are recorded and wrapped: `codex/rollout.rs` (`parse_rollout`,
  `prompt_of`, `request_of`).
- A turn cut off partway, and how Codealong knows nothing is running it any more: a terminal UI
  killed in a command (0.157.1 run without the daemon, in tmux, 2026-10-12) leaves its rollout
  after `task_started`, the call and its output, and a `token_count`: no `task_complete` or
  `turn_aborted`, and its command runs on. Whatever has a thread loaded holds its rollout open:
  the terminal UI without the daemon, the daemon behind one with it, the app's app-server (every
  thread it has loaded), the app-server under codex-acp. `codex/terminals.rs` (`held_rollouts`),
  `codex/mod.rs` (`let_go_now`, `let_go`: not when `ps` failed, nor for a rollout grown since the
  last look, which a half-written last line from a killed writer doesn't keep doing), `board.rs` (`codex_session`'s `cut_off`). Not said there: the
  0.157.1 CLI couldn't use the 0.162.1 daemon here ("Cannot use the background server ·
  Experimental feature request failed", offering "Run without daemon this time"), so whether the
  daemon finishes a turn whose terminal UI died is unchecked; it holds the rollout while it runs
  one, so it isn't taken as cut off meanwhile.
- Approvals aren't recorded anywhere on disk: not in the rollout, `logs_2.sqlite` or the app's
  logs. A call left without its output past its tool's limit is taken as Needs you:
  `codex/rollout.rs` (`ThreadInfo::pending_calls`, `call_limit_ms`), `board.rs`
  (`codex_unanswered`).
- Commands (`CommandExecution`, with the folder the agent chose as a `file://` URL), file edits
  (`FileChange`) and the PRs a thread made: `codex/rollout.rs` (`work_of`, `file_path_of`,
  `pr_made`). Scripts don't `cd`, and many run outside the thread's own folder.
- The app runs every thread's commands under its one `codex app-server` process, with no thread
  id in their environment, so `codealong` finds the thread that runs it by the calls with no
  output yet at the end of a rollout: `codex/conversation.rs` (`open_calls`, `call_items`),
  `codex/mod.rs` (`runs_its_commands`). The app's `exec` tool's `input` is JavaScript holding
  the command in a string literal (`tools.exec_command({cmd:\"…\"})`); a `function_call`'s
  `arguments` are JSON.
- Forks (`forked_from_id`), threads the app's agent made (`thread_source:
  "agent_created_thread"`, named in the `<codex_delegation>` that opens them) and subagents,
  which aren't threads of their own: `codex/rollout.rs` (`ThreadInfo::forked_from`,
  `started_by`, `parse_meta`, `delegated_by`).
- Side conversations, as Codex's `/side` makes them (0.157.1): `codex/side.rs` (`SideThread`,
  `last_turn`). Not there: without `excludeTurns` the fork is refused ("ephemeral paginated
  thread/fork requires `excludeTurns: true`"); `thread/start {ephemeral: true}` runs turns with
  nothing written too, and `thread/delete {threadId}` removes a thread and its rollout; a
  `turn_context` line also names the turn's `effort`, and its `sandbox_policy` its roots. Codex's
  own `/side` injects a "Side conversation boundary" user message, gives its copy the thread's
  permissions too, and tells it to change nothing unless asked. A conversation of its own about a
  PR is `thread/start {ephemeral: true, cwd, sandbox: "read-only", approvalPolicy: "never",
  developerInstructions}` on the same app-server (`SideThread::fresh`); read-only, it read and
  `rg`'d files under Codealong's support folder outside its `cwd` (0.160, 2026-10-07).
- A prompt's images, which are in the rollout twice: `codex/conversation.rs` (the comment above
  `prompt_images`, and `sent_images`). A pasted one's `local_image` path is
  `$TMPDIR/codex-clipboard-<uuid>.png`.
- Plan usage, from each turn's `token_count` event: `codex/rollout.rs` (`RateLimit`,
  `parse_rate_limit`), `codex/usage.rs`.
- The app (ChatGPT.app, known by its bundle id), the CLI inside it, and its links:
  `detect.rs` (`codex_app_cli`), `codex/mod.rs` (`APP_BUNDLE_ID`), `codex/terminals.rs`
  (`codex_cli`), `launch.rs` (`open_codex`, `codex_thread_url`, `codex_review_url`). The app
  reads a `codex://threads/<uuid>` thread first and ignores one it can't.
- The socket the app's windows talk on (`<CODEX_HOME>/ipc/ipc.sock`): its frames, joining it,
  answering the router, finding the window that owns a thread and starting a turn there:
  `codex/ipc.rs` (`ipc_frame`, `Ipc::connect`, `Ipc::response`, `Ipc::owner`,
  `codex_start_request`). Starting a turn from an app-server of your own would make a second
  copy of the thread the app doesn't see.
- A window owns only the threads it has loaded, and takes one as it starts loading it, before
  its settings are in: `codex/ipc.rs` (`Ipc::owner`, `codex_start_turn`).
- The router runs in whichever process started it first: the app, or the Codex extension in VS
  Code or Cursor, which has the same code (26.908's `out/extension.js`). Each of the app's
  clients has a router manager of its own, and each that finds no router, as the app starts or
  when the process running it quits, starts one, removing the socket the one before it made; the
  path leads only to the last, and a client that joined an earlier one stays with it until the
  app is reopened. So the app usually has more than one router on `ipc.sock` (two after a fresh
  start on 26.930, 2026-10-10, the window that had the thread on the last), and when a window is left on another,
  Codealong's lookups never reach it (`no-client-found` after ten seconds). Seen 2026-10-10: five
  listening sockets since the router moved at 2026-10-08 19:57Z, and the window that owned the
  thread on one of the others. netstat counts the routers, and the app's log says a window took
  the thread: `codex/ipc.rs` (`routers_split`, `routers_at`, `taken_in_log`). The logs are in
  `~/Library/Logs/com.openai.codex/<year>/<month>/<day>/`, a file a day for each process, named
  with its pid; `thread_stream_role_changed … role=owner` is logged as a window takes a thread,
  and nothing as it lets one go. `[IpcRouter] I am the router` is logged for each client that
  joins a router in that process, not for each router.
- Stopping a thread's turn in the app (`thread-follower-interrupt-turn`, `user-stop`):
  `codex/ipc.rs` (`codex_interrupt_request`). With `expectedTurnId` (version 4) it stops the turn
  only while it's still the one running. The window sends `turn/interrupt` and kills the exec
  running the call; the rollout gets `turn_aborted`. A command already handed to a background
  terminal keeps running, as after Escape in the CLI; `thread/backgroundTerminals/clean
  {threadId}` ends those (checked 2026-09-29).
- Sending into a running turn in the app (`thread-follower-steer-turn`, with the composer's
  message as `restoreMessage`): `codex/ipc.rs` (`codex_steer_request`, `codex_steer_turn`). Not
  there: its params also take `serviceTier`, `additionalContext` and `toolOutput`; the window
  sends `turn/steer {threadId, input, expectedTurnId}`, and refuses a thread whose turn has
  ended ("Cannot steer conversation … because its active turn already ended"); the router's
  refusals before it hands a request on are `no-client-found`, `no-handler-for-request` and
  `request-version-mismatch` (26.924's code).
- What's sent while a thread's turn runs in the app (26.1007): the app's box steers it in or
  queues it to start once the turn ends, as `[desktop] followUpQueueMode` in config.toml says
  (`steer` unless set, `queue`, or `interrupt`). The queue is the app-server's
  (`thread/queue/add`, `list`, `delete`, `reorder`, `update` and `start`, which take
  `capabilities.experimentalApi` at `initialize`; an addition needs a `clientUserMessageId`),
  kept in `queue_1.sqlite`. The app-server that has the thread loaded starts its head as each
  turn ends, and notices another process's addition within seconds while the thread is idle
  (`PRAGMA data_version`, `queued_thread_revisions`); one that hasn't loaded it starts nothing
  (two app-servers on one CODEX_HOME, 2026-10-12). `thread-follower-start-turn` during a turn
  doesn't wait for it: the window shows a turn of its own holding the prompt, and its
  `turn/start` puts the prompt into the running turn, so the window shows it twice (seen
  2026-10-12). A follower window's queue goes to the owner whole
  (`thread-follower-set-queued-follow-ups-state` replaces the owner's), so Codealong adds to the
  app-server's instead: `codex/ipc.rs` (`queues_follow_ups`, `InApp::deliver`),
  `codex/appserver.rs` (`queue`).
- A thread made for the app to run, its first turn started through the window socket:
  `codex/appserver.rs` (`new_thread`), `codex/ipc.rs` (`codex_start_request`), `codex/mod.rs`
  (`start_in_app`). `thread/inject_items` refuses an empty list, and the app can't open a
  thread without its rollout ("rollout_not_found"). Such a thread says `originator:
  "codealong"`; the app's thread list doesn't filter on originator for local threads.
- The CLI inside the app bundle (0.158.0-alpha.2 in 26.924) can be older than the account's
  default model, which the API then refuses ("requires a newer version of Codex"); `-m` picks
  another. A new folder asks to be trusted, then about new or changed hooks.
- The CLI (Homebrew cask `codex`, `/opt/homebrew/bin/codex`) runs its TUI against a shared
  app-server daemon it spawns, `~/.codex/packages/app-server-daemon/releases/<version>/bin/codex
  app-server --listen unix:// --managed-daemon`, beside a `codex app-server daemon
  pid-update-loop` (seen running 0.160.1, 2026-10-05). The daemon, not the TUI, holds the
  rollout open, so a TUI's thread isn't in its open files, and a thread made through it says
  `source` "vscode". The app-server's `thread/fork {threadId, cwd, …}` copies a thread as `codex
  fork` does. Escape in the TUI stops a running turn ("Conversation interrupted",
  `turn_aborted`) and leaves its background terminals running. In code: telling TUIs from
  Codex's other processes and pairing them with threads, `codex/terminals.rs` (`terminals`,
  `terminal_args`, `pair_terminals`); `originator`, `codex/rollout.rs` (`parse_meta`); starting,
  resuming and forking, `codex/mod.rs` (`start_in_terminal`, `resume`, `fork`); Return steering
  and Tab queueing, `codex/mod.rs` (`steers`); the `@` list in its box, `send.rs` (`boxed`).
- Links that fill a prompt in (`codex://threads/new?path=<folder>&prompt=<text>`,
  `codex://threads/<uuid>?prompt=`) and the projects a new folder is added to: `codex/mod.rs`
  (`app_link`, `start_in_app`), `codex/ipc.rs` (`InApp`), `codex/rollout.rs`
  (`app_project_roots`). Not there: the new-thread link also takes `originUrl`, `projectId`,
  `mode` and `browserUrl`, and no images, and nothing in 26.924's link parser sends the prompt;
  `mode` is chat, work or codex, not a worktree choice (the new-thread page has its own Local or
  Worktree). The app sends images as `{type: "localImage", path}`, and so does the app-server's
  `turn/start` input.
- Projectless chats, each in a folder the app makes under `~/Documents/Codex`, all in one lane:
  `codex/rollout.rs` (`chats_dir`), `codex/mod.rs` (`chats`). `.codex-global-state.json` lists
  them in `projectless-thread-ids` (26.924's code).
- The app's unread dots are in that file too, `electron-thread-read-state-v1.unreadByIdentity.
  <account hash>.<host key>`: the ids of threads with a turn you haven't looked at, rewritten as
  one changes. Only the app's own threads and their subagents were ever in it (none from the
  CLI, `exec` or Codealong), and threads Codealong called Your turn were read there (checked
  2026-09-28). Codealong doesn't read it yet.
- Actions (archiving, naming, making a thread) go through an app-server of Codealong's own, a
  short-lived `<the app's CLI> app-server --listen stdio://` with `CODEX_HOME` set, never the
  CLI's daemon: `codex/appserver.rs` (header, `codex_binary`, `with_server`, `serve`). `codex
  app-server generate-json-schema --out <dir>` dumps the protocol. Logs:
  `~/Library/Logs/com.openai.codex/`. Your own app-server can't archive a thread the app has
  loaded ("thread … already has an active writer"), nor one codex-acp holds for a session
  Codealong runs, even after its `session/close`; codex-acp's `session/delete` archives it
  through its own app-server and lets go of it (`codex/mod.rs` `archive`, `acp::delete_held`;
  checked 2026-10-11).
- Naming a thread (`thread/name/set`), which refuses an archived one: `codex/appserver.rs`
  (`set_name`), `codex/mod.rs` (`rename`). It works on a thread the app has loaded too; the
  window socket has no rename, so the app shows a new name when it next reads its threads.

## Unchecked

- Not yet checked by hand (2026-10-02): a steer sent through `thread-follower-steer-turn`,
  since that needs a running turn in the app.

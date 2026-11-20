---
paths:
  - "src-tauri/src/claude/routes.rs"
  - "src-tauri/src/claude/peer.rs"
  - "src-tauri/src/claude/held.rs"
  - "src-tauri/src/claude/daemon.rs"
  - "src-tauri/src/claude/unqueue.rs"
  - "src-tauri/src/claude/switch.rs"
  - "src-tauri/src/send.rs"
---

# Sending to a Claude Code session

How a prompt, a message, a key or a command gets into a running Claude Code session and what
Claude Code does with it: its input box, the socket every live session listens on for other
sessions' messages and the ones it holds, the background service's socket, what waits in its
queue, and `/model` and `/effort`. Found by reading its code and testing on this Mac; read it
before changing a send (`send.rs`) or one of Claude Code's own ways in (`claude/routes.rs`).
Paths are under `src-tauri/src/`.

- Clearing the input box from outside (2.1.285, in tmux): Ctrl+C clears it, a key and a paste
  arriving together are one paste: `launch.rs` (`terminal_type_session`). In an empty box Ctrl+C
  only says to press it again to exit, which a second within the next second does. Ctrl+L is
  bound to `chat:clearInput` but runs the clear-screen handler and leaves the text; Esc Esc
  clears it but opens the rewind list in an empty box. Whether to clear first is the adapter's
  (`claude/mod.rs` `clears_before_typing`: an idle session whose box may hold a prompt Claude
  Code put back), and `launch::terminal_type_session` presses Ctrl+C.
- A prompt pasted into a box that would end in an `@` word gets a space, or Return picks from
  the file list instead of sending (2.1.287): `send.rs` (`boxed`, `ends_in_mention`).
- `!` at the start of the box is bash mode: the rest runs in the session's shell, and the agent
  takes a turn on what it printed. A paste led by `!` into an empty box switches to it as a typed
  `!` does (by its code, not tried: with text already in the box, or the cursor past its start,
  the box keeps its mode), long and multi-line
  pastes too: one line of 644 characters and 12 lines of 1,300 ran (2.1.293 in tmux, bracketed
  as `launch/tmux.rs` pastes, 2026-10-07); a daemon `reply` led by `!` runs the same way (its
  code takes the reply's mode from its text). What it records, and Run on a `!` block:
  `claude/conversation.rs` (`shell_items`), `send.rs` (`shell_command`, `shell_routes`); ⌥⏎ in a
  window's box sends one this way (`claude/mod.rs` `typed_shell_commands`). Agents are told to
  write `! <command>` for a command you should run yourself.
- Another session's message (`origin: {kind: "peer", name, body}`, on an `isMeta` `user` line
  or a `queued_command` attachment), what you send a Claude app session from Codealong, and a
  subagent's report back (`handback: true`): `claude/conversation.rs` (`Peer`, `line_peer`,
  `sent_from_codealong`), `claude/peer.rs` (`FROM_CODEALONG`, `FROM_YOU`),
  `claude/transcript.rs` (`you_typed`). What Codealong sends by itself (send.rs `tell`) goes the
  same way under the name Codealong, its body led by `send::FROM_CODEALONG_ITSELF`; sent to a
  terminal session that skips prompts it arrived claiming `bypass`, unheld, and the model was told
  it came from another session and isn't the user's (2.1.296, 2026-10-10). Told so, an agent
  answers with SendMessage to the envelope's `from`; Codealong's names none (`from: "unknown"` on
  the line), so it sent to the name, "Codealong", which SendMessage matched by prefix to the
  sessions named `codealong-xx` (the names Claude Code gives sessions in a folder `codealong`),
  offered both, and the agent sent its answer to the more active one (2.1.300, 2026-10-14). Both
  first lines (`FROM_YOU`, `FROM_CODEALONG_ITSELF`) say the answer goes in its reply.
- `/model <name>` in a running session switched that session alone in 2.1.280, where its picker
  (`/model` with no name) saved the pick as the default for new sessions too; in 2.1.299 both
  save it ("Set model to Opus 5.5 and saved as your default for new sessions", settings.json's
  `model`). What each writes to the transcript, and the model a reply names (without its context
  size): `claude/mod.rs` (`model_command`), `claude/transcript.rs` (`model_set`).
- `/model <name>` and `/effort <level>` typed into a session whose conversation is cached ask
  first ("Switch model?", "Change effort level?", Yes picked; Return takes it) and write their
  outcome to the transcript only once that's answered; `/effort` saves low to xhigh as the
  default for the session's model (`modelSettings.<model>.effortLevel`), max being session-only.
  A default put back too soon after Claude Code saved it is never read: its settings watcher
  reports a change once the file holds still for a second, and takes the first within 5 seconds
  of its own write for that write's echo, so its next settings write saves the default again.
  How Codealong answers, waits and puts them back, and that the session keeps what it was
  switched to (checked in tmux on 2026-10-14): `claude/switch.rs`. A background job takes both
  from its service's `reply` as typed commands, and the question's Return through `attach`: a
  Sonnet job switched to `low`, then to `haiku`, wrote both outcomes, its next reply named
  `claude-haiku-5-5` at `low`, and the defaults they saved went back (2.1.322, 2026-11-09).
- `/model` and `/effort` are `immediate` commands (2.1.322's command table, `immediate:!0` on the
  `local-jsx` ones): submitted while a turn runs they run at once instead of waiting in the queue
  as a prompt does, a different model's "Switch model?" coming up under the running turn, and the
  turn's next step goes on what they set (a Sonnet turn's next step ran on `claude-haiku-5-5` at
  `perTurnEffort: low`). What they did shows only on screen then, and nothing of it reaches the
  transcript, during the turn or after; they still save the default, settings.json being written
  again even with nothing in it changed, and `/effort max` writes nothing. `/effort` asked no
  question, mid-turn or between turns. Return in an empty box mid-turn does nothing. All in tmux,
  2026-11-10: `claude/switch.rs` (`typed`), `claude/mod.rs` (`switches_mid_turn`). A background
  job's through its service's `reply` mid-turn wasn't tried. By 2.1.322's code (not tried): a
  level over the model's cap is set to the cap and saves no default, and with
  `CLAUDE_CODE_EFFORT_LEVEL` naming another level (`auto` and `unset` too) nothing is set in the
  session, though a level it can save is still saved. The variable can come from a settings file's
  `env` (or the state file's): each source's is put in Claude Code's own environment after launch,
  in rank order, over what it started with, so the kernel's copy of its environment doesn't show it. The registry's `waiting` comes from a table of
  Claude Code's dialogs (`permission prompt`, `input needed`, `dialog open`), and `/model`'s
  question is a `dialog open` like any command's: a background job's "Switch model?" between
  turns left it `waiting` / `dialog open` (2.1.322, 2026-11-12), and a check that took that for
  something else to answer kept Codealong from answering its own switch. By 2.1.323's code
  (`isShowingLocalJSXCommand && !isResponseStreaming`) a command's panel is `dialog open` only
  while no reply streams, and a dialog of its own outranks it, among them the two a turn brings up
  when it can't go on ("retry on fallback model or edit prompt", "continue on usage credits or
  switch models", both `dialog open`). So between turns Return goes to a dialog that came up
  after the switch was typed, and mid-turn to none (`claude/switch.rs` `waits_on_another`).
- Every live session (terminal, Claude app, background) listens on a Unix socket, the registry's
  `messagingSocketPath` (`/tmp/cc-socks/<pid>.sock`, `peerProtocol: 1`; from 2.1.282's code),
  for other sessions' messages: the line written there (one per connection, nothing comes back)
  and the envelope with the permission class it claims: `claude/peer.rs` (`peer_frame`,
  `peer_content`). The auth line is optional on macOS. The receiver starts a turn when idle and
  folds it into the running one when busy, tells the model it came from another session and
  isn't the user's approval, and holds it for you when its permission class differs from the
  one the sender claims (below). Claude Code writes the envelope's attributes in the order
  `from`, `from-session`, `hop-chain`, `from-name`, `from-mode`, `from-plugin`. `bypass` is
  `bypassPermissions` (and `plan` entered from it). Where the socket comes among Codealong's
  ways in (last for every live session, and the only one for an agent's message) and when
  Codealong claims `bypass` (what you type in its windows on this Mac or on a linked phone, which
  runs no agents, an agent's message when the agent's own session skips prompts too, a helper's
  message to the session that started it or that session's message to its helper, when the start
  said which session it made, send.rs `from_its_helper` and `to_its_helper`, or a review to the
  session whose PR it reviews, autoreview.rs `reviews_for`; never from another Mac, where an agent
  may have asked for it:
  actions.rs `From::Phone`, `asked_by`):
  `claude/routes.rs` (`claude_routes`), `claude/peer.rs` (module header, `Message`).
- A message is held when the session bypasses permissions and the sender claims another class
  or none (`cause: "no-mode-asserted"`): it shows "Held message from another session", Deny
  first, unless the session's settings say `"crossSessionInbound": "accept"`; typing into its
  terminal isn't held. What a hold writes and doesn't (a line as it's held, one as it's
  delivered, nothing as it's denied or expires), the registry's `waiting` / `permission prompt`
  while it's up (a turn goes on under it), and the dialog's keys (no numbers, choices that wrap,
  End then Return delivers, Escape denies; checked in tmux and through a job's `attach` on
  2.1.292, 2026-10-07): `claude/held.rs` (module header). It expires after settings.json's
  `dialogExpiry` (`60s`, `5m`, `10m`, `never`; `CLAUDE_CODE_USER_DIALOG_TIMEOUT_MS` wins), five
  minutes by default, except in a background job nobody is attached to, and the sender is
  told it expired as it's told of a denial. A terminal session's transcript, where Codealong reads
  its mode, is made only with its first prompt, a second or so after its registry entry, so a
  message sent a session just started goes by the mode it starts in (`claude/mode.rs`
  `starting_mode`, `claude/mod.rs` `live_mode`): without it a meeting's brief, sent 0.3 s before
  the transcript was made, claimed no mode and was held (2.1.323, 2026-11-13). By 2.1.323's code
  `--dangerously-skip-permissions` wins over `--permission-mode` wherever it comes, and a
  `defaultMode` of Bypass or Auto from a repo's own settings is ignored ("only policy/user/flag
  settings may grant bypass mode"), the session starting in Manual. Under the Claude app (stream-json) a hold is only a `{"type":
  "system","subtype":"peer_message_hold","state":"held"}` line to the app, which (2.9939.2) has
  no code for it: the message is never shown or delivered, and the sender never hears.
- Background jobs through the account's daemon, `/tmp/cc-daemon-<uid>/<first 8 hex of
  sha256(config dir)>/control.sock`: `reply` hands a job a prompt (pasted, then Return), and
  `attach` types into its terminal, an Escape that stops its turn included: `claude/daemon.rs`,
  with the key from `<home>/daemon/control.key` (`claude/routes.rs`). Nothing else on that
  socket stops a job's turn; 2.1.285's ops are attach, await-ack, dispatch, has, kill, lease,
  leases, list, nudge, ping, reply, resize, respawn-stale, shutdown and yield.
- Send now, Ctrl+X Ctrl+S (`chat:sendNow`), and what it does to a running command (2.1.287, in
  tmux and through `attach`): `claude/mod.rs` (`SEND_NOW`). Return while a turn runs queues the
  prompt for the turn to take in once the step it's on ends (`priority: "next"`), however long a
  command keeps it. After Send now the call's result has
  `toolUseResult.backgroundedToDeliverMessage`, and the prompt goes in a second later as a
  `queued_command` attachment with `origin: {kind: "human"}`. With the box empty it sends the
  prompts already queued, which is what its terminal offers under them ("ctrl+x ctrl+s send
  now"), and Codealong's Send now on a held message (`claude/mod.rs` `steer_queued`); that stops
  the step it's on instead, as Escape does: a running command's result is "User rejected tool use"
  and `[Request interrupted by user for tool use]`, then a `dequeue` and the held prompt as a new
  turn's `user` line (2.1.307, a background job through `attach`, 2026-10-21), where the key after
  a typed prompt moved the same command to the background (`remove`, `reason:
  "absorbed_mid_turn"`). Ctrl+X Enter
  (`chat:queueSubmit`) queues one for after the turn. What it holds, by its transcript's
  `queue-operation` lines, and how a send finds the one it made: `claude/transcript.rs`
  (`queue_step`, `queued_as`). 128 of 9,493 `enqueue`s here had no `content`, and three held a
  long paste as `[Pasted text #1 …]` (the latest 800 transcripts, 2026-10-21).
- Taking a held message out (Remove): ↑ in an empty box gives back everything of yours it holds,
  a `popAll` line each, Ctrl+C clears the box but stops a running turn as it does (a background
  job's command was "rejected"), and Ctrl+U and Ctrl+K edit the box without touching the turn
  (2.1.312's code, and through a job's `attach` on 2026-10-29): `claude/unqueue.rs` (module
  header, `UP`, `KILLS_BACK`), `claude/transcript.rs` (`holding`, `given_back`). Not there: what
  ↑ gives back is the queue's commands that are "editable" (`popAllEditable`: not a notification,
  not `isMeta`, a human origin, so not another session's message), and with `!` ones among
  prompts only the prompts; with a draft in the box they come first, the draft after them on a
  line of its own and the cursor in it; ↑ with the cursor past the box's first line break moves
  the cursor instead; `popEditableAt` (↑ to pick one, Enter to edit it) is used only with
  `CLAUDE_CODE_KB_COHESION_FIXES` set. A clear (Ctrl+C) is kept for the next ↑ even after a
  `reply` sends a prompt (`holdCleared`, `restoreCleared`).
- The socket's `priority` takes `now`, `next` (the default) and `later`, but `now` is no send now
  (2.1.287): in a terminal session a `now` message sent during a command waited for it to end and
  then started a turn of its own, and a headless `-p --input-format stream-json` run, as the
  Claude app runs Claude Code, ended its turn after the command with no reply and answered the
  message as a new one (`claude/mod.rs` `steers`). The stream-json `control_request` `{subtype:
  "interrupt", send_now: true, message_uuid}` (`interrupt_send_now_v1` on `system/init`) is the
  app's send now, and only the process's stdin takes it, so a Claude app session can't be sent to
  now from outside.
- Nothing from outside stops a Claude app session's turn: the app sends a stream-json
  `control_request` `{subtype: "interrupt"}` down the pipe it runs Claude Code through, and the
  session's socket takes only messages.

## Elsewhere

- The background service retiring a job left idle, which another session's message may resume
  (send.rs `wakes`), is in `claude-code.md`.
- The permission mode a session runs in, Shift+Tab, and answering a permission prompt or a
  question with keys are in `claude-asks.md`.
- How a prompt you typed, or one taken in mid-turn (a `queued_command` attachment), is written
  is in `claude-transcript.md`.
- What each terminal does with a paste, a key and Return is in `terminals.md` and `rex.md`.

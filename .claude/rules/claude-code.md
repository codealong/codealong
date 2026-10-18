---
paths:
  - "src-tauri/src/claude/**"
  - "src-tauri/src/send.rs"
  - "src-tauri/src/side.rs"
  - "src-tauri/src/start.rs"
  - "src-tauri/src/starts.rs"
  - "src-tauri/src/origins.rs"
  - "src-tauri/src/prlinks.rs"
  - "src-tauri/src/attachments.rs"
  - "src-tauri/src/transcript.rs"
---

# Claude Code and the Claude app

What Claude Code and the Claude app record and accept, found by reading their code and testing
on this Mac; read it before changing the Claude adapter (paths are under `src-tauri/src/`).

## Claude Code

- Live sessions, `<home>/sessions/<pid>.json`, their fields, and when a busy one is stuck or a
  `shell` one has left a command sitting there: `claude/registry.rs` (`RegistryEntry`),
  `board.rs` (`CLAUDE_STALL_MS`, `claude_busy_since`, `BACKGROUND_QUIET_MS`),
  `claude/shells.rs`. `statusUpdatedAt` isn't refreshed while busy. A `shell` session is never
  stuck by the 30-minute rule, but Jev can still call its background command stuck (`board.rs`
  `BACKGROUND_STALLED`). While its command is active it's counted as working but drawn waiting,
  a clock and "Waiting on `<command>`" (`Session::waits_in_background`, `lanes.ts`
  `waitsInBackground`): a CI watch looked like a stuck turn behind the spinner (2026-10-07).
- Transcripts, `<home>/projects/<encoded cwd>/<session>.jsonl`, subagents' logs,
  `<session>/subagents/agent-*.jsonl` (the same format, every line `isSidechain: true`, with an
  `agent-*.meta.json` beside each: `description`, `requestShape: "background"` for one run in
  the background), the `ai-title` lines and `gitBranch`
  fields that repeat through them, and their deletion after about 30 days:
  `claude/transcript.rs` (module header, `TranscriptInfo`, `apply_tail`), `claude/mod.rs`
  (`subagent_logs`, `subagent_logs_under`), `origins.rs` (`KEEP_MS`). A subagent's log is under
  the project of the folder its session was in as it started it, not always its transcript's: 3
  of 105 sessions' here (2026-10-12).
- Which `user` lines are prompts you typed (`origin: {kind: "human"}`, not `task-notification`,
  `peer` or `isCompactSummary`; older transcripts have no `origin`), and a prompt far behind the
  256 KB tail: `claude/transcript.rs` (`you_typed`, `user_prompt`, `parse_transcript`). A
  slash command that starts a turn is written as its tags alone, marked typed, and a command
  of Claude Code's own as tags unmarked (`command_run`).
  `last-prompt` lines repeat the prompt's text but carry no time. In 150 transcripts here
  (2026-10-15) they often came only after the reply had begun, named none of 17 messages sent
  while a session worked, and sometimes went on naming an earlier prompt after a new one (most
  often after `continue`), so a prompt's own line says what a new turn is on. A message sent while a session
  is busy doesn't change the registry's `statusUpdatedAt`, nor is it a `user` line: it's written
  when the turn takes it in, as a `type: "attachment"` line whose `attachment` is `{type:
  "queued_command", prompt}` (background tasks' notifications come the same way, as
  `<task-notification>…`; `claude/conversation.rs` `conversation_items`). It's timed when it
  came, before lines written ahead of it, and it's your last prompt as a typed one is
  (`claude/transcript.rs` `you_sent_while_working`): without that, a message sent while the
  session worked stayed "Sent" at the end of its thread once the turn had taken it in (`ahead.ts`
  `took`), whenever the page the thread read began after it.
- Escape before the agent has written anything back, with the input box empty, takes the prompt
  back instead of interrupting the turn, and nothing in the transcript says so (2.1.285's
  `auto_restore_cancel`): `claude/conversation.rs` (the comment above `fate_of`, and `fate_of`).
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
  `claude/conversation.rs` (`shell_items`), `send.rs` (`shell_command`, `typed_routes`). Agents
  are told to write `! <command>` for a command you should run yourself.
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
- A line's `cwd` is the shell's folder, which goes back to the session's after a command that
  `cd`s outside it; which worktree an agent works in, and commands that go elsewhere only to
  look: `board.rs` (`session_seed`), `util.rs` (`command_dir`, `only_looks`). A look where the
  shell already is still counts: without those, a session that once ran a command elsewhere
  would stay listed there while it reads at home.
- Background jobs (`kind: bg`) have no terminal; `<account command> attach <jobId>` opens one:
  `claude/registry.rs` (`RegistryEntry::job_id`).
- A background job's record, `<home>/jobs/<job>/state.json` (`sessionId`, `state`, `intent`,
  `name`…), kept after the job ends, and the `forkParentSessionId` a copy names:
  `claude/registry.rs` (`Forks`). It also has `forkBoundaryAt` and `forkSourceAlive` (2.1.286).
  A fork into a terminal (`--resume --fork-session` without `--bg`) records nothing of the
  kind: its transcript's lines are copies (the same uuids) with no mark, so Codealong notes the
  ones it makes (`origins.rs`). A `claude` started from another session's Bash tool inherits
  `CLAUDE_CODE_SESSION_ID` and `CLAUDE_CODE_CHILD_SESSION=1` (and `CLAUDE_PID`, the session's
  process), but nothing on disk says so. The kernel keeps them in the environment the process
  started with (`procs::environment`), however far it is from the session: an eval's driver an
  agent started with `start_new_session` was launchd's once its launcher returned, and it and
  every `claude -p` its judge ran (named `cwd-xx` from the judge's `cwd` folder) still named
  the agent's session, which is how a headless run no session is above is found to be one's
  (`claude/mod.rs` `session_var`, `engine.rs` `with_run_origins`). An inherited
  `CLAUDE_CODE_ENTRYPOINT=cli` didn't keep such a run from being `sdk-cli` (2.1.296,
  2026-10-14).
- Warm spares (from 2.1.286), which Claude Code's own lookups and `read_registry` skip:
  `claude/registry.rs` (`RegistryEntry::spare`, `read_registry`). A spare's registry entry says
  `"spare": true`, `kind: bg`, `status: idle` and `name` its job ID, and its roster entry
  `dispatch.source: "spare"` with an empty `seed.intent`. Each spare runs in a pty host of its
  own under the service, and a job runs in the spare it claimed (the session's pid is that
  `claude bg-spare` process); with the service gone they're launchd's, and nothing claims them
  again: `claude/mod.rs` (`leftover`), which `codealong hygiene` ends.
- A Bash call's shell, a background one too, is a child of the session's process, which is how
  the `codealong` command finds its session; a terminal keeps `CODEALONG_SESSION` after its
  session ends: `claude/mod.rs` (`runs_its_commands`), `cli.rs` (`find_me`). The call is
  usually in the transcript before its command runs (`open_calls`), but not always: in a long
  session Codealong ran over ACP (2.1.280, a 20 MB transcript), commands that ran within about
  130 ms of their call's stamp read the transcript before the call was there, while one that
  ran `sleep 5` first found it. Fresh 2.1.280 runs on Haiku, Sonnet and Opus always had it
  written by the time their command started, 44 ms after the stamp (2026-10-16). Lines go
  through a write queue drained every 100 ms (`FLUSH_INTERVAL_MS`), and while a reply streams
  its lines are held back until its `stop_reason` is known. Such a session gets no process on
  the board, so the command finds it by `CLAUDE_CODE_SESSION_ID` in its shells instead
  (`cli.rs` `hosted_above`).
- A PR a session made or changed, from `toolUseResult.gitOperation.pr` or a `pr-link` line:
  `claude/transcript.rs` (`pr_made`); `prlinks.rs` links the session to it, subagents' logs
  included (`engine.rs`). `gitOperation` also has `push`, `commit` and `branch`; `push.branch`,
  the remote's branch, is how often a session pushed to a PR's (`claude/transcript.rs` `pushed`,
  board.rs `pr_primary`), though a quiet push leaves none. Its `pr` is
  written only for commands Claude Code reads as acting on a PR (`gh pr create`…): a PR made
  with `gh api repos/<owner>/<name>/pulls`, or by a script, gets none. A `pr-link` line is
  `{"type":"pr-link","sessionId","prNumber","prUrl","prRepository","timestamp"}`, "last-wins"
  metadata re-stamped with the session's current PR every 32 KB of writes, so one PR has dozens
  of lines. Both marks go by what a command printed, so a command that only prints another
  session's PR-making command and its URL (a `jq` over a transcript) gets both marks too,
  `action: "created"` included (2.1.289, 2026-10-05).
- Starting with `--session-id`, attaching files with `@"<path>"` mentions in the prompt (up to
  256 KB, 262,144 bytes; a bigger one is silently not attached and left for the Read tool),
  `--add-dir` (it takes several folders, so it goes before the other options or as
  `--add-dir=<dir>`, or it swallows the prompt), and a prompt that would read as an option or a
  subcommand: `attachments.rs` (module header), `claude/mod.rs` (`start_in_terminal`),
  `start.rs` (`as_argument`). Image paths without `@` are only picked up when pasted, and
  there's no image flag.
- A session's name, the `custom-title` line and `<session>/custom-title.json`, and when a
  running session takes up a name written from outside (2.1.285): `claude/names.rs`
  (`name_transcript`), `claude/mod.rs` (`rename`). `/rename` also writes an `agent-name` line
  and the registry's `name` with `nameSource: "user"`, the name other sessions message it by. A
  running session reads the last 64 KB of its transcript for the latest `custom-title`, and a
  re-stamp skips a line equal to the last of its type within 32 KB. The adapter never writes the
  registry, which the running process owns.
- `--resume <id> --fork-session` copies a conversation as far as it's written, the original
  carrying on, and takes `--session-id` for the copy only alongside `--fork-session`
  ("--session-id can only be used with --continue or --resume if --fork-session is also
  specified"): `claude/start.rs` (`fork_as`, `fork_args`, `fork_command`). A turn still running
  shows as interrupted in the copy. The registry has the new ID at once, the transcript from the
  first prompt, as with a start (2.1.284). With `--bg` the copy is a background job, and the
  roster's `workers.<job>.dispatch.launch` says `{mode: "resume", sessionId: <id>, fork: true}`;
  without a prompt it prints `(idle — send a prompt to start)` and has no transcript until its
  first prompt, so the board shows it with the conversation it copies
  (`claude::registry::resumed_from`).
- A turn refused at a usage limit is an `assistant` line with `isApiErrorMessage`, `error:
  "rate_limit"`, `apiErrorStatus: 429` and `quotaLimits`: `claude/transcript.rs` (`limit_of`).
  Its `message.model` is `"<synthetic>"`, its `quotaLimits` also says `status: "rejected"` and
  `isUsingOverage`, and its text is Claude Code's words ("You've hit your session limit · resets
  6:10pm (America/Los_Angeles)", "You've hit your monthly spend limit · raise it at
  claude.ai/settings/usage"). The "monthly spend limit" text means extra usage couldn't carry a
  plan limit that was hit, so the turn can go on at `resetsAt` too. Nothing else is written: no
  line says the limit lifted, and a background task's notification wakes the session into
  another refusal each time. The usage endpoint's meters show no spend limit (2.1.257 to
  2.1.287, 2026-10-02).
- Claude Code carries on by itself when a limit lifts (2.1.300's code, 2026-10-15): the setting
  `autoContinueAtUsageLimit` ("Continue automatically at usage limit" in `/config`, true when
  absent), behind the remote flag `tengu_marble_heron` (`enabled`, `autoArm`, `jitterMinMs`
  30,000 and `jitterMaxMs` 90,000 by default), arms itself on a refusal whose `resetsAt` is
  within 24 hours and isn't carried by extra usage, and fires a random 30 to 90 seconds after
  `resetsAt` ("Usage limit available again · continuing now"). Further out it says it won't
  ("the usage limit now resets more than 24 hours out"); `/rate-limit-options` ("Wait for limit
  to reset") arms it by hand; a session that slept through the reset says "press enter to
  continue". Only an open interactive session does it, and nothing is written to the transcript
  until its turn goes on. So Codealong tells a session still stopped there a few minutes after
  the limit lifts (reconnect.rs `LIFTED_GRACE_MS`).
- How a turn ends in the transcript, and what one cut off partway leaves (a hangup in a foreground
  command, and Escape there, in tmux on 2.1.296, 2026-10-12): `claude/transcript.rs`
  (`turn_step`), `board.rs` (`claude_session`'s `cut_off`). Not said there: only the terminal
  (entrypoint `cli`, background jobs too) writes `system` / `turn_duration` as each turn ends; the
  Claude app (`claude-desktop`) and headless runs (`sdk-cli`, `sdk-ts`) never do, which is why
  only terminal sessions are read for it. A reply is written as one line per part (thinking,
  text, call), each with the message's `stop_reason`, so a finished reply's last line says
  `end_turn` (or `stop_sequence` in older versions) where a call's says `tool_use`. Across 503
  terminal transcripts here, a few turns had no `turn_duration` after a finished reply, never
  after a call or a result. A long foreground `sleep` is refused by Claude Code itself and run in
  the background instead; a `python3 -c 'time.sleep(…)'` isn't.
- Who's at a session, by `entrypoint` (registry and transcript lines): Claude Code sets
  `CLAUDE_CODE_ENTRYPOINT` to `sdk-cli` when it runs non-interactively (`-p`, or its output not a
  terminal) and nothing set it already, `cli` otherwise; the Agent SDKs set `sdk-ts` and `sdk-py`.
  Its own session picker leaves out those three and `sessionKind` `daemon`/`daemon-worker`
  (`isHiddenFromSessionPicker`, 2.1.296). A `claude -p … --no-session-persistence` run writes no
  transcript, only its `sessions/<pid>.json` while it runs, named from its folder (`judge-cwd-xx`,
  `nameSource: derived`): an eval's judge made about 1,100 of them in one folder (2026-10-13).
  Codealong calls only `sdk-cli` headless (`board.rs` `claude_session`, `apply_headless`): an SDK
  program is often a front end someone types into, as Codealong's own sessions are.
- A turn ended for want of a connection is an `assistant` line with `isApiErrorMessage`,
  `error: "server_error"` and no `apiErrorStatus`, its text "API Error: " and Claude Code's words
  ("Can't reach the API server — check your internet or DNS (ENOTFOUND)", "Your computer went to
  sleep mid-response…"): `claude/transcript.rs` (`CONNECTION_LOST`, `connection_lost_at`). Its
  retries and why they run out with the lid closed (2.1.293's code, and `pmset -g log` on
  2026-10-07) are there too. `CLAUDE_CODE_MAX_RETRIES` (default 10) is clamped to 15 unless the
  undocumented `CLAUDE_CODE_RETRY_WATCHDOG` is set, which allows 300 and retries 429s and 529s
  for good, so Codealong sets neither and carries such a session on itself (reconnect.rs).
  Claude Code's own automatic resume is only for a usage limit that lifts (above).
- `--resume <id>` run in another folder goes on as the same session, its transcript staying in
  the first folder's project with the new folder on its lines, and `--bg --resume <id>` keeps
  the ID only for a session no background job has had, copying one that has into a new job
  (2.1.289): `claude/mod.rs` (`move_folder`, `move_folder_background`), `claude/transcript.rs`
  (`TranscriptInfo::cwd`), `start.rs` (`move_to_folder`). The agent is told the new folder in a
  prompt's `environment` attachment (`snapshot.workingDirectory`, with a `changes` entry for
  `workingDirectory`); that attachment follows every `cd` too, so it marks no move. A terminal
  resume of a stopped job's session keeps its ID.
- Moving a session to another account: `--resume` takes a transcript's path, from any
  account's folder, the copy is written under the account that runs it, another org's account
  goes on with it, and a copy closed before its first prompt is made again by the same
  arguments: `claude/mod.rs` (`move_to`, `move_to_background`, `Reader::moved_copy`,
  `Reader::closed_copies`, `resume_args`), `claude/start.rs` (`fork_args`), `start.rs`
  (`move_to`). A background copy's `dispatch.launch` is as above with the path and
  `restoresTranscript: true`, and its `state.json` has no `forkParentSessionId` (2.1.294), so
  the parent is known by its ID alone and its account is the one whose transcripts have it
  (`Reader::parent_key`). Haiku took on a Sonnet conversation too, and
  `--no-session-persistence` wrote nothing but an empty `memory` folder for the project
  (2.1.287, 2026-10-02).
- `/btw <question>` in a session asks a separate call over the conversation's context that has
  no tools and answers once; the answer shows in that terminal only and isn't written to the
  transcript, so typing it into a session from outside is no use. The same thing is the
  stream-json control request `side_question` (`{"type":"control_request","request_id",
  "request":{"subtype":"side_question","question","history":[{question,response}]}}`, answered
  whole in a `control_response`), which only a process's own stdin takes: not the session's
  socket, nor the daemon's.
- A side conversation's copy, a headless fork that keeps no record (2.1.286 to 2.1.289): its command line,
  its stream-json lines, the mode it runs in, its prompt cache and model, and why its registry
  entry and its permission hook are skipped: `claude/mod.rs` (`side`), `claude/side.rs`
  (`side_mode`, `SideCopy`), `claude/registry.rs` (`read_registry`), `side.rs` (`runs`),
  `server.rs`. It runs in the session's folder with its stdin kept open. A command the session
  left running in the background is reported to the copy as it resumes (`system` /
  `task_notification`, `status: "stopped"`, "Background shell command didn't finish before the
  previous session ended"), a turn of its own whose `result` line has `origin: {kind:
  "task-notification"}`, `num_turns: 0` and an empty `result`. A call refused for want
  of someone to ask is a `system` / `permission_denied` line (in `default`, a Write or a
  `touch`). The help lists the modes as acceptEdits, auto, bypassPermissions, manual, dontAsk
  and plan; `default`, which transcripts write, is still taken. `--permission-prompts host`
  would hand a `-p` run's prompts to whoever runs it; side.rs doesn't use it yet. The recorded
  system prompt is sent by `--system-prompt-snapshot`, on by default. While it runs the copy has
  a messaging socket and a derived name in the registry, and its `SessionStart` (matcher `fork`)
  hooks run too.
- A conversation of its own about a PR (`claude/mod.rs` `fresh_side`, `side.rs` `ask_pr`): the
  same headless run without `--resume`, which works and writes no transcript. In `default` mode
  with nobody to ask, `gh pr diff` and `gh pr view` were refused as needing approval while
  `grep`, `sed -n` and `head` on files went ahead, and a `touch` was refused as a sensitive edit
  (2.1.292, 2026-10-07), so Codealong fetches the PR itself and the run reads it from a folder
  `--add-dir` names. The Codealong plugin's `SessionStart` (`startup`) hook runs for it too.
- `--model`'s aliases (2.1.282) and the account's `settings.json` `model` a start goes on without
  it: `claude/start.rs` (`MODELS`, `default_model`); it takes a full name too (`claude/mod.rs`
  `offers_model`). Quote it when typing it: zsh reads `fable[1m]` as a file pattern ("no matches
  found").
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
  switched to (checked in tmux on 2026-10-14): `claude/switch.rs`.
- `--effort`'s levels and what a model that doesn't take one gets (2.1.284): `claude/start.rs`
  (`EFFORTS`). Without it a session starts on `settings.json`'s
  `modelSettings.<model>.effortLevel`, else its `effortLevel`, else the model's own default (the
  environment's `CLAUDE_CODE_EFFORT_LEVEL` wins over those settings). claude-agent-acp offers it
  as the config option `effort` (category `thought_level`), listed only once the session's model
  takes one, so it's set after the model (`acp.rs` `start`). Its `currentValue` is `default`
  where nothing set one, or the model's recommended level, so it doesn't say what a session is on.
- The effort a reply was made at is on its assistant line: `perTurnEffort` where it's there, else
  `effort` (older lines, and the Claude app's, have only `effort`), as Claude Code reads its own
  back; it can be a number. A model that takes none (Haiku 4.5) gets `perTurnEffort: null` and
  no `effort`. `/effort` writes "Set effort level to <level> (this session only): …", "… (saved
  as your default for new sessions): …" and "Effort level set to auto" (2.1.299's code):
  `claude/transcript.rs` (`reply_effort`, `effort_set`).
- `-w/--worktree [name]` makes a worktree for the session (read from 2.1.282's code): folder
  `.claude/worktrees/<name>` under the checkout, branch `worktree-<name>`, a word-pair name when
  none is given, based on a freshly fetched `origin/<default branch>` (settings
  `"worktree": {"baseRef": "head"}` bases it on HEAD). A `WorktreeCreate` hook in settings.json
  replaces all of that, even in a git repo, and prints the path. It refuses folders that haven't
  been trusted yet. Its name is optional, so it goes before the prompt: `claude/mod.rs`
  (`start_in_terminal`), `claude/start.rs` (`background_args`).
- The worktree a session made and holds, from its transcript's `worktree-state` lines, and what
  happens to it as the session ends (2.1.292's code, checked in tmux 2026-10-07):
  `claude/transcript.rs` (`own_worktree`), `idle.rs` (`exit`, `EXIT_WAIT`). `/exit` removes it
  when nothing is uncommitted, no commit was added and the session has no name of yours
  ("Worktree removed (no changes)"), else asks "Exiting worktree session" (Keep worktree /
  Remove worktree, Keep first); a hangup, as closing its tab gives, removes nothing, runs no
  `WorktreeRemove` hook and leaves the git lock (`claude session <name> (pid … start …)`) behind.
  The lock comes off just before the files are deleted. A session sent no prompt yet has no
  `worktree-state` line, and the exit writes none: it writes a `relocated` line with the
  checkout's folder.
- `--bg` (2.1.283): the job ID it prints, the session ID the roster has as soon as it returns,
  `--session-id` ignored, its arguments quoted in the command since the service keeps the
  environment of whichever shell started it, and an untrusted folder refused:
  `claude/start.rs` (`background_args`, `background_job`, `background_session`,
  `start_background`), `claude/mod.rs` (`background_command`, `before_start`). Without a prompt
  it prints `(idle — send a prompt to start)`. `--worktree`, `--model` and `--add-dir` work
  with it. A stale `CODEALONG_SESSION` in the service's environment reaches the job (`cli.rs`
  `find_me`). `stop <job>` keeps the conversation and the worktree; `rm <job>` (below) removes
  the job's state and, by its own rule, its worktree.
- The background service retires a job left idle for an hour, whether its turn ended
  (`settled`) or it sits at its prompt (`idle-prompt`): `<home>/daemon.log` says `[bg] bg retire
  <job>: idle-prompt, idle 61m`, then `bg settled <job> (done)`, and the job leaves the roster's
  `workers`. Its `state.json` keeps `state: "done"` (`lastTerminalAt` the retirement), and
  `--resume <id> --bg` brings it back under the same job ID (`bg claimed-spare <job> (fleet)`).
  Seen across a day's jobs on 2.1.296 (2026-10-12); whether a setting changes the hour wasn't
  looked into. Why another session's message may resume it: send.rs (`wakes`).
- Removing a worktree from outside Claude Code (2.1.296, a scratch repo, 2026-10-12):
  `cleanup.rs`, `claude/teardown.rs`. A `WorktreeCreate` hook's stdin is `{session_id,
  transcript_path, cwd, scratchpad_dir, hook_event_name, name}`, with no base ref; a
  `WorktreeRemove` hook's has `worktree_path` too (and `session_id`, `transcript_path`, `cwd`).
  A worktree a hook made isn't locked; one Claude Code makes itself is (`claude session …`). `/exit`
  in a session whose clean worktree a hook made runs `WorktreeRemove`, and the folder goes. `claude
  rm <job>` stops a running job too; on a worktree Claude Code made it removes the worktree and its
  branch unless a commit is on no remote (then it refuses, naming `--discard-unpushed
  <commit>@<worktree-id>`), keeping the transcript; on one a hook made it refuses ("The worktree has
  files that weren't checked against a repository (it has none of its own, or a hook manages it).
  They may be unsaved work.") and never runs `WorktreeRemove`, clean or not, and once the folder is
  gone it succeeds ("removed <job>"). `--force-remove-worktree <id>` deletes the folder without the
  hook, leaking what the hook would take down (one repo's runs `yarn worktree delete`: routes,
  ports, its own databases). A job's `state.json` names the worktree it made (`worktreePath`,
  `worktreeBranch`) after it ends too. So Codealong removes a worktree itself, by the repo's hook
  (run only where an account trusts the repo, as Claude Code runs a project's hooks) or by git,
  then runs `claude rm <job>`; that worked for a hook's worktree and for Claude Code's own, whose
  lock, where it's still there, comes off once the stopped job's process has ended.
- `--bg` takes itself out of the arguments before it reads them (2.1.284 to 2.1.290), so a
  background start's prompt goes after `--`: `claude/start.rs` (`background_args`). The job gets
  what's left with `--session-id` added (the roster's `dispatch.launch.args`); after `--` the
  arguments are the prompt, joined with spaces, and the job gets the `--` and them as they are.
- `/background` (`/bg`, optional prompt) moves a terminal's conversation to a background job
  with a session of its own (2.1.284): `board.rs` (`ClaudeMoves`, `claude_moves`),
  `claude/registry.rs` (`RegistryEntry::parked_job`), `claude/transcript.rs`
  (`TranscriptInfo::continued_in`). The roster's `workers.<job>.dispatch` says `source:
  "slash"`, with `launch` as above naming the old transcript's path. The old transcript's last
  line is `{"type":"continued-in","continuedInSessionId":<the job's session>}`, and the window's
  registry entry has `parkedJobId: <job>` while it shows Claude Code's list of agents ("Your
  conversation moved to the background — enter opens it · esc returns to it"); leaving the list
  clears it. The job's transcript has only its `ai-title` and `agent-name` lines until its next
  prompt, whose copy of the conversation has the same message uuids. Claude Code's `/resume`
  hides a transcript whose continuation has messages. A window that hadn't been told anything
  moves as a fresh job (`mode: "prompt"`, no transcript at all).
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
  runs no agents, an agent's message when the agent's own session skips prompts too, and a
  helper's message to the session that started it, when the start said which session it made,
  send.rs `from_its_helper`; never from another Mac, where an agent may have asked for it:
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
  told it expired as it's told of a denial. Under the Claude app (stream-json) a hold is only a `{"type":
  "system","subtype":"peer_message_hold","state":"held"}` line to the app, which (2.9939.2) has
  no code for it: the message is never shown or delivered, and the sender never hears.
- A Claude app session's permission mode is in the app's record of it, a terminal session's on
  its transcript's prompt lines and its `permission-mode` line: `claude/mode.rs`
  (`permission_mode`, `session_mode` with the names Claude Code's own table gives them),
  `claude/app.rs` (`permission_mode`, `Records::modes`), `claude/transcript.rs`
  (`TranscriptInfo::permission_mode`). `permission-mode` (and `mode`, `normal` here) are
  last-wins metadata re-stamped with the title and the rest; a switch sets the stamp without
  writing it (2.1.296's code). Of this Mac's 300 latest transcripts on 2026-10-13, 296 ended in
  `bypassPermissions`.
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
  prompts already queued. Ctrl+X Enter (`chat:queueSubmit`) queues one for after the turn.
  Transcripts also have `queue-operation` lines (`enqueue`, `dequeue`, `remove`, with the prompt
  as `content`) as the queue changes.
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
- A permission prompt in a terminal (2.1.283, in tmux) and answering it with a key: the
  registry's `waiting` / `permission prompt` (`claude/registry.rs` `PERMISSION_PROMPT`), `1` for
  Yes and Escape for No, pressed only while the registry and the transcript agree the prompt is
  up for the call you saw: `claude/mod.rs` (`ClaudeCode`, `answer`, `waiting_call`). The prompt
  lists `1. Yes`, `2. Yes, and always allow…`, `3. No`, and the digit alone answers it (no
  Return); Escape ends the turn with "Interrupted · What should Claude do instead?". A stray
  digit with no prompt up lands in the input box. A plan's prompt (`ExitPlanMode`, 2.1.292's
  code) lists "Yes, and switch to bypass permissions for this session" (started bypassing),
  "Yes, and use auto mode" (auto offered) or "Yes, auto-accept edits", then "Yes, manually
  approve edits" and "No, keep planning", so its 1 switches mode and keeps the conversation;
  with `showClearContextOnPlanAccept` in settings (default false) "Yes, clear context and …"
  comes first, so Codealong presses no key for a plan then and says to answer it in the
  terminal (`claude::clears_context_first`). Read-only commands (`git status`, `ls`) aren't
  asked about. Only typed keys answer it: a bracketed paste goes to Claude Code's paste handler,
  never to its keys, so a pasted `1` does nothing. Where the terminal has the kitty keyboard
  protocol Claude Code pushes flags 5 (1 in its legacy mode), never "report all keys", so a
  typed digit still arrives as the plain character.
- Trust: the account's state file (`projects.<path>.hasTrustDialogAccepted`), which folders
  count as trusted, the trust prompt whose default answer quits, and writing an entry under
  Claude Code's own lock (2.1.285's code, checked in tmux): `claude/trust.rs` (`state_file`,
  `trust_folder`, `trust_keys`, `StateLock`). Yes at the prompt writes the entry for the
  folder's git root, or the folder outside a repo. `--bg`'s "Workspace not trusted" goes by the
  same entries, with the path's links resolved and in NFC. Running sessions reread the file
  every second, and Claude Code strips `hasCompletedProjectOnboarding`, `history`,
  `projectOnboardingSeenCount` and `devIntentsDetected` from project entries whenever it saves.
- The Codealong plugin's `PermissionRequest` hook (2.1.284): what it's handed, how Claude Code's
  own prompt races it, how a background subagent's prompt waits for it
  (`awaitAutomatedChecksBeforeDialog`), and the decision it prints: `claude/asks.rs` (module
  header, `parse`, `call_record`, `answers_tool`, `decision_json`, `ask_until_replied`). It also
  gets `cwd` and `permission_mode`, but no tool-use id, and its process is a child of the
  session's. Answered at the prompt, the hook's process is left running until its `timeout`
  (default 600 s). The decision goes in
  `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":…}}`. Once the hook
  lets a background subagent's request go, its prompt shows in its session, which the registry
  then says waits on a permission prompt, while the call is only in the subagent's own log, under
  the project of the folder the session was in when it started the subagent (a session gone on
  into a worktree writes them there): `claude/mod.rs` finds it there when plainly the one and
  answers it with the same keys, a background job's typed through its daemon (`waiting_on`,
  `helper_logs`, `helper_waiting`; reproduced and allowed in a `--bg` Haiku job, 2.1.295,
  2026-10-08). An allow shows
  "Allowed by PermissionRequest hook", and a `setMode` suggestion in `updatedPermissions`
  switches the session's mode. A deny with `"interrupt":true` ends the turn as the prompt's No
  does; without `interrupt` the turn goes on, the message being the call's result as given: the
  prompt's own No with words typed in writes `The user doesn't want to proceed with this tool
  use. … To tell you how to proceed, the user said:\n<words>` (`asks::REFUSED_SAYING`), so a
  hook's deny in those words reads as one typed at the prompt (checked with `-p` and a stub hook
  on 2.1.289, 2026-10-05: the agent read the words and answered them). The registry's `waiting` / `permission prompt` comes from the
  session's `requires_action` state, in a terminal and under the Claude app alike (an
  `AskUserQuestion` says `input needed`). The call's own transcript line can be missing while
  its prompt is up, written only with its result once it's answered, so the keypress path in
  `claude/mod.rs` may not find it there until then; the hook has it.
- While a turn runs, Claude Code's writer holds back every line from the first assistant line
  whose `stop_reason` is still null (2.1.291's code). An `AskUserQuestion`'s message often stays
  so until it's answered: its thinking, the text written before the question and the call all
  reach the transcript only with the answer, while the terminal already shows them (seen
  2026-10-06; a Bash call's prompt didn't hold its line back). It varies: on 2.1.302 Opus 5.5
  sessions held it until the answer every time, while a Haiku 5.5 one had the text and the call
  in its transcript, `stop_reason: "tool_use"`, as its question waited (2026-10-16). The hook's
  input and the registry have only the call.
- The one live copy of a reply outside Claude Code is a `MessageDisplay` hook, which gets its
  text in batches of finished lines as the terminal shows them (at most ten a second), and which
  Codealong's plugin has with `[sessions] live_replies` on: `claude/live.rs` (module header,
  `take`, `stream`), `install.rs` (`LIVE_HOOK`). Its input (2.1.302) is `session_id`,
  `transcript_path`, `cwd`, `scratchpad_dir`, `prompt_id`, `turn_id`, `message_id` (Claude
  Code's own, not the line's `uuid` nor the API's `msg_…`), `index`, `final` and `delta`; its
  environment has `CLAUDE_CONFIG_DIR` and `CLAUDE_CODE_SESSION_ID`. A message's batches put
  together are its text block exactly as the transcript writes it, a background subagent's
  messages fired none, and with each batch's hook a process of its own they arrive out of order
  (index 1 before 0). Hooks are read once when a session starts, and a hook's `if` filters only
  tool calls, so one can't be turned on for a single running session. Any such hook changes the
  terminal for every session that has it: Claude Code shows the reply from the hook's batches,
  so a Markdown paragraph, one line, shows whole once it's finished, `"async": true` or not
  (2.1.291 with an `http` hook answering at once, 2026-10-06: three paragraphs 1.8 s apart where
  the reply otherwise grew a screen line every 0.3 s; 2.1.302 with an async command hook on Opus
  5.5, 2026-10-16: paragraphs of about 750 characters each appeared whole 1.2 to 1.8 s apart,
  where the reply otherwise grew a row every 0.15 to 0.5 s). The reply ends at the same time
  either way, and nothing is slower but a paragraph's start in the terminal; the transcript got
  the whole reply 4.5 s after the hook had its first paragraph. Hence a setting, off by default
  (the user chose it on for their accounts, 2026-10-16). Without it the panel only says where to
  read what a question came with (`Pending.unwritten`, set by `asks::written_in`).
- An image that came with a prompt is an `image` part (`source: {type: "base64", media_type,
  data}`; PNG, JPEG and WebP seen) in the prompt's own `message.content`, numbered by
  `imagePasteIds` and named `[Image #N]` in the text when pasted into the terminal:
  `claude/conversation.rs` (`images_of`); Codealong keeps each (`attachments.rs` module header,
  `keep_image_in`, `written_in`). Pasted, the text part comes first; attached in the Claude app,
  the image comes first. A prompt sent while it worked has the same list as its `queued_command`
  attachment's `prompt`, with `imagePasteIds` on the attachment. A `@"<path>"` mention stays in
  the text as typed, and the file it attached is the next line, `attachment: {type: "file",
  filename, displayPath, content}`, an image's content `{type: "image", file: {base64, type,
  originalSize, dimensions}}`; Codealong doesn't read that line (2.1.281 to 2.1.287).
- Your answers to an `AskUserQuestion`, on its result line's `toolUseResult.answers`:
  `claude/conversation.rs` (`answers_of`). `toolUseResult` also has `questions` and
  `annotations`. Declined, the result is an error, "The user doesn't want to proceed…", with
  `toolUseResult` the string "User rejected tool use".
- An `Artifact` call's result line (`toolUseResult`) and the link the conversation shows for it:
  `claude/conversation.rs` (`artifact_of`), which the session's list of them for its header
  (`artifact_made`, artifacts.rs) goes by too, subagents' logs included. Across this Mac's
  transcripts on 2026-10-12, only `Artifact` calls had results with `artifact_id` or
  `created_from_type`: 169 publishes and one open, all with a `url`. The call's input is
  `file_path`, or `type_url` and `title`, with an `action` when it isn't a publish. The result
  also has `version`, `path`, `icon`, `audience`…; `url` is `https://claude.ai/artifact/<id>`,
  older versions write no `seq`, and `updated` is false on the first publish. One made from a
  type has no `artifact_id`; a read keeps `url` and `title` under `read` (beside
  `artifactRead`), and a list has `artifacts` (2026-10-02).
- A `SendUserFile` call and the files the conversation lists under it: `claude/conversation.rs`
  (`sent_of`), `claude/transcript.rs` (`SEND_FILE`). Its input is `files` (paths, absolute or
  from the folder it works in), `caption`, `status` (`normal`, or `proactive` to reach your
  phone) and `display` (`render` or `attach`, absent before about 2.1.280). Its result's
  `toolUseResult` is `{caption, display, rendered_locally, attachments: [{path, size, isImage,
  media_type, pathValidated, file_uuid}]}`, an attachment that didn't upload having
  `upload_error` in place of `file_uuid`; one failing among several doesn't make the result an
  error. Across this Mac's 71 calls (2.1.219 to 2.1.296, terminal and Claude app sessions, none
  in a subagent's log, 2026-10-16): 100 files, 82% images, then PDFs, Markdown, HTML and an MP4,
  and 61 of them already gone from disk, sent from scratchpads and background jobs' `tmp`
  folders. Claude Code uploads each to `POST /api/oauth/file_upload`; `GET
  https://api.anthropic.com/api/oauth/files/<file_uuid>/content` with the sending account's
  bearer token (and `anthropic-beta: oauth-2025-04-20`) gave the bytes back exactly, as
  `application/octet-stream`, for a file sent that day and one sent 22 days before whose copy on
  disk was gone (one `~/.claude` account's terminal sessions). Codealong doesn't fetch them: a
  click opens the file where it is (`launch::open_file`).
- Sign-in in the login keychain (its services, the account, reading it without a prompt) and
  never refreshing, logging or passing the token in arguments: `claude/usage.rs` (module header,
  `keychain_services`, `read_sign_in`, `fetch`).
- Help improve Claude (whether Anthropic trains on the account), read and changed through the
  account's settings: `claude/training.rs`. The settings answer holds much else (dismissed
  banners, feature flags) and `grove_updated_at`, `grove_notice_viewed_at`. Claude Code's notice
  of the new terms also reads `GET /api/claude_code_grove` (`grove_enabled` there is whether the
  notice applies, with `domain_excluded` for emails whose domain is left out of training,
  `notice_is_grace_period`, `notice_reminder_frequency`); it answered 403 to both accounts'
  OAuth tokens here (2026-10-13), so Codealong goes by the account settings alone.
- Plan usage, `GET https://api.anthropic.com/api/oauth/usage`: the request and its `limits`
  list in `claude/usage.rs` (`fetch`, `claude_meters`), how often the app asks and how it backs
  off in `engine.rs` (`USAGE_EVERY`, `USAGE_BACKOFF`, `keep_last_good`). It returns 429 when
  called often, so don't loop `--usage` snapshots. The same answer has `extra_usage`, this billing
  period's extra-usage spend in minor units (`claude/usage.rs` `extra_spend`; Claude Code
  2.1.296's schema for it describes the units), and `spend`, the same as `{used, limit}` of
  `{amount_minor, currency, exponent}` with `balance` and `auto_reload`, and
  `seven_day_breakdown`, the week's share by surface (Claude Code, Chats, Cowork; read
  2026-10-12). Nothing in a transcript says which calls extra usage paid for: overage fields
  (`quotaLimits.isUsingOverage`) are only on refused turns.
- What each call used, for the token ledger: `claude/transcript.rs` (`token_use`). A reply's
  usage is written again on each line of it as it streams, the first ones short of the final
  count, and copied by forks, moves and side questions, so the ledger keeps one row per message
  and request ID with each count's largest. The 1-hour cache writes cost more than the 5-minute
  ones (`cache_creation.ephemeral_1h_input_tokens`); sessions write all theirs for an hour.
  Claude Code works out costs itself (`.claude.json` `projects.<path>.lastCost` and
  `lastModelUsage.<model>.costUSD`, the last run's), and prices.rs's test pins two of them.
- How full a session's context is, as Claude Code's own meter has it (2.1.296's code: the status
  line's `context_window`): the latest reply's `input_tokens + cache_read_input_tokens +
  cache_creation_input_tokens`, not its output, against the model's window, a million for a
  `[1m]` model and for the models that have a million by default, 200,000 otherwise
  (`CLAUDE_CODE_MAX_CONTEXT_TOKENS` can set it with compacting off): `claude/transcript.rs`
  (`TranscriptInfo::context`, `window_model`), `prices.rs` (`context_window`). A compaction is a
  `system` line, `subtype: "compact_boundary"`, whose `compactMetadata` has `trigger` (`auto`,
  `manual`), `preTokens` and `postTokens`, what's left (967,152 to 6,627 here). A reply names its
  model without the context size a `/model` switch gave it.

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
- A Code session started without a folder runs in a scratch workspace the app makes for it and
  removes once the session is gone (2.9939's code): `claude/app.rs` (`scratch_dir`). The engine
  lists every such session in one lane (`engine.rs` `chat_roots`), and the app's
  `<system-reminder>` part ahead of the first prompt is skipped (`claude/transcript.rs`
  `typed_text`).

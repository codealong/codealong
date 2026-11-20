---
paths:
  - "src-tauri/src/claude/transcript.rs"
  - "src-tauri/src/claude/conversation.rs"
  - "src-tauri/src/claude/turns.rs"
  - "src-tauri/src/claude/names.rs"
  - "src-tauri/src/claude/wakeups.rs"
---

# Claude Code's transcripts

What Claude Code writes in a session's transcript, line by line, and what its reader goes by:
which lines are prompts you typed, what began each turn and how it ended, the errors and limits
a turn stops at, what a call used, compaction, names and wakeups. Found by reading its code and
this Mac's transcripts; read it before changing how a transcript is read (`claude/transcript.rs`,
`claude/turns.rs`, `claude/names.rs`, `claude/wakeups.rs`). Paths are under `src-tauri/src/`.

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
  slash command that starts a turn is written as its tags alone, marked typed, several run
  together as a line each, and a command of Claude Code's own as tags unmarked, or from 2.1.302
  (`/color`, in tmux) as a `system` / `local_command` line (`command_run`). One of its own that
  takes a turn (2.1.307's `/goal <condition>`) is still an unmarked line, its output's `user` line
  after it carrying the turn's marks: `claude/transcript.rs` (`ran_a_turn`), `claude/conversation.rs`
  (`name_commands`). Without that, a `/goal` sent from Codealong stayed "Sent" at the end of its
  thread under the reply to it (2026-10-21).
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
- What began each turn, for the analytics (turns.rs): a turn's first `user` line says it in
  `turnOrigin` (`human`, `peer`, `task_notification`, `scheduled`, `sdk`; 2.1.322, 2026-11-11),
  beside `origin.kind` on all but a scheduled one, and a `queued_command` attachment in
  `attachment.origin.kind`: `claude/turns.rs` (module header, `turn_step`). Not there: such
  lines also have `promptSource`, `turnPosition`, `turnAttributionKey`, `wakeupSource`,
  `scheduledTaskId` and `scheduledFireId`, unread. Over four days of this machine's transcripts
  the turn-starting lines were 898 another session's, 884 typed, 938 a wakeup's and 848 a task's
  report, with 660 more reports and 391 messages taken in mid-turn. A prompt a session was
  started or resumed with is marked `human` whoever wrote it, so Codealong's own and an agent's
  are told by the line they open with (`turns::whose`); a tool result that only quotes "[Request
  interrupted by user" or the words of a refusal (a search of the code that has them) is no stop
  and no refusal, so only a result that's an error and opens with those words is one.
- A line's `cwd` is the shell's folder, which goes back to the session's after a command that
  `cd`s outside it; which worktree an agent works in, and commands that go elsewhere only to
  look: `board.rs` (`session_seed`), `util.rs` (`command_dir`, `only_looks`). A look where the
  shell already is still counts: without those, a session that once ran a command elsewhere
  would stay listed there while it reads at home.
- A session's name, the `custom-title` line and `<session>/custom-title.json`, and when a
  running session takes up a name written from outside (2.1.285): `claude/names.rs`
  (`name_transcript`), `claude/mod.rs` (`rename`). `/rename` also writes an `agent-name` line
  and the registry's `name` with `nameSource: "user"`, the name other sessions message it by. A
  running session reads the last 64 KB of its transcript for the latest `custom-title`, and a
  re-stamp skips a line equal to the last of its type within 32 KB. The adapter never writes the
  registry, which the running process owns. So a name is written again only by a process that read
  it. A first prompt's writes come in a burst (over 190 KB in 15 s, with re-stamps 40 to 110 KB apart
  after it): of three scheduled runs named just after their first prompt on 2.1.319 (2026-11-09),
  the one whose next re-stamp was 14 KB on took the name up, and the two at 194 KB and 281 KB never
  did, so the line fell out of the board's tail and the title went back. That's why `schedules`
  (`NAME_AFTER_MS`) names a run late, and `names.rs` (`kept_name`) reads the sidecar. `/resume` goes
  by a `custom-title` in the last 64 KB, then the sidecar, then the head (2.1.319's code), and an
  empty one deletes the sidecar. On 2.1.319 an `agent-name` line is also Claude Code's own title
  (`nameSource: "auto"` in the registry and `state.json`), re-stamped with its `ai-title`.
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
  (`turn_step`), `board/sessions.rs` (`claude_session`'s `cut_off`). Not said there: only the terminal
  (entrypoint `cli`, background jobs too) writes `system` / `turn_duration` as each turn ends; the
  Claude app (`claude-desktop`) and headless runs (`sdk-cli`, `sdk-ts`) never do, which is why
  only terminal sessions are read for it. A reply is written as one line per part (thinking,
  text, call), each with the message's `stop_reason`, so a finished reply's last line says
  `end_turn` (or `stop_sequence` in older versions) where a call's says `tool_use`. Across 503
  terminal transcripts here, a few turns had no `turn_duration` after a finished reply, never
  after a call or a result. A long foreground `sleep` is refused by Claude Code itself and run in
  the background instead; a `python3 -c 'time.sleep(…)'` isn't.
- A turn ended for want of a connection is an `assistant` line with `isApiErrorMessage`,
  `error: "server_error"` and no `apiErrorStatus`, its text "API Error: " and Claude Code's words
  ("Can't reach the API server — check your internet or DNS (ENOTFOUND)", "Your computer went to
  sleep mid-response…"): `claude/transcript.rs` (`CONNECTION_LOST`, `connection_lost_at`). Its
  retries and why they run out with the lid closed (2.1.293's code, and `pmset -g log` on
  2026-10-07) are there too. `CLAUDE_CODE_MAX_RETRIES` (default 10) is clamped to 15 unless the
  undocumented `CLAUDE_CODE_RETRY_WATCHDOG` is set, which allows 300 and retries 429s and 529s
  for good, so Codealong sets neither and carries such a session on itself (reconnect.rs).
  Claude Code's own automatic resume is only for a usage limit that lifts (above). A refused
  connection ("Connection refused — a firewall or proxy may be blocking it (ECONNREFUSED)") is
  carried on too since 2026-11-11: one here went on when told to try again 5 minutes after.
- Every other error that ends a turn is written the same way, `isApiErrorMessage` with `error` its
  kind (`server_error` for "529 Overloaded…", "No response from API (waited 10m…)", "The response
  stopped arriving…"; `rate_limit` without a 429 for a model that's unavailable;
  `authentication_failed` for "Login expired · Please run /login"; `invalid_request` for the
  safeguards' flags). A window shows them with Try again, but for those that trying again doesn't
  mend: `claude/transcript.rs` (`api_error_of`). A sign-in turned down (`authentication_failed`,
  `apiErrorStatus: 401`, also "OAuth token revoked · Please run /login"), how often it came right
  by itself as another process renewed the sign-in, what the background service's `daemon.log`
  says as its renewal fails and a sign-in turns up again, and a running process taking up the new
  one unprompted: `claude/transcript.rs` (`signed_out_of`), reconnect.rs. `claude auth login`
  (`--email`, `--sso`, `--console`; 2.1.327's help) only signs in, then ends, where a bare `claude`
  with a revoked sign-in still kept asks for none until a prompt fails: `claude/mod.rs`
  (`sign_in_command`).
- The effort a reply was made at is on its assistant line: `perTurnEffort` where it's there, else
  `effort` (older lines, and the Claude app's, have only `effort`), as Claude Code reads its own
  back; it can be a number. A model that takes none (Haiku 4.5) gets `perTurnEffort: null` and
  no `effort`. `/effort` writes "Set effort level to <level> (this session only): …", "… (saved
  as your default for new sessions): …" and "Effort level set to auto" (2.1.299's code):
  `claude/transcript.rs` (`reply_effort`, `effort_set`).
- What a session scheduled to wake itself (`ScheduleWakeup`, a self-paced `/loop`'s next tick;
  `CronCreate`, a `/loop` at an interval or a one-shot check at a time) lives in its process, and
  a resumed process brings back from the transcript what's still to come: what the calls'
  results record, how Claude Code reads them back (2.1.310's code) and what each kind lasts:
  `claude/wakeups.rs` (module header), which `codealong hygiene` asks before it ends a quiet
  session's process (`hygiene.rs`, `Adapter::wakes_at`), and which the Schedules view lists for
  running sessions (`upcoming`, `Adapter::own_schedules`; a `ScheduleWakeup` call's input has
  `delaySeconds`, `prompt` and `reason`, a `CronCreate`'s `cron`, `prompt` and `recurring`). Not there: a session resumed after its
  wakeup's time is told in a task notification that the wakeup "will not fire", and a recurring
  `/loop` cron isn't cancelled by `ScheduleWakeup({stop: true})`. On this Mac's transcripts
  (2026-10-24) `scheduled_task_fire` lines name the wakeup's `taskId`, `cron` and `prompt`
  ("Claude resuming /loop wakeup (Oct 20 10:04am)"), and `session_cron_carry` attachments follow
  an orchestrator's compactions with its recurring job. A wakeup's turn starts with that
  `system` line, then its prompt as a `user` line with no `origin` (an orchestrator's ticks,
  2026-10-22). When anything last came in from outside the agent, wakeups, other sessions'
  messages and background tasks' reports too, read from the record after its turn has ended:
  `claude/transcript.rs` (`brings_input`, `TranscriptInfo::input_at`).
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
- Idle compaction (2.1.306's code; seen from 2.1.296, 2026-10-10, 38 times here by 2026-10-21):
  in the terminal, a conversation of 200K tokens or more left idle for 90% of its one-hour prompt
  cache (about 54 minutes) is compacted before the cache expires. The remote flag
  `tengu_sunny_locket` sets the mode (`off`, `log`, `compact`, `compact_on_return`), the minimum
  and the fraction; `CLAUDE_CODE_IDLE_COMPACT_MIN_TOKENS` the minimum too, and settings.json's
  `"idleCompaction": false` turns it off. The registry goes `busy` for the minute it takes and
  `idle` again, moving `statusUpdatedAt`, though no turn was taken. As it starts, the transcript
  often gets the `pr-link` and `frame-link` lines stamped again; once it's done, a
  `compact_boundary` (`trigger: "auto"`, as one partway through a turn has), the summary (a `user`
  line with `isCompactSummary`), the files, listings and hook output it attaches again (more than
  256 KB of them in a third of 109 compactions here) and a `system` / `informational` notice,
  "Compacted while idle, before the prompt cache expired", and no reply. So neither the registry
  nor the transcript's last line says when the turn ended; the last reply, message or
  `turn_duration` before the boundary does (a `system` / `away_summary`, the recap Claude Code
  writes about three minutes after a turn while you're away, isn't one). `compact_on_return`
  compacts as you come back instead, its notice "Compacted after an hour idle…" coming before
  your prompt's turn. Read as a turn, it brought a session you'd seen back to Your turn an hour
  after its last one: `claude/transcript.rs` (`TranscriptInfo::compacted`, `said`,
  `said_before`), `board/sessions.rs` (`claude_session`). While it runs nothing in the transcript says
  so, so the session shows as working; but it has said nothing since its turn ended (all 52
  idle compactions here came straight after a turn's end, 2026-10-22), where a turn starts by
  saying something, so it keeps its tag and suggested replies: `claude/transcript.rs`
  (`TranscriptInfo::ended_at`, `ends_turn`), `board/overlays.rs` (`moved_on`). Read as a turn, it took
  them away from sessions still waiting on what they were for.
- Provider flags (2026-10-19): assistant lines with `message.stop_reason: "refusal"`,
  including the synthetic `error: "invalid_request"` reply whose text says the model's safeguards
  flagged the session. The refusal is also on the preceding empty assistant message. Read by
  `transcript.rs` (`flagged`), ignoring inline sidechains; kept across later turns. A fresh read
  scans before the usual tail for it, so a restart doesn't forget an older refusal.
  `providerflags.rs` caches the checked prefix across restarts and settings reloads; changed or
  truncated files are checked again.
  Claude Code 2.1.303 also treats `apiError: "dlp_request_denied"` and
  `apiError: "safety_monitor_blocked"` on assistant messages as flags; the reader does too.

## Elsewhere

- The background shells, agents and workflows a transcript says a session started
  (`background_step`, `agent_started`) are in `claude-code.md`.
- What a prompt and a call's result record (images, answers, artifacts, files, the PR a call
  made: `pr_made`, `pushed`) is in `claude-conversation.md`.
- What a held or queued message leaves (`queue_step`, `queued_as`, `holding`), another session's
  message and a model switch (`model_set`) are in `claude-send.md`.
- A session's `permission-mode` lines and the worktree it made (`own_worktree`) are in
  `claude-asks.md`.
- A copy's lines, and a conversation moved to the background (`continued_in`), are in
  `claude-start.md`.
- What the Claude app writes ahead of a prompt and in a fork (`AHEAD_OF_PROMPT`, `copied_from`)
  is in `claude-code.md`.

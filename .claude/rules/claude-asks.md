---
paths:
  - "src-tauri/src/claude/asks.rs"
  - "src-tauri/src/claude/questions.rs"
  - "src-tauri/src/claude/mode.rs"
  - "src-tauri/src/claude/live.rs"
  - "src-tauri/src/claude/dialog.rs"
  - "src-tauri/src/claude/exiting.rs"
  - "src-tauri/src/claude/teardown.rs"
---

# What a Claude Code session asks you

What Claude Code asks and how it's answered from outside: a permission prompt and its keys, the
Codealong plugin's hooks (`PermissionRequest`, `MessageDisplay`), a question's dialog, the
permission mode a session runs in, and the dialog `/exit` brings up over a worktree the session
made, with how that worktree is removed. Found by reading its code and testing on this Mac; read
it before changing how Codealong answers a session, or ends one that holds a worktree. Paths are
under `src-tauri/src/`.

## Prompts, questions and modes

- A background job waiting on you: its record says `tempo: "blocked"` and `needs` (a line for the
  list of agents: "answer: <question> (<labels>)", "approve Bash: <command>", "approve plan"), and
  for an `AskUserQuestion` `block.questions` with each question's text and choices, without its
  header or whether it takes several (2.1.303's code). The service's `reply` to a job blocked on
  questions answers the first of them, a choice's label matched whatever its case, else as words
  ("red, Blue" for several), leaving the rest unanswered; a reply led by `/` or `!` is a command
  instead; Escape typed through `attach` declines them ("User rejected tool use") and ends the
  turn (Haiku and Opus 5.5 jobs, 2026-10-19). The record has them while an Opus 5.5 job's
  transcript doesn't, and when the plugin's hook went away before they were answered (seen once,
  across an app relaunch and a plugin update, cause not found; on 2026-11-03 again and again,
  most of them background jobs, because the server had run out of open files: launchd's 256,
  "Too many open files" in `server.log`, so a held request's registry didn't read and was taken
  for gone, and `asks.log` had the hook's side alone. The server raises its limit now
  (`os::system::more_open_files`), a registry entry that doesn't read is read again
  (`registry::entry_of`), and the first time a server runs out it logs what it held, by kind
  (`server.rs` `out_of_files`), since what ran it out wasn't caught): `claude/registry.rs` (`job_questions`), `claude/mod.rs` (`job_pending`,
  `answer_job_questions`). Several are answered with the dialog's keys (questions.rs) typed
  through `attach` once the transcript has the call: two Haiku 5.5 jobs on 2.1.317 took a pick,
  ticks and words of your own that way (2026-11-03). Still on 2.1.317, `block` says no more than
  above.
- A Claude app session's permission mode is in the app's record of it, a terminal session's on
  its transcript's prompt lines and its `permission-mode` line: `claude/mode.rs`
  (`permission_mode`, `session_mode` with the names Claude Code's own table gives them),
  `claude/app.rs` (`permission_mode`, `Records::modes`), `claude/transcript.rs`
  (`TranscriptInfo::permission_mode`). `permission-mode` (and `mode`, `normal` here) are
  last-wins metadata re-stamped with the title and the rest; a switch sets the stamp without
  writing it (2.1.296's code, and 2.1.308's: no hook fires on a switch either, and the status
  line's input has no mode). Of this Mac's 300 latest transcripts on 2026-10-13, 296 ended in
  `bypassPermissions`; by 2026-10-22 both accounts' `defaultMode` was `auto`.
- Shift+Tab in a terminal steps the mode on (2.1.308's `y8t`; Auto, Manual, Accept edits, Plan
  and Auto again in a terminal UI under a pseudo-terminal, 2026-10-22): Manual, Accept edits,
  Plan, then Bypass where it's offered, then Auto where it's offered, and round; Don't Ask goes
  to Manual. Bypass is offered to a session started in it (`--dangerously-skip-permissions`,
  `--permission-mode bypassPermissions`, `defaultMode`) or let to (`--allow-dangerously-skip-permissions`)
  unless `permissions.disableBypassPermissionsMode` is `disable`; Auto unless `disableAutoMode`
  (at the top or under `permissions`) is `disable`, in any settings file (the account's, the
  repo's `.claude/settings.json` and `settings.local.json`, the managed
  `/Library/Application Support/ClaudeCode/managed-settings.json`, `/etc/claude-code` on Linux,
  which ranks over all; an organization's from Anthropic's servers or a macOS profile, and the
  managed drop-in folder, aren't read), `tengu_auto_mode_config.enabled` (kept in the
  state file's `cachedGrowthBookFeatures`, `enabled` when missing) is `disabled`, or the model
  doesn't take it (its list wasn't looked into); `opt-in` there wasn't either. Its key parser
  takes `ESC [ Z` as Shift+Tab whatever keyboard protocol it pushed. A permission prompt takes
  Shift+Tab as its own (`confirm:cycleMode` in its keybindings; what it does to the call that
  waits wasn't looked into), and lists and panels above the prompt as their previous item, so
  cycle.rs presses it only while the session doesn't wait on you. `claude/mode.rs`
  (`cycled_modes`), cycle.rs. A background job takes it typed through its service's `attach`, as
  its own footer shows ("⏵⏵ auto mode on (shift+tab to cycle)"), several in one attach too, and
  writes the mode with its next prompt as a terminal does (a Haiku job on 2.1.325, Bypass by the
  account's `defaultMode`, went to Auto, then two more to Accept edits, and its next prompt was
  written `acceptEdits`; 2026-11-13): `claude/mod.rs` (`press_in_job`). Its process is a spare's,
  whose command line names no flag, so what it was started with is the roster's
  `dispatch.launch.args`, or `flagArgs` for one the service resumed (`mode: "resume"`, the
  resume's options without its own: `--model opus -- <prompt>`), each argument whole, a prompt
  given without `--` among them (`claude/registry.rs` `launched_with`).
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
  2026-10-08). Each request's way through the hook and the server, and why one was let go, is in
  `asks.log` (`claude/asks.rs` `note`). An allow shows
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
  in its transcript, `stop_reason: "tool_use"`, as its question waited (2026-10-16). On 2.1.316
  and 2.1.317 (2026-11-03) an Opus 5.5 session had the call written as its question waited, twice,
  and held it back once, while a Haiku 5.5 one held it back. A line written after it lets it out:
  Haiku 5.5 and Opus 5.5 background jobs asked straight away held the call back, and Haiku jobs
  whose background command reported while they waited (its `queue-operation` `enqueue`) had it
  written by then, and so had an Opus job whose report came 33 s after its question (2.1.317,
  2026-11-03). The hook's input and the registry
  have only the call.
- Claude Code's question dialog in a terminal (2.1.317, in tmux, 2026-11-03), its keys, and
  answering one the hook didn't hand over by pressing them: `claude/questions.rs` (module header,
  `keys`), `claude/mod.rs` (`keyed_questions`). With the call held back as above, such questions
  can't be read at all, so they're answered in the terminal.
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
  (the user chose it on for their accounts, 2026-10-16). Without it what a question came with
  shows in the conversation once it's answered (`Pending.unwritten`, set by `asks::written_in`,
  which the panel no longer says in words: asked 2026-11-18).

## A session's worktree

- The worktree a session made and holds, from its transcript's `worktree-state` lines, and what
  happens to it as the session ends (2.1.292's code, checked in tmux 2026-10-07):
  `claude/transcript.rs` (`own_worktree`), `idle.rs` (`exit`, `EXIT_WAIT`). `/exit` removes it
  when nothing is uncommitted, no commit was added and the session has no name of yours
  ("Worktree removed (no changes)"), else asks "Exiting worktree session" (Keep worktree /
  Remove worktree, Keep first). It always asks of a worktree a `WorktreeCreate` hook made
  (2.1.326's code; there since 2.1.321 at least): its `worktree-state` has `hookBased: true` and
  no `originalHeadCommit`, so the count of added commits is unknown, which counts as some ("A
  WorktreeCreate hook made this worktree, so Claude Code can't tell whether it has unpushed
  commits."), clean or not. The commits it counts are those since `originalHeadCommit`, pushed and
  landed ones too, others' that a rebase brought in among them: a session whose HEAD had landed on
  `origin/main`, nothing uncommitted, asked about "68 commits", every one from where it began to
  that HEAD (2026-11-18). Its sentence (2.1.328's code, composed beside the dialog's choices)
  can also say uncommitted files, commits it couldn't count, a detached HEAD ("It may be lost"),
  the branch HEAD is on now ("which is kept either way"), submodules it couldn't check, files
  saved from connectors in the worktree's `connector-downloads` folder (which git doesn't track),
  a name of yours, or only "You are working in a worktree"; its Remove deletes the branch the
  worktree was made with, `worktreeBranch`, with `git branch -D`, whichever branch HEAD is on.
  It asks with nothing counted where HEAD went on to another branch ("You are working in a
  worktree. Keep it to continue working there, or remove it to clean up. The worktree is now on
  branch <branch>, which is kept either way.", Remove "Clean up the worktree directory.";
  2.1.329, a session whose branch had merged, 2026-11-19). So Mark as done answers Keep, whatever
  it says, where its terminal shows it and its Keep "Stays at" the session's own worktree (not one
  Codealong only supposes is up, which may be another dialog by then; not one started with
  `--tmux`, whose "Keep worktree, end tmux session" runs `tmux kill-session` and takes every other
  pane there with it), and once the session has ended removes the worktree itself, as Clean up
  worktree does, where Codealong's own check finds nothing would be lost, files saved from
  connectors among what it looks at (a folder of that name in any case and with trailing dots or
  spaces, as Claude Code matches it). A Keep you pressed yourself keeps the worktree:
  `claude/exiting.rs` (`keep`), `idle.rs`
  (`keep_worktree`), `actions.rs` (`own_worktree_after`), `cleanup.rs` (`after_end`,
  `saved_from_connectors`). Before, it answered Remove for the reasons its check stood for, which
  left that session asking (2026-11-18 to 2026-11-19). Codealong's removal runs the repo's
  `WorktreeRemove` hooks from its main checkout's settings, without a `session_id`, and deletes the
  branch HEAD is on with `git branch -d`, not `worktreeBranch`, which stays where HEAD went on
  from it. Remove runs the repo's `WorktreeRemove` hook under its `timeout`; one
  that runs out is killed partway, and the exit carries on as removed (`worktree-state` null),
  whatever the hook left: on 2026-11-18 a repo's hook under a 120 s timeout left its worktree's
  folder half deleted and still listed by git. While that's up the registry says `waiting` / `dialog open`, as
  for any command's dialog, and the transcript nothing (`/exit` is written once it closes, not at
  all when it's called off); its screen, its keys (a digit picks, Escape calls the exit off) and
  answering it from Codealong: `claude/exiting.rs` (module header, 2.1.322 in tmux, 2026-11-10).
  Any other dialog its terminal shows while the registry says `dialog open` (the offer to make
  auto mode the default, "Make auto mode your default permission mode?", which came up at a
  terminal session's start and between turns on 2.1.327 and, declined once, not at the next
  start), read off its screen and answered with the arrow keys and Return: `claude/dialog.rs`
  (module header). `/model`'s panel has two spaces after each number, and a More models row and
  an effort control under its list, so it isn't read as one.
  Before it, a session Mark as done had exited came back as Needs you with nothing Codealong
  could show, and was answered only in its terminal. A hangup, as closing its tab gives, removes nothing, runs no
  `WorktreeRemove` hook and leaves the git lock (`claude session <name> (pid … start …)`) behind.
  The lock comes off just before the files are deleted. A session sent no prompt yet has no
  `worktree-state` line, and the exit writes none: it writes a `relocated` line with the
  checkout's folder.
- Removing a worktree from outside Claude Code (2.1.296, a scratch repo, 2026-10-12):
  `cleanup.rs`, `claude/teardown.rs`. A `WorktreeCreate` hook's stdin is `{session_id,
  transcript_path, cwd, scratchpad_dir, hook_event_name, name}`, with no base ref; a
  `WorktreeRemove` hook's has `worktree_path` too (and `session_id`, `transcript_path`, `cwd`).
  A worktree a hook made isn't locked; one Claude Code makes itself is (`claude session …`). `/exit`
  in a session whose clean worktree a hook made ran `WorktreeRemove` then, and the folder went;
  2.1.321 and later ask first (above). `claude
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

## Elsewhere

- A message held from another session (`claude/held.rs`), the mode a session just started is
  taken to be in for one (`starting_mode`) and the question `/model` asks (`claude/switch.rs`)
  are in `claude-send.md`.
- Your answers to a question as its result records them (`answers_of`) are in
  `claude-conversation.md`.
- Making a worktree as a session starts (`-w`, a background job isolating itself) is in
  `claude-start.md`.

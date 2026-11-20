---
paths:
  - "src-tauri/src/claude/mod.rs"
  - "src-tauri/src/claude/start.rs"
  - "src-tauri/src/claude/trust.rs"
  - "src-tauri/src/start.rs"
  - "src-tauri/src/starts.rs"
  - "src-tauri/src/origins.rs"
---

# Starting a Claude Code session

What Claude Code's command line takes to start, resume, copy and move a session, in a terminal
or as a background job (`--bg`), the folders an account trusts first, and what it records of
where a session came from. Found by reading its code and testing on this Mac; read it before
changing a start (`start.rs`, `claude/start.rs`) or what's noted of one (`origins.rs`). Paths are
under `src-tauri/src/`.

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
- Starting with `--session-id`, attaching files with `@"<path>"` mentions in the prompt (up to
  256 KB, 262,144 bytes; a bigger one is silently not attached and left for the Read tool),
  `--add-dir` (it takes several folders, so it goes before the other options or as
  `--add-dir=<dir>`, or it swallows the prompt), and a prompt that would read as an option or a
  subcommand: `attachments.rs` (module header), `claude/mod.rs` (`start_in_terminal`),
  `start.rs` (`as_argument`). Image paths without `@` are only picked up when pasted, and
  there's no image flag.
- `--resume <id> --fork-session` copies a conversation as far as it's written, the original
  carrying on, and takes `--session-id` for the copy only alongside `--fork-session`
  ("--session-id can only be used with --continue or --resume if --fork-session is also
  specified"): `claude/start.rs` (`fork_as`, `fork_args`, `fork_command`). A turn still running
  shows as interrupted in the copy. The registry has the new ID at once, the transcript from the
  first prompt, as with a start (2.1.284). With `--bg` the copy is a background job, and the
  roster's `workers.<job>.dispatch.launch` says `{mode: "resume", sessionId: <id>, fork: true}`;
  without a prompt it prints `(idle — send a prompt to start)` and has no transcript until its
  first prompt, so the board shows it with the conversation it copies
  (`claude::registry::resumed_from`). On 2.1.329, `--name <title>` alongside `--fork-session`
  sets the session's title and agent name in memory and writes `custom-title` when the transcript
  is first written (so a copy closed before its first prompt still writes nothing, which
  `closed_copies` and `fork_args` rely on), and with `--bg` sets `nameSource: "user"` in the job
  record, keeping Claude Code's auto-titler from renaming the copy on its next turn.
- A copy that goes only as far as a point in the conversation (Edit from here):
  `--resume-session-at <uuid>` (and `--resume-drops-turn`) is for print mode only, and ignored
  elsewhere, `--bg` too (2.1.305's help), so the Agent SDK's `forkSession(id, {dir,
  upToMessageId})` writes the copy, through claude-agent-acp: `claude/mod.rs` (`fork_from`),
  `claude/conversation.rs` (`fork_point_before`), `acp.rs` (`fork_point`). It copies the chain through
  that line with new uuids, each line marked `forkedFrom: {sessionId, messageUuid}`, titles it
  "<title> (fork)", leaves the file-history snapshots behind, and finds the transcript by the
  project of `dir`. In place, the stream-json `control_request` `rewind_conversation`
  (`target_message_uuid`, `interrupt_if_running`…) rewinds a session, but only its process's stdin
  and Remote Control take it, not the daemon's socket; `--rewind-files <user message id>` (with
  `--resume`, no prompt) puts the files back as they were at a message. Checked on a Haiku
  session, 2026-10-20: the copy answered from the turn before the cut, not the one after. A copy
  cut just after a call (Rewind to this question, at an `AskUserQuestion`'s message) is resumed
  with a result Claude Code writes for it, an error, "[Tool call interrupted: the session ended
  before this call's result was recorded, so its outcome is unknown…]", then an assistant line
  "No response requested.", and the agent still has the call's questions in mind: asked to ask
  them again under `-p`, where it has no `AskUserQuestion`, it listed all three with their choices
  (2.1.326, Haiku 5.5, 2026-11-17): `claude/mod.rs` (`fork_from`).
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
  (2.1.287, 2026-10-02). The copy is written from the conversation's last `compact_boundary` on
  (a conversation compacted twice moved on 2026-10-19: the copy's first message was that
  boundary's, and none of the original's lines before it were there), so what the session did
  before then (a PR it opened, its pushes) is in the original's transcript alone: `prlinks.rs`
  (`carry_moves`).
- `--model`'s aliases (2.1.282) and the account's `settings.json` `model` a start goes on without
  it: `claude/start.rs` (`MODELS`, `default_model`); it takes a full name too (`claude/mod.rs`
  `offers_model`). Quote it when typing it: zsh reads `fable[1m]` as a file pattern ("no matches
  found").
- `--effort`'s levels and what a model that doesn't take one gets (2.1.284): `claude/start.rs`
  (`EFFORTS`). Without it a session starts on `settings.json`'s
  `modelSettings.<model>.effortLevel`, else its `effortLevel`, else the model's own default (the
  environment's `CLAUDE_CODE_EFFORT_LEVEL` wins over those settings). claude-agent-acp offers it
  as the config option `effort` (category `thought_level`), listed only once the session's model
  takes one, so it's set after the model (`acp.rs` `start`). Its `currentValue` is `default`
  where nothing set one, or the model's recommended level, so it doesn't say what a session is on.
- `-w/--worktree [name]` makes a worktree for the session (read from 2.1.282's code): folder
  `.claude/worktrees/<name>` under the checkout, branch `worktree-<name>`, a word-pair name when
  none is given, based on a freshly fetched `origin/<default branch>` (settings
  `"worktree": {"baseRef": "head"}` bases it on HEAD). A `WorktreeCreate` hook in settings.json
  replaces all of that, even in a git repo, and prints the path. It refuses folders that haven't
  been trusted yet. Its name is optional, so it goes before the prompt: `claude/mod.rs`
  (`start_in_terminal`), `claude/start.rs` (`background_args`).
- A background job isolates itself (2.1.325's code; on this Mac's 96 latest sessions started in
  one repo's main checkout, 86 called EnterWorktree, 2026-11-16): its prompt tells it to call
  EnterWorktree before its first code change unless its folder is under `.claude/worktrees/`,
  and an Edit or Write in the shared checkout is refused until it has. It reads and answers in
  place. `CLAUDE_BG_ISOLATION`, then `worktree.bgIsolation` (`worktree`, the default, or `none`,
  which lets it edit the working copy) turn it off, the variable being what the job was launched
  with as the highest-ranked settings file's `env` replaces it, empty or another word saying
  nothing: `claude/start.rs` (`isolates_in_background`, `bg_isolation`),
  which the quick panel starts such a job by in a main checkout holding other work (start.rs
  `note_isolation`, src/quick.ts `startLaneIn`). Not there: a conversation moved to the
  background with `/background` is a job with `bgIsolation: "none"` in its `state.json`, and a
  subagent of an unisolated job is refused too, told to re-spawn with `isolation: "worktree"`.
- `--bg` (2.1.283): the job ID it prints, the session ID the roster has as soon as it returns,
  `--session-id` ignored, its arguments quoted in the command since the service keeps the
  environment of whichever shell started it, and an untrusted folder refused:
  `claude/start.rs` (`background_args`, `background_job`, `background_session`,
  `start_background`), `claude/mod.rs` (`background_command`, `before_start`). Without a prompt
  it prints `(idle — send a prompt to start)`. `--worktree`, `--model` and `--add-dir` work
  with it. A stale `CODEALONG_SESSION` in the service's environment reaches the job
  (`cli/whoami.rs` `find_me`). `stop <job>` keeps the conversation and the worktree; `rm <job>`
  (below) removes the job's state and, by its own rule, its worktree.
- `--bg` takes itself out of the arguments before it reads them (2.1.284 to 2.1.290), so a
  background start's prompt goes after `--`: `claude/start.rs` (`background_args`). The job gets
  what's left with `--session-id` added (the roster's `dispatch.launch.args`); after `--` the
  arguments are the prompt, joined with spaces, and the job gets the `--` and them as they are.
- `/background` (`/bg`, optional prompt) moves a terminal's conversation to a background job
  with a session of its own (2.1.284): `board/sessions.rs` (`ClaudeMoves`, `claude_moves`),
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
- Trust: the account's state file (`projects.<path>.hasTrustDialogAccepted`), which folders
  count as trusted, the trust prompt whose default answer quits, and writing an entry under
  Claude Code's own lock (2.1.285's code, checked in tmux): `claude/trust.rs` (`state_file`,
  `trust_folder`, `trust_keys`, `StateLock`). Yes at the prompt writes the entry for the
  folder's git root, or the folder outside a repo. `--bg`'s "Workspace not trusted" goes by the
  same entries, with the path's links resolved and in NFC. Running sessions reread the file
  every second, and Claude Code strips `hasCompletedProjectOnboarding`, `history`,
  `projectOnboardingSeenCount` and `devIntentsDetected` from project entries whenever it saves.
  From 2.1.326 a folder `--add-dir` names is checked by the same entries (`untrustedAddedFolder`,
  `foldersToTrustForFlag`; its resolved path and the path as given): `--bg` refuses an untrusted
  one before it starts anything ("Added folder not trusted: <path>. Leave out its --add-dir or,
  if you trust it…", exit 1), and a terminal start asks about each at startup
  (`AddedFolderTrustPrompt`), its No quitting with "start again … without --add-dir <path>". A
  headless run (`-p`) isn't asked, nor is a folder under the session's own outside home, and
  `CLAUDE_CODE_SANDBOXED` or the remote gate `bot_protected_tengu_slate_orchard` (on when
  missing) turns the check off. Images and files Codealong attaches went to a background start
  as `--add-dir` on their folders, refused from that version on (2026-11-16), so `before_start`
  trusts those folders too (`claude/mod.rs` `start_trusts`).

## Elsewhere

- Removing a job and the worktree it made (`claude rm <job>`, a repo's `WorktreeRemove` hooks),
  and what `/exit` asks of a session in a worktree, are in `claude-asks.md`.
- A side conversation's copy, a headless fork that keeps no record, is in `claude-side.md`.
- How long Claude Code keeps a transcript (`origins.rs` `KEEP_MS`) is in `claude-transcript.md`.
- The background service's jobs once they run (spares, a job retired after an hour idle) are in
  `claude-code.md`.

---
paths:
  - "src-tauri/src/claude/mod.rs"
  - "src-tauri/src/claude/side.rs"
  - "src-tauri/src/side.rs"
---

# Claude Code's side conversations

How something is asked beside a Claude Code session without telling it: `/btw` and why it's no
use from outside, the headless fork that keeps no record, and a conversation of its own about a
PR, with what each may run unasked. Found by reading its code and testing on this Mac; read it
before changing `side.rs` or `claude/side.rs`. Paths are under `src-tauri/src/`.

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
  hooks run too. Claude Code's own `/fork [prompt]` ("Copy this conversation into a new
  background session and keep working here"), `/branch` and `/btw` aren't in such a run's
  commands: a `/fork` sent to the copy as a follow-up is answered at once, with no call to the
  model, "/fork isn't available in this environment.", and nothing is started (2.1.314, Haiku,
  2026-10-30). So `/fork` in a side conversation's box is Codealong's, forking the session told
  what was said on the side (`side::fork_prompt`). The copy is told its session's key and may tell
  it something when asked (`side::rules`, `SIDE_VAR`), and `--allowedTools=Bash(codealong
  session send <key> *)` lets that command run in `default` mode with nobody to ask: on 2.1.321
  (Haiku, 2026-11-09) a copy asked a plain question ran nothing, and one asked to tell the main
  conversation ran the command once, unrefused, with the variable in its environment, writing the
  decision out for a conversation that hadn't seen the side one.
- A conversation of its own about a PR (`claude/mod.rs` `fresh_side`, `side.rs` `ask_pr`): the
  same headless run without `--resume`, which works and writes no transcript. In `default` mode
  with nobody to ask, `gh pr diff` and `gh pr view` were refused as needing approval while
  `grep`, `sed -n` and `head` on files went ahead, and a `touch` was refused as a sensitive edit
  (2.1.292, 2026-10-07), so Codealong fetches the PR itself and the run reads it from a folder
  `--add-dir` names. The Codealong plugin's `SessionStart` (`startup`) hook runs for it too.
  On 2.1.329 (Haiku, a scratch repo, 2026-11-19) `git log origin/main` with `--format` and
  `--since` ran unasked, while `git -C <the folder it's in> log`, `git fetch origin`, `gh pr view`
  and `gh pr list` were refused ("This command requires approval"); with `--allowedTools=Bash(gh
  pr view *),Bash(gh pr list *),Bash(git fetch origin)`, one argument, the three ran and `-C` was
  still refused. Allowed so, `gh pr view … > <file>` was still refused, in the folder and outside
  it ("…which is a sensitive file"), as were `gh pr view $(…)` ("A nested command in this command
  can't be checked before it runs") and `git fetch origin main:<branch>`, and nothing was written.
  So it may run `gh`'s lookups (`claude/side.rs` `PR_LOOKUPS`), but no fetch, which a remote's
  configured refspecs can have move local refs.

## Elsewhere

- `--resume <id> --fork-session`, which the copy is made with, is in `claude-start.md`.

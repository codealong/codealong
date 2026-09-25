# Plan: Codealong for agents (CLI + skills)

Status: built 2026-09-24 (phases 1 to 3). README's *For agents* and CLAUDE.md have the result;
this is the reasoning. Decided against: an MCP server (a skill plus the command reaches every
harness here, and an MCP server would add tool schemas to every prompt for the same
operations), a follow mode (an agent asks when it needs to know), and transcript search (Claude
and Codex records are JSONL an agent greps itself; `session show` prints the paths, and
OpenCode's and Antigravity's are databases).

## The idea

Give every coding agent on the Mac (Claude Code on any account, Codex, OpenCode, Antigravity)
a way to use Codealong: create todos and move them along, see which accounts exist and how
much room they have, find other sessions, and know which session and todo it is itself. Two
pieces:

1. **A `codealong` command** on PATH, with JSON output, that talks to the running app and
   falls back to the files when the app is closed.
2. **One skill** (Agent Skills format: a folder with `SKILL.md`) that tells an agent when to
   reach for the command and how, installed into every account Codealong knows about, plus a
   Claude Code plugin wrapper so it can also come from a marketplace.

Skills and a CLI, not an MCP server, is the default: a skill costs the agent a few hundred
tokens until it's needed, where an MCP server's tool schemas sit in every prompt. An MCP
server over the same client is a later option for harnesses that take nothing else.

## What's there today

- The engine binary already doubles as a CLI: `codealong --snapshot [--accounts|--config|
  --start-options|--start-plan|--pr|--open]`. It's for checking behavior, not for agents:
  a board snapshot rebuilds the whole engine in-process (config, sessions, repos, visitors,
  git over every worktree), which takes about 13 s here for 96 lanes and prints 500 KB.
  `--accounts` and `--config` are quick (under a second) because they only read files.
- The app has no IPC. Windows talk to the engine through Tauri commands in `lib.rs` and an
  `engine::Handle` (`board()` clones the published board; `todo(op)` sends a `TodoOp` to
  the engine thread and returns when a board showing it has gone out).
- Todos live in `todos.json` (`todos::load`, `apply`, `save`), written through a temp file
  and rename. The engine watches the file and reloads when its text differs from what it
  last wrote, so an outside writer already works, with one gap: if the engine saves at the
  same moment (a window's edit, or linking a session to a todo), one write is lost.
- Todo ids are letters, digits, `-` and `_` (`t-<base36>` from the UI); a hand-written todo
  without one gets one on read. Status accepts `todo`, `in-progress` (and `in_progress`,
  `doing`), `done` (and `complete`). Session keys are `harness:account:id`; a worktree's id
  is its path. All of this stays as it is: the CLI speaks the same model types (`model.rs`,
  `todos.rs`) through serde, so its JSON is the board's JSON.
- `start.rs` picks the session id for terminal starts and puts the prompt in the window's
  environment (`CODEALONG_PROMPT`), so the app can tell a launched session who it is.
- The design system artifact wasn't readable from this account, so this plan goes by the
  code and CLAUDE.md.

## How the command reaches the data

**Preferred: the running app.** The app opens a Unix domain socket at
`~/Library/Application Support/Codealong/codealong.sock` (mode 0600) in `lib.rs` setup and
serves JSON lines on it: one request object per line, one reply per line. A thread per
connection; each handler holds a clone of `engine::Handle` and does what a Tauri command
does. Requests are the same operations the windows make, so the engine thread stays the
single writer of `todos.json` and the CLI never races a window.

```
→ {"method":"todo.apply","op":{"op":"update","id":"t-…","patch":{"status":"done"}}}
← {"ok":true,"result":1790300000000}
→ {"method":"sessions.list","filter":{"account":"claude2","since":86400000}}
← {"ok":true,"result":[…]}
← {"ok":false,"error":"There's no todo t-zz."}
```

Methods for v1: `status`, `board` (rarely; big), `todos.list`, `todo.apply` (any `TodoOp`),
`accounts`, `sessions.list`, `sessions.find`, `worktrees.list`, `whoami`, and later
`start`. Filtering and trimming happen on the app side so a reply is a few KB, not 500.

**Fallback: the files.** When the socket isn't there (app closed, or an older app), the
command does the least it can in-process:

- Todos: `todos::load`, `apply`, `save`, under an advisory lock on `todos.json.lock` (flock).
  The engine's `save_todos` takes the same lock, which also closes today's lost-write gap.
  `apply` needs a `Lanes` for project lookup; the fallback gives it the repo list from a
  cheap `repos` scan, or an empty one, and a todo created without a project stays
  project-less until linked, as it does from the UI.
- Accounts: what `--snapshot --accounts` does now.
- Sessions and worktrees: `Engine::new` + `load_config` + `scan_sessions` + `load_repos`
  and `compose`, without visitors, git or GitHub: about 2 s here. Rows carry no git status
  or PR then; the JSON says `"partial": true`.

The command prints which path it used on `--verbose`, and `codealong status` says whether
the app is up, the socket path, the app's version, and the data folder.

## The command

One binary. `main.rs` dispatches: a first argument that's a known subcommand runs the CLI
and exits; anything else (or nothing) starts the app as now. `--snapshot` stays for
checking behavior. The parser is `clap` with derive: its `--help` is the reference the skill
points agents at, so it has to be complete and consistent, which hand-parsed args aren't.

Output: JSON when stdout isn't a terminal or with `--json`; a table for people otherwise.
Errors are one line on stderr (JSON when `--json`), exit 1; usage errors exit 2. Ids take a
unique prefix (`t-mug4`, a session's short id). Times are epoch milliseconds, as on the
board, with `--relative` for people.

```
codealong status
codealong accounts [--usage]                      name, harness, signed in, room left, default
codealong whoami                                  this agent's session key, worktree, todos
codealong todo list [--status todo|in-progress|done|all] [--project P] [--label L] [--mine]
codealong todo show <id>                          full description, attachments as paths
codealong todo add <title> [--description TEXT | --description-file F | -] [--project P]
                  [--label L]... [--attach FILE]... [--link-session KEY|me] [--link-worktree PATH|.]
codealong todo update <id> [--title T] [--description ...] [--status S] [--project P]
                  [--add-label L] [--remove-label L]
codealong todo start|done|reopen <id>             status shorthands; start links --me by default
codealong todo link|unlink <id> [--session KEY|me] [--worktree PATH|.]
codealong todo delete <id>
codealong todo labels [add NAME --color COLOR | remove NAME]
codealong sessions [--account A] [--status S] [--project P] [--worktree PATH|.] [--live]
                   [--since 24h] [--limit N]
codealong sessions find <query> [--transcripts] [same filters]
codealong session show <key>                      status, excerpt, cwd, branch, PR, resume command
codealong worktrees [--project P] [--status S]    lanes with branch, PR, bay, agents
codealong start <todo id | --prompt TEXT> --account A [--worktree PATH|.|--new-worktree]
                [--in terminal|claude-app|codex-app] [--model M]        (v2)
codealong install [--command] [--skill] [--account A]...                (see below)
```

`whoami` is what makes todo work cheap for an agent: it finds the calling session without
the agent knowing its id. Two ways, in order: `CODEALONG_SESSION=<key>` in the environment,
which `start.rs` adds to every terminal start it chose the id for; else the command walks
its parent processes and matches a pid the board has for a live session (`Session.pid`,
from `procs.rs`). Through the socket the app does the match; in fallback the scan does. If
neither finds one, `whoami` says so and the todo commands take an explicit key.

`sessions` rows are lean: key, account, harness, status, live, title, excerpt, cwd,
worktree, branch, PR number, lastActivity, lastPromptAt, resumeCommand, openTarget. `find`
matches the same fields the Sessions view searches (title, account, excerpt, callsign, repo,
branch, PR). `--transcripts` also reads each account's own records for prompts and titles:
Claude's `projects/**/*.jsonl` user lines, Codex rollouts, OpenCode's `session_message`
rows, Antigravity's summaries, through the parsers in `claude.rs`, `codex.rs`, `opencode.rs`
and `antigravity.rs`, in parallel and bounded by `--since` and `--limit`. That one is
slower and says so in the skill.

`todo add --attach` goes through `attachments::import`, so files land in the content store
and show in the app like a pasted one. `todo show` prints the description with attachments
as full paths (`attachments::expand`), the same text the app hands an agent as a prompt.

## The skill

One folder, `skills/codealong/`, in the Agent Skills format every harness here reads:

```
skills/codealong/
  SKILL.md          frontmatter: name, description (the trigger), then the how
  reference.md      the command reference, generated from `codealong --help` per subcommand
```

`SKILL.md` says, in about 60 lines: what Codealong is; run `codealong whoami` first; record
follow-up work the user mentions as a todo (`todo add`), with the worktree linked; when
taking on a todo mark it in progress and link yourself, and mark it done when the work is
merged or the user says so, never on your own guess; use `sessions find` before
re-deriving what another agent did in this repo; `accounts --usage` before advising which
account to start something on; prefer `--json`. And what not to do: don't edit
`todos.json` by hand when the command is there; don't loop `accounts --usage`.

The description is the trigger and is written for it: "Codealong todos, sessions and
accounts: record or update todo items, find what other agents or sessions did, check which
accounts have room. Use when the user mentions a todo, asks what else is running or was
done, or asks which account to use."

The reference is generated at build time (`scripts/skill-reference.sh`, run by
`npm run build`), so the skill never drifts from the command.

### Installing it

Codealong knows every account's home from `config.toml`, so it can put the skill where each
harness looks and keep it fresh. `codealong install` (and Settings › Agents, a new pane with
one row per account and a button for the command-line tool):

- The command: a symlink at `/usr/local/bin/codealong` (or `~/.local/bin`, whichever is on
  PATH and writable; ask through an osascript admin prompt for `/usr/local/bin`) to
  `Codealong.app/Contents/MacOS/codealong`. The bundle's path doesn't change across rebuilds,
  so the link survives `scripts/relaunch.sh`.
- The skill: copied (not linked: not every harness follows links, and a bundle can move) into
  each account's skills folder, with a `.codealong-version` file; the app refreshes copies
  whose version is older than its own at launch. Where each harness reads personal skills,
  from their docs (2026-09-24):
  - Claude Code: `<home>/skills/codealong/`. A second account runs with
    `CLAUDE_CONFIG_DIR=<home>`, which moves everything, so `~/.claude2/skills/` is read by
    the `claude2` command and `CLAUDE_CONFIG_DIR=~/.claude2 claude plugin install …` installs
    a plugin there.
  - Codex: `<CODEX_HOME>/skills/codealong/`.
  - OpenCode: `~/.config/opencode/skills/codealong/` (it also reads Claude's folders).
  - Antigravity: `~/.gemini/config/skills/codealong/` (workspace ones in `.agents/skills/`).
  All four read the Agent Skills layout (`SKILL.md` with `name` and `description`, optional
  `scripts/`, `references/`, `assets/`), so it's one folder. `allowed-tools` and
  `disable-model-invocation` are Claude Code extensions and stay out of the shared file.
- Claude Code plugin: `plugin/` in this repo holds `.claude-plugin/plugin.json`, the same
  `skills/` folder, `hooks/hooks.json`, and `bin/codealong`: a plugin's `bin/` is added to
  the Bash tool's PATH while the plugin is enabled, so the shim there (a few lines of shell
  that exec the binary inside `Codealong.app`, found by its bundle id through
  `mdfind`, or at `/Applications`) gives the command to marketplace installs without the
  symlink. `.claude-plugin/marketplace.json` at the repo root makes the repo a marketplace:
  `/plugin marketplace add codealong/codealong`, `/plugin install codealong@codealong`; and
  `claude --plugin-dir plugin/` loads it for one session while developing. The plugin's
  hook is `SessionStart`: it runs `codealong whoami --context` and prints, when the session
  was started from a todo, the todo's title, status and links, so the agent knows what it's
  on without being told. It exits 0 and prints nothing when the command isn't installed.
  (The hooks reference confirms a `SessionStart` command hook's stdout is added to the
  model's context.)

The app's own installer is the main path because it covers every account at once and
Codealong already has the list; the marketplace is for Claude Code users who find the
plugin first.

## Phases

1. **Command, fallback mode** (engine only, no app change): `cli.rs` with clap; `status`,
   `accounts`, `todo *`, `sessions`, `sessions find` (board fields), `session show`,
   `worktrees`, `whoami` by environment and pid; the `todos.json.lock`; `main.rs` dispatch;
   `--snapshot` untouched. Tests: parsing, JSON shape against `model.rs`, lock contention
   (two writers, nothing lost), `whoami` with a fake parent chain. Done means an agent in a
   terminal can create and finish a todo and see it move in the app.
2. **Socket** (app + command): the server in `lib.rs` setup over `engine::Handle`; the
   client in `cli.rs` tries the socket first; `sessions` and `whoami` answered from the
   published board; `CODEALONG_SESSION` set by `start.rs`. Perf log line per request when
   `[debug] perf_log` is on. Done means every command answers in under 100 ms with the app
   up, and the fallback still works with it closed.
3. **Skill and installers**: `skills/codealong/`, the generated reference, `codealong
   install`, Settings › Agents, the plugin and marketplace files, the `SessionStart` hook.
   Checked with `claude plugin eval` or `/skill-doctor` for the Claude Code side, and by
   hand in Codex and OpenCode. README and CLAUDE.md updated (the command reference, the
   socket, the install pane).
4. **Later**: `start` through the socket (the app launches, the todo is linked as from the
   UI); `sessions find --transcripts`; `codealong mcp` (stdio server over the same client)
   for harnesses that take only MCP; `sessions --follow` streaming board changes for a
   long-running agent.

## Open questions

- Antigravity's docs are thin on rules files and MCP; its skills folder is documented and
  that's all the plan needs from it. Check each folder once against the harness before
  phase 3 ships.
- Whether to let an agent delete todos or reorder them from the command. The plan says
  delete yes (it's undoable in the app only while the window's undo lasts, so the skill
  says ask first), reorder no.
- `/usr/local/bin` needs an admin prompt; `~/.local/bin` doesn't but isn't on every PATH.
  The pane can offer both and say which is on PATH.

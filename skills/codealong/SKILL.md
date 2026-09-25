---
name: codealong
description: Codealong todos, sessions and accounts on this Mac, through the `codealong` command. Use when the user mentions a todo or asks to note work for later, when you take on or finish a piece of work, when you need to know what other agents or sessions did in this repo, or when choosing which account to start something on.
---

# Codealong

Codealong is the Mac app that tracks every coding agent running here (Claude Code on any
account, Codex, OpenCode, Antigravity), grouped by project, repo and worktree, and keeps
the todo list of work meant for agents. The `codealong` command gives you the same data and
the same todo operations. Every subcommand prints JSON when you run it (stdout isn't a
terminal), and `codealong <subcommand> --help` is the full reference. `references/reference.md`
has the same text.

## Start here

Run `codealong whoami` once at the start of a task. It says which session you are (key like
`claude:claude2:<uuid>`), which worktree you're in, and which todos are linked to either.
`me` and `.` in other commands mean that session and that worktree. If it finds no session,
todo commands still work; give a session key when you want one linked.

## Todos

- **Record work the user mentions for later** rather than losing it in the conversation:
  `codealong todo add "<title>" --description "<what and why>" --worktree .` The description is
  plain text; Markdown reads fine in the app. `--label <name>` adds a label (made if new);
  `--attach <file>` copies a screenshot or log into the todo. Read a long description from a
  file with `--description-file`, or from stdin with `--description-file -`.
- **When you take on a todo:** `codealong todo start <id>`. It marks the todo in progress and
  links your session and worktree, so the app shows the todo on both.
- **When the work is done:** `codealong todo done <id>`, once it's merged or the user says
  it's done. Don't mark a todo done on your own guess that it is; ask.
- **Look before adding:** `codealong todo list` shows open todos (`--status all` for every
  one, `--mine` for yours, `--worktree .` for this worktree's). `codealong todo show <id>` has
  the description with attachments as full paths you can read.
- `codealong todo update <id>` changes fields (`--title`, `--description`, `--status`,
  `--project`, `--add-label`, `--remove-label`); `link` and `unlink` change what a todo is
  attached to; `delete` removes one, which the app can't undo later, so ask first.
- Ids look like `t-mug4eym1sir0an`; a unique prefix is enough.

## Sessions and worktrees

- `codealong sessions` lists sessions from the last 7 days: status (`needs` means it's
  waiting on the user, `working`, `unread` means its turn ended and nobody looked, `idle`),
  account, worktree, title and what it's doing now. Filter with `--account`, `--status`,
  `--project`, `--worktree .`, `--live`, `--since 24h|3d|all`.
- `codealong sessions find <words>` searches titles, excerpts, worktrees, branches and PRs.
  Use it before re-deriving what another agent did in this repo.
- `codealong session show <key>` has one session in full: its folder, branch, PR, the
  command that resumes it, linked todos, and `records`: the paths of its own files. Claude
  Code and Codex records are JSONL you can grep; OpenCode's and Antigravity's are SQLite
  databases. Codealong doesn't search transcripts itself.
- `codealong worktrees` lists worktrees with branch, PR state, bay (`needs`, `working`,
  `unread`, `review`, `idle`, `merged`) and how many agents are in each.

## Accounts

`codealong accounts` lists the configured accounts and whether each is signed in;
`--usage` adds each plan limit's percent used and `room`, how much of the tightest limit is
left. Read it once when advising which account to use; don't poll it, the usage endpoint
rate-limits.

## Starting sessions

`codealong start --todo <id>` starts a session on the todo (its title and description as the
prompt) in the current worktree, on the account with the most room, in a new terminal
window, and links the session to the todo. `--account`, `--worktree`, `--new-worktree`,
`--in claude-app|codex-app`, `--model` change that. Only do this when the user asks for a
session to be started.

## Configuring Codealong

Codealong's settings are `config.toml` next to its data (`codealong status` prints the path).
The app watches the file, so a change shows at once, and every change the command makes
keeps the file's comments and order. The settings:

- `scan`: folders searched for repos, three levels deep. `codealong config scan add|remove
  <folder>`.
- `[apps]`: `terminal` and `editor` profile ids (`ghostty`, `cursor`; `config show` lists
  them). `codealong config set apps.terminal <id>`.
- `[appearance] theme`: `system`, `light` or `dark`. `codealong config set appearance.theme
  dark`.
- `[debug] perf_log`: `true` writes timings to `~/Library/Logs/Codealong/perf.log`. Set it
  back to `false` when done.
- `[[accounts]]`: one per harness config folder, with `name`, `harness` (`claude`, `codex`,
  `opencode`, `antigravity`), `home` (`~/.claude2`, `~/.codex`, `~/.local/share/opencode`,
  `~/.gemini/antigravity`) and `command` (what starts it; a second Claude Code account
  needs `CLAUDE_CONFIG_DIR=~/.claude2 claude` or a `claude2` wrapper). `codealong config
  account add <name> --harness <h> --home <dir> [--command <c>]` and `account remove
  <name>`. `config show` lists accounts found on this Mac that aren't configured yet.
- `[[projects]]`: groups of repos by pattern (`github.com/owner/*` or
  `github.com/owner/name`) plus `folders` that aren't repos; anything unmatched shows under
  Elsewhere. `codealong config project add <name> --repo <pattern>... [--folder <dir>...]`
  replaces a project of the same name; `project remove <name>`.

`codealong config show` prints the file (with `--json`, the parsed settings, path, text,
parse error and profiles); `codealong config check [file]` says whether a file parses and
passes the app's checks. Prefer the subcommands to editing the file, and run `check` after
any edit you make by hand; while the file doesn't parse the app keeps its last good
settings and the command refuses to write over it.

## Don'ts

- Don't edit `todos.json` by hand; the command exists so the app and you never write over
  each other.
- Don't loop `accounts --usage` or `sessions`; ask when you need to know.
- Don't mark todos done, delete them, or start sessions without the user's say-so.

# Codealong

A Mac app that keeps track of every coding agent you have running: Claude Code on two
accounts (`claude`, `claude2`) and the Codex app. It sorts them by project, worktree,
branch and PR, shows what each one is doing, and opens any session where it lives:
Ghostty for terminal sessions, the Claude app, Codex, or Cursor for the folder.

**Every agent runs in a worktree, and every worktree is a lane.**

The design lives in the Codealong design system (tokens, components, and the model,
surfaces, harness adapters, apps, architecture and build plan pages):
https://claude.ai/artifact/Eg7p4HkWgTVUR54RfYgUPc

## Status: 0.1, the board

- Lanes for every worktree of your project repos, with callsigns and origin tags.
- Live Claude Code sessions for every config folder (`~/.claude`, `~/.claude2`) from their
  session registries, plus transcripts from the last 14 days.
- Codex threads from `~/.codex` (threads you started; guardian reviews and subagents are
  skipped).
- Git state per lane (branch, ahead/behind, uncommitted files, diff against the default
  branch), read-only.
- PRs and checks from GitHub through your `gh` sign-in.
- Bays: Needs you, Working, Your turn, In review, Idle, Merged. Parked lanes fold away.
- Open where it lives: focus a live terminal session in Ghostty, resume a stopped one
  (`claude2 --resume <id>`), open Claude app sessions and Codex threads by deep link, open
  the folder in Cursor, open the PR on GitHub or review it in Codex.

Next (0.2): the menu bar icon and popover, usage for both Claude accounts and Codex, and
notifications. See the design system's *Build plan*.

## Requirements

- macOS 13 or later
- Rust (`rustup`), Node 20+
- `gh` signed in (`gh auth login`) for PRs
- Ghostty 1.3+ and Cursor (both swappable later through app profiles)

## Develop

```bash
npm install
npm run tauri dev      # the app with hot reload
npm run tauri build    # src-tauri/target/release/bundle/macos/Codealong.app
npm run snapshot       # print the board as a table without opening a window
```

Tests: `npm test` (formatting), `npm run typecheck`, and `cargo test` in `src-tauri`
(model rules, parsers, git and GitHub parsing, deep links, Ghostty scripts).

## Settings

`~/Library/Application Support/Codealong/config.toml`, written on first run from what
Codealong finds and watched for changes:

```toml
scan = ["~/projects"]          # where repos are found, three levels deep

[apps]
terminal = "ghostty"
editor = "cursor"

[[accounts]]
name = "claude2"               # what the board shows
harness = "claude"
home = "~/.claude2"
command = "claude2"            # used for `claude2 --resume <id>`

[[projects]]
name = "Northwind"
repos = ["github.com/northwindai/*"]
folders = []
```

What you've seen (for Your turn) is kept in `state.json` next to it.

## Permissions and safety

- The first time Codealong opens or focuses a terminal session, macOS asks whether it may
  control Ghostty (Automation). Without it, Codealong can still open plain Ghostty windows.
- Codealong only reads: it never creates, resets or removes worktrees, never changes
  branches, and never types into a live session. Git runs with `GIT_OPTIONAL_LOCKS=0`.
- Transcripts stay on the Mac; only titles, statuses and short excerpts are kept in memory.

## Code

| Path | What |
| --- | --- |
| `src-tauri/src/engine.rs` | Caches, file watching (FSEvents), git workers, GitHub, publishing the board |
| `src-tauri/src/board.rs` | The model's rules: status, bays, headlines, open targets (pure, tested) |
| `src-tauri/src/claude.rs`, `codex.rs` | Harness adapters: registries, transcripts, rollouts, incremental tailing |
| `src-tauri/src/repos.rs`, `git.rs`, `github.rs` | Repo discovery, callsigns, worktrees, git status, PRs |
| `src-tauri/src/launch.rs` | App profiles: Ghostty (AppleScript), Cursor (CLI), deep links |
| `src/` | The React UI, built from the design system's components and tokens |
| `design/tokens.json` | Design tokens; `npm run tokens` regenerates `src/styles/tokens.css` |

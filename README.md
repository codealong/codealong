# Codealong

A Mac app that keeps track of every coding agent you have running: Claude Code on two
accounts (`claude`, `claude2`) and the Codex app. It sorts them by project, worktree,
branch and PR, shows what each one is doing, and opens any session where it lives:
Ghostty for terminal sessions, the Claude app, Codex, or Cursor for the folder.

**Every agent runs in a worktree, and every worktree is a lane.**

The design lives in the Codealong design system (tokens, components, and the model,
surfaces, harness adapters, apps, architecture and build plan pages):
https://claude.ai/artifact/Eg7p4HkWgTVUR54RfYgUPc

## What it does

- **Three views** of the current project, each in a window of its own if you like (⌘⇧N):
  - **Worktrees**: pinned worktrees first in your order, then agents that need you, are
    working or just finished, then worktrees in review, idle and merged. Stuck agents (busy
    but silent for 30 minutes) show in Needs you with the command they're stuck on. Pin a
    worktree with `p` or its right-click menu, and reorder pins by dragging or with ⌥↑ ⌥↓; a
    pinned worktree shows as one row with its agents, and stays whatever the age filter says.
  - **Sessions**: every session, pinned ones first in your order (drag or ⌥↑ ⌥↓), then by
    day. `p` pins a session here. Archive a session with `a` or its right-click menu; Codex threads are archived in
    Codex itself.
  - **PRs**: every PR in the project's repos, with the worktree and sessions that worked on
    each. Reviews asked of you come first, with one-click Approve. Selecting a PR shows its
    description, reviews, checks and changed files with their diffs.
- **Open where it lives**: the session's own Ghostty tab, the Claude app session, the Codex
  thread, a background job attached in a new window, or the folder in Cursor.
- **New session** (⌘N, the title bar, the inspector, or a worktree's or session's right-click
  menu): pick the worktree, the account, and where it runs. Ghostty starts Claude Code on your
  prompt; the Claude app and Codex open a new session with the prompt filled in, and you send it
  there (their own worktree choice included). Auto starts whichever account of that harness has
  the most room before a limit. Codealong only launches: the session shows on the board like
  any other, and one started in Ghostty is selected as soon as it appears.
- **Usage** for every account in a bar across the top: 5-hour and weekly limits, per-model
  limits such as Fable, and time to reset. Add accounts in Accounts (⌘,).
- **Filters**: the Inbox (Needs you, Working, Your turn, In review, Checks failing), search,
  and how recently things were active (24h, 3d, 7d, all).

Next: the menu bar icon and popover, and notifications (the design system's *Build plan*).

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

Tests: `npm test` (vitest), `npm run typecheck`, and `cargo test --release` in `src-tauri`
(model rules, parsers, git and GitHub parsing, usage, config writes, deep links, Ghostty
scripts). `scripts/preview.sh` renders the UI in headless Chrome against your real data with
Tauri's IPC stubbed. CLAUDE.md has the rest of the working notes.

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

What you've seen (for Your turn), what you've pinned and what you've archived in Codealong
are kept in `state.json` next to it.

## Permissions and safety

- The first time Codealong opens or focuses a terminal session, macOS asks whether it may
  control Ghostty (Automation). Without it, Codealong can still open plain Ghostty windows.
- Codealong only reads: it never creates, resets or removes worktrees, never changes
  branches, and never types into a live session. Git runs with `GIT_OPTIONAL_LOCKS=0`.
- Starting a session types the account's command into a new Ghostty window; the prompt goes in
  the window's environment (`CODEALONG_PROMPT`), so nothing in it is typed or needs quoting.
- Transcripts stay on the Mac; only titles, statuses and short excerpts are kept in memory.

## Code

| Path | What |
| --- | --- |
| `src-tauri/src/engine.rs` | Caches, file watching (FSEvents), git workers, GitHub, publishing the board |
| `src-tauri/src/board.rs` | The model's rules: status, bays, headlines, open targets (pure, tested) |
| `src-tauri/src/claude.rs`, `codex.rs` | Harness adapters: registries, transcripts, rollouts, incremental tailing |
| `src-tauri/src/repos.rs`, `git.rs`, `github.rs` | Repo discovery, callsigns, worktrees, git status, PRs |
| `src-tauri/src/launch.rs` | App profiles: Ghostty (AppleScript, exact tab focus), Cursor (CLI), deep links |
| `src-tauri/src/start.rs`, `src/start.ts` | New sessions: where each account can start one, the launch it makes, Auto's pick |
| `src-tauri/src/usage.rs` | Plan usage: Claude through its sign-in and usage endpoint, Codex from its rollouts |
| `src-tauri/src/appserver.rs` | Codex actions through its app-server (archiving) |
| `src-tauri/src/claudeapp.rs` | The Claude app's session records (its IDs, what you archived there) |
| `src-tauri/src/config.rs`, `state.rs` | `config.toml` (accounts saved in place) and `state.json` (seen, pinned, archived) |
| `src/` | The React UI, built from the design system's components and tokens; `lanes.ts` groups the board and mirrors the engine's status rules for the age filter; `pins.ts` keeps the pinned lists |
| `scripts/preview.sh` | Headless preview against real data, for screenshots and scripted checks |
| `design/tokens.json` | Design tokens; `npm run tokens` regenerates `src/styles/tokens.css` |

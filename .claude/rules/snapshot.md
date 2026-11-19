---
paths:
  - "src-tauri/src/snapshot.rs"
---

# `--snapshot`: the engine without a window

`codealong --snapshot …` checks behavior without a window (snapshot.rs). With no mode it reads
everything once and prints the board; a mode prints one thing the app shows, or does one thing
it does, over a board read once. `"$(scripts/engine.sh)" --snapshot …` runs your checkout's
engine; the `codealong` on your PATH is the running app's build.

Every mode is a line of snapshot.rs `MODES` and a line here, with its arguments as `MODES` has
them: a test holds the two together (`every_mode_has_its_line_in_the_notes`), so a new mode or a
changed argument is written here in the same change.

- A session's modes are the app's own actions over a board read once (actions.rs), asked as
  another device asks them, so each does what a window's does and none claims to be you at a
  window.
- The ones marked **Real** act for real: a message is sent, a session ends, usage is spent. Real
  sessions are fine to test with; what reaches other people isn't (don't approve or merge PRs to
  test).
- Claude usage is read only with `--usage`, and its endpoint rate-limits: don't loop it.
- An argument that doesn't read as the mode's prints its usage and exits 2; a mode that fails
  says why and exits 1.

## The board

- `--snapshot [--table] [--no-github] [--usage]`: one pass over everything, printed as JSON, or
  as a table with `--table`. GitHub is asked unless `--no-github` says not to.
- `--snapshot --bar [--no-github]`: the floating bar's feed.
- `--snapshot --checkout`: the checkout the binary was built from.
- `--snapshot --improve`: where Improve Codealong starts (improve.rs), without the pictures.

## Accounts and settings

- `--snapshot --accounts`: the configured accounts, each with whether it's signed in.
- `--snapshot --found`: accounts on this Mac that config.toml doesn't list.
- `--snapshot --config`: the settings and config.toml, as Settings gets them.
- `--snapshot --setup`: what the setup wizard offers (setup.rs).
- `--snapshot --training`: whether each account's provider may train on it, asked of each
  provider now (training.rs).
- `--snapshot --training-off <account>`: turns training off for the account at its provider, as
  the usage bar's flag does, and prints where it stands after. **Real.**
- `--snapshot --resets`: the limit resets each account holds, read live (resets.rs).
- `--snapshot --use-reset <account> [credit id]`: uses one of the account's limit resets, the one
  named or else the first to expire, as the usage bar's card does. **Real:** it spends the reset,
  so anything else on the line (a `--dry-run`, say, which nothing here reads) is a usage error
  rather than a reset spent.
- `--snapshot --mcp`: each account's MCP servers and connectors (mcp.rs).
- `--snapshot --skills [folder…]`: the skills a session in each folder could be asked to run by
  name, and each account's own, as the `/` lists get them.
- `--snapshot --editor-icons`: the installed editors' app icons, as the Open in menus get them.
- `--snapshot --shell-commands`: what a new terminal's shell takes as a command's first word, as
  a conversation's code blocks get it for Run.

## Opening things

- `--snapshot --open <lane id> <target id>`: opens a target the way a click does. **Real:** the
  editor, terminal or page opens.
- `--snapshot --link <url>`: where a click on a web link in a conversation would open it, without
  opening it (a worktree's app opens in its preview's Chrome profile).

## Pull requests

- `--snapshot --pr <owner/name> <number>`: a PR's detail, as the PR panel gets it.
- `--snapshot --pr-card <owner/name> <number>`: a PR the board doesn't list, as a link to it gets
  it on hover.
- `--snapshot --merge-box <owner/name> <number>`: what a PR's merge box offers you. It only
  reads: what the box does is GitHub's, on the PR.
- `--snapshot --review-plan <owner/name> <number> [--usage]`: what an automatic review of it
  would do (autoreview.rs `plan`), without doing it. With `--usage`, a kept reviewer on an
  account at its limit is passed over as the server would.
- `--snapshot --pr-draft <owner/name> <number>`: your draft review of a PR with its comments, and
  an agent's review of yours you submitted, as its Review tab gets them.
- `--snapshot --pr-since <owner/name> <commit> <commit>`: what changed between two commits, as
  Changes shows it since your last review.
- `--snapshot --pr-side <owner/name> <number> <account> <question> [a follow-up…] [--model <model>] [--effort <level>]`:
  asks about a PR the board lists on an account, as Ask in its card's box does, and prints the
  questions with their answers. **Real:** it spends usage.

## The quick panel and Jev

- `--snapshot --route <text> [--project <name>]`: what Jev suggests in the quick panel for it. It
  asks TypeSafe: it needs the key, and `[jev] quick_panel` on.
- `--snapshot --routes [days]`: how often what you did there was Jev's first suggestion, over the
  last days (7 unless given), with the times it wasn't (routelog.rs).
- `--snapshot --jev-usage [days]`: what Settings › Jev says Jev's requests used each of the last
  days (30 unless given), from the app's ledger.

## A session, read

- `--snapshot --transcript <session key> [--before <cursor>]`: a page of a session's
  conversation, as its panel gets it.
- `--snapshot --pending <session key>`: what it waits on you to allow, as its panel shows it.
- `--snapshot --tab-picture <session key> <file>`: a session's terminal tab, as Improve Codealong
  pictures it, written to the file. macOS lets the process that runs this picture another app's
  window only with the Screen Recording permission of the app it runs in.

## A session, acted on

All **Real**.

- `--snapshot --answer <session key> <id> <allow|deny|always> [answers to its questions, as JSON]`:
  answers what it waits on, as the panel's buttons do.
- `--snapshot --send <session key> <text> [--now]`: sends it a message, as another device does;
  `--now` sends it into the turn it's taking, as Send now does.
- `--snapshot --tell <session key> <text>`: sends it what Codealong sends by itself, as a PR's
  news or a review handed back goes (send.rs `tell`).
- `--snapshot --steer <session key> <queued message id>`: sends a queued message into the running
  turn.
- `--snapshot --unqueue <session key> <queued message id>`: takes one out of the queue.
- `--snapshot --rename <session key> <name>`: names it where its harness keeps names, as Rename…
  in its menu does.
- `--snapshot --stop <session key>`: stops the turn it's taking, as its Stop does.
- `--snapshot --done <session key> [--stop] [--end-helpers]`: ends it as Mark as done does,
  without archiving; `--stop` ends work under way too, as Mark as done pressed again does, and
  `--end-helpers` ends each helper under it whose work isn't over, as the dialog's choice does.
- `--snapshot --fork <session key> [prompt] [--from <cursor>]`: forks it into the background, as
  `/fork` in its box does; `--from` copies it up to just before that prompt of yours and goes on
  from `prompt` in its place, as editing the prompt does. A copy to remove after.
- `--snapshot --move <session key> <account | worktree folder>`: moves it to another account of
  its harness, or to a worktree, as Move to in its menu does.
- `--snapshot --resume <session key> <terminal|background|claude-app|codex-app|codealong> [prompt] [--model <model>] [--effort <level>]`:
  goes on with a session that isn't running, there, as its box and Resume in do.
- `--snapshot --switch-model <session key> <model>`: switches a running session's model, or a
  Codex app thread's for its next turn, as its box does.
- `--snapshot --switch-effort <session key> <level>`: the same for its effort.
- `--snapshot --switch-mode <session key> <mode id>`: switches it to one of the modes it offers
  (`Session::modes`), as its mode chip does: in a terminal, by Shift+Tab pressed there (cycle.rs).
- `--snapshot --side <session key> <question> [a follow-up…]`: asks a throwaway copy of it, as
  `/side` in its box does, and prints the questions with their answers. It spends usage.
- `--snapshot --remove-worktree <worktree folder> [--dry-run]`: removes a worktree only where
  nothing in it would be lost, as Clean up worktree does; `--dry-run` says whether it can go and
  what would run.

## Starting a session

Starting is checked without starting one.

- `--snapshot --start-options`: where each account can start a session.
- `--snapshot --start-plan <folder> <account> <terminal|background|claude-app|codex-app> [prompt] [--model <model>] [--effort <level>] [--worktree]`:
  what a start would run, without starting: where it would go now, and for a terminal whether it
  would run in tmux (`start::plan_now`).

## Terminals, hibernating and hearing

- `--snapshot --stale-terminals [how long unused: 90m, 24h, 3d, 1w]`: the tabs Close stale
  terminals would list, unused that long (a day unless given), closing none.
- `--snapshot --hibernate [how long idle: 30m, 2h, 1d]`: the sessions that would hibernate now
  and why the rest stay, after `[sessions] hibernate_after` (two hours while that's off) or the
  time given, closing none.
- `--snapshot --hearing`: what this Mac hears with, and the apps that record or play now
  (ear.rs), asking the person for nothing.
- `--snapshot --transcribe <sound file> [locale]`: a recording's words as a listening would write
  them to its transcript, the file in place of the microphone.
- `--snapshot --listen-plan [what you'd type] [--in <worktree folder>]`: what Listen now would
  start for a meeting that's in no calendar (meetings.rs `plan`), hearing nothing.

## Schedules

- `--snapshot --schedules`: the schedules as their view lists them, with when each runs next and
  its latest runs.
- `--snapshot --harness-schedules`: the ones the harnesses keep themselves
  (schedules/harnesses.rs).
- `--snapshot --schedule-plan <schedule>`: what a run would start or send now, as Run now would,
  without doing it.
- `--snapshot --schedule-check <schedule>`: runs its watch's command once and says what's new,
  noting nothing.
- `--snapshot --schedule-times <time, as a schedule writes it>`: a time as a schedule reads it.
- `--snapshot --schedule-run <schedule>`: runs it as Run now does, and notes the run. **Real:** a
  session starts, or one is sent to.

## Analytics

Both read this Mac's records afresh into a ledger of the check's own (a temporary file, or
`--into` one to keep and read on from), never the app's.

- `--snapshot --tokens [days] [--account <name>] [--session <key>] [--into <ledger file>]`: the
  Analytics window's report for the last days (30 unless given); `--session` prints that
  session's instead.
- `--snapshot --analytics [days] [--account <name>] [--into <ledger file>]`: Analytics' Your part
  and Sessions.
- `--snapshot --stance [how many] --into <ledger file>`: reads the newest prompts that wait on a
  read in such a ledger (10 unless given) and prints each with what Jev made of it. **Real:** it
  asks TypeSafe, and keeps each read in that ledger.

---
paths:
  - "src-tauri/src/harness.rs"
---

# Adding a harness

A harness is a module that knows its files, sockets and commands: a folder split by subject
for the big ones (`claude/`, `codex/`, the adapter and the reader in `mod.rs` and each file's
first lines saying what it holds), a file for the others (`opencode.rs`, `antigravity.rs`). Two
things in it are what the rest of the app goes through (`harness.rs`): an `Adapter`, which
answers for a session and for an account, and a `Reader`, which reads its sessions for the
board. Nothing outside a harness's module branches on which harness it is: it asks
`harness::adapter(session.harness)` or `harness::of(account)`, or every adapter
(`harness::all()`) for what any of them may have, such as the PR a line of a record says a
session made. What a harness's own sockets and windows are written is pinned byte for byte by
tests beside the code that writes it (`the_daemon_is_written_these_lines`,
`a_sessions_socket_is_written_these_lines`, `the_codex_apps_windows_are_written_these_frames`):
a change to what's sent shows there. What each harness records and accepts is in
`.claude/rules/<harness>.md`. Adding one:

1. A variant of `model::Harness` with its name in `Harness::word` and its config string in
   `Harness::config` and `from_config` (codealong-peer's `model.rs`), in `config::HARNESSES`, and
   in `src/types.ts` and `src/harness.ts`.
2. A module with a struct implementing `harness::Adapter`, listed in `harness::ADAPTERS`. It must
   say `harness`, `spec` (its folder, the variable that moves it and its command, which is how
   detect.rs finds its accounts), `records`, `targets` (the ways to open a session) and `reader`;
   everything else defaults to saying it can't. What it can answer, by what asks:
   - a session's panel: `conversation` and `conversation_after` (`transcript.rs` pages a file of
     JSON lines given a line parser), `pending`, `answer`, `deny_saying`, `answer_questions`,
     `open_calls`;
   - opening: `targets`, `open` for a way of its own and `app` for its app's name, `app_bundle`
     for the app a way lands in (so it can be brought forward); the engine does `focus`,
     `resume` (`Session::resume_command`) and `attach` for every harness;
   - sending (send.rs): `routes`, best first: typing into its terminal and resuming it in a new
     one (`resume` for what that types) are send.rs's, and a way in of its own (its service, a
     socket, its app) is a type in its module that implements `send::Own` and sends by it, told
     whose the message is and whether it goes in now (`send::Delivery`); `typed_attachments`,
     `clears_before_typing`, `takes_messages`, and for Send now `steers` and `sends_now_with`
     (the keys typed in place of Return);
   - acting on it: `fork`, `fork_background`, `moves_accounts` with `move_to` and
     `move_to_background` (to another of its accounts), `move_folder` and
     `move_folder_background` (to another folder, as itself), `side` and `fresh_side` (a
     throwaway conversation of a session's, or one of its own about a PR), `stop`, `rename`,
     `interrupts` and `interrupt`, `archive`, `skips_permission_prompts`, `runs_its_commands`;
   - starting (start.rs): `places`, `start_in_terminal`, `runs_in_background` with
     `background_command` and `start_in_background`, `app_link` and `start_in_app`, `models`,
     `takes_model`, `offers_model`, `efforts`, `default_model`, `makes_worktrees`,
     `before_start`, `app_projects`;
   - search (search.rs): `search_files` and `prose` for records that are files of lines, `said`
     for messages kept in a database;
   - skills (skills.rs): `worktree_skills` and `account_skills` (the folders it reads them
     from), `skill_mention` (how a prompt runs one by name) and `skill_uses`;
   - its accounts: `installed`, `signed_in`, `sign_in_command`, `folder_use`, `skills_dir`,
     `installs_plugin` (Codealong goes in as a plugin, not a skill copy), `acp_command`,
     `session_cwds` (first run's projects), `chats` (the folder of sessions it starts without a
     folder of yours, which get one lane), `no_limits`, and `polls_usage` with `read_usage` for
     usage asked from somewhere slow;
   - the engine: `scratch_owner` (the session a scratch folder of its own is for, whose worktrees
     show it), `databases` (the ones in WAL mode, whose `-wal` the watcher keeps), and, for its
     records read as text without parsing, `output_marks` and `folder_marks` (a tool's output,
     and what comes before the folder a command ran in) and `pr_needles` with `pr_made` (a PR
     the session made, for prlinks.rs).
3. A `harness::Reader` in the module, which `Adapter::reader` makes and the engine keeps without
   looking inside: `scan` (every account's sessions afresh, at launch and when config.toml
   changes), `watch` (the folders to watch), `owns` and `apply` (taking in the files that
   change), what the watcher never says (`tick` every ten seconds, `glance` every second or two,
   `poll` for what's asked off the engine's thread), `archives_changed`, and `sessions`, put
   together with a `board::*_session` constructor for its status rules, with where each came
   from (`Session::origin`: what it's a fork of, or which session started it) when its records
   say; `cwds` (the folders sessions ran in, for their lanes) and `logs` (records that are files
   of lines, searched for a session's worktrees and PRs). Plan usage that comes with the
   sessions is `Reader::usage`. A database is opened with `harness::open_database`, which reads
   a WAL database its writer has closed.
4. What's still said harness by harness, and so needs a line for a new one: a new way in of its
   own needs a `Way` in codealong-peer, for what the reply box says (`send::Own::way`); a new
   place a session starts is a `Place` there too; a worktree folder the harness makes for itself
   is known by its path in repos.rs (`callsign`, `origin`, with an `Origin` of its own in
   codealong-peer); `board::session_headline`'s words for an approval, mirrored in
   `src/lanes.ts`; and the UI's own lists, which the engine doesn't send: names and apps in
   `src/harness.ts` (and there which can answer questions about a PR, `FRESH_SIDES`), which
   sessions take `/fork` and `/side` in `src/slash.ts`, a new account's defaults and help in
   `src/ui/AccountsSection.tsx`, the reply box's words in `src/ui/SessionReply.tsx`.

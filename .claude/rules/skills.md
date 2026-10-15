---
paths:
  - "src-tauri/src/skills.rs"
  - "src-tauri/src/install.rs"
  - "src-tauri/src/*/skills.rs"
  - "src/skills.ts"
  - "src/ui/SlashList.tsx"
  - "plugin/**"
  - "skills/**"
---

# Skills

Where each harness reads skills (one folder in the Agent Skills layout, `SKILL.md` with `name`
and `description`, optional `references/`, read by all five harnesses), how a prompt runs one
by name, and how `claude plugin` installs Codealong's plugin, moved from CLAUDE.md's Harness
facts (from the harnesses' docs, 2026-09-24, and their code and runs on this Mac). Paths are
under `src-tauri/src/` unless they start with `plugin/`.

- Each harness's personal skills folder: its adapter's `skills_dir` (`claude/mod.rs`,
  `codex/mod.rs`, `opencode.rs`, `antigravity.rs`, `cursor.rs`). Why the skill is copied rather than linked:
  `install.rs` (header).
- Where Claude Code finds the skills and commands a prompt can run by name (`/name args`, as a
  first prompt too), and how often each ran: `claude/skills.rs` (header, `plugin_roots`,
  `uses`), `claude/mod.rs` (`skill_mention`), and the frontmatter `skills.rs` reads (`Skill`:
  `argument-hint`, `user-invocable`, `disable-model-invocation`). Not there (2.1.289): it reads
  a `.claude` folder in the session's folder and those above it, not `.agents/skills`, and
  `skillUsage` has no repo.
- The skills Claude Code's program holds (`/design`, `/slides`, `/loop`), which only it can list,
  told apart from its commands (`/compact`), and how Codealong asks without spending usage:
  `claude/builtin.rs` (header, `ARGS`, `read`, `skills_in`). Not there (2.1.299 and 2.1.300,
  2026-10-14): the same answer to `initialize` has `agents`, `models`, `account` and
  `output_style` beside `commands`; `reload_skills` answers `{skills}`, the skills the model may
  load itself (those a skill's file keeps from the model aren't in it); `get_skills_dialog`
  lists the plugins' and claude.ai's skills with their `source` and `state`. A command the
  terminal's menu hides (`heapdump`, `__remote-workflow`) still comes in `initialize`'s answer,
  and a synced skill comes there by its name alone (`pdf`, with `anthropic-skills:pdf` an alias),
  where `init`'s `skills` has it whole. `init` also has `terminal_slash_commands`, the commands a
  phone or remote front end should hide. Only a skill that starts a prompt is run as one (a
  `/name` later on is a hint to the model at most), so the windows count only that one.
- Where Codex finds skills, and `$name` in a prompt's text: `codex/mod.rs` (`worktree_skills`,
  `account_skills`, `skill_mention`), `skills.rs` (`walk_skills`, a skill reached through a
  link counted once). Not there (0.157.1): a name two skills share is named by file,
  `[$name](<path to SKILL.md>)`, and its app-server has `skills/list {cwds}`.
- OpenCode 2.0.16 reads `skill` and `skills` under `.opencode`, `.claude` and `.agents`, up the
  tree and at home, and lists them at `GET /api/skill?location=`; one is run only as a
  structured mention on a prompt (`skills: [{id, mention}]` in the body of `POST
  /api/session/<id>/prompt`, `@id` in the TUI) or `POST /api/experimental/session/<id>/skill
  {id}`, never by text, so Codealong offers none there yet.
- Antigravity reads `.agents/skills` and `~/.gemini/config/skills` and has `/skills` in its TUI;
  nothing found runs one by name.
- Where Cursor finds skills (the app 3.22.7's `cursor-agent-exec` extension and the command line
  2026.08.11's bundle) and `/name` in a prompt: `cursor.rs` (`SKILL_FOLDERS`, `OWN_SKILLS`,
  `skill_root`, `skill_mention`, `worktree_skills`: the workspace's own folder, none above it),
  `skills.rs` (`AsCommands`, `command_skills`, `surfaces`). Not there: the app also finds
  `.cursor/skills` and the others nested anywhere under the workspace, ten folders down,
  skipping hidden ones, `node_modules`, `dist` and `build`. A skill is keyed by its path with the
  `.cursor/`, `.claude/`, `.codex/`, `.grok/` or `.agents/` part taken out, so
  `~/.cursor/skills/codealong` and `~/.codex/skills/codealong` are one skill. The app's
  third-party setting is `thirdPartyExtensibilityEnabled` in the ItemTable's `applicationUser`
  (true when absent); with it off it reads only `.cursor` and `.agents`. It leaves out Codex's own
  skills (`imagegen`, `openai-docs`, `plugin-creator`, `skill-creator`, `skill-installer`)
  wherever they are under `.codex/skills`, and loads Claude Code's enabled plugins
  (`~/.claude/plugins/installed_plugins.json`) with their skills, which Codealong doesn't list for
  it. The command line also adds `-2` to a name already taken, which Codealong doesn't, and
  within a link it follows out of your personal folders it reads the linked folder's own
  `SKILL.md` only, not those below it. It runs `.claude/commands` and `.cursor/commands` files
  (top level only) as `/name args` too. Cursor's own `canvas` and `update-cursor-settings` list
  only `ide` (and `cloud`) in `metadata.surfaces`, `update-cli-config` only `cli`.
  Checked with `-p` and with its terminal UI's first prompt in tmux (2026-10-15): a `/name` from
  `.cursor/skills` and from a repo's `.claude/skills` ran a skill kept from the model
  (`disable-model-invocation: true`); in the terminal UI the agent found it itself ("Used
  pineapple-probe") rather than having it attached, likely because its list of skills, read in
  the background, wasn't in yet as the first prompt went. The app's box makes a skill of a `/name` picked from its menu (a `cursor_skill`
  mention); a `/name` in pasted text was not tried.
- A Claude Code plugin's `bin/` on the Bash tool's PATH: `plugin/bin/codealong` (header).
  `${CLAUDE_PLUGIN_ROOT}` resolves in hooks.
- `claude plugin`, as `install.rs` runs it for an account (`CLAUDE_CONFIG_DIR` choosing it):
  folder marketplaces, updating on a new version string, `enabledPlugins`, the files it reads,
  and why it passes `--scope user`: `install.rs` (header, `shipped_version`, `PluginState`,
  `plugin_state_in`, `plugin_verb`, `install_plugin`). Not there (2.1.284): `marketplace add
  <folder>` also writes `extraKnownMarketplaces` in settings.json; `install codealong@codealong`
  copies the plugin, links followed, into `plugins/cache/<marketplace>/<plugin>/<version>`, its
  `installPath`; `list --json` says whether it's enabled; `update` succeeds when there's nothing
  to do, and new sessions get the update; `enable` fails on an enabled plugin; `marketplace
  remove` uninstalls the marketplace's plugins with it. A GitHub marketplace (`owner/repo`) is
  cloned over SSH, then HTTPS, with git's prompts off: a private repo works wherever git can
  already clone it. A `SessionStart` or `UserPromptSubmit` command hook's stdout (exit 0) is
  added to the model's context, which is how `plugin/hooks/hooks.json` hands a session its todo.
  `allowed-tools` and `disable-model-invocation` in SKILL.md are Claude Code's own.
- Plugins enabled on claude.ai sync into `<config dir>/plugins/synced/<org>_<account>/` and load
  as `<name>@synced`, though `claude plugin list` and `details` don't show them. One is turned
  off on its own by `"enabledPlugins": {"<name>@synced": false}` in that account's settings.json,
  and all of them by `syncClaudeAiPlugins: false` (2.1.296, 2026-10-13: a headless start's
  `system`/`init` event listed `codex` and its `codex:codex-rescue` agent, and neither with the
  setting). A headless start with `CLAUDE_CONFIG_DIR` set to `~/.claude` looks for other
  credentials than the default does and loads no synced plugin, so check the default account
  with that variable unset. OpenAI's Codex plugin (`openai/codex-plugin-cc`) starts Codex
  threads named "Codex Companion Task: …" with `originator: "Claude Code"`. Each job's
  `sessionId` (the Claude session) and `threadId` are in
  `plugins/data/codex-*/state/<folder>-<hash>/state.json`. Codealong doesn't read it, so those
  threads aren't helpers; the skill steers agents to `codealong start --helper` instead.

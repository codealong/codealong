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
and `description`, optional `references/`, read by all four harnesses), how a prompt runs one
by name, and how `claude plugin` installs Codealong's plugin, moved from CLAUDE.md's Harness
facts (from the harnesses' docs, 2026-09-24, and their code and runs on this Mac). Paths are
under `src-tauri/src/` unless they start with `plugin/`.

- Each harness's personal skills folder: its adapter's `skills_dir` (`claude/mod.rs`,
  `codex/mod.rs`, `opencode.rs`, `antigravity.rs`). Why the skill is copied rather than linked:
  `install.rs` (header).
- Where Claude Code finds the skills and commands a prompt can run by name (`/name args`, as a
  first prompt too), and how often each ran: `claude/skills.rs` (header, `plugin_roots`,
  `uses`), `claude/mod.rs` (`skill_mention`), and the frontmatter `skills.rs` reads (`Skill`:
  `argument-hint`, `user-invocable`, `disable-model-invocation`). Not there (2.1.289): it reads
  a `.claude` folder in the session's folder and those above it, not `.agents/skills`, and
  `skillUsage` has no repo.
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

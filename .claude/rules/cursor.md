---
paths:
  - "src-tauri/src/cursor.rs"
---

# Cursor

What the Cursor app 3.22.7 and its command line, `cursor-agent` 2026.08.11, record and accept,
from the app's bundle (`Contents/Resources/app/out/vs/workbench/workbench.desktop.main.js` and
`workbench.glass.main.js`, the Agents window's), the command line's bundle (its `*.index.js`
chunks) and this Mac's files (2026-10-09). Paths are under `src-tauri/src/`.

- One account, `~/.cursor`, read as the app's chats and the command line's: `cursor.rs`
  (header, `app_db`). The app's data folder is `~/Library/Application Support/Cursor`; the
  command line's config folder is `CURSOR_CONFIG_DIR`, else `$XDG_CONFIG_HOME/cursor`, else
  `~/.cursor`. Neither lists the other's chats: the command line never writes `composerHeaders`,
  and the app never reads `store.db`. They share the transcripts, worktrees
  (`~/.cursor/worktrees/<repo>/<name>`) and plans.
- Every app chat is in the global `User/globalStorage/state.vscdb` (SQLite in WAL mode, ~97 MB
  here); the per-workspace databases have an empty `composerHeaders`. `composerHeaders` is a row
  a chat (`composerId`, `workspaceId`, `createdAt`, `lastUpdatedAt`, `isArchived`, `isSubagent`,
  `recency`, `checkpointAt`, `value` JSON): `cursor.rs` (`read_app_chats`). A new window makes
  empty composers (no name, no `lastUpdatedAt`), and drafts have `isDraft`; subagents' chats are
  `task-…` ids or have `subagentInfo.parentComposerId`. A deleted chat is kept archived with its
  name gone. The folder is `value.workspaceIdentifier.uri.fsPath` (a multi-root workspace has
  `configPath` and no `uri`, an empty window `id: "empty-window"`). The header is written up to
  a minute late (VS Code's flush), and the ItemTable key `composer.composerHeaders.version` is
  bumped with each write.
- `cursorDiskKV`'s `composerData:<id>`: `status` (`none`, `generating`, `completed`,
  `aborted`, the only four the code sets), `fullConversationHeadersOnly` (the messages' order),
  `hasBlockingPendingActions`, `gitWorktree {worktreePath, branchName}`, `modelConfig`,
  `generatingBubbleIds`: `cursor.rs` (`read_state`, `message_ids`). The model is
  `modelConfig.selectedModels[0].modelId`, else `modelConfig.modelName` (`default` is Auto), as
  the app reads it: older chats keep a legacy `modelName` (`claude-4.5-sonnet`) beside a newer
  pick. The effort is that entry's `parameters` (`[{id, value}]`): `reasoning_effort`, `effort` or
  `reasoning` by model (`effort_of`), beside `context`, `fast` and `thinking`; a model left at
  its own levels has none listed. Its
  `lastUpdatedAt` is when you last sent something, not when the reply ended, so a chat's last
  activity is its newest message's time (`Tail::last_at`). It's rewritten with each message and
  every 30 seconds while a turn runs, so its length says whether to read it again (`written`).
  On loading a chat the app rewrites a `generating` left by a crash to `aborted`; until then it
  stays, which is why a generating chat counts only while the app runs and has written lately
  (`board.rs` `cursor_session`, `CURSOR_STALE_MS`).
- Values are declared BLOB but stored as JSON text (the `agentKv` blobs aside), so they're read
  through `CAST(value AS TEXT)`. `LIKE 'bubbleId:<id>:%'` passes over the key's index (210 ms
  here); exact keys and `IN` lists use it. Don't scan `agentKv` (48 MB of protobuf blobs).
- A message, `bubbleId:<chat>:<message>`: `type` 1 yours, 2 the agent's, `text`, `createdAt` as
  an ISO string, `errorDetails {title, message}`, `thinking`; each tool call is a message of its
  own with `toolFormerData` (`name` such as `run_terminal_command_v2`, `edit_file_v2`,
  `read_file_v2`, `ripgrep_raw_search`, `mcp-<server>-<tool>`; `params` and `result` strings of
  JSON; `status` `completed`, `cancelled`, `error` or `loading`; `userDecision`;
  `additionalData`): `cursor.rs` (`bubble`, `call`, `tool_word`). A call that waits to be allowed
  is `loading` with `additionalData.reviewData.status: "Requested"` (and a `blockReason`); a
  question (`ask_question`) waits with `additionalData.status: "pending"`. The app's sidebar says
  "needs attention" for `hasBlockingPendingActions` or a pending plan, else in progress while
  generating. An aborted chat can keep `hasBlockingPendingActions`.
- `hasUnreadMessages` (header) is set when a run ends in a chat that isn't showing, and cleared as
  it's opened; Codealong goes by its own seen marks, as for the other harnesses' apps.
- The transcript both write, `~/.cursor/projects/<slug>/agent-transcripts/<id>/<id>.jsonl`
  (subagents' under `subagents/`): `{role, message: {content: [text, tool_use{name, input}]}}`
  lines and `{"type": "turn_ended", "status": "success|error|aborted"}`, no tool results and no
  times but the one in each prompt's `<timestamp>`, the words in `<user_query>`: `cursor.rs`
  (`line`, `prompt_of`, `prompt_time`, `transcript_tail`). That time is the minute the prompt
  was written in (`1:12 PM` for 13:12:47), so a review's reply is dated by its prompt, from the
  minute it was asked for in (`autoreview::reply_after`; a review read none until 2026-10-20).
  `session wait` takes such a reply only once the session has held still a minute. Its tools are named as Claude Code's
  (`Shell`, `Read`, `StrReplace`…). The app writes it at checkpoints. The slug is the folder's
  path with each run of other characters one `-`, so the folder is found by walking the disk
  (`slug`, `unslug`).
- A turn that ended on an error has its reason on the `turn_ended` line, `error`: `cursor.rs`
  (`turn_error`). Seen here: "[resource_exhausted] Error", Cursor's servers turning a review's
  request away while its plan was at 7% (2026-10-22), and "Agent turn stopped after repeated
  resume attempts made no progress". Sending `continue` rewrote the transcript without that
  line: the reason is in the records only until the turn after it.
- A command line chat is `chats/<md5 of its folder>/<id>/store.db` (`blobs`, sha256-addressed
  protobufs, and `meta`, one row `"0"` whose value is hex for JSON: `name`, `createdAt`,
  `lastUsedModel`, `mode`…, no folder): `cursor.rs` (`read_cli_chats`, `parse_meta`). Beside it
  is `meta.json` (`{schemaVersion, createdAtMs, updatedAtMs, hasConversation, title, cwd,
  isSubagent}`, 2026.08.11 to 2026.10.16), which has the folder and is written as the chat is
  made, before the transcript: `cursor.rs` (`Sidecar`, `parse_sidecar`). Its own list of chats
  leaves out subagents' and ones with no conversation yet. A subagent's chat is in the same
  `chats/<hash>` folder with no `meta.json` (2026.10.16), its `meta` row naming it a subagent's
  (`subagentInfo {parentAgentId, rootParentAgentId}`, name "New Agent"): a review's agent started
  three (2026-10-18), which Codealong leaves out (`Meta::subagent`). Its ACP sessions are in
  `acp-sessions/<id>/` (`store.db` and `meta.json {cwd, title}`); Codealong doesn't run it over
  ACP yet.
- Two folders (2026.08.11's code): `chats` and `cli-config.json` are in
  `CURSOR_CONFIG_DIR`, else `$XDG_CONFIG_HOME/cursor`, else `~/.cursor`; `projects` (transcripts
  and trust) in `CURSOR_DATA_DIR`, else `~/.cursor`: `cursor.rs` (`config_dir_of`, `data_dir`,
  `launch_var`: a start gets it from the account's command or the login shell, not from Codealong).
  An account of its own is `CURSOR_CONFIG_DIR=<folder> cursor-agent` (detect.rs `naming`), so
  its transcripts are with everyone's, where its reader doesn't look for them yet. A folder's
  long path also gets a short records folder of its own for its worker's socket
  (`projects/<slug cut to 84 characters>-<7 hex of its sha256>`), empty once the socket's gone.
- Before it writes its chat the terminal UI asks, each in a box of its own, in a folder it doesn't
  trust "Workspace Trust Required" (`a` trusts, `q` quits), for each folder it works in, one
  `--add-dir` adds too; then, in a trusted one, "MCP Server Approval Required" for servers in the
  workspace's `.cursor/mcp.json` not yet approved. Nothing is written while it waits there:
  `cursor.rs` (`Chat::unwritten`), `board.rs` (`CURSOR_STARTING_MS`). Trust is a
  `.workspace-trusted` file (`{trustedAt, workspacePath}`; `trustMethod` too from `--trust`) in
  the folder's `projects/<slug>` in the data folder, or in that of a folder above that isn't your home or one of the
  top two: `cursor.rs` (`TRUSTED`, `trust_folder`, `Cursor::before_start`). `--trust` trusts
  every folder it works in, in the terminal UI from 2026.08.11 (2026.07.x took it with `-p`
  only). Started behind your windows without either, a review's `cursor-agent` sat at the trust
  box and wrote nothing (2026-10-18; 2026.08.11 and 2026.10.16 alike, in tmux).
- The command line's launcher runs its bundled node as the name it was run by (`exec -a`), so
  `ps` shows that name, `--use-system-ca`, the bundle's `index.js`, and the arguments; the
  version folder's `.running` holds an empty file a pid while one runs, with no chat in it:
  `cursor.rs` (`cli_args`, `held_chat`, `processes`).
- Starting and resuming: `--new-session-id <uuid>` (hidden; a v4 UUID, not with `--resume`),
  `--resume <id>`, `--model`, `--add-dir`, `-w/--worktree [name]` (the name is optional, so it
  goes after the prompt): `cursor.rs` (`Cursor::start_in_terminal`, `Cursor::resume`). It has
  `-p` with `--output-format stream-json`, `--mode plan|ask`, `-f/--yolo`, `--trust`, `ls` and
  `resume` (pickers only), `create-chat`, `models`, and a hidden `acp` command, a full Agent
  Client Protocol server (`loadSession`, `unstable_listSessions`, permission options
  `allow-once`, `allow-always`, `reject-once`).
- A chat's mode and auto-run setting: `cursor.rs` (`Chat::mode`, `Chat::approval`, `AutoRun`,
  `read_auto_run`, `chat_mode`). The app's modes are `agent`, `chat` (Ask), `edit` (Manual),
  `plan`, `spec`, `debug`, `multitask`, `triage`, `project` and `background` (read as `agent`
  on load), and ones you make; `forceMode`, `isAgentic` and each message's `unifiedMode` number
  are kept in step with it and say nothing more. The agent's `switch_mode` tool changes it, and
  so does building a plan. A chat's `agentApprovalModeOverride` is written only as
  `unrestricted`; a subagent's chat takes its parent's. `approvalMode` (`manual`, `allowlist`,
  `unrestricted`) in `~/.cursor/permissions.json` wins over both (`enforced_approval`); a
  project's `.cursor/permissions.json` can set one too, which isn't read, its precedence
  unchecked. With `cursor/sandboxAutoRunSettingEnabled`
  on, the auto-run names say "(with Sandbox)". The command line's `approvalMode` is written by
  `-f/--force/--yolo` (`unrestricted`, after the screen first draws; with `-p` only
  `isRunEverything`), `--auto-review`, `/run-everything` and `/auto-review`;
  `~/.cursor/cli-config.json` holds the defaults (3.22.7's and 2026.08.11's code, 2026-10-13).
- The terminal UI's permission prompt takes `y`, `n` or Escape (skip and say what to do
  instead), `p`, Tab (allow from now on), Shift+Tab (run everything) and arrows with Return; a
  call waiting there isn't written anywhere, so Codealong shows no prompt for the command line's
  chats.
- Sign-in: the app keeps it in its ItemTable (`cursorAuth/accessToken`, `cursorAuth/cachedEmail`,
  `cursorAuth/stripeMembershipType`…); the command line in the keychain (account `cursor-user`,
  services `cursor-access-token`, `cursor-refresh-token`) or, with
  `AGENT_CLI_CREDENTIAL_STORE=file`, `~/.cursor/auth.json`: `cursor.rs` (`Cursor::signed_in`,
  which reads the email only). The command line's keychain item is written by a keychain
  module of its own, so the usage read every couple of minutes uses only the app's sign-in or
  the file (`saved_token`).
- Usage: `GetCurrentPeriodUsage` on the DashboardService with `{}`, as the app's
  `UsageDataService` asks it, and `GetPlanInfo` for the plan's name: `cursor.rs` (Plan usage
  section, `read_usage`, `usage_meters`). Spends are cents, percents out of 100, times strings of
  epoch milliseconds. How Settings shows them is the app's settings' (`usage_display`, 3.22.7's
  `usageDisplay`): a plan that spills over (`hasTieredSelfServeTeamSpillover`, or
  `hasAutoSpillover` other than Enterprise's) as two bars, Cursor Models (Auto, Composer,
  Cursor Grok; `autoPercentUsed`) and Other Models (`apiPercentUsed`), its chat box's line
  saying "Cursor Models: N% used · Other Models: N% used" with 0 < N < 1 shown as 1; Express
  (`membershipType` `express`) the first alone; any other as "Total N% used"
  (`totalPercentUsed`) with the two shares beneath it, though the answer has them all whatever
  the plan. `displayMessage` ("You've used 2% of your included usage") goes by spend. The titles can come from Cursor's server config (`auto_spillover_ui_config`); older
  ones were "Auto + Composer" and "API". On 2026-10-14 a Pro+ account answered spend 138 of
  7,000 cents, auto 0.06%, API 0.62%, total 0.11%.
- Privacy Mode (whether Cursor trains on the account) is the account's on Cursor's servers,
  read and set with the app's token: `cursor.rs` (Privacy Mode section). The enum is
  `aiserver.v1.PrivacyMode`: 0 unspecified, 1 `NO_STORAGE` (Legacy, offered only to accounts
  already on it), 2 `NO_TRAINING` (Privacy Mode), 3 `USAGE_DATA_TRAINING_ALLOWED` and 4
  `USAGE_CODEBASE_TRAINING_ALLOWED` (Share Data). The answer can also carry
  `hoursRemainingInGracePeriod`, `isEnforcedByTeam` and `partnerDataShare` (prompts shared with
  a model's provider when you pick its model), and an `accountPrivacyMode` the app's code doesn't
  define. The app sends `x-ghost-mode` on every request by it; the command line caches it in
  `cli-config.json` as `privacyCache` and picks private or other agent servers by it. The app
  goes by its teams' most private setting, and for Enterprise hides the picker.
- Links: `cursor://anysphere.cursor-deeplink/…` (the built-in `cursor-deeplink` extension).
  `/prompt?text=&workspace=<folder name>&mode=` fills in a new chat after a confirmation dialog
  and doesn't send it, in the Agents window too; `/agent?id=<composer id>` opens the Agents
  window on that chat (`glass.openCloudAgentById`, which looks the id up among the chats the
  window lists, local ones included, and hydrates only `bc-` cloud ids; tried on a local chat
  2026-10-14). So an app chat opens there (`cursor.rs` `chat_link`, `open_chat`) and a prompt
  for it goes on the clipboard (`OnClipboard`). The extension activates `onUri`, so the link
  opens Cursor when it isn't running.
- The app runs Claude Code's hooks from `~/.claude/settings.json` and a project's
  `.claude/settings*.json` too, on by default (`UserPromptSubmit`, `Stop`, `PreToolUse`…, not
  `Notification` or `PermissionRequest`), with Cursor's payload; its own are in
  `~/.cursor/hooks.json`, every payload with `conversation_id` (the composer id) and
  `transcript_path`.
- Skills, which both read from `~/.cursor/skills`, the shared `.agents/skills` and Claude Code's
  and Codex's folders, and which the command line runs from a prompt's `/name`: `skills.md`.
- Models the app offers: ItemTable
  `src.vs.platform.reactivestorage.browser.reactiveStorageServiceImpl.persistentStorage.applicationUser`
  → `availableDefaultModels2` (half a megabyte: read with `json_extract`), each with `name`, its
  id, and `clientDisplayName`, which the picker shows and a session says (`model_names`); the
  default is `aiSettings.modelConfig.composer.modelName`. Each has `defaultOn`, and the picker
  shows one on by default or in `aiSettings.modelOverrideEnabled` and not in
  `modelOverrideDisabled` (3.22.7's `autoEnableSelectedModels`; `offered_models`). A start and a
  resume offer those (`start_models`, `Cursor::models`), `default` as the command line's `auto`.
- The command line's models (2026.08.11): `cursor-agent models` lists every model at every
  setting as an id of its own (`grok-4.7-high-fast`, about 260 here), and says "use --model <id>
  (or /model <id> in interactive mode) to switch. Parameterized models also accept quoted
  overrides, e.g. --model 'claude-opus-4-8[context=1m,effort=high,fast=false]'". The app's
  `name`s are the parameterized models' ids, and `--model <name>` takes one on its default
  settings (`handleInitialModel`); an id it doesn't know stops it ("Cannot use this model: …
  Available models: …"). Both `--model` and `/model` save the pick as its default
  (`cli-config.json` `model`, `selectedModel`, `hasChangedDefaultModel: true`), so a start
  without one goes on the last one asked for, and Codealong types no `/model` into a running
  chat. Signed out it says "Authentication required" for `models`; it takes
  `CURSOR_AUTH_TOKEN` (the app's token worked), which Codealong doesn't hand it, since a
  session's terminal would pass it to every command the agent runs. `cli-config.json` has
  `hasChangedDefaultModel` and `modelSlashCommands` (true here) but no model of its own. A
  model's effort isn't asked for: the parameter differs by model (`effort_of`), and a start's
  effort is one list an account.

## Unchecked

- Typing into the command line's terminal and Escape weren't tried. Its chats, transcripts,
  processes' arguments and `--new-session-id` were seen on real chats on 2026-10-18 (2026.08.11
  and 2026.10.16, signed in).
- A chat generating or waiting on a call while Codealong read it: no chat ran during the work,
  so `generating`, `hasBlockingPendingActions` and `reviewData.status: "Requested"` are as the
  code and old chats have them.
- `/agent?id=` with Cursor not running, and on a chat the Agents window doesn't list (an
  archived one, say), which shows "We can't find this agent, or it doesn't exist on this
  machine." in Cursor: only a listed chat, with the window open, was tried.
- Usage on a plan without spillover, and with an on-demand limit set: read from the app's code
  only (`usage_meters`).
- A start or a resume with `--model <name>` (and `auto`): read from the command line's code and
  its `models` text.
- MCP servers: `~/.cursor/mcp.json` (`mcpServers`), the app's and the command line's; a workspace's
  `.cursor/mcp.json` beside it asks approval first. One turned off in Cursor's settings stays in
  the file: `cursor.rs` (`Cursor::mcp`). The file was empty (0 bytes) here, 2026-10-20.

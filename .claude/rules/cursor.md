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
  `hasBlockingPendingActions`, `gitWorktree {worktreePath, branchName}`, `modelConfig.modelName`
  (`default` is Auto), `generatingBubbleIds`: `cursor.rs` (`read_state`, `message_ids`). Its
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
  (`line`, `prompt_of`, `prompt_time`, `transcript_tail`). Its tools are named as Claude Code's
  (`Shell`, `Read`, `StrReplace`…). The app writes it at checkpoints. The slug is the folder's
  path with each run of other characters one `-`, so the folder is found by walking the disk
  (`slug`, `unslug`).
- A command line chat is `chats/<md5 of its folder>/<id>/store.db` (`blobs`, sha256-addressed
  protobufs, and `meta`, one row `"0"` whose value is hex for JSON: `name`, `createdAt`,
  `lastUsedModel`, `mode`…, no folder): `cursor.rs` (`read_cli_chats`, `parse_meta`). Its ACP
  sessions are in `acp-sessions/<id>/` (`store.db` and `meta.json {cwd, title}`); Codealong
  doesn't run it over ACP yet.
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
  `unrestricted`; a subagent's chat takes its parent's. With `cursor/sandboxAutoRunSettingEnabled`
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
  which reads the email only). Usage is asked of Cursor's servers with the token; nothing local
  says it (`Cursor::no_limits`).
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
  and doesn't send it; `/agent?id=` opens the Agents window and asks `glass.openCloudAgentById`,
  which wasn't shown to open a local chat. So an app chat opens as its folder's window
  (`open -a Cursor <folder>`, `cursor.rs` `open_app`) and a prompt for it goes on the clipboard
  (`OnClipboard`).
- The app runs Claude Code's hooks from `~/.claude/settings.json` and a project's
  `.claude/settings*.json` too, on by default (`UserPromptSubmit`, `Stop`, `PreToolUse`…, not
  `Notification` or `PermissionRequest`), with Cursor's payload; its own are in
  `~/.cursor/hooks.json`, every payload with `conversation_id` (the composer id) and
  `transcript_path`.
- Models the app offers: ItemTable
  `src.vs.platform.reactivestorage.browser.reactiveStorageServiceImpl.persistentStorage.applicationUser`
  → `availableDefaultModels2` (half a megabyte: read with `json_extract`); the default is
  `aiSettings.modelConfig.composer.modelName`. Whether the command line's `--model` takes the
  same names wasn't checked, so a start takes no model from Codealong yet.

## Unchecked

- Everything of the command line's on a real chat: it isn't signed in on this Mac, so its
  `store.db`, its transcripts, its processes' arguments and `--new-session-id` were read in its
  code only. Typing into its terminal and Escape weren't tried.
- A chat generating or waiting on a call while Codealong read it: no chat ran during the work,
  so `generating`, `hasBlockingPendingActions` and `reviewData.status: "Requested"` are as the
  code and old chats have them.
- Whether `/agent?id=<composer id>` opens a local chat; it would show an error notification in
  Cursor when it doesn't.

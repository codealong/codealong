// The engine lives in Rust; this is the only place the UI talks to it. Outside Tauri (a
// browser preview of a snapshot), the board comes from ./board.json and actions do nothing.

import { invoke } from '@tauri-apps/api/core';
import { emit, emitTo, listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import type { AccountCfg, AccountInfo, AgentsStatus, Answers, Attachment, BarFeed, BarNote, Board, CloseOutcome, Config, ConfigView, DesktopFeed, DesktopPinned, DeviceFound, DevicesView, EditorIcon, FoundAccount, HostedStream, IdlePlan, LabelColor, Lane, LineageFeed, Notice, PeekOpen, PeekShown, Pending, PrAt, PrDetail, PrItem, Route, Sent, Session, ShortcutStatus, Side, SideKept, SkillsListed, Started, StartOptions, StartSpec, TextFound, Theme, TodoSuggested, TranscriptPage, UpdateView, Verdict, Way } from './types';
import type { TodoOp } from './todos';

export const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export async function getBoard(): Promise<Board> {
  if (inTauri) return invoke<Board>('get_board');
  const res = await fetch('./board.json');
  if (!res.ok) throw new Error('No board.json next to the page. Run the app with `npm run tauri dev`.');
  return (await res.json()) as Board;
}

/** A timing line for ~/Library/Logs/Codealong/perf.log, while `[debug] perf_log` is on. */
export function perfMark(line: string): void {
  if (inTauri) void invoke('perf_mark', { line });
}

/** Hands the page its focus again, after a key typed into a text field while WebKit had the page
 *  as unfocused, which hides the text cursor (caret.ts). */
export async function pageUnfocused(): Promise<void> {
  if (inTauri) await invoke('page_unfocused');
}

/** Pops up a menu made with `Menu.new` at `at` (logical window coordinates) or the pointer, and
 *  resolves when it has closed. */
export async function popupMenu(rid: number, at?: { x: number; y: number }): Promise<void> {
  await invoke('popup_menu', { rid, at: at ? { Logical: at } : null });
}

/** Calls `cb` with every board the engine publishes. Returns an unsubscribe function. */
export function onBoard(cb: (board: Board) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void listen<Board>('board', (e) => cb(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** The installed editors' app icons (editors.rs), drawn once a run. */
export async function editorIcons(): Promise<EditorIcon[]> {
  if (!inTauri) return [];
  return invoke<EditorIcon[]>('editor_icons');
}

export async function openTarget(laneId: string, targetId: string): Promise<void> {
  if (!inTauri) return;
  await invoke('open_target', { laneId, targetId });
}

export async function openConfig(): Promise<void> {
  if (!inTauri) return;
  await invoke('open_config');
}

/** Marks sessions seen. Returns the time of the board that shows it, or null when the engine
 *  didn't say (sessions on other devices). Windows mark seen through ahead.ts, which shows it at once. */
export async function markSeen(keys: string[]): Promise<number | null> {
  if (!inTauri || keys.length === 0) return null;
  return (await invoke<number | null>('mark_seen', { keys })) ?? null;
}

/** Snoozes sessions until `until` (ms since the epoch), or wakes them now with null. Returns the
 *  time of the board that shows it, as `markSeen` does; windows snooze through ahead.ts. */
export async function snoozeSessions(keys: string[], until: number | null): Promise<number | null> {
  if (!inTauri || keys.length === 0) return null;
  return (await invoke<number | null>('snooze_sessions', { keys, until })) ?? null;
}

/** Snoozes your review of PRs (`owner/name#number`) until `until`, or wakes them now with null.
 *  Returns the time of the board that shows it, as `markSeen` does; windows snooze through
 *  ahead.ts. */
export async function snoozeReviews(keys: string[], until: number | null): Promise<number | null> {
  if (!inTauri || keys.length === 0) return null;
  return (await invoke<number | null>('snooze_reviews', { keys, until })) ?? null;
}

export async function refresh(): Promise<void> {
  if (!inTauri) return;
  await invoke('refresh');
}

/** A PR's description, reviews and changed files. */
export async function prDetail(repo: string, number: number): Promise<PrDetail> {
  if (!inTauri) throw new Error('PR details need the app.');
  return invoke<PrDetail>('pr_detail', { repo, number });
}

/** A PR the board doesn't list, for a link to it: what GitHub says of it, with the worktrees and
 *  sessions here that worked on it. */
export async function prCard(repo: string, number: number): Promise<PrItem> {
  if (!inTauri) throw new Error('PR details need the app.');
  return invoke<PrItem>('pr_card', { repo, number });
}

/** Which of a PR's comments ask for a change, by comment id, from Jev. Empty while that's off. */
export async function prCommentAsks(title: string, comments: { id: string; author: string; body: string; path: string | null }[]): Promise<Record<string, number>> {
  if (!inTauri || comments.length === 0) return {};
  return (await invoke<Record<string, number> | null>('pr_comment_asks', { title, comments })) ?? {};
}

/** Reviews a PR as you: approves it, comments on it or asks for changes, with `body` as the
 *  review's text (a comment or a request for changes needs some). */
export async function reviewPr(repo: string, number: number, verdict: Verdict, body: string): Promise<void> {
  if (!inTauri) throw new Error('Reviewing needs the app.');
  await invoke('review_pr', { repo, number, verdict, body });
}

export async function reviewInCodex(url: string): Promise<void> {
  if (!inTauri) return;
  await invoke('review_in_codex', { url });
}

/** Opens a web link in the browser. */
export async function openUrl(url: string): Promise<void> {
  if (!inTauri) {
    window.open(url, '_blank', 'noopener');
    return;
  }
  await invoke('open_url', { url });
}

/** Types a block of shell commands from a session's conversation into a new terminal in `cwd`
 *  (runcode.rs): what it runs leaves out the comments. */
export async function runInTerminal(cwd: string, text: string): Promise<void> {
  if (!inTauri) throw new Error('Running a command needs the app.');
  await invoke('run_in_terminal', { cwd, text });
}

/** Opens a project in a window of its own, or brings that window forward; with `another`
 *  (File › New window), always opens one more. */
export async function openProjectWindow(project: string, another = false): Promise<void> {
  if (!inTauri) return;
  await invoke('open_project_window', { project, another });
}

/** Calls `cb` when File › New window asks this window for another one like it. */
export function onNewWindow(cb: () => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void getCurrentWebviewWindow()
    .listen('new-window', () => cb())
    .then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** Calls `cb` when File › Close idle terminals… asks this window to show its dialog. */
export function onCloseIdle(cb: () => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void getCurrentWebviewWindow()
    .listen('close-idle', () => cb())
    .then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** Which terminals have sat idle for `afterMs` with nothing asked of you, and which stay open. */
export async function idleTerminals(afterMs: number): Promise<IdlePlan> {
  if (!inTauri) return { afterMs, close: [], kept: [] };
  return invoke<IdlePlan>('idle_terminals', { afterMs });
}

/** Closes these sessions' terminals, each checked again first. */
export async function closeIdleTerminals(keys: string[], afterMs: number): Promise<CloseOutcome[]> {
  if (!inTauri) return [];
  return invoke<CloseOutcome[]>('close_idle_terminals', { keys, afterMs });
}

/** Closes one session: its terminal tab, or a background job stopped. Says what happened. */
export async function closeSession(key: string): Promise<string> {
  if (!inTauri) return '';
  return invoke<string>('close_session', { key });
}

/** Archives or unarchives a session; Codex threads are archived in Codex itself. Returns the
 *  time of the board that shows it, as `markSeen` does. */
export async function archiveSession(key: string, archived: boolean): Promise<number | null> {
  if (!inTauri) return null;
  return (await invoke<number | null>('archive_session', { key, archived })) ?? null;
}

/** Names a session where its harness keeps its name, so the harness shows it too. Returns the
 *  time of the board that shows it, as `markSeen` does. */
export async function renameSession(key: string, name: string): Promise<number | null> {
  if (!inTauri) return null;
  return (await invoke<number | null>('rename_session', { key, name })) ?? null;
}

/** Tells every window about a change shown ahead of the engine (ahead.ts). */
export function emitAhead<T>(event: T): void {
  if (inTauri) void emit('ahead', event);
}

/** Calls `cb` with what other windows say about their changes shown ahead of the engine. */
export function onAhead<T>(cb: (event: T) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void listen<T>('ahead', (e) => cb(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** A notice from a window that doesn't draw them (the bar, the quick panel, the peek…), or its
 *  end, for the windows that do (notices.ts). */
export type WindowNotice = { notice: Notice } | { clear: string };

/** Hands a notice, or its end, to the windows that draw them. */
export function emitNotice(m: WindowNotice): void {
  if (inTauri) void emit('window-notice', m);
}

/** Calls `cb` with the notices other windows hand on, and their ends. */
export function onWindowNotice(cb: (m: WindowNotice) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void listen<WindowNotice>('window-notice', (e) => cb(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** Saves the whole pinned list, in order. */
export async function setPinned(keys: string[]): Promise<void> {
  if (!inTauri) return;
  await invoke('set_pinned', { keys });
}

/** Saves the whole list of pinned worktrees (lane ids), in order. */
/** Sets a worktree's preview URL and the Chrome profile it opens in over its project's (null
 *  for the project's), or clears them with a null URL. Resolves with the time of the board that
 *  shows it, or null when the engine didn't say in time. */
export async function setPreview(laneId: string, url: string | null, port: number | null, chromeProfile: string | null): Promise<number | null> {
  if (!inTauri) return null;
  return invoke<number | null>('set_preview', { laneId, url, port, chromeProfile });
}

/** Marks a worktree reusable or not (reusable.rs). Resolves with the time of the board that
 *  shows it, or null when the engine didn't say in time. */
export async function setReusable(laneId: string, on: boolean): Promise<number | null> {
  if (!inTauri) return null;
  return invoke<number | null>('set_reusable', { laneId, on });
}

export async function setPinnedWorktrees(ids: string[]): Promise<void> {
  if (!inTauri) return;
  await invoke('set_pinned_worktrees', { ids });
}

/** Saves the whole list of pinned PRs (`owner/name#number`), in order. */
export async function setPinnedPrs(keys: string[]): Promise<void> {
  if (!inTauri) return;
  await invoke('set_pinned_prs', { keys });
}

export async function refreshUsage(): Promise<void> {
  if (!inTauri) return;
  await invoke('refresh_usage');
}

export async function getAccounts(): Promise<AccountInfo[]> {
  if (inTauri) return invoke<AccountInfo[]>('get_accounts');
  const res = await fetch('./accounts.json');
  return res.ok ? ((await res.json()) as AccountInfo[]) : [];
}

/** Stars an account, or takes its star away: Auto starts preferred accounts first. */
export async function setPreferred(account: string, preferred: boolean): Promise<void> {
  if (!inTauri) throw new Error('Starring an account needs the app.');
  await invoke('set_preferred', { account, preferred });
}

export async function saveAccounts(accounts: AccountCfg[]): Promise<void> {
  if (!inTauri) throw new Error('Saving accounts needs the app.');
  await invoke('save_accounts', { accounts });
}

/** Accounts on this Mac that config.toml doesn't list. */
export async function foundAccounts(): Promise<FoundAccount[]> {
  if (inTauri) return invoke<FoundAccount[]>('found_accounts');
  const res = await fetch('./found.json');
  return res.ok ? ((await res.json()) as FoundAccount[]) : [];
}

/** The settings as parsed and config.toml as written. */
export async function getConfig(): Promise<ConfigView> {
  if (inTauri) return invoke<ConfigView>('get_config');
  const res = await fetch('./config.json');
  if (!res.ok) throw new Error('No config.json next to the page.');
  return (await res.json()) as ConfigView;
}

/** Saves every setting, in place in config.toml. */
export async function saveConfig(config: Config): Promise<void> {
  if (!inTauri) throw new Error('Saving settings needs the app.');
  await invoke('save_config', { config });
}

/** Saves config.toml as typed, once it parses. */
export async function saveConfigText(text: string): Promise<void> {
  if (!inTauri) throw new Error('Saving settings needs the app.');
  await invoke('save_config_text', { text });
}

/** Where automatic updates stand; `null` outside the app. */
export async function updateStatus(): Promise<UpdateView | null> {
  if (!inTauri) return null;
  return invoke<UpdateView>('update_status');
}

/** Looks for a newer release now; `onUpdateStatus` hears how it goes. */
export async function checkForUpdates(): Promise<void> {
  if (inTauri) await invoke('check_for_updates');
}

/** Quits and opens the version an update put in place. */
export async function restartToUpdate(): Promise<void> {
  if (inTauri) await invoke('restart_to_update');
}

/** Calls `cb` whenever where updates stand changes. */
export function onUpdateStatus(cb: (view: UpdateView) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void listen<UpdateView>('update-status', (e) => cb(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** Calls `cb` when the app menu's Check for updates… asks this window to show how it goes. */
export function onCheckUpdates(cb: () => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void getCurrentWebviewWindow()
    .listen('check-updates', () => cb())
    .then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** Calls `cb` when the app menu's Settings… asks this window to open Settings. */
export function onSettings(cb: () => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void getCurrentWebviewWindow()
    .listen('settings', () => cb())
    .then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** Opens a terminal window that signs the account in. */
export async function signIn(name: string): Promise<void> {
  if (!inTauri) return;
  await invoke('sign_in', { name });
}

/** The `codealong` command link and the agent skill per account, for Settings › Agents. */
export async function agentsStatus(): Promise<AgentsStatus> {
  if (!inTauri) throw new Error('Reading the agent tools needs the app.');
  return invoke<AgentsStatus>('agents_status');
}

/** Links the command and installs the skill for the named accounts; returns the new state. */
export async function installAgents(command: boolean, accounts: string[]): Promise<AgentsStatus> {
  if (!inTauri) throw new Error('Installing the agent tools needs the app.');
  return invoke<AgentsStatus>('install_agents', { command, accounts });
}

/** Where each account can start a session, for the new-session sheet. */
export async function startOptions(): Promise<StartOptions> {
  if (inTauri) return invoke<StartOptions>('start_options');
  const res = await fetch('./start-options.json');
  return res.ok ? ((await res.json()) as StartOptions) : { accounts: [], codexProjects: [], claudeModels: [] };
}

/** Where each account on another of your devices can start a session, asked of that device. */
export async function deviceStartOptions(device: string): Promise<StartOptions> {
  if (inTauri) return invoke<StartOptions>('device_start_options', { device });
  return startOptions();
}

/** Starts a session in a worktree: in the terminal, or in the Claude or Codex app. */
export async function startSession(spec: StartSpec): Promise<Started | null> {
  if (!inTauri) throw new Error('Starting a session needs the app.');
  return invoke<Started | null>('start_session', { spec });
}

/** Forks a session: the copy goes on from what it had written. Into a new terminal in its
 *  folder, or, `background`, where no window comes forward (a background job, or a session a
 *  service or Codealong runs), on `prompt` when there's one. */
export async function forkSession(key: string, { background = false, prompt = '' }: { background?: boolean; prompt?: string } = {}): Promise<Started | null> {
  if (!inTauri) throw new Error('Forking a session needs the app.');
  return invoke<Started | null>('fork_session', { key, background, prompt });
}

/** Moves a session to `account`, another account of its harness (start.rs `move_to`): a copy goes
 *  on in a new terminal under that account, and the session is closed and archived. */
export async function moveSession(key: string, account: string): Promise<Started | null> {
  if (!inTauri) throw new Error('Moving a session needs the app.');
  return invoke<Started | null>('move_session', { key, account });
}

/** Moves a session to the worktree `laneId` (start.rs `move_to_folder`), or to a new chat when
 *  that's Codealong's chats: it's closed where it runs and goes on there as itself. */
export async function moveSessionTo(key: string, laneId: string): Promise<Started | null> {
  if (!inTauri) throw new Error('Moving a session needs the app.');
  return invoke<Started | null>('move_session', { key, lane: laneId });
}

/** Asks something about a session on the side (side.rs): a throwaway copy of its conversation
 *  answers, and the session carries on, unaware. Resolves with the side conversation, the
 *  question in it; the answer comes through `onSide`. */
export async function sideAsk(key: string, text: string): Promise<Side> {
  if (!inTauri) throw new Error('A side conversation needs the app.');
  return invoke<Side>('side_ask', { key, text });
}

/** A session's side conversation, if it has one. */
export async function sideGet(key: string): Promise<Side | null> {
  if (!inTauri) return null;
  return invoke<Side | null>('side_get', { key });
}

/** The side conversations kept for a session, the latest first: the open one too, as far as it's
 *  been answered. */
export async function sidePast(key: string): Promise<SideKept[]> {
  if (!inTauri) return [];
  return invoke<SideKept[]>('side_past', { key });
}

/** Forgets a kept side conversation. */
export async function sideForget(id: string): Promise<void> {
  if (!inTauri) return;
  await invoke('side_forget', { id });
}

/** Ends a session's side conversation: its copy stops, and what was said is kept to read. */
export async function sideClose(key: string): Promise<void> {
  if (!inTauri) return;
  await invoke('side_close', { key });
}

/** Calls `cb` as a side conversation changes: a question, more of its answer, its end (one with
 *  no turns). */
export function onSide(cb: (side: Side) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void listen<Side>('side', (e) => cb(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** A change to a todo or the labels. Resolves with the time of the first board that shows it. */
/** Where what's typed in the quick panel should go, from Jev. */
export async function routeQuick(project: string, text: string): Promise<Route> {
  if (!inTauri) throw new Error('Routing needs the app.');
  return invoke<Route>('route_quick', { project, text });
}

/** The skills a session in each of `folders` could be asked to run by name, and each of
 *  `accounts`' own (skills.rs). This Mac's folders and accounts only. */
export async function listSkills(folders: string[], accounts: string[]): Promise<SkillsListed> {
  return invoke<SkillsListed>('skills', { folders, accounts });
}

/** Sessions whose prompts and replies have every word (search.rs). */
export async function searchText(words: string[]): Promise<TextFound> {
  if (!inTauri) return { hits: [], building: false, off: true };
  return invoke<TextFound>('search_text', { words });
}

/** Sends a prompt to a session that's already going, where it runs (send.rs); `now` sends it
 *  into the turn it's taking at once, where its harness can be reached that way. */
export async function sendToSession(key: string, text: string, now = false): Promise<Sent> {
  if (!inTauri) return { message: 'Sent.', opened: false };
  return invoke<Sent>('send_to_session', { key, text, now });
}

/** Whether there's a TypeSafe API key in the keychain. */
export async function jevKey(): Promise<boolean> {
  if (!inTauri) return false;
  return (await invoke<boolean | null>('jev_key')) ?? false;
}

/** Saves the TypeSafe API key in the keychain; an empty one removes it. */
export async function setJevKey(key: string): Promise<void> {
  if (!inTauri) return;
  await invoke('set_jev_key', { key });
}

/** One small call to TypeSafe: which model answered and how fast. */
export async function testJev(model: string): Promise<string> {
  if (!inTauri) throw new Error('Testing needs the app.');
  return invoke<string>('test_jev', { model });
}

/** What Jev suggests for a todo being written: its project, labels and what it waits on. */
export async function suggestTodo(id: string | null, title: string, description: string, project: string): Promise<TodoSuggested> {
  if (!inTauri) throw new Error('Suggestions need the app.');
  return invoke<TodoSuggested>('suggest_todo', { id, title, description, project });
}

/** Says a todo doesn't look done after all, until something linked to it moves on. */
export async function dismissTodoHint(id: string): Promise<void> {
  if (!inTauri) return;
  await invoke('dismiss_todo_hint', { id });
}

export async function todoOp(op: TodoOp): Promise<number> {
  if (!inTauri) return 0;
  return invoke<number>('todo_op', { op });
}

/** Keeps a pasted or picked file and says how text refers to it. The bytes go as they are,
 *  not as JSON, and the name in a header. */
export async function saveAttachment(file: Blob, name: string): Promise<Attachment> {
  if (!inTauri) throw new Error('Attachments need the app.');
  return invoke<Attachment>('save_attachment', new Uint8Array(await file.arrayBuffer()), { headers: { 'x-name': encodeURIComponent(name) } });
}

/** Keeps a copy of a file dropped from the Finder. */
export async function importAttachment(path: string): Promise<Attachment> {
  if (!inTauri) throw new Error('Attachments need the app.');
  return invoke<Attachment>('import_attachment', { path });
}

/** The text with each attachment as its full path, for pasting into an agent. */
export async function expandAttachments(text: string): Promise<string> {
  if (!inTauri) return text;
  return invoke<string>('expand_attachments', { text });
}

/** Opens a kept attachment in the app that opens its kind (Preview for an image, say). */
export async function openAttachment(path: string): Promise<void> {
  if (!inTauri) return;
  await invoke('open_attachment', { path });
}

/** A browser preview of the UI (scripts/preview.sh), where Tauri's IPC is a stand-in. */
export const inPreview = typeof window !== 'undefined' && '__CODEALONG_PREVIEW__' in window;

/** Where a window loads a kept image from. A preview serves them from ./images. */
export function imageUrl(name: string): string {
  return inTauri && !inPreview ? `codealong-image://localhost/${name}` : `./images/${name}`;
}

/** Calls `cb` with the paths of files dropped on this window and where they landed. */
export function onFileDrop(cb: (paths: string[], at: { x: number; y: number }) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void getCurrentWebviewWindow()
    .onDragDropEvent((e) => {
      if (e.payload.type !== 'drop') return;
      // Tauri reports physical pixels; the page works in CSS pixels.
      const scale = window.devicePixelRatio || 1;
      cb(e.payload.paths, { x: e.payload.position.x / scale, y: e.payload.position.y / scale });
    })
    .then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** Saves the theme to config.toml. Every window follows. */
export async function setTheme(theme: Theme): Promise<void> {
  if (!inTauri) return;
  await invoke('set_theme', { theme });
}

/** What the floating bar, the quick panel or a card on the desktop asks the main window to show. */
export type Reveal = { kind: 'session'; laneId: string; key: string } | { kind: 'worktree'; laneId: string } | { kind: 'pr'; key: string } | { kind: 'todo'; id: string };

/** Brings the main window forward, showing a session, worktree, PR or todo if given. */
export async function showMain(reveal: Reveal | null = null): Promise<void> {
  if (!inTauri) return;
  await invoke('show_main', { reveal });
}

/** Calls `cb` when the floating bar asks this window to show something. Only events sent to
 *  this window count: a plain listener would hear ones sent to any window. */
export function onReveal(cb: (r: Reveal) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void getCurrentWebviewWindow()
    .listen<Reveal>('reveal', (e) => cb(e.payload))
    .then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** The floating bar measured itself; its window takes that size. */
export async function fitBar(width: number, height: number): Promise<void> {
  if (!inTauri) return;
  await invoke('fit_bar', { width, height });
}

/** Opens a session from the floating bar, which also asks macOS to bring its app forward. */
export async function openFromBar(laneId: string, targetId: string): Promise<void> {
  if (!inTauri) return;
  await invoke('open_from_bar', { laneId, targetId });
}

/** A PR the board lists, with the worktrees that hold it or that its sessions are listed under,
 *  and the apps, for a window without the board to show its card (PrCard.tsx). Null once the
 *  board no longer lists it. */
export async function prAt(key: string): Promise<PrAt | null> {
  if (!inTauri) return null;
  return (await invoke<PrAt | null>('pr_at', { key })) ?? null;
}

/** A session with its worktree and the apps, for a window without the board to build the
 *  session's menu (menus.ts). Null once the board no longer has it. */
export async function sessionAt(key: string): Promise<{ lane: Lane; session: Session; apps: Board['apps'] } | null> {
  if (!inTauri) return null;
  return (await invoke<{ lane: Lane; session: Session; apps: Board['apps'] } | null>('session_at', { key })) ?? null;
}

/** What the floating bar shows. Outside the app it comes from ./bar-feed.json. */
export async function getBarFeed(): Promise<BarFeed> {
  if (inTauri) return invoke<BarFeed>('bar_feed');
  const res = await fetch('./bar-feed.json');
  if (!res.ok) throw new Error('No bar-feed.json next to the page.');
  return (await res.json()) as BarFeed;
}

/** Calls `cb` with each new bar feed; the engine sends one only when it changed. */
export function onBarFeed(cb: (feed: BarFeed) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void listen<BarFeed>('bar-feed', (e) => cb(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/* ---- Cards pinned to the desktop (src-tauri/src/desktop.rs) ---- */

/** What's pinned to the desktop, in the order it was pinned. */
export async function desktopPins(): Promise<DesktopPinned[]> {
  if (!inTauri) return [];
  return invoke<DesktopPinned[]>('desktop_pins');
}

/** Calls `cb` with what's pinned whenever a card is pinned or taken off, from any window. */
export function onDesktopPins(cb: (pins: DesktopPinned[]) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void listen<DesktopPinned[]>('desktop-pins', (e) => cb(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** Pins a session, worktree or todo to the desktop, or brings its card up if it's there. */
export async function pinToDesktop(pin: DesktopPinned): Promise<void> {
  if (!inTauri) return;
  await invoke('pin_to_desktop', { pin });
}

/** A thing is being dragged in this window (a pill on the bar, a row in a list, a result in the
 *  quick panel): once the pointer leaves the window its card follows it onto the desktop, until
 *  the button comes up (desktop.rs `drag_out`). */
export async function dragToDesktop(pin: DesktopPinned): Promise<void> {
  if (!inTauri) return;
  await invoke('drag_to_desktop', { pin });
}

/** Calls `cb` when the pointer of a drag this window started leaves the window (true) or comes
 *  back over it: a page may hear nothing of a pointer outside its window. */
export function onDesktopDragAway(cb: (away: boolean) => void): () => void {
  return onWindowEvent<boolean>('desktop-drag-away', cb);
}

/** Calls `cb` when a drag this window started is over: `kept` when its card stayed on the
 *  desktop, not when the button came up back over this window. */
export function onDesktopDragEnd(cb: (kept: boolean) => void): () => void {
  return onWindowEvent<{ kept: boolean }>('desktop-drag-end', (e) => cb(e.kept));
}

/** Takes a card off the desktop. */
export async function unpinFromDesktop(pin: DesktopPinned): Promise<void> {
  if (!inTauri) return;
  await invoke('unpin_from_desktop', { pin });
}

/** What this window's card shows; null once it's no longer pinned. */
export async function getDesktopFeed(): Promise<DesktopFeed | null> {
  return invoke<DesktopFeed | null>('desktop_feed');
}

/** Calls `cb` with each new feed for this window's card; the engine sends one only when it changed. */
export function onDesktopFeed(cb: (feed: DesktopFeed) => void): () => void {
  return onWindowEvent<DesktopFeed>('desktop-feed', cb);
}

/** A card on the desktop measured itself; its window takes that size. */
export async function fitDesktop(width: number, height: number): Promise<void> {
  if (!inTauri) return;
  await invoke('fit_desktop', { width, height });
}

/** The pointer came onto this desktop card, or left it: a session's card shows lines to the cards
 *  of the sessions it came from or that came of it. */
export async function desktopHover(on: boolean): Promise<void> {
  if (!inTauri) return;
  await invoke('desktop_hover', { on });
}

/** What the window of lines between desktop cards draws now (lineage.rs). */
export async function getLineageFeed(): Promise<LineageFeed | null> {
  if (!inTauri) return null;
  return invoke<LineageFeed>('lineage_feed');
}

/** Calls `cb` with what the window of lines between desktop cards draws, as it changes. */
export function onLineageFeed(cb: (feed: LineageFeed) => void): () => void {
  return onWindowEvent<LineageFeed>('lineage-feed', cb);
}

/* ---- Your other devices (src-tauri/src/peers.rs) ---- */

/** Your other devices as the engine sees them now; null outside the app. */
export async function getDevices(): Promise<DevicesView | null> {
  if (!inTauri) return null;
  return invoke<DevicesView>('devices');
}

/** Calls `cb` with every change to your other devices: linking, connecting, and their boards. */
export function onDevices(cb: (view: DevicesView) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void listen<DevicesView>('devices', (e) => cb(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** Asks one of your devices on your tailnet to link. Resolves to the code both devices show. */
export async function linkDevice(key: string): Promise<string> {
  return invoke<string>('link_device', { key });
}

export async function answerLink(key: string, accept: boolean): Promise<void> {
  await invoke('answer_link', { key, accept });
}

/** A linked device's color, for its badge everywhere; null goes back to the one its key picks. */
export async function setDeviceColor(key: string, color: LabelColor | null): Promise<void> {
  await invoke('set_device_color', { key, color });
}

export async function unlinkDevice(key: string): Promise<void> {
  await invoke('unlink_device', { key });
}

/** A link code for a device that isn't on your tailnet: it works once, for ten minutes. */
export async function deviceLinkCode(): Promise<string> {
  return invoke<string>('device_link_code');
}

/** A link code as a QR code (SVG) of the link the phone app opens, so a phone's camera links it. */
export async function linkCodeQr(code: string): Promise<string> {
  return invoke<string>('link_code_qr', { code });
}

/** Links with the device that made this code. Resolves to its name. */
export async function linkWithCode(code: string): Promise<string> {
  return invoke<string>('link_with_code', { code });
}

/** What was said in sessions on your other devices, searched on each. */
export async function searchDevices(words: string[]): Promise<DeviceFound[]> {
  if (!inTauri || words.length === 0) return [];
  return invoke<DeviceFound[]>('search_devices', { words });
}

/** Whether the floating bar is on. Outside the app it counts as on. */
export async function barOn(): Promise<boolean> {
  if (!inTauri) return true;
  return (await invoke<boolean | null>('bar_on')) ?? true;
}

/** Turns the floating bar on or off. */
export async function setBarOn(on: boolean): Promise<void> {
  if (!inTauri) return;
  await invoke('set_bar_on', { on });
}

/** Where the bar shows: floating where you put it, docked in the notch, or not at all. */
export type BarMode = 'off' | 'floating' | 'notch';

/** The notch the bar docks into, in screen points (bar.rs); `real` is false where the screen has
 *  none and the bar draws one. */
export interface Notch {
  x: number;
  top: number;
  width: number;
  height: number;
  real: boolean;
}

export interface BarModeView {
  mode: BarMode;
  notch: Notch | null;
  /** Docked, it stays open rather than opening under the pointer. */
  keepOpen?: boolean;
}

/** Where the bar shows, with the notch it docks into. Outside the app it floats, unless a
 *  preview says otherwise (`window.__BAR_MODE__`). */
export async function barMode(): Promise<BarModeView> {
  if (!inTauri) return (window as { __BAR_MODE__?: BarModeView }).__BAR_MODE__ ?? { mode: 'floating', notch: null };
  return invoke<BarModeView>('bar_mode');
}

/** Keeps the bar docked in the notch open, or lets it close when the pointer leaves. */
export async function setNotchOpen(keep: boolean): Promise<void> {
  if (!inTauri) return;
  await invoke('set_notch_open', { keep });
}

/** Floats the bar, docks it in the notch, or turns it off. */
export async function setBarMode(mode: BarMode): Promise<void> {
  if (!inTauri) return;
  await invoke('set_bar_mode', { mode });
}

/** Calls `cb` whenever the bar moves between floating, the notch and off, or the notch it docks
 *  into changes (a display plugged in or out). */
export function onBarMode(cb: (view: BarModeView) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void listen<BarModeView>('bar-mode', (e) => cb(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** Tells Codealong the shape the docked bar shows, in its window's points from the top left:
 *  the window takes clicks only on it (bar.rs). */
export async function notchShape(x: number, y: number, w: number, h: number): Promise<void> {
  if (!inTauri) return;
  await invoke('notch_shape', { x, y, w, h });
}

/** Calls `cb` when the pointer comes onto the docked bar's shape or leaves it: its window lets the
 *  pointer through elsewhere, so the page hears nothing of it itself. */
export function onNotchPointer(cb: (on: boolean) => void): () => void {
  return onWindowEvent<boolean>('notch-pointer', cb);
}

/** The bar docked in the notch shows a session under its pills and takes the keyboard for it, or
 *  has put it away and hands the keyboard back to the app you're in (bar.rs `set_keys`). */
export async function notchKeys(on: boolean): Promise<void> {
  if (!inTauri) return;
  await invoke('notch_keys', { on });
}

/** Calls `cb` when the docked bar loses the keyboard without having given it up: a click
 *  somewhere else, or another window or app taking it. */
export function onNotchKeys(cb: (has: boolean) => void): () => void {
  return onWindowEvent<boolean>('notch-keys', cb);
}

/** Calls `cb` when the shortcut that goes through what waits on you is pressed while the bar is
 *  docked in the notch (quick.rs `work_through`). */
export function onWaitingShortcut(cb: () => void): () => void {
  return onWindowEvent<null>('waiting-shortcut', () => cb());
}

/** Keeps the session the docked bar shows on the desktop, in a window of its own where its card
 *  is (kept.rs): the card as the page measured it, in CSS pixels from the window's top left. */
export async function keepNotchSession(key: string, card: PillRect): Promise<void> {
  if (!inTauri) return;
  await invoke('keep_notch_session', { key, ...card });
}

/** Calls `cb` whenever the floating bar is turned on or off, from any window or menu. */
export function onBarOn(cb: (on: boolean) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void listen<boolean>('floating-bar', (e) => cb(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** Where a pill sits in the floating bar's window, in CSS pixels from its top left. */
export interface PillRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Shows a session in the floating bar's peek: for a hover over its pill, with `focus` ready to
 *  type into, or with `rename` its name ready to change. Without a pill it opens where it last
 *  was. */
export async function peekOpen(key: string, pill: PillRect | null, focus = false, rename = false): Promise<void> {
  if (!inTauri) return;
  await invoke('peek_open', { key, pill, focus, rename });
}

/** The peek's page is laid out for the open `id`, at this height: time to show it. */
export async function peekReady(id: number, height: number): Promise<void> {
  if (!inTauri) return;
  await invoke('peek_ready', { id, height });
}

/** The peek's page changed size while up. */
export async function peekFit(height: number): Promise<void> {
  if (!inTauri) return;
  await invoke('peek_fit', { height });
}

/** A click in the peek: it takes the keyboard. */
export async function peekFocus(): Promise<void> {
  if (!inTauri) return;
  await invoke('peek_focus');
}

export async function peekHide(): Promise<void> {
  if (!inTauri) return;
  await invoke('peek_hide');
}

/** You're dragging the peek's edge: its window takes that size (peek.rs). */
export async function peekResize(width: number, height: number): Promise<void> {
  if (!inTauri) return;
  await invoke('peek_resize', { width, height });
}

/** You're dragging the peek by its header, and the pointer has gone this far since the button
 *  went down: from here its window goes where the pointer takes it, and the session is kept on
 *  the desktop where the button comes up (peek.rs). */
export async function peekDrag(dx: number, dy: number): Promise<void> {
  if (!inTauri) return;
  await invoke('peek_drag', { dx, dy });
}

/** Keeps the session the peek shows on the desktop, in a window of its own where the peek is
 *  (kept.rs). */
export async function keepSession(): Promise<void> {
  if (!inTauri) return;
  await invoke('keep_session');
}

/** The session this window keeps on the desktop; null once it's no longer kept. */
export async function keptSession(): Promise<string | null> {
  return invoke<string | null>('kept_session');
}

/** A kept session's page has laid it out: time to show its window. */
export async function keptReady(): Promise<void> {
  if (!inTauri) return;
  await invoke('kept_ready');
}

/** A click in a kept session: it takes the keyboard. */
export async function keptFocus(): Promise<void> {
  if (!inTauri) return;
  await invoke('kept_focus');
}

/** You're dragging a kept session's edge: its window takes that size, keeping its top left corner. */
export async function keptResize(width: number, height: number): Promise<void> {
  if (!inTauri) return;
  await invoke('kept_resize', { width, height });
}

/** Takes this window's session off the desktop, which closes the window. */
export async function keptClose(): Promise<void> {
  if (!inTauri) return;
  await invoke('kept_close');
}

/** Calls `cb` when this kept session's window takes the keyboard (true) or loses it. */
export function onKeptKeys(cb: (has: boolean) => void): () => void {
  return onWindowEvent<boolean>('kept-keys', cb);
}

/** Calls `cb` when the peek is asked to show a session. */
export function onPeekOpen(cb: (open: PeekOpen) => void): () => void {
  return onWindowEvent<PeekOpen>('peek-open', cb);
}

/** Calls `cb` when the peek has hidden. */
export function onPeekHidden(cb: () => void): () => void {
  return onWindowEvent<null>('peek-hidden', () => cb());
}

/** Calls `cb` with what the peek shows whenever that changes; the bar marks the pill with it. */
export function onPeekShown(cb: (shown: PeekShown) => void): () => void {
  return onWindowEvent<PeekShown>('peek', cb);
}

/** Asks the floating bar to show a note in the place of a session's pill. */
export async function barNote(note: BarNote): Promise<void> {
  if (!inTauri) return;
  await emitTo('bar', 'bar-note', note);
}

/** Calls `cb` with notes other windows ask the bar to show. */
export function onBarNote(cb: (note: BarNote) => void): () => void {
  return onWindowEvent<BarNote>('bar-note', cb);
}

/** Listens for an event sent to this window only (`emit_to` or `emitTo`); a plain `listen`
 *  would hear ones sent to any window. Returns an unsubscribe function. */
function onWindowEvent<T>(name: string, cb: (payload: T) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void getCurrentWebviewWindow()
    .listen<T>(name, (e) => cb(e.payload))
    .then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** What the quick panel opens for. Everything is optional: the shortcut opens it with nothing. */
export interface QuickOpen {
  /** The worktree a new session starts in. */
  laneId?: string | null;
  /** The project to show; the panel's own choice when left out. */
  project?: string | null;
  /** The todo a new session is for, and the prompt it starts as. Without the prompt, the
   *  panel writes the todo's. */
  todo?: string | null;
  /** Several todos for one new session, in place of `todo`; the prompt has each of them. */
  todos?: string[];
  prompt?: string;
  /** The board window that asked, which hears how a start went. */
  opener?: string | null;
  /** Brings the panel back as you left it (after a failed start). */
  keep?: boolean;
  /** A session to answer: the panel opens on its conversation and a box for the reply. */
  reply?: string | null;
  /** A PR waiting on your review, by its key: the panel opens on its card, with the box that
   *  reviews it. */
  review?: string | null;
  /** The shortcut that goes through what waits on you: the panel opens on the first session that
   *  does, to answer one after another, as ⇥ on one in its list does. With none, it opens as usual. */
  waiting?: boolean;
  /** How tall the panel's window can be: from its top edge to the bottom of the screen's usable
   *  part. quick.rs adds it to every open. */
  room?: number;
}

/** Opens the quick panel. */
export async function showQuick(context: QuickOpen = {}): Promise<void> {
  if (!inTauri) return;
  await invoke('show_quick', { context });
}

export async function hideQuick(): Promise<void> {
  if (!inTauri) return;
  await invoke('hide_quick');
}

/** The quick panel's page is laid out for the latest open, at this height: time to show it. An
 *  open from Codealong's own script gets its height as the script's result instead (Quick.tsx);
 *  this is for the page that opened itself. */
export async function quickReady(height: number): Promise<void> {
  if (!inTauri) return;
  await invoke('quick_ready', { height });
}

/** The quick panel's page changed size while up. */
export async function fitQuick(height: number): Promise<void> {
  if (!inTauri) return;
  await invoke('fit_quick', { height });
}

/** Calls `cb` when the quick panel has hidden, by Escape, a click elsewhere or its shortcut. */
export function onQuickHidden(cb: () => void): () => void {
  return onWindowEvent<null>('quick-hidden', () => cb());
}

/** Starts a session from the quick panel and brings the app it started in forward. */
export async function startFromQuick(spec: StartSpec): Promise<Started | null> {
  if (!inTauri) throw new Error('Starting a session needs the app.');
  return invoke<Started | null>('start_from_quick', { spec });
}

/** A start from the quick panel, as the window that asked for it hears about it: when it
 *  begins, and when it's done, with the new session's key or what to tell you. */
export type QuickStart = { phase: 'begin'; id: number; label: string } | { phase: 'end'; id: number; key: string | null; message: string | null };

/** Tells a board window how a start it asked for is going. */
export async function tellOpener(label: string, event: QuickStart): Promise<void> {
  if (!inTauri) return;
  await emitTo(label, 'quick-start', event);
}

/** Calls `cb` as starts this window asked the quick panel for go. */
export function onQuickStart(cb: (event: QuickStart) => void): () => void {
  return onWindowEvent<QuickStart>('quick-start', cb);
}

/** This window's label, which the quick panel reports back to. */
export function windowLabel(): string | null {
  return inTauri ? getCurrentWebviewWindow().label : null;
}

/** One of the shortcuts that work in every app, the quick panel's unless `which` says, and
 *  whether it works. */
export async function quickShortcut(which: ShortcutStatus['which'] = 'quick'): Promise<ShortcutStatus> {
  const preview: ShortcutStatus = { which, shortcut: which === 'quick' ? 'Option+Cmd+/' : 'Option+Cmd+.', error: null };
  if (!inTauri) return preview;
  return (await invoke<ShortcutStatus | null>('quick_shortcut', { which })) ?? preview;
}

/** Turns the shortcuts off while Settings records a new one, and back on. */
export async function pauseQuickShortcut(paused: boolean): Promise<void> {
  if (!inTauri) return;
  await invoke('pause_quick_shortcut', { paused });
}

/** Calls `cb` when one of the shortcuts changes or stops working; the status says which. */
export function onQuickShortcut(cb: (status: ShortcutStatus) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void listen<ShortcutStatus>('quick-shortcut', (e) => cb(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** A project's canvas as last saved (src/canvas/model.ts), or null when it has none yet. */
export async function canvasLoad(project: string): Promise<unknown> {
  if (!inTauri) return null;
  return invoke<unknown>('canvas_load', { project });
}

/** Saves a project's canvas. */
export async function canvasSave(project: string, doc: unknown): Promise<void> {
  if (!inTauri) return;
  await invoke('canvas_save', { project, doc });
}

/** A page of a session's conversation (transcript.rs): the newest, or what came before `before`. */
export async function sessionPage(key: string, before: number | null): Promise<TranscriptPage> {
  if (!inTauri) return { items: [], before: null, after: 0 };
  return invoke<TranscriptPage>('session_page', { key, before });
}

/** What a session said and did after `after`. */
export async function sessionNewer(key: string, after: number): Promise<TranscriptPage> {
  if (!inTauri) return { items: [], before: null, after };
  return invoke<TranscriptPage>('session_newer', { key, after });
}

/** What a session waiting on you wants to do, when Codealong can answer it for you. */
/** How a message sent to a session would reach it, for its box to say (send.rs `way_in`); null
 *  for one Codealong runs, and from a device that can't say yet. */
export async function sessionWay(key: string): Promise<Way | null> {
  if (!inTauri) return null;
  return invoke<Way | null>('session_way', { key });
}

export async function sessionPending(key: string): Promise<Pending | null> {
  if (!inTauri) return null;
  return invoke<Pending | null>('session_pending', { key });
}

/** The reply a session Codealong runs is writing, when a turn is running. */
export async function sessionStream(key: string): Promise<HostedStream | null> {
  if (!inTauri) return null;
  return invoke<HostedStream | null>('session_stream', { key });
}

/** Stops the turn a session is taking, as its harness's own Stop or Escape does; says what
 *  happened. */
export async function sessionCancel(key: string): Promise<string> {
  if (!inTauri) throw new Error('Stopping needs the app.');
  return invoke<string>('session_cancel', { key });
}

/** The replies sessions Codealong runs write, as they stream. */
export function onHostedStream(cb: (s: HostedStream) => void): () => void {
  if (!inTauri) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  void listen<HostedStream>('hosted-stream', (e) => cb(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** Allows or denies the call `id` a session waits on; says what happened. A deny with a
 *  `message` tells it what to do instead. */
export async function sessionAnswer(key: string, id: string, answer: 'allow' | 'deny' | 'always', answers?: Answers, message?: string): Promise<string> {
  if (!inTauri) throw new Error('Answering needs the app.');
  return invoke<string>('session_answer', { key, id, answer, answers: answers ?? null, message: message ?? null });
}

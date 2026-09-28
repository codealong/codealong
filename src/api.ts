// The engine lives in Rust; this is the only place the UI talks to it. Outside Tauri (a
// browser preview of a snapshot), the board comes from ./board.json and actions do nothing.

import { invoke } from '@tauri-apps/api/core';
import { emit, emitTo, listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import type { DeviceFound, DevicesView, LabelColor, AccountCfg, AccountInfo, AgentsStatus, Attachment, BarFeed, BarNote, CloseOutcome, IdlePlan, Pending, HostedStream, PeekOpen, PeekShown, TranscriptPage, Board, Config, ConfigView, FoundAccount, PrDetail, Route, Sent, ShortcutStatus, TextFound, StartOptions, StartSpec, Started, Theme, TodoSuggested } from './types';
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

export async function refresh(): Promise<void> {
  if (!inTauri) return;
  await invoke('refresh');
}

/** A PR's description, reviews and changed files. */
export async function prDetail(repo: string, number: number): Promise<PrDetail> {
  if (!inTauri) throw new Error('PR details need the app.');
  return invoke<PrDetail>('pr_detail', { repo, number });
}

/** Which of a PR's comments ask for a change, by comment id, from Jev. Empty while that's off. */
export async function prCommentAsks(title: string, comments: { id: string; author: string; body: string; path: string | null }[]): Promise<Record<string, number>> {
  if (!inTauri || comments.length === 0) return {};
  return (await invoke<Record<string, number> | null>('pr_comment_asks', { title, comments })) ?? {};
}

/** Approves a PR as you. */
export async function approvePr(repo: string, number: number): Promise<void> {
  if (!inTauri) throw new Error('Approving needs the app.');
  await invoke('approve_pr', { repo, number });
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

/** Archives or unarchives a session; Codex threads are archived in Codex itself. Returns the
 *  time of the board that shows it, as `markSeen` does. */
export async function archiveSession(key: string, archived: boolean): Promise<number | null> {
  if (!inTauri) return null;
  return (await invoke<number | null>('archive_session', { key, archived })) ?? null;
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

/** Saves the whole pinned list, in order. */
export async function setPinned(keys: string[]): Promise<void> {
  if (!inTauri) return;
  await invoke('set_pinned', { keys });
}

/** Saves the whole list of pinned worktrees (lane ids), in order. */
export async function setPinnedWorktrees(ids: string[]): Promise<void> {
  if (!inTauri) return;
  await invoke('set_pinned_worktrees', { ids });
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

/** A change to a todo or the labels. Resolves with the time of the first board that shows it. */
/** Where what's typed in the quick panel should go, from Jev. */
export async function routeQuick(project: string, text: string): Promise<Route> {
  if (!inTauri) throw new Error('Routing needs the app.');
  return invoke<Route>('route_quick', { project, text });
}

/** Sessions whose prompts and replies have every word (search.rs). */
export async function searchText(words: string[]): Promise<TextFound> {
  if (!inTauri) return { hits: [], building: false, off: true };
  return invoke<TextFound>('search_text', { words });
}

/** Sends a prompt to a session that's already going, where it runs (send.rs). */
export async function sendToSession(key: string, text: string): Promise<Sent> {
  if (!inTauri) return { message: 'Sent.', opened: false };
  return invoke<Sent>('send_to_session', { key, text });
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

/** What the floating bar asks the main window to show. */
export type Reveal = { kind: 'session'; laneId: string; key: string } | { kind: 'pr'; key: string } | { kind: 'todo'; id: string };

/** Brings the main window forward, showing a session, PR or todo if given. */
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

/** Shows a session in the floating bar's peek: for a hover over its pill, or with `focus` ready
 *  to type into. Without a pill it opens where it last was. */
export async function peekOpen(key: string, pill: PillRect | null, focus = false): Promise<void> {
  if (!inTauri) return;
  await invoke('peek_open', { key, pill, focus });
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

/** The quick panel's shortcut and whether it works. */
export async function quickShortcut(): Promise<ShortcutStatus> {
  if (!inTauri) return { shortcut: 'Option+Cmd+/', error: null };
  return (await invoke<ShortcutStatus | null>('quick_shortcut')) ?? { shortcut: 'Option+Cmd+/', error: null };
}

/** Turns the quick panel's shortcut off while Settings records a new one, and back on. */
export async function pauseQuickShortcut(paused: boolean): Promise<void> {
  if (!inTauri) return;
  await invoke('pause_quick_shortcut', { paused });
}

/** Calls `cb` when the quick panel's shortcut changes or stops working. */
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
export async function sessionPending(key: string): Promise<Pending | null> {
  if (!inTauri) return null;
  return invoke<Pending | null>('session_pending', { key });
}

/** The reply a session Codealong runs is writing, when a turn is running. */
export async function sessionStream(key: string): Promise<HostedStream | null> {
  if (!inTauri) return null;
  return invoke<HostedStream | null>('session_stream', { key });
}

/** Stops the turn a session Codealong runs is taking. */
export async function sessionCancel(key: string): Promise<void> {
  if (!inTauri) throw new Error('Stopping needs the app.');
  await invoke('session_cancel', { key });
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

/** Allows or denies the call `id` a session waits on; says what happened. */
export async function sessionAnswer(key: string, id: string, answer: 'allow' | 'deny'): Promise<string> {
  if (!inTauri) throw new Error('Answering needs the app.');
  return invoke<string>('session_answer', { key, id, answer });
}

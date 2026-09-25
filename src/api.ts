// The engine lives in Rust; this is the only place the UI talks to it. Outside Tauri (a
// browser preview of a snapshot), the board comes from ./board.json and actions do nothing.

import { invoke } from '@tauri-apps/api/core';
import { emitTo, listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import type { AccountCfg, AccountInfo, AgentsStatus, Attachment, BarFeed, Board, Config, ConfigView, FoundAccount, PrDetail, Route, Sent, TextFound, ShortcutStatus, StartOptions, StartSpec, Started, Theme } from './types';
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

export async function markSeen(keys: string[]): Promise<void> {
  if (!inTauri || keys.length === 0) return;
  await invoke('mark_seen', { keys });
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

/** Archives or unarchives a session; Codex threads are archived in Codex itself. */
export async function archiveSession(key: string, archived: boolean): Promise<void> {
  if (!inTauri) return;
  await invoke('archive_session', { key, archived });
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
  /** The todo a new session is for, and the prompt it starts as. */
  todo?: string | null;
  prompt?: string;
  /** The board window that asked, which hears how a start went. */
  opener?: string | null;
  /** Brings the panel back as you left it (after a failed start). */
  keep?: boolean;
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

/** The quick panel's page is laid out for the latest open, at this height: time to show it. */
export async function quickReady(height: number): Promise<void> {
  if (!inTauri) return;
  await invoke('quick_ready', { height });
}

/** The quick panel's page changed size while up. */
export async function fitQuick(height: number): Promise<void> {
  if (!inTauri) return;
  await invoke('fit_quick', { height });
}

/** Calls `cb` when the quick panel is asked to open, with what for. */
export function onQuickOpen(cb: (context: QuickOpen) => void): () => void {
  return onWindowEvent<QuickOpen | null>('quick-open', (c) => cb(c ?? {}));
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
  if (!inTauri) return { shortcut: 'Option+Shift+Space', error: null };
  return (await invoke<ShortcutStatus | null>('quick_shortcut')) ?? { shortcut: 'Option+Shift+Space', error: null };
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

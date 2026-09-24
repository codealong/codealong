// The engine lives in Rust; this is the only place the UI talks to it. Outside Tauri (a
// browser preview of a snapshot), the board comes from ./board.json and actions do nothing.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import type { AccountCfg, AccountInfo, Board, PrDetail, StartOptions, Started, StartSpec, Theme } from './types';
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

/** Opens a project in a window of its own, or brings that window forward. */
export async function openProjectWindow(project: string): Promise<void> {
  if (!inTauri) return;
  await invoke('open_project_window', { project });
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

/** Opens a terminal window that signs the account in. */
export async function signIn(name: string): Promise<void> {
  if (!inTauri) return;
  await invoke('sign_in', { name });
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
export async function todoOp(op: TodoOp): Promise<number> {
  if (!inTauri) return 0;
  return invoke<number>('todo_op', { op });
}

/** Keeps a pasted image and returns the name text refers to it by (`images/<name>`). The bytes
 *  go as they are, not as JSON. */
export async function saveImage(image: Blob): Promise<string> {
  if (!inTauri) throw new Error('Images need the app.');
  return invoke<string>('save_image', new Uint8Array(await image.arrayBuffer()));
}

/** Keeps a copy of an image file dropped from the Finder. */
export async function importImage(path: string): Promise<string> {
  if (!inTauri) throw new Error('Images need the app.');
  return invoke<string>('import_image', { path });
}

/** The text with each kept image as its full path, for pasting into an agent. */
export async function expandImages(text: string): Promise<string> {
  if (!inTauri) return text;
  return invoke<string>('expand_images', { text });
}

/** Opens a kept image in Preview (or whatever opens images). */
export async function openImage(name: string): Promise<void> {
  if (!inTauri) return;
  await invoke('open_image', { name });
}

/** A browser preview of the UI (scripts/preview.sh), where Tauri's IPC is a stand-in. */
const inPreview = typeof window !== 'undefined' && '__CODEALONG_PREVIEW__' in window;

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
export type Reveal = { kind: 'session'; laneId: string; key: string } | { kind: 'pr'; key: string };

/** Brings the main window forward, showing a session or PR if given. */
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

/** Turns the floating bar on or off. */
export async function setBarOn(on: boolean): Promise<void> {
  if (!inTauri) return;
  await invoke('set_bar_on', { on });
}

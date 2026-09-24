// The engine lives in Rust; this is the only place the UI talks to it. Outside Tauri (a
// browser preview of a snapshot), the board comes from ./board.json and actions do nothing.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AccountCfg, AccountInfo, Board, PrDetail, StartOptions, Started, StartSpec } from './types';

export const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export async function getBoard(): Promise<Board> {
  if (inTauri) return invoke<Board>('get_board');
  const res = await fetch('./board.json');
  if (!res.ok) throw new Error('No board.json next to the page. Run the app with `npm run tauri dev`.');
  return (await res.json()) as Board;
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
  return res.ok ? ((await res.json()) as StartOptions) : { accounts: [], codexProjects: [] };
}

/** Starts a session in a worktree: in the terminal, or in the Claude or Codex app. */
export async function startSession(spec: StartSpec): Promise<Started | null> {
  if (!inTauri) throw new Error('Starting a session needs the app.');
  return invoke<Started | null>('start_session', { spec });
}

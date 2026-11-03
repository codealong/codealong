// Codealong's own terminals as a window knows them (terminals.rs): which one each session's
// drawer shows and whether it's open (`useDrawer`), the same for every place in the window that
// shows the session, and opening one (`openTerminal`), which the drawer's +, the session's New
// terminal (⌃`) and Run (RunButton.tsx) all do. The board lists the terminals themselves
// (`Board.terminals`); a window only keeps what it shows.
//
// A terminal is drawn in the window's own colors, the `term-*` tokens, which the server is told
// (`themeOf`) so programs that ask for the colors get them.

import { useSyncExternalStore } from 'react';
import { onTerminalWindows, terminalOpen, terminalWindows } from './api';
import type { TerminalGlance, TerminalSize, TerminalTheme } from './types';

/** A session's drawer: open or folded to its tabs, the tab shown, and when it last asked for the
 *  keyboard (a new terminal does). */
export interface Drawer {
  open: boolean;
  active: string | null;
  focusAt: number;
}

const CLOSED: Drawer = { open: false, active: null, focusAt: 0 };
const drawers = new Map<string, Drawer>();
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

export function drawerOf(key: string): Drawer {
  return drawers.get(key) ?? CLOSED;
}

export function setDrawer(key: string, change: Partial<Drawer>): void {
  drawers.set(key, { ...drawerOf(key), ...change });
  emit();
}

/** A session's drawer, as it changes. */
export function useDrawer(key: string): Drawer {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => drawerOf(key),
  );
}

/** The terminal a drawer shows: the one it was left on, else the last opened. */
export function shownOf(drawer: Drawer, terminals: TerminalGlance[]): TerminalGlance | null {
  return terminals.find((t) => t.id === drawer.active) ?? terminals[terminals.length - 1] ?? null;
}

/** What a tab calls a terminal: the title its program set, else the program in front, else the
 *  command it was opened for, else its shell. */
export function terminalName(t: TerminalGlance): string {
  return t.title.trim() || t.program || t.command || t.shell;
}

/** The window's colors for a terminal, from the `term-*` tokens on the root. */
export function themeOf(root: Element = document.documentElement): TerminalTheme {
  const style = getComputedStyle(root);
  const hex = (name: string, fallback: number) => {
    const v = style.getPropertyValue(name).trim();
    const m = /^#([0-9a-f]{6})$/i.exec(v);
    return m ? parseInt(m[1], 16) : fallback;
  };
  const bg = hex('--term-bg', 0xffffff);
  const r = (bg >> 16) & 255;
  const g = (bg >> 8) & 255;
  const b = bg & 255;
  return {
    fg: hex('--term-fg', 0x000000),
    bg,
    cursor: hex('--term-cursor', 0x000000),
    palette: Array.from({ length: 16 }, (_, i) => hex(`--term-${i}`, 0x808080)),
    dark: 0.2126 * r + 0.7152 * g + 0.0722 * b < 128,
  };
}

/** The terminals popped out into windows of their own (kept.rs `pop_terminal`), which their
 *  drawers leave to those windows: read once, then followed. */
let popped: readonly string[] = [];
let following = false;
const poppedListeners = new Set<() => void>();

function follow() {
  if (following) return;
  following = true;
  const set = (ids: string[]) => {
    popped = ids;
    for (const l of poppedListeners) l();
  };
  onTerminalWindows(set);
  terminalWindows().then(set, () => {
    // A window that can't ask shows every terminal in its drawer, as before any popped out.
  });
}

/** The terminals that have windows of their own, as that changes. */
export function usePoppedTerminals(): readonly string[] {
  follow();
  return useSyncExternalStore(
    (l) => {
      poppedListeners.add(l);
      return () => poppedListeners.delete(l);
    },
    () => popped,
  );
}

/** The last size a terminal was measured at in this window, for the next one to start at. */
let lastSize: TerminalSize | null = null;

export function rememberSize(size: TerminalSize): void {
  lastSize = size;
}

/** ⌃` on a session: its drawer, open with the keyboard in its terminal, or a new terminal when
 *  it has none, as VS Code's key does. */
export async function showTerminal(key: string, folder: string, terminals: TerminalGlance[]): Promise<void> {
  if (!terminals.length) {
    await openTerminal(key, folder);
    return;
  }
  setDrawer(key, { open: true, focusAt: Date.now() });
}

/** Opens a terminal for a session in `folder`, a shell or `command`, and shows it in the session's
 *  drawer with the keyboard. */
export async function openTerminal(key: string, folder: string, command: string | null = null): Promise<TerminalGlance> {
  const t = await terminalOpen(key, folder, command, lastSize, themeOf());
  setDrawer(key, { open: true, active: t.id, focusAt: Date.now() });
  return t;
}

// Codealong's own terminals as a window knows them (terminals.rs): which one each session's
// drawer shows and whether it's open (`useDrawer`), the same for every place in the window that
// shows the session, and opening one (`openTerminal`), which the drawer's +, the session's New
// terminal (⌃`) and Run (RunButton.tsx) all do. The board lists the terminals themselves
// (`Board.terminals`); a window only keeps what it shows.
//
// A terminal is drawn in the window's own colors, the `term-*` tokens, which the server is told
// (`themeOf`) so programs that ask for the colors get them.
//
// What a terminal beside a session shows can go to the session: its selection, or all its text,
// into the session's box (`addToBox`), for you to say what it's for and send. It's never sent
// without you: the shell there is yours, and so is what it printed.

import { useSyncExternalStore } from 'react';
import { inTauri, onTerminalWindows, onWindowKeys, saveAttachment, terminalOpen, terminalWindows, windowKeys } from './api';
import { shortOf } from './devices';
import { addToDraft } from './drafts';
import { attachmentRef } from './todos';
import type { TerminalGlance, TerminalSize, TerminalTheme } from './types';

/** A session's drawer: open or folded to its tabs, the tab shown, and when it last asked for the
 *  keyboard (a new terminal does); and when the terminal the session runs in last asked for it,
 *  which its panel shows (`SessionView`), apart from the drawer's so the two don't both take it. */
export interface Drawer {
  open: boolean;
  active: string | null;
  focusAt: number;
  homeAt: number;
}

const CLOSED: Drawer = { open: false, active: null, focusAt: 0, homeAt: 0 };
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

/** What a session's panel shows: its conversation, or the terminal it runs in, for a session
 *  that runs in one of Codealong's (`TerminalGlance.hosts`). The same in every place in the window
 *  that shows the session. */
export type SessionView = 'conversation' | 'terminal';
const views = new Map<string, SessionView>();

export function sessionViewOf(key: string): SessionView {
  return views.get(key) ?? 'conversation';
}

export function setSessionView(key: string, view: SessionView): void {
  views.set(key, view);
  emit();
}

export function useSessionView(key: string): SessionView {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => sessionViewOf(key),
  );
}

/** The terminal a session runs in, of its terminals: where it lives, not one beside it. */
export function homeTerminalOf(terminals: readonly TerminalGlance[] | undefined): TerminalGlance | null {
  return terminals?.find((t) => t.hosts) ?? null;
}

/** The terminals a drawer lists: those beside the session, not the one it runs in, which its
 *  panel shows in place of the conversation. */
export function besideOf(terminals: readonly TerminalGlance[]): TerminalGlance[] {
  return terminals.filter((t) => !t.hosts);
}

/** Every terminal the board lists, whoever's it is. */
export function allTerminals(byKey: Record<string, TerminalGlance[]> | undefined): TerminalGlance[] {
  return Object.values(byKey ?? {}).flat();
}

/** Where a terminal the server asked to show (a reveal of kind `terminal`) goes, by what the board
 *  says of it: its session's panel (the terminal it runs in, or its drawer's tab), a window of its
 *  own for one that's no session's, or nothing yet (`wait`) while one opened for a command may
 *  still turn out to be a session's, or one the board doesn't list yet may still come, until
 *  `waited` says that's long enough; then one never listed is nowhere. */
export type TerminalPlace = { to: 'session'; key: string; home: boolean } | { to: 'window' } | { to: 'wait' } | { to: 'none' };

/** How long a window waits for the board to say whose a terminal is (`placeOf`): a start's
 *  session is on the board a second or two after its process starts. */
export const TERMINAL_WAIT_MS = 5000;

export function placeOf(t: TerminalGlance | undefined, wait: boolean, waited: boolean): TerminalPlace {
  if (!t) return waited ? { to: 'none' } : { to: 'wait' };
  if (t.session) return { to: 'session', key: t.session, home: !!t.hosts };
  return wait && !waited ? { to: 'wait' } : { to: 'window' };
}

/** The terminal a drawer shows: the one it was left on, else the last opened. */
export function shownOf(drawer: Drawer, terminals: TerminalGlance[]): TerminalGlance | null {
  return terminals.find((t) => t.id === drawer.active) ?? terminals[terminals.length - 1] ?? null;
}

/** What each terminal that asks something asks (terminals.rs `Glance::asks`), by its id. */
export function askingNow(terminals: TerminalGlance[]): Map<string, string> {
  return new Map(terminals.flatMap((t) => (t.running && t.asks ? [[t.id, t.asks] as const] : [])));
}

/** The terminals that started asking since `before` (what each asked then, `askingNow`): one
 *  that asks another question counts again, as does each that asks as the drawer first shows. */
export function startedAsking(terminals: TerminalGlance[], before: Map<string, string>): TerminalGlance[] {
  return terminals.filter((t) => t.running && t.asks && before.get(t.id) !== t.asks);
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

/** Whether this window has the keyboard, which only the window you're in gives a terminal its
 *  size and colors by (terminal/controller.ts): as the OS says, asked as this is first read and
 *  then followed (`windowKeys`, `onWindowKeys`), and not as the page says, which a window that
 *  floats keeps true while it doesn't have it. Until the first answer, no: what waited on it is
 *  given then (`onKeysHere`). */
let keysHere: boolean | null = null;
const keysListeners = new Set<(has: boolean) => void>();

function setKeysHere(has: boolean) {
  keysHere = has;
  for (const l of keysListeners) l(has);
}

if (inTauri) {
  let told = false;
  // Asked once the events are heard, so a change can't come between the answer and them; one
  // that comes while it's asked is newer than the answer, and one before it older (one of the two
  // listeners may have been in before the other).
  const ask = () => {
    told = false;
    return windowKeys().then(
      (has) => {
        if (!told) setKeysHere(has === true);
      },
      () => {
        if (!told) setKeysHere(document.hasFocus());
      },
    );
  };
  onWindowKeys((has) => {
    told = true;
    setKeysHere(has);
  }, () => void ask());
}

export function hasKeys(): boolean {
  // Outside the app (a test), as the page says.
  if (!inTauri) return typeof document !== 'undefined' && document.hasFocus();
  return keysHere === true;
}

/** Calls `cb` as this window gets the keyboard or loses it, and as it's first known. */
export function onKeysHere(cb: (has: boolean) => void): () => void {
  keysListeners.add(cb);
  return () => keysListeners.delete(cb);
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

/** A session's New terminal and Show terminal (⌃`, menus.ts `SessionCan.ownTerminal`) in a window
 *  that shows its drawer: a terminal of Codealong's own under it, in `folder`, for a session on
 *  this Mac, unless config.toml keeps them in your terminal app (`[apps] run_in`). */
export function ownTerminalFor(key: string, folder: string, runIn: string | undefined, terminals: TerminalGlance[] | undefined, failed: (e: unknown) => void): { has: boolean; run: () => void } | undefined {
  if (runIn === 'terminal' || shortOf(key)) return undefined;
  const list = terminals ?? [];
  return { has: list.length > 0, run: () => void showTerminal(key, folder, list).catch(failed) };
}

/** The most lines of a terminal's text that go into a session's box as they are; more go as a
 *  file attached to it, which keeps the box short, and which the agent reads whole. */
export const BOX_LINES = 40;

/** How a terminal's text goes into a session's box (`addToBox`): in a code block whose fence is
 *  longer than any run of backticks in it, or past `BOX_LINES` as a file's text. Blank lines at
 *  either end are left out; null when nothing's left. */
export function boxedText(text: string): { block: string } | { file: string } | null {
  const t = text.replace(/^(?:[ \t]*\n)+/, '').trimEnd();
  if (!t) return null;
  if (t.split('\n').length > BOX_LINES) return { file: `${t}\n` };
  const longest = Math.max(0, ...(t.match(/`+/g) ?? []).map((run) => run.length));
  const fence = '`'.repeat(Math.max(3, longest + 1));
  return { block: `${fence}\n${t}\n${fence}` };
}

/** Adds a terminal's text to the end of a session's box: the box in this window takes the
 *  cursor, and the session's box shows it in every other window (drafts.ts). False when there was
 *  nothing to add. */
export async function addToBox(key: string, text: string): Promise<boolean> {
  const boxed = boxedText(text);
  if (!boxed) return false;
  const words = 'block' in boxed ? boxed.block : attachmentRef(await saveAttachment(new Blob([boxed.file], { type: 'text/plain' }), 'terminal.txt'));
  addToDraft(key, words);
  return true;
}

/** Opens a terminal for a session in `folder`, a shell or `command`, and shows it in the session's
 *  drawer with the keyboard. */
export async function openTerminal(key: string, folder: string, command: string | null = null): Promise<TerminalGlance> {
  const t = await terminalOpen(key, folder, command, lastSize, themeOf());
  setDrawer(key, { open: true, active: t.id, focusAt: Date.now() });
  return t;
}

// Where a key goes while a terminal holds the keyboard: to the program, as the key it is (the
// server encodes it for the program's modes, terminals.rs), or to the app (platform.ts
// `appKeyInTerminal`: ⌘ on macOS), to copying and pasting, or to the input method, whose text
// comes as it's composed. The modifiers are read off the event as they are, since the program is
// told exactly which were held; whether a key is the app's goes by platform.ts.

import { TERMINAL_CHORD } from '../../menus';
import { appKeyInTerminal, chordPressed, platform, terminalClipboard } from '../../platform';
import type { TerminalInput, TerminalMods } from '../../types';

export type KeyLike = Pick<KeyboardEvent, 'key' | 'code' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'repeat' | 'isComposing'>;

export type KeyRoute =
  /** The program's: send it. */
  | { to: 'terminal'; input: TerminalInput }
  /** The app's: leave it be. */
  | { to: 'app' }
  | { to: 'copy' }
  /** Pasting happens in the paste event the key brings, which has the clipboard. */
  | { to: 'paste' }
  | { to: 'selectAll' }
  /** An input method's or a dead key's: its text comes when it's done. */
  | { to: 'compose' };

export function modsOf(e: Pick<KeyboardEvent, 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>): TerminalMods {
  return { shift: e.shiftKey, ctrl: e.ctrlKey, alt: e.altKey, meta: e.metaKey };
}

/** A key's name for a key that types nothing (Enter, ArrowUp, F1, Shift): two or more letters or
 *  digits starting with a capital. Anything else is what the key typed. */
const NAMED = /^[A-Z][A-Za-z0-9]+$/;

/** Where a key pressed (`down`) or let go goes. */
export function routeKey(e: KeyLike, down: boolean): KeyRoute {
  if (e.isComposing || e.key === 'Process' || e.key === 'Dead') return { to: 'compose' };
  // ⌃` takes the keyboard back to the box, as it opens the terminal from there.
  if (chordPressed(TERMINAL_CHORD, e)) return { to: 'app' };
  const clip = terminalClipboard(e);
  if (clip) return { to: clip };
  if (platform() === 'mac' && e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey && e.code === 'KeyA') return { to: 'selectAll' };
  if (appKeyInTerminal(e)) return { to: 'app' };
  const mods = modsOf(e);
  const char = e.key !== 'Unidentified' && !NAMED.test(e.key) ? e.key : undefined;
  // A release names the key as its press did, for a program that pairs them.
  if (!down) return { to: 'terminal', input: { kind: 'key', code: e.code, unshifted: unshiftedOf(e, char), mods, action: 'release' } };
  // What the key typed goes with it; with Ctrl the program goes by the key alone.
  const text = !e.ctrlKey && !e.metaKey ? char : undefined;
  return { to: 'terminal', input: { kind: 'key', code: e.code, text, unshifted: unshiftedOf(e, char), mods, action: e.repeat ? 'repeat' : 'press' } };
}

/** The character the key gives with no modifiers on your layout, which Ctrl and a letter go by
 *  (on AZERTY Ctrl+A is the key a US keyboard has Q on): a letter's own, in lower case, and
 *  anything else only when neither Shift nor Option changed it. None where it can't be told. */
function unshiftedOf(e: Pick<KeyboardEvent, 'shiftKey' | 'altKey'>, char: string | undefined): string | undefined {
  if (!char || [...char].length !== 1) return undefined;
  if (/^\p{L}$/u.test(char)) return e.altKey ? undefined : char.toLowerCase();
  return e.shiftKey || e.altKey ? undefined : char;
}

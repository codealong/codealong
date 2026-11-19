// What a unit test needs to mount a hook and press a key: a hook rendered in a component of its
// own with React's own test utilities (`act`, `createRoot`), and key events as a keyboard sends
// them. The page is jsdom's, for the files that ask for one: a test file that imports this starts
// with `// @vitest-environment jsdom`, and the rest of the tests stay in plain Node.
//
// It brings the engine's stand-in with it (./tauri), which has to load before src/api.ts does:
// import this first in the test file, after vitest.
//
// Only tests import this file. Nothing the app's pages load reaches it, so it isn't in the bundle.

import './tauri';
import { act, createElement, type ReactNode } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach } from 'vitest';
import { parseChord, platformChord, setPlatform } from '../platform';

export { act } from 'react';
export { engine } from './tauri';

// React warns of a state change outside `act` only where a test says it's in one.
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const roots = new Set<Root>();

/** A hook as a window calls it. */
export interface Mounted<R, P> {
  /** What it returned on its last render. */
  readonly current: R;
  /** Renders it again with other arguments, as a window does when what it has changes. */
  rerender: (props: P) => void;
  unmount: () => void;
}

/** Mounts `hook` in a component of its own on the page, and keeps what it returns. Whatever a
 *  test mounted is unmounted as it ends. */
export function mountHook<R>(hook: () => R): Mounted<R, void>;
export function mountHook<R, P>(hook: (props: P) => R, props: P): Mounted<R, P>;
export function mountHook<R, P>(hook: (props: P) => R, props?: P): Mounted<R, P> {
  let current!: R;
  function Host({ with: p }: { with: P }): ReactNode {
    current = hook(p);
    return null;
  }
  const root = createRoot(document.body.appendChild(document.createElement('div')));
  roots.add(root);
  const render = (p: P) => act(() => root.render(createElement(Host, { with: p })));
  render(props as P);
  return {
    get current() {
      return current;
    },
    rerender: render,
    unmount: () => {
      roots.delete(root);
      act(() => root.unmount());
    },
  };
}

/** Lets what waits on a promise run (an `invoke` answered, a listener the engine has by now), and
 *  the page draw what came of it. */
export function settle(): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));
}

/** Where each key that isn't a letter or a digit is on the keyboard, and what it types with ⇧. */
const KEYS: Record<string, { key: string; code: string; shifted?: string }> = {
  Up: { key: 'ArrowUp', code: 'ArrowUp' },
  Down: { key: 'ArrowDown', code: 'ArrowDown' },
  Left: { key: 'ArrowLeft', code: 'ArrowLeft' },
  Right: { key: 'ArrowRight', code: 'ArrowRight' },
  Esc: { key: 'Escape', code: 'Escape' },
  Space: { key: ' ', code: 'Space' },
  ']': { key: ']', code: 'BracketRight', shifted: '}' },
  '[': { key: '[', code: 'BracketLeft', shifted: '{' },
  '-': { key: '-', code: 'Minus', shifted: '_' },
  '=': { key: '=', code: 'Equal', shifted: '+' },
  '`': { key: '`', code: 'Backquote', shifted: '~' },
  ',': { key: ',', code: 'Comma', shifted: '<' },
  '.': { key: '.', code: 'Period', shifted: '>' },
  '/': { key: '/', code: 'Slash', shifted: '?' },
};

/** The key event a US keyboard sends for a chord written as the app writes them, in macOS's terms
 *  (`Shift+Cmd+E`, `Ctrl+Cmd+Up`, `Esc`, `j`), pressed as the platform a test set has it
 *  (platform.ts `setPlatform`): on Linux, `Shift+Cmd+E` is Ctrl, Shift and E. A letter types its
 *  capital with ⇧ held, as a keyboard does. */
export function keyOf(chord: string): KeyboardEventInit {
  const { mods, key: name } = parseChord(platformChord(chord));
  const shift = mods.includes('Shift');
  const known = KEYS[name];
  const letter = /^[A-Za-z]$/.test(name);
  const key = known ? ((shift && known.shifted) || known.key) : letter ? (shift ? name.toUpperCase() : name.toLowerCase()) : name;
  const code = known ? known.code : letter ? `Key${name.toUpperCase()}` : /^[0-9]$/.test(name) ? `Digit${name}` : name;
  return { key, code, metaKey: mods.includes('Cmd'), ctrlKey: mods.includes('Ctrl'), altKey: mods.includes('Alt'), shiftKey: shift };
}

/** Sends a key event where the keyboard is: to the field that has it, else the page's body, from
 *  where it comes up to the window's listeners in the order a real one reaches them. Answers the
 *  event, which says whether something took it (`defaultPrevented`). */
export function type(init: KeyboardEventInit): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });
  act(() => void (document.activeElement ?? document.body).dispatchEvent(event));
  return event;
}

/** Presses a chord (`keyOf`) where the keyboard is (`type`). */
export function press(chord: string): KeyboardEvent {
  return type(keyOf(chord));
}

// Each test starts on an empty page that remembers nothing (stored.ts), on the platform the page
// says it is, with nothing mounted.
afterEach(() => {
  for (const root of roots) act(() => root.unmount());
  roots.clear();
  document.body.replaceChildren();
  window.localStorage.clear();
  setPlatform(null);
});

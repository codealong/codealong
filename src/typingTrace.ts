// What moves the text cursor in a session's reply box, for the perf log while `[debug] perf_log`
// is on. Reported twice (2026-10-30, 2026-11-06): typing in the notch's box with the same session
// open in the main window, the caret jumps to near the start and what's typed next lands there.
// Nothing in the code explains it: WebKit puts the caret at the end whenever a box's value is
// replaced (a draft taken from another window, drafts.ts), and only the window you type in writes
// the draft. So each window writes down what happened to its boxes: typing, with where it went
// in; the caret moving when nothing here moved it (no click, no arrow key, no typing); drafts
// taken from another window; focus coming and going, the page's and the box's; and every call of
// the page's own that places the caret (`focus`, `setSelectionRange`, `selectionStart`), with
// where in the bundle it came from. Lines start `box:`, after the window's `ui[label]`.

import { useEffect } from 'react';
import { perfMark } from './api';

/** The last caret seen in a box, and its text's length then. */
interface Seen {
  caret: number;
  length: number;
}

/** How long after a click or a key that moves the caret a move counts as yours. */
const YOURS_MS = 600;
/** How long after the page placed the caret a move to that place counts as the call's. */
const PLACED_MS = 1000;

let installed: (() => void) | null = null;
let seen = new WeakMap<HTMLTextAreaElement, Seen>();
/** Boxes taking a draft from another window: the caret's next move is the draft's doing. */
let taking = new WeakSet<HTMLTextAreaElement>();
/** Where the page last placed the caret in a box, which was said with the call: a move there soon
 *  after is the call's, and any other move is still said. */
let placed = new WeakMap<HTMLTextAreaElement, { caret: number; at: number }>();
let yoursAt = -Infinity;

/** The reply box an element is, if it's one. */
function box(el: unknown): HTMLTextAreaElement | null {
  return el instanceof HTMLTextAreaElement && el.closest('.ca-composer') ? el : null;
}

function yes(b: boolean): string {
  return b ? 'yes' : 'no';
}

/** Where in the bundle a call came from: its first few frames past this file, as file:line:col.
 *  The frames below the message line (Chrome's) are this function's, the wrapper's, then the
 *  caller's. */
function caller(): string {
  const frames = (new Error().stack ?? '').split('\n').filter((f) => f.trim() && f.trim() !== 'Error').slice(2, 6);
  return frames
    .map((f) => f.replace(/^\s*(at\s+)?/, '').replace(/https?:\/\/[^/]+\//, '').replace(/^tauri:\/\/localhost\//, ''))
    .filter(Boolean)
    .join(' < ');
}

function note(line: string): void {
  perfMark(`box: ${line}`);
}

function look(el: HTMLTextAreaElement): Seen {
  return { caret: el.selectionStart, length: el.value.length };
}

/** A draft from another window is about to replace what `el` holds (SessionReply.tsx): its caret
 *  moving then is the draft's doing, which is said with it. */
export function noteDraftTaken(el: HTMLTextAreaElement | null, from: string, to: string): void {
  if (!installed || !el || from === to) return;
  const focused = document.activeElement === el;
  if (focused) taking.add(el);
  note(`taking a draft from another window: ${from.length} → ${to.length} long, caret ${el.selectionStart}; box focused ${yes(focused)}, page focused ${yes(document.hasFocus())}`);
}

/** How many hooks in this window want the trace now. */
let wanted = 0;

/** The trace on while `on`, the perf log's flag as the window has it. Several parts of a window
 *  can ask (its pushes' timing, a card it shows): it's on while any of them does. */
export function useTypingTrace(on: boolean): void {
  useEffect(() => {
    if (!on) return;
    wanted++;
    traceTyping(true);
    return () => {
      wanted--;
      if (wanted === 0) traceTyping(false);
    };
  }, [on]);
}

/** Turns the trace on or off. */
function traceTyping(on: boolean): void {
  if (on === !!installed) return;
  if (!on) {
    installed?.();
    installed = null;
    return;
  }
  const navKeys = new Set(['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown']);
  const onKey = (e: KeyboardEvent) => {
    if (navKeys.has(e.key) || ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'a')) yoursAt = performance.now();
  };
  const onPointer = () => {
    yoursAt = performance.now();
  };
  const onBeforeInput = (e: Event) => {
    const el = box(e.target);
    if (!el) return;
    const was = seen.get(el);
    const ie = e as InputEvent;
    if (was && (was.caret !== el.selectionStart || was.length !== el.value.length) && performance.now() - yoursAt > YOURS_MS) {
      note(`${ie.inputType} went in at ${el.selectionStart}-${el.selectionEnd} of ${el.value.length}, where the caret was last seen at ${was.caret} of ${was.length}; page focused ${yes(document.hasFocus())}`);
    }
  };
  const onInput = (e: Event) => {
    const el = box(e.target);
    if (!el) return;
    const ie = e as InputEvent;
    const now = look(el);
    seen.set(el, now);
    note(`typed ${ie.inputType} (${ie.data?.length ?? 0}), caret ${now.caret} of ${now.length}; page focused ${yes(document.hasFocus())}`);
  };
  const onSelection = () => {
    const el = box(document.activeElement);
    if (!el) return;
    const now = look(el);
    const was = seen.get(el);
    seen.set(el, now);
    if (taking.delete(el)) {
      placed.delete(el);
      note(`caret after the draft: ${now.caret} of ${now.length}`);
      return;
    }
    const mark = placed.get(el);
    placed.delete(el);
    if (mark && mark.caret === now.caret && performance.now() - mark.at < PLACED_MS) return;
    if (!was || (was.caret === now.caret && was.length === now.length) || performance.now() - yoursAt < YOURS_MS) return;
    note(`caret moved by itself: ${was.caret} of ${was.length} → ${now.caret} of ${now.length}; page focused ${yes(document.hasFocus())}`);
  };
  // Where focusing puts the caret comes after this (WebKit restores the box's last selection), so
  // what was seen before stays: a restore to somewhere else shows as a move by itself.
  const onFocus = (e: FocusEvent) => {
    const el = box(e.target);
    if (!el) return;
    note(`box focused (caret ${el.selectionStart} of ${el.value.length} as it comes); page focused ${yes(document.hasFocus())}`);
  };
  const onBlur = (e: FocusEvent) => {
    const el = box(e.target);
    if (!el) return;
    const to = e.relatedTarget instanceof Element ? e.relatedTarget.tagName.toLowerCase() + (e.relatedTarget.className ? `.${String(e.relatedTarget.className).split(' ')[0]}` : '') : 'nothing';
    note(`box lost focus to ${to}, caret ${el.selectionStart} of ${el.value.length}; page focused ${yes(document.hasFocus())}`);
  };
  const onPage = (e: Event) => note(`page ${e.type === 'focus' ? 'focused' : 'unfocused'}; box focused ${yes(!!box(document.activeElement))}`);

  // The page's own calls that place the caret, said with where they came from.
  const proto = HTMLTextAreaElement.prototype;
  const ownFocus = Object.getOwnPropertyDescriptor(proto, 'focus');
  const focus = ownFocus ?? Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'focus');
  const setRange = Object.getOwnPropertyDescriptor(proto, 'setSelectionRange');
  const start = Object.getOwnPropertyDescriptor(proto, 'selectionStart');
  Object.defineProperty(proto, 'focus', {
    configurable: true,
    writable: true,
    value: function (this: HTMLTextAreaElement, options?: FocusOptions) {
      // Focusing a box that hasn't the focus puts back its last selection, which its
      // `selectionStart` reads until then; one that has it stays as it is.
      if (box(this) && document.activeElement !== this) {
        placed.set(this, { caret: this.selectionStart, at: performance.now() });
        note(`focus() called, caret ${this.selectionStart} of ${this.value.length}: ${caller()}`);
      }
      return (focus?.value as HTMLElement['focus']).call(this, options);
    },
  });
  Object.defineProperty(proto, 'setSelectionRange', {
    configurable: true,
    writable: true,
    value: function (this: HTMLTextAreaElement, a: number | null, b: number | null, dir?: 'forward' | 'backward' | 'none') {
      if (box(this)) {
        placed.set(this, { caret: Math.min(a ?? 0, this.value.length), at: performance.now() });
        note(`setSelectionRange(${a}, ${b}) called, of ${this.value.length}: ${caller()}`);
      }
      return (setRange?.value as HTMLTextAreaElement['setSelectionRange']).call(this, a, b, dir);
    },
  });
  if (start?.set && start.get) {
    const get = start.get;
    const set = start.set;
    Object.defineProperty(proto, 'selectionStart', {
      configurable: true,
      get(this: HTMLTextAreaElement) {
        return get.call(this) as number;
      },
      set(this: HTMLTextAreaElement, v: number) {
        if (box(this)) {
          placed.set(this, { caret: Math.min(v, this.value.length), at: performance.now() });
          note(`selectionStart = ${v} set, of ${this.value.length}: ${caller()}`);
        }
        set.call(this, v);
      },
    });
  }

  window.addEventListener('keydown', onKey, true);
  window.addEventListener('pointerdown', onPointer, true);
  document.addEventListener('beforeinput', onBeforeInput, true);
  document.addEventListener('input', onInput, true);
  document.addEventListener('selectionchange', onSelection);
  document.addEventListener('focusin', onFocus, true);
  document.addEventListener('focusout', onBlur, true);
  window.addEventListener('focus', onPage);
  window.addEventListener('blur', onPage);
  installed = () => {
    window.removeEventListener('keydown', onKey, true);
    window.removeEventListener('pointerdown', onPointer, true);
    document.removeEventListener('beforeinput', onBeforeInput, true);
    document.removeEventListener('input', onInput, true);
    document.removeEventListener('selectionchange', onSelection);
    document.removeEventListener('focusin', onFocus, true);
    document.removeEventListener('focusout', onBlur, true);
    window.removeEventListener('focus', onPage);
    window.removeEventListener('blur', onPage);
    // The page's own again: the prototype's own properties put back, an inherited `focus` uncovered.
    if (ownFocus) Object.defineProperty(proto, 'focus', ownFocus);
    else delete (proto as { focus?: unknown }).focus;
    if (setRange) Object.defineProperty(proto, 'setSelectionRange', setRange);
    if (start) Object.defineProperty(proto, 'selectionStart', start);
    // Turned on again later, it starts from nothing it saw before.
    seen = new WeakMap();
    taking = new WeakSet();
    placed = new WeakMap();
    yoursAt = -Infinity;
  };
}

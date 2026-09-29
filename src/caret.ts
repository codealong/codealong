// The text cursor lost in a window you're typing in: WebKit draws it only while it has the page as
// focused in a key window, and can get that wrong while keys still arrive (caret.rs). A key typed
// into a text field of a page that says it isn't focused is the sign; Codealong then logs what
// macOS says of the window (with `[debug] perf_log` on) and hands the page its focus again, and the
// page logs whether that brought it back.

import { pageUnfocused, perfMark } from './api';

/** How long after a repair another may be asked for, so a page that stays unfocused isn't asked
 *  about on every key. */
const AGAIN_MS = 5000;

function typing(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target instanceof HTMLTextAreaElement) return !target.readOnly;
  if (target instanceof HTMLInputElement) return !target.readOnly && !['button', 'checkbox', 'radio', 'range', 'color', 'file', 'submit', 'reset', 'image'].includes(target.type);
  return target.isContentEditable;
}

export function watchCaret(): void {
  let last = -Infinity;
  document.addEventListener(
    'keydown',
    (e) => {
      if (document.hasFocus() || !typing(e.target) || performance.now() - last < AGAIN_MS) return;
      last = performance.now();
      pageUnfocused()
        .then(() => new Promise((r) => requestAnimationFrame(() => window.setTimeout(r, 50))))
        .then(() => perfMark(`typing into a text field WebKit had as unfocused: ${document.hasFocus() ? 'focused again' : 'still unfocused'} after the repair`))
        .catch(() => {});
    },
    { capture: true },
  );
}

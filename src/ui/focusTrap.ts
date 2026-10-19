// Keeps the keyboard inside a dialog while it's up: Tab and ⇧Tab go round its controls and
// never out to the window behind it, the dialog has the keyboard as it opens, and what had it
// before gets it back when the dialog closes. `Dialog` in primitives.tsx uses it. The window's
// own keys, heard on the window itself, ask `dialogUp` and leave the keyboard to it.

import { useEffect, useState, type RefObject } from 'react';
import { typed } from '../platform';

const STOPS = 'a[href], button, input, select, textarea, [tabindex]';

/** Whether a dialog is up, so the keys of what's behind it (a list's, a session's chords, the
 *  queue's, ⌘⏎ allowing what a session waits on) don't act: a dialog a session's panel opens
 *  (MoveAccount.tsx) is in a window whose keys work on that session. */
export function dialogUp(): boolean {
  return document.querySelector('[aria-modal="true"]') !== null;
}

/** What Tab stops at inside `root`, in the page's order: controls that are shown, enabled and
 *  not taken out of the order (`tabindex="-1"`). */
export function tabStops(root: Element): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(STOPS)).filter((el) => el.tabIndex >= 0 && !el.matches(':disabled') && el.getClientRects().length > 0);
}

/** Where Tab (or ⇧Tab, `back`) goes from `active` when the browser's own next stop would be
 *  outside: round to the other end from the last or first stop, and to an end from anywhere
 *  that isn't a stop (the dialog itself, the window behind it). Null leaves it to the browser,
 *  which moves between two stops inside. */
export function tabStop<T>(stops: readonly T[], active: T | null, back: boolean): T | null {
  if (stops.length === 0) return null;
  const first = stops[0];
  const last = stops[stops.length - 1];
  const at = active === null ? -1 : stops.indexOf(active);
  if (at < 0) return back ? last : first;
  if (back && at === 0) return last;
  if (!back && at === stops.length - 1) return first;
  return null;
}

/** Traps the keyboard in `ref`'s element while it's mounted. The element takes the keyboard
 *  itself when nothing in it asked for it (`autoFocus`), so it needs a `tabIndex` of -1. */
export function useFocusTrap(ref: RefObject<HTMLElement | null>): void {
  // What had the keyboard before, read as the dialog first renders: by the time an effect runs,
  // a field in it that focuses itself already has it.
  const [before] = useState(() => (typeof document === 'undefined' ? null : (document.activeElement as HTMLElement | null)));
  useEffect(() => {
    const box = ref.current;
    if (!box) return;
    if (!box.contains(document.activeElement)) box.focus({ preventScroll: true });
    // After the page's own handlers, so a field that uses Tab itself keeps it.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Tab' || e.defaultPrevented || !typed(e)) return;
      const stops = tabStops(box);
      const active = document.activeElement as HTMLElement | null;
      const to = stops.length === 0 ? box : tabStop(stops, active, e.shiftKey);
      if (!to) return;
      e.preventDefault();
      to.focus();
    };
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('keydown', onKey);
      // Back to what had it, unless something has taken the keyboard since.
      const now = document.activeElement;
      if ((!now || now === document.body || box.contains(now)) && before?.isConnected) before.focus({ preventScroll: true });
    };
  }, [ref, before]);
}

// How a popover goes away, written once: a press anywhere else, and Escape. Every menu, picker
// and card that opens over the window uses this (a dialog too, for Escape), so they agree on
// what counts as "anywhere else" and on whose Escape it is when several are up.
//
// The press is `pointerdown`, heard on the window as it comes down (the capture phase), not
// `mousedown` on the document as it comes back up. Three reasons, all of them things this app
// does. A chip or a button on a row stops its mousedown from reaching the row, which stopped it
// reaching a listener on the document too: a press there left a popover up. A pointerdown that's
// been taken (`preventDefault`, for a drag) is followed by no mousedown at all in WebKit. And
// popovers nest (a picker in a dialog, a card over a panel): heard on the way down, each one
// is asked only whether the press landed inside it, before anything inside can have an opinion,
// so a press in the inner one closes neither and a press in the outer one closes the inner.
//
// Escape goes to the one opened last, which takes the key: a picker in a dialog closes and the
// dialog stays. It's heard on the document after the page's own handlers, so a field that uses
// Escape itself (a name being typed, a turn to stop) gets it first and says so by taking it.

import { useEffect, useRef, type RefObject } from 'react';

export interface DismissOptions {
  /** Whether it's up: nothing is listened for while it isn't. Default true. */
  when?: boolean;
  /** What else a press may land on without closing it: the button that opened it, a part of it
   *  drawn elsewhere in the page. A selector some ancestor of the press matches, or a test. */
  inside?: string | ((target: Element) => boolean);
  /** The keys that close it. `escape` (the default) closes the one opened last and takes the
   *  key. `any` is for a card the pointer brought up: any key puts it away and still does what
   *  it does. `none` leaves keys to the caller. */
  keys?: 'escape' | 'any' | 'none';
  /** Closes it when the window goes to the background too. */
  blur?: boolean;
}

/** Whether a press on `target` is outside `el` and whatever else `inside` allows. */
export function pressedOutside(target: EventTarget | null, el: Pick<Element, 'contains'> | null, inside?: DismissOptions['inside']): boolean {
  const t = target as (Element & Node) | null;
  if (!t) return true;
  if (el?.contains(t)) return false;
  if (typeof inside === 'string') return !t.closest?.(inside);
  return !inside?.(t);
}

interface Open {
  close: () => void;
}

// What's up that Escape closes, in the order it opened: the last is the one on top.
const escapes: Open[] = [];

/** Escape, for whatever opened last. A key something in the page already took isn't ours. */
export function escapeTopmost(e: Pick<KeyboardEvent, 'key' | 'defaultPrevented' | 'isComposing' | 'preventDefault' | 'stopPropagation'>, stack: readonly Open[] = escapes): boolean {
  if (e.key !== 'Escape' || e.defaultPrevented || e.isComposing) return false;
  const top = stack[stack.length - 1];
  if (!top) return false;
  // It's taken: the window's own keys (the list's, a dialog under this one) don't see it.
  e.preventDefault();
  e.stopPropagation();
  top.close();
  return true;
}

const onEscape = (e: KeyboardEvent) => void escapeTopmost(e);

function pushEscape(entry: Open): () => void {
  if (escapes.length === 0) document.addEventListener('keydown', onEscape);
  escapes.push(entry);
  return () => {
    const at = escapes.indexOf(entry);
    if (at >= 0) escapes.splice(at, 1);
    if (escapes.length === 0) document.removeEventListener('keydown', onEscape);
  };
}

/** Escape closes it, while `when`: the one opened last when several are up. A dialog's part of
 *  `useDismiss`, for what a press outside doesn't close the same way. */
export function useEscape(onClose: () => void, when = true): void {
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    if (!when) return;
    return pushEscape({ close: () => close.current() });
  }, [when]);
}

/** Closes a popover on a press outside `ref` (anywhere at all, without a ref) and on Escape.
 *  `onClose` may be a new function every render: the latest is called. */
export function useDismiss(ref: RefObject<Element | null> | null, onClose: () => void, options: DismissOptions = {}): void {
  const { when = true, keys = 'escape', blur = false } = options;
  const latest = useRef({ onClose, inside: options.inside });
  latest.current = { onClose, inside: options.inside };
  useEscape(onClose, when && keys === 'escape');
  useEffect(() => {
    if (!when) return;
    const close = () => latest.current.onClose();
    const onDown = (e: PointerEvent) => {
      if (pressedOutside(e.target, ref?.current ?? null, latest.current.inside)) close();
    };
    window.addEventListener('pointerdown', onDown, true);
    if (keys === 'any') window.addEventListener('keydown', close, true);
    if (blur) window.addEventListener('blur', close);
    return () => {
      window.removeEventListener('pointerdown', onDown, true);
      if (keys === 'any') window.removeEventListener('keydown', close, true);
      if (blur) window.removeEventListener('blur', close);
    };
  }, [ref, when, keys, blur]);
}

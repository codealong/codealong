// The quick panel's size. The card is one height whatever it lists, less only on a screen without
// the room (asked 2026-10-16: it grew and shrank as suggestions, results and actions came and
// went, and holding the list's height while you typed only put off the shrink until you paused).
// What it lists scrolls within it, the new-session row stays above the bar, and the bar's menus
// open over the list. quick.rs sizes its window to the card, plus room for the shadow; the open
// reports that height, to the script that opened it, which returns it, or with quickReady.
// Opened to listen (Quick.tsx) it lists nothing, and is one height of its own: what a listening
// asks, with room for every choice it can show.

import { useLayoutEffect, type MutableRefObject, type RefObject } from 'react';
import { quickReady } from '../api';

/** Room around the card for its shadow, which is drawn to fit it (`--shadow-quick`, held to it by
 *  quick.rs's tests). The window (quick.rs `MARGIN`) is the card and this on each side. */
export const MARGIN = 20;
/** The card's height: what it reached with the list at its tallest, before it was fixed. */
export const HEIGHT = 640;

/** The card's height when the panel is opened to listen: the box, what a listening asks with
 *  every choice showing (a meeting from the calendar, a session on each request), the row that
 *  starts it and the bar, with a line to spare for something that went wrong. */
export const LISTEN_HEIGHT = 500;

/** The card's height where the window has `room` on its screen (null: off any screen), `tall`
 *  when it has the room. */
export function cardHeight(room: number | null, tall: number = HEIGHT): number {
  return room ? Math.min(tall, room - 2 * MARGIN) : tall;
}

export interface QuickSize {
  list: RefObject<HTMLDivElement | null>;
  scroller: RefObject<HTMLDivElement | null>;
  input: RefObject<HTMLTextAreaElement | null>;
  /** True while a dialog over the panel has the keyboard (DevSetupDialog.tsx): an open leaves it
   *  there rather than giving it to the input behind it. */
  asking: { readonly current: boolean };
  /** Counts opens; each is answered with the height. */
  opens: number;
  /** How tall the window can be on its screen. */
  room: number | null;
  /** How tall the card is with the room for it: `HEIGHT`, or `LISTEN_HEIGHT` opened to listen. */
  tall: number;
  /** Who opens the panel while it's being laid out: the script that returns the height, or the page. */
  opening: MutableRefObject<'script' | 'page' | null>;
  /** The height last reported, which the opening script returns. */
  reported: MutableRefObject<number>;
  /** Set by the open that unfolds the panel (quick.rs `put_aside`): the caret stays where it was. */
  caretKept: MutableRefObject<boolean>;
}

export function useQuickSize({ list, scroller, input, asking, opens, room, tall, opening, reported, caretKept }: QuickSize): void {
  // A scrollbar, where macOS shows one, narrows the rows while the list scrolls; the new-session
  // row below it keeps in line with them.
  useLayoutEffect(() => {
    const el = scroller.current;
    list.current?.style.setProperty('--quick-gutter', `${el ? el.offsetWidth - el.clientWidth : 0}px`);
  });

  useLayoutEffect(() => {
    if (opens === 0) return;
    const h = cardHeight(room, tall) + 2 * MARGIN;
    reported.current = h;
    // The cursor goes at the end of whatever the panel opens with: nothing, a todo's prompt, or
    // what a failed start left; unfolded, where you left it.
    const el = input.current;
    if (el && !asking.current) {
      el.focus();
      if (!caretKept.current) el.setSelectionRange(el.value.length, el.value.length);
    }
    caretKept.current = false;
    if (opening.current !== 'script') void quickReady(h);
    // The room and what it's opened for come with each open. The refs are the same objects on
    // every render.
  }, [opens, room, tall, input, asking, opening, reported, caretKept]);
}

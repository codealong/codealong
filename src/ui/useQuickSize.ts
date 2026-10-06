// The quick panel's height. quick.rs sizes its window to the card, plus room for the shadow, or
// to a menu where it reaches below the card: the open reports the height it laid out to (to the
// script that opened it, which returns it, or with quickReady), and each change after is sent
// with fitQuick. While you type, the list keeps the height it reached until typing pauses and
// the answers are in, so the panel doesn't bounce with every key.

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type MutableRefObject, type RefObject } from 'react';
import { fitQuick, quickReady } from '../api';

/** Room around the card for its shadow. The window (quick.rs) is the card and this on each side. */
export const MARGIN = 20;
/** How long typing has to pause before the list may shrink to what it shows. */
const SETTLE_MS = 600;

export interface QuickSize {
  card: RefObject<HTMLDivElement | null>;
  list: RefObject<HTMLDivElement | null>;
  scroller: RefObject<HTMLDivElement | null>;
  input: RefObject<HTMLTextAreaElement | null>;
  text: string;
  /** Counts opens; each is answered with the height. */
  opens: number;
  shown: boolean;
  /** How tall the window can be on its screen. */
  room: number | null;
  /** Jev's suggestions and the search of what sessions said are in for what's typed. */
  answered: boolean;
  /** Who opens the panel while it's being laid out: the script that returns the height, or the page. */
  opening: MutableRefObject<'script' | 'page' | null>;
  /** The height last reported, which the opening script returns. */
  reported: MutableRefObject<number>;
}

export function useQuickSize({ card, list, scroller, input, text, opens, shown, room, answered, opening, reported }: QuickSize): void {
  // The window is as tall as the card, or an open menu where it reaches below the card.
  const measure = useCallback(() => {
    const c = card.current?.getBoundingClientRect();
    const pop = document.querySelector('.ca-picker')?.getBoundingClientRect();
    return Math.ceil(Math.max(c?.bottom ?? 0, pop?.bottom ?? 0) + MARGIN);
  }, [card]);

  // While you type, the list only grows: Jev's suggestions and what sessions said arrive a moment
  // after each key, and letting the list shrink in between made the panel bounce with every key.
  // Once typing pauses and the answers are in, it takes the height of what it shows.
  const [paused, setPaused] = useState(true);
  useEffect(() => {
    setPaused(false);
    const id = window.setTimeout(() => setPaused(true), SETTLE_MS);
    return () => window.clearTimeout(id);
  }, [text]);
  const settled = paused && answered;
  const held = useRef({ open: 0, height: 0 });
  useLayoutEffect(() => {
    const el = list.current;
    if (!el) return;
    el.style.minHeight = '';
    if (!text.trim() || settled || held.current.open !== opens) held.current = { open: opens, height: 0 };
    if (!text.trim() || settled) return;
    held.current.height = Math.max(held.current.height, el.offsetHeight);
    // Only as far as the screen has room: past that the list scrolls, as it does once settled.
    const spare = room && card.current ? room - 2 * MARGIN - card.current.offsetHeight : Infinity;
    el.style.minHeight = `${Math.min(held.current.height, el.offsetHeight + spare)}px`;
  });
  // A scrollbar, where macOS shows one, narrows the rows while the list scrolls; the new-session
  // row below it keeps in line with them.
  useLayoutEffect(() => {
    const el = scroller.current;
    list.current?.style.setProperty('--quick-gutter', `${el ? el.offsetWidth - el.clientWidth : 0}px`);
  });

  useLayoutEffect(() => {
    if (opens === 0) return;
    const h = measure();
    reported.current = h;
    // The cursor goes at the end of whatever the panel opens with: nothing, a todo's prompt, or
    // what a failed start left.
    const el = input.current;
    if (el) {
      el.focus();
      el.setSelectionRange(el.value.length, el.value.length);
    }
    if (opening.current !== 'script') void quickReady(h);
    // The refs are the same objects on every render: it runs as each open lays out.
  }, [opens, measure, input, opening, reported]);
  useLayoutEffect(() => {
    if (!shown || opens === 0) return;
    const h = measure();
    if (h === reported.current) return;
    reported.current = h;
    void fitQuick(h);
  });
}

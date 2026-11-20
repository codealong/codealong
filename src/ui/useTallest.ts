// An element that only grows while it's on the page: it keeps the tallest it has been as its
// least height, so what's laid out under it stays put when it has less to show.

import { useLayoutEffect, useRef, type RefObject } from 'react';

/** A ref for the element to hold. After every render it's measured as it would be by itself and
 *  given the tallest it has measured as its `min-height`; once it's off the page the next one
 *  starts afresh. The quick panel's Suggested group has it (Quick.tsx): an answer from Jev with
 *  fewer suggestions than the last leaves room where they were. */
export function useTallest<E extends HTMLElement>(): RefObject<E | null> {
  const held = useRef<E>(null);
  const tall = useRef(0);
  useLayoutEffect(() => {
    const el = held.current;
    if (!el) {
      tall.current = 0;
      return;
    }
    el.style.minHeight = '';
    tall.current = Math.max(tall.current, el.offsetHeight);
    el.style.minHeight = `${tall.current}px`;
  });
  return held;
}

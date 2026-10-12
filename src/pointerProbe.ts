// Where this page has the pointer, for the engine to compare with macOS and WebKit's drawing side
// (pointer.rs, whose header says why). While `[debug] perf_log` is on, each time the pointer comes
// to rest over the page it reports where it has it, what it finds there, and how far each list
// holding that is scrolled, with the list's last scroll positions.

import { useEffect } from 'react';
import { pointerProbe } from './api';
import type { PointerScroller } from './types';
import { pageZoom } from './zoom';

/** How long the pointer stays still before the page reports where it has it. */
const REST_MS = 500;
/** How many of a list's last scroll positions go with a report. */
const RECENT = 8;

function scrolls(el: Element): el is HTMLElement {
  if (!(el instanceof HTMLElement) || el.scrollHeight <= el.clientHeight) return false;
  const o = getComputedStyle(el).overflowY;
  return o === 'auto' || o === 'scroll';
}

/** What a report calls the thing at the pointer: its row's title, or the element. */
function nameAt(el: Element): string {
  const row = el.closest('[role="option"]');
  const title = row?.getAttribute('aria-label') || row?.querySelector('[class*="title"]')?.textContent;
  if (title) return title.slice(0, 60);
  return `${el.tagName.toLowerCase()}${el.classList[0] ? `.${el.classList[0]}` : ''}`;
}

export function usePointerProbe(on: boolean) {
  useEffect(() => {
    if (!on) return;
    let last: PointerEvent | null = null;
    let timer = 0;
    const recent = new WeakMap<Element, { at: number; top: number }[]>();
    const report = () => {
      const e = last;
      if (!e || document.hidden) return;
      const el = document.elementFromPoint(e.clientX, e.clientY);
      if (!el) return;
      const now = performance.now();
      const scrollers: PointerScroller[] = [];
      for (let n: Element | null = el; n; n = n.parentElement) {
        if (!scrolls(n)) continue;
        const lately = (recent.get(n) ?? []).map((r) => `${Math.round(r.top)}@-${Math.round(now - r.at)}ms`);
        scrollers.push({ name: n.classList[0] ?? n.tagName.toLowerCase(), top: n.scrollTop, height: n.scrollHeight, viewW: n.clientWidth, viewH: n.clientHeight, recent: lately });
      }
      // A report that doesn't arrive costs a line in the perf log, which is all it's for.
      pointerProbe({ x: e.clientX, y: e.clientY, age: now - e.timeStamp, zoom: pageZoom(), under: nameAt(el), scrollers }).catch(() => {});
    };
    const onMove = (e: PointerEvent) => {
      last = e;
      window.clearTimeout(timer);
      timer = window.setTimeout(report, REST_MS);
    };
    // Out of the page, the last place the page had the pointer is nowhere macOS has it.
    const onOut = (e: PointerEvent) => {
      if (e.relatedTarget !== null) return;
      last = null;
      window.clearTimeout(timer);
    };
    const onScroll = (e: Event) => {
      const t = e.target;
      if (!(t instanceof Element)) return;
      const list = recent.get(t) ?? [];
      list.push({ at: performance.now(), top: t.scrollTop });
      recent.set(t, list.slice(-RECENT));
    };
    window.addEventListener('pointermove', onMove, { capture: true, passive: true });
    window.addEventListener('pointerout', onOut, { capture: true, passive: true });
    // A list's scroll doesn't bubble, but it's heard on its way down.
    document.addEventListener('scroll', onScroll, { capture: true, passive: true });
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener('pointermove', onMove, { capture: true });
      window.removeEventListener('pointerout', onOut, { capture: true });
      document.removeEventListener('scroll', onScroll, { capture: true });
    };
  }, [on]);
}

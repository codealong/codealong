// What of an element shows, as the boxes around it cut it off: the finds mark what they found
// with boxes of their own, drawn in a layer over the text (SettingsFind.tsx, WindowFind.tsx), and
// a mark over text an ellipsis cut off, or a list scrolled out of sight, mustn't show.

import { scaleOf } from '../zoom';

/** The part of the page an element's ancestors leave in sight, in the page's coordinates. */
export interface Clip {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** No cut at all. */
export const ALL: Clip = { left: -Infinity, top: -Infinity, right: Infinity, bottom: Infinity };

/**
 * What of an element inside `root` its ancestors' overflow leaves in sight, where they cut it
 * off (an ellipsis, a list that scrolls), within `outer`, in the page's coordinates. With `fixed`,
 * what scrolling can bring into sight: what a box that scrolls holds reaches as far as its part
 * in sight stretched by how far it can still scroll each way, and what moves what's in it another
 * way (`data-find-pans`, the canvas) can show anything in it where some of it is in sight. Each
 * element's is worked out once a call.
 */
export function clipsIn(root: Element | null, outer: Clip = ALL, fixed = false): (el: Element | null) => Clip {
  const scrolls = (overflow: string) => overflow === 'auto' || overflow === 'scroll';
  const known = new Map<Element, Clip>();
  const clipOf = (el: Element | null): Clip => {
    if (!el || el === root) return outer;
    const was = known.get(el);
    if (was) return was;
    // A copy: the parent's is kept for its other children.
    const clip = { ...clipOf(el.parentElement) };
    const { overflowX, overflowY } = getComputedStyle(el);
    const pans = fixed && el.hasAttribute('data-find-pans');
    // Each way, what it does to what's in it: moves it (scrolls or pans, with `fixed`), cuts it
    // off at its box, or neither.
    const moveX = fixed && (pans || scrolls(overflowX));
    const moveY = fixed && (pans || scrolls(overflowY));
    const cutX = overflowX !== 'visible';
    const cutY = overflowY !== 'visible';
    if (moveX || moveY || cutX || cutY) {
      // Its rect is on the screen's scale, its own lengths on its own (a card's CSS zoom, the
      // canvas's camera): zoom.ts.
      const r = el.getBoundingClientRect();
      const s = el instanceof HTMLElement ? scaleOf(el) : 1;
      const left = Math.max(clip.left, r.left + el.clientLeft * s);
      const top = Math.max(clip.top, r.top + el.clientTop * s);
      const right = Math.min(clip.right, r.left + (el.clientLeft + el.clientWidth) * s);
      const bottom = Math.min(clip.bottom, r.top + (el.clientTop + el.clientHeight) * s);
      // Its part in sight, stretched by how far it can scroll back and on; none of it in sight
      // brings nothing in.
      const reach = (lo: number, hi: number, back: number, on: number): [number, number] => (hi <= lo ? [lo, lo] : pans ? [-Infinity, Infinity] : [lo - back * s, hi + on * s]);
      if (moveX) [clip.left, clip.right] = reach(left, right, el.scrollLeft, el.scrollWidth - el.clientWidth - el.scrollLeft);
      else if (cutX) [clip.left, clip.right] = [left, right];
      if (moveY) [clip.top, clip.bottom] = reach(top, bottom, el.scrollTop, el.scrollHeight - el.clientHeight - el.scrollTop);
      else if (cutY) [clip.top, clip.bottom] = [top, bottom];
    }
    known.set(el, clip);
    return clip;
  };
  return clipOf;
}

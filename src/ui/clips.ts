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
 * what no scrolling can bring into sight: what scrolls, or moves what's in it another way
 * (`data-find-pans`, the canvas), can show anything in it where some of it is left in sight, so
 * then only what cuts off inside it counts; one that can't scroll that way, or that's cut off
 * itself, cuts as any box does. Each element's is worked out once a call.
 */
export function clipsIn(root: Element | null, outer: Clip = ALL, fixed = false): (el: Element | null) => Clip {
  const scrolls = (overflow: string) => overflow === 'auto' || overflow === 'scroll';
  const known = new Map<Element, Clip>();
  const clipOf = (el: Element | null): Clip => {
    if (!el || el === root) return outer;
    const was = known.get(el);
    if (was) return was;
    let clip = clipOf(el.parentElement);
    const { overflowX, overflowY } = getComputedStyle(el);
    let x = overflowX !== 'visible';
    let y = overflowY !== 'visible';
    if (fixed) {
      const pans = el.hasAttribute('data-find-pans');
      const r = el.getBoundingClientRect();
      if ((pans || (scrolls(overflowX) && el.scrollWidth > el.clientWidth)) && r.right > clip.left && r.left < clip.right) [x, clip] = [false, { ...clip, left: -Infinity, right: Infinity }];
      if ((pans || (scrolls(overflowY) && el.scrollHeight > el.clientHeight)) && r.bottom > clip.top && r.top < clip.bottom) [y, clip] = [false, { ...clip, top: -Infinity, bottom: Infinity }];
    }
    if (x || y) {
      // Its rect is on the screen's scale, its own lengths on its own (a card's CSS zoom, the
      // canvas's camera): zoom.ts.
      const r = el.getBoundingClientRect();
      const s = el instanceof HTMLElement ? scaleOf(el) : 1;
      const left = r.left + el.clientLeft * s;
      const top = r.top + el.clientTop * s;
      clip = {
        left: x ? Math.max(clip.left, left) : clip.left,
        right: x ? Math.min(clip.right, left + el.clientWidth * s) : clip.right,
        top: y ? Math.max(clip.top, top) : clip.top,
        bottom: y ? Math.min(clip.bottom, top + el.clientHeight * s) : clip.bottom,
      };
    }
    known.set(el, clip);
    return clip;
  };
  return clipOf;
}

// What of an element shows, as the boxes around it cut it off: the finds mark what they found
// with boxes of their own, drawn in a layer over the text (SettingsFind.tsx, WindowFind.tsx), and
// a mark over text an ellipsis cut off, or a list scrolled out of sight, mustn't show.

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
 * off (an ellipsis, a list that scrolls), within `outer`, in the page's coordinates. Each
 * element's is worked out once a call.
 */
export function clipsIn(root: Element | null, outer: Clip = ALL): (el: Element | null) => Clip {
  const known = new Map<Element, Clip>();
  const clipOf = (el: Element | null): Clip => {
    if (!el || el === root) return outer;
    const was = known.get(el);
    if (was) return was;
    let clip = clipOf(el.parentElement);
    const { overflowX, overflowY } = getComputedStyle(el);
    if (overflowX !== 'visible' || overflowY !== 'visible') {
      const r = el.getBoundingClientRect();
      const left = r.left + el.clientLeft;
      const top = r.top + el.clientTop;
      clip = {
        left: overflowX === 'visible' ? clip.left : Math.max(clip.left, left),
        right: overflowX === 'visible' ? clip.right : Math.min(clip.right, left + el.clientWidth),
        top: overflowY === 'visible' ? clip.top : Math.max(clip.top, top),
        bottom: overflowY === 'visible' ? clip.bottom : Math.min(clip.bottom, top + el.clientHeight),
      };
    }
    known.set(el, clip);
    return clip;
  };
  return clipOf;
}

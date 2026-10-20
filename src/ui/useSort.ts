// Dragging things in a line into another order: the usage bar's chips sideways (UsageBar.tsx), a
// routing group's accounts up and down (SettingsDialog.tsx `RoutingPane`). The one held follows
// the pointer along the line, the others slide aside to make way for it, and letting go puts it
// there. A pointer drag, as `useRowDrag` is: clicks are left alone until the pointer has moved a
// few pixels, and Escape puts it back.

import { useEffect, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent, type RefObject } from 'react';
import { useLatest } from '../latest';
import { bare } from '../platform';

/** How far the pointer moves with the button down before it drags a thing rather than clicks it. */
const DRAG_AFTER = 4;

/** A thing's start and end along the line: its left and right across, its top and bottom down. */
export interface Edges {
  start: number;
  end: number;
}

/** Where a thing dragged from `from` lands with its middle at `middle`: after every other thing
 *  whose middle it has passed. */
export function dropPlace(edges: Edges[], from: number, middle: number): number {
  return edges.filter((e, i) => i !== from && middle > (e.start + e.end) / 2).length;
}

/** How far the thing at `i` moves aside while the one from `from` would land at `to`: one place,
 *  `by` (the dragged one's length and a gap), toward where the dragged one came from. */
export function shiftOf(i: number, from: number, to: number, by: number): number {
  if (from < i && i <= to) return -by;
  if (to <= i && i < from) return by;
  return 0;
}

/** `items` with the one at `from` moved to `to`. */
export function moved<T>(items: T[], from: number, to: number): T[] {
  const next = items.filter((_, i) => i !== from);
  next.splice(to, 0, items[from]);
  return next;
}

/** What a thing in the line gets for being dragged to another place, and for making way. */
export interface SortProps {
  onPointerDown: (e: ReactPointerEvent<HTMLElement>) => void;
  className?: string;
  style?: CSSProperties;
}

/** A thing being dragged: where it came from and where it would land. */
interface Sorting {
  from: number;
  to: number;
  /** How far the things it passes move aside: its length and the gap beside it. */
  by: number;
}

/**
 * Dragging the children of `line` that match `item` along `axis`. `onStart` is told as a press
 * becomes a drag; `onPlace` gets the items the drag started over in their new order, when it
 * left one in another place. While a drag lasts `sorting` is true, for the line's `is-sorting`
 * under which the others slide aside; the one held is `is-lifted`.
 */
export function useSort<T>(line: RefObject<HTMLElement | null>, item: string, axis: 'x' | 'y', onStart: () => void, onPlace: (items: T[]) => void) {
  const [sorting, setSorting] = useState<Sorting | null>(null);
  const stopNow = useRef<(() => void) | null>(null);
  const on = useLatest({ onStart, onPlace });
  useEffect(() => () => stopNow.current?.(), []);
  const across = axis === 'x';

  const start = (e: ReactPointerEvent<HTMLElement>, from: number, items: T[]) => {
    const el = line.current;
    // ⌘-, ⌃-, ⌥- and ⇧-clicks stay clicks.
    if (!el || e.button !== 0 || !bare(e)) return;
    stopNow.current?.();
    const held = e.currentTarget;
    // Places along the line from its start, so the line or what holds it scrolling under the
    // pointer moves the thing as the pointer moving would.
    const along = (cx: number, cy: number) => {
      const r = el.getBoundingClientRect();
      return across ? cx - r.left + el.scrollLeft : cy - r.top + el.scrollTop;
    };
    const x0 = e.clientX;
    const y0 = e.clientY;
    const at0 = along(x0, y0);
    let x = x0;
    let y = y0;
    // Measured along the line once it's a drag, before anything moves aside.
    let edges: Edges[] | null = null;
    let to = from;
    let by = 0;

    const follow = () => {
      if (!edges) return;
      const d = along(x, y) - at0;
      // It's drawn inside the line; where it lands goes by the pointer, so a thing longer than
      // the last one can still go after it.
      const lo = edges[0].start - edges[from].start;
      const hi = edges[edges.length - 1].end - edges[from].end;
      held.style.transform = `${across ? 'translateX' : 'translateY'}(${Math.min(hi, Math.max(lo, d))}px)`;
      const next = dropPlace(edges, from, (edges[from].start + edges[from].end) / 2 + d);
      if (next === to) return;
      to = next;
      setSorting({ from, to, by });
    };
    const move = (ev: PointerEvent) => {
      x = ev.clientX;
      y = ev.clientY;
      if (!edges) {
        if (Math.abs(x - x0) + Math.abs(y - y0) < DRAG_AFTER) return;
        edges = Array.from(el.querySelectorAll(`:scope > ${item}`), (c) => {
          const r = c.getBoundingClientRect();
          return across ? { start: along(r.left, 0), end: along(r.right, 0) } : { start: along(0, r.top), end: along(0, r.bottom) };
        });
        // One thing has nowhere to go.
        if (edges.length < 2) return stop();
        by = from < edges.length - 1 ? edges[from + 1].start - edges[from].start : edges[from].end - edges[from - 1].end;
        document.documentElement.classList.add('ca-dragging');
        on.current.onStart();
        setSorting({ from, to, by });
      }
      follow();
    };
    // The click a drag's release makes, over the thing it started on, isn't a click on it.
    const swallowClick = () => {
      const swallow = (c: MouseEvent) => {
        c.stopPropagation();
        c.preventDefault();
      };
      window.addEventListener('click', swallow, true);
      window.setTimeout(() => window.removeEventListener('click', swallow, true), 0);
    };
    const key = (ev: KeyboardEvent) => {
      if (ev.key !== 'Escape' || !edges) return;
      ev.preventDefault();
      ev.stopPropagation();
      stop();
      // The button is still down; letting go of it after is no click either.
      window.addEventListener('pointerup', swallowClick, { once: true });
    };
    const stop = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', end);
      window.removeEventListener('pointercancel', stop);
      window.removeEventListener('keydown', key, true);
      window.removeEventListener('scroll', follow, true);
      stopNow.current = null;
      held.style.transform = '';
      document.documentElement.classList.remove('ca-dragging');
      setSorting(null);
    };
    const end = () => {
      const dragged = !!edges;
      stop();
      if (!dragged) return;
      swallowClick();
      if (to !== from) on.current.onPlace(moved(items, from, to));
    };

    stopNow.current = stop;
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', end);
    window.addEventListener('pointercancel', stop);
    window.addEventListener('keydown', key, true);
    // The mouse wheel scrolls a line too long for its room, or what holds it, while a thing is
    // held. Scrolling doesn't bubble, so it's heard on its way down.
    window.addEventListener('scroll', follow, { capture: true, passive: true });
  };

  /** What the thing at `i` of `items` gets for being dragged, and for making way. */
  const sortProps = (i: number, items: T[]): SortProps => ({
    onPointerDown: (e) => start(e, i, items),
    className: sorting?.from === i ? 'is-lifted' : undefined,
    style: sorting && sorting.from !== i ? { transform: `${across ? 'translateX' : 'translateY'}(${shiftOf(i, sorting.from, sorting.to, sorting.by)}px)` } : undefined,
  });
  return { sorting: !!sorting, sortProps };
}

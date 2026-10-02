// Dragging rows: every list's rows, and the quick panel's results. A row dropped among the
// ordered rows at the top of its list (the pinned ones; a todo section's) lands at that place,
// which pins a row that wasn't pinned. A pinned row dropped on the rest of the list is unpinned.
// A row dragged out of the window is pinned to the desktop: its card follows the pointer from
// where it leaves (desktop.rs `drag_out`), and a pinned row that goes that way leaves its list's
// pins. A pointer drag, not HTML drag and drop: it behaves the same in every webview and leaves
// clicks alone until the pointer has moved a few pixels.

import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { dragToDesktop, onDesktopDragAway, onDesktopDragEnd } from '../api';
import type { DesktopPinned } from '../types';
import { cx, Icon } from './primitives';

/** What a draggable row needs. */
export interface DragProps {
  /** Position among the ordered rows, for a row that's one of them. */
  pinIndex?: number;
  onPointerDown: (e: ReactPointerEvent<HTMLElement>) => void;
  /** Marks the row being dragged and where it would land. */
  extraClass?: string;
}

/** How far the pointer moves with the button down before it drags the row rather than clicks it. */
const DRAG_AFTER = 4;
/** How far past the last ordered row a drop still lands after it. */
const SLACK = 20;
/** How near the list's top or bottom edge a drag scrolls it. */
const EDGE = 40;

/** A row's top and bottom in the window. */
export interface Span {
  top: number;
  bottom: number;
}

/**
 * Where a drop at height `y` lands: an insertion point among the `ordered` rows, or `'list'`,
 * the rest of the list. The ordered rows take drops down to a little past the last of them. A
 * row that isn't one of them joins them only when `joins`, and only from above its own place, so
 * a press that wobbles pins nothing; with no ordered rows yet, it joins from the top of the list
 * (above the middle of `first`, the first row).
 */
export function zoneAt(y: number, ordered: Span[], first: Span | null, source: Span, inOrder: boolean, joins: boolean): number | 'list' {
  if (!inOrder && !joins) return 'list';
  const last = ordered[ordered.length - 1];
  let limit: number;
  if (last) limit = last.bottom + Math.min(SLACK, (last.bottom - last.top) / 2);
  else if (first) limit = (first.top + first.bottom) / 2;
  else return 'list';
  if (!inOrder) limit = Math.min(limit, source.top);
  if (y >= limit) return 'list';
  const index = ordered.findIndex((r) => y < (r.top + r.bottom) / 2);
  return index < 0 ? ordered.length : index;
}

/** Where a drop would land: among the ordered rows, on the rest of the list, elsewhere in the
 *  window (nowhere), or outside the window (the desktop). */
type Zone = { at: 'order'; index: number } | { at: 'list' } | { at: 'none' } | { at: 'out' };

function sameZone(a: Zone, b: Zone): boolean {
  return a.at === b.at && (a.at !== 'order' || b.at !== 'order' || a.index === b.index);
}

export interface RowDragOptions {
  /** A row dropped among the ordered rows, at insertion point `index`: one of them moves there,
   *  another joins them there. */
  onPlace?: (key: string, index: number) => void;
  /** Rows that aren't among the ordered ones can be dropped among them (a pin). */
  joins?: boolean;
  /** An ordered row dropped on the rest of the list, or on the desktop, leaves them (an unpin). */
  onLeave?: (key: string) => void;
  /** What a row is on the desktop: dragged out of the window, it's pinned there. Null for a row
   *  that can't be. */
  desktop?: (key: string) => DesktopPinned | null;
  /** What the drag says it holds: the row's title. */
  label?: (key: string) => string;
}

/** A drag from the button going down until it comes up. */
interface Live {
  key: string;
  inOrder: boolean;
  row: HTMLElement;
  /** The rows it can be dropped among are looked for in here. */
  list: HTMLElement | null;
  /** What scrolls the list; a drop on the list is a drop in here. */
  scroller: HTMLElement | null;
  pin: DesktopPinned | null;
  x0: number;
  y0: number;
  x: number;
  y: number;
  /** The pointer has moved far enough: it's a drag. */
  active: boolean;
  /** The engine says the pointer has left the window. */
  away: boolean;
  zone: Zone;
  frame: number;
  late: number;
  stop: () => void;
}

function spanOf(el: Element): Span {
  const r = el.getBoundingClientRect();
  return { top: r.top, bottom: r.bottom };
}

/** The nearest element around `el` that scrolls up and down. */
function scrollerOf(el: HTMLElement | null): HTMLElement | null {
  for (let n = el; n; n = n.parentElement) {
    const o = getComputedStyle(n).overflowY;
    if (o === 'auto' || o === 'scroll') return n;
  }
  return null;
}

function inWindow(x: number, y: number): boolean {
  return x >= 0 && y >= 0 && x <= window.innerWidth && y <= window.innerHeight;
}

/**
 * Drags for the rows of one list: the element marked `data-drag-list` around them, where the
 * ordered rows carry a `data-pin-index`. Spread `rowProps` on each row and render `ghost`.
 */
export function useRowDrag(options: RowDragOptions) {
  const opts = useRef(options);
  opts.current = options;
  const live = useRef<Live | null>(null);
  // A row let go outside the window: the engine says whether its card stayed on the desktop.
  const out = useRef<{ key: string; inOrder: boolean; timer: number } | null>(null);
  const ghost = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<{ key: string; inOrder: boolean; zone: Zone } | null>(null);

  const canOrder = (d: Live) => d.inOrder || (!!opts.current.joins && !!opts.current.onPlace);

  /** Where a drop would land with the pointer where it is. */
  const locate = (d: Live): Zone => {
    if (d.away || !inWindow(d.x, d.y)) return d.pin ? { at: 'out' } : { at: 'none' };
    const area = (d.scroller ?? d.list)?.getBoundingClientRect();
    if (!d.list || !area || d.x < area.left || d.x > area.right || d.y < area.top || d.y > area.bottom || !canOrder(d)) return { at: 'none' };
    const ordered = [...d.list.querySelectorAll('[data-pin-index]')].map(spanOf);
    const first = d.list.querySelector('.ca-lane');
    const at = zoneAt(d.y, ordered, first ? spanOf(first) : null, spanOf(d.row), d.inOrder, !!opts.current.joins);
    return at === 'list' ? { at: 'list' } : { at: 'order', index: at };
  };

  /** Moves what the pointer holds to it, and shows where a drop would land. */
  const look = (d: Live, first = false) => {
    const zone = locate(d);
    if (ghost.current) ghost.current.style.transform = `translate(${d.x}px, ${d.y}px)`;
    if (!first && sameZone(zone, d.zone)) return;
    d.zone = zone;
    setDrag({ key: d.key, inOrder: d.inOrder, zone });
  };
  // The engine's word of a drag is listened for once (below), and reads the latest of it here.
  const lookNow = useRef(look);
  lookNow.current = look;

  /** Near the list's top or bottom edge, a drag that can land in the list scrolls it. */
  const scroll = () => {
    const d = live.current;
    if (!d?.active || !d.scroller) return;
    const r = d.scroller.getBoundingClientRect();
    if (d.x >= r.left && d.x <= r.right && d.y >= r.top && d.y <= r.bottom) {
      const up = r.top + EDGE - d.y;
      const down = d.y - (r.bottom - EDGE);
      const by = up > 0 ? -Math.ceil(up / 3) : down > 0 ? Math.ceil(down / 3) : 0;
      const before = d.scroller.scrollTop;
      if (by) d.scroller.scrollTop += by;
      if (d.scroller.scrollTop !== before) look(d);
    }
    d.frame = requestAnimationFrame(scroll);
  };

  const start = (e: ReactPointerEvent<HTMLElement>, key: string, index: number | null) => {
    // ⌘- and ⇧-clicks pick rows; they never start a drag. Nor does a press on a row's own controls.
    if (e.button !== 0 || e.metaKey || e.shiftKey || e.ctrlKey || (e.target as HTMLElement).closest('button, a, input, textarea, select')) return;
    const o = opts.current;
    const inOrder = index != null && !!o.onPlace;
    const pin = o.desktop?.(key) ?? null;
    if (!inOrder && !(o.joins && o.onPlace) && !pin) return;
    live.current?.stop();
    const row = e.currentTarget;
    const list = row.closest<HTMLElement>('[data-drag-list]');

    const move = (ev: PointerEvent) => {
      const d = live.current;
      if (!d) return;
      d.x = ev.clientX;
      d.y = ev.clientY;
      const first = !d.active;
      if (first) {
        if (Math.abs(d.x - d.x0) + Math.abs(d.y - d.y0) < DRAG_AFTER) return;
        d.active = true;
        document.documentElement.classList.add('ca-dragging');
        // The engine watches the pointer from here on, for when it leaves the window.
        if (d.pin) void dragToDesktop(d.pin).catch(() => {});
        if (d.scroller && canOrder(d)) d.frame = requestAnimationFrame(scroll);
      }
      look(d, first);
    };
    const stop = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', end);
      window.removeEventListener('pointercancel', stop);
      const d = live.current;
      if (d) {
        cancelAnimationFrame(d.frame);
        window.clearTimeout(d.late);
      }
      live.current = null;
      document.documentElement.classList.remove('ca-dragging');
      setDrag(null);
    };
    const end = (ev: PointerEvent) => {
      const d = live.current;
      stop();
      if (!d?.active) return;
      d.x = ev.clientX;
      d.y = ev.clientY;
      const zone = locate(d);
      // The click a drag ends in, when the button comes up over the row it went down on, is
      // not a click on the row.
      const swallow = (c: MouseEvent) => {
        c.stopPropagation();
        c.preventDefault();
      };
      window.addEventListener('click', swallow, true);
      window.setTimeout(() => window.removeEventListener('click', swallow, true), 0);
      if (zone.at === 'order') opts.current.onPlace?.(d.key, zone.index);
      else if (zone.at === 'list' && d.inOrder) opts.current.onLeave?.(d.key);
      else if (zone.at === 'out') {
        if (out.current) window.clearTimeout(out.current.timer);
        out.current = { key: d.key, inOrder: d.inOrder, timer: window.setTimeout(() => (out.current = null), 2000) };
      }
    };

    live.current = { key, inOrder, row, list, scroller: scrollerOf(list ?? row), pin, x0: e.clientX, y0: e.clientY, x: e.clientX, y: e.clientY, active: false, away: false, zone: { at: 'none' }, frame: 0, late: 0, stop };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', end);
    window.addEventListener('pointercancel', stop);
  };

  // The engine says when the button came up, and whether the card it made stayed on the
  // desktop. It's the only word of a drop the page didn't see: one outside its window.
  useEffect(
    () =>
      onDesktopDragEnd((kept) => {
        const o = out.current;
        if (o) {
          window.clearTimeout(o.timer);
          out.current = null;
          if (kept && o.inOrder) opts.current.onLeave?.(o.key);
          return;
        }
        const d = live.current;
        if (!d?.active || !d.pin) return;
        if (kept) {
          d.stop();
          if (d.inOrder) opts.current.onLeave?.(d.key);
        } else {
          // Let go over this window: its own pointerup, on its way, says where.
          window.clearTimeout(d.late);
          d.late = window.setTimeout(() => live.current === d && d.stop(), 600);
        }
      }),
    [],
  );
  // It also says when the pointer leaves the window and comes back, which the page may not
  // hear of: what it shows of the drag goes while the card is out there.
  useEffect(
    () =>
      onDesktopDragAway((away) => {
        const d = live.current;
        if (!d?.active || d.away === away) return;
        d.away = away;
        lookNow.current(d);
      }),
    [],
  );
  useEffect(() => () => live.current?.stop(), []);

  /** The drag props for the row with `key`: `index` is its place among the `count` ordered rows,
   *  null for a row that isn't one of them. */
  const rowProps = (key: string, index: number | null, count: number): DragProps => {
    const at = drag?.zone.at === 'order' ? drag.zone.index : -1;
    const extraClass = cx(drag?.key === key && 'is-drag-source', index != null && at === index && 'is-drop-before', index != null && index === count - 1 && at === count && 'is-drop-after');
    return { pinIndex: index ?? undefined, onPointerDown: (e) => start(e, key, index), extraClass: extraClass || undefined };
  };

  // What the pointer holds: the row's title, and what a drop here would do when that isn't plain.
  let held: ReactNode = null;
  if (drag) {
    const d = live.current;
    const act = drag.zone.at === 'order' && !drag.inOrder ? { icon: 'pin', word: 'Pin' } : drag.zone.at === 'list' && drag.inOrder && options.onLeave ? { icon: 'pin-off', word: 'Unpin' } : null;
    const title = options.label?.(drag.key);
    held = createPortal(
      <div ref={ghost} className={cx('ca-dragghost', drag.zone.at === 'out' && 'is-out', act && 'is-act')} style={{ transform: `translate(${d?.x ?? 0}px, ${d?.y ?? 0}px)` }} aria-hidden="true">
        {act && (
          <span className="ca-dragghost-act">
            <Icon name={act.icon} size={12} />
            {act.word}
          </span>
        )}
        {title && <span className="ca-dragghost-title">{title}</span>}
      </div>,
      document.body,
    );
  }

  return {
    rowProps,
    /** What the pointer holds while it drags: render it anywhere. */
    ghost: held,
    /** The insertion point among the ordered rows a drop would land at, if it would land there. */
    dropIndex: drag?.zone.at === 'order' ? drag.zone.index : null,
  };
}

export type RowDrag = ReturnType<typeof useRowDrag>;

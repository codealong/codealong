// A card that opens beside what the pointer rests on: a PR's link or chip (PrLink.tsx), a
// session's helpers chip (HelpersCard.tsx). It shows once the pointer has rested a moment, or at
// once while another has just closed, and takes the pointer: moving onto it keeps it up, so its
// rows and buttons can be used. Once something on it is pressed it stays up when the pointer
// leaves, so a merge's commit message can be written there and a menu from it used. One is up at
// a time: another's showing closes it, but not once it's been pressed, when the other waits. It
// goes on a key (which still does what it does), but not on a modifier alone or ⌘C, which copies
// what's selected on it (useDismiss.ts `keyLeavesHover`), nor one that goes to something on it
// but Escape; on a press elsewhere; and when the window goes to the background. It follows what
// it's beside as the page scrolls and as it grows, under it or over it, wherever there's more
// room, and stays on its side once pressed, so it doesn't jump from under the pointer. A
// right-click on it shows no menu but a text field's own, for copy and paste.

import { useEffect, useLayoutEffect, useRef, useState, type MouseEvent as ReactMouseEvent, type MutableRefObject, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { fieldAt } from '../contextMenu';
import { cx } from './primitives';
import { useDismiss } from './useDismiss';

const SHOW_MS = 350;
const HIDE_MS = 200;
/** After a card closes, another one shows at once for this long. */
const WARM_MS = 500;
/** The one card that's up, so a new one closes it, unless it's held. */
let shown: { close: () => void; held: () => boolean } | null = null;
let warmUntil = 0;
/** Room kept between the card and the window's edges, and between it and what it's beside. */
const EDGE = 8;
const GAP = 4;

/** A card's state, and the handlers what it's beside takes: `show` and `hide` on the pointer's
 *  coming and going, `close` when something on it is done. `hold` keeps it up as the pointer
 *  leaves, once something on it is pressed, until it closes; `held` says whether it is. */
export interface HoverCardState {
  anchor: MutableRefObject<HTMLElement | null>;
  open: boolean;
  show: () => void;
  hide: () => void;
  close: () => void;
  hold: () => void;
  held: () => boolean;
}

export function useHoverCard(): HoverCardState {
  const anchor = useRef<HTMLElement | null>(null);
  const [open, setOpen] = useState(false);
  const timer = useRef(0);
  const holding = useRef(false);
  const me = useRef({
    close: () => {
      holding.current = false;
      setOpen(false);
    },
    held: () => holding.current,
  });

  useEffect(
    () => () => {
      window.clearTimeout(timer.current);
      if (shown === me.current) shown = null;
    },
    [],
  );

  const show = () => {
    window.clearTimeout(timer.current);
    const go = () => {
      if (shown && shown !== me.current) {
        // What's being written on one pressed isn't lost to the pointer passing over another.
        if (shown.held()) return;
        shown.close();
      }
      shown = me.current;
      setOpen(true);
    };
    if (performance.now() < warmUntil) go();
    else timer.current = window.setTimeout(go, SHOW_MS);
  };
  const close = () => {
    window.clearTimeout(timer.current);
    holding.current = false;
    if (shown === me.current) {
      shown = null;
      warmUntil = performance.now() + WARM_MS;
    }
    setOpen(false);
  };
  const hide = () => {
    window.clearTimeout(timer.current);
    if (!holding.current) timer.current = window.setTimeout(close, HIDE_MS);
  };
  const hold = () => {
    window.clearTimeout(timer.current);
    holding.current = true;
  };
  return { anchor, open, show, hide, close, hold, held: () => holding.current };
}

/** The card itself, for a component that renders it only while `card.open`: in the page's body,
 *  beside `card.anchor`, scrolling within itself when the window is short of room. */
export function HoverCard({ card: c, label, className, children }: { card: HoverCardState; label: string; className?: string; children: ReactNode }) {
  const el = useRef<HTMLDivElement>(null);
  const anchor = c.anchor.current;
  /** Whether it was last placed under what it's beside. */
  const side = useRef<boolean | null>(null);

  const place = useRef(() => {});
  place.current = () => {
    const box = el.current;
    if (!box) return;
    const rects = anchor?.isConnected ? anchor.getClientRects() : null;
    if (!rects || rects.length === 0) return c.close();
    const first = rects[0];
    const last = rects[rects.length - 1];
    // Measured at its full height, which puts it back at its top: where it was scrolled to is
    // kept, so placing it again as it renders leaves it there.
    const scrolled = box.scrollTop;
    box.style.maxHeight = '';
    const natural = box.offsetHeight;
    const below = window.innerHeight - last.bottom - GAP - EDGE;
    const above = first.top - GAP - EDGE;
    const under = c.held() && side.current !== null ? side.current : natural <= below || below >= above;
    side.current = under;
    // As high as there's room for, so it grows as what's on it does, which the observer below
    // hears, and is placed again.
    const room = Math.max(0, under ? below : above);
    const height = Math.min(natural, room);
    const at = under ? last : first;
    box.style.maxHeight = `${room}px`;
    box.style.top = `${under ? last.bottom + GAP : first.top - GAP - height}px`;
    box.style.left = `${Math.max(EDGE, Math.min(at.left, window.innerWidth - box.offsetWidth - EDGE))}px`;
    box.scrollTop = scrolled;
  };
  useLayoutEffect(() => place.current());

  useDismiss(el, c.close, { inside: (t) => !!anchor?.contains(t), keys: 'any', blur: true });
  useEffect(() => {
    let frame = 0;
    // Scrolling within the card itself moves nothing it's placed by.
    const onMove = (e: Event) => {
      if (e.target instanceof Node && el.current?.contains(e.target)) return;
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => place.current());
    };
    window.addEventListener('scroll', onMove, true);
    window.addEventListener('resize', onMove);
    // What's on it changing its height (a PR's merge box read, a commit message to write): over
    // what it's beside, it grows upwards.
    const grown = new ResizeObserver(() => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => place.current());
    });
    if (el.current) grown.observe(el.current);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener('scroll', onMove, true);
      window.removeEventListener('resize', onMove);
      grown.disconnect();
    };
  }, []);

  // React passes a portal's events up to what renders it, a list's row say, which would select
  // itself, start dragging itself or show its own menu: presses, drags and right-clicks on the
  // card are the card's alone. It has no menu of its own, so a right-click away from its rows
  // shows none, and one in a text field the field's own, as main.tsx has it.
  const own = (e: ReactMouseEvent | ReactPointerEvent) => e.stopPropagation();
  const pressed = (e: ReactPointerEvent) => {
    own(e);
    c.hold();
  };
  return createPortal(
    <div
      ref={el}
      className={cx('ca-hovercard', className)}
      role="dialog"
      aria-label={label}
      onMouseEnter={c.show}
      onMouseLeave={c.hide}
      onPointerDown={pressed}
      onMouseDown={own}
      onClick={own}
      onDoubleClick={own}
      onContextMenu={(e) => {
        e.stopPropagation();
        if (!fieldAt(e.target)) e.preventDefault();
      }}
    >
      {children}
    </div>,
    document.body,
  );
}

// The bar docked in the notch (bar.rs): one black shape with the hardware notch at the top of the
// screen, always dark whatever the theme. Closed, it's the notch with a side on each hand: the
// mark on the left, and on the right the most urgent group and how many. Pointing at it for a
// moment, or clicking it, opens it down into the bar's pills; it closes a moment after the
// pointer leaves, unless the peek or a menu is up. The window is sized to the shape: grown before
// it opens and shrunk once it has closed, since a page takes the clicks on its clear parts too
// and a bigger window would block the menu bar under it.

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { fitBar, type Notch } from '../api';
import { NOTCH, notchCount, notchSizes, type Group, type Size } from '../bar';
import type { LogoStatus, Status } from '../types';
import { Mark, StatusGlyph } from './primitives';

/** How long the pointer rests on the closed notch before it opens, and is off it before it
 *  closes; and how long closing takes (app.css .ca-notch). */
const OPEN_AFTER_MS = 250;
const CLOSE_AFTER_MS = 400;
const CLOSE_MS = 320;
/** How long an open waits for the window to grow before drawing anyway. */
const GROW_WAIT_MS = 120;

const GLYPH: Record<Group, Status> = {
  needs: 'needs',
  unread: 'unread',
  working: 'working',
};
const WORD: Record<Group, string> = {
  needs: 'need you',
  unread: 'your turn',
  working: 'working',
};

export interface NotchShellProps {
  notch: Notch;
  logoStatus: LogoStatus;
  counts: Record<Group, number>;
  /** The project button, beside the mark while it's open. */
  project: ReactNode;
  /** Keeps it open: the peek is up, or a menu. */
  held: boolean;
  onMark: () => void;
  onMenu: () => void;
  /** The bar's pills, shown below while it's open. */
  children: ReactNode;
}

export function NotchShell({ notch, logoStatus, counts, project, held, onMark, onMenu, children }: NotchShellProps) {
  // Asked to open, so the window is at its open size; drawn open; drawing itself closed, with the
  // window still at its open size until it's done.
  const [want, setWant] = useState(false);
  const [open, setOpen] = useState(false);
  const [closing, setClosing] = useState(false);
  const inside = useRef(false);
  const heldNow = useRef(held);
  heldNow.current = held;
  const timer = useRef<number | null>(null);
  const body = useRef<HTMLDivElement>(null);
  const left = useRef<HTMLDivElement>(null);
  const right = useRef<HTMLDivElement>(null);
  const [natural, setNatural] = useState<{ body: Size; side: number }>({
    body: { w: 0, h: 0 },
    side: 0,
  });
  const sizes = notchSizes(notch, natural.body, natural.side);

  // What opens below, and what the sides hold open, measured at their own size whatever the
  // shape's size is now.
  useLayoutEffect(() => {
    const measure = () => {
      const b = body.current?.getBoundingClientRect();
      const side = Math.max(left.current?.scrollWidth ?? 0, right.current?.scrollWidth ?? 0);
      const next = {
        body: { w: Math.ceil(b?.width ?? 0), h: Math.ceil(b?.height ?? 0) },
        side: Math.ceil(side),
      };
      setNatural((was) => (was.body.w === next.body.w && was.body.h === next.body.h && was.side === next.side ? was : next));
    };
    measure();
    const observer = new ResizeObserver(measure);
    for (const el of [body.current, left.current, right.current]) if (el) observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // The window follows: open-sized from the moment an open is asked until it has drawn itself
  // closed again.
  const big = want || open || closing;
  const win = big ? sizes.openWindow : sizes.closedWindow;
  const reported = useRef('');
  useLayoutEffect(() => {
    const key = `${win.w}x${win.h}`;
    if (key === reported.current) return;
    reported.current = key;
    void fitBar(win.w, win.h);
  }, [win.w, win.h]);

  const clear = () => {
    if (timer.current != null) window.clearTimeout(timer.current);
    timer.current = null;
  };

  // Opens once the window has grown: WebKit says so with a resize, or it's drawn anyway.
  const ask = useCallback(() => {
    clear();
    setClosing(false);
    setWant(true);
    let done = false;
    const draw = () => {
      if (done) return;
      done = true;
      window.removeEventListener('resize', draw);
      setOpen(true);
    };
    window.addEventListener('resize', draw);
    window.setTimeout(draw, GROW_WAIT_MS);
  }, []);

  const close = useCallback(() => {
    clear();
    setOpen(false);
    setWant(false);
    setClosing(true);
    timer.current = window.setTimeout(() => {
      timer.current = null;
      setClosing(false);
    }, CLOSE_MS);
  }, []);

  const leaveSoon = useCallback(() => {
    clear();
    timer.current = window.setTimeout(() => {
      timer.current = null;
      if (!inside.current && !heldNow.current) close();
    }, CLOSE_AFTER_MS);
  }, [close]);

  // The peek or a menu going away with the pointer elsewhere closes it too.
  useEffect(() => {
    if (!held && (want || open) && !inside.current) leaveSoon();
  }, [held, want, open, leaveSoon]);

  useEffect(() => clear, []);

  const top = notchCount(counts);
  const shape = open ? sizes.open : sizes.closed;
  const all = (['needs', 'unread', 'working'] as Group[]).filter((g) => counts[g] > 0);

  return (
    <div
      className={['ca-notch', open && 'is-open', !notch.real && 'is-drawn'].filter(Boolean).join(' ')}
      style={
        {
          width: shape.w,
          height: shape.h,
          '--notch-w': `${notch.width}px`,
          '--notch-h': `${notch.height}px`,
          '--notch-ear': `${open ? NOTCH.ear.open : NOTCH.ear.closed}px`,
          '--notch-radius': `${open ? NOTCH.radius.open : NOTCH.radius.closed}px`,
        } as CSSProperties
      }
      onPointerEnter={() => {
        inside.current = true;
        clear();
        if (!want && !open) timer.current = window.setTimeout(ask, OPEN_AFTER_MS);
      }}
      onPointerLeave={() => {
        inside.current = false;
        clear();
        if (want || open) leaveSoon();
      }}
      onClick={() => {
        if (!want && !open) ask();
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        onMenu();
      }}
    >
      <span className="ca-notch-ear is-left" aria-hidden="true" />
      <span className="ca-notch-ear is-right" aria-hidden="true" />
      <div className="ca-notch-clip">
        <div className="ca-notch-top">
          <div className="ca-notch-side is-left">
            <div ref={left} className="ca-notch-side-in">
              {open ? (
                <button type="button" className="ca-hud-mark" title="Show Codealong" onClick={onMark}>
                  <Mark size={16} status={logoStatus} />
                </button>
              ) : (
                <span className="ca-notch-mark">
                  <Mark size={16} status={logoStatus} />
                </span>
              )}
              {open && project}
            </div>
          </div>
          <div className="ca-notch-cam" aria-hidden="true" />
          <div className="ca-notch-side is-right">
            <div ref={right} className="ca-notch-side-in">
              {open ? all.map((g) => <Count key={g} group={g} n={counts[g]} />) : top && <Count key={`${top.group}-${top.n}`} group={top.group} n={top.n} pulse={top.group === 'needs'} />}
            </div>
          </div>
        </div>
        <div ref={body} className="ca-notch-body" aria-hidden={!open}>
          {children}
        </div>
      </div>
    </div>
  );
}

/** A group's glyph and how many are in it. One that needs you pulses when it comes, and again
 *  when the number changes. */
function Count({ group, n, pulse }: { group: Group; n: number; pulse?: boolean }) {
  return (
    <span className={['ca-notch-count', `is-${group}`, pulse && 'is-pulse'].filter(Boolean).join(' ')} title={`${n} ${WORD[group]}`}>
      <StatusGlyph status={GLYPH[group]} title="" />
      <span>{n}</span>
    </span>
  );
}

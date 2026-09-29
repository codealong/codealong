// The bar docked in the notch (bar.rs): one black shape with the hardware notch at the top of the
// screen, always dark whatever the theme. Closed, it's the notch with a side on each hand: the
// mark on the left, and on the right the most urgent group and how many. Pointing at it for a
// moment, or clicking it, opens it down into the bar's pills; it closes a moment after the
// pointer leaves, unless the peek or a menu is up.
//
// The shape is laid out once at its open size and shown through a clip, so opening and closing
// animate the clip and the corners alone: nothing inside moves or lays out again, and the window
// keeps its size throughout (a window resized as it opens shows its old picture for a frame in
// the wrong place). The window takes clicks only on the shape, so Codealong watches the pointer
// and says when it comes onto the shape and leaves it (`notch-pointer`).

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { fitBar, notchShape, onNotchPointer, type Notch } from '../api';
import { NOTCH, notchClip, notchSizes, type Group, type NotchState, type Size } from '../bar';
import type { LogoStatus, Status } from '../types';
import { Mark, StatusGlyph } from './primitives';

/** How long the pointer rests on the closed notch before it opens, and is off it before it
 *  closes. */
const OPEN_AFTER_MS = 250;
const CLOSE_AFTER_MS = 400;

const GLYPH: Record<Group, Status> = { needs: 'needs', unread: 'unread', working: 'working' };
const WORD: Record<Group, string> = { needs: 'need you', unread: 'your turn', working: 'working' };
const ORDER: Group[] = ['needs', 'unread', 'working'];

export interface NotchShellProps {
  notch: Notch;
  logoStatus: LogoStatus;
  counts: Record<Group, number>;
  /** The widest the pills below can get. */
  room: number;
  /** The project button, beside the mark while it's open. */
  project: ReactNode;
  /** Keeps it open: the peek is up, or a menu. */
  held: boolean;
  onMark: () => void;
  onMenu: () => void;
  /** The bar's pills, shown below while it's open. */
  children: ReactNode;
}

export function NotchShell({ notch, logoStatus, counts, room, project, held, onMark, onMenu, children }: NotchShellProps) {
  const [open, setOpen] = useState(false);
  // The pointer is on the shape.
  const [on, setOn] = useState(false);
  const onNow = useRef(false);
  const heldNow = useRef(held);
  heldNow.current = held;
  const timer = useRef<number | null>(null);
  const body = useRef<HTMLDivElement>(null);
  const left = useRef<HTMLDivElement>(null);
  const right = useRef<HTMLDivElement>(null);
  const [natural, setNatural] = useState<{ body: Size; side: number }>({ body: { w: 0, h: 0 }, side: 0 });
  // The window's width, to place the shape at a whole point: centering by halves blurs it.
  const [viewW, setViewW] = useState(() => window.innerWidth);
  useEffect(() => {
    const resized = () => setViewW(window.innerWidth);
    window.addEventListener('resize', resized);
    return () => window.removeEventListener('resize', resized);
  }, []);
  const sizes = notchSizes(notch, natural.body, natural.side, room);
  const state: NotchState = open ? 'open' : on ? 'hover' : 'closed';
  const clip = notchClip(sizes, state);

  // What opens below, and what the sides hold open, measured at their own size.
  useLayoutEffect(() => {
    const measure = () => {
      const b = body.current?.getBoundingClientRect();
      const side = Math.max(left.current?.scrollWidth ?? 0, right.current?.scrollWidth ?? 0);
      const next = { body: { w: Math.ceil(b?.width ?? 0), h: Math.ceil(b?.height ?? 0) }, side: Math.ceil(side) };
      setNatural((was) => (was.body.w === next.body.w && was.body.h === next.body.h && was.side === next.side ? was : next));
    };
    measure();
    const observer = new ResizeObserver(measure);
    for (const el of [body.current, left.current, right.current]) if (el) observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // The window keeps one size; the shape it shows, where the window takes clicks, changes.
  const reported = useRef('');
  useLayoutEffect(() => {
    const key = `${sizes.window.w}x${sizes.window.h}`;
    if (key === reported.current) return;
    reported.current = key;
    void fitBar(sizes.window.w, sizes.window.h);
  }, [sizes.window.w, sizes.window.h]);
  const x = Math.round((viewW - sizes.open.w) / 2);
  const shownW = sizes.open.w - 2 * clip.side;
  const shownH = sizes.open.h - clip.bottom;
  useLayoutEffect(() => {
    void notchShape(x + clip.side, 0, shownW, shownH);
  }, [x, clip.side, shownW, shownH]);

  const clear = () => {
    if (timer.current != null) window.clearTimeout(timer.current);
    timer.current = null;
  };

  const leaveSoon = useCallback(() => {
    clear();
    timer.current = window.setTimeout(() => {
      timer.current = null;
      if (!onNow.current && !heldNow.current) setOpen(false);
    }, CLOSE_AFTER_MS);
  }, []);

  const enter = useCallback(() => {
    if (onNow.current) return;
    onNow.current = true;
    setOn(true);
    clear();
    timer.current = window.setTimeout(() => {
      timer.current = null;
      setOpen(true);
    }, OPEN_AFTER_MS);
  }, []);

  const leave = useCallback(() => {
    if (!onNow.current) return;
    onNow.current = false;
    setOn(false);
    leaveSoon();
  }, [leaveSoon]);

  // Codealong says where the pointer is; in a preview, the page's own events do.
  useEffect(() => onNotchPointer((isOn) => (isOn ? enter() : leave())), [enter, leave]);

  // The peek or a menu going away with the pointer elsewhere closes it too.
  useEffect(() => {
    if (!held && open && !onNow.current) leaveSoon();
  }, [held, open, leaveSoon]);

  useEffect(() => clear, []);

  const shown = ORDER.filter((g) => counts[g] > 0);
  const earScale = (open ? NOTCH.ear.open : NOTCH.ear.closed) / NOTCH.ear.open;

  return (
    <div
      className={['ca-notch', `is-${state}`, !notch.real && 'is-drawn'].filter(Boolean).join(' ')}
      style={{ left: x, width: sizes.open.w, height: sizes.open.h, '--notch-w': `${notch.width}px`, '--notch-h': `${notch.height}px` } as CSSProperties}
    >
      <span className="ca-notch-ear is-left" aria-hidden="true" style={{ transform: `translateX(${clip.side - NOTCH.ear.open}px) scale(${earScale})` }} />
      <span className="ca-notch-ear is-right" aria-hidden="true" style={{ transform: `translateX(${sizes.open.w - clip.side}px) scale(${earScale})` }} />
      <div
        className="ca-notch-shape"
        style={{ clipPath: `inset(0 ${clip.side}px ${clip.bottom}px ${clip.side}px round 0 0 ${clip.radius}px ${clip.radius}px)` }}
        onPointerEnter={enter}
        onPointerLeave={leave}
        onClick={() => {
          if (!open) {
            clear();
            setOpen(true);
          }
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          onMenu();
        }}
      >
        <div className="ca-notch-top">
          <div className="ca-notch-side is-left">
            <div ref={left} className="ca-notch-side-in">
              <button type="button" className="ca-hud-mark" title="Show Codealong" tabIndex={open ? 0 : -1} onClick={(e) => (open ? onMark() : e.preventDefault())}>
                <Mark size={16} status={logoStatus} />
              </button>
              {project && <span className="ca-notch-extra">{project}</span>}
            </div>
          </div>
          <div className="ca-notch-cam" aria-hidden="true" />
          <div className="ca-notch-side is-right">
            <div ref={right} className="ca-notch-side-in">
              {shown.map((g, i) => (
                <Count key={i === 0 ? `${g}-${counts[g]}` : g} group={g} n={counts[g]} extra={i > 0} pulse={i === 0 && g === 'needs'} />
              ))}
            </div>
          </div>
        </div>
        <div ref={body} className="ca-notch-body" style={{ left: Math.round((sizes.open.w - natural.body.w) / 2) }} aria-hidden={!open}>
          {children}
        </div>
      </div>
    </div>
  );
}

/** A group's glyph and how many are in it. The first beside the camera shows closed too; the
 *  others only open. One that needs you pulses when it comes, and again when the number changes. */
function Count({ group, n, extra, pulse }: { group: Group; n: number; extra?: boolean; pulse?: boolean }) {
  return (
    <span className={['ca-notch-count', `is-${group}`, extra && 'ca-notch-extra', pulse && 'is-pulse'].filter(Boolean).join(' ')} title={`${n} ${WORD[group]}`}>
      <StatusGlyph status={GLYPH[group]} title="" />
      <span>{n}</span>
    </span>
  );
}

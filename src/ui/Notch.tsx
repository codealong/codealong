// The bar docked in the notch (bar.rs): one black shape with the hardware notch at the top of the
// screen, always dark whatever the theme. It shows one of three things. Closed, it's the notch
// with a side on each hand: the mark on the left, and on the right everything that waits on you
// as one count (the sessions that need you, the PRs that wait on your review and the turns that
// ended, drawn as the glyphs of the kinds in it), else what's running. Pointing at it for a moment, or clicking
// it, opens it down into the bar's pills, with the project and each count on its own beside the
// camera; it closes a moment after the pointer leaves, unless a menu is up or its pin keeps it
// open. And a click opens it further, into a session under the pills, to read and answer
// (NotchSession.tsx): a click on a session's pill into that one, a click on the notch itself into
// the current one, and a second click on either
// puts it away. There's no peek beside a docked bar. The session's bottom corners and bottom edge
// drag it to a size of your own, which it keeps (a double-click on one gives it its usual size
// back); the notch stays centered, so a corner pulled out widens it as much on the other side. A session has the keyboard, and stays up whatever the
// pointer does until it's put away; one that lost the keyboard to another app stays while the
// pointer is on it.
//
// Everything is laid out once, at the biggest a session can be dragged to, and shown through a clip, so
// each state animates the clip and the corners alone: nothing inside moves or lays out again, and
// the window keeps its size throughout (a window resized as it opens shows its old picture for a
// frame in the wrong place). The window takes clicks only on the shape, so Codealong watches the
// pointer and says when it comes onto the shape and leaves it (`notch-pointer`).

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import { fitBar, notchShape, onNotchPointer, type Notch } from '../api';
import { NOTCH, NOTCH_ORDER, notchClip, notchDragged, notchSizes, notchSum, type Group, type NotchEdge, type NotchKind, type NotchState, type Size } from '../bar';
import type { Moment } from '../mark';
import type { LogoStatus } from '../types';
import { MarkLive } from './MarkLive';
import { cx, Icon, StatusGlyph, WaitsGlyph, WaitsGlyphs } from './primitives';
import { readStored, writeStored, type Codec } from '../stored';
import { zoomFactor } from '../zoom';

/** How long the pointer rests on the closed notch before it opens, and is off it before it
 *  closes. */
const OPEN_AFTER_MS = 250;
const CLOSE_AFTER_MS = 400;
/** Where the size you dragged the session to is kept: its width, and its height under the pills. */
const SIZE_KEY = 'codealong.notch.size';
/** How far an edge moves with the button down before it sizes the session rather than clicks. */
const SIZE_AFTER = 3;

/** The size you dragged the session to, as JSON; one that doesn't read is none. */
const SIZE_STORED: Codec<Size | null> = {
  parse: (raw) => {
    const v: unknown = JSON.parse(raw);
    if (!v || typeof v !== 'object') return null;
    const { w, h } = v as { w?: unknown; h?: unknown };
    return typeof w === 'number' && typeof h === 'number' && w > 0 && h > 0 ? { w, h } : null;
  },
  format: (size) => (size ? JSON.stringify(size) : null),
};

const WORD: Record<Group, string> = { needs: 'need you', unread: 'your turn', working: 'working' };

export interface NotchShellProps {
  notch: Notch;
  logoStatus: LogoStatus;
  /** What the mark plays as something happens (mark.ts). */
  moment: Moment | null;
  /** How many sessions are in each of the bar's groups, and how many open PRs wait on your
   *  review. */
  counts: Record<NotchKind, number>;
  /** The widest the pills below can get, and the screen, which a session fits. */
  room: number;
  screen: Size;
  /** The zoom, in percent: the session shown draws at it, and opens as much bigger (zoom.ts). */
  zoom?: number;
  /** The project button, beside the mark while it's open. */
  project: ReactNode;
  /** Keeps it open: a menu is up, or a pill is being dragged off it. */
  held: boolean;
  /** The session shown under the pills, while there is one and for a moment after, as it's put
   *  away; `sessionOpen` while it shows, and `sessionKeys` while it has the keyboard. */
  session: ReactNode;
  sessionOpen: boolean;
  sessionKeys: boolean;
  /** The pointer has been off a session without the keyboard for a moment, or the keyboard went
   *  with the pointer elsewhere: time to put it away. */
  onSessionLeft: () => void;
  /** A click on a group's count, open: its first session shows. */
  /** A click on a count beside the camera, open: its group's first session, or the first review. */
  onCount: (kind: NotchKind) => void;
  /** A click on the notch itself, off its pills and buttons: the current session shows, or the
   *  one showing is put away. Closed, a click anywhere on it is one. */
  onPress: () => void;
  /** Kept open until it's unpinned, whatever the pointer does. */
  keepOpen: boolean;
  onKeepOpen: (keep: boolean) => void;
  onMark: () => void;
  onMenu: () => void;
  /** The bar's pills, shown below while it's open. */
  children: ReactNode;
}

export function NotchShell({ notch, logoStatus, moment, counts, room, screen, zoom, project, held, session, sessionOpen, sessionKeys, onSessionLeft, onCount, onPress, keepOpen, onKeepOpen, onMark, onMenu, children }: NotchShellProps) {
  // The pills show, under the pointer or pinned; a session shows under them.
  const [open, setOpen] = useState(keepOpen);
  // The pointer is on the shape.
  const [on, setOn] = useState(false);
  const onNow = useRef(false);
  // A session with the keyboard holds it open as a menu does.
  const holding = held || (sessionOpen && sessionKeys);
  const heldNow = useRef(holding);
  heldNow.current = holding;
  const keepNow = useRef(keepOpen);
  keepNow.current = keepOpen;
  const sessionNow = useRef(sessionOpen);
  sessionNow.current = sessionOpen;
  const sessionLeft = useRef(onSessionLeft);
  sessionLeft.current = onSessionLeft;
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
  // The size you dragged the session to, if you have: its width, and its height under the pills.
  const [want, setWant] = useState<Size | null>(() => readStored(SIZE_KEY, SIZE_STORED, null));
  // An edge is being dragged: the shape follows the pointer, with no easing to catch up.
  const [sizing, setSizing] = useState(false);
  // The zoom changed: the shape takes its new size at once, as the card in it does, rather than
  // easing after a card that's already bigger. Known as the render that changes it is, so its
  // size and the easing's going come in one style change; back two frames on, once that's drawn.
  const [zoomDrawn, setZoomDrawn] = useState(zoom);
  const zooming = zoom !== zoomDrawn;
  useEffect(() => {
    if (!zooming) return;
    let second = 0;
    const first = requestAnimationFrame(() => {
      second = requestAnimationFrame(() => setZoomDrawn(zoom));
    });
    return () => {
      cancelAnimationFrame(first);
      cancelAnimationFrame(second);
    };
  }, [zooming, zoom]);
  const sizes = notchSizes(notch, natural.body, natural.side, room, screen, want, zoomFactor(zoom));
  const state: NotchState = sessionOpen ? 'session' : open ? 'pills' : on ? 'hover' : 'closed';
  const clip = notchClip(sizes, state);
  // Something shows under the notch.
  const below = state === 'pills' || state === 'session';

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
  const x = Math.round((viewW - sizes.full.w) / 2);
  const shownW = sizes.full.w - 2 * clip.side;
  const shownH = sizes.full.h - clip.bottom;
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
      if (onNow.current || heldNow.current) return;
      // A session still up here is one without the keyboard: it goes with the pointer.
      if (sessionNow.current) sessionLeft.current();
      if (!keepNow.current) setOpen(false);
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

  // A menu going away, or the pin coming out, with the pointer elsewhere closes it too; pinned,
  // it opens at once.
  useEffect(() => {
    if (keepOpen) {
      clear();
      setOpen(true);
    } else if (!holding && open && !onNow.current) leaveSoon();
  }, [holding, keepOpen, open, leaveSoon]);

  // A session shows under the pills, so they show with it. Put away with the pointer elsewhere,
  // the whole shape goes back into the notch in one move, unless the pin keeps the pills. Before
  // the page is drawn, so it never shows the pills alone on its way.
  const wasSession = useRef(sessionOpen);
  useLayoutEffect(() => {
    if (sessionOpen) {
      clear();
      setOpen(true);
    } else if (wasSession.current && !onNow.current && !keepNow.current && !heldNow.current) {
      clear();
      setOpen(false);
    }
    wasSession.current = sessionOpen;
  }, [sessionOpen]);

  // The keyboard went from a session (a click somewhere else, another app taking it) with the
  // pointer elsewhere too: it's put away at once. With the pointer still on it, it stays.
  const hadKeys = useRef(sessionKeys);
  useEffect(() => {
    if (sessionOpen && hadKeys.current && !sessionKeys && !onNow.current) sessionLeft.current();
    hadKeys.current = sessionKeys;
  }, [sessionOpen, sessionKeys]);

  useEffect(() => clear, []);

  // An edge of the session being dragged: where the pointer went down and the size it had then.
  const drag = useRef<{ edge: NotchEdge; x: number; y: number; from: Size; to: Size; moved: boolean; frame: number } | null>(null);
  const sizeNow = useRef<Size>({ w: 0, h: 0 });
  sizeNow.current = { w: sizes.session.w, h: sizes.session.h - sizes.pills.h };
  const startSize = (edge: NotchEdge) => (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    // The edge keeps the pointer while it runs ahead of it; a pointer that can't be kept (a
    // check's made-up one) still drags while it stays on the edge.
    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {
      // No such pointer to keep.
    }
    drag.current = { edge, x: e.screenX, y: e.screenY, from: sizeNow.current, to: sizeNow.current, moved: false, frame: 0 };
  };
  const moveSize = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    const [dx, dy] = [e.screenX - d.x, e.screenY - d.y];
    if (!d.moved && Math.abs(dx) + Math.abs(dy) < SIZE_AFTER) return;
    if (!d.moved) setSizing(true);
    d.moved = true;
    d.to = notchDragged(d.from, d.edge, dx, dy);
    if (d.frame) return;
    d.frame = requestAnimationFrame(() => {
      d.frame = 0;
      setWant(d.to);
    });
  };
  /** Let go, it keeps the size it has, which is what was asked for within what there's room for. */
  const endSize = () => {
    const d = drag.current;
    drag.current = null;
    if (!d?.moved) return;
    cancelAnimationFrame(d.frame);
    setSizing(false);
    const s = notchSizes(notch, natural.body, natural.side, room, screen, d.to, zoomFactor(zoom));
    const kept = { w: s.session.w, h: s.session.h - s.pills.h };
    setWant(kept);
    writeStored(SIZE_KEY, SIZE_STORED, kept);
  };
  /** A double-click on an edge gives the session its usual size back. */
  const resetSize = () => {
    setWant(null);
    writeStored(SIZE_KEY, SIZE_STORED, null);
  };
  const grip = (edge: NotchEdge) => (
    <div className={`ca-notch-grip is-${edge}`} aria-hidden="true" onPointerDown={startSize(edge)} onPointerMove={moveSize} onPointerUp={endSize} onPointerCancel={endSize} onDoubleClick={resetSize} />
  );

  const shown = NOTCH_ORDER.filter((k) => counts[k] > 0);
  // Closed, what waits of more than one kind shows as one count, in place of the counts that add
  // up to it; one kind alone is its own count, the first, which stays where it is as it opens.
  const sum = notchSum(counts);
  const merged = sum && sum.kinds.length > 1 ? sum : null;
  // The corners move and grow by their place and size, as the clip does, not by a transform:
  // WebKit runs a transform's animation apart from the page's, and the corners fell out of step
  // with the shape. Each reaches 3 points under the shape's edge: where two antialiased edges
  // meet at a fraction of a pixel as they move, the menu bar would show through between them.
  const ear = below ? NOTCH.ear.open : NOTCH.ear.closed;
  const under = 3;

  return (
    <div
      className={cx('ca-notch', `is-${state}`, below && 'is-open', sizing && 'is-sizing', zooming && 'is-zooming', !notch.real && 'is-drawn')}
      style={{ left: x, width: sizes.full.w, height: sizes.full.h, '--notch-w': `${notch.width}px`, '--notch-h': `${notch.height}px` } as CSSProperties}
    >
      <span
        className="ca-notch-shadow"
        aria-hidden="true"
        style={{ left: clip.side, right: clip.side, bottom: clip.bottom, borderRadius: `0 0 ${clip.radius}px ${clip.radius}px` }}
      />
      <span className="ca-notch-ear is-left" aria-hidden="true" style={{ left: clip.side - ear, width: ear + under, height: ear }} />
      <span className="ca-notch-ear is-right" aria-hidden="true" style={{ left: sizes.full.w - clip.side - under, width: ear + under, height: ear }} />
      <div
        className="ca-notch-shape"
        style={{ clipPath: `inset(0 ${clip.side}px ${clip.bottom}px ${clip.side}px round 0 0 ${clip.radius}px ${clip.radius}px)` }}
        onPointerEnter={enter}
        onPointerLeave={leave}
        onClick={(e) => {
          // What's in it has clicks of its own once it shows.
          if (open && e.target instanceof Element && e.target.closest('button, a, input, textarea, [role="button"], .ca-notch-session')) return;
          clear();
          setOpen(true);
          onPress();
        }}
        onContextMenu={(e) => {
          // The session has its own menu, and its box the one for copy and paste.
          if (e.defaultPrevented || (e.target instanceof Element && e.target.closest('.ca-notch-session'))) return;
          e.preventDefault();
          onMenu();
        }}
      >
        <div className="ca-notch-top">
          <div className="ca-notch-side is-left">
            <div ref={left} className="ca-notch-side-in">
              <button type="button" className="ca-hud-mark" title="Show Codealong" tabIndex={open ? 0 : -1} onClick={(e) => (open ? onMark() : e.preventDefault())}>
                <MarkLive size={16} status={logoStatus} moment={moment} />
              </button>
              {project && <span className="ca-notch-extra">{project}</span>}
            </div>
          </div>
          <div className="ca-notch-cam" aria-hidden="true" />
          <div className="ca-notch-side is-right">
            <div ref={right} className="ca-notch-side-in">
              {merged && (
                <span key={`sum-${counts.needs}`} className={cx('ca-notch-count', 'ca-notch-sum', counts.needs > 0 && 'is-pulse')} title={merged.kinds.map((k) => countWords(k, counts[k])).join(', ')}>
                  <WaitsGlyphs waits={merged.kinds} />
                  <span>{merged.n}</span>
                </span>
              )}
              {shown.map((k, i) => (
                <Count key={i === 0 && !merged ? `${k}-${counts[k]}` : k} kind={k} n={counts[k]} extra={i > 0 || !!merged} pulse={i === 0 && !merged && k === 'needs'} open={open} onPick={onCount} />
              ))}
              <button
                type="button"
                className={cx('ca-notch-pin', 'ca-notch-extra', keepOpen && 'is-on')}
                title={keepOpen ? 'Let it close' : 'Keep it open'}
                aria-label={keepOpen ? 'Let the notch close' : 'Keep the notch open'}
                aria-pressed={keepOpen}
                tabIndex={open ? 0 : -1}
                onClick={(e) => {
                  // Closed, it's out of sight: the click opens the notch, as anywhere on it.
                  if (!open) return;
                  e.stopPropagation();
                  onKeepOpen(!keepOpen);
                }}
              >
                <Icon name={keepOpen ? 'pinned' : 'pin'} size={14} />
              </button>
            </div>
          </div>
        </div>
        <div ref={body} className="ca-notch-body" style={{ left: Math.round((sizes.full.w - natural.body.w) / 2) }} aria-hidden={!open}>
          {children}
        </div>
        {/* The session, under the pills: as wide as the shape and as tall as it opens, with the
            edges that size it over its foot. */}
        <div className="ca-notch-session" style={{ top: sizes.pills.h, left: (sizes.full.w - sizes.session.w) / 2, width: sizes.session.w, height: sizes.session.h - sizes.pills.h }} aria-hidden={!sessionOpen}>
          {session}
          {grip('bottom')}
          {grip('left')}
          {grip('right')}
        </div>
      </div>
    </div>
  );
}

/** How many of a kind, in words. */
function countWords(kind: NotchKind, n: number): string {
  return kind === 'reviews' ? (n === 1 ? '1 PR waits on your review' : `${n} PRs wait on your review`) : `${n} ${WORD[kind]}`;
}

/** A group's glyph, or the PR icon for reviews asked of you, and how many there are. The first
 *  beside the camera shows closed too, unless what waits is counted as one there; the others only
 *  open. One that needs you pulses when it comes, and again when the number changes, as the one
 *  count does while it has any. Open, a count is a button: its group's first
 *  session, or the first PR waiting on your review, shows under the pills, the way into working
 *  through them. */
function Count({ kind, n, extra, pulse, open, onPick }: { kind: NotchKind; n: number; extra?: boolean; pulse?: boolean; open: boolean; onPick: (kind: NotchKind) => void }) {
  const className = cx('ca-notch-count', `is-${kind}`, extra && 'ca-notch-extra', pulse && 'is-pulse');
  const words = countWords(kind, n);
  return (
    <button
      type="button"
      className={className}
      title={`${words}${open ? '. Click to show the first.' : ''}`}
      tabIndex={open ? 0 : -1}
      onClick={(e) => {
        // Closed, the click opens the notch, as anywhere on it.
        if (!open) return;
        e.stopPropagation();
        onPick(kind);
      }}
    >
      {kind === 'working' ? <StatusGlyph status="working" title="" /> : <WaitsGlyph waits={kind} />}
      <span>{n}</span>
    </button>
  );
}

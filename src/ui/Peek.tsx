// The floating bar's peek (peek.rs has its window): the session whose pill is under the pointer,
// with the header its panel has in the main window, its conversation, and the box that answers it
// (SessionCard.tsx, which the waiting window shows too). A click in it gives it the
// keyboard; ⏎ sends and the peek closes at once, so the app you were in has the keyboard back (a
// message that doesn't go says so on the bar, and is back in the box). Once an answer went
// through it closes too, but stays while the box holds something for you to send next. Escape
// closes it too, and what you'd typed stays with the session (drafts.ts), here and in its panels,
// until you send it. Its header's ✓ marks a finished turn seen; its ⋯ button, or a right-click,
// has the session's menu (menus.ts), where snoozing and archiving it are, either of which takes it
// off the bar, as marking it seen does. The Sessions list's shortcuts work on it too (⌘E, ⌃⌘A,
// ⌃⌘R, ⌃⌘P). Rename in that menu, or in a pill's, shows the session's name ready to change; ⏎
// renames it and the peek closes. While it's up, the
// session follows the board, and the peek stays on it as its pill moves along the bar.
//
// Its header's × puts it away, as Escape does. The pin there keeps the session on the desktop,
// and so does dragging the peek's right edge, its far edge or the corner between them to make it
// bigger, or dragging it somewhere else by its header: the session gets a window of its own where
// the peek is let go (kept.rs), which this page draws too (`kept`). There it stays up whatever
// you do: sending leaves it open, its header drags it, its edges size it, and its × (or ⌘W)
// takes it off the desktop. It comes back at launch where it was left.
//
// A session's side conversation popped out (kept.rs `pop_side`) is a kept window too, which this
// page draws (`side`) as the side conversation and the box that asks it (`SideReply`), dragged by
// the strip's top line and sized by its edges as a kept session is. Its × and ⌘W end the side
// conversation and close the window; Put back closes the window alone.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { barNote, getBoard, inPreview, keepSession, keptClose, keptFocus, keptReady, keptResize, keptSession, onKeptKeys, onPeekHidden, onPeekOpen, onPeekShown, peekDrag, peekFit, peekFocus, peekHide, peekOpen, peekReady, peekResize, sideClose } from '../api';
import { renameAhead } from '../ahead';
import { readDraft } from '../drafts';
import { followBoard } from '../followBoard';
import { useLatest } from '../latest';
import { endSide } from '../side';
import { homeOf } from '../lanes';
import { attentionRows, ALL_PROJECTS } from '../quick';
import type { Board, Lane, PeekOpen, Session } from '../types';
import { useKeyPerf } from '../perf';
import { useCardHost, useLiveSession } from './cardHost';
import { SessionCard, type WindowCan } from './SessionCard';
import { SideReply } from './SessionReply';
import { useTheme } from './ThemeSwitch';
import { useNow } from './useNow';
import { errorText } from '../report';
import { cardZoom, zoomFactor } from '../zoom';
import { cx } from './primitives';

/** Room around the card for its shadow (`--shadow-peek`, which falls away from the bar): the
 *  window (peek.rs) is the card, this on its sides, `FAR` on the side away from the bar, and
 *  `NEAR` on the side toward the bar, where it mustn't cover the bar's pills. */
const MARGIN = 20;
const FAR = 32;
const NEAR = 8;
/** How far an edge moves with the button down before it sizes the card rather than clicks. */
const SIZE_AFTER = 3;
/** How far the header moves with the button down before it drags the window. */
const DRAG_AFTER = 4;
/** The edges that size the card: its right edge, its far edge (the one away from the bar), and
 *  the corner between them. */
type Edge = 'side' | 'far' | 'corner';
/** How long an open waits for the conversation before showing what it has. */
const LOAD_WAIT_MS = 250;
/** Where a preview (scripts/preview.sh) says which session to peek at. */
const PREVIEW_KEY = 'codealong.preview.peek';

/** A preview has no engine to open the peek: it opens on the session named for it, else the first
 *  one that needs you. */
async function previewOpen(): Promise<PeekOpen | null> {
  const board = await getBoard();
  let key: string | null = null;
  try {
    key = window.localStorage.getItem(PREVIEW_KEY);
  } catch {
    // No storage: the first session that needs you.
  }
  key = key || attentionRows(board, ALL_PROJECTS).rows.find((r) => r.kind === 'session')?.key || null;
  const home = homeOf(board.lanes, key);
  if (!home) return null;
  const { lane, session } = home;
  return { id: 1, lane, session, terminal: board.apps.terminal, apps: board.apps, theme: board.theme, zoom: board.zoom, room: Math.round(580 * zoomFactor(board.zoom)) + FAR + NEAR, below: true, focus: false, rename: false };
}

/** The peek, or with `kept` a session kept on the desktop, or with `side` too a session's side
 *  conversation in a window of its own. */
export function Peek({ kept = false, side = false }: { kept?: boolean; side?: boolean }) {
  const [open, setOpen] = useState<PeekOpen | null>(null);
  // A kept session the board no longer has: there's only its window to close.
  const [gone, setGone] = useState(false);
  // You dragged an edge, or the header: the card fills the window, which keeps the size you gave
  // it, or the one it had.
  const [sized, setSized] = useState(false);
  const [shown, setShown] = useState(false);
  const [engaged, setEngaged] = useState(false);
  // The session whose name the peek's own menu asked to change.
  const [renameAsked, setRenameAsked] = useState<string | null>(null);
  const now = useNow(30_000);
  const card = useRef<HTMLDivElement>(null);
  // The open answered with its height, and the height last reported after it.
  const answered = useRef<number | null>(null);
  const reported = useRef(0);
  // The session whose conversation is laid out.
  const loadedView = useRef<string | null>(null);

  useEffect(() => {
    if (kept) return;
    const offOpen = onPeekOpen((o) => {
      setOpen(o);
      setRenameAsked(null);
      setShown(true);
      setSized(false);
      setEngaged(o.focus);
    });
    const offHidden = onPeekHidden(() => {
      setShown(false);
      setEngaged(false);
      setSized(false);
      loadedView.current = null;
    });
    const offShown = onPeekShown((s) => setEngaged(s.engaged));
    if (inPreview) {
      void previewOpen().then((o) => {
        if (!o) return;
        setOpen(o);
        setShown(true);
      });
    }
    return () => {
      offOpen();
      offHidden();
      offShown();
    };
  }, [kept]);

  // A kept session's window knows which session it keeps; the board says the rest, once it has
  // read the session (at launch, the window is there before the first scan is done).
  useEffect(() => {
    if (!kept) return;
    let stop = () => {};
    let over = false;
    const offKeys = onKeptKeys(setEngaged);
    void (inPreview ? previewOpen().then((o) => o?.session.key ?? null) : keptSession()).then((key) => {
      if (over) return;
      if (!key) {
        setGone(true);
        void keptReady();
        return;
      }
      const take = (b: Board) => {
        const home = homeOf(b.lanes, key);
        if (home) {
          setGone(false);
          setShown(true);
          setOpen((o) => o ?? { id: 1, lane: home.lane, session: home.session, terminal: b.apps.terminal, apps: b.apps, theme: b.theme, zoom: b.zoom, room: 0, below: true, focus: false, rename: false });
          stop();
        } else if (!b.loading) {
          setGone(true);
          void keptReady();
        }
      };
      // Read once, then taken push by push (followBoard.ts): a read that didn't come back is made
      // up for by the next push.
      stop = followBoard(take).off;
    });
    return () => {
      over = true;
      stop();
      offKeys();
    };
  }, [kept]);

  // The session as the board has it now, while the peek is up, and the board's PRs, worktrees and
  // todos for what its header says around it (cardHost.ts). Each open tells of the session
  // afresh.
  const key = open?.session.key ?? null;
  const told = useMemo(() => (open ? { lane: open.lane, session: open.session } : null), [open]);
  const live = useLiveSession(told, shown);
  const { lane, session } = live;
  const theme = live.theme ?? open?.theme ?? 'system';
  useTheme(theme);
  // The card draws at the zoom, which the open said and the board keeps up (zoom.ts); the peek's
  // window opens as much bigger (peek.rs `place`).
  const zoom = live.zoom ?? open?.zoom;

  const measure = () => Math.ceil((card.current?.getBoundingClientRect().height ?? 0) + FAR + NEAR);

  /** Answers the open with the page's height, once: when the conversation is laid out, or after a
   *  moment without it. A kept session's window has its size already, and shows. */
  const ready = useCallback(() => {
    if (!open || answered.current === open.id) return;
    answered.current = open.id;
    if (kept) {
      void keptReady();
      return;
    }
    reported.current = measure();
    void peekReady(open.id, reported.current);
  }, [open, kept]);
  useEffect(() => {
    if (!open) return;
    const id = window.setTimeout(ready, LOAD_WAIT_MS);
    return () => window.clearTimeout(id);
  }, [open, ready]);

  // An open for the session already shown (Reply here on the pill it shows) answers at once; a
  // hidden peek shows nothing, so each showing reads the conversation afresh.
  const view = open?.session.key ?? '';
  const loaded = () => {
    loadedView.current = view;
    ready();
  };
  useLayoutEffect(() => {
    if (loadedView.current === view) ready();
  }, [open, view, ready]);

  // Once shown, the window follows the card: new steps, a longer draft, an approval coming up.
  // Not once you've given it a size, which it keeps.
  useLayoutEffect(() => {
    const el = card.current;
    if (!el || kept || sized) return;
    const observer = new ResizeObserver(() => {
      if (!open || answered.current !== open.id) return;
      const h = measure();
      if (h === reported.current) return;
      reported.current = h;
      void peekFit(h);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [open, shown, kept, sized]);

  // With the perf log on, how long typed keys take to show.
  const keyed = useKeyPerf(open?.perfLog ?? false);
  useEffect(() => {
    window.addEventListener('keydown', keyed, true);
    return () => window.removeEventListener('keydown', keyed, true);
  }, [keyed]);

  /** Ends a popped-out side conversation and closes its window, for its × and ⌘W. */
  const closeSide = useLatest(() => {
    if (key) endSide(key, sideClose);
    void keptClose();
  });

  // Escape closes it, and ⌘Q, ⌘H and ⌘W close it rather than going to Codealong: the menu bar is
  // the other app's. A kept session stays up for all of them but ⌘W, which takes it off the
  // desktop, and a side conversation's window for ⌘W, which ends it too. Typing with nothing
  // focused goes to the box.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      if (e.key === 'Escape' && !kept) {
        e.preventDefault();
        void peekHide();
        return;
      }
      if (e.metaKey && !e.ctrlKey && !e.altKey && ['q', 'h', 'w'].includes(e.key.toLowerCase())) {
        e.preventDefault();
        if (!kept) void peekHide();
        else if (e.key.toLowerCase() === 'w') {
          if (side) closeSide.current();
          else void keptClose();
        }
        return;
      }
      const el = document.activeElement;
      if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && (!el || el === document.body)) {
        document.querySelector<HTMLTextAreaElement>('.ca-peek .ca-composer textarea')?.focus();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [kept, side, closeSide]);

  /** Puts the peek away. A session kept on the desktop stays where it is. */
  const away = () => {
    if (!kept) void peekHide();
  };

  // An edge being dragged: where the pointer went down, and the window's size then.
  const sizing = useRef<{ edge: Edge; x: number; y: number; w: number; h: number; moved: boolean; frame: number; to: [number, number] } | null>(null);
  const below = open?.below ?? true;
  const startSize = (edge: Edge) => (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    // With the keyboard it stays up while the pointer runs ahead of its edge, where a peek up
    // from a hover would take the pointer for gone.
    if (!engaged) void (kept ? keptFocus() : peekFocus());
    sizing.current = { edge, x: e.screenX, y: e.screenY, w: window.innerWidth, h: window.innerHeight, moved: false, frame: 0, to: [window.innerWidth, window.innerHeight] };
  };
  const moveSize = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = sizing.current;
    if (!d) return;
    const [dx, dy] = [e.screenX - d.x, e.screenY - d.y];
    if (!d.moved && Math.abs(dx) + Math.abs(dy) < SIZE_AFTER) return;
    if (!d.moved) setSized(true);
    d.moved = true;
    // The far edge is the bottom under the bar and in a kept window, the top over the bar.
    d.to = [d.edge === 'far' ? d.w : d.w + dx, d.edge === 'side' ? d.h : d.h + (below ? dy : -dy)];
    if (d.frame) return;
    d.frame = requestAnimationFrame(() => {
      d.frame = 0;
      void (kept ? keptResize(d.to[0], d.to[1]) : peekResize(d.to[0], d.to[1]));
    });
  };
  /** Letting go of an edge that made the peek bigger keeps it on the desktop at that size. */
  const endSize = () => {
    const d = sizing.current;
    sizing.current = null;
    if (!d?.moved) return;
    cancelAnimationFrame(d.frame);
    void (kept ? keptResize(d.to[0], d.to[1]) : peekResize(d.to[0], d.to[1]).then(keepSession)).catch((e: unknown) => host.report.error(errorText(e)));
  };

  // The header drags the window, as a card's face drags a card: a kept session's as any window
  // is dragged, and the peek's by the engine (peek.rs), which takes it where the pointer goes and
  // keeps it on the desktop where it's let go.
  const moving = useRef<{ x: number; y: number } | null>(null);
  const startMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const on = e.target instanceof Element ? e.target : null;
    moving.current = e.button === 0 && on?.closest('.ca-sessinsp-head, .ca-sidestrip-head') && !on.closest('button, a, input, [role="button"]') ? { x: e.screenX, y: e.screenY } : null;
  };
  const moveWindow = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = moving.current;
    if (!d || !(e.buttons & 1)) return;
    const [dx, dy] = [e.screenX - d.x, e.screenY - d.y];
    if (Math.abs(dx) + Math.abs(dy) < DRAG_AFTER) return;
    moving.current = null;
    if (kept) {
      void getCurrentWindow().startDragging();
      return;
    }
    // It keeps the size it has from here, whatever its card comes to hold.
    setSized(true);
    void peekDrag(dx, dy);
  };

  // What's around the session shows in the main window, and what comes of an open or a menu is
  // said on the bar by the peek's own session (cardHost.ts).
  const host = useCardHost({ sessionKey: key, live, apps: open?.apps, terminal: open?.terminal ?? 'Terminal', away });

  // A kept session's window is never empty: a clear window would take the clicks meant for what's
  // under it. Until the board has read the session it says so, and so it does when the board has
  // lost it (aged out, its transcript deleted); either way it can be closed.
  if (kept && !open) {
    return (
      <div className="ca-peek-root is-kept" style={{ padding: `${NEAR}px ${MARGIN}px ${FAR}px` }}>
        <div className="ca-peek ca-peek-gone" onPointerDown={() => void keptFocus()}>
          <p>{gone ? 'This session isn’t on the board any more.' : 'Reading sessions…'}</p>
          <button type="button" className="ca-btn ca-btn--sm" onClick={() => void keptClose()}>
            Close
          </button>
        </div>
      </div>
    );
  }
  if (!open || !lane || !session || !shown) return null;

  const { report } = host;

  /** Takes the session off the bar, and puts the peek away. */
  const dismiss = (act: Promise<void>) => {
    away();
    act.catch((e: unknown) => report.error(errorText(e)));
  };

  /** Renames the session, or leaves its name, and puts the peek away: that's what it opened for. */
  const renamed = (name: string | null) => {
    const here = renameAsked === session.key && !open.rename;
    setRenameAsked(null);
    if (name) dismiss(renameAhead(session.key, name));
    // Asked from the peek's own menu, leaving the name goes back to the conversation.
    else if (!here) away();
  };

  /** What the peek does for its session's actions (SessionCard.tsx has the rest): whatever takes
   *  the session off the bar puts the peek away. */
  const can: WindowCan = { report, open: host.open, reveal: () => host.showSession(lane, session), rename: () => setRenameAsked(session.key), left: away };
  /** A session it came from, or that came of it, takes the peek's place; from a kept session it
   *  shows in Codealong, and the kept one stays. Its menu is its own. */
  const showKin = (l: Lane, s: Session) => (kept ? host.showSession(l, s) : void peekOpen(s.key, null, engaged).catch((e: unknown) => report.error(errorText(e))));
  /** Keeps the session on the desktop, where the peek is. */
  const keep = () => void keepSession().catch((e: unknown) => report.error(errorText(e)));

  // The card fills its window once the window has a size of its own: kept, or dragged to one.
  const filled = kept || sized;
  const grip = (edge: Edge) => <div className={`ca-peek-grip is-${edge}`} onPointerDown={startSize(edge)} onPointerMove={moveSize} onPointerUp={endSize} onPointerCancel={endSize} />;
  if (side) {
    return (
      <div className="ca-peek-root is-kept is-filled" style={{ padding: `${NEAR}px ${MARGIN}px ${FAR}px` }}>
        <div
          ref={card}
          className="ca-peek"
          onPointerDown={(e) => {
            if (!engaged) void keptFocus();
            startMove(e);
          }}
          onPointerMove={moveWindow}
        >
          <div className="ca-reply" style={cardZoom(zoom)}>
            <SideReply session={session} terminal={open.terminal} inWindow={{ about: session.title, show: () => host.showSession(lane, session), putBack: () => void keptClose(), close: () => closeSide.current() }} />
          </div>
        </div>
        {grip('side')}
        {grip('far')}
        {grip('corner')}
      </div>
    );
  }
  return (
    <div
      className={cx('ca-peek-root', !open.below && 'is-over', kept && 'is-kept', filled && 'is-filled')}
      style={{ padding: open.below ? `${NEAR}px ${MARGIN}px ${FAR}px` : `${FAR}px ${MARGIN}px ${NEAR}px` }}
      onMouseDown={(e) => {
        // The shadow around the card is part of the window; a click there is a click outside.
        if (e.target === e.currentTarget) away();
      }}
    >
      <div
        ref={card}
        className="ca-peek"
        style={filled ? undefined : { maxHeight: open.room - FAR - NEAR }}
        onPointerDown={(e) => {
          // Hovering never gives it the keyboard; a click does.
          if (!engaged) void (kept ? keptFocus() : peekFocus());
          startMove(e);
        }}
        onPointerMove={moveWindow}
      >
        <SessionCard
          lane={lane}
          session={session}
          apps={open.apps ?? { terminal: open.terminal, terminalId: '', editor: 'Editor', editorId: '' }}
          now={now}
          zoom={zoom}
          board={live.board}
          can={can}
          // A session it came from, or that came of it: its menu shows it in Codealong. What's
          // done to it leaves the peek up, which is on another session.
          canFor={(l, s) => ({ ...can, reveal: () => host.showSession(l, s), rename: undefined, left: undefined })}
          onShowSession={showKin}
          onShowLane={host.showLane}
          onShowTodo={host.showTodo}
          onShowPr={host.showPr}
          autoFocus={open.focus}
          onLoaded={loaded}
          renaming={open.rename || renameAsked === session.key}
          onRename={renamed}
          onKeep={kept ? undefined : keep}
          onClose={kept ? () => void keptClose() : away}
          closeTitle={kept ? undefined : 'Close (esc)'}
          onSending={away}
          onSent={(sent) => {
            // What you'd want to know goes on the bar: the prompt is on the clipboard, say, or it
            // went as a message from Codealong because its terminal couldn't be typed into.
            if (sent.opened || sent.fellBack) void barNote({ key: session.key, message: sent.message, tone: 'info' });
          }}
          // It's back in the box for when the peek shows it again.
          onFailed={(why) => void barNote({ key: session.key, message: why, tone: 'error' })}
          onAnswered={() => {
            // What you wrote while it waited is likely what you'd send next.
            if (!readDraft(session.key)) away();
          }}
        />
      </div>
      {/* Its edges size it, in the room the window keeps around the card. */}
      {grip('side')}
      {grip('far')}
      {grip('corner')}
    </div>
  );
}

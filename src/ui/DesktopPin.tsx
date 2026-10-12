// A card pinned to the desktop: a session, worktree, todo or PR, pinned from its right-click menu
// or dragged out of its window, in a window of its own that floats over every other as the floating
// bar does (desktop.rs). It's the Canvas view's card (card/CardFace.tsx), and under a worktree, a
// todo or a PR its sessions. A click on a session (the card's own, a session line, or a todo's)
// shows it in the bar's peek beside the card, its conversation and a box to answer it (peek.rs,
// Peek.tsx); a double-click opens it where it lives. The worktree's PR chip and a PR's own card
// show the PR in Codealong on a click and open it on GitHub on two.
// Dragging moves the card, and its right-click menu has the rest, with Remove from desktop. The
// engine sends it what it shows, and only when that changed; the page sizes the window to itself.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent, type PointerEvent } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { desktopHover, fitDesktop, getDesktopFeed, onDesktopFeed, onPeekShown, openUrl, peekHide, peekOpen, showMain, showQuick, todoOp, unpinFromDesktop, type PillRect } from '../api';
import { boardView, lanesAhead, observe, useAhead } from '../ahead';
import { CLICK_WAIT_MS, plain } from '../bar';
import { showContextMenu, type MenuSpec } from '../contextMenu';
import { useEditorIcons } from '../editors';
import { sessionPlace } from '../harness';
import * as menus from '../menus';
import { liveOrLatest, sessionHomes } from '../lanes';
import { prRefOf } from '../prLinks';
import type { DesktopFeed, Label, Lane, PeekShown, Pr, PrItem, Session } from '../types';
import { CardFace, CardWaiting } from './card/CardFace';
import { SessionLine } from './card/CardPopover';
import { cardData, cardSessions, sessionLine, type CardData } from './card/data';
import { cx, STATUS_WORD } from './primitives';
import { useTheme } from './ThemeSwitch';
import { useNow } from './useNow';
import { errorText, type Report } from '../report';
import { reviewSnoozeMenu } from '../snooze';
import { openWhere, where } from '../open';

/** Room around the card for its shadow: the window is this much bigger on every side. */
const MARGIN = 12;
/** The most sessions a worktree's or todo's card lists. */
const SESSIONS = 4;
/** How far the pointer moves with the button down before it drags the card rather than clicks it. */
const DRAG_AFTER = 4;

/** What the card's tooltip says: what it leaves out, and what a click does. */
function tooltip(d: CardData | null, now: number, terminal: string, lines: number): string {
  switch (d?.ref) {
    case 'session': {
      const s = d.session;
      return [s.title, `${STATUS_WORD[s.status]} · ${d.lane.callsign} · ${s.account}`, plain(sessionLine(s), now), `Click to read and answer it, double-click to open it in ${sessionPlace(s, terminal)}`].filter(Boolean).join('\n');
    }
    case 'worktree':
      return [d.lane.callsign, d.lane.branch && d.lane.branch !== d.lane.callsign ? d.lane.branch : '', plain(d.lane.headline, now), lines ? 'Click a session to read and answer it, double-click to open it' : ''].filter(Boolean).join('\n');
    case 'todo':
      return [d.todo.title || 'Untitled', d.sessions.length ? 'Click to read and answer its session, double-click to open it' : 'Double-click to start a session for it'].join('\n');
    case 'pr':
      return [`#${d.pr.number} ${d.pr.title}`, `${d.pr.repo} · ${d.pr.head}`, 'Click to show it in Codealong, double-click to open it on GitHub'].join('\n');
    default:
      return '';
  }
}

/** Where an element is in the window, for the peek to open by. */
function rectOf(el: Element): PillRect {
  const r = el.getBoundingClientRect();
  return { x: r.left, y: r.top, w: r.width, h: r.height };
}

export function DesktopPin() {
  const [feed, setFeed] = useState<DesktopFeed | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const now = useNow(30_000);
  const root = useRef<HTMLDivElement>(null);
  const card = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; moved: boolean } | null>(null);
  // A click waiting to see whether it's the first of two.
  const clickTimer = useRef<number | null>(null);
  // The session the peek shows, wherever it was opened from.
  const [peeked, setPeeked] = useState<PeekShown>({ key: null, engaged: false });
  const peekedNow = useRef(peeked);
  peekedNow.current = peeked;
  useEffect(() => onPeekShown(setPeeked), []);
  useEffect(() => () => {
    if (clickTimer.current != null) window.clearTimeout(clickTimer.current);
  }, []);

  useEffect(() => {
    const stop = onDesktopFeed(setFeed);
    getDesktopFeed()
      .then((first) => setFeed((current) => current ?? first))
      // The engine pushes the feed as it changes, so one that didn't read comes with the next.
      .catch(() => {});
    return stop;
  }, []);

  // Changes made in any window that the engine hasn't shown yet (ahead.ts): marked seen in the
  // main window, the card shows it at once.
  const ahead = useAhead();
  const lanes = useMemo(() => (feed ? lanesAhead(feed.lanes, ahead) : []), [feed, ahead]);
  useEffect(() => {
    if (feed) observe(boardView(feed.generatedAt, [feed.lanes]));
  }, [feed]);

  const theme = feed?.theme ?? 'system';
  useTheme(theme);

  useEffect(() => {
    if (!failed) return;
    const id = window.setTimeout(() => setFailed(null), 6000);
    return () => window.clearTimeout(id);
  }, [failed]);

  const item = useMemo(() => ({ ref: feed?.pin.ref ?? 'session', key: feed?.pin.key ?? '', title: feed?.title ?? undefined }), [feed]);
  const data = useMemo(() => {
    if (!feed) return null;
    const entities = { lanes: new Map(lanes.map((l) => [l.id, l])), sessions: sessionHomes(lanes), todos: new Map(feed.todos.map((t) => [t.id, t])), prs: new Map((feed.prs ?? []).map((p): [string, PrItem] => [p.key, p])) };
    const labels = new Map<string, Label>(feed.labels.map((l) => [l.id, l]));
    return cardData(entities, { kind: 'ref', x: 0, y: 0, ...item }, labels);
  }, [feed, lanes, item]);
  const sessions = cardSessions(data, lanes);
  const terminal = feed?.terminal ?? 'Terminal';
  const icons = useEditorIcons(feed ?? undefined);
  // What's on the desktop, this card surely: it's there before the list is.
  const pinned = menus.useDesktopPins();
  const onDesktop = useMemo(() => (feed ? new Set([...pinned, `${feed.pin.ref}:${feed.pin.key}`]) : pinned), [pinned, feed]);

  // The window follows the card's size: after every render, and on resizes renders don't cause
  // (fonts loading).
  const reported = useRef('');
  const report = useCallback(() => {
    const r = root.current?.getBoundingClientRect();
    if (!r) return;
    const size = [Math.ceil(r.width) + 2 * MARGIN, Math.ceil(r.height) + 2 * MARGIN];
    if (size.join('x') === reported.current) return;
    reported.current = size.join('x');
    void fitDesktop(size[0], size[1]);
  }, []);
  useLayoutEffect(report);
  useEffect(() => {
    if (!root.current) return;
    const observer = new ResizeObserver(report);
    observer.observe(root.current);
    return () => observer.disconnect();
  }, [report]);

  const fail = (e: unknown) => setFailed(errorText(e));
  // What comes of an open or a menu's action shows on the card.
  const said: Report = { error: setFailed, info: setFailed };

  /** Opens a session where it lives and brings that app forward: a click here leaves Codealong
   *  in the background, as one in the bar does. */
  const openOn = (lane: Lane, target: string) => openWhere(where(lane, target), { report: said, from: 'panel' });
  const openSession = (lane: Lane, s: Session) => openOn(lane, s.openTarget);

  /** What a click on the card does: a session opens where it lives, a worktree the way its row's
   *  double-click opens it, and a todo its session, or the quick panel ready to start one. */
  const openIt = () => {
    switch (data?.ref) {
      case 'session':
        openSession(data.lane, data.session);
        return;
      case 'worktree': {
        const l = data.lane;
        if (l.defaultTarget) openOn(l, l.defaultTarget);
        else void showMain({ kind: 'worktree', laneId: l.id });
        return;
      }
      case 'todo': {
        const s = liveOrLatest(data.sessions);
        if (s) openSession(s.lane, s.session);
        else void showQuick({ todo: data.todo.id, project: data.todo.project || null });
        return;
      }
      case 'pr':
        openUrl(data.pr.url).catch(fail);
        return;
    }
  };

  const remove = () => {
    if (feed) unpinFromDesktop(feed.pin).catch(fail);
  };

  /** Whether the press that ended in a click on the PR's chip dragged the card instead. */
  const dragged = () => {
    const moved = drag.current?.moved;
    drag.current = null;
    return !!moved;
  };
  /** Shows the worktree's PR in Codealong: a click on its chip, unless the press dragged the card.
   *  The card has only its link, whose key the main window knows it by too. */
  const showPr = (pr: Pr) => {
    if (dragged()) return;
    const ref = prRefOf(pr.url);
    if (ref) void showMain({ kind: 'pr', key: ref.key });
    else openUrl(pr.url).catch(fail);
  };
  /** Opens it on GitHub: a double-click on its chip. */
  const openPr = (pr: Pr) => {
    if (!dragged()) openUrl(pr.url).catch(fail);
  };
  /** The menus, as every window has them (menus.ts): the card's thing's, or a session's for a
   *  session line in a worktree's or todo's card. What comes of them shows on the card. */
  const editors = feed ? { apps: feed, icons } : undefined;
  const sessionMenu = (lane: Lane, s: Session, el: Element | null): MenuSpec =>
    menus.sessionMenu(lane, s, {
      report: said,
      open: (l, target) => (target === s.openTarget ? openSession(l, s) : openOn(l, target)),
      terminal,
      reveal: () => void showMain({ kind: 'session', laneId: lane.id, key: s.key }),
      reply: el ? () => peekOpen(s.key, rectOf(el), true).catch(fail) : undefined,
      // Its name, in the peek beside what was clicked.
      rename: el ? () => peekOpen(s.key, rectOf(el), true, true).catch(fail) : undefined,
      editors,
      desktop: onDesktop,
    });
  const menu = (target: Element): MenuSpec => {
    const line = target.closest<HTMLElement>('[data-session]');
    const hit = line && sessions.find((x) => x.session.key === line.dataset.session);
    if (line && hit) return sessionMenu(hit.lane, hit.session, line);
    switch (data?.ref) {
      case 'session':
        return sessionMenu(data.lane, data.session, card.current);
      case 'worktree': {
        const l = data.lane;
        return menus.laneMenu(l, { report: said, open: openOn, reveal: () => void showMain({ kind: 'worktree', laneId: l.id }), editors, desktop: onDesktop });
      }
      case 'todo': {
        const t = data.todo;
        const s = liveOrLatest(data.sessions);
        return menus.todoMenu(t, {
          report: said,
          start: () => void showQuick({ todo: t.id, project: t.project || null }),
          session: s ? { ...s, open: () => openSession(s.lane, s.session) } : undefined,
          reveal: () => void showMain({ kind: 'todo', id: t.id }),
          // Its status and due date: the card's feed has neither the other todos (for waiting),
          // every label, nor the projects.
          edit: { run: (op) => void todoOp(op).catch(fail) },
          desktop: onDesktop,
        });
      }
      case 'pr': {
        const p = data.pr;
        const s = liveOrLatest(lanes.flatMap((lane) => lane.sessions.map((session) => ({ lane, session }))));
        const snooze = reviewSnoozeMenu(p, Date.now(), (act) => void act.catch(fail));
        return menus.prMenu(p, { report: said, session: s ? { ...s, open: () => openSession(s.lane, s.session) } : undefined, reveal: () => void showMain({ kind: 'pr', key: p.key }), snooze, desktop: onDesktop });
      }
      default:
        return [{ label: 'Remove from desktop', run: remove }];
    }
  };

  const onContextMenu = (e: MouseEvent) => {
    e.preventDefault();
    void showContextMenu(menu(e.target as Element)).catch(fail);
  };

  // The whole card is the handle: pressing and moving drags the window, a press without moving
  // clicks it.
  const onPointerDown = (e: PointerEvent) => {
    if (e.button === 0) drag.current = { x: e.screenX, y: e.screenY, moved: false };
  };
  const onPointerMove = (e: PointerEvent) => {
    const d = drag.current;
    if (!d || d.moved || !(e.buttons & 1) || Math.abs(e.screenX - d.x) + Math.abs(e.screenY - d.y) < DRAG_AFTER) return;
    d.moved = true;
    void getCurrentWindow().startDragging();
  };
  /** The session a click shows in the peek, and what it opens by: the session line clicked, else
   *  the card's own session, or a todo's live or latest one. */
  const clicked = (target: Element): { lane: Lane; session: Session; el: Element } | null => {
    const line = target.closest<HTMLElement>('[data-session]');
    const s = line && sessions.find((x) => x.session.key === line.dataset.session);
    if (line && s) return { ...s, el: line };
    const el = card.current;
    if (!el) return null;
    if (data?.ref === 'session') return { lane: data.lane, session: data.session, el };
    const t = data?.ref === 'todo' ? liveOrLatest(data.sessions) : null;
    return t ? { ...t, el } : null;
  };

  /** Shows a session in the peek beside the card, ready to answer; clicked again, puts it away. */
  const peek = (key: string, at: PillRect) => {
    if (peekedNow.current.key === key) void peekHide();
    else peekOpen(key, at, true).catch(fail);
  };

  const onClick = (e: MouseEvent) => {
    const moved = drag.current?.moved;
    drag.current = null;
    if (moved || !data) return;
    if (clickTimer.current != null) window.clearTimeout(clickTimer.current);
    clickTimer.current = null;
    const hit = clicked(e.target as Element);
    // The second click of a double-click: where it lives. The peek goes if the first opened it.
    if (e.detail >= 2) {
      if (peekedNow.current.key) void peekHide();
      if (hit && hit.el !== card.current) openSession(hit.lane, hit.session);
      else openIt();
      return;
    }
    if (!hit) {
      // A PR has no conversation to peek at: its panel in Codealong says where it stands.
      if (data.ref === 'pr') {
        const key = data.pr.key;
        clickTimer.current = window.setTimeout(() => {
          clickTimer.current = null;
          void showMain({ kind: 'pr', key });
        }, CLICK_WAIT_MS);
      }
      return;
    }
    const at = rectOf(hit.el);
    clickTimer.current = window.setTimeout(() => {
      clickTimer.current = null;
      peek(hit.session.key, at);
    }, CLICK_WAIT_MS);
  };

  const waiting = !feed || (feed.loading && !data);
  return (
    <div ref={root} className="ca-desk" onContextMenu={onContextMenu}>
      <div
        ref={card}
        className={cx('ca-desk-card', data?.ref === 'session' && peeked.key === data.session.key && 'is-peeked')}
        role="button"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        title={tooltip(data, now, terminal, sessions.length) || undefined}
        // A session's card shows lines to the cards of the sessions it came from or that came of it.
        onPointerEnter={data?.ref === 'session' ? () => void desktopHover(true) : undefined}
        onPointerLeave={data?.ref === 'session' ? () => void desktopHover(false) : undefined}
        onClick={onClick}
      >
        {waiting ? (
          <CardWaiting className="is-desk" />
        ) : (
          <CardFace item={item} data={data} now={now} className="is-desk" onPrShow={showPr} onPrOpen={openPr}>
            {sessions.length > 0 && (
              <div className="ca-desk-sessions">
                {sessions.slice(0, SESSIONS).map(({ session }) => (
                  <div key={session.key} className={cx('ca-desk-session', peeked.key === session.key && 'is-peeked')} data-session={session.key}>
                    <SessionLine s={session} now={now} />
                  </div>
                ))}
                {sessions.length > SESSIONS && <div className="ca-cvp-dim">and {sessions.length - SESSIONS} more</div>}
              </div>
            )}
          </CardFace>
        )}
      </div>
      {failed && (
        <button type="button" className="ca-desk-note" role="alert" title={failed} onClick={() => setFailed(null)}>
          {failed}
        </button>
      )}
    </div>
  );
}

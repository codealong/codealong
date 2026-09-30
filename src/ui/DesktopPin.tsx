// A card pinned to the desktop: a session, worktree or todo, pinned from its right-click menu, in a
// window of its own that floats over every other as the floating bar does (desktop.rs). It shows
// what its Canvas card shows, and under a worktree or a todo its sessions. A click on a session
// (the card's own, a session line, or a todo's) shows it in the bar's peek beside the card, its
// conversation and a box to answer it (peek.rs, Peek.tsx); a double-click opens it where it lives.
// A click on the worktree's PR chip opens the PR on GitHub.
// Dragging moves the card, and its right-click menu has the rest, with Remove from desktop. The
// engine sends it what it shows, and only when that changed; the page sizes the window to itself.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent, type PointerEvent } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { fitDesktop, getDesktopFeed, onDesktopFeed, onPeekShown, openFromBar, openUrl, peekHide, peekOpen, showMain, showQuick, unpinFromDesktop, type PillRect } from '../api';
import { boardView, lanesAhead, observe, seenAhead, useAhead } from '../ahead';
import { CLICK_WAIT_MS, plain } from '../bar';
import { showContextMenu, type MenuSpec } from '../contextMenu';
import { shortOf } from '../devices';
import { folderMenu, openInMenu, opensInEditor, useEditorIcons } from '../editors';
import { renames, sessionPlace } from '../harness';
import { seenable } from '../lanes';
import { snoozeMenu } from '../snooze';
import { sessionHomes } from '../todos';
import type { DesktopFeed, Label, Lane, PeekShown, Pr, Session } from '../types';
import { cardData, CardFace, SessionLine, sessionLine, type CardData } from './CanvasCard';
import { STATUS_WORD } from './primitives';
import { applyTheme } from './ThemeSwitch';

/** Room around the card for its shadow: the window is this much bigger on every side. */
const MARGIN = 12;
/** The most sessions a worktree's or todo's card lists. */
const SESSIONS = 4;
/** How far the pointer moves with the button down before it drags the card rather than clicks it. */
const DRAG_AFTER = 4;

function useNow(everyMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), everyMs);
    return () => window.clearInterval(id);
  }, [everyMs]);
  return now;
}

/** Where a session opens, as its lane says it when it lists it (lanes list their first few
 *  sessions' targets only). */
function openLabel(lane: Lane, s: Session, terminal: string): string {
  return lane.targets.find((t) => t.id === s.openTarget)?.label ?? `Open in ${sessionPlace(s, terminal)}`;
}

/** The sessions a worktree's or todo's card lists under it: a todo's all, a worktree's the ones
 *  running or waiting on you (it keeps every session it ever had). */
function listed(d: CardData | null): { lane: Lane; session: Session }[] {
  if (d?.ref === 'worktree') return d.lane.sessions.filter((s) => !s.visitor && !s.archived && (s.live || s.status !== 'idle' && s.status !== 'stopped')).map((session) => ({ lane: d.lane, session }));
  if (d?.ref === 'todo') return d.sessions;
  return [];
}

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
  useEffect(() => applyTheme(theme), [theme]);

  useEffect(() => {
    if (!failed) return;
    const id = window.setTimeout(() => setFailed(null), 6000);
    return () => window.clearTimeout(id);
  }, [failed]);

  const item = useMemo(() => ({ ref: feed?.pin.ref ?? 'session', key: feed?.pin.key ?? '', title: feed?.title ?? undefined }), [feed]);
  const data = useMemo(() => {
    if (!feed) return null;
    const entities = { lanes: new Map(lanes.map((l) => [l.id, l])), sessions: sessionHomes(lanes), todos: new Map(feed.todos.map((t) => [t.id, t])), prs: new Map() };
    const labels = new Map<string, Label>(feed.labels.map((l) => [l.id, l]));
    return cardData(entities, { kind: 'ref', x: 0, y: 0, ...item }, labels);
  }, [feed, lanes, item]);
  const sessions = listed(data);
  const terminal = feed?.terminal ?? 'Terminal';
  const icons = useEditorIcons(feed ?? undefined);

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

  const fail = (e: unknown) => setFailed(String(e).replace(/^Error: /, ''));

  /** Opens a session where it lives and brings that app forward: a click here leaves Codealong
   *  in the background, as one in the bar does. */
  const openSession = (lane: Lane, s: Session) => {
    const target = lane.targets.find((t) => t.id === s.openTarget);
    if (target?.disabled) {
      setFailed(target.detail ?? `${target.label} isn't available right now.`);
      return;
    }
    openFromBar(lane.id, s.openTarget).catch(fail);
  };

  /** What a click on the card does: a session opens where it lives, a worktree the way its row's
   *  double-click opens it, and a todo its session, or the quick panel ready to start one. */
  const openIt = () => {
    switch (data?.ref) {
      case 'session':
        openSession(data.lane, data.session);
        return;
      case 'worktree': {
        const l = data.lane;
        const target = l.targets.find((t) => t.id === l.defaultTarget);
        if (!l.defaultTarget) void showMain({ kind: 'worktree', laneId: l.id });
        else if (target?.disabled) setFailed(target.detail ?? `${target.label} isn't available right now.`);
        else openFromBar(l.id, l.defaultTarget).catch(fail);
        return;
      }
      case 'todo': {
        const s = data.sessions.find((x) => x.session.live) ?? data.sessions[0];
        if (s) openSession(s.lane, s.session);
        else void showQuick({ todo: data.todo.id, project: data.todo.project || null });
        return;
      }
    }
  };

  const remove = () => {
    if (feed) unpinFromDesktop(feed.pin).catch(fail);
  };

  /** Opens the worktree's PR on GitHub: a click on its chip, unless the press dragged the card. */
  const openPr = (pr: Pr) => {
    const moved = drag.current?.moved;
    drag.current = null;
    if (!moved) openUrl(pr.url).catch(fail);
  };
  const prItem = (lane: Lane): MenuSpec => {
    const pr = lane.pr;
    return pr ? [{ label: `Open PR #${pr.number} on GitHub`, run: () => openUrl(pr.url).catch(fail) }] : [];
  };

  const menu = (): MenuSpec => {
    const off: MenuSpec = ['separator', { label: 'Remove from desktop', run: remove }];
    switch (data?.ref) {
      case 'session': {
        const { lane, session: s } = data;
        return [
          { label: openLabel(lane, s, terminal), run: () => openSession(lane, s) },
          ...(feed && opensInEditor(lane) ? folderMenu(feed, icons, (id) => void openFromBar(lane.id, id).catch(fail)) : []),
          ...prItem(lane),
          { label: 'Show in Codealong', run: () => void showMain({ kind: 'session', laneId: lane.id, key: s.key }) },
          ...(renames(s) ? [{ label: 'Rename…', run: () => rename(s.key) }] : []),
          ...(seenable(s) ? [{ label: 'Mark as seen', run: () => seenAhead([s.key]).catch(fail) }] : []),
          ...(s.archived || shortOf(s.key) ? [] : [snoozeMenu([s.key], Date.now(), { until: s.snoozedUntil, onDone: (act) => void act.catch(fail) })]),
          ...off,
        ];
      }
      case 'worktree': {
        const l = data.lane;
        // Its default open, its editors, Finder, then its app in the browser.
        const opens = [l.defaultTarget, 'l|editor', 'l|finder', 'l|preview'].flatMap((id, i): MenuSpec => {
          const t = l.targets.find((x) => x.id === id);
          const item = t && (i === 0 || id !== l.defaultTarget) ? [{ label: t.label, enabled: !t.disabled, run: () => openFromBar(l.id, t.id).catch(fail) }] : [];
          // The other editors installed here, after Open in <yours>.
          return id === 'l|editor' && feed && opensInEditor(l) ? [...item, ...openInMenu(feed, icons, (target) => void openFromBar(l.id, target).catch(fail))] : item;
        });
        return [...opens, ...prItem(l), { label: 'Show in Codealong', run: () => void showMain({ kind: 'worktree', laneId: l.id }) }, ...off];
      }
      case 'todo': {
        const t = data.todo;
        const s = data.sessions.find((x) => x.session.live) ?? data.sessions[0];
        return [
          { label: 'Start session…', run: () => void showQuick({ todo: t.id, project: t.project || null }) },
          ...(s ? [{ label: `Open ${s.session.account}: ${s.session.title.slice(0, 48)}`, run: () => openSession(s.lane, s.session) }] : []),
          { label: 'Show in Codealong', run: () => void showMain({ kind: 'todo', id: t.id }) },
          ...off,
        ];
      }
      default:
        return [{ label: 'Remove from desktop', run: remove }];
    }
  };

  const onContextMenu = (e: MouseEvent) => {
    e.preventDefault();
    void showContextMenu(menu()).catch(fail);
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
    const t = data?.ref === 'todo' ? (data.sessions.find((x) => x.session.live) ?? data.sessions[0]) : null;
    return t ? { ...t, el } : null;
  };

  /** Shows a session in the peek beside the card, ready to answer; clicked again, puts it away. */
  const peek = (key: string, at: PillRect) => {
    if (peekedNow.current.key === key) void peekHide();
    else peekOpen(key, at, true).catch(fail);
  };

  /** Shows the card's session in the peek beside it with its name ready to change. */
  const rename = (key: string) => {
    if (card.current) peekOpen(key, rectOf(card.current), true, true).catch(fail);
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
    if (!hit) return;
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
        className={['ca-desk-card', data?.ref === 'session' && peeked.key === data.session.key && 'is-peeked'].filter(Boolean).join(' ')}
        role="button"
        title={tooltip(data, now, terminal, sessions.length) || undefined}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onClick={onClick}
      >
        {waiting ? (
          <div className="ca-cv-card is-desk is-gone">
            <div className="ca-cv-l2">Reading sessions…</div>
          </div>
        ) : (
          <CardFace item={item} data={data} now={now} className="is-desk" onPrOpen={openPr}>
            {sessions.length > 0 && (
              <div className="ca-desk-sessions">
                {sessions.slice(0, SESSIONS).map(({ session }) => (
                  <div key={session.key} className={['ca-desk-session', peeked.key === session.key && 'is-peeked'].filter(Boolean).join(' ')} data-session={session.key}>
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

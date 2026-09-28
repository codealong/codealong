// A card pinned to the desktop: a session, worktree or todo, pinned from its right-click menu, in a
// window of its own that floats over every other as the floating bar does (desktop.rs). It shows
// what its Canvas card shows, and under a worktree or a todo its sessions. A click opens it where
// it lives (a session line, that session), dragging moves it, and its right-click menu has the
// rest, with Remove from desktop. The engine sends it what it shows, and only when that changed;
// the page sizes the window to itself.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent, type PointerEvent } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { fitDesktop, getDesktopFeed, onDesktopFeed, openFromBar, showMain, showQuick, unpinFromDesktop } from '../api';
import { boardView, lanesAhead, observe, seenAhead, useAhead } from '../ahead';
import { plain } from '../bar';
import { showContextMenu, type MenuSpec } from '../contextMenu';
import { shortOf } from '../devices';
import { sessionPlace } from '../harness';
import { seenable } from '../lanes';
import { snoozeMenu } from '../snooze';
import { sessionHomes } from '../todos';
import type { DesktopFeed, Label, Lane, Session } from '../types';
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
function tooltip(d: CardData | null, now: number, terminal: string): string {
  switch (d?.ref) {
    case 'session': {
      const s = d.session;
      return [s.title, `${STATUS_WORD[s.status]} · ${d.lane.callsign} · ${s.account}`, plain(sessionLine(s), now), `Click to open in ${sessionPlace(s, terminal)}`].filter(Boolean).join('\n');
    }
    case 'worktree':
      return [d.lane.callsign, d.lane.branch && d.lane.branch !== d.lane.callsign ? d.lane.branch : '', plain(d.lane.headline, now)].filter(Boolean).join('\n');
    case 'todo':
      return [d.todo.title || 'Untitled', d.sessions.length ? 'Click to open its session' : 'Click to start a session for it'].join('\n');
    default:
      return '';
  }
}

export function DesktopPin() {
  const [feed, setFeed] = useState<DesktopFeed | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const now = useNow(30_000);
  const root = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; moved: boolean } | null>(null);

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

  const menu = (): MenuSpec => {
    const off: MenuSpec = ['separator', { label: 'Remove from desktop', run: remove }];
    switch (data?.ref) {
      case 'session': {
        const { lane, session: s } = data;
        return [
          { label: openLabel(lane, s, terminal), run: () => openSession(lane, s) },
          { label: 'Show in Codealong', run: () => void showMain({ kind: 'session', laneId: lane.id, key: s.key }) },
          ...(seenable(s) ? [{ label: 'Mark as seen', run: () => seenAhead([s.key]).catch(fail) }] : []),
          ...(s.archived || shortOf(s.key) ? [] : [snoozeMenu([s.key], Date.now(), { until: s.snoozedUntil, onDone: (act) => void act.catch(fail) })]),
          ...off,
        ];
      }
      case 'worktree': {
        const l = data.lane;
        const opens = [l.defaultTarget, 'l|editor', 'l|finder'].flatMap((id, i) => {
          const t = l.targets.find((x) => x.id === id);
          return t && (i === 0 || id !== l.defaultTarget) ? [{ label: t.label, enabled: !t.disabled, run: () => openFromBar(l.id, t.id).catch(fail) }] : [];
        });
        return [...opens, { label: 'Show in Codealong', run: () => void showMain({ kind: 'worktree', laneId: l.id }) }, ...off];
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
  const onClick = (e: MouseEvent) => {
    const moved = drag.current?.moved;
    drag.current = null;
    if (moved || !data) return;
    const line = (e.target as Element).closest<HTMLElement>('[data-session]');
    const s = line && sessions.find((x) => x.session.key === line.dataset.session);
    if (s) openSession(s.lane, s.session);
    else openIt();
  };

  const waiting = !feed || (feed.loading && !data);
  return (
    <div ref={root} className="ca-desk" onContextMenu={onContextMenu}>
      <div
        className="ca-desk-card"
        role="button"
        title={tooltip(data, now, terminal) || undefined}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onClick={onClick}
      >
        {waiting ? (
          <div className="ca-cv-card is-desk is-gone">
            <div className="ca-cv-l2">Reading sessions…</div>
          </div>
        ) : (
          <CardFace item={item} data={data} now={now} className="is-desk">
            {sessions.length > 0 && (
              <div className="ca-desk-sessions">
                {sessions.slice(0, SESSIONS).map(({ session }) => (
                  <div key={session.key} className="ca-desk-session" data-session={session.key}>
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

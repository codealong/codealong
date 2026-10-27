// The floating bar's page: pills for the sessions that need you, whose turn ended, or that are
// running, one for the sessions you snoozed, one for the rest of the sessions started in the last
// day, one for PRs waiting on your review, and one for todos ready to start, in the project
// picked beside the mark or in all of them. The engine keeps the window up and sends it what to
// show (barfeed.rs); the page sizes the window to itself. Pills that don't fit fold into one per
// group that lists them. Hovering a session's pill shows it in the peek under the bar (peek.rs,
// Peek.tsx), its conversation and a box to answer it; clicking the pill shows the session in
// Codealong and double-clicking opens it where it lives, dragging it off the bar pins its card to
// the desktop where you let go (desktop.rs), right-clicking shows its menu, and the mark brings
// Codealong forward. Docked in the
// notch (bar.rs), the same pills show below the notch when it opens (Notch.tsx), dark, and there's
// no peek: a click on a session's pill opens the notch further, into that session under the pills
// (NotchSession.tsx), where its conversation is read and answered and the ones that wait on you
// are worked through in turn. There a session picked from a folded pill's menu or from the recent
// ones shows the same way, and a count beside the camera shows its group's first. Your other
// devices' sessions are among the pills (barfeed.rs), each with its device's color.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent, type PointerEvent, type ReactNode } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { barMode, dragToDesktop, fitBar, getBarFeed, onBarFeed, onDesktopDragEnd, onBarMode, onBarNote, onPeekShown, onWaitingShortcut, peekHide, peekOpen, sessionAt, setBarMode, setNotchOpen, showMain, showQuick, showWaiting, type BarModeView, type PillRect } from '../api';
import { BAR_PROJECT_KEY, CLICK_WAIT_MS, elsewhereWaits, fitGroups, GROUPS, plain, projectWaits, type Group, type GroupFit, type WaitCounts } from '../bar';
import { showContextMenu, type MenuAction, type MenuSpec } from '../contextMenu';
import { useEditorIcons, type Apps } from '../editors';
import { ago } from '../format';
import * as menus from '../menus';
import { usePerf } from '../perf';
import { useLatest } from '../latest';
import { useViewKeys } from '../mainKeys';
import { barAhead, barView, observe, seenAhead, snoozeAhead, useAhead } from '../ahead';
import { ALL_PROJECTS, inProject } from '../quick';
import { snoozeMenu, untilWords } from '../snooze';
import type { BarFeed, BarItem, BarRecent, BarReview, BarSnoozed, BarTodo, PeekShown, Status } from '../types';
import { MarkLive } from './MarkLive';
import { cx, Icon, SessionEmblem, STATUS_WORD, StatusGlyph } from './primitives';
import { applyTheme } from './ThemeSwitch';
import { NotchShell } from './Notch';
import { NotchReview } from './NotchReview';
import { NotchSession, useNotchStage } from './NotchSession';
import { QueueEnd } from './QueueEnd';
import { placeWords, ProjectShown, usePlaceWords, type PlaceWords } from './ProjectSaid';
import { LOGO_LABEL, useLogo } from '../logo';
import { useMarkMoment } from '../mark';
import { useNow } from './useNow';
import { errorText, type Report } from '../report';
import { openWhere, openWords } from '../open';
import { dueWords } from '../due';
import { asText, readStored, writeStored } from '../stored';

/** Room around the bar for its shadow: the window is this much bigger on every side. */
const MARGIN = 12;
/** The widest the bar gets, and the room it leaves at the screen's sides. */
const MAX_WIDTH = 900;
const SCREEN_ROOM = 48;
/** Sizes from app.css (.ca-hud) that decide what fits. */
const GAP = 4;
const DIVIDER = 1;
const PADDING = 5;
const MARK = 24;
/** A folded pill is measured holding this, so a real count never outgrows it. */
const FOLD_SAMPLE = '+88';
/** How long the pointer rests on a session's pill before the peek shows it, and before it moves
 *  to another pill once it's up: long enough that passing over the bar doesn't open it. */
const PEEK_AFTER_MS = 300;
const PEEK_SWITCH_MS = 90;
/** How far a session's pill moves with the button down before it's dragged off the bar rather
 *  than clicked. */
const DRAG_AFTER = 6;

const GROUP_GLYPH: Record<Group, Status> = { needs: 'needs', unread: 'unread', working: 'working' };
const GROUP_WORD: Record<Group, string> = { needs: 'need you', unread: 'your turn', working: 'working' };

/** The pill's class for a status: errors are red, the rest of Needs you amber. */
function tone(status: Status): string {
  return status === 'error' ? 'is-error' : `is-${status}`;
}

/** What a pill says to a screen reader; the peek shows the rest. `place` says its worktree, after
 *  its project where the bar lists every project (ProjectSaid.tsx), on the device it's on when
 *  that's another. */
function sessionLabel(i: BarItem, now: number, place: PlaceWords): string {
  const device = i.device ? `On ${i.device.name} · ` : '';
  const where = `${STATUS_WORD[i.status]} · ${device}${place(i.project, i.callsign)} · ${i.account}`;
  return [i.title, where, i.detail ? plain(i.detail, now) : ''].filter(Boolean).join('. ');
}

interface SessionPillProps {
  item: BarItem;
  now: number;
  /** The peek shows this session, or the notch does under its pills. */
  peeked?: boolean;
  /** Says the rest in a tooltip, where no peek does (docked in the notch). */
  told?: boolean;
  onOpen?: (e: MouseEvent<HTMLButtonElement>) => void;
  onMenu?: () => void;
  /** The pointer came onto the pill or moved on it. */
  onHover?: (e: PointerEvent<HTMLElement>) => void;
  onLeave?: () => void;
  /** The button went down on the pill, and the pointer moved with it down: a drag off the bar. */
  onPress?: (e: PointerEvent<HTMLElement>) => void;
  onDrag?: (e: PointerEvent<HTMLElement>) => void;
  /** Being dragged off the bar. */
  dragged?: boolean;
  measure?: boolean;
}

function SessionPill({ item, now, peeked, told, onOpen, onMenu, onHover, onLeave, onPress, onDrag, dragged, measure }: SessionPillProps) {
  const place = usePlaceWords();
  return (
    <button
      type="button"
      className={cx('ca-hud-pill', tone(item.status), peeked && 'is-peeked', dragged && 'is-dragged')}
      aria-label={measure ? undefined : sessionLabel(item, now, place)}
      title={told && !measure && !peeked ? sessionLabel(item, now, place) : undefined}
      aria-pressed={measure ? undefined : peeked}
      data-pill={measure ? item.group : undefined}
      data-key={measure ? undefined : item.key}
      onClick={onOpen}
      onPointerDown={onPress}
      onPointerEnter={onHover}
      onPointerMove={(e) => {
        onDrag?.(e);
        onHover?.(e);
      }}
      onPointerLeave={onLeave}
      onContextMenu={
        onMenu
          ? (e) => {
              e.preventDefault();
              e.stopPropagation();
              onMenu();
            }
          : undefined
      }
    >
      <StatusGlyph status={item.status} waiting={item.waiting} title="" />
      <SessionEmblem emblem={item.emblem} of={item.key} size={14} />
      <span>{item.title}</span>
      {item.device && <i className={`ca-labeldot is-${item.device.color}`} aria-hidden="true" />}
    </button>
  );
}

function FoldPill({ group, label, title, onOpen, peeked, measure }: { group: Group; label: string; title?: string; onOpen?: (e: MouseEvent<HTMLButtonElement>) => void; peeked?: boolean; measure?: boolean }) {
  return (
    <button
      type="button"
      className={cx('ca-hud-pill', 'ca-hud-fold', `is-${group}`, peeked && 'is-peeked')}
      title={title}
      data-fold={measure ? group : undefined}
      onClick={onOpen}
      onContextMenu={
        onOpen
          ? (e) => {
              e.preventDefault();
              e.stopPropagation();
              onOpen(e);
            }
          : undefined
      }
    >
      <StatusGlyph status={GROUP_GLYPH[group]} title="" />
      <span>{label}</span>
    </button>
  );
}

function ReviewsPill({ prs, peeked, onOpen, measure }: { prs: BarReview[]; peeked?: boolean; onOpen?: (e: MouseEvent<HTMLButtonElement>) => void; measure?: boolean }) {
  const place = usePlaceWords();
  return (
    <button
      type="button"
      className={cx('ca-hud-pill', 'is-reviews', peeked && 'is-peeked')}
      title={measure ? undefined : prs.map((p) => reviewLabel(p, place)).join('\n')}
      data-reviews={measure ? '' : undefined}
      onClick={onOpen}
      onContextMenu={
        onOpen
          ? (e) => {
              e.preventDefault();
              e.stopPropagation();
              onOpen(e);
            }
          : undefined
      }
    >
      <Icon name="pr" size={12} stroke={1.75} />
      <span>{prs.length} to review</span>
    </button>
  );
}

/** A PR that waits on your review as its menu item and tooltip line say it: its number and
 *  title, its repo (after its project, as `place` says it), who asked, and that it's a sign-off
 *  when Codealong's agent approves it. */
function reviewLabel(p: BarReview, place: PlaceWords): string {
  return [`#${p.number} ${p.title}`, place(p.project, p.repo), p.requestedBy ? `asked by ${p.requestedBy}` : '', p.approves ? 'agent approves' : ''].filter(Boolean).join(' · ');
}

/** The rest of the sessions started in the last day, as one pill: a click drops a menu of them. */
function RecentPill({ items, now, onOpen, measure }: { items: BarRecent[]; now: number; onOpen?: (e: MouseEvent<HTMLButtonElement>) => void; measure?: boolean }) {
  const place = usePlaceWords();
  return (
    <button
      type="button"
      className="ca-hud-pill is-recent"
      title={measure ? undefined : `${items.length === 1 ? '1 other session' : `${items.length} other sessions`} from the last 24 hours:\n${items.map((i) => `${i.title} · ${place(i.project, i.callsign)} · ${ago(i.since, now)}`).join('\n')}`}
      data-recent={measure ? '' : undefined}
      onClick={onOpen}
      onContextMenu={
        onOpen
          ? (e) => {
              e.preventDefault();
              e.stopPropagation();
              onOpen(e);
            }
          : undefined
      }
    >
      <Icon name="clock" size={12} stroke={1.75} />
      <span>{items.length} recent</span>
    </button>
  );
}

/** The sessions you snoozed, as one pill: a click drops a menu to open or wake them. */
function SnoozedPill({ items, now, onOpen, measure }: { items: BarSnoozed[]; now: number; onOpen?: (e: MouseEvent<HTMLButtonElement>) => void; measure?: boolean }) {
  const place = usePlaceWords();
  return (
    <button
      type="button"
      className="ca-hud-pill is-snoozed"
      title={measure ? undefined : `${items.length === 1 ? '1 session' : `${items.length} sessions`} snoozed, back on the bar when their time comes:\n${items.map((i) => `${i.title} · ${place(i.project, i.callsign)} · until ${untilWords(i.snoozedUntil, now)}`).join('\n')}`}
      data-snoozed={measure ? '' : undefined}
      onClick={onOpen}
      onContextMenu={
        onOpen
          ? (e) => {
              e.preventDefault();
              e.stopPropagation();
              onOpen(e);
            }
          : undefined
      }
    >
      <Icon name="zzz" size={12} stroke={1.75} />
      <span>{items.length} snoozed</span>
    </button>
  );
}

/** The todos ready to start, as one pill beside the reviews: a click opens the quick panel ready
 *  to start one (the only one, or the one picked from its menu). */
function TodosPill({ todos, onOpen, onMenu, measure }: { todos: BarTodo[]; onOpen?: (e: MouseEvent<HTMLButtonElement>) => void; onMenu?: (e: MouseEvent<HTMLButtonElement>) => void; measure?: boolean }) {
  const place = usePlaceWords();
  return (
    <button
      type="button"
      className="ca-hud-pill is-todos"
      title={measure ? undefined : `${todos.length === 1 ? '1 todo' : `${todos.length} todos`} ready to start:\n${todos.map((t) => todoLabel(t, place)).join('\n')}`}
      data-todos={measure ? '' : undefined}
      onClick={onOpen}
      onContextMenu={
        onMenu
          ? (e) => {
              e.preventDefault();
              e.stopPropagation();
              onMenu(e);
            }
          : undefined
      }
    >
      <Icon name="list-check" size={12} stroke={1.75} />
      <span>{todos.length} ready</span>
    </button>
  );
}

/** A todo as its menu item and tooltip line say it: its title, where it starts, and when it's
 *  due. */
function todoLabel(t: BarTodo, place: PlaceWords): string {
  const where = t.callsign ? place(t.project, t.callsign) : t.project;
  const due = t.due ? dueWords(t.due, Date.now())?.text : undefined;
  return [t.title || 'Untitled', where, due && due[0].toLowerCase() + due.slice(1)].filter(Boolean).join(' · ');
}

/** The project the bar shows, beside the mark. Other projects' sessions that need you badge it,
 *  and their finished turns put a dot on it, so nothing waits unseen behind it. */
function ProjectButton({ name, others, open, onOpen, measure }: { name: string; others: WaitCounts; open?: boolean; onOpen?: (e: MouseEvent<HTMLButtonElement>) => void; measure?: boolean }) {
  const note = others.needs > 0 ? `${others.needs} in other projects ${others.needs === 1 ? 'needs' : 'need'} you` : others.unread > 0 ? `${others.unread} finished in other projects` : '';
  return (
    <button
      type="button"
      className="ca-hud-project"
      title={measure ? undefined : `Project the bar shows${note ? `. ${note}.` : ''}`}
      aria-haspopup="menu"
      aria-expanded={open}
      data-project={measure ? '' : undefined}
      onClick={onOpen}
      onContextMenu={
        onOpen
          ? (e) => {
              e.preventDefault();
              e.stopPropagation();
              onOpen(e);
            }
          : undefined
      }
    >
      <span>{name}</span>
      {others.needs > 0 ? <span className="ca-projswitch-badge">{others.needs}</span> : others.unread > 0 ? <span className="ca-projswitch-dot" /> : null}
      <Icon name="chevron-down" size={12} />
    </button>
  );
}

/** What went wrong opening something, or what a send from the peek did that you should know
 *  (the prompt is on the clipboard, say), in the place of the pill it came from. A click clears it. */
function NotePill({ message, tone: kind, onDismiss }: { message: string; tone: 'error' | 'info'; onDismiss: () => void }) {
  return (
    <button type="button" className={`ca-hud-pill ${kind === 'error' ? 'is-failed' : 'is-info'}`} role={kind === 'error' ? 'alert' : 'status'} title={message} onClick={onDismiss}>
      <Icon name={kind === 'error' ? 'alert' : 'check'} size={12} stroke={1.75} />
      <span>{message}</span>
    </button>
  );
}

/** Where a menu dropped from a pill goes: under its left edge. */
function below(e: MouseEvent<HTMLElement>): { x: number; y: number } {
  const r = e.currentTarget.getBoundingClientRect();
  return { x: r.left, y: r.bottom + 4 };
}

export function Bar() {
  const [engineFeed, setFeed] = useState<BarFeed | null>(null);
  // Changes made in any window that the engine hasn't shown yet (ahead.ts): a pill marked seen in
  // the peek goes at once.
  const ahead = useAhead();
  const feed = useMemo(() => (engineFeed ? barAhead(engineFeed, ahead) : null), [engineFeed, ahead]);
  useEffect(() => {
    if (engineFeed) observe(barView(engineFeed));
  }, [engineFeed]);
  const [fits, setFits] = useState<GroupFit[] | null>(null);
  // A failed open, or a note from the peek, shown on the pill it came from (a session's key, or
  // `fold:<group>` for a folded pill's menu) while the rest of the bar stays as it was.
  const [failed, setFailed] = useState<{ anchor: string; message: string; tone?: 'error' | 'info' } | null>(null);
  // The session the peek shows, and whether it has the keyboard.
  const [peeked, setPeeked] = useState<PeekShown>({ key: null, engaged: false });
  const peekedNow = useRef(peeked);
  peekedNow.current = peeked;
  const hoverTimer = useRef<number | null>(null);
  // A click on a pill waiting to see whether it's the first of two.
  const clickTimer = useRef<number | null>(null);
  // Where the button went down on a session's pill, and whether it has been dragged off the bar
  // since: then the click it ends in isn't one.
  const press = useRef<{ key: string; x: number; y: number; dragged: boolean } | null>(null);
  // The session whose card is being dragged onto the desktop (desktop.rs `drag_out`).
  const [dragging, setDragging] = useState<string | null>(null);
  useEffect(() => onDesktopDragEnd(() => setDragging(null)), []);
  // What's pinned to the desktop, for the pill menu's Pin to desktop.
  const onDesktop = menus.useDesktopPins();
  // The editors, once a pill's menu has fetched them, for their icons in the next one.
  const [apps, setApps] = useState<Apps | undefined>(undefined);
  const icons = useEditorIcons(apps);
  useEffect(() => () => {
    if (clickTimer.current != null) window.clearTimeout(clickTimer.current);
  }, []);
  // The session a pending hover is for.
  const hoverFor = useRef<string | null>(null);
  // Where the pointer last moved on the bar, in screen points.
  const pointerAt = useRef<{ x: number; y: number } | null>(null);
  // Fonts load after the first layout; pills are measured again once they have.
  const [fontsReady, setFontsReady] = useState(false);
  const [project, setProject] = useState(() => readStored(BAR_PROJECT_KEY, asText, ALL_PROJECTS));
  const [projectMenuOpen, setProjectMenuOpen] = useState(false);
  // How many of the bar's menus are up: docked in the notch, it stays open under them.
  const [menusUp, setMenusUp] = useState(0);
  const popup = (spec: MenuSpec | Promise<MenuSpec>, at?: { x: number; y: number }) => {
    setMenusUp((n) => n + 1);
    return Promise.resolve(spec)
      .then((items) => showContextMenu(items, at))
      .finally(() => setMenusUp((n) => n - 1));
  };
  // Floating, or docked in the notch (bar.rs), with the notch's size.
  const [place, setPlace] = useState<BarModeView | null>(null);
  useEffect(() => {
    void barMode().then((v) => setPlace((current) => current ?? v));
    return onBarMode(setPlace);
  }, []);
  const notch = place?.mode === 'notch' ? place.notch : null;
  const keepOpen = place?.keepOpen ?? false;
  /** Pins the docked bar open or lets it close; it shows at once, and Codealong saves it. */
  const pinOpen = (keep: boolean) => {
    setPlace((p) => (p ? { ...p, keepOpen: keep } : p));
    void setNotchOpen(keep);
  };
  const now = useNow(30_000);
  const hud = useRef<HTMLDivElement>(null);
  const measure = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; moved: boolean } | null>(null);

  // The engine sends the bar a small feed, and only when it changes, rather than the board.
  const feedArrived = useRef<number | null>(null);
  usePerf(engineFeed, feedArrived);
  useEffect(() => {
    const stop = onBarFeed((next) => {
      feedArrived.current = performance.now();
      setFeed(next);
    });
    getBarFeed()
      .then((first) => setFeed((current) => current ?? first))
      // The engine pushes the feed as it changes, so one that didn't read comes with the next.
      .catch(() => {});
    return stop;
  }, []);

  useEffect(() => {
    void document.fonts.ready.then(() => setFontsReady(true));
  }, []);

  useEffect(() => {
    const offShown = onPeekShown(setPeeked);
    const offNote = onBarNote((n) => setFailed({ anchor: n.key, message: n.message, tone: n.tone }));
    return () => {
      offShown();
      offNote();
    };
  }, []);

  // The theme from config.toml, as in the board's windows.
  const theme = feed?.theme ?? 'system';
  const logoStatus = useLogo(feed?.logoStatus);
  // What the mark plays as something happens (mark.ts).
  const moment = useMarkMoment(feed, ahead);
  // Docked in the notch it's dark whatever the theme, as the notch is.
  const docked = notch !== null;
  useEffect(() => {
    applyTheme(docked ? 'dark' : theme);
    document.documentElement.classList.toggle('is-notch', docked);
  }, [theme, docked]);

  useEffect(() => {
    if (!failed) return;
    const id = window.setTimeout(() => setFailed(null), 6000);
    return () => window.clearTimeout(id);
  }, [failed]);

  // The project picked in the menu, while it's still one; the menu needs two to choose from.
  const projects = useMemo(() => feed?.projects ?? [], [feed]);
  const picking = projects.length > 1;
  const current = picking && projects.includes(project) ? project : ALL_PROJECTS;
  // A session's worktree is said after its project while the bar lists more than one.
  const shownProject = picking ? current : null;
  const placeOf: PlaceWords = (p, callsign) => placeWords(p, callsign, shownProject);
  const allItems = useMemo(() => feed?.items ?? [], [feed]);
  const items = useMemo(() => allItems.filter((i) => inProject(i, current)), [allItems, current]);
  const recent = useMemo(() => (feed?.recent ?? []).filter((i) => inProject(i, current)), [feed, current]);
  const snoozed = useMemo(() => (feed?.snoozed ?? []).filter((i) => inProject(i, current)), [feed, current]);
  const reviews = useMemo(() => (feed?.reviews ?? []).filter((r) => current === ALL_PROJECTS || r.project === current), [feed, current]);
  const todos = useMemo(() => (feed?.todos ?? []).filter((t) => current === ALL_PROJECTS || t.project === current), [feed, current]);
  const waits = useMemo(() => projectWaits(allItems, projects), [allItems, projects]);
  const others = useMemo(() => elsewhereWaits(waits, projects, current), [waits, projects, current]);
  const byGroup = useMemo(() => {
    const out: Record<Group, BarItem[]> = { needs: [], unread: [], working: [] };
    for (const i of items) out[i.group].push(i);
    return out;
  }, [items]);

  // Docked in the notch, the session shown under the pills (NotchSession.tsx), in place of the
  // peek: which one, what it works through, and its keys.
  const onBar = useMemo(() => new Set([...allItems, ...(feed?.recent ?? []), ...(feed?.snoozed ?? [])].map((i) => i.key)), [allItems, feed]);
  const stage = useNotchStage(items, reviews, (key) => onBar.has(key) || reviews.some((r) => r.key === key), notch !== null);
  // The shortcut that goes through what waits on you, docked in the notch: the notch opens on the
  // session that waits (or puts it away), and with nothing waiting, on the end of what waits,
  // which says so, even with sessions running, where a click on the notch would open one. The
  // waiting window is for a bar that isn't docked.
  const pressNow = useLatest(stage.workThrough);
  const dockedNow = useLatest(notch !== null);
  useEffect(
    () =>
      onWaitingShortcut(() => {
        if (dockedNow.current) pressNow.current();
        else void showWaiting();
      }),
    [pressNow, dockedNow],
  );
  // The session the peek shows, or the notch: its pill is marked.
  const shownKey = notch ? (stage.open && stage.kind !== 'end' ? stage.key : null) : peeked.key;

  // Every pill measured at full size, then folded until the bar fits the screen.
  useLayoutEffect(() => {
    const el = measure.current;
    if (!el) return;
    const width = (sel: string) => el.querySelector<HTMLElement>(sel)?.getBoundingClientRect().width ?? 0;
    const pills: Record<Group, number[]> = { needs: [], unread: [], working: [] };
    el.querySelectorAll<HTMLElement>('[data-pill]').forEach((p) => pills[p.dataset.pill as Group].push(p.getBoundingClientRect().width));
    const fold = { needs: width('[data-fold="needs"]'), unread: width('[data-fold="unread"]'), working: width('[data-fold="working"]') };
    const room = Math.min(MAX_WIDTH, window.screen.availWidth - SCREEN_ROOM);
    setFits(
      fitGroups(
        {
          pills,
          fold,
          mark: MARK,
          project: picking ? width('[data-project]') : 0,
          snoozed: snoozed.length ? width('[data-snoozed]') : 0,
          recent: recent.length ? width('[data-recent]') : 0,
          reviews: reviews.length ? width('[data-reviews]') : 0,
          todos: todos.length ? width('[data-todos]') : 0,
          gap: GAP,
          divider: DIVIDER,
          padding: PADDING,
        },
        room,
      ),
    );
  }, [items, snoozed, recent, reviews, todos, picking, current, others, fontsReady]);

  // The window follows the bar's size: after every render, since WebKit holds back resize
  // notifications while the window is hidden, and on resizes renders don't cause (fonts).
  const reported = useRef('');
  const report = useCallback(() => {
    const r = hud.current?.getBoundingClientRect();
    if (!r) return;
    const size = [Math.ceil(r.width) + 2 * MARGIN, Math.ceil(r.height) + 2 * MARGIN];
    if (size.join('x') === reported.current) return;
    reported.current = size.join('x');
    void fitBar(size[0], size[1]);
  }, []);
  useLayoutEffect(report);
  useEffect(() => {
    if (!hud.current) return;
    const observer = new ResizeObserver(report);
    observer.observe(hud.current);
    return () => observer.disconnect();
  }, [report]);

  const stopHover = () => {
    if (hoverTimer.current != null) window.clearTimeout(hoverTimer.current);
    hoverTimer.current = null;
    hoverFor.current = null;
  };

  /** Where a pill is in the window, for the peek to open by. */
  const rectOf = (el: Element | null): PillRect | null => {
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: r.left, y: r.top, w: r.width, h: r.height };
  };

  /** The pointer came onto a session's pill or moved on it: after a moment the peek shows it,
   *  sooner when the peek is already up. While you're typing in the peek, it stays on its session.
   *  Only a pointer that moved points at a pill: when sessions change groups the pills shift, and
   *  the one that slides under a still pointer doesn't take the peek. */
  const hover = (i: BarItem, e: PointerEvent<HTMLElement>) => {
    // A pill pressed is on its way to a click or a drag, not a look. Docked, there's no peek: a
    // click shows the session in the notch.
    if (e.buttons & 1 || notch) return;
    const at = pointerAt.current;
    if ((at && at.x === e.screenX && at.y === e.screenY) || hoverFor.current === i.key) return;
    stopHover();
    const p = peekedNow.current;
    if (p.key === i.key || (p.engaged && p.key)) return;
    const el = e.currentTarget;
    hoverFor.current = i.key;
    hoverTimer.current = window.setTimeout(
      () => {
        hoverTimer.current = null;
        hoverFor.current = null;
        // A hover has nowhere to say a peek didn't open; the next hover asks again.
        void peekOpen(i.key, rectOf(el)).catch(() => {});
      },
      p.key ? PEEK_SWITCH_MS : PEEK_AFTER_MS,
    );
  };

  /** The peek with the keyboard, ready to answer the session, or with `rename` to change its
   *  name; docked, the notch opens on it. */
  const reply = (i: BarItem, rename = false) => {
    stopHover();
    if (notch) return stage.show(i.key, rename);
    const el = hud.current?.querySelector(`[data-key="${CSS.escape(i.key)}"]`) ?? null;
    peekOpen(i.key, rectOf(el), true, rename).catch((e: unknown) => setFailed({ anchor: i.key, message: errorText(e) }));
  };

  /** Opens a session where it lives (open.ts), or with `target` another of its worktree's
   *  targets; what goes wrong shows on the pill `anchor`. A session on another device has nothing
   *  to open here (its feed has no open target, peerfeed.rs `shareable`): it shows in Codealong,
   *  where its panel reads it from that device. */
  const open = (i: BarItem | BarRecent | BarSnoozed, anchor = i.key, target = i.openTarget) => {
    stopHover();
    if (!target) {
      reveal(i);
      return;
    }
    const w = target === i.openTarget ? { laneId: i.laneId, target, listed: i.target } : { laneId: i.laneId, target };
    openWhere(w, {
      report: { error: (message) => setFailed({ anchor, message }), info: (message) => setFailed({ anchor, message, tone: 'info' }) },
      from: 'panel',
      before: () => {
        if (peekedNow.current.key) void peekHide();
        // The keyboard goes back before the session's own app comes forward.
        stage.close();
      },
    });
  };

  /** Puts the peek and the notch's session away, for Codealong's window to come forward. */
  const away = () => {
    stopHover();
    if (peekedNow.current.key) void peekHide();
    stage.close();
  };

  /** Shows the session in Codealong's window: what a click on its pill, or an item in the bar's
   *  menus, does. */
  const reveal = (i: BarItem | BarRecent | BarSnoozed) => {
    away();
    void showMain({ kind: 'session', laneId: i.laneId, key: i.key });
  };
  // A view's keys, while the notch's session has the keyboard.
  useViewKeys(away);

  /** What picking a session from one of the bar's menus does: it shows in Codealong's window, or,
   *  docked, in the notch, as a click on its pill has it. */
  const pick = (i: BarItem | BarRecent) => (notch ? stage.show(i.key) : reveal(i));

  /** A click on a session's pill shows it in Codealong, as a click on its row there selects it; a
   *  double-click opens it where it lives. The first click waits for a second, so a double-click
   *  doesn't bring Codealong forward on its way. Docked, a click shows it in the notch at once,
   *  as a click on a row shows its panel, and a click on the pill of the one shown puts it away. */
  const clickPill = (i: BarItem, e: MouseEvent<HTMLButtonElement>) => {
    if (clickTimer.current != null) window.clearTimeout(clickTimer.current);
    clickTimer.current = null;
    stopHover();
    const p = press.current;
    press.current = null;
    if (p?.dragged) return;
    if (e.detail >= 2) {
      open(i);
      return;
    }
    if (notch) {
      stage.toggle(i.key);
      return;
    }
    clickTimer.current = window.setTimeout(() => {
      clickTimer.current = null;
      reveal(i);
    }, CLICK_WAIT_MS);
  };

  const pressPill = (i: BarItem, e: PointerEvent<HTMLElement>) => {
    press.current = e.button === 0 ? { key: i.key, x: e.screenX, y: e.screenY, dragged: false } : null;
  };

  /** A pill moved far enough with the button down comes off the bar: its session's card follows
   *  the pointer onto the desktop, pinned where you let go, or put back if that's on the bar. */
  const dragPill = (i: BarItem, e: PointerEvent<HTMLElement>) => {
    const p = press.current;
    if (!p || p.key !== i.key || p.dragged || !(e.buttons & 1) || Math.abs(e.screenX - p.x) + Math.abs(e.screenY - p.y) < DRAG_AFTER) return;
    p.dragged = true;
    stopHover();
    if (clickTimer.current != null) window.clearTimeout(clickTimer.current);
    clickTimer.current = null;
    if (peekedNow.current.key) void peekHide();
    stage.close();
    setDragging(i.key);
    dragToDesktop({ ref: 'session', key: i.key }).catch((err: unknown) => {
      setDragging(null);
      setFailed({ anchor: i.key, message: errorText(err) });
    });
  };

  /** Snoozing or waking takes the sessions off the bar or puts them back; the peek goes away
   *  from a session leaving it. What goes wrong shows where the menu came from. */
  const snoozeDone = (keys: string[], anchor: string) => (act: Promise<void>) => {
    const p = peekedNow.current.key;
    if (p && keys.includes(p)) void peekHide();
    // Several leave the bar at once: the notch closes rather than go on to one of them.
    if (stage.key && keys.includes(stage.key)) stage.close();
    act.catch((e: unknown) => setFailed({ anchor, message: errorText(e) }));
  };

  /** Marks sessions seen: their pills go at once, and come back with why if the engine can't. */
  const markSeen = (keys: string[], anchor: string) => {
    if (stage.key && keys.includes(stage.key)) stage.close();
    seenAhead(keys).catch((e: unknown) => setFailed({ anchor, message: errorText(e) }));
  };

  /** A pill's menu: the session's, as every window has it (menus.ts). The feed has only what the
   *  pills show, so the rest of the session is asked of the engine as the menu opens. */
  const sessionMenu = (i: BarItem) => {
    const report: Report = {
      error: (message) => setFailed({ anchor: i.key, message }),
      info: (message) => setFailed({ anchor: i.key, message, tone: 'info' }),
    };
    const spec = sessionAt(i.key).then((at): MenuSpec => {
      if (!at) return [{ label: `${i.title} is no longer on the board`, run: () => {}, enabled: false }];
      setApps(at.apps);
      return menus.sessionMenu(at.lane, at.session, {
        report,
        open: (_, target) => open(i, i.key, target),
        terminal: at.apps.terminal,
        heading: true,
        reveal: () => reveal(i),
        reply: () => reply(i),
        rename: () => reply(i, true),
        editors: { apps: at.apps, icons },
        desktop: onDesktop,
        // The peek goes with a session that leaves the bar; the notch goes on to the next.
        left: (done) => {
          if (peekedNow.current.key === i.key) void peekHide();
          stage.left(i.key, done);
        },
      });
    });
    void popup(spec).catch(failedOn(i.key));
  };

  /** Shows what went wrong on the pill it came from. */
  const failedOn = (anchor: string) => (e: unknown) => setFailed({ anchor, message: errorText(e) });

  const foldMenu = (group: Group, folded: BarItem[], e: MouseEvent<HTMLButtonElement>) => {
    const menu: MenuSpec = folded.map((i) => ({ label: `${i.title} · ${placeOf(i.project, i.callsign)}`, checked: i.key === shownKey || undefined, run: () => pick(i) }));
    const keys = folded.map((i) => i.key);
    if (group !== 'working') menu.push('separator');
    if (group === 'unread') menu.push({ label: 'Mark these as seen', run: () => markSeen(keys, `fold:${group}`) });
    if (group !== 'working') menu.push(snoozeMenu(keys, Date.now(), { label: 'Snooze these', onDone: snoozeDone(keys, `fold:${group}`) }));
    void popup(menu, below(e));
  };

  /** Wakes a snoozed session and opens it: you're taking it up now, so it doesn't come back later.
   *  The engine hears the wake before the open's mark seen, which comes once the session is open. */
  const wakeAndOpen = (i: BarSnoozed) => {
    snoozeAhead([i.key], null).catch((e: unknown) => setFailed({ anchor: 'snoozed', message: errorText(e) }));
    open(i, 'snoozed');
  };

  const snoozedMenu = (e: MouseEvent<HTMLButtonElement>) => {
    const t = Date.now();
    const terminal = feed?.terminal ?? 'Terminal';
    const menu: MenuSpec = snoozed.map((i) => ({
      label: `${i.title} · ${placeOf(i.project, i.callsign)} · until ${untilWords(i.snoozedUntil, t)}`,
      items: [
        { label: 'Show in Codealong', run: () => reveal(i) },
        { label: i.target?.label ?? openWords(i.openTarget, i.terminal ?? terminal), run: () => wakeAndOpen(i) },
        'separator',
        { label: 'Wake now', run: () => snoozeDone([i.key], 'snoozed')(snoozeAhead([i.key], null)) },
        snoozeMenu([i.key], t, { label: 'Snooze instead', onDone: snoozeDone([i.key], 'snoozed') }),
      ],
    }));
    if (snoozed.length > 1) {
      const keys = snoozed.map((i) => i.key);
      menu.push('separator', { label: 'Wake all', run: () => snoozeDone(keys, 'snoozed')(snoozeAhead(keys, null)) });
    }
    void popup(menu, below(e));
  };

  const recentMenu = (e: MouseEvent<HTMLButtonElement>) => {
    void popup(
      recent.map((i) => ({ label: `${i.title} · ${placeOf(i.project, i.callsign)} · ${ago(i.since, now)}`, run: () => pick(i) })),
      below(e),
    );
  };

  /** The reviews pill: docked in the notch, the one PR opens under the pills, as a session's pill
   *  opens its session, and several drop a menu to pick from; floating, it shows the PR in the
   *  waiting window, with the box that reviews it. */
  const reviewsMenu = (e: MouseEvent<HTMLButtonElement>) => {
    const to = (p: BarReview) => (notch ? stage.show(p.key) : void showWaiting({ key: p.key }));
    if (reviews.length === 1) {
      if (notch) stage.toggle(reviews[0].key);
      else to(reviews[0]);
      return;
    }
    void popup(
      reviews.map((p) => ({ label: reviewLabel(p, placeOf), checked: p.key === shownKey || undefined, run: () => to(p) })),
      below(e),
    );
  };

  /** Opens the quick panel ready to start a session for a todo: its prompt, in its worktree. */
  const startTodo = (t: BarTodo) => void showQuick({ todo: t.id, project: t.project || null });

  const todosMenu = (e: MouseEvent<HTMLButtonElement>) => {
    void popup(
      [
        ...todos.map((t) => ({ label: todoLabel(t, placeOf), run: () => startTodo(t) })),
        'separator',
        { label: 'Show todos in Codealong', run: () => void showMain({ kind: 'todo', id: todos[0].id }) },
      ],
      below(e),
    );
  };

  const openTodos = (e: MouseEvent<HTMLButtonElement>) => {
    if (todos.length === 1) startTodo(todos[0]);
    else todosMenu(e);
  };

  const pickProject = (name: string) => {
    setProject(name);
    writeStored(BAR_PROJECT_KEY, asText, name);
  };

  const projectMenu = (e: MouseEvent<HTMLButtonElement>) => {
    const item = (name: string): MenuAction => {
      const w = waits[name];
      const said = [w?.needs ? `${w.needs} ${w.needs === 1 ? 'needs' : 'need'} you` : '', w?.unread ? `${w.unread} finished` : ''].filter(Boolean);
      return { label: [name, ...said].join(' · '), checked: name === current, run: () => pickProject(name) };
    };
    setProjectMenuOpen(true);
    void popup([...projects.map(item), 'separator', item(ALL_PROJECTS)], below(e)).finally(() => setProjectMenuOpen(false));
  };

  const barMenu = () => {
    const unread = byGroup.unread;
    void popup([
      { label: 'Show Codealong', run: () => void showMain() },
      ...(unread.length ? [{ label: 'Mark all as seen', run: () => markSeen(unread.map((i) => i.key), 'fold:unread') }] : []),
      'separator',
      ...(docked ? [{ label: 'Keep the notch open', checked: keepOpen, run: () => pinOpen(!keepOpen) }] : []),
      docked ? { label: 'Float the bar instead', run: () => void setBarMode('floating') } : { label: 'Put the bar in the notch', run: () => void setBarMode('notch') },
      { label: docked ? 'Hide the bar' : 'Hide floating bar', run: () => void setBarMode('off') },
    ]);
  };

  // What the bar shows, in sections with a divider between each. A failed open takes the place
  // of the pill it came from; one that has since left the bar goes at the end.
  const sections: ReactNode[][] = [];
  let failedShown = false;
  const failedPill = () => {
    failedShown = true;
    return <NotePill key="failed" message={failed?.message ?? ''} tone={failed?.tone ?? 'error'} onDismiss={() => setFailed(null)} />;
  };
  // Until the engine's first scan has finished, its feed has no sessions yet.
  if (!feed || feed.loading) {
    sections.push([
      <span key="note" className="ca-hud-note">
        Reading sessions…
      </span>,
    ]);
  } else if (items.length === 0 && snoozed.length === 0 && recent.length === 0 && reviews.length === 0 && todos.length === 0) {
    sections.push([
      <span key="note" className="ca-hud-note">
        {current === ALL_PROJECTS ? 'No agents running' : `No agents running in ${current}`}
      </span>,
    ]);
  } else {
    for (const group of GROUPS) {
      const list = byGroup[group];
      if (list.length === 0) continue;
      const f = fits?.find((x) => x.group === group);
      const shown = list.slice(0, f?.shown ?? list.length);
      const folded = list.slice(shown.length);
      sections.push([
        ...shown.map((i) =>
          failed?.anchor === i.key ? (
            failedPill()
          ) : (
            <SessionPill key={i.key} item={i} now={now} peeked={shownKey === i.key} told={docked} dragged={dragging === i.key} onOpen={(e) => clickPill(i, e)} onMenu={() => sessionMenu(i)} onHover={(e) => hover(i, e)} onLeave={stopHover} onPress={(e) => pressPill(i, e)} onDrag={(e) => dragPill(i, e)} />
          ),
        ),
        ...(folded.length
          ? [
              failed?.anchor === `fold:${group}` ? (
                failedPill()
              ) : (
                <FoldPill
                  key={`fold-${group}`}
                  group={group}
                  label={shown.length ? `+${folded.length}` : String(folded.length)}
                  title={`${folded.length} ${shown.length ? 'more ' : ''}${GROUP_WORD[group]}:\n${folded.map((i) => i.title).join('\n')}`}
                  peeked={folded.some((i) => i.key === shownKey)}
                  onOpen={(e) => foldMenu(group, folded, e)}
                />
              ),
            ]
          : []),
      ]);
    }
    const last = [
      ...(snoozed.length ? [failed?.anchor === 'snoozed' ? failedPill() : <SnoozedPill key="snoozed" items={snoozed} now={now} onOpen={snoozedMenu} />] : []),
      ...(recent.length ? [failed?.anchor === 'recent' ? failedPill() : <RecentPill key="recent" items={recent} now={now} onOpen={recentMenu} />] : []),
      ...(reviews.length ? [failed?.anchor === 'reviews' ? failedPill() : <ReviewsPill key="reviews" prs={reviews} peeked={reviews.some((p) => p.key === shownKey)} onOpen={reviewsMenu} />] : []),
      ...(todos.length ? [<TodosPill key="todos" todos={todos} onOpen={openTodos} onMenu={todosMenu} />] : []),
    ];
    if (last.length) sections.push(last);
  }
  if (failed && !failedShown) sections.push([failedPill()]);

  const measured = (
    <div ref={measure} className="ca-hud-measure" aria-hidden="true">
      {items.map((i) => (
        <SessionPill key={i.key} item={i} now={now} measure />
      ))}
      {GROUPS.map((g) => (
        <FoldPill key={g} group={g} label={FOLD_SAMPLE} measure />
      ))}
      {snoozed.length > 0 && <SnoozedPill items={snoozed} now={now} measure />}
      {recent.length > 0 && <RecentPill items={recent} now={now} measure />}
      {reviews.length > 0 && <ReviewsPill prs={reviews} measure />}
      {todos.length > 0 && <TodosPill todos={todos} measure />}
      {picking && <ProjectButton name={current} others={others} measure />}
    </div>
  );

  if (notch) {
    return (
      <ProjectShown.Provider value={shownProject}>
        <NotchShell
          notch={notch}
          logoStatus={logoStatus}
          moment={moment}
          counts={{ needs: byGroup.needs.length, reviews: reviews.length, unread: byGroup.unread.length, working: byGroup.working.length }}
          room={Math.min(MAX_WIDTH, window.screen.availWidth - SCREEN_ROOM)}
          screen={{ w: window.screen.availWidth, h: window.screen.height }}
          zoom={feed?.zoom}
          project={picking ? <ProjectButton name={current} others={others} open={projectMenuOpen} onOpen={projectMenu} /> : null}
          held={projectMenuOpen || menusUp > 0 || dragging !== null}
          session={
            stage.key && stage.kind === 'review' ? (
              <NotchReview
                prKey={stage.key}
                now={now}
                zoom={feed?.zoom}
                queue={stage.queue}
                notice={stage.notice}
                onBack={stage.back}
                keys={stage.keys}
                onKeys={stage.takeKeys}
                onShow={stage.show}
                from={stage.from}
                onReturn={stage.returnFrom}
                onClose={stage.close}
                onLeft={stage.left}
                onFailed={stage.failed}
                onGone={(key) => {
                  // One shown from a session's chip goes back to the session, saying so there.
                  if (stage.from) {
                    stage.returnFrom(key, `${key} is no longer on the board.`);
                    return;
                  }
                  stage.close();
                  setFailed({ anchor: 'reviews', message: `${key} no longer waits on your review.` });
                }}
              />
            ) : stage.key && stage.kind === 'end' ? (
              <QueueEnd said={stage.ended} className="ca-notch-end" />
            ) : stage.key ? (
              <NotchSession
                sessionKey={stage.key}
                rename={stage.rename}
                now={now}
                zoom={feed?.zoom}
                queue={stage.queue}
                notice={stage.notice}
                onBack={stage.back}
                keys={stage.keys}
                onKeys={stage.takeKeys}
                onShow={stage.show}
                onShowPr={stage.showPr}
                onClose={stage.close}
                onNext={stage.next}
                onLeft={stage.left}
                onUnsent={stage.unsent}
                onGone={(key) => {
                  stage.close();
                  setFailed({ anchor: key, message: 'That session is no longer on the board.' });
                }}
              />
            ) : null
          }
          sessionOpen={stage.open}
          sessionEnd={stage.kind === 'end'}
          sessionKeys={stage.keys}
          onSessionLeft={stage.close}
          onPress={stage.press}
          onMove={stage.close}
          onCount={(kind) => {
            const first = kind === 'reviews' ? reviews[0] : byGroup[kind][0];
            if (first) stage.show(first.key);
          }}
          keepOpen={keepOpen}
          onKeepOpen={pinOpen}
          onMark={() => {
            // The keyboard goes to Codealong's window with it.
            stage.close();
            void showMain();
          }}
          onMenu={barMenu}
        >
          {sections.flatMap((section, i) => [...(i > 0 ? [<span key={`divider-${i}`} className="ca-hud-divider" aria-hidden="true" />] : []), ...section])}
        </NotchShell>
        {measured}
      </ProjectShown.Provider>
    );
  }

  return (
    <ProjectShown.Provider value={shownProject}>
      <div
        ref={hud}
        className="ca-hud"
        data-tauri-drag-region="deep"
        onContextMenu={(e) => {
          e.preventDefault();
          barMenu();
        }}
        // After the pills' own handlers, which compare with where it was before.
        onPointerMove={(e) => {
          pointerAt.current = { x: e.screenX, y: e.screenY };
        }}
        onPointerLeave={() => {
          pointerAt.current = null;
        }}
      >
        <button
          type="button"
          className="ca-hud-mark"
          title={`Show Codealong · ${LOGO_LABEL[logoStatus]}`}
          onPointerDown={(e) => {
            if (e.button === 0) drag.current = { x: e.screenX, y: e.screenY, moved: false };
          }}
          onPointerMove={(e) => {
            // The mark doubles as the bar's handle: pressing and moving drags the window.
            const d = drag.current;
            if (!d || d.moved || !(e.buttons & 1) || Math.abs(e.screenX - d.x) + Math.abs(e.screenY - d.y) < 4) return;
            d.moved = true;
            void getCurrentWindow().startDragging();
          }}
          onClick={() => {
            const moved = drag.current?.moved;
            drag.current = null;
            if (!moved) void showMain();
          }}
        >
          <MarkLive size={16} status={logoStatus} moment={moment} />
        </button>
        {picking && <ProjectButton name={current} others={others} open={projectMenuOpen} onOpen={projectMenu} />}
        {sections.flatMap((section, i) => [<span key={`divider-${i}`} className="ca-hud-divider" aria-hidden="true" />, ...section])}
      </div>
      {measured}
    </ProjectShown.Provider>
  );
}

// The floating bar's page: pills for the sessions that need you, whose turn ended, or that are
// running, one for PRs waiting on your review, and one for todos ready to start, in the project
// picked beside the mark or in all of them. The engine keeps the window up and sends it what to
// show (barfeed.rs); the page sizes the window to itself. Pills that don't fit fold into one per
// group that lists them. Hovering a session's pill shows it in the peek under the bar (peek.rs,
// Peek.tsx), its conversation and a box to answer it; clicking the pill opens the session where
// it lives, right-clicking shows its menu, and the mark brings Codealong forward.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent, type ReactNode } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { fitBar, getBarFeed, markSeen, onBarFeed, onBarNote, onPeekShown, openFromBar, peekHide, peekOpen, setBarOn, showMain, showQuick, type PillRect } from '../api';
import { elsewhereWaits, fitGroups, GROUPS, plain, projectWaits, type Group, type GroupFit, type WaitCounts } from '../bar';
import { showContextMenu, type MenuAction, type MenuSpec } from '../contextMenu';
import { usePerf } from '../perf';
import { ALL_PROJECTS, inProject } from '../quick';
import type { BarFeed, BarItem, BarReview, BarTodo, PeekShown, Status } from '../types';
import { Icon, Mark, STATUS_WORD, StatusGlyph } from './primitives';
import { applyTheme } from './ThemeSwitch';
import { LOGO_LABEL, useLogo } from '../logo';

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
/** Where the project picked in the bar's menu is kept. */
const PROJECT_KEY = 'codealong.bar.project';
/** How long the pointer rests on a session's pill before the peek shows it, and before it moves
 *  to another pill once it's up: long enough that passing over the bar doesn't open it. */
const PEEK_AFTER_MS = 300;
const PEEK_SWITCH_MS = 90;

const GROUP_GLYPH: Record<Group, Status> = { needs: 'needs', unread: 'unread', working: 'working' };
const GROUP_WORD: Record<Group, string> = { needs: 'need you', unread: 'your turn', working: 'working' };

function storedProject(): string {
  try {
    return window.localStorage.getItem(PROJECT_KEY) ?? ALL_PROJECTS;
  } catch {
    return ALL_PROJECTS;
  }
}

function useNow(everyMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), everyMs);
    return () => window.clearInterval(id);
  }, [everyMs]);
  return now;
}

/** The pill's class for a status: errors are red, the rest of Needs you amber. */
function tone(status: Status): string {
  return status === 'error' ? 'is-error' : `is-${status}`;
}

/** The session menu's open item: its lane's label for it, or one from how the session opens
 *  (lanes list targets for their first few sessions only). */
function openLabel(i: BarItem, terminal: string): string {
  if (i.target) return i.target.label;
  const how: Record<string, string> = {
    focus: `Show in ${terminal}`,
    attach: `Attach in ${terminal}`,
    resume: `Resume in ${terminal}`,
    'claude-app': 'Open in Claude app',
    codex: 'Open in Codex',
    antigravity: 'Open Antigravity',
  };
  return how[i.openTarget.split('|')[2]] ?? 'Open';
}

/** What a pill says to a screen reader; the peek shows the rest. */
function sessionLabel(i: BarItem, now: number): string {
  const where = `${STATUS_WORD[i.status]} · ${i.callsign} · ${i.account}`;
  return [i.title, where, i.detail ? plain(i.detail, now) : ''].filter(Boolean).join('. ');
}

interface SessionPillProps {
  item: BarItem;
  now: number;
  /** The peek shows this session. */
  peeked?: boolean;
  onOpen?: () => void;
  onMenu?: () => void;
  onHover?: (el: HTMLElement) => void;
  onLeave?: () => void;
  measure?: boolean;
}

function SessionPill({ item, now, peeked, onOpen, onMenu, onHover, onLeave, measure }: SessionPillProps) {
  return (
    <button
      type="button"
      className={['ca-hud-pill', tone(item.status), peeked && 'is-peeked'].filter(Boolean).join(' ')}
      aria-label={measure ? undefined : sessionLabel(item, now)}
      data-pill={measure ? item.group : undefined}
      data-key={measure ? undefined : item.key}
      onClick={onOpen}
      onPointerEnter={onHover ? (e) => onHover(e.currentTarget) : undefined}
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
      <StatusGlyph status={item.status} title="" />
      <span>{item.title}</span>
    </button>
  );
}

function FoldPill({ group, label, title, onOpen, measure }: { group: Group; label: string; title?: string; onOpen?: (e: MouseEvent<HTMLButtonElement>) => void; measure?: boolean }) {
  return (
    <button
      type="button"
      className={`ca-hud-pill ca-hud-fold is-${group}`}
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

function ReviewsPill({ prs, onOpen, measure }: { prs: BarReview[]; onOpen?: (e: MouseEvent<HTMLButtonElement>) => void; measure?: boolean }) {
  return (
    <button
      type="button"
      className="ca-hud-pill is-reviews"
      title={measure ? undefined : prs.map((p) => `#${p.number} ${p.title} · ${p.repo}`).join('\n')}
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

/** The todos ready to start, as one pill beside the reviews: a click opens the quick panel ready
 *  to start one (the only one, or the one picked from its menu). */
function TodosPill({ todos, onOpen, onMenu, measure }: { todos: BarTodo[]; onOpen?: (e: MouseEvent<HTMLButtonElement>) => void; onMenu?: (e: MouseEvent<HTMLButtonElement>) => void; measure?: boolean }) {
  return (
    <button
      type="button"
      className="ca-hud-pill is-todos"
      title={measure ? undefined : `${todos.length === 1 ? '1 todo' : `${todos.length} todos`} ready to start:\n${todos.map(todoLabel).join('\n')}`}
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

/** A todo as its menu item and tooltip line say it: its title, then where it starts. */
function todoLabel(t: BarTodo): string {
  const where = t.callsign ?? t.project;
  return `${t.title || 'Untitled'}${where ? ` · ${where}` : ''}`;
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
  const [feed, setFeed] = useState<BarFeed | null>(null);
  const [fits, setFits] = useState<GroupFit[] | null>(null);
  // A failed open, or a note from the peek, shown on the pill it came from (a session's key, or
  // `fold:<group>` for a folded pill's menu) while the rest of the bar stays as it was.
  const [failed, setFailed] = useState<{ anchor: string; message: string; tone?: 'error' | 'info' } | null>(null);
  // The session the peek shows, and whether it has the keyboard.
  const [peeked, setPeeked] = useState<PeekShown>({ key: null, engaged: false });
  const peekedNow = useRef(peeked);
  peekedNow.current = peeked;
  const hoverTimer = useRef<number | null>(null);
  // Fonts load after the first layout; pills are measured again once they have.
  const [fontsReady, setFontsReady] = useState(false);
  const [project, setProject] = useState(storedProject);
  const [projectMenuOpen, setProjectMenuOpen] = useState(false);
  const now = useNow(30_000);
  const hud = useRef<HTMLDivElement>(null);
  const measure = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; moved: boolean } | null>(null);

  // The engine sends the bar a small feed, and only when it changes, rather than the board.
  const feedArrived = useRef<number | null>(null);
  usePerf(feed, feedArrived);
  useEffect(() => {
    const stop = onBarFeed((next) => {
      feedArrived.current = performance.now();
      setFeed(next);
    });
    getBarFeed()
      .then((first) => setFeed((current) => current ?? first))
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
  useEffect(() => applyTheme(theme), [theme]);

  useEffect(() => {
    if (!failed) return;
    const id = window.setTimeout(() => setFailed(null), 6000);
    return () => window.clearTimeout(id);
  }, [failed]);

  // The project picked in the menu, while it's still one; the menu needs two to choose from.
  const projects = useMemo(() => feed?.projects ?? [], [feed]);
  const picking = projects.length > 1;
  const current = picking && projects.includes(project) ? project : ALL_PROJECTS;
  const allItems = useMemo(() => feed?.items ?? [], [feed]);
  const items = useMemo(() => allItems.filter((i) => inProject(i, current)), [allItems, current]);
  const reviews = useMemo(() => (feed?.reviews ?? []).filter((r) => current === ALL_PROJECTS || r.project === current), [feed, current]);
  const todos = useMemo(() => (feed?.todos ?? []).filter((t) => current === ALL_PROJECTS || t.project === current), [feed, current]);
  const waits = useMemo(() => projectWaits(allItems, projects), [allItems, projects]);
  const others = useMemo(() => elsewhereWaits(waits, projects, current), [waits, projects, current]);
  const byGroup = useMemo(() => {
    const out: Record<Group, BarItem[]> = { needs: [], unread: [], working: [] };
    for (const i of items) out[i.group].push(i);
    return out;
  }, [items]);

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
          reviews: reviews.length ? width('[data-reviews]') : 0,
          todos: todos.length ? width('[data-todos]') : 0,
          gap: GAP,
          divider: DIVIDER,
          padding: PADDING,
        },
        room,
      ),
    );
  }, [items, reviews, todos, picking, current, others, fontsReady]);

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
  };

  /** Where a pill is in the window, for the peek to open by. */
  const rectOf = (el: Element | null): PillRect | null => {
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: r.left, y: r.top, w: r.width, h: r.height };
  };

  /** The pointer rests on a session's pill: after a moment the peek shows it, sooner when the peek
   *  is already up. While you're typing in the peek, it stays on its session. */
  const hover = (i: BarItem, el: HTMLElement) => {
    stopHover();
    const p = peekedNow.current;
    if (p.key === i.key || (p.engaged && p.key)) return;
    hoverTimer.current = window.setTimeout(
      () => {
        hoverTimer.current = null;
        void peekOpen(i.key, rectOf(el)).catch(() => {});
      },
      p.key ? PEEK_SWITCH_MS : PEEK_AFTER_MS,
    );
  };

  /** The peek with the keyboard, ready to answer the session. */
  const reply = (i: BarItem) => {
    stopHover();
    const el = hud.current?.querySelector(`[data-key="${CSS.escape(i.key)}"]`) ?? null;
    peekOpen(i.key, rectOf(el), true).catch((e: unknown) => setFailed({ anchor: i.key, message: String(e).replace(/^Error: /, '') }));
  };

  const open = (i: BarItem, anchor = i.key) => {
    stopHover();
    if (peekedNow.current.key) void peekHide();
    if (i.target?.disabled) {
      setFailed({ anchor, message: i.target.detail ?? `${i.target.label} isn't available right now.` });
      return;
    }
    openFromBar(i.laneId, i.openTarget).catch((e: unknown) => setFailed({ anchor, message: String(e).replace(/^Error: /, '') }));
  };

  const reveal = (i: BarItem) => void showMain({ kind: 'session', laneId: i.laneId, key: i.key });

  const sessionMenu = (i: BarItem) => {
    void showContextMenu([
      { label: `${i.title} · ${i.callsign}`, run: () => {}, enabled: false },
      'separator',
      { label: openLabel(i, feed?.terminal ?? 'Terminal'), run: () => open(i) },
      { label: 'Reply here', run: () => reply(i) },
      { label: 'Show in Codealong', run: () => reveal(i) },
      ...(i.status === 'unread' ? [{ label: 'Mark as seen', run: () => void markSeen([i.key]) }] : []),
    ]);
  };

  const foldMenu = (group: Group, folded: BarItem[], e: MouseEvent<HTMLButtonElement>) => {
    const menu: MenuSpec = folded.map((i) => ({ label: `${i.title} · ${i.callsign}`, run: () => open(i, `fold:${group}`) }));
    if (group === 'unread') menu.push('separator', { label: 'Mark these as seen', run: () => void markSeen(folded.map((i) => i.key)) });
    void showContextMenu(menu, below(e));
  };

  const reviewsMenu = (e: MouseEvent<HTMLButtonElement>) => {
    if (reviews.length === 1) {
      void showMain({ kind: 'pr', key: reviews[0].key });
      return;
    }
    void showContextMenu(
      reviews.map((p) => ({ label: `#${p.number} ${p.title} · ${p.repo}`, run: () => void showMain({ kind: 'pr', key: p.key }) })),
      below(e),
    );
  };

  /** Opens the quick panel ready to start a session for a todo: its prompt, in its worktree. */
  const startTodo = (t: BarTodo) => void showQuick({ todo: t.id, project: t.project || null });

  const todosMenu = (e: MouseEvent<HTMLButtonElement>) => {
    void showContextMenu(
      [
        ...todos.map((t) => ({ label: todoLabel(t), run: () => startTodo(t) })),
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
    try {
      window.localStorage.setItem(PROJECT_KEY, name);
    } catch {
      // Storage can be unavailable; the choice just won't be remembered.
    }
  };

  const projectMenu = (e: MouseEvent<HTMLButtonElement>) => {
    const item = (name: string): MenuAction => {
      const w = waits[name];
      const said = [w?.needs ? `${w.needs} ${w.needs === 1 ? 'needs' : 'need'} you` : '', w?.unread ? `${w.unread} finished` : ''].filter(Boolean);
      return { label: [name, ...said].join(' · '), checked: name === current, run: () => pickProject(name) };
    };
    setProjectMenuOpen(true);
    void showContextMenu([...projects.map(item), 'separator', item(ALL_PROJECTS)], below(e)).finally(() => setProjectMenuOpen(false));
  };

  const barMenu = () => {
    const unread = byGroup.unread;
    void showContextMenu([
      { label: 'Show Codealong', run: () => void showMain() },
      ...(unread.length ? [{ label: 'Mark all as seen', run: () => void markSeen(unread.map((i) => i.key)) }] : []),
      'separator',
      { label: 'Hide floating bar', run: () => void setBarOn(false) },
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
  if (!feed) {
    sections.push([
      <span key="note" className="ca-hud-note">
        Reading sessions…
      </span>,
    ]);
  } else if (items.length === 0 && reviews.length === 0 && todos.length === 0) {
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
            <SessionPill key={i.key} item={i} now={now} peeked={peeked.key === i.key} onOpen={() => open(i)} onMenu={() => sessionMenu(i)} onHover={(el) => hover(i, el)} onLeave={stopHover} />
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
                  onOpen={(e) => foldMenu(group, folded, e)}
                />
              ),
            ]
          : []),
      ]);
    }
    const last = [
      ...(reviews.length ? [<ReviewsPill key="reviews" prs={reviews} onOpen={reviewsMenu} />] : []),
      ...(todos.length ? [<TodosPill key="todos" todos={todos} onOpen={openTodos} onMenu={todosMenu} />] : []),
    ];
    if (last.length) sections.push(last);
  }
  if (failed && !failedShown) sections.push([failedPill()]);

  return (
    <>
      <div
        ref={hud}
        className="ca-hud"
        data-tauri-drag-region="deep"
        onContextMenu={(e) => {
          e.preventDefault();
          barMenu();
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
          <Mark size={16} status={logoStatus} />
        </button>
        {picking && <ProjectButton name={current} others={others} open={projectMenuOpen} onOpen={projectMenu} />}
        {sections.flatMap((section, i) => [<span key={`divider-${i}`} className="ca-hud-divider" aria-hidden="true" />, ...section])}
      </div>
      <div ref={measure} className="ca-hud-measure" aria-hidden="true">
        {items.map((i) => (
          <SessionPill key={i.key} item={i} now={now} measure />
        ))}
        {GROUPS.map((g) => (
          <FoldPill key={g} group={g} label={FOLD_SAMPLE} measure />
        ))}
        {reviews.length > 0 && <ReviewsPill prs={reviews} measure />}
        {todos.length > 0 && <TodosPill todos={todos} measure />}
        {picking && <ProjectButton name={current} others={others} measure />}
      </div>
    </>
  );
}

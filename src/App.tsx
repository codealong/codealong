// The main window. One project at a time, picked in the switcher at top left. The sidebar
// lists that project's worktrees. The main area has five views: Sessions lists every session,
// pinned ones first in your order; Worktrees lists pinned worktrees in your order, then every
// other worktree once, in Needs you, Working, Your turn, In review, Idle or Merged, with a line
// for each agent that needs you, is working or just finished; PRs lists the project's pull
// requests; Todo lists work you mean to hand an agent, and the sessions on it;
// Canvas lays the project's work out in space, to be remembered by where it is (CanvasView.tsx).
// Keyboard map: the design system's Surfaces page.

import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { below, showContextMenu, type MenuAction, type MenuSpec } from './contextMenu';
import { folderMenu, openInMenu, opensInEditor, useEditorIcons } from './editors';
import { AGENT_BAY, BAYS, buildBays, filterByAge, inboxCounts, lanePromptAt, narrowed, prNeedsWork, rowItems, seenable, worktreeRow, type BoardRow, type Filter, type WorktreeRow } from './lanes';
import { answerLink, approvePr, barMode, desktopPins, getDevices, onDevices, dismissTodoHint, expandAttachments, getBoard, onBarMode, onBoard, onCloseIdle, onDesktopPins, onNewWindow, onQuickStart, onReveal, onSettings, onCheckUpdates, openConfig, openProjectWindow, openTarget, openUrl, pinToDesktop, refresh, refreshUsage, reviewInCodex, setBarMode, startOptions, startSession, forkSession, setPinned, setPinnedWorktrees, setTheme, showQuick, signIn, unpinFromDesktop, windowLabel, type BarMode, type Reveal } from './api';
import { agoPhrase, middleTruncate } from './format';
import { usePerf } from './perf';
import { archiveAhead, boardAhead, boardView, lanesAhead, observe, seenAhead, useAhead } from './ahead';
import { reuse } from './reuse';
import { reorderPinned, usePins } from './pins';
import { reviewLane, reviewPicks, reviewPrompt } from './review';
import { remembered } from './start';
import { blockedBy, COLOR_WORD, depsOf, newTodo, pickTo, pickToggle, sectionOf, sessionHomes, snippet, STATUSES, STATUS_WORD, todoIndex, todoInFilter, todoInQuery, todoItem, todoPrompt, todosPrompt, todoSections, useTodoStore, type TodoItem, type TodoPick, type TodoSectionId } from './todos';
import type { Bay, Board, DesktopPinned, DevicesView, Label, Lane, Pr, PrItem, Session, StartOptions, Theme, Todo, TodoStatus } from './types';
import { byDevice, deviceOf, needsOn, shortOf, withDevices, type DeviceFilter } from './devices';
import { snoozeMenu } from './snooze';
import { queueAfter, statusTime, type Handled } from './quick';
import { DeviceBadge } from './ui/DeviceBadge';
import { LABEL_COLORS } from './types';
import { SettingsDialog, type SettingsSection } from './ui/SettingsDialog';
import { CloseIdleDialog } from './ui/CloseIdleDialog';
import { PreviewDialog } from './ui/PreviewBits';
import { Inspector } from './ui/Inspector';
import { SessionPanel } from './ui/SessionPanel';
import { LaneRow } from './ui/LaneRow';
import { PinnedSection, type PinDrag } from './ui/PinnedSection';
import { BayHeader, Button, Icon, Kbd, NavItem, StatusGlyph } from './ui/primitives';
import { ALL_PROJECTS, openSwitcher, projectForDigit, ProjectSwitcher, type ActionCounts } from './ui/ProjectSwitcher';
import { useLogo } from './logo';
import { PrInspector } from './ui/PrInspector';
import { prIsOpen, prPlaces, prSections, PrsView, type PrRowItem } from './ui/PrsView';
import { Resizer, usePanelWidth } from './ui/Resizer';
import { SessionsView, timeSections, waitingQueue, type SessionItem } from './ui/SessionsView';
import { LabelDot } from './ui/TodoBits';
import { TodoPanel, TodoPickPanel } from './ui/TodoPanel';
import { TodoComposer, TodoView, type ComposerContext, type SelectHow } from './ui/TodoView';
import { CanvasView } from './ui/CanvasView';
import { entitiesOf, homeLane, refId, type RefItem, type RefKind } from './canvas/model';
import { useCanvas } from './canvas/useCanvas';
import { applyTheme, ThemeSwitch } from './ui/ThemeSwitch';
import { RestartToUpdate } from './ui/Updates';
import { UsageBar } from './ui/UsageBar';
import { WorktreeTree, type TreeGroup } from './ui/WorktreeTree';

const ELSEWHERE = 'Elsewhere';

declare global {
  interface Window {
    /** Set on a project's own window before the page loads. */
    __CODEALONG_PROJECT__?: string;
  }
}

/** The project this window was opened for; null in the main window. */
const WINDOW_PROJECT = typeof window !== 'undefined' ? (window.__CODEALONG_PROJECT__ ?? null) : null;
const PROJECT_KEY = 'codealong.project';
const VIEW_KEY = 'codealong.view';
const AGE_KEY = 'codealong.age';
const GROUP_KEY = 'codealong.byDevice';
const SIDEBAR_WIDTH_KEY = 'codealong.sidebarWidth';
const INSPECTOR_WIDTH_KEY = 'codealong.inspectorWidth';
const PR_PANEL_WIDTH_KEY = 'codealong.prPanelWidth';
const TODO_PANEL_WIDTH_KEY = 'codealong.todoPanelWidth';
const SESSION_PANEL_WIDTH_KEY = 'codealong.sessionPanelWidth';
const DAY = 24 * 60 * 60 * 1000;

type View = 'worktrees' | 'sessions' | 'prs' | 'todo' | 'canvas';
const VIEWS: { id: View; label: string }[] = [
  { id: 'sessions', label: 'Sessions' },
  { id: 'worktrees', label: 'Worktrees' },
  { id: 'prs', label: 'PRs' },
  { id: 'todo', label: 'Todo' },
  { id: 'canvas', label: 'Canvas' },
];
/** How recently a worktree or session must have been active to show. */
type Age = '1d' | '3d' | '7d' | 'all';
const AGES: { id: Age; label: string; title: string; ms: number }[] = [
  { id: '1d', label: '24h', title: 'Active in the last 24 hours', ms: DAY },
  { id: '3d', label: '3d', title: 'Active in the last 3 days', ms: 3 * DAY },
  { id: '7d', label: '7d', title: 'Active in the last 7 days', ms: 7 * DAY },
  { id: 'all', label: 'All', title: 'Everything from the last 14 days', ms: 0 },
];

type Row = BoardRow | PrRowItem | TodoItem;

function inProject(lane: Lane, project: string): boolean {
  if (project === ALL_PROJECTS) return !lane.hidden;
  return lane.project === project && (project === ELSEWHERE || !lane.hidden);
}

function inQuery(lane: Lane, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  const hay = [lane.callsign, lane.repo, lane.branch ?? '', lane.headline, lane.path, lane.pr ? `#${lane.pr.number} ${lane.pr.title}` : '', ...lane.sessions.map((s) => `${s.account} ${s.title}`)];
  return hay.some((h) => h.toLowerCase().includes(needle));
}

/** Search in the Sessions view: the session, then its worktree and PR. */
function sessionInQuery(r: SessionItem, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  const { lane, session: s } = r;
  const hay = [s.title, s.account, s.excerpt ?? '', lane.callsign, lane.repo, lane.branch ?? '', lane.pr ? `#${lane.pr.number} ${lane.pr.title}` : ''];
  return hay.some((h) => h.toLowerCase().includes(needle));
}

/** The Inbox filters, applied to one session. */
function sessionInFilter(r: SessionItem, f: Filter | null): boolean {
  const pr = r.lane.pr;
  switch (f) {
    case null:
      return true;
    case 'needs':
      return r.session.status === 'needs' || r.session.status === 'error';
    case 'working':
    case 'unread':
      return r.session.status === f;
    case 'review':
      return !!pr && pr.state !== 'merged' && pr.state !== 'closed';
    case 'work':
      return prNeedsWork(r.lane.pr);
    default:
      return r.lane.bay === f;
  }
}

/** What selecting a worktree on the board selects: its first agent's line, else the worktree. */
function boardRowKey(lane: Lane): string {
  return rowItems(worktreeRow(lane))[0].key;
}

/** Search in the PRs view: number, title, author, branch, repo. */
function prInQuery(p: PrItem, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase().replace(/^#/, '');
  return [String(p.number), p.title, p.author ?? '', p.head, p.repoSlug].some((h) => h.toLowerCase().includes(needle));
}

/** The Inbox filters, applied to a PR and the sessions that worked on it. */
function prInFilter(r: PrRowItem, f: Filter | null): boolean {
  const p = r.pr;
  switch (f) {
    case null:
      return true;
    case 'needs':
    case 'working':
    case 'unread':
      // As on the board, a session from another lane doesn't lend its status: one agent that
      // made many worktrees would otherwise put all of their PRs in Working.
      return r.sessions.some(({ session }) => !session.visitor && AGENT_BAY[session.status] === f);
    case 'review':
      return prIsOpen(p);
    case 'work':
      return prNeedsWork(p);
    default:
      return false;
  }
}

/** A row's element id, for the list's active descendant. From its key, not its place in the
 *  list, so a row keeps its id when others move and needn't render again. */
function domIdOf(key: string): string {
  return `row-${key}`;
}
function domId(r: { key: string }): string {
  return domIdOf(r.key);
}

function storedAge(): Age {
  try {
    const v = window.localStorage.getItem(AGE_KEY);
    return AGES.some((a) => a.id === v) ? (v as Age) : 'all';
  } catch {
    return 'all';
  }
}

function storedView(): View {
  try {
    const v = window.localStorage.getItem(VIEW_KEY);
    return VIEWS.some((x) => x.id === v) ? (v as View) : 'sessions';
  } catch {
    return 'sessions';
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

function storedProject(): string | null {
  try {
    return window.localStorage.getItem(PROJECT_KEY);
  } catch {
    return null;
  }
}

/** Whether the keyboard is in a text field (other than search, which has keys of its own). */
function inField(el: Element | null, search: HTMLInputElement | null): boolean {
  if (!el || el === search) return false;
  return el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement || (el as HTMLElement).isContentEditable;
}

/** A dragged panel width for the grid's style; left out, the panel takes its default from the CSS. */
const widthVar = (width: number | null) => (width == null ? undefined : `${width}px`);

export function App() {
  const [engineBoard, setBoard] = useState<Board | null>(null);
  // Changes you made that the engine hasn't shown yet (ahead.ts), on top of the board it sent.
  const ahead = useAhead();
  const board = useMemo(() => (engineBoard ? boardAhead(engineBoard, ahead) : null), [engineBoard, ahead]);
  // Your other devices (src-tauri/src/peers.rs): their worktrees join this device's in every view,
  // the sidebar's Devices narrow the views to one of them, and Worktrees can group by device.
  const [devices, setDevices] = useState<DevicesView | null>(null);
  const [deviceFilter, setDeviceFilter] = useState<DeviceFilter>(null);
  const [groupByDevice, setGroupByDevice] = useState(() => {
    try {
      return window.localStorage.getItem(GROUP_KEY) === '1';
    } catch {
      return false;
    }
  });
  const toggleGroupByDevice = () =>
    setGroupByDevice((on) => {
      try {
        window.localStorage.setItem(GROUP_KEY, on ? '0' : '1');
      } catch {
        // Grouping still changes for this window; it just isn't remembered.
      }
      return !on;
    });
  // Other devices' worktrees, with your changes to their sessions too.
  const deviceLanes = useMemo(() => (devices?.enabled ? lanesAhead(devices.lanes, ahead) : []), [devices, ahead]);
  useEffect(() => observe(boardView(engineBoard?.generatedAt ?? null, [engineBoard?.lanes ?? [], devices?.lanes ?? []])), [engineBoard, devices]);
  useEffect(() => {
    const show = (d: DevicesView) => setDevices((prev) => (prev ? reuse(prev, d) : d));
    void getDevices().then((d) => d && show(d));
    return onDevices(show);
  }, []);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [project, setProject] = useState<string | null>(() => WINDOW_PROJECT ?? storedProject());
  const [bayFilter, setBayFilter] = useState<Filter | null>(null);
  const [query, setQuery] = useState('');
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [view, setViewState] = useState<View>(storedView);
  const [age, setAgeState] = useState<Age>(storedAge);
  // Panel widths as you last dragged them. The PR panel has its own: it starts out wider.
  const [sidebarWidth, setSidebarWidth] = usePanelWidth(SIDEBAR_WIDTH_KEY);
  const [inspectorWidth, setInspectorWidth] = usePanelWidth(INSPECTOR_WIDTH_KEY);
  const [prPanelWidth, setPrPanelWidth] = usePanelWidth(PR_PANEL_WIDTH_KEY);
  const [todoPanelWidth, setTodoPanelWidth] = usePanelWidth(TODO_PANEL_WIDTH_KEY);
  const [sessionPanelWidth, setSessionPanelWidth] = usePanelWidth(SESSION_PANEL_WIDTH_KEY);
  const [undo, setUndo] = useState<{ key: string; title: string } | null>(null);
  const [showArchived, setShowArchived] = useState(false);
  const [showDonePrs, setShowDonePrs] = useState(false);
  // PRs being approved, by key: the button waits on GitHub.
  const [approving, setApproving] = useState<Set<string>>(() => new Set());
  const [showParked, setShowParked] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  // Settings, open at a section; at one account when a usage chip opened it. ⌘, comes back to
  // the section you left, as macOS settings do.
  const [settings, setSettings] = useState<{ section: SettingsSection; focus: string | null } | null>(null);
  const lastSection = useRef<SettingsSection>('general');
  const openSettings = useCallback((section?: SettingsSection, focus: string | null = null) => {
    setSettings({ section: section ?? lastSection.current, focus });
  }, []);
  const closeSettings = useCallback(() => setSettings(null), []);
  useEffect(() => {
    if (settings) lastSection.current = settings.section;
  }, [settings]);
  useEffect(() => onSettings(() => openSettings()), [openSettings]);
  // The app menu's Check for updates…: Settings › General says how it goes.
  useEffect(() => onCheckUpdates(() => openSettings('general')), [openSettings]);
  // File › Close idle terminals… (ui/CloseIdleDialog.tsx).
  const [closeIdle, setCloseIdle] = useState(false);
  // The worktree whose preview URL is being set (ui/PreviewBits.tsx), by id.
  const [previewFor, setPreviewFor] = useState<string | null>(null);
  useEffect(() => onCloseIdle(() => setCloseIdle(true)), []);
  // New session opens the quick panel (ui/Quick.tsx) at a worktree, maybe for a todo, in the
  // project this window shows. The panel tells this window how the start goes.
  const projectNow = useRef<string>(ALL_PROJECTS);
  const newSession = useCallback((at: { laneId: string | null; todo?: string | null; todos?: string[]; prompt?: string }) => {
    // The quick panel starts sessions on this device; one on another device's worktree starts without one.
    const laneId = at.laneId && shortOf(at.laneId) ? null : at.laneId;
    void showQuick({ ...at, laneId, project: projectNow.current, opener: windowLabel() });
  }, []);
  // Opens and starts still waiting on their app: a spinner shows once one takes a moment.
  const [busy, setBusy] = useState<{ id: number; label: string }[]>([]);
  const busySeq = useRef(0);
  /** Marks sessions seen, shown at once; the engine turning it down puts them back and says why. */
  const markSeen = useCallback((keys: string[]) => {
    seenAhead(keys).catch((e: unknown) => setToast(String(e).replace(/^Error: /, '')));
  }, []);
  const track = useCallback(<T,>(label: string, work: Promise<T>): Promise<T> => {
    const id = ++busySeq.current;
    const timer = window.setTimeout(() => setBusy((b) => [...b, { id, label }]), 300);
    const done = () => {
      window.clearTimeout(timer);
      setBusy((b) => b.filter((x) => x.id !== id));
    };
    work.then(done, done);
    return work;
  }, []);
  // A session started from here, selected once it shows on the board.
  const [pendingSession, setPendingSession] = useState<{ key: string; until: number } | null>(null);
  const now = useNow(15_000);
  const searchRef = useRef<HTMLInputElement>(null);
  const gPending = useRef(false);

  const boardArrived = useRef<number | null>(null);
  usePerf(engineBoard, boardArrived);
  useEffect(() => {
    getBoard()
      .then((b) => setBoard((prev) => (prev ? reuse(prev, b) : b)))
      .catch((e: unknown) => setLoadError(String(e)));
    // A window that can't be seen keeps only the latest push and shows it when it's seen again:
    // the engine publishes many times a minute, and every render of a board no one sees is waste.
    let held: Board | null = null;
    const show = (b: Board) => {
      boardArrived.current = performance.now();
      setBoard((prev) => (prev ? reuse(prev, b) : b));
    };
    const onVisible = () => {
      if (document.hidden || !held) return;
      const b = held;
      held = null;
      show(b);
    };
    document.addEventListener('visibilitychange', onVisible);
    const off = onBoard((b) => {
      if (document.hidden) held = b;
      else show(b);
    });
    return () => {
      off();
      document.removeEventListener('visibilitychange', onVisible);
    };
  }, []);

  useEffect(() => {
    const onFocus = () => void refresh();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, []);

  useEffect(() => {
    if (!toast) return;
    const id = window.setTimeout(() => setToast(null), 6000);
    return () => window.clearTimeout(id);
  }, [toast]);

  // The theme from config.toml, or the one just picked here until the board has it.
  const [pickedTheme, setPickedTheme] = useState<Theme | null>(null);
  const theme = pickedTheme ?? board?.theme ?? 'system';
  const logoStatus = useLogo(board?.logoStatus);
  useEffect(() => applyTheme(theme), [theme]);
  useEffect(() => {
    if (pickedTheme && board?.theme === pickedTheme) setPickedTheme(null);
  }, [board?.theme, pickedTheme]);
  const pickTheme = useCallback((t: Theme) => {
    setPickedTheme(t);
    setTheme(t).catch((e: unknown) => {
      setPickedTheme(null);
      setToast(String(e));
    });
  }, []);

  // This device's projects, then any only another device has: its worktrees go into this device's
  // projects by repo (peers.rs), and the rest keep that device's own project, listed here too.
  const projects = useMemo(() => {
    const here = board?.projects ?? [];
    const names = new Set(here.map((p) => p.name));
    const elsewhere: Map<string, number> = new Map();
    for (const l of deviceLanes) {
      if (!names.has(l.project) && !l.hidden) elsewhere.set(l.project, (elsewhere.get(l.project) ?? 0) + 1);
    }
    if (elsewhere.size === 0) return here;
    const extra = [...elsewhere].map(([name, lanes]) => ({ name, lanes, repos: [] }));
    const last = here.length > 0 && here[here.length - 1].name === ELSEWHERE ? here.length - 1 : here.length;
    return [...here.slice(0, last), ...extra, ...here.slice(last)];
  }, [board?.projects, deviceLanes]);
  const known = project === ALL_PROJECTS || projects.some((p) => p.name === project);
  const current = known && project ? project : (projects.find((p) => p.name !== ELSEWHERE)?.name ?? ALL_PROJECTS);
  projectNow.current = current;

  const pickProject = (name: string) => {
    setProject(name);
    setBayFilter(null);
    setShowParked(false);
    setSelectedKey(null);
    // A project window keeps its choice to itself; the main window's is remembered.
    if (WINDOW_PROJECT) return;
    try {
      window.localStorage.setItem(PROJECT_KEY, name);
    } catch {
      // Private windows can refuse storage; the choice just won't persist.
    }
  };

  const setView = useCallback((v: View) => {
    setViewState(v);
    try {
      window.localStorage.setItem(VIEW_KEY, v);
    } catch {
      // Storage can be unavailable; the view just won't persist.
    }
  }, []);

  const setAge = useCallback((a: Age) => {
    setAgeState(a);
    try {
      window.localStorage.setItem(AGE_KEY, a);
    } catch {
      // Storage can be unavailable; the filter just won't persist.
    }
  }, []);

  // Pinned sessions head the Sessions view and pinned worktrees the Worktrees view, each in
  // your order.
  const sessionPins = usePins(board?.pinned, setPinned, setToast);
  const lanePins = usePins(board?.pinnedWorktrees, setPinnedWorktrees, setToast);

  // Archived sessions leave every view but the Archived list.
  const rawLanes = useMemo(() => withDevices(board?.lanes ?? [], deviceLanes, deviceFilter), [board?.lanes, deviceLanes, deviceFilter]);
  // What needs you on each device, for the sidebar, whichever device the views show.
  const deviceNeeds = useMemo(() => {
    const all = withDevices(board?.lanes ?? [], deviceLanes, null);
    return new Map<string | null, number>([null, ...(devices?.devices ?? []).map((d) => d.short)].map((m) => [m, needsOn(all, m)]));
  }, [board?.lanes, deviceLanes, devices?.devices]);
  const lanes = useMemo(() => rawLanes.map((l) => (l.sessions.some((s) => s.archived) ? { ...l, sessions: l.sessions.filter((s) => !s.archived) } : l)), [rawLanes]);
  const projectLanes = useMemo(() => lanes.filter((l) => inProject(l, current)), [lanes, current]);
  const archivedItems = useMemo(
    () =>
      rawLanes
        .filter((l) => inProject(l, current))
        .flatMap((lane) =>
          lane.sessions
            .filter((s) => !s.visitor && s.archived)
            .map((s): SessionItem => ({ kind: 'agent', key: `${lane.id}::${s.key}`, lane, session: s })),
        )
        .sort((a, b) => b.session.lastActivity - a.session.lastActivity),
    [rawLanes, current],
  );
  // The age filter: worktrees and sessions active since the cutoff. Pinned worktrees always
  // show on the board and in the sidebar, and pinned sessions in Sessions.
  const ageMs = AGES.find((a) => a.id === age)?.ms ?? 0;
  const cutoff = ageMs > 0 ? now - ageMs : 0;
  const since = (AGES.find((a) => a.id === age)?.title ?? '').replace(/^Active /, '');
  // Worktrees active since the cutoff and pinned ones, each showing only its sessions active
  // since then.
  const recentLanes = useMemo(
    () => filterByAge(projectLanes, cutoff, lanePins.set, since),
    [projectLanes, cutoff, lanePins.set, since],
  );
  // The Sessions view: each session once, in the worktree it runs in.
  const sessionItems = useMemo(
    () => projectLanes.flatMap((lane) => lane.sessions.filter((s) => !s.visitor).map((s): SessionItem => ({ kind: 'agent', key: `${lane.id}::${s.key}`, lane, session: s }))),
    [projectLanes],
  );
  const shownSessions = useMemo(
    () =>
      sessionItems.filter(
        (r) => (r.session.lastActivity >= cutoff || sessionPins.set.has(r.session.key)) && sessionInQuery(r, query) && sessionInFilter(r, bayFilter),
      ),
    [sessionItems, cutoff, sessionPins.set, query, bayFilter],
  );
  const shownSessionCount = useMemo(
    () => sessionItems.filter((r) => r.session.lastActivity >= cutoff || sessionPins.set.has(r.session.key)).length,
    [sessionItems, cutoff, sessionPins.set],
  );
  const pinnedRows = useMemo(() => {
    const byKey = new Map(shownSessions.map((r) => [r.session.key, r]));
    return sessionPins.keys.flatMap((k) => byKey.get(k) ?? []);
  }, [shownSessions, sessionPins.keys]);
  const sessionSections = useMemo(() => timeSections(shownSessions.filter((r) => !sessionPins.set.has(r.session.key)), now), [shownSessions, sessionPins.set, now]);
  // Pinned worktrees that match the search and the Inbox filter, in your order. Each shows at the
  // top of the board and leaves the bays below.
  const pinnedLaneRows = useMemo(() => {
    const byId = new Map(recentLanes.map((l) => [l.id, l]));
    return lanePins.keys.flatMap((id): WorktreeRow[] => {
      const lane = byId.get(id);
      const shown = lane && inQuery(lane, query) ? narrowed(lane, bayFilter, since) : null;
      return shown ? [worktreeRow(shown)] : [];
    });
  }, [recentLanes, lanePins.keys, query, bayFilter, since]);
  // Every other worktree once, in the bay its status puts it in. The Inbox filter narrows each to
  // what it's about, which can move it to that bay.
  const bayRows = useCallback((ls: Lane[]) => buildBays(ls.flatMap((l) => (inQuery(l, query) && !lanePins.set.has(l.id) ? (narrowed(l, bayFilter, since) ?? []) : []))), [query, lanePins.set, bayFilter, since]);
  const bays = useMemo(() => bayRows(recentLanes), [bayRows, recentLanes]);
  // Grouped by device, each device's worktrees get bays of their own: this device's first.
  const grouped = groupByDevice && deviceFilter === null && (devices?.enabled ?? false) && (devices?.lanes.length ?? 0) > 0;
  const deviceGroups = useMemo(() => (grouped ? byDevice(recentLanes, devices).map((g) => ({ short: g.short, bays: bayRows(g.lanes) })) : null), [grouped, recentLanes, devices, bayRows]);
  const foldParked = !showParked && !query;
  const fold = useCallback((bs: typeof bays) => bs.map((b) => ({ ...b, rows: b.rows.filter((r) => !foldParked || !r.lane.parked) })), [foldParked]);
  const shownBays = useMemo(() => fold(bays), [fold, bays]);
  const shownGroups = useMemo(() => (deviceGroups ?? [{ short: undefined as string | null | undefined, bays }]).map((g) => ({ ...g, shown: fold(g.bays) })), [deviceGroups, bays, fold]);
  // What you can select on the board, top to bottom: agents' lines, and worktrees without any.
  const worktreeItems = useMemo(() => [...pinnedLaneRows, ...shownGroups.flatMap((g) => g.shown.flatMap((b) => b.rows))].flatMap(rowItems), [pinnedLaneRows, shownGroups]);
  // The PRs view: the project's PRs with the worktrees that hold them and the sessions that
  // worked on them. A session is looked up in the worktree it's listed under, which for one that
  // made or worked in a worktree isn't its own.
  const sessionIndex = useMemo(() => {
    const m = new Map<string, { lane: Lane; session: Session }>();
    for (const lane of lanes) for (const session of lane.sessions) m.set(`${lane.id}::${session.key}`, { lane, session });
    return m;
  }, [lanes]);
  const laneById = useMemo(() => new Map(lanes.map((l) => [l.id, l])), [lanes]);
  const projectPrs = useMemo(() => (board?.prs ?? []).filter((p) => current === ALL_PROJECTS || p.project === current), [board?.prs, current]);
  const prRows = useMemo(
    () =>
      projectPrs
        .filter((p) => p.updatedAt >= cutoff && prInQuery(p, query))
        .map((p): PrRowItem => {
          const holding = p.lanes.flatMap((id) => laneById.get(id) ?? []);
          const sessions = p.sessions.flatMap((s) => sessionIndex.get(`${s.lane}::${s.key}`) ?? []);
          return { kind: 'pr', key: `pr:${p.key}`, pr: p, lane: holding[0] ?? null, sessions, places: prPlaces(holding, sessions) };
        })
        .filter((r) => prInFilter(r, bayFilter)),
    [projectPrs, cutoff, query, bayFilter, laneById, sessionIndex],
  );
  const prSecs = useMemo(() => prSections(prRows), [prRows]);
  const prVisible = useMemo<Row[]>(() => prSecs.flatMap((s) => (s.id === 'done' && !showDonePrs ? [] : s.rows)), [prSecs, showDonePrs]);
  const openPrCount = useMemo(() => projectPrs.filter((p) => prIsOpen(p) && p.updatedAt >= cutoff).length, [projectPrs, cutoff]);
  const requestedCount = useMemo(() => projectPrs.filter((p) => prIsOpen(p) && p.reviewRequested).length, [projectPrs]);

  // The Todo view: the project's todos as the engine has them, with this window's changes on top
  // until the engine's board shows them.
  const todoStore = useTodoStore(board, setToast);
  const runTodo = todoStore.run;
  const [showDoneTodos, setShowDoneTodos] = useState(false);
  const [labelFilter, setLabelFilter] = useState<string | null>(null);
  // Todos picked together with the selected one (⌘-click, ⇧-click, ⇧↑ ⇧↓), to start in one
  // session.
  const [pick, setPick] = useState<TodoPick | null>(null);
  // The box that adds todos, and what a new one gets linked to.
  const [composer, setComposer] = useState<{ text: string; context: ComposerContext | null }>({ text: '', context: null });
  const composerRef = useRef<HTMLInputElement>(null);
  const [composerFocus, setComposerFocus] = useState(0);
  const [todoUndo, setTodoUndo] = useState<{ message: string; icon?: string; undo: () => void } | null>(null);
  const homes = useMemo(() => sessionHomes(lanes), [lanes]);
  const labelById = useMemo(() => new Map(todoStore.labels.map((l) => [l.id, l])), [todoStore.labels]);
  const projectTodos = useMemo(() => todoStore.todos.filter((t) => current === ALL_PROJECTS || t.project === current), [todoStore.todos, current]);
  const todoItems = useMemo(() => projectTodos.map((t) => todoItem(t, laneById, homes, labelById)), [projectTodos, laneById, homes, labelById]);
  const todoRows = useMemo(
    () => todoItems.filter((r) => todoInQuery(r, query) && todoInFilter(r, bayFilter) && (!labelFilter || r.todo.labels.includes(labelFilter))),
    [todoItems, query, bayFilter, labelFilter],
  );
  const todoSecs = useMemo(() => todoSections(todoRows, todoStore.todos), [todoRows, todoStore.todos]);
  const todoVisible = useMemo<Row[]>(() => todoSecs.flatMap((s) => (s.id === 'done' && !showDoneTodos ? [] : s.rows)), [todoSecs, showDoneTodos]);
  const openTodoCount = useMemo(() => projectTodos.filter((t) => t.status !== 'done').length, [projectTodos]);
  // Which todos each session and worktree is on, for their chips.
  const todoLinks = useMemo(() => todoIndex(todoStore.todos, homes), [todoStore.todos, homes]);
  // Which todos each session is linked to depends on the todos alone, so rows that ask keep one
  // function between pushes and needn't render again.
  const sessionTodos = useMemo(() => todoIndex(todoStore.todos, new Map()).bySession, [todoStore.todos]);
  const todosFor = useCallback((k: string) => sessionTodos.get(k), [sessionTodos]);

  // The Canvas view: the project's document, kept up to date with the board once it's been shown.
  const canvasEntities = useMemo(
    () => entitiesOf(board, projectLanes, todoStore.todos, (p) => current === ALL_PROJECTS || p === current),
    [board, projectLanes, todoStore.todos, current],
  );
  const canvas = useCanvas(current, view === 'canvas', canvasEntities, now, lanePins.set);
  const [canvasFocus, setCanvasFocus] = useState<{ id: string; n: number } | null>(null);
  const canvasKeys = useRef<(e: KeyboardEvent) => boolean>(() => false);
  // The one card selected on the canvas: its panel shows on the right, and ⌘N starts in its worktree.
  const [canvasSel, setCanvasSel] = useState<RefItem | null>(null);
  const canvasLane = view === 'canvas' && canvasSel ? homeLane(canvasEntities, canvasSel.ref, canvasSel.key) : null;
  /** Puts a thing on the canvas if it isn't there, keeps it, and shows it there. */
  const showOnCanvas = (ref: RefKind, key: string) => {
    const id = canvas.keep(ref, key);
    setView('canvas');
    setCanvasFocus((f) => ({ id, n: (f?.n ?? 0) + 1 }));
  };
  /** The menu item that does that, in the other views' menus. */
  const canvasItem = (ref: RefKind, key: string): MenuSpec =>
    view === 'canvas' ? [] : [{ label: canvas.has(ref, key) ? 'Show on canvas' : 'Add to canvas', run: () => showOnCanvas(ref, key) }];
  // What's pinned to the desktop, as `ref:key`, so menus offer to take it off (desktop.rs).
  const [onDesktop, setOnDesktop] = useState<Set<string>>(() => new Set());
  useEffect(() => {
    const take = (pins: DesktopPinned[]) => setOnDesktop(new Set(pins.map((p) => `${p.ref}:${p.key}`)));
    void desktopPins().then(take);
    return onDesktopPins(take);
  }, []);
  /** Pins a session, worktree or todo to the desktop, over every window, or takes it off. */
  const desktopItem = (ref: DesktopPinned['ref'], key: string): MenuSpec => {
    const pinned = onDesktop.has(`${ref}:${key}`);
    return [{ label: pinned ? 'Remove from desktop' : 'Pin to desktop', run: () => void (pinned ? unpinFromDesktop({ ref, key }) : pinToDesktop({ ref, key })).catch((e: unknown) => setToast(String(e))) }];
  };
  useEffect(() => {
    if (composerFocus === 0) return;
    composerRef.current?.focus();
    composerRef.current?.select();
  }, [composerFocus]);
  useEffect(() => {
    if (!todoUndo) return;
    const id = window.setTimeout(() => setTodoUndo(null), 8000);
    return () => window.clearTimeout(id);
  }, [todoUndo]);

  const sessionRows = useMemo<Row[]>(
    () => [...pinnedRows, ...sessionSections.flatMap((s) => s.rows), ...(showArchived ? archivedItems : [])],
    [pinnedRows, sessionSections, showArchived, archivedItems],
  );
  const visible = view === 'sessions' ? sessionRows : view === 'prs' ? prVisible : view === 'todo' ? todoVisible : view === 'canvas' ? [] : worktreeItems;
  // When the selected agent's line goes (you marked it seen, it stopped), the selection stays
  // on its worktree rather than jumping to the top. A todo that leaves the list (marked done
  // while Done is folded, or no longer matching the search or a filter) stays in the panel: the
  // next key you press is still about the todo you see, and an edit in progress isn't cut off.
  const selectedLaneId = selectedKey?.split('::')[0];
  const selected =
    visible.find((r) => r.key === selectedKey) ??
    (view === 'todo' ? todoItems.find((r) => r.key === selectedKey) : visible.find((r) => r.kind !== 'pr' && r.kind !== 'todo' && r.lane.id === selectedLaneId)) ??
    visible[0] ??
    null;
  const selectedLane = selected?.lane ?? null;
  // The todos picked together, in the list's order. They hold while the keyboard stays on the
  // row they were picked from: selecting a row any other way ends them. Fewer than two is none.
  const pickedRows = useMemo(() => {
    if (view !== 'todo' || !pick || pick.focus !== selected?.key) return [];
    const rows = todoVisible.filter((r): r is TodoItem => r.kind === 'todo' && pick.keys.includes(r.key));
    return rows.length > 1 ? rows : [];
  }, [view, pick, selected?.key, todoVisible]);
  const pickedKeys = useMemo(() => new Set(pickedRows.map((r) => r.key)), [pickedRows]);
  // A session shown in a PR's or todo's panel, from its list of sessions, until you go back.
  // It's tied to the panel it came from: selecting another PR or todo leaves it.
  const [drill, setDrill] = useState<{ from: string; key: string } | null>(null);
  const panelFrom =
    view === 'canvas'
      ? canvasSel && (canvasSel.ref === 'pr' || canvasSel.ref === 'todo')
        ? `${canvasSel.ref}:${canvasSel.key}`
        : null
      : (view === 'prs' || view === 'todo') && pickedRows.length === 0
        ? (selected?.key ?? null)
        : null;
  useEffect(() => {
    if (drill && drill.from !== panelFrom) setDrill(null);
  }, [drill, panelFrom]);
  // Where a new session starts: the selected worktree, session or PR's worktree; for a PR with
  // no worktree on this Mac, its repo's main checkout.
  const startLaneId = useMemo(() => {
    if (view === 'canvas') return canvasLane;
    if (!selected) return null;
    if (selected.lane) return selected.lane.id;
    if (selected.kind !== 'pr') return null;
    return lanes.find((l) => !l.device && l.repoSlug === selected.pr.repoSlug && l.origin === 'main')?.id ?? null;
  }, [selected, lanes, view, canvasLane]);

  const counts = useMemo(() => inboxCounts(recentLanes), [recentLanes]);

  const totalLanes = useMemo(() => lanes.filter((l) => !l.hidden).length, [lanes]);

  // Needs you and Your turn per project, for the switcher's badge and menu.
  const actions = useMemo(() => {
    const out: Record<string, ActionCounts> = {};
    for (const name of [...projects.map((p) => p.name), ALL_PROJECTS]) {
      const { needs, unread } = inboxCounts(filterByAge(lanes.filter((l) => inProject(l, name)), cutoff, lanePins.set, since));
      out[name] = { needs, unread };
    }
    return out;
  }, [lanes, projects, cutoff, lanePins.set, since]);

  useEffect(() => {
    const base = WINDOW_PROJECT ? `Codealong · ${current}` : 'Codealong';
    document.title = counts.needs > 0 ? `${base} · ${counts.needs} need you` : base;
  }, [counts.needs, current]);

  useEffect(() => {
    if (selected) document.getElementById(domId(selected))?.scrollIntoView({ block: 'nearest' });
  }, [selected?.key]);

  const open = useCallback((lane: Lane | null, targetId: string | null | undefined) => {
    if (!lane || !targetId) return;
    // A session Codealong runs lives in its panel here: Sessions, with it selected.
    if (targetId.startsWith('s|') && targetId.endsWith('|hosted')) {
      const key = targetId.slice(2, -'|hosted'.length);
      reveal.current({ kind: 'session', laneId: lane.id, key });
      setView('sessions');
      markSeen([key]);
      return;
    }
    const target = lane.targets.find((t) => t.id === targetId);
    if (target?.disabled) {
      setToast(target.detail ?? `${target.label} isn't available right now.`);
      return;
    }
    track(`${target?.label ?? 'Opening'}…`, openTarget(lane.id, targetId)).catch((e: unknown) => setToast(String(e)));
  }, [track, markSeen]);

  const openRow = useCallback(
    (row: Row | null) => {
      if (!row) return;
      if (row.kind === 'pr') {
        openUrl(row.pr.url).catch((e: unknown) => setToast(String(e)));
        return;
      }
      if (row.kind === 'todo') {
        // A todo opens its agent where it lives: the live one, else the latest. Without one,
        // it's time to start one.
        const s = row.sessions.find((x) => x.session.live) ?? row.sessions[0];
        if (s) open(s.lane, s.session.openTarget);
        else newSession({ laneId: row.lane?.id ?? null, todo: row.todo.id, prompt: todoPrompt(row.todo) });
        return;
      }
      open(row.lane, row.kind === 'agent' ? row.session.openTarget : row.lane.defaultTarget);
    },
    [open],
  );

  /** Approves a PR as you, in one click. */
  const approve = useCallback((pr: PrItem) => {
    setApproving((a) => new Set(a).add(pr.key));
    approvePr(pr.repoSlug, pr.number)
      .then(() => setToast(`Approved #${pr.number}.`))
      .catch((e: unknown) => setToast(String(e).replace(/^Error: /, '')))
      .finally(() =>
        setApproving((a) => {
          const next = new Set(a);
          next.delete(pr.key);
          return next;
        }),
      );
  }, []);

  const copy = useCallback((text: string | null | undefined, what: string) => {
    if (!text) return;
    navigator.clipboard
      .writeText(text)
      .then(() => setToast(`Copied the ${what}.`))
      .catch(() => setToast(`Couldn't copy the ${what}.`));
  }, []);

  const move = useCallback(
    (delta: number) => {
      if (visible.length === 0) return;
      const i = selected ? visible.indexOf(selected) : -1;
      const next = visible[Math.min(visible.length - 1, Math.max(0, i + delta))];
      if (next) setSelectedKey(next.key);
    },
    [visible, selected],
  );

  // Working through what waits on you from a session's panel in Sessions: ⌘⏎ in its box sends (or
  // allows the call it waits on) and goes on to the next listed that waits on you, and ⌘] skips to
  // it; a skipped one comes round again after the rest. The ones dealt with stay out until they
  // wait on you anew, since an allowed call isn't shown ahead of the board.
  const waitingRows = useMemo(() => waitingQueue(shownSessions), [shownSessions]);
  const handled = useRef<Handled>(new Map());
  // How the last one went, on the panel of the one gone on to, for a moment.
  const [queueNote, setQueueNote] = useState<{ key: string; text: string } | null>(null);
  // The row gone on to, whose box takes the cursor.
  const [boxFocus, setBoxFocus] = useState<string | null>(null);
  // Read when an answer comes in, which can be after you've moved on by hand.
  const waitingNow = useRef(waitingRows);
  waitingNow.current = waitingRows;
  const selectedNow = useRef(selected?.key);
  selectedNow.current = selected?.key;
  /** On from the session in row `from` to the next that waits on you, with what was done to it, or
   *  none for a skip. With nothing left, the panel stays where it is. */
  const goOn = (from: string, s: Session, done: string | null) => {
    if (view !== 'sessions' || selectedNow.current !== from) return;
    if (done) handled.current.set(s.key, statusTime(s));
    const next = queueAfter(waitingNow.current, s.key, handled.current)[0];
    if (!next) return;
    setQueueNote(done ? { key: next.key, text: `${s.title}: ${done}` } : null);
    setBoxFocus(next.key);
    setSelectedKey(next.key);
  };
  useEffect(() => {
    if (!queueNote) return;
    const t = window.setTimeout(() => setQueueNote(null), 4000);
    return () => window.clearTimeout(t);
  }, [queueNote]);
  useEffect(() => {
    if (boxFocus && selected?.key !== boxFocus) setBoxFocus(null);
  }, [selected?.key]);
  // A session you click in Sessions is one you mean to answer: the cursor goes to its panel's box,
  // once the click has given the list the keyboard, as a press does. Moving along the list with
  // the keys leaves them with the list, and Escape in the box gives them back to it.
  const listRef = useRef<HTMLDivElement>(null);
  const focusBox = () =>
    window.setTimeout(() => {
      const el = document.querySelector<HTMLTextAreaElement>('.ca-sessinsp .ca-composer textarea');
      if (!el) return;
      el.focus();
      el.setSelectionRange(el.value.length, el.value.length);
    });

  /** Archives a session (Codex threads in Codex itself), or unarchives it. */
  const setArchived = useCallback((key: string, archived: boolean, title: string) => {
    setUndo(archived ? { key, title } : null);
    archiveAhead(key, archived).catch((e: unknown) => {
      setUndo(null);
      setToast(String(e).replace(/^Error: /, ''));
    });
  }, []);

  useEffect(() => {
    if (!undo) return;
    const id = window.setTimeout(() => setUndo(null), 8000);
    return () => window.clearTimeout(id);
  }, [undo]);

  /** Opens the quick panel for a todo: in its worktree, with the todo as the prompt. */
  const startForTodo = (r: TodoItem) => newSession({ laneId: r.lane?.id ?? selectedLane?.id ?? null, todo: r.todo.id, prompt: todoPrompt(r.todo) });

  /** Opens the quick panel for one session on several todos: in the worktree of the first that
   *  has one, on a prompt that has each of them. */
  const startForTodos = (rows: TodoItem[]) => {
    if (rows.length === 1) return startForTodo(rows[0]);
    newSession({ laneId: rows.find((r) => r.lane)?.lane?.id ?? selectedLane?.id ?? null, todos: rows.map((r) => r.todo.id), prompt: todosPrompt(rows.map((r) => r.todo)) });
  };

  /** Selects a todo's row: alone, or with ⌘ adds it to the ones picked (or leaves it out), or
   *  with ⇧ picks the rows from the anchor to it. */
  const selectTodoRow = (r: TodoItem, how: SelectHow) => {
    if (how === 'one') {
      setPick(null);
      setSelectedKey(r.key);
      return;
    }
    const order = todoVisible.map((x) => x.key);
    const live = pickedRows.length > 0 ? pick : null;
    const next = how === 'toggle' ? pickToggle(order, live, selected?.key ?? null, r.key) : pickTo(order, live, selected?.key ?? null, r.key);
    setPick(next);
    setSelectedKey(next.focus);
  };

  /** Shows a todo in the Todo view, in its project, with nothing hiding it. */
  const showTodo = (t: Todo) => {
    const home = t.project && projects.some((p) => p.name === t.project) ? t.project : ALL_PROJECTS;
    if (current !== ALL_PROJECTS && current !== home) pickProject(home);
    setView('todo');
    setBayFilter(null);
    setLabelFilter(null);
    setQuery('');
    if (t.status === 'done') setShowDoneTodos(true);
    setSelectedKey(`todo:${t.id}`);
  };

  /** Adds a todo from the box at the top of the list, linked to what it was made from. */
  const createTodo = (title: string, context: ComposerContext | null) => {
    const lane = context?.worktree ? laneById.get(context.worktree) : undefined;
    const project = current !== ALL_PROJECTS ? current : (lane?.project ?? '');
    const t = newTodo(
      {
        title,
        project,
        worktrees: context?.worktree ? [context.worktree] : [],
        sessions: context?.session ? [context.session] : [],
        // Made while a label filter is on, it takes the label, so it stays in view.
        labels: labelFilter ? [labelFilter] : [],
      },
      Date.now(),
    );
    runTodo({ op: 'create', todo: t });
    setSelectedKey(`todo:${t.id}`);
    setComposer({ text: '', context: null });
  };

  /** Opens the Todo view with the box at the top ready for a todo linked to `context`. */
  const newTodoFrom = (context: ComposerContext, title = '') => {
    setView('todo');
    setBayFilter(null);
    setComposer({ text: title, context });
    setComposerFocus((n) => n + 1);
  };

  const setTodoStatus = (t: Todo, status: TodoStatus) => runTodo({ op: 'update', id: t.id, patch: { status } });

  /** Moves todos to Waiting by hand, or back out. Any not under Waiting are marked; when all
   *  are, stopping ends each one's mark and its waits on todos that aren't done, so it's ready at
   *  once. The toast can put that back. */
  const toggleWaiting = (list: Todo[]) => {
    const ts = list.filter((t) => t.status !== 'done');
    const out = ts.filter((t) => sectionOf(t, todoStore.todos) !== 'waiting');
    if (out.length > 0) {
      for (const t of out) runTodo({ op: 'update', id: t.id, patch: { waiting: { note: '', since: 0 } } });
      return;
    }
    if (ts.length === 0) return;
    // One an agent is on leaves Waiting with its mark; one not started, only once it waits on
    // nothing.
    const stopped = ts.map((t) => {
      const open = t.status === 'todo' ? blockedBy(t, todoStore.todos) : [];
      const before = { waiting: t.waiting ?? null, dependsOn: depsOf(t) };
      const dependsOn = depsOf(t).filter((d) => !open.some((o) => o.id === d));
      runTodo({ op: 'update', id: t.id, patch: { waiting: null, ...(open.length > 0 ? { dependsOn } : {}) } });
      return { t, open, before };
    });
    const [{ t, open }] = stopped;
    const on = open.length === 0 ? '' : open.length === 1 ? ` on “${open[0].title || 'Untitled'}”` : ` on ${open.length} todos`;
    const message = stopped.length === 1 ? `“${t.title || 'Untitled'}” no longer waits${on}.` : `${stopped.length} todos no longer wait.`;
    setTodoUndo({ message, icon: 'hourglass', undo: () => stopped.forEach((x) => runTodo({ op: 'update', id: x.t.id, patch: x.before })) });
  };

  /** The right-click item that moves todos into Waiting or out of it, named for what it ends. */
  const waitingItem = (list: Todo[]): MenuSpec => {
    const ts = list.filter((t) => t.status !== 'done');
    if (ts.length === 0) return [];
    if (ts.some((t) => sectionOf(t, todoStore.todos) !== 'waiting')) return [{ label: 'Mark as waiting', run: () => toggleWaiting(ts) }];
    const open = ts.length === 1 && ts[0].status === 'todo' ? blockedBy(ts[0], todoStore.todos) : [];
    const label = ts.length > 1 || ts[0].waiting || open.length === 0 ? 'Stop waiting' : open.length === 1 ? `Stop waiting on “${snippet(open[0].title || 'Untitled', 44)}”` : `Stop waiting on ${open.length} todos`;
    return [{ label, run: () => toggleWaiting(ts) }];
  };

  /** Deletes todos; the toast can put them back where they were. */
  const deleteTodos = (ts: Todo[]) => {
    if (ts.length === 0) return;
    const index = new Map(ts.map((t) => [t.id, todoStore.todos.findIndex((x) => x.id === t.id)]));
    const keys = new Set(ts.map((t) => `todo:${t.id}`));
    // The selection moves to the row after the last one deleted, else the one before the first.
    if (selected && keys.has(selected.key)) {
      const at = todoVisible.flatMap((r, i) => (keys.has(r.key) ? [i] : []));
      const next = todoVisible.slice((at[at.length - 1] ?? -1) + 1).find((r) => !keys.has(r.key)) ?? todoVisible.slice(0, at[0] ?? 0).reverse().find((r) => !keys.has(r.key));
      setSelectedKey(next?.key ?? null);
    }
    setPick(null);
    for (const t of ts) runTodo({ op: 'delete', id: t.id });
    // They go back one after another, first to last, so each lands where it was.
    const back = [...ts].sort((a, b) => (index.get(a.id) ?? 0) - (index.get(b.id) ?? 0));
    setTodoUndo({
      message: ts.length === 1 ? `Deleted “${ts[0].title || 'Untitled'}”.` : `Deleted ${ts.length} todos.`,
      undo: () => void back.reduce((done, t) => done.then(() => runTodo({ op: 'restore', todo: t, index: index.get(t.id) ?? 0 })), Promise.resolve()),
    });
  };

  /** Moves a todo to insertion point `index` among the rows its section shows. */
  const moveTodo = (section: TodoSectionId, id: string, index: number) => {
    const shown = todoSecs.find((s) => s.id === section)?.rows.map((r) => r.todo.id) ?? [];
    const all = todoStore.todos.map((t) => t.id);
    const next = reorderPinned(all, shown, id, index);
    if (next !== all) runTodo({ op: 'order', ids: next });
  };

  const labelsRef = useRef(todoStore.labels);
  labelsRef.current = todoStore.labels;
  /** Deletes a label everywhere; the toast can bring it back where it was. */
  const deleteLabel = (label: Label) => {
    const at = todoStore.labels.findIndex((l) => l.id === label.id);
    runTodo({ op: 'labels', labels: todoStore.labels.filter((l) => l.id !== label.id) });
    if (labelFilter === label.id) setLabelFilter(null);
    setTodoUndo({
      message: `Deleted the label “${label.name}”.`,
      undo: () => {
        const labels = labelsRef.current.filter((l) => l.id !== label.id);
        labels.splice(Math.min(at, labels.length), 0, label);
        runTodo({ op: 'labels', labels });
      },
    });
  };

  /** A label's right-click menu, in the label picker: its color, rename, delete. */
  const labelMenu = (label: Label, rename: () => void) => {
    void showContextMenu([
      {
        label: 'Color',
        items: LABEL_COLORS.map((c) => ({
          label: COLOR_WORD[c],
          checked: label.color === c,
          run: () => runTodo({ op: 'labels', labels: labelsRef.current.map((l) => (l.id === label.id ? { ...l, color: c } : l)) }),
        })),
      },
      { label: 'Rename…', run: rename },
      'separator',
      { label: 'Delete label', run: () => deleteLabel(label) },
    ]).catch((e: unknown) => setToast(String(e)));
  };

  /** The labels as checkmarks that put them on a todo or take them off. */
  const labelItems = (t: Todo): MenuSpec =>
    todoStore.labels.map((l) => ({
      label: l.name,
      checked: t.labels.includes(l.id),
      run: () => runTodo({ op: 'update', id: t.id, patch: { labels: t.labels.includes(l.id) ? t.labels.filter((x) => x !== l.id) : [...t.labels, l.id] } }),
    }));

  /** The projects a todo can be in: every one but Elsewhere. */
  const todoProjects = useMemo(() => projects.filter((p) => p.name !== ELSEWHERE).map((p) => p.name), [projects]);

  /** Projects a todo can move to, the one it's in checked. */
  const projectItems = (t: Todo): MenuSpec =>
    projects
      .filter((p) => p.name !== ELSEWHERE)
      .map((p) => ({ label: p.name, checked: t.project === p.name, run: () => runTodo({ op: 'update', id: t.id, patch: { project: p.name } }) }));

  /** Copies todos as the prompt they'd start a session with, attachments as their paths. */
  const copyPrompt = (ts: Todo[]) => {
    expandAttachments(todosPrompt(ts))
      .then((text) => copy(text, ts.length > 1 ? 'todos as a prompt' : 'todo as a prompt'))
      .catch((e: unknown) => setToast(String(e)));
  };

  /** A todo's right-click menu. */
  const todoMenu = (r: TodoItem) => void showContextMenu(todoMenuSpec(r)).catch((e: unknown) => setToast(String(e)));
  const todoMenuSpec = (r: TodoItem): MenuSpec => {
    const t = r.todo;
    const agent = r.sessions.find((x) => x.session.live) ?? r.sessions[0];
    return [
      { label: 'Start session…', run: () => startForTodo(r) },
      ...(agent ? [{ label: `Open ${agent.session.account}: ${agent.session.title.slice(0, 48)}`, run: () => open(agent.lane, agent.session.openTarget) }] : []),
      'separator',
      ...STATUSES.map((st) => ({ label: STATUS_WORD[st], checked: t.status === st, run: () => setTodoStatus(t, st) })),
      ...waitingItem([t]),
      'separator',
      { label: 'Labels', items: labelItems(t) },
      { label: 'Move to project', items: projectItems(t) },
      ...desktopItem('todo', t.id),
      'separator',
      { label: 'Copy as prompt', run: () => copyPrompt([t]) },
      { label: 'Copy todo ID', run: () => copy(t.id, 'todo ID') },
      ...canvasItem('todo', t.id),
      'separator',
      { label: 'Delete todo', run: () => deleteTodos([t]) },
    ];
  };

  /** The right-click menu of several todos picked together, and their panel's ⋯ menu. */
  const pickMenuSpec = (rows: TodoItem[]): MenuSpec => {
    const ts = rows.map((r) => r.todo);
    return [
      { label: `Start one session for ${rows.length} todos…`, run: () => startForTodos(rows) },
      'separator',
      ...STATUSES.map((st) => ({ label: STATUS_WORD[st], checked: ts.every((t) => t.status === st), run: () => ts.forEach((t) => setTodoStatus(t, st)) })),
      ...waitingItem(ts),
      'separator',
      { label: 'Copy as prompt', run: () => copyPrompt(ts) },
      { label: 'Copy todo IDs', run: () => copy(ts.map((t) => t.id).join(' '), 'todo IDs') },
      'separator',
      { label: `Delete ${rows.length} todos`, run: () => deleteTodos(ts) },
    ];
  };
  const pickMenu = (rows: TodoItem[], at?: { x: number; y: number }) => void showContextMenu(pickMenuSpec(rows), at).catch((e: unknown) => setToast(String(e)));

  /** The panel's ⋯ menu. */
  const todoMoreMenu = (r: TodoItem, at: { x: number; y: number }) => {
    const t = r.todo;
    void showContextMenu(
      [
        { label: 'Move to project', items: projectItems(t) },
        { label: 'Copy as prompt', run: () => copyPrompt([t]) },
        { label: 'Copy todo ID', run: () => copy(t.id, 'todo ID') },
        'separator',
        { label: 'Delete todo', run: () => deleteTodos([t]) },
      ],
      at,
    ).catch((e: unknown) => setToast(String(e)));
  };

  /** Link…: the project's worktrees (pinned first) and recent sessions a todo isn't linked to. */
  const todoLinkMenu = (r: TodoItem, at: { x: number; y: number }) => {
    const t = r.todo;
    const inProjectLanes = lanes.filter((l) => !l.hidden && (t.project ? l.project === t.project : inProject(l, current)));
    const pinnedFirst = [...lanePins.keys.flatMap((id) => inProjectLanes.filter((l) => l.id === id)), ...inProjectLanes.filter((l) => !lanePins.set.has(l.id)).sort((a, b) => b.lastActivity - a.lastActivity)];
    const worktreeItems = pinnedFirst
      .filter((l) => !t.worktrees.includes(l.id))
      .slice(0, 20)
      .map((l) => ({
        label: l.branch && l.branch !== l.callsign ? `${l.callsign} · ${l.branch}` : l.callsign,
        run: () => runTodo({ op: 'link', id: t.id, worktree: l.id, linked: true }),
      }));
    const sessionItems = inProjectLanes
      .flatMap((lane) => lane.sessions.filter((s) => !s.visitor && !s.archived && !t.sessions.includes(s.key)).map((session) => ({ lane, session })))
      .sort((a, b) => b.session.lastActivity - a.session.lastActivity)
      .slice(0, 20)
      .map(({ lane, session }) => ({
        label: `${session.account}: ${session.title.slice(0, 56)} (${lane.callsign})`,
        run: () => runTodo({ op: 'link', id: t.id, session: session.key, linked: true }),
      }));
    void showContextMenu(
      [
        { label: 'Worktree', items: worktreeItems },
        { label: 'Session', items: sessionItems },
      ],
      at,
    ).catch((e: unknown) => setToast(String(e)));
  };

  /**
   * "Add to todo" for a session's or a worktree's menu: the project's open todos it isn't on,
   * newest first, and a new one; then taking it off the todos it's on.
   */
  const todoMenuItems = (project: string, link: { worktree?: string; session?: string }, context: ComposerContext, title = ''): MenuSpec => {
    const on = todoStore.todos.filter((t) => (link.session ? t.sessions.includes(link.session) : t.worktrees.includes(link.worktree ?? '')));
    const open = todoStore.todos
      .filter((t) => t.status !== 'done' && !on.includes(t) && (!t.project || t.project === project))
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .slice(0, 15);
    return [
      {
        label: 'Add to todo',
        items: [
          ...open.map((t) => ({ label: t.title || 'Untitled', run: () => runTodo({ op: 'link', id: t.id, ...link, linked: true }) })),
          ...(open.length > 0 ? ['separator' as const] : []),
          { label: 'New todo…', run: () => newTodoFrom(context, title) },
        ],
      },
      ...on.map((t) => ({ label: `Remove from todo “${(t.title || 'Untitled').slice(0, 40)}”`, run: () => runTodo({ op: 'link', id: t.id, ...link, linked: false }) })),
    ];
  };

  // Where each account can start a session, for Review with: read at launch, and again each time
  // the menu is built, so a menu goes by the last answer and never waits on one.
  const startOpts = useRef<StartOptions | null>(null);
  const loadStartOpts = useCallback(() => {
    startOptions()
      .then((o) => (startOpts.current = o))
      .catch(() => {
        // Keeps the last answer.
      });
  }, []);
  useEffect(loadStartOpts, [loadStartOpts]);
  /** Review with…'s items for a PR: its review page in the Codex app, or a session on a review
   *  prompt in a worktree of its repo on this Mac (review.ts). */
  const reviewItems = (pr: Pick<PrItem, 'number' | 'title' | 'url' | 'repoSlug' | 'lanes'>): MenuAction[] => {
    loadStartOpts();
    const at = reviewLane(pr, lanes);
    const picks = reviewPicks(startOpts.current?.accounts ?? [], board?.usage ?? [], Date.now(), !!at, remembered().places);
    return picks.map((pick) => ({
      label: pick.label,
      run: () => {
        if (pick.kind === 'codex-app' || !at) {
          reviewInCodex(pr.url).catch((e: unknown) => setToast(String(e)));
          return;
        }
        const spec = { laneId: at.lane.id, account: pick.account, place: pick.place, prompt: reviewPrompt(pr, at.checkedOut), model: null, effort: null, worktree: false, todos: [] };
        track(`Starting a review of #${pr.number} on ${pick.account}…`, startSession(spec))
          .then((started) => {
            if (started?.message) setToast(started.message);
            if (started?.key) setPendingSession({ key: started.key, until: Date.now() + 60_000 });
          })
          .catch((e: unknown) => setToast(String(e).replace(/^Error: /, '')));
      },
    }));
  };
  /** A worktree's PR as Review with… needs it: the board's, or what the worktree knows of one
   *  the board doesn't list (on another Mac, say). */
  const reviewedPr = (lane: Lane, p: Pr) => board?.prs.find((x) => x.url === p.url) ?? { ...p, repoSlug: lane.repoSlugNow ?? lane.repoSlug ?? '', lanes: [lane.id] };
  /** Drops Review with… from a button, as the PR panels have it. */
  const reviewMenuFrom = (pr: Pick<PrItem, 'number' | 'title' | 'url' | 'repoSlug' | 'lanes'>, el: Element) => {
    const items = reviewItems(pr);
    const spec = items.length > 0 ? items : [{ label: 'No account can start a review here', run: () => {}, enabled: false }];
    void showContextMenu(spec, below(el)).catch((e: unknown) => setToast(String(e)));
  };

  /** A PR's right-click menu. */
  const prMenuSpec = (r: PrRowItem): MenuSpec => {
    const p = r.pr;
    return [
        { label: 'Open on GitHub', run: () => void openUrl(p.url) },
        { label: 'Review with', items: reviewItems(p) },
        ...(p.reviewRequested && prIsOpen(p) ? ['separator' as const, { label: `Approve #${p.number}`, run: () => approve(p) }] : []),
        'separator',
        ...(r.lane ? [{ label: `Show worktree ${r.lane.callsign}`, run: () => r.lane && focusLane(r.lane) }] : []),
        { label: 'Copy link', run: () => copy(p.url, 'PR link') },
        { label: 'Copy branch name', run: () => copy(p.head, 'branch name') },
        ...canvasItem('pr', p.key),
    ];
  };
  const prMenu = (r: PrRowItem) => void showContextMenu(prMenuSpec(r)).catch((e: unknown) => setToast(String(e)));
  /** The right-click menu of a worktree's PR chip: the PR's own. A PR the board doesn't list,
   *  such as one on another Mac in a repo this one doesn't have, gets what its link allows. */
  const lanePrMenu = (lane: Lane) => {
    const p = lane.pr;
    if (!p) return;
    const item = board?.prs.find((x) => x.url === p.url);
    const spec: MenuSpec = item
      ? prMenuSpec(prRowOf(item))
      : [
          { label: 'Open on GitHub', run: () => void openUrl(p.url) },
          { label: 'Review with', items: reviewItems(reviewedPr(lane, p)) },
          'separator',
          { label: 'Copy link', run: () => copy(p.url, 'PR link') },
        ];
    void showContextMenu(spec).catch((e: unknown) => setToast(String(e)));
  };

  /** Pins or unpins a worktree. Its lines keep their keys as the row moves, so what was selected stays selected. */
  const toggleLanePin = useCallback((lane: Lane) => lanePins.toggle(lane.id), [lanePins]);

  /**
   * A session's right-click menu: open it, fork it, mark it seen, pin it, snooze it, archive it,
   * copy its ID. Snoozes keep sessions off the floating bar, which shows this Mac's only. Each list pins what
   * it shows: Sessions pins the session; the board and the sidebar, lists of worktrees, pin the
   * session's worktree.
   */
  const todoItemsRef = useRef(todoMenuItems);
  todoItemsRef.current = todoMenuItems;
  /** Marks a finished turn seen: it goes idle, and leaves Your turn and the bar. */
  const seen = (s: Session) => markSeen([s.key]);
  /** Forks a session into a new terminal, and selects the copy once it shows, as a start does. */
  const fork = (s: Session) =>
    track(`Forking ${s.title}…`, forkSession(s.key))
      .then((started) => {
        if (started?.message) setToast(started.message);
        if (started?.key) setPendingSession({ key: started.key, until: Date.now() + 60_000 });
      })
      .catch((e: unknown) => setToast(String(e).replace(/^Error: /, '')));
  const editorIcons = useEditorIcons(board?.apps);
  /** Opens a worktree's folder in an editor from a session's menu: yours, or, with others
   *  installed, any of them. */
  const folderMenuItems = (lane: Lane): MenuSpec => (board && opensInEditor(lane) ? folderMenu(board.apps, editorIcons, (id) => open(lane, id)) : []);
  const sessionMenuSpec = (lane: Lane, s: Session, pin: 'session' | 'worktree'): MenuSpec => {
      const target = lane.targets.find((t) => t.id === s.openTarget);
      const pinItem =
        pin === 'session'
          ? { label: sessionPins.set.has(s.key) ? 'Unpin session' : 'Pin session', run: () => sessionPins.toggle(s.key) }
          : { label: lanePins.set.has(lane.id) ? 'Unpin worktree' : 'Pin worktree', run: () => toggleLanePin(lane) };
      return [
        { label: target?.label ?? 'Open', run: () => open(lane, s.openTarget) },
        ...folderMenuItems(lane),
        { label: 'New session here…', run: () => newSession({ laneId: lane.id }) },
        // Forks open in a terminal, where only sessions that resume in one can go.
        ...(s.resumeCommand ? [{ label: 'Fork in terminal', run: () => void fork(s) }] : []),
        'separator',
        ...todoItemsRef.current(lane.project, { session: s.key }, { session: s.key, worktree: lane.id, label: `${s.account} in ${lane.callsign}` }, s.title),
        'separator',
        ...(seenable(s) ? [{ label: 'Mark as seen', run: () => seen(s) }] : []),
        ...(s.archived ? [] : [pinItem, ...desktopItem('session', s.key)]),
        ...(s.archived || shortOf(s.key) ? [] : [snoozeMenu([s.key], Date.now(), { until: s.snoozedUntil, onDone: (act) => void act.catch((e: unknown) => setToast(String(e))) })]),
        { label: s.archived ? 'Unarchive' : s.harness === 'codex' ? 'Archive in Codex' : 'Archive', run: () => setArchived(s.key, !s.archived, s.title) },
        'separator',
        { label: 'Copy session ID', run: () => copy(s.id, 'session ID') },
        ...(s.resumeCommand ? [{ label: 'Copy resume command', run: () => copy(s.resumeCommand, 'resume command') }] : []),
        ...(s.visitor ? [] : canvasItem('session', s.key)),
      ];
  };
  const sessionMenu = (lane: Lane, s: Session, pin: 'session' | 'worktree') => void showContextMenu(sessionMenuSpec(lane, s, pin)).catch((e: unknown) => setToast(String(e)));

  /** A worktree's right-click menu: open it, pin it, copy its path, branch or PR link. */
  const laneMenuSpec = (lane: Lane): MenuSpec => {
      const main = lane.targets.find((t) => t.id === lane.defaultTarget);
      const pr = lane.pr;
      const item = (id: string): MenuSpec => {
        const t = lane.targets.find((x) => x.id === id);
        return t && id !== lane.defaultTarget ? [{ label: t.label, run: () => open(lane, id) }] : [];
      };
      return [
        { label: main?.label ?? 'Open', run: () => open(lane, lane.defaultTarget) },
        // Open in <yours>, then the other editors installed here.
        ...item('l|editor'),
        ...(board && opensInEditor(lane) ? openInMenu(board.apps, editorIcons, (id) => open(lane, id)) : []),
        ...item('l|finder'),
        ...item('l|preview'),
        { label: 'New session here…', run: () => newSession({ laneId: lane.id }) },
        ...(lane.device ? [] : [{ label: lane.preview?.set ? 'Change preview URL…' : 'Set preview URL…', run: () => setPreviewFor(lane.id) }]),
        'separator',
        ...todoItemsRef.current(lane.project, { worktree: lane.id }, { worktree: lane.id, label: lane.callsign }),
        'separator',
        { label: lanePins.set.has(lane.id) ? 'Unpin worktree' : 'Pin worktree', run: () => toggleLanePin(lane) },
        ...desktopItem('worktree', lane.id),
        'separator',
        { label: 'Copy path', run: () => copy(lane.path, 'worktree path') },
        ...(lane.branch ? [{ label: 'Copy branch name', run: () => copy(lane.branch, 'branch name') }] : []),
        ...(pr ? [{ label: 'Copy PR link', run: () => copy(pr.url, 'PR link') }] : []),
        ...(lane.preview ? [{ label: 'Copy preview URL', run: () => copy(lane.preview?.url, 'preview URL') }] : []),
        ...canvasItem('worktree', lane.id),
      ];
  };
  const laneMenu = (lane: Lane) => void showContextMenu(laneMenuSpec(lane)).catch((e: unknown) => setToast(String(e)));

  /** Moves a pinned session to insertion point `index` among the pinned ones showing. */
  const movePinnedSession = useCallback(
    (key: string, index: number) => sessionPins.move(pinnedRows.map((r) => r.session.key), key, index),
    [sessionPins, pinnedRows],
  );

  /** Moves a pinned worktree to insertion point `index` among the pinned ones showing. */
  const movePinnedLane = useCallback(
    (id: string, index: number) => lanePins.move(pinnedLaneRows.map((r) => r.lane.id), id, index),
    [lanePins, pinnedLaneRows],
  );

  /** Selects a worktree from the sidebar, in the Worktrees view: its first agent's line, else the worktree. */
  const focusLane = (lane: Lane) => {
    // On the canvas, the worktree's card: selected, in view, with its panel.
    if (view === 'canvas') {
      showOnCanvas('worktree', lane.id);
      return;
    }
    setView('worktrees');
    setBayFilter(null);
    if (lane.parked) setShowParked(true);
    setSelectedKey(boardRowKey(lane));
  };

  /** Selects a session: in Sessions its own row; on the board its line on its worktree's row, else the worktree. */
  const focusSession = (lane: Lane, session: Session) => {
    setBayFilter(null);
    if (view === 'sessions') {
      const item = sessionItems.find((r) => r.session.key === session.key);
      if (item) {
        setSelectedKey(item.key);
        return;
      }
    }
    const hasLine = !session.visitor && AGENT_BAY[session.status];
    if (!hasLine && lane.parked) setShowParked(true);
    setSelectedKey(hasLine ? `${lane.id}::${session.key}` : boardRowKey(lane));
  };

  /** Shows a session in its own panel, from the sidebar: its card on the canvas, else its row in
   *  Sessions with the cursor in its box, as a click there does. */
  const showSession = (lane: Lane, session: Session) => {
    if (view === 'canvas') {
      showOnCanvas('session', session.key);
      return;
    }
    // Sessions lists it once, under the worktree it runs in: a visitor here is a row there, and
    // that worktree can be in another project.
    const home = lanes.find((l) => l.sessions.some((s) => s.key === session.key && !s.visitor)) ?? lane;
    const item: SessionItem = { kind: 'agent', key: `${home.id}::${session.key}`, lane: home, session: home.sessions.find((s) => s.key === session.key) ?? session };
    if (!inProject(home, current) && !home.hidden) pickProject(home.project);
    // The sidebar doesn't follow the search or the Inbox filter; Sessions does.
    if (!sessionInQuery(item, query)) setQuery('');
    if (!sessionInFilter(item, bayFilter)) setBayFilter(null);
    setView('sessions');
    setSelectedKey(item.key);
    focusBox();
  };

  // Where the bar shows: floating, docked in the notch, or off; changed here, from its own menu or
  // from the Window menu. Picking the place it's in turns it off.
  const [barPlace, setBarPlace] = useState<BarMode>('floating');
  useEffect(() => {
    void barMode().then((v) => setBarPlace(v.mode));
    return onBarMode((v) => setBarPlace(v.mode));
  }, []);
  const pickBarPlace = (picked: Exclude<BarMode, 'off'>) => {
    const before = barPlace;
    const next = before === picked ? 'off' : picked;
    setBarPlace(next);
    setBarMode(next).catch((e: unknown) => {
      setBarPlace(before);
      setToast(String(e));
    });
  };

  // The floating bar, the quick panel or a card on the desktop asks to show a session, worktree,
  // PR or todo here: in its project, with no search hiding it.
  const reveal = useRef<(r: Reveal) => void>(() => {});
  reveal.current = (r: Reveal) => {
    // A todo the quick panel found: the Todo view, in its project.
    if (r.kind === 'todo') {
      const t = todoStore.todos.find((x) => x.id === r.id);
      if (t) showTodo(t);
      return;
    }
    const lane = r.kind === 'session' || r.kind === 'worktree' ? lanes.find((l) => l.id === r.laneId) : undefined;
    const session = lane?.sessions.find((s) => s.key === (r.kind === 'session' ? r.key : ''));
    const pr = r.kind === 'pr' ? board?.prs.find((p) => p.key === r.key) : undefined;
    const home = lane?.project ?? pr?.project;
    if (!home) return;
    if (current !== ALL_PROJECTS && current !== home) pickProject(home);
    setQuery('');
    if (lane && r.kind === 'worktree') {
      focusLane(lane);
    } else if (lane && session) {
      // A session Codealong runs is read and answered in its panel, which Sessions shows.
      if (session.hosted) setView('sessions');
      else if (view === 'prs' || view === 'todo') setView('worktrees');
      setBayFilter(null);
      // Its own row in Sessions, its line on its worktree's row on the board.
      setSelectedKey(`${lane.id}::${session.key}`);
    } else if (pr) {
      setView('prs');
      setBayFilter(null);
      setSelectedKey(`pr:${pr.key}`);
    }
  };
  useEffect(() => onReveal((r) => reveal.current(r)), []);

  // The Todo view's own keys, bound fresh each render so they see the rows as they are. N adds a
  // todo, X marks one done (or reopens it), W moves it to Waiting or out, Delete deletes it,
  // ⌥↑ ⌥↓ move it. ⇧↑ ⇧↓ pick the rows above or below too; then X, W and Delete act on all of
  // them, Return starts one session for them, and Escape goes back to one.
  const todoKeys = useRef<(e: KeyboardEvent, key: string) => boolean>(() => false);
  todoKeys.current = (e, key) => {
    if (view !== 'todo' || e.metaKey || e.ctrlKey) return false;
    if (key === 'n' && !e.altKey) {
      e.preventDefault();
      setComposerFocus((n) => n + 1);
      return true;
    }
    if (selected?.kind !== 'todo') return false;
    const t = selected.todo;
    const ts = pickedRows.length > 0 ? pickedRows.map((r) => r.todo) : [t];
    if (e.shiftKey && !e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      e.preventDefault();
      const i = visible.indexOf(selected);
      const to = visible[Math.min(visible.length - 1, Math.max(0, i + (e.key === 'ArrowUp' ? -1 : 1)))];
      if (to?.kind === 'todo') selectTodoRow(to, 'range');
      return true;
    }
    if (e.key === 'Escape' && pickedRows.length > 0) {
      setPick(null);
      return true;
    }
    if (e.key === 'Enter' && !e.altKey && pickedRows.length > 0) {
      e.preventDefault();
      startForTodos(pickedRows);
      return true;
    }
    if (key === 'x' && !e.altKey) {
      // Done, unless they all are: then each goes back to where it was.
      const reopen = ts.every((x) => x.status === 'done');
      for (const x of ts) if (reopen || x.status !== 'done') setTodoStatus(x, !reopen ? 'done' : x.sessions.length > 0 ? 'in-progress' : 'todo');
      return true;
    }
    if (key === 'w' && !e.altKey) {
      toggleWaiting(ts);
      return true;
    }
    // Only from the list: a button in the panel keeps the keyboard's other uses.
    const onList = !document.activeElement || document.activeElement === document.body || !!document.activeElement.closest('.ca-board-list');
    if ((e.key === 'Backspace' || e.key === 'Delete') && onList) {
      e.preventDefault();
      deleteTodos(ts);
      return true;
    }
    // Reordering moves one todo at a time.
    if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown') && pickedRows.length > 0) {
      e.preventDefault();
      return true;
    }
    if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown') && t.status !== 'done') {
      e.preventDefault();
      const section = todoSecs.find((s) => s.rows.some((r) => r.key === selected.key));
      const i = section?.rows.findIndex((r) => r.key === selected.key) ?? -1;
      // Insertion points: one up is the row's own index less one; one down is two past it.
      if (section && i >= 0) moveTodo(section.id, t.id, e.key === 'ArrowUp' ? i - 1 : i + 2);
      return true;
    }
    return false;
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = document.activeElement === searchRef.current;
      const mod = e.metaKey;
      const key = e.key.toLowerCase();
      // Escape in a session panel's box gives the list its keys back (a click on a row put them
      // there).
      if (e.key === 'Escape' && !e.isComposing && view === 'sessions' && document.activeElement?.closest('.ca-sessinsp .ca-composer')) {
        e.preventDefault();
        listRef.current?.focus();
        return;
      }
      // So does a text field, a todo's title or description say, but for the window's own ⌘ keys.
      if (inField(document.activeElement, searchRef.current) && !(mod && (key === 'k' || key === 'p' || e.key === ',' || (e.code === 'BracketRight' && view === 'sessions') || (e.altKey && /^Digit\d$/.test(e.code))))) return;
      if (mod && e.key === ',') {
        e.preventDefault();
        openSettings();
        return;
      }
      // Settings has the keyboard while it's open, as the preview URL sheet does.
      if (settings || previewFor) return;
      // ⌘] skips to the next session that waits on you, from the panel's box too. ] goes by its
      // key's code, which layouts that type it with ⌥ keep.
      if (mod && !e.ctrlKey && !e.shiftKey && e.code === 'BracketRight' && view === 'sessions') {
        e.preventDefault();
        if (selected?.kind === 'agent') goOn(selected.key, selected.session, null);
        return;
      }
      if (mod && key === 'k') {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
        return;
      }
      if (mod && key === 'p') {
        e.preventDefault();
        openSwitcher();
        return;
      }
      // ⌥⌘ digits switch projects. ⌥ changes the character, so they go by the key's code.
      const digit = /^Digit(\d)$/.exec(e.code);
      if (mod && e.altKey && !e.shiftKey && digit) {
        e.preventDefault();
        const name = projectForDigit(Number(digit[1]), projects);
        if (name) pickProject(name);
        return;
      }
      // ⌘⇧N is File › New window's, so it works with no window open too.
      if (mod && !e.shiftKey && key === 'n') {
        e.preventDefault();
        // On a todo, the session is for the todo; on several picked, one session for all of them.
        if (pickedRows.length > 0) startForTodos(pickedRows);
        else if (selected?.kind === 'todo') startForTodo(selected);
        else newSession({ laneId: startLaneId });
        return;
      }
      if (typing) {
        if (e.key === 'Escape') {
          setQuery('');
          searchRef.current?.blur();
        } else if (e.key === 'ArrowDown' || e.key === 'Enter') {
          e.preventDefault();
          searchRef.current?.blur();
          if (e.key === 'Enter') {
            if (view === 'canvas') setCanvasFit((n) => n + 1);
            else openRow(selected);
          }
        }
        return;
      }
      // The canvas's keys, unless the keyboard is in the panel beside it.
      if (view === 'canvas' && !document.activeElement?.closest('.ca-insp') && canvasKeys.current(e)) return;
      if (gPending.current) {
        gPending.current = false;
        const bay = ({ n: 'needs', w: 'working', t: 'unread', r: 'review' } as Record<string, Bay | undefined>)[key];
        const first =
          view === 'worktrees'
            ? [...pinnedLaneRows, ...shownBays.flatMap((b) => b.rows)].filter((r) => r.lane.bay === bay).flatMap(rowItems)[0]
            : visible.find((r) => r.kind === 'agent' && AGENT_BAY[r.session.status] === bay);
        if (first) setSelectedKey(first.key);
        return;
      }
      const lane = selectedLane;
      if (key === 'v' && !mod) {
        const i = VIEWS.findIndex((x) => x.id === view);
        setView(VIEWS[(i + 1) % VIEWS.length].id);
        return;
      }
      if (todoKeys.current(e, key)) return;
      // P pins what the view lists: worktrees on the board, sessions in Sessions.
      if (key === 'p' && !mod && selected && selected.kind !== 'pr' && selected.kind !== 'todo') {
        if (view === 'worktrees') toggleLanePin(selected.lane);
        else if (selected.kind === 'agent' && !selected.session.archived) sessionPins.toggle(selected.session.key);
        return;
      }
      if (key === 'a' && !mod && selected?.kind === 'agent') {
        setArchived(selected.session.key, !selected.session.archived, selected.session.title);
        return;
      }
      if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
        // Insertion points: one up is the row's own index less one; one down is two past it.
        const step = (i: number) => (e.key === 'ArrowUp' ? i - 1 : i + 2);
        if (view === 'sessions' && selected?.kind === 'agent' && sessionPins.set.has(selected.session.key)) {
          e.preventDefault();
          movePinnedSession(selected.session.key, step(pinnedRows.findIndex((r) => r.key === selected.key)));
          return;
        }
        if (view === 'worktrees' && selected && selected.kind !== 'pr' && selected.kind !== 'todo' && lanePins.set.has(selected.lane.id)) {
          e.preventDefault();
          movePinnedLane(selected.lane.id, step(pinnedLaneRows.findIndex((r) => r.key === selected.lane.id)));
          return;
        }
      }
      if (e.key === '/') {
        e.preventDefault();
        searchRef.current?.focus();
      } else if (e.key === 'ArrowDown' || (key === 'j' && !mod)) {
        e.preventDefault();
        move(1);
      } else if (e.key === 'ArrowUp' || (key === 'k' && !mod)) {
        e.preventDefault();
        move(-1);
      } else if (e.key === 'Enter' && !mod) {
        e.preventDefault();
        openRow(selected);
      } else if (e.key === 'Enter' && mod) {
        e.preventDefault();
        const primary = selected?.kind === 'agent' ? selected.session.openTarget : lane?.defaultTarget;
        const alt = lane?.targets.find(
          (t) => t.group === 'session' && !t.disabled && t.id !== primary && (selected?.kind !== 'agent' || t.id.startsWith(`s|${selected.session.key}|`)),
        );
        open(lane, alt?.id ?? 'l|editor');
      } else if (mod && key === '1') {
        e.preventDefault();
        open(lane, 'l|terminal');
      } else if (mod && key === '2') {
        e.preventDefault();
        open(lane, 'l|editor');
      } else if (mod && key === '3') {
        e.preventDefault();
        open(lane, lane?.targets.find((t) => t.id.endsWith('|claude-app') && !t.disabled)?.id);
      } else if (mod && key === '4') {
        e.preventDefault();
        open(lane, lane?.targets.find((t) => t.id.endsWith('|codex') && !t.disabled)?.id);
      } else if (mod && key === '5') {
        e.preventDefault();
        if (lane?.preview) open(lane, 'l|preview');
      } else if (mod && key === 'o') {
        e.preventDefault();
        if (selected?.kind === 'pr') openRow(selected);
        else if (lane?.pr) open(lane, 'p|github');
      } else if (mod && e.shiftKey && key === 'c') {
        e.preventDefault();
        copy(lane?.path, 'worktree path');
      } else if (mod && key === 'c' && !window.getSelection()?.toString()) {
        e.preventDefault();
        copy(lane?.branch, 'branch name');
      } else if (key === 'e' && !mod) {
        const sessions = selected?.kind === 'agent' ? [selected.session] : selected?.kind === 'todo' ? (pickedRows.length > 0 ? pickedRows : [selected]).flatMap((r) => r.sessions.map((x) => x.session)) : (lane?.sessions ?? []);
        markSeen(sessions.filter(seenable).map((s) => s.key));
      } else if (key === 'g' && !mod) {
        gPending.current = true;
        window.setTimeout(() => (gPending.current = false), 1200);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [visible, selected, pickedRows, selectedLane, startLaneId, move, open, openRow, copy, settings, previewFor, openSettings, newSession, view, setView, toggleLanePin, sessionPins, lanePins, pinnedRows, pinnedLaneRows, shownBays, movePinnedSession, movePinnedLane, current, setArchived, projects]);

  // Starts this window asked the quick panel for. A spinner shows while the harness takes the
  // session; then what's next, and the session is selected when it shows up if Codealong chose
  // its ID. A failed start brings the panel back with what you wrote and why.
  useEffect(() => {
    const timers = new Map<number, number>();
    const off = onQuickStart((e) => {
      if (e.phase === 'begin') {
        timers.set(e.id, window.setTimeout(() => setBusy((b) => [...b, { id: e.id, label: e.label }]), 300));
        return;
      }
      window.clearTimeout(timers.get(e.id));
      timers.delete(e.id);
      setBusy((b) => b.filter((x) => x.id !== e.id));
      if (e.message) setToast(e.message);
      if (e.key) setPendingSession({ key: e.key, until: Date.now() + 60_000 });
    });
    return () => {
      off();
      timers.forEach((t) => window.clearTimeout(t));
    };
  }, []);

  useEffect(() => {
    if (!pendingSession) return;
    if (Date.now() > pendingSession.until) {
      setPendingSession(null);
      return;
    }
    for (const lane of lanes) {
      const session = lane.sessions.find((s) => s.key === pendingSession.key);
      if (!session) continue;
      setPendingSession(null);
      // Started from a todo: the todo stays selected, and its panel shows the new session.
      if (view === 'todo') return;
      if (view === 'prs') setView('worktrees');
      focusSession(lane, session);
      return;
    }
  }, [lanes, pendingSession]);

  const toggleBay = (f: Filter) => {
    setBayFilter(bayFilter === f ? null : f);
    if (view === 'canvas' && bayFilter !== f) setCanvasFit((n) => n + 1);
  };
  const openWindow = (name: string, another = false) => {
    openProjectWindow(name, another).catch((e: unknown) => setToast(String(e)));
  };
  // File › New window (⌘⇧N): one more window on the project this one shows.
  const newWindow = useRef(() => {});
  newWindow.current = () => openWindow(current, true);
  useEffect(() => onNewWindow(() => newWindow.current()), []);
  // In the switcher, the button on the project you're looking at does the same (its tooltip
  // says ⌘⇧N); on another project it brings that project's window forward if it has one.
  const switcherWindow = (name: string) => openWindow(name, name === current);
  const editConfig = () => {
    openConfig().catch((e: unknown) => setToast(String(e)));
  };
  const startSignIn = (account: string) => {
    signIn(account).catch((e: unknown) => setToast(String(e)));
  };

  // The sidebar's worktrees: pinned ones first in your order, then the rest grouped by repo,
  // active ones first and parked ones last.
  const worktrees = useMemo((): TreeGroup[] => {
    const byId = new Map(recentLanes.map((l) => [l.id, l]));
    const pinned = lanePins.keys.flatMap((id) => byId.get(id) ?? []);
    const byRepo = new Map<string, Lane[]>();
    for (const l of recentLanes) if (!lanePins.set.has(l.id)) byRepo.set(l.repo, [...(byRepo.get(l.repo) ?? []), l]);
    const rank = (l: Lane) => (l.parked ? 9 : BAYS.indexOf(l.bay));
    const repos = [...byRepo.entries()]
      .sort((a, b) => b[1].length - a[1].length || a[0].localeCompare(b[0]))
      .map(([repo, ls]) => ({ repo, lanes: ls.sort((a, b) => rank(a) - rank(b) || lanePromptAt(b) - lanePromptAt(a) || b.lastActivity - a.lastActivity) }));
    return pinned.length > 0 ? [{ repo: '', lanes: pinned, pinned: true }, ...repos] : repos;
  }, [recentLanes, lanePins]);

  /** Opens a card on the canvas where it lives, as a double-click on its row would. */
  const openCard = (it: RefItem) => {
    const e = canvasEntities;
    if (it.ref === 'worktree') {
      const lane = e.lanes.get(it.key);
      if (lane) open(lane, lane.defaultTarget);
    } else if (it.ref === 'session') {
      const p = e.sessions.get(it.key);
      if (p) open(p.lane, p.session.openTarget);
    } else if (it.ref === 'todo') {
      openRow(todoItems.find((r) => r.todo.id === it.key) ?? null);
    } else {
      const pr = e.prs.get(it.key);
      if (pr) openUrl(pr.url).catch((err: unknown) => setToast(String(err)));
    }
  };

  /** Whether a card matches the filter at the top and the Inbox filter, as its row would in its own view. */
  const canvasMatches = useCallback(
    (it: RefItem) => {
      const e = canvasEntities;
      switch (it.ref) {
        case 'worktree': {
          const lane = e.lanes.get(it.key);
          return !!lane && inQuery(lane, query) && !!narrowed(lane, bayFilter, since);
        }
        case 'session': {
          const p = e.sessions.get(it.key);
          const row: SessionItem | null = p ? { kind: 'agent', key: it.key, lane: p.lane, session: p.session } : null;
          return !!row && sessionInQuery(row, query) && sessionInFilter(row, bayFilter);
        }
        case 'todo': {
          const r = todoItems.find((x) => x.todo.id === it.key);
          return !!r && todoInQuery(r, query) && todoInFilter(r, bayFilter);
        }
        case 'pr': {
          const pr = e.prs.get(it.key);
          return !!pr && prInQuery(pr, query) && prInFilter(prRowOf(pr), bayFilter);
        }
      }
    },
    [canvasEntities, todoItems, query, bayFilter, since],
  );
  // Picking an Inbox filter, or pressing Return in the filter, brings what matches into view.
  const [canvasFit, setCanvasFit] = useState(0);

  /** A card's right-click menu: the one its row has in the other views. */
  const cardMenu = (it: RefItem): MenuSpec => {
    const e = canvasEntities;
    if (it.ref === 'worktree') {
      const lane = e.lanes.get(it.key);
      return lane ? laneMenuSpec(lane) : [];
    }
    if (it.ref === 'session') {
      const p = e.sessions.get(it.key);
      return p ? sessionMenuSpec(p.lane, p.session, 'session') : [];
    }
    if (it.ref === 'todo') {
      const r = todoItems.find((x) => x.todo.id === it.key);
      return r ? [...todoMenuSpec(r), 'separator', { label: 'Show in Todo', run: () => showTodo(r.todo) }] : [];
    }
    const pr = e.prs.get(it.key);
    return pr ? prMenuSpec(prRowOf(pr)) : [];
  };

  /** A PR as the PRs view lists it: with the worktrees that hold it and the sessions that worked on it. */
  const prRowOf = (pr: PrItem): PrRowItem => {
    const holding = pr.lanes.flatMap((id) => laneById.get(id) ?? []);
    const sessions = pr.sessions.flatMap((x) => sessionIndex.get(`${x.lane}::${x.key}`) ?? []);
    return { kind: 'pr', key: `pr:${pr.key}`, pr, lane: holding[0] ?? null, sessions, places: prPlaces(holding, sessions) };
  };

  // In the canvas's panel, a worktree or todo it links to is shown on the canvas when it's there.
  const showLaneFromPanel = (lane: Lane) => {
    if (view === 'canvas' && canvas.has('worktree', lane.id)) setCanvasFocus((f) => ({ id: refId('worktree', lane.id), n: (f?.n ?? 0) + 1 }));
    else focusLane(lane);
  };
  const showTodoFromPanel = (t: Todo) => {
    if (view === 'canvas' && canvas.has('todo', t.id)) setCanvasFocus((f) => ({ id: refId('todo', t.id), n: (f?.n ?? 0) + 1 }));
    else showTodo(t);
  };

  /** The panels on the right, as each view shows them. */
  const todoPanel = (item: TodoItem) =>
    drilledPanel(item.key, middleTruncate(item.todo.title, 40)) ?? (
    <TodoPanel
      key={item.todo.id}
      item={item}
      todos={todoStore.todos}
      onShowTodo={showTodoFromPanel}
      labels={todoStore.labels}
      lanes={laneById}
      now={now}
      onRun={runTodo}
      onStart={() => startForTodo(item)}
      onShowSession={(_lane, session) => setDrill({ from: item.key, key: session.key })}
      onOpenSession={(lane, session) => open(lane, session.openTarget)}
      onShowLane={showLaneFromPanel}
      onLaneMenu={(lane) => laneMenu(lane)}
      onSessionMenu={(lane, s) => sessionMenu(lane, s, 'worktree')}
      onLinkMenu={(at) => todoLinkMenu(item, at)}
      onMoreMenu={(at) => todoMoreMenu(item, at)}
      onLabelMenu={labelMenu}
      onError={setToast}
      hint={board?.jev?.todos ? board.todoHints?.[item.todo.id] : null}
      onDismissHint={() => void dismissTodoHint(item.todo.id)}
      suggest={!!board?.jev?.todos}
      projects={todoProjects}
    />
  );
  /** Several todos picked together: one session for all of them. */
  const todoPickPanel = (rows: TodoItem[]) => (
    <TodoPickPanel
      items={rows}
      todos={todoStore.todos}
      showProject={current === ALL_PROJECTS}
      onStart={() => startForTodos(rows)}
      onShow={(r) => selectTodoRow(r, 'one')}
      onLeaveOut={(r) => selectTodoRow(r, 'toggle')}
      onMoreMenu={(at) => pickMenu(rows, at)}
    />
  );
  const prPanel = (item: PrRowItem) =>
    drilledPanel(item.key, `PR #${item.pr.number}`) ?? (
    <PrInspector
      key={item.pr.key}
      item={item}
      now={now}
      approving={approving.has(item.pr.key)}
      onApprove={() => approve(item.pr)}
      onReview={(el) => reviewMenuFrom(item.pr, el)}
      onShowSession={(_lane, session) => setDrill({ from: item.key, key: session.key })}
      onOpenSession={(lane, session) => open(lane, session.openTarget)}
      onShowLane={showLaneFromPanel}
      onLaneMenu={laneMenu}
      onSessionMenu={(lane, s) => sessionMenu(lane, s, 'worktree')}
    />
  );
  const inspector = (lane: Lane | null, focusKey: string | null) => (
    <Inspector
      lane={lane}
      focusKey={focusKey}
      apps={board?.apps ?? { terminal: 'Terminal', terminalId: '', editor: 'Editor', editorId: '' }}
      now={now}
      onOpen={(id) => open(lane, id)}
      onNewSession={() => lane && newSession({ laneId: lane.id })}
      todos={lane ? todoLinks.byLane.get(lane.id) : undefined}
      todosFor={todosFor}
      onShowTodo={showTodoFromPanel}
      onPrMenu={() => lane && lanePrMenu(lane)}
      onReviewPr={lane?.pr ? (el) => lane.pr && reviewMenuFrom(reviewedPr(lane, lane.pr), el) : undefined}
      onSetPreview={lane && !lane.device ? () => setPreviewFor(lane.id) : undefined}
    />
  );

  /** A session's own panel: its last prompt and response, how to open it, its details. Given its
   *  row in Sessions, it works through what waits on you from there. */
  const sessionPanel = (lane: Lane, session: Session, back?: { label: string; onBack: () => void }, row?: string) => {
    // In Sessions it works through what waits on you: what's left after it, and how the last went.
    const note = row !== undefined && queueNote?.key === row ? { text: queueNote.text, error: false } : null;
    const left = row !== undefined ? queueAfter(waitingRows, session.key, handled.current) : [];
    const queued = row !== undefined && (!!note || waitingRows.some((r) => r.key === row));
    return (
      <SessionPanel
        key={session.key}
        lane={lane}
        session={session}
        apps={board?.apps ?? { terminal: 'Terminal', terminalId: '', editor: 'Editor', editorId: '' }}
        now={now}
        onOpen={(id) => open(lane, id)}
        onNewSession={() => newSession({ laneId: lane.id })}
        onShowLane={showLaneFromPanel}
        todos={todoLinks.bySession.get(session.key)}
        onShowTodo={showTodoFromPanel}
        onMoreMenu={(at) => void showContextMenu(sessionMenuSpec(lane, session, 'session'), at).catch((err: unknown) => setToast(String(err)))}
        onSeen={() => seen(session)}
        device={deviceOf(devices, session.key)}
        back={back}
        onSendFailed={(why) => {
          // Not dealt with after all: it comes round again.
          handled.current.delete(session.key);
          setToast(`Didn’t send to ${session.title}, so it’s back in its box: ${why}`);
        }}
        queue={row !== undefined && queued ? { left: left.length, next: left[0]?.session ?? null, onNext: () => goOn(row, session, null) } : undefined}
        notice={note}
        onDone={row !== undefined ? (done) => goOn(row, session, done) : undefined}
        autoFocus={row !== undefined && boxFocus === row}
      />
    );
  };
  /** The session a PR's or todo's panel was asked to show, in its place, with the way back. */
  const drilledPanel = (from: string, label: string) => {
    if (drill?.from !== from) return null;
    for (const lane of lanes) {
      const session = lane.sessions.find((x) => x.key === drill.key);
      if (session) return sessionPanel(lane, session, { label, onBack: () => setDrill(null) });
    }
    return null;
  };

  /** The canvas's selected card as its view's panel shows it; null when nothing (or a gone card) is selected. */
  const canvasPanel = (() => {
    if (view !== 'canvas' || !canvasSel) return null;
    const e = canvasEntities;
    switch (canvasSel.ref) {
      case 'worktree': {
        const lane = e.lanes.get(canvasSel.key);
        return lane ? { kind: 'lane' as const, node: inspector(lane, null) } : null;
      }
      case 'session': {
        const p = e.sessions.get(canvasSel.key);
        return p ? { kind: 'session' as const, node: sessionPanel(p.lane, p.session) } : null;
      }
      case 'todo': {
        const item = todoItems.find((r) => r.todo.id === canvasSel.key);
        return item ? { kind: 'todo' as const, node: todoPanel(item) } : null;
      }
      case 'pr': {
        const pr = e.prs.get(canvasSel.key);
        return pr ? { kind: 'pr' as const, node: prPanel(prRowOf(pr)) } : null;
      }
    }
  })();

  /** A worktree's row on the board; `drag` when it's pinned. */
  const laneRow = (r: WorktreeRow, drag?: PinDrag) => (
    <LaneRow
      key={r.key}
      row={r}
      now={now}
      selectedKey={selected?.key ?? null}
      domId={domIdOf}
      onSelect={setSelectedKey}
      onOpen={(lane) => open(lane, lane.defaultTarget)}
      onOpenTarget={open}
      pinned={drag != null}
      onContextMenu={() => laneMenu(r.lane)}
      onAgentMenu={(s) => sessionMenu(r.lane, s, 'worktree')}
      onPrMenu={() => lanePrMenu(r.lane)}
      todos={todoLinks.byLane.get(r.lane.id)}
      worktreeTodos={todoLinks.byWorktree.get(r.lane.id)}
      todosFor={todosFor}
      onShowTodo={showTodo}
      {...drag}
    />
  );

  // Which panel is on the right, for its width: the view's own, or on the canvas the selected card's.
  const panelKind = view === 'canvas' ? (canvasPanel?.kind ?? null) : view === 'prs' ? 'pr' : view === 'todo' ? 'todo' : view === 'sessions' && selected?.kind === 'agent' ? 'session' : 'lane';
  const panelWidth = { pr: prPanelWidth, todo: todoPanelWidth, session: sessionPanelWidth, lane: inspectorWidth };
  const setPanelWidth = { pr: setPrPanelWidth, todo: setTodoPanelWidth, session: setSessionPanelWidth, lane: setInspectorWidth };
  // The worktree the preview URL sheet is for, as the latest board has it.
  const previewLane = previewFor ? board?.lanes.find((l) => l.id === previewFor) : undefined;

  return (
    <div className="ca-win ca-win--app">
      <div className="ca-win-bar" data-tauri-drag-region>
        <ProjectSwitcher logoStatus={logoStatus} projects={projects} current={current} total={totalLanes} actions={actions} onPick={pickProject} onOpenWindow={switcherWindow} onEditProjects={() => openSettings('projects')} />
        <label className="ca-search">
          <Icon name="search" size={14} />
          <input
            ref={searchRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={`Filter ${current === ALL_PROJECTS ? 'all projects' : current}`}
            aria-label="Filter worktrees, sessions and PRs"
            spellCheck={false}
          />
          {query ? (
            <button
              type="button"
              className="ca-search-clear"
              aria-label="Clear filter"
              title="Clear filter"
              onClick={() => {
                setQuery('');
                searchRef.current?.focus();
              }}
            >
              <Icon name="x" size={12} />
            </button>
          ) : (
            <Kbd keys={['⌘', 'K']} />
          )}
        </label>
        <div className="ca-bar-tools" data-tauri-drag-region>
          <div className="ca-segctl ca-barplace" role="group" aria-label="Agents bar">
            <button
              type="button"
              className={barPlace === 'floating' ? 'is-on' : undefined}
              aria-pressed={barPlace === 'floating'}
              aria-label="Floating bar"
              title={barPlace === 'floating' ? 'Hide the floating bar' : 'Floating bar: the agents that need you, on top of every window, where you put it'}
              onClick={() => pickBarPlace('floating')}
            >
              <Icon name="capsule-horizontal" size={15} />
            </button>
            <button
              type="button"
              className={barPlace === 'notch' ? 'is-on' : undefined}
              aria-pressed={barPlace === 'notch'}
              aria-label="Bar in the notch"
              title={barPlace === 'notch' ? 'Hide the bar in the notch' : 'In the notch: the agents that need you, around the notch at the top of the screen, opening when you point at it'}
              onClick={() => pickBarPlace('notch')}
            >
              <Icon name="notch" size={15} />
            </button>
          </div>
        </div>
        <Button
          size="sm"
          icon="plus"
          title={pickedRows.length > 0 ? `Start one session for the ${pickedRows.length} selected todos (⌘N)` : selected?.kind === 'todo' ? 'Start a session for the selected todo (⌘N)' : 'Start a session in the selected worktree (⌘N)'}
          onClick={() => (pickedRows.length > 0 ? startForTodos(pickedRows) : selected?.kind === 'todo' ? startForTodo(selected) : newSession({ laneId: startLaneId }))}
        >
          New session
        </Button>
      </div>
      <UsageBar
        usage={board?.usage ?? []}
        now={now}
        onRefresh={() => void refreshUsage()}
        onAccounts={(account) => openSettings('accounts', account ?? null)}
        onSignIn={startSignIn}
      />
      <div
        className={['ca-win-body', panelKind === 'pr' && 'is-prs', panelKind === 'todo' && 'is-todo', panelKind === 'session' && 'is-session', view === 'canvas' && 'is-canvas', view === 'canvas' && canvasPanel && 'has-panel'].filter(Boolean).join(' ')}
        style={{ '--sidebar-w': widthVar(sidebarWidth), '--panel-w': widthVar(panelKind ? panelWidth[panelKind] : null) } as CSSProperties}
      >
        <nav className="ca-side" aria-label="Sidebar">
          <div className="ca-label ca-side-label">Inbox</div>
          <NavItem glyph="needs" label="Needs you" count={counts.needs} tone="attn" active={bayFilter === 'needs'} onClick={() => toggleBay('needs')} />
          <NavItem glyph="working" label="Working" count={counts.working} active={bayFilter === 'working'} onClick={() => toggleBay('working')} />
          <NavItem glyph="unread" label="Your turn" count={counts.unread} active={bayFilter === 'unread'} onClick={() => toggleBay('unread')} />
          <NavItem icon="pr" label="In review" count={counts.review} active={bayFilter === 'review'} onClick={() => toggleBay('review')} />
          <NavItem glyph="error" label="PR needs work" count={counts.work} tone="alert" active={bayFilter === 'work'} title="Open PRs with failing checks or changes requested" onClick={() => toggleBay('work')} />
          {devices?.enabled && devices.devices.length > 0 && (
            <>
              <div className="ca-label ca-side-label">Devices</div>
              <NavItem icon="device-desktop" label={devices.me.name || 'This device'} count={deviceNeeds.get(null) || undefined} tone="attn" active={deviceFilter === 'here'} onClick={() => setDeviceFilter(deviceFilter === 'here' ? null : 'here')} />
              {devices.devices.map((d) => (
                <NavItem
                  key={d.key}
                  lead={<i className={`ca-labeldot ca-side-devdot is-${d.color}`} aria-hidden="true" />}
                  label={d.online ? d.name : `${d.name} · away`}
                  className={d.online ? undefined : 'is-away'}
                  title={d.online ? `Connected ${d.via === 'iroh' ? 'through iroh' : 'on your tailnet'}` : d.lastSeen ? `Away: last seen ${agoPhrase(d.lastSeen, now)}` : 'Away'}
                  count={deviceNeeds.get(d.short) || undefined}
                  tone="attn"
                  active={deviceFilter === d.short}
                  onClick={() => setDeviceFilter(deviceFilter === d.short ? null : d.short)}
                />
              ))}
            </>
          )}
          <div className="ca-label ca-side-label">Worktrees · {recentLanes.length}</div>
          <div className="ca-side-scroll">
            <WorktreeTree
              groups={worktrees}
              now={now}
              selectedLaneId={view === 'canvas' ? canvasLane : (selectedLane?.id ?? null)}
              selectedSessionKey={view === 'canvas' ? (canvasSel?.ref === 'session' ? canvasSel.key : null) : selected?.kind === 'agent' ? selected.session.key : null}
              onLaneMenu={laneMenu}
              onSessionMenu={(lane, s) => sessionMenu(lane, s, 'worktree')}
              onSelectLane={focusLane}
              onOpenLane={(lane) => open(lane, lane.defaultTarget)}
              onShowSession={showSession}
              onOpenSession={(lane, session) => open(lane, session.openTarget)}
            />
          </div>
          <RestartToUpdate />
          <div className="ca-side-foot">
            <button type="button" className="ca-linkbtn ca-side-config" title="Settings (⌘,)" onClick={() => openSettings()}>
              <Icon name="settings" size={14} />
              <span>Settings</span>
            </button>
            <ThemeSwitch theme={theme} onPick={pickTheme} />
          </div>
          {board && board.warnings.length > 0 && (
            <div className="ca-side-warnings" role="status">
              {board.warnings.map((w) =>
                w.includes('config.toml') ? (
                  <button key={w} type="button" className="ca-warning is-link" title="Open config.toml in Settings" onClick={() => openSettings('file')}>
                    <Icon name="alert" size={14} />
                    <span>{w}</span>
                  </button>
                ) : (
                  <div key={w} className="ca-warning">
                    <Icon name="alert" size={14} />
                    <span>{w}</span>
                  </div>
                ),
              )}
            </div>
          )}
        </nav>
        <Resizer panel="before" cssVar="--sidebar-w" onResize={setSidebarWidth} />
        <main className={view === 'canvas' ? 'ca-board is-canvas' : 'ca-board'} aria-label={VIEWS.find((v) => v.id === view)?.label}>
          {devices?.asks.map((a) => (
            <div key={a.key} className="ca-device-banner" role="alert">
              <Icon name="device-desktop" size={14} />
              <span>
                <b>{a.name}</b> wants to link with this device. Link only if it shows <span className="ca-device-code">{a.code}</span>.
              </span>
              <Button size="sm" variant="primary" onClick={() => void answerLink(a.key, true).catch((e: unknown) => setToast(String(e)))}>
                Link
              </Button>
              <Button size="sm" onClick={() => void answerLink(a.key, false).catch((e: unknown) => setToast(String(e)))}>
                Not now
              </Button>
            </div>
          ))}
          <div className="ca-board-head">
            <div className="ca-segctl" role="tablist" aria-label="View">
              {VIEWS.map((v) => (
                <button
                  key={v.id}
                  type="button"
                  role="tab"
                  aria-selected={view === v.id}
                  className={view === v.id ? 'is-on' : undefined}
                  title={v.id === 'prs' && requestedCount > 0 ? `${requestedCount} PRs want your review. Switch views (V)` : 'Switch views (V)'}
                  onClick={() => setView(v.id)}
                >
                  {v.label}
                  <span className="ca-segctl-count">{v.id === 'worktrees' ? recentLanes.length : v.id === 'sessions' ? shownSessionCount : v.id === 'todo' ? openTodoCount : v.id === 'canvas' ? (canvas.doc?.items.filter((it) => it.kind === 'ref').length ?? '') : openPrCount}</span>
                  {v.id === 'prs' && requestedCount > 0 && <span className="ca-segctl-badge">{requestedCount}</span>}
                </button>
              ))}
            </div>
            {view === 'todo' ? (
              <div className="ca-agefilter ca-todofilter">
                {labelFilter && labelById.get(labelFilter) && (
                  <button type="button" className="ca-labelchip ca-labelchip--button ca-labelfilter" title="Show every label" onClick={() => setLabelFilter(null)}>
                    <LabelDot label={labelById.get(labelFilter) as Label} />
                    <span>{labelById.get(labelFilter)?.name}</span>
                    <Icon name="x" size={10} stroke={2.25} />
                  </button>
                )}
              </div>
            ) : view === 'canvas' ? null : (
              <>
              {view === 'worktrees' && devices?.enabled && devices.lanes.length > 0 && deviceFilter === null && (
                <div className="ca-segctl ca-groupby">
                  <button type="button" aria-pressed={groupByDevice} className={groupByDevice ? 'is-on' : undefined} title="Group worktrees by the device they're on" onClick={toggleGroupByDevice}>
                    By device
                  </button>
                </div>
              )}
              <div className="ca-segctl ca-agefilter" role="radiogroup" aria-label="Active in">
                {AGES.map((a) => (
                  <button key={a.id} type="button" role="radio" aria-checked={age === a.id} className={age === a.id ? 'is-on' : undefined} title={a.title} onClick={() => setAge(a.id)}>
                    {a.label}
                  </button>
                ))}
              </div>
              </>
            )}
          </div>
          {view !== 'prs' && view !== 'todo' && view !== 'canvas' && (
            <div className={`ca-board-cols ca-label${view === 'sessions' ? ' is-sessions' : ''}`} aria-hidden="true">
              <span />
              {view === 'sessions' ? <span>Session</span> : <><span>Worktree</span><span>Now</span></>}
              <span className="ca-board-cols-age">Age</span>
            </div>
          )}
          {board && !board.loading && view === 'prs' && prRows.length === 0 && (
            <div className="ca-board-empty">
              {query ? `No PRs in ${current} match “${query}”.` : board.githubAt ? 'No PRs here right now.' : 'Loading PRs from GitHub…'}
            </div>
          )}
          {loadError && <div className="ca-board-empty">{loadError}</div>}
          {!loadError && (!board || (board.loading && lanes.length === 0)) && <div className="ca-board-empty">Reading sessions…</div>}
          {board && !board.loading && view === 'sessions' && sessionRows.length === 0 && (
            <div className="ca-board-empty">
              {query
                ? `No sessions in ${current} match “${query}”.`
                : bayFilter
                  ? 'No sessions here right now.'
                  : cutoff > 0
                    ? `No sessions ${AGES.find((a) => a.id === age)?.title.toLowerCase()}.`
                    : 'No sessions in the last 14 days.'}
            </div>
          )}
          {board && !board.loading && view === 'worktrees' && bays.length === 0 && pinnedLaneRows.length === 0 && (
            <div className="ca-board-empty">
              {query
                ? `Nothing in ${current} matches “${query}”.`
                : bayFilter === 'work'
                  ? 'No PRs need work: no failing checks or changes requested.'
                  : bayFilter
                    ? 'Nothing here right now.'
                  : cutoff > 0
                    ? `No worktrees ${AGES.find((a) => a.id === age)?.title.toLowerCase()}.`
                    : 'No worktrees yet. Codealong looks for repos under the folders in config.toml and for the agents working in them.'}
            </div>
          )}
          {view === 'canvas' && board && canvas.doc && (
            <CanvasView
              doc={canvas.doc}
              update={canvas.update}
              entities={canvasEntities}
              pinned={lanePins.set}
              labels={labelById}
              todos={todoStore.todos}
              now={now}
              terminal={board.apps.terminal}
              theme={theme}
              project={current}
              focus={canvasFocus}
              onOpen={openCard}
              menuFor={cardMenu}
              onToast={setToast}
              onUndo={(message, undo, icon) => setTodoUndo({ message, undo, icon })}
              keys={canvasKeys}
              onSelectItem={setCanvasSel}
              query={query}
              filtering={!!query.trim() || bayFilter != null}
              matches={canvasMatches}
              fitMatches={canvasFit}
            />
          )}
          {view !== 'canvas' && (
          <div
            role="listbox"
            aria-label={view === 'sessions' ? 'Sessions' : view === 'prs' ? 'Pull requests' : view === 'todo' ? 'Todos' : 'Agents and worktrees'}
            aria-activedescendant={selected ? domId(selected) : undefined}
            aria-multiselectable={view === 'todo' || undefined}
            tabIndex={0}
            ref={listRef}
            className="ca-board-list"
          >
            {view === 'todo' && board && (
              <TodoView
                sections={todoSecs}
                todos={todoStore.todos}
                now={now}
                selectedKey={selected?.key ?? null}
                picked={pickedKeys}
                domId={domId}
                showProject={current === ALL_PROJECTS}
                canReorder
                onSelect={selectTodoRow}
                onOpen={(r) => {
                  selectTodoRow(r, 'one');
                  openRow(r);
                }}
                onContextMenu={(r) => {
                  // On one of the todos picked, the menu is for all of them.
                  if (pickedKeys.has(r.key)) {
                    pickMenu(pickedRows);
                    return;
                  }
                  selectTodoRow(r, 'one');
                  todoMenu(r);
                }}
                onOpenSession={(lane, session) => open(lane, session.openTarget)}
                onFilterLabel={(l) => setLabelFilter(labelFilter === l.id ? null : l.id)}
                onMove={moveTodo}
                hints={board.jev?.todos ? board.todoHints : undefined}
                showDone={showDoneTodos}
                onToggleDone={() => setShowDoneTodos(!showDoneTodos)}
                composer={
                  <TodoComposer
                    ref={composerRef}
                    value={composer.text}
                    onChange={(text) => setComposer((c) => ({ ...c, text }))}
                    context={composer.context}
                    onClearContext={() => setComposer((c) => ({ ...c, context: null }))}
                    onCreate={(title) => createTodo(title, composer.context)}
                    where={current === ALL_PROJECTS ? (composer.context?.worktree ? (laneById.get(composer.context.worktree)?.project ?? null) : 'no project') : null}
                  />
                }
                empty={
                  projectTodos.length === 0
                    ? 'No todos yet. Add work you mean to hand an agent: a title, labels, screenshots. When it’s time, start a session from it, and the session is linked to it.'
                    : query
                      ? `No todos in ${current} match “${query}”.`
                      : 'No todos here right now.'
                }
              />
            )}
            {view === 'prs' && (
              <PrsView
                sections={prSecs}
                now={now}
                selectedKey={selected?.key ?? null}
                domId={domId}
                showDone={showDonePrs}
                onToggleDone={() => setShowDonePrs(!showDonePrs)}
                approving={approving}
                onSelect={(r) => setSelectedKey(r.key)}
                onOpen={(r) => {
                  setSelectedKey(r.key);
                  openRow(r);
                }}
                onApprove={(r) => approve(r.pr)}
                onOpenSession={(lane, session) => open(lane, session.openTarget)}
                onContextMenu={(r) => prMenu(r)}
              />
            )}
            {view === 'sessions' && (
              <SessionsView
                pinned={pinnedRows}
                sections={sessionSections}
                now={now}
                selectedKey={selected?.key ?? null}
                domId={domId}
                onSelect={(r, e) => {
                  setSelectedKey(r.key);
                  if (e.button === 0) focusBox();
                }}
                onOpen={(r) => {
                  setSelectedKey(r.key);
                  openRow(r);
                }}
                onContextMenu={(r) => sessionMenu(r.lane, r.session, 'session')}
                onPrMenu={(r) => lanePrMenu(r.lane)}
                onMovePinned={movePinnedSession}
                archived={archivedItems}
                showArchived={showArchived}
                onToggleArchived={() => setShowArchived(!showArchived)}
                todosFor={todosFor}
                onShowTodo={showTodo}
              />
            )}
            {view === 'worktrees' && pinnedLaneRows.length > 0 && (
              <PinnedSection items={pinnedLaneRows} keyOf={(r) => r.lane.id} onMove={movePinnedLane} row={laneRow} />
            )}
            {view === 'worktrees' &&
              shownGroups.map((g) => {
                const groupParked = g.bays.flatMap((b) => b.rows).filter((r) => r.lane.parked).length;
                const other = g.short ? devices?.devices.find((d) => d.short === g.short) : undefined;
                return (
                  <Fragment key={g.short ?? 'here'}>
                    {g.short !== undefined && (
                      <div className="ca-devgroup-head">
                        {other ? (
                          <DeviceBadge device={other} now={now} heading />
                        ) : (
                          <span className="ca-devgroup-here">
                            <Icon name="device-desktop" size={13} />
                            {devices?.me.name || 'This device'}
                          </span>
                        )}
                      </div>
                    )}
                    {g.shown.map((b) => {
                      const count = g.bays.find((x) => x.bay === b.bay)?.rows.length ?? b.rows.length;
                      const unreadKeys = b.bay === 'unread' ? b.rows.flatMap((r) => r.agents.filter((a) => a.session.status === 'unread').map((a) => a.session.key)) : [];
                      return (
                        <section key={b.bay} className="ca-bayblock">
                          <BayHeader
                            bay={b.bay}
                            count={count}
                            action={
                              unreadKeys.length > 0 ? (
                                <button type="button" className="ca-linkbtn" onClick={() => markSeen(unreadKeys)}>
                                  Mark all seen
                                </button>
                              ) : undefined
                            }
                          />
                          <div className="ca-bay-lanes">
                            {b.rows.map((r) => laneRow(r))}
                            {b.bay === 'idle' && foldParked && groupParked > 0 && (
                              <button type="button" className="ca-parked" onClick={() => setShowParked(true)}>
                                Show {groupParked} parked {groupParked === 1 ? 'worktree' : 'worktrees'}
                              </button>
                            )}
                          </div>
                        </section>
                      );
                    })}
                  </Fragment>
                );
              })}
          </div>
          )}
        </main>
        {panelKind && <Resizer panel="after" cssVar="--panel-w" onResize={setPanelWidth[panelKind]} />}
        {view === 'canvas' ? (
          canvasPanel?.node ?? null
        ) : view === 'todo' ? (
          pickedRows.length > 0 ? (
            todoPickPanel(pickedRows)
          ) : selected?.kind === 'todo' ? (
            todoPanel(selected)
          ) : (
            <aside className="ca-insp ca-insp--empty">
              <p>{projectTodos.length ? 'Select a todo to see its description, labels and the sessions on it.' : 'Todos are work you mean to hand an agent. Add one on the left.'}</p>
            </aside>
          )
        ) : view === 'prs' ? (
          selected?.kind === 'pr' ? (
            prPanel(selected)
          ) : (
            <aside className="ca-insp ca-insp--empty">
              <p>Select a PR to see its description, reviews and changes.</p>
            </aside>
          )
        ) : view === 'sessions' && selected?.kind === 'agent' ? (
          sessionPanel(selected.lane, selected.session, undefined, selected.key)
        ) : (
          inspector(selectedLane, selected?.kind === 'agent' ? selected.session.key : null)
        )}
      </div>
      {settings && (
        <SettingsDialog
          section={settings.section}
          onSection={(section) => setSettings({ section, focus: null })}
          focus={settings.focus}
          usage={board?.usage ?? []}
          apps={board?.apps ?? { terminal: 'Terminal', editor: 'Editor' }}
          theme={theme}
          onPickTheme={pickTheme}
          now={now}
          onClose={closeSettings}
          onOpenConfig={editConfig}
        />
      )}
      {closeIdle && <CloseIdleDialog onClose={() => setCloseIdle(false)} onDone={setToast} />}
      {previewLane && <PreviewDialog lane={previewLane} onClose={() => setPreviewFor(null)} />}
      {busy.length > 0 && !toast && (
        <div className="ca-toast is-busy" role="status" aria-live="polite">
          <StatusGlyph status="working" size={14} title="In progress" />
          <span>{busy[busy.length - 1].label}</span>
        </div>
      )}
      {undo && !toast && busy.length === 0 && (
        <div className="ca-toast" role="status">
          <Icon name="archive" size={16} />
          <span>Archived “{undo.title}”.</span>
          <button
            type="button"
            className="ca-toast-action"
            onClick={() => {
              setArchived(undo.key, false, undo.title);
              setUndo(null);
            }}
          >
            Undo
          </button>
        </div>
      )}
      {todoUndo && !toast && !undo && busy.length === 0 && (
        <div className="ca-toast" role="status">
          <Icon name={todoUndo.icon ?? 'trash'} size={16} />
          <span>{todoUndo.message}</span>
          <button
            type="button"
            className="ca-toast-action"
            onClick={() => {
              todoUndo.undo();
              setTodoUndo(null);
            }}
          >
            Undo
          </button>
        </div>
      )}
      {toast && (
        <div className="ca-toast" role="status" onClick={() => setToast(null)}>
          <Icon name="alert" size={16} />
          <span>{toast}</span>
        </div>
      )}
    </div>
  );
}

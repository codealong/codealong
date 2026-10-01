// The main window. One project at a time, picked in the switcher at top left. The sidebar
// picks the view and narrows it: device, how recently active, status, and views you saved
// (Sidebar.tsx); it collapses to a rail. The main area has five views: Sessions lists every session,
// pinned ones first in your order; Worktrees lists pinned worktrees in your order, then every
// other worktree once, in Needs you, Working, Your turn, In review, Idle or Merged, with a line
// for each agent that needs you, is working or just finished; PRs lists the project's pull
// requests; Todo lists work you mean to hand an agent, and the sessions on it;
// Canvas lays the project's work out in space, to be remembered by where it is (CanvasView.tsx).
// Keyboard map: the design system's Surfaces page.

import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { below, showContextMenu, type MenuAction, type MenuSpec } from './contextMenu';
import { useEditorIcons } from './editors';
import { AGENT_BAY, buildBays, filterByAge, homeOf, inboxCounts, liveOrLatest, narrowed, prNeedsWork, rowItems, seenable, sessionPrs, worktreeRow, sessionHomes, type BoardRow, type Filter, type WorktreeRow } from './lanes';
import { answerLink, approvePr, barMode, getDevices, onDevices, dismissTodoHint, expandAttachments, getBoard, onBarMode, onBoard, onCloseIdle, onNewWindow, onQuickStart, onReveal, onSettings, onCheckUpdates, openConfig, openProjectWindow, openUrl, refresh, refreshUsage, reviewInCodex, setBarMode, startOptions, startSession, setPinned, setPinnedPrs, setPinnedWorktrees, setTheme, showQuick, signIn, windowLabel, type BarMode, type Reveal } from './api';
import { dueChoices } from './due';
import { middleTruncate } from './format';
import * as menus from './menus';
import { prRefOf } from './prLinks';
import { usePerf } from './perf';
import { archiveAhead, boardAhead, boardView, lanesAhead, observe, seenAhead, useAhead } from './ahead';
import { reuse } from './reuse';
import { reorderPinned, usePins } from './pins';
import { reviewLane, reviewPicks, reviewPrompt } from './review';
import { remembered } from './start';
import { blockedBy, COLOR_WORD, depsOf, newTodo, pickTo, pickToggle, sectionOf, snippet, STATUSES, STATUS_WORD, todoIndex, todoInFilter, todoInQuery, todoItem, todoPrompt, todosPrompt, todoSections, useTodoStore, type TodoItem, type TodoPick, type TodoSectionId } from './todos';
import type { Bay, Board, DevicesView, Label, Lane, Pr, PrItem, Session, StartOptions, Theme, Todo, TodoStatus } from './types';
import { byDevice, deviceOf, needsOn, shortOf, withDevices, workingOn, type DeviceFilter } from './devices';
import { queueAfter, statusTime, type Handled } from './quick';
import { DeviceBadge } from './ui/DeviceBadge';
import { LABEL_COLORS } from './types';
import { SettingsDialog, type SettingsSection } from './ui/SettingsDialog';
import { CloseIdleDialog } from './ui/CloseIdleDialog';
import { PreviewDialog } from './ui/PreviewBits';
import { RenameDialog } from './ui/RenameDialog';
import { Inspector } from './ui/Inspector';
import { SessionPanel } from './ui/SessionPanel';
import { lineageOf } from './lineage';
import { LaneRow } from './ui/LaneRow';
import { PinnedSection } from './ui/PinnedSection';
import { useRowDrag, type DragProps } from './ui/useRowDrag';
import { BayHeader, Button, Icon, Kbd, StatusGlyph } from './ui/primitives';
import { ALL_PROJECTS, openSwitcher, projectForDigit, ProjectSwitcher, type ActionCounts } from './ui/ProjectSwitcher';
import { useLogo } from './logo';
import { PrInspector } from './ui/PrInspector';
import { usePrLinks } from './ui/PrLink';
import { prIsOpen, prPlaces, prSections, PrsView, type PrRowItem } from './ui/PrsView';
import { Resizer, usePanelWidth } from './ui/Resizer';
import { SESSION_ROWS, SESSION_SORTS, SessionsView, sortSessions, waitingQueue, type SessionItem, type SessionRows, type SessionSort } from './ui/SessionsView';
import { TodoPanel, TodoPickPanel } from './ui/TodoPanel';
import { TodoComposer, TodoView, type ComposerContext, type SelectHow } from './ui/TodoView';
import { CanvasView } from './ui/CanvasView';
import { entitiesOf, homeLane, refId, type RefItem, type RefKind } from './canvas/model';
import { useCanvas } from './canvas/useCanvas';
import { ThemeSwitch, useTheme } from './ui/ThemeSwitch';
import { UsageBar } from './ui/UsageBar';
import { Sidebar, type SideStatus } from './ui/Sidebar';
import { loadSaved, newSavedId, onSavedChange, sameView, storeSaved, suggestName, type SavedView, type ViewState } from './savedViews';
import { useNow } from './ui/useNow';
import { errorText, type Report } from './report';
import { useSessionHistory } from './history';
import { openWhere, where } from './open';

const ELSEWHERE = 'Elsewhere';
/** The chords Sessions' box hands the list: ⌥↑ ⌥↓ (with ⇧, to what waits on you), ⌃⌘ with a
 *  menu's letter or ↑ ↓, ⇧⌘F and ⌃Tab. A text field has no use of its own for any of them but
 *  ⌥↑ ⌥↓, which jump to a paragraph's start or end. */
function listChord(e: KeyboardEvent): boolean {
  const vertical = e.key === 'ArrowUp' || e.key === 'ArrowDown';
  if (e.altKey && !e.metaKey && !e.ctrlKey) return vertical;
  if (e.ctrlKey && e.metaKey && !e.altKey && !e.shiftKey) return vertical || Object.values(menus.KEYS).includes(e.key.toUpperCase());
  if (e.shiftKey && e.metaKey && !e.ctrlKey && !e.altKey) return e.key.toLowerCase() === 'f';
  return e.ctrlKey && !e.metaKey && !e.altKey && e.key === 'Tab';
}
/** ⌃- and ⌃⇧-, back and forward through the sessions you looked at, as VS Code has them. The key
 *  goes by what it types or where it is, so a layout that puts - elsewhere has it too. */
function historyChord(e: KeyboardEvent): boolean {
  return e.ctrlKey && !e.metaKey && !e.altKey && (e.key === '-' || e.key === '_' || e.code === 'Minus');
}
/** No worktrees yet, as one list, so Settings doesn't see a new one on every render. */
const NO_LANES: Lane[] = [];

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
const SESSION_SORT_KEY = 'codealong.sessionSort';
const SESSION_ROWS_KEY = 'codealong.sessionRows';
const SIDEBAR_WIDTH_KEY = 'codealong.sidebarWidth';
const SIDEBAR_COLLAPSED_KEY = 'codealong.sidebarCollapsed';
const INSPECTOR_WIDTH_KEY = 'codealong.inspectorWidth';
const PR_PANEL_WIDTH_KEY = 'codealong.prPanelWidth';
const TODO_PANEL_WIDTH_KEY = 'codealong.todoPanelWidth';
const SESSION_PANEL_WIDTH_KEY = 'codealong.sessionPanelWidth';
const SESSION_PANEL_MAX_KEY = 'codealong.sessionPanelMax';
// Each window has its own way back.
const HISTORY_KEY = WINDOW_PROJECT ? `codealong.sessionHistory:${WINDOW_PROJECT}` : 'codealong.sessionHistory';
const DAY = 24 * 60 * 60 * 1000;

type View = 'worktrees' | 'sessions' | 'prs' | 'todo' | 'canvas';
const VIEWS: { id: View; label: string; icon: string }[] = [
  { id: 'sessions', label: 'Sessions', icon: 'terminal' },
  { id: 'worktrees', label: 'Worktrees', icon: 'branch' },
  { id: 'prs', label: 'PRs', icon: 'pr' },
  { id: 'todo', label: 'Todo', icon: 'list-check' },
  { id: 'canvas', label: 'Canvas', icon: 'layout-grid' },
];
/** What each view says until the engine has read this Mac's board once. */
const READING: Record<View, string> = {
  sessions: 'Reading sessions…',
  worktrees: 'Reading worktrees…',
  prs: 'Reading repos…',
  todo: 'Reading todos…',
  canvas: 'Reading sessions…',
};
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

/** The status filters, applied to one session: any of them. */
function sessionInFilter(r: SessionItem, fs: readonly Filter[]): boolean {
  return fs.length === 0 || fs.some((f) => sessionIn(r, f));
}

function sessionIn(r: SessionItem, f: Filter): boolean {
  const pr = r.lane.pr;
  switch (f) {
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

/** The status filters, applied to a PR and the sessions that worked on it: any of them. */
function prInFilter(r: PrRowItem, fs: readonly Filter[]): boolean {
  return fs.length === 0 || fs.some((f) => prIn(r, f));
}

function prIn(r: PrRowItem, f: Filter): boolean {
  const p = r.pr;
  switch (f) {
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

function storedSessionSort(): SessionSort {
  try {
    const v = window.localStorage.getItem(SESSION_SORT_KEY);
    return SESSION_SORTS.some((o) => o.id === v) ? (v as SessionSort) : 'status';
  } catch {
    return 'status';
  }
}

function storedSessionRows(): SessionRows {
  try {
    const v = window.localStorage.getItem(SESSION_ROWS_KEY);
    return SESSION_ROWS.some((o) => o.id === v) ? (v as SessionRows) : 'two';
  } catch {
    return 'two';
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
  const [deviceFilter, setDeviceFilter] = useState<DeviceFilter>([]);
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
  // Until the engine's first scan has finished, the board has none of this Mac's sessions,
  // worktrees, todos or PRs: the views say they're being read rather than show nothing, or only
  // the other devices' worktrees, which come in first from what their feeds kept.
  const loading = !loadError && (!engineBoard || engineBoard.loading);
  const [project, setProject] = useState<string | null>(() => WINDOW_PROJECT ?? storedProject());
  const [bayFilter, setBayFilter] = useState<readonly Filter[]>([]);
  const [query, setQuery] = useState('');
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [view, setViewState] = useState<View>(storedView);
  const [age, setAgeState] = useState<Age>(storedAge);
  // Panel widths as you last dragged them. The PR panel has its own: it starts out wider.
  const [sidebarWidth, setSidebarWidth] = usePanelWidth(SIDEBAR_WIDTH_KEY);
  // The sidebar folds to a rail of the views and devices (⌃⌘S), and stays as you left it.
  const [sideCollapsed, setSideCollapsed] = useState(() => {
    try {
      return window.localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === '1';
    } catch {
      return false;
    }
  });
  const toggleSidebar = useCallback(() => {
    setSideCollapsed((c) => {
      try {
        window.localStorage.setItem(SIDEBAR_COLLAPSED_KEY, c ? '0' : '1');
      } catch {
        // It still folds for this window; it just isn't remembered.
      }
      return !c;
    });
  }, []);
  // A session's panel in Sessions can take the whole window (⇧⌘F), for working through sessions one
  // after another with the list out of the way; it stays as you left it.
  const [panelMax, setPanelMax] = useState(() => {
    try {
      return window.localStorage.getItem(SESSION_PANEL_MAX_KEY) === '1';
    } catch {
      return false;
    }
  });
  const togglePanelMax = useCallback(() => {
    setPanelMax((m) => {
      try {
        window.localStorage.setItem(SESSION_PANEL_MAX_KEY, m ? '0' : '1');
      } catch {
        // It still expands for this window; it just isn't remembered.
      }
      return !m;
    });
  }, []);
  const [savedViews, setSavedViews] = useState<SavedView[]>(loadSaved);
  useEffect(() => onSavedChange(setSavedViews), []);
  const keepSaved = (next: SavedView[]) => {
    setSavedViews(next);
    storeSaved(next);
  };
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
  // The session being renamed (ui/RenameDialog.tsx), by key.
  const [renaming, setRenaming] = useState<string | null>(null);
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
    seenAhead(keys).catch((e: unknown) => setToast(errorText(e)));
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
  /** How this window's menus say how things went (menus.ts): a toast, and the spinner while
   *  something takes a moment. */
  const report = useMemo<Report>(() => ({ error: setToast, info: setToast, busy: track }), [track]);
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
  useTheme(theme);
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
  // The board lists no projects until it's read, so the one you left it on holds until then.
  const known = project === ALL_PROJECTS || projects.some((p) => p.name === project) || (loading && !!project);
  const current = known && project ? project : (projects.find((p) => p.name !== ELSEWHERE)?.name ?? ALL_PROJECTS);
  projectNow.current = current;

  const pickProject = (name: string) => {
    setProject(name);
    setBayFilter([]);
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
  const prPins = usePins(board?.pinnedPrs, setPinnedPrs, setToast);
  // How Sessions orders the sessions that aren't pinned: by status, or the newest start first.
  const [sessionSort, setSessionSortState] = useState<SessionSort>(storedSessionSort);
  const setSessionSort = useCallback((sort: SessionSort) => {
    setSessionSortState(sort);
    try {
      window.localStorage.setItem(SESSION_SORT_KEY, sort);
    } catch {
      // Storage can be unavailable; the order just won't persist.
    }
  }, []);
  // How much each session's row says: two lines, or one.
  const [sessionLines, setSessionLinesState] = useState<SessionRows>(storedSessionRows);
  const setSessionLines = useCallback((rows: SessionRows) => {
    setSessionLinesState(rows);
    try {
      window.localStorage.setItem(SESSION_ROWS_KEY, rows);
    } catch {
      // Storage can be unavailable; the rows just won't stay that way.
    }
  }, []);

  // Archived sessions leave every view but the Archived list.
  const rawLanes = useMemo(() => withDevices(board?.lanes ?? [], deviceLanes, deviceFilter), [board?.lanes, deviceLanes, deviceFilter]);
  // What needs you on each device, for the sidebar, whichever device the views show.
  const deviceNeeds = useMemo(() => {
    const all = withDevices(board?.lanes ?? [], deviceLanes, []);
    return new Map<string | null, number>([null, ...(devices?.devices ?? []).map((d) => d.short)].map((m) => [m, needsOn(all, m)]));
  }, [board?.lanes, deviceLanes, devices?.devices]);
  const deviceWorking = useMemo(() => {
    const all = withDevices(board?.lanes ?? [], deviceLanes, []);
    return new Map<string | null, number>([null, ...(devices?.devices ?? []).map((d) => d.short)].map((m) => [m, workingOn(all, m)]));
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
  const sessionList = useMemo(() => sortSessions(shownSessions.filter((r) => !sessionPins.set.has(r.session.key)), sessionSort, now), [shownSessions, sessionPins.set, sessionSort, now]);
  // Pinned worktrees that match the search and the status filter, in your order. Each shows at the
  // top of the board and leaves the bays below.
  const pinnedLaneRows = useMemo(() => {
    const byId = new Map(recentLanes.map((l) => [l.id, l]));
    return lanePins.keys.flatMap((id): WorktreeRow[] => {
      const lane = byId.get(id);
      const shown = lane && inQuery(lane, query) ? narrowed(lane, bayFilter, since) : null;
      return shown ? [worktreeRow(shown)] : [];
    });
  }, [recentLanes, lanePins.keys, query, bayFilter, since]);
  // Every other worktree once, in the bay its status puts it in. The status filter narrows each to
  // what it's about, which can move it to that bay.
  const bayRows = useCallback((ls: Lane[]) => buildBays(ls.flatMap((l) => (inQuery(l, query) && !lanePins.set.has(l.id) ? (narrowed(l, bayFilter, since) ?? []) : []))), [query, lanePins.set, bayFilter, since]);
  const bays = useMemo(() => bayRows(recentLanes), [bayRows, recentLanes]);
  // Grouped by device, each device's worktrees get bays of their own: this device's first.
  const grouped = groupByDevice && deviceFilter.length !== 1 && (devices?.enabled ?? false) && (devices?.lanes.length ?? 0) > 0;
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
        .filter((p) => (p.updatedAt >= cutoff || prPins.set.has(p.key)) && prInQuery(p, query))
        .map((p): PrRowItem => {
          const holding = p.lanes.flatMap((id) => laneById.get(id) ?? []);
          const sessions = p.sessions.flatMap((s) => sessionIndex.get(`${s.lane}::${s.key}`) ?? []);
          return { kind: 'pr', key: `pr:${p.key}`, pr: p, lane: holding[0] ?? null, sessions, places: prPlaces(holding, sessions) };
        })
        .filter((r) => prInFilter(r, bayFilter)),
    [projectPrs, cutoff, query, bayFilter, laneById, sessionIndex, prPins.set],
  );
  // Pinned PRs lead the PRs view in your order, whatever their age, and leave its sections.
  const pinnedPrRows = useMemo(() => {
    const byKey = new Map(prRows.map((r) => [r.pr.key, r]));
    return prPins.keys.flatMap((k) => byKey.get(k) ?? []);
  }, [prRows, prPins.keys]);
  const prSecs = useMemo(() => prSections(prRows.filter((r) => !prPins.set.has(r.pr.key))), [prRows, prPins.set]);
  const prVisible = useMemo<Row[]>(() => [...pinnedPrRows, ...prSecs.flatMap((s) => (s.id === 'done' && !showDonePrs ? [] : s.rows))], [pinnedPrRows, prSecs, showDonePrs]);
  const openPrCount = useMemo(() => projectPrs.filter((p) => prIsOpen(p) && p.updatedAt >= cutoff).length, [projectPrs, cutoff]);
  const requestedCount = useMemo(() => projectPrs.filter((p) => prIsOpen(p) && p.reviewRequested).length, [projectPrs]);

  // The Todo view: the project's todos as the engine has them, with this window's changes on top
  // until the engine's board shows them.
  const todoStore = useTodoStore(board, setToast);
  const runTodo = todoStore.run;
  const [showDoneTodos, setShowDoneTodos] = useState(false);
  // The todo whose panel should open its due date's picker, for Pick a date… and D.
  const [duePick, setDuePick] = useState<string | null>(null);
  const duePicked = useCallback(() => setDuePick(null), []);
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
  const canvas = useCanvas(current, view === 'canvas', !loading, canvasEntities, now, lanePins.set);
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
  // What's pinned to the desktop, so menus offer to take it off (desktop.rs).
  const onDesktop = menus.useDesktopPins();
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
    () => [...pinnedRows, ...sessionList, ...(showArchived ? archivedItems : [])],
    [pinnedRows, sessionList, showArchived, archivedItems],
  );
  // Nothing is listed until the board is read, so nothing can be selected: not the other devices'
  // rows that come first.
  const visible = loading ? [] : view === 'sessions' ? sessionRows : view === 'prs' ? prVisible : view === 'todo' ? todoVisible : view === 'canvas' ? [] : worktreeItems;
  // A session listed under another worktree now (its commands moved there) stays selected. When
  // the selected agent's line goes (you marked it seen, it stopped), the selection stays
  // on its worktree rather than jumping to the top. A todo that leaves the list (marked done
  // while Done is folded, or no longer matching the search or a filter) stays in the panel: the
  // next key you press is still about the todo you see, and an edit in progress isn't cut off.
  const [selectedLaneId, selectedSessionKey] = selectedKey?.split('::') ?? [];
  const selected =
    visible.find((r) => r.key === selectedKey) ??
    (view === 'todo'
      ? todoItems.find((r) => r.key === selectedKey)
      : (visible.find((r) => r.kind === 'agent' && r.session.key === selectedSessionKey) ?? visible.find((r) => r.kind !== 'pr' && r.kind !== 'todo' && r.lane.id === selectedLaneId))) ??
    visible[0] ??
    null;
  // With nothing picked yet (the window just opened, or you changed projects) the panel shows
  // the top row, and it holds there: a session that comes to the top later doesn't take it.
  useEffect(() => {
    if (selectedKey === null && selected) setSelectedKey(selected.key);
  }, [selectedKey, selected?.key]);
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
    openWhere(where(lane, targetId), { report, from: 'main' });
  }, [report, markSeen]);

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
        const s = liveOrLatest(row.sessions);
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
      .catch((e: unknown) => setToast(errorText(e)))
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
  // allows the call it waits on) and goes on to the next listed that waits on you, ⌘E marks a
  // finished turn seen and goes on, and ⌘] (or ⌥⇧↓) skips to it, ⌥⇧↑ back; a skipped one comes
  // round again after the rest. The ones dealt with stay out until they wait on you anew, since an
  // allowed call isn't shown ahead of the board.
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
   *  none for a skip. With nothing left, the panel stays where it is. The next one's box takes the
   *  cursor unless `box` says to leave the keyboard where it is. */
  const goOn = (from: string, s: Session, done: string | null, box = true) => {
    if (view !== 'sessions' || selectedNow.current !== from) return;
    if (done) handled.current.set(s.key, statusTime(s));
    const next = queueAfter(waitingNow.current, s.key, handled.current)[0];
    if (!next) return;
    setQueueNote(done ? { key: next.key, text: `${s.title}: ${done}` } : null);
    if (box) setBoxFocus(next.key);
    setSelectedKey(next.key);
  };
  /** Back from the session in row `from` to the one before it that waits on you, the way ⌘]
   *  goes on, with the cursor in its box. */
  const goBack = (from: string, s: Session) => {
    if (view !== 'sessions' || selectedNow.current !== from) return;
    const before = queueAfter(waitingNow.current, s.key, handled.current);
    const prev = before[before.length - 1];
    if (!prev) return;
    setQueueNote(null);
    setBoxFocus(prev.key);
    setSelectedKey(prev.key);
  };
  /** Marks a finished turn seen and, when it was waiting on you, goes on to the next that is. The
   *  keyboard stays where it was: on the list, `e` works down it; from a box, the next one's box
   *  takes the cursor. */
  const seenAndOn = (from: string, s: Session, box: boolean) => {
    if (!seenable(s)) return;
    const waiting = waitingNow.current.some((r) => r.session.key === s.key);
    markSeen([s.key]);
    if (waiting) goOn(from, s, 'marked seen.', box);
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
  // once the click has given the list the keyboard, as a press does. The list's chords work from
  // the box too (`listChord`), and Escape in the box gives the list the keyboard back.
  const listRef = useRef<HTMLDivElement>(null);
  const focusBox = () =>
    window.setTimeout(() => {
      const el = document.querySelector<HTMLTextAreaElement>('.ca-sessinsp .ca-composer textarea');
      if (!el) return;
      el.focus();
      el.setSelectionRange(el.value.length, el.value.length);
    });

  // Back and forward through the sessions you looked at (history.ts): the one whose conversation
  // the panel shows, in Sessions, in a PR's or todo's panel, or as the canvas's selected card.
  const shownSession =
    view === 'sessions'
      ? selected?.kind === 'agent'
        ? selected.session.key
        : null
      : drill && drill.from === panelFrom && homes.has(drill.key)
        ? drill.key
        : view === 'canvas' && canvasSel?.ref === 'session'
          ? canvasSel.key
          : null;
  // One that's gone, archived, older than the sidebar's age filter shows or in a worktree no
  // project lists is passed over: the list wouldn't have it.
  const sessionHistory = useSessionHistory(shownSession, (key) => {
    const home = homes.get(key);
    return !!home && (!home.lane.hidden || inProject(home.lane, current)) && (home.session.lastActivity >= cutoff || sessionPins.set.has(key));
  }, HISTORY_KEY);
  /** Goes back or forward a session. It shows in Sessions: where it's listed, when the list has it
   *  as it's filtered now, else as a session the bar asks for is shown. From a session's box, the
   *  cursor goes to the box of the one gone to. */
  const travel = useRef<(dir: 'back' | 'forward', box: boolean) => void>(() => {});
  travel.current = (dir, box) => {
    const key = dir === 'back' ? sessionHistory.goBack() : sessionHistory.goForward();
    const home = key ? homes.get(key) : undefined;
    if (!key || !home) return;
    const row = sessionRows.find((r) => r.kind === 'agent' && r.session.key === key);
    if (row) {
      setView('sessions');
      setSelectedKey(row.key);
    } else {
      reveal.current({ kind: 'session', laneId: home.lane.id, key });
    }
    setQueueNote(null);
    if (box) setBoxFocus(`${home.lane.id}::${key}`);
  };
  /** What a history button says: where it goes, and its key. */
  const historyTitle = (to: string | null, word: string, none: string, keys: string) => {
    const title = to ? homes.get(to)?.session.title : null;
    return `${title ? `${word} to ${middleTruncate(title, 60)}` : none} (${keys})`;
  };

  /** Archives a session (Codex threads in Codex itself), or unarchives it. */
  const setArchived = useCallback((key: string, archived: boolean, title: string) => {
    setUndo(archived ? { key, title } : null);
    archiveAhead(key, archived).catch((e: unknown) => {
      setUndo(null);
      setToast(errorText(e));
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
    setBayFilter([]);
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
    setBayFilter([]);
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

  /** The Due date submenu: the days `dueChoices` offers, any day in the panel's picker (for one
   *  todo), and taking it off. A day is checked when every todo is due then. */
  const dueItems = (ts: Todo[]): MenuSpec => {
    const set = (due: string | null) => ts.forEach((t) => runTodo({ op: 'update', id: t.id, patch: { due } }));
    const same = ts.every((t) => t.due === ts[0].due) ? ts[0].due : null;
    return [
      ...dueChoices(Date.now()).map((c) => ({ label: c.label, checked: same === c.due, run: () => set(c.due) })),
      ...(ts.length === 1
        ? [
            {
              label: 'Pick a date…',
              run: () => {
                showTodo(ts[0]);
                setDuePick(ts[0].id);
              },
            },
          ]
        : []),
      ...(ts.some((t) => t.due) ? ['separator' as const, { label: ts.length === 1 ? 'Remove due date' : 'Remove due dates', run: () => set(null) }] : []),
    ];
  };

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
    const agent = liveOrLatest(r.sessions);
    return menus.todoMenu(t.id, {
      report,
      start: () => startForTodo(r),
      session: agent ? { ...agent, open: () => open(agent.lane, agent.session.openTarget) } : undefined,
      edit: [
        ...STATUSES.map((st) => ({ label: STATUS_WORD[st], checked: t.status === st, run: () => setTodoStatus(t, st) })),
        ...waitingItem([t]),
        'separator',
        { label: 'Due date', items: dueItems([t]) },
        { label: 'Labels', items: labelItems(t) },
        { label: 'Move to project', items: projectItems(t) },
      ],
      desktop: onDesktop,
      copyPrompt: () => copyPrompt([t]),
      canvas: canvasItem('todo', t.id),
      remove: () => deleteTodos([t]),
    });
  };

  /** The right-click menu of several todos picked together, and their panel's ⋯ menu. */
  const pickMenuSpec = (rows: TodoItem[]): MenuSpec => {
    const ts = rows.map((r) => r.todo);
    return [
      { label: `Start one session for ${rows.length} todos…`, run: () => startForTodos(rows) },
      'separator',
      ...STATUSES.map((st) => ({ label: STATUS_WORD[st], checked: ts.every((t) => t.status === st), run: () => ts.forEach((t) => setTodoStatus(t, st)) })),
      ...waitingItem(ts),
      { label: 'Due date', items: dueItems(ts) },
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
          })
          .catch((e: unknown) => setToast(errorText(e)));
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

  /** A PR's right-click menu, as every window has it (menus.ts), with this window's own. */
  const prMenuSpec = (r: PrRowItem): MenuSpec => {
    const p = r.pr;
    const lane = r.lane;
    return menus.prMenu(p, {
      report,
      review: reviewItems(p),
      approve: p.reviewRequested && prIsOpen(p) ? () => approve(p) : undefined,
      showLane: lane ? { lane, run: () => focusLane(lane) } : undefined,
      pin: { label: prPins.set.has(p.key) ? 'Unpin PR' : 'Pin PR', run: () => prPins.toggle(p.key), accelerator: 'P' },
      desktop: onDesktop,
      canvas: canvasItem('pr', p.key),
    });
  };
  const prMenu = (r: PrRowItem) => void showContextMenu(prMenuSpec(r)).catch((e: unknown) => setToast(String(e)));
  /** Opens a worktree's PR on GitHub: a click on its chip, or Open PR in a menu. */
  const openLanePr = (lane: Lane) => {
    if (lane.pr) openUrl(lane.pr.url).catch((e: unknown) => setToast(String(e)));
  };
  /** The right-click menu of a worktree's PR chip: the PR's own. */
  const lanePrMenu = (lane: Lane) => {
    if (lane.pr) prChipMenu(lane, lane.pr);
  };
  /** A PR's menu where a worktree or one of its sessions shows it. A PR the board doesn't list,
   *  such as one on another Mac in a repo this one doesn't have, gets what its link allows. */
  const prChipMenu = (lane: Lane, p: Pr) => {
    const item = board?.prs.find((x) => x.url === p.url);
    const spec: MenuSpec = item ? prMenuSpec(prRowOf(item)) : menus.prMenu(p, { report, review: reviewItems(reviewedPr(lane, p)) });
    void showContextMenu(spec).catch((e: unknown) => setToast(String(e)));
  };

  /** Pins or unpins a worktree. Its lines keep their keys as the row moves, so what was selected stays selected. */
  const toggleLanePin = useCallback((lane: Lane) => lanePins.toggle(lane.id), [lanePins]);

  /**
   * A session's right-click menu, as every window has it (menus.ts), with what only this window
   * does: rename it in a sheet, add it to a todo, pin it, add it to the canvas, undo an archive. Each list pins what it
   * shows: Sessions pins the session; the board and the sidebar, lists of worktrees, pin the
   * session's worktree.
   */
  const todoItemsRef = useRef(todoMenuItems);
  todoItemsRef.current = todoMenuItems;
  const editorIcons = useEditorIcons(board?.apps);
  const editors = board ? { apps: board.apps, icons: editorIcons } : undefined;
  /** What this window can do to a session, for its menu and its keys, which run the same
   *  actions (menus.ts). */
  const sessionCan = (lane: Lane, s: Session, pin: 'session' | 'worktree'): menus.SessionCan => ({
      report,
      keys: view === 'sessions' ? 'chords' : true,
      open,
      terminal: board?.apps.terminal ?? 'Terminal',
      editors,
      newSession: () => newSession({ laneId: lane.id }),
      todos: todoItemsRef.current(lane.project, { session: s.key }, { session: s.key, worktree: lane.id, label: `${s.account} in ${lane.callsign}` }, s.title),
      pin:
        pin === 'session'
          ? { label: sessionPins.set.has(s.key) ? 'Unpin session' : 'Pin session', run: () => sessionPins.toggle(s.key) }
          : { label: lanePins.set.has(lane.id) ? 'Unpin worktree' : 'Pin worktree', run: () => toggleLanePin(lane) },
      desktop: onDesktop,
      rename: () => setRenaming(s.key),
      archive: (archived) => setArchived(s.key, archived, s.title),
      canvas: canvasItem('session', s.key),
  });
  const sessionMenuSpec = (lane: Lane, s: Session, pin: 'session' | 'worktree'): MenuSpec => menus.sessionMenu(lane, s, sessionCan(lane, s, pin));
  const sessionMenu = (lane: Lane, s: Session, pin: 'session' | 'worktree') => void showContextMenu(sessionMenuSpec(lane, s, pin)).catch((e: unknown) => setToast(String(e)));

  /** A worktree's right-click menu, as every window has it (menus.ts), with this window's own. */
  const laneCan = (lane: Lane): menus.LaneCan => ({
      report,
      keys: true,
      open,
      editors,
      newSession: () => newSession({ laneId: lane.id }),
      setPreview: () => setPreviewFor(lane.id),
      todos: todoItemsRef.current(lane.project, { worktree: lane.id }, { worktree: lane.id, label: lane.callsign }),
      pin: { label: lanePins.set.has(lane.id) ? 'Unpin worktree' : 'Pin worktree', run: () => toggleLanePin(lane) },
      desktop: onDesktop,
      canvas: canvasItem('worktree', lane.id),
  });
  const laneMenuSpec = (lane: Lane): MenuSpec => menus.laneMenu(lane, laneCan(lane));
  // The keys run the same actions, as this render has them.
  const actionsRef = useRef({ session: (lane: Lane, s: Session, pin: 'session' | 'worktree') => menus.sessionActions(lane, s, sessionCan(lane, s, pin)), lane: (lane: Lane) => menus.laneActions(lane, laneCan(lane)) });
  actionsRef.current = { session: (lane, s, pin) => menus.sessionActions(lane, s, sessionCan(lane, s, pin)), lane: (lane) => menus.laneActions(lane, laneCan(lane)) };
  const laneMenu = (lane: Lane) => void showContextMenu(laneMenuSpec(lane)).catch((e: unknown) => setToast(String(e)));

  // A worktree's row dragged on the board: among the pinned ones to pin it there or move it, from
  // them down to the bays to unpin it, out of the window onto the desktop (useRowDrag.tsx).
  // What's on another device has no card here.
  const laneDrag = useRowDrag({
    onPlace: (id, index) => lanePins.place(pinnedLaneRows.map((r) => r.lane.id), id, index),
    joins: true,
    onLeave: lanePins.remove,
    desktop: (id) => (laneById.get(id)?.device ? null : { ref: 'worktree', key: id }),
    label: (id) => laneById.get(id)?.callsign ?? '',
  });

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
    setBayFilter([]);
    if (lane.parked) setShowParked(true);
    setSelectedKey(boardRowKey(lane));
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
      // Its own row in Sessions, with its conversation in the panel, whichever view was showing:
      // the board would only show its line on its worktree's row.
      setView('sessions');
      setBayFilter([]);
      setSelectedKey(`${lane.id}::${session.key}`);
    } else if (pr) {
      setView('prs');
      setBayFilter([]);
      setSelectedKey(`pr:${pr.key}`);
    }
  };
  useEffect(() => onReveal((r) => reveal.current(r)), []);

  // The Todo view's own keys, bound fresh each render so they see the rows as they are. N adds a
  // todo, X marks one done (or reopens it), W moves it to Waiting or out, D picks its due date,
  // Delete deletes it, ⌃⌘↑ ⌃⌘↓ move it. ⇧↑ ⇧↓ pick the rows above or below too; then X, W and Delete act on all of
  // them, Return starts one session for them, and Escape goes back to one.
  const todoKeys = useRef<(e: KeyboardEvent, key: string) => boolean>(() => false);
  todoKeys.current = (e, key) => {
    if (view !== 'todo') return false;
    // Reordering moves one todo at a time.
    if (e.ctrlKey && e.metaKey && !e.altKey && !e.shiftKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      e.preventDefault();
      if (selected?.kind !== 'todo' || pickedRows.length > 0 || selected.todo.status === 'done') return true;
      const section = todoSecs.find((s) => s.rows.some((r) => r.key === selected.key));
      const i = section?.rows.findIndex((r) => r.key === selected.key) ?? -1;
      // Insertion points: one up is the row's own index less one; one down is two past it.
      if (section && i >= 0) moveTodo(section.id, selected.todo.id, e.key === 'ArrowUp' ? i - 1 : i + 2);
      return true;
    }
    if (e.metaKey || e.ctrlKey) return false;
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
    if (key === 'd' && !e.altKey && pickedRows.length === 0) {
      e.preventDefault();
      setDuePick(t.id);
      return true;
    }
    // Only from the list: a button in the panel keeps the keyboard's other uses.
    const onList = !document.activeElement || document.activeElement === document.body || !!document.activeElement.closest('.ca-board-list');
    if ((e.key === 'Backspace' || e.key === 'Delete') && onList) {
      e.preventDefault();
      deleteTodos(ts);
      return true;
    }
    return false;
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = document.activeElement === searchRef.current;
      const mod = e.metaKey;
      const key = e.key.toLowerCase();
      const inBox = view === 'sessions' && !!document.activeElement?.closest('.ca-sessinsp .ca-composer');
      // Sessions takes no plain letters: a click on a row puts the cursor in the panel's box, and a
      // word typed on the list by mistake would otherwise pin, archive or rename. Its keys are
      // chords, which work from the box too (`listChord`) and in the other views as well.
      const letters = view !== 'sessions';
      // ⌃⌘, with a menu's letter or ↑ ↓.
      const chord = e.ctrlKey && mod && !e.altKey && !e.shiftKey;
      const vertical = e.key === 'ArrowUp' || e.key === 'ArrowDown';
      // Escape in a session panel's box gives the list its keys back (a click on a row put them
      // there), unless it stopped the session's turn. With the panel expanded there's no list to
      // give them to, so the window has them.
      if (e.key === 'Escape' && !e.isComposing && !e.defaultPrevented && inBox) {
        e.preventDefault();
        if (panelMax) (document.activeElement as HTMLElement).blur();
        else listRef.current?.focus();
        return;
      }
      // A text field, a todo's title or description say, keeps its keys but for the window's own ⌘
      // ones, and Sessions' box for the list's chords.
      if (inField(document.activeElement, searchRef.current) && !(inBox && listChord(e)) && !historyChord(e) && !(mod && (key === 'k' || key === 'p' || e.key === ',' || ((e.code === 'BracketRight' || key === 'e') && view === 'sessions') || (e.altKey && /^Digit\d$/.test(e.code))))) return;
      if (mod && e.key === ',') {
        e.preventDefault();
        openSettings();
        return;
      }
      // Settings has the keyboard while it's open, as the preview URL and rename sheets do.
      if (settings || previewFor || renaming) return;
      // ⌃- goes back to the session you were looking at before and ⌃⇧- forward again, from any
      // view and any field. From a session's box the cursor goes on to the next one's.
      if (historyChord(e)) {
        e.preventDefault();
        travel.current(e.shiftKey ? 'forward' : 'back', !!document.activeElement?.closest('.ca-sessinsp .ca-composer'));
        return;
      }
      // ⌘] (or ⌥⇧↓, below) skips to the next session that waits on you, from the panel's box too.
      // ] goes by its key's code, which layouts that type it with ⌥ keep.
      if (mod && !e.ctrlKey && !e.shiftKey && e.code === 'BracketRight' && view === 'sessions') {
        e.preventDefault();
        if (selected?.kind === 'agent') goOn(selected.key, selected.session, null);
        return;
      }
      // ⌘E marks a finished turn seen and goes on, from the panel's box too. Elsewhere it marks
      // seen as `e` does (below).
      if (mod && !e.ctrlKey && !e.shiftKey && !e.altKey && key === 'e' && view === 'sessions') {
        e.preventDefault();
        if (selected?.kind === 'agent') seenAndOn(selected.key, selected.session, inBox);
        return;
      }
      if (mod && key === 'k') {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
        return;
      }
      if (mod && !e.ctrlKey && key === 'p') {
        e.preventDefault();
        openSwitcher();
        return;
      }
      if (mod && e.ctrlKey && key === 's') {
        e.preventDefault();
        toggleSidebar();
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
      // ⌃Tab and ⌃⇧Tab go through the views as they go through tabs; so does v outside Sessions.
      const tab = e.key === 'Tab' && e.ctrlKey && !mod && !e.altKey;
      if (tab || (key === 'v' && !mod && letters)) {
        e.preventDefault();
        const i = VIEWS.findIndex((x) => x.id === view);
        setView(VIEWS[(i + (tab && e.shiftKey ? VIEWS.length - 1 : 1)) % VIEWS.length].id);
        return;
      }
      if (todoKeys.current(e, key)) return;
      if (mod && e.shiftKey && !e.ctrlKey && !e.altKey && key === 'f' && view === 'sessions' && selected?.kind === 'agent') {
        e.preventDefault();
        togglePanelMax();
        return;
      }
      // P, A and R run the selected row's menu actions (menus.ts `KEYS`), plain outside Sessions and
      // with ⌃⌘ anywhere, so each applies where the menu's would. P pins what the view lists:
      // worktrees on the board, sessions in Sessions. E, below, marks seen more than one session
      // at once.
      if (((!mod && letters) || chord) && selected && selected.kind !== 'pr' && selected.kind !== 'todo') {
        const acts = selected.kind === 'agent' ? actionsRef.current.session(selected.lane, selected.session, view === 'worktrees' ? 'worktree' : 'session') : actionsRef.current.lane(selected.lane);
        const found = menus.keyed(acts, key);
        if (found && found.id !== 'seen') {
          // Not typed into a field the action opens (Rename's).
          e.preventDefault();
          found.action.run();
          return;
        }
      }
      // P pins the PR selected in PRs, as it pins what the other views list.
      if (((!mod && letters) || chord) && key === 'p' && view === 'prs' && selected?.kind === 'pr') {
        e.preventDefault();
        prPins.toggle(selected.pr.key);
        return;
      }
      // ⌃⌘↑ ⌃⌘↓ move a pinned session, worktree or PR among the pins (a todo in Todos, above).
      if (chord && vertical) {
        e.preventDefault();
        // Insertion points: one up is the row's own index less one; one down is two past it.
        const step = (i: number) => (e.key === 'ArrowUp' ? i - 1 : i + 2);
        if (view === 'sessions' && selected?.kind === 'agent' && sessionPins.set.has(selected.session.key)) {
          movePinnedSession(selected.session.key, step(pinnedRows.findIndex((r) => r.key === selected.key)));
        } else if (view === 'worktrees' && selected && selected.kind !== 'pr' && selected.kind !== 'todo' && lanePins.set.has(selected.lane.id)) {
          movePinnedLane(selected.lane.id, step(pinnedLaneRows.findIndex((r) => r.key === selected.lane.id)));
        } else if (view === 'prs' && selected?.kind === 'pr' && prPins.set.has(selected.pr.key)) {
          prPins.move(pinnedPrRows.map((r) => r.pr.key), selected.pr.key, step(pinnedPrRows.findIndex((r) => r.key === selected.key)));
        }
        return;
      }
      // ⌥⇧↓ goes on to the next that waits on you, as ⌘] does, and ⌥⇧↑ back to the one before.
      if (view === 'sessions' && e.altKey && e.shiftKey && !mod && !e.ctrlKey && vertical) {
        e.preventDefault();
        if (selected?.kind !== 'agent') return;
        if (e.key === 'ArrowDown') goOn(selected.key, selected.session, null);
        else goBack(selected.key, selected.session);
        return;
      }
      if (e.key === '/') {
        e.preventDefault();
        searchRef.current?.focus();
      } else if (e.key === 'ArrowDown' || (key === 'j' && !mod && letters)) {
        // The arrows, and ⌥↑ ⌥↓ from Sessions' box, where the cursor goes on to the next one's.
        e.preventDefault();
        move(1);
        if (inBox && selected !== visible[visible.length - 1]) focusBox();
      } else if (e.key === 'ArrowUp' || (key === 'k' && !mod && letters)) {
        e.preventDefault();
        move(-1);
        if (inBox && selected !== visible[0]) focusBox();
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
      } else if (key === 'e' && ((!mod && letters) || (mod && !e.ctrlKey && !e.shiftKey && !e.altKey))) {
        // ⌘E too, as in Sessions (above).
        const sessions = selected?.kind === 'agent' ? [selected.session] : selected?.kind === 'todo' ? (pickedRows.length > 0 ? pickedRows : [selected]).flatMap((r) => r.sessions.map((x) => x.session)) : (lane?.sessions ?? []);
        markSeen(sessions.filter(seenable).map((s) => s.key));
      } else if (key === 'g' && !mod && letters) {
        gPending.current = true;
        window.setTimeout(() => (gPending.current = false), 1200);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [visible, selected, pickedRows, selectedLane, startLaneId, move, open, openRow, copy, settings, previewFor, renaming, openSettings, newSession, view, setView, toggleLanePin, sessionPins, lanePins, prPins, pinnedRows, pinnedLaneRows, pinnedPrRows, shownBays, movePinnedSession, movePinnedLane, current, setArchived, projects, toggleSidebar, panelMax, togglePanelMax]);

  // Starts this window asked the quick panel for. A spinner shows while the harness takes the
  // session; then what's next. The new session isn't selected when it shows up: by then you may
  // be reading another, and the panel stays on what you picked. A failed start brings the panel
  // back with what you wrote and why.
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
    });
    return () => {
      off();
      timers.forEach((t) => window.clearTimeout(t));
    };
  }, []);


  const toggleBay = (f: Filter) => {
    setBayFilter((fs) => (fs.includes(f) ? fs.filter((x) => x !== f) : [...fs, f]));
    if (view === 'canvas' && !bayFilter.includes(f)) setCanvasFit((n) => n + 1);
  };
  const toggleDevice = (id: string) => setDeviceFilter((d) => (d.includes(id) ? d.filter((x) => x !== id) : [...d, id]));
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

  /** Whether a card matches the filter at the top and the status filter, as its row would in its own view. */
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
  // Picking a status, or pressing Return in the filter, brings what matches into view.
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
  const showSessionFromPanel = (lane: Lane, session: Session) => {
    if (view === 'canvas' && canvas.has('session', session.key)) setCanvasFocus((f) => ({ id: refId('session', session.key), n: (f?.n ?? 0) + 1 }));
    else reveal.current({ kind: 'session', laneId: lane.id, key: session.key });
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
      pickDue={duePick === item.todo.id}
      onPickDue={duePicked}
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
        onShowLane={showLaneFromPanel}
        prs={sessionPrs(board?.prs ?? [], lane, session)}
        onOpenPr={(p) => openUrl(p.url).catch((e: unknown) => setToast(String(e)))}
        onPrMenu={(p) => prChipMenu(lane, p)}
        todos={todoLinks.bySession.get(session.key)}
        onShowTodo={showTodoFromPanel}
        lineage={lineageOf(session, homes)}
        onShowSession={showSessionFromPanel}
        onSessionMenu={(l, x) => sessionMenu(l, x, 'session')}
        onMenu={(at) => void showContextMenu(sessionMenuSpec(lane, session, 'session'), at).catch((err: unknown) => setToast(String(err)))}
        onSeen={() => (row !== undefined ? seenAndOn(row, session, false) : markSeen([session.key]))}
        device={deviceOf(devices, session.key)}
        back={back}
        onFailed={(why) => {
          // Not dealt with after all: it comes round again.
          handled.current.delete(session.key);
          setToast(`Didn’t send to ${session.title}, so it’s back in its box: ${why}`);
        }}
        queue={row !== undefined && queued ? { left: left.length, next: left[0]?.session ?? null, onNext: () => goOn(row, session, null) } : undefined}
        notice={note}
        onDone={row !== undefined ? (done) => goOn(row, session, done) : undefined}
        autoFocus={row !== undefined && boxFocus === row}
        maximized={row !== undefined && panelMax}
        onMaximize={row !== undefined ? togglePanelMax : undefined}
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

  /** A worktree's row on the board: `index` is its place among the pinned ones. */
  const laneRow = (r: WorktreeRow, index: number | null, drag: DragProps) => (
    <LaneRow
      key={r.key}
      row={r}
      now={now}
      selectedKey={selected?.key ?? null}
      domId={domIdOf}
      onSelect={setSelectedKey}
      onOpen={(lane) => open(lane, lane.defaultTarget)}
      onOpenTarget={open}
      pinned={index != null}
      onContextMenu={() => laneMenu(r.lane)}
      onAgentMenu={(s) => sessionMenu(r.lane, s, 'worktree')}
      onPrMenu={() => lanePrMenu(r.lane)}
      onPrOpen={() => openLanePr(r.lane)}
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
  // The session the rename sheet is for, in its own worktree.
  const renamed = useMemo(() => homeOf(board?.lanes ?? [], renaming), [renaming, board]);

  // The sidebar: the views with their counts, the statuses to narrow them to, and what narrows
  // the view now, for the collapsed rail's badge.
  const sideViews = VIEWS.map((v) => ({
    id: v.id,
    label: v.label,
    icon: v.icon,
    count: v.id === 'worktrees' ? recentLanes.length : v.id === 'sessions' ? shownSessionCount : v.id === 'todo' ? openTodoCount : v.id === 'canvas' ? (canvas.doc?.items.filter((it) => it.kind === 'ref').length ?? '') : openPrCount,
    badge: v.id === 'prs' && requestedCount > 0 ? requestedCount : undefined,
    badgeTitle: v.id === 'prs' ? `${requestedCount} want your review` : undefined,
  }));
  const sideStatuses: SideStatus[] = [
    { id: 'needs', label: 'Needs you', glyph: 'needs', count: counts.needs, tone: 'attn' },
    { id: 'unread', label: 'Your turn', glyph: 'unread', count: counts.unread },
    { id: 'working', label: 'Working', glyph: 'working', count: counts.working },
    { id: 'review', label: 'In review', icon: 'pr', count: counts.review },
    { id: 'work', label: 'PR needs work', glyph: 'error', count: counts.work, tone: 'alert', title: 'Open PRs with failing checks or changes requested' },
  ];
  const ageApplies = view !== 'todo' && view !== 'canvas';
  const shownLabel = view === 'todo' && labelFilter ? (labelById.get(labelFilter) ?? null) : null;
  const sideFilters = [
    ageApplies && age !== 'all' ? (AGES.find((a) => a.id === age)?.title.replace(/^Active in the /, '') ?? age) : null,
    ...bayFilter.map((f) => sideStatuses.find((x) => x.id === f)?.label ?? f),
    shownLabel ? `label ${shownLabel.name}` : null,
  ].filter((x): x is string => !!x);
  const viewNow: ViewState = { view, project: current, device: deviceFilter, age, filter: bayFilter };
  const savedOn = savedViews.find((v) => sameView(v, viewNow))?.id ?? null;
  const deviceName = (id: string) => (id === 'here' ? devices?.me.name || 'This device' : (devices?.devices.find((d) => d.short === id)?.name ?? null));
  const either = (words: (string | null | undefined)[]) => words.filter(Boolean).join(' + ') || null;
  const viewSuggestion = suggestName([
    VIEWS.find((v) => v.id === view)?.label,
    current === ALL_PROJECTS ? null : current,
    either(deviceFilter.map(deviceName)),
    either(bayFilter.map((f) => sideStatuses.find((x) => x.id === f)?.label)),
    ageApplies && age !== 'all' ? AGES.find((a) => a.id === age)?.label : null,
  ]);
  const applySaved = (v: SavedView) => {
    // Every field is checked against what exists now: a project or device can be gone since.
    if (v.project !== current && (v.project === ALL_PROJECTS || projects.some((p) => p.name === v.project))) pickProject(v.project);
    const known = VIEWS.find((x) => x.id === v.view);
    if (known) setView(known.id);
    setDeviceFilter(v.device.filter((id) => id === 'here' || devices?.devices.some((d) => d.short === id)));
    const a = AGES.find((x) => x.id === v.age);
    if (a) setAge(a.id);
    setBayFilter(v.filter);
  };
  const savedMenu = (v: SavedView) =>
    void showContextMenu([
      { label: 'Update to this view', enabled: savedOn === null, run: () => keepSaved(savedViews.map((x) => (x.id === v.id ? { ...x, ...viewNow } : x))) },
      'separator',
      { label: 'Delete', run: () => keepSaved(savedViews.filter((x) => x.id !== v.id)) },
    ]).catch((e: unknown) => setToast(String(e)));

  // PR links in conversations and descriptions (PrLink.tsx): their cards show the PR and its
  // sessions here.
  usePrLinks(board?.prs, lanes, {
    showPr: (pr) => reveal.current({ kind: 'pr', key: pr.key }),
    showSession: (lane, session) => reveal.current({ kind: 'session', laneId: lane.id, key: session.key }),
    openSession: (lane, session) => open(lane, session.openTarget),
    sessionMenu: (lane, s) => sessionMenu(lane, s, 'worktree'),
    prMenu: (url, pr) => {
      if (pr) prMenu(prRowOf(pr));
      else void showContextMenu(menus.prMenu({ url, number: prRefOf(url)?.number ?? 0 }, { report })).catch((e: unknown) => setToast(String(e)));
    },
  });

  return (
    <div className="ca-win ca-win--app">
      <div className="ca-win-bar" data-tauri-drag-region>
        <button
          type="button"
          className={sideCollapsed ? 'ca-sidetoggle' : 'ca-sidetoggle is-on'}
          aria-label={sideCollapsed ? 'Show sidebar' : 'Hide sidebar'}
          aria-pressed={!sideCollapsed}
          title={`${sideCollapsed ? 'Show' : 'Hide'} the sidebar (⌃⌘S)`}
          onClick={toggleSidebar}
        >
          <Icon name="layout-sidebar" size={16} />
        </button>
        <div className="ca-histnav" role="group" aria-label="Sessions you looked at">
          <button
            type="button"
            aria-label="Back"
            aria-disabled={!sessionHistory.back}
            title={historyTitle(sessionHistory.back, 'Back', 'Back to the session you looked at before', '⌃-')}
            onClick={() => travel.current('back', false)}
          >
            <Icon name="chevron-left" size={16} />
          </button>
          <button
            type="button"
            aria-label="Forward"
            aria-disabled={!sessionHistory.forward}
            title={historyTitle(sessionHistory.forward, 'Forward', 'Forward again, after going back', '⌃⇧-')}
            onClick={() => travel.current('forward', false)}
          >
            <Icon name="chevron-right" size={16} />
          </button>
        </div>
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
        loading={loading}
        now={now}
        onRefresh={() => void refreshUsage()}
        onAccounts={(account) => openSettings('accounts', account ?? null)}
        onSignIn={startSignIn}
      />
      <div
        className={['ca-win-body', sideCollapsed && 'is-railed', panelKind === 'pr' && 'is-prs', panelKind === 'todo' && 'is-todo', panelKind === 'session' && 'is-session', panelKind === 'session' && panelMax && 'is-panel-max', view === 'canvas' && 'is-canvas', view === 'canvas' && canvasPanel && 'has-panel'].filter(Boolean).join(' ')}
        style={{ '--sidebar-w': widthVar(sidebarWidth), '--panel-w': widthVar(panelKind ? panelWidth[panelKind] : null) } as CSSProperties}
      >
        <Sidebar
          collapsed={sideCollapsed}
          onExpand={toggleSidebar}
          now={now}
          views={sideViews}
          view={view}
          onView={(id) => setView(id as View)}
          devices={devices}
          deviceFilter={deviceFilter}
          onDevice={toggleDevice}
          onClearDevices={() => setDeviceFilter([])}
          deviceNeeds={deviceNeeds}
          deviceWorking={deviceWorking}
          groupByDevice={view === 'worktrees' && devices?.enabled && devices.lanes.length > 0 && deviceFilter.length !== 1 ? groupByDevice : null}
          onGroupByDevice={toggleGroupByDevice}
          ages={AGES}
          age={age}
          ageApplies={ageApplies}
          onAge={(id) => setAge(id as Age)}
          sorts={view === 'sessions' ? SESSION_SORTS : null}
          sort={sessionSort}
          onSort={(id) => setSessionSort(id as SessionSort)}
          rowStyles={view === 'sessions' ? SESSION_ROWS : null}
          rowStyle={sessionLines}
          onRowStyle={(id) => setSessionLines(id as SessionRows)}
          statuses={sideStatuses}
          filter={bayFilter}
          onFilter={toggleBay}
          onClearFilters={() => setBayFilter([])}
          label={shownLabel}
          onClearLabel={() => setLabelFilter(null)}
          saved={savedViews}
          savedOn={savedOn}
          onApplySaved={applySaved}
          onSavedMenu={savedMenu}
          suggestion={viewSuggestion}
          onSave={(name) => keepSaved([...savedViews, { id: newSavedId(), name, ...viewNow }])}
          filters={sideFilters}
          onSettings={() => openSettings()}
          hasWarnings={(board?.warnings.length ?? 0) > 0}
          loading={loading}
          foot={
            <div className="ca-side-foot">
              <button type="button" className="ca-linkbtn ca-side-config" title="Settings (⌘,)" onClick={() => openSettings()}>
                <Icon name="settings" size={14} />
                <span>Settings</span>
              </button>
              <ThemeSwitch theme={theme} onPick={pickTheme} />
            </div>
          }
          warnings={
            <>
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
            </>
          }
        />
        {sideCollapsed ? <span aria-hidden="true" /> : <Resizer panel="before" cssVar="--sidebar-w" onResize={setSidebarWidth} />}
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
          {board && !board.loading && view === 'prs' && prRows.length === 0 && (
            <div className="ca-board-empty">
              {query ? `No PRs in ${current} match “${query}”.` : board.githubAt ? 'No PRs here right now.' : 'Loading PRs from GitHub…'}
            </div>
          )}
          {loadError && <div className="ca-board-empty">{loadError}</div>}
          {loading && (
            <div className="ca-board-empty ca-board-loading" role="status">
              <StatusGlyph status="working" size={12} title="" />
              <span>{READING[view]}</span>
            </div>
          )}
          {board && !board.loading && view === 'sessions' && sessionRows.length === 0 && (
            <div className="ca-board-empty">
              {query
                ? `No sessions in ${current} match “${query}”.`
                : bayFilter.length > 0
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
                : bayFilter.length === 1 && bayFilter[0] === 'work'
                  ? 'No PRs need work: no failing checks or changes requested.'
                  : bayFilter.length > 0
                    ? 'Nothing here right now.'
                  : cutoff > 0
                    ? `No worktrees ${AGES.find((a) => a.id === age)?.title.toLowerCase()}.`
                    : 'No worktrees yet. Codealong looks for repos under the folders in config.toml and for the agents working in them.'}
            </div>
          )}
          {view === 'canvas' && board && canvas.doc && !loading && (
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
              filtering={!!query.trim() || bayFilter.length > 0}
              matches={canvasMatches}
              fitMatches={canvasFit}
            />
          )}
          {view !== 'canvas' && !loading && (
          <div
            role="listbox"
            aria-label={view === 'sessions' ? 'Sessions' : view === 'prs' ? 'Pull requests' : view === 'todo' ? 'Todos' : 'Agents and worktrees'}
            aria-activedescendant={selected ? domId(selected) : undefined}
            aria-multiselectable={view === 'todo' || undefined}
            tabIndex={0}
            ref={listRef}
            className="ca-board-list"
            data-drag-list
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
                  // On one of the todos picked, the menu is for all of them; on another, for that
                  // one, and the selection stays where it was.
                  if (pickedKeys.has(r.key)) {
                    pickMenu(pickedRows);
                    return;
                  }
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
                pinned={pinnedPrRows}
                onPlacePinned={(key, index) => prPins.place(pinnedPrRows.map((r) => r.pr.key), key, index)}
                onUnpin={prPins.remove}
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
                rows={sessionList}
                sort={sessionSort}
                lines={sessionLines}
                now={now}
                selectedKey={selected?.key ?? null}
                domId={domId}
                onSelect={(r) => {
                  setSelectedKey(r.key);
                  focusBox();
                }}
                onOpen={(r) => {
                  setSelectedKey(r.key);
                  openRow(r);
                }}
                onContextMenu={(r) => sessionMenu(r.lane, r.session, 'session')}
                onPrMenu={(r) => lanePrMenu(r.lane)}
                onPrOpen={(r) => openLanePr(r.lane)}
                onPlacePinned={(key, index) => sessionPins.place(pinnedRows.map((r) => r.session.key), key, index)}
                onUnpin={sessionPins.remove}
                desktop={(r) => (r.lane.device || r.session.archived ? null : { ref: 'session', key: r.session.key })}
                archived={archivedItems}
                showArchived={showArchived}
                onToggleArchived={() => setShowArchived(!showArchived)}
                todosFor={todosFor}
                onShowTodo={showTodo}
              />
            )}
            {/* The first pin lands at the top of the board. */}
            {view === 'worktrees' && pinnedLaneRows.length === 0 && laneDrag.dropIndex === 0 && <div className="ca-dropline" />}
            {view === 'worktrees' && pinnedLaneRows.length > 0 && (
              <PinnedSection items={pinnedLaneRows} row={(r, i) => laneRow(r, i, laneDrag.rowProps(r.lane.id, i, pinnedLaneRows.length))} />
            )}
            {view === 'worktrees' && laneDrag.ghost}
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
                            {b.rows.map((r) => laneRow(r, null, laneDrag.rowProps(r.lane.id, null, pinnedLaneRows.length)))}
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
              <p>{projectTodos.length || loading ? 'Select a todo to see its description, labels and the sessions on it.' : 'Todos are work you mean to hand an agent. Add one on the left.'}</p>
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
          lanes={board?.lanes ?? NO_LANES}
        />
      )}
      {closeIdle && <CloseIdleDialog onClose={() => setCloseIdle(false)} onDone={setToast} />}
      {previewLane && <PreviewDialog lane={previewLane} onClose={() => setPreviewFor(null)} />}
      {renamed && <RenameDialog lane={renamed.lane} session={renamed.session} onClose={() => setRenaming(null)} onFailed={setToast} />}
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

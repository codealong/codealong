// The main window. One project at a time, picked in the switcher at top left. The sidebar
// picks the view and narrows it: device, how recently active, status, and views you saved
// (Sidebar.tsx); it collapses to a rail. The main area has five views: Sessions lists every session,
// pinned ones first in your order; Worktrees lists pinned worktrees in your order, then every
// other worktree once, in Needs you, Working, Your turn, In review, Idle or Merged, with a line
// for each agent that needs you, is working or just finished; PRs lists the project's pull
// requests; Todo lists work you mean to hand an agent, and the sessions on it;
// Canvas lays the project's work out in space, to be remembered by where it is (CanvasView.tsx).
// Keyboard map: the design system's Surfaces page.

import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { below, showContextMenu, type MenuAction, type MenuSpec } from './contextMenu';
import { useEditorIcons } from './editors';
import { homeOf, narrowed, rowItems, worktreeRow, type BoardRow, type Filter, type WorktreeRow } from './lanes';
import { answerLink, barMode, getDevices, onDevices, onBarMode, onCloseIdle, onNewWindow, onReveal, onSettings, onCheckUpdates, openConfig, openProjectWindow, openUrl, refresh, refreshUsage, reviewPr, setBarMode, startOptions, setPinned, setPinnedPrs, setPinnedWorktrees, setTheme, showQuick, signIn, windowLabel, zoomWindows, onZoomMenu, type BarMode, type Reveal } from './api';
import { count, middleTruncate } from './format';
import * as menus from './menus';
import { prRefOf } from './prLinks';
import { archiveAhead, boardView, lanesAhead, observe, seenAhead } from './ahead';
import { reuse } from './reuse';
import { usePins } from './pins';
import { reviewItems as reviewItemsFor } from './review';
import { START_OPTIONS_NOTICE, startOptionsFailed } from './start';
import { itemPrimaryHolds, todoAgent, todoInFilter, todoInQuery, todoPrompt, useTodoStore, type TodoItem } from './todos';
import { repoReviews } from './repoSettings';
import type { DevicesView, Lane, Pr, PrItem, Session, StartOptions, TeamFile, Theme, Todo } from './types';
import { shortOf, type DeviceFilter } from './devices';
import { NEW_PROJECT, SettingsDialog, type SettingsSection } from './ui/SettingsDialog';
import { CloseIdleDialog } from './ui/CloseIdleDialog';
import { PreviewDialog } from './ui/PreviewBits';
import { RenameDialog } from './ui/RenameDialog';
import { LaneRow } from './ui/LaneRow';
import { PinnedSection } from './ui/PinnedSection';
import { useRowDrag, type DragProps } from './ui/useRowDrag';
import { BayHeader, Button, cx, Icon, Kbd, Seg, StatusGlyph } from './ui/primitives';
import { ALL_PROJECTS, ProjectSwitcher } from './ui/ProjectSwitcher';
import { useLogo } from './logo';
import { usePrLinks } from './ui/PrLink';
import { reviewedWords } from './prDraft';
import { mayApprove } from './prStatus';
import { PrsView, type PrRowItem } from './ui/PrsView';
import { Resizer, usePanelWidth } from './ui/Resizer';
import { SESSION_ROWS, SESSION_SORTS, SessionsBar, SessionsView, type SessionItem, type SessionRows, type SessionSort } from './ui/SessionsView';
import { TodoComposer, TodoView } from './ui/TodoView';
import { CanvasView } from './ui/CanvasView';
import { entitiesOf, homeLane, refId, type RefItem, type RefKind } from './canvas/model';
import { useCanvas } from './canvas/useCanvas';
import { ThemeSwitch, useTheme } from './ui/ThemeSwitch';
import { Tip } from './ui/Tip';
import { UsageBar } from './ui/UsageBar';
import { Notices } from './ui/Notices';
import { clearNotice, keptDismissals, noticeId, reportNotice, shownNotices, useReportedNotices } from './notices';
import { Sidebar, type SideStatus } from './ui/Sidebar';
import { loadSaved, newSavedId, onSavedChange, sameView, storeSaved, suggestName, type SavedView, type ViewState } from './savedViews';
import { useNow } from './ui/useNow';
import { errorText } from './report';
import { reviewSnoozeMenu } from './snooze';
import { useSessionHistory } from './history';
import { QUEUE_CHORDS } from './queue';
import { SESSION_READS, shownSession as queueShown, useWaitingQueue } from './useWaitingQueue';
import { openWhere, where } from './open';
import { asText, flag, oneOf, strings, useStored } from './stored';
import { useBoard } from './useBoard';
import { useLatest } from './latest';
import { useMainKeys, viewChord } from './mainKeys';
import { useListPicks } from './useListPicks';
import { useTodoView } from './useTodoView';
import { canvasPicked, MainPanel } from './ui/MainPanel';
import { Toasts, useToasts } from './ui/Toasts';
import { ageListingPr, ageListingSession, laneInQuery, prInDone, prInFilter, prInQuery, sessionInFilter, sessionInQuery, toggleShown, useBoardLists } from './boardLists';
import { inProject } from './quick';
import { setPageZoom, type ZoomStep } from './zoom';

/** No worktrees yet, as one list, so Settings doesn't see a new one on every render. */
const NO_LANES: Lane[] = [];
const NO_TEAM_FILES: TeamFile[] = [];

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
const DISMISSED_KEY = 'codealong.dismissedNotices';
const SESSION_SORT_KEY = 'codealong.sessionSort';
const SESSION_ROWS_KEY = 'codealong.sessionRows';
const SIDEBAR_WIDTH_KEY = 'codealong.sidebarWidth';
const SIDEBAR_COLLAPSED_KEY = 'codealong.sidebarCollapsed';
const INSPECTOR_WIDTH_KEY = 'codealong.inspectorWidth';
const PR_PANEL_WIDTH_KEY = 'codealong.prPanelWidth';
const TODO_PANEL_WIDTH_KEY = 'codealong.todoPanelWidth';
const SESSION_PANEL_WIDTH_KEY = 'codealong.sessionPanelWidth';
const SESSION_PANEL_MAX_KEY = 'codealong.sessionPanelMax';
const PR_PANEL_MAX_KEY = 'codealong.prPanelMax';
const HELPERS_SHOWN_KEY = 'codealong.helpersShown';
// Each window has its own way back.
const HISTORY_KEY = WINDOW_PROJECT ? `codealong.sessionHistory:${WINDOW_PROJECT}` : 'codealong.sessionHistory';
const DAY = 24 * 60 * 60 * 1000;

type View = 'worktrees' | 'sessions' | 'prs' | 'todo' | 'canvas';
const VIEWS: { id: View; label: string; icon: string }[] = [
  { id: 'sessions', label: 'Sessions', icon: 'terminal' },
  { id: 'worktrees', label: 'Worktrees', icon: 'folder' },
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
/** No rows, as one list, so what's keyed on the rows shown doesn't see a new one on every render. */
const NO_ROWS: Row[] = [];

/** What selecting a worktree on the board selects: its first agent's line, else the worktree. */
function boardRowKey(lane: Lane): string {
  return rowItems(worktreeRow(lane))[0].key;
}

/** A row's element id, for the list's active descendant. From its key, not its place in the
 *  list, so a row keeps its id when others move and needn't render again. */
function domIdOf(key: string): string {
  return `row-${key}`;
}
function domId(r: { key: string }): string {
  return domIdOf(r.key);
}

// How each remembered choice is read back: one of the ids it can be, else the default.
const VIEW_STORED = oneOf(VIEWS.map((v) => v.id));
const AGE_STORED = oneOf(AGES.map((a) => a.id));
const SORT_STORED = oneOf(SESSION_SORTS.map((o) => o.id));
const ROWS_STORED = oneOf(SESSION_ROWS.map((o) => o.id));

/** A dragged panel width for the grid's style; left out, the panel takes its default from the CSS. */
const widthVar = (width: number | null) => (width == null ? undefined : `${width}px`);

export function App() {
  // The board, with changes you made that the engine hasn't shown yet on top (useBoard.ts).
  const { engine: engineBoard, board, ahead, loadError, loading } = useBoard();
  // Your other devices (src-tauri/src/peers.rs): their worktrees join this device's in every view,
  // and the sidebar's Devices narrow the views to one of them.
  const [devices, setDevices] = useState<DevicesView | null>(null);
  const [deviceFilter, setDeviceFilter] = useState<DeviceFilter>([]);
  // Other devices' worktrees, with your changes to their sessions too.
  const deviceLanes = useMemo(() => (devices?.enabled ? lanesAhead(devices.lanes, ahead) : []), [devices, ahead]);
  useEffect(() => observe(boardView(engineBoard?.generatedAt ?? null, [engineBoard?.lanes ?? [], devices?.lanes ?? []])), [engineBoard, devices]);
  useEffect(() => {
    const show = (d: DevicesView) => setDevices((prev) => (prev ? reuse(prev, d) : d));
    void getDevices().then((d) => d && show(d));
    return onDevices(show);
  }, []);
  // A project window keeps its choice to itself; the main window's is remembered.
  const [project, setProject] = useStored(WINDOW_PROJECT ? null : PROJECT_KEY, asText, WINDOW_PROJECT);
  const [bayFilter, setBayFilter] = useState<readonly Filter[]>([]);
  const [query, setQuery] = useState('');
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [view, setView] = useStored<View>(VIEW_KEY, VIEW_STORED, 'sessions');
  const [age, setAge] = useStored<Age>(AGE_KEY, AGE_STORED, 'all');
  // Panel widths as you last dragged them. The PR panel has its own: it starts out wider.
  const [sidebarWidth, setSidebarWidth] = usePanelWidth(SIDEBAR_WIDTH_KEY);
  // The sidebar folds to a rail of the views and devices (⌃⌘S), and stays as you left it.
  const [sideCollapsed, setSideCollapsed] = useStored(SIDEBAR_COLLAPSED_KEY, flag, false);
  const toggleSidebar = useCallback(() => setSideCollapsed((c) => !c), [setSideCollapsed]);
  // A session's panel in Sessions can take the whole window (⇧⌘F), for working through sessions one
  // after another with the list out of the way; it stays as you left it.
  const [panelMax, setPanelMax] = useStored(SESSION_PANEL_MAX_KEY, flag, false);
  const togglePanelMax = useCallback(() => setPanelMax((m) => !m), [setPanelMax]);
  // A PR's panel in PRs can too, for reading its description and its diffs at the window's width;
  // apart from the session's, since each view is used for its own.
  const [prPanelMax, setPrPanelMax] = useStored(PR_PANEL_MAX_KEY, flag, false);
  const togglePrPanelMax = useCallback(() => setPrPanelMax((m) => !m), [setPrPanelMax]);
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
  const [showArchived, setShowArchived] = useState(false);
  // The sessions whose finished helpers Sessions lists under them too, as their chip's
  // double-click left them; remembered across launches.
  const [helpersShownList, setHelpersShownList] = useStored(HELPERS_SHOWN_KEY, strings, [] as string[]);
  const helpersShown = useMemo(() => new Set(helpersShownList), [helpersShownList]);
  const toggleHelpers = useCallback((key: string) => setHelpersShownList((prev) => toggleShown(prev, key)), [setHelpersShownList]);
  const [showDonePrs, setShowDonePrs] = useState(false);
  // PRs being approved, by key: the button waits on GitHub.
  const [approving, setApproving] = useState<Set<string>>(() => new Set());
  const [showParked, setShowParked] = useState(false);
  // What went wrong with something you did here, what's under way, and the ways back (Toasts.tsx).
  const toasts = useToasts();
  const { setToast, track, report, setTodoUndo } = toasts;
  // What isn't working in the background (notices.ts): the engine's and this window's own,
  // less the ones dismissed, which stay dismissed across launches while they still stand.
  const reported = useReportedNotices();
  const allNotices = useMemo(() => [...(board?.notices ?? []), ...reported], [board?.notices, reported]);
  const [dismissed, setDismissed] = useStored<string[]>(DISMISSED_KEY, strings, []);
  const boardRead = !!board && !board.loading;
  useEffect(() => {
    // Before the board is read it lists none, which isn't their having gone.
    if (!boardRead) return;
    setDismissed((d) => {
      const kept = keptDismissals(d, allNotices);
      return kept.length === d.length ? d : kept;
    });
  }, [allNotices, boardRead, setDismissed]);
  const notices = useMemo(() => shownNotices(allNotices, dismissed), [allNotices, dismissed]);
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
  // File › Close idle sessions… (ui/CloseIdleDialog.tsx).
  const [closeIdle, setCloseIdle] = useState(false);
  // The worktree whose preview URL is being set (ui/PreviewBits.tsx), by id.
  const [previewFor, setPreviewFor] = useState<string | null>(null);
  // The session being renamed (ui/RenameDialog.tsx), by key.
  const [renaming, setRenaming] = useState<string | null>(null);
  useEffect(() => onCloseIdle(() => setCloseIdle(true)), []);
  // ⌘= ⌘- ⌘0 (mainKeys.ts) and View's zoom items zoom every board window: the engine saves the
  // zoom and sets it on them, and the board says what it is (zoom.ts).
  const zoom = useCallback(
    (step: ZoomStep) => {
      zoomWindows(step)
        .then((percent) => setToast(`Zoomed to ${percent}%.`))
        .catch((e: unknown) => setToast(errorText(e)));
    },
    [setToast],
  );
  useEffect(() => onZoomMenu(zoom), [zoom]);
  useEffect(() => setPageZoom(board?.zoom), [board?.zoom]);
  /** Marks sessions seen, shown at once; the engine turning it down puts them back and says why. */
  const markSeen = useCallback(
    (keys: string[]) => {
      seenAhead(keys).catch((e: unknown) => setToast(errorText(e)));
    },
    [setToast],
  );
  const now = useNow(15_000);
  const searchRef = useRef<HTMLInputElement>(null);
  const gPending = useRef(false);

  useEffect(() => {
    const onFocus = () => void refresh();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, []);

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
  }, [setToast]);

  const pickProject = useCallback(
    (name: string) => {
      setProject(name);
      setBayFilter([]);
      setShowParked(false);
      setSelectedKey(null);
    },
    [setProject],
  );

  // Pinned sessions head the Sessions view and pinned worktrees the Worktrees view, each in
  // your order.
  const sessionPins = usePins(board?.pinned, setPinned, setToast);
  const lanePins = usePins(board?.pinnedWorktrees, setPinnedWorktrees, setToast);
  const prPins = usePins(board?.pinnedPrs, setPinnedPrs, setToast);
  // How Sessions orders the sessions that aren't pinned: by status, or the newest start first.
  const [sessionSort, setSessionSort] = useStored<SessionSort>(SESSION_SORT_KEY, SORT_STORED, 'status');
  // How much each session's row says: two lines, or one.
  const [sessionLines, setSessionLines] = useStored<SessionRows>(SESSION_ROWS_KEY, ROWS_STORED, 'two');

  // The age filter: worktrees and sessions active since the cutoff. Pinned worktrees always
  // show on the board and in the sidebar, and pinned sessions in Sessions.
  const ageMs = AGES.find((a) => a.id === age)?.ms ?? 0;
  const cutoff = ageMs > 0 ? now - ageMs : 0;
  const since = (AGES.find((a) => a.id === age)?.title ?? '').replace(/^Active /, '');

  // The Todo view: the project's todos as the engine has them, with this window's changes on top
  // until the engine's board shows them.
  const todoStore = useTodoStore(board, setToast);
  const [showDoneTodos, setShowDoneTodos] = useState(false);
  const [labelFilter, setLabelFilter] = useState<string | null>(null);

  // What each view lists, from the board, the filters and the search (boardLists.ts).
  const {
    projects,
    current,
    lanes,
    projectLanes,
    deviceNeeds,
    deviceWorking,
    archivedItems,
    recentLanes,
    shownSessionCount,
    pinnedRows,
    pinnedHelpers,
    sessionSecs,
    sessionRows,
    waitingRows,
    pinnedLaneRows,
    bays,
    shownBays,
    foldParked,
    parkedCount,
    worktreeRows,
    worktreeItems,
    laneById,
    prRowOf,
    prRows,
    pinnedPrRows,
    prSecs,
    prVisible,
    openPrCount,
    requestedCount,
    homes,
    labelById,
    projectTodos,
    todoItems,
    todoSecs,
    todoVisible,
    openTodoCount,
    todoLinks,
    todosFor,
    helpersOf,
    counts,
    totalLanes,
    actions,
  } = useBoardLists({
    board,
    deviceLanes,
    devices,
    deviceFilter,
    project,
    loading,
    cutoff,
    since,
    now,
    query,
    filter: bayFilter,
    sessionPins,
    helpersShown,
    lanePins,
    prPins,
    sessionSort,
    showParked,
    showArchived,
    showDonePrs,
    todos: todoStore.todos,
    labels: todoStore.labels,
    labelFilter,
    showDoneTodos,
  });
  // New session opens the quick panel (ui/Quick.tsx) at a worktree, maybe for a todo, in the
  // project this window shows. The panel tells this window how the start goes.
  const projectNow = useLatest(current);
  const newSession = useCallback(
    (at: { laneId: string | null; todo?: string | null; todos?: string[]; prompt?: string }) => {
      // The quick panel starts sessions on this device; one on another device's worktree starts without one.
      const laneId = at.laneId && shortOf(at.laneId) ? null : at.laneId;
      void showQuick({ ...at, laneId, project: projectNow.current, opener: windowLabel() });
    },
    [projectNow],
  );

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
  // Picking a status, or pressing Return in the filter, brings what matches into view.
  const [canvasFit, setCanvasFit] = useState(0);
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

  // Nothing is listed until the board is read, so nothing can be selected: not the other devices'
  // rows that come first.
  const visible = loading ? NO_ROWS : view === 'sessions' ? sessionRows : view === 'prs' ? prVisible : view === 'todo' ? todoVisible : view === 'canvas' ? NO_ROWS : worktreeItems;
  // A session listed under another worktree now (its commands moved there) stays selected. When
  // the selected agent's line goes (you marked it seen, it stopped), the selection stays
  // on its worktree rather than jumping to the top. A todo that leaves the list (marked done
  // while Done is folded, or no longer matching the search or a filter) stays in the panel: the
  // next key you press is still about the todo you see, and an edit in progress isn't cut off. A
  // PR that leaves it stays the same way: one you merge moves under Merged & closed, folded,
  // as you confirm it (ahead.ts `mergeAhead`), and its panel says how the merge went.
  const [selectedLaneId, selectedSessionKey] = selectedKey?.split('::') ?? [];
  const selected =
    visible.find((r) => r.key === selectedKey) ??
    (view === 'todo'
      ? todoItems.find((r) => r.key === selectedKey)
      : view === 'prs'
        ? prRows.find((r) => r.key === selectedKey)
        : (visible.find((r) => r.kind === 'agent' && r.session.key === selectedSessionKey) ?? visible.find((r) => r.kind !== 'pr' && r.kind !== 'todo' && r.lane.id === selectedLaneId))) ??
    visible[0] ??
    null;
  // With nothing picked yet (the window just opened, or you changed projects) the panel shows
  // the top row, and it holds there: a session that comes to the top later doesn't take it.
  // A click in the moment before this runs has picked a row already: it isn't taken back.
  useEffect(() => {
    const top = selected?.key;
    if (selectedKey === null && top !== undefined) setSelectedKey((k) => k ?? top);
  }, [selectedKey, selected?.key]);
  const selectedLane = selected?.lane ?? null;
  // The Todo view's picks and its box, and what's done to todos from here (useTodoView.ts).
  const {
    duePick,
    duePicked,
    pickedRows,
    pickedKeys,
    pickAll: pickAllTodos,
    clearPick: clearTodoPick,
    composer,
    setComposer,
    composerRef,
    createTodo,
    selectTodoRow,
    showTodo,
    startForTodo,
    startForTodos,
    projectsForTodos,
    todoMenu,
    todoMenuSpec,
    pickMenu,
    todoMoreMenu,
    todoLinkMenu,
    labelMenu,
    moveTodo,
    todoMenuItems,
    todoKeys,
  } = useTodoView({
    store: todoStore,
    view,
    selected,
    setSelectedKey,
    visible,
    todoVisible,
    todoSecs,
    current,
    projects,
    lanes,
    laneById,
    lanePins,
    selectedLane,
    labelFilter,
    setLabelFilter,
    setShowDoneTodos,
    report,
    setToast,
    setTodoUndo,
    newSession,
    // Read when a menu runs: the window's opener is made further down, with what it reveals.
    open: (lane, targetId) => open(lane, targetId),
    pickProject,
    setView,
    setBayFilter,
    setQuery,
    onDesktop,
    canvasItem,
  });
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

  useEffect(() => {
    const base = WINDOW_PROJECT ? `Codealong · ${current}` : 'Codealong';
    document.title = counts.needs > 0 ? `${base} · ${counts.needs} need you` : base;
  }, [counts.needs, current]);

  // Keyed on the row's key, which is all its id is made of: the row itself is a new object on
  // every push, and scrolling then would pull the list back from where you'd scrolled it.
  useEffect(() => {
    const key = selected?.key;
    if (key !== undefined) document.getElementById(domIdOf(key))?.scrollIntoView({ block: 'nearest' });
  }, [selected?.key]);

  // The floating bar, the quick panel or a card on the desktop asks to show a session, worktree,
  // PR or todo here: in its project, with no search hiding it, nor one of its sessions shown in
  // its panel's place (`drill`), which selecting what's selected already would leave.
  const reveal = useLatest((r: Reveal) => {
    // A todo the quick panel found: the Todo view, in its project.
    if (r.kind === 'todo') {
      const t = todoStore.todos.find((x) => x.id === r.id);
      if (t) {
        setDrill(null);
        showTodo(t);
      }
      return;
    }
    const lane = r.kind === 'session' || r.kind === 'worktree' ? lanes.find((l) => l.id === r.laneId) : undefined;
    const session = lane?.sessions.find((s) => s.key === (r.kind === 'session' ? r.key : ''));
    // An archived session isn't in the lists the views show (`unarchived`): it's in Sessions'
    // Archived fold, so it's looked for on the board as the engine sent it.
    const archivedLane = r.kind === 'session' && !session ? [...(board?.lanes ?? []), ...deviceLanes].find((l) => l.id === r.laneId) : undefined;
    const archived = archivedLane?.sessions.find((s) => s.key === (r.kind === 'session' ? r.key : '') && s.archived);
    const pr = r.kind === 'pr' ? board?.prs.find((p) => p.key === r.key) : undefined;
    const home = lane?.project ?? archivedLane?.project ?? pr?.project;
    if (!home) return;
    if (current !== ALL_PROJECTS && current !== home) pickProject(home);
    setQuery('');
    setDrill(null);
    if (lane && r.kind === 'worktree') {
      focusLane(lane);
    } else if (archivedLane && archived) {
      // Its row in the Archived fold, opened, with its conversation in the panel; the age
      // filter doesn't apply there.
      setView('sessions');
      setBayFilter([]);
      setShowArchived(true);
      setSelectedKey(`${archivedLane.id}::${archived.key}`);
    } else if (lane && session) {
      // Its own row in Sessions, with its conversation in the panel, whichever view was showing:
      // the board would only show its line on its worktree's row.
      setView('sessions');
      setBayFilter([]);
      // One older than the age filter lets through: the filter widens as far as it, as for a PR.
      const wider = ageListingSession(session, sessionPins.set, AGES, age, Date.now());
      if (wider) setAge(wider);
      setSelectedKey(`${lane.id}::${session.key}`);
    } else if (pr) {
      setView('prs');
      setBayFilter([]);
      if (prInDone(pr, prPins.set)) setShowDonePrs(true);
      // One older than the age filter lets through: the filter widens as far as it.
      const wider = ageListingPr(pr, prPins.set, AGES, age, Date.now());
      if (wider) setAge(wider);
      setSelectedKey(`pr:${pr.key}`);
    }
  });

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
  }, [report, markSeen, setView, reveal]);

  const openRow = useCallback(
    (row: Row | null) => {
      if (!row) return;
      if (row.kind === 'pr') {
        openUrl(row.pr.url).catch((e: unknown) => setToast(String(e)));
        return;
      }
      if (row.kind === 'todo') {
        // A todo opens its agent where it lives: its primary session while that runs, else the
        // live one, else the latest. Without one, it's time to start one.
        const s = todoAgent(row);
        if (s) open(s.lane, s.session.openTarget);
        else newSession({ laneId: row.lane?.id ?? null, todo: row.todo.id, prompt: todoPrompt(row.todo, itemPrimaryHolds(row, Date.now())) });
        return;
      }
      open(row.lane, row.kind === 'agent' ? row.session.openTarget : row.lane.defaultTarget);
    },
    [open, newSession, setToast],
  );

  /** Approves a PR as you, in one click. */
  const approve = useCallback((pr: PrItem) => {
    setApproving((a) => new Set(a).add(pr.key));
    reviewPr(pr.repoSlug, pr.number, 'approve', '')
      .then((done) => setToast(reviewedWords('approve', pr.number, done)))
      .catch((e: unknown) => setToast(errorText(e)))
      .finally(() =>
        setApproving((a) => {
          const next = new Set(a);
          next.delete(pr.key);
          return next;
        }),
      );
  }, [setToast]);

  /** Approves several PRs as you, one after another, and says how it went. */
  const approveAll = useCallback(
    (prs: PrItem[]) => {
      const keys = prs.map((p) => p.key);
      setApproving((a) => new Set([...a, ...keys]));
      const wrong: string[] = [];
      let approved = 0;
      const each = prs.reduce<Promise<void>>(
        (before, p) =>
          before.then(() =>
            reviewPr(p.repoSlug, p.number, 'approve', '').then(
              () => void approved++,
              (e: unknown) => void wrong.push(`Couldn’t approve #${p.number}: ${errorText(e).replace(/\.$/, '')}.`),
            ),
          ),
        Promise.resolve(),
      );
      track(`Approving ${count(prs.length, 'PR')}…`, each)
        .then(() => setToast([approved > 0 ? `Approved ${count(approved, 'PR')}.` : '', ...wrong].filter(Boolean).join(' ')))
        .finally(() =>
          setApproving((a) => {
            const next = new Set(a);
            for (const k of keys) next.delete(k);
            return next;
          }),
        );
    },
    [setToast, track],
  );

  const copy = useCallback((text: string | null | undefined, what: string) => {
    if (!text) return;
    navigator.clipboard
      .writeText(text)
      .then(() => setToast(`Copied the ${what}.`))
      .catch(() => setToast(`Couldn't copy the ${what}.`));
  }, [setToast]);

  const move = useCallback(
    (delta: number) => {
      if (visible.length === 0) return;
      const i = selected ? visible.indexOf(selected) : -1;
      const next = visible[Math.min(visible.length - 1, Math.max(0, i + delta))];
      if (next) setSelectedKey(next.key);
    },
    [visible, selected],
  );

  // Working through what waits on you from a session's panel in Sessions (useWaitingQueue.ts,
  // which the notch and the quick panel go by too): ⌘⏎ in its box sends (or allows the call it
  // waits on) and goes on to the next listed that waits on you, ⌘E marks a finished turn seen and
  // goes on, and ⌘] (or ⌥⇧↓) skips to it, ⌘[ (or ⌥⇧↑) back; a skipped one comes round again after the
  // rest. The ones dealt with stay out until they wait on you anew, since an allowed call isn't
  // shown ahead of the board.
  // The row gone on to, whose box takes the cursor.
  const [boxFocus, setBoxFocus] = useState<string | null>(null);
  // What differs here: ⌘E is this window's to run (it marks seen in every view), a session that
  // leaves what waits on you without having waited stays selected (archiving down a list of old
  // sessions doesn't jump to one that needs you), and ⌥↑ ⌥↓ and ⌃- ⌃⇧- are the window's own,
  // which work in every view (`move`, `travel`). The one gone on to takes the cursor in its box
  // unless the caller says the keyboard stays where it is.
  const queue = useWaitingQueue<SessionItem, boolean>({
    waiting: waitingRows,
    reads: SESSION_READS,
    shown: view === 'sessions' && selected?.kind === 'agent' ? queueShown(selected.session) : null,
    show: (row, box = true) => {
      if (box) setBoxFocus(row.key);
      setSelectedKey(row.key);
    },
    markSeen: (key) => markSeen([key]),
    leftStays: true,
  });
  const { run: runQueue, left: leftQueue, say: sayQueue } = queue;
  /** A session's menu, its chords or the strip's buttons took it off what waits on you (snoozed,
   *  closed, archived, marked seen): from the one shown in Sessions, on to the next that waits, as
   *  ⌘E goes, the keyboard staying where it was. */
  const leftAndOn = (s: Session, done: menus.Left) => void leftQueue(s.key, done, !!document.activeElement?.closest('.ca-sessinsp .ca-composer'));
  // Only when the selection moves: the row gone on to keeps its claim on the cursor until then.
  useEffect(() => {
    const key = selected?.key;
    setBoxFocus((box) => (box && key !== box ? null : box));
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
  // Where ⌃` opens a terminal (mainKeys.ts): in the folder of that session, or of the session whose
  // line is selected on the board, else of the worktree a new session would start in.
  const terminalAt = useMemo(() => {
    const home = shownSession ? homes.get(shownSession) : selected?.kind === 'agent' ? selected : undefined;
    if (home) return { lane: home.lane, session: home.session };
    const lane = startLaneId ? laneById.get(startLaneId) : undefined;
    return lane ? { lane, session: null } : null;
  }, [shownSession, homes, selected, startLaneId, laneById]);
  // One that's gone, archived, older than the sidebar's age filter shows or in a worktree no
  // project lists is passed over: the list wouldn't have it.
  const sessionHistory = useSessionHistory(shownSession, (key) => {
    const home = homes.get(key);
    return !!home && (!home.lane.hidden || inProject(home.lane, current)) && (home.session.lastActivity >= cutoff || sessionPins.set.has(key));
  }, HISTORY_KEY);
  /** Goes back or forward a session. It shows in Sessions: where it's listed, when the list has it
   *  as it's filtered now, else as a session the bar asks for is shown. From a session's box, the
   *  cursor goes to the box of the one gone to. */
  const travel = useLatest((dir: 'back' | 'forward', box: boolean) => {
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
    sayQueue(null);
    if (box) setBoxFocus(`${home.lane.id}::${key}`);
  });
  /** What a history button says: where it goes, and its key. */
  const historyTitle = (to: string | null, word: string, none: string, keys: string) => {
    const title = to ? homes.get(to)?.session.title : null;
    return `${title ? `${word} to ${middleTruncate(title, 60)}` : none} (${keys})`;
  };

  /** Archives sessions (Codex threads in Codex itself), or unarchives them; archived, they can
   *  be put back from the toast. `title` is what it says of one. */
  const setArchivedUndo = toasts.setArchived;
  const setArchived = useCallback(
    (keys: string[], archived: boolean, title: string) => {
      setArchivedUndo(archived ? { keys, title, message: keys.length > 1 ? `Archived ${keys.length} sessions.` : undefined } : null);
      Promise.all(keys.map((key) => archiveAhead(key, archived))).catch((e: unknown) => {
        setArchivedUndo(null);
        setToast(errorText(e));
      });
    },
    [setArchivedUndo, setToast],
  );

  // Rows picked together in Sessions, on the Worktrees board and in PRs, and what's done to all
  // of them (useListPicks.ts); the Todo view's are its own.
  const picks = useListPicks({
    view,
    loading,
    selected,
    setSelectedKey,
    sessionRows,
    worktreeRows,
    prVisible,
    showArchived,
    report,
    setToast,
    sessionPins,
    lanePins,
    prPins,
    archive: setArchived,
    undoArchive: setArchivedUndo,
    approve: approveAll,
  });


  // Where each account can start a session, for Review with: read at launch, and again each time
  // the menu is built, so a menu goes by the last answer and never waits on one.
  const startOpts = useRef<StartOptions | null>(null);
  const loadStartOpts = useCallback(() => {
    startOptions()
      .then((o) => {
        startOpts.current = o;
        clearNotice(START_OPTIONS_NOTICE);
      })
      // Keeps the last answer, and says so until a read works.
      .catch((e: unknown) => reportNotice(startOptionsFailed(e)));
  }, []);
  useEffect(loadStartOpts, [loadStartOpts]);
  /** Review with…'s items for a PR: a round of Codealong's own reviews on an account, or its
   *  review page in the Codex app (review.ts). */
  const reviewItems = (pr: Pick<PrItem, 'number' | 'url' | 'repoSlug' | 'lanes'> & Partial<Pick<PrItem, 'key' | 'mine'>>): MenuAction[] => {
    loadStartOpts();
    return reviewItemsFor(pr, lanes, startOpts.current?.accounts ?? [], board?.usage ?? [], { error: setToast, info: setToast, busy: track }, undefined, Date.now(), repoReviews(board, pr.repoSlug));
  };
  /** A worktree's PR as Review with… needs it: the board's, or what the worktree knows of one
   *  the board doesn't list (on another Mac, say). */
  const reviewedPr = (lane: Lane, p: Pr) => board?.prs.find((x) => x.url === p.url) ?? { ...p, repoSlug: lane.repoSlugNow ?? lane.repoSlug ?? '', lanes: [lane.id] };
  /** Drops Review with… from a button, as the PR panels have it. */
  const reviewMenuFrom = (pr: Pick<PrItem, 'number' | 'url' | 'repoSlug' | 'lanes'> & Partial<Pick<PrItem, 'key' | 'mine'>>, el: Element) => {
    const items = reviewItems(pr);
    const spec = items.length > 0 ? items : [{ label: 'No account can start a review here', run: () => {}, enabled: false }];
    void showContextMenu(spec, below(el)).catch((e: unknown) => setToast(String(e)));
  };

  /** Snooze review, for a PR that asks for your review: the times it can wait, and Wake now. */
  const snoozeReview = (p: PrItem) => reviewSnoozeMenu(p, Date.now(), (act) => void act.catch((e: unknown) => setToast(errorText(e))));
  /** Drops Snooze review's times from a button, as the PR panel has it. */
  const snoozeMenuFrom = (p: PrItem, el: Element) => {
    const menu = snoozeReview(p);
    if (menu) void showContextMenu(menu.items, below(el)).catch((e: unknown) => setToast(String(e)));
  };

  /** A PR's right-click menu, as every window has it (menus.ts), with this window's own. */
  const prMenuSpec = (r: PrRowItem): MenuSpec => {
    const p = r.pr;
    const lane = r.lane;
    return menus.prMenu(p, {
      report,
      review: reviewItems(p),
      approve: mayApprove(p) ? () => approve(p) : undefined,
      snooze: snoozeReview(p),
      showLane: lane ? { lane, run: () => focusLane(lane) } : undefined,
      pin: { label: prPins.set.has(p.key) ? 'Unpin PR' : 'Pin PR', run: () => prPins.toggle(p.key), accelerator: 'P' },
      desktop: onDesktop,
      canvas: canvasItem('pr', p.key),
    });
  };
  const prMenu = (r: PrRowItem, at?: { x: number; y: number }) => void showContextMenu(prMenuSpec(r), at).catch((e: unknown) => setToast(String(e)));
  /** Opens a worktree's PR on GitHub: a double-click on its chip. */
  const openLanePr = (lane: Lane) => {
    if (lane.pr) openUrl(lane.pr.url).catch((e: unknown) => setToast(String(e)));
  };
  /** Shows a PR in PRs, with its panel: a click on its chip, wherever this window shows one. A PR
   *  the board doesn't list has no panel here, so it opens on GitHub instead. */
  const showPrOf = (p: Pr) => {
    const item = board?.prs.find((x) => x.url === p.url);
    if (item) reveal.current({ kind: 'pr', key: item.key });
    else openUrl(p.url).catch((e: unknown) => setToast(String(e)));
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
  const todoItemsRef = useLatest(todoMenuItems);
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
      worktrees: board?.lanes,
      archive: (archived) => {
        if (archived) leftAndOn(s, 'archived');
        setArchived([s.key], archived, s.title);
      },
      // Undo takes it out of the archive; a terminal it closed stays closed, and it resumes as any other.
      undoDone: (message) => setArchivedUndo({ keys: [s.key], title: s.title, message }),
      left: (done) => leftAndOn(s, done),
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
      automation: (repo) => openSettings('automation', repo),
      todos: todoItemsRef.current(lane.project, { worktree: lane.id }, { worktree: lane.id, label: lane.callsign }),
      pin: { label: lanePins.set.has(lane.id) ? 'Unpin worktree' : 'Pin worktree', run: () => toggleLanePin(lane) },
      desktop: onDesktop,
      canvas: canvasItem('worktree', lane.id),
  });
  const laneMenuSpec = (lane: Lane): MenuSpec => menus.laneMenu(lane, laneCan(lane));
  // The keys run the same actions, as this render has them.
  const actionsRef = useLatest({ session: (lane: Lane, s: Session, pin: 'session' | 'worktree') => menus.sessionActions(lane, s, sessionCan(lane, s, pin)), lane: (lane: Lane) => menus.laneActions(lane, laneCan(lane)) });
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

  useEffect(() => onReveal((r) => reveal.current(r)), [reveal]);


  useMainKeys({
    view,
    views: VIEWS,
    settings,
    previewFor,
    renaming,
    panelMax,
    selected,
    visible,
    pickedRows,
    // The Todo view's keys act on its picks themselves (useTodoView.ts): ⌘A and Esc are the lists' own.
    listPicks: picks.keys ?? (view === 'todo' ? { count: pickedRows.length, step: () => {}, all: pickAllTodos, clear: clearTodoPick, actions: () => null } : null),
    selectedLane,
    startLaneId,
    terminalAt,
    projects,
    pinnedRows,
    pinnedLaneRows,
    pinnedPrRows,
    shownBays,
    sessionPins,
    lanePins,
    prPins,
    searchRef,
    listRef,
    canvasKeys,
    gPending,
    todoKeys,
    travel,
    actionsRef,
    openSettings,
    runQueue,
    toggleSidebar,
    pickProject,
    startForTodos,
    startForTodo,
    newSession,
    setQuery,
    setCanvasFit,
    openRow,
    setView,
    togglePanelMax,
    togglePrPanelMax,
    movePinnedSession,
    movePinnedLane,
    move,
    focusBox,
    open,
    copy,
    say: setToast,
    markSeen,
    setSelectedKey,
    zoom,
  });

  const toggleBay = (f: Filter) => {
    setBayFilter((fs) => (fs.includes(f) ? fs.filter((x) => x !== f) : [...fs, f]));
    if (view === 'canvas' && !bayFilter.includes(f)) setCanvasFit((n) => n + 1);
  };
  const toggleDevice = (id: string) => setDeviceFilter((d) => (d.includes(id) ? d.filter((x) => x !== id) : [...d, id]));
  const openWindow = (name: string, another = false) => {
    openProjectWindow(name, another).catch((e: unknown) => setToast(String(e)));
  };
  // File › New window (⌘⇧N): one more window on the project this one shows.
  const newWindow = useLatest(() => openWindow(current, true));
  useEffect(() => onNewWindow(() => newWindow.current()), [newWindow]);
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
          return !!lane && laneInQuery(lane, query) && !!narrowed(lane, bayFilter, since);
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
    [canvasEntities, todoItems, query, bayFilter, since, prRowOf],
  );

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
    else {
      // From a session shown in this todo's own panel: back to the todo.
      setDrill(null);
      showTodo(t);
    }
  };

  // The canvas's selected card, which the panel on the right shows on the canvas.
  const canvasPick = view === 'canvas' ? canvasPicked(canvasSel, canvasEntities, todoItems) : null;

  /** A worktree's row on the board: `index` is its place among the pinned ones. */
  const laneRow = (r: WorktreeRow, index: number | null, drag: DragProps) => (
    <LaneRow
      key={r.key}
      row={r}
      now={now}
      selectedKey={selected?.key ?? null}
      picked={picks.worktrees.keys.has(r.key)}
      domId={domIdOf}
      // A press with ⌘ or ⇧ picks the whole worktree, whichever part of its row it's on.
      onSelect={(key, how) => {
        if (how !== 'one') return picks.worktrees.select(r, how);
        picks.worktrees.clear();
        setSelectedKey(key);
      }}
      onOpen={(lane) => open(lane, lane.defaultTarget)}
      onOpenTarget={open}
      pinned={index != null}
      // On a worktree picked with others, the menu is for all of them.
      onContextMenu={() => (picks.worktrees.keys.has(r.key) ? picks.worktrees.menu() : laneMenu(r.lane))}
      onAgentMenu={(s) => (picks.worktrees.keys.has(r.key) ? picks.worktrees.menu() : sessionMenu(r.lane, s, 'worktree'))}
      onPrMenu={() => lanePrMenu(r.lane)}
      onPrShow={() => r.lane.pr && showPrOf(r.lane.pr)}
      onPrOpen={() => openLanePr(r.lane)}
      todosFor={todosFor}
      onShowTodo={showTodo}
      drag={drag}
    />
  );

  // Which panel is on the right, for its width: the view's own, or on the canvas the selected card's.
  const panelKind = view === 'canvas' ? (canvasPick?.kind ?? null) : view === 'prs' ? 'pr' : view === 'todo' ? 'todo' : view === 'sessions' && selected?.kind === 'agent' ? 'session' : 'lane';
  const panelWidth = { pr: prPanelWidth, todo: todoPanelWidth, session: sessionPanelWidth, lane: inspectorWidth };
  const setPanelWidth = { pr: setPrPanelWidth, todo: setTodoPanelWidth, session: setSessionPanelWidth, lane: setInspectorWidth };
  // The panel takes the whole window: one session's in Sessions, one PR's in PRs (⇧⌘F).
  const maxed = (panelKind === 'session' && panelMax && picks.sessions.rows.length === 0) || (view === 'prs' && selected?.kind === 'pr' && prPanelMax && picks.prs.rows.length === 0);
  // The worktree the preview URL sheet is for, as the latest board has it.
  const previewLane = previewFor ? board?.lanes.find((l) => l.id === previewFor) : undefined;
  // The session the rename sheet is for, in its own worktree.
  const renamed = useMemo(() => homeOf(board?.lanes ?? [], renaming), [renaming, board]);

  // The sidebar: the views with their counts, the statuses to narrow them to, and what narrows
  // the view now, for the collapsed rail's badge.
  const sideViews = VIEWS.map((v, i) => ({
    id: v.id,
    label: v.label,
    icon: v.icon,
    kbd: menus.chordKeys(viewChord(i)),
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
  }, board);

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
            title={historyTitle(sessionHistory.back, 'Back', 'Back to the session you looked at before', menus.chordSigns(QUEUE_CHORDS.historyBack[0]))}
            onClick={() => travel.current('back', false)}
          >
            <Icon name="chevron-left" size={16} />
          </button>
          <button
            type="button"
            aria-label="Forward"
            aria-disabled={!sessionHistory.forward}
            title={historyTitle(sessionHistory.forward, 'Forward', 'Forward again, after going back', menus.chordSigns(QUEUE_CHORDS.historyForward[0]))}
            onClick={() => travel.current('forward', false)}
          >
            <Icon name="chevron-right" size={16} />
          </button>
        </div>
        <ProjectSwitcher logoStatus={logoStatus} projects={projects} current={current} total={totalLanes} actions={actions} onPick={pickProject} onOpenWindow={switcherWindow} onAddProject={() => openSettings('projects', NEW_PROJECT)} onEditProjects={() => openSettings('projects')} />
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
          <Seg
            className="ca-barplace"
            label="Agents bar"
            toggles
            look="icon"
            iconSize={15}
            options={[
              { id: 'floating', label: 'Floating bar', icon: 'capsule-horizontal', title: barPlace === 'floating' ? 'Hide the floating bar' : 'Floating bar: the agents that need you, on top of every window, where you put it' },
              { id: 'notch', label: 'Bar in the notch', icon: 'notch', title: barPlace === 'notch' ? 'Hide the bar in the notch' : 'In the notch: the agents that need you, around the notch at the top of the screen, opening when you point at it' },
            ]}
            value={barPlace === 'off' ? null : barPlace}
            onPick={pickBarPlace}
          />
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
        notices={<Notices notices={notices} now={now} onDismiss={(n) => setDismissed((d) => [...d, noticeId(n)])} onOpen={() => openSettings('file')} />}
      />
      <div
        className={cx('ca-win-body', sideCollapsed && 'is-railed', panelKind === 'pr' && 'is-prs', panelKind === 'todo' && 'is-todo', panelKind === 'session' && 'is-session', maxed && 'is-panel-max', view === 'canvas' && 'is-canvas', view === 'canvas' && canvasPick && 'has-panel')}
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
          ages={AGES}
          age={age}
          ageApplies={ageApplies}
          onAge={(id) => setAge(id as Age)}
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
        />
        {sideCollapsed ? <span aria-hidden="true" /> : <Resizer panel="before" cssVar="--sidebar-w" onResize={setSidebarWidth} />}
        <main className={view === 'canvas' ? 'ca-board is-canvas' : 'ca-board'} aria-label={VIEWS.find((v) => v.id === view)?.label}>
          {view === 'sessions' && <SessionsBar sort={sessionSort} onSort={setSessionSort} lines={sessionLines} onLines={setSessionLines} />}
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
            <div className="ca-board-empty ca-board-loading">
              <div className="ca-board-reading" role="status">
                <StatusGlyph status="working" size={12} title="" />
                <span>{READING[view]}</span>
              </div>
              <Tip place="board" />
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
              onShowPr={showPrOf}
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
            aria-multiselectable
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
                onShowPr={showPrOf}
                onOpenPr={(p) => openUrl(p.url).catch((e: unknown) => setToast(String(e)))}
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
                picked={picks.prs.keys}
                domId={domId}
                showDone={showDonePrs}
                onToggleDone={() => setShowDonePrs(!showDonePrs)}
                onSelect={picks.prs.select}
                onOpen={(r) => {
                  picks.prs.select(r, 'one');
                  openRow(r);
                }}
                onOpenSession={(lane, session) => open(lane, session.openTarget)}
                // On one of the PRs picked, the menu is for all of them.
                onContextMenu={(r) => (picks.prs.keys.has(r.key) ? picks.prs.menu() : prMenu(r))}
              />
            )}
            {view === 'sessions' && (
              <SessionsView
                pinned={pinnedRows}
                pinnedHelpers={pinnedHelpers}
                sections={sessionSecs}
                sort={sessionSort}
                lines={sessionLines}
                now={now}
                selectedKey={selected?.key ?? null}
                picked={picks.sessions.keys}
                domId={domId}
                onSelect={(r, how) => {
                  picks.sessions.select(r, how);
                  if (how === 'one') focusBox();
                }}
                onOpen={(r) => {
                  picks.sessions.select(r, 'one');
                  openRow(r);
                }}
                onContextMenu={(r) => {
                  // On one of the sessions picked, the menu is for all of them; on another, for
                  // that one, and the selection stays where it was.
                  if (picks.sessions.keys.has(r.key)) picks.sessions.menu();
                  else sessionMenu(r.lane, r.session, 'session');
                }}
                onPrMenu={(r) => lanePrMenu(r.lane)}
                onPrShow={(r) => r.lane.pr && showPrOf(r.lane.pr)}
                onPrOpen={(r) => openLanePr(r.lane)}
                onPlacePinned={(key, index) => sessionPins.place(pinnedRows.map((r) => r.session.key), key, index)}
                onUnpin={sessionPins.remove}
                desktop={(r) => (r.lane.device || r.session.archived ? null : { ref: 'session', key: r.session.key })}
                archived={archivedItems}
                showArchived={showArchived}
                onToggleArchived={() => setShowArchived(!showArchived)}
                todosFor={todosFor}
                helpersOf={helpersOf}
                helpersShown={helpersShown}
                onToggleHelpers={toggleHelpers}
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
              shownBays.map((b) => {
                const count = bays.find((x) => x.bay === b.bay)?.rows.length ?? b.rows.length;
                const unreadKeys = b.bay === 'unread' ? b.rows.flatMap((r) => r.lines.filter((a) => a.session.status === 'unread').map((a) => a.session.key)) : [];
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
                      {b.bay === 'idle' && foldParked && parkedCount > 0 && (
                        <button type="button" className="ca-parked" onClick={() => setShowParked(true)}>
                          Show {parkedCount} parked {parkedCount === 1 ? 'worktree' : 'worktrees'}
                        </button>
                      )}
                    </div>
                  </section>
                );
              })}
          </div>
          )}
        </main>
        {panelKind && <Resizer panel="after" cssVar="--panel-w" onResize={setPanelWidth[panelKind]} />}
        <MainPanel
          view={view}
          selected={selected}
          selectedLane={selectedLane}
          canvasPicked={canvasPick}
          board={board}
          now={now}
          loading={loading}
          lanes={lanes}
          laneById={laneById}
          homes={homes}
          devices={devices}
          current={current}
          projectTodos={projectTodos}
          todoStore={todoStore}
          todoLinks={todoLinks}
          todosFor={todosFor}
          prRowOf={prRowOf}
          pickedRows={pickedRows}
          picks={picks}
          todo={{ startForTodo, startForTodos, todoMenu, todoLinkMenu, todoMoreMenu, labelMenu, projectsForTodos, duePick, duePicked, selectTodoRow, pickMenu }}
          approving={approving}
          drill={drill}
          setDrill={setDrill}
          queue={queue}
          boxFocus={boxFocus}
          panelMax={panelMax}
          togglePanelMax={togglePanelMax}
          prPanelMax={prPanelMax}
          togglePrPanelMax={togglePrPanelMax}
          actionsRef={actionsRef}
          open={open}
          newSession={newSession}
          showLaneFromPanel={showLaneFromPanel}
          showSessionFromPanel={showSessionFromPanel}
          showTodoFromPanel={showTodoFromPanel}
          laneMenu={laneMenu}
          sessionMenu={sessionMenu}
          sessionMenuSpec={sessionMenuSpec}
          lanePrMenu={lanePrMenu}
          showPr={showPrOf}
          prMenu={prMenu}
          prChipMenu={prChipMenu}
          approve={approve}
          reviewMenuFrom={reviewMenuFrom}
          snoozeMenuFrom={snoozeMenuFrom}
          reviewedPr={reviewedPr}
          setPreviewFor={setPreviewFor}
          markSeen={markSeen}
          setToast={setToast}
        />
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
          teamFiles={board?.teamFiles ?? NO_TEAM_FILES}
        />
      )}
      {closeIdle && <CloseIdleDialog onClose={() => setCloseIdle(false)} onDone={setToast} />}
      {previewLane && <PreviewDialog lane={previewLane} onClose={() => setPreviewFor(null)} />}
      {renamed && <RenameDialog lane={renamed.lane} session={renamed.session} onClose={() => setRenaming(null)} onFailed={setToast} />}
      <Toasts state={toasts} onUnarchive={(u) => setArchived(u.keys, false, u.title)} />
    </div>
  );
}

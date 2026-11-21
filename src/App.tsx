// The main window. One project at a time, picked in the switcher at top left. The sidebar
// picks the view and narrows it: device, how recently active, status, and views you saved
// (Sidebar.tsx); it collapses to a rail. The main area has five views: Sessions lists every session,
// pinned ones first in your order; Worktrees lists pinned worktrees in your order, then every
// other worktree once, in Needs you, Working, Your turn, In review, Idle or Merged, with a line
// for each agent that needs you, is working or just finished; PRs lists the project's pull
// requests; Todo lists work you mean to hand an agent, and the sessions on it; the Board view
// shows one of the project's boards (boards/model.ts) as Kanban, in columns (KanbanView.tsx), or
// as Canvas, laid out in space to be remembered by where it is (CanvasView.tsx).
// The box in the middle of the title bar opens the quick panel, the way into finding and starting
// anything; ⌘F finds in the whole window, as a browser's find does (ui/WindowFind.tsx), and ⌥⌘F
// filters the view's list (ui/ListBar.tsx).
// Keyboard map: the design system's Surfaces page.

import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type Dispatch, type SetStateAction } from 'react';
import { showContextMenu, type MenuSpec } from './contextMenu';
import { useEditorIcons } from './editors';
import { movedTargets } from './lineage';
import { homeOf, runsShownKey, sidesShownKey, type BoardRow, type Filter } from './lanes';
import { answerLink, onCloseIdle, onCloseStale, onUsageMenu, onWebsiteMenu, onWindowToast, onNewWindow, onSetupMenu, onCheckUpdates, openProjectWindow, openUrl, orderAccounts, refresh, refreshUsage, setPinned, setPinnedPrs, setPinnedRepos, setPinnedWorktrees, setTheme, setupNeeded, showQuick, windowLabel, WEBSITE, type Reveal } from './api';
import { count, middleTruncate } from './format';
import * as menus from './menus';
import { prRefOf } from './prLinks';
import { archiveAhead, reviewAhead, seenAhead } from './ahead';
import { usePins } from './pins';
import { tryAgain } from './review';
import { itemPrimaryHolds, statusSection, todoAgent, todoPrompt, useTodoStore, type TodoItem, type TodoSort } from './todos';
import type { Lane, Pr, PrItem, Session, TeamFile, Theme, Todo } from './types';
import { useKeptSessions } from './kept';
import { shortOf, type DeviceFilter } from './devices';
import { NEW_PROJECT, SettingsDialog } from './ui/SettingsDialog';
import { CleanUpDialog } from './ui/CleanUpDialog';
import { CloseIdleDialog } from './ui/CloseIdleDialog';
import { CloseStaleDialog } from './ui/CloseStaleDialog';
import { SetupDialog } from './ui/SetupDialog';
import { UsageDialog } from './ui/UsageDialog';
import { PreviewDialog } from './ui/PreviewBits';
import { RenameDialog } from './ui/RenameDialog';
import { SnoozeDialog } from './ui/SnoozeDialog';
import { useMergeDone } from './ui/MergeDone';
import { useDoneHelpers } from './ui/DoneHelpers';
import { Button, cx, Icon, Tabs } from './ui/primitives';
import { ALL_PROJECTS, ProjectSwitcher } from './ui/ProjectSwitcher';
import { ProjectIcons, projectIconMap, ProjectShown } from './ui/ProjectSaid';
import { useLogo } from './logo';
import { useGlass } from './glass';
import { usePrLinks } from './ui/PrLink';
import { reviewedWords } from './prDraft';
import { inPrTab, PR_GROUPS, PR_ROWS, PR_SORTS, PR_TABS, PrsView, type PrGroupBy, type PrRowItem, type PrSort, type PrTab } from './ui/PrsView';
import { Resizer, usePanelWidth } from './ui/Resizer';
import { compassTitle, freeScopes, orchestratorOn, startOrchestrator } from './orchestrators';
import { StartOrchestratorDialog } from './ui/StartOrchestrator';
import { headItem, OrchestratorButton, SESSION_GROUPS, SESSION_ROWS, SESSION_SORTS, SessionsBar, SessionsView, STARTED_PLACES, type SessionItem, type SessionSort, type StartedPlace } from './ui/SessionsView';
import { groupsOf, type ListGroupBy, type RepoName } from './groups';
import { groupFoldsHolding, orderedGroupKeys, sessionGroupFoldsHolding, useGroupPrefs } from './groupPrefs';
import { byWords, FilterChips, ListBar, ListFind, ListTools, rowsSetting, ViewMenu, viewSetting, type RowLines } from './ui/ListBar';
import { TODO_GROUPS, TODO_ROWS, TODO_SORTS, TodoComposer, TodoView } from './ui/TodoView';
import { ScheduleDialog } from './ui/ScheduleDialog';
import { HarnessSchedulePanel, SchedulePanel } from './ui/SchedulePanel';
import { SCHEDULE_GROUPS, SCHEDULE_ROWS, SCHEDULE_SORTS, SchedulesView } from './ui/SchedulesView';
import { scheduleOf, scheduleProject, type ScheduleSort } from './schedules';
import { KanbanView } from './ui/KanbanView';
import { applyKanbanDrop, boardOrderWith, columnWith, kanbanCardId, kanbanStep, orderedIds, type KanbanActs } from './kanban';
import { CanvasView } from './ui/CanvasView';
import { entitiesOf, homeLane, refId, type OnBoard, type RefItem, type RefKind } from './canvas/model';
import { useCanvas } from './canvas/useCanvas';
import { columnsOf, holdsOf, takesAll } from './boards/model';
import { columnOf } from './boards/rules';
import { useBoardAsks } from './boards/useBoardAsks';
import { useBoardEdits } from './boards/useBoardEdits';
import { useBoards } from './boards/useBoards';
import { BoardBar, type BoardLayout } from './ui/BoardBar';
import { BoardDialog, ColumnDialog } from './ui/BoardDialog';
import { BoardEmpty } from './ui/BoardEmpty';
import { ThemeSwitch, useTheme } from './ui/ThemeSwitch';
import { Reading } from './ui/Tip';
import { UsageBar } from './ui/UsageBar';
import { Notices } from './ui/Notices';
import { noticeId } from './notices';
import { analyticsTitle, Sidebar } from './ui/Sidebar';
import { checkedSays, untilChecked } from './ui/Updates';
import { newSavedId, sameView, savedViewMenu, type SavedView, type ViewState } from './savedViews';
import { useNow } from './ui/useNow';
import { errorText } from './report';
import { askWords, type SnoozeAsk } from './snooze';
import { useSessionHistory } from './history';
import { inOrder, queueKinds } from './queue';
import { SESSION_READS, shownSession as queueShown, useWaitingQueue } from './useWaitingQueue';
import { useViewQueues } from './useViewQueues';
import { openWhere, where } from './open';
import { asText, flag, oneOf, readStored, strings, useStored } from './stored';
import { useBoard } from './useBoard';
import { useDeviceLanes, useDevices, useObserve } from './useDevices';
import { useTextSearch } from './useTextSearch';
import { useLatest } from './latest';
import { reorderKey, useMainKeys } from './mainKeys';
import { accountActsOf } from './accountActs';
import { cardActsOf } from './canvas/cardActs';
import { laneCanOf, sessionCanOf, type MainHas } from './mainCan';
import { helperActsOf } from './mainHelpers';
import { panelKindOf, selectedRow, shownSessionOf } from './mainSelection';
import { viewGroupActs, type GroupsHas } from './mainGroups';
import { AGES, FIND_IN, READING, VIEWS, VIEWS_SANS_BOARDS, type Age, type View } from './mainViews';
import { ageAppliesIn, listFiltersOf, pickNamesOf, sideBoardsOf, sideFacetsOf, sideFiltersOf, sideStatusesOf, sideViewsOf, viewSuggestionOf, type FiltersOn } from './sideLists';
import { useBarPlace } from './useBarPlace';
import { useListFind } from './useListFind';
import { useListPicks } from './useListPicks';
import type { SelectHow } from './picks';
import { useNewSession } from './useNewSession';
import { usePrMenus } from './usePrMenus';
import { useRevealAsks } from './useRevealAsks';
import { useRevealFind } from './useRevealFind';
import { useSavedViews } from './useSavedViews';
import { useSchedulesView } from './useSchedulesView';
import { useSettingsAt } from './useSettingsAt';
import { useShownNotices } from './useShownNotices';
import { useShownTerminal } from './useShownTerminal';
import { useZoomReload } from './useZoomReload';
import { useTodoView } from './useTodoView';
import { MainBar } from './ui/MainBar';
import { canvasPicked, MainPanel, type Drill } from './ui/MainPanel';
import { showPrTab } from './ui/PrInspector';
import { Toasts, useToasts } from './ui/Toasts';
import { NewSessionPanel } from './ui/NewSession';
import { ListenHost } from './ui/Listening';
import { MeetingOffered } from './ui/Meeting';
import { MeetingsDialog } from './ui/Meetings';
import { ageListingChain, ageListingPr, canvasLabelOptions, cardMatches, chainOf, columnShown, currentProject, facetsIn, facetsOn, labelPick, onlyPick, prInDone, revealing, unarchived, scheduleSessions, sessionItemOf, sessionItemsOf, togglePick, toggleShown, useBoardLists, projectsOf, type FacetId, type Facets, type KanbanColumn } from './boardLists';
import { activeSessionRows, groupOfRow, sectionOfRow, useSectionFolds } from './sectionFolds';
import { inProject } from './quick';
import { CODEALONG_REPO } from './improve';
import { bare, keysIn } from './platform';
import { useKeysShown } from './keysShown';

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
const BOARD_LAYOUT_KEY = 'codealong.boardLayout';
/** Whether boards were on (`[experimental] boards`) when the window last had the board. */
const BOARDS_ON_KEY = 'codealong.boardsOn';
const AGE_KEY = 'codealong.age';
const SESSION_SORT_KEY = 'codealong.sessionSort';
const SESSION_ROWS_KEY = 'codealong.sessionRows';
const SESSION_GROUP_KEY = 'codealong.sessionGroup';
const SESSION_STARTED_KEY = 'codealong.sessionStarted';
const PR_GROUP_KEY = 'codealong.prGroup';
const TODO_GROUP_KEY = 'codealong.todoGroup';
const PR_ROWS_KEY = 'codealong.prRows';
const PR_SORT_KEY = 'codealong.prSort';
const PR_TAB_KEY = 'codealong.prTab';
const TODO_SORT_KEY = 'codealong.todoSort';
const TODO_ROWS_KEY = 'codealong.todoRows';
const SCHEDULE_SORT_KEY = 'codealong.scheduleSort';
const SCHEDULE_GROUP_KEY = 'codealong.scheduleGroup';
const SCHEDULE_ROWS_KEY = 'codealong.scheduleRows';
const SIDEBAR_WIDTH_KEY = 'codealong.sidebarWidth';
const SIDEBAR_COLLAPSED_KEY = 'codealong.sidebarCollapsed';
const INSPECTOR_WIDTH_KEY = 'codealong.inspectorWidth';
const PR_PANEL_WIDTH_KEY = 'codealong.prPanelWidth';
const TODO_PANEL_WIDTH_KEY = 'codealong.todoPanelWidth';
const SESSION_PANEL_WIDTH_KEY = 'codealong.sessionPanelWidth';
const SCHEDULE_PANEL_WIDTH_KEY = 'codealong.schedulePanelWidth';
const SESSION_PANEL_MAX_KEY = 'codealong.sessionPanelMax';
const PR_PANEL_MAX_KEY = 'codealong.prPanelMax';
const HELPERS_SHOWN_KEY = 'codealong.helpersShown';
// Each window has its own way back.
const HISTORY_KEY = WINDOW_PROJECT ? `codealong.sessionHistory:${WINDOW_PROJECT}` : 'codealong.sessionHistory';
const NO_PICKS: readonly string[] = [];
/** How long Sessions waits for a moved session's copy to be listed before letting it go: the
 *  minute its card on the desktop waits too (lib.rs `move_session`). */
const COPY_WAIT_MS = 60_000;

type Row = BoardRow | PrRowItem | TodoItem;
/** No rows, as one list, so what's keyed on the rows shown doesn't see a new one on every render. */
const NO_ROWS: Row[] = [];

/** A row's element id, for the list's active descendant. From its key, not its place in the
 *  list, so a row keeps its id when others move and needn't render again. */
function domIdOf(key: string): string {
  return `row-${key}`;
}
function domId(r: { key: string }): string {
  return domIdOf(r.key);
}

// How each remembered choice is read back: one of the ids it can be, else the default. A window
// left on the Worktrees view, which Sessions grouped by worktree took the place of, opens on
// Sessions, grouped by worktree until you pick otherwise (`sessionGroup`).
const VIEW_STORED = oneOf(VIEWS.map((v) => v.id));
const LAYOUT_STORED = oneOf<BoardLayout>(['kanban', 'canvas']);
const AGE_STORED = oneOf(AGES.map((a) => a.id));
const SORT_STORED = oneOf(SESSION_SORTS.map((o) => o.id));
const PR_SORT_STORED = oneOf(PR_SORTS.map((o) => o.id));
const PR_TAB_STORED = oneOf(PR_TABS.map((o) => o.id));
const TODO_SORT_STORED = oneOf(TODO_SORTS.map((o) => o.id));
const SCHEDULE_SORT_STORED = oneOf(SCHEDULE_SORTS.map((o) => o.id));
const ROWS_STORED = oneOf(SESSION_ROWS.map((o) => o.id));
const GROUP_STORED = oneOf(SESSION_GROUPS.map((o) => o.id));
const PR_GROUP_STORED = oneOf(PR_GROUPS.map((o) => o.id));
const STARTED_STORED = oneOf(STARTED_PLACES.map((o) => o.id));

/** A dragged panel width for the grid's style; left out, the panel takes its default from the CSS. */
const widthVar = (width: number | null) => (width == null ? undefined : `${width}px`);

export function App() {
  // The board, with changes you made that the engine hasn't shown yet on top (useBoard.ts).
  const { engine: engineBoard, board, ahead, loadError, loading } = useBoard();
  // Your other devices (useDevices.ts): their worktrees join this device's in every view, with
  // your changes to their sessions too, and the sidebar's Devices narrow the views to one of them.
  const devices = useDevices();
  const [deviceFilter, setDeviceFilter] = useState<DeviceFilter>([]);
  const deviceLanes = useDeviceLanes(devices, ahead);
  useObserve(engineBoard, devices);
  // A project window keeps its choice to itself; the main window's is remembered.
  const [project, setProject] = useStored(WINDOW_PROJECT ? null : PROJECT_KEY, asText, WINDOW_PROJECT);
  const [bayFilter, setBayFilter] = useState<readonly Filter[]>([]);
  // What else the sidebar narrows a view to: agents and labels in Sessions, repos and labels in PRs,
  // labels in Todo and Kanban (boardLists.ts `Facets`). Each view keeps its own while another shows.
  const [facets, setFacets] = useState<Facets>({});
  /** Nothing in the sidebar hides anything: for showing a row wherever it is. */
  const clearFilters = useCallback(() => {
    setBayFilter([]);
    setFacets({});
  }, []);
  const [query, setQuery] = useState('');
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  // The session shown in Sessions as you moved it (menus.ts `moving`), and its copy's key once the
  // move says it: the panel holds it until the copy is listed, then shows the copy.
  const [moving, setMoving] = useState<{ from: SessionItem; to: string | null } | null>(null);
  const [storedView, setView] = useStored<View>(VIEW_KEY, VIEW_STORED, 'sessions');
  // Boards are an experiment, off until Settings › Experimental turns them on: till then the
  // Board view isn't among the views, its keys go nowhere, and one remembered from when they were
  // on shows Sessions. Until the board comes they're as the window last saw them, so it opens as
  // it was left, with no Board coming or going in the sidebar as the board comes.
  const boardsSaid = board ? !!board.experimental?.boards : null;
  const [boardsSeen, setBoardsSeen] = useStored(BOARDS_ON_KEY, flag, false);
  useEffect(() => {
    if (boardsSaid !== null) setBoardsSeen(boardsSaid);
  }, [boardsSaid, setBoardsSeen]);
  const boardsOn = boardsSaid ?? boardsSeen;
  const views = boardsOn ? VIEWS : VIEWS_SANS_BOARDS;
  const view: View = views.some((v) => v.id === storedView) ? storedView : 'sessions';
  // The Board view's layout the sidebar's Board goes back to: the last one shown.
  const [lastLayout, setLastLayout] = useStored<BoardLayout>(BOARD_LAYOUT_KEY, LAYOUT_STORED, 'kanban');
  useEffect(() => {
    if (view === 'kanban' || view === 'canvas') setLastLayout(view);
  }, [view, setLastLayout]);
  const [age, setAge] = useStored<Age>(AGE_KEY, AGE_STORED, 'all');
  // Panel widths as you last dragged them. The PR panel has its own: it starts out wider.
  const [sidebarWidth, setSidebarWidth] = usePanelWidth(SIDEBAR_WIDTH_KEY);
  // The sidebar folds to a rail of the views and devices (⌃⌘S), and stays as you left it. It
  // starts folded: the list gets the room, and the filters are a click away.
  const [sideCollapsed, setSideCollapsed] = useStored(SIDEBAR_COLLAPSED_KEY, flag, true);
  const toggleSidebar = useCallback(() => setSideCollapsed((c) => !c), [setSideCollapsed]);
  // A session's panel in Sessions can take the whole window (⇧⌘F), for working through sessions one
  // after another with the list out of the way; it stays as you left it.
  const [panelMax, setPanelMax] = useStored(SESSION_PANEL_MAX_KEY, flag, false);
  const togglePanelMax = useCallback(() => setPanelMax((m) => !m), [setPanelMax]);
  // A PR's panel in PRs can too, for reading its description and its diffs at the window's width;
  // apart from the session's, since each view is used for its own.
  const [prPanelMax, setPrPanelMax] = useStored(PR_PANEL_MAX_KEY, flag, false);
  const togglePrPanelMax = useCallback(() => setPrPanelMax((m) => !m), [setPrPanelMax]);
  const [savedViews, keepSaved] = useSavedViews();
  const [inspectorWidth, setInspectorWidth] = usePanelWidth(INSPECTOR_WIDTH_KEY);
  const [prPanelWidth, setPrPanelWidth] = usePanelWidth(PR_PANEL_WIDTH_KEY);
  const [todoPanelWidth, setTodoPanelWidth] = usePanelWidth(TODO_PANEL_WIDTH_KEY);
  const [sessionPanelWidth, setSessionPanelWidth] = usePanelWidth(SESSION_PANEL_WIDTH_KEY);
  const [schedulePanelWidth, setSchedulePanelWidth] = usePanelWidth(SCHEDULE_PANEL_WIDTH_KEY);
  const sectionFolds = useSectionFolds(!!query);
  const groupPrefs = useGroupPrefs(!!query);
  const { showArchived, setShowArchived, unfold: unfoldSection } = sectionFolds;
  const [pendingUnfold, setPendingUnfold] = useState<string | null>(null);
  // The sessions whose finished helpers Sessions lists under them too, as their chip's
  // double-click left them; remembered across launches.
  const [helpersShownList, setHelpersShownList] = useStored(HELPERS_SHOWN_KEY, strings, [] as string[]);
  const helpersShown = useMemo(() => new Set(helpersShownList), [helpersShownList]);
  const toggleHelpers = useCallback((key: string) => setHelpersShownList((prev) => toggleShown(prev, key)), [setHelpersShownList]);
  const [showDonePrs, setShowDonePrs] = useState(false);
  // What went wrong with something you did here, what's under way, and the ways back (Toasts.tsx),
  // said in the usage bar.
  const toasts = useToasts();
  const { setToast, track, report, setTodoUndo } = toasts;
  // What isn't working in the background (useShownNotices.ts): the engine's and this window's own,
  // less the ones dismissed, which stay dismissed across launches while they still stand.
  const { notices, setDismissed } = useShownNotices(board);
  const boardRead = !!board && !board.loading;
  // Settings, open at a section; at one account when a usage chip opened it. ⌘, comes back to
  // the section you left, as macOS settings do (useSettingsAt.ts).
  const { settings, setSettings, openSettings, closeSettings } = useSettingsAt();
  // The app menu's Check for updates…: a toast in the window you asked from says how it goes
  // (ui/Updates.tsx), and Settings stays shut.
  useEffect(
    () =>
      onCheckUpdates(() =>
        void track('Looking for updates…', untilChecked()).then(
          (view) => setToast(checkedSays(view, Date.now())),
          (e: unknown) => setToast(errorText(e)),
        ),
      ),
    [track, setToast],
  );
  // File › Close idle sessions… (ui/CloseIdleDialog.tsx) and Close stale terminals…
  // (ui/CloseStaleDialog.tsx).
  const [closeIdle, setCloseIdle] = useState(false);
  // Start orchestrator… from Sessions' bar, for the scope it was asked for (null: All projects).
  const [orchestratorAsk, setOrchestratorAsk] = useState<{ scopes: (string | null)[] } | null>(null);
  const [closeStale, setCloseStale] = useState(false);
  // Set up Codealong (ui/SetupDialog.tsx): by itself in the main window on a first run, and from
  // the app menu's Set up Codealong… whenever asked.
  const [setup, setSetup] = useState(false);
  useEffect(() => onSetupMenu(() => setSetup(true)), []);
  useEffect(() => {
    if (windowLabel() !== 'main') return;
    // Not saying it failed: the wizard is in the app menu still, and Settings has all it does.
    setupNeeded()
      .then((needed) => needed && setSetup(true))
      .catch(() => {});
  }, []);
  // View › Analytics… and the usage bar (ui/UsageDialog.tsx), for one account or all of them.
  const [usageOf, setUsageOf] = useState<{ account: string | null } | null>(null);
  // Your meetings (ui/Meetings.tsx), opened from the window's bar while voice is on.
  const [meetingsOpen, setMeetingsOpen] = useState(false);
  useEffect(() => onUsageMenu(() => setUsageOf({ account: null })), []);
  // Help › Codealong Website opens the website in the browser.
  useEffect(() => onWebsiteMenu(() => void openUrl(WEBSITE).catch((e: unknown) => setToast(errorText(e)))), [setToast]);
  // How something done in the peek a PR's or todo's panel opened went: a send that didn't go,
  // which is back in its box (Peek.tsx), as the bar says it for one opened from a pill.
  useEffect(() => onWindowToast(setToast), [setToast]);
  // The sidebar's GitHub button opens Codealong's repo in the browser.
  const openRepo = () => void openUrl(CODEALONG_REPO).catch((e: unknown) => setToast(errorText(e)));
  // The worktree whose preview URL is being set (ui/PreviewBits.tsx), by id.
  const [previewFor, setPreviewFor] = useState<string | null>(null);
  // The session being renamed (ui/RenameDialog.tsx), by key.
  const [renaming, setRenaming] = useState<string | null>(null);
  // What Pick a time… in a Snooze menu, in this window or another, asked to snooze
  // (ui/SnoozeDialog.tsx).
  const [snoozing, setSnoozing] = useState<SnoozeAsk | null>(null);
  // The worktrees Clean up worktree… asks about (ui/CleanUpDialog.tsx).
  const [cleaning, setCleaning] = useState<Lane[] | null>(null);
  // Merge and done…'s dialog (ui/MergeDone.tsx), over whatever the window shows, so one GitHub
  // refused comes back in front of you.
  const mergingDone = useMergeDone(true);
  // Mark as done's question about helpers whose work isn't over (menus.ts `doneHelpers`).
  const askingHelpers = useDoneHelpers();
  const doneHelpers = board ? { lanes: [...board.lanes, ...deviceLanes], ask: askingHelpers.ask } : undefined;
  useEffect(() => onCloseIdle(() => setCloseIdle(true)), []);
  useEffect(() => onCloseStale(() => setCloseStale(true)), []);
  // ⌘= ⌘- ⌘0 (mainKeys.ts) and View's zoom items zoom every board window, and ⌘R and View › Reload
  // read again what the window shows, usage too (useZoomReload.ts).
  const { zoom, reload } = useZoomReload(board, setToast);
  /** Marks sessions seen, shown at once; the engine turning it down puts them back and says why. */
  const markSeen = useCallback(
    (keys: string[]) => {
      seenAhead(keys).catch((e: unknown) => setToast(errorText(e)));
    },
    [setToast],
  );
  const now = useNow(15_000);
  // The list's find (useListFind.ts, ui/ListBar.tsx): open from ⌥⌘F or / until the keyboard leaves
  // it, and shown as long as it has words. It's over the list, so a panel taking the whole window
  // (⇧⌘F, `maxed` below) gives the list its place back as it opens.
  const { boardRef, searchRef, findShown, openFind, setFindOpen } = useListFind({ query, view, loading, setPanelMax, setPrPanelMax });
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
  useGlass(!!board?.glass);
  useKeysShown(board?.showKeys);
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
      clearFilters();
      setSelectedKey(null);
    },
    [setProject, clearFilters],
  );

  // Pinned sessions head the Sessions view, then grouped, its pinned worktrees or repos, each in
  // your order.
  const sessionPins = usePins(board?.pinned, setPinned, setToast);
  const lanePins = usePins(board?.pinnedWorktrees, setPinnedWorktrees, setToast);
  const repoPins = usePins(board?.pinnedRepos, setPinnedRepos, setToast);
  const prPins = usePins(board?.pinnedPrs, setPinnedPrs, setToast);
  // How Sessions orders the sessions that aren't pinned: by status, or the newest start first.
  const [sessionSort, setSessionSort] = useStored<SessionSort>(SESSION_SORT_KEY, SORT_STORED, 'status');
  // Whether Sessions lists a session another one's agent started under that one, or on its own.
  const [sessionStarted, setSessionStarted] = useStored<StartedPlace>(SESSION_STARTED_KEY, STARTED_STORED, 'apart');
  // Whether Sessions, PRs and Todo group by worktree or by repo, each as you left it, and each
  // one's groups opened past their fold, until the window closes (groups.ts).
  const [leftOnWorktrees] = useState(() => readStored(VIEW_KEY, asText, '') === 'worktrees');
  const [sessionGroup, setSessionGroup] = useStored<ListGroupBy>(SESSION_GROUP_KEY, GROUP_STORED, leftOnWorktrees ? 'worktree' : 'none');
  const [prGroup, setPrGroup] = useStored<PrGroupBy>(PR_GROUP_KEY, PR_GROUP_STORED, 'none');
  const [todoGroup, setTodoGroup] = useStored<ListGroupBy>(TODO_GROUP_KEY, GROUP_STORED, 'none');
  const [openGroups, setOpenGroups] = useState<ReadonlySet<string>>(() => new Set());
  const [openPrGroups, setOpenPrGroups] = useState<ReadonlySet<string>>(() => new Set());
  const [openTodoGroups, setOpenTodoGroups] = useState<ReadonlySet<string>>(() => new Set());
  const [scheduleGroup, setScheduleGroup] = useStored<ListGroupBy>(SCHEDULE_GROUP_KEY, GROUP_STORED, 'none');
  const [openScheduleGroups, setOpenScheduleGroups] = useState<ReadonlySet<string>>(() => new Set());
  // How PRs and Todo order what isn't pinned or done: by status, or the newest opened or added first.
  const [prSort, setPrSort] = useStored<PrSort>(PR_SORT_KEY, PR_SORT_STORED, 'status');
  const [prTab, setPrTab] = useStored<PrTab>(PR_TAB_KEY, PR_TAB_STORED, 'all');
  const [todoSort, setTodoSort] = useStored<TodoSort>(TODO_SORT_KEY, TODO_SORT_STORED, 'status');
  // How Schedules orders the ones Codealong keeps: the next run soonest first, until you pick otherwise.
  const [scheduleSort, setScheduleSort] = useStored<ScheduleSort>(SCHEDULE_SORT_KEY, SCHEDULE_SORT_STORED, 'next');
  // How much each row says in Sessions, PRs and Todo, each as you left it: two lines, or one.
  const [sessionLines, setSessionLines] = useStored<RowLines>(SESSION_ROWS_KEY, ROWS_STORED, 'one');
  const [prLines, setPrLines] = useStored<RowLines>(PR_ROWS_KEY, ROWS_STORED, 'one');
  const [todoLines, setTodoLines] = useStored<RowLines>(TODO_ROWS_KEY, ROWS_STORED, 'one');
  const [scheduleLines, setScheduleLines] = useStored<RowLines>(SCHEDULE_ROWS_KEY, ROWS_STORED, 'one');

  // The age filter: worktrees and sessions active since the cutoff. Pinned worktrees always
  // show on the board and in the sidebar, and pinned sessions in Sessions.
  const ageMs = AGES.find((a) => a.id === age)?.ms ?? 0;
  const cutoff = ageMs > 0 ? now - ageMs : 0;
  const since = (AGES.find((a) => a.id === age)?.title ?? '').replace(/^Active /, '');

  // The Todo view: the project's todos as the engine has them, with this window's changes on top
  // until the engine's board shows them.
  const todoStore = useTodoStore(board, setToast);
  const [showDoneTodos, setShowDoneTodos] = useState(false);
  const [showArchivedTodos, setShowArchivedTodos] = useState(false);
  // The columns of a board's Kanban that start folded (Done) and that you opened, by board and
  // column: each is a rail until it's opened.
  const [openedColumns, setOpenedColumns] = useState<ReadonlySet<string>>(() => new Set());
  // The board the Board view shows, of the project's boards (boards/useBoards.ts), none while the
  // project has none. The project is worked out as the lists work it out, which need the board.
  const boardsProject = currentProject(project, projectsOf(board?.projects ?? [], deviceLanes), loading);
  const boards = useBoards(boardsProject);
  const shownBoard = boards.board;
  const columnOpen = useCallback((c: KanbanColumn) => columnShown(c, openedColumns.has(`${shownBoard?.id}:${c.id}`)), [openedColumns, shownBoard?.id]);
  const toggleColumn = (c: { id: string }) =>
    setOpenedColumns((o) => {
      const key = `${shownBoard?.id}:${c.id}`;
      const next = new Set(o);
      if (!next.delete(key)) next.add(key);
      return next;
    });

  // What was said in sessions, here and on your other devices, for the search to find them by.
  const textSearch = useTextSearch(query, { ready: !!board, devices: deviceLanes.length > 0 });

  // What each view lists, from the board, the filters and the search (boardLists.ts).
  const {
    projects,
    current,
    lanes,
    projectLanes,
    deviceNeeds,
    deviceWorking,
    archivedItems,
    saidOf,
    shownSessionCount,
    listeningRows,
    pinnedRows,
    orchestratorRows,
    topHelpers,
    sessionSecs,
    groupedSecs,
    groupedPrs,
    groupedTodos,
    sessionParts,
    sessionRows,
    hasSessions,
    waitingRows,
    laneById,
    prRowOf,
    prRows,
    pinnedPrRows,
    prSecs,
    prVisible,
    openPrCount,
    requestedCount,
    prCounts,
    githubPrs,
    homes,
    allHomes,
    labelById,
    facetOptions,
    kanbanFacetOptions,
    projectTodos,
    todoItems,
    todoSecs,
    todoVisible,
    openTodoCount,
    todoLinks,
    todosFor,
    subjects,
    allSubjects,
    member,
    canvasShows,
    rawLanes,
    boardLanes,
    boardArchived,
    kanbanCols,
    kanbanVisible,
    openCardCount,
    helpersOf,
    runsOf,
    sidesOf,
    sideGlancesOf,
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
    said: textSearch.said,
    filter: bayFilter,
    sessionPins,
    helpersShown,
    lanePins,
    prPins,
    sessionSort,
    sessionStarted,
    sessionGroup,
    groupOrders: groupPrefs.orders,
    groupCollapsed: groupPrefs.isCollapsed,
    openGroups,
    prGroup,
    openPrGroups,
    todoGroup,
    openTodoGroups,
    prSort,
    prTab,
    todoSort,
    showArchived,
    sectionFolded: sectionFolds.isFolded,
    showDonePrs,
    todos: todoStore.todos,
    labels: todoStore.labels,
    facets,
    showDoneTodos,
    showArchivedTodos,
    shownBoard,
    columnOpen,
  });
  const unfoldSessionRow = useCallback(
    (key: string) => {
      const sec = sectionOfRow(key, sessionParts);
      if (sec) unfoldSection(sec);
      const grp = groupOfRow(key, groupedSecs);
      if (grp) groupPrefs.uncollapse('sessions', grp.fold);
      return !!(sec || grp);
    },
    [sessionParts, groupedSecs, unfoldSection, groupPrefs],
  );
  // What an orchestrator can be started for from this view, which Sessions' compass offers.
  const freeOrchestrators = useMemo(() => freeScopes(current, board?.projects, board?.schedules), [current, board?.projects, board?.schedules]);
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

  // The board's canvas: its document, kept up to date with what the board holds once it's been
  // shown. It's of the board's project's work, which under All projects isn't the window's. While
  // boards are off it's left as it is, nothing saved, and catches up once they're on again; with
  // no board, there's none.
  const boardProject = shownBoard?.project ?? current;
  const canvasEntities = useMemo(
    () => entitiesOf(board, boardLanes, todoStore.todos, (p) => boardProject === ALL_PROJECTS || p === boardProject, boardArchived),
    [board, boardLanes, todoStore.todos, boardProject, boardArchived],
  );
  const holds = useMemo<OnBoard>(() => (shownBoard ? holdsOf(shownBoard, member, canvasShows) : { shows: canvasShows, has: member, removed: new Set() }), [member, shownBoard, canvasShows]);
  const canvas = useCanvas(shownBoard?.id ?? null, view === 'canvas', !loading && boardsOn, canvasEntities, now, holds);
  /** A change to a board, here at once; what the engine turns down is said in a toast. */
  const applyBoard = useCallback((op: Parameters<typeof boards.apply>[0]) => boards.apply(op).catch((e: unknown) => setToast(errorText(e))), [boards, setToast]);
  const [canvasFocus, setCanvasFocus] = useState<{ ids: string[]; n: number } | null>(null);
  const canvasKeys = useRef<(e: KeyboardEvent) => boolean>(() => false);
  // Whether the canvas is drawn. CanvasView gives the window its keys while it is, and they go with
  // it: a board deleted, or a project with none, leaves no keys behind to act on a canvas that's gone.
  const canvasShown = view === 'canvas' && !!board && !!shownBoard && !!canvas.doc && !loading;
  useEffect(() => {
    if (!canvasShown) canvasKeys.current = () => false;
  }, [canvasShown]);
  // The one card selected on the canvas: its panel shows on the right, and ⌘N starts in its worktree.
  const [canvasSel, setCanvasSel] = useState<RefItem | null>(null);
  // Picking a status, or pressing Return in the filter, brings what matches into view.
  const [canvasFit, setCanvasFit] = useState(0);
  const canvasLane = view === 'canvas' && canvasSel ? homeLane(canvasEntities, canvasSel.ref, canvasSel.key) : null;
  /** Puts a thing on the board's canvas if it isn't there, and shows it there. */
  const showOnCanvas = (ref: RefKind, key: string) => {
    const id = canvas.place(ref, key);
    setView('canvas');
    setCanvasFocus((f) => ({ ids: [id], n: (f?.n ?? 0) + 1 }));
  };
  // Making and changing the boards (boards/useBoardEdits.ts): the board or the column its dialog
  // makes or changes, what its menus and the dialogs' Save do, and Add to board › in the other
  // menus (one thing's or several of a kind picked together).
  const { boardItems, boardEdit, setBoardEdit, columnEdit, setColumnEdit, showBoard, saveBoard, boardMenu, columnMenu, saveColumn, ruleChoices } = useBoardEdits({ boards, applyBoard, boardsOn, view, setView, lastLayout, current, projects, pickProject, setSelectedKey, setToast, board, rawLanes, projectLanes, todoStore, member, allSubjects, now });
  const boardLayout: BoardLayout = view === 'canvas' ? 'canvas' : 'kanban';
  // What's pinned to the desktop, so menus offer to take it off (desktop.rs).
  const onDesktop = menus.useDesktopPins();
  // The sessions kept in windows of their own, so a session's menu offers to pop it out or show
  // its window (kept.rs `pop_session`).
  const keptSessions = useKeptSessions();

  // Nothing is listed until the board is read, so nothing can be selected: not the other devices'
  // rows that come first.
  const visible = loading ? NO_ROWS : view === 'sessions' ? sessionRows : view === 'prs' ? prVisible : view === 'todo' ? todoVisible : view === 'kanban' ? kanbanVisible : NO_ROWS;
  // The PRs view with none of its PRs yet, while GitHub's are on their way: it says it's reading
  // them, as the board does before its first read, rather than that there are none.
  const prsComing = boardRead && view === 'prs' && prRows.length === 0 && githubPrs === 'coming';
  const reading = loading || prsComing;
  // A session listed under another worktree now (its commands moved there) stays selected. When
  // the selected agent's line goes (you marked it seen, it stopped), the selection stays
  // on its worktree rather than jumping to the top. A todo that leaves the list (marked done
  // while Done is folded, or no longer matching the search or a filter) stays in the panel: the
  // next key you press is still about the todo you see, and an edit in progress isn't cut off. A
  // PR that leaves it stays the same way: one you merge moves under Merged & closed, folded,
  // as you confirm it (ahead.ts `mergeAhead`), and its panel says how the merge went. A session
  // you're moving stays in Sessions' panel as it was until its copy takes its place (`moving`).
  const [, selectedSessionKey] = selectedKey?.split('::') ?? [];
  const held = view === 'sessions' && moving && moving.from.session.key === selectedSessionKey ? moving.from : undefined;
  // In Sessions, a worktree selected by its id is selected whether the list has its heading or not
  // (one shown from elsewhere, `focusLane`, in a list not grouped by worktree or with no session of
  // it): its panel shows, and ⌘N, ⌃`, Enter and its chords act on it.
  const chosenLane = useMemo(() => {
    const lane = view === 'sessions' && selectedKey && !selectedKey.includes('::') ? laneById.get(selectedKey) : undefined;
    return lane ? headItem(lane) : undefined;
  }, [view, selectedKey, laneById]);
  const activeSessions = useMemo(() => activeSessionRows(sessionParts), [sessionParts]);
  const selected = selectedRow(view, selectedKey, visible, { todoItems, prRows, held, activeSessions, chosenLane });
  // With nothing picked yet (the window just opened, or you changed projects) the panel shows
  // the top session (grouped, not its worktree's heading), and it holds there: a session that
  // comes to the top later doesn't take it.
  // A click in the moment before this runs has picked a row already: it isn't taken back.
  useEffect(() => {
    const top = selected?.key;
    if (selectedKey === null && top !== undefined) setSelectedKey((k) => k ?? top);
  }, [selectedKey, selected?.key]);
  const selectedLane = selected?.lane ?? null;
  // What the find bar in a session's panel opens on, wherever the panel shows it (useRevealFind.ts):
  // for a session this window's search found by what was said in it, the words it found there; else
  // the words the quick panel found it by (`Reveal.find`), until another session is selected or a
  // search is typed here.
  const { findIn, setRevealFind } = useRevealFind({ selectedSessionKey, query, saidOf });
  // The runs of todos listed in your order, which ⌃⌘↑ ⌃⌘↓ move one within: the Todo view's open
  // sections while it's sorted by status. The Kanban view moves a card in its column itself
  // (`kanbanKeys`).
  const todoRuns = useMemo(
    () => (view === 'kanban' || todoGroup !== 'none' ? [] : todoSecs.filter((s) => s.id !== 'done' && s.id !== 'archived' && statusSection(s.id)).map((s) => s.rows)),
    [view, todoSecs, todoGroup],
  );
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
    todoEdit,
    pickDueHere,
  } = useTodoView({
    store: todoStore,
    selected,
    setSelectedKey,
    visible,
    todoVisible: view === 'kanban' ? kanbanVisible : todoVisible,
    todoSecs,
    ordered: todoRuns,
    current,
    projects,
    lanes,
    laneById,
    lanePins,
    selectedLane,
    labelPicks: facets.labels ?? NO_PICKS,
    unpickLabel: (id) => setFacets((f) => ({ ...f, labels: (f.labels ?? []).filter((x) => x !== id) })),
    setShowDoneTodos,
    setShowArchivedTodos,
    report,
    setToast,
    setTodoUndo,
    newSession,
    // Read when a menu runs: the window's opener is made further down, with what it reveals.
    open: (lane, targetId) => open(lane, targetId),
    pickProject,
    setView,
    listsTodos: view === 'todo' || view === 'kanban',
    clearFilters,
    setQuery,
    onDesktop,
    boardItems,
  });
  // The box that adds a todo: at the top of the Todo view, and of the column of a board's Kanban
  // a new todo lands in. On a board that doesn't take everything, a new todo is put on it.
  const composerColumn = useMemo(() => {
    const fresh = { card: 'todo:', ref: 'todo' as const, title: '', activity: now, stage: 'todo' as const, statuses: [], labels: (facets.labels ?? []).slice(0, 1), repos: [], accounts: [] };
    return shownBoard ? columnOf(fresh, columnsOf(shownBoard), undefined, now) : null;
  }, [shownBoard, facets.labels, now]);
  // A todo made on a board's Kanban is in the board's project, which under All projects may not be the window's.
  const todoProject = view === 'kanban' ? boardProject : current;
  const todoComposer = (
    <TodoComposer
      ref={composerRef}
      value={composer.text}
      onChange={(text) => setComposer((c) => ({ ...c, text }))}
      context={composer.context}
      onClearContext={() => setComposer((c) => ({ ...c, context: null }))}
      onCreate={(title) => {
        const id = createTodo(title, composer.context, todoProject);
        if (view === 'kanban' && shownBoard && !takesAll(shownBoard.rule)) void applyBoard({ op: 'add', id: shownBoard.id, cards: [`todo:${id}`] });
      }}
      where={current !== ALL_PROJECTS ? null : todoProject !== ALL_PROJECTS ? todoProject : composer.context?.worktree ? (laneById.get(composer.context.worktree)?.project ?? null) : 'no project'}
    />
  );
  // The Kanban view's arrows: up and down a column, left and right across them (kanban.ts); and
  // ⌃⌘↑ ⌃⌘↓, which move the card selected up or down a column that keeps your order, as a drag does.
  const kanbanKeys = useLatest((e: KeyboardEvent): boolean => {
    const reorder = view === 'kanban' ? reorderKey(e) : 0;
    if (reorder) {
      const col = kanbanCols.find((c) => c.cards.some((x) => x.key === selected?.key));
      const at = col?.cards.findIndex((x) => x.key === selected?.key) ?? -1;
      if (!col || at < 0 || col.column?.sort !== 'hand' || pickedKeys.size > 0) return false;
      e.preventDefault();
      // Insertion points: one up is the card's own index less one; one down is two past it.
      kanbanActs.order(col.cards[at], col.id, reorder < 0 ? at - 1 : at + 2);
      return true;
    }
    const dir = ({ ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right' } as const)[e.key as 'ArrowUp'];
    if (view !== 'kanban' || !dir || !bare(e)) return false;
    e.preventDefault();
    const to = kanbanStep(kanbanCols, selected?.key ?? null, dir, columnOpen);
    if (to) {
      clearTodoPick();
      setSelectedKey(to);
    }
    return true;
  });
  // A PR in a session's, worktree's or todo's panel, from its chip (`showPrOf`), or a worktree in
  // any panel, from its chip or a link (`focusLane`), until you go back or press its row again
  // (`pressRow`). It's tied to the panel it came from: selecting another row leaves it. Rows
  // picked together show their own panel over it (MainPanel.tsx). A session a PR's or todo's panel
  // lists shows in the peek instead (MainPanel.tsx `peekFromPanel`), and the canvas shows each
  // thing on its own card, so nothing there shows in another's place.
  const [drill, setDrill] = useState<Drill | null>(null);
  /** A press on a list's row, before it selects the row. On the row whose panel shows a PR or
   *  worktree in its place, it brings the row's own panel back, which selecting a row that's
   *  selected already wouldn't. */
  const pressRow = (key: string, how: SelectHow) => {
    if (how === 'one' && drill?.from === key) setDrill(null);
  };
  // The Schedules view (useSchedulesView.ts): the schedule selected and a session selected under
  // it, whose panel shows meanwhile; the project's schedules as the find narrows them and as the
  // view lists them, and the ones the harnesses keep themselves; and what's done to them from its
  // rows, panel and menus. In the Schedules view, that session is what the window's keys act on
  // (`scheduleRow`), as a row selected in a list is elsewhere (mainKeys.ts).
  const { setScheduleId, setScheduleSession, schedulesAll, setSchedulesAll, showFinished, setShowFinished, scheduleEdit, setScheduleEdit, projectSchedules, shownHarnessSchedules, scheduleListed, selectedHarness, selectedSchedule, scheduleSessionShown, scheduleRow, sessionTitle, runSchedule, openHarness, pauseSchedule, newSchedule, scheduleMenu } = useSchedulesView({
    view,
    board,
    current,
    query,
    homes,
    allHomes,
    selectedLane,
    scheduleSort,
    scheduleGroup,
    groupOrder: scheduleGroup === 'none' ? undefined : groupPrefs.orders[scheduleGroup],
    openScheduleGroups,
    report,
    setToast,
    // Read when a schedule opens: the window's opener is made further down.
    open: (lane, targetId) => open(lane, targetId),
  });
  // What the panel beside the list shows, which what a chip or a group's heading shows in its
  // place (`drill`) belongs to: the row selected, and in Schedules the schedule selected or the
  // session shown under it.
  const scheduleFrom = selectedHarness ?? selectedSchedule;
  const panelFrom =
    view === 'schedules'
      ? scheduleFrom
        ? `schedule:${selectedHarness?.key ?? selectedSchedule?.id}${scheduleSessionShown ? `/${scheduleSessionShown.session.key}` : ''}`
        : null
      : view !== 'canvas' && pickedRows.length === 0
        ? (selected?.key ?? null)
        : null;
  useEffect(() => {
    if (drill && drill.from !== panelFrom) setDrill(null);
  }, [drill, panelFrom]);

  // Where a new session starts: the selected worktree, session or PR's worktree; for a PR with
  // no worktree on this Mac, its repo's main checkout.
  const startLaneId = useMemo(() => {
    if (view === 'canvas') return canvasLane;
    if (view === 'schedules') return scheduleRow?.lane.id ?? null;
    if (!selected) return null;
    if (selected.lane) return selected.lane.id;
    if (selected.kind !== 'pr') return null;
    return lanes.find((l) => !l.device && l.repoSlug === selected.pr.repoSlug && l.origin === 'main')?.id ?? null;
  }, [selected, lanes, view, canvasLane, scheduleRow?.lane.id]);

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
  // PR or todo here: in its project, with no search or machine filter hiding it, nor a PR or
  // worktree shown in its panel's place (`drill`), which selecting what's selected already would
  // leave.
  // Each time something asks to show a row here, which opens the group that folds it away even
  // when it's the row selected already.
  const [revealed, setRevealed] = useState(0);
  const reveal = useLatest((r: Reveal) => {
    if (r.kind === 'link') {
      takeLink();
      return;
    }
    // Sessions, with the new session in its panel and the cursor in its box.
    if (r.kind === 'new') {
      setView('sessions');
      setDrill(null);
      setNewShown({ draft: r.draft, over: selectedKey, opened: Date.now(), sending: null, error: null, started: null });
      return;
    }
    if (r.kind === 'terminal') {
      showTerminal(r.id, !!r.wait);
      return;
    }
    // A view's chord pressed in a window that floats: that view, as the chord goes to it here.
    // One this window doesn't have (a board's, with boards off) leaves it on the one it shows.
    if (r.kind === 'view') {
      const to = views.find((v) => v.id === r.view);
      if (to) setView(to.id);
      return;
    }
    // Pick a time… in a Snooze menu: the window it was in is left as it is, and this one asks.
    if (r.kind === 'snooze') {
      setSnoozing({ keys: r.keys, reviews: r.reviews });
      return;
    }
    setRevealed((n) => n + 1);
    setRevealFind(r.kind === 'session' && r.find ? { key: r.key, ask: { words: r.find, n: Date.now() } } : null);
    // A board `codealong board open` asked for, or the canvas `board arrange --show` laid out,
    // with the frames it made in view. The command turns these down while boards are off.
    if (r.kind === 'board') {
      if (!boardsOn) return;
      if (current !== r.project) pickProject(r.project);
      boards.pick(r.board, r.project);
      setQuery('');
      clearFilters();
      setDrill(null);
      setView(r.layout);
      if (r.focus?.length) setCanvasFocus((f) => ({ ids: r.focus ?? [], n: (f?.n ?? 0) + 1 }));
      return;
    }
    // A schedule a session came from: the Schedules view, on it, in the project that lists it
    // (All projects for one in none).
    if (r.kind === 'schedule') {
      const x = board?.schedules?.find((s) => s.id === r.id);
      const home = x ? (scheduleProject(x, board?.lanes ?? []) ?? ALL_PROJECTS) : current;
      if (current !== ALL_PROJECTS && current !== home) pickProject(home);
      setQuery('');
      setView('schedules');
      if (scheduleListed.grouped) {
        const g = groupsOf(scheduleListed.grouped).find((x) => x.all.some((s) => s.id === r.id));
        if (g) groupPrefs.uncollapse('schedules', g.fold);
      }
      setScheduleId(r.id);
      setScheduleSession(null);
      // Its own panel, not what a heading showed in its place.
      setDrill(null);
      return;
    }
    // A todo the quick panel found: the Todo view, in its project.
    if (r.kind === 'todo') {
      const t = todoStore.todos.find((x) => x.id === r.id);
      if (t) {
        setDrill(null);
        showTodo(t);
      }
      return;
    }
    // A worktree on a machine the sidebar's machine filter leaves out shows with the filter
    // cleared, as the sidebar's other filters are, and is looked for among every machine's.
    const everyLane = [...(board?.lanes ?? []), ...deviceLanes];
    const laneId = r.kind === 'session' || r.kind === 'worktree' ? r.laneId : null;
    const offMachine = laneId != null && !rawLanes.some((l) => l.id === laneId) && everyLane.some((l) => l.id === laneId);
    const listed = offMachine ? unarchived(everyLane) : lanes;
    const lane = listed.find((l) => l.id === laneId);
    const session = lane?.sessions.find((s) => s.key === (r.kind === 'session' ? r.key : ''));
    // An archived session isn't in the lists the views show (`unarchived`): it's in Sessions'
    // Archived fold, so it's looked for on the board as the engine sent it.
    const archivedLane = r.kind === 'session' && !session ? everyLane.find((l) => l.id === r.laneId) : undefined;
    const archived = archivedLane?.sessions.find((s) => s.key === (r.kind === 'session' ? r.key : '') && s.archived);
    const pr = r.kind === 'pr' ? board?.prs.find((p) => p.key === r.key) : undefined;
    const home = lane?.project ?? archivedLane?.project ?? pr?.project;
    if (!home) return;
    if (current !== ALL_PROJECTS && current !== home) pickProject(home);
    if (offMachine) setDeviceFilter([]);
    setQuery('');
    setDrill(null);
    if (lane && r.kind === 'worktree') {
      // In another project, or on a machine that wasn't listed, what's listed now isn't what will be.
      focusLane(lane, undefined, (current !== ALL_PROJECTS && current !== home) || offMachine);
    } else if (archivedLane && archived) {
      // Its row in the Archived fold, opened, with its conversation in the panel; the age
      // filter doesn't apply there.
      setView('sessions');
      clearFilters();
      setShowArchived(true);
      setSelectedKey(`${archivedLane.id}::${archived.key}`);
    } else if (lane && session) {
      // Its own row in Sessions, with its conversation in the panel, whichever view was showing:
      // the board would only show its line on its worktree's row.
      setView('sessions');
      clearFilters();
      // One older than the age filter lets through: the filter widens as far as it, as for a PR,
      // and as far as the sessions a helper or a run is listed under. A finished helper, or a run
      // its session's row only counts, is listed under its session.
      const item = sessionItemOf({ lane, session });
      const items = sessionItemsOf(listed);
      const wider = ageListingChain(chainOf(item, items).map((r) => r.session), sessionPins.set, AGES, age, Date.now());
      if (wider) setAge(wider);
      setHelpersShownList((prev) => revealing(prev, items, item));
      const key = `${lane.id}::${session.key}`;
      unfoldSessionRow(key);
      setPendingUnfold(key);
      setSelectedKey(key);
    } else if (pr) {
      if (r.kind === 'pr' && r.tab) showPrTab(pr.key, r.tab);
      setView('prs');
      clearFilters();
      if (prInDone(pr, prPins.set)) setShowDonePrs(true);
      // Under its own tab when the one shown doesn't list it (a pinned one shows in every tab).
      // Only as it's shown: a tab picked after it stays, with it selected out of sight.
      if (!prPins.set.has(pr.key) && !inPrTab(pr, prTab)) setPrTab(pr.mine ? 'mine' : 'others');
      // One older than the age filter lets through: the filter widens as far as it.
      const wider = ageListingPr(pr, prPins.set, AGES, age, Date.now());
      if (wider) setAge(wider);
      setSelectedKey(`pr:${pr.key}`);
      // The list goes to it, though it was selected already: under a tab picked since, or
      // scrolled out of sight, its selection alone wouldn't move the list.
      setScrollTo(`pr:${pr.key}`);
    }
  });
  // A terminal of Codealong's the server asked to show (useShownTerminal.ts): its session's panel,
  // or a window of its own for one that's no session's, once the board says whose it is.
  const { showTerminal } = useShownTerminal({ board, reveal, setToast });

  const open = useCallback((lane: Lane | null, targetId: string | null | undefined) => {
    if (!lane || !targetId) return;
    // A session Codealong runs lives in its panel here: Sessions, with it selected.
    if (targetId.startsWith('s|') && targetId.endsWith('|hosted')) {
      const key = targetId.slice(2, -'|hosted'.length);
      reveal.current({ kind: 'session', laneId: lane.id, key });
      setView('sessions');
      // What a side conversation answered goes with being seen, and its panel has yet to show it.
      if (!lane.sessions.some((s) => s.key === key && s.sideAnswer)) markSeen([key]);
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
    reviewAhead(pr, 'approve', '')
      .then((done) => setToast(reviewedWords('approve', pr.number, done)))
      .catch((e: unknown) => setToast(errorText(e)));
  }, [setToast]);

  /** Approves several PRs as you, one after another, and says how it went. */
  const approveAll = useCallback(
    (prs: PrItem[]) => {
      const wrong: string[] = [];
      let approved = 0;
      // All of them show approved, and on their way, at once; GitHub is asked one after another.
      const each = prs.reduce<Promise<void>>(
        (before, p) =>
          reviewAhead(p, 'approve', '', false, null, before).then(
            () => void approved++,
            (e: unknown) => void wrong.push(`Couldn’t approve #${p.number}: ${errorText(e).replace(/\.$/, '')}.`),
          ),
        Promise.resolve(),
      );
      void track(`Approving ${count(prs.length, 'PR')}…`, each).then(() => setToast([approved > 0 ? `Approved ${count(approved, 'PR')}.` : '', ...wrong].filter(Boolean).join(' ')));
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
    // In the order `[queue]` puts what waits on you in: of the sessions', needs you and Your turn.
    reads: inOrder(SESSION_READS, queueKinds(board?.queue?.order)),
    shown: view === 'sessions' && selected?.kind === 'agent' ? queueShown(selected.session) : null,
    show: (row, box = true) => {
      unfoldSessionRow(row.key);
      if (box) setBoxFocus(row.key);
      setSelectedKey(row.key);
    },
    markSeen: (key) => markSeen([key]),
    leftStays: true,
  });
  const { run: runQueue, left: leftQueue, say: sayQueue } = queue;
  // Next in PRs, Todo and Kanban, and on from each view to the next where its own run out
  // (useViewQueues.ts), the strip over every panel going by it.
  const viewQueues = useViewQueues<SessionItem>({
    view,
    selected,
    board,
    project: current,
    now,
    prListed: prVisible,
    todoListed: todoVisible,
    kanbanListed: kanbanVisible,
    sessions: {
      waiting: waitingRows,
      reads: inOrder(SESSION_READS, queueKinds(board?.queue?.order)),
      queue,
      show: (row) => {
        unpick.current();
        unfoldSessionRow(row.key);
        setBoxFocus(row.key);
        setSelectedKey(row.key);
      },
    },
    setView,
    // One row, as a click selects it: what was picked together isn't picked again as it comes back.
    select: (key) => {
      unpick.current();
      setSelectedKey(key);
    },
  });
  const stepView = useLatest(viewQueues.step);
  /** A session's menu, its chords or the strip's buttons took it off what waits on you (snoozed,
   *  closed, archived, marked seen): from the one shown in Sessions, on to the next that waits, as
   *  ⌘E goes, the keyboard staying where it was. */
  const leftAndOn = (s: Session, done: menus.Left) => void leftQueue(s.key, done, !!document.activeElement?.closest('.ca-sessinsp .ca-composer'));
  // Only when the selection moves: the row gone on to keeps its claim on the cursor until then.
  useEffect(() => {
    const key = selected?.key;
    setBoxFocus((box) => (box && key !== box ? null : box));
  }, [selected?.key]);

  // A start the quick panel left for its first prompt (useNewSession.ts, newSession.ts): Sessions'
  // panel shows it in place of the selected row's until another row is picked, and once sent, until
  // the board lists the session it started, which then takes its place with the cursor in its box.
  const { newShown, setNewShown, sendNew } = useNewSession({ selectedKey, board, deviceLanes, reveal, setBoxFocus, setToast });
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
  // the panel shows, in Sessions, under a schedule, or as the canvas's selected card. One a PR's or
  // todo's panel shows in the peek isn't among them: the panel stays on the PR or todo.
  const shownSession = shownSessionOf(view, { scheduleSession: scheduleSessionShown?.session.key ?? null, drilled: !!drill, selected, canvasSel });
  // Where ⌃` opens a terminal (mainKeys.ts): in the folder of that session, or of the session whose
  // line is selected on the board, else of the worktree a new session would start in.
  const terminalAt = useMemo(() => {
    const home = shownSession ? (homes.get(shownSession) ?? allHomes.get(shownSession)) : selected?.kind === 'agent' ? selected : undefined;
    if (home) return { lane: home.lane, session: home.session };
    const lane = startLaneId ? laneById.get(startLaneId) : undefined;
    return lane ? { lane, session: null } : null;
  }, [shownSession, homes, allHomes, selected, startLaneId, laneById]);
  // One that's gone, older than the sidebar's age filter shows or in a worktree no project lists
  // is passed over: the list wouldn't have it. An archived one isn't, whatever its age: Sessions'
  // Archived fold has it, and Mark as done archives the one you've just left for the next.
  const sessionHistory = useSessionHistory(shownSession, (key) => {
    const home = homes.get(key) ?? allHomes.get(key);
    if (!home || (home.lane.hidden && !inProject(home.lane, current))) return false;
    return home.session.archived || home.session.lastActivity >= cutoff || sessionPins.set.has(key);
  }, HISTORY_KEY);
  /** Goes back or forward a session. It shows in Sessions: where it's listed, when the list has it
   *  as it's filtered now, else as a session the bar asks for is shown (an archived one in the
   *  Archived fold). From a session's box, the cursor goes to the box of the one gone to. */
  const travel = useLatest((dir: 'back' | 'forward', box: boolean) => {
    const key = dir === 'back' ? sessionHistory.goBack() : sessionHistory.goForward();
    const home = key ? (homes.get(key) ?? allHomes.get(key)) : undefined;
    if (!key || !home) return;
    const row = activeSessions.find((r) => r.session.key === key) ?? (showArchived ? archivedItems.find((r) => r.session.key === key) : undefined);
    if (row) {
      unfoldSessionRow(row.key);
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
    const title = to ? (homes.get(to) ?? allHomes.get(to))?.session.title : null;
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
    cleanUp: setCleaning,
    boardItems,
    labels: todoStore.labels,
    doneHelpers,
  });
  // What Next takes the window to is one row (useViewQueues.ts): nothing stays picked in any list.
  const unpick = useLatest(() => {
    picks.sessions.clear();
    picks.worktrees.clear();
    picks.prs.clear();
    clearTodoPick();
  });

  // A session moved from Sessions' panel (`moving`): once the board lists its copy, the copy is
  // selected where the list has it, else shown as Open in main window shows a session, since the
  // filters may leave it out. A PR opened from its chip meanwhile stays in the panel, its way
  // back going to the copy. Another row selected or rows picked together let it go, as does a
  // copy that isn't listed within a minute.
  const picking = picks.sessions.rows.length > 0;
  useEffect(() => {
    if (!moving) return;
    if (selected?.key !== moving.from.key || picking) {
      setMoving(null);
      return;
    }
    const copy = moving.to ? homes.get(moving.to) : undefined;
    if (!copy) return;
    setMoving(null);
    const row = activeSessions.find((r) => r.session.key === copy.session.key);
    if (row) {
      unfoldSessionRow(row.key);
      setSelectedKey(row.key);
    } else {
      reveal.current({ kind: 'session', laneId: copy.lane.id, key: copy.session.key });
    }
    // After the reveal, which puts away whatever the panel was showing in the session's place.
    if (drill?.from === moving.from.key) setDrill({ ...drill, from: row?.key ?? sessionItemOf(copy).key });
  }, [moving, selected?.key, picking, homes, activeSessions, unfoldSessionRow, reveal, drill]);
  useEffect(() => {
    if (!moving?.to) return;
    const t = window.setTimeout(() => setMoving(null), COPY_WAIT_MS);
    return () => window.clearTimeout(t);
  }, [moving?.to]);


  // A PR's menus (usePrMenus.ts): its right-click menu with this window's own, the same menu
  // where a worktree or a session shows the PR on a chip, and Review with… dropped from a button.
  const { reviewedPr, reviewMenuFrom, prMenuSpec, prMenu, lanePrMenu, prChipMenu } = usePrMenus({
    board,
    lanes,
    report,
    setToast,
    track,
    approve,
    view,
    selectPr: (r) => picks.prs.select(r, 'one'),
    prPins,
    onDesktop,
    boardItems,
    prRowOf,
    // Read when a menu runs: the way a worktree is shown is made further down.
    focusLane: (lane, row) => focusLane(lane, row),
  });
  /** Opens a worktree's PR on GitHub: a double-click on its chip. */
  const openLanePr = (lane: Lane) => {
    if (lane.pr) openUrl(lane.pr.url).catch((e: unknown) => setToast(String(e)));
  };
  /** A click on a PR's chip, or on a link to it in a panel (PrLink.tsx), wherever this window shows
   *  one. In Sessions, Todo and Kanban, the PR's panel shows in place of the one beside the
   *  list, with the way back to it (`drill`), so the list stays as it was; a chip on a list's row
   *  selects the row first (`row`), so the way back is to its panel. In PRs and on the canvas, its
   *  row or card (`reveal`). A PR the board doesn't list has no panel here, so it opens on GitHub
   *  instead. */
  const showPrOf = (p: Pick<Pr, 'url'>, row?: { key: string; select: () => void }) => {
    const item = board?.prs.find((x) => x.url === p.url);
    if (!item) {
      openUrl(p.url).catch((e: unknown) => setToast(String(e)));
      return;
    }
    const from = row?.key ?? panelFrom;
    if ((view !== 'sessions' && view !== 'todo' && view !== 'kanban') || !from) {
      reveal.current({ kind: 'pr', key: item.key });
      return;
    }
    row?.select();
    setDrill({ from, key: item.key, kind: 'pr' });
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
  /** What this window can do to a session and to a worktree, for their menus and its keys, which
   *  run the same actions (mainCan.ts), from what this render has. */
  const mainHas: MainHas = { report, view, open, board, editors, newSession, todoItems: todoItemsRef, sessionPins, lanePins, toggleLanePin, onDesktop, keptSessions, setRenaming, leftAndOn, setArchived, setArchivedUndo, queue, mergingDone, doneHelpers, selected, setMoving, boardItems, helpersOf, runsOf, sidesOf, helpersShown, toggleHelpers, todoStore, setPreviewFor, openSettings, setCleaning };
  const sessionCan = (lane: Lane, s: Session, pin: 'session' | 'worktree') => sessionCanOf(mainHas, lane, s, pin);
  const sessionMenuSpec = (lane: Lane, s: Session, pin: 'session' | 'worktree'): MenuSpec => menus.sessionMenu(lane, s, sessionCan(lane, s, pin));
  const sessionMenu = (lane: Lane, s: Session, pin: 'session' | 'worktree') => void showContextMenu(sessionMenuSpec(lane, s, pin)).catch((e: unknown) => setToast(String(e)));
  // A helpers chip's card (HelpersCard.tsx, mainHelpers.ts): a helper shown in Sessions, under its
  // session, which lists it first when it doesn't yet (a finished one), with its conversation in
  // the panel; opened where it runs, listed so too, since one Codealong runs opens in its panel
  // here; its menu. And a side conversation's row under its session, or a row of its chip's card:
  // the session selected, and the side conversation above its box, which takes what's typed.
  const movedByKey = useMemo(() => movedTargets(allHomes), [allHomes]);
  const { showSideOf, helperActions } = helperActsOf({ homes, archivedItems, picks, focusBox, facets, setFacets, setHelpersShownList, sessionRows: activeSessions, lanes, openRow, sessionMenu });

  /** A worktree's right-click menu, as every window has it (menus.ts), with this window's own. */
  const laneCan = (lane: Lane) => laneCanOf(mainHas, lane);
  const laneMenuSpec = (lane: Lane): MenuSpec => menus.laneMenu(lane, laneCan(lane));
  // The keys run the same actions, as this render has them.
  const actionsRef = useLatest({ session: (lane: Lane, s: Session, pin: 'session' | 'worktree') => menus.sessionActions(lane, s, sessionCan(lane, s, pin)), lane: (lane: Lane) => menus.laneActions(lane, laneCan(lane)) });
  const laneMenu = (lane: Lane) => void showContextMenu(laneMenuSpec(lane)).catch((e: unknown) => setToast(String(e)));
  // What a card dropped on the Kanban view does it with (kanban.ts): the todo store, the quick
  // panel, a session's own Mark as done, and the toast's Undo.
  const markSessionDone = (r: SessionItem) => {
    const done = actionsRef.current.session(r.lane, r.session, 'session').done;
    if (done) done.run();
    else setToast('It’s archived already.');
  };
  const kanbanActs: KanbanActs = {
    run: todoStore.run,
    start: startForTodo,
    markDone: markSessionDone,
    order: (card, column, index) => {
      if (!shownBoard) return;
      const seq = columnWith(orderedIds(kanbanCols.find((c) => c.id === column)?.cards ?? []), kanbanCardId(card), index);
      void applyBoard({ op: 'order', id: shownBoard.id, order: boardOrderWith(shownBoard.order ?? [], seq) });
    },
    place: (card, column) => {
      if (shownBoard) void applyBoard({ op: 'place', id: shownBoard.id, card: kanbanCardId(card), column });
    },
    said: (message, undo) => setTodoUndo({ message, undo }),
  };
  // What `codealong board …` asks of the main window: a board's Kanban and canvas, moving a card,
  // arranging its canvas (boards/useBoardAsks.ts).
  useBoardAsks({ deviceLanes, deviceFilter, shown: shownBoard, canvas, apply: boards.apply });

  /** Moves a pinned session to insertion point `index` among the pinned ones showing. */
  const movePinnedSession = useCallback(
    (key: string, index: number) => sessionPins.move(pinnedRows.map((r) => r.session.key), key, index),
    [sessionPins, pinnedRows],
  );

  // The worktree headings showing in Sessions grouped by worktree, for ⌃⌘↑ / ⌃⌘↓ reordering.
  const orderedLanes = useMemo(() => (sessionGroup === 'worktree' ? orderedGroupKeys(groupedSecs) : NO_PICKS), [sessionGroup, groupedSecs]);
  const moveGroupLane = useCallback((id: string, index: number) => groupPrefs.placeGroup('worktree', orderedLanes, id, index), [groupPrefs, orderedLanes]);

  // A row selected where its group folds or collapses it away opens its group, once as selected.
  const unfoldedFor = useRef<string | null>(null);
  const [scrollTo, setScrollTo] = useState<string | null>(null);
  useEffect(() => {
    const ask = `${selectedKey}#${revealed}#${view}:${view === 'prs' ? prGroup : view === 'todo' ? todoGroup : `${sessionGroup}:${sessionStarted}`}`;
    if (loading || !selectedKey || unfoldedFor.current === ask) return;
    unfoldedFor.current = ask;
    if (visible.some((r) => r.key === selectedKey)) return;
    const openFolds = (v: string, set: Dispatch<SetStateAction<ReadonlySet<string>>>, folds: string[]) => {
      if (folds.length === 0) return;
      for (const f of folds) groupPrefs.uncollapse(v, f);
      set((o) => (folds.every((k) => o.has(k)) ? o : new Set([...o, ...folds])));
      setScrollTo(selectedKey);
    };
    if (view === 'sessions') {
      const chainKeys = () => {
        const home = homes.get(selectedKey.split('::')[1] ?? '');
        return home ? chainOf(sessionItemOf(home), sessionItemsOf(lanes), sessionStarted === 'under').map((r) => r.key) : [];
      };
      openFolds('sessions', setOpenGroups, sessionGroupFoldsHolding(groupedSecs, selectedKey, chainKeys));
    } else if (view === 'prs') openFolds('prs', setOpenPrGroups, groupFoldsHolding(groupedPrs, selectedKey));
    else if (view === 'todo') openFolds('todo', setOpenTodoGroups, groupFoldsHolding(groupedTodos, selectedKey));
  }, [loading, selectedKey, revealed, visible, view, sessionGroup, sessionStarted, prGroup, todoGroup, homes, lanes, groupedSecs, groupedPrs, groupedTodos, groupPrefs]);
  // A revealed session's section or group opens after filters and age widen to list it (`pendingUnfold`).
  useEffect(() => {
    if (!pendingUnfold) return;
    if (pendingUnfold !== selectedKey || unfoldSessionRow(pendingUnfold)) setPendingUnfold(null);
  }, [pendingUnfold, selectedKey, unfoldSessionRow]);
  useEffect(() => {
    if (!scrollTo) return;
    // Only while it's still the row selected: another selected meanwhile is scrolled to as it's
    // selected, and a later push mustn't take the list back to this one.
    if (scrollTo !== selectedKey) {
      setScrollTo(null);
      return;
    }
    if (!visible.some((r) => r.key === scrollTo)) return;
    document.getElementById(domIdOf(scrollTo))?.scrollIntoView({ block: 'nearest' });
    setScrollTo(null);
  }, [scrollTo, selectedKey, visible]);

  /** Shows a worktree: on the canvas its card; in Sessions grouped by worktree its heading,
   *  selected; else its panel in place of the one beside the list, with the way back to it
   *  (`drill`), from `row` where a row's menu asked (selecting the row), or from what the panel
   *  shows. With no panel to show it from, or in another project than the one shown (`moved`),
   *  selected in Sessions, its heading or not (`chosenLane`). */
  const focusLane = (lane: Lane, row?: { key: string; select: () => void }, moved = false) => {
    if (!moved && view === 'canvas' && shownBoard && canvasEntities.lanes.has(lane.id)) {
      showOnCanvas('worktree', lane.id);
      return;
    }
    if (!moved && view === 'sessions' && unfoldSessionRow(lane.id)) {
      setDrill(null);
      setSelectedKey(lane.id);
      return;
    }
    const from = moved ? null : (row?.key ?? panelFrom);
    if (view !== 'canvas' && from) {
      row?.select();
      setDrill({ from, key: lane.id, kind: 'lane' });
      return;
    }
    setView('sessions');
    clearFilters();
    setQuery('');
    setDrill(null);
    setSelectedKey(lane.id);
  };

  // Where the bar shows: floating, docked in the notch, in the menu bar, or off; changed here, from
  // its own menu or from the Window menu. Picking the place it's in turns it off (useBarPlace.ts).
  const { barPlace, pickBarPlace } = useBarPlace(setToast);

  // What's asked of this window from outside it (useRevealAsks.ts): what the bar, the quick panel
  // or a card on the desktop asks it to show, and a link to a session or a todo clicked in another
  // app, which macOS opens with Codealong: taken once this window hears what it's told to show,
  // and whenever the app says there's one (`takeLink`).
  const takeLink = useRevealAsks({ board, devices, deviceLanes, reveal, setToast, todoStore });

  useMainKeys({
    view,
    views,
    settings: settings ?? usageOf,
    previewFor,
    renaming,
    panelMax,
    // Under a schedule, the session selected there (`scheduleRow`): Return opens it, and its menu's
    // and its worktree's keys act on it.
    selected: view === 'schedules' ? scheduleRow : selected,
    visible,
    pickedRows,
    // The Todo view's keys act on its picks themselves (useTodoView.ts): ⌘A and Esc are the lists' own.
    listPicks: picks.keys ?? (view === 'todo' || view === 'kanban' ? { count: pickedRows.length, step: () => {}, all: pickAllTodos, clear: clearTodoPick, actions: () => null } : null),
    selectedLane: view === 'schedules' ? (scheduleRow?.lane ?? null) : selectedLane,
    startLaneId,
    terminalAt,
    projects,
    pinnedRows,
    orderedLanes,
    pinnedPrRows,
    sessionPins,
    prPins,
    searchRef,
    listRef,
    canvasKeys,
    kanbanKeys,
    gPending,
    todoKeys,
    travel,
    actionsRef,
    openSettings,
    runQueue,
    stepView,
    toggleSidebar,
    pickProject,
    startForTodos,
    startForTodo,
    newSession,
    openFind,
    setQuery,
    setCanvasFit,
    openRow,
    setView,
    togglePanelMax,
    togglePrPanelMax,
    movePinnedSession,
    moveGroupLane,
    move,
    focusBox,
    open,
    copy,
    say: setToast,
    markSeen,
    setSelectedKey,
    zoom,
    reload,
  });

  const toggleBay = (f: Filter) => {
    setBayFilter((fs) => (fs.includes(f) ? fs.filter((x) => x !== f) : [...fs, f]));
    if (view === 'canvas' && !bayFilter.includes(f)) setCanvasFit((n) => n + 1);
  };
  const toggleFacet = (id: FacetId, pick: string) => {
    const adding = !(facets[id] ?? NO_PICKS).includes(pick);
    setFacets((f) => togglePick(f, id, pick));
    if (view === 'canvas' && adding) setCanvasFit((n) => n + 1);
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
  // What the usage bar, its notices and Settings ask for an account (accountActs.ts): each says how
  // it went in a toast.
  const { editConfig, startSignIn, turnOffTraining, resetLimits } = accountActsOf({ setToast, track });

  // What a card on the canvas does (canvas/cardActs.ts): opened where it lives, as a double-click on
  // its row would, and its right-click menu, the one its row has in the other views.
  const { openCard, cardMenu } = cardActsOf({ canvasEntities, todoItems, open, openRow, setToast, laneMenuSpec, sessionMenuSpec, todoMenuSpec, prMenuSpec, prRowOf, showTodo });

  /** Whether a card matches the list's find, the status filter and any labels picked, as its row would in its own view. */
  const canvasMatchesBase = useCallback(
    (it: Pick<RefItem, 'ref' | 'key'>) => cardMatches(it, canvasEntities, todoItems, prRowOf, query, textSearch.said, bayFilter, since),
    [canvasEntities, todoItems, query, textSearch.said, bayFilter, since, prRowOf],
  );
  const canvasMatches = useCallback(
    (it: RefItem) => cardMatches(it, canvasEntities, todoItems, prRowOf, query, textSearch.said, bayFilter, since, facets.labels ?? NO_PICKS, subjects, todoStore.labels),
    [canvasEntities, todoItems, query, textSearch.said, bayFilter, since, prRowOf, facets.labels, subjects, todoStore.labels],
  );
  const canvasFacetOptions = useMemo(() => {
    const cards = (canvas.doc?.items ?? []).filter((it): it is RefItem => it.kind === 'ref');
    return { ...facetOptions, labels: canvasLabelOptions(cards, todoStore.labels, facets, subjects, canvasMatchesBase) };
  }, [canvas.doc?.items, facetOptions, todoStore.labels, facets, subjects, canvasMatchesBase]);

  // In the canvas's panel, a worktree or todo it links to is shown on the canvas when it's there.
  const showLaneFromPanel = (lane: Lane) => {
    if (view === 'canvas' && canvas.has('worktree', lane.id)) setCanvasFocus((f) => ({ ids: [refId('worktree', lane.id)], n: (f?.n ?? 0) + 1 }));
    else focusLane(lane);
  };
  const showSessionFromPanel = (lane: Lane, session: Session) => {
    if (view === 'canvas' && canvas.has('session', session.key)) setCanvasFocus((f) => ({ ids: [refId('session', session.key)], n: (f?.n ?? 0) + 1 }));
    else reveal.current({ kind: 'session', laneId: lane.id, key: session.key });
  };
  const showTodoFromPanel = (t: Todo) => {
    if (view === 'canvas' && canvas.has('todo', t.id)) setCanvasFocus((f) => ({ ids: [refId('todo', t.id)], n: (f?.n ?? 0) + 1 }));
    else {
      // From a session shown in this todo's own panel: back to the todo.
      setDrill(null);
      showTodo(t);
    }
  };

  // The canvas's selected card, which the panel on the right shows on the canvas.
  const canvasPick = view === 'canvas' ? canvasPicked(canvasSel, canvasEntities, todoItems) : null;

  /** Opens a folded group, or folds it again. */
  const toggleIn = (set: Dispatch<SetStateAction<ReadonlySet<string>>>) => (key: string) =>
    set((o) => {
      const next = new Set(o);
      if (!next.delete(key)) next.add(key);
      return next;
    });
  /** A repo's menu, on its heading: its page on GitHub, its pin, its name. */
  const repoMenu = (key: string, repo: RepoName) =>
    void showContextMenu(menus.repoMenu(repo, { report, pin: { label: repoPins.set.has(key) ? 'Unpin repo' : 'Pin repo', run: () => repoPins.toggle(key) } })).catch((e: unknown) => setToast(String(e)));
  /** What a group's heading does (Groups.tsx, mainGroups.ts). */
  const groupsHas: GroupsHas = { focusLane, picks, pressRow, setSelectedKey, open, repoMenu, laneMenu, lanePrMenu, showPrOf, openLanePr };
  const groupActs = viewGroupActs(groupsHas, 'sessions', true, sessionGroup, groupedSecs, groupPrefs, toggleIn(setOpenGroups));
  const prGroupActs = viewGroupActs(groupsHas, 'prs', false, prGroup, groupedPrs, groupPrefs, toggleIn(setOpenPrGroups));
  const todoGroupActs = viewGroupActs(groupsHas, 'todo', false, todoGroup, groupedTodos, groupPrefs, toggleIn(setOpenTodoGroups));
  const scheduleGroupActs = viewGroupActs(groupsHas, 'schedules', false, scheduleGroup, scheduleListed.grouped, groupPrefs, toggleIn(setOpenScheduleGroups));

  // The Board view with no board to show: nothing can be selected, so it has no panel.
  const boardless = (view === 'kanban' || view === 'canvas') && !shownBoard;
  // Which panel is on the right, for its width: the view's own, or on the canvas the selected card's.
  const panelKind = panelKindOf(view, { boardless, scheduleSession: !!scheduleSessionShown, canvasKind: canvasPick?.kind ?? null, selectedKind: selected?.kind });
  const panelWidth = { pr: prPanelWidth, todo: todoPanelWidth, session: sessionPanelWidth, schedule: schedulePanelWidth, lane: inspectorWidth };
  const setPanelWidth = { pr: setPrPanelWidth, todo: setTodoPanelWidth, session: setSessionPanelWidth, schedule: setSchedulePanelWidth, lane: setInspectorWidth };
  // The panel takes the whole window: one session's in Sessions, one PR's in PRs (⇧⌘F).
  const maxed = (panelKind === 'session' && view !== 'kanban' && view !== 'schedules' && panelMax && picks.sessions.rows.length === 0) || (view === 'prs' && selected?.kind === 'pr' && prPanelMax && picks.prs.rows.length === 0);
  // The worktree the preview URL sheet is for, as the latest board has it.
  const previewLane = previewFor ? board?.lanes.find((l) => l.id === previewFor) : undefined;
  // The session the rename sheet is for, in its own worktree.
  const renamed = useMemo(() => homeOf(board?.lanes ?? [], renaming), [renaming, board]);

  // The sidebar: the views with their counts, the statuses to narrow them to, and what narrows
  // the view now, for the collapsed rail's badge.
  // What the board shown holds, in the layout the Board view shows it in, or would: its open cards
  // in Kanban, its cards on the canvas.
  const boardShowing = view === 'kanban' || view === 'canvas';
  const layoutShown = boardShowing ? view : lastLayout;
  const boardCount = layoutShown === 'canvas' ? (canvas.doc?.items.filter((it) => it.kind === 'ref').length ?? '') : openCardCount;
  // The project's schedules that aren't paused or finished, for the sidebar's count.
  const activeSchedules = projectSchedules.filter((x) => !x.paused && x.finished == null).length;
  const sideViews = sideViewsOf(views, layoutShown, { sessions: shownSessionCount, todos: openTodoCount, schedules: activeSchedules, prs: openPrCount, prsComing: githubPrs === 'coming', requested: requestedCount });
  // The project's boards go under the Board view (under All projects, every project's, each
  // saying its project). The one it shows has the view's keys beside it, which go back to it, and
  // its count while it shows.
  const sideBoards = sideBoardsOf(boards.boards, { current, labels: todoStore.labels, shown: shownBoard?.id, showing: boardShowing, count: boardCount, layoutShown });
  const sideStatuses = sideStatusesOf(counts);
  const ageApplies = ageAppliesIn(view);
  // The facets the view offers, and the picks among them that narrow it now, by their names.
  // The Kanban's and canvas's labels count their sessions too (boardLists.ts `todoLabelOptions`).
  const sideFacets = sideFacetsOf(view, view === 'kanban' ? kanbanFacetOptions : view === 'canvas' ? canvasFacetOptions : facetOptions, facets);
  const facetsNow = Object.fromEntries(facetsOn(facets, view).map((id) => [id, facets[id]])) as Facets;
  const pickNames = pickNamesOf(sideFacets);
  const viewNow: ViewState = { view, project: current, device: deviceFilter, age, filter: bayFilter, facets: facetsNow };
  const savedOn = savedViews.find((v) => sameView(v, viewNow))?.id ?? null;
  const deviceName = (id: string) => (id === 'here' ? devices?.me.name || 'This device' : (devices?.devices.find((d) => d.short === id)?.name ?? null));
  // What narrows the view now (sideLists.ts): in words for the collapsed rail's badge, and as chips
  // on the list's strip (ListBar.tsx `FilterChips`), each with the way to take it off, so what
  // narrows the list shows with the sidebar folded away.
  const filtersOn: FiltersOn = { devices: deviceFilter, deviceName, ageApplies, age, statuses: bayFilter, sideStatuses, facets: sideFacets };
  const sideFilters = sideFiltersOf(filtersOn, pickNames);
  const listFilters = listFiltersOf(filtersOn, { device: toggleDevice, age: () => setAge('all'), status: toggleBay, facet: (facet, pick) => setFacets((f) => togglePick(f, facet, pick)) });
  const clearListFilters = () => {
    setDeviceFilter([]);
    if (ageApplies) setAge('all');
    setBayFilter([]);
    setFacets((f) => ({ ...f, ...Object.fromEntries(facetsIn(view).map((id) => [id, []])) }));
  };
  const filterChips = <FilterChips filters={listFilters} onClearAll={clearListFilters} />;
  const viewSuggestion = viewSuggestionOf(view, current, filtersOn, pickNames);
  const applySaved = (v: SavedView) => {
    // Every field is checked against what exists now: a project or device can be gone since.
    if (v.project !== current && (v.project === ALL_PROJECTS || projects.some((p) => p.name === v.project))) pickProject(v.project);
    const known = views.find((x) => x.id === v.view);
    if (known) setView(known.id);
    else if (v.view === 'worktrees') {
      setView('sessions');
      setSessionGroup('worktree');
    }
    setDeviceFilter(v.device.filter((id) => id === 'here' || devices?.devices.some((d) => d.short === id)));
    const a = AGES.find((x) => x.id === v.age);
    if (a) setAge(a.id);
    setBayFilter(v.filter);
    setFacets({ ...v.facets, labels: v.facets?.labels?.filter((id) => labelById.has(id)) });
  };
  const savedMenu = (v: SavedView) => void showContextMenu(savedViewMenu(v, { views: savedViews, now: viewNow, saved: savedOn !== null, keep: keepSaved })).catch((e: unknown) => setToast(String(e)));

  // PR links in conversations and descriptions (PrLink.tsx): a click shows the PR as its chip
  // does, and their cards show the PR and its sessions here. A link to a session or a todo
  // (SessionLink.tsx) shows it here the same way.
  // Its worktrees with their archived sessions, which a PR lists with the rest.
  usePrLinks(board?.prs, rawLanes, {
    report,
    showPr: (pr, tab) => reveal.current({ kind: 'pr', key: pr.key, tab: tab ?? undefined }),
    followPr: (pr, tab) => {
      if (tab) showPrTab(pr.key, tab);
      showPrOf(pr);
    },
    showSession: (lane, session) => reveal.current({ kind: 'session', laneId: lane.id, key: session.key }),
    showTodo: (todo) => reveal.current({ kind: 'todo', id: todo.id }),
    openSession: (lane, session) => open(lane, session.openTarget),
    sessionMenu: (lane, s) => sessionMenu(lane, s, 'worktree'),
    prMenu: (url, pr) => {
      if (pr) prMenu(prRowOf(pr));
      else void showContextMenu(menus.prMenu({ url, number: prRefOf(url)?.number ?? 0 }, { report })).catch((e: unknown) => setToast(String(e)));
    },
  }, board, deviceLanes);

  // Everything in it says a worktree's or a PR's project where it isn't the one shown
  // (ProjectSaid.tsx): under All projects, everywhere.
  const page = (
    <div className="ca-win ca-win--app">
      <MainBar
        board={board}
        now={now}
        sideCollapsed={sideCollapsed}
        onToggleSidebar={toggleSidebar}
        history={sessionHistory}
        historyTitle={historyTitle}
        onTravel={(dir) => travel.current(dir, false)}
        switcher={<ProjectSwitcher logoStatus={logoStatus} projects={projects} current={current} total={totalLanes} actions={actions} onPick={pickProject} onOpenWindow={switcherWindow} onAddProject={() => openSettings('projects', NEW_PROJECT)} onEditProjects={() => openSettings('projects')} />}
        onQuick={() => newSession({ laneId: startLaneId })}
        onShowListening={(l) => reveal.current({ kind: 'session', laneId: l.laneId, key: l.key })}
        onListen={() => void showQuick({ listen: true, project: projectNow.current, opener: windowLabel() })}
        onMeetings={() => setMeetingsOpen(true)}
        barPlace={barPlace}
        onBarPlace={pickBarPlace}
        pickedTodos={pickedRows.length}
        todoSelected={selected?.kind === 'todo'}
        onNew={() => (pickedRows.length > 0 ? startForTodos(pickedRows) : selected?.kind === 'todo' ? startForTodo(selected) : newSession({ laneId: startLaneId }))}
        onImprove={() => void showQuick({ improve: true, project: projectNow.current, opener: windowLabel() })}
      />
      <div
        className={cx('ca-win-body', sideCollapsed && 'is-railed', panelKind === 'pr' && 'is-prs', panelKind === 'todo' && 'is-todo', panelKind === 'schedule' && 'is-schedule', panelKind === 'session' && 'is-session', maxed && 'is-panel-max', view === 'canvas' && 'is-canvas', view === 'canvas' && canvasPick && 'has-panel', boardless && 'is-boardless', reading && 'is-loading')}
        style={{ '--sidebar-w': widthVar(sidebarWidth), '--panel-w': widthVar(panelKind ? panelWidth[panelKind] : null) } as CSSProperties}
      >
        <Sidebar
          collapsed={sideCollapsed}
          onExpand={toggleSidebar}
          now={now}
          views={sideViews}
          view={view === 'kanban' || view === 'canvas' ? 'board' : view}
          onView={(id) => setView(id === 'board' ? lastLayout : (id as View))}
          boards={sideBoards}
          board={shownBoard?.id ?? null}
          onBoard={(id) => (id ? showBoard(id) : setView(lastLayout))}
          onBoardMenu={(id) => boardMenu(id === null ? null : (boards.boards.find((b) => b.id === id) ?? null))}
          onNewBoard={() => setBoardEdit({ board: null })}
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
          facets={sideFacets}
          onFacet={toggleFacet}
          onClearFacet={(id) => setFacets((f) => ({ ...f, [id]: [] }))}
          saved={savedViews}
          savedOn={savedOn}
          onApplySaved={applySaved}
          onSavedMenu={savedMenu}
          suggestion={viewSuggestion}
          onSave={(name) => keepSaved([...savedViews, { id: newSavedId(), name, ...viewNow }])}
          filters={sideFilters}
          onSettings={() => openSettings()}
          onAnalytics={() => setUsageOf({ account: null })}
          onGitHub={openRepo}
          loading={loading}
          foot={
            <div className="ca-side-foot">
              <Button variant="ghost" size="sm" icon="settings" className="ca-side-config" aria-label="Settings" title={keysIn('Settings (⌘,)')} onClick={() => openSettings()} />
              <Button variant="ghost" size="sm" icon="chart-bar" className="ca-side-analytics" aria-label="Analytics" title={analyticsTitle()} onClick={() => setUsageOf({ account: null })} />
              <Button variant="ghost" size="sm" icon="brand-github" className="ca-side-github" aria-label="Codealong on GitHub" title="Codealong on GitHub" onClick={openRepo} />
              <ThemeSwitch theme={theme} onPick={pickTheme} />
            </div>
          }
        />
        {sideCollapsed ? <span aria-hidden="true" /> : <Resizer panel="before" cssVar="--sidebar-w" onResize={setSidebarWidth} />}
        {/* Where you are for the finds (findKeys.ts): ⌘F starts here, and ⌥⌘F filters the list from here, but not while a panel takes the whole window. */}
        <main ref={boardRef} className={cx('ca-board', view === 'canvas' && 'is-canvas', view === 'kanban' && 'is-kanban')} aria-label={VIEWS.find((v) => v.id === view)?.label} data-find={maxed ? undefined : 'window'}>
          {/* One strip in every view, so the find keeps the keyboard as the view changes under it. */}
          {!loading && (view === 'sessions' || view === 'prs' || view === 'todo' || view === 'schedules' || ((view === 'kanban' || view === 'canvas') && shownBoard) || findShown) && (
            <ListBar>
              {(view === 'kanban' || view === 'canvas') && shownBoard && (
                <BoardBar board={shownBoard} project={shownBoard.project !== current ? shownBoard.project : null} layout={boardLayout} labels={todoStore.labels} onLayout={setView} onMenu={() => boardMenu()} onEdit={() => setBoardEdit({ board: shownBoard })} />
              )}
              {(view === 'kanban' || view === 'canvas') && shownBoard && filterChips}
              {view === 'sessions' && (
                <SessionsBar
                  sort={sessionSort}
                  onSort={setSessionSort}
                  group={sessionGroup}
                  plainGroup={leftOnWorktrees ? 'worktree' : 'none'}
                  onGroup={setSessionGroup}
                  started={sessionStarted}
                  onStarted={setSessionStarted}
                  lines={sessionLines}
                  onLines={setSessionLines}
                  filters={filterChips}
                />
              )}
              {view === 'prs' && (
                <Tabs
                  className="ca-listtabs"
                  label="Whose PRs"
                  idBase="ca-prtabs"
                  tabs={PR_TABS.map((t) => ({ ...t, count: prCounts[t.id] || null, title: prCounts[t.id] ? `${t.title}. ${prCounts[t.id]} ${prCounts[t.id] === 1 ? 'needs' : 'need'} you` : t.title }))}
                  value={prTab}
                  onPick={setPrTab}
                />
              )}
              {view === 'prs' && (
                <ListTools label="How PRs are listed">
                  <ViewMenu
                    label="How PRs are listed"
                    settings={[viewSetting('Sort', PR_SORTS, prSort, 'status', setPrSort, byWords), viewSetting('Group', PR_GROUPS, prGroup, 'none', setPrGroup, byWords), rowsSetting(PR_ROWS, prLines, setPrLines)]}
                  />
                  {filterChips}
                </ListTools>
              )}
              {view === 'todo' && (
                <ListTools label="How Todo is listed">
                  <ViewMenu
                    label="How Todo is listed"
                    settings={[viewSetting('Sort', TODO_SORTS, todoSort, 'status', setTodoSort, byWords), viewSetting('Group', TODO_GROUPS, todoGroup, 'none', setTodoGroup, byWords), rowsSetting(TODO_ROWS, todoLines, setTodoLines)]}
                  />
                  {filterChips}
                </ListTools>
              )}
              {view === 'schedules' && (
                <ListTools label="How Schedules is listed">
                  <ViewMenu
                    label="How Schedules is listed"
                    settings={[viewSetting('Sort', SCHEDULE_SORTS, scheduleSort, 'next', setScheduleSort, byWords), viewSetting('Group', SCHEDULE_GROUPS, scheduleGroup, 'none', setScheduleGroup, byWords), rowsSetting(SCHEDULE_ROWS, scheduleLines, setScheduleLines)]}
                  />
                </ListTools>
              )}
              {findShown && (
                <ListFind
                  inputRef={searchRef}
                  value={query}
                  onChange={setQuery}
                  placeholder={FIND_IN[view]}
                  onLeave={() => setFindOpen(false)}
                  onClose={() => {
                    setQuery('');
                    setFindOpen(false);
                  }}
                />
              )}
              {/* Offered while the view's project, or All projects, has none running: once one runs,
                  its session leads the list and its menu turns it off. */}
              {view === 'sessions' && freeOrchestrators.length > 0 && <OrchestratorButton title={compassTitle(freeOrchestrators)} run={() => setOrchestratorAsk({ scopes: freeOrchestrators })} />}
            </ListBar>
          )}
          <div className="ca-board-body">
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
          {board && !board.loading && view === 'prs' && prRows.length === 0 && !prsComing && (
            <div className="ca-board-empty">
              {query
                ? `No PRs in ${current} match “${query}”.`
                : githubPrs === 'failing'
                  ? `PRs can't be read: ${board.notices.find((n) => n.key === 'github')?.title ?? "GitHub didn't answer"}.`
                  : 'No PRs here right now.'}
            </div>
          )}
          {loadError && <div className="ca-board-empty">{loadError}</div>}
          {/* One reading from launch until the PRs view has its PRs, so its tip carries on through. */}
          {reading && <Reading what={loading ? READING[view] : 'Reading PRs from GitHub…'} place={view === 'prs' ? 'pr' : 'board'} className="ca-board-loading" />}
          {board && !board.loading && view === 'sessions' && !hasSessions && (
            <div className="ca-board-empty">
              {query
                ? `No sessions in ${current} match “${query}”${textSearch.building ? ' yet: Codealong is still reading what recent sessions said' : ''}.`
                : bayFilter.length > 0 || facetsOn(facets, view).length > 0
                  ? 'No sessions here right now.'
                  : cutoff > 0
                    ? `No sessions ${AGES.find((a) => a.id === age)?.title.toLowerCase()}.`
                    : 'No sessions in the last 14 days.'}
            </div>
          )}
          {(view === 'kanban' || view === 'canvas') && !loading && boards.read && !shownBoard && <BoardEmpty project={current} error={boards.error} onNew={() => setBoardEdit({ board: null })} />}
          {canvasShown && board && shownBoard && canvas.doc && (
            <CanvasView
              doc={canvas.doc}
              update={canvas.update}
              entities={canvasEntities}
              holds={holds}
              onTakeOff={(cards) => void applyBoard({ op: 'remove', id: shownBoard.id, cards })}
              onPutOn={(cards) => void applyBoard({ op: 'add', id: shownBoard.id, cards })}
              onSettings={() => setBoardEdit({ board: shownBoard })}
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
              filtering={!!query.trim() || bayFilter.length > 0 || facetsOn(facets, 'canvas').length > 0}
              matches={canvasMatches}
              fitMatches={canvasFit}
            />
          )}
          {view === 'schedules' && board && !loading && (
            <SchedulesView
              listing={scheduleListed}
              groupActs={scheduleGroupActs}
              sort={scheduleSort}
              slim={scheduleLines === 'one'}
              harnessItems={shownHarnessSchedules}
              problem={board.schedulesProblem}
              now={now}
              selectedId={selectedHarness?.key ?? selectedSchedule?.id ?? null}
              selectedSession={scheduleSessionShown?.session.key ?? null}
              sessionTitle={sessionTitle}
              sessionsOf={(x) => scheduleSessions(x, allHomes, schedulesAll.has(x.id))}
              placed={(key) => allHomes.get(key) ?? null}
              allShown={schedulesAll}
              showFinished={showFinished}
              onToggleFinished={() => setShowFinished(!showFinished)}
              onToggleAll={(id) =>
                setSchedulesAll((all) => {
                  const next = new Set(all);
                  if (!next.delete(id)) next.add(id);
                  return next;
                })
              }
              // A press on the row whose panel shows a worktree in its place brings its own back.
              onSelect={(id) => {
                pressRow(`schedule:${id}`, 'one');
                setScheduleId(id);
                setScheduleSession(null);
              }}
              onSelectSession={(id, p) => {
                pressRow(`schedule:${id}/${p.session.key}`, 'one');
                setScheduleId(id);
                setScheduleSession(p.session.key);
              }}
              onOpenSession={(p) => open(p.lane, p.session.openTarget)}
              onSessionMenu={(p) => sessionMenu(p.lane, p.session, 'session')}
              onOpenHarness={openHarness}
              onEdit={(item) => setScheduleEdit(scheduleOf(item))}
              onMenu={scheduleMenu}
              onNew={newSchedule}
            />
          )}
          {view !== 'canvas' && view !== 'schedules' && !(view === 'kanban' && !shownBoard) && !loading && (
          <div
            role="listbox"
            // The PRs' tabs control the list they show.
            id={view === 'prs' ? `ca-prtabs-${prTab}` : undefined}
            aria-label={view === 'sessions' ? 'Sessions' : view === 'prs' ? 'Pull requests' : view === 'todo' ? 'Todos' : 'Todos and sessions'}
            aria-activedescendant={selected ? domId(selected) : undefined}
            aria-multiselectable
            tabIndex={0}
            ref={listRef}
            className="ca-board-list"
            data-drag-list
          >
            {view === 'kanban' && board && shownBoard && (
              <KanbanView
                columns={kanbanCols}
                todos={todoStore.todos}
                now={now}
                selectedKey={selected?.key ?? null}
                picked={pickedKeys}
                domId={domId}
                open={columnOpen}
                onFold={toggleColumn}
                onColumnMenu={columnMenu}
                onAddColumn={() => setColumnEdit({ column: null })}
                onSelect={(c, how) => {
                  pressRow(c.key, how);
                  // Only todos are picked together; a session's card is selected alone.
                  if (c.kind === 'todo') return selectTodoRow(c, how);
                  clearTodoPick();
                  setSelectedKey(c.key);
                }}
                onOpen={(c) => {
                  if (c.kind === 'todo') selectTodoRow(c, 'one');
                  else setSelectedKey(c.key);
                  openRow(c);
                }}
                onContextMenu={(c) => {
                  // On one of the todos picked, the menu is for all of them.
                  if (c.kind === 'todo' && pickedKeys.has(c.key)) pickMenu(pickedRows);
                  else if (c.kind === 'todo') todoMenu(c);
                  else sessionMenu(c.lane, c.session, 'session');
                }}
                onShowPr={(p, c) => showPrOf(p, { key: c.key, select: () => setSelectedKey(c.key) })}
                onOpenPr={(p) => openUrl(p.url).catch((e: unknown) => setToast(String(e)))}
                onDrop={(c, drop) => applyKanbanDrop(c, drop, kanbanActs, shownBoard.placed?.[kanbanCardId(c)] ?? null)}
                dropContext={{ columns: columnsOf(shownBoard), placed: shownBoard.placed, subjects, todos: todoStore.todos, prs: board.prs, now }}
                canReorder
                composerColumn={composerColumn}
                hints={board.decisions?.todos ? board.todoHints : undefined}
                composer={todoComposer}
              />
            )}
            {view === 'todo' && board && (
              <TodoView
                sections={todoSecs}
                groups={groupedTodos}
                groupActs={todoGroupActs}
                todos={todoStore.todos}
                sort={todoSort}
                lines={todoLines}
                now={now}
                selectedKey={selected?.key ?? null}
                picked={pickedKeys}
                domId={domId}
                canReorder={todoSort === 'status' && todoGroup === 'none'}
                onSelect={(r, how) => {
                  pressRow(r.key, how);
                  selectTodoRow(r, how);
                }}
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
                onShowPr={(p, r) => showPrOf(p, { key: r.key, select: () => selectTodoRow(r, 'one') })}
                onOpenPr={(p) => openUrl(p.url).catch((e: unknown) => setToast(String(e)))}
                onFilterLabel={(l) => setFacets((f) => onlyPick(f, 'labels', l.id))}
                onMove={moveTodo}
                hints={board.decisions?.todos ? board.todoHints : undefined}
                showDone={showDoneTodos}
                onToggleDone={() => setShowDoneTodos(!showDoneTodos)}
                showArchived={showArchivedTodos}
                onToggleArchived={() => setShowArchivedTodos(!showArchivedTodos)}
                composer={todoComposer}
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
                groups={groupedPrs}
                groupActs={prGroupActs}
                sort={prSort}
                onShowRepo={(slug) => setFacets((f) => onlyPick(f, 'repos', slug))}
                lines={prLines}
                now={now}
                selectedKey={selected?.key ?? null}
                picked={picks.prs.keys}
                domId={domId}
                showDone={showDonePrs}
                onToggleDone={() => setShowDonePrs(!showDonePrs)}
                onSelect={(r, how) => {
                  pressRow(r.key, how);
                  picks.prs.select(r, how);
                }}
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
                orchestrators={orchestratorRows}
                listening={listeningRows}
                pinned={pinnedRows}
                topHelpers={topHelpers}
                sections={sessionSecs}
                groups={groupedSecs}
                groupActs={groupActs}
                sort={sessionSort}
                lines={sessionLines}
                now={now}
                selectedKey={selected?.key ?? null}
                picked={picks.sessions.keys}
                domId={domId}
                onSelect={(r, how) => {
                  pressRow(r.key, how);
                  picks.worktrees.clear();
                  picks.sessions.select(r, how);
                  if (how === 'one') focusBox();
                }}
                onPickThese={(rows) => {
                  picks.worktrees.clear();
                  picks.sessions.these(rows.map((r) => r.key));
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
                onPrShow={(r) => r.lane.pr && showPrOf(r.lane.pr, { key: r.key, select: () => picks.sessions.select(r, 'one') })}
                onPrOpen={(r) => openLanePr(r.lane)}
                onPlacePinned={(key, index) => sessionPins.place(pinnedRows.map((r) => r.session.key), key, index)}
                pinnedKeys={sessionPins.set}
                onUnpin={sessionPins.remove}
                desktop={(r) => (r.lane.device || r.session.archived ? null : { ref: 'session', key: r.session.key })}
                archived={archivedItems}
                sectionFolded={sectionFolds.isFolded}
                sectionToggle={sectionFolds.toggleFor}
                todosFor={todosFor}
                helpersOf={helpersOf}
                helpersShown={helpersShown}
                onToggleHelpers={toggleHelpers}
                runsOf={runsOf}
                onToggleRuns={(key) => toggleHelpers(runsShownKey(key))}
                sidesOf={sidesOf}
                sideGlancesOf={sideGlancesOf}
                onToggleSides={(key) => toggleHelpers(sidesShownKey(key))}
                onShowSide={showSideOf}
                helperActions={helperActions}
                onShowTodo={showTodo}
                onFilterLabel={(l) => setFacets((f) => onlyPick(f, 'sessionLabels', labelPick(l)))}
                saidOf={saidOf}
                movedToOf={(key) => movedByKey.get(key)}
                onShowSession={(p) => showSessionFromPanel(p.lane, p.session)}
              />
            )}
          </div>
          )}
          </div>
        </main>
        {/* Until the board is read, or the PRs view's PRs, the panel has nothing to show either: the board takes its room. */}
        {panelKind && !reading && <Resizer panel="after" cssVar="--panel-w" onResize={setPanelWidth[panelKind]} />}
        {!reading && !boardless && (
          <MainPanel
            view={view}
            selected={selected}
            findIn={findIn}
            selectedLane={selectedLane}
            canvasPicked={canvasPick}
            board={board}
            now={now}
            loading={loading}
            laneById={laneById}
            homes={homes}
            devices={devices}
            projectTodos={projectTodos}
            todoStore={todoStore}
            todoLinks={todoLinks}
            todosFor={todosFor}
            prRowOf={prRowOf}
            pickedRows={pickedRows}
            picks={picks}
            todo={{ startForTodo, startForTodos, todoMenu, todoLinkMenu, todoMoreMenu, labelMenu, projectsForTodos, duePick, duePicked, selectTodoRow, pickMenu, todoEdit, pickDueHere }}
            drill={drill}
            setDrill={setDrill}
            queue={queue}
            views={viewQueues}
            boxFocus={boxFocus}
            panelMax={panelMax}
            togglePanelMax={togglePanelMax}
            prPanelMax={prPanelMax}
            togglePrPanelMax={togglePrPanelMax}
            actionsRef={actionsRef}
            open={open}
            newSession={newSession}
            newSessionPanel={
              newShown && (
                <NewSessionPanel
                  key={newShown.opened}
                  draft={newShown.draft}
                  lane={[...(board?.lanes ?? []), ...deviceLanes].find((l) => l.id === newShown.draft.spec.laneId) ?? null}
                  sending={newShown.sending}
                  error={newShown.error}
                  started={!!newShown.started}
                  onSend={sendNew(newShown)}
                  onClose={() => setNewShown(null)}
                />
              )
            }
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
            reviewMenuFrom={reviewMenuFrom}
            reviewedPr={reviewedPr}
            setPreviewFor={setPreviewFor}
            setToast={setToast}
            report={report}
            showSchedule={(id) => reveal.current({ kind: 'schedule', id })}
            scheduleSession={scheduleSessionShown && (selectedSchedule || selectedHarness) ? { ...scheduleSessionShown, back: { label: selectedHarness?.name ?? selectedSchedule?.name ?? '', onBack: () => setScheduleSession(null) } } : null}
            scheduleFrom={view === 'schedules' && panelFrom ? { key: panelFrom, label: scheduleSessionShown?.session.title ?? selectedHarness?.name ?? selectedSchedule?.name ?? '' } : null}
            schedulePanel={
              selectedHarness ? (
                <HarnessSchedulePanel item={selectedHarness} now={now} sessionOf={(key) => homes.get(key) ?? allHomes.get(key) ?? null} onShowSession={showSessionFromPanel} onOpen={() => openHarness(selectedHarness)} />
              ) : (
              <SchedulePanel
                item={selectedSchedule}
                now={now}
                sessionOf={(key) => homes.get(key) ?? null}
                onShowSession={showSessionFromPanel}
                onRun={() => selectedSchedule && runSchedule(selectedSchedule)}
                onPause={(paused) => selectedSchedule && pauseSchedule(selectedSchedule, paused)}
                onEdit={() => selectedSchedule && setScheduleEdit(scheduleOf(selectedSchedule))}
                onMenu={(at) => selectedSchedule && scheduleMenu(selectedSchedule, at)}
              />
              )
            }
          />
        )}
      </div>
      {/* The usage bar is the window's status bar: what every account has left, what's under way
          and what isn't working, at its foot, out of the way of what you read and act on. */}
      <UsageBar
        usage={board?.usage ?? []}
        loading={loading}
        now={now}
        onRefresh={() => void refreshUsage()}
        onUsage={(account) => setUsageOf({ account: account ?? null })}
        onSignIn={startSignIn}
        training={board?.training}
        load={board?.load}
        machines={devices?.enabled ? devices.devices : undefined}
        onStopTraining={turnOffTraining}
        resets={board?.resets}
        onUseReset={resetLimits}
        onOpenUrl={(url) => openUrl(url).catch((e: unknown) => setToast(String(e)))}
        onReorder={(accounts) =>
          orderAccounts(accounts).catch((e: unknown) => {
            setToast(String(e));
            throw e;
          })
        }
        toast={<Toasts state={toasts} onUnarchive={(u) => setArchived(u.keys, false, u.title)} />}
        notices={
          <>
            {/* The meeting about to start (meetings.rs), to take its notes: where there's room for it
                whatever the window's width, beside what else the window has to say. */}
            {board?.meeting && <MeetingOffered offer={board.meeting} look="line" />}
            <Notices
            notices={notices}
            now={now}
            onDismiss={(n) => setDismissed((d) => [...d, noticeId(n)])}
            onAction={(a) => ('opens' in a ? openSettings('file') : 'signsIn' in a ? startSignIn(a.signsIn) : tryAgain(a.reviews, { error: setToast, info: setToast, busy: track }))}
          />
          </>
        }
      />
      {settings && (
        <SettingsDialog
          section={settings.section}
          onSection={(section) => setSettings({ section, focus: null })}
          focus={settings.focus}
          usage={board?.usage ?? []}
          training={board?.training}
          mcp={board?.mcp}
          signIns={board?.signIns}
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
      {orchestratorAsk && (
        <StartOrchestratorDialog
          scopes={orchestratorAsk.scopes}
          others={(board?.schedules ?? []).filter((i) => orchestratorOn(i) && i.orchestrates?.project).map((i) => i.orchestrates?.project ?? '')}
          onStart={(project) => startOrchestrator(project)}
          onClose={() => setOrchestratorAsk(null)}
        />
      )}
      {closeStale && <CloseStaleDialog onClose={() => setCloseStale(false)} onDone={setToast} />}
      {setup && <SetupDialog onClose={() => setSetup(false)} onDone={setToast} onSettings={(section) => openSettings(section)} now={now} />}
      {usageOf && board && (
        <UsageDialog
          board={board}
          account={usageOf.account}
          now={now}
          onClose={() => setUsageOf(null)}
          onShowSession={(key) => {
            const lane = board.lanes.find((l) => !l.device && l.sessions.some((s) => s.key === key));
            if (!lane) return;
            setUsageOf(null);
            reveal.current({ kind: 'session', laneId: lane.id, key });
          }}
          onAccounts={(account) => {
            setUsageOf(null);
            openSettings('accounts', account ?? null);
          }}
        />
      )}
      {previewLane && <PreviewDialog lane={previewLane} onClose={() => setPreviewFor(null)} />}
      {cleaning && <CleanUpDialog lanes={cleaning} onClose={() => setCleaning(null)} onDone={setToast} />}
      {renamed && <RenameDialog lane={renamed.lane} session={renamed.session} onClose={() => setRenaming(null)} onFailed={setToast} />}
      {snoozing && <SnoozeDialog ask={snoozing} what={askWords(snoozing, (key) => homeOf([...(board?.lanes ?? []), ...deviceLanes], key)?.session.title)} onClose={() => setSnoozing(null)} onFailed={setToast} />}
      {mergingDone.dialog}
      {askingHelpers.dialog}
      {scheduleEdit && board && <ScheduleDialog schedule={scheduleEdit} board={board} devices={devices} now={now} onClose={() => setScheduleEdit(null)} onSaved={setScheduleId} />}
      {boardEdit && <BoardDialog board={boardEdit.board} project={current} onSave={saveBoard} onClose={() => setBoardEdit(null)} {...ruleChoices} />}
      {columnEdit && <ColumnDialog column={columnEdit.column} onSave={saveColumn} onClose={() => setColumnEdit(null)} {...ruleChoices} />}
      {meetingsOpen && (
        <MeetingsDialog
          onClose={() => setMeetingsOpen(false)}
          onListen={(meeting) => {
            setMeetingsOpen(false);
            void showQuick({ listen: true, meeting, project: projectNow.current, opener: windowLabel() });
          }}
          onShowSession={(key) => {
            const home = homeOf(board?.lanes ?? [], key);
            if (home) reveal.current({ kind: 'session', laneId: home.lane.id, key });
            return home !== null;
          }}
        />
      )}
      <ListenHost here />
    </div>
  );
  const projectIcons = useMemo(() => projectIconMap(projects), [projects]);
  return (
    <ProjectShown.Provider value={current}>
      <ProjectIcons.Provider value={projectIcons}>{page}</ProjectIcons.Provider>
    </ProjectShown.Provider>
  );
}

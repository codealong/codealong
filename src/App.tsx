// The main window. One project at a time, picked in the switcher at top left. The sidebar
// lists that project's worktrees. The main area has three views: Worktrees lists pinned
// worktrees in your order, then every other worktree once, in Needs you, Working, Your turn, In
// review, Idle or Merged, with a line for each agent that needs you, is working or just
// finished; Sessions lists every session, pinned ones first in your order; PRs lists the
// project's pull requests. Keyboard map: the design system's Surfaces page.

import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { showContextMenu } from './contextMenu';
import { AGENT_BAY, BAYS, buildBays, failingPr, filterByAge, inboxCounts, lanePromptAt, narrowed, rowItems, worktreeRow, type BoardRow, type Filter, type WorktreeRow } from './lanes';
import { approvePr, archiveSession, barOn, getBoard, markSeen, onBarOn, onBoard, onNewWindow, onReveal, onSettings, openConfig, openProjectWindow, openTarget, openUrl, refresh, refreshUsage, reviewInCodex, setBarOn, setPinned, setPinnedWorktrees, setTheme, signIn, startSession, type Reveal } from './api';
import { usePerf } from './perf';
import { usePins } from './pins';
import type { Bay, Board, Lane, PrItem, Session, StartSpec, Theme } from './types';
import { SettingsDialog, type SettingsSection } from './ui/SettingsDialog';
import { Inspector } from './ui/Inspector';
import { NewSessionSheet, type SheetDraft } from './ui/NewSessionSheet';
import { LaneRow } from './ui/LaneRow';
import { PinnedSection, type PinDrag } from './ui/PinnedSection';
import { BayHeader, Button, Icon, Kbd, NavItem, StatusGlyph } from './ui/primitives';
import { ALL_PROJECTS, openSwitcher, ProjectSwitcher, type ActionCounts } from './ui/ProjectSwitcher';
import { PrInspector } from './ui/PrInspector';
import { prIsOpen, prPlaces, prSections, PrsView, type PrRowItem } from './ui/PrsView';
import { Resizer, usePanelWidth } from './ui/Resizer';
import { SessionsView, timeSections, type SessionItem } from './ui/SessionsView';
import { applyTheme, ThemeSwitch } from './ui/ThemeSwitch';
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
const SIDEBAR_WIDTH_KEY = 'codealong.sidebarWidth';
const INSPECTOR_WIDTH_KEY = 'codealong.inspectorWidth';
const PR_PANEL_WIDTH_KEY = 'codealong.prPanelWidth';
const DAY = 24 * 60 * 60 * 1000;

type View = 'worktrees' | 'sessions' | 'prs';
const VIEWS: { id: View; label: string }[] = [
  { id: 'worktrees', label: 'Worktrees' },
  { id: 'sessions', label: 'Sessions' },
  { id: 'prs', label: 'PRs' },
];
/** How recently a worktree or session must have been active to show. */
type Age = '1d' | '3d' | '7d' | 'all';
const AGES: { id: Age; label: string; title: string; ms: number }[] = [
  { id: '1d', label: '24h', title: 'Active in the last 24 hours', ms: DAY },
  { id: '3d', label: '3d', title: 'Active in the last 3 days', ms: 3 * DAY },
  { id: '7d', label: '7d', title: 'Active in the last 7 days', ms: 7 * DAY },
  { id: 'all', label: 'All', title: 'Everything from the last 14 days', ms: 0 },
];

type Row = BoardRow | PrRowItem;

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
    case 'failing':
      return !!failingPr(r.lane);
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
      return prIsOpen(p) && (p.reviewRequested || (p.mine && (p.checks.failed > 0 || p.state === 'changes')));
    case 'working':
    case 'unread':
      // As on the board, a session from another lane doesn't lend its status: one agent that
      // made many worktrees would otherwise put all of their PRs in Working.
      return r.sessions.some(({ session }) => !session.visitor && session.status === f);
    case 'review':
      return prIsOpen(p);
    case 'failing':
      return prIsOpen(p) && p.checks.failed > 0;
    default:
      return false;
  }
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
    return VIEWS.some((x) => x.id === v) ? (v as View) : 'worktrees';
  } catch {
    return 'worktrees';
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

/** A dragged panel width for the grid's style; left out, the panel takes its default from the CSS. */
const widthVar = (width: number | null) => (width == null ? undefined : `${width}px`);

export function App() {
  const [board, setBoard] = useState<Board | null>(null);
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
  // Archives made here (key → archived), shown until the board catches up: Codex takes a moment.
  const [archiveOverride, setArchiveOverride] = useState<Record<string, boolean>>({});
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
  // The new-session sheet, open at a worktree; after a failed start, with what you wrote and why.
  const [newSession, setNewSession] = useState<{ laneId: string | null; draft?: SheetDraft; error?: string } | null>(null);
  const closeNewSession = useCallback(() => setNewSession(null), []);
  const sheetOpen = useRef(false);
  useEffect(() => {
    sheetOpen.current = !!newSession;
  }, [newSession]);
  // Opens and starts still waiting on their app: a spinner shows once one takes a moment.
  const [busy, setBusy] = useState<{ id: number; label: string }[]>([]);
  const busySeq = useRef(0);
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
  usePerf(board, boardArrived);
  useEffect(() => {
    getBoard()
      .then(setBoard)
      .catch((e: unknown) => setLoadError(String(e)));
    return onBoard((b) => {
      boardArrived.current = performance.now();
      setBoard(b);
    });
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

  const projects = board?.projects ?? [];
  const known = project === ALL_PROJECTS || projects.some((p) => p.name === project);
  const current = known && project ? project : (projects.find((p) => p.name !== ELSEWHERE)?.name ?? ALL_PROJECTS);

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
  const rawLanes = useMemo(() => board?.lanes ?? [], [board?.lanes]);
  const lanes = useMemo(
    () =>
      rawLanes.map((l) => {
        if (!l.sessions.some((s) => s.archived || s.key in archiveOverride)) return l;
        const sessions = l.sessions.filter((s) => !(archiveOverride[s.key] ?? s.archived)).map((s) => (s.archived ? { ...s, archived: false } : s));
        return { ...l, sessions };
      }),
    [rawLanes, archiveOverride],
  );
  useEffect(() => {
    const settled = Object.keys(archiveOverride).filter((k) => rawLanes.some((l) => l.sessions.some((s) => s.key === k && s.archived === archiveOverride[k])));
    if (settled.length === 0) return;
    setArchiveOverride((o) => Object.fromEntries(Object.entries(o).filter(([k]) => !settled.includes(k))));
  }, [rawLanes, archiveOverride]);
  const projectLanes = useMemo(() => lanes.filter((l) => inProject(l, current)), [lanes, current]);
  const archivedItems = useMemo(
    () =>
      rawLanes
        .filter((l) => inProject(l, current))
        .flatMap((lane) =>
          lane.sessions
            .filter((s) => !s.visitor && (archiveOverride[s.key] ?? s.archived))
            .map((s): SessionItem => ({ kind: 'agent', key: `${lane.id}::${s.key}`, lane, session: { ...s, archived: true } })),
        )
        .sort((a, b) => b.session.lastActivity - a.session.lastActivity),
    [rawLanes, archiveOverride, current],
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
  const bays = useMemo(
    () => buildBays(recentLanes.flatMap((l) => (inQuery(l, query) && !lanePins.set.has(l.id) ? (narrowed(l, bayFilter, since) ?? []) : []))),
    [recentLanes, query, lanePins.set, bayFilter, since],
  );
  const foldParked = !showParked && !query;
  const parkedCount = useMemo(() => bays.flatMap((b) => b.rows).filter((r) => r.lane.parked).length, [bays]);
  const shownBays = useMemo(() => bays.map((b) => ({ ...b, rows: b.rows.filter((r) => !foldParked || !r.lane.parked) })), [bays, foldParked]);
  // What you can select on the board, top to bottom: agents' lines, and worktrees without any.
  const worktreeItems = useMemo(() => [...pinnedLaneRows, ...shownBays.flatMap((b) => b.rows)].flatMap(rowItems), [pinnedLaneRows, shownBays]);
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

  const sessionRows = useMemo<Row[]>(
    () => [...pinnedRows, ...sessionSections.flatMap((s) => s.rows), ...(showArchived ? archivedItems : [])],
    [pinnedRows, sessionSections, showArchived, archivedItems],
  );
  const visible = view === 'sessions' ? sessionRows : view === 'prs' ? prVisible : worktreeItems;
  // When the selected agent's line goes (you marked it seen, it stopped), the selection stays
  // on its worktree rather than jumping to the top.
  const selectedLaneId = selectedKey?.split('::')[0];
  const selected =
    visible.find((r) => r.key === selectedKey) ?? visible.find((r) => r.kind !== 'pr' && r.lane.id === selectedLaneId) ?? visible[0] ?? null;
  const selectedLane = selected?.lane ?? null;
  // Where a new session starts: the selected worktree, session or PR's worktree; for a PR with
  // no worktree on this Mac, its repo's main checkout.
  const startLaneId = useMemo(() => {
    if (!selected) return null;
    if (selected.lane) return selected.lane.id;
    if (selected.kind !== 'pr') return null;
    return lanes.find((l) => l.repoSlug === selected.pr.repoSlug && l.origin === 'main')?.id ?? null;
  }, [selected, lanes]);
  const indexByKey = useMemo(() => new Map(visible.map((r, i) => [r.key, i])), [visible]);
  const domIdOf = (key: string) => `row-${indexByKey.get(key) ?? -1}`;
  const domId = (r: Row) => domIdOf(r.key);

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
    const target = lane.targets.find((t) => t.id === targetId);
    if (target?.disabled) {
      setToast(target.detail ?? `${target.label} isn't available right now.`);
      return;
    }
    track(`${target?.label ?? 'Opening'}…`, openTarget(lane.id, targetId)).catch((e: unknown) => setToast(String(e)));
  }, [track]);

  const openRow = useCallback(
    (row: Row | null) => {
      if (!row) return;
      if (row.kind === 'pr') {
        openUrl(row.pr.url).catch((e: unknown) => setToast(String(e)));
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

  /** Archives a session (Codex threads in Codex itself), or unarchives it. */
  const setArchived = useCallback((key: string, archived: boolean, title: string) => {
    setArchiveOverride((o) => ({ ...o, [key]: archived }));
    setUndo(archived ? { key, title } : null);
    archiveSession(key, archived).catch((e: unknown) => {
      setArchiveOverride((o) => Object.fromEntries(Object.entries(o).filter(([k]) => k !== key)));
      setUndo(null);
      setToast(String(e));
    });
  }, []);

  useEffect(() => {
    if (!undo) return;
    const id = window.setTimeout(() => setUndo(null), 8000);
    return () => window.clearTimeout(id);
  }, [undo]);

  /** A PR's right-click menu. */
  const prMenu = useCallback(
    (r: PrRowItem) => {
      const p = r.pr;
      void showContextMenu([
        { label: 'Open on GitHub', run: () => void openUrl(p.url) },
        { label: 'Review in Codex', run: () => void reviewInCodex(p.url) },
        ...(p.reviewRequested && prIsOpen(p) ? ['separator' as const, { label: `Approve #${p.number}`, run: () => approve(p) }] : []),
        'separator',
        ...(r.lane ? [{ label: `Show worktree ${r.lane.callsign}`, run: () => r.lane && focusLane(r.lane) }] : []),
        { label: 'Copy link', run: () => copy(p.url, 'PR link') },
        { label: 'Copy branch name', run: () => copy(p.head, 'branch name') },
      ]).catch((e: unknown) => setToast(String(e)));
    },
    [approve, copy],
  );

  /** Pins or unpins a worktree. Its lines keep their keys as the row moves, so what was selected stays selected. */
  const toggleLanePin = useCallback((lane: Lane) => lanePins.toggle(lane.id), [lanePins]);

  /**
   * A session's right-click menu: open it, pin it, archive it, copy its ID. Each list pins what
   * it shows: Sessions pins the session; the board and the sidebar, lists of worktrees, pin the
   * session's worktree.
   */
  const sessionMenu = useCallback(
    (lane: Lane, s: Session, pin: 'session' | 'worktree') => {
      const target = lane.targets.find((t) => t.id === s.openTarget);
      const pinItem =
        pin === 'session'
          ? { label: sessionPins.set.has(s.key) ? 'Unpin session' : 'Pin session', run: () => sessionPins.toggle(s.key) }
          : { label: lanePins.set.has(lane.id) ? 'Unpin worktree' : 'Pin worktree', run: () => toggleLanePin(lane) };
      void showContextMenu([
        { label: target?.label ?? 'Open', run: () => open(lane, s.openTarget) },
        { label: 'New session here…', run: () => setNewSession({ laneId: lane.id }) },
        'separator',
        ...(s.archived ? [] : [pinItem]),
        { label: s.archived ? 'Unarchive' : s.harness === 'codex' ? 'Archive in Codex' : 'Archive', run: () => setArchived(s.key, !s.archived, s.title) },
        'separator',
        { label: 'Copy session ID', run: () => copy(s.id, 'session ID') },
        ...(s.resumeCommand ? [{ label: 'Copy resume command', run: () => copy(s.resumeCommand, 'resume command') }] : []),
      ]).catch((e: unknown) => setToast(String(e)));
    },
    [sessionPins, lanePins, toggleLanePin, open, setArchived, copy],
  );

  /** A worktree's right-click menu: open it, pin it, copy its path or branch. */
  const laneMenu = useCallback(
    (lane: Lane) => {
      const main = lane.targets.find((t) => t.id === lane.defaultTarget);
      const more = ['l|editor', 'l|finder'].flatMap((id) => {
        const t = lane.targets.find((x) => x.id === id);
        return t && id !== lane.defaultTarget ? [{ label: t.label, run: () => open(lane, id) }] : [];
      });
      void showContextMenu([
        { label: main?.label ?? 'Open', run: () => open(lane, lane.defaultTarget) },
        ...more,
        { label: 'New session here…', run: () => setNewSession({ laneId: lane.id }) },
        'separator',
        { label: lanePins.set.has(lane.id) ? 'Unpin worktree' : 'Pin worktree', run: () => toggleLanePin(lane) },
        'separator',
        { label: 'Copy path', run: () => copy(lane.path, 'worktree path') },
        ...(lane.branch ? [{ label: 'Copy branch name', run: () => copy(lane.branch, 'branch name') }] : []),
      ]).catch((e: unknown) => setToast(String(e)));
    },
    [lanePins, toggleLanePin, open, copy],
  );

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

  // The floating bar is on unless turned off here, from its own menu or from the Window menu.
  const [floatingBar, setFloatingBar] = useState(true);
  useEffect(() => {
    void barOn().then(setFloatingBar);
    return onBarOn(setFloatingBar);
  }, []);
  const toggleFloatingBar = () => {
    const on = !floatingBar;
    setFloatingBar(on);
    setBarOn(on).catch((e: unknown) => {
      setFloatingBar(!on);
      setToast(String(e));
    });
  };

  // The floating bar asks to show a session or PR here: in its project, with no search hiding it.
  const reveal = useRef<(r: Reveal) => void>(() => {});
  reveal.current = (r: Reveal) => {
    const lane = r.kind === 'session' ? lanes.find((l) => l.id === r.laneId) : undefined;
    const session = lane?.sessions.find((s) => s.key === (r.kind === 'session' ? r.key : ''));
    const pr = r.kind === 'pr' ? board?.prs.find((p) => p.key === r.key) : undefined;
    const home = lane?.project ?? pr?.project;
    if (!home) return;
    if (current !== ALL_PROJECTS && current !== home) pickProject(home);
    setQuery('');
    if (lane && session) {
      if (view === 'prs') setView('worktrees');
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

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = document.activeElement === searchRef.current;
      const mod = e.metaKey;
      const key = e.key.toLowerCase();
      // The new-session sheet has the keyboard while it's open.
      if (newSession) return;
      if (mod && e.key === ',') {
        e.preventDefault();
        openSettings();
        return;
      }
      // Settings has the keyboard while it's open.
      if (settings) return;
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
      // ⌘⇧N is File › New window's, so it works with no window open too.
      if (mod && !e.shiftKey && key === 'n') {
        e.preventDefault();
        setNewSession({ laneId: startLaneId });
        return;
      }
      if (typing) {
        if (e.key === 'Escape') {
          setQuery('');
          searchRef.current?.blur();
        } else if (e.key === 'ArrowDown' || e.key === 'Enter') {
          e.preventDefault();
          searchRef.current?.blur();
          if (e.key === 'Enter') openRow(selected);
        }
        return;
      }
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
      // P pins what the view lists: worktrees on the board, sessions in Sessions.
      if (key === 'p' && !mod && selected && selected.kind !== 'pr') {
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
        if (view === 'worktrees' && selected && selected.kind !== 'pr' && lanePins.set.has(selected.lane.id)) {
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
        const sessions = selected?.kind === 'agent' ? [selected.session] : (lane?.sessions ?? []);
        void markSeen(sessions.filter((s) => s.status === 'unread').map((s) => s.key));
      } else if (key === 'g' && !mod) {
        gPending.current = true;
        window.setTimeout(() => (gPending.current = false), 1200);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [visible, selected, selectedLane, startLaneId, move, open, openRow, copy, settings, openSettings, newSession, view, setView, toggleLanePin, sessionPins, lanePins, pinnedRows, pinnedLaneRows, shownBays, movePinnedSession, movePinnedLane, current, setArchived]);

  /** The sheet's Start. The sheet closes and a spinner shows while the harness takes the
   *  session; then it says what's next, and selects the session when it shows up if Codealong
   *  chose its ID. A failed start brings the sheet back with what you wrote and why. */
  const startFromSheet = useCallback(
    (spec: StartSpec, draft: SheetDraft, label: string) => {
      setNewSession(null);
      track(label, startSession(spec))
        .then((started) => {
          if (started?.message) setToast(started.message);
          if (started?.key) setPendingSession({ key: started.key, until: Date.now() + 60_000 });
        })
        .catch((e: unknown) => {
          const error = String(e).replace(/^Error: /, '');
          // A sheet opened meanwhile keeps its place; the failure shows as a toast.
          if (sheetOpen.current) setToast(error);
          else setNewSession({ laneId: spec.laneId, draft, error });
        });
    },
    [track],
  );

  useEffect(() => {
    if (!pendingSession) return;
    if (Date.now() > pendingSession.until) {
      setPendingSession(null);
      return;
    }
    for (const lane of lanes) {
      const session = lane.sessions.find((s) => s.key === pendingSession.key);
      if (!session) continue;
      if (view === 'prs') setView('worktrees');
      focusSession(lane, session);
      setPendingSession(null);
      return;
    }
  }, [lanes, pendingSession]);

  // Where the sheet can start a session: this project's worktrees, and the one it opened at.
  const startLanes = useMemo(() => {
    const at = newSession?.laneId;
    const extra = at && !projectLanes.some((l) => l.id === at) ? lanes.filter((l) => l.id === at) : [];
    return [...projectLanes, ...extra];
  }, [projectLanes, lanes, newSession?.laneId]);

  const toggleBay = (f: Filter) => setBayFilter(bayFilter === f ? null : f);
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
      {...drag}
    />
  );

  return (
    <div className="ca-win ca-win--app">
      <div className="ca-win-bar" data-tauri-drag-region>
        <ProjectSwitcher projects={projects} current={current} total={totalLanes} actions={actions} onPick={pickProject} onOpenWindow={switcherWindow} onEditProjects={() => openSettings('projects')} />
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
          {!query && <Kbd keys={['⌘', 'K']} />}
        </label>
        <div className="ca-bar-tools" data-tauri-drag-region>
          <button
            type="button"
            className={['ca-btn', 'ca-btn--ghost', 'ca-btn--sm', 'is-icon', 'ca-bartoggle', floatingBar && 'is-on'].filter(Boolean).join(' ')}
            aria-pressed={floatingBar}
            aria-label="Floating bar"
            title={floatingBar ? 'Hide the floating bar' : 'Show the floating bar: the agents that need you, on top of every window'}
            onClick={toggleFloatingBar}
          >
            <Icon name="capsule-horizontal" size={16} />
          </button>
        </div>
        <Button size="sm" icon="plus" title="Start a session in the selected worktree (⌘N)" onClick={() => setNewSession({ laneId: startLaneId })}>
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
        className={['ca-win-body', view === 'prs' && 'is-prs'].filter(Boolean).join(' ')}
        style={{ '--sidebar-w': widthVar(sidebarWidth), '--panel-w': widthVar(view === 'prs' ? prPanelWidth : inspectorWidth) } as CSSProperties}
      >
        <nav className="ca-side" aria-label="Sidebar">
          <div className="ca-label ca-side-label">Inbox</div>
          <NavItem glyph="needs" label="Needs you" count={counts.needs} tone="attn" active={bayFilter === 'needs'} onClick={() => toggleBay('needs')} />
          <NavItem glyph="working" label="Working" count={counts.working} active={bayFilter === 'working'} onClick={() => toggleBay('working')} />
          <NavItem glyph="unread" label="Your turn" count={counts.unread} active={bayFilter === 'unread'} onClick={() => toggleBay('unread')} />
          <NavItem icon="pr" label="In review" count={counts.review} active={bayFilter === 'review'} onClick={() => toggleBay('review')} />
          <NavItem glyph="error" label="Checks failing" count={counts.failing} tone="alert" active={bayFilter === 'failing'} onClick={() => toggleBay('failing')} />
          <div className="ca-label ca-side-label">Worktrees · {recentLanes.length}</div>
          <div className="ca-side-scroll">
            <WorktreeTree
              groups={worktrees}
              now={now}
              selectedLaneId={selectedLane?.id ?? null}
              selectedSessionKey={selected?.kind === 'agent' ? selected.session.key : null}
              onLaneMenu={laneMenu}
              onSessionMenu={(lane, s) => sessionMenu(lane, s, 'worktree')}
              onSelectLane={focusLane}
              onOpenLane={(lane) => open(lane, lane.defaultTarget)}
              onOpenSession={(lane, session) => {
                focusSession(lane, session);
                open(lane, session.openTarget);
              }}
            />
          </div>
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
        <main className="ca-board" aria-label={VIEWS.find((v) => v.id === view)?.label}>
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
                  <span className="ca-segctl-count">{v.id === 'worktrees' ? recentLanes.length : v.id === 'sessions' ? shownSessionCount : openPrCount}</span>
                  {v.id === 'prs' && requestedCount > 0 && <span className="ca-segctl-badge">{requestedCount}</span>}
                </button>
              ))}
            </div>
            <div className="ca-segctl ca-agefilter" role="radiogroup" aria-label="Active in">
              {AGES.map((a) => (
                <button key={a.id} type="button" role="radio" aria-checked={age === a.id} className={age === a.id ? 'is-on' : undefined} title={a.title} onClick={() => setAge(a.id)}>
                  {a.label}
                </button>
              ))}
            </div>
          </div>
          {view !== 'prs' && (
            <div className="ca-board-cols ca-label" aria-hidden="true">
              <span />
              <span>Worktree</span>
              <span>{view === 'sessions' ? 'Session' : 'Now'}</span>
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
                : bayFilter === 'failing'
                  ? 'No PR checks are failing.'
                  : bayFilter
                    ? 'Nothing here right now.'
                  : cutoff > 0
                    ? `No worktrees ${AGES.find((a) => a.id === age)?.title.toLowerCase()}.`
                    : 'No worktrees yet. Codealong looks for repos under the folders in config.toml and for the agents working in them.'}
            </div>
          )}
          <div
            role="listbox"
            aria-label={view === 'sessions' ? 'Sessions' : view === 'prs' ? 'Pull requests' : 'Agents and worktrees'}
            aria-activedescendant={selected ? domId(selected) : undefined}
            tabIndex={0}
            className="ca-board-list"
          >
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
                onSelect={(r) => setSelectedKey(r.key)}
                onOpen={(r) => {
                  setSelectedKey(r.key);
                  openRow(r);
                }}
                onContextMenu={(r) => sessionMenu(r.lane, r.session, 'session')}
                onMovePinned={movePinnedSession}
                archived={archivedItems}
                showArchived={showArchived}
                onToggleArchived={() => setShowArchived(!showArchived)}
              />
            )}
            {view === 'worktrees' && pinnedLaneRows.length > 0 && (
              <PinnedSection items={pinnedLaneRows} keyOf={(r) => r.lane.id} onMove={movePinnedLane} row={laneRow} />
            )}
            {view === 'worktrees' && shownBays.map((b) => {
              const count = bays.find((x) => x.bay === b.bay)?.rows.length ?? b.rows.length;
              const unreadKeys = b.bay === 'unread' ? b.rows.flatMap((r) => r.agents.filter((a) => a.session.status === 'unread').map((a) => a.session.key)) : [];
              return (
                <section key={b.bay} className="ca-bayblock">
                  <BayHeader
                    bay={b.bay}
                    count={count}
                    action={
                      unreadKeys.length > 0 ? (
                        <button type="button" className="ca-linkbtn" onClick={() => void markSeen(unreadKeys)}>
                          Mark all seen
                        </button>
                      ) : undefined
                    }
                  />
                  <div className="ca-bay-lanes">
                    {b.rows.map((r) => laneRow(r))}
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
        </main>
        <Resizer panel="after" cssVar="--panel-w" onResize={view === 'prs' ? setPrPanelWidth : setInspectorWidth} />
        {view === 'prs' ? (
          selected?.kind === 'pr' ? (
            <PrInspector
              key={selected.pr.key}
              item={selected}
              now={now}
              approving={approving.has(selected.pr.key)}
              onApprove={() => approve(selected.pr)}
              onOpenSession={(lane, session) => open(lane, session.openTarget)}
              onShowLane={focusLane}
              onLaneMenu={laneMenu}
              onSessionMenu={(lane, s) => sessionMenu(lane, s, 'worktree')}
            />
          ) : (
            <aside className="ca-insp ca-insp--empty">
              <p>Select a PR to see its description, reviews and changes.</p>
            </aside>
          )
        ) : (
          <Inspector
            lane={selectedLane}
            focusKey={selected?.kind === 'agent' ? selected.session.key : null}
            apps={board?.apps ?? { terminal: 'Terminal', terminalId: '', editor: 'Editor', editorId: '' }}
            now={now}
            onOpen={(id) => open(selectedLane, id)}
            onNewSession={() => selectedLane && setNewSession({ laneId: selectedLane.id })}
          />
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
      {newSession && (
        <NewSessionSheet
          lanes={startLanes}
          laneId={newSession.laneId}
          draft={newSession.draft}
          error={newSession.error}
          pinned={lanePins.keys}
          usage={board?.usage ?? []}
          now={now}
          onClose={closeNewSession}
          onStart={startFromSheet}
        />
      )}
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
      {toast && (
        <div className="ca-toast" role="status" onClick={() => setToast(null)}>
          <Icon name="alert" size={16} />
          <span>{toast}</span>
        </div>
      )}
    </div>
  );
}

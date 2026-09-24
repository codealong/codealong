// The main window. One project at a time, picked in the switcher at top left. The sidebar
// lists that project's worktrees. The main area has two views: Worktrees lists agents in Needs
// you, Working and Your turn, and worktrees in In review, Idle and Merged; Sessions lists every
// session, pinned ones first in your order. Keyboard map: the design system's Surfaces page.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { showContextMenu } from './contextMenu';
import { ageFiltered } from './lanes';
import { archiveSession, getBoard, markSeen, onBoard, openConfig, openProjectWindow, openTarget, refresh, refreshUsage, setPinned, signIn } from './api';
import { ago } from './format';
import type { Bay, Board, Lane, Session, Status } from './types';
import { AccountsDialog } from './ui/AccountsDialog';
import { Inspector } from './ui/Inspector';
import { AgentRow, failingPr, LaneRow } from './ui/LaneRow';
import { BayHeader, Icon, Kbd, NavItem } from './ui/primitives';
import { ALL_PROJECTS, openSwitcher, ProjectSwitcher, type ActionCounts } from './ui/ProjectSwitcher';
import { reorderPinned, SessionsView, timeSections, type SessionItem } from './ui/SessionsView';
import { UsageBar } from './ui/UsageBar';
import { WorktreeTree } from './ui/WorktreeTree';

const BAYS: Bay[] = ['needs', 'working', 'unread', 'review', 'idle', 'merged'];
const AGENT_BAY: Partial<Record<Status, Bay>> = { needs: 'needs', error: 'needs', working: 'working', unread: 'unread' };
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
const DAY = 24 * 60 * 60 * 1000;

type View = 'worktrees' | 'sessions';
/** How recently a worktree or session must have been active to show. */
type Age = '1d' | '3d' | '7d' | 'all';
const AGES: { id: Age; label: string; title: string; ms: number }[] = [
  { id: '1d', label: '24h', title: 'Active in the last 24 hours', ms: DAY },
  { id: '3d', label: '3d', title: 'Active in the last 3 days', ms: 3 * DAY },
  { id: '7d', label: '7d', title: 'Active in the last 7 days', ms: 7 * DAY },
  { id: 'all', label: 'All', title: 'Everything from the last 14 days', ms: 0 },
];

type Row = { kind: 'agent'; key: string; lane: Lane; session: Session } | { kind: 'lane'; key: string; lane: Lane };
/** What the Inbox narrows the board to: one bay, or worktrees whose PR checks are failing. */
type Filter = Bay | 'failing';

/** Agents go in the agent bays, one row each; worktrees go in the rest, one row each. */
function buildBays(lanes: Lane[]): { bay: Bay; rows: Row[] }[] {
  const groups: Record<Bay, Row[]> = { needs: [], working: [], unread: [], review: [], idle: [], merged: [] };
  for (const lane of lanes) {
    const own = lane.sessions.filter((s) => !s.visitor);
    for (const s of own) {
      const bay = AGENT_BAY[s.status];
      if (bay) groups[bay].push({ kind: 'agent', key: `${lane.id}::${s.key}`, lane, session: s });
    }
    const needsFromAgents = own.some((s) => s.status === 'needs' || s.status === 'error');
    const agentBay = lane.bay === 'working' || lane.bay === 'unread' || (lane.bay === 'needs' && needsFromAgents);
    if (!agentBay) groups[lane.bay].push({ kind: 'lane', key: lane.id, lane });
  }
  const at = (r: Row) => (r.kind === 'agent' ? r.session.lastActivity : r.lane.lastActivity);
  return BAYS.map((bay) => ({ bay, rows: groups[bay].sort((a, b) => at(b) - at(a)) })).filter((g) => g.rows.length > 0);
}

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

/** The bay a row sits in, for the g shortcuts. */
function rowBay(r: Row): Bay | undefined {
  return r.kind === 'agent' ? AGENT_BAY[r.session.status] : r.lane.bay;
}

function sameList(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((x, i) => x === b[i]);
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
    return window.localStorage.getItem(VIEW_KEY) === 'sessions' ? 'sessions' : 'worktrees';
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

export function App() {
  const [board, setBoard] = useState<Board | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [project, setProject] = useState<string | null>(() => WINDOW_PROJECT ?? storedProject());
  const [bayFilter, setBayFilter] = useState<Filter | null>(null);
  const [query, setQuery] = useState('');
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [view, setViewState] = useState<View>(storedView);
  const [age, setAgeState] = useState<Age>(storedAge);
  // Pins as last changed here, shown until the engine's board catches up.
  const [pinOverride, setPinOverride] = useState<string[] | null>(null);
  // Archives made here (key → archived), shown until the board catches up: Codex takes a moment.
  const [archiveOverride, setArchiveOverride] = useState<Record<string, boolean>>({});
  const [undo, setUndo] = useState<{ key: string; title: string } | null>(null);
  const [showArchived, setShowArchived] = useState(false);
  const [showParked, setShowParked] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  // The Accounts dialog, open at one account when a usage chip opened it.
  const [accountsDialog, setAccountsDialog] = useState<{ focus: string | null } | null>(null);
  const closeAccounts = useCallback(() => setAccountsDialog(null), []);
  const now = useNow(15_000);
  const searchRef = useRef<HTMLInputElement>(null);
  const gPending = useRef(false);

  useEffect(() => {
    getBoard()
      .then(setBoard)
      .catch((e: unknown) => setLoadError(String(e)));
    return onBoard(setBoard);
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

  const pinnedKeys = useMemo(() => pinOverride ?? board?.pinned ?? [], [pinOverride, board?.pinned]);
  const pinnedSet = useMemo(() => new Set(pinnedKeys), [pinnedKeys]);
  useEffect(() => {
    if (pinOverride && board && sameList(board.pinned ?? [], pinOverride)) setPinOverride(null);
  }, [board, pinOverride]);
  const savePins = useCallback((next: string[]) => {
    setPinOverride(next);
    setPinned(next).catch((e: unknown) => {
      setPinOverride(null);
      setToast(String(e));
    });
  }, []);
  /** New pins go to the top of Pinned. */
  const togglePin = useCallback(
    (key: string) => savePins(pinnedSet.has(key) ? pinnedKeys.filter((k) => k !== key) : [key, ...pinnedKeys]),
    [pinnedKeys, pinnedSet, savePins],
  );

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
  // The age filter: worktrees and sessions active since the cutoff. Pinned sessions always show.
  const ageMs = AGES.find((a) => a.id === age)?.ms ?? 0;
  const cutoff = ageMs > 0 ? now - ageMs : 0;
  const since = (AGES.find((a) => a.id === age)?.title ?? '').replace(/^Active /, '');
  // Worktrees active since the cutoff, each showing only its sessions active since then.
  const recentLanes = useMemo(
    () => (cutoff > 0 ? projectLanes.filter((l) => l.lastActivity >= cutoff).map((l) => ageFiltered(l, cutoff, pinnedSet, since)) : projectLanes),
    [projectLanes, cutoff, pinnedSet, since],
  );
  // The Sessions view: each session once, in the worktree it runs in.
  const sessionItems = useMemo(
    () => projectLanes.flatMap((lane) => lane.sessions.filter((s) => !s.visitor).map((s): SessionItem => ({ kind: 'agent', key: `${lane.id}::${s.key}`, lane, session: s }))),
    [projectLanes],
  );
  const shownSessions = useMemo(
    () =>
      sessionItems.filter(
        (r) => (r.session.lastActivity >= cutoff || pinnedSet.has(r.session.key)) && sessionInQuery(r, query) && sessionInFilter(r, bayFilter),
      ),
    [sessionItems, cutoff, pinnedSet, query, bayFilter],
  );
  const shownSessionCount = useMemo(() => sessionItems.filter((r) => r.session.lastActivity >= cutoff || pinnedSet.has(r.session.key)).length, [sessionItems, cutoff, pinnedSet]);
  const pinnedRows = useMemo(() => {
    const byKey = new Map(shownSessions.map((r) => [r.session.key, r]));
    return pinnedKeys.flatMap((k) => byKey.get(k) ?? []);
  }, [shownSessions, pinnedKeys]);
  const sessionSections = useMemo(() => timeSections(shownSessions.filter((r) => !pinnedSet.has(r.session.key)), now), [shownSessions, pinnedSet, now]);
  const allBays = useMemo(() => buildBays(recentLanes.filter((l) => inQuery(l, query))), [recentLanes, query]);
  const bays = useMemo(() => {
    if (bayFilter === 'failing') {
      return allBays.map((b) => ({ ...b, rows: b.rows.filter((r) => failingPr(r.lane)) })).filter((b) => b.rows.length > 0);
    }
    return bayFilter ? allBays.filter((b) => b.bay === bayFilter) : allBays;
  }, [allBays, bayFilter]);
  const foldParked = !showParked && !query;
  const parkedCount = useMemo(() => bays.flatMap((b) => b.rows).filter((r) => r.kind === 'lane' && r.lane.parked).length, [bays]);
  const worktreeRows = useMemo(() => bays.flatMap((b) => b.rows.filter((r) => !foldParked || r.kind === 'agent' || !r.lane.parked)), [bays, foldParked]);
  const sessionRows = useMemo<Row[]>(
    () => [...pinnedRows, ...sessionSections.flatMap((s) => s.rows), ...(showArchived ? archivedItems : [])],
    [pinnedRows, sessionSections, showArchived, archivedItems],
  );
  const visible = view === 'sessions' ? sessionRows : worktreeRows;
  const selected = visible.find((r) => r.key === selectedKey) ?? visible[0] ?? null;
  const selectedLane = selected?.lane ?? null;
  const domId = (r: Row) => `row-${visible.indexOf(r)}`;

  const counts = useMemo(() => {
    const groups = buildBays(recentLanes);
    const n = (bay: Bay) => groups.find((g) => g.bay === bay)?.rows.length ?? 0;
    return { needs: n('needs'), working: n('working'), unread: n('unread'), review: n('review'), failing: recentLanes.filter((l) => failingPr(l)).length };
  }, [recentLanes]);

  const totalLanes = useMemo(() => lanes.filter((l) => !l.hidden).length, [lanes]);

  // Needs you and Your turn per project, for the switcher's badge and menu.
  const actions = useMemo(() => {
    const out: Record<string, ActionCounts> = {};
    for (const name of [...projects.map((p) => p.name), ALL_PROJECTS]) {
      const groups = buildBays(lanes.filter((l) => inProject(l, name) && l.lastActivity >= cutoff).map((l) => ageFiltered(l, cutoff, pinnedSet, since)));
      const n = (bay: Bay) => groups.find((g) => g.bay === bay)?.rows.length ?? 0;
      out[name] = { needs: n('needs'), unread: n('unread') };
    }
    return out;
  }, [lanes, projects, cutoff, pinnedSet, since]);

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
    openTarget(lane.id, targetId).catch((e: unknown) => setToast(String(e)));
  }, []);

  const openRow = useCallback(
    (row: Row | null) => {
      if (!row) return;
      open(row.lane, row.kind === 'agent' ? row.session.openTarget : row.lane.defaultTarget);
    },
    [open],
  );

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

  /** A session's right-click menu: open it, pin it, archive it, copy its ID. */
  const sessionMenu = useCallback(
    (lane: Lane, s: Session) => {
      const target = lane.targets.find((t) => t.id === s.openTarget);
      const pinned = pinnedSet.has(s.key);
      void showContextMenu([
        { label: target?.label ?? 'Open', run: () => open(lane, s.openTarget) },
        'separator',
        ...(s.archived ? [] : [{ label: pinned ? 'Unpin' : 'Pin', run: () => togglePin(s.key) }]),
        { label: s.archived ? 'Unarchive' : s.harness === 'codex' ? 'Archive in Codex' : 'Archive', run: () => setArchived(s.key, !s.archived, s.title) },
        'separator',
        { label: 'Copy session ID', run: () => copy(s.id, 'session ID') },
        ...(s.resumeCommand ? [{ label: 'Copy resume command', run: () => copy(s.resumeCommand, 'resume command') }] : []),
      ]).catch((e: unknown) => setToast(String(e)));
    },
    [pinnedSet, open, togglePin, setArchived, copy],
  );

  /** Moves a pinned session to insertion point `index` among the pinned ones showing. */
  const movePinned = useCallback(
    (key: string, index: number) => {
      const next = reorderPinned(pinnedKeys, pinnedRows.map((r) => r.session.key), key, index);
      if (next !== pinnedKeys) savePins(next);
    },
    [pinnedKeys, pinnedRows, savePins],
  );

  /** Selects a worktree from the sidebar, in the Worktrees view: its first agent row, else its own row. */
  const focusLane = (lane: Lane) => {
    setView('worktrees');
    setBayFilter(null);
    if (lane.parked) setShowParked(true);
    const own = lane.sessions.find((s) => !s.visitor && AGENT_BAY[s.status]);
    setSelectedKey(own ? `${lane.id}::${own.key}` : lane.id);
  };

  /** Selects a session's row: in Sessions its own row; on the board its agent row, else its worktree's row. */
  const focusSession = (lane: Lane, session: Session) => {
    setBayFilter(null);
    if (view === 'sessions') {
      const item = sessionItems.find((r) => r.session.key === session.key);
      if (item) {
        setSelectedKey(item.key);
        return;
      }
    }
    const onBoardAsAgent = !session.visitor && AGENT_BAY[session.status];
    if (!onBoardAsAgent && lane.parked) setShowParked(true);
    setSelectedKey(onBoardAsAgent ? `${lane.id}::${session.key}` : lane.id);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = document.activeElement === searchRef.current;
      const mod = e.metaKey;
      const key = e.key.toLowerCase();
      if (mod && e.key === ',') {
        e.preventDefault();
        setAccountsDialog({ focus: null });
        return;
      }
      // The Accounts dialog has the keyboard while it's open.
      if (accountsDialog) return;
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
      if (mod && e.shiftKey && key === 'n') {
        e.preventDefault();
        openWindow(current);
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
        const first = bay ? visible.find((r) => rowBay(r) === bay) : undefined;
        if (first) setSelectedKey(first.key);
        return;
      }
      const lane = selectedLane;
      if (key === 'v' && !mod) {
        setView(view === 'sessions' ? 'worktrees' : 'sessions');
        return;
      }
      if (key === 'p' && !mod && selected?.kind === 'agent') {
        togglePin(selected.session.key);
        return;
      }
      if (key === 'a' && !mod && selected?.kind === 'agent') {
        setArchived(selected.session.key, !selected.session.archived, selected.session.title);
        return;
      }
      if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown') && view === 'sessions' && selected?.kind === 'agent' && pinnedSet.has(selected.session.key)) {
        e.preventDefault();
        const i = pinnedRows.findIndex((r) => r.key === selected.key);
        movePinned(selected.session.key, e.key === 'ArrowUp' ? i - 1 : i + 2);
        return;
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
        if (lane?.pr) open(lane, 'p|github');
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
  }, [visible, selected, selectedLane, move, open, openRow, copy, accountsDialog, view, setView, togglePin, pinnedSet, pinnedRows, movePinned, current, setArchived]);

  const toggleBay = (f: Filter) => setBayFilter(bayFilter === f ? null : f);
  const openWindow = (name: string) => {
    openProjectWindow(name).catch((e: unknown) => setToast(String(e)));
  };
  const editConfig = () => {
    openConfig().catch((e: unknown) => setToast(String(e)));
  };
  const startSignIn = (account: string) => {
    signIn(account).catch((e: unknown) => setToast(String(e)));
  };

  // The sidebar's worktrees, grouped by repo: active ones first, parked ones last.
  const worktrees = useMemo(() => {
    const byRepo = new Map<string, Lane[]>();
    for (const l of recentLanes) byRepo.set(l.repo, [...(byRepo.get(l.repo) ?? []), l]);
    const rank = (l: Lane) => (l.parked ? 9 : BAYS.indexOf(l.bay));
    return [...byRepo.entries()]
      .sort((a, b) => b[1].length - a[1].length || a[0].localeCompare(b[0]))
      .map(([repo, ls]) => ({ repo, lanes: ls.sort((a, b) => rank(a) - rank(b) || b.lastActivity - a.lastActivity) }));
  }, [projectLanes]);

  return (
    <div className="ca-win ca-win--app">
      <div className="ca-win-bar" data-tauri-drag-region>
        <ProjectSwitcher projects={projects} current={current} total={totalLanes} actions={actions} onPick={pickProject} onOpenWindow={openWindow} onEditConfig={editConfig} />
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
        <div className="ca-bar-status" data-tauri-drag-region>
          {board?.githubAt ? `PRs as of ${ago(board.githubAt, now)}` : board && !board.loading ? 'Loading PRs' : ''}
        </div>
      </div>
      <UsageBar
        usage={board?.usage ?? []}
        now={now}
        onRefresh={() => void refreshUsage()}
        onAccounts={(account) => setAccountsDialog({ focus: account ?? null })}
        onSignIn={startSignIn}
      />
      <div className="ca-win-body">
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
              pinned={pinnedSet}
              onSessionMenu={sessionMenu}
              onSelectLane={focusLane}
              onOpenSession={(lane, session) => {
                focusSession(lane, session);
                open(lane, session.openTarget);
              }}
            />
          </div>
          <div className="ca-side-foot">
            <button type="button" className="ca-linkbtn ca-side-config" title="~/Library/Application Support/Codealong/config.toml" onClick={editConfig}>
              <Icon name="settings" size={14} />
              <span>Edit config.toml</span>
            </button>
          </div>
          {board && board.warnings.length > 0 && (
            <div className="ca-side-warnings" role="status">
              {board.warnings.map((w) => (
                <div key={w} className="ca-warning">
                  <Icon name="alert" size={14} />
                  <span>{w}</span>
                </div>
              ))}
            </div>
          )}
        </nav>
        <main className="ca-board" aria-label={view === 'sessions' ? 'Sessions' : 'Worktrees'}>
          <div className="ca-board-head">
            <div className="ca-segctl" role="tablist" aria-label="View">
              {(['worktrees', 'sessions'] as View[]).map((v) => (
                <button key={v} type="button" role="tab" aria-selected={view === v} className={view === v ? 'is-on' : undefined} title="Switch views (V)" onClick={() => setView(v)}>
                  {v === 'worktrees' ? 'Worktrees' : 'Sessions'}
                  <span className="ca-segctl-count">{v === 'worktrees' ? recentLanes.length : shownSessionCount}</span>
                </button>
              ))}
            </div>
            {view === 'sessions' && pinnedKeys.length === 0 && (
              <span className="ca-board-head-hint">
                Pin a session with <Kbd keys={['P']} /> to keep it at the top
              </span>
            )}
            <div className="ca-agefilter">
              <span className="ca-agefilter-label">Active in</span>
              <div className="ca-segctl" role="radiogroup" aria-label="Active in">
                {AGES.map((a) => (
                  <button key={a.id} type="button" role="radio" aria-checked={age === a.id} className={age === a.id ? 'is-on' : undefined} title={a.title} onClick={() => setAge(a.id)}>
                    {a.label}
                  </button>
                ))}
              </div>
            </div>
          </div>
          <div className="ca-board-cols ca-label" aria-hidden="true">
            <span />
            <span>Worktree</span>
            <span>{view === 'sessions' ? 'Session' : 'Now'}</span>
            <span className="ca-board-cols-age">Age</span>
          </div>
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
          {board && !board.loading && view === 'worktrees' && bays.length === 0 && (
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
            aria-label={view === 'sessions' ? 'Sessions' : 'Agents and worktrees'}
            aria-activedescendant={selected ? domId(selected) : undefined}
            tabIndex={0}
            className="ca-board-list"
          >
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
                onContextMenu={(r) => sessionMenu(r.lane, r.session)}
                onMovePinned={movePinned}
                archived={archivedItems}
                showArchived={showArchived}
                onToggleArchived={() => setShowArchived(!showArchived)}
              />
            )}
            {view === 'worktrees' && bays.map((b) => {
              const shown = b.rows.filter((r) => !foldParked || r.kind === 'agent' || !r.lane.parked);
              const unreadKeys = b.bay === 'unread' ? b.rows.flatMap((r) => (r.kind === 'agent' ? [r.session.key] : [])) : [];
              return (
                <section key={b.bay} className="ca-bayblock">
                  <BayHeader
                    bay={b.bay}
                    count={b.rows.length}
                    action={
                      unreadKeys.length > 0 ? (
                        <button type="button" className="ca-linkbtn" onClick={() => void markSeen(unreadKeys)}>
                          Mark all seen
                        </button>
                      ) : undefined
                    }
                  />
                  <div className="ca-bay-lanes">
                    {shown.map((r) =>
                      r.kind === 'agent' ? (
                        <AgentRow
                          key={r.key}
                          lane={r.lane}
                          session={r.session}
                          domId={domId(r)}
                          now={now}
                          selected={r.key === selected?.key}
                          onSelect={() => setSelectedKey(r.key)}
                          onOpen={() => {
                            setSelectedKey(r.key);
                            openRow(r);
                          }}
                          pinned={pinnedSet.has(r.session.key)}
                          onContextMenu={() => sessionMenu(r.lane, r.session)}
                        />
                      ) : (
                        <LaneRow
                          key={r.key}
                          lane={r.lane}
                          domId={domId(r)}
                          now={now}
                          selected={r.key === selected?.key}
                          onSelect={() => setSelectedKey(r.key)}
                          onOpen={(lane) => open(lane, lane.defaultTarget)}
                          onOpenTarget={(lane, id) => {
                            setSelectedKey(r.key);
                            open(lane, id);
                          }}
                        />
                      ),
                    )}
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
        <Inspector
          lane={selectedLane}
          focusKey={selected?.kind === 'agent' ? selected.session.key : null}
          apps={board?.apps ?? { terminal: 'Terminal', terminalId: '', editor: 'Editor', editorId: '' }}
          now={now}
          onOpen={(id) => open(selectedLane, id)}
        />
      </div>
      {accountsDialog && (
        <AccountsDialog
          focus={accountsDialog.focus}
          usage={board?.usage ?? []}
          terminal={board?.apps.terminal ?? 'Terminal'}
          now={now}
          onClose={closeAccounts}
          onEditConfig={editConfig}
        />
      )}
      {undo && !toast && (
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

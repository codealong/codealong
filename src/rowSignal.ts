// Rules for keeping the main window's index rows (Sessions, PRs, Todo, Schedules) high-signal
// in both one-line and two-line mode: what a worktree or repo chip says, when a session's helper,
// run or side chips show, how working rows avoid repeating their duration, which places and
// sessions a todo row lists, and the context-menu toggles for finished helpers, runs and sides.

import type { HelperCount, RunCount, SideCount } from './boardLists';
import type { MenuSpec } from './contextMenu';
import { AGENT_BAY, runsShownKey, sidesShownKey } from './lanes';
import type { Lane, Session } from './types';
import { projectSaid } from './ui/ProjectSaid';

export interface WorktreeChipParts {
  /** What the chip names at the end: the worktree's own name (when reusable or explicitly asked
   *  for), else its project (when cross-project and the repo shares the project's name), else its
   *  repo. */
  name: string;
  /** The names already said beside the project prefix, for `ProjectPrefix`. */
  beside: string[];
  /** Shown before `name` in two-line mode for a named worktree in a multi-repo project. */
  repoPrefix: string | null;
  /** Shown after `name` in two-line mode for a main checkout. */
  mainSuffix: boolean;
  /** Whether the window lists another or all projects, so the project's org icon stands in for
   *  the folder icon. */
  crossProject: boolean;
  /** Fallback icon when no project org icon is shown: repeat for a named reusable worktree, else
   *  folder. */
  fallbackIcon: 'folder' | 'repeat';
}

/** What a `WorktreeChip` displays: project/repo for disposable worktrees (whose random or
 *  task-slug names add noise), and the worktree's own name for reusable slots or panels that
 *  inspect a specific worktree (`alwaysName`). Main checkouts say `/ main` only on two lines. */
export function worktreeChipParts(
  lane: Pick<Lane, 'project' | 'repo' | 'callsign' | 'origin' | 'reuse'>,
  shown: string | null,
  { short = false, alwaysName = false }: { short?: boolean; alwaysName?: boolean } = {},
): WorktreeChipParts {
  const main = lane.origin === 'main';
  const showWorktree = alwaysName || !!lane.reuse;
  const own = !main && showWorktree && lane.callsign !== lane.repo;
  const crossProject = !!lane.project && shown != null && lane.project !== shown;
  const repoIsProject = !!lane.project && lane.project.toLowerCase() === lane.repo.toLowerCase();
  const name = own ? lane.callsign : crossProject && repoIsProject ? lane.project : lane.repo;
  return {
    name,
    beside: [own ? lane.callsign : lane.repo],
    repoPrefix: own && !short && !repoIsProject ? lane.repo : null,
    mainSuffix: main && !short,
    crossProject,
    fallbackIcon: !main && !!lane.reuse && own ? 'repeat' : 'folder',
  };
}

/** What a `RepoChip` displays: its project's casing when cross-project and the repo shares the
 *  project's name, so the project org icon stands in without repeating `Project / project`. */
export function repoChipParts(repo: string, project: string | null | undefined, shown: string | null): { name: string; crossProject: boolean } {
  const crossProject = !!project && shown != null && project !== shown;
  const repoIsProject = !!project && project.toLowerCase() === repo.toLowerCase();
  return { name: crossProject && repoIsProject ? project : repo, crossProject };
}

/** Whether a session's helpers chip should show without the user having expanded finished ones:
 *  only while at least one helper is waiting on you, working or has an unread turn. */
export function hasActiveHelpers(h: HelperCount | undefined): boolean {
  return !!h && (h.waiting > 0 || h.working > 0 || h.each.some((g) => AGENT_BAY[g.status] != null));
}

/** Whether a session's headless runs chip should show without the user having expanded them: only
 *  while at least one run is working or waiting on you. */
export function hasActiveRuns(r: RunCount | undefined): boolean {
  return !!r && (r.working > 0 || r.waiting > 0);
}

/** Whether a session's side conversations chip should show without the user having expanded
 *  answered ones: only while one is answering or its answer hasn't been seen yet. */
export function hasActiveSides(c: SideCount | undefined, unreadAnswer = false): boolean {
  return !!c && (c.working > 0 || unreadAnswer);
}

/** Strips a leading `{for:<ms>} · ` (or lone `{for:<ms>}`) from a working session's detail when
 *  the row's own age column already shows how long the turn has been running. */
export function stripWorkingFor(detail: string | null): string | null {
  if (!detail) return null;
  const trimmed = detail.replace(/^\{for:-?\d+\}(?: · )?/, '');
  return trimmed || null;
}

/** Which worktrees a todo's row shows as chips: skips the worktree heading it sits under, drops
 *  disposable worktrees whose repo repeats a `ProjectTag` already shown on the row, and
 *  deduplicates multiple disposable worktrees of the same repo so `[acme]` doesn't appear
 *  twice. Reusable worktrees, remote worktrees and distinct repos stay. */
export function todoRowLanes(lanes: Lane[], project: string | null | undefined, shown: string | null, under?: string): Lane[] {
  const tagShown = !!projectSaid(project, shown);
  const seenDisposable = new Set<string>();
  const out: Lane[] = [];
  for (const l of lanes) {
    if (l.id === under) continue;
    const disposable = !l.reuse && l.origin !== 'main';
    if (disposable) {
      if (tagShown && !l.device && l.project === project && l.repo.toLowerCase() === (project ?? '').toLowerCase()) continue;
      const key = `${l.device?.short ?? ''}|${l.project}|${l.repo}`;
      if (seenDisposable.has(key)) continue;
      seenDisposable.add(key);
    }
    out.push(l);
  }
  return out;
}

const SESSION_URGENCY = (s: Pick<Session, 'status'>) => (s.status === 'needs' || s.status === 'error' ? 0 : s.status === 'working' ? 1 : s.status === 'unread' ? 2 : 3);

/** The linked sessions a todo's row shows as chips: on one line, just the most urgent one (with
 *  the rest behind `+N`) so titles keep their room; on two lines, up to three. */
export function todoRowSessions<T extends { session: Pick<Session, 'status'> }>(sessions: T[], slim: boolean): { shown: T[]; rest: T[] } {
  const limit = slim ? 1 : 3;
  if (sessions.length <= limit) return { shown: sessions, rest: [] };
  const ordered = slim ? [...sessions].sort((a, b) => SESSION_URGENCY(a.session) - SESSION_URGENCY(b.session)) : sessions;
  return { shown: ordered.slice(0, limit), rest: ordered.slice(limit) };
}

/** Context-menu items that list or hide a session's finished helpers, answered side conversations
 *  or headless runs under its row in Sessions, so they stay reachable even while their idle chips
 *  are hidden from the list. */
export function relatedMenuItems(
  sessionKey: string,
  counts: { helpers?: HelperCount; runs?: RunCount; sides?: SideCount },
  shown: ReadonlySet<string>,
  onToggle: (shownKey: string) => void,
): MenuSpec {
  const items: MenuSpec = [];
  const { helpers, runs, sides } = counts;
  if (helpers && helpers.count > 0) {
    const isShown = shown.has(sessionKey);
    const hidden = helpers.each.filter((g) => AGENT_BAY[g.status] == null).length;
    if (isShown || hidden > 0) {
      const label = isShown ? 'Hide finished helpers' : helpers.count === 1 ? 'Show finished helper' : `Show all ${helpers.count} helpers`;
      items.push({ label, run: () => onToggle(sessionKey) });
    }
  }
  if (runs && runs.count > 0) {
    const key = runsShownKey(sessionKey);
    const isShown = shown.has(key);
    const hidden = runs.each.filter((g) => g.status !== 'needs' && g.status !== 'error').length;
    if (isShown || hidden > 0) {
      const label = isShown ? 'Hide headless runs' : runs.count === 1 ? 'Show headless run' : `Show ${runs.count} headless runs`;
      items.push({ label, run: () => onToggle(key) });
    }
  }
  if (sides && sides.count > 0) {
    const key = sidesShownKey(sessionKey);
    const isShown = shown.has(key);
    const hidden = sides.each.filter((g) => g.status !== 'working').length;
    if (isShown || hidden > 0) {
      const label = isShown ? 'Hide answered side conversations' : sides.count === 1 ? 'Show side conversation' : `Show ${sides.count} side conversations`;
      items.push({ label, run: () => onToggle(key) });
    }
  }
  return items;
}

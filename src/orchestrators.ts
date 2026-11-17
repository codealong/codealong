// Orchestrators in the windows (the engine's schedules/orchestrate.rs): a session Codealong keeps
// going that keeps watch over a project's sessions, or every project's without one of its own
// (All projects'), for you. Its session leads the Sessions of what it keeps watch over, under no
// worktree and never among the sessions it watches; nothing is listed while there's none, and
// Sessions' bar offers to start one (StartOrchestrator.tsx). Each is a schedule, so starting,
// looking now, starting afresh and turning one off are schedule changes (`scheduleOp`): its
// scheduler marks the session it had done (runner.rs `Job::Retire`, `retire`).

import { scheduleOp } from './api';
import type { MenuAction } from './contextMenu';
import { ALL_PROJECTS } from './quick';
import { errorText, type Report } from './report';
import type { Lane, ScheduleItem, Session } from './types';

/** What an orchestrator keeps watch over, in a word or two. */
export function scopeWords(project: string | null | undefined): string {
  return project ?? ALL_PROJECTS;
}

function sameProject(a: string | null | undefined, b: string | null | undefined): boolean {
  return (a ?? '').toLowerCase() === (b ?? '').toLowerCase();
}

/** The project an orchestrator started from a view keeps watch over: the view's, or none (All
 *  projects) under All projects. */
export function scopeOfView(current: string): string | null {
  return current === ALL_PROJECTS ? null : current;
}

/** The orchestrators' sessions a view's Sessions leads with: under a project, its orchestrator's;
 *  under All projects, every one, All projects' first and then by project. From every worktree,
 *  since an orchestrator's session lives among Codealong's chats, in no project. */
export function orchestratorsIn<T extends { lane: Lane; session: Session }>(rows: T[], current: string): T[] {
  const all = current === ALL_PROJECTS;
  return rows
    .filter((r) => r.session.orchestrator && !r.session.archived && (all || sameProject(r.session.orchestrator.project, current)))
    .sort((a, b) => (a.session.orchestrator?.project ?? '').localeCompare(b.session.orchestrator?.project ?? ''));
}

/** Whether a view can have an orchestrator of its own: All projects, or a project of yours (the
 *  board's `projects`). Elsewhere, the sessions in none, is All projects' to keep watch over. */
export function orchestrates(current: string, projects: readonly { name: string }[] | undefined): boolean {
  return current === ALL_PROJECTS || (current !== ELSEWHERE && !!projects?.some((p) => p.name === current));
}

/** The sessions in no project, as the engine names them (board.rs `ELSEWHERE`). */
const ELSEWHERE = 'Elsewhere';

/** What an orchestrator can be started for from a view, of what has none running yet: the view's
 *  project, or under All projects, All projects (null) and then each project of yours, in the
 *  board's order. Empty once every one runs, and for a view that can't have one. */
export function freeScopes(current: string, projects: readonly { name: string }[] | undefined, items: ScheduleItem[] | undefined): (string | null)[] {
  if (!orchestrates(current, projects)) return [];
  const scopes = current === ALL_PROJECTS ? [null, ...(projects ?? []).map((p) => p.name).filter((n) => n !== ELSEWHERE)] : [current];
  return scopes.filter((p) => !orchestratorOn(orchestratorFor(items, p)));
}

/** What the compass in Sessions' bar says it starts: one scope by name, or for several, that the
 *  dialog asks which. */
export function compassTitle(scopes: readonly (string | null)[]): string {
  const what = 'an agent that keeps watch over sessions for you';
  return scopes.length === 1 ? `Start an orchestrator for ${scopeWords(scopes[0])}: ${what}` : `Start an orchestrator for All projects or one of your projects: ${what}`;
}

/** The orchestrator for a scope, on or not, as the Schedules view has it. */
export function orchestratorFor(items: ScheduleItem[] | undefined, project: string | null): ScheduleItem | undefined {
  return items?.find((i) => i.orchestrates && sameProject(i.orchestrates.project, project));
}

/** Whether a scope's orchestrator runs: there, not paused, not finished. */
export function orchestratorOn(item: ScheduleItem | undefined): boolean {
  return !!item && !item.paused && item.finished == null;
}

/** Starts the orchestrator for a scope: the one there is goes on, else one's made; it takes its
 *  first look at once. */
export function startOrchestrator(project: string | null, account = ''): Promise<unknown> {
  return scheduleOp({ op: 'orchestrate', project, account });
}

/** What a session's menu offers for the orchestrator it's the session of: look now, start its
 *  session afresh, and turn it off. None for any other session. */
export function orchestratorActions(s: Session, report: Report): { look?: MenuAction; fresh?: MenuAction; turnOff?: MenuAction } {
  const o = s.orchestrator;
  if (!o || s.archived) return {};
  const fail = (e: unknown) => report.error(errorText(e));
  const words = scopeWords(o.project);
  return {
    look: o.paused ? undefined : { label: 'Look now', run: () => void scheduleOp({ op: 'run', id: o.schedule }).then(() => report.info(`The orchestrator for ${words} looks once its turn has ended.`), fail) },
    fresh: o.paused ? undefined : { label: 'Start afresh', run: () => void scheduleOp({ op: 'fresh', id: o.schedule }).then(() => report.info(`A new session takes over for ${words}, going on from its notes.`), fail) },
    turnOff: {
      label: 'Turn off orchestrator',
      run: () => void scheduleOp({ op: 'pause', id: o.schedule, paused: true }).then(() => report.info(`The orchestrator for ${words} is off: its session is marked done, and its notes are kept for when it's started again.`), fail),
    },
  };
}

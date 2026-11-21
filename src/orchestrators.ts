// Orchestrators in the windows (the engine's schedules/orchestrate.rs): a session Codealong keeps
// going that keeps watch over a project's sessions, or every project's without one of its own
// (All projects'), for you. Its session leads the Sessions of what it keeps watch over, under no
// worktree and never among the sessions it watches; nothing is listed while there's none, and
// Sessions' bar offers to start one (StartOrchestrator.tsx). Each is a schedule, so starting,
// looking now, starting afresh and turning one off are schedule changes (`scheduleOp`): its
// scheduler marks the session it had done (runner.rs `Job::Retire`, `retire`).

import { scheduleOp } from './api';
import type { MenuAction } from './contextMenu';
import { saysNothing } from './listening';
import { ALL_PROJECTS } from './quick';
import { errorText, type Report } from './report';
import type { Lane, ScheduleItem, Session, TranscriptItem } from './types';

/** What an orchestrator keeps watch over, in a word or two. */
export function scopeWords(project: string | null | undefined): string {
  return project ?? ALL_PROJECTS;
}

function sameProject(a: string | null | undefined, b: string | null | undefined): boolean {
  return (a ?? '').toLowerCase() === (b ?? '').toLowerCase();
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
 *  dialog asks which, All projects among them only while it's free. */
export function compassTitle(scopes: readonly (string | null)[]): string {
  const what = 'an agent that keeps watch over sessions for you';
  if (scopes.length === 1) return `Start an orchestrator for ${scopeWords(scopes[0])}: ${what}`;
  return `Start an orchestrator for ${scopes.includes(null) ? 'All projects or one of your projects' : 'one of your projects'}: ${what}`;
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
 *  session afresh, and turn it off, which stands for Mark as done on it (menus.ts `done`, ⇧⌘E):
 *  marking its session done or archiving it would only have its next look start another, and an
 *  orchestrator is turned off on purpose. None for any other session. */
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

/** The paragraph that ends Codealong's message for a schedule's run, saying which schedule it's
 *  from (runner.rs `prompt_of`), whatever the schedule tells it before (its prompt can be edited).
 *  A fresh session's first prompt is recorded as yours, so it's told by it; a stopped one's
 *  resumed with it comes as Codealong's (`send::by_codealong`), as do the rest. */
const FROM_SCHEDULE = /\n\nThis comes from the schedule “[^\n]*$/;

/** An orchestrator's conversation with each run that found nothing to tell you folded into a
 *  line: Codealong's message that woke it, what it did, and its reply, a dash (listening.ts
 *  `saysNothing`), as skills/codealong/references/orchestrate.md asks it to answer then, which the
 *  engine says as its headline (board.rs `quiet_orchestrator`). Such runs in a row fold into one
 *  line. A run that told you something, asked you something (a question and your answers stay in
 *  view), or that's still going, stays as it is. */
export function foldQuietRuns(items: TranscriptItem[], fromStart: boolean): TranscriptItem[] {
  // Codealong's messages, and the session's very first prompt where the page begins with it, the
  // run that started it.
  const startsRun = (it: TranscriptItem, i: number) => it.kind === 'codealong' || (fromStart && i === 0 && it.kind === 'you' && FROM_SCHEDULE.test(it.text));
  const out: TranscriptItem[] = [];
  // The line the last run folded into, while nothing has come after it.
  let fold: { line: TranscriptItem; runs: number } | null = null;
  for (let i = 0; i < items.length; ) {
    const it = items[i];
    // A run is what Codealong told it and everything up to the next message to it.
    let end = i + 1;
    if (startsRun(it, i)) while (end < items.length && !['codealong', 'you', 'peer'].includes(items[end].kind)) end++;
    const run = items.slice(i, end);
    // Its last step: a reply that ends it, or a call it's still making.
    const last = [...run].reverse().find((x) => x.kind === 'agent' || x.kind === 'tool');
    const asked = run.some((x) => x.questions?.length || x.answers);
    const quiet = startsRun(it, i) && !asked && last?.kind === 'agent' && saysNothing(last.text);
    if (!quiet) {
      out.push(...run);
      fold = null;
    } else if (fold) {
      fold.runs += 1;
      fold.line.text = `Looked ${fold.runs} times: nothing new`;
    } else {
      fold = { line: { at: it.at, kind: 'note', text: 'Looked: nothing new', failed: false, cursor: it.cursor, n: it.n }, runs: 1 };
      out.push(fold.line);
    }
    i = end;
  }
  return out;
}

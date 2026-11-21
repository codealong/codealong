// Schedules as the Schedules view says them (ui/SchedulesView.tsx, ui/SchedulePanel.tsx,
// ui/ScheduleDialog.tsx): what a run came to, when one comes next, what a schedule does, which
// schedule a session came from, and a new schedule's draft. What's due and what a run does are the
// engine's (src-tauri/src/schedules/); these only say it.

import { resetsPhrase } from './format';
import { groupRows, groupsOf, placeIn, type GroupBy, type Grouped, type ListGroupBy, type Place } from './groups';
import { homeOf, sessionHomes, type Placed } from './lanes';
import { ALL_PROJECTS, laneGroups } from './quick';
import type { DevicesView, Lane, Schedule, ScheduleItem, ScheduleOutcome, ScheduleRunsOn, ScheduleWhen } from './types';

export const OUTCOME_WORD: Record<ScheduleOutcome, string> = {
  started: 'Started',
  sent: 'Sent',
  resumed: 'Went on',
  queued: 'Waiting',
  skipped: 'Skipped',
  missed: 'Missed',
  failed: 'Failed',
};

export const OUTCOME_ICON: Record<ScheduleOutcome, string> = {
  started: 'player-play',
  sent: 'send',
  resumed: 'player-play',
  queued: 'hourglass',
  skipped: 'arrow-back-up',
  missed: 'moon',
  failed: 'alert',
};

/** What a schedule watches for, as its glyph: a clock for times, an eye for a watch, a bolt for
 *  what happens on the board or a script's fire. */
export function whenIcon(when: ScheduleWhen): string {
  switch (when.kind) {
    case 'time':
      return 'clock';
    case 'watch':
      return 'eye';
    case 'event':
    case 'fired':
      return 'bolt';
  }
}

/** When it runs next, as its row says it: `in 42m`, `Fri 9:00 AM`; `Paused`, or nothing for one
 *  that runs only when something happens, or that's finished. A time that's come and not yet run
 *  is `due now`. */
/** What the Schedules view calls a schedule: its name, or Orchestrator for an orchestrator with the
 *  name it was given ("Northwind orchestrator", orchestrate.rs `name`), which stays its own so
 *  `codealong schedule` finds each by it, its when saying which project (`whenWords`). A name you
 *  gave one yourself stays. */
export function scheduleName(s: Pick<ScheduleItem, 'name' | 'orchestrates'>): string {
  // Spaces as one, in the same case, as orchestrate.rs `session_title` matches it.
  const said = (t: string) => t.trim().split(/\s+/).join(' ');
  const given = s.orchestrates?.project ? `${s.orchestrates.project} orchestrator` : 'Orchestrator';
  return s.orchestrates && said(s.name) === said(given) ? 'Orchestrator' : s.name;
}

/** When it runs next: on its row in the list (`Next in 2h`, `Looks in 5m`, `Paused`), or at the
 *  start of the strip over its panel (SchedulePanel.tsx), where Next is the queue's button beside
 *  it, so the words say what comes next (`Next run in 2h`, `Next look in 5m`) and why none comes
 *  where that's so, as the panel's tag does (`Finished`, `Can’t run`, `Paused`). */
export function nextWords(item: Pick<ScheduleItem, 'paused' | 'nextAt' | 'when' | 'finished' | 'problem'>, now: number, on: 'row' | 'strip' = 'row'): string {
  const strip = on === 'strip';
  if (item.finished != null) return strip ? 'Finished' : '';
  if (strip && item.problem) return 'Can’t run';
  if (item.paused) return 'Paused';
  if (item.nextAt == null) return strip && item.when.kind === 'fired' ? 'Runs when it’s fired' : '';
  const watch = item.when.kind === 'watch';
  if (item.nextAt <= now) return watch ? 'Looks now' : 'Due now';
  const at = resetsPhrase(item.nextAt, now);
  if (strip) return `${watch ? 'Next look' : 'Next run'} ${at}`;
  return watch ? `Looks ${at}` : `Next ${at}`;
}

/** What it does, in a few words: `New session in web on Auto`, `Sends to “Fix the header”`. The
 *  folder's own name, as its worktree's row says it: where it runs here, for one whose repo's
 *  checkout is elsewhere on this machine than on the one it came from. */
export function doesWords(s: Pick<ScheduleItem, 'run' | 'runsIn' | 'thread'>, sessionTitle: (key: string) => string | null): string {
  const r = s.run;
  const folder = (s.runsIn ?? r.folder).trim();
  const name = folder ? (folder.split('/').filter(Boolean).pop() ?? folder) : 'a chat';
  const on = r.account.trim() || 'Auto';
  switch (r.to) {
    case 'new':
      return `New session in ${name}${r.worktree ? ', a new worktree each time,' : ''} on ${on}`;
    case 'thread':
      return `Keeps one session going in ${name} on ${on}`;
    case 'session': {
      // Gone, it went on as a copy, or a new session took its place (runner.rs `in_place_of`).
      const title = sessionTitle(r.session) ?? (s.thread ? sessionTitle(s.thread) : null);
      return title ? `Sends to “${title}”` : `Sends to a session that’s gone, so a new one starts in ${name}`;
    }
  }
}

/** Whether a schedule runs once and is then finished: one told to stop after its first run
 *  (`once`), or one at a `once <date>` time, told by its first word as the engine reads a time
 *  (schedules/rule.rs `parse`). */
export function runsOnce(s: Pick<Schedule, 'once' | 'when'>): boolean {
  return !!s.once || (s.when.kind === 'time' && /^[\s,]*once(?=[\s,]|$)/i.test(s.when.rule));
}

/** How the Schedules view orders what it lists (its View menu, SchedulesView.tsx
 *  `SCHEDULE_SORTS`): the next run soonest first, the latest run first, the newest first, or by
 *  name. */
export type ScheduleSort = 'next' | 'last' | 'added' | 'name';

/** Smaller first, two that are the same (two never coming, say) kept in their order. */
const ascending = (a: number, b: number) => (a === b ? 0 : a < b ? -1 : 1);

/** When its time next comes, for the order: a watch's `nextAt` is its next look, not a run, and a
 *  paused one's doesn't come. */
const comes = (x: ScheduleItem) => (x.when.kind === 'time' && !x.paused ? (x.nextAt ?? Infinity) : Infinity);

/** Each order as a comparison. The ones it can't tell apart (no time to say, never run) come in
 *  the order they were made, which the file's isn't always: one your machines share is added to
 *  it as it arrives. */
const BY: Record<ScheduleSort, (a: ScheduleItem, b: ScheduleItem) => number> = {
  next: (a, b) => ascending(comes(a), comes(b)),
  last: (a, b) => ascending(-(a.runs[0]?.at ?? -Infinity), -(b.runs[0]?.at ?? -Infinity)),
  added: (a, b) => ascending(-a.createdAt, -b.createdAt),
  name: (a, b) => scheduleName(a).localeCompare(scheduleName(b), undefined, { sensitivity: 'base', numeric: true }),
};
const SORTED = (sort: ScheduleSort) => (a: ScheduleItem, b: ScheduleItem) => BY[sort](a, b) || ascending(a.createdAt, b.createdAt);

/** The Schedules view's lists: the schedules that go on running; the ones still to run that run
 *  once (`runsOnce`: a follow-up an agent left itself, mostly), apart so they don't crowd the
 *  rest; each in the order picked (`ScheduleSort`), by default the next to come first and the
 *  ones with no time to say (a watch, an event, a paused one) after, as they were made; and the
 *  finished ones, the latest to finish first whatever the order, which the view folds away. */
export function scheduleSections(items: ScheduleItem[], sort: ScheduleSort = 'next'): { active: ScheduleItem[]; oneOff: ScheduleItem[]; finished: ScheduleItem[] } {
  const running = items.filter((x) => x.finished == null);
  return {
    active: running.filter((x) => !runsOnce(x)).sort(SORTED(sort)),
    oneOff: running.filter(runsOnce).sort(SORTED(sort)),
    finished: items.filter((x) => x.finished != null).sort((a, b) => (b.finished ?? 0) - (a.finished ?? 0)),
  };
}

/** One of the Schedules view's sections, as Group by groups them: the ones that go on running
 *  (no heading), the one-offs, the finished. */
export interface ScheduleSection {
  id: 'active' | 'oneOff' | 'finished';
  rows: ScheduleItem[];
}

/** What the Schedules view lists, in order: its sections, and grouped by worktree or repo
 *  (groups.ts) when the view is, the pinned groups first with the ones that go on running, and
 *  the one-offs and the finished each grouped among themselves. */
export interface ScheduleListing {
  sections: ScheduleSection[];
  grouped: Grouped<ScheduleSection, ScheduleItem> | null;
}

/** Where a schedule goes when grouped by project, repo or worktree. */
export function schedulePlace(s: Pick<ScheduleItem, 'run' | 'runsIn' | 'worktree' | 'orchestrates'>, lanes: Lane[], by: GroupBy): Place {
  if (by === 'project') {
    const project = scheduleProject(s, lanes);
    return project ? { key: project, lane: null, repo: null, project } : { key: '', lane: null, repo: null, project: null };
  }
  return placeIn(scheduleLane(s, lanes), by);
}

export interface ScheduleGrouping {
  by: ListGroupBy;
  /** Every worktree on the board, for where each schedule runs (`scheduleLane`). */
  lanes: Lane[];
  /** Custom user group order for `by`; alphabetical by default. */
  order?: readonly string[];
  /** The groups opened past `GROUP_SHOWN`. */
  opened: ReadonlySet<string>;
  /** A search found them: every group lists all of its own. */
  whole: boolean;
  /** One that never folds away: the one picked, or one with a session at work or waiting on you
   *  under it (boardLists.ts `scheduleAtWork`). */
  awake?: (item: ScheduleItem) => boolean;
}

export function scheduleListing(items: ScheduleItem[], sort: ScheduleSort, group: ScheduleGrouping): ScheduleListing {
  const { active, oneOff, finished } = scheduleSections(items, sort);
  const sections: ScheduleSection[] = [
    { id: 'active', rows: active },
    { id: 'oneOff', rows: oneOff },
    { id: 'finished', rows: finished },
  ];
  const by = group.by;
  const grouped =
    by === 'none'
      ? null
      : groupRows(sections, by, (x) => schedulePlace(x, group.lanes, by), {
          order: group.order,
          opened: group.opened,
          whole: group.whole,
          awake: ([x]) => !!group.awake?.(x),
          apart: (id) => id !== 'active',
        });
  return { sections, grouped };
}

/** The schedule the view shows until one is picked: the first it lists, which is another than the
 *  first there is when that one runs once, is finished, or isn't in the first group. */
export function firstSchedule(listing: ScheduleListing): ScheduleItem | null {
  const rows = listing.grouped ? groupsOf(listing.grouped).flatMap((g) => g.rows) : listing.sections.flatMap((s) => s.rows);
  return rows[0] ?? null;
}

/** The machines a schedule that runs on one of yours may prefer, for its dialog's menu: none in
 *  particular, this one, and your other machines (not phones, which run nothing). One it prefers
 *  that's no longer linked stays, so saving doesn't change it unasked. */
export function preferOptions(devices: DevicesView | null, prefer: string): { id: string; label: string }[] {
  const out = [{ id: '', label: 'No machine in particular' }];
  if (devices?.me.key) out.push({ id: devices.me.key, label: `This machine, ${devices.me.name}` });
  for (const d of devices?.devices ?? []) if (d.kind === 'mac') out.push({ id: d.key, label: d.name });
  if (prefer && !out.some((o) => o.id === prefer)) out.push({ id: prefer, label: 'A machine that’s no longer linked' });
  return out;
}

/** Whether a schedule's dialog asks which of your machines run it: once you've linked another,
 *  or for one that already runs on them. */
export function asksRunsOn(devices: DevicesView | null, runsOn: ScheduleRunsOn | undefined): boolean {
  return (runsOn?.kind ?? 'this') !== 'this' || (!!devices?.enabled && devices.devices.some((d) => d.kind === 'mac'));
}

/** The schedule each session came from, by its key: the session a run started or sent to, and
 *  the session a schedule keeps going. The latest run's schedule wins a session two ran. */
export function scheduleBySession(items: ScheduleItem[] | undefined): Map<string, { id: string; name: string }> {
  const out = new Map<string, { id: string; name: string; at: number }>();
  for (const s of items ?? []) {
    const note = (key: string | undefined, at: number) => {
      if (!key) return;
      const had = out.get(key);
      if (!had || had.at < at) out.set(key, { id: s.id, name: s.name, at });
    };
    for (const r of s.runs) if (r.outcome === 'started' || r.outcome === 'resumed') note(r.session, r.at);
    note(s.thread, 0);
  }
  return new Map([...out].map(([k, { id, name }]) => [k, { id, name }]));
}

/** A new schedule, as the dialog starts one: weekdays at 9, a new session each time, in the
 *  worktree you're on (none for a chat). */
export function blankSchedule(folder: string): Schedule {
  return {
    id: '',
    name: '',
    paused: false,
    when: { kind: 'time', rule: 'weekdays 9:00' },
    onlyIf: '',
    run: { to: 'new', session: '', folder, worktree: false, account: '', model: null, effort: null, place: null, prompt: '', freshAfter: '' },
    quiet: false,
    overlap: 'skip',
    catchUp: 'once',
    perDay: null,
    notes: false,
    createdAt: 0,
    updatedAt: 0,
    once: false,
  };
}

/** A schedule as it's saved from an item on the board: the item's own fields, without what the
 *  board adds to say how it's going. */
export function scheduleOf(item: ScheduleItem): Schedule {
  return {
    id: item.id,
    name: item.name,
    paused: item.paused,
    when: item.when,
    onlyIf: item.onlyIf,
    run: item.run,
    quiet: item.quiet,
    overlap: item.overlap,
    catchUp: item.catchUp,
    perDay: item.perDay ?? null,
    notes: item.notes,
    createdAt: item.createdAt,
    updatedAt: item.updatedAt,
    createdBy: item.createdBy,
    runsOn: item.runsOn,
    once: item.once ?? false,
    // An orchestrator stays one through an edit: the engine turns down its event without it.
    ...(item.orchestrates ? { orchestrates: item.orchestrates } : {}),
  };
}

/** What `codealong schedule fire` is written as for a schedule, for the dialog to show. */
export function fireCommand(name: string): string {
  const quoted = /^[\w.-]+$/.test(name) ? name : `"${name.replace(/(["\\$`])/g, '\\$1')}"`;
  return `codealong schedule fire ${quoted} --key <what it's for> --title <what happened>`;
}

/** The worktrees a schedule can be in, as its dialog offers them: this Mac's on the board, but
 *  not Codealong's chats (a schedule in no worktree is one of those). */
export function scheduleLanes(lanes: Lane[]): Lane[] {
  return lanes.filter((l) => !l.device && !l.hidden && !l.startsChats && l.path);
}

/** The sessions a schedule can send to, as its dialog offers them: this Mac's, the ones in its
 *  worktree apart and first, the latest first in each. An archived one is offered only when it's
 *  the one already picked. */
export function scheduleSessions(lanes: Lane[], folder: string, keep: string): { here: Placed[]; rest: Placed[] } {
  const all = [...sessionHomes(lanes.filter((l) => !l.device)).values()].filter((p) => !p.session.archived || p.session.key === keep).sort((a, b) => b.session.lastActivity - a.session.lastActivity);
  const inFolder = (p: Placed) => !!folder && !p.lane.startsChats && p.lane.path === folder;
  return { here: all.filter(inFolder), rest: all.filter((p) => !inFolder(p)) };
}

/** The worktree a session works in, for a schedule that sends to it: where its watch and Only if
 *  run, and the project it's listed in. None for a chat or a session that's gone. */
export function sessionFolder(lanes: Lane[], key: string): string {
  const home = homeOf(lanes, key);
  return home && !home.lane.device && !home.lane.startsChats ? home.lane.path : '';
}

/** The project a schedule is in, for the Schedules view to list it under: its worktree's, else
 *  the project of the session it sends to. None for a chat of its own, or a worktree that's gone. */
export function scheduleProject(s: Pick<ScheduleItem, 'run' | 'runsIn' | 'worktree' | 'orchestrates'>, lanes: Lane[]): string | null {
  // An orchestrator's is the project it keeps watch over; All projects' is in none.
  if (s.orchestrates) return s.orchestrates.project ?? null;
  return scheduleLane(s, lanes)?.project ?? null;
}

/** The worktree a schedule runs in, on this Mac: the one the engine found for its folder
 *  (`worktree`: a folder written from your home, `~/code/atlas`, as one your machines share writes
 *  it, is read only there), else the one at its folder's path, else the worktree of the session it
 *  sends to. None for a chat of its own, or a worktree that's gone. What Group by puts it under,
 *  and what says its project (`scheduleProject`). */
export function scheduleLane(s: Pick<ScheduleItem, 'run' | 'runsIn' | 'worktree'>, lanes: Lane[]): Lane | null {
  const folder = s.runsIn ?? s.run.folder;
  const here = (l: Lane) => !l.device && !l.startsChats;
  const lane = (s.worktree ? lanes.find((l) => here(l) && l.id === s.worktree) : undefined) ?? (folder ? lanes.find((l) => here(l) && l.path === folder) : undefined);
  if (lane) return lane;
  const home = s.run.to === 'session' ? homeOf(lanes, s.run.session) : null;
  return home && !home.lane.device && !home.lane.startsChats ? home.lane : null;
}

/** Where a new schedule starts: the worktree selected, when it's in the project shown, else the
 *  one the quick panel would start in there (a pinned one, else the latest repo's main checkout);
 *  in no project under All projects with none selected. */
export function newScheduleFolder(lanes: Lane[], project: string, selected: Lane | null, pinned: string[]): string {
  const mine = scheduleLanes(lanes);
  // The selection may be a copy of the board's worktree (boardLists.ts `unarchived`).
  const here = selected && mine.find((l) => l.id === selected.id);
  if (here && (project === ALL_PROJECTS || here.project === project)) return here.path;
  if (project === ALL_PROJECTS) return '';
  return laneGroups(mine.filter((l) => l.project === project), pinned)[0]?.lanes[0]?.path ?? '';
}

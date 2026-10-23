// Schedules as the Schedules view says them (ui/SchedulesView.tsx, ui/SchedulePanel.tsx,
// ui/ScheduleDialog.tsx): what a run came to, when one comes next, what a schedule does, which
// schedule a session came from, and a new schedule's draft. What's due and what a run does are the
// engine's (src-tauri/src/schedules/); these only say it.

import { resetsPhrase } from './format';
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
 *  that runs only when something happens. A time that's come and not yet run is `due now`. */
export function nextWords(item: Pick<ScheduleItem, 'paused' | 'nextAt' | 'when'>, now: number): string {
  if (item.paused) return 'Paused';
  if (item.nextAt == null) return '';
  if (item.nextAt <= now) return item.when.kind === 'watch' ? 'Looks now' : 'Due now';
  const at = resetsPhrase(item.nextAt, now);
  return item.when.kind === 'watch' ? `Looks ${at}` : `Next ${at}`;
}

/** What it does, in a few words: `New session in web on Auto`, `Sends to “Fix the header”`. The
 *  folder's own name, as its worktree's row says it: where it runs here, for one whose repo's
 *  checkout is elsewhere on this machine than on the one it came from. */
export function doesWords(s: Pick<ScheduleItem, 'run' | 'runsIn'>, sessionTitle: (key: string) => string | null): string {
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
      const title = sessionTitle(r.session);
      return title ? `Sends to “${title}”` : 'Sends to a session that’s gone';
    }
  }
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
export function scheduleProject(s: Pick<ScheduleItem, 'run' | 'runsIn'>, lanes: Lane[]): string | null {
  const folder = s.runsIn ?? s.run.folder;
  const lane = folder ? lanes.find((l) => !l.device && !l.startsChats && l.path === folder) : null;
  if (lane) return lane.project;
  const home = s.run.to === 'session' ? homeOf(lanes, s.run.session) : null;
  return home && !home.lane.device && !home.lane.startsChats ? home.lane.project : null;
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

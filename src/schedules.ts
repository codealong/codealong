// Schedules as the Schedules view says them (ui/SchedulesView.tsx, ui/SchedulePanel.tsx,
// ui/ScheduleDialog.tsx): what a run came to, when one comes next, what a schedule does, which
// schedule a session came from, and a new schedule's draft. What's due and what a run does are the
// engine's (src-tauri/src/schedules/); these only say it.

import { resetsPhrase } from './format';
import type { Lane, Schedule, ScheduleItem, ScheduleOutcome, ScheduleWhen, Session } from './types';

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
 *  folder's own name, as its worktree's row says it. */
export function doesWords(s: Pick<Schedule, 'run'>, sessionTitle: (key: string) => string | null): string {
  const r = s.run;
  const folder = r.folder.trim();
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
  };
}

/** What `codealong schedule fire` is written as for a schedule, for the dialog to show. */
export function fireCommand(name: string): string {
  const quoted = /^[\w.-]+$/.test(name) ? name : `"${name.replace(/(["\\$`])/g, '\\$1')}"`;
  return `codealong schedule fire ${quoted} --key <what it's for> --title <what happened>`;
}

/** The worktrees a schedule can start in, as the dialog lists them: this Mac's, by project and
 *  name, a repo's main checkout first. */
export function scheduleFolders(lanes: Lane[]): { id: string; label: string }[] {
  return lanes
    .filter((l) => !l.device && !l.hidden && l.path)
    .slice()
    .sort((a, b) => a.project.localeCompare(b.project) || a.repo.localeCompare(b.repo) || Number(b.origin === 'main') - Number(a.origin === 'main') || a.callsign.localeCompare(b.callsign))
    .map((l) => ({ id: l.path, label: `${l.project} · ${l.callsign}` }));
}

/** The sessions a schedule can send to, as the dialog lists them: this Mac's, the latest first. */
export function scheduleTargets(lanes: Lane[], keep: string): { id: string; label: string }[] {
  const sessions: Session[] = lanes.filter((l) => !l.device).flatMap((l) => l.sessions.filter((s) => !s.visitor && (!s.archived || s.key === keep)));
  return sessions
    .sort((a, b) => b.lastActivity - a.lastActivity)
    .slice(0, 60)
    .map((s) => ({ id: s.key, label: `${s.title} · ${s.account}` }));
}

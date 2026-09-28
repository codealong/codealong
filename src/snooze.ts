// Snoozing a session: it stays off the floating bar's Needs you and Your turn until the time you
// pick, then comes back, as Your turn if nothing else would bring it (board::apply_snooze). The
// engine keeps snoozes in state.json; this is the menu that sets them.

import { snoozeSessions } from './api';
import type { MenuAction, MenuGroup } from './contextMenu';

const MIN = 60_000;
const HOUR = 60 * MIN;
/** Tomorrow's snooze ends at this hour, and before this hour "tomorrow" still means this morning. */
const MORNING = 9;
const SMALL_HOURS = 5;

export interface SnoozeChoice {
  label: string;
  until: number;
}

/** The times a snooze can run to: a few from now, and the next morning. */
export function snoozeChoices(now: number): SnoozeChoice[] {
  const d = new Date(now);
  const morning = new Date(d.getFullYear(), d.getMonth(), d.getDate() + (d.getHours() < SMALL_HOURS ? 0 : 1), MORNING).getTime();
  return [
    { label: 'For 15 minutes', until: now + 15 * MIN },
    { label: 'For 1 hour', until: now + HOUR },
    { label: 'For 3 hours', until: now + 3 * HOUR },
    { label: `Until ${untilWords(morning, now)}`, until: morning },
  ];
}

function clock(at: number): string {
  return new Date(at).toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' }).replace(':00', '').replace(/\s/g, ' ');
}

/** When a snooze ends, as a clock says it: `3:40 PM` today, `tomorrow 9 AM`, `Fri 9 AM` within the
 *  week, `Oct 3, 9 AM` beyond. */
export function untilWords(at: number, now: number): string {
  const day = (t: number) => {
    const d = new Date(t);
    return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  };
  const days = Math.round((day(at) - day(now)) / (24 * HOUR));
  if (days <= 0) return clock(at);
  if (days === 1) return `tomorrow ${clock(at)}`;
  const date = new Date(at).toLocaleDateString('en-US', days < 7 ? { weekday: 'short' } : { month: 'short', day: 'numeric' });
  return `${date}${days < 7 ? '' : ','} ${clock(at)}`;
}

/** The Snooze submenu for sessions: the times, and Wake now for one that's snoozed already. */
export function snoozeMenu(keys: string[], now: number, opts: { label?: string; until?: number | null; onDone?: (act: Promise<void>) => void } = {}): MenuGroup {
  const done = opts.onDone ?? ((act: Promise<void>) => void act.catch(() => {}));
  const items: (MenuAction | 'separator')[] = snoozeChoices(now).map((c) => ({ label: c.label, run: () => done(snoozeSessions(keys, c.until)) }));
  if (opts.until) items.push('separator', { label: 'Wake now', run: () => done(snoozeSessions(keys, null)) });
  return { label: opts.label ?? (opts.until ? `Snoozed until ${untilWords(opts.until, now)}` : 'Snooze'), items };
}

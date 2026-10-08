// Snoozing a session: it stays off the floating bar's Needs you and Your turn until the time you
// pick, then comes back, as Your turn if nothing else would bring it (board::apply_snooze). A
// review asked of you snoozes the same way: the PR stays out of Review requested, the bar's
// reviews and the mark until its time (board::apply_review_snoozes). The engine keeps snoozes in
// state.json; these are the menus that set them.

import { snoozeAhead, snoozeReviewsAhead } from './ahead';
import type { MenuAction, MenuGroup } from './contextMenu';
import type { PrItem } from './types';

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

/** The chord for each time a snooze can run to, in the menu's order: ⌃⌘1 for the first (15
 *  minutes) to ⌃⌘4 for the last (the next morning). */
export const SNOOZE_CHORDS = ['Ctrl+Cmd+1', 'Ctrl+Cmd+2', 'Ctrl+Cmd+3', 'Ctrl+Cmd+4'];

/** Which of a snooze menu's times ⌃⌘ with this key picks, from 0; -1 for a key that picks none. */
export function snoozeChordIndex(key: string): number {
  return SNOOZE_CHORDS.indexOf(`Ctrl+Cmd+${key}`);
}

interface SnoozeOpts {
  label?: string;
  /** When the snooze it has now ends, for one that's snoozed. */
  until?: number | null;
  /** Gets each snooze as it goes, to say how it went the window's way. */
  onDone: (act: Promise<void>) => void;
  /** Each time says its chord, where the window takes them. */
  chords?: boolean;
}

/** A snooze's times as a submenu, each setting it with `set`, and Wake now for one that's
 *  snoozed already. */
function timesMenu(set: (until: number | null) => Promise<void>, now: number, opts: SnoozeOpts, label: string): MenuGroup {
  const done = opts.onDone;
  const items: (MenuAction | 'separator')[] = snoozeChoices(now).map((c, i) => ({ label: c.label, run: () => done(set(c.until)), ...(opts.chords && SNOOZE_CHORDS[i] ? { accelerator: SNOOZE_CHORDS[i] } : {}) }));
  if (opts.until) items.push('separator', { label: 'Wake now', run: () => done(set(null)) });
  return { label, items };
}

/** The Snooze submenu for sessions: the times, and Wake now for one that's snoozed already. */
export function snoozeMenu(keys: string[], now: number, opts: SnoozeOpts): MenuGroup {
  return timesMenu((until) => snoozeAhead(keys, until), now, opts, opts.label ?? (opts.until ? `Snoozed until ${untilWords(opts.until, now)}` : 'Snooze'));
}

/** Snooze review for several PRs at once, each an open one that asks for your review: the times,
 *  and Wake now where any is snoozed already. */
export function reviewsSnoozeMenu(prs: Pick<PrItem, 'key' | 'snoozedUntil'>[], now: number, onDone: SnoozeOpts['onDone'], chords = false): MenuGroup {
  const until = Math.max(0, ...prs.map((p) => p.snoozedUntil ?? 0)) || null;
  return timesMenu((u) => snoozeReviewsAhead(prs.map((p) => p.key), u), now, { until, onDone, chords }, `Snooze ${prs.length} ${prs.length === 1 ? 'review' : 'reviews'}`);
}

/** The Snooze review submenu for an open PR that asks for your review: the times, and Wake now
 *  for one that's snoozed already. None for any other PR. */
export function reviewSnoozeMenu(pr: Pick<PrItem, 'key' | 'reviewRequested' | 'state' | 'snoozedUntil'>, now: number, onDone: SnoozeOpts['onDone'], chords = false): MenuGroup | undefined {
  if (!pr.reviewRequested || pr.state === 'merged' || pr.state === 'closed') return undefined;
  const until = pr.snoozedUntil;
  return timesMenu((u) => snoozeReviewsAhead([pr.key], u), now, { until, onDone, chords }, until ? `Review snoozed until ${untilWords(until, now)}` : 'Snooze review');
}

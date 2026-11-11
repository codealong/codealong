// Snoozing a session: it stays off the floating bar's Needs you and Your turn until the time you
// pick, then comes back, as Your turn if nothing else would bring it (board::apply_snooze). A
// review asked of you snoozes the same way: the PR stays out of Review requested, the bar's
// reviews and the mark until its time (board::apply_review_snoozes). The engine keeps snoozes in
// state.json; these are the menus that set them, and Pick a time… asks the main window for any
// other time (ui/SnoozeDialog.tsx). A session whose last reply says its work goes on at a later
// time is offered a snooze until then (`suggestedSnooze`): first in its Snooze menu, with a chord
// of its own, and above its box (ui/SnoozeSuggestion.tsx).

import { snoozeAhead, snoozedFrom, snoozeReviewsAhead } from './ahead';
import { showMain } from './api';
import type { MenuAction, MenuGroup } from './contextMenu';
import { readBy } from './insights';
import type { PrItem, Session } from './types';

const MIN = 60_000;
const HOUR = 60 * MIN;
/** Tomorrow's snooze ends at this hour, and before this hour "tomorrow" still means this morning. */
export const MORNING = 9;
const SMALL_HOURS = 5;
/** The end of the working day, offered while it's an hour off or more. */
const EVENING = 17;
const MONDAY = 1;

export interface SnoozeChoice {
  label: string;
  until: number;
  /** Its chord (`SNOOZE_CHORDS`): the same for the same time, whichever others are offered. */
  chord: string;
}

/** The chord for each time a snooze can run to, in the menu's order: ⌃⌘1 for the first (15
 *  minutes) to ⌃⌘7 for the last (the next week). 5 PM's, ⌃⌘5, picks nothing while it isn't
 *  offered. */
export const SNOOZE_CHORDS = ['Ctrl+Cmd+1', 'Ctrl+Cmd+2', 'Ctrl+Cmd+3', 'Ctrl+Cmd+4', 'Ctrl+Cmd+5', 'Ctrl+Cmd+6', 'Ctrl+Cmd+7'];
/** The chord for the time a snooze is suggested until (`suggestedSnooze`), after the times' own:
 *  it picks nothing for a session with none. */
export const SUGGESTED_CHORD = 'Ctrl+Cmd+8';
/** Every chord a Snooze menu can have. */
const MENU_CHORDS = [...SNOOZE_CHORDS, SUGGESTED_CHORD];

/** A time to offer a snooze until, and who read the session's last reply that way. */
export interface SuggestedSnooze {
  until: number;
  /** Jev's read, or the agent that tagged it (insights.ts `readBy`), with the agent's note. */
  by: string;
}

/** A suggested time less than this far off isn't worth a snooze. */
const WORTH = 5 * MIN;

/**
 * The time to offer a snooze until for a session: when its last reply says the work goes on,
 * with nothing for you or it to do before then, as Jev read it (insights.rs `waits_question`) or
 * an agent that tagged it said (`codealong session tag --waits-until`). None for a reply that
 * names no such time, once the time is all but here, and for a session that's snoozed already or
 * archived. Only offered: nothing snoozes by itself.
 */
export function suggestedSnooze(s: Pick<Session, 'insight' | 'snoozedUntil' | 'archived'>, now: number): SuggestedSnooze | null {
  const until = s.insight?.waitsUntil;
  if (!until || until - now < WORTH || s.snoozedUntil || s.archived) return null;
  const note = s.insight?.by && s.insight.note ? `: ${s.insight.note}` : '';
  return { until, by: `${readBy(s.insight)}${note}` };
}

/** The next morning at nine: this morning's in the small hours. */
export function nextMorning(now: number): Date {
  const d = new Date(now);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + (d.getHours() < SMALL_HOURS ? 0 : 1), MORNING);
}

/** The times a snooze can run to: a few from now; 5 PM, from the morning until it's less than
 *  an hour off; the next morning; and the Monday morning after it, a week on when the next
 *  morning is a Monday's, so the two are never the same time. */
export function snoozeChoices(now: number): SnoozeChoice[] {
  const d = new Date(now);
  const day = nextMorning(now);
  const morning = day.getTime();
  const evening = new Date(d.getFullYear(), d.getMonth(), d.getDate(), EVENING).getTime();
  const week = new Date(day.getFullYear(), day.getMonth(), day.getDate() + ((MONDAY - day.getDay() + 7) % 7 || 7), MORNING).getTime();
  // In the chords' order: one left out keeps its chord to itself.
  const times = [
    { label: 'For 15 minutes', until: now + 15 * MIN },
    { label: 'For 30 minutes', until: now + 30 * MIN },
    { label: 'For 1 hour', until: now + HOUR },
    { label: 'For 3 hours', until: now + 3 * HOUR },
    d.getHours() >= SMALL_HOURS && evening - now >= HOUR ? { label: `Until ${untilWords(evening, now)}`, until: evening } : null,
    { label: `Until ${untilWords(morning, now)}`, until: morning },
    { label: `Until ${untilWords(week, now)}`, until: week },
  ];
  return times.flatMap((t, i) => (t ? [{ ...t, chord: SNOOZE_CHORDS[i] }] : []));
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

/** Which of a Snooze menu's chords ⌃⌘ with this key is, from 0, the suggested time's after the
 *  times' own; -1 for a key that's none of them. */
export function snoozeChordIndex(key: string): number {
  return MENU_CHORDS.indexOf(`Ctrl+Cmd+${key}`);
}

/** A time in a snooze menu, with the chord that picks it whether or not the menu shows it. */
interface TimeAction extends MenuAction {
  chord: string;
}

/** The time ⌃⌘ with this key picks in a snooze menu (`timesMenu`), or none: a key that isn't a
 *  snooze chord, or the chord of a time the menu doesn't offer now. */
export function snoozeChorded(menu: MenuGroup, key: string): MenuAction | null {
  const chord = MENU_CHORDS[snoozeChordIndex(key)];
  const it = chord ? menu.items.find((i) => i !== 'separator' && 'chord' in i && i.chord === chord) : undefined;
  return it && it !== 'separator' && !('items' in it) ? it : null;
}

/** The suggested time in a session's Snooze menu, to offer by itself (ui/SnoozeSuggestion.tsx):
 *  none where the menu has none. */
export function suggestedIn(menu: MenuGroup | undefined): MenuAction | null {
  return menu ? snoozeChorded(menu, SUGGESTED_CHORD.slice(-1)) : null;
}

/** What Pick a time… asks the main window to snooze (api.ts `Reveal`): sessions, or your review
 *  of PRs. */
export interface SnoozeAsk {
  keys: string[];
  reviews: boolean;
}

/** What Pick a time… snoozes, for its dialog's title: a session by its title (`titleOf`, none
 *  for one the window doesn't list), your review of a PR by its number, or how many. */
export function askWords(ask: SnoozeAsk, titleOf: (key: string) => string | undefined): string {
  const n = ask.keys.length;
  const one = n === 1 ? ask.keys[0] : undefined;
  if (ask.reviews) return one ? `your review of #${one.slice(one.lastIndexOf('#') + 1)}` : `${n} reviews`;
  const title = one && titleOf(one);
  return title ? `“${title}”` : `${n} ${n === 1 ? 'session' : 'sessions'}`;
}

interface SnoozeOpts {
  label?: string;
  /** When the snooze it has now ends, for one that's snoozed. */
  until?: number | null;
  /** Gets each snooze as it goes, to say how it went the window's way. */
  onDone: (act: Promise<void>) => void;
  /** Each time says its chord, where the window takes them. */
  chords?: boolean;
  /** The time its last reply says the work goes on, for one session (`suggestedSnooze`): offered
   *  first, with a chord of its own. */
  suggested?: number;
}

/** A snooze's times as a submenu, each setting it with `set`; Pick a time…, which asks the main
 *  window for another (ui/SnoozeDialog.tsx), whichever window this is; and Wake now for one
 *  that's snoozed already. The time suggested for it comes first, apart from the rest. */
function timesMenu(set: (until: number | null) => Promise<void>, ask: SnoozeAsk, now: number, opts: SnoozeOpts, label: string): MenuGroup {
  const done = opts.onDone;
  const time = (c: SnoozeChoice): TimeAction => ({ label: c.label, run: () => done(set(c.until)), chord: c.chord, ...(opts.chords ? { accelerator: c.chord } : {}) });
  const items: (TimeAction | MenuAction | 'separator')[] = snoozeChoices(now).map(time);
  if (opts.suggested) items.unshift(time({ label: `Until ${untilWords(opts.suggested, now)}, as its reply says`, until: opts.suggested, chord: SUGGESTED_CHORD }), 'separator');
  items.push({
    label: 'Pick a time…',
    run: () => {
      // The main window asks, snoozes and says how that went; this one says they left, as for a
      // time of its own, once they're snoozed (a queue goes on to the next), and nothing if the
      // dialog is put away. The main window coming forward is all there is to fail here.
      snoozedFrom(ask.keys, ask.reviews, () => done(Promise.resolve()));
      void showMain({ kind: 'snooze', ...ask }).catch(() => {});
    },
  });
  if (opts.until) items.push('separator', { label: 'Wake now', run: () => done(set(null)) });
  return { label, items };
}

/** The Snooze submenu for sessions: the times, and Wake now for one that's snoozed already. */
export function snoozeMenu(keys: string[], now: number, opts: SnoozeOpts): MenuGroup {
  return timesMenu((until) => snoozeAhead(keys, until), { keys, reviews: false }, now, opts, opts.label ?? (opts.until ? `Snoozed until ${untilWords(opts.until, now)}` : 'Snooze'));
}

/** Snooze review for several PRs at once, each an open one that asks for your review: the times,
 *  and Wake now where any is snoozed already. */
export function reviewsSnoozeMenu(prs: Pick<PrItem, 'key' | 'snoozedUntil'>[], now: number, onDone: SnoozeOpts['onDone'], chords = false): MenuGroup {
  const until = Math.max(0, ...prs.map((p) => p.snoozedUntil ?? 0)) || null;
  const keys = prs.map((p) => p.key);
  return timesMenu((u) => snoozeReviewsAhead(keys, u), { keys, reviews: true }, now, { until, onDone, chords }, `Snooze ${prs.length} ${prs.length === 1 ? 'review' : 'reviews'}`);
}

/** The Snooze review submenu for an open PR that asks for your review: the times, and Wake now
 *  for one that's snoozed already. None for any other PR. */
export function reviewSnoozeMenu(pr: Pick<PrItem, 'key' | 'reviewRequested' | 'state' | 'snoozedUntil'>, now: number, onDone: SnoozeOpts['onDone'], chords = false): MenuGroup | undefined {
  if (!pr.reviewRequested || pr.state === 'merged' || pr.state === 'closed') return undefined;
  const until = pr.snoozedUntil;
  return timesMenu((u) => snoozeReviewsAhead([pr.key], u), { keys: [pr.key], reviews: true }, now, { until, onDone, chords }, until ? `Review snoozed until ${untilWords(until, now)}` : 'Snooze review');
}

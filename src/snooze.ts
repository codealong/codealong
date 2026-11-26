// Snoozing a session: it stays off the floating bar's Needs you and Your turn until the time you
// pick, then comes back, as Your turn if nothing else would bring it (board::apply_snooze). A
// review asked of you snoozes the same way: the PR stays out of Review requested, the bar's
// reviews and the mark until its time (board::apply_review_snoozes). The engine keeps snoozes in
// state.json; these are the menus that set them, and Pick a time… asks the main window for any
// other time (ui/SnoozeDialog.tsx). A session whose last reply says its work goes on at a later
// time is offered a snooze until then (`suggestedSnooze`): first in its Snooze menu, with a chord
// of its own, and above its box (ui/SnoozeSuggestion.tsx). One stopped at its usage limit is
// offered one until the limit lifts the same way, its banner's button in place of the line above
// its box (ui/SessionHeader.tsx); and one whose account is out of credits, or whose limit doesn't
// say when it lifts, one until its account has room again, which the engine ends once the limit
// is over (board::room_back), at the latest when it lifts.

import { snoozeAhead, snoozedFrom, snoozeReviewsAhead } from './ahead';
import { showMain, todoOp } from './api';
import type { MenuAction, MenuGroup } from './contextMenu';
import { readBy, readerOf } from './insights';
import type { PrItem, Session, SessionLimit, Todo } from './types';

const MIN = 60_000;
const HOUR = 60 * MIN;
/** The longest a snooze until room lasts, from the refusal, where the limit doesn't say when it
 *  lifts: as long as the engine counts a refusal as word that its account is out (start.rs
 *  `REFUSED_FOR`), which a monthly spend limit's month is inside. */
const ROOM_LONGEST = 31 * 24 * HOUR;
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

/** A time to offer a snooze until, and why then. */
export interface SuggestedSnooze {
  until: number;
  /** What comes then: its last reply says the work goes on (`reply`), the usage limit that
   *  stopped it lifts (`limit`), or its account has room again (`room`: out of credits, or a limit
   *  that doesn't say when it lifts), `until` being the latest. */
  why: 'reply' | 'limit' | 'room';
  /** Who read the reply that way, by name: Jev, or the agent that tagged it (insights.ts
   *  `readerOf`), said with the time wherever it's offered (`suggestedWhy`). None for a limit,
   *  which its harness said. */
  reader?: string;
  /** That read, for a tooltip (insights.ts `readBy`), with the agent's note. */
  by?: string;
}

/** What ends a suggested time's line wherever it's offered, by why it's suggested: who read the
 *  reply that way, as two readers can (Jev, or the agent that tagged it), or the limit that
 *  lifts then. A snooze until room says no time (`suggestedLabel`). */
export function suggestedWhy(s: Pick<SuggestedSnooze, 'why' | 'reader'>): string {
  return s.why === 'reply' ? `as ${s.reader ?? 'Jev'} read its reply` : 'when its limit lifts';
}

/** What a snooze until room runs to, in place of its time. */
const ROOM_WORDS = 'it has room';

/** The suggested time's line in a Snooze menu: `Until 10 PM, when its limit lifts`, or `Until it
 *  has room`. */
function suggestedLabel(s: Pick<SuggestedSnooze, 'until' | 'why' | 'reader'>, now: number): string {
  return s.why === 'room' ? `Until ${ROOM_WORDS}` : `Until ${untilWords(s.until, now)}, ${suggestedWhy(s)}`;
}

/** When a session's snooze ends, in words: its time (`untilWords`), or `it has room` for one
 *  until its account has room (`Session.snoozedForRoom`), whose time is only the latest. */
export function snoozedWords(s: { snoozedUntil?: number | null; snoozedForRoom?: boolean }, now: number): string {
  return s.snoozedForRoom ? ROOM_WORDS : untilWords(s.snoozedUntil ?? now, now);
}

/** A suggested time less than this far off isn't worth a snooze. */
const WORTH = 5 * MIN;

/**
 * The time to offer a snooze until for a session: when the usage limit that stopped its turn
 * lifts (`Session.limit`), as nothing goes on before then, and Codealong may tell it to carry on
 * a few minutes after (`SessionLimit.continues`); until its account has room again where the
 * limit can be over at a time nobody said (`room`): extra usage refused too (out of credits,
 * which buying more ends) or no time said, the lift (else a month after the refusal) the latest;
 * else when its last reply says the work goes on, with nothing for you or it to do before then,
 * as Jev read it (insights.rs `waits_question`) or an agent that tagged it said (`codealong
 * session tag --waits-until`). None for a reply that names no such time (nor one
 * that asks you something, which the engine holds of every read it shows, insights.rs
 * `settle_put_off`), once the time is all but here, and for a session that's snoozed already or
 * archived. Only offered: nothing snoozes by itself.
 */
export function suggestedSnooze(s: Pick<Session, 'insight' | 'snoozedUntil' | 'archived' | 'limit'> & Partial<Pick<Session, 'status'>>, now: number): SuggestedSnooze | null {
  if (s.snoozedUntil || s.archived) return null;
  const limit = s.status !== 'working' ? s.limit : null;
  const lifts = limit?.resetsAt;
  const latest = limit && (lifts ?? limit.at + ROOM_LONGEST);
  if (limit && latest && latest - now >= WORTH) return { until: latest, why: limit.spendCapped || lifts == null ? 'room' : 'limit' };
  const until = s.insight?.waitsUntil;
  if (!until || until - now < WORTH) return null;
  // Who read the day: Jev's, under a tag that read something else, is said as Jev's.
  const read = s.insight?.waitsByJev ? undefined : s.insight;
  const note = read?.by && read.note ? `: ${read.note}` : '';
  return { until, why: 'reply', reader: readerOf(read), by: `${readBy(read)}${note}` };
}

/** What the limit banner's snooze does, under the pointer (ui/SessionHeader.tsx): until the limit
 *  lifts, or until the account has room again, and what brings that. */
export function limitSnoozeTitle(s: Pick<SuggestedSnooze, 'until' | 'why'>, limit: Pick<SessionLimit, 'spendCapped' | 'resetsAt'> | null | undefined, now: number): string {
  const when = untilWords(s.until, now);
  if (s.why !== 'room') return `It leaves what waits on you until ${when}, when the limit lifts`;
  const how = limit?.spendCapped ? 'credits are bought, its spend limit is raised or a reset is used' : 'another turn on it goes through or its usage reads room';
  const latest = limit?.resetsAt != null ? `when the limit lifts, ${when},` : when;
  return `It leaves what waits on you until its account has room again: once ${how}, and ${latest} at the latest`;
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
  /** The snooze it has now is until its account has room, `until` only the latest. */
  forRoom?: boolean;
  /** Gets each snooze as it goes, to say how it went the window's way. */
  onDone: (act: Promise<void>) => void;
  /** Each time says its chord, where the window takes them. */
  chords?: boolean;
  /** The time suggested for one session, and why (`suggestedSnooze`): offered first, with a chord
   *  of its own. */
  suggested?: Pick<SuggestedSnooze, 'until' | 'why' | 'reader'> | null;
}

/** A snooze's times as a submenu, each setting it with `set`; Pick a time…, which asks the main
 *  window for another (ui/SnoozeDialog.tsx), whichever window this is, where it can (`ask`); and
 *  Wake now for one that's snoozed already. The time suggested for it comes first, apart from the
 *  rest. */
function timesMenu(set: (until: number | null, room?: boolean) => Promise<void>, ask: SnoozeAsk | null, now: number, opts: SnoozeOpts, label: string): MenuGroup {
  const done = opts.onDone;
  const time = (c: SnoozeChoice, room = false): TimeAction => ({ label: c.label, run: () => done(set(c.until, room)), chord: c.chord, ...(opts.chords ? { accelerator: c.chord } : {}) });
  const items: (TimeAction | MenuAction | 'separator')[] = snoozeChoices(now).map((c) => time(c));
  const suggested = opts.suggested;
  if (suggested) items.unshift(time({ label: suggestedLabel(suggested, now), until: suggested.until, chord: SUGGESTED_CHORD }, suggested.why === 'room'), 'separator');
  if (ask) {
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
  }
  if (opts.until) items.push('separator', { label: 'Wake now', run: () => done(set(null)) });
  return { label, items };
}

/** The Snooze submenu for sessions: the times, and Wake now for one that's snoozed already. */
export function snoozeMenu(keys: string[], now: number, opts: SnoozeOpts): MenuGroup {
  const until = opts.until ? `Snoozed until ${snoozedWords({ snoozedUntil: opts.until, snoozedForRoom: opts.forRoom }, now)}` : 'Snooze';
  return timesMenu((u, room) => snoozeAhead(keys, u, false, room), { keys, reviews: false }, now, opts, opts.label ?? until);
}

/** Snooze review for several PRs at once, each an open one that asks for your review or waits on
 *  you to merge it: the times, and Wake now where any is snoozed already. */
export function reviewsSnoozeMenu(prs: Pick<PrItem, 'key' | 'snoozedUntil' | 'reviewRequested'>[], now: number, onDone: SnoozeOpts['onDone'], chords = false): MenuGroup {
  const until = Math.max(0, ...prs.map((p) => p.snoozedUntil ?? 0)) || null;
  const keys = prs.map((p) => p.key);
  const what = prs.every((p) => p.reviewRequested) ? (prs.length === 1 ? 'review' : 'reviews') : prs.length === 1 ? 'PR' : 'PRs';
  return timesMenu((u) => snoozeReviewsAhead(keys, u), { keys, reviews: true }, now, { until, onDone, chords }, `Snooze ${prs.length} ${what}`);
}

/** The Snooze submenu for a todo in what waits on you (the notch's todo card): the times, and
 *  Wake now for one that's snoozed already. Its due date stays as it is. The main window's
 *  snooze dialog takes sessions and reviews, so there's no Pick a time… yet. */
export function todoSnoozeMenu(t: Pick<Todo, 'id' | 'snoozedUntil'>, now: number, onDone: SnoozeOpts['onDone'], chords = false): MenuGroup {
  const until = t.snoozedUntil ?? null;
  const set = (u: number | null) => todoOp({ op: 'update', id: t.id, patch: { snoozed: u } }).then(() => undefined);
  return timesMenu(set, null, now, { until, onDone, chords }, until ? `Snoozed until ${untilWords(until, now)}` : 'Snooze');
}

/** The Snooze review submenu for an open PR that asks for your review, or Snooze for one of yours
 *  that waits on you to merge it (`PrItem.waitsToMerge`): the times, and Wake now for one that's
 *  snoozed already. None for any other PR. */
export function reviewSnoozeMenu(pr: Pick<PrItem, 'key' | 'reviewRequested' | 'waitsToMerge' | 'state' | 'snoozedUntil'>, now: number, onDone: SnoozeOpts['onDone'], chords = false): MenuGroup | undefined {
  if (!(pr.reviewRequested || pr.waitsToMerge) || pr.state === 'merged' || pr.state === 'closed') return undefined;
  const until = pr.snoozedUntil;
  const label = pr.reviewRequested ? (until ? `Review snoozed until ${untilWords(until, now)}` : 'Snooze review') : until ? `Snoozed until ${untilWords(until, now)}` : 'Snooze';
  return timesMenu((u) => snoozeReviewsAhead([pr.key], u), { keys: [pr.key], reviews: true }, now, { until, onDone, chords }, label);
}

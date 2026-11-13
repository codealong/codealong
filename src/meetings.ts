// The meeting on offer, in words (src-tauri/src/meetings.rs): as one in your calendar is about
// to start, the board says so (`Board.meeting`), and the windows ask whether to take its notes
// (ui/Meeting.tsx). And where your calendar stands, as Settings says it.

import { duration } from './format';
import { nowMeeting } from './listening';
import type { CalendarInfo, CalendarStatus, KeptMeeting, Meeting, MeetingAhead } from './types';

/** Names as a sentence lists them: "a", "a and b", "a, b and c" (util.rs `listed`). */
function listed(names: readonly string[]): string {
  return names.length < 2 ? (names[0] ?? '') : `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
}

/** When a meeting starts, said from `now`: "in 1 min" until it does, "now" for its first minute,
 *  then how long ago. */
export function meetingWhen(m: Pick<Meeting, 'start'>, now: number): string {
  const until = m.start - now;
  if (until > 30_000) return `in ${Math.max(1, Math.round(until / 60_000))} min`;
  if (until > -60_000) return 'now';
  return `started ${Math.floor(-until / 60_000)} min ago`;
}

/** The others in a meeting: "with Dana and Lee", the first four and how many more of a crowd, or
 *  nothing when it's only you (meetings.rs `with_whom`). */
export function meetingWith(m: Pick<Meeting, 'attendees'>): string {
  const names = m.attendees;
  if (names.length === 0) return '';
  if (names.length > 4) return `with ${names.slice(0, 4).join(', ')} and ${names.length - 4} more`;
  return `with ${listed(names)}`;
}

/** A time of day, as this Mac writes one. */
function hour(at: number): string {
  return new Date(at).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
}

/** When a meeting is, for a list of the day ahead: its start and end as this Mac writes times,
 *  with "Tomorrow" ahead of one that isn't today. */
export function meetingTime(m: Pick<Meeting, 'start' | 'end'>, now: number): string {
  const day = (at: number) => new Date(at).toDateString();
  return `${day(m.start) === day(now) ? '' : 'Tomorrow, '}${hour(m.start)} – ${hour(m.end)}`;
}

/** Where a meeting in the day ahead stands, said from `now`, for your meetings to head it with:
 *  heard, how long until it starts, or how long it has been going on. */
export function meetingStands(m: Pick<MeetingAhead, 'start' | 'heard'>, now: number): string {
  if (m.heard) return 'Being heard now';
  if (m.start - now > 30_000) return `Starts in ${duration(now, m.start)}`;
  if (m.start - now > -60_000) return 'Starting now';
  return `Started ${duration(m.start, now)} ago`;
}

/** What starting to listen to a meeting ahead does, said before you do: Codealong hears from
 *  the moment you start (meetings.rs `begin`), so one that hasn't begun is heard from now. One
 *  you haven't answered is offered as it's about to start (`on_offer`), which may be what you'd
 *  rather wait for. Nothing for one being heard. */
export function startWords(m: Pick<MeetingAhead, 'takeable' | 'heard' | 'answered'>): string {
  if (m.heard) return '';
  if (m.takeable) return 'Codealong hears it from the moment you start, with a session started for it that keeps its notes.';
  const offered = m.answered ? '' : ' Codealong offers to take its notes as it’s about to start.';
  return `It hasn’t begun: start now and Codealong hears from now, with a session started for it, so whatever is said until it begins is heard too.${offered}`;
}

/** The meeting your meetings show: one in the day ahead by its id, or one that was heard by its
 *  folder. */
export type MeetingPick = { ahead: string } | { kept: string };

/** The meeting your meetings open on: the one going on that nothing hears yet, which is the one
 *  you'd take, else the latest heard, else the first ahead. */
export function firstPick(kept: readonly Pick<KeptMeeting, 'folder'>[], ahead: readonly MeetingAhead[]): MeetingPick | null {
  const on = nowMeeting(ahead);
  if (on) return { ahead: on.id };
  if (kept[0]) return { kept: kept[0].folder };
  return ahead[0] ? { ahead: ahead[0].id } : null;
}

/** Whether your meetings as read are behind the day ahead: a meeting there is being heard whose
 *  folder isn't among them, or doesn't name its session yet (meetings.rs `begin` writes it as
 *  the hearing starts, and its session once that has come up). */
export function keptBehind(ahead: readonly Pick<MeetingAhead, 'id' | 'heard'>[], kept: readonly Pick<KeptMeeting, 'id' | 'session'>[]): boolean {
  return ahead.some((m) => m.heard && !kept.some((k) => k.id === m.id && k.session !== ''));
}

/** Your calendars by the account each is from, in the order they came (by account, then name),
 *  for Settings to list under each account's name. */
export function calendarsByAccount(calendars: readonly CalendarInfo[]): { source: string; calendars: CalendarInfo[] }[] {
  const out: { source: string; calendars: CalendarInfo[] }[] = [];
  for (const c of calendars) {
    const last = out[out.length - 1];
    if (last && last.source === c.source) last.calendars.push(c);
    else out.push({ source: c.source, calendars: [c] });
  }
  return out;
}

/** `[voice] meeting_calendars_off` with a calendar ticked or unticked: the ids left out, each once. */
export function calendarsOff(off: readonly string[], id: string, counts: boolean): string[] {
  const rest = off.filter((x) => x !== id);
  return counts ? rest : [...rest, id];
}

/** Where your calendar stands, for Settings: whose calendars are read and what's ahead, or why
 *  none is and what to do about it. Nothing before it has been read. */
export function calendarSays(c: CalendarStatus | null | undefined): string {
  if (!c) return '';
  if (c.problem) return `Your calendar couldn’t be read: ${c.problem}`;
  if (c.access === 'unasked') return 'macOS hasn’t asked yet whether Codealong may read your calendar.';
  if (c.access === 'denied' || c.access === 'restricted') return 'macOS isn’t letting Codealong read your calendar: turn Codealong on in System Settings › Privacy & Security › Calendars, with full access.';
  if (c.access !== 'allowed') return '';
  if (c.sources.length === 0) return 'Your calendar is read, and has no accounts in it: add yours (Google, iCloud, Exchange) in System Settings › Internet Accounts, with Calendars on.';
  const from = listed(c.sources.map((s) => s.title));
  const ahead = c.meetings === 0 ? 'No meetings in the day ahead.' : `${c.meetings === 1 ? '1 meeting' : `${c.meetings} meetings`} in the day ahead${c.nextAt ? `, the next at ${hour(c.nextAt)}` : ''}.`;
  return `Reading the calendars of ${from}. ${ahead}`;
}

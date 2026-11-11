// The meeting on offer, in words (src-tauri/src/meetings.rs): as one in your calendar is about
// to start, the board says so (`Board.meeting`), and the windows ask whether to take its notes
// (ui/Meeting.tsx). And where your calendar stands, as Settings says it.

import type { CalendarInfo, CalendarStatus, Meeting } from './types';

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

import { describe, expect, it } from 'vitest';
import { calendarsByAccount, calendarSays, calendarsOff, firstPick, heardIn, keptBehind, meetingCountdown, meetingStands, meetingTime, meetingWhen, meetingWith, startWords } from './meetings';
import type { CalendarStatus, MeetingAhead } from './types';

const NOW = 1_794_330_000_000;
const MIN = 60_000;

describe('the meeting on offer, in words', () => {
  it('says when it starts, from now', () => {
    const m = { start: NOW };
    expect(meetingWhen(m, NOW - 5 * MIN)).toBe('in 5 min');
    expect(meetingWhen(m, NOW - MIN)).toBe('in 1 min');
    expect(meetingWhen(m, NOW - 40_000)).toBe('in 1 min');
    expect(meetingWhen(m, NOW - 20_000)).toBe('now');
    expect(meetingWhen(m, NOW + 59_000)).toBe('now');
    expect(meetingWhen(m, NOW + MIN)).toBe('started 1 min ago');
    expect(meetingWhen(m, NOW + 9.5 * MIN)).toBe('started 9 min ago');
  });

  it('counts down to it in minutes, rounded up, where there’s little room', () => {
    const m = { start: NOW };
    expect(meetingCountdown(m, NOW - 5 * MIN)).toBe('5m');
    expect(meetingCountdown(m, NOW - 4 * MIN - 20_000)).toBe('5m');
    expect(meetingCountdown(m, NOW - 4 * MIN)).toBe('4m');
    expect(meetingCountdown(m, NOW - 30_000)).toBe('1m');
    // Never none: it's counted down to only until it's offered, at the latest as it starts.
    expect(meetingCountdown(m, NOW)).toBe('1m');
  });

  it('says who else is in it, as the engine does', () => {
    expect(meetingWith({ attendees: [] })).toBe('');
    expect(meetingWith({ attendees: ['Dana'] })).toBe('with Dana');
    expect(meetingWith({ attendees: ['Dana', 'Lee'] })).toBe('with Dana and Lee');
    expect(meetingWith({ attendees: ['A', 'B', 'C', 'D'] })).toBe('with A, B, C and D');
    expect(meetingWith({ attendees: ['A', 'B', 'C', 'D', 'E'] })).toBe('with A, B, C, D and 1 more');
    expect(meetingWith({ attendees: ['A', 'B', 'C', 'D', 'E', 'F'] })).toBe('with A, B, C, D and 2 more');
  });
});

describe('a meeting in the day ahead, in your meetings', () => {
  const ahead = (over: Partial<MeetingAhead> = {}): MeetingAhead => ({ id: 'e1', title: 'Renewal call', start: NOW, end: NOW + 30 * MIN, calendar: 'Work', attendees: ['Dana Kestrel'], takeable: false, ...over });

  it('says where it stands, from now', () => {
    expect(meetingStands(ahead(), NOW - 87 * MIN)).toBe('Starts in 1h 27m');
    expect(meetingStands(ahead(), NOW - 14 * MIN)).toBe('Starts in 14m');
    expect(meetingStands(ahead(), NOW - 10_000)).toBe('Starting now');
    expect(meetingStands(ahead(), NOW + 50_000)).toBe('Starting now');
    expect(meetingStands(ahead(), NOW + 5 * MIN)).toBe('Started 5m ago');
    expect(meetingStands(ahead({ heard: true }), NOW - 87 * MIN)).toBe('Being heard now');
  });

  it('says that starting now hears from now, and that it’s offered later', () => {
    expect(startWords(ahead({ takeable: true }))).toBe('Codealong hears it from the moment you start, with a session started for it that keeps its notes.');
    expect(startWords(ahead())).toBe('It hasn’t begun: start now and Codealong hears from now, with a session started for it, so whatever is said until it begins is heard too. Codealong offers to take its notes as it’s about to start.');
    // Passed on already: it isn't offered again.
    expect(startWords(ahead({ answered: true }))).not.toContain('offers');
    expect(startWords(ahead({ heard: true }))).toBe('');
  });

  it('opens on the one going on, else the latest heard, else the first ahead', () => {
    const kept = [{ folder: '/m/2026-11-10 1400 Renewal call' }, { folder: '/m/2026-11-09 0900 Standup' }];
    const later = ahead({ id: 'e2' });
    expect(firstPick(kept, [later, ahead({ id: 'e3', takeable: true })])).toEqual({ ahead: 'e3' });
    expect(firstPick(kept, [later, ahead({ id: 'e3', takeable: true, heard: true })])).toEqual({ kept: '/m/2026-11-10 1400 Renewal call' });
    expect(firstPick([], [later])).toEqual({ ahead: 'e2' });
    expect(firstPick([], [])).toBeNull();
  });

  it('finds the folder of the hearing going on, of a meeting taken twice too', () => {
    const first = { folder: '/m/2026-11-13 1100 Planning', listening: 'l1', session: 'claude:work:first' };
    const again = { folder: '/m/2026-11-13 1100 Planning (2)', listening: 'l2', session: '' };
    const heard = ahead({ id: 'e2', heard: true, listening: 'l2' });
    expect(heardIn(heard, [first, again])).toBe(again);
    expect(heardIn(heard, [first])).toBeNull();
    expect(heardIn(ahead({ id: 'e2', listening: 'l1' }), [first])).toBeNull();
  });

  it('says your meetings are behind while one being heard has no folder, or no session in it yet', () => {
    const heard = [ahead({ id: 'e2', heard: true, listening: 'l2' }), ahead({ id: 'e3' })];
    expect(keptBehind([ahead()], [])).toBe(false);
    expect(keptBehind(heard, [])).toBe(true);
    // Its folder is written as the hearing starts, its session once that has come up.
    expect(keptBehind(heard, [{ listening: 'l2', session: '' }])).toBe(true);
    expect(keptBehind(heard, [{ listening: 'l2', session: 'claude:work:meeting' }])).toBe(false);
    // Taken again: the folder of the time before, with its session, isn't this hearing's.
    expect(keptBehind(heard, [{ listening: 'l1', session: 'claude:work:first' }])).toBe(true);
    expect(keptBehind(heard, [{ listening: 'l2', session: '' }, { listening: 'l1', session: 'claude:work:first' }])).toBe(true);
  });
});

describe('the calendars that count', () => {
  it('are listed under the account each is from', () => {
    const cal = (id: string, title: string, source: string) => ({ id, title, source });
    expect(calendarsByAccount([])).toEqual([]);
    expect(calendarsByAccount([cal('1', 'Home', 'iCloud'), cal('2', 'Family', 'iCloud'), cal('3', 'Work', 'sam@northwind.example')])).toEqual([
      { source: 'iCloud', calendars: [cal('1', 'Home', 'iCloud'), cal('2', 'Family', 'iCloud')] },
      { source: 'sam@northwind.example', calendars: [cal('3', 'Work', 'sam@northwind.example')] },
    ]);
  });

  it('are left out by id, each once, and taken back in', () => {
    expect(calendarsOff([], 'a', false)).toEqual(['a']);
    expect(calendarsOff(['a'], 'a', false)).toEqual(['a']);
    expect(calendarsOff(['a', 'b'], 'a', true)).toEqual(['b']);
    expect(calendarsOff(['b'], 'a', true)).toEqual(['b']);
  });

  it('say when a meeting ahead is, and that it’s tomorrow when it is', () => {
    const today = new Date(2026, 10, 10, 9, 0).getTime();
    const at = (h: number, m: number, d = 10) => new Date(2026, 10, d, h, m).getTime();
    expect(meetingTime({ start: at(14, 0), end: at(14, 30) }, today)).toMatch(/^2:00.* – 2:30/);
    expect(meetingTime({ start: at(8, 0, 11), end: at(9, 0, 11) }, today)).toMatch(/^Tomorrow, 8:00.* – 9:00/);
  });
});

describe('where your calendar stands', () => {
  const status = (over: Partial<CalendarStatus> = {}): CalendarStatus => ({ on: true, access: 'allowed', sources: [{ title: 'sam@northwind.example', kind: 'caldav' }], meetings: 0, folder: '~/Meetings', ...over });

  it('says nothing before it’s been read', () => {
    expect(calendarSays(null)).toBe('');
    expect(calendarSays(undefined)).toBe('');
    expect(calendarSays(status({ access: '' }))).toBe('');
  });

  it('says whose calendars are read and what’s ahead', () => {
    expect(calendarSays(status())).toBe('Reading the calendars of sam@northwind.example. No meetings in the day ahead.');
    expect(calendarSays(status({ sources: [{ title: 'iCloud', kind: 'icloud' }, { title: 'Work', kind: 'exchange' }], meetings: 1 }))).toBe('Reading the calendars of iCloud and Work. 1 meeting in the day ahead.');
    expect(calendarSays(status({ meetings: 3, nextAt: NOW }))).toMatch(/^Reading the calendars of sam@northwind\.example\. 3 meetings in the day ahead, the next at \d{1,2}:\d\d/);
  });

  it('says why none is read, and what to do about it', () => {
    expect(calendarSays(status({ access: 'unasked', sources: [] }))).toBe('macOS hasn’t asked yet whether Codealong may read your calendar.');
    expect(calendarSays(status({ access: 'denied', sources: [] }))).toBe('macOS isn’t letting Codealong read your calendar: turn Codealong on in System Settings › Privacy & Security › Calendars, with full access.');
    expect(calendarSays(status({ access: 'restricted', sources: [] }))).toContain('isn’t letting Codealong read your calendar');
    expect(calendarSays(status({ sources: [] }))).toBe('Your calendar is read, and has no accounts in it: add yours (Google, iCloud, Exchange) in System Settings › Internet Accounts, with Calendars on.');
    expect(calendarSays(status({ problem: 'This Mac can’t hear.' }))).toBe('Your calendar couldn’t be read: This Mac can’t hear.');
  });
});

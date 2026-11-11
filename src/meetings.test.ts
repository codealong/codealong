import { describe, expect, it } from 'vitest';
import { calendarSays, meetingWhen, meetingWith } from './meetings';
import type { CalendarStatus } from './types';

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

  it('says who else is in it, as the engine does', () => {
    expect(meetingWith({ attendees: [] })).toBe('');
    expect(meetingWith({ attendees: ['Dana'] })).toBe('with Dana');
    expect(meetingWith({ attendees: ['Dana', 'Lee'] })).toBe('with Dana and Lee');
    expect(meetingWith({ attendees: ['A', 'B', 'C', 'D'] })).toBe('with A, B, C and D');
    expect(meetingWith({ attendees: ['A', 'B', 'C', 'D', 'E'] })).toBe('with A, B, C, D and 1 more');
    expect(meetingWith({ attendees: ['A', 'B', 'C', 'D', 'E', 'F'] })).toBe('with A, B, C, D and 2 more');
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

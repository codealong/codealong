import { describe, expect, it } from 'vitest';
import { dayKey, daysUntil, dueChoices, dueDate, dueWords } from './due';

// Local times, so the tests read the same in any time zone.
const at = (month: number, day: number, hour = 14) => new Date(2026, month - 1, day, hour).getTime();
const NOW = at(9, 30); // Wednesday afternoon

describe('due dates', () => {
  it('are local days', () => {
    expect(dayKey(NOW)).toBe('2026-09-30');
    expect(dayKey(at(10, 1, 0))).toBe('2026-10-01');
    expect(dueDate('2026-02-30')).toBeNull();
    expect(dueDate('Oct 3')).toBeNull();
    expect(daysUntil('2026-10-03', NOW)).toBe(3);
    expect(daysUntil('2026-09-30', at(9, 30, 23))).toBe(0);
    // Across the end of daylight saving time, a day is still a day.
    expect(daysUntil('2026-11-02', NOW)).toBe(33);
    expect(daysUntil('2026-09-28', NOW)).toBe(-2);
  });

  it('say how far off they are', () => {
    const said = (due: string) => {
      const w = dueWords(due, NOW);
      return w && [w.text, w.tone];
    };
    expect(said('2026-09-27')).toEqual(['3 days overdue', 'overdue']);
    expect(said('2026-09-29')).toEqual(['Due yesterday', 'overdue']);
    expect(said('2026-09-30')).toEqual(['Due today', 'today']);
    expect(said('2026-10-01')).toEqual(['Due tomorrow', 'soon']);
    expect(said('2026-10-06')).toEqual(['Due Tue', 'soon']);
    expect(said('2026-10-07')).toEqual(['Due Oct 7', 'later']);
    expect(said('2027-01-04')).toEqual(['Due Jan 4, 2027', 'later']);
    expect(dueWords('2026-10-02', NOW)?.long).toBe('Friday, Oct 2, in 2 days');
    expect(dueWords('soon', NOW)).toBeNull();
  });

  it('offer today, tomorrow, the coming Friday and Monday, each once', () => {
    const choices = (now: number) => dueChoices(now).map((c) => `${c.label} ${c.due.slice(5)}`);
    expect(choices(NOW)).toEqual(['Today 09-30', 'Tomorrow 10-01', 'Friday 10-02', 'Monday 10-05']);
    // On a Thursday, Friday is tomorrow.
    expect(choices(at(10, 1))).toEqual(['Today 10-01', 'Tomorrow 10-02', 'Monday 10-05']);
    // On a Friday, the coming Friday is a week off.
    expect(choices(at(10, 2))).toEqual(['Today 10-02', 'Tomorrow 10-03', 'Monday 10-05', 'Next Friday 10-09']);
    expect(choices(at(10, 4))).toEqual(['Today 10-04', 'Tomorrow 10-05', 'Friday 10-09']);
  });
});

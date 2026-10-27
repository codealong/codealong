import { describe, expect, it } from 'vitest';
import { reviewSnoozeMenu, snoozeChoices, snoozeMenu, untilWords } from './snooze';

// Local times, so the tests read the same in any time zone.
const at = (day: number, hour: number, minute = 0) => new Date(2026, 8, day, hour, minute).getTime();
const oct = (day: number, hour: number, minute = 0) => new Date(2026, 9, day, hour, minute).getTime();
const NOW = at(28, 14, 10); // Monday 2:10 PM

describe('snooze', () => {
  it('offers a few times from now, the next morning and the next week', () => {
    expect(snoozeChoices(NOW)).toEqual([
      { label: 'For 15 minutes', until: at(28, 14, 25) },
      { label: 'For 30 minutes', until: at(28, 14, 40) },
      { label: 'For 1 hour', until: at(28, 15, 10) },
      { label: 'For 3 hours', until: at(28, 17, 10) },
      { label: 'Until tomorrow 9 AM', until: at(29, 9) },
      { label: 'Until Oct 5, 9 AM', until: oct(5, 9) },
    ]);
  });

  it('takes the small hours as still the night before', () => {
    expect(snoozeChoices(at(29, 1, 30))[4]).toEqual({ label: 'Until 9 AM', until: at(29, 9) });
    expect(snoozeChoices(at(29, 5))[4]).toEqual({ label: 'Until tomorrow 9 AM', until: at(30, 9) });
  });

  it('snoozes for the next week until the Monday after the next morning', () => {
    const week = (now: number) => snoozeChoices(now)[5];
    // Friday afternoon: past the weekend.
    expect(week(oct(2, 16))).toEqual({ label: 'Until Mon 9 AM', until: oct(5, 9) });
    // Sunday evening, when the next morning is Monday's: the Monday after that.
    expect(week(oct(4, 20))).toEqual({ label: 'Until Oct 12, 9 AM', until: oct(12, 9) });
    // The small hours of a Monday, whose own morning is the next.
    expect(week(oct(5, 2))).toEqual({ label: 'Until Oct 12, 9 AM', until: oct(12, 9) });
    expect(week(oct(5, 10))).toEqual({ label: 'Until Oct 12, 9 AM', until: oct(12, 9) });
  });

  it('says when a snooze ends as a clock does', () => {
    expect(untilWords(at(28, 15, 40), NOW)).toBe('3:40 PM');
    expect(untilWords(at(29, 9), NOW)).toBe('tomorrow 9 AM');
    expect(untilWords(oct(2, 9, 5), at(30, 8))).toBe('Fri 9:05 AM');
    expect(untilWords(oct(10, 21), NOW)).toBe('Oct 10, 9 PM');
  });

  it('offers Wake now for a session that is snoozed already, and says until when', () => {
    const onDone = () => {};
    const fresh = snoozeMenu(['k'], NOW, { onDone });
    expect(fresh.label).toBe('Snooze');
    expect(fresh.items.map((i) => (i === 'separator' ? '-' : i.label))).toEqual(['For 15 minutes', 'For 30 minutes', 'For 1 hour', 'For 3 hours', 'Until tomorrow 9 AM', 'Until Oct 5, 9 AM']);
    const snoozed = snoozeMenu(['k'], NOW, { until: at(28, 15, 40), onDone });
    expect(snoozed.label).toBe('Snoozed until 3:40 PM');
    expect(snoozed.items.slice(-2).map((i) => (i === 'separator' ? '-' : i.label))).toEqual(['-', 'Wake now']);
    expect(snoozeMenu(['a', 'b'], NOW, { label: 'Snooze these', onDone }).label).toBe('Snooze these');
  });

  it('names a review’s snooze as the review’s, with the same times', () => {
    const onDone = () => {};
    const pr = { key: 'o/r#12', reviewRequested: true, state: 'open' as const, snoozedUntil: null };
    const fresh = reviewSnoozeMenu(pr, NOW, onDone);
    expect(fresh?.label).toBe('Snooze review');
    expect(fresh?.items).toHaveLength(6);
    const snoozed = reviewSnoozeMenu({ ...pr, snoozedUntil: at(29, 9) }, NOW, onDone);
    expect(snoozed?.label).toBe('Review snoozed until tomorrow 9 AM');
    expect(snoozed?.items.slice(-1).map((i) => (i === 'separator' ? '-' : i.label))).toEqual(['Wake now']);
    // Only a review that waits on you snoozes.
    expect(reviewSnoozeMenu({ ...pr, reviewRequested: false }, NOW, onDone)).toBeUndefined();
    expect(reviewSnoozeMenu({ ...pr, state: 'merged' }, NOW, onDone)).toBeUndefined();
  });
});

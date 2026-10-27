import { describe, expect, it } from 'vitest';
import { askWords, reviewSnoozeMenu, snoozeChoices, snoozeChorded, snoozeMenu, untilWords } from './snooze';
import type { MenuAction, MenuGroup } from './contextMenu';

// Local times, so the tests read the same in any time zone.
const at = (day: number, hour: number, minute = 0) => new Date(2026, 8, day, hour, minute).getTime();
const oct = (day: number, hour: number, minute = 0) => new Date(2026, 9, day, hour, minute).getTime();
const NOW = at(28, 14, 10); // Monday 2:10 PM

const labels = (menu: MenuGroup | undefined) => (menu?.items ?? []).map((i) => (i === 'separator' ? '-' : i.label));
const named = (choices: ReturnType<typeof snoozeChoices>, label: RegExp) => choices.find((c) => label.test(c.label));

describe('snooze', () => {
  it('offers a few times from now, 5 PM, the next morning and the next week, each with its chord', () => {
    expect(snoozeChoices(NOW)).toEqual([
      { label: 'For 15 minutes', until: at(28, 14, 25), chord: 'Ctrl+Cmd+1' },
      { label: 'For 30 minutes', until: at(28, 14, 40), chord: 'Ctrl+Cmd+2' },
      { label: 'For 1 hour', until: at(28, 15, 10), chord: 'Ctrl+Cmd+3' },
      { label: 'For 3 hours', until: at(28, 17, 10), chord: 'Ctrl+Cmd+4' },
      { label: 'Until 5 PM', until: at(28, 17), chord: 'Ctrl+Cmd+5' },
      { label: 'Until tomorrow 9 AM', until: at(29, 9), chord: 'Ctrl+Cmd+6' },
      { label: 'Until Oct 5, 9 AM', until: oct(5, 9), chord: 'Ctrl+Cmd+7' },
    ]);
  });

  it('offers 5 PM from the morning until it is less than an hour off, and keeps the others’ chords', () => {
    const evening = (now: number) => named(snoozeChoices(now), /^Until 5 PM$/);
    expect(evening(at(28, 5))).toEqual({ label: 'Until 5 PM', until: at(28, 17), chord: 'Ctrl+Cmd+5' });
    expect(evening(at(28, 16))?.until).toBe(at(28, 17));
    expect(evening(at(28, 16, 1))).toBeUndefined();
    expect(evening(at(28, 20))).toBeUndefined();
    // In the small hours it would come after the next morning, which is that same day's.
    expect(evening(at(29, 1, 30))).toBeUndefined();
    const late = snoozeChoices(at(28, 16, 30));
    expect(late.map((c) => c.chord)).toEqual(['Ctrl+Cmd+1', 'Ctrl+Cmd+2', 'Ctrl+Cmd+3', 'Ctrl+Cmd+4', 'Ctrl+Cmd+6', 'Ctrl+Cmd+7']);
  });

  it('takes the small hours as still the night before', () => {
    expect(named(snoozeChoices(at(29, 1, 30)), /9 AM$/)).toEqual({ label: 'Until 9 AM', until: at(29, 9), chord: 'Ctrl+Cmd+6' });
    expect(named(snoozeChoices(at(29, 5)), /9 AM$/)).toEqual({ label: 'Until tomorrow 9 AM', until: at(30, 9), chord: 'Ctrl+Cmd+6' });
  });

  it('snoozes for the next week until the Monday after the next morning', () => {
    const week = (now: number) => snoozeChoices(now).find((c) => c.chord === 'Ctrl+Cmd+7');
    // Friday afternoon: past the weekend.
    expect(week(oct(2, 16))).toEqual({ label: 'Until Mon 9 AM', until: oct(5, 9), chord: 'Ctrl+Cmd+7' });
    // Sunday evening, when the next morning is Monday's: the Monday after that.
    expect(week(oct(4, 20))?.until).toBe(oct(12, 9));
    // The small hours of a Monday, whose own morning is the next.
    expect(week(oct(5, 2))?.until).toBe(oct(12, 9));
    expect(week(oct(5, 10))).toEqual({ label: 'Until Oct 12, 9 AM', until: oct(12, 9), chord: 'Ctrl+Cmd+7' });
  });

  it('says when a snooze ends as a clock does', () => {
    expect(untilWords(at(28, 15, 40), NOW)).toBe('3:40 PM');
    expect(untilWords(at(29, 9), NOW)).toBe('tomorrow 9 AM');
    expect(untilWords(oct(2, 9, 5), at(30, 8))).toBe('Fri 9:05 AM');
    expect(untilWords(oct(10, 21), NOW)).toBe('Oct 10, 9 PM');
  });

  it('offers Pick a time…, and Wake now for a session that is snoozed already, and says until when', () => {
    const onDone = () => {};
    const fresh = snoozeMenu(['k'], NOW, { onDone });
    expect(fresh.label).toBe('Snooze');
    expect(labels(fresh)).toEqual(['For 15 minutes', 'For 30 minutes', 'For 1 hour', 'For 3 hours', 'Until 5 PM', 'Until tomorrow 9 AM', 'Until Oct 5, 9 AM', 'Pick a time…']);
    const snoozed = snoozeMenu(['k'], NOW, { until: at(28, 15, 40), onDone });
    expect(snoozed.label).toBe('Snoozed until 3:40 PM');
    expect(labels(snoozed).slice(-3)).toEqual(['Pick a time…', '-', 'Wake now']);
    expect(snoozeMenu(['a', 'b'], NOW, { label: 'Snooze these', onDone }).label).toBe('Snooze these');
  });

  it('finds a time by its chord, which shows only where the window takes chords', () => {
    const onDone = () => {};
    const plain = snoozeMenu(['k'], at(28, 16, 30), { onDone });
    expect(plain.items.some((i) => i !== 'separator' && 'accelerator' in i && i.accelerator)).toBe(false);
    expect(snoozeChorded(plain, '6')?.label).toBe('Until tomorrow 9 AM');
    // 5 PM's chord picks nothing once it's no longer offered, rather than the next time down.
    expect(snoozeChorded(plain, '5')).toBeNull();
    expect(snoozeChorded(plain, '8')).toBeNull();
    const chorded = snoozeMenu(['k'], NOW, { onDone, chords: true });
    expect((chorded.items[4] as MenuAction).accelerator).toBe('Ctrl+Cmd+5');
    expect(snoozeChorded(chorded, '5')).toBe(chorded.items[4]);
  });

  it('names a review’s snooze as the review’s, with the same times', () => {
    const onDone = () => {};
    const pr = { key: 'o/r#12', reviewRequested: true, state: 'open' as const, snoozedUntil: null };
    const fresh = reviewSnoozeMenu(pr, NOW, onDone);
    expect(fresh?.label).toBe('Snooze review');
    expect(labels(fresh)).toHaveLength(8);
    const snoozed = reviewSnoozeMenu({ ...pr, snoozedUntil: at(29, 9) }, NOW, onDone);
    expect(snoozed?.label).toBe('Review snoozed until tomorrow 9 AM');
    expect(labels(snoozed).slice(-1)).toEqual(['Wake now']);
    // Only a review that waits on you snoozes.
    expect(reviewSnoozeMenu({ ...pr, reviewRequested: false }, NOW, onDone)).toBeUndefined();
    expect(reviewSnoozeMenu({ ...pr, state: 'merged' }, NOW, onDone)).toBeUndefined();
  });

  it('names what Pick a time… snoozes', () => {
    const titles: Record<string, string> = { a: 'Fix the flaky test' };
    const titleOf = (key: string) => titles[key];
    expect(askWords({ keys: ['a'], reviews: false }, titleOf)).toBe('“Fix the flaky test”');
    expect(askWords({ keys: ['gone'], reviews: false }, titleOf)).toBe('1 session');
    expect(askWords({ keys: ['a', 'b'], reviews: false }, titleOf)).toBe('2 sessions');
    expect(askWords({ keys: ['northwind/app#12'], reviews: true }, titleOf)).toBe('your review of #12');
    expect(askWords({ keys: ['northwind/app#12', 'northwind/app#13'], reviews: true }, titleOf)).toBe('2 reviews');
  });
});

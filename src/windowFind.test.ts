import { describe, expect, it } from 'vitest';
import { countIn, countWords, findPattern, says, spansIn, startOf, stepTo } from './windowFind';

describe('findPattern', () => {
  it('finds the words in any case, a run of spaces matching any whitespace', () => {
    const p = findPattern('  Session   thread ')!;
    expect(countIn(p, 'the session thread, the SESSION\nTHREAD and the session  thread')).toBe(3);
    expect(says(p, 'session-thread')).toBe(false);
  });

  it('takes what it finds as written, not as a pattern', () => {
    const p = findPattern('a.b (c)* [d] $e^ {f} | \\g')!;
    expect(says(p, 'x a.b (c)* [d] $e^ {f} | \\g y')).toBe(true);
    expect(says(findPattern('a.b')!, 'axb')).toBe(false);
  });

  it('is nothing for nothing typed', () => {
    expect(findPattern('')).toBeNull();
    expect(findPattern('  \n ')).toBeNull();
  });

  it('can be used again and again', () => {
    const p = findPattern('one')!;
    expect(countIn(p, 'one one')).toBe(2);
    expect(says(p, 'one')).toBe(true);
    expect(says(p, 'one')).toBe(true);
    expect(countIn(p, 'one')).toBe(1);
  });
});

describe('spansIn', () => {
  // "the light", "house" in bold, then " keeper and the lighthouse".
  const pieces = ['the light', 'house', ' keeper and the lighthouse'];
  const starts = [0, 9, 14];
  const text = pieces.join('');

  it('places each match in the pieces it starts and ends in', () => {
    expect(spansIn(text, starts, findPattern('lighthouse')!)).toEqual([
      { first: 0, from: 4, last: 1, to: 5 },
      { first: 2, from: 16, last: 2, to: 26 },
    ]);
  });

  it('finds within one piece, and at a piece’s very start', () => {
    expect(spansIn(text, starts, findPattern('house')!)).toEqual([
      { first: 1, from: 0, last: 1, to: 5 },
      { first: 2, from: 21, last: 2, to: 26 },
    ]);
  });

  it('stops at the most it’s asked for', () => {
    expect(spansIn(text, starts, findPattern('e')!, 2)).toHaveLength(2);
    expect(spansIn(text, starts, findPattern('e')!, 0)).toEqual([]);
  });

  it('finds nothing in no pieces', () => {
    expect(spansIn('', [], findPattern('a')!)).toEqual([]);
  });
});

describe('startOf', () => {
  it('starts on the first match in view', () => {
    expect(startOf([-1, -1, 0, 0, 1])).toBe(2);
  });

  it('else on the first below the view, else the last above it', () => {
    expect(startOf([-1, -1, 1, 1])).toBe(2);
    expect(startOf([-1, -1, -1])).toBe(2);
  });

  it('is none for no matches', () => {
    expect(startOf([])).toBe(-1);
  });
});

describe('stepTo', () => {
  it('goes on and back, round from the last to the first and back', () => {
    expect(stepTo(1, 3, 1)).toBe(2);
    expect(stepTo(2, 3, 1)).toBe(0);
    expect(stepTo(1, 3, -1)).toBe(0);
    expect(stepTo(0, 3, -1)).toBe(2);
  });

  it('starts at the first going on, the last going back', () => {
    expect(stepTo(null, 4, 1)).toBe(0);
    expect(stepTo(null, 4, -1)).toBe(3);
  });

  it('goes nowhere among none', () => {
    expect(stepTo(null, 0, 1)).toBe(-1);
    expect(stepTo(0, 0, -1)).toBe(-1);
  });
});

describe('countWords', () => {
  const none = { at: null, total: 0, partial: false, reading: false, looking: false };
  it('counts from the top, so far while more is to read', () => {
    expect(countWords({ ...none, at: 2, total: 12 })).toBe('3 of 12');
    expect(countWords({ ...none, at: 2, total: 12, partial: true, reading: true })).toBe('3 of 12 so far');
    expect(countWords({ ...none, total: 5, partial: true, reading: true })).toBe('5 so far');
  });

  it('says when there are none, or it’s still looking', () => {
    expect(countWords(none)).toBe('No matches');
    expect(countWords({ ...none, partial: true, reading: true })).toBe('Looking…');
    expect(countWords({ ...none, partial: true })).toBe('No matches so far');
    expect(countWords({ ...none, at: 0, total: 4, looking: true })).toBe('Looking earlier…');
  });

  it('says there are more than it marks', () => {
    expect(countWords({ ...none, at: 0, total: 1000, capped: true })).toBe('1 of 1000+');
    expect(countWords({ ...none, total: 1004, capped: true, partial: true, reading: true })).toBe('1004+ so far');
  });
});

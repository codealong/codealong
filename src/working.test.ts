import { describe, expect, it } from 'vitest';
import { LONG_MS, LONG_WORDS, shuffled, TIP_AFTER_MS, TIP_EVERY_MS, turnOf, WORD_FOR_MS, WORKING_WORDS, workingTip, workingWord, type TipClaim } from './working';

const T0 = 1_760_000_000_000;

describe('the words', () => {
  it('are sentence case with no stop of their own, each once', () => {
    const all = [...WORKING_WORDS, ...LONG_WORDS];
    expect(new Set(all).size).toBe(all.length);
    for (const w of all) {
      expect(w, w).toMatch(/^[A-Z][a-z]/);
      expect(w, w).not.toMatch(/[.…!?]$/);
      expect(w.length, w).toBeLessThanOrEqual(24);
    }
  });
});

describe('shuffled', () => {
  it('keeps every word, in an order of its seed', () => {
    const a = shuffled(WORKING_WORDS, 'one');
    expect([...a].sort()).toEqual([...WORKING_WORDS].sort());
    expect(shuffled(WORKING_WORDS, 'one')).toEqual(a);
    expect(shuffled(WORKING_WORDS, 'two')).not.toEqual(a);
  });
});

describe('workingWord', () => {
  const since = T0 - 5_000;
  it('is the same at the same moment, for every window', () => {
    expect(workingWord('s1', since, T0)).toBe(workingWord('s1', since, T0));
  });
  it('holds for a while, then changes, never to the same word', () => {
    const start = Math.ceil(T0 / WORD_FOR_MS) * WORD_FOR_MS;
    expect(workingWord('s1', since, start + 1)).toBe(workingWord('s1', since, start + WORD_FOR_MS - 1));
    for (let i = 0; i < WORKING_WORDS.length * 2; i++) {
      const at = start + i * WORD_FOR_MS;
      expect(workingWord('s1', since, at + WORD_FOR_MS), String(i)).not.toBe(workingWord('s1', since, at));
    }
  });
  it('differs between sessions working side by side', () => {
    const words = new Set(['a', 'b', 'c', 'd', 'e'].map((k) => workingWord(k, since, T0)));
    expect(words.size).toBeGreaterThan(1);
  });
  it('says now and then that a long turn is still going', () => {
    const at = (i: number) => Math.ceil(T0 / WORD_FOR_MS) * WORD_FOR_MS + i * WORD_FOR_MS;
    const early = Array.from({ length: 30 }, (_, i) => workingWord('s1', at(0) - 60_000, at(i)));
    expect(early.some((w) => LONG_WORDS.includes(w))).toBe(false);
    const late = Array.from({ length: 30 }, (_, i) => workingWord('s1', at(0) - LONG_MS, at(i)));
    const long = late.filter((w) => LONG_WORDS.includes(w)).length;
    expect(long).toBe(10);
    for (let i = 1; i < late.length; i++) expect(late[i], String(i)).not.toBe(late[i - 1]);
  });
});

describe('workingTip', () => {
  const turn = turnOf('s1', T0 - 60_000);
  const claim = (over: Partial<TipClaim>): TipClaim => ({ turn: turnOf('s0', T0 - 3_600_000), id: 'find', at: T0 - TIP_EVERY_MS, ...over });

  it('gives none in the first moments of a turn', () => {
    expect(workingTip(null, turn, TIP_AFTER_MS - 1, T0)).toBe(null);
    expect(workingTip(null, turn, TIP_AFTER_MS, T0)).toBe('take');
  });
  it('gives none for a while after the last, wherever it showed', () => {
    expect(workingTip(claim({ at: T0 - TIP_EVERY_MS + 1 }), turn, 60_000, T0)).toBe(null);
    expect(workingTip(claim({ at: T0 - TIP_EVERY_MS }), turn, 60_000, T0)).toBe('take');
  });
  it('keeps the tip a turn got for as long as it goes on', () => {
    expect(workingTip(claim({ turn, at: T0 - 1_000 }), turn, 0, T0)).toBe('keep');
    expect(workingTip(claim({ turn, at: T0 - 2 * TIP_EVERY_MS }), turn, 10 * TIP_EVERY_MS, T0)).toBe('keep');
  });
  it('takes one when the last says it showed later than now (the clock went back)', () => {
    expect(workingTip(claim({ at: T0 + 60_000 }), turn, 60_000, T0)).toBe('take');
  });
});

import { describe, expect, it } from 'vitest';
import { keysLine } from './perf';

describe('keysLine', () => {
  it('says how long keys took to show and how much of it came before the page had them', () => {
    const line = keysLine([
      { reached: 3, drawn: 20 },
      { reached: 40, drawn: 61.6 },
      { reached: 5, drawn: 25 },
    ]);
    expect(line).toBe('keys: 3 in 1 s, drawn 25 ms after the key (median), 62 ms at most; in the page 5 ms after it (median), 40 ms at most');
  });

  it('keeps what scripts/perf-summary.mjs reads from it', () => {
    const line = keysLine([{ reached: 1, drawn: 9 }]);
    // The worst, as perf-summary.mjs has always read it: the first "ms at most".
    expect(/, (\d+) ms at most/.exec(line)?.[1]).toBe('9');
    expect(/drawn (\d+) ms/.exec(line)?.[1]).toBe('9');
    expect(/ in the page (\d+) ms/.exec(line)?.[1]).toBe('1');
  });
});

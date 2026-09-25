import { describe, expect, it } from 'vitest';
import { barWidth, fitGroups, plain, type Metrics } from './bar';

// Which sessions and PRs the bar shows, and their order, are the engine's (barfeed.rs, with tests).

const NOW = 1_790_200_000_000;

describe('fitGroups', () => {
  const metrics = (needs: number[], unread: number[], working: number[], reviews = 0): Metrics => ({
    pills: { needs, unread, working },
    fold: { needs: 30, unread: 30, working: 30 },
    mark: 24,
    reviews,
    gap: 4,
    divider: 1,
    padding: 5,
  });

  it('measures the bar as its styles lay it out', () => {
    // padding 10 + mark 24 + divider 1 + pills 100 + 50, with 3 gaps between 4 children.
    expect(barWidth(fitGroups(metrics([100, 50], [], []), 10_000), metrics([100, 50], [], []))).toBe(10 + 24 + 1 + 150 + 3 * 4);
    expect(barWidth([], metrics([], [], []))).toBe(10 + 24);
  });

  it('folds nothing when there is room', () => {
    expect(fitGroups(metrics([100], [100], [100]), 10_000)).toEqual([
      { group: 'needs', shown: 1, folded: 0 },
      { group: 'unread', shown: 1, folded: 0 },
      { group: 'working', shown: 1, folded: 0 },
    ]);
  });

  it('folds Working from its last pill back, before touching Your turn', () => {
    const m = metrics([100], [100, 100], [100, 100, 100]);
    const full = barWidth(fitGroups(m, 10_000), m);
    const fits = fitGroups(m, full - 60);
    expect(fits.find((f) => f.group === 'working')).toEqual({ group: 'working', shown: 2, folded: 1 });
    expect(fits.find((f) => f.group === 'unread')).toEqual({ group: 'unread', shown: 2, folded: 0 });
    expect(barWidth(fits, m)).toBeLessThanOrEqual(full - 60);
  });

  it('then folds Your turn, then Needs you, down to one pill each', () => {
    const m = metrics([100, 100], [100, 100], [100, 100], 60);
    const fits = fitGroups(m, 400);
    expect(fits.map((f) => [f.shown, f.folded])).toEqual([
      [2, 0],
      [0, 2],
      [0, 2],
    ]);
    const tight = fitGroups(m, 0);
    expect(tight.map((f) => [f.shown, f.folded])).toEqual([
      [0, 2],
      [0, 2],
      [0, 2],
    ]);
  });

  it('keeps empty groups off the bar', () => {
    expect(barWidth(fitGroups(metrics([], [80], []), 10_000), metrics([], [80], []))).toBe(10 + 24 + 1 + 80 + 2 * 4);
  });
});

describe('plain', () => {
  it('drops code marks and fills in times', () => {
    expect(plain('Stuck on `gcloud auth login` for {for:' + (NOW - 3 * 3_600_000) + '}', NOW)).toBe('Stuck on gcloud auth login for 3h');
  });
});

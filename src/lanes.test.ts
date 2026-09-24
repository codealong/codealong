import { describe, expect, it } from 'vitest';
import { ageFiltered, bayOf, buildBays, filterByAge, laneInFilter } from './lanes';
import type { Lane, Pr, Session } from './types';

const NOW = 1_790_200_000_000;
const DAY = 86_400_000;

const session = (key: string, status: Session['status'], lastActivity: number): Session => ({ key, status, lastActivity, visitor: null }) as Session;
const pr = (state: Pr['state'], failed = 0): Pr => ({ number: 7, title: 'Fix it', state, checks: { passed: 3, failed, pending: 0, skipped: 0, failing: [] } }) as unknown as Pr;
const lane = (sessions: Session[], p: Pr | null): Lane => {
  const [bay, glyph] = bayOf(sessions, p);
  return { id: '/r', sessions, pr: p, bay, glyph, headline: 'from the engine', detail: 'x' } as unknown as Lane;
};

describe('bayOf', () => {
  it('follows the engine: agents first, then the PR, then work', () => {
    expect(bayOf([session('a', 'needs', NOW)], pr('open'))).toEqual(['needs', 'needs']);
    expect(bayOf([session('a', 'idle', NOW)], pr('open', 2))).toEqual(['needs', 'error']);
    expect(bayOf([session('a', 'working', NOW)], pr('open', 2))).toEqual(['working', 'working']);
    expect(bayOf([], pr('merged'))).toEqual(['merged', 'idle']);
    expect(bayOf([], null)).toEqual(['idle', 'idle']);
  });
});

describe('ageFiltered', () => {
  const stuck = session('stuck', 'needs', NOW - 3 * DAY);
  const mine = session('mine', 'working', NOW - 60_000);

  it("drops old sessions and takes the lane's status from the rest", () => {
    const l = ageFiltered(lane([stuck, mine], null), NOW - DAY, 'in the last 24 hours');
    expect(l.sessions.map((s) => s.key)).toEqual(['mine']);
    expect([l.bay, l.glyph]).toEqual(['working', 'working']);
  });

  it('writes a plain headline when nothing active is left', () => {
    const l = ageFiltered(lane([stuck], pr('open')), NOW - DAY, 'in the last 24 hours');
    expect(l.bay).toBe('review');
    expect(l.headline).toBe('#7 Fix it');
    expect(ageFiltered(lane([stuck], null), NOW - DAY, 'in the last 24 hours').headline).toBe('No agents active in the last 24 hours');
  });

  it('takes the headline from the agent that sets the new status', () => {
    const done = { ...session('done', 'unread', NOW - 3_600_000), headline: '`claude` finished · Fix it', detail: 'Pushed the fix' } as Session;
    const l = ageFiltered(lane([stuck, done], pr('merged')), NOW - DAY, 'in the last 24 hours');
    expect([l.bay, l.headline, l.detail]).toEqual(['unread', '`claude` finished · Fix it', 'Pushed the fix']);
  });

  it('leaves a lane alone when nothing in it is filtered out', () => {
    const l = lane([stuck, mine], null);
    expect(ageFiltered(l, NOW - 4 * DAY, '')).toBe(l);
    expect(ageFiltered(l, 0, '')).toBe(l);
  });
});

describe('filterByAge', () => {
  const quiet = { ...lane([session('old', 'unread', NOW - 5 * DAY)], null), id: '/r/quiet', lastActivity: NOW - 5 * DAY } as Lane;
  const busy = { ...lane([session('mine', 'working', NOW - 60_000)], null), id: '/r/busy', lastActivity: NOW - 60_000 } as Lane;

  it('keeps pinned worktrees however quiet, with only their recent sessions', () => {
    const shown = filterByAge([quiet, busy], NOW - DAY, new Set(['/r/quiet']), 'in the last 24 hours');
    expect(shown.map((l) => l.id)).toEqual(['/r/quiet', '/r/busy']);
    expect(shown[0].sessions).toEqual([]);
    expect([shown[0].bay, shown[0].headline]).toEqual(['idle', 'No agents active in the last 24 hours']);
  });

  it('drops quiet worktrees that are not pinned, and filters nothing without a cutoff', () => {
    expect(filterByAge([quiet, busy], NOW - DAY, new Set(), '').map((l) => l.id)).toEqual(['/r/busy']);
    const all = [quiet, busy];
    expect(filterByAge(all, 0, new Set(), '')).toBe(all);
  });
});

describe('buildBays', () => {
  it('gives agents that need you, work or just finished rows of their own, and other worktrees one row', () => {
    const busy = { ...lane([session('a', 'needs', NOW), session('b', 'working', NOW - 1000), session('c', 'idle', NOW)], null), id: '/r/busy', lastActivity: NOW } as Lane;
    const quiet = { ...lane([session('d', 'idle', NOW)], pr('open')), id: '/r/quiet', lastActivity: NOW } as Lane;
    expect(buildBays([busy, quiet]).map((g) => [g.bay, g.rows.map((r) => r.key)])).toEqual([
      ['needs', ['/r/busy::a']],
      ['working', ['/r/busy::b']],
      ['review', ['/r/quiet']],
    ]);
  });
});

describe('laneInFilter', () => {
  it('matches a worktree when any of its rows on the board would', () => {
    const busy = { ...lane([session('a', 'needs', NOW), session('b', 'working', NOW)], pr('open', 1)), id: '/r/busy' } as Lane;
    expect(laneInFilter(busy, null)).toBe(true);
    expect(laneInFilter(busy, 'needs')).toBe(true);
    expect(laneInFilter(busy, 'working')).toBe(true);
    expect(laneInFilter(busy, 'failing')).toBe(true);
    expect(laneInFilter(busy, 'unread')).toBe(false);
    expect(laneInFilter(busy, 'review')).toBe(false);
    const inReview = { ...lane([session('c', 'idle', NOW)], pr('open')), id: '/r/review' } as Lane;
    expect(laneInFilter(inReview, 'review')).toBe(true);
    expect(laneInFilter(inReview, 'failing')).toBe(false);
  });
});

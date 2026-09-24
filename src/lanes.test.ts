import { describe, expect, it } from 'vitest';
import { ageFiltered, bayOf } from './lanes';
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
    const l = ageFiltered(lane([stuck, mine], null), NOW - DAY, new Set(), 'in the last 24 hours');
    expect(l.sessions.map((s) => s.key)).toEqual(['mine']);
    expect([l.bay, l.glyph]).toEqual(['working', 'working']);
  });

  it('writes a plain headline when nothing active is left', () => {
    const l = ageFiltered(lane([stuck], pr('open')), NOW - DAY, new Set(), 'in the last 24 hours');
    expect(l.bay).toBe('review');
    expect(l.headline).toBe('#7 Fix it');
    expect(ageFiltered(lane([stuck], null), NOW - DAY, new Set(), 'in the last 24 hours').headline).toBe('No agents active in the last 24 hours');
  });

  it('keeps pinned sessions and leaves unfiltered lanes alone', () => {
    const l = lane([stuck, mine], null);
    expect(ageFiltered(l, NOW - DAY, new Set(['stuck']), '')).toBe(l);
    expect(ageFiltered(l, 0, new Set(), '')).toBe(l);
  });
});

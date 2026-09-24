import { describe, expect, it } from 'vitest';
import { ageFiltered, bayOf, buildBays, bySession, filterByAge, inboxCounts, narrowed, rowItems, withoutAccount } from './lanes';
import type { Lane, Pr, Session } from './types';

const NOW = 1_790_200_000_000;
const DAY = 86_400_000;

const session = (key: string, status: Session['status'], lastActivity: number, lastPromptAt = lastActivity): Session =>
  ({ key, status, lastActivity, lastPromptAt, visitor: null, archived: false }) as Session;
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
  it('shows each worktree once, in its bay, with a line for each agent that needs you, works or just finished', () => {
    const busy = { ...lane([session('a', 'needs', NOW), session('b', 'working', NOW - 1000), session('c', 'unread', NOW), session('d', 'idle', NOW)], null), id: '/r/busy', lastActivity: NOW } as Lane;
    const quiet = { ...lane([session('e', 'idle', NOW)], pr('open')), id: '/r/quiet', lastActivity: NOW } as Lane;
    const bays = buildBays([busy, quiet]);
    expect(bays.map((g) => [g.bay, g.rows.map((r) => r.key)])).toEqual([
      ['needs', ['/r/busy']],
      ['review', ['/r/quiet']],
    ]);
    expect(bays[0].rows[0].agents.map((a) => a.key)).toEqual(['/r/busy::a', '/r/busy::b', '/r/busy::c']);
    // What you can select: a worktree's agents, or the worktree when it has none.
    expect(bays.flatMap((g) => g.rows.flatMap(rowItems)).map((r) => r.key)).toEqual(['/r/busy::a', '/r/busy::b', '/r/busy::c', '/r/quiet']);
  });

  it('orders by status, then by when you last sent a prompt, not by activity', () => {
    // `old` is busier right now, but you asked `recent` more recently.
    const old = session('old', 'working', NOW, NOW - 3_600_000);
    const recent = session('recent', 'working', NOW - 300_000, NOW - 600_000);
    const done = session('done', 'unread', NOW - 60_000, NOW - 60_000);
    const one = { ...lane([old, done, recent], null), id: '/r/one', lastActivity: NOW } as Lane;
    expect(buildBays([one])[0].rows[0].agents.map((a) => a.session.key)).toEqual(['recent', 'old', 'done']);
    const a = { ...lane([session('x', 'working', NOW, NOW - 3_600_000)], null), id: '/r/a', lastActivity: NOW } as Lane;
    const b = { ...lane([session('y', 'working', NOW - 120_000, NOW - 1_800_000)], null), id: '/r/b', lastActivity: NOW - 120_000 } as Lane;
    expect(buildBays([a, b])[0].rows.map((r) => r.key)).toEqual(['/r/b', '/r/a']);
    expect([old, recent, done].sort(bySession).map((s) => s.key)).toEqual(['recent', 'old', 'done']);
  });
});

describe('narrowed', () => {
  const mixed = { ...lane([session('w', 'working', NOW), session('u', 'unread', NOW), session('i', 'idle', NOW)], pr('open', 1)), id: '/r/mixed' } as Lane;

  it('narrows a worktree to the agents a filter is about, in that bay', () => {
    expect(mixed.bay).toBe('working');
    const yours = narrowed(mixed, 'unread', '')!;
    expect([yours.bay, yours.sessions.map((s) => s.key)]).toEqual(['unread', ['u']]);
    expect(narrowed(mixed, 'working', '')!.sessions.map((s) => s.key)).toEqual(['w']);
    expect(narrowed(mixed, null, '')).toBe(mixed);
  });

  it('keeps a worktree whole for its PR, and one that needs you for its PR alone', () => {
    expect(narrowed(mixed, 'failing', '')).toBe(mixed);
    expect(narrowed(mixed, 'review', '')).toBeNull();
    expect(narrowed(mixed, 'needs', '')).toBeNull();
    const failing = { ...lane([session('u', 'unread', NOW)], pr('open', 2)), id: '/r/failing' } as Lane;
    expect(failing.bay).toBe('needs');
    const shown = narrowed(failing, 'needs', '')!;
    expect([shown.bay, shown.sessions]).toEqual(['needs', []]);
    const inReview = { ...lane([session('c', 'idle', NOW)], pr('open')), id: '/r/review' } as Lane;
    expect(narrowed(inReview, 'review', '')).toBe(inReview);
    expect(narrowed(inReview, 'failing', '')).toBeNull();
  });
});

describe('inboxCounts', () => {
  it('counts agents, and worktrees for what their PR needs', () => {
    const busy = { ...lane([session('a', 'needs', NOW), session('b', 'working', NOW), session('c', 'working', NOW), session('d', 'unread', NOW)], null), id: '/r/busy' } as Lane;
    const failing = { ...lane([session('e', 'unread', NOW)], pr('open', 2)), id: '/r/failing' } as Lane;
    const inReview = { ...lane([], pr('open')), id: '/r/review' } as Lane;
    expect(inboxCounts([busy, failing, inReview])).toEqual({ needs: 2, working: 2, unread: 2, review: 1, failing: 1 });
  });
});

describe('withoutAccount', () => {
  it('drops the account the chip already shows', () => {
    expect(withoutAccount('`claude` · Fix the test')).toBe('Fix the test');
    expect(withoutAccount('`claude2` finished · Fix the test')).toBe('finished · Fix the test');
    expect(withoutAccount('`codex` asks for approval')).toBe('asks for approval');
    expect(withoutAccount('Checks failing on #7')).toBe('Checks failing on #7');
  });
});

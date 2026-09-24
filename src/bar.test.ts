import { describe, expect, it } from 'vitest';
import { barItems, barWidth, fitGroups, groupOf, plain, reviewRequests, type Metrics } from './bar';
import type { Board, Lane, PrItem, Session } from './types';

const NOW = 1_790_200_000_000;

const session = (key: string, status: Session['status'], statusSince: number, extra: Partial<Session> = {}): Session =>
  ({ key, status, statusSince, lastActivity: statusSince, visitor: null, archived: false, ...extra }) as Session;
const lane = (id: string, sessions: Session[], hidden = false): Lane => ({ id, sessions, hidden }) as unknown as Lane;
const board = (lanes: Lane[], prs: Partial<PrItem>[] = []): Board => ({ lanes, prs }) as unknown as Board;

describe('barItems', () => {
  it('lists what needs you, then finished turns, then running agents, newest first in each', () => {
    const b = board([
      lane('/a', [session('w1', 'working', NOW - 50), session('i1', 'idle', NOW), session('u1', 'unread', NOW - 10)]),
      lane('/b', [session('n1', 'needs', NOW - 90), session('e1', 'error', NOW - 5), session('w2', 'working', NOW - 20), session('s1', 'stopped', NOW)]),
    ]);
    expect(barItems(b).map((i) => [i.key, i.group])).toEqual([
      ['e1', 'needs'],
      ['n1', 'needs'],
      ['u1', 'unread'],
      ['w2', 'working'],
      ['w1', 'working'],
    ]);
  });

  it("leaves out visitors, archived sessions and lanes the board keeps out of sight", () => {
    const b = board([
      lane('/a', [session('v', 'working', NOW, { visitor: 'worked here' }), session('x', 'unread', NOW, { archived: true })]),
      lane('/elsewhere', [session('h', 'working', NOW)], true),
      lane('/b', [session('ok', 'working', NOW)]),
    ]);
    expect(barItems(b).map((i) => i.key)).toEqual(['ok']);
  });

  it('keeps pills in place while a status lasts, whatever the latest activity', () => {
    const a = session('a', 'working', NOW - 100, { lastActivity: NOW });
    const c = session('c', 'working', NOW - 50, { lastActivity: NOW - 40 });
    expect(barItems(board([lane('/a', [a, c])])).map((i) => i.key)).toEqual(['c', 'a']);
  });
});

describe('reviewRequests', () => {
  it('keeps open PRs that ask for your review, newest first', () => {
    const b = board(
      [],
      [
        { key: 'o/r#1', reviewRequested: true, state: 'open', updatedAt: 1 },
        { key: 'o/r#2', reviewRequested: true, state: 'merged', updatedAt: 3 },
        { key: 'o/r#3', reviewRequested: false, state: 'open', updatedAt: 4 },
        { key: 'o/r#4', reviewRequested: true, state: 'draft', updatedAt: 2 },
      ],
    );
    expect(reviewRequests(b).map((p) => p.key)).toEqual(['o/r#4', 'o/r#1']);
  });
});

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

describe('groupOf and plain', () => {
  it('puts errors with what needs you and leaves idle sessions off', () => {
    expect(['needs', 'error', 'unread', 'working', 'idle', 'stopped'].map((s) => groupOf(s as Session['status']))).toEqual(['needs', 'needs', 'unread', 'working', null, null]);
  });

  it('drops code marks and fills in times', () => {
    expect(plain('Stuck on `gcloud auth login` for {for:' + (NOW - 3 * 3_600_000) + '}', NOW)).toBe('Stuck on gcloud auth login for 3h');
  });
});

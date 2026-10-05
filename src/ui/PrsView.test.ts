import { describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import type { GitInfo, PrItem } from '../types';
import { prPlaces, prSections, syncWords, type PrRowItem } from './PrsView';

const lane = (id: string) => fx.lane({ id });
const session = (key: string) => fx.session({ key });

describe('prPlaces', () => {
  it('lists the worktrees that hold the PR first, each with its sessions', () => {
    const [held, other] = [lane('held'), lane('other')];
    const places = prPlaces([held], [
      { lane: other, session: session('a') },
      { lane: held, session: session('b') },
      { lane: other, session: session('c') },
    ]);
    expect(places.map((p) => [p.lane.id, p.holds, p.sessions.map((s) => s.key)])).toEqual([
      ['held', true, ['b']],
      ['other', false, ['a', 'c']],
    ]);
  });

  it('keeps a worktree that holds the PR even with no sessions', () => {
    expect(prPlaces([lane('held')], []).map((p) => [p.lane.id, p.sessions.length])).toEqual([['held', 0]]);
  });
});

describe('syncWords', () => {
  const git = (g: Partial<GitInfo>): GitInfo => fx.git({ upstream: 'origin/x', ...g });

  it('says what GitHub is missing or has that the worktree lacks', () => {
    expect(syncWords(git({}))).toBe('pushed · clean');
    expect(syncWords(git({ ahead: 2, dirty: 1 }))).toBe('2 unpushed commits · 1 uncommitted file');
    expect(syncWords(git({ behind: 1 }))).toBe('1 commit behind');
  });

  it("doesn't claim a branch is pushed without an upstream", () => {
    expect(syncWords(git({ upstream: null }))).toBe('clean');
    expect(syncWords(git({ upstream: null, dirty: 3 }))).toBe('3 uncommitted files');
    expect(syncWords(null)).toBeNull();
  });
});

describe('prSections', () => {
  const row = (number: number, over: Partial<PrItem> = {}): PrRowItem => {
    const pr = fx.prItem({ number, ...over });
    return { kind: 'pr', key: pr.key, pr, lane: null, sessions: [], places: [] };
  };

  it('puts snoozed reviews apart, the soonest back first, until they wait on you again', () => {
    const rows = [
      row(1, { reviewRequested: true }),
      row(2, { reviewRequested: true, snoozedUntil: 300 }),
      row(3, { mine: true }),
      row(4),
      row(5, { reviewRequested: true, snoozedUntil: 100 }),
      row(6, { state: 'merged', mergedAt: 5 }),
    ];
    expect(prSections(rows).map((s) => [s.id, s.rows.map((r) => r.pr.number)])).toEqual([
      ['requested', [1]],
      ['mine', [3]],
      ['others', [4]],
      ['snoozed', [5, 2]],
      ['done', [6]],
    ]);
  });
});

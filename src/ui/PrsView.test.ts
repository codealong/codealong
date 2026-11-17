import { describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import type { GitInfo, PrItem, Session } from '../types';
import { groupRows } from '../groups';
import { inPrTab, prPlace, prPlaces, prSections, prTabCounts, prWorking, syncWords, type PrRowItem } from './PrsView';

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
    return { kind: 'pr', key: pr.key, pr, lane: null, sessions: [], places: [], primary: null };
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

  it('puts what needs you first, by what it needs: a review, a merge, then work', () => {
    const held = { stage: 'reviewing', head: 'abc', at: 1, rounds: 1, holds: true } as const;
    const rows = [
      row(1, { mine: true }),
      row(2, { mine: true, merge: 'clean', checks: fx.checks({ passed: 2 }) }),
      row(3, { reviewRequested: true, autoReview: held }),
      row(4, { mine: true, checks: fx.checks({ failed: 1 }) }),
      row(5, { reviewRequested: true }),
      row(6, { mine: true, state: 'changes' }),
      row(7, { mine: true, state: 'changes', turn: { kind: 'rereviewAsked', who: 'ken' } }),
      row(8, { merge: 'clean' }),
    ];
    expect(prSections(rows).map((s) => [s.id, s.label, s.rows.map((r) => r.pr.number)])).toEqual([
      ['requested', 'Review requested', [5]],
      ['ready', 'Ready to merge', [2]],
      ['work', 'Needs work', [4, 6]],
      ['agent', 'Agent reviewing', [3]],
      ['mine', 'Yours', [1, 7]],
      ['others', 'Others', [8]],
    ]);
  });

  describe('in tabs', () => {
    const held = { stage: 'reviewing', head: 'abc', at: 1, rounds: 1, holds: true } as const;
    const rows = [
      row(1, { mine: true }),
      row(2, { mine: true, merge: 'clean', checks: fx.checks({ passed: 2 }) }),
      row(3, { reviewRequested: true, autoReview: held }),
      row(4, { mine: true, state: 'draft' }),
      row(5, { reviewRequested: true }),
      row(6, { reviewRequested: true, snoozedUntil: 100 }),
      row(7),
      row(8, { mine: true, state: 'merged', mergedAt: 5 }),
      row(9, { state: 'closed', closedAt: 4 }),
    ];
    const shown = (tab: 'all' | 'mine' | 'others') => prSections(rows, 'status', 0, tab).map((s) => [s.label, s.rows.map((r) => r.pr.number)]);

    it('All lists everyone’s, what needs you first', () => {
      expect(shown('all')).toEqual([
        ['Review requested', [5]],
        ['Ready to merge', [2]],
        ['Agent reviewing', [3]],
        ['Yours', [1, 4]],
        ['Others', [7]],
        ['Snoozed', [6]],
        ['Merged & closed', [8, 9]],
      ]);
    });

    it('Yours lists yours: ready to merge, then waiting on others, then drafts', () => {
      expect(shown('mine')).toEqual([
        ['Ready to merge', [2]],
        ['Waiting on others', [1]],
        ['Drafts', [4]],
        ['Merged & closed', [8]],
      ]);
    });

    it('Others lists everyone else’s: reviews asked of you, the agent’s, snoozed, then the rest', () => {
      expect(shown('others')).toEqual([
        ['Review requested', [5]],
        ['Agent reviewing', [3]],
        ['Snoozed', [6]],
        ['Not asking you', [7]],
        ['Merged & closed', [9]],
      ]);
    });

    it('sorted by when opened, lists only the tab’s', () => {
      expect(prSections(rows, 'opened', 0, 'mine').flatMap((s) => s.rows.map((r) => r.pr.number)).sort()).toEqual([1, 2, 4, 8]);
    });

    it('counts what needs you in each', () => {
      expect(prTabCounts(rows)).toEqual({ all: 2, mine: 1, others: 1 });
      expect([inPrTab({ mine: true }, 'mine'), inPrTab({ mine: true }, 'others'), inPrTab({ mine: false }, 'all')]).toEqual([true, false, true]);
    });
  });

  it('by when opened, puts the open ones under the day each was opened, the newest first, snoozed ones too', () => {
    const now = new Date(2026, 9, 14, 12).getTime();
    const day = (n: number) => new Date(2026, 9, 14 - n, 9).getTime();
    const rows = [
      row(1, { createdAt: day(3), reviewRequested: true }),
      row(2, { createdAt: day(0) }),
      row(3, { createdAt: day(1), reviewRequested: true, snoozedUntil: now + 1 }),
      row(4, { createdAt: day(30) }),
      row(5, { createdAt: day(0) + 1000, mine: true }),
      row(6, { createdAt: day(0), state: 'merged', mergedAt: 5 }),
    ];
    expect(prSections(rows, 'opened', now).map((s) => [s.id, s.rows.map((r) => r.pr.number)])).toEqual([
      ['today', [5, 2]],
      ['yesterday', [3]],
      ['week', [1]],
      ['older', [4]],
      ['done', [6]],
    ]);
  });
});

describe('grouped by whether an agent is working', () => {
  const row = (number: number, over: Partial<PrItem> = {}, statuses: Session['status'][] = []): PrRowItem => {
    const pr = fx.prItem({ number, ...over });
    const sessions = statuses.map((status, i) => ({ lane: lane('w'), session: fx.session({ key: `s${number}-${i}`, status }) }));
    return { kind: 'pr', key: pr.key, pr, lane: null, sessions, places: [], primary: null };
  };

  const one = (over: Partial<Session>) => ({ ...row(9), sessions: [{ lane: lane('w'), session: fx.session(over) }] });

  it('counts a session working on it, and one set aside for its side conversation', () => {
    expect(prWorking(row(1, {}, ['idle', 'working']))).toBe(true);
    expect(prWorking(row(2, {}, ['unread', 'needs', 'stopped']))).toBe(false);
    expect(prWorking(one({ status: 'needs', aside: 1 }))).toBe(true);
    expect(prWorking(row(5))).toBe(false);
  });

  it('leaves out a session that only made or visited its worktree, and an archived one, as the Working filter does', () => {
    expect(prWorking(one({ status: 'working', visitor: 'made this worktree' }))).toBe(false);
    expect(prWorking(one({ status: 'working', archived: true }))).toBe(false);
  });

  it('counts Codealong’s review under way on the commit the PR is at, not one waiting its turn', () => {
    const review = (stage: 'queued' | 'reviewing', head: string) => ({ headOid: 'abc', autoReview: { stage, head, at: 1, rounds: 1, holds: true } });
    expect(prWorking(row(1, review('reviewing', 'abc')))).toBe(true);
    expect(prWorking(row(2, review('queued', 'abc')))).toBe(false);
    expect(prWorking(row(3, review('reviewing', 'old')))).toBe(false);
  });

  it('splits every section into the PRs an agent is working on, then the rest', () => {
    const rows = [row(1, { mine: true }), row(2, { mine: true }, ['working']), row(3, { mine: true, state: 'draft' }, ['working']), row(4, { mine: true, state: 'draft' }), row(5, { mine: true, state: 'merged', mergedAt: 5 }, ['working'])];
    const sections = prSections(rows, 'status', 0, 'mine');
    const g = groupRows(sections, 'working', (r) => prPlace(r, 'working'), { pinned: [], lanes: [], empty: false, opened: new Set(), whole: false, apart: () => true });
    expect(g.pinned).toEqual([]);
    expect(g.sections.map((s) => [s.label, s.count, s.groups.map((x) => [x.key, x.rows.map((r) => r.pr.number)])])).toEqual([
      ['Waiting on others', 2, [['working', [2]], ['', [1]]]],
      ['Drafts', 2, [['working', [3]], ['', [4]]]],
      ['Merged & closed', 1, [['working', [5]]]],
    ]);
  });
});

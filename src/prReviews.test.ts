import { describe, expect, it } from 'vitest';
import { isOpen, openFromOthers, reviewersOf, reviewsCount, saidShown } from './prReviews';
import type { PrComment, PrReview } from './types';

const comment = (id: string, author: string, at: number, more: Partial<PrComment> = {}): PrComment => ({ id, author, body: id, at, url: null, path: null, resolved: false, mine: author === 'me', ...more });
const review = (author: string, state: string, at: number, mine = false): PrReview => ({ author, state, at, mine });

describe('isOpen', () => {
  it('is a thread on the code nobody resolved, or a comment Jev says asks for a change', () => {
    expect(isOpen(comment('t', 'ana', 1, { path: 'a.ts' }), new Set())).toBe(true);
    expect(isOpen(comment('t', 'ana', 1, { path: 'a.ts', resolved: true }), new Set())).toBe(false);
    expect(isOpen(comment('c', 'ana', 1), new Set())).toBe(false);
    expect(isOpen(comment('c', 'ana', 1), new Set(['c']))).toBe(true);
  });
});

describe('reviewersOf', () => {
  // Comments come from the engine newest first.
  const detail = {
    reviews: [review('ken', 'APPROVED', 50), review('ana', 'CHANGES_REQUESTED', 40), review('me', 'COMMENTED', 30, true), review('bo', 'COMMENTED', 20)],
    requested: ['gwen', 'platform', 'ken'],
    comments: [
      comment('bot1', 'vercel', 90),
      comment('reply', 'author1', 80),
      comment('ana-text', 'ana', 40, { review: 'CHANGES_REQUESTED' }),
      comment('ana-done', 'ana', 35, { path: 'b.ts', resolved: true }),
      comment('ana-open', 'ana', 30, { path: 'a.ts', line: 12 }),
      comment('bo-note', 'bo', 20, { path: 'c.ts' }),
    ],
  };

  it('lists each person once, whoever asked for changes first and whoever only wrote last', () => {
    const got = reviewersOf(detail, 'author1', new Set());
    expect(got.map((r) => `${r.who}:${r.state ?? (r.requested ? 'asked' : '-')}`)).toEqual([
      'ana:CHANGES_REQUESTED',
      // Two who commented: the one with a thread still open first.
      'bo:COMMENTED',
      'me:COMMENTED',
      'ken:APPROVED',
      'gwen:asked',
      'platform:asked',
      // Only wrote, the latest first: a bot, the author.
      'vercel:-',
      'author1:-',
    ]);
  });

  it('puts a verdict with its words, what’s still open first', () => {
    const ana = reviewersOf(detail, 'author1', new Set()).find((r) => r.who === 'ana');
    expect(ana?.said.map((c) => c.id)).toEqual(['ana-open', 'ana-text', 'ana-done']);
    expect(ana?.open).toBe(1);
    // Jev saying the review's text asks for a change makes it open too.
    const asked = reviewersOf(detail, 'author1', new Set(['ana-text'])).find((r) => r.who === 'ana');
    expect(asked?.said.map((c) => c.id)).toEqual(['ana-text', 'ana-open', 'ana-done']);
    expect(asked?.open).toBe(2);
  });

  it('says who you are, who wrote it, and who’s asked again after reviewing', () => {
    const got = reviewersOf(detail, 'author1', new Set());
    const of = (who: string) => got.find((r) => r.who === who);
    expect(of('me')?.mine).toBe(true);
    expect(of('author1')?.author).toBe(true);
    expect(of('ken')?.requested).toBe(true);
    expect(of('ken')?.state).toBe('APPROVED');
    // A review of yours with nothing written is still yours.
    expect(reviewersOf({ reviews: [review('me', 'APPROVED', 1, true)], requested: [], comments: [] }, null, new Set())[0].mine).toBe(true);
  });

  it('counts what’s open from others than you', () => {
    const got = reviewersOf({ ...detail, comments: [...detail.comments, comment('mine-open', 'me', 5, { path: 'd.ts' })] }, 'author1', new Set());
    // ana's thread and bo's; your own thread waits on the author, not you.
    expect(openFromOthers(got)).toBe(2);
  });

  it('copes with an older engine that sends no comments', () => {
    expect(reviewersOf({ reviews: [review('ken', 'APPROVED', 1)], requested: [] }, null, new Set()).map((r) => r.said.length)).toEqual([0]);
  });
});

describe('saidShown', () => {
  it('shows what’s open and at least three, or everything', () => {
    const said = ['a', 'b', 'c', 'd', 'e'].map((id, i) => comment(id, 'ana', 10 - i, { path: 'x.ts' }));
    const r = { who: 'ana', mine: false, author: false, state: null, at: null, requested: false, said, open: 0 };
    expect(saidShown(r, false).length).toBe(3);
    expect(saidShown({ ...r, open: 4 }, false).length).toBe(4);
    expect(saidShown(r, true).length).toBe(5);
  });
});

describe('reviewsCount', () => {
  it('counts your draft’s comments while you have one, else what’s still open', () => {
    expect(reviewsCount({ count: 2 }, 5).count).toBe(2);
    expect(reviewsCount(null, 3).count).toBe(3);
    expect(reviewsCount(undefined, 0).count).toBeNull();
    expect(reviewsCount(null, 3).title).toMatch(/3 still open/);
  });
});

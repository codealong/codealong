import { describe, expect, it } from 'vitest';
import { asksFor, commentPreview, commentsToAsk, heldForAgent, mayApprove, nudgeWords, prFlag, prPills, prReady, reviewMark, reviewersWords, type PrFacts } from './prStatus';
import * as fx from './testing/fixtures';
import type { PrComment, PrItem } from './types';

const checks = fx.checks;
const facts = (f: Partial<PrFacts> = {}): PrFacts => ({ state: 'open', checks: checks({ passed: 12 }), merge: 'blocked', base: 'main', ...f });
const item = (f: Partial<PrItem> = {}): PrItem => fx.prItem({ ...facts(), mine: true, reviewRequested: false, ...f });
const texts = (f: Partial<PrFacts>) => prPills(facts(f)).map((p) => [p.text, p.tone]);

describe('prPills', () => {
  it('folds into one pill when GitHub would merge it', () => {
    const ready = facts({ state: 'approved', merge: 'clean' });
    expect(prReady(ready)).toBe(true);
    expect(prPills(ready).map((p) => [p.text, p.tone, p.title])).toEqual([['Ready to merge', 'ready', 'Approved, 12 checks passed, no conflicts: GitHub can merge it']]);
    // A repo that requires no review.
    expect(prPills(facts({ merge: 'clean', checks: checks() }))[0].title).toBe('No conflicts: GitHub can merge it');
  });

  it("doesn't call it ready on an approval and passing checks alone", () => {
    expect(texts({ state: 'approved', merge: 'unknown' })).toEqual([['Approved', 'pass'], ['12', 'pass']]);
    expect(texts({ state: 'approved', merge: 'conflicts', checks: checks() })).toEqual([['Approved', 'pass'], ['Conflicts', 'attn']]);
    expect(texts({ state: 'approved', merge: 'behind' })).toEqual([['Approved', 'pass'], ['12', 'pass'], ['Out of date', 'attn']]);
    // Clean, but a check is still running.
    expect(prReady(facts({ state: 'approved', merge: 'clean', checks: checks({ passed: 11, pending: 1 }) }))).toBe(false);
  });

  it('shows the review and the checks side by side, each in its own color', () => {
    expect(texts({ checks: checks({ passed: 3, failed: 2, failing: ['lint', 'e2e'] }) })).toEqual([['Awaiting review', 'plain'], ['2 failing', 'alert']]);
    expect(texts({ state: 'changes', checks: checks({ pending: 4 }) })).toEqual([['Changes requested', 'attn'], ['4 running', 'plain']]);
    expect(texts({ state: 'draft', checks: checks() })).toEqual([['Draft', 'quiet']]);
    const failing = prPills(facts({ checks: checks({ failed: 2, failing: ['lint', 'e2e'] }) }))[1];
    expect([failing.short, failing.title]).toEqual(['2', '2 checks failing: lint, e2e']);
    // Short of room, Conflicts keeps only its icon.
    expect(prPills(facts({ merge: 'conflicts' })).map((p) => p.short)).toEqual([null, null, '']);
  });

  it("says only merged or closed once it's done", () => {
    expect(texts({ state: 'merged', merge: 'unknown', checks: checks({ failed: 1 }) })).toEqual([['Merged', 'merged']]);
    expect(texts({ state: 'closed', merge: 'unknown' })).toEqual([['Closed', 'quiet']]);
  });
});

describe('prFlag', () => {
  it('flags a review asked of you, whoever wrote the PR', () => {
    expect(prFlag(item({ mine: false, reviewRequested: true }))).toBe('attn');
    expect(prFlag(item({ mine: false, reviewRequested: true, snoozedUntil: 5 }))).toBeNull();
  });

  it('flags your PRs that are ready or held up by something you can fix', () => {
    expect(prFlag(item({ state: 'approved', merge: 'clean' }))).toBe('ready');
    expect(prFlag(item({ checks: checks({ failed: 1 }) }))).toBe('alert');
    expect(prFlag(item({ state: 'changes' }))).toBe('attn');
    expect(prFlag(item({ state: 'approved', merge: 'conflicts' }))).toBe('attn');
    expect(prFlag(item({ state: 'approved', merge: 'behind' }))).toBe('attn');
  });

  it("leaves waiting PRs, other people's and finished ones alone", () => {
    expect(prFlag(item())).toBeNull();
    expect(prFlag(item({ state: 'approved', merge: 'unknown' }))).toBeNull();
    expect(prFlag(item({ mine: false, state: 'approved', merge: 'clean' }))).toBeNull();
    expect(prFlag(item({ mine: false, checks: checks({ failed: 3 }) }))).toBeNull();
    expect(prFlag(item({ state: 'merged', merge: 'unknown', reviewRequested: true }))).toBeNull();
  });
});

describe('mayApprove', () => {
  it('approves any open PR but your own, whether or not your review was asked for', () => {
    expect(mayApprove(item({ mine: false, reviewRequested: true }))).toBe(true);
    expect(mayApprove(item({ mine: false }))).toBe(true);
    expect(mayApprove(item({ mine: false, state: 'draft' }))).toBe(true);
    expect(mayApprove(item({ mine: true }))).toBe(false);
    expect(mayApprove(item({ mine: false, state: 'merged' }))).toBe(false);
    expect(mayApprove(item({ mine: false, state: 'closed' }))).toBe(false);
  });
});

describe('nudgeWords', () => {
  const made = { title: 'marketing email broadcast', runs: true };
  it('names the one session that is told, and says when nobody is', () => {
    expect(nudgeWords(item({}), true, made)).toMatch(/^On for this PR\. “marketing email broadcast”, its primary session, is sent a message/);
    expect(nudgeWords(item({ nudgeByDefault: true }), true, made)).toMatch(/^On for every PR of yours, by Settings\. “marketing/);
    expect(nudgeWords(item({}), true, { ...made, runs: false })).toBe('On for this PR. “marketing email broadcast”, its primary session, isn’t running here, so nobody is told until it is.');
    expect(nudgeWords(item({}), true, null)).toBe('On for this PR. No session of it is known here, so nobody is told.');
  });
  it('says what turning it on would do, an approval it has already first', () => {
    expect(nudgeWords(item({ state: 'approved' }), false, made)).toBe('Off for this PR. It’s approved already: turning this on tells “marketing email broadcast” that now, then of what happens on it next.');
    expect(nudgeWords(item({}), false, null)).toBe('Off for this PR. Turned on, its primary session is told of reviews, comments, an approval, a merge and failing checks.');
  });
  it('says only what Settings has nudges tell of', () => {
    expect(nudgeWords(item({}), true, made, 'feedback')).toBe('On for this PR. “marketing email broadcast”, its primary session, is sent a message as one of yours when someone else asks for changes or comments, never for an approval or a merge.');
    expect(nudgeWords(item({}), true, made, 'feedback-and-checks')).toMatch(/asks for changes or comments and when its checks fail, never for an approval or a merge\.$/);
    // An approval it has already isn't told either.
    expect(nudgeWords(item({ state: 'approved' }), false, made, 'feedback')).toBe('Off for this PR. Turned on, “marketing email broadcast” is told of comments and requests for changes, never an approval or a merge.');
  });
});

describe('review comments', () => {
  const c = (id: string, extra: Partial<PrComment> = {}): PrComment => ({ id, author: 'ana', body: 'x', at: null, url: null, path: null, resolved: false, mine: false, ...extra });
  const all = [c('a'), c('mine', { mine: true }), c('done', { resolved: true }), c('b')];

  it('asks about others’ comments on open threads only, and marks the likely ones', () => {
    expect(commentsToAsk(all).map((x) => x.id)).toEqual(['a', 'b']);
    expect([...asksFor(all, { a: 0.9, b: 0.4, mine: 0.99, done: 0.99 })]).toEqual(['a']);
  });

  it('shows a comment as plain text on one line', () => {
    expect(commentPreview('**Please** add a [test](http://x)\n\n```ts\ncode\n```')).toBe('Please add a test [code]');
    expect(commentPreview('a'.repeat(300), 10)).toBe('aaaaaaaaa…');
  });
});

describe('reviewMark', () => {
  const key = (f: Partial<PrItem>) => reviewMark(item(f))?.key ?? null;

  it('marks what the chip’s color can’t tell apart', () => {
    expect(key({ state: 'approved' })).toBe('approved');
    expect(key({ state: 'changes' })).toBe('changes');
    expect(key({ state: 'open' })).toBe(null);
  });

  it('puts a review asked of you first, unless it’s snoozed, and the agent’s before that', () => {
    expect(reviewMark(item({ state: 'approved', mine: false, reviewRequested: true, requestedBy: 'gwen' }))).toMatchObject({ key: 'asked', tone: 'attn', title: 'Your review is asked for, by gwen' });
    expect(key({ state: 'approved', mine: false, reviewRequested: true, snoozedUntil: Date.now() + 60_000 })).toBe('approved');
    expect(key({ mine: false, reviewRequested: true, autoReview: { stage: 'reviewing', head: 'abc', at: 0, rounds: 1, holds: true } })).toBe('agent');
  });

  it('marks nothing on a draft or a PR that’s done', () => {
    expect(key({ state: 'draft', reviewRequested: true })).toBe(null);
    expect(key({ state: 'merged' })).toBe(null);
    expect(key({ state: 'closed' })).toBe(null);
  });

  it('goes by the state alone for a worktree’s PR', () => {
    expect(reviewMark(fx.pr({ state: 'approved' }))?.key).toBe('approved');
    expect(reviewMark(fx.pr({ state: 'open' }))).toBe(null);
  });
});

describe('reviewersWords', () => {
  it('says whose reviews an open PR waits on, you first', () => {
    expect(reviewersWords(item({ mine: false, reviewRequested: true, reviewersAsked: ['ana', 'platform'] }))).toBe('Waiting on a review from you, ana and platform');
    expect(reviewersWords(item({ reviewersAsked: ['ana'] }))).toBe('Waiting on a review from ana');
    expect(reviewersWords(item({ mine: false, reviewRequested: true }))).toBe('Waiting on a review from you');
  });

  it('says nothing when it waits on nobody’s, or once it’s done', () => {
    expect(reviewersWords(item({}))).toBe(null);
    expect(reviewersWords(item({ state: 'merged', reviewersAsked: ['ana'] }))).toBe(null);
    expect(reviewersWords(fx.pr({ state: 'open' }))).toBe(null);
  });
});

describe('heldForAgent', () => {
  const asked = (autoReview: PrItem['autoReview']) => item({ mine: false, reviewRequested: true, autoReview });
  it('keeps a review the agent is still on out of what waits on you, and says so', () => {
    const held = asked({ stage: 'reviewing', head: '309811a', at: 0, rounds: 1, holds: true });
    expect(heldForAgent(held)).toBe(true);
    expect(prFlag(held)).toBe(null);
    expect(prPills(held)[0]).toMatchObject({ text: 'Agent reviewing', tone: 'plain' });
  });
  it('hands it to you once the agent is done', () => {
    const done = asked({ stage: 'posted', head: '309811a', at: 0, rounds: 1 });
    expect(heldForAgent(done)).toBe(false);
    expect(prFlag(done)).toBe('attn');
    expect(prPills(done)[0].text).toBe('Awaiting review');
    expect(prFlag(asked(undefined))).toBe('attn');
  });
});

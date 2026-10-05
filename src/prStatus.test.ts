import { describe, expect, it } from 'vitest';
import { asksFor, commentPreview, commentsToAsk, prFlag, prPills, prReady, type PrFacts } from './prStatus';
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

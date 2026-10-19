import { describe, expect, it } from 'vitest';
import { againFor, askAccount, askAccounts, autoReviewWords, groupReviewWords, reviewLane, reviewLineWords, reviewPicks, reviewsRunIn, suggestWords } from './review';
import * as fx from './testing/fixtures';
import type { AutoReview, Lane, PrItem, StartAccount } from './types';

describe('autoReviewWords', () => {
  const r = (stage: AutoReview['stage'], more: Partial<AutoReview> = {}): AutoReview => ({ stage, head: '309811aaaabbbb', at: 0, rounds: 1, ...more });
  it('says where the latest round stands, by its commit', () => {
    expect(autoReviewWords(r('reviewing'))).toEqual({ icon: 'eye', says: 'Agent reviewing 309811a', note: null });
    expect(autoReviewWords(r('posted', { comment: 'https://github.com/o/r/pull/7#issuecomment-1' })).says).toBe('Agent reviewed 309811a and commented');
    expect(autoReviewWords(r('skipped', { note: 'The diff is generated code.' }))).toEqual({ icon: 'circle-dashed', says: 'Agent passed on 309811a', note: 'The diff is generated code.' });
    expect(autoReviewWords(r('failed', { note: 'Its session is gone.', parts: [{ who: 'auto', session: 'codex:codex:x', state: 'failed' }] })).says).toBe('Agent’s review of 309811a didn’t finish');
    // No session ever began: GitHub couldn't be read, or no account could start one.
    expect(autoReviewWords(r('failed', { note: 'GitHub: gh error connecting to api.github.com' })).says).toBe('Agent’s review of 309811a didn’t start');
    expect(autoReviewWords(r('failed', { parts: [{ who: 'auto', state: 'failed', note: 'Its session never showed up.' }] })).says).toBe('Agent’s review of 309811a didn’t start');
    expect(autoReviewWords(r('drafted'))).toEqual({ icon: 'note', says: 'Agent drafted a review of 309811a', note: 'Only you see it until you submit it.' });
  });
  it('says where a review of a PR of yours went back to, or why it hasn’t', () => {
    expect(autoReviewWords(r('kept', { own: true, sent: 'k' }), undefined, 'Add teams')).toEqual({ icon: 'send', says: 'Agent reviewed 309811a', note: 'It went back to “Add teams”.' });
    expect(autoReviewWords(r('kept', { own: true, sent: 'k' })).note).toBe('It went back to the session that opened the PR.');
    expect(autoReviewWords(r('kept', { own: true, note: 'It found nothing to raise, so it isn’t sent.' }))).toEqual({ icon: 'check', says: 'Agent reviewed 309811a', note: 'It found nothing to raise, so it isn’t sent.' });
    // Not "posting is off": it never posts.
    expect(autoReviewWords(r('kept', { own: true })).note).toBe(null);
  });
  it('says what the agent’s review was submitted for you as, when it was', () => {
    expect(autoReviewWords(r('drafted', { submitted: 'approve', submittedAt: 5, suggests: 'approve' }))).toEqual({ icon: 'check', says: 'Agent approved 309811a for you', note: null });
    expect(autoReviewWords(r('drafted', { submitted: 'request-changes', suggests: 'request-changes' }), { draft: false, submitted: 'CHANGES_REQUESTED' })).toEqual({ icon: 'alert', says: 'Agent requested changes on 309811a for you', note: null });
    expect(autoReviewWords(r('posted', { submitted: 'comment', suggests: 'request-changes' })).says).toBe('Agent commented on 309811a for you');
    expect(autoReviewWords(r('kept', { submitted: 'approve' })).note).toBe('Its review is in its session: posting is off.');
    expect(suggestWords('request-changes')).toBe('Suggests changes');
  });
  it('offers to try a review that didn’t get done again: posting what was written, else reviewing', () => {
    const pr = (autoReview: AutoReview | undefined, more: Partial<PrItem> = {}) => ({ autoReview, headOid: '309811aaaabbbb', state: 'open' as PrItem['state'], ...more });
    const wrote = [{ who: 'auto', session: 'k', state: 'done' as const }];
    expect(againFor(pr(r('failed')))).toBe('review');
    expect(againFor(pr(r('failed', { parts: [{ who: 'auto', session: 'k', state: 'failed' }] })))).toBe('review');
    expect(againFor(pr(r('failed', { parts: wrote, note: 'It couldn’t be posted: gh: HTTP 422' })))).toBe('post');
    // A review of a commit the PR has moved on from isn't worth posting: it's reviewed again.
    expect(againFor(pr(r('failed', { parts: wrote }), { headOid: '4444444cccc' }))).toBe('review');
    // Nothing to try again: done, under way, closed, or the next round coming by itself.
    expect(againFor(pr(r('drafted')))).toBeNull();
    expect(againFor(pr(r('reviewing')))).toBeNull();
    expect(againFor(pr(undefined))).toBeNull();
    expect(againFor(pr(r('failed'), { state: 'merged' }))).toBeNull();
    expect(againFor(pr(r('failed', { behind: true })))).toBeNull();
  });
  it('says over the box where the review stands, with your draft', () => {
    const draft = { id: 'R', body: 'x', commit: null, url: null, comments: [], count: 3 };
    expect(reviewLineWords({ autoReview: r('drafted'), mine: false }, draft)).toEqual({ icon: 'note', says: 'Agent drafted a review of 309811a: your draft review, 3 comments', busy: false });
    expect(reviewLineWords({ autoReview: r('reviewing'), mine: false }, null)).toEqual({ icon: 'eye', says: 'Agent reviewing 309811a', busy: true });
    expect(reviewLineWords({ autoReview: undefined, mine: false }, draft).says).toBe('Your draft review, 3 comments');
    expect(reviewLineWords({ autoReview: undefined, mine: false }, null).says).toBe('No review of yours yet');
    expect(reviewLineWords({ autoReview: undefined, mine: true }, null).says).toBe('No agent review of your PR');
  });
  it('says a draft that is no longer one on GitHub was submitted, or thrown away', () => {
    expect(autoReviewWords(r('drafted'), { draft: true, submitted: null }).note).toBe(null);
    expect(autoReviewWords(r('drafted'), { draft: false, submitted: 'APPROVED' }).note).toBe('You submitted it, approving it.');
    expect(autoReviewWords(r('drafted'), { draft: false, submitted: null }).note).toBe('It isn’t a draft on GitHub any more: it was submitted or thrown away.');
    // Only a draft goes away so.
    expect(autoReviewWords(r('posted'), { draft: false, submitted: null }).note).toBe(null);
  });
  it('says a round is to come, and that the PR waits on you once the agent is done', () => {
    expect(autoReviewWords(r('queued', { holds: true }))).toEqual({ icon: 'hourglass', says: 'Agent review of 309811a to come', note: 'It waits on your review once the agent’s is done.' });
    expect(autoReviewWords(r('posted', { behind: true, holds: true })).note).toBe('New commits since: it reviews them again once they settle. It waits on your review once the agent’s is done.');
  });
  it('says a review kept in its session is there, and that new commits get another', () => {
    expect(autoReviewWords(r('kept')).note).toBe('Its review is in its session: posting is off.');
    expect(autoReviewWords(r('kept', { note: 'Not posted: the PR was closed before the review was done.' })).note).toBe('Not posted: the PR was closed before the review was done.');
    expect(autoReviewWords(r('posted', { behind: true })).note).toBe('New commits since: it reviews them again once they settle.');
    expect(autoReviewWords(r('reviewing', { behind: true })).note).toBe('New commits since: it reviews them once this one’s done.');
  });
});

const ghostty = { place: 'terminal' as const, label: 'Ghostty' };
const background = { place: 'background' as const, label: 'Background' };
const codexApp = { place: 'codex-app' as const, label: 'Codex app' };

const ACCOUNTS: StartAccount[] = [
  fx.startAccount({ name: 'claude', places: [ghostty, background] }),
  fx.startAccount({ name: 'claude2', unavailable: 'Not signed in.' }),
  fx.startAccount({ name: 'codex', harness: 'codex', places: [codexApp] }),
];

const lane = (id: string, repoSlug: string | null, origin: Lane['origin'] = 'other'): Lane => fx.lane({ id, repoSlug, origin });

describe('reviewLane', () => {
  const pr = { repoSlug: 'acme/app', lanes: ['wt-2'] };
  it('starts in a worktree that holds the PR', () => {
    expect(reviewLane(pr, [lane('main', 'acme/app', 'main'), lane('wt-2', 'acme/app')])).toEqual({ lane: lane('wt-2', 'acme/app'), checkedOut: true });
  });
  it('else in the repo’s main checkout, never on another device', () => {
    const lanes = [lane('@mini:wt', 'acme/app'), lane('other', 'acme/app'), lane('main', 'acme/app', 'main'), lane('x', 'acme/x', 'main')];
    expect(reviewLane({ repoSlug: 'acme/app', lanes: ['@mini:wt'] }, lanes)).toEqual({ lane: lane('main', 'acme/app', 'main'), checkedOut: false });
  });
  it('knows a repo by its name now', () => {
    const renamed = { ...lane('old', 'acme/old'), repoSlugNow: 'acme/app' };
    expect(reviewLane({ repoSlug: 'acme/app', lanes: [] }, [renamed])?.lane.id).toBe('old');
  });
  it('is null without a checkout of the repo', () => {
    expect(reviewLane({ repoSlug: 'acme/app', lanes: [] }, [lane('x', 'acme/x', 'main')])).toBeNull();
  });
});

describe('reviewPicks', () => {
  // claude reviews in the background, opencode in Codealong, codex only opens its app; claude2
  // isn't signed in.
  const behind = { place: 'background' as const, label: 'Background', behind: true };
  const more = [...ACCOUNTS, fx.startAccount({ name: 'opencode', harness: 'opencode', places: [ghostty, behind, { place: 'codealong', label: 'Codealong' }] })];
  it('lists Auto, then each account that can review without a terminal, then Codex’s own review page', () => {
    expect(reviewPicks(more, [], 0, true)).toEqual([
      { kind: 'review', label: 'Auto (claude) in the background', account: null },
      { kind: 'review', label: 'claude in the background', account: 'claude' },
      { kind: 'review', label: 'opencode in Codealong', account: 'opencode' },
      { kind: 'codex-app', label: 'The Codex app’s own review' },
    ]);
  });
  it('names the reviewer Settings has first, and Auto for one of yours as another harness', () => {
    const reviews = { auto: false, account: 'opencode', model: 'big', own: { auto: false } };
    expect(reviewPicks(more, [], 0, true, reviews)[0]).toEqual({ kind: 'review', label: 'opencode, big in Codealong, as Settings says', account: null });
    expect(reviewPicks(more, [], 0, true, reviews, true)[0]).toEqual({ kind: 'review', label: 'Auto (another harness than its session’s)', account: null });
    const ownGroup = { auto: false, own: { auto: false, account: 'reviewers', other_harness: true } };
    const groups = [{ name: 'reviewers', strategy: 'alternate-harness' as const, members: [{ account: 'opencode', model: '' }, { account: 'claude', model: '' }] }];
    expect(reviewPicks(more, [], 0, true, ownGroup, true, groups)).toEqual([
      { kind: 'review', label: 'reviewers (opencode) in Codealong, as Settings says', account: null },
      { kind: 'review', label: 'claude in the background', account: 'claude' },
      { kind: 'review', label: 'opencode in Codealong', account: 'opencode' },
      { kind: 'codex-app', label: 'The Codex app’s own review' },
    ]);
  });
  it('offers no choice of one, and without a checkout only Codex’s own review page', () => {
    expect(reviewPicks(ACCOUNTS, [], 0, true).map((p) => p.label)).toEqual(['claude in the background', 'The Codex app’s own review']);
    expect(reviewPicks(ACCOUNTS, [], 0, false)).toEqual([{ kind: 'codex-app', label: 'The Codex app’s own review' }]);
  });
});

describe('reviewsRunIn', () => {
  const codealong = { place: 'codealong' as const, label: 'Codealong' };
  const behind = { place: 'background' as const, label: 'Background', behind: true };
  const claude = fx.startAccount({ name: 'claude', places: [ghostty, background, codealong] });
  const codex = fx.startAccount({ name: 'codex', harness: 'codex', places: [codexApp, ghostty, behind, codealong] });
  const cursor = fx.startAccount({ name: 'cursor', harness: 'cursor', places: [ghostty, behind] });
  const where = (start: Parameters<typeof reviewsRunIn>[1]) => [claude, codex, cursor].map((a) => reviewsRunIn(a, start));

  it('never opens a terminal unless Settings says so', () => {
    // A harness with neither a background service nor Codealong's adapter doesn't review.
    expect(where('no-terminal')).toEqual(['in the background', 'in Codealong', null]);
    expect(where(undefined)).toEqual(where('no-terminal'));
    expect(where('codealong')).toEqual(['in Codealong', 'in Codealong', null]);
  });
  it('opens one where Background is a terminal behind your windows, or in a window when asked', () => {
    expect(where('background')).toEqual(['in the background', 'in Codealong', 'in a terminal behind your windows']);
    expect(where('terminal')).toEqual(['in Ghostty', 'in Ghostty', 'in Ghostty']);
  });
  it('says which of a routing group’s accounts can’t review where reviews run, which it passes over', () => {
    const agy = fx.startAccount({ name: 'antigravity', harness: 'antigravity', places: [ghostty, behind] });
    const accounts = [claude, codex, cursor, agy];
    const reviewers = { name: 'reviewers', strategy: 'round-robin', members: [{ account: 'codex' }, { account: 'cursor' }, { account: 'antigravity' }] };
    expect(groupReviewWords(reviewers, accounts, 'no-terminal', 'without a terminal')).toBe('Routing group reviewers (round-robin) picks among its accounts that can start a review without a terminal: cursor and antigravity can’t, so only codex reviews.');
    expect(groupReviewWords(reviewers, accounts, 'background', 'in the background')).toBe('Routing group reviewers (round-robin) picks among its accounts that can start a review in the background.');
    // One turned off, or gone from the accounts, can't either.
    const two = { ...reviewers, members: [{ account: 'claude' }, { account: 'codex', model: 'gpt-5.5' }, { account: 'codex' }, { account: 'gone' }] };
    expect(groupReviewWords(two, accounts, 'no-terminal', 'without a terminal')).toBe('Routing group reviewers (round-robin) picks among its accounts that can start a review without a terminal: gone can’t, so only claude and codex review.');
    expect(groupReviewWords({ ...reviewers, members: [{ account: 'cursor' }] }, accounts, 'codealong', 'in Codealong')).toBe('Routing group reviewers (round-robin) picks among its accounts that can start a review in Codealong, and none of them can: pick another reviewer, or change where reviews run.');
  });
  it('keeps Review with… to the accounts that can start one there', () => {
    const accounts = [claude, codex, cursor];
    expect(reviewPicks(accounts, [], 0, true, { auto: false, start: 'no-terminal' }).filter((p) => p.kind === 'review' && p.account).map((p) => p.label)).toEqual(['claude in the background', 'codex in Codealong']);
    expect(reviewPicks(accounts, [], 0, true, { auto: false, start: 'background' }).map((p) => p.label)).toContain('cursor in a terminal behind your windows');
  });
});

describe('askAccount', () => {
  const opencode = fx.startAccount({ name: 'oc', harness: 'opencode', places: [ghostty] });
  const all = [opencode, ...ACCOUNTS];

  it('offers the accounts whose harness holds a conversation of its own and that can start here', () => {
    expect(askAccounts(all).map((a) => a.name)).toEqual(['claude', 'codex']);
  });

  it('asks the account picked while it can answer, else the one Auto would start', () => {
    expect(askAccount(all, [], 0, 'codex')).toBe('codex');
    // One that can't answer, or can't start here, gives way.
    expect(askAccount(all, [], 0, 'oc')).toBe('claude');
    expect(askAccount(all, [], 0, 'claude2')).toBe('claude');
    expect(askAccount([opencode], [], 0, null)).toBeNull();
  });
});

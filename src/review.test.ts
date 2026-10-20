import { describe, expect, it } from 'vitest';
import { againFor, askAccount, askAccounts, askOn, autoReviewWords, groupReviewWords, reviewLane, reviewLineWords, reviewPicks, reviewsRunIn, sinceNote, sinceWords, suggestWords } from './review';
import * as fx from './testing/fixtures';
import type { AutoReview, Draft, Lane, LastReview, PrItem, Since, StartAccount } from './types';

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
  it('says a PR the agent approved waits on your sign-off once the commits since are reviewed', () => {
    const approved = { suggests: 'approve' as const, approves: '309811aaaabbbb' };
    expect(autoReviewWords(r('posted', approved)).note).toBe(null);
    expect(autoReviewWords(r('posted', { ...approved, behind: true, holds: true })).note).toBe('New commits since: it reviews them again once they settle. It approved 309811a before: the PR waits on your sign-off once the agent approves the commits since.');
    expect(autoReviewWords(r('reviewing', { approves: '1234567ffff', suggests: null, holds: true })).note).toBe('It approved 1234567 before: the PR waits on your sign-off once the agent approves the commits since.');
    // Their review didn't get done, so it's yours: what the agent made of the commit before.
    expect(autoReviewWords(r('failed', { approves: '1234567ffff', note: 'Its session is gone.' })).note).toBe('Its session is gone. It approved 1234567 before.');
    // The line over the box names the commit it approved, when its round is of a later one.
    expect(reviewLineWords({ autoReview: r('reviewing', { approves: '1234567ffff' }), mine: false }, null)).toMatchObject({ says: 'Agent reviewing 309811a', approved: '1234567' });
    expect(reviewLineWords({ autoReview: r('posted', approved), mine: false }, null).approved).toBe(null);
    // Before it approved anything, new commits hold the PR for their review.
    expect(autoReviewWords(r('posted', { behind: true, holds: true })).note).toBe('New commits since: it reviews them again once they settle. It waits on your review once the agent’s is done.');
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
    expect(reviewLineWords({ autoReview: r('drafted'), mine: false }, draft)).toEqual({ icon: 'note', says: 'Agent drafted a review of 309811a: your draft review, 3 comments', busy: false, approved: null, agent: null, since: null });
    expect(reviewLineWords({ autoReview: r('reviewing'), mine: false }, null)).toEqual({ icon: 'eye', says: 'Agent reviewing 309811a', busy: true, approved: null, agent: null, since: null });
    expect(reviewLineWords({ autoReview: undefined, mine: false }, draft).says).toBe('Your draft review, 3 comments');
    expect(reviewLineWords({ autoReview: undefined, mine: false }, null).says).toBe('No review of yours yet');
    expect(reviewLineWords({ autoReview: undefined, mine: true }, null).says).toBe('No agent review of your PR');
  });
  it('leads the line over the box with your own last review, by whether it’s of the commit the PR’s at', () => {
    const head = '309811aaaabbbb';
    const yours = (state: LastReview['state'], commit: string | null = head): LastReview => ({ state, commit, at: 5 });
    const line = (lastReview: LastReview, autoReview?: AutoReview, draft: Draft | null = null) => reviewLineWords({ autoReview, mine: false, headOid: head, lastReview }, draft);
    // Of the commit it's at: it says so, and the agent's round is for the line's title.
    expect(line(yours('approved'), r('posted', { suggests: 'approve' }))).toEqual({ icon: 'check', says: 'You approved 309811a, its latest commit', busy: false, approved: null, agent: 'Agent reviewed 309811a and commented', since: null });
    expect(line(yours('changes')).says).toBe('You asked for changes on 309811a, its latest commit');
    expect(line(yours('commented')).icon).toBe('message-circle');
    // Of an earlier one: the line offers what changed since, while the agent reviews the new ones.
    expect(line(yours('approved', '1234567ffff'), r('reviewing'))).toMatchObject({ says: 'You approved 1234567', busy: true, agent: 'Agent reviewing 309811a', since: '1234567ffff' });
    // Dismissed, it stands no more, of whichever commit.
    expect(line(yours('dismissed'))).toMatchObject({ icon: 'x', says: 'Your review of 309811a was dismissed', since: null });
    expect(line(yours('dismissed', '1234567ffff')).since).toBe('1234567ffff');
    // Your draft follows it.
    expect(line(yours('approved', '1234567ffff'), undefined, { id: 'R', body: '', commit: null, url: null, comments: [], count: 1 }).says).toBe('You approved 1234567 · your draft review, 1 comment');
    // Not on a PR of yours, which you don't review.
    expect(reviewLineWords({ autoReview: undefined, mine: true, headOid: head, lastReview: yours('commented') }, null).says).toBe('No agent review of your PR');
  });
  it('says what changed since your review in the pill beside the line, and under Changes', () => {
    const since = (commits: number, files: number, follows = true): Since => ({ follows, commits, files: Array.from({ length: files }, (_, i) => ({ path: `f${i}`, status: 'modified', additions: 1, deletions: 0, patch: null })), moreFiles: false });
    expect(sinceWords('1234567ffff', null)).toEqual({ text: 'New commits', title: 'Show what changed since your review of 1234567, under Changes' });
    expect(sinceWords('1234567ffff', since(3, 2))).toEqual({ text: '3 new commits', title: 'Show what changed since your review of 1234567: 2 files, under Changes' });
    expect(sinceWords('1234567ffff', since(0, 0, false)).text).toBe('Rewritten since');
    expect(sinceNote('1234567ffff', null)).toBe('Reading what changed since your review of 1234567…');
    expect(sinceNote('1234567ffff', since(1, 2))).toBe('1 commit since your review of 1234567.');
    expect(sinceNote('1234567ffff', since(2, 0))).toBe('2 commits since your review of 1234567, which changed no file.');
    expect(sinceNote('1234567ffff', since(0, 0, false))).toMatch(/^The branch was rebased or force-pushed since your review of 1234567/);
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
    // A group that lists no members has every account that can start, as a start picks from it.
    const everyone = { ...reviewers, members: [] };
    expect(groupReviewWords(everyone, accounts, 'no-terminal', 'without a terminal')).toBe('Routing group reviewers (round-robin) picks among its accounts that can start a review without a terminal: cursor and antigravity can’t, so only claude and codex review.');
    expect(groupReviewWords(everyone, [cursor, agy], 'no-terminal', 'without a terminal')).toBe('Routing group reviewers (round-robin) picks among its accounts that can start a review without a terminal, and none of them can: pick another reviewer, or change where reviews run.');
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

  it('asks on the model and effort picked with the account, and on the account’s own for Auto’s', () => {
    expect(askOn(all, [], 0, { account: 'codex', model: 'gpt-6-luna', effort: 'high' })).toEqual({ account: 'codex', model: 'gpt-6-luna', effort: 'high' });
    expect(askOn(all, [], 0, null)).toEqual({ account: 'claude', model: null, effort: null });
    // An account that can't answer gives way to Auto's, without the model picked for it.
    expect(askOn(all, [], 0, { account: 'oc', model: 'big-pickle', effort: null })).toEqual({ account: 'claude', model: null, effort: null });
    expect(askOn([opencode], [], 0, null)).toBeNull();
  });
});

import { describe, expect, it } from 'vitest';
import { askAccount, askAccounts, autoReviewWords, reviewLane, reviewPicks, reviewPrompt } from './review';
import * as fx from './testing/fixtures';
import type { AutoReview, Lane, StartAccount } from './types';

describe('autoReviewWords', () => {
  const r = (stage: AutoReview['stage'], more: Partial<AutoReview> = {}): AutoReview => ({ stage, head: '309811aaaabbbb', at: 0, rounds: 1, ...more });
  it('says where the latest round stands, by its commit', () => {
    expect(autoReviewWords(r('reviewing'))).toEqual({ icon: 'eye', says: 'Agent reviewing 309811a', note: null });
    expect(autoReviewWords(r('posted', { comment: 'https://github.com/o/r/pull/7#issuecomment-1' })).says).toBe('Agent reviewed 309811a and commented');
    expect(autoReviewWords(r('skipped', { note: 'The diff is generated code.' }))).toEqual({ icon: 'circle-dashed', says: 'Agent passed on 309811a', note: 'The diff is generated code.' });
    expect(autoReviewWords(r('failed', { note: 'Its session is gone.' })).says).toBe('Agent’s review of 309811a didn’t finish');
    expect(autoReviewWords(r('drafted'))).toEqual({ icon: 'note', says: 'Agent drafted a review of 309811a', note: 'Only you see it until you submit it.' });
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
  it('lists Auto, then each account that can start, where it starts', () => {
    expect(reviewPicks(ACCOUNTS, [], 0, true)).toEqual([
      { kind: 'start', label: 'Auto (claude) in Ghostty', account: 'claude', place: 'terminal' },
      { kind: 'start', label: 'claude in Ghostty', account: 'claude', place: 'terminal' },
      { kind: 'codex-app', label: 'codex in the Codex app' },
    ]);
  });
  it('goes where the account last started', () => {
    expect(reviewPicks(ACCOUNTS, [], 0, true, { claude: 'background' })[1]).toEqual({ kind: 'start', label: 'claude in the background', account: 'claude', place: 'background' });
  });
  it('without a checkout offers only the Codex app’s review page', () => {
    expect(reviewPicks(ACCOUNTS, [], 0, false)).toEqual([{ kind: 'codex-app', label: 'codex in the Codex app' }]);
  });
});

describe('reviewPrompt', () => {
  it('names the PR and asks for a review that changes nothing', () => {
    const text = reviewPrompt({ number: 12, repoSlug: 'acme/app', url: 'https://github.com/acme/app/pull/12', title: 'Fix it' }, true);
    expect(text).toContain('#12 in acme/app');
    expect(text).toContain('gh pr diff 12 -R acme/app');
    expect(text).toContain('has its branch checked out');
    expect(text).toContain('Don’t change files');
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

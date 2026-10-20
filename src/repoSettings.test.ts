import { describe, expect, it } from 'vitest';
import { mineOf, nudgeAboutFor, offerFor, offerKey, offerWords, pullRequestsFor, repoChoices, repoKey, reviewsFor, sourceOf, teamInstructions, withOwn, withPullRequests, withRepo, withReviews, without } from './repoSettings';
import * as fx from './testing/fixtures';
import type { RepoCfg, TeamCfg, TeamFile } from './types';

const team: TeamCfg = {
  reviewInstructions: 'Check migrations.',
  ownReviewInstructions: '',
  neverSubmit: ['approve'],
  nudgeAbout: 'feedback',
  suggests: { reviews: { auto: true, post: 'comment' }, pull_requests: { nudge_by_default: true } },
};
const file = (over: Partial<TeamFile> = {}): TeamFile => ({ repo: 'acme/web', branch: 'main', default: true, commit: '1a2b3c4d', settings: team, skipped: [], offers: { reviews: { auto: true } }, ...over });

describe('reviewsFor and pullRequestsFor', () => {
  it('are yours, then your table for the repo; the team only says what a nudge tells about', () => {
    const global = { auto: false, account: 'claude', post: 'draft' as const, submit: 'approve' as const };
    expect(reviewsFor(global, undefined)).toMatchObject({ auto: false, account: 'claude', post: 'draft', submit: 'approve', own: { auto: false, send: true, drafts: false } });
    const mine: RepoCfg = { reviews: { auto: true, account: ' codex ', own: { send: false, drafts: true } }, pull_requests: { nudge_by_default: true } };
    expect(reviewsFor(global, mine)).toMatchObject({ auto: true, account: 'codex', post: 'draft', submit: 'approve', own: { send: false, drafts: true } });
    expect(pullRequestsFor({ nudge_by_default: false }, team, undefined)).toEqual({ nudge_by_default: false, nudge_about: 'feedback' });
    expect(pullRequestsFor({ nudge_by_default: false }, team, mine)).toEqual({ nudge_by_default: true, nudge_about: 'feedback' });
    expect(pullRequestsFor(undefined, team, { pull_requests: { nudge_about: 'everything' } }).nudge_about).toBe('everything');
  });

  it('say what a PR’s nudge tells about, by its base branch’s team file', () => {
    const board = { nudgeAbout: 'everything' as const, teamFiles: [file()] };
    expect(nudgeAboutFor(board, { repoSlug: 'acme/web', base: 'main' })).toBe('feedback');
    expect(nudgeAboutFor(board, { repoSlug: 'acme/web', base: 'release' })).toBe('everything');
    expect(nudgeAboutFor(board, { repoSlug: 'acme/api', base: 'main' })).toBe('everything');
    expect(nudgeAboutFor({ ...board, repos: { [repoKey('acme/web')]: { pull_requests: { nudge_about: 'feedback-and-checks' } } } }, { repoSlug: 'acme/web', base: 'main' })).toBe('feedback-and-checks');
  });
});

describe('sourceOf', () => {
  it('says whether a value is yours for the repo, the team’s, or as it is everywhere', () => {
    const mine: RepoCfg = { reviews: { model: 'opus', own: { send: false, drafts: true } } };
    expect(sourceOf('reviews.reviewer', mine, team)).toBe('repo');
    expect(sourceOf('reviews.auto', mine, team)).toBe('everywhere');
    expect(sourceOf('own.send', mine, team)).toBe('repo');
    expect(sourceOf('own.drafts', mine, team)).toBe('repo');
    expect(sourceOf('own.drafts', undefined, team)).toBe('everywhere');
    expect(sourceOf('prs.nudge_about', mine, team)).toBe('team');
    expect(sourceOf('prs.nudge_about', { pull_requests: { nudge_about: 'everything' } }, team)).toBe('repo');
    expect(sourceOf('prs.nudge_about', undefined, undefined)).toBe('everywhere');
  });
});

describe('a repo’s table', () => {
  it('gets what a field sets, and loses it on Reset, gone once it has nothing', () => {
    let mine = withReviews(undefined, { post: 'none' });
    expect(mine).toEqual({ reviews: { post: 'none' } });
    mine = withOwn(mine, { auto: true });
    mine = withPullRequests(mine, { nudge_about: 'feedback' });
    expect(mine).toEqual({ reviews: { post: 'none', own: { auto: true } }, pull_requests: { nudge_about: 'feedback' } });
    mine = withReviews(mine, { account: 'codex', model: 'opus', effort: 'xhigh' });
    expect(reviewsFor(undefined, mine).effort).toBe('xhigh');
    expect(sourceOf('reviews.reviewer', { reviews: { effort: 'high' } }, team)).toBe('repo');
    mine = without(mine, 'reviews.reviewer');
    expect(mine?.reviews).toEqual({ post: 'none', own: { auto: true } });
    mine = without(without(without(mine, 'reviews.post'), 'own.auto'), 'prs.nudge_about');
    expect(mine).toBeUndefined();
    const repos = withRepo({ [repoKey('o/r')]: { reviews: { auto: true } } }, 'acme/web', { reviews: { post: 'none' } });
    expect(Object.keys(repos)).toEqual(['github.com/o/r', 'github.com/acme/web']);
    expect(mineOf(withRepo(repos, 'o/r', undefined), 'o/r')).toBeUndefined();
  });
});

describe('repoChoices', () => {
  it('lists the repos here, the ones you have a table for and the ones with a team file, by project', () => {
    const lanes = [fx.lane({ repoSlug: 'acme/web', project: 'Acme' }), fx.lane({ repoSlug: 'acme/web', project: 'Acme' }), fx.lane({ repoSlug: 'zed/app', project: 'Zed' }), fx.lane({ repoSlug: 'peer/x', project: 'Acme', device: { id: 'd', name: 'Studio' } as never })];
    const choices = repoChoices(lanes, { [repoKey('old/lib')]: { reviews: { auto: true } } }, [file({ repo: 'zed/app' })]);
    expect(choices).toEqual([
      { repo: 'acme/web', project: 'Acme', team: false, mine: false },
      { repo: 'zed/app', project: 'Zed', team: true, mine: false },
      { repo: 'old/lib', project: '', team: false, mine: true },
    ]);
  });
});

describe('the team’s offer', () => {
  it('says what Use them would change', () => {
    expect(offerWords({ reviews: { auto: true, post: 'comment', own: { auto: true, send: false } }, pull_requests: { nudge_by_default: true } })).toEqual([
      'an agent reviews each pull request that waits on your review',
      'each review posted as a comment on the pull request',
      'an agent on another model reviews the pull requests your sessions open',
      'those reviews kept for you to send',
      'the session that opened a pull request told what happens on it',
    ]);
    expect(offerWords({})).toEqual([]);
  });

  it('shows for a PR into the file’s branch until it’s dismissed, and again once the team offers something else', () => {
    const files = [file(), file({ branch: 'release', default: false, offers: {} })];
    const pr = { repoSlug: 'acme/web', base: 'main' };
    expect(offerFor(files, pr, [])?.branch).toBe('main');
    expect(offerFor(files, { ...pr, base: 'release' }, [])).toBeUndefined();
    expect(offerFor(files, pr, [offerKey(files[0])])).toBeUndefined();
    const more = [file({ offers: { reviews: { auto: true }, pull_requests: { nudge_by_default: true } } })];
    expect(offerFor(more, pr, [offerKey(files[0])])?.repo).toBe('acme/web');
    expect(offerFor([file({ settings: null, error: 'broken' })], pr, [])).toBeUndefined();
  });

  it('says whether the team adds words to a PR’s reviews', () => {
    expect(teamInstructions([file()], { repoSlug: 'acme/web', base: 'main' })).toBe('Check migrations.');
    expect(teamInstructions([file()], { repoSlug: 'acme/web', base: 'main', mine: true })).toBe('');
    expect(teamInstructions([file()], { repoSlug: 'acme/web', base: 'other' })).toBe('');
  });
});

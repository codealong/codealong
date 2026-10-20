// Settings for one repo's pull requests: yours everywhere (`[reviews]`, `[pull_requests]`), then
// what the repo's team sets for everyone in its `.codealong/config.toml` (src-tauri/src/teamfile.rs),
// then yours for that repo (`[repos."github.com/owner/name"]`). Mirrors config.rs `reviews_for`
// and `pull_requests_for`, for Settings › Automation, which says where each value comes from, and
// for the windows that say how a PR's review or nudge goes. What the team only suggests goes for
// nobody until they take it (Use them), which writes it into their table for the repo.

import type { Lane, NudgeAbout, OwnReviewsCfg, PullRequestsCfg, RepoCfg, ReviewPost, ReviewsCfg, TeamCfg, TeamFile } from './types';

export const OWN_REVIEWS_OFF: Required<OwnReviewsCfg> = { auto: false, account: '', model: '', effort: '', prompt: '', send: true, drafts: false, other_harness: true };
export const REVIEWS_OFF: Required<ReviewsCfg> = { auto: false, account: '', model: '', effort: '', prompt: '', start: 'no-terminal', post: 'draft', submit: 'never', other_harness: true, own: OWN_REVIEWS_OFF };
export const PULL_REQUESTS_OFF: Required<PullRequestsCfg> = { nudge_by_default: false, nudge_about: 'everything' };

/** `[reviews]` with every value filled in, its own table too. */
export type Reviews = Required<ReviewsCfg> & { own: Required<OwnReviewsCfg> };

/** The key a repo's table has in `[repos]` (config.rs `repo_key`). */
export function repoKey(slug: string): string {
  return `github.com/${slug}`;
}

/** Your table for repo `slug`. */
export function mineOf(repos: Record<string, RepoCfg> | undefined, slug: string): RepoCfg | undefined {
  return repos?.[repoKey(slug)];
}

/** `[reviews]` as config.toml has it, every value filled in. */
export function reviewsEverywhere(global: ReviewsCfg | undefined): Reviews {
  return { ...REVIEWS_OFF, ...global, own: { ...OWN_REVIEWS_OFF, ...global?.own } };
}

const given = <T>(v: T | undefined | null): v is T => v !== undefined && v !== null;

/** `[reviews]` as it goes for a repo: yours, then your table for it. The team adds words and keeps
 *  verdicts from being submitted, which the engine does; neither is a value here. Where reviews
 *  start goes for every repo. */
export function reviewsFor(global: ReviewsCfg | undefined, mine: RepoCfg | undefined): Reviews {
  const g = reviewsEverywhere(global);
  const m = mine?.reviews ?? {};
  const o = m.own ?? {};
  return {
    auto: m.auto ?? g.auto,
    account: given(m.account) ? m.account.trim() : g.account,
    model: given(m.model) ? m.model.trim() : g.model,
    effort: given(m.effort) ? m.effort.trim() : g.effort,
    prompt: given(m.prompt) ? m.prompt.trim() : g.prompt,
    start: g.start,
    post: m.post ?? g.post,
    submit: m.submit ?? g.submit,
    other_harness: m.other_harness ?? g.other_harness,
    own: {
      auto: o.auto ?? g.own.auto,
      account: given(o.account) ? o.account.trim() : g.own.account,
      model: given(o.model) ? o.model.trim() : g.own.model,
      effort: given(o.effort) ? o.effort.trim() : g.own.effort,
      prompt: given(o.prompt) ? o.prompt.trim() : g.own.prompt,
      send: o.send ?? g.own.send,
      drafts: o.drafts ?? g.own.drafts,
      other_harness: o.other_harness ?? g.own.other_harness,
    },
  };
}

/** `[reviews]` as it goes for repo `slug`'s pull requests, from the board: what Review with…
 *  starts first. */
export function repoReviews(board: { reviews?: ReviewsCfg; repos?: Record<string, RepoCfg> } | null | undefined, slug: string): Reviews {
  return reviewsFor(board?.reviews, mineOf(board?.repos, slug));
}

/** `[pull_requests]` as it goes for a repo: yours, then what its team says a nudge tells about,
 *  then your table for it. */
export function pullRequestsFor(global: PullRequestsCfg | undefined, team: TeamCfg | null | undefined, mine: RepoCfg | undefined): Required<PullRequestsCfg> {
  const g = { ...PULL_REQUESTS_OFF, ...global };
  const m = mine?.pull_requests ?? {};
  return { nudge_by_default: m.nudge_by_default ?? g.nudge_by_default, nudge_about: m.nudge_about ?? team?.nudgeAbout ?? g.nudge_about };
}

/** The team file read from a repo's `branch`, or its default branch's, which Settings shows. */
export function teamFileOf(files: readonly TeamFile[] | undefined, repo: string, branch?: string | null): TeamFile | undefined {
  return files?.find((f) => f.repo === repo && (branch ? f.branch === branch : f.default));
}

/** What a nudge on a PR tells about, as its repo says (`nudgeAbout` is `[pull_requests]`'s). */
export function nudgeAboutFor(board: { nudgeAbout?: NudgeAbout; repos?: Record<string, RepoCfg>; teamFiles?: TeamFile[] } | null | undefined, pr: { repoSlug: string; base?: string | null }): NudgeAbout {
  const team = teamFileOf(board?.teamFiles, pr.repoSlug, pr.base)?.settings;
  return pullRequestsFor({ nudge_by_default: false, nudge_about: board?.nudgeAbout }, team, mineOf(board?.repos, pr.repoSlug)).nudge_about;
}

/** A setting Settings shows for a repo, by where it is in `[reviews]` and `[pull_requests]`. */
export type Field = 'reviews.auto' | 'reviews.reviewer' | 'reviews.other_harness' | 'reviews.prompt' | 'reviews.post' | 'reviews.submit' | 'own.auto' | 'own.reviewer' | 'own.other_harness' | 'own.prompt' | 'own.send' | 'own.drafts' | 'prs.nudge_by_default' | 'prs.nudge_about';

/** Where a value for a repo comes from: your table for it, its team's file, or `[reviews]` and
 *  `[pull_requests]` as they go everywhere. */
export type Source = 'repo' | 'team' | 'everywhere';

/** Where `field`'s value comes from for a repo with your table `mine` and its team's `team`. */
export function sourceOf(field: Field, mine: RepoCfg | undefined, team: TeamCfg | null | undefined): Source {
  const r = mine?.reviews ?? {};
  const o = r.own ?? {};
  const p = mine?.pull_requests ?? {};
  const set: Record<Field, unknown[]> = {
    'reviews.auto': [r.auto],
    'reviews.reviewer': [r.account, r.model, r.effort],
    'reviews.other_harness': [r.other_harness],
    'reviews.prompt': [r.prompt],
    'reviews.post': [r.post],
    'reviews.submit': [r.submit],
    'own.auto': [o.auto],
    'own.reviewer': [o.account, o.model, o.effort],
    'own.other_harness': [o.other_harness],
    'own.prompt': [o.prompt],
    'own.send': [o.send],
    'own.drafts': [o.drafts],
    'prs.nudge_by_default': [p.nudge_by_default],
    'prs.nudge_about': [p.nudge_about],
  };
  if (set[field].some(given)) return 'repo';
  if (field === 'prs.nudge_about' && given(team?.nudgeAbout)) return 'team';
  return 'everywhere';
}

/** Takes nulls and empty tables out of a repo's table, as config.toml would have it. */
function pruned(t: RepoCfg): RepoCfg | undefined {
  const keep = <T extends object>(o: T | undefined): T | undefined => {
    if (!o) return undefined;
    const out = Object.fromEntries(Object.entries(o).filter(([, v]) => given(v))) as T;
    return Object.keys(out).length ? out : undefined;
  };
  const own = keep(t.reviews?.own);
  const reviews = keep({ ...t.reviews, own });
  const prs = keep(t.pull_requests);
  const out: RepoCfg = { ...(reviews ? { reviews } : {}), ...(prs ? { pull_requests: prs } : {}) };
  return Object.keys(out).length ? out : undefined;
}

/** Your table for a repo with `next` set into its `[reviews]` (each key given is set, the rest
 *  stay), as a field changed with the repo picked saves it. Undefined once it has nothing. */
export function withReviews(mine: RepoCfg | undefined, next: Partial<Omit<ReviewsCfg, 'own'>>): RepoCfg | undefined {
  return pruned({ ...mine, reviews: { ...mine?.reviews, ...next } });
}

/** The same for its `[reviews.own]`. */
export function withOwn(mine: RepoCfg | undefined, next: Partial<OwnReviewsCfg>): RepoCfg | undefined {
  return pruned({ ...mine, reviews: { ...mine?.reviews, own: { ...mine?.reviews?.own, ...next } } });
}

/** The same for its `[pull_requests]`. */
export function withPullRequests(mine: RepoCfg | undefined, next: Partial<PullRequestsCfg>): RepoCfg | undefined {
  return pruned({ ...mine, pull_requests: { ...mine?.pull_requests, ...next } });
}

/** Your table for a repo without `field`, so it goes as the team or `[reviews]` says (Reset). */
export function without(mine: RepoCfg | undefined, field: Field): RepoCfg | undefined {
  const gone = (keys: string[]) => Object.fromEntries(keys.map((k) => [k, undefined]));
  const [table, key] = field.split('.') as ['reviews' | 'own' | 'prs', string];
  const keys = key === 'reviewer' ? ['account', 'model', 'effort'] : [key];
  if (table === 'reviews') return withReviews(mine, gone(keys));
  if (table === 'own') return withOwn(mine, gone(keys));
  return withPullRequests(mine, gone(keys));
}

/** `repos` with repo `slug`'s table set to `table`, or taken out for none. */
export function withRepo(repos: Record<string, RepoCfg> | undefined, slug: string, table: RepoCfg | undefined): Record<string, RepoCfg> {
  const out = { ...repos };
  if (table) out[repoKey(slug)] = table;
  else delete out[repoKey(slug)];
  return out;
}

/** A repo Settings › Automation can show: one checked out here, one you have a table for, or one
 *  whose team file was read. */
export interface RepoChoice {
  repo: string;
  /** The project it's in here; empty for none. */
  project: string;
  /** Its default branch has a team file. */
  team: boolean;
  /** You have a table for it. */
  mine: boolean;
}

/** The repos Settings can show, by project, then name. */
export function repoChoices(lanes: readonly Lane[], repos: Record<string, RepoCfg> | undefined, files: readonly TeamFile[] | undefined): RepoChoice[] {
  const projects = new Map<string, string>();
  for (const l of lanes) if (!l.device && l.repoSlug && !projects.has(l.repoSlug)) projects.set(l.repoSlug, l.project);
  const slugs = new Set<string>([...projects.keys(), ...Object.keys(repos ?? {}).map((k) => k.replace(/^github\.com\//, '')), ...(files ?? []).map((f) => f.repo)]);
  return [...slugs]
    .map((repo) => ({ repo, project: projects.get(repo) ?? '', team: !!teamFileOf(files, repo), mine: !!mineOf(repos, repo) }))
    .sort((a, b) => (a.project || '￿').localeCompare(b.project || '￿') || a.repo.localeCompare(b.repo));
}

const POSTED: Record<ReviewPost, string> = {
  draft: 'each review left as a draft review for you to finish',
  comment: 'each review posted as a comment on the pull request',
  none: 'each review kept in Codealong',
};

/** What a team offers you, in words: each thing Use them would change. */
export function offerWords(offers: RepoCfg | undefined): string[] {
  const r = offers?.reviews ?? {};
  const o = r.own ?? {};
  const p = offers?.pull_requests ?? {};
  const out: string[] = [];
  if (given(r.auto)) out.push(r.auto ? 'an agent reviews each pull request that waits on your review' : 'no agent reviews pull requests that wait on your review');
  if (given(r.post)) out.push(POSTED[r.post]);
  if (given(o.auto)) out.push(o.auto ? 'an agent on another model reviews the pull requests your sessions open' : 'no agent reviews the pull requests your sessions open');
  if (given(o.send)) out.push(o.send ? 'each of those reviews handed to the session that opened the pull request' : 'those reviews kept for you to send');
  if (given(p.nudge_by_default)) out.push(p.nudge_by_default ? 'the session that opened a pull request told what happens on it' : 'no session told what happens on its pull request unless you turn it on');
  return out;
}

/** What identifies an offer, for remembering it was dismissed: the repo, the branch and what's
 *  offered. A team that comes to offer something else asks again. */
export function offerKey(file: Pick<TeamFile, 'repo' | 'branch' | 'offers'>): string {
  return `${file.repo}@${file.branch}:${offerWords(file.offers).join('; ')}`;
}

/** The team file whose offer shows for a PR: its base branch's, with something to offer that
 *  hasn't been dismissed. */
export function offerFor(files: readonly TeamFile[] | undefined, pr: { repoSlug: string; base?: string | null }, dismissed: readonly string[]): TeamFile | undefined {
  const f = teamFileOf(files, pr.repoSlug, pr.base);
  if (!f?.settings || offerWords(f.offers).length === 0) return undefined;
  return dismissed.includes(offerKey(f)) ? undefined : f;
}

/** Whether the team adds words to what reviewers of a PR are told: for one of yours, the ones
 *  for your own pull requests. */
export function teamInstructions(files: readonly TeamFile[] | undefined, pr: { repoSlug: string; base?: string | null; mine?: boolean }): string {
  const t = teamFileOf(files, pr.repoSlug, pr.base)?.settings;
  return (pr.mine ? t?.ownReviewInstructions : t?.reviewInstructions)?.trim() ?? '';
}

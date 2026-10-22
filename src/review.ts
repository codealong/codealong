// Review with…: Codealong's own review of a PR (autoreview.rs), started by hand on the account
// picked: the same round `[reviews] auto` starts, the PR written out for the reviewer, told what
// Settings says, its review posted and submitted as Settings says, and shown on the PR's card and
// in its Reviews tab (`review_pr_now`). Codex's app has a review page of its own (`codex://review`),
// offered beside it. And the account that answers questions about a PR in its card's box (Ask,
// PrCard.tsx): the one picked there, else Auto's.

import { reviewAgain, reviewInCodex, reviewNow } from './api';
import type { MenuAction } from './contextMenu';
import { agoPhrase, count } from './format';
import { FRESH_SIDES } from './harness';
import { commentsWord, submittedWord } from './prDraft';
import { reviewedBefore, yourReview, yourReviewSaid } from './prStatus';
import { errorText, type Report } from './report';
import { autoPick, isGroup, pickFromGroup } from './start';
import type { AutoReview, Draft, Lane, LastReview, PrItem, ReviewsCfg, ReviewStart, RoutingGroup, Since, StartAccount, StartPlace, Usage, Verdict } from './types';

/** One entry of the Review with menu: a round on `account`, or on the reviewer Settings names for
 *  none; or Codex's own review page. */
export type ReviewPick = { kind: 'codex-app'; label: string } | { kind: 'review'; label: string; account: string | null };

/** Where a review's session may be, as `[reviews] start` says, first choice first, for a harness
 *  with a background service of its own (`service`) or not: autoreview.rs `review_places`. */
export function reviewPlaces(start: ReviewStart, service: boolean): StartPlace[] {
  if (start === 'terminal') return ['terminal'];
  if (start === 'codealong') return ['codealong'];
  if (service) return ['background', 'codealong'];
  return start === 'background' ? ['codealong', 'background'] : ['codealong'];
}

/** Where an account's reviews would run, as autoreview.rs `review_place` picks for `[reviews]
 *  start`: never in a terminal by default, under its harness's background service, else in
 *  Codealong. Null for an account that can't start one there, which doesn't review. A Background
 *  that's a terminal behind your windows says so (`behind`), which is also how a harness without
 *  a service shows: where Background isn't offered, which it lacks makes no difference. */
export function reviewsRunIn(a: StartAccount, start: ReviewStart = 'no-terminal'): string | null {
  const background = a.places.find((p) => p.place === 'background');
  for (const place of reviewPlaces(start, !!background && !background.behind)) {
    const offered = a.places.find((p) => p.place === place);
    if (!offered) continue;
    if (place === 'codealong') return 'in Codealong';
    if (place === 'terminal') return `in ${offered.label}`;
    return offered.behind ? 'in a terminal behind your windows' : 'in the background';
  }
  return null;
}

/** "a", "a and b", "a, b and c". */
function listed(names: string[]): string {
  return names.length < 2 ? names.join('') : `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
}

/** What a routing group named as the reviewer reviews with, as Settings says it (`where`, how
 *  `start` reads): it picks among its accounts that can start a review there (`reviewsRunIn`),
 *  passing over the rest without a word, so the ones that can't are named, and the ones left. A
 *  group that lists no members has every account that can start (start.ts `pickFromGroup`). */
export function groupReviewWords(group: RoutingGroup, accounts: StartAccount[], start: ReviewStart, where: string): string {
  const members = group.members.length ? group.members.map((m) => m.account) : accounts.filter((a) => a.places.length > 0).map((a) => a.name);
  const names = [...new Set(members)];
  const can = names.filter((n) => accounts.some((a) => a.name === n && reviewsRunIn(a, start)));
  const cant = names.filter((n) => !can.includes(n));
  const picks = `Routing group ${group.name} (${group.strategy}) picks among its accounts that can start a review ${where}`;
  if (!cant.length) return `${picks}.`;
  if (!can.length) return `${picks}, and none of them can: pick another reviewer, or change where reviews run.`;
  return `${picks}: ${listed(cant)} can’t, so only ${listed(can)} review${can.length === 1 ? 's' : ''}.`;
}

/** Whether this Mac has a checkout a review of the PR can start in, as side.rs `pr_lane` picks
 *  one: a worktree that holds it, else the repo's main checkout, else any worktree of the repo.
 *  Null when it has none. */
export function reviewLane(pr: Pick<PrItem, 'repoSlug' | 'lanes'>, lanes: Lane[]): { lane: Lane; checkedOut: boolean } | null {
  const here = lanes.filter((l) => !l.id.startsWith('@'));
  const holding = here.find((l) => pr.lanes.includes(l.id));
  if (holding) return { lane: holding, checkedOut: true };
  const same = here.filter((l) => l.repoSlug === pr.repoSlug || l.repoSlugNow === pr.repoSlug);
  const lane = same.find((l) => l.origin === 'main') ?? same[0];
  return lane ? { lane, checkedOut: false } : null;
}

/** The reviewer Settings names for a PR (autoreview.rs `seat_for`): `[reviews.own]`'s for a PR of
 *  yours, whose Auto keeps off the harness of the session that opened it, else `[reviews]`'s. */
function settingsReviewer(reviews: ReviewsCfg | null | undefined, mine: boolean): { account: string; model: string; avoids: boolean } {
  const r = mine ? reviews?.own : reviews;
  const account = r?.account?.trim() ?? '';
  const otherHarness = r?.other_harness ?? mine;
  return { account, model: r?.model?.trim() ?? '', avoids: otherHarness && !account };
}

/**
 * The menu's entries, each a round of Codealong's own reviews (`review_pr_now`): first the
 * reviewer Settings names, else Auto's (the quick panel's pick among the accounts that can review,
 * another harness than the opener's for a PR of yours), when there's a choice; then each account
 * that can start one where Settings says reviews start (`reviewsRunIn`). They need a checkout of
 * the repo (`hasLane`); Codex's own review page, last, doesn't.
 */
export function reviewPicks(accounts: StartAccount[], usage: Usage[], now: number, hasLane: boolean, reviews?: ReviewsCfg | null, mine = false, groups: RoutingGroup[] = [], lanes: Lane[] = []): ReviewPick[] {
  const start = reviews?.start ?? 'no-terminal';
  const runsIn = (a: StartAccount) => reviewsRunIn(a, start);
  const can = hasLane ? accounts.filter((a) => runsIn(a)) : [];
  const each: ReviewPick[] = can.map((a) => ({ kind: 'review', label: `${a.name} ${runsIn(a)}`, account: a.name }));
  const named = settingsReviewer(reviews, mine);
  const set = named.account ? can.find((a) => a.name === named.account) : undefined;
  const group = named.account ? isGroup(named.account, groups) : undefined;
  const groupPick = group ? pickFromGroup(group, can, usage, now, named.model || null, null, lanes) : null;
  const groupAccount = groupPick ? can.find((a) => a.name === groupPick.account) : undefined;
  const pick = named.account || named.avoids ? null : autoPick(can, usage, now, null, null, lanes);
  const picked = pick ? can.find((a) => a.name === pick.account) : undefined;
  const first: ReviewPick | null = set
    ? { kind: 'review', label: `${set.name}${named.model ? `, ${named.model}` : ''} ${runsIn(set)}, as Settings says`, account: null }
    : group && groupAccount
      ? { kind: 'review', label: `${group.name} (${groupAccount.name}) ${runsIn(groupAccount)}, as Settings says`, account: null }
      : named.avoids && can.length > 1
        ? { kind: 'review', label: 'Auto (another harness than its session’s)', account: null }
        : picked && can.length > 1
          ? { kind: 'review', label: `Auto (${picked.name}) ${runsIn(picked)}`, account: null }
          : null;
  const app: ReviewPick[] = accounts.some((a) => a.places.some((p) => p.place === 'codex-app')) ? [{ kind: 'codex-app', label: 'The Codex app’s own review' }] : [];
  return [...(first ? [first] : []), ...each, ...app];
}

/**
 * Review with…'s items, as every window builds them (`reviewPicks` says which): each starts a
 * round of Codealong's own reviews of the PR now (autoreview.rs `review_now`), whatever `[reviews]
 * auto` says, its progress and its review on the PR's card and in its Reviews tab, posted and
 * submitted as Settings says; or opens the PR's review page in the Codex app. A PR the board
 * doesn't list (no `key`) has only that. How it went is reported the window's way (a start under
 * way through `report.busy`, where the window shows one), and `started` hears that the review is
 * under way, where the window goes on to the next thing that waits on you.
 */
export function reviewItems(pr: Pick<PrItem, 'number' | 'url' | 'repoSlug' | 'lanes'> & Partial<Pick<PrItem, 'key' | 'mine'>>, lanes: Lane[], accounts: StartAccount[], usage: Usage[], report: Report, started?: () => void, now = Date.now(), reviews?: ReviewsCfg | null, groups: RoutingGroup[] = []): MenuAction[] {
  const key = pr.key;
  const picks = reviewPicks(accounts, usage, now, !!key && !!reviewLane(pr, lanes), reviews, !!pr.mine, groups, lanes);
  return picks.map((pick) => ({
    label: pick.label,
    run: () => {
      if (pick.kind === 'codex-app' || !key) {
        reviewInCodex(pr.url).then(
          () => started?.(),
          (e: unknown) => report.error(errorText(e)),
        );
        return;
      }
      startReview({ key, number: pr.number }, pick.account, report, started);
    },
  }));
}

/** Starts a round of Codealong's own reviews of a PR now (autoreview.rs `review_now`), on
 *  `account`, else on the reviewer Settings names, and says how it went the window's way, as
 *  Review with…'s items do. */
export function startReview(pr: { key: string; number: number }, account: string | null, report: Report, started?: () => void): void {
  const work = reviewNow(pr.key, account).then(() => `Reviewing #${pr.number}: its card says how it goes.`);
  sayHowItGoes(work, `Starting a review of #${pr.number}…`, report, started);
}

/** Tries a PR's agent review that didn't get done again (autoreview.rs `again`), as Try again on
 *  its line over the box, in its Reviews tab and on its notice does: posts a review its agent
 *  wrote that wasn't posted, else reviews it again. Says what the engine did, the window's way. */
export function tryAgain(pr: { key: string; number: number }, report: Report, started?: () => void): void {
  sayHowItGoes(reviewAgain(pr.key), `Trying #${pr.number}’s review again…`, report, started);
}

/** Says how a review asked for goes, the window's way: under way through `report.busy`, where the
 *  window shows one, then what was done, which `started` hears too. */
function sayHowItGoes(work: Promise<string>, busy: string, report: Report, started?: () => void): void {
  (report.busy ? report.busy(busy, work) : work)
    .then((said) => {
      report.info(said);
      started?.();
    })
    .catch((e: unknown) => report.error(errorText(e)));
}

/** What Try again does for a PR whose agent review didn't get done (autoreview.rs `again`): posts
 *  the review its agent wrote, when that was of the commit the PR is at and only posting failed
 *  (`Reviewer::repost`); else reviews the PR again. Null while there's nothing to try again: its
 *  latest round got done or is under way, the PR is closed, or new commits have the next round
 *  coming by itself (`behind`). */
export function againFor(pr: Pick<PrItem, 'autoReview' | 'headOid' | 'state'>): 'post' | 'review' | null {
  const r = pr.autoReview;
  if (!r || r.stage !== 'failed' || r.behind || pr.state === 'merged' || pr.state === 'closed') return null;
  return r.head === pr.headOid && r.parts?.some((p) => p.state === 'done') ? 'post' : 'review';
}

/** What Try again says it does on hover, as `againFor` has it. */
export function againTitle(kind: 'post' | 'review'): string {
  return kind === 'post' ? 'Post the review the agent wrote, which didn’t go through' : 'Review it again: in the session that reviewed it before while that can go on, else a new one';
}

/** The accounts that can answer questions about a PR: those whose harness holds a conversation
 *  of its own (harness.ts `FRESH_SIDES`) and that can start here. */
export function askAccounts(accounts: StartAccount[]): StartAccount[] {
  return accounts.filter((a) => FRESH_SIDES.includes(a.harness) && a.places.length > 0);
}

/** The account a question about a PR goes to: `chosen` while it can answer, else the one Auto
 *  would start among those that can (start.ts `autoPick`), else the first. Null when none can. */
export function askAccount(accounts: StartAccount[], usage: Usage[], now: number, chosen: string | null): string | null {
  const can = askAccounts(accounts);
  if (chosen && can.some((a) => a.name === chosen)) return chosen;
  return autoPick(can, usage, now)?.account ?? can[0]?.name ?? null;
}

/** Who answers a question about a PR, as the agent menu beside Ask picked it (`chosen`, Auto's
 *  for none): the account `askAccount` says, on the model and effort picked with it, or the
 *  account's own when it's Auto's or the one picked can't answer. Null when none can. */
export function askOn(accounts: StartAccount[], usage: Usage[], now: number, chosen: { account: string; model: string | null; effort: string | null } | null): { account: string; model: string | null; effort: string | null } | null {
  const account = askAccount(accounts, usage, now, chosen?.account ?? null);
  if (!account) return null;
  return account === chosen?.account ? { account, model: chosen.model, effort: chosen.effort } : { account, model: null, effort: null };
}

/** What a PR's card says of Codealong's own review of it (autoreview.rs, `[reviews] auto`): the
 *  latest round in a few words with its icon, then why it didn't go as it would, and that new
 *  commits are reviewed again. `onGitHub`, once your review of the PR there is read (prDraft.ts):
 *  a draft that's no longer one was submitted, or thrown away. A review of a PR of yours
 *  (`[reviews.own]`) says where it went back to, by the session's title (`sentTo`), or why it
 *  hasn't, as the engine has it. */
export function autoReviewWords(r: AutoReview, onGitHub?: { draft: boolean; submitted: string | null }, sentTo?: string | null): { icon: string; says: string; note: string | null } {
  const at = r.head.slice(0, 7);
  // Another machine of yours reviews it (autoreview.rs `reviewer_of`), and says only how its
  // round stands, not how its session went.
  const on = r.machine ? ` on ${r.machine}` : '';
  const agent = r.machine ? `Agent on ${r.machine}` : 'Agent';
  const [icon, says] = {
    queued: ['hourglass', `Agent review of ${at} to come${on}`],
    reviewing: ['eye', `Agent reviewing ${at}${on}`],
    posted: ['message-circle', `Agent reviewed ${at}${on} and commented`],
    drafted: ['note', `Agent drafted a review of ${at}${on}`],
    kept: ['check', `Agent reviewed ${at}${on}`],
    skipped: ['circle-dashed', `${agent} passed on ${at}`],
    // One whose session never began (GitHub couldn't be read, no account could start it) didn't
    // start, rather than stop partway.
    failed: ['alert', `Agent’s review of ${at}${on} ${r.machine ? 'didn’t get done' : r.parts?.some((p) => p.session) ? 'didn’t finish' : 'didn’t start'}`],
  }[r.stage];
  const kept = r.stage === 'kept' && !r.own ? (r.machine ? `Its review is in its session on ${r.machine}: posting is off there.` : 'Its review is in its session: posting is off.') : null;
  const back = r.own && r.sent ? `It went back to ${sentTo ? `“${sentTo}”` : 'the session that opened the PR'}.` : null;
  const gone = r.stage === 'drafted' && onGitHub && !onGitHub.draft ? (onGitHub.submitted ? `You submitted it, ${submittedWord(onGitHub.submitted)}.` : 'It isn’t a draft on GitHub any more: it was submitted or thrown away.') : null;
  // A draft that shows below says so itself, and one submitted for you isn't a draft any more.
  const drafted = r.submitted ? null : (gone ?? (r.stage === 'drafted' && !onGitHub?.draft ? 'Only you see it until you submit it.' : null));
  const why = r.note ?? kept ?? back ?? drafted;
  const next = r.behind ? (r.stage === 'reviewing' ? 'New commits since: it reviews them once this one’s done.' : 'New commits since: it reviews them again once they settle.') : null;
  // An approval of an earlier commit says what's asked of you once the commits since are read.
  const before = r.approves && (r.behind || r.approves !== r.head) ? r.approves.slice(0, 7) : null;
  const held = r.holds ? (before ? `It approved ${before} before: the PR waits on your sign-off once the agent approves the commits since.` : 'It waits on your review once the agent’s is done.') : before && `It approved ${before} before.`;
  const note = [why, next, held].filter((x): x is string => !!x).join(' ');
  if (r.submitted) return { ...submittedWords(r.submitted, at, agent), note: note || null };
  return { icon: r.own && r.sent && r.stage === 'kept' ? 'send' : icon, says, note: note || null };
}

/** What an agent's review of commit `at` was submitted as yours as (`[reviews] submit`). */
function submittedWords(v: Verdict, at: string, agent = 'Agent'): { icon: string; says: string } {
  return {
    approve: { icon: 'check', says: `${agent} approved ${at} for you` },
    'request-changes': { icon: 'alert', says: `${agent} requested changes on ${at} for you` },
    comment: { icon: 'message-circle', says: `${agent} commented on ${at} for you` },
  }[v];
}

/** What the agent would do with a PR, as its windows suggest it. */
export function suggestWords(v: Verdict): string {
  return { approve: 'Suggests approving', 'request-changes': 'Suggests changes', comment: 'Suggests a comment' }[v];
}

/** The line over a PR's box (PrBox.tsx): where its review stands. Once you've reviewed it, your
 *  last review leads (prStatus.ts `yourReview`): of the commit the PR's at, it says so, and of an
 *  earlier one, `since` is that commit, whose changes since the line offers. The agent's round is
 *  then `agent`, for the line's title, and your draft review on GitHub follows yours. Without one,
 *  it's an agent's round and your draft together, else that there's none yet. `busy` while an
 *  agent reviews. `approved` is the commit the agent approved before the round the line is of
 *  (the round's own suggestion says the rest), where your review doesn't lead. */
export function reviewLineWords(pr: Pick<PrItem, 'autoReview' | 'mine' | 'lastReview' | 'headOid'>, draft: Draft | null): { icon: string; says: string; busy: boolean; approved: string | null; agent: string | null; since: string | null } {
  const r = pr.autoReview;
  const yours = draft ? `your draft review${draft.count > 0 ? `, ${commentsWord(draft.count)}` : ''}` : null;
  const agent = r ? autoReviewWords(r, draft ? { draft: true, submitted: null } : undefined) : null;
  const busy = r?.stage === 'reviewing';
  const mine = yourReview(pr);
  if (mine) {
    const { icon, says } = yourReviewSaid(mine.review);
    const at = mine.current ? `${says}, its latest commit` : says;
    return { icon, says: yours ? `${at} · ${yours}` : at, busy, approved: null, agent: agent?.says ?? null, since: reviewedBefore(pr) };
  }
  if (r && agent) {
    const approved = r.approves && r.approves !== r.head ? r.approves.slice(0, 7) : null;
    return { icon: agent.icon, says: yours && r.stage === 'drafted' ? `${agent.says}: ${yours}` : agent.says, busy, approved, agent: null, since: null };
  }
  if (yours) return { icon: 'note', says: yours[0].toUpperCase() + yours.slice(1), busy: false, approved: null, agent: null, since: null };
  return { icon: 'eye', says: pr.mine ? 'No agent review of your PR' : 'No review of yours yet', busy: false, approved: null, agent: null, since: null };
}

/** What Approve says on hover where you approved the commit the PR's at (prStatus.ts
 *  `yourApproval`): when, and that another press is another approval. */
export function approvedWords(r: LastReview, now: number): string {
  return `You approved ${r.commit?.slice(0, 7) ?? 'it'}, the commit it’s at${r.at != null ? `, ${agoPhrase(r.at, now)}` : ''}. Approving again adds another approval`;
}

/** What the pill beside the line over a PR's box says of the changes since your review of an
 *  earlier commit (`reviewLineWords`'s `since`), as GitHub's comparison has them (`Since`), and
 *  what a click on it shows. Before the comparison is read, only that there are new commits. */
export function sinceWords(commit: string, since: Since | null): { text: string; title: string } {
  const at = commit.slice(0, 7);
  if (since && !since.follows) return { text: 'Rewritten since', title: `The branch was rebased or force-pushed since your review of ${at}, so what changed since can’t be told apart: Changes shows all of it` };
  const text = since ? count(since.commits, 'new commit') : 'New commits';
  return { text, title: `Show what changed since your review of ${at}${since ? `: ${count(since.files.length, 'file')}${since.moreFiles ? ' or more' : ''}` : ''}, under Changes` };
}

/** What the Changes tab says under its heading while it shows what changed since your review of
 *  `commit` (PrFiles.tsx `SinceScope`): that it's being read, how many commits, or why it shows
 *  every change instead. */
export function sinceNote(commit: string, since: Since | null): string {
  const at = commit.slice(0, 7);
  if (!since) return `Reading what changed since your review of ${at}…`;
  if (!since.follows) return `The branch was rebased or force-pushed since your review of ${at}, so what changed since can’t be told apart: these are all its changes.`;
  const files = since.files.length === 0 ? ', which changed no file' : since.moreFiles ? `; the first ${since.files.length} files they changed` : '';
  return `${count(since.commits, 'commit')} since your review of ${at}${files}.`;
}

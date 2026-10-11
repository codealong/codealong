// Review with…: Codealong's own review of a PR (autoreview.rs), started by hand on the account
// picked: the same round `[reviews] auto` starts, the PR written out for the reviewer, told what
// Settings says, its review posted and submitted as Settings says, and shown on the PR's card and
// in its Review tab (`review_pr_now`). Codex's app has a review page of its own (`codex://review`),
// offered beside it. And the account that answers questions about a PR in its card's box (Ask,
// PrCard.tsx): the one picked there, else Auto's.

import { reviewInCodex, reviewNow } from './api';
import type { MenuAction } from './contextMenu';
import { FRESH_SIDES } from './harness';
import { commentsWord, submittedWord } from './prDraft';
import { errorText, type Report } from './report';
import { autoPick } from './start';
import type { AutoReview, Draft, Lane, PrItem, ReviewsCfg, StartAccount, Usage, Verdict } from './types';

/** One entry of the Review with menu: a round on `account`, or on the reviewer Settings names for
 *  none; or Codex's own review page. */
export type ReviewPick = { kind: 'codex-app'; label: string } | { kind: 'review'; label: string; account: string | null };

/** Where an account's reviews would run, as autoreview.rs `reviewer_place` picks: unseen under
 *  its harness's background service, else in Codealong, else in a terminal behind your windows.
 *  Null for an account that can start none of those, which can't review. */
export function reviewsRunIn(a: StartAccount): string | null {
  const background = a.places.find((p) => p.place === 'background');
  if (background && !background.behind) return 'in the background';
  if (a.places.some((p) => p.place === 'codealong')) return 'in Codealong';
  return background ? 'in a terminal behind your windows' : null;
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
  return { account, model: r?.model?.trim() ?? '', avoids: mine && !account };
}

/**
 * The menu's entries, each a round of Codealong's own reviews (`review_pr_now`): first the
 * reviewer Settings names, else Auto's (the quick panel's pick among the accounts that can review,
 * another harness than the opener's for a PR of yours), when there's a choice; then each account
 * that can start a session out of sight, which is where reviews run (`reviewsRunIn`). They need a
 * checkout of the repo (`hasLane`); Codex's own review page, last, doesn't.
 */
export function reviewPicks(accounts: StartAccount[], usage: Usage[], now: number, hasLane: boolean, reviews?: ReviewsCfg | null, mine = false): ReviewPick[] {
  const can = hasLane ? accounts.filter((a) => reviewsRunIn(a)) : [];
  const each: ReviewPick[] = can.map((a) => ({ kind: 'review', label: `${a.name} ${reviewsRunIn(a)}`, account: a.name }));
  const named = settingsReviewer(reviews, mine);
  const set = named.account ? can.find((a) => a.name === named.account) : undefined;
  const pick = named.account || named.avoids ? null : autoPick(can, usage, now);
  const picked = pick ? can.find((a) => a.name === pick.account) : undefined;
  const first: ReviewPick | null = set
    ? { kind: 'review', label: `${set.name}${named.model ? `, ${named.model}` : ''} ${reviewsRunIn(set)}, as Settings says`, account: null }
    : named.avoids && can.length > 1
      ? { kind: 'review', label: 'Auto (another harness than its session’s)', account: null }
      : picked && can.length > 1
        ? { kind: 'review', label: `Auto (${picked.name}) ${reviewsRunIn(picked)}`, account: null }
        : null;
  const app: ReviewPick[] = accounts.some((a) => a.places.some((p) => p.place === 'codex-app')) ? [{ kind: 'codex-app', label: 'The Codex app’s own review' }] : [];
  return [...(first ? [first] : []), ...each, ...app];
}

/**
 * Review with…'s items, as every window builds them (`reviewPicks` says which): each starts a
 * round of Codealong's own reviews of the PR now (autoreview.rs `review_now`), whatever `[reviews]
 * auto` says, its progress and its review on the PR's card and in its Review tab, posted and
 * submitted as Settings says; or opens the PR's review page in the Codex app. A PR the board
 * doesn't list (no `key`) has only that. How it went is reported the window's way (a start under
 * way through `report.busy`, where the window shows one), and `started` hears that the review is
 * under way, where the window goes on to the next thing that waits on you.
 */
export function reviewItems(pr: Pick<PrItem, 'number' | 'url' | 'repoSlug' | 'lanes'> & Partial<Pick<PrItem, 'key' | 'mine'>>, lanes: Lane[], accounts: StartAccount[], usage: Usage[], report: Report, started?: () => void, now = Date.now(), reviews?: ReviewsCfg | null): MenuAction[] {
  const key = pr.key;
  const picks = reviewPicks(accounts, usage, now, !!key && !!reviewLane(pr, lanes), reviews, !!pr.mine);
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
      const work = reviewNow(key, pick.account);
      (report.busy ? report.busy(`Starting a review of #${pr.number}…`, work) : work)
        .then(() => {
          report.info(`Reviewing #${pr.number}: its card says how it goes.`);
          started?.();
        })
        .catch((e: unknown) => report.error(errorText(e)));
    },
  }));
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

/** What a PR's card says of Codealong's own review of it (autoreview.rs, `[reviews] auto`): the
 *  latest round in a few words with its icon, then why it didn't go as it would, and that new
 *  commits are reviewed again. `onGitHub`, once your review of the PR there is read (prDraft.ts):
 *  a draft that's no longer one was submitted, or thrown away. A review of a PR of yours
 *  (`[reviews.own]`) says where it went back to, by the session's title (`sentTo`), or why it
 *  hasn't, as the engine has it. */
export function autoReviewWords(r: AutoReview, onGitHub?: { draft: boolean; submitted: string | null }, sentTo?: string | null): { icon: string; says: string; note: string | null } {
  const at = r.head.slice(0, 7);
  const [icon, says] = {
    queued: ['hourglass', `Agent review of ${at} to come`],
    reviewing: ['eye', `Agent reviewing ${at}`],
    posted: ['message-circle', `Agent reviewed ${at} and commented`],
    drafted: ['note', `Agent drafted a review of ${at}`],
    kept: ['check', `Agent reviewed ${at}`],
    skipped: ['circle-dashed', `Agent passed on ${at}`],
    failed: ['alert', `Agent’s review of ${at} didn’t finish`],
  }[r.stage];
  const kept = r.stage === 'kept' && !r.own ? 'Its review is in its session: posting is off.' : null;
  const back = r.own && r.sent ? `It went back to ${sentTo ? `“${sentTo}”` : 'the session that opened the PR'}.` : null;
  const gone = r.stage === 'drafted' && onGitHub && !onGitHub.draft ? (onGitHub.submitted ? `You submitted it, ${submittedWord(onGitHub.submitted)}.` : 'It isn’t a draft on GitHub any more: it was submitted or thrown away.') : null;
  // A draft that shows below says so itself, and one submitted for you isn't a draft any more.
  const drafted = r.submitted ? null : (gone ?? (r.stage === 'drafted' && !onGitHub?.draft ? 'Only you see it until you submit it.' : null));
  const why = r.note ?? kept ?? back ?? drafted;
  const next = r.behind ? (r.stage === 'reviewing' ? 'New commits since: it reviews them once this one’s done.' : 'New commits since: it reviews them again once they settle.') : null;
  const held = r.holds ? 'It waits on your review once the agent’s is done.' : null;
  const note = [why, next, held].filter((x): x is string => !!x).join(' ');
  if (r.submitted) return { ...submittedWords(r.submitted, at), note: note || null };
  return { icon: r.own && r.sent && r.stage === 'kept' ? 'send' : icon, says, note: note || null };
}

/** What an agent's review of commit `at` was submitted as yours as (`[reviews] submit`). */
function submittedWords(v: Verdict, at: string): { icon: string; says: string } {
  return {
    approve: { icon: 'check', says: `Agent approved ${at} for you` },
    'request-changes': { icon: 'alert', says: `Agent requested changes on ${at} for you` },
    comment: { icon: 'message-circle', says: `Agent commented on ${at} for you` },
  }[v];
}

/** What the agent would do with a PR, as its windows suggest it. */
export function suggestWords(v: Verdict): string {
  return { approve: 'Suggests approving', 'request-changes': 'Suggests changes', comment: 'Suggests a comment' }[v];
}

/** The line over a PR's box (PrBox.tsx): where its review stands, an agent's round and your
 *  draft review on GitHub together, else that there's none yet; `busy` while an agent reviews. */
export function reviewLineWords(pr: Pick<PrItem, 'autoReview' | 'mine'>, draft: Draft | null): { icon: string; says: string; busy: boolean } {
  const r = pr.autoReview;
  const yours = draft ? `your draft review${draft.count > 0 ? `, ${commentsWord(draft.count)}` : ''}` : null;
  if (r) {
    const { icon, says } = autoReviewWords(r, draft ? { draft: true, submitted: null } : undefined);
    return { icon, says: yours && r.stage === 'drafted' ? `${says}: ${yours}` : says, busy: r.stage === 'reviewing' };
  }
  if (yours) return { icon: 'note', says: yours[0].toUpperCase() + yours.slice(1), busy: false };
  return { icon: 'eye', says: pr.mine ? 'No agent review of your PR' : 'No review of yours yet', busy: false };
}

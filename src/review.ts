// Review with…: a PR's review started on any account. Codex's app has a review page of its own
// (`codex://review`); every other account gets a session in a worktree of the PR's repo, on a
// prompt that asks for a review. And the account that answers questions about a PR in its card's
// box (Ask, PrCard.tsx): the one picked there, else Auto's.

import { reviewInCodex, startSession } from './api';
import type { MenuAction } from './contextMenu';
import { FRESH_SIDES } from './harness';
import { errorText, type Report } from './report';
import { AUTO, autoPick, initialPlace, remembered, type Choice } from './start';
import type { AutoReview, Lane, PrItem, StartAccount, StartPlace, StartSpec, Started, Usage } from './types';

/** One entry of the Review with menu. */
export type ReviewPick =
  | { kind: 'codex-app'; label: string }
  | { kind: 'start'; label: string; account: string; place: StartPlace };

/** What a review session is asked. It reads the PR through `gh`, so it works from any checkout of
 *  the repo, and leaves the code and GitHub alone: what to do with the findings is yours to say. */
export function reviewPrompt(pr: Pick<PrItem, 'number' | 'repoSlug' | 'url' | 'title'>, checkedOut: boolean): string {
  return [
    `Review pull request #${pr.number} in ${pr.repoSlug}, “${pr.title}”: ${pr.url}`,
    '',
    `Read its description, discussion and changes (\`gh pr view ${pr.number} -R ${pr.repoSlug} --comments\`, \`gh pr diff ${pr.number} -R ${pr.repoSlug}\`) and the code around them.${checkedOut ? ' This worktree has its branch checked out.' : ''}`,
    'Report bugs, risks and anything that doesn’t do what the PR says, most serious first, each with where it is.',
    'Don’t change files, push, or comment on GitHub.',
  ].join('\n');
}

/** The worktree on this Mac a review of the PR starts in: one that holds it, else the repo's main
 *  checkout, else any worktree of the repo. Null when this Mac has no checkout of it. */
export function reviewLane(pr: Pick<PrItem, 'repoSlug' | 'lanes'>, lanes: Lane[]): { lane: Lane; checkedOut: boolean } | null {
  const here = lanes.filter((l) => !l.id.startsWith('@'));
  const holding = here.find((l) => pr.lanes.includes(l.id));
  if (holding) return { lane: holding, checkedOut: true };
  const same = here.filter((l) => l.repoSlug === pr.repoSlug || l.repoSlugNow === pr.repoSlug);
  const lane = same.find((l) => l.origin === 'main') ?? same[0];
  return lane ? { lane, checkedOut: false } : null;
}

/** Where a start runs, as the menu says it: `label` is the terminal's name for a terminal start. */
function placeWords(place: StartPlace, label: string): string {
  if (place === 'background') return 'in the background';
  if (place === 'claude-app') return 'in the Claude app';
  return `in ${label}`;
}

/**
 * The menu's entries: Auto (the account the quick panel's Auto would start) when there's a choice,
 * then every account that can start a session. A Codex account whose place is the Codex app opens
 * its review page, which needs no checkout; the others need one (`hasLane`). Each starts where the
 * quick panel would start it.
 */
export function reviewPicks(accounts: StartAccount[], usage: Usage[], now: number, hasLane: boolean, lastPlaces: Record<Choice, StartPlace> = {}): ReviewPick[] {
  const pick = autoPick(accounts, usage, now);
  const entry = (choice: Choice, label: string): ReviewPick | null => {
    const place = initialPlace(choice, accounts, pick, lastPlaces[choice]);
    const account = choice === AUTO ? pick?.account : choice;
    const a = accounts.find((x) => x.name === account);
    if (!place || !a) return null;
    if (place === 'codex-app') return { kind: 'codex-app', label: `${label} in the Codex app` };
    if (!hasLane) return null;
    const where = a.places.find((p) => p.place === place)?.label ?? place;
    return { kind: 'start', label: `${label} ${placeWords(place, where)}`, account: a.name, place };
  };
  const own = accounts.filter((a) => a.places.length > 0).flatMap((a) => entry(a.name, a.name) ?? []);
  const auto = pick && own.length > 1 ? entry(AUTO, `Auto (${pick.account})`) : null;
  return auto ? [auto, ...own] : own;
}

/**
 * Review with…'s items, as every window builds them (`reviewPicks` says which): each starts a
 * review of the PR on an account, in the worktree `reviewLane` picks, on the review prompt, or
 * opens the PR's review page in the Codex app. How it went is reported the window's way (a start
 * under way through `report.busy`, where the window shows one), and `started` hears that the
 * review is under way, where the window goes on to the next thing that waits on you.
 */
export function reviewItems(pr: Pick<PrItem, 'number' | 'title' | 'url' | 'repoSlug' | 'lanes'>, lanes: Lane[], accounts: StartAccount[], usage: Usage[], report: Report, started?: () => void, now = Date.now()): MenuAction[] {
  const at = reviewLane(pr, lanes);
  const picks = reviewPicks(accounts, usage, now, !!at, remembered().places);
  return picks.map((pick) => ({
    label: pick.label,
    run: () => {
      if (pick.kind === 'codex-app' || !at) {
        reviewInCodex(pr.url).then(
          () => started?.(),
          (e: unknown) => report.error(errorText(e)),
        );
        return;
      }
      const spec: StartSpec = { laneId: at.lane.id, account: pick.account, place: pick.place, prompt: reviewPrompt(pr, at.checkedOut), model: null, effort: null, worktree: false, todos: [] };
      const work: Promise<Started | null> = startSession(spec);
      (report.busy ? report.busy(`Starting a review of #${pr.number} on ${pick.account}…`, work) : work)
        .then((started_) => {
          if (started_?.message) report.info(started_.message);
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
 *  commits are reviewed again. */
export function autoReviewWords(r: AutoReview): { icon: string; says: string; note: string | null } {
  const at = r.head.slice(0, 7);
  const [icon, says] = {
    queued: ['hourglass', `Agent review of ${at} to come`],
    reviewing: ['eye', `Agent reviewing ${at}`],
    posted: ['message-circle', `Agent reviewed ${at} and commented`],
    kept: ['check', `Agent reviewed ${at}`],
    skipped: ['circle-dashed', `Agent passed on ${at}`],
    failed: ['alert', `Agent review of ${at} didn’t finish`],
  }[r.stage];
  const why = r.note ?? (r.stage === 'kept' ? 'Its review is in its session: posting is off.' : null);
  const next = r.behind ? (r.stage === 'reviewing' ? 'New commits since: it reviews them once this one’s done.' : 'New commits since: it reviews them again once they settle.') : null;
  const held = r.holds ? 'It waits on your review once the agent’s is done.' : null;
  const note = [why, next, held].filter((x): x is string => !!x).join(' ');
  return { icon, says, note: note || null };
}

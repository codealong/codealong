// Review with…: a PR's review started on any account. Codex's app has a review page of its own
// (`codex://review`); every other account gets a session in a worktree of the PR's repo, on a
// prompt that asks for a review.

import { AUTO, autoPick, initialPlace, type Choice } from './start';
import type { Lane, PrItem, StartAccount, StartPlace, Usage } from './types';

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

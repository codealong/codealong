// The review Codealong's agent wrote of a PR (autoreview.rs), as its panel and cards show it
// (ui/AgentReview.tsx): each comment on a line of the code with the lines around it from the PR's
// diff, and a link to that line on GitHub at the commit it reviewed.

import type { PrItem, Verdict } from './types';

/** A line of a diff around a comment's line, as `excerptAt` gives it. */
export interface ExcerptLine {
  /** Its number in the file's new version; none for a removed line. */
  n: number | null;
  kind: 'add' | 'ctx' | 'del';
  text: string;
  /** The line the comment is on. */
  at: boolean;
}

/**
 * The lines of a file's patch around line `line` of its new version: `around` lines of the new
 * version on each side within the same hunk, with the removed lines among them, the line itself
 * marked. Empty when the patch doesn't show that line (GitHub refuses a comment on one it doesn't,
 * so such a comment was written into the review's text).
 */
export function excerptAt(patch: string, line: number, around = 2): ExcerptLine[] {
  const hunks: ExcerptLine[][] = [];
  let next: number | null = null;
  for (const raw of patch.split('\n')) {
    if (raw.startsWith('@@')) {
      const m = /\+(\d+)/.exec(raw);
      next = m ? Number(m[1]) : null;
      hunks.push([]);
      continue;
    }
    const hunk = hunks[hunks.length - 1];
    if (next === null || !hunk || raw.startsWith('\\')) continue;
    if (raw.startsWith('-')) {
      hunk.push({ n: null, kind: 'del', text: raw.slice(1), at: false });
      continue;
    }
    hunk.push({ n: next, kind: raw.startsWith('+') ? 'add' : 'ctx', text: raw.slice(1), at: next === line });
    next += 1;
  }
  const hunk = hunks.find((h) => h.some((l) => l.at));
  if (!hunk) return [];
  const near = (l: ExcerptLine) => l.n !== null && Math.abs(l.n - line) <= around;
  const first = hunk.findIndex(near);
  const last = hunk.length - 1 - [...hunk].reverse().findIndex(near);
  return hunk.slice(first, last + 1);
}

/** A line of a file at a commit on GitHub. */
export function lineLink(repoSlug: string, commit: string, path: string, line: number): string {
  return `https://github.com/${repoSlug}/blob/${commit}/${path.split('/').map(encodeURIComponent).join('/')}#L${line}`;
}

/** The parts of a PR its details show one at a time (PrInspector.tsx `PrDetails`). */
export type PrTab = 'description' | 'changes' | 'review';

/** The tab a PR's details open on, when it hasn't shown one yet: its review, when the agent's
 *  is there to read and the PR still waits on yours, or it's a review of a PR of yours, else its
 *  description. */
export function openingTab(pr: Pick<PrItem, 'reviewRequested' | 'state' | 'autoReview'>): PrTab {
  const a = pr.autoReview;
  const stage = a?.stage;
  const open = pr.state !== 'merged' && pr.state !== 'closed';
  const theirs = pr.reviewRequested && (stage === 'drafted' || stage === 'posted' || stage === 'kept');
  const yours = !!a?.own && stage === 'kept';
  return open && (theirs || yours) ? 'review' : 'description';
}

/** The verdict a PR's box submits the agent's review as, in one press (PrBox.tsx; autoreview.rs
 *  `submit_now`): the one it suggests, when that's a request for changes or a comment (an approval
 *  needs no text of its own, so its button is as it always is), while its latest review is of the
 *  commit the PR's at, there to submit (drafted, posted or kept), not submitted yet, and of
 *  someone else's open PR. Null otherwise. */
export function agentsVerdict(pr: Pick<PrItem, 'state' | 'mine' | 'headOid' | 'autoReview'>): Verdict | null {
  const a = pr.autoReview;
  if (!a?.suggests || a.suggests === 'approve' || a.submitted || a.own || pr.mine) return null;
  if (pr.state === 'merged' || pr.state === 'closed' || !pr.headOid || a.head !== pr.headOid) return null;
  return a.stage === 'drafted' || a.stage === 'posted' || a.stage === 'kept' ? a.suggests : null;
}

/** What the agent's review goes as, in its button's words: "with your draft review and its 3
 *  comments", "pointing to the agent's review on the PR", "with the agent's review". */
export function agentsWords(stage: string, draftComments: number | null): string {
  if (draftComments !== null) return `with your draft review${draftComments > 0 ? ` and its ${draftComments === 1 ? 'comment' : `${draftComments} comments`}` : ''}`;
  return stage === 'posted' ? 'pointing to the agent’s review on the PR' : 'with the agent’s review as its text';
}

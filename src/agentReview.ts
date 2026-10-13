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
 * The lines of a file's patch around line `line` of its new version, or of its old one with
 * `before` (a thread on a line the PR removes): `around` lines of that version on each side within
 * the same hunk, with the other version's lines among them, the line itself marked. Empty when the
 * patch doesn't show that line (GitHub refuses a comment on one it doesn't, so such a comment was
 * written into the review's text).
 */
export function excerptAt(patch: string, line: number, around = 2, before = false): ExcerptLine[] {
  // Each line with its number in the side counted: the new version's, or the old one's.
  const hunks: { l: ExcerptLine; at: number | null }[][] = [];
  let next: number | null = null;
  let old: number | null = null;
  for (const raw of patch.split('\n')) {
    if (raw.startsWith('@@')) {
      const m = /-(\d+)(?:,\d+)? \+(\d+)/.exec(raw);
      old = m ? Number(m[1]) : null;
      next = m ? Number(m[2]) : null;
      hunks.push([]);
      continue;
    }
    const hunk = hunks[hunks.length - 1];
    if (next === null || old === null || !hunk || raw.startsWith('\\')) continue;
    const kind = raw.startsWith('-') ? 'del' : raw.startsWith('+') ? 'add' : 'ctx';
    const n = kind === 'del' ? null : next;
    const o = kind === 'add' ? null : old;
    const counted = before ? o : n;
    hunk.push({ l: { n, kind, text: raw.slice(1), at: counted === line }, at: counted });
    if (kind !== 'del') next += 1;
    if (kind !== 'add') old += 1;
  }
  const hunk = hunks.find((h) => h.some((x) => x.l.at));
  if (!hunk) return [];
  const near = (x: { at: number | null }) => x.at !== null && Math.abs(x.at - line) <= around;
  const first = hunk.findIndex(near);
  const last = hunk.length - 1 - [...hunk].reverse().findIndex(near);
  return hunk.slice(first, last + 1).map((x) => x.l);
}

/** A line of a file at a commit on GitHub. */
export function lineLink(repoSlug: string, commit: string, path: string, line: number): string {
  return `https://github.com/${repoSlug}/blob/${commit}/${path.split('/').map(encodeURIComponent).join('/')}#L${line}`;
}

/** The parts of a PR its details show one at a time (PrInspector.tsx `PrDetails`). */
export type PrTab = 'description' | 'changes' | 'review';

/** The tab a PR's details open on, when it hasn't shown one yet: its reviews, when the agent's
 *  is there to read and the PR still waits on yours, when it's a review of a PR of yours, or when
 *  someone asked for changes to a PR of yours, else its description. */
export function openingTab(pr: Pick<PrItem, 'reviewRequested' | 'state' | 'autoReview' | 'mine'>): PrTab {
  const a = pr.autoReview;
  const stage = a?.stage;
  const open = pr.state !== 'merged' && pr.state !== 'closed';
  const theirs = pr.reviewRequested && (stage === 'drafted' || stage === 'posted' || stage === 'kept');
  const yours = !!a?.own && stage === 'kept';
  const asked = pr.mine && pr.state === 'changes';
  return open && (theirs || yours || asked) ? 'review' : 'description';
}

/** The verdict a PR's box submits the agent's review as, in one press (PrBox.tsx; autoreview.rs
 *  `submit_now`): the one it suggests, when that's a request for changes or a comment (an approval
 *  needs no text of its own, so its button is as it always is), while its latest review is of the
 *  commit the PR's at, there to submit (drafted, posted or kept), not submitted yet, and of
 *  someone else's open PR that still asks for your review. A drafted one needs its draft still on
 *  GitHub (`hasDraft`, your draft review as read): one submitted or thrown away there has nothing
 *  to submit, and the box goes back to its own verdicts. Null otherwise. */
export function agentsVerdict(pr: Pick<PrItem, 'state' | 'mine' | 'reviewRequested' | 'headOid' | 'autoReview'>, hasDraft: boolean): Verdict | null {
  const a = pr.autoReview;
  if (!a?.suggests || a.suggests === 'approve' || a.submitted || a.own || pr.mine || !pr.reviewRequested) return null;
  if (pr.state === 'merged' || pr.state === 'closed' || !pr.headOid || a.head !== pr.headOid) return null;
  if (a.stage === 'drafted') return hasDraft ? a.suggests : null;
  return a.stage === 'posted' || a.stage === 'kept' ? a.suggests : null;
}

/** What the agent's review goes as, in its button's words: "with your draft review and its 3
 *  comments", "pointing to the agent's review on the PR", "with the agent's review". */
export function agentsWords(stage: string, draftComments: number | null): string {
  if (draftComments !== null) return `with your draft review${draftComments > 0 ? ` and its ${draftComments === 1 ? 'comment' : `${draftComments} comments`}` : ''}`;
  return stage === 'posted' ? 'pointing to the agent’s review on the PR' : 'with the agent’s review as its text';
}

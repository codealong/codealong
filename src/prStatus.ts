// Where a PR stands, as the PRs view and the PR panel show it: a pill for its review, one for
// its checks, and one for anything else that keeps GitHub from merging it. When nothing does,
// they fold into a single "Ready to merge". The PRs view also flags the rows that are your move.

import type { AutoReview, Checks, Merge, NudgeAbout, PrComment, PrItem, PrState } from './types';

/** A pill's color: green for approved and passing, amber for what needs you, red for failing. */
export type PrTone = 'ready' | 'pass' | 'attn' | 'alert' | 'plain' | 'quiet' | 'merged';

export interface PrPill {
  key: 'ready' | 'review' | 'checks' | 'merge' | 'state';
  tone: PrTone;
  icon: string | null;
  text: string;
  /** What it says in a row short of room: `3` for `3 failing`, or nothing beside its icon. */
  short: string | null;
  title: string;
}

/** What the pills read: a worktree's PR or a PRs-view item. */
export interface PrFacts {
  state: PrState;
  checks: Checks;
  merge: Merge;
  /** A PRs-view item's automatic review: one the agent is still on reads so in place of
   *  Awaiting review. */
  autoReview?: AutoReview;
  base?: string | null;
}

const isOpen = (state: PrState) => state !== 'merged' && state !== 'closed';

/** Codealong's agent hasn't finished reviewing the PR's commit (autoreview.rs, `[reviews] auto`):
 *  it's out of what waits on you until it has, as a snoozed review is until its time (model.rs
 *  `PrItem::waits_on_review`). */
export function heldForAgent(pr: { autoReview?: { holds?: boolean } }): boolean {
  return !!pr.autoReview?.holds;
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

/** GitHub says it can be merged now: the reviews the repo requires, passing checks and no conflicts.
 *  Only GitHub can say so; approved with passing checks can still conflict with its base. */
export function prReady(pr: PrFacts): boolean {
  return isOpen(pr.state) && pr.state !== 'draft' && pr.merge === 'clean' && pr.checks.failed === 0 && pr.checks.pending === 0;
}

function reviewPill(pr: PrFacts): PrPill {
  const pill = (tone: PrTone, text: string, title: string): PrPill => ({ key: 'review', tone, icon: null, text, short: null, title });
  switch (pr.state) {
    case 'draft':
      return pill('quiet', 'Draft', 'A draft: not ready for review yet');
    case 'changes':
      return pill('attn', 'Changes requested', 'A reviewer asked for changes');
    case 'approved':
      if (pr.merge === 'blocked') return pill('pass', 'Approved', 'Approved, but something the repo requires still holds back the merge');
      if (pr.merge === 'unknown') return pill('pass', 'Approved', "Approved. GitHub hasn't said yet whether it can merge");
      return pill('pass', 'Approved', 'Approved');
    default:
      if (heldForAgent(pr)) return pill('plain', 'Agent reviewing', 'Codealong’s agent is reviewing it: it waits on your review once that’s done');
      return pill('plain', 'Awaiting review', 'Not approved yet');
  }
}

function checksPill(c: Checks): PrPill | null {
  if (c.failed > 0) {
    const names = c.failing.length > 0 ? `: ${c.failing.join(', ')}` : '';
    return { key: 'checks', tone: 'alert', icon: 'x', text: `${c.failed} failing`, short: `${c.failed}`, title: `${plural(c.failed, 'check')} failing${names}` };
  }
  if (c.pending > 0) {
    const passed = c.passed > 0 ? `, ${c.passed} passed` : '';
    return { key: 'checks', tone: 'plain', icon: 'circle-dashed', text: `${c.pending} running`, short: `${c.pending}`, title: `${plural(c.pending, 'check')} running${passed}` };
  }
  if (c.passed > 0) return { key: 'checks', tone: 'pass', icon: 'check', text: `${c.passed}`, short: null, title: `${plural(c.passed, 'check')} passed` };
  return null;
}

/** The pills for a PR, in a fixed order so they line up down a list: review, checks, merge. */
export function prPills(pr: PrFacts): PrPill[] {
  if (pr.state === 'merged') return [{ key: 'state', tone: 'merged', icon: null, text: 'Merged', short: null, title: 'Merged' }];
  if (pr.state === 'closed') return [{ key: 'state', tone: 'quiet', icon: null, text: 'Closed', short: null, title: 'Closed without merging' }];
  const c = pr.checks;
  if (prReady(pr)) {
    const why = [pr.state === 'approved' ? 'Approved' : null, c.passed > 0 ? `${plural(c.passed, 'check')} passed` : null, 'no conflicts'].filter(Boolean).join(', ');
    return [{ key: 'ready', tone: 'ready', icon: 'merge', text: 'Ready to merge', short: null, title: `${why[0].toUpperCase()}${why.slice(1)}: GitHub can merge it` }];
  }
  const base = pr.base ?? 'its base';
  const pills = [reviewPill(pr), checksPill(c)].filter((p): p is PrPill => p !== null);
  if (pr.merge === 'conflicts') {
    pills.push({ key: 'merge', tone: 'attn', icon: 'alert', text: 'Conflicts', short: '', title: `Conflicts with ${base}: resolve them before it can merge` });
  } else if (pr.merge === 'behind') {
    pills.push({ key: 'merge', tone: 'attn', icon: 'refresh', text: 'Out of date', short: '', title: `Behind ${base}, and the repo wants it up to date before it merges` });
  }
  return pills;
}

/** Why a PR is your move, if it is: your review is asked for, or it's yours and GitHub would
 *  merge it, or its checks fail, or it's held up by something you can fix. `attn` and `alert`
 *  are what the Needs you filter keeps. */
export function prFlag(pr: PrItem): 'ready' | 'attn' | 'alert' | null {
  if (!isOpen(pr.state)) return null;
  // A review you snoozed waits until its time, and one the agent is on until it's done.
  if (pr.reviewRequested && !pr.snoozedUntil && !heldForAgent(pr)) return 'attn';
  if (!pr.mine) return null;
  if (prReady(pr)) return 'ready';
  if (pr.checks.failed > 0) return 'alert';
  if (pr.state === 'changes' || pr.merge === 'conflicts' || pr.merge === 'behind') return 'attn';
  return null;
}

/** Whether you can approve it: any open PR but your own, which GitHub doesn't let you approve,
 *  whether or not your review was asked for. Request changes goes by the same. */
export function mayApprove(pr: Pick<PrItem, 'state' | 'mine'>): boolean {
  return isOpen(pr.state) && !pr.mine;
}

/* ---- Nudges ---- */

/** When a nudge sends its message, and what it tells of, as `[pull_requests] nudge_about` says
 *  (nudges.rs `keep`). */
const NUDGED_WHEN: Record<NudgeAbout, { when: string; of: string }> = {
  everything: { when: 'when someone else reviews it, asks for changes or comments, when it’s approved, merged or closed, and when its checks fail', of: 'reviews, comments, an approval, a merge and failing checks' },
  feedback: { when: 'when someone else asks for changes or comments, never for an approval or a merge', of: 'comments and requests for changes, never an approval or a merge' },
  'feedback-and-checks': { when: 'when someone else asks for changes or comments and when its checks fail, never for an approval or a merge', of: 'comments, requests for changes and failing checks, never an approval or a merge' },
};

/** What a PR's card says under its nudge switch (nudges.rs): whether it's on and why, and who's
 *  told of what (`about`, `[pull_requests] nudge_about`). `primary` is its primary session's
 *  title and whether it runs on this Mac, when the card has it; only that session is ever told. */
export function nudgeWords(pr: Pick<PrItem, 'state' | 'nudgeByDefault'>, on: boolean, primary: { title: string; runs: boolean } | null, about: NudgeAbout = 'everything'): string {
  const said = NUDGED_WHEN[about] ?? NUDGED_WHEN.everything;
  if (on) {
    const lead = pr.nudgeByDefault ? 'On for every PR of yours, by Settings.' : 'On for this PR.';
    if (!primary) return `${lead} No session of it is known here, so nobody is told.`;
    if (!primary.runs) return `${lead} “${primary.title}”, its primary session, isn’t running here, so nobody is told until it is.`;
    return `${lead} “${primary.title}”, its primary session, is sent a message as one of yours ${said.when}.`;
  }
  const who = primary ? `“${primary.title}”` : 'its primary session';
  return pr.state === 'approved' && about === 'everything'
    ? `Off for this PR. It’s approved already: turning this on tells ${who} that now, then of what happens on it next.`
    : `Off for this PR. Turned on, ${who} is told of ${said.of}.`;
}

/* ---- Review comments ---- */

/** A comment "asks for a change" at this probability or more. */
export const ASKS_AT = 0.6;

/** The comments worth asking Jev about: others' comments on threads that are still open. */
export function commentsToAsk(comments: PrComment[]): PrComment[] {
  return comments.filter((c) => !c.mine && !c.resolved);
}

/** The comments that ask for a change, by what Jev said. */
export function asksFor(comments: PrComment[], asks: Record<string, number>): Set<string> {
  return new Set(commentsToAsk(comments).filter((c) => (asks[c.id] ?? 0) >= ASKS_AT).map((c) => c.id));
}

/** A comment's text for a list: Markdown's marks dropped, on one line, cut short. */
export function commentPreview(body: string, max = 280): string {
  const text = body
    .replace(/```[\s\S]*?```/g, ' [code] ')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' [image] ')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/[*_`>#]+/g, '')
    .replace(/\s+/g, ' ')
    .trim();
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

// Where a PR stands, as the PRs view and the PR panel show it: a pill for its review, one for
// its checks, and one for anything else that keeps GitHub from merging it. When nothing does,
// they fold into a single "Ready to merge". The PRs view also flags the rows that are your move.

import { count } from './format';
import type { AutoReview, Checks, LastReview, Merge, NudgeAbout, PrComment, PrItem, PrState, PrStopped, StaleApproval } from './types';

/** A pill's color: green for approved and passing, amber for what needs you, red for failing. */
export type PrTone = 'ready' | 'pass' | 'attn' | 'alert' | 'plain' | 'quiet' | 'merged';

export interface PrPill {
  key: 'ready' | 'review' | 'stale' | 'checks' | 'merge' | 'state';
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
   *  Review requested or Awaiting review. */
  autoReview?: AutoReview;
  base?: string | null;
  /** Whose reviews it waits on, yours and others': Review requested while anyone's is asked
   *  for, Awaiting review while nobody's is. */
  reviewRequested?: boolean;
  reviewersAsked?: string[];
  /** The commit it's at, which says whether an approval of the agent's is of it (`agentApproves`),
   *  and whether yours is (`yourReview`). */
  headOid?: string;
  /** The review you last gave it, and whether it's yours, which you don't review (`yourReview`). */
  lastReview?: LastReview;
  mine?: boolean;
  /** Approved only by approvals of an earlier commit (`staleWords`). */
  staleApproval?: StaleApproval;
}

/** Your last review of a PR against the commit it's at (`PrItem.lastReview`): `current` when it's
 *  of that commit and still stands, which a dismissed one doesn't. Null without one, and on a PR
 *  of yours, which you don't review. */
export function yourReview(pr: Pick<PrFacts, 'lastReview' | 'headOid' | 'mine'>): { review: LastReview; current: boolean } | null {
  const r = pr.lastReview;
  if (!r || pr.mine) return null;
  return { review: r, current: r.state !== 'dismissed' && !!r.commit && r.commit === pr.headOid };
}

/** Your approval of the commit a PR's at, where that's your last review of it: Approve reads
 *  Approved, and another press is another approval. */
export function yourApproval(pr: Pick<PrFacts, 'lastReview' | 'headOid' | 'mine'>): LastReview | null {
  const y = yourReview(pr);
  return y?.current && y.review.state === 'approved' ? y.review : null;
}

/** Your approval of the commit a PR's at, or request for changes on it: what you'd do with it is
 *  said, so what an agent suggests you do no longer shows (PrBox.tsx). */
export function yourVerdictStands(pr: Pick<PrFacts, 'lastReview' | 'headOid' | 'mine'>): boolean {
  const y = yourReview(pr);
  return !!y?.current && (y.review.state === 'approved' || y.review.state === 'changes');
}

/** The commit your last review of a PR was of, where the PR has moved on from it: what its
 *  changes since are compared with (since.ts). Null where it's of the commit the PR's at, where
 *  either commit isn't known, and without one. */
export function reviewedBefore(pr: Pick<PrFacts, 'lastReview' | 'headOid' | 'mine'>): string | null {
  const y = yourReview(pr);
  return y && !y.current && y.review.commit && pr.headOid && y.review.commit !== pr.headOid ? y.review.commit : null;
}

/** What your last review of a PR said, by its commit (`You approved abc1234`), with its icon. */
export function yourReviewSaid(r: LastReview): { icon: string; says: string } {
  const at = r.commit ? r.commit.slice(0, 7) : 'it';
  return {
    approved: { icon: 'check', says: `You approved ${at}` },
    changes: { icon: 'alert', says: `You asked for changes on ${at}` },
    commented: { icon: 'message-circle', says: `You commented on ${at}` },
    dismissed: { icon: 'x', says: `Your review of ${at} was dismissed` },
  }[r.state];
}

/** Your last review of a PR in a sentence, for a pill's or chip's title: what it said, and
 *  whether it's of the commit the PR's at. Null without one. */
export function yourReviewNote(pr: Pick<PrFacts, 'lastReview' | 'headOid' | 'mine'>): string | null {
  const y = yourReview(pr);
  if (!y) return null;
  const { says } = yourReviewSaid(y.review);
  if (y.current) return `${says}, the commit it’s at.`;
  return y.review.commit && pr.headOid && y.review.state !== 'dismissed' ? `${says}, and it’s had commits since.` : `${says}.`;
}

/** Names as a sentence lists them: `ana`, `ana and ken`, `ana, ken and sam`. */
const listed = (names: string[]) => (names.length > 1 ? `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}` : names[0]);

/** A PR approved only before its latest commits, in a sentence, for its mark's and pill's titles:
 *  who approved which commit, and that GitHub still counts it, which is why it can read Ready to
 *  merge. Null for a PR that isn't approved, or is approved as it is. */
export function staleWords(pr: Pick<PrFacts, 'state' | 'staleApproval'>): string | null {
  const s = pr.state === 'approved' ? pr.staleApproval : undefined;
  if (!s) return null;
  const at = s.commit.slice(0, 7);
  const who = s.by.length > 0 ? listed(s.by) : 'A reviewer';
  if (s.by.length > 1) return `Approved before its latest commits: ${who} approved earlier ones, the latest ${at}. GitHub still counts the approvals, as the repo doesn’t dismiss them when commits come.`;
  return `Approved before its latest commits: ${who} approved ${at}. GitHub still counts the approval, as the repo doesn’t dismiss it when commits come.`;
}

/** A PR that asks for your review after you gave one: Re-review, saying what yours was and whether
 *  commits came since, and who asked. Null when it doesn't ask, or you hadn't reviewed it. */
function reReviewPill(pr: PrFacts): PrPill | null {
  if (!pr.reviewRequested || !isOpen(pr.state) || pr.state === 'draft') return null;
  const note = yourReviewNote(pr);
  if (!note) return null;
  const asked = reviewersWords(pr);
  return { key: 'review', tone: 'plain', icon: 'repeat', text: 'Re-review', short: null, title: `Your review is asked for again. ${note}${asked ? ` ${asked}.` : ''}` };
}

const isOpen = (state: PrState) => state !== 'merged' && state !== 'closed';

/** Codealong's agent hasn't finished reviewing the PR's commit (autoreview.rs, `[reviews] auto`):
 *  it's out of what waits on you until it has, as a snoozed review is until its time (model.rs
 *  `PrItem::waits_on_review`). */
export function heldForAgent(pr: { autoReview?: { holds?: boolean } }): boolean {
  return !!pr.autoReview?.holds;
}

type AgentFacts = Pick<PrFacts, 'state' | 'reviewRequested' | 'autoReview' | 'headOid'>;

/** Codealong's agent approves the commit someone else's PR is at, and the PR waits on your review:
 *  what's asked of you is a sign-off (autoreview.rs `approved`). */
export function agentApproves(pr: AgentFacts): boolean {
  const a = pr.autoReview;
  return !!a?.approves && !a.holds && a.approves === pr.headOid && !!pr.reviewRequested && isOpen(pr.state);
}

/** The commit, short, the agent approved before the PR's author pushed again, while the commits
 *  since hold the PR out of what waits on you for their review (`heldForAgent`): not yet yours to
 *  sign off, as only what you can act on waits on you. Null otherwise. */
export function approvedBefore(pr: AgentFacts): string | null {
  const a = pr.autoReview;
  return a?.approves && a.holds && a.approves !== pr.headOid && pr.reviewRequested && isOpen(pr.state) ? a.approves.slice(0, 7) : null;
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

/** GitHub says it can be merged now: the reviews the repo requires, passing checks and no conflicts.
 *  Only GitHub can say so; approved with passing checks can still conflict with its base. */
export function prReady(pr: PrFacts): boolean {
  return isOpen(pr.state) && pr.state !== 'draft' && pr.merge === 'clean' && pr.checks.failed === 0 && pr.checks.pending === 0;
}

const reviewPillOf = (tone: PrTone, text: string, title: string, icon: string | null = null): PrPill => ({ key: 'review', tone, icon, text, short: null, title });

/** Where Codealong's agent stands on a PR that waits on your review: approving the commit it's at,
 *  so it's yours to sign off; re-reviewing the commits pushed after one it approved; or reviewing
 *  it. Either of the last two holds it out of what waits on you. It says more to you than GitHub's
 *  verdict, another reviewer's approval or a request for changes the agent submitted for you
 *  before (which GitHub keeps until you approve), and shows beside Ready to merge. */
function agentPill(pr: PrFacts): PrPill | null {
  const yours = yourReviewNote(pr);
  if (agentApproves(pr)) return reviewPillOf('pass', 'Agent approves', `Codealong’s agent approves ${pr.autoReview?.approves?.slice(0, 7)}: it waits on your sign-off${yours ? `. ${yours}` : ''}`);
  const before = approvedBefore(pr);
  if (before) return reviewPillOf('plain', 'Agent re-reviewing', `${rereviewWords(before)}: it waits on your sign-off once the agent approves them`, 'hourglass');
  if (heldForAgent(pr) && isOpen(pr.state)) return reviewPillOf('plain', 'Agent reviewing', 'Codealong’s agent is reviewing it: it waits on your review once that’s done');
  return null;
}

function reviewPill(pr: PrFacts): PrPill {
  const pill = reviewPillOf;
  const agent = pr.state === 'draft' ? null : agentPill(pr);
  if (agent) return agent;
  // Asked for again after you gave yours, which says more than who it waits on or that it's
  // approved, your approval perhaps.
  const again = reReviewPill(pr);
  if (again) return again;
  const yours = yourReviewNote(pr);
  switch (pr.state) {
    case 'draft':
      return pill('quiet', 'Draft', 'A draft: not ready for review yet');
    case 'changes':
      return pill('attn', 'Changes requested', 'A reviewer asked for changes');
    case 'approved': {
      const and = yours ? `. ${yours}` : '';
      if (pr.merge === 'blocked') return pill('pass', 'Approved', `Approved, but something the repo requires still holds back the merge${and}`);
      if (pr.merge === 'unknown') return pill('pass', 'Approved', `Approved. GitHub hasn't said yet whether it can merge${and}`);
      return pill('pass', 'Approved', `Approved${and}`);
    }
    default:
      const asked = reviewersWords(pr);
      if (asked) return pill('plain', 'Review requested', asked);
      return pill('plain', 'Awaiting review', 'Not approved yet, and nobody is asked to review it');
  }
}

/** An approved PR's approvals are all of an earlier commit (`staleWords`): it goes after the pill
 *  they make it, Approved or Ready to merge, which stays first, as GitHub says it. */
function stalePill(pr: PrFacts): PrPill | null {
  const title = staleWords(pr);
  return title ? { key: 'stale', tone: 'attn', icon: 'history', text: 'Stale approval', short: '', title } : null;
}

/** Why a PR the agent approved before its author pushed again is held. */
const rereviewWords = (before: string) => `Codealong’s agent approved ${before} and reviews the commits since`;

/** Where a PR's review stands, as its chip marks it after its number (primitives.tsx PrChip):
 *  what the chip's color can't tell apart, an approved PR being as green as an open one. `tone`
 *  is the chip's checks' (`.ca-checks.is-…`). */
export interface ReviewMark {
  key: 'asked' | 'agent' | 'approved' | 'stale' | 'changes';
  tone: 'attn' | 'pending' | 'pass' | 'fail';
  icon: string;
  title: string;
}

/** The chip's review mark: your review asked for (unless snoozed), Codealong's agent still
 *  reviewing it, approved, approved only before its latest commits, or changes asked for. None
 *  while it only awaits review, for a draft, whose icon says so, or once it's merged or closed. A
 *  worktree's PR (`Pr`) has only its state and its approvals to go by; the board's (`PrItem`)
 *  says what's asked of you. */
export function reviewMark(pr: Pick<PrItem, 'state'> & Partial<Pick<PrItem, 'reviewRequested' | 'requestedBy' | 'snoozedUntil' | 'autoReview' | 'headOid' | 'lastReview' | 'mine' | 'staleApproval'>>): ReviewMark | null {
  if (!isOpen(pr.state) || pr.state === 'draft') return null;
  if (heldForAgent(pr)) {
    const before = approvedBefore(pr);
    return { key: 'agent', tone: 'pending', icon: 'hourglass', title: before ? `${rereviewWords(before)}: it waits on your sign-off once the agent approves them` : 'Codealong’s agent is reviewing it: it waits on your review once that’s done' };
  }
  if (pr.reviewRequested && !pr.snoozedUntil) {
    const by = pr.requestedBy ? `, by ${pr.requestedBy}` : '';
    const yours = yourReviewNote(pr);
    const again = yours ? ' again' : '';
    const asked = agentApproves(pr) ? `Your sign-off is asked for${again}${by}: Codealong’s agent approves ${pr.autoReview?.approves?.slice(0, 7)}` : `Your review is asked for${again}${by}`;
    return { key: 'asked', tone: 'attn', icon: 'eye', title: yours ? `${asked}. ${yours}` : asked };
  }
  const stale = staleWords(pr);
  if (stale) return { key: 'stale', tone: 'attn', icon: 'history', title: stale };
  if (pr.state === 'approved') return { key: 'approved', tone: 'pass', icon: 'user-check', title: 'Approved' };
  if (pr.state === 'changes') return { key: 'changes', tone: 'fail', icon: 'file-diff', title: 'A reviewer asked for changes' };
  return null;
}

/** Whose reviews an open PR still waits on, as its card says it: you first when it's asked of you,
 *  snoozed or not, then the others as GitHub lists them. Null when it waits on nobody's, or is
 *  done. */
export function reviewersWords(pr: Pick<PrItem, 'state'> & Partial<Pick<PrItem, 'reviewRequested' | 'reviewersAsked'>>): string | null {
  if (!isOpen(pr.state)) return null;
  const names = [...(pr.reviewRequested ? ['you'] : []), ...(pr.reviewersAsked ?? [])];
  if (names.length === 0) return null;
  return `Waiting on a review from ${listed(names)}`;
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

/** The pills for a PR, in a fixed order so they line up down a list: review, checks, merge, with
 *  a stale approval after the pill it qualifies. */
export function prPills(pr: PrFacts): PrPill[] {
  if (pr.state === 'merged') return [{ key: 'state', tone: 'merged', icon: null, text: 'Merged', short: null, title: 'Merged' }];
  if (pr.state === 'closed') return [{ key: 'state', tone: 'quiet', icon: null, text: 'Closed', short: null, title: 'Closed without merging' }];
  const c = pr.checks;
  if (prReady(pr)) {
    const why = [pr.state === 'approved' ? 'Approved' : null, c.passed > 0 ? `${plural(c.passed, 'check')} passed` : null, 'no conflicts'].filter(Boolean).join(', ');
    const ready: PrPill = { key: 'ready', tone: 'ready', icon: 'merge', text: 'Ready to merge', short: null, title: `${why[0].toUpperCase()}${why.slice(1)}: GitHub can merge it` };
    // Where the agent stands goes beside it, else, asked for your review again, Re-review: it's
    // likely your approval of an earlier commit that has it ready, and a review it waits on
    // isn't to be taken for one done.
    const first = agentPill(pr) ?? reReviewPill(pr);
    return [first, ready, stalePill(pr)].filter((p): p is PrPill => p !== null);
  }
  const base = pr.base ?? 'its base';
  const pills = [reviewPill(pr), stalePill(pr), checksPill(c)].filter((p): p is PrPill => p !== null);
  if (pr.merge === 'conflicts') {
    pills.push({ key: 'merge', tone: 'attn', icon: 'alert', text: 'Conflicts', short: '', title: `Conflicts with ${base}: resolve them before it can merge` });
  } else if (pr.merge === 'behind') {
    pills.push({ key: 'merge', tone: 'attn', icon: 'refresh', text: 'Out of date', short: '', title: `Behind ${base}, and the repo wants it up to date before it merges` });
  }
  return pills;
}

/** What a PR's chip says after its number where it says it in words (primitives.tsx PrChip):
 *  the first of the pills its header shows, Ready to merge, Review requested, Approved and the
 *  rest. A worktree's PR, which doesn't say whose review it waits on, reads Open rather than
 *  Awaiting review. */
export function chipWord(pr: PrFacts): string {
  const first = prPills(pr)[0];
  if (first.key === 'review' && pr.state === 'open' && pr.reviewRequested === undefined && !heldForAgent(pr)) return 'Open';
  return first.text;
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
  everything: { when: 'when someone else reviews it, asks for changes or comments, when it’s approved, merged or closed, and when its checks fail or its branch conflicts with its base', of: 'reviews, comments, an approval, a merge, failing checks and conflicts' },
  feedback: { when: 'when someone else asks for changes or comments, never for an approval or a merge', of: 'comments and requests for changes, never an approval or a merge' },
  'feedback-and-checks': { when: 'when someone else asks for changes or comments and when its checks fail or its branch conflicts with its base, never for an approval or a merge', of: 'comments, requests for changes, failing checks and conflicts, never an approval or a merge' },
  'feedback-and-merges': { when: 'when someone else asks for changes or comments and when it’s merged, never for an approval', of: 'comments, requests for changes and a merge, never an approval' },
  'feedback-checks-and-merges': { when: 'when someone else asks for changes or comments, when its checks fail or its branch conflicts with its base, and when it’s merged, never for an approval', of: 'comments, requests for changes, failing checks, conflicts and a merge, never an approval' },
};

/** What a nudge tells of, merges aside: what Settings' select picks, beside its merges box. */
export function nudgeKind(about: NudgeAbout): NudgeAbout {
  return about === 'feedback-and-merges' ? 'feedback' : about === 'feedback-checks-and-merges' ? 'feedback-and-checks' : about;
}

/** Whether a nudge tells of a merge (config.rs `NudgeAbout::merges`). */
export function nudgeMerges(about: NudgeAbout): boolean {
  return about === 'everything' || about === 'feedback-and-merges' || about === 'feedback-checks-and-merges';
}

/** `kind` (`nudgeKind`) with a merge told or not; everything tells of one either way. */
export function withMerges(kind: NudgeAbout, merges: boolean): NudgeAbout {
  const base = nudgeKind(kind);
  if (base === 'everything' || !merges) return base;
  return base === 'feedback' ? 'feedback-and-merges' : 'feedback-checks-and-merges';
}

/** What a session that isn't running is resumed to be told (nudges.rs `for_stopped`): what asks
 *  for work, never an approval or a merge. */
function forStopped(about: NudgeAbout): NudgeAbout {
  return about === 'everything' ? 'feedback-and-checks' : nudgeKind(about);
}

/** What a PR's card says under its nudge switch (nudges.rs): whether it's on and why, and who's
 *  told of what (`about`, `[pull_requests] nudge_about`). `primary` is its primary session's
 *  title, whether it runs on this Mac and what becomes of the news while it doesn't
 *  (`Board.prStopped`), when the card has it; only that session is ever told. */
export function nudgeWords(pr: Pick<PrItem, 'state' | 'nudgeByDefault'>, on: boolean, primary: { title: string; runs: boolean; stopped?: PrStopped; blocked?: boolean } | null, about: NudgeAbout = 'everything'): string {
  const said = NUDGED_WHEN[about] ?? NUDGED_WHEN.everything;
  if (primary?.blocked) {
    const lead = on ? (pr.nudgeByDefault ? 'On for every PR of yours, by Settings.' : 'On for this PR.') : 'Off for this PR.';
    return `${lead} “${primary.title}”, its primary session, was flagged by its provider. PR notifications are blocked in Settings › Automation.`;
  }
  if (on) {
    const lead = pr.nudgeByDefault ? 'On for every PR of yours, by Settings.' : 'On for this PR.';
    if (!primary) return `${lead} No session of it is known here, so nobody is told.`;
    if (!primary.runs && primary.stopped === 'resumes') {
      const work = NUDGED_WHEN[forStopped(about)] ?? NUDGED_WHEN['feedback-and-checks'];
      return `${lead} “${primary.title}”, its primary session, isn’t running here, so it’s resumed out of sight to be told ${work.when}.`;
    }
    if (!primary.runs && primary.stopped === 'beside') return `${lead} “${primary.title}”, its primary session, isn’t running here, and another of the PR’s sessions is, so it isn’t resumed beside that one: nobody is told until it runs.`;
    if (!primary.runs) return `${lead} “${primary.title}”, its primary session, isn’t running here, so nobody is told until it is.`;
    return `${lead} “${primary.title}”, its primary session, is sent a message as one of yours ${said.when}.`;
  }
  const who = primary ? `“${primary.title}”` : 'its primary session';
  return pr.state === 'approved' && about === 'everything'
    ? `Off for this PR. It’s approved already: turning this on tells ${who} that now, then of what happens on it next.`
    : `Off for this PR. Turned on, ${who} is told of ${said.of}.`;
}

/** What a PR's Primary tag says (board.rs `pr_primary`): what being its primary session means,
 *  why this one is (`primaryBy`), and where to pick another. */
export function primaryWords(pr: Pick<PrItem, 'primary' | 'primaryBy' | 'sessions'>): string {
  const pushes = pr.sessions.find((s) => s.key === pr.primary)?.pushes ?? 0;
  const why = {
    picked: 'It was picked as the primary one.',
    pushed: `It pushed to the PR’s branch most, ${count(pushes, 'time')}.`,
    opened: 'It opened the PR, and no other session pushed to it more.',
    started: 'No session’s records show it opened the PR or pushed to it, so it’s the one that started first.',
  }[pr.primaryBy ?? 'started'];
  return `The PR’s primary session: when the PR is nudged, only this one is told what happens on it. ${pr.primaryBy ? `${why} ` : ''}The PR’s menu picks another (Primary session).`;
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

/** The HTML tags people and bots write into GitHub comments, which GitHub draws and the Markdown
 *  view doesn't. Named, so that `Vec<String>` in a comment keeps its type. */
const HTML_TAG = /<\/?(?:a|b|br|details|div|em|h[1-6]|hr|i|img|li|ol|p|picture|source|span|strong|summary|sup|table|tbody|td|th|thead|tr|ul)\b[^>]*>/gi;

/** What a comment's text is read as, whichever starts first, code as the Markdown view reads it
 *  (ui/Markdown.tsx): a fenced block, from a line starting with a fence, indented or not, to the
 *  next such line or the text's end; a hidden HTML comment; a code span, which doesn't run past a
 *  blank line; and each of `<sub>` and `</sub>` on its own, so that one inside code or a comment
 *  ends nothing. A fence written inside code doesn't end it, and a backtick inside a hidden
 *  comment doesn't open code. `m`: `^` and `$` are a line's start and end. */
const PARTS = /^[ \t]*```[^\n]*(?:\n[\s\S]*?^[ \t]*```[^\n]*$|[\s\S]*)|<!--[\s\S]*?-->|`(?:[^`\n]|\n(?![ \t]*\n))+`|<sub>\s*|\s*<\/sub>/gim;

/** A GitHub comment's text as the Markdown view shows it (ui/Markdown.tsx): its hidden HTML
 *  comments dropped (Codealong's marks on the reviews it posts are some), the line saying an agent
 *  wrote it, which GitHub shows small (`<sub>`), in italics, and the HTML tags it draws left out,
 *  everywhere but in its code, which keeps every character. */
export function commentText(body: string): string {
  const code: string[] = [];
  let out = '';
  let from = 0;
  const prose = (text: string) => text.replace(/<br\s*\/?>/gi, '\n').replace(HTML_TAG, '');
  for (const m of body.matchAll(PARTS)) {
    out += prose(body.slice(from, m.index));
    const part = m[0];
    // Code is set aside as it is, its place held by a mark no comment has, so that running
    // blank lines together below never touches it.
    if (part.startsWith('<!--')) out += '';
    else if (/^\s*<\/?sub>\s*$/i.test(part)) out += '*';
    else out += `\u0000${code.push(part) - 1}\u0000`;
    from = m.index + part.length;
  }
  out += prose(body.slice(from));
  return out
    .replace(/\n{3,}/g, '\n\n')
    .trim()
    .replace(/\u0000(\d+)\u0000/g, (_, i: string) => code[Number(i)]);
}

/** A comment's text for a list: Markdown's marks dropped, on one line, cut short. */
export function commentPreview(body: string, max = 280): string {
  const text = commentText(body)
    .replace(/```[\s\S]*?```/g, ' [code] ')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' [image] ')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/[*_`>#]+/g, '')
    .replace(/\s+/g, ' ')
    .trim();
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

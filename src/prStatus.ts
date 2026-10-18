// Where a PR stands, as the PRs view and the PR panel show it: a pill for its review, one for
// its checks, and one for anything else that keeps GitHub from merging it. When nothing does,
// they fold into a single "Ready to merge". The PRs view also flags the rows that are your move.

import { count } from './format';
import type { AutoReview, Checks, Lane, Merge, NudgeAbout, PrComment, PrItem, PrState, Session } from './types';

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
   *  Review requested or Awaiting review. */
  autoReview?: AutoReview;
  base?: string | null;
  /** Whose reviews it waits on, yours and others': Review requested while anyone's is asked
   *  for, Awaiting review while nobody's is. */
  reviewRequested?: boolean;
  reviewersAsked?: string[];
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
      const asked = reviewersWords(pr);
      if (asked) return pill('plain', 'Review requested', asked);
      return pill('plain', 'Awaiting review', 'Not approved yet, and nobody is asked to review it');
  }
}

/** Where a PR's review stands, as its chip marks it after its number (primitives.tsx PrChip):
 *  what the chip's color can't tell apart, an approved PR being as green as an open one. `tone`
 *  is the chip's checks' (`.ca-checks.is-…`). */
export interface ReviewMark {
  key: 'asked' | 'agent' | 'approved' | 'changes';
  tone: 'attn' | 'pending' | 'pass' | 'fail';
  icon: string;
  title: string;
}

/** The chip's review mark: your review asked for (unless snoozed), Codealong's agent still
 *  reviewing it, approved, or changes asked for. None while it only awaits review, for a draft,
 *  whose icon says so, or once it's merged or closed. A worktree's PR (`Pr`) has only its state
 *  to go by; the board's (`PrItem`) says what's asked of you. */
export function reviewMark(pr: Pick<PrItem, 'state'> & Partial<Pick<PrItem, 'reviewRequested' | 'requestedBy' | 'snoozedUntil' | 'autoReview'>>): ReviewMark | null {
  if (!isOpen(pr.state) || pr.state === 'draft') return null;
  if (heldForAgent(pr)) return { key: 'agent', tone: 'pending', icon: 'hourglass', title: 'Codealong’s agent is reviewing it: it waits on your review once that’s done' };
  if (pr.reviewRequested && !pr.snoozedUntil) return { key: 'asked', tone: 'attn', icon: 'eye', title: pr.requestedBy ? `Your review is asked for, by ${pr.requestedBy}` : 'Your review is asked for' };
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
  const said = names.length > 1 ? `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}` : names[0];
  return `Waiting on a review from ${said}`;
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
  everything: { when: 'when someone else reviews it, asks for changes or comments, when it’s approved, merged or closed, and when its checks fail or its branch conflicts with its base', of: 'reviews, comments, an approval, a merge, failing checks and conflicts' },
  feedback: { when: 'when someone else asks for changes or comments, never for an approval or a merge', of: 'comments and requests for changes, never an approval or a merge' },
  'feedback-and-checks': { when: 'when someone else asks for changes or comments and when its checks fail or its branch conflicts with its base, never for an approval or a merge', of: 'comments, requests for changes, failing checks and conflicts, never an approval or a merge' },
};

/** What a PR's card says under its nudge switch (nudges.rs): whether it's on and why, and who's
 *  told of what (`about`, `[pull_requests] nudge_about`). `primary` is its primary session's
 *  title, whether it runs on this Mac and what becomes of the news while it doesn't
 *  (`whenStopped`), when the card has it; only that session is ever told. */
export function nudgeWords(pr: Pick<PrItem, 'state' | 'nudgeByDefault'>, on: boolean, primary: { title: string; runs: boolean; stopped?: Stopped } | null, about: NudgeAbout = 'everything'): string {
  const said = NUDGED_WHEN[about] ?? NUDGED_WHEN.everything;
  if (on) {
    const lead = pr.nudgeByDefault ? 'On for every PR of yours, by Settings.' : 'On for this PR.';
    if (!primary) return `${lead} No session of it is known here, so nobody is told.`;
    if (!primary.runs && primary.stopped === 'resumes') {
      const work = NUDGED_WHEN[about === 'everything' ? 'feedback-and-checks' : about] ?? NUDGED_WHEN['feedback-and-checks'];
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

/** What becomes of a PR's news while its primary session isn't running, as nudges.rs `resumable`
 *  has it: it's resumed out of sight to be told (`resumes`); not while another of the PR's
 *  sessions runs on a device that's connected, unless that one's a helper or a review (`beside`);
 *  or never, with `[sessions] resume_for_pr_news` off (`on`), or a session you archived or one on
 *  another device, whose own Codealong sees to it. */
export type Stopped = 'resumes' | 'beside' | 'never';

export function whenStopped(primary: { session: Session; lane: Lane }, sessions: readonly { session: Session; lane: Lane }[], on: boolean): Stopped {
  if (!on || primary.session.archived || primary.lane.device) return 'never';
  const runs = (x: { session: Session; lane: Lane }) => x.session.live && !x.session.archived && !x.session.helper && (!x.lane.device || x.lane.device.online);
  return sessions.some((x) => x.session.key !== primary.session.key && runs(x)) ? 'beside' : 'resumes';
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

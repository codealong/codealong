// What everyone said of a PR, by who said it, as its Reviews tab lists it under your own review
// (ui/PrReviewers.tsx): each person once, with where their review stands (their last approval or
// request for changes, else a comment), whether it's still asked of them, and everything they
// wrote on it, the conversation's comments, their reviews' text and the threads they began on
// the code alike. A verdict and its words are one entry, where GitHub's data has them apart
// (github.rs `parse_detail`: `reviews` holds each author's standing state, `comments` the words).

import type { PrComment, PrDetail } from './types';

/** Someone who reviewed the PR, is asked to, or wrote on it. */
export interface Reviewer {
  /** Their login, or a team's name for a team asked to review. */
  who: string;
  /** Whoever `gh` is signed in as. */
  mine: boolean;
  /** The PR's author, who can't review it but answers on it. */
  author: boolean;
  /** Their standing review (`PrReview.state`), none for someone who only wrote or is only asked. */
  state: string | null;
  /** When they gave it. */
  at: number | null;
  /** Their review is asked for, and they haven't given it since it was. */
  requested: boolean;
  /** What they wrote: what's still open first (`isOpen`), then the newest. */
  said: PrComment[];
  /** How many of `said` are still open. */
  open: number;
}

/** What GitHub calls a review's state, in words. */
export const REVIEW_WORD: Record<string, string> = { APPROVED: 'Approved', CHANGES_REQUESTED: 'Requested changes', COMMENTED: 'Commented', DISMISSED: 'Dismissed' };

/** A comment still to be dealt with: a thread on the code nobody resolved, or one Jev says asks
 *  for a change (prStatus.ts `asksFor`). */
export function isOpen(c: PrComment, asks: Set<string>): boolean {
  return asks.has(c.id) || (!!c.path && !c.resolved);
}

/** Who comes first: whoever asked for changes, then whoever commented, approved or was dismissed,
 *  then whoever is asked and hasn't reviewed, then whoever only wrote (the author, bots). */
function rank(r: Reviewer): number {
  if (r.state) return ['CHANGES_REQUESTED', 'COMMENTED', 'APPROVED', 'DISMISSED'].indexOf(r.state) + 1 || 4;
  return r.requested ? 5 : 6;
}

/** When they last did anything on it: their review or the newest of what they wrote. */
function lastAt(r: Reviewer): number {
  return Math.max(r.at ?? 0, ...r.said.map((c) => c.at ?? 0));
}

/**
 * Everyone on a PR, each once: by `rank`, then the one with more still open, then the latest.
 * `author` is the PR's author's login, `asks` the comments Jev says ask for a change.
 */
export function reviewersOf(detail: Pick<PrDetail, 'reviews' | 'requested' | 'comments'>, author: string | null, asks: Set<string>): Reviewer[] {
  const by = new Map<string, Reviewer>();
  const of = (who: string): Reviewer => {
    let r = by.get(who);
    if (!r) {
      r = { who, mine: false, author: who === author, state: null, at: null, requested: false, said: [], open: 0 };
      by.set(who, r);
    }
    return r;
  };
  for (const v of detail.reviews) {
    const r = of(v.author);
    Object.assign(r, { state: v.state, at: v.at, mine: r.mine || !!v.mine });
  }
  for (const who of detail.requested) of(who).requested = true;
  for (const c of detail.comments ?? []) {
    const r = of(c.author);
    r.said.push(c);
    if (c.mine) r.mine = true;
  }
  const all = [...by.values()];
  for (const r of all) {
    // Comments come newest first; what's open moves ahead of the rest, newest first among it.
    r.said = [...r.said.filter((c) => isOpen(c, asks)), ...r.said.filter((c) => !isOpen(c, asks))];
    r.open = r.said.filter((c) => isOpen(c, asks)).length;
  }
  return all.sort((a, b) => rank(a) - rank(b) || b.open - a.open || lastAt(b) - lastAt(a) || a.who.localeCompare(b.who));
}

/** What's still open from others than you: what the Reviews tab counts when you've no draft. */
export function openFromOthers(reviewers: Reviewer[]): number {
  return reviewers.filter((r) => !r.mine).reduce((n, r) => n + r.open, 0);
}

/** How many of someone's comments show before Show more: whatever's open, and at least this many. */
export const SAID_SHOWN = 3;

/** The comments of someone's that show: all, or what's open and at least `SAID_SHOWN`. */
export function saidShown(r: Reviewer, all: boolean): PrComment[] {
  return all ? r.said : r.said.slice(0, Math.max(r.open, SAID_SHOWN));
}

/** What the Reviews tab's count is and says: your draft's comments while you have a draft, else
 *  what's still open from others. Null for no count. */
export function reviewsCount(draft: { count: number } | null | undefined, open: number): { count: number | null; title: string } {
  if (draft) return { count: draft.count, title: 'Your draft review, to submit, then everyone’s reviews and comments' };
  if (open > 0) return { count: open, title: `Everyone’s reviews and comments: ${open} still open (threads on the code not resolved, comments that ask for a change)` };
  return { count: null, title: 'Your review, then everyone’s reviews and comments' };
}

/** Text long enough to show cut short until asked for whole. */
export const LONG_TEXT = 600;

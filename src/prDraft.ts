// Your review of a PR on GitHub, as its Reviews tab and its card's box know it (github.rs
// `your_review`): the draft review you haven't submitted, an agent's (autoreview.rs) or one you
// began on GitHub, with its comments, and the agent's review you did submit. It's read when
// something that shows the PR wants it (`useYourReview`), once for every part of the window, and
// again when the PR changes on GitHub or its agent review moves on. What's done to the draft here
// (its text or a comment changed, a comment taken out, the draft submitted) shows at once
// (`changeDraft`) and is read again after (`readAgain`).
//
// A verdict on a PR you have a draft of submits the draft (github.rs `review`), what's written
// going before its text, so what the windows say about a verdict says so too (`verdictWords`,
// `reviewedWords`).

import { useEffect, useSyncExternalStore } from 'react';
import { prDraft } from './api';
import { errorText } from './report';
import type { Draft, PrItem, Reviewed, Verdict, YourReview } from './types';

/** What's known of a PR's review on GitHub. */
export interface Held {
  /** What it was read for (`draftStamp`). */
  stamp: string;
  review?: YourReview;
  error?: string;
  loading: boolean;
}

const held = new Map<string, Held>();
const listeners = new Set<() => void>();
const tell = () => listeners.forEach((l) => l());
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};

/** How long what was read stands: until the PR changes on GitHub or its agent review moves. */
export function draftStamp(pr: Pick<PrItem, 'updatedAt' | 'autoReview'>): string {
  const a = pr.autoReview;
  return `${pr.updatedAt}|${a ? `${a.stage}@${a.at}:${a.rounds}` : ''}`;
}

function read(pr: Pick<PrItem, 'key' | 'repoSlug' | 'number'>, stamp: string) {
  const was = held.get(pr.key);
  held.set(pr.key, { stamp, review: was?.review, loading: true });
  tell();
  const settle = (next: Omit<Held, 'stamp' | 'loading'>) => {
    // A later read for the PR has the say.
    if (held.get(pr.key)?.stamp !== stamp) return;
    held.set(pr.key, { stamp, loading: false, ...next });
    tell();
  };
  prDraft(pr.repoSlug, pr.number).then(
    (review) => settle({ review }),
    (e: unknown) => settle({ review: was?.review, error: errorText(e) }),
  );
}

/** Your review of `pr` on GitHub, read when `want` and nothing read for it stands. Without
 *  `want` it's what another part of the window had read, if anything. */
export function useYourReview(pr: Pick<PrItem, 'key' | 'repoSlug' | 'number' | 'updatedAt' | 'autoReview'>, want: boolean): Held | undefined {
  const stamp = draftStamp(pr);
  const { key, repoSlug, number } = pr;
  useEffect(() => {
    if (!want) return;
    // Read or being read for this stamp already, by this part of the window or another.
    if (held.get(key)?.stamp === stamp) return;
    read({ key, repoSlug, number }, stamp);
  }, [want, key, repoSlug, number, stamp]);
  return useSyncExternalStore(subscribe, () => held.get(key));
}

/** Reads it again, after something was done to it here. */
export function readAgain(pr: Pick<PrItem, 'key' | 'repoSlug' | 'number' | 'updatedAt' | 'autoReview'>) {
  read(pr, draftStamp(pr));
}

/** Shows a change to the draft at once, ahead of the read that confirms it: `change` gives the
 *  draft as it is now, or null once it's submitted. */
export function changeDraft(key: string, change: (d: Draft) => Draft | null) {
  const h = held.get(key);
  const draft = h?.review?.draft;
  if (!h || !h.review || !draft) return;
  held.set(key, { ...h, review: { ...h.review, draft: change(draft) } });
  tell();
}

/** Its text as the Markdown view shows it: the line saying an agent wrote it, which GitHub
 *  shows small (`<sub>`), in italics. */
export function draftShown(body: string): string {
  return body.replace(/<sub>([\s\S]*?)<\/sub>/g, (_, inner: string) => `*${inner.trim()}*`).trim();
}

/** A draft's comments, in words: "4 comments". */
export function commentsWord(n: number): string {
  return `${n.toLocaleString('en-US')} ${n === 1 ? 'comment' : 'comments'}`;
}

/** Whether a verdict has something to say: what's written, or a draft with text or comments.
 *  An approval needs nothing. */
export function saysSomething(verdict: Verdict, written: string, draft: Draft | null | undefined): boolean {
  if (verdict === 'approve' || written.trim()) return true;
  return !!draft && (draft.body.trim().length > 0 || draft.count > 0);
}

const PAST: Record<Verdict, string> = { approve: 'Approved', comment: 'Commented on', 'request-changes': 'Requested changes on' };

/** What a window says once a review went: what was done, and the draft submitted with it. */
export function reviewedWords(verdict: Verdict, number: number, done: Reviewed | null | undefined, agents = false): string {
  const n = done?.draft;
  if (n == null) return agents ? `${PAST[verdict]} #${number} with the agent’s review.` : `${PAST[verdict]} #${number}.`;
  return `${PAST[verdict]} #${number}, submitting your draft review${n > 0 ? ` and its ${commentsWord(n)}` : ''}.`;
}

/** What a verdict's button says it does with a draft there: "Approve, submitting your draft
 *  review and its 4 comments". */
export function verdictWords(verdict: Verdict, draft: Draft | null | undefined): string | null {
  if (!draft) return null;
  const what = { approve: 'Approve', comment: 'Comment', 'request-changes': 'Request changes' }[verdict];
  return `${what}, submitting your draft review${draft.count > 0 ? ` and its ${commentsWord(draft.count)}` : ''}`;
}

/** How you submitted an agent's review, from GitHub's word for the review: "approving it". */
export function submittedWord(state: string): string {
  return { APPROVED: 'approving it', CHANGES_REQUESTED: 'asking for changes', COMMENTED: 'as a comment', DISMISSED: 'and it was dismissed' }[state] ?? state.toLowerCase().replace(/_/g, ' ');
}

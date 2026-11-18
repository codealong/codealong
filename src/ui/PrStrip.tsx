// The strip over a PR's header, wherever a window shows one PR with the box that reviews it: its
// card in the notch and the waiting window (PrCard.tsx) and its panel in the main window
// (PrInspector.tsx). It's the strip a session's card and panel have (SessionPanel.tsx
// `QueueStrip`), with what's commonly done to the PR in place of the session's: Approve and
// Comment, with what's written on Review as the review's text, Snooze, Review with and GitHub,
// each a button that says its key, the ones its menu runs (menus.ts `PrActions`). Its keys run
// them while it shows (`PR_CHORDS`): ⌘⏎ approves, ⌥⌘⏎ comments, ⌃⌘1 to ⌃⌘7 snooze the review,
// ⌃⌘R starts a review on Review with's first choice and ⌃⌘G opens it on GitHub. One that doesn't
// apply to the PR stays in its place, dimmed, and its key does nothing.
//
// A card's window is worked from the keyboard, so there its keys work from anywhere, and what
// takes the PR off it goes on to what waits next (`after`). The main window's panel stays where it
// is, and its list keeps ⌘⏎ for opening the row selected (mainKeys.ts): there ⌘⏎ and ⌥⌘⏎ give a
// verdict from the box (PrBox.tsx `ownKeys`), and the rest are the PR's from anywhere in the
// window while its panel shows it (`panel`), ahead of the keys of the row selected in the list,
// which may be the session the PR was shown from.

import { useEffect } from 'react';
import { openUrl } from '../api';
import { useLatest } from '../latest';
import * as menus from '../menus';
import { verdictWords } from '../prDraft';
import { errorText, type Report } from '../report';
import { reviewItems } from '../review';
import { reviewSnoozeMenu } from '../snooze';
import type { Lane, PrItem, ReviewsCfg, Usage, Verdict } from '../types';
import { dialogUp } from './focusTrap';
import type { PrBox } from './PrBox';
import { QueueStrip, snoozeKeys, snoozeRange, type Hotkeys, type Notice, type Queue, type StripAction } from './SessionPanel';

/** What a PR's actions need of the window that shows it. */
export interface PrActionsHost {
  report: Report;
  /** The board's worktrees and plan usage, for where a review starts and on which account. */
  lanes: Lane[];
  usage: Usage[];
  /** The board's `[reviews]` for the PR's repo, for whom Review with… starts on by default. */
  reviews?: ReviewsCfg | null;
  now: number;
  /** It left what waits on you, and how: a card's window goes on, or back to the session it was
   *  shown from. None where the window stays on it (the panel). */
  leave?: (done: menus.Left) => void;
}

/** How a card's window says the PR left with each verdict. */
export const LEFT: Record<Verdict, menus.Left> = { approve: 'approved', comment: 'commented on', 'request-changes': 'changes requested' };

/** What's commonly done to a PR shown with its box, by name: what its strip, its keys and its
 *  menu run. A verdict goes with what's written on Review. */
export function prActions(pr: PrItem, box: PrBox, { report, lanes, usage, reviews, now, leave }: PrActionsHost): menus.PrActions {
  const { reviewable, review } = box;
  const reviewWith = reviewItems(pr, lanes, box.accounts, usage, report, () => leave?.('review started'), now, reviews, box.groups);
  return {
    approve: reviewable ? { label: `Approve #${pr.number}`, run: () => review('approve') } : undefined,
    // A comment still goes to a PR that's closed, as from the box.
    comment: { label: 'Comment', run: () => review('comment') },
    changes: reviewable ? { label: 'Request changes', run: () => review('request-changes') } : undefined,
    review: reviewWith.length > 0 ? { label: 'Review with', items: reviewWith } : undefined,
    // As a session's snooze settles (menus.ts `settle`): the review has left what waits on you
    // at once, and what goes wrong is reported after.
    snooze: reviewSnoozeMenu(
      pr,
      now,
      (act) => {
        leave?.('snoozed');
        act.catch((e: unknown) => report.error(errorText(e)));
      },
      true,
    ),
    github: { label: 'Open on GitHub', run: () => void openUrl(pr.url).catch((e: unknown) => report.error(errorText(e))) },
  };
}

interface PrStripProps {
  pr: PrItem;
  actions: menus.PrActions;
  /** The box the verdicts go from: what they'd submit with what's written, and whether one is on
   *  its way. */
  box: PrBox;
  /** Where the window goes once a verdict takes the PR off it: on to what waits next, or back to
   *  the session it was shown from. None where it stays (the panel). */
  after?: 'on' | 'back';
  queue?: Queue;
  notice?: Notice | null;
  hotkeys: Hotkeys;
  /** Over the PR's panel in the main window, whose list has ⌘⏎ for itself: ⌘⏎ and ⌥⌘⏎ give a
   *  verdict from the box alone (PrBox.tsx `ownKeys`), and the rest go ahead of the list's. */
  panel?: boolean;
  /** The PR shown is on its way out: the window asked for another, which hasn't come yet. Its
   *  keys do nothing until then. */
  outgoing?: boolean;
}

export function PrStrip({ pr, actions, box, after, queue, notice, hotkeys, panel = false, outgoing = false }: PrStripProps) {
  const latest = useLatest(actions);
  const leaving = useLatest(outgoing);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Under a dialog (the merge box's), its keys are the dialog's: ⌘⏎ there isn't Approve.
      if (e.defaultPrevented || e.isComposing || dialogUp()) return;
      const name = menus.prChordOf(e);
      if (!name || (panel && (name === 'approve' || name === 'comment'))) return;
      // Taken whether it applies or not, so that what's behind the PR (the row selected in the
      // main window's list) doesn't take it instead.
      e.preventDefault();
      const action = menus.prChorded(latest.current, e);
      if (action && !leaving.current) action.run();
    };
    // The main window's keys may have been listening since before the panel showed the PR (it's
    // shown from a session's chip, the session still selected), so in the panel these go first, as
    // the event comes down. A card's window has nothing else for them, and listens as the event
    // comes back up, after what in the card has ⌘⏎ for itself (a draft comment's editor's).
    window.addEventListener('keydown', onKey, panel);
    return () => window.removeEventListener('keydown', onKey, panel);
  }, [latest, leaving, panel]);

  const sign = (name: keyof typeof menus.PR_CHORDS) => menus.chordSigns(menus.PR_CHORDS[name] ?? '');
  const notReviewable = pr.mine ? 'GitHub doesn’t let you approve your own PR' : 'It’s closed';
  const then = after ? `, and go ${after}` : '';
  // Where the verdicts' keys are the box's, the buttons say so.
  const fromBox = (name: 'approve' | 'comment') => (panel ? ` (${sign(name)} from the box)` : '');
  // What a verdict submits, as the box's buttons say it: the agent's review, your draft review on
  // GitHub with its comments, or what's written on Review alone.
  const says = (verdict: 'approve' | 'comment', alone: string) =>
    box.agents === verdict
      ? `${verdict === 'approve' ? 'Approve' : 'Comment'} with the agent’s review, what’s written on Review going first`
      : box.draft
        ? `${verdictWords(verdict, box.draft) ?? ''}, what’s written on Review going first`
        : alone;
  const { approve, comment, snooze, review, github } = actions;
  const first = review?.items[0];
  const sending = box.sending ? 'Sending a review to GitHub' : null;
  const buttons: StripAction[] = [
    { label: 'Approve', chord: menus.PR_CHORDS.approve, title: sending ?? (approve ? `${says('approve', 'Approve it, with what’s written on Review as the review’s text')}${then}${fromBox('approve')}` : notReviewable), run: approve && !sending ? () => approve.run() : undefined },
    { label: 'Comment', chord: menus.PR_CHORDS.comment, title: sending ?? `${box.stillOpen ? '' : 'It’s closed, and a comment still goes to it. '}${says('comment', 'Comment on it with what’s written on Review')}${then}${fromBox('comment')}`, run: comment && !sending ? () => comment.run() : undefined },
    { label: 'Snooze', keys: snoozeRange(), title: snooze ? `Snooze the review: it waits apart until its time comes. ${snoozeKeys(snooze)}` : pr.reviewRequested ? 'The review can’t be snoozed' : 'Your review isn’t asked for: there’s nothing to snooze', menu: snooze?.items },
    { label: 'Review with', chord: menus.PR_CHORDS.review, title: review ? `Start Codealong’s review of it on an account${first && first !== 'separator' && !('items' in first) ? `; the key starts it: ${first.label}` : ''}` : 'No account can start a review here', menu: review?.items },
    { label: 'GitHub', chord: menus.PR_CHORDS.github, title: 'Open it on GitHub', run: () => github.run() },
  ];
  return <QueueStrip queue={queue} notice={notice} hotkeys={hotkeys} buttons={buttons} />;
}

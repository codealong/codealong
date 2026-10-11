// A PR waiting on your review as a card of its own, wherever a window works through what waits
// on you with a PR among the sessions: under the pills of the bar docked in the notch
// (NotchReview.tsx) and in the waiting window (Waiting.tsx). It's the PR panel's details
// (PrInspector.tsx `PrDetails`) with what's the same in both done once: the strip over the header
// with what's commonly done to the PR as buttons that say their keys, the box at its foot that
// reviews it, the PR's menu on a right-click, and its chords (menus.ts `PR_CHORDS`): ⌘⏎
// approves it with what's in the box and goes on, as it sends a session's message and goes on;
// ⌥⌘⏎ comments; ⌃⌘1 to ⌃⌘4 snooze the review; ⌃⌘R starts a review on an account (review.ts);
// ⌃⌘G opens it on GitHub. Asking for changes is a button in the box, with no key to press by
// mistake, and needs something written, as a comment does. It draws at the zoom, and ⌘= ⌘- ⌘0
// zoom it, as a session's card (SessionCard.tsx).
//
// Any review settles GitHub's request to you, so each of those is how the PR left what waits on
// you (`can.left`), and the window goes on to the next at once, as it does from a session's
// message on its way: the review goes to GitHub behind it. One GitHub refuses comes back round
// (`can.failed`): the PR is back in what waits on you with what you wrote, and the strip says
// why. A merge from the merge box under its title (MergeBox.tsx `goesOn`) leaves the same way.
// What you wrote stays with the PR until it goes, so skipping to the next and coming back loses
// nothing. The box is the PR panel's too (PrBox.tsx), with the line over it saying where the PR's
// review stands, which shows its Review tab; a verdict's keys work from either of its sides.
//
// A window says only what differs: how it reports, how it opens things and shows them in
// Codealong, what it does once the PR has left, and what it adds (a queue).

import { useEffect, useRef, type ReactNode } from 'react';
import { openUrl } from '../api';
import { below, showContextMenu, type MenuAction, type MenuSpec } from '../contextMenu';
import { useLatest } from '../latest';
import * as menus from '../menus';
import { errorText, type Report } from '../report';
import { reviewItems } from '../review';
import { reviewSnoozeMenu } from '../snooze';
import type { Board, Lane, Session, Usage, Verdict } from '../types';
import { cardZoom } from '../zoom';
import { zoomKey } from './cardHost';
import { PrBoxView, usePrBox } from './PrBox';
import { PrDetails, usePrTab } from './PrInspector';
import type { PrRowItem } from './PrsView';
import { Button, cx } from './primitives';
import { Hotkey, QueueStrip, snoozeKeys, snoozeRange, type Hotkeys, type Notice, type Queue } from './SessionPanel';

/** What a window does for a PR's actions, less what the card knows for every window. */
export interface PrWindowCan {
  report: Report;
  /** Opens one of a worktree's targets: a session where it lives. */
  open: (lane: Lane, target: string) => void;
  /** Shows it in the main window, from the other windows. */
  reveal?: () => void;
  /** It left what waits on you, and how: the window goes on to the next. */
  left?: (done: menus.Left) => void;
  /** The review it left on didn't go (GitHub refused it): it's back in what waits on you, and
   *  the window says why, on whatever it shows now. */
  failed?: (why: string) => void;
}

interface PrCardProps {
  item: PrRowItem;
  now: number;
  /** The board's worktrees and plan usage, for where a review starts and on which account. */
  lanes: Lane[];
  usage: Usage[];
  can: PrWindowCan;
  /** The PR shown is on its way out: the window asked for another, which hasn't come yet. Its
   *  chords do nothing until then. */
  outgoing?: boolean;
  queue?: Queue;
  notice?: Notice | null;
  /** The strip is always there, with the PR's common actions and the keys for them. */
  hotkeys?: Hotkeys;
  /** Puts the cursor in the box. */
  autoFocus?: boolean;
  onShowSession: (lane: Lane, session: Session) => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  onShowLane: (lane: Lane) => void;
  onLaneMenu: (lane: Lane) => void;
  onSessionMenu: (lane: Lane, session: Session) => void;
  /** It draws at the zoom, in percent, as a session's card does (zoom.ts `cardZoom`). */
  zoom?: number;
  /** The board's apps: its changed files open in your editors (PrInspector.tsx `fileOpener`). */
  apps?: Board['apps'];
}

/** How the window says the PR left with each verdict. */
const LEFT: Record<Verdict, menus.Left> = { approve: 'approved', comment: 'commented on', 'request-changes': 'changes requested' };

export function PrCard({ item, now, lanes, usage, can, outgoing = false, queue, notice, hotkeys, autoFocus, onShowSession, onOpenSession, onShowLane, onLaneMenu, onSessionMenu, zoom, apps }: PrCardProps) {
  const { pr } = item;
  const { report } = can;
  const box = usePrBox(pr, { report, usage, now, autoFocus, onSent: (verdict) => can.left?.(LEFT[verdict]), onFailed: can.failed });
  const { reviewable, stillOpen, review } = box;
  const tabs = usePrTab(pr);
  const onDesktop = menus.useDesktopPins();

  const open = (url: string) => void openUrl(url).catch((e: unknown) => report.error(errorText(e)));
  const show = (spec: MenuSpec, at?: { x: number; y: number }) => void showContextMenu(spec, at).catch((e: unknown) => report.error(errorText(e)));

  const reviewWith = reviewItems(pr, lanes, box.accounts, usage, report, () => can.left?.('review started'), now);
  const actions: menus.PrActions = {
    approve: reviewable ? { label: `Approve #${pr.number}`, run: () => review('approve') } : undefined,
    comment: stillOpen ? { label: 'Comment', run: () => review('comment') } : undefined,
    changes: reviewable ? { label: 'Request changes', run: () => review('request-changes') } : undefined,
    review: reviewWith.length > 0 ? { label: 'Review with', items: reviewWith } : undefined,
    // As a session's snooze settles (menus.ts `settle`): the review has left what waits on you
    // at once, and what goes wrong is reported after.
    snooze: reviewSnoozeMenu(
      pr,
      now,
      (act) => {
        can.left?.('snoozed');
        act.catch((e: unknown) => report.error(errorText(e)));
      },
      true,
    ),
    github: { label: 'Open on GitHub', run: () => open(pr.url) },
  };

  // The PR's chords, for the card shown, and ⌘= ⌘- ⌘0, which zoom it as a session's card. Not for
  // one on its way out.
  const latest = useRef(actions);
  latest.current = actions;
  const leaving = useRef(outgoing);
  leaving.current = outgoing;
  const reportNow = useLatest(report);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      if (zoomKey(e, reportNow.current)) return;
      const action = menus.prChorded(latest.current, e);
      if (!action) return;
      e.preventDefault();
      if (leaving.current) return;
      action.run();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [reportNow]);

  const menu = (at?: { x: number; y: number }) =>
    show(
      menus.prMenu(pr, {
        report,
        open: () => open(pr.url),
        review: actions.review?.items.filter((x): x is MenuAction => x !== 'separator' && !('items' in x)),
        reveal: can.reveal,
        approve: actions.approve?.run,
        snooze: actions.snooze,
        desktop: onDesktop,
      }),
      at,
    );

  // A merge leaves what waits on you as a review does: the window goes on at once, and one GitHub
  // refuses comes back round.
  const left = can.left;
  const goesOn = left && { left: () => left('merged'), failed: (why: string) => (can.failed ? can.failed(why) : report.error(why)) };
  const sign = (name: keyof typeof menus.PR_CHORDS) => menus.chordSigns(menus.PR_CHORDS[name] ?? '');
  const notReviewable = pr.mine ? 'GitHub doesn’t let you approve your own PR' : 'It’s closed';
  const snooze = actions.snooze;
  const buttons: ReactNode = (
    <>
      <Hotkey label="Approve" keys={sign('approve')} title={actions.approve ? 'Approve it, with what’s written on Review as the review’s text, and go on' : notReviewable} run={actions.approve && (() => actions.approve?.run())} />
      <Hotkey label="Comment" keys={sign('comment')} title={actions.comment ? 'Comment on it with what’s written on Review, and go on' : 'It’s closed'} run={actions.comment && (() => actions.comment?.run())} />
      <Hotkey label="Snooze" keys={snoozeRange()} title={snooze ? `Snooze the review: it waits apart until its time comes. ${snoozeKeys(snooze)}` : 'The review can’t be snoozed'} run={snooze && ((el) => void showContextMenu(snooze.items, below(el)).catch(() => {}))} />
      <Hotkey label="Review with" keys={sign('review')} title={actions.review ? `Start a review on an account; the key starts it ${actions.review.items[0] && actions.review.items[0] !== 'separator' && !('items' in actions.review.items[0]) ? actions.review.items[0].label.replace(/^Auto \([^)]*\) /, 'on Auto ') : ''}` : 'No account can start a review here'} run={actions.review && ((el) => void showContextMenu(actions.review?.items ?? [], below(el)).catch(() => {}))} />
      <Hotkey label="GitHub" keys={sign('github')} title="Open it on GitHub" run={() => actions.github.run()} />
    </>
  );

  return (
    <div
      className={cx('ca-reply', 'ca-prreview')}
      style={cardZoom(zoom)}
      onContextMenu={(e) => {
        if (e.defaultPrevented || (e.target instanceof Element && e.target.closest('input, textarea, [contenteditable="true"]'))) return;
        e.preventDefault();
        menu({ x: e.clientX, y: e.clientY });
      }}
    >
      <QueueStrip queue={queue} notice={notice} hotkeys={hotkeys} buttons={buttons} />
      <div className="ca-prreview-top">
        <Button variant="ghost" size="sm" icon="dots" aria-label="More" aria-haspopup="menu" title="Snooze, review with, pin, copy and more" onClick={(e) => menu(below(e.currentTarget))} />
      </div>
      <div className="ca-prreview-body ca-prinsp">
        <PrDetails item={item} now={now} keys goesOn={goesOn} tabs={tabs} report={report} apps={apps} onShowSession={onShowSession} onOpenSession={onOpenSession} onShowLane={onShowLane} onLaneMenu={onLaneMenu} onSessionMenu={onSessionMenu} />
      </div>
      <PrBoxView pr={pr} box={box} keys={{ approve: sign('approve'), comment: sign('comment') }} goesOn={!!can.left} onShowReview={() => tabs.show('review')} />
    </div>
  );
}

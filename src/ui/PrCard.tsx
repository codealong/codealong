// A PR as a card of its own, wherever a window works through what waits on you with a PR among
// the sessions: under the pills of the bar docked in the notch (NotchReview.tsx) and in the
// waiting window (Waiting.tsx). It's the PR panel's details
// (PrInspector.tsx `PrDetails`) with what's the same in both done once: the strip over the header
// with what's commonly done to the PR as buttons that say their keys, the box at its foot that
// reviews it, the PR's menu on a right-click, and its chords (menus.ts `PR_CHORDS`): ⌘⏎
// approves it with what's in the box and goes on, as it sends a session's message and goes on;
// ⌥⌘⏎ comments; ⌃⌘1 to ⌃⌘7 snooze the review; ⌃⌘R starts a review on an account (review.ts);
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
// review stands, which shows its Reviews tab; a verdict's keys work from either of its sides.
// Its ⋯ is the details' own, on the title's line, as the PR panel has it.
//
// Those windows show any PR the board lists this way, not only one waiting on your review: a
// click on a session's PR chip, or on a link to the PR in its conversation, shows the PR's card in
// the session's place (`from`), as the main window's panel does (MainPanel.tsx `drill`). The card
// then leads back to the session, and once the PR has left as above, the window goes back to it
// rather than on to what waits next, saying there how the PR went. Snooze is dimmed on the strip
// while your review isn't asked for.
//
// A window says only what differs: how it reports, how it opens things and shows them in
// Codealong, what it does once the PR has left, and what it adds (a queue).

import { useEffect, useRef } from 'react';
import { openUrl } from '../api';
import { showContextMenu, type MenuAction, type MenuSpec } from '../contextMenu';
import { middleTruncate } from '../format';
import { useLatest } from '../latest';
import * as menus from '../menus';
import { wentWords } from '../queue';
import { errorText, type Report } from '../report';
import { reviewItems } from '../review';
import { reviewSnoozeMenu } from '../snooze';
import type { Board, Lane, ReviewsCfg, Session, TeamFile, Usage, Verdict } from '../types';
import { cardZoom } from '../zoom';
import { reloadKey, zoomKey } from './cardHost';
import { PrBoxView, usePrBox } from './PrBox';
import { PrDetails, usePrTab } from './PrInspector';
import type { PrRowItem } from './PrsView';
import { cx, type PanelBackTo } from './primitives';
import { QueueStrip, snoozeKeys, snoozeRange, type Hotkeys, type Notice, type Queue, type StripAction } from './SessionPanel';

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
  /** The board's `[reviews]`, for whom Review with… starts on by default. */
  reviews?: ReviewsCfg | null;
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
  /** The repos' team files, for its Reviews tab (AgentReview.tsx). */
  teamFiles?: TeamFile[];
  /** The session it's shown in place of, and the way back to it, which says there how the PR
   *  went once it's left; the window goes back in place of `can.left`. */
  from?: CameFrom & { onBack: (said?: string) => void };
}

/** The session a PR's card was shown in place of: a click on its PR chip, or on a link to the PR
 *  in its conversation. */
export interface CameFrom {
  key: string;
  title: string;
}

/** How the window says the PR left with each verdict. */
const LEFT: Record<Verdict, menus.Left> = { approve: 'approved', comment: 'commented on', 'request-changes': 'changes requested' };

export function PrCard({ item, now, lanes, usage, reviews, can, outgoing = false, queue, notice, hotkeys, autoFocus, onShowSession, onOpenSession, onShowLane, onLaneMenu, onSessionMenu, zoom, apps, teamFiles, from }: PrCardProps) {
  const { pr } = item;
  const { report } = can;
  // Where the window goes once the PR has left: back to the session it was shown from, else on.
  const leave: ((done: menus.Left) => void) | undefined = from ? (done) => from.onBack(wentWords(`#${pr.number}`, `${done}.`)) : can.left;
  const after = from ? 'back' : leave ? 'on' : undefined;
  const back: PanelBackTo | undefined = from && { label: middleTruncate(from.title, 40), onBack: () => from.onBack() };
  const box = usePrBox(pr, { report, usage, now, autoFocus, onSent: (verdict) => leave?.(LEFT[verdict]), onFailed: can.failed });
  const { reviewable, stillOpen, review } = box;
  const tabs = usePrTab(pr);
  const onDesktop = menus.useDesktopPins();

  const open = (url: string) => void openUrl(url).catch((e: unknown) => report.error(errorText(e)));
  const show = (spec: MenuSpec, at?: { x: number; y: number }) => void showContextMenu(spec, at).catch((e: unknown) => report.error(errorText(e)));

  const reviewWith = reviewItems(pr, lanes, box.accounts, usage, report, () => leave?.('review started'), now, reviews, box.groups);
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
        leave?.('snoozed');
        act.catch((e: unknown) => report.error(errorText(e)));
      },
      true,
    ),
    github: { label: 'Open on GitHub', run: () => open(pr.url) },
  };

  // The PR's chords, for the card shown, ⌘= ⌘- ⌘0, which zoom it as a session's card, and ⌘R,
  // which reloads it. Not for one on its way out.
  const latest = useRef(actions);
  latest.current = actions;
  const leaving = useRef(outgoing);
  leaving.current = outgoing;
  const reportNow = useLatest(report);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      if (zoomKey(e, reportNow.current) || reloadKey(e, reportNow.current)) return;
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
        lanes,
      }),
      at,
    );

  // A merge leaves what waits on you as a review does: the window goes on (or back) at once, and
  // one GitHub refuses comes back round.
  const goesOn = leave && { left: () => leave('merged'), failed: (why: string) => (can.failed ? can.failed(why) : report.error(why)) };
  const sign = (name: keyof typeof menus.PR_CHORDS) => menus.chordSigns(menus.PR_CHORDS[name] ?? '');
  const notReviewable = pr.mine ? 'GitHub doesn’t let you approve your own PR' : 'It’s closed';
  const snooze = actions.snooze;
  const then = after ? `, and go ${after}` : '';
  const buttons: StripAction[] = [
    { label: 'Approve', chord: menus.PR_CHORDS.approve, title: actions.approve ? `Approve it, with what’s written on Review as the review’s text${then}` : notReviewable, run: actions.approve && (() => actions.approve?.run()) },
    { label: 'Comment', chord: menus.PR_CHORDS.comment, title: actions.comment ? `Comment on it with what’s written on Review${then}` : 'It’s closed', run: actions.comment && (() => actions.comment?.run()) },
    { label: 'Snooze', keys: snoozeRange(), title: snooze ? `Snooze the review: it waits apart until its time comes. ${snoozeKeys(snooze)}` : pr.reviewRequested ? 'The review can’t be snoozed' : 'Your review isn’t asked for: there’s nothing to snooze', menu: snooze?.items },
    { label: 'Review with', chord: menus.PR_CHORDS.review, title: actions.review ? `Start Codealong’s review of it on an account${actions.review.items[0] && actions.review.items[0] !== 'separator' && !('items' in actions.review.items[0]) ? `; the key starts it: ${actions.review.items[0].label}` : ''}` : 'No account can start a review here', menu: actions.review?.items },
    { label: 'GitHub', chord: menus.PR_CHORDS.github, title: 'Open it on GitHub', run: () => actions.github.run() },
  ];

  return (
    <div
      className={cx('ca-reply', 'ca-prreview')}
      data-find="files"
      style={cardZoom(zoom)}
      onContextMenu={(e) => {
        if (e.defaultPrevented || (e.target instanceof Element && e.target.closest('input, textarea, [contenteditable="true"]'))) return;
        e.preventDefault();
        menu({ x: e.clientX, y: e.clientY });
      }}
    >
      <QueueStrip queue={queue} notice={notice} hotkeys={hotkeys} buttons={buttons} />
      <div className="ca-prreview-body ca-prinsp">
        <PrDetails item={item} now={now} onMenu={menu} keys goesOn={goesOn} back={back} tabs={tabs} report={report} apps={apps} teamFiles={teamFiles} onShowSession={onShowSession} onOpenSession={onOpenSession} onShowLane={onShowLane} onLaneMenu={onLaneMenu} onSessionMenu={onSessionMenu} />
      </div>
      <PrBoxView pr={pr} box={box} keys={{ approve: sign('approve'), comment: sign('comment') }} after={after} onShow={tabs.show} />
    </div>
  );
}

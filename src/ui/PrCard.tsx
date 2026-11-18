// A PR as a card of its own, wherever a window works through what waits on you with a PR among
// the sessions: under the pills of the bar docked in the notch (NotchReview.tsx) and in the
// waiting window (Waiting.tsx). It's the PR panel's details
// (PrInspector.tsx `PrDetails`) with what's the same in both done once: the strip over the header
// with what's commonly done to the PR as buttons that say their keys, which work from anywhere in
// the window (PrStrip.tsx, the panel's strip too), the box at its foot that reviews it, and the
// PR's menu on a right-click. ⌘⏎ approves it with what's in the box and goes on, as it sends a
// session's message and goes on; ⌥⌘⏎ comments; ⌃⌘1 to ⌃⌘7 snooze the review; ⌃⌘R starts a review
// on an account (review.ts); ⌃⌘G opens it on GitHub. Asking for changes is a button in the box,
// with no key to press by mistake, and needs something written, as a comment does. It draws at the
// zoom, and ⌘= ⌘- ⌘0 zoom it, as a session's card (SessionCard.tsx).
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

import { useEffect } from 'react';
import { openUrl } from '../api';
import { showContextMenu, type MenuAction, type MenuSpec } from '../contextMenu';
import { middleTruncate } from '../format';
import { useLatest } from '../latest';
import * as menus from '../menus';
import { wentWords } from '../queue';
import { errorText, type Report } from '../report';
import type { Board, Lane, ReviewsCfg, Session, TeamFile, Usage } from '../types';
import { cardZoom } from '../zoom';
import { reloadKey, zoomKey } from './cardHost';
import { PrBoxView, usePrBox } from './PrBox';
import { PrDetails, usePrTab } from './PrInspector';
import type { PrRowItem } from './PrsView';
import { LEFT, prActions, PrStrip } from './PrStrip';
import { cx, DialogHostContext, useDialogPlace, type PanelBackTo } from './primitives';
import type { Hotkeys, Notice, Queue } from './SessionPanel';

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
  /** The strip's own: back, and putting the card away (PrStrip.tsx). */
  hotkeys: Hotkeys;
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

export function PrCard({ item, now, lanes, usage, reviews, can, outgoing = false, queue, notice, hotkeys, autoFocus, onShowSession, onOpenSession, onShowLane, onLaneMenu, onSessionMenu, zoom, apps, teamFiles, from }: PrCardProps) {
  const { pr } = item;
  const { report } = can;
  // Where the window goes once the PR has left: back to the session it was shown from, else on.
  const leave: ((done: menus.Left) => void) | undefined = from ? (done) => from.onBack(wentWords(`#${pr.number}`, `${done}.`)) : can.left;
  const after = from ? 'back' : leave ? 'on' : undefined;
  const back: PanelBackTo | undefined = from && { label: middleTruncate(from.title, 40), onBack: () => from.onBack() };
  const box = usePrBox(pr, { report, usage, now, autoFocus, onSent: (verdict) => leave?.(LEFT[verdict]), onFailed: can.failed });
  const tabs = usePrTab(pr);
  const onDesktop = menus.useDesktopPins();
  // The merge box's dialog covers the card, as a session's card has its dialogs.
  const [placeDialogs, dialogHost] = useDialogPlace();

  const open = (url: string) => void openUrl(url).catch((e: unknown) => report.error(errorText(e)));
  const show = (spec: MenuSpec, at?: { x: number; y: number }) => void showContextMenu(spec, at).catch((e: unknown) => report.error(errorText(e)));
  const actions = prActions(pr, box, { report, lanes, usage, reviews, now, leave });

  // ⌘= ⌘- ⌘0 zoom the card shown as a session's card, and ⌘R reloads it; the PR's own keys are
  // its strip's (PrStrip.tsx).
  const reportNow = useLatest(report);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      if (!zoomKey(e, reportNow.current)) reloadKey(e, reportNow.current);
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

  return (
    <div
      ref={placeDialogs}
      className={cx('ca-reply', 'ca-prreview')}
      data-find="files"
      style={cardZoom(zoom)}
      onContextMenu={(e) => {
        if (e.defaultPrevented || (e.target instanceof Element && e.target.closest('input, textarea, [contenteditable="true"]'))) return;
        e.preventDefault();
        menu({ x: e.clientX, y: e.clientY });
      }}
    >
      <DialogHostContext.Provider value={dialogHost}>
        <PrStrip pr={pr} actions={actions} box={box} after={after} queue={queue} notice={notice} hotkeys={hotkeys} outgoing={outgoing} />
        <div className="ca-prreview-body ca-prinsp">
          <PrDetails item={item} now={now} onMenu={menu} keys goesOn={goesOn} back={back} tabs={tabs} report={report} apps={apps} teamFiles={teamFiles} onShowSession={onShowSession} onOpenSession={onOpenSession} onShowLane={onShowLane} onLaneMenu={onLaneMenu} onSessionMenu={onSessionMenu} />
        </div>
        <PrBoxView pr={pr} box={box} keys={{ approve: sign('approve'), comment: sign('comment') }} after={after} onShow={tabs.show} tab={tabs.tab} />
      </DialogHostContext.Provider>
    </div>
  );
}

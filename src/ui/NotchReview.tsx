// A PR waiting on your review, under the pills of the bar docked in the notch (Notch.tsx), in the
// place a session takes (NotchSession.tsx): a click on the reviews pill, or on the review's count
// beside the camera, opens the notch into the PR's card (PrCard.tsx), dark as the notch is, with
// the strip over its header, its details and the box that reviews it. It works through what waits
// on you with the same keys as a session there, from the same code (queue.ts, useWaitingQueue.ts),
// the reviews among the sessions in the bar's order (bar.ts `notchRows`): ⌘⏎ approves it and goes
// on to the next that waits on you, ⌃⌘1 to ⌃⌘7 snooze the review and go on, ⌘] skips, and so on.
// Approved, commented on, asked for changes, snoozed or handed to a review session, the PR leaves
// what waits on you and the next takes its place; with none left the notch closes.
//
// Any PR the board lists shows here the same way from a session's PR chip, or from a link to it in
// the session's conversation (`from`): its card leads back to the session, and the notch goes back
// there once the PR has left (PrCard.tsx).
//
// The card reads its PR from the board while it's up (cardHost.ts `useLivePr`): the pill carries
// only the PR's key and title.

import { useEffect, useRef } from 'react';
import { prCardItem } from '../boardLists';
import { showContextMenu } from '../contextMenu';
import { useEditorIcons } from '../editors';
import * as menus from '../menus';
import { errorText } from '../report';
import { repoReviews } from '../repoSettings';
import type { Lane, Session } from '../types';
import { useCardHost, useLivePr } from './cardHost';
import { PrCard, type CameFrom, type PrWindowCan } from './PrCard';
import type { Notice, Queue } from './SessionPanel';

interface NotchReviewProps {
  /** The PR to show, by its key. */
  prKey: string;
  now: number;
  queue: Queue | undefined;
  notice: Notice | null;
  /** Back to the one looked at before (⌃-); null with none. */
  onBack: (() => void) | null;
  /** It has the keyboard; a click in it takes it. */
  keys: boolean;
  onKeys: () => void;
  /** Shows a session in the notch in its place: one that worked on the PR. */
  onShow: (key: string) => void;
  /** The session it was shown in place of, from its chip, and the way back there, which says how
   *  the PR went (NotchStageState `returnFrom`). */
  from: CameFrom | null;
  onReturn: (key: string, said?: string) => void;
  onClose: () => void;
  /** The PR left what waits on you, and how: on to the next. */
  onLeft: (key: string, done: menus.Left) => void;
  /** The review it left on didn't go: back among what waits, with why. */
  onFailed: (key: string, why: string) => void;
  /** The board no longer lists it. */
  onGone: (key: string) => void;
  /** The zoom the card draws at, in percent, from the bar's feed (zoom.ts). */
  zoom?: number;
}

export function NotchReview({ prKey, now, queue, notice, onBack, keys, onKeys, onShow, from, onReturn, onClose, onLeft, onFailed, onGone, zoom }: NotchReviewProps) {
  const live = useLivePr(prKey, true);
  const gone = useRef(onGone);
  gone.current = onGone;
  useEffect(() => {
    if (live.gone) gone.current(prKey);
  }, [live.gone, prKey]);

  const apps = live.apps ?? undefined;
  const terminal = apps?.terminal ?? 'Terminal';
  const host = useCardHost({ sessionKey: null, anchor: 'reviews', live, apps, terminal, away: onClose });
  const icons = useEditorIcons(apps);
  const onDesktop = menus.useDesktopPins();
  const { report } = host;

  const { pr } = live;
  if (!pr) return null;

  /** What the notch does for the PR's actions: what takes it off the bar goes on to the next that
   *  waits on you, or back to the session it was shown from. */
  const can: PrWindowCan = { report, open: host.open, reveal: () => host.showPr(pr), left: (done) => onLeft(pr.key, done), failed: (why) => onFailed(pr.key, why) };
  const sessionCan = (l: Lane, s: Session): menus.SessionCan => ({ report, open: host.open, terminal, reveal: () => host.showSession(l, s), editors: apps ? { apps, icons } : undefined, desktop: onDesktop });
  const show = (spec: Parameters<typeof showContextMenu>[0]) => void showContextMenu(spec).catch((e: unknown) => report.error(errorText(e)));

  return (
    <div
      className="ca-notch-card"
      onPointerDown={() => {
        if (!keys) onKeys();
      }}
    >
      <PrCard
        key={pr.key}
        // Where it lives on this Mac, as the PR panel shows it.
        item={prCardItem(pr, live.lanes)}
        now={now}
        zoom={zoom}
        apps={apps}
        lanes={live.lanes}
        usage={live.usage}
        reviews={repoReviews(live.board, pr.repoSlug)}
        teamFiles={live.board?.teamFiles}
        can={can}
        from={from ? { ...from, onBack: (said) => onReturn(pr.key, said) } : undefined}
        outgoing={pr.key !== prKey}
        queue={queue}
        notice={notice}
        hotkeys={{ onBack, onClose }}
        autoFocus
        onShowSession={(_l, s) => onShow(s.key)}
        onOpenSession={(l, s) => host.open(l, s.openTarget)}
        onShowLane={host.showLane}
        onLaneMenu={(l) => show(menus.laneMenu(l, { report, open: host.open, reveal: () => host.showLane(l), editors: apps ? { apps, icons } : undefined, desktop: onDesktop }))}
        onSessionMenu={(l, s) => show(menus.sessionMenu(l, s, sessionCan(l, s)))}
      />
    </div>
  );
}

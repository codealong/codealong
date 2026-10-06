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
// you (`can.left`), and the window goes on to the next. What you wrote stays with the PR until it
// goes, so skipping to the next and coming back loses nothing.
//
// A window says only what differs: how it reports, how it opens things and shows them in
// Codealong, what it does once the PR has left, and what it adds (a queue).

import { useEffect, useRef, useState, type ReactNode } from 'react';
import { openUrl, reviewPr } from '../api';
import { below, showContextMenu, type MenuAction, type MenuSpec } from '../contextMenu';
import { useLatest } from '../latest';
import * as menus from '../menus';
import { errorText, type Report } from '../report';
import { reviewItems } from '../review';
import { reviewSnoozeMenu } from '../snooze';
import { useStartAccounts } from '../startAccounts';
import type { Lane, Session, Usage, Verdict } from '../types';
import { cardZoom } from '../zoom';
import { zoomKey } from './cardHost';
import { PrDetails } from './PrInspector';
import { prIsOpen, type PrRowItem } from './PrsView';
import { BoxText, Button, cx } from './primitives';
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
}

/** What's written for each PR, kept while the card goes from one to the next and back. */
const drafts = new Map<string, string>();

/** How the box says each verdict went. */
const DONE: Record<Verdict, { past: string; left: menus.Left }> = {
  approve: { past: 'Approved', left: 'approved' },
  comment: { past: 'Commented on', left: 'commented on' },
  'request-changes': { past: 'Requested changes on', left: 'changes requested' },
};

export function PrCard({ item, now, lanes, usage, can, outgoing = false, queue, notice, hotkeys, autoFocus, onShowSession, onOpenSession, onShowLane, onLaneMenu, onSessionMenu, zoom }: PrCardProps) {
  const { pr } = item;
  const { report } = can;
  const [text, setText] = useState(() => drafts.get(pr.key) ?? '');
  const [busy, setBusy] = useState<Verdict | null>(null);
  const [note, setNote] = useState<Notice | null>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  const accounts = useStartAccounts();
  const onDesktop = menus.useDesktopPins();
  const requested = pr.reviewRequested && prIsOpen(pr);
  const written = text.trim().length > 0;

  useEffect(() => {
    drafts.set(pr.key, text);
  }, [pr.key, text]);
  useEffect(() => {
    if (autoFocus) box.current?.focus();
  }, [autoFocus, pr.key]);

  const open = (url: string) => void openUrl(url).catch((e: unknown) => report.error(errorText(e)));
  const show = (spec: MenuSpec, at?: { x: number; y: number }) => void showContextMenu(spec, at).catch((e: unknown) => report.error(errorText(e)));

  /** Sends a review with what's in the box. The PR has then left what waits on you. */
  const review = (verdict: Verdict) => {
    if (busy) return;
    const body = text.trim();
    if (!body && verdict !== 'approve') {
      setNote({ text: verdict === 'comment' ? 'Write the comment first.' : 'Say what to change first.', error: true });
      box.current?.focus();
      return;
    }
    setBusy(verdict);
    setNote(null);
    reviewPr(pr.repoSlug, pr.number, verdict, body)
      .then(() => {
        drafts.delete(pr.key);
        setText('');
        report.info(`${DONE[verdict].past} #${pr.number}.`);
        can.left?.(DONE[verdict].left);
      })
      .catch((e: unknown) => setNote({ text: errorText(e), error: true }))
      .finally(() => setBusy(null));
  };

  const reviewWith = reviewItems(pr, lanes, accounts, usage, report, () => can.left?.('review started'), now);
  const actions: menus.PrActions = {
    approve: requested ? { label: `Approve #${pr.number}`, run: () => review('approve') } : undefined,
    comment: requested ? { label: 'Comment', run: () => review('comment') } : undefined,
    changes: requested ? { label: 'Request changes', run: () => review('request-changes') } : undefined,
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

  const sign = (name: keyof typeof menus.PR_CHORDS) => menus.chordSigns(menus.PR_CHORDS[name] ?? '');
  const snooze = actions.snooze;
  const buttons: ReactNode = (
    <>
      <Hotkey label="Approve" keys={sign('approve')} title={actions.approve ? 'Approve it, with what’s in the box as the review’s text, and go on' : 'It doesn’t ask for your review'} run={actions.approve && (() => actions.approve?.run())} />
      <Hotkey label="Comment" keys={sign('comment')} title={actions.comment ? 'Comment on it with what’s in the box, and go on' : 'It doesn’t ask for your review'} run={actions.comment && (() => actions.comment?.run())} />
      <Hotkey label="Snooze" keys={snoozeRange()} title={snooze ? `Snooze the review: it waits apart until its time comes. ${snoozeKeys(snooze)}` : 'The review can’t be snoozed'} run={snooze && ((el) => void showContextMenu(snooze.items, below(el)).catch(() => {}))} />
      <Hotkey label="Review with" keys={sign('review')} title={actions.review ? `Start a review on an account; the key starts it ${actions.review.items[0] && actions.review.items[0] !== 'separator' && !('items' in actions.review.items[0]) ? actions.review.items[0].label.replace(/^Auto \([^)]*\) /, 'on Auto ') : ''}` : 'No account can start a review here'} run={actions.review && ((el) => void showContextMenu(actions.review?.items ?? [], below(el)).catch(() => {}))} />
      <Hotkey label="GitHub" keys={sign('github')} title="Open it on GitHub" run={() => actions.github.run()} />
    </>
  );

  const said = note?.text ?? (busy ? `${DONE[busy].past.replace(/d( on)?$/, 'ing$1')}…` : requested ? `${sign('approve')} approves and goes on · ${sign('comment')} comments` : 'Your review isn’t asked for; a comment still goes to the PR');
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
        <PrDetails item={item} now={now} onShowSession={onShowSession} onOpenSession={onOpenSession} onShowLane={onShowLane} onLaneMenu={onLaneMenu} onSessionMenu={onSessionMenu} />
      </div>
      <div className="ca-composer ca-prreview-box">
        <div
          className="ca-composer-field"
          onMouseDown={(e) => {
            if ((e.target as Element).closest('button, textarea')) return;
            e.preventDefault();
            box.current?.focus();
          }}
        >
          <BoxText
            ref={box}
            value={text}
            rows={Math.min(6, Math.max(2, text.split('\n').length))}
            hint={requested ? 'Your review, if it says anything' : `Comment on #${pr.number}`}
            disabled={busy !== null}
            onChange={(e) => {
              setText(e.target.value);
              if (note?.error) setNote(null);
            }}
            aria-label={`Review of #${pr.number}`}
          />
          <div className="ca-composer-buttons ca-prreview-buttons">
            {actions.changes && (
              <Button size="sm" disabled={!written || busy !== null} title="Ask for changes, with what’s written" onClick={() => actions.changes?.run()}>
                Request changes
              </Button>
            )}
            <Button size="sm" disabled={!written || busy !== null} title={actions.comment ? `Comment with what’s written (${sign('comment')})` : 'Comment on the PR'} onClick={() => review('comment')}>
              Comment
            </Button>
            {actions.approve && (
              <Button variant="primary" size="sm" icon="check" disabled={busy !== null} title={`Approve (${sign('approve')})`} onClick={() => actions.approve?.run()}>
                Approve
              </Button>
            )}
          </div>
        </div>
        <span className={cx('ca-composer-note', note?.error && 'is-error')} role="status">
          {said}
        </span>
      </div>
    </div>
  );
}

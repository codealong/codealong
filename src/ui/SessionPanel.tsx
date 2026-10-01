// A session with its conversation, wherever it's shown: where it stands and how to open it at the
// top (SessionHeader.tsx), its whole conversation in the middle (SessionThread.tsx), and at the
// bottom a box that sends it a message where it runs, or what it waits on you to allow
// (SessionReply.tsx). The main window shows it as a panel, on the canvas and in Sessions; the
// floating bar's peek, a session kept on the desktop and the quick panel's reply view show it as a
// card of their own (SessionCard.tsx, which wires it the same for all three), for the pill under
// the pointer, the session kept and the one picked there, and a right-click anywhere on the card
// but the box gives the session's menu.
//
// Answered in turn, it works through what waits on you: ⌘⏎ in the box sends (or allows the call
// the session waits on) and goes on to the next, the check (⌘E) marks a finished turn seen and
// goes on, ⌘] skips to it, and a strip above the header says how many more there are and how the
// last one went. In Sessions, ⇧⌘F expands it to the whole window, so the list is out of the way
// while you do. Rename in a pill's or a row's menu shows it with its name ready to change.

import { useMemo } from 'react';
import { shortOf } from '../devices';
import type { Sent, Session } from '../types';
import { Button, Rich } from './primitives';
import { runFolder } from './RunButton';
import { SessionHeader, type SessionHeaderProps } from './SessionHeader';
import { Reply } from './SessionReply';
import { SessionThread } from './SessionThread';

/** Working through what waits on you: how many are left after this one, and the next. */
export interface Queue {
  left: number;
  next: Session | null;
  onNext: () => void;
}
/** How the one before went: sent, or why it didn't go. */
export type Notice = { text: string; error: boolean };

export interface SessionPanelProps extends Omit<SessionHeaderProps, 'next' | 'compact'> {
  /** A panel in the main window, or a card of its own (the bar's peek, the quick panel's reply
   *  view), where a right-click anywhere but the box gives the session's menu. */
  variant?: 'panel' | 'card';
  /** Working through what waits on you: what's left after this one, and how the one before went. */
  queue?: Queue;
  notice?: Notice | null;
  /** Puts the cursor in the box. */
  autoFocus?: boolean;
  /** ⌘⏎ allows what it waits on (the quick panel, which is used from the keyboard). */
  keys?: boolean;
  /** As a message goes on its way; `onSent` once it went, `onFailed` if it didn't and the box
   *  isn't showing to say so (SessionReply.tsx). */
  onSending?: () => void;
  onSent?: (sent: Sent) => void;
  onFailed?: (why: string) => void;
  onAnswered?: (said: string) => void;
  /** Sent or allowed with ⌘⏎: on to the next, with what was done. */
  onDone?: (done: string) => void;
  /** Once the conversation's first page is laid out. */
  onLoaded?: () => void;
}

/** Above a session answered in turn: how the one before went, else how many more wait on you, and
 *  the way on to the next (⌘]). */
export function QueueStrip({ queue, notice }: { queue?: Queue; notice?: Notice | null }) {
  const next = queue?.next;
  if (!notice && !next) return null;
  return (
    <div className="ca-reply-queue">
      <span className={`ca-reply-queue-note${notice?.error ? ' is-error' : ''}`} role="status">
        {notice ? notice.text : `${queue?.left} more waiting on you`}
      </span>
      {next && queue && (
        <Button variant="ghost" size="sm" kbd={['⌘', ']']} title={`Next: ${next.title}`} onClick={queue.onNext}>
          {notice ? `${queue.left} more` : 'Next'}
        </Button>
      )}
    </div>
  );
}

export function SessionPanel({ variant = 'panel', queue, notice, autoFocus, keys, onSending, onSent, onFailed, onAnswered, onDone, onLoaded, ...header }: SessionPanelProps) {
  const { lane, session: s, apps, now, onOpen, onMenu, device, renaming } = header;
  const terminal = apps.terminal;
  const openIt = () => onOpen(s.openTarget);
  // Its shell blocks run in a terminal here, so not for a session on another device.
  const run = useMemo(() => (shortOf(s.key) ? undefined : { folder: runFolder(lane, s), terminal }), [s.key, s.cwd, lane.path, terminal]);
  const body = (
    <>
      <QueueStrip queue={queue} notice={notice} />
      <SessionHeader {...header} next={!!queue?.next} compact={variant === 'card'} />
      <SessionThread
        key={s.key}
        session={s}
        now={now}
        onOpen={openIt}
        onLoaded={onLoaded}
        answeredBelow={!device || (device.online && device.remoteControl)}
        run={run}
        fallback={
          s.excerpt ? (
            <p className="ca-sessinsp-now">
              <Rich text={s.excerpt} now={now} />
            </p>
          ) : null
        }
      />
      {device && !device.online ? (
        <p className="ca-sessinsp-remote" role="status">
          {device.name} isn't connected, so nothing can reach this session from here right now.
        </p>
      ) : device && !device.remoteControl ? (
        <p className="ca-sessinsp-remote" role="status">
          {device.name} doesn't take prompts from your other devices. To send from here, turn on <b>Let my linked devices act on sessions here</b> in Settings › Devices on {device.name}.
        </p>
      ) : (
        <Reply session={s} terminal={terminal} onOpen={openIt} autoFocus={autoFocus && !renaming} keys={keys} onSending={onSending} onSent={onSent} onFailed={onFailed} onAnswered={onAnswered} onNext={queue?.next ? onDone : undefined} />
      )}
    </>
  );
  if (variant === 'card') {
    return (
      <div
        className="ca-reply"
        onContextMenu={(e) => {
          // The box keeps its own menu, for copy and paste, as does anything inside with one.
          if (!onMenu || e.defaultPrevented || (e.target instanceof Element && e.target.closest('input, textarea, [contenteditable="true"]'))) return;
          e.preventDefault();
          onMenu();
        }}
      >
        {body}
      </div>
    );
  }
  return (
    <aside className="ca-insp ca-sessinsp" aria-label={`${s.title} details`}>
      {body}
    </aside>
  );
}

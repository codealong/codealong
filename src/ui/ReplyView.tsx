// A session to answer at a glance: who it is and where at the top, its conversation, and the box
// that answers it at the bottom (SessionReply.tsx). The floating bar's peek shows it for the pill
// under the pointer, and the quick panel's reply view for the session picked there. Both give its
// header a way to mark a finished turn seen or archive the session, off the list it came from, and
// a way to snooze it off the bar. Answering from the quick panel's list, a strip above the header
// says how the last one went and what's left: the view goes on to the next as each one's done.
// Rename in a pill's or a row's menu shows it with its name ready to change.

import { useMemo, useRef, useState, type MouseEvent } from 'react';
import { shortOf } from '../devices';
import { agoPhrase, duration } from '../format';
import { sessionName, sessionPlace } from '../harness';
import { seenable } from '../lanes';
import { untilWords } from '../snooze';
import type { Lane, Sent, Session } from '../types';
import { Button, Rich, STATUS_WORD, StatusGlyph } from './primitives';
import { runFolder } from './RunButton';
import { Reply } from './SessionReply';
import { SessionThread } from './SessionThread';

interface ReplyViewProps {
  lane: Lane;
  session: Session;
  terminal: string;
  now: number;
  /** Opens the session where it lives. */
  onOpen: () => void;
  /** Shows the session in Codealong's main window. */
  onReveal: () => void;
  /** Back to where it was picked (the quick panel's list). */
  onBack?: () => void;
  /** Takes it off the list it was picked from: marks a finished turn seen, which the header offers
   *  only for Your turn and a reply that asks you something (seen, it's idle), or archives it. */
  onSeen?: () => void;
  onArchive?: () => void;
  /** Drops the snooze menu from the header's button. */
  onSnooze?: (e: MouseEvent<HTMLButtonElement>) => void;
  /** Its name is a field, ready to change; `onRename` hears the new one, or null when it's left
   *  as it was. */
  renaming?: boolean;
  onRename?: (name: string | null) => void;
  autoFocus?: boolean;
  /** ⌘⏎ allows what it waits on (the quick panel, which is used from the keyboard). */
  keys?: boolean;
  /** As a message goes on its way; `onSent` once it went, `onFailed` if it didn't and the box
   *  isn't showing to say so (SessionReply.tsx). */
  onSending?: () => void;
  onSent?: (sent: Sent) => void;
  onFailed?: (why: string) => void;
  onAnswered?: (said: string) => void;
  /** Once the conversation's first page is laid out. */
  onLoaded?: () => void;
  queue?: Queue;
  notice?: Notice | null;
}

/** Working through what waits on you: how many are left after this one, and the next. */
export interface Queue {
  left: number;
  next: Session | null;
  onNext: () => void;
}
/** How the one before went: sent, or why it didn't go. */
export type Notice = { text: string; error: boolean };

/** Above a session answered in turn: how the one before went, else how many more wait on you, and
 *  the way on to the next (⌘]). The quick panel's reply view and the main window's session panel
 *  show it. */
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

/** The open button's words: the lane's own for the session, else where it opens. */
function openWords(lane: Lane, s: Session, terminal: string): { label: string; detail: string | null; disabled: boolean } {
  const t = lane.targets.find((x) => x.id === s.openTarget);
  return t ? { label: t.label, detail: t.detail, disabled: t.disabled } : { label: `Open in ${sessionPlace(s, terminal)}`, detail: null, disabled: false };
}

/** A session's name as a field: ⏎ keeps what's typed, Escape leaves the name as it was, and so
 *  does clicking away without changing it. Clicking away after changing it keeps the change, as
 *  renaming a file in the Finder does. */
function NameField({ title, onDone }: { title: string; onDone: (name: string | null) => void }) {
  // The title it opened on: one the board changes meanwhile isn't a change of yours.
  const [opened] = useState(title);
  const [name, setName] = useState(title);
  const done = useRef(false);
  const finish = (next: string | null) => {
    if (done.current) return;
    done.current = true;
    onDone(next);
  };
  // Left as it was, a title the board cut short isn't taken for a new name.
  const keep = () => finish(name !== opened ? sessionName(name) || null : null);
  return (
    <input
      className="ca-reply-name"
      aria-label="Name"
      value={name}
      autoFocus
      spellCheck={false}
      onFocus={(e) => e.currentTarget.select()}
      onChange={(e) => setName(e.target.value)}
      onKeyDown={(e) => {
        if (e.nativeEvent.isComposing) return;
        if (e.key === 'Enter') {
          e.preventDefault();
          keep();
        } else if (e.key === 'Escape') {
          e.preventDefault();
          finish(null);
        }
      }}
      onBlur={keep}
    />
  );
}

export function ReplyView({ lane, session: s, terminal, now, onOpen, onReveal, onBack, onSeen, onArchive, onSnooze, renaming, onRename, autoFocus, keys, onSending, onSent, onFailed, onAnswered, onLoaded, queue, notice }: ReplyViewProps) {
  const open = openWords(lane, s, terminal);
  const since = s.statusSince ? (s.status === 'working' ? `for ${duration(s.statusSince, now)}` : agoPhrase(s.statusSince, now)) : agoPhrase(s.lastActivity, now);
  const snoozed = s.snoozedUntil ? ` · snoozed until ${untilWords(s.snoozedUntil, now)}` : '';
  const status = (s.stalledSince ? `Quiet since ${agoPhrase(s.stalledSince, now)}` : `${s.asks ? 'Asks you' : STATUS_WORD[s.status]} ${since}`) + snoozed;
  // Its shell blocks run in a terminal here, so not for a session on another device.
  const run = useMemo(() => (shortOf(s.key) ? undefined : { folder: runFolder(lane, s), terminal }), [s.key, s.cwd, lane.path, terminal]);
  return (
    <div className="ca-reply">
      <QueueStrip queue={queue} notice={notice} />
      <header className="ca-reply-head">
        {onBack && <Button variant="ghost" size="sm" icon="chevron-left" aria-label="Back" title="Back to the list (Esc)" onClick={onBack} />}
        <StatusGlyph status={s.status} size={12} />
        <div className="ca-reply-who">
          {renaming && onRename ? (
            <NameField title={s.title} onDone={onRename} />
          ) : (
            <div className="ca-reply-title" title={s.title}>
              {s.title}
            </div>
          )}
          <div className="ca-reply-meta">{renaming && onRename ? '⏎ renames it · Esc keeps the old name' : `${s.account} · ${lane.callsign} · ${status}`}</div>
        </div>
        {onSeen && seenable(s) && <Button variant="ghost" size="sm" icon="check" aria-label="Mark as seen" title="Mark as seen: it leaves the list" onClick={onSeen} />}
        {onSnooze && (
          <Button
            variant="ghost"
            size="sm"
            icon="zzz"
            aria-label={s.snoozedUntil ? 'Snoozed' : 'Snooze'}
            aria-haspopup="menu"
            title={s.snoozedUntil ? `Snoozed until ${untilWords(s.snoozedUntil, now)}: snooze it longer, or wake it now` : 'Snooze: off the bar until the time you pick, then back'}
            onClick={onSnooze}
          />
        )}
        {onArchive && (
          <Button
            variant="ghost"
            size="sm"
            icon="archive"
            aria-label="Archive"
            title={s.harness === 'codex' ? 'Archive: it leaves the list and the board, and Codex archives the thread' : 'Archive: it leaves the list and the board until you unarchive it'}
            onClick={onArchive}
          />
        )}
        <Button variant="ghost" size="sm" icon="sidebar" aria-label="Show in Codealong" title="Show in Codealong" onClick={onReveal} />
        <Button size="sm" icon="arrow-up-right" title={open.detail ?? undefined} disabled={open.disabled} onClick={onOpen}>
          {open.label}
        </Button>
      </header>
      <SessionThread
        key={s.key}
        session={s}
        now={now}
        onOpen={onOpen}
        onLoaded={onLoaded}
        answeredBelow
        run={run}
        fallback={
          s.excerpt ? (
            <p className="ca-sessinsp-now">
              <Rich text={s.excerpt} now={now} />
            </p>
          ) : null
        }
      />
      <Reply session={s} terminal={terminal} onOpen={onOpen} autoFocus={autoFocus && !renaming} keys={keys} onSending={onSending} onSent={onSent} onFailed={onFailed} onAnswered={onAnswered} />
    </div>
  );
}

// A session with its conversation, wherever it's shown: where it stands and how to open it at the
// top (SessionHeader.tsx), its whole conversation in the middle (SessionThread.tsx), and at the
// bottom a box that sends it a message where it runs, or what it waits on you to allow
// (SessionReply.tsx). The main window shows it as a panel, on the canvas and in Sessions; the
// floating bar's peek, a session kept on the desktop and the waiting window show it as a
// card of their own (SessionCard.tsx, which wires it the same for all three), for the pill under
// the pointer, the session kept and the one picked there, and a right-click anywhere on the card
// but the box gives the session's menu. In a panel the header gives it (SessionHeader.tsx): the
// conversation under it keeps a right-click for what's in it.
//
// Answered in turn, it works through what waits on you: ⌘⏎ in the box sends (or allows the call
// the session waits on) and goes on to the next, the check (⌘E) marks a finished turn seen and
// goes on, ⌘] skips to it, and a strip above the header says how many more there are and how the
// last one went. Where a session is worked from the keyboard (its panel in Sessions, the notch)
// the strip is always there and has what's commonly done to a session as buttons that say their
// keys: mark seen, mark done, snooze, close its terminal, archive, back and next (`Hotkeys`). The keys it
// says are read from where they're kept: a session's own from menus.ts `CHORDS` and snooze.ts
// `SNOOZE_CHORDS`, the ones for getting about from queue.ts `QUEUE_CHORDS`. In Sessions, ⇧⌘F
// expands it to the whole window, so the list is out of the way while you do. Rename in a pill's
// or a row's menu shows it with its name ready to change.

import { useMemo, type ReactNode } from 'react';
import { sendAhead } from '../ahead';
import { below, showContextMenu, type MenuGroup } from '../contextMenu';
import { shortOf } from '../devices';
import { seenable } from '../lanes';
import { useLatest } from '../latest';
import { CHORDS, chordSigns, doneTitle, type SessionActions } from '../menus';
import { AWAY_CHORD, QUEUE_CHORDS, type QueueAction } from '../queue';
import { SNOOZE_CHORDS } from '../snooze';
import type { Sent, Session } from '../types';
import { cardZoom } from '../zoom';
import { Button, cx, Icon, Rich } from './primitives';
import { runFolder, runsInSession } from './RunButton';
import { SessionHeader, type SessionHeaderProps } from './SessionHeader';
import { Reply, useWay } from './SessionReply';
import { SessionThread } from './SessionThread';

/** Working through what waits on you: how many are left after this one, and the next. */
export interface Queue {
  left: number;
  /** The next one, by its title: a row's session, or a pill on the bar. */
  next: Pick<Session, 'title'> | null;
  onNext: () => void;
  /** False when the session shown isn't itself one of those waiting (it runs, say): the strip
   *  still says how many wait and goes to them, but sending doesn't go on. */
  waiting?: boolean;
}

/** The strip with what's commonly done to a session, each with its key, where the window is
 *  worked from the keyboard. What it adds to the session's own actions is the window's. */
export interface Hotkeys {
  /** Back to the session looked at before (⌃-), where the strip is the way there: null with
   *  nowhere to go back to. */
  onBack?: (() => void) | null;
  /** Puts the session away (esc), where it can be. */
  onClose?: () => void;
  /** The strip is its window's title bar (the waiting window has no other): it drags the window,
   *  and leaves room at its start for the window's buttons. */
  titleBar?: boolean;
}
/** How the one before went: sent, or why it didn't go. */
export type Notice = { text: string; error: boolean };

export interface SessionPanelProps extends Omit<SessionHeaderProps, 'next' | 'compact'> {
  /** A panel in the main window, or a card of its own (the bar's peek, the waiting
   *  window), where a right-click anywhere but the box gives the session's menu. */
  variant?: 'panel' | 'card';
  /** A card draws at the zoom, in percent, in the windows that float (zoom.ts `cardZoom`); the
   *  main window zooms its whole page instead. */
  zoom?: number;
  /** Working through what waits on you: what's left after this one, and how the one before went. */
  queue?: Queue;
  notice?: Notice | null;
  /** The strip is always there, with what's commonly done to the session and the keys for it. */
  hotkeys?: Hotkeys;
  /** Puts the cursor in the box. */
  autoFocus?: boolean;
  /** ⌘⏎ allows what it waits on (the waiting window and the notch, which are used from the keyboard). */
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

/** What the strip says: how the one before went, else how many wait on you. */
function queueWords(queue: Queue | undefined, notice: Notice | null | undefined, always: boolean): string {
  if (notice) return notice.text;
  if (!queue) return '';
  if (queue.left > 0) return `${queue.left} ${queue.waiting === false ? '' : 'more '}waiting on you`;
  return always && queue.waiting !== false ? 'The last one waiting on you' : '';
}

/** A Snooze menu's times with the chord for each, as a tooltip says them: `⌃⌘1 for 15 minutes`. */
export function snoozeKeys(snooze: MenuGroup): string {
  return snooze.items
    .flatMap((it) => (it !== 'separator' && !('items' in it) && it.accelerator ? [`${chordSigns(it.accelerator)} ${it.label[0].toLowerCase()}${it.label.slice(1)}`] : []))
    .join(', ');
}

/** The snooze chords as one range, for the button that opens their menu: ⌃⌘1–4. */
export function snoozeRange(): string {
  const last = SNOOZE_CHORDS[SNOOZE_CHORDS.length - 1] ?? '';
  return `${chordSigns(SNOOZE_CHORDS[0] ?? '')}–${last.slice(-1)}`;
}

/** A queue action's keys as signs: the one its button says, and any others, for its tooltip. */
export function queueSigns(action: QueueAction): string[] {
  return QUEUE_CHORDS[action].map(chordSigns);
}

/** A button on the strip that says its key. One that doesn't apply to the session shown stays in
 *  its place, dimmed, so the strip holds still from one session to the next. */
export function Hotkey({ label, icon, keys, title, run }: { label?: string; icon?: string; keys?: string; title: string; run: ((el: HTMLElement) => void) | undefined }) {
  return (
    <button type="button" className="ca-hotkey" title={title} aria-label={label ? undefined : title} disabled={!run} onClick={(e) => run?.(e.currentTarget)}>
      {icon && <Icon name={icon} size={12} />}
      {label && <span>{label}</span>}
      {keys && <kbd>{keys}</kbd>}
    </button>
  );
}

interface QueueStripProps {
  queue?: Queue;
  notice?: Notice | null;
  /** With these, the strip is always there and has the session's common actions and their keys. */
  hotkeys?: Hotkeys;
  session?: Session;
  actions?: SessionActions;
  onSeen?: () => void;
  /** What's commonly done to the thing shown, as `Hotkey` buttons, where it isn't a session (a
   *  PR waiting on your review, PrCard.tsx): in place of the session's five. */
  buttons?: ReactNode;
}

/** Above a session answered in turn: how the one before went, else how many more wait on you, and
 *  the way on to the next (⌘]). With `hotkeys` it's always there, and has what's commonly done to
 *  the session as buttons that say their keys, the ones the session's menu runs (menus.ts); the
 *  keys for back, next and putting it away are the queue's (queue.ts). */
export function QueueStrip({ queue, notice, hotkeys, session, actions, onSeen, buttons: own }: QueueStripProps) {
  const next = queue?.next;
  const [nextKey, nextToo] = queueSigns('next');
  if (!hotkeys) {
    if (!notice && !next) return null;
    return (
      <div className="ca-reply-queue">
        <span className={`ca-reply-queue-note${notice?.error ? ' is-error' : ''}`} role="status">
          {queueWords(queue, notice, false)}
        </span>
        {next && queue && (
          <Button variant="ghost" size="sm" kbd={[...nextKey]} title={`Next: ${next.title}`} onClick={queue.onNext}>
            {notice ? `${queue.left} more` : 'Next'}
          </Button>
        )}
      </div>
    );
  }
  const seen = session && seenable(session) ? (onSeen ?? actions?.seen?.run) : undefined;
  const snooze = actions?.snooze;
  const close = actions?.close;
  const done = actions?.done;
  const archive = actions?.archive;
  const sign = (name: keyof typeof CHORDS) => chordSigns(CHORDS[name] ?? '');
  const gap = <span className="ca-hotkeys-gap" aria-hidden="true" />;
  const buttons: ReactNode = (
    <div className="ca-hotkeys" role="toolbar" aria-label={own ? 'What’s commonly done to this PR' : 'What’s commonly done to this session'}>
      {own ?? (
        <>
          <Hotkey
            label="Seen"
            keys={sign('seen')}
            title={!seen ? (next && queue?.waiting === false ? `Nothing to mark as seen here: ${sign('seen')} goes on to the next` : 'Nothing to mark as seen here') : next && queue?.waiting !== false ? 'Mark as seen and go on to the next' : 'Mark as seen: it goes idle and leaves what waits on you'}
            run={seen && (() => seen())}
          />
          <Hotkey
            label="Done"
            keys={sign('done')}
            title={done && session ? doneTitle(session) : session && shortOf(session.key) ? 'Mark it done on the Mac it runs on' : 'It’s archived already'}
            run={done && (() => done.run())}
          />
          {/* A menu that didn't pop up leaves nothing half done, and the snooze keys beside it still work. */}
          <Hotkey label="Snooze" keys={snoozeRange()} title={snooze ? `Snooze: it waits apart until its time comes. ${snoozeKeys(snooze)}` : 'It can’t be snoozed'} run={snooze && ((el) => void showContextMenu(snooze.items, below(el)).catch(() => {}))} />
          {/* Short, as Done beside it is: the notch's strip keeps to one line at its narrowest. */}
          <Hotkey label={close?.label === 'End background job' ? 'End job' : 'Close'} keys={sign('close')} title={close ? `${close.label}: the conversation stays, to go on with later` : 'It has no terminal of its own to close'} run={close && (() => close.run())} />
          <Hotkey label={archive?.label === 'Unarchive' ? 'Unarchive' : 'Archive'} keys={sign('archive')} title={archive?.label ?? 'Archive'} run={archive && (() => archive.run())} />
        </>
      )}
      {gap}
      {hotkeys.onBack !== undefined && <Hotkey label="Back" keys={queueSigns('historyBack')[0]} title={`Back to the session you looked at before; ${queueSigns('historyForward')[0]} goes forward again`} run={hotkeys.onBack ?? undefined} />}
      <Hotkey label="Next" keys={nextKey} title={next ? `Next that waits on you: ${next.title}. ${nextToo} goes on too, and ${queueSigns('back')[0]} back` : 'Nothing else waits on you'} run={next && queue ? () => queue.onNext() : undefined} />
      {hotkeys.onClose && (
        <>
          {gap}
          <Hotkey icon="x" keys={chordSigns(AWAY_CHORD)} title="Put it away" run={hotkeys.onClose} />
        </>
      )}
    </div>
  );
  return (
    <div className={cx('ca-reply-queue', 'has-keys', hotkeys.titleBar && 'is-titlebar')} data-tauri-drag-region={hotkeys.titleBar ? 'deep' : undefined}>
      <span className={`ca-reply-queue-note${notice?.error ? ' is-error' : ''}`} role="status">
        {queueWords(queue, notice, true)}
      </span>
      {buttons}
    </div>
  );
}

export function SessionPanel({ variant = 'panel', zoom, queue, notice, hotkeys, autoFocus, keys, onSending, onSent, onFailed, onAnswered, onDone, onLoaded, ...header }: SessionPanelProps) {
  const { lane, session: s, apps, now, onOpen, onMenu, device, renaming } = header;
  const terminal = apps.terminal;
  const openIt = () => onOpen(s.openTarget);
  // Sending and marking seen go on to the next only from a session that waits on you itself.
  const goesOn = queue?.waiting !== false && queue?.next;
  // Its shell blocks run in a terminal here, so not for a session on another device; a block led
  // by `!` goes into the session itself, as a reply does, where it can.
  const folder = shortOf(s.key) ? null : runFolder(lane, s);
  const inSession = runsInSession(s, useWay(s));
  const sessionNow = useLatest(s);
  const run = useMemo(
    () => (folder === null ? undefined : { folder, terminal, session: inSession ? (command: string) => sendAhead(sessionNow.current, command, { toDraft: false }) : undefined }),
    [folder, terminal, inSession, sessionNow],
  );
  const body = (
    <>
          <QueueStrip queue={queue} notice={notice} hotkeys={hotkeys} session={s} actions={header.actions} onSeen={header.onSeen} />
          <SessionHeader {...header} next={!!goesOn} compact={variant === 'card'} />
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
            <Reply session={s} terminal={terminal} onOpen={openIt} autoFocus={autoFocus && !renaming} keys={keys} onSending={onSending} onSent={onSent} onFailed={onFailed} onAnswered={onAnswered} onNext={goesOn ? onDone : undefined} />
          )}
    </>
  );
  if (variant === 'card') {
    return (
      <div
        className="ca-reply"
        style={cardZoom(zoom)}
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

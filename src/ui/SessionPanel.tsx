// A session with its conversation, wherever it's shown: where it stands and how to open it at the
// top (SessionHeader.tsx), its whole conversation in the middle (SessionThread.tsx), and at the
// bottom a box that sends it a message where it runs, or what it waits on you to allow
// (SessionReply.tsx). The main window shows it as a panel, on the canvas and in Sessions; the
// floating bar's peek, a session kept on the desktop and the waiting window show it as a
// card of their own (SessionCard.tsx, which wires it the same for all three), for the pill under
// the pointer, the session kept and the one picked there, and a right-click anywhere on the card
// but the box gives the session's menu, under a message's Copy message where it's on one. In a
// panel the header gives it (SessionHeader.tsx): the conversation under it keeps a right-click for
// what's in it.
//
// Answered in turn, it works through what waits on you: ⌘⏎ in the box sends (or allows the call
// the session waits on) and goes on to the next, the check (⌘E) marks a finished turn seen and
// goes on, ⌘] skips to it, and a strip above the header says how many more there are and how the
// last one went; pointing at its chip of counts lists the ones it counts, and a click on one shows
// it. Where a session is worked from the keyboard (its panel in Sessions, the notch)
// the strip is always there and has what's commonly done to a session as buttons that say their
// keys: mark seen, mark done, snooze, close its terminal, archive, back and next (`Hotkeys`). The keys it
// says are read from where they're kept: a session's own from menus.ts `CHORDS` and snooze.ts
// `SNOOZE_CHORDS`, the ones for getting about from queue.ts `QUEUE_CHORDS`. In Sessions, ⇧⌘F
// expands it to the whole window, so the list is out of the way while you do. Rename in a pill's
// or a row's menu shows it with its name ready to change.

import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from 'react';
import { sendAhead } from '../ahead';
import { below, showContextMenu, type MenuGroup, type MenuSpec } from '../contextMenu';
import { shortOf } from '../devices';
import { seenable } from '../lanes';
import { useLatest } from '../latest';
import { CHORDS, chordSigns, doneTitle, type SessionActions } from '../menus';
import { AWAY_CHORD, QUEUE_CHORDS, WAITS, waitCountWords, type Listed, type QueueAction, type WaitCounts } from '../queue';
import { SNOOZE_CHORDS } from '../snooze';
import type { Sent, Session } from '../types';
import { cardZoom, scaleOf } from '../zoom';
import { Button, Chip, cx, Icon, Rich, StatusGlyph, WaitsGlyph } from './primitives';
import { runFolder, runsInSession } from './RunButton';
import { SessionHeader, type SessionHeaderProps } from './SessionHeader';
import { Reply, useWay } from './SessionReply';
import { SessionThread } from './SessionThread';
import { useDismiss } from './useDismiss';

/** Working through what waits on you: what's left after this one, and the next. */
export interface Queue {
  /** How many of each kind wait on you besides the one shown, which the strip counts. */
  counts: WaitCounts;
  /** Those it counts, the next first, as pointing at its chip lists them (queue.ts `Listed`). */
  left: Listed[];
  /** The next one, by its title: a row's session, or a pill on the bar. */
  next: Pick<Session, 'title'> | null;
  onNext: () => void;
  /** Shows one of `left` in place of the one shown: a step by hand, as ⌘] is. */
  onPick: (key: string) => void;
  /** False when the session shown isn't itself one of those waiting (it runs, say): the strip
   *  still counts them and goes to them, but sending doesn't go on. */
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

export interface SessionPanelProps extends Omit<SessionHeaderProps, 'seenAbove' | 'compact'> {
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
  /** Shows the menu a right-click on a message in the conversation asks for (Copy message), the
   *  window's way: a card puts the session's own under it, as a right-click anywhere else on it
   *  gives the session's menu. */
  onThreadMenu?: (items: MenuSpec) => void;
}

/** What else waits on you in words, as the chip says it to a screen reader and the list it shows
 *  is named: `Waiting on you after this one: 2 need you, 1 your turn`. */
function queueWords(queue: Queue): string {
  const words = WAITS.filter((w) => queue.counts[w] > 0)
    .map((w) => waitCountWords(w, queue.counts[w]))
    .join(', ');
  return `${queue.waiting === false ? 'Waiting on you' : 'Waiting on you after this one'}: ${words}`;
}

/** How long the pointer rests on the chip before its list shows, and is off both before it goes. */
const LIST_AFTER_MS = 250;
const LIST_GOES_MS = 200;
/** Room kept between the chip and its list, and between the list and the session's edges. */
const LIST_GAP = 4;
const LIST_EDGE = 8;

/** Whether the chip's list is up, and whether a click put it there (it stays until a press
 *  elsewhere or Escape) or the pointer did (it goes when the pointer leaves, or on any key). */
interface CountList {
  up: { clicked: boolean } | null;
  chip: RefObject<HTMLSpanElement>;
  point: () => void;
  leave: () => void;
  keep: () => void;
  click: () => void;
  close: () => void;
}

function useCountList(): CountList {
  const [up, setUp] = useState<CountList['up']>(null);
  const upNow = useLatest(up);
  const chip = useRef<HTMLSpanElement>(null);
  const timer = useRef(0);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const keep = () => window.clearTimeout(timer.current);
  return {
    up,
    chip,
    keep,
    point: () => {
      keep();
      if (!upNow.current) timer.current = window.setTimeout(() => setUp({ clicked: false }), LIST_AFTER_MS);
    },
    leave: () => {
      keep();
      if (!upNow.current?.clicked) timer.current = window.setTimeout(() => setUp(null), LIST_GOES_MS);
    },
    click: () => {
      keep();
      setUp((u) => (u?.clicked ? null : { clicked: true }));
    },
    close: () => {
      keep();
      setUp(null);
    },
  };
}

/** What else waits on you, as the notch counts it, in one chip: each kind's glyph and how many.
 *  Pointing at it, or clicking it, lists what it counts (`QueueList`). */
function QueueCounts({ queue, list }: { queue: Queue; list: CountList }) {
  const kinds = WAITS.filter((w) => queue.counts[w] > 0);
  if (kinds.length === 0) return null;
  return (
    <span ref={list.chip} className="ca-reply-queue-chip" onPointerEnter={list.point} onPointerLeave={list.leave}>
      <Chip className={cx('ca-reply-queue-counts', list.up && 'is-open')} label={queueWords(queue)} expanded={!!list.up} act={list.click}>
        {kinds.map((w) => (
          <span key={w} className="ca-reply-queue-count" data-waits={w}>
            <WaitsGlyph waits={w} />
            {queue.counts[w]}
          </span>
        ))}
      </Chip>
    </span>
  );
}

/** What the strip says: how the one before went, else what else waits on you, counted. */
function QueueNote({ queue, notice, list }: { queue: Queue | undefined; notice: Notice | null | undefined; list: CountList }) {
  return (
    <span className={cx('ca-reply-queue-note', notice && 'is-notice', notice?.error && 'is-error')} role="status" title={notice?.text}>
      {notice ? notice.text : queue && <QueueCounts queue={queue} list={list} />}
    </span>
  );
}

/** What the chip counts, under it: a part for each kind, in the order the chip counts them, each
 *  in the order ⌘] goes through them, as a folded pill on the bar lists its sessions: each one's
 *  glyph, its title and where it is. A click on one shows it here. It's drawn in the strip, so a
 *  card's zoom draws it too, and kept within the session. */
function QueueList({ queue, list, strip }: { queue: Queue; list: CountList; strip: RefObject<HTMLDivElement> }) {
  const pop = useRef<HTMLDivElement>(null);
  const up = list.up;
  const parts = up ? WAITS.map((w) => ({ waits: w, rows: queue.left.filter((l) => l.waits === w) })).filter((p) => p.rows.length > 0) : [];
  useDismiss(pop, list.close, { when: parts.length > 0, inside: (t) => !!list.chip.current?.contains(t), keys: up?.clicked ? 'escape' : 'any', blur: true });
  // Under the chip, the rows' glyphs under its first glyph, as far down as the session goes;
  // scrolling within itself past that.
  useLayoutEffect(() => {
    const el = pop.current;
    const s = strip.current;
    const chip = list.chip.current;
    const first = chip?.querySelector('.ca-reply-queue-count > :first-child');
    if (!el || !s || !chip || !first) return;
    const scale = scaleOf(s);
    const sr = s.getBoundingClientRect();
    const cr = chip.getBoundingClientRect();
    const glyph = first.getBoundingClientRect().left;
    const rowGlyph = el.querySelector('.ca-queue-list-row > :first-child');
    const inset = rowGlyph ? rowGlyph.getBoundingClientRect().left - el.getBoundingClientRect().left : 0;
    const room = (s.parentElement ?? s).getBoundingClientRect();
    el.style.maxHeight = `${Math.max(0, (room.bottom - cr.bottom) / scale - LIST_GAP - LIST_EDGE)}px`;
    el.style.top = `${(cr.bottom - sr.top) / scale + LIST_GAP}px`;
    el.style.left = `${Math.max(LIST_EDGE, Math.min((glyph - inset - sr.left) / scale, s.offsetWidth - el.offsetWidth - LIST_EDGE))}px`;
  });
  if (parts.length === 0) return null;
  return (
    <div
      ref={pop}
      className="ca-queue-list"
      role="menu"
      aria-label={queueWords(queue)}
      data-tauri-drag-region="false"
      onPointerEnter={list.keep}
      onPointerLeave={list.leave}
      onContextMenu={(e) => {
        // A row is another session than the one shown, whose menu the card has.
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      {parts.map((p) => (
        <div key={p.waits} className="ca-queue-list-part" role="group" aria-label={waitCountWords(p.waits, p.rows.length)}>
          <p className="ca-queue-list-head">{waitCountWords(p.waits, p.rows.length)}</p>
          {p.rows.map((r) => (
            <button
              key={r.key}
              type="button"
              role="menuitem"
              className="ca-queue-list-row"
              title={r.where ? `${r.title} · ${r.where}` : r.title}
              onClick={() => {
                list.close();
                queue.onPick(r.key);
              }}
            >
              {r.status ? <StatusGlyph status={r.status} title="" /> : <WaitsGlyph waits={r.waits} />}
              <span className="ca-queue-list-title">{r.title}</span>
              {r.where && <span className="ca-queue-list-where">{r.where}</span>}
            </button>
          ))}
        </div>
      ))}
    </div>
  );
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

/** Above a session answered in turn: how the one before went, else what else waits on you, and
 *  the way on to the next (⌘]). With `hotkeys` it's always there, and has what's commonly done to
 *  the session as buttons that say their keys, the ones the session's menu runs (menus.ts); the
 *  keys for back, next and putting it away are the queue's (queue.ts). */
export function QueueStrip({ queue, notice, hotkeys, session, actions, onSeen, buttons: own }: QueueStripProps) {
  const next = queue?.next;
  const [nextKey, nextToo] = queueSigns('next');
  const strip = useRef<HTMLDivElement>(null);
  const list = useCountList();
  const listed = queue && !notice && <QueueList queue={queue} list={list} strip={strip} />;
  if (!hotkeys) {
    if (!notice && !next) return null;
    return (
      <div ref={strip} className="ca-reply-queue">
        <QueueNote queue={queue} notice={notice} list={list} />
        {listed}
        {next && queue && (
          <Button variant="ghost" size="sm" kbd={[...nextKey]} title={`Next: ${next.title}`} onClick={queue.onNext}>
            Next
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
            title={done && session ? doneTitle(session) : 'It’s archived already'}
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
    <div ref={strip} className={cx('ca-reply-queue', 'has-keys', hotkeys.titleBar && 'is-titlebar')} data-tauri-drag-region={hotkeys.titleBar ? 'deep' : undefined}>
      <QueueNote queue={queue} notice={notice} list={list} />
      {listed}
      {buttons}
    </div>
  );
}

export function SessionPanel({ variant = 'panel', zoom, queue, notice, hotkeys, autoFocus, keys, onSending, onSent, onFailed, onAnswered, onDone, onLoaded, onThreadMenu, ...header }: SessionPanelProps) {
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
          <SessionHeader {...header} seenAbove={!!hotkeys} compact={variant === 'card'} />
          <SessionThread
            key={s.key}
            session={s}
            now={now}
            onOpen={openIt}
            onLoaded={onLoaded}
            answeredBelow={!device || (device.online && device.remoteControl)}
            run={run}
            onMenu={onThreadMenu}
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
        data-find="conversation"
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
    <aside className="ca-insp ca-sessinsp" aria-label={`${s.title} details`} data-find="conversation">
      {body}
    </aside>
  );
}

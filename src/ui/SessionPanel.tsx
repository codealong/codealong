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
// goes on, and ⌘] skips to it. Where a session is worked from the keyboard (its panel in
// Sessions, the notch, the waiting window) a strip above the header has what's commonly done to
// a session as buttons that say their keys: mark seen, mark done, snooze, close its terminal,
// archive, back and next (`Hotkeys`), with Merge and done… ahead of them for a PR's primary
// session once the PR is ready to merge. They keep to one line: the ones it hasn't room for, from
// the last, are in a ⋯ menu at their end instead (`fitFor`), with their keys, which work as ever.
// At its left it says how the last one went, on one line; what
// the box has to say is said in the box's own foot, beside where you write (SessionReply.tsx
// `BoxSays`). The keys it says are read from where they're kept: a session's own from
// menus.ts `CHORDS` and snooze.ts `SNOOZE_CHORDS`, the ones for getting about from queue.ts
// `QUEUE_CHORDS`. In Sessions, ⇧⌘F
// expands it to the whole window, so the list is out of the way while you do. Rename in a pill's
// or a row's menu shows it with its name ready to change.

import { useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from 'react';
import { runCommand } from '../api';
import { below, showContextMenu, type MenuAction, type MenuGroup, type MenuSpec } from '../contextMenu';
import { shortOf } from '../devices';
import { seenable } from '../lanes';
import { CHORDS, chordSigns, doneTitle, type SessionActions } from '../menus';
import { AWAY_CHORD, QUEUE_CHORDS, type QueueAction } from '../queue';
import { SNOOZE_CHORDS } from '../snooze';
import type { Sent, Session, TranscriptItem } from '../types';
import { cardZoom, scaleOf } from '../zoom';
import { EditDialog } from './EditFrom';
import { MoveDialog } from './MoveAccount';
import { Button, cx, Icon, OneLine, Rich } from './primitives';
import { runFolder } from './RunButton';
import { SessionHeader, type SessionHeaderProps } from './SessionHeader';
import { Reply } from './SessionReply';
import { SessionThread } from './SessionThread';
import type { FindAsk } from './ThreadFind';
import { keysSaid } from '../platform';

/** Working through what waits on you: the next after this one. */
export interface Queue {
  /** The next one, by its title: a row's session, or a pill on the bar. */
  next: Pick<Session, 'title'> | null;
  onNext: () => void;
  /** False when the session shown isn't itself one of those waiting (it runs, say): the strip
   *  still goes on to them, but sending doesn't go on. */
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

export interface SessionPanelProps extends Omit<SessionHeaderProps, 'seenAbove' | 'compact' | 'accountBelow' | 'inMain' | 'onMove'> {
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
  /** What the conversation's find bar opens on: the words a search found it by in what was said. */
  find?: FindAsk | null;
  /** A dialog the window holds for the session (Merge and done…, SessionCard.tsx), drawn with
   *  the panel, at a card's zoom, as Move's is. */
  dialog?: ReactNode;
}

/** What the strip says at its left: what the window said (how the one before went, or how
 *  something done to this one went). It keeps to one line beside the buttons, which keep their
 *  place, cut short where they leave it too little room and all of it under the pointer, over the
 *  strip and down from it (primitives.tsx `OneLine`). */
function StripNote({ notice }: { notice: Notice | null | undefined }) {
  return <OneLine className={cx('ca-reply-queue-note', notice?.error && 'is-error')} role="status" down text={notice?.text ?? ''} />;
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
 *  its place, dimmed, so the strip holds still from one session to the next. One there only at
 *  times says as much of itself as the strip has `room` for (`fitFor`), its icon always, so its
 *  coming never moves a strip that fits without it. */
export function Hotkey({ label, icon, keys, title, run, room, className }: { label?: string; icon?: string; keys?: string; title: string; run: ((el: HTMLElement) => void) | undefined; room?: Room; className?: string }) {
  return (
    <button type="button" className={cx('ca-hotkey', room && 'is-roomy', room && room !== 'all' && 'is-wordless', room === 'icon' && 'is-keyless', className)} title={title} aria-label={label && !room ? undefined : title} disabled={!run} onClick={(e) => run?.(e.currentTarget)}>
      {icon && <Icon name={icon} size={12} />}
      {label && <span>{label}</span>}
      {keys && <kbd>{keysSaid(keys)}</kbd>}
    </button>
  );
}

/** One of what's commonly done on the strip: a `Hotkey` where the strip has room for it, and an
 *  item of its ⋯ menu where it hasn't (`fitFor`), which shows the same key beside it. */
export interface StripAction {
  label: string;
  /** Its chord, as menus.ts keeps it: the button says it, and the ⋯ menu shows it. */
  chord?: string;
  /** What the button says in its place, for several chords: Snooze's ⌃⌘1–4. */
  keys?: string;
  icon?: string;
  title: string;
  /** None where it doesn't apply to what's shown. */
  run?: () => void;
  /** The menu it drops instead (Snooze's times), which is a submenu of ⋯. */
  menu?: MenuSpec;
  /** It's there only at times (Merge and done…), and gives up its words and key before any of the
   *  rest go into ⋯, so its coming moves nothing else (`Room`). */
  roomy?: boolean;
}

/** An action that's in the strip's ⋯ menu, as it is there. */
function itemOf(action: StripAction): MenuAction | MenuGroup {
  const chord = action.chord;
  if (action.menu) {
    // A menu's chord runs its first choice (menus.ts `prChorded`: Review with's ⌃⌘R), so that's
    // where the submenu shows it.
    const first = chord ? action.menu.find((it): it is MenuAction => it !== 'separator' && !('items' in it)) : undefined;
    return { label: action.label, items: first && !first.accelerator ? action.menu.map((it) => (it === first ? { ...first, accelerator: chord } : it)) : action.menu };
  }
  const run = action.run;
  return { label: action.label, run: () => run?.(), enabled: !!run, ...(chord ? { accelerator: chord } : {}) };
}

/** Drops a menu from the button clicked. One that didn't pop up leaves nothing half done, and the
 *  keys for what's in it still work. */
function drop(menu: MenuSpec): (el: HTMLElement) => void {
  return (el) => void showContextMenu(menu, below(el)).catch(() => {});
}

/** How much of a button that's there only at times the strip has room for: its words and key, its
 *  key alone, or its icon alone. */
export type Room = 'all' | 'key' | 'icon';

/** How the strip's actions fit on its one line beside what it says: how much the one there only at
 *  times shows, and how many of them, from the last, are in its ⋯ menu instead. */
interface Fit {
  room: Room;
  over: number;
}
const FITS: Fit = { room: 'all', over: 0 };

/** A computed length in pixels, or none. */
const px = (value: string) => parseFloat(value) || 0;

/** As much of the strip's actions as fits on one line beside the rest (back, next, ×), with the room
 *  kept at the strip's start for what it says: the one there only at times gives up its words, then
 *  its key, and then the actions go into ⋯ from the last, as few as will do, it saying as much
 *  again as the room they leave has for. Measured, not read off the strip's width:
 *  what's beside them differs from window to window (the notch's and the waiting window's Back and
 *  ×, a panel's Next alone) and Linux says its keys in words. Whatever doesn't show is laid out all
 *  the same, out of the line and unseen (SessionPanel.css), ⋯ too, so what it finds doesn't depend
 *  on what it found before. */
function fitFor(strip: HTMLElement): Fit {
  const keys = strip.querySelector<HTMLElement>('.ca-hotkeys');
  if (!keys) return FITS;
  // Rects are drawn at a card's zoom, lengths in the element's own CSS pixels (zoom.ts).
  const scale = scaleOf(strip);
  const wide = (el: Element) => el.getBoundingClientRect().width / scale;
  const outer = (el: Element) => {
    const style = getComputedStyle(el);
    return wide(el) + px(style.marginLeft) + px(style.marginRight);
  };
  // From where what it says starts (after the title bar's room for the window's buttons, say),
  // less the least kept for that, to the end of the strip's own box.
  const own = getComputedStyle(strip);
  const note = strip.querySelector('.ca-reply-queue-note');
  const end = strip.getBoundingClientRect().right - (px(own.paddingRight) + px(own.borderRightWidth)) * scale;
  const start = note ? note.getBoundingClientRect().left : strip.getBoundingClientRect().left + (px(own.paddingLeft) + px(own.borderLeftWidth)) * scale;
  const room = (end - start) / scale - (note ? px(getComputedStyle(note).minWidth) + px(own.columnGap) : 0);
  const items = [...keys.children];
  const more = keys.querySelector(':scope > .is-more');
  const folding = items.filter((el) => el.classList.contains('is-foldable'));
  const staying = items.filter((el) => el !== more && !folding.includes(el));
  // The one there only at times, by how much of it shows: its words and key measured whether
  // they show or not.
  const roomy = keys.querySelector(':scope > .is-roomy');
  const sizes = roomy && sizesOf(roomy, outer(roomy), wide);
  const width = (el: Element, at: Room) => (el === roomy && sizes ? sizes[at] : outer(el));
  const line = (at: Room, over: number) => {
    const shown = [...staying, ...folding.slice(0, folding.length - over), ...(over && more ? [more] : [])];
    return shown.reduce((sum, el) => sum + width(el, at), px(getComputedStyle(keys).columnGap) * (shown.length - 1));
  };
  for (let over = 0; over <= folding.length; over += 1) {
    const at = (['all', 'key', 'icon'] as const).find((r) => line(r, over) <= room);
    if (at) return { room: at, over };
  }
  return { room: 'icon', over: folding.length };
}

/** A button there only at times, as wide as each `Room` has it. */
function sizesOf(button: Element, whole: number, wide: (el: Element) => number): Record<Room, number> {
  const gap = px(getComputedStyle(button).columnGap);
  const part = (el: Element | null) => (el ? { width: wide(el) + gap, shown: getComputedStyle(el).position !== 'absolute' } : { width: 0, shown: false });
  const words = part(button.querySelector(':scope > span'));
  const key = part(button.querySelector(':scope > kbd'));
  const bare = whole - (words.shown ? words.width : 0) - (key.shown ? key.width : 0);
  return { all: bare + words.width + key.width, key: bare + key.width, icon: bare };
}

/** `fitFor` the strip, again as it or any of its buttons change size: one in ⋯ too, whose words
 *  change from one session to the next (Close, End job), and again when `shape` (the actions it
 *  has) does. */
function useFit(strip: RefObject<HTMLDivElement | null>, on: boolean, shape: string): Fit {
  const [fit, setFit] = useState<Fit>(FITS);
  useLayoutEffect(() => {
    const el = strip.current;
    if (!on || !el) return;
    const measure = () => {
      const next = fitFor(el);
      setFit((was) => (was.room === next.room && was.over === next.over ? was : next));
    };
    // Once now, before the strip is drawn, then as the window or the buttons change.
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    el.querySelectorAll('.ca-hotkeys, .ca-hotkey').forEach((button) => observer.observe(button));
    return () => observer.disconnect();
  }, [strip, on, shape]);
  return fit;
}

interface QueueStripProps {
  queue?: Queue;
  notice?: Notice | null;
  /** With these, the strip is always there and has the session's common actions and their keys. */
  hotkeys?: Hotkeys;
  session?: Session;
  actions?: SessionActions;
  onSeen?: () => void;
  /** What's commonly done to the thing shown, where it isn't a session (a PR waiting on your
   *  review, PrCard.tsx): in place of the session's five. */
  buttons?: StripAction[];
}

/** Above a session answered in turn: how the one before went, and the way on to the next (⌘]).
 *  With `hotkeys` it's always there, and has what's commonly done to the session as buttons that
 *  say their keys, the ones the session's menu runs (menus.ts); the keys for back, next and
 *  putting it away are the queue's (queue.ts). */
export function QueueStrip({ queue, notice, hotkeys, session, actions, onSeen, buttons: own }: QueueStripProps) {
  const next = queue?.next;
  const [nextKey, nextToo] = queueSigns('next');
  const stripRef = useRef<HTMLDivElement>(null);
  const seen = session && seenable(session) ? (onSeen ?? actions?.seen?.run) : undefined;
  const snooze = actions?.snooze;
  const close = actions?.close;
  const done = actions?.done;
  const archive = actions?.archive;
  const mergeDone = actions?.mergeDone;
  const sign = (name: keyof typeof CHORDS) => chordSigns(CHORDS[name] ?? '');
  const strip: StripAction[] = own ?? [
    // Only where there's a PR to merge, and first, where its coming and going moves nothing
    // else: the strip keeps to the right.
    ...(mergeDone ? [{ label: 'Merge and done', icon: 'merge', chord: CHORDS.mergeDone, roomy: true, title: `${mergeDone.label.replace('…', '')} (${sign('mergeDone')}): it merges once you confirm, and then this session is marked done`, run: () => mergeDone.run() }] : []),
    {
      label: 'Seen',
      chord: CHORDS.seen,
      title: !seen ? (next && queue?.waiting === false ? `Nothing to mark as seen here: ${sign('seen')} goes on to the next` : 'Nothing to mark as seen here') : next && queue?.waiting !== false ? 'Mark as seen and go on to the next' : 'Mark as seen: it goes idle and leaves what waits on you',
      run: seen && (() => seen()),
    },
    { label: 'Done', chord: CHORDS.done, title: done && session ? doneTitle(session) : 'It’s archived already', run: done && (() => done.run()) },
    { label: 'Snooze', keys: snoozeRange(), title: snooze ? `Snooze: it waits apart until its time comes. ${snoozeKeys(snooze)}` : 'It can’t be snoozed', menu: snooze?.items },
    // Short, as Done beside it is, so more of the strip shows before any goes into ⋯.
    { label: close?.label === 'End background job' ? 'End job' : 'Close', chord: CHORDS.close, title: close ? `${close.label}: the conversation stays, to go on with later` : 'It has no terminal of its own to close', run: close && (() => close.run()) },
    { label: archive?.label === 'Unarchive' ? 'Unarchive' : 'Archive', chord: CHORDS.archive, title: archive?.label ?? 'Archive', run: archive && (() => archive.run()) },
  ];
  const fit = useFit(stripRef, !!hotkeys, strip.map((action) => action.label).join('|'));
  if (!hotkeys) {
    if (!notice && !next) return null;
    return (
      <div className="ca-reply-queue">
        <StripNote notice={notice} />
        {next && queue && (
          <Button variant="ghost" size="sm" kbd={[...nextKey]} title={`Next: ${next.title}`} onClick={queue.onNext}>
            Next
          </Button>
        )}
      </div>
    );
  }
  const shown = strip.length - fit.over;
  const over = strip.slice(shown);
  const gap = <span className="ca-hotkeys-gap" aria-hidden="true" />;
  const buttons: ReactNode = (
    <div className="ca-hotkeys" role="toolbar" aria-label={own ? 'What’s commonly done to this PR' : 'What’s commonly done to this session'}>
      {strip.map((action, i) => (
        <Hotkey key={action.label} label={action.label} icon={action.icon} keys={action.keys ?? (action.chord && chordSigns(action.chord))} title={action.title} run={action.menu ? drop(action.menu) : action.run} room={action.roomy ? fit.room : undefined} className={cx('is-foldable', i >= shown && 'is-folded')} />
      ))}
      <Hotkey icon="dots" title={over.length ? `More: ${over.map((action) => action.label).join(', ')}` : 'More'} run={over.length ? drop(over.map(itemOf)) : undefined} className={cx('is-more', !over.length && 'is-folded')} />
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
    <div ref={stripRef} className={cx('ca-reply-queue', 'has-keys', hotkeys.titleBar && 'is-titlebar')} data-tauri-drag-region={hotkeys.titleBar ? 'deep' : undefined}>
      <StripNote notice={notice} />
      {buttons}
    </div>
  );
}

export function SessionPanel({ variant = 'panel', zoom, queue, notice, hotkeys, autoFocus, keys, onSending, onSent, onFailed, onAnswered, onDone, onLoaded, onThreadMenu, find, dialog, ...header }: SessionPanelProps) {
  const { lane, session: s, apps, now, onOpen, onMenu, device, renaming } = header;
  const terminal = apps.terminal;
  const openIt = () => onOpen(s.openTarget);
  // Sending and marking seen go on to the next only from a session that waits on you itself.
  const goesOn = queue?.waiting !== false && queue?.next;
  // Its shell blocks run in a tab of yours here, as `!` in its box does, so not for a session on
  // another device, and the session is told what they printed.
  const folder = shortOf(s.key) ? null : runFolder(lane, s);
  // The box goes under the conversation unless the session's device can't be reached from here.
  const replies = !device || (device.online && device.remoteControl);
  const key = s.key;
  const run = useMemo(() => (folder === null ? undefined : { folder, terminal, run: (command: string, at: string, block: boolean) => runCommand(key, at, command, block) }), [folder, terminal, key]);
  // Moving it to another account asks first (MoveAccount.tsx), from its header's box or the
  // account in its box, for the session it was asked for only.
  const moves = header.actions?.moveAccounts;
  const [moving, setMoving] = useState<{ key: string; to: string | null } | null>(null);
  const ask = (to: string | null) => setMoving({ key, to });
  // Edit from here asks for your edit first (EditFrom.tsx), where the session's Mac takes requests
  // from here, as its box does, and only for the session it was asked for.
  const edit = replies ? header.actions?.edit : undefined;
  const [editing, setEditing] = useState<{ key: string; prompt: TranscriptItem } | null>(null);
  const body = (
    <>
          <QueueStrip queue={queue} notice={notice} hotkeys={hotkeys} session={s} actions={header.actions} onSeen={header.onSeen} />
          <SessionHeader {...header} seenAbove={!!hotkeys} compact={variant === 'card'} accountBelow={replies} inMain={variant === 'panel'} onMove={moves && (() => ask(null))} />
          <SessionThread
            key={s.key}
            session={s}
            now={now}
            onOpen={openIt}
            onLoaded={onLoaded}
            answeredBelow={replies}
            run={run}
            onMenu={onThreadMenu}
            find={find}
            tips={variant === 'panel'}
            onEdit={edit && ((prompt) => setEditing({ key, prompt }))}
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
            <Reply session={s} terminal={terminal} runIn={folder ?? undefined} onOpen={openIt} autoFocus={autoFocus && !renaming} keys={keys} onSending={onSending} onSent={onSent} onFailed={onFailed} onAnswered={onAnswered} onNext={goesOn ? onDone : undefined} sayKeys={!!hotkeys} moves={moves} onMove={ask} />
          )}
          {moves && moving?.key === key && <MoveDialog session={s} moves={moves} to={moving.to} terminal={terminal} onClose={() => setMoving(null)} />}
          {edit && editing?.key === key && <EditDialog session={s} prompt={editing.prompt} onEdit={edit} onClose={() => setEditing(null)} />}
          {dialog}
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

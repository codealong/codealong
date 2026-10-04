// The session the bar docked in the notch shows under its pills (Notch.tsx): a click on a
// session's pill opens the notch further, into that session with its header, its conversation and
// the box that answers it, the card the peek shows under a floating bar (SessionCard.tsx), dark
// as the notch is. There's no peek beside a docked bar: the notch itself is where a session is
// read and answered. A click on the notch itself, off its pills and buttons, opens it on the
// current one: the session it showed last while that still waits on you, else the first that
// does. A click on the pill again or on the notch, Escape, or a click anywhere else puts it away.
//
// It has the keyboard from the click, and works through what waits on you as a session's panel
// in Sessions does, with the same keys, from the same code (queue.ts, useWaitingQueue.ts): ⌘⏎ in
// the box sends (or allows the call the session waits on) and goes on to the next that waits on
// you, ⌘E marks a finished turn seen and goes on, ⌘] or ⌥⇧↓ skips to it and ⌥⇧↑ goes back, ⌥↑ ⌥↓
// step along the bar's sessions, the running ones too, and ⌃- and ⌃⇧- go back and forward
// through the ones you looked at here (history.ts). The pill
// of the one shown is marked, and a strip over its header has what's commonly done to it as
// buttons that say their keys (SessionPanel.tsx `Hotkeys`) and says how many more wait
// and how the last one went. Marked seen, snoozed or archived, a session leaves the bar, and the
// next that waits takes its place; with none left the notch closes, and the app you were in has
// the keyboard back. The session's own shortcuts (⌃⌘A, ⌃⌘R, ⌃⌘P) are its card's.
//
// `useNotchSession` is the bar's part: which session shows, the queue and the keys. `NotchSession`
// is the card, which reads its session from the board while it's up (cardHost.ts).

import { useCallback, useEffect, useRef, useState } from 'react';
import { keepNotchSession, notchKeys, onNotchKeys, sessionAt } from '../api';
import { renameAhead } from '../ahead';
import { currentItem, waitingItems } from '../bar';
import { useSessionHistory } from '../history';
import { seenable } from '../lanes';
import type { Left } from '../menus';
import type { Reads } from '../queue';
import { errorText } from '../report';
import type { BarItem, Board } from '../types';
import { useWaitingQueue } from '../useWaitingQueue';
import { useCardHost, useLiveSession, type Told } from './cardHost';
import { SessionCard, type WindowCan } from './SessionCard';
import type { Notice, Queue } from './SessionPanel';

/** How long a session's card stays after it's put away, while the notch closes over it. */
const LINGER_MS = 360;
/** Where the sessions looked at in the notch are kept, apart from the main window's. */
const HISTORY_KEY = 'codealong.sessionHistory:notch';
/** How the queue reads the bar's pills, which carry their session's key and times without it. */
const ITEMS: Reads<BarItem> = { keyOf: (i) => i.key, sinceOf: (i) => i.since, titleOf: (i) => i.title };

export interface NotchSessionState {
  /** The session shown, or the one being put away while its card lingers. */
  key: string | null;
  /** It shows; false as it's put away. */
  open: boolean;
  /** It has the keyboard. */
  keys: boolean;
  /** Asked for with its name ready to change (Rename… in a pill's menu). */
  rename: boolean;
  /** What's left to work through after it, and how the last one went. */
  queue: Queue | undefined;
  notice: Notice | null;
  /** Back to the session looked at before it; null with none. */
  back: (() => void) | null;
  /** Shows a session, with the keyboard; `rename` with its name ready to change. */
  show: (key: string, rename?: boolean) => void;
  /** Shows it, or puts it away if it's the one showing: a click on its pill. */
  toggle: (key: string) => void;
  /** A click on the notch itself: it opens on the current session, or puts the one showing away.
   *  False with no session to show. */
  press: () => boolean;
  close: () => void;
  /** A click in the card of a session that lost the keyboard takes it back. */
  takeKeys: () => void;
  /** The session `from` was sent a message or allowed its call with ⌘⏎: on to the next that
   *  waits on you, saying what was done. */
  next: (done: string, from: string) => void;
  /** The session left the bar (marked seen, snoozed, archived): on to the next, or closed when
   *  none waits. */
  left: (key: string, done: Left) => void;
}

/** The session the docked bar shows: which one, what it works through, and its keys. `items` are
 *  the bar's sessions as it shows them, in its order; `known` says whether the bar still has a
 *  session, in any of its pills; `docked` while the bar is in the notch. */
export function useNotchSession(items: BarItem[], known: (key: string) => boolean, docked: boolean): NotchSessionState {
  const [shown, setShown] = useState<{ key: string; rename: boolean } | null>(null);
  const [open, setOpen] = useState(false);
  const [keys, setKeys] = useState(false);
  const linger = useRef<number | null>(null);
  const itemsNow = useRef(items);
  itemsNow.current = items;
  // The session showing, as of the last call rather than the last render: two clicks can come
  // before the page draws again.
  const shownNow = useRef<string | null>(null);
  // The one shown last, which a click on the notch comes back to while it still waits on you.
  const last = useRef<string | null>(null);

  // Back and forward through the sessions looked at here, as in the main window. One the bar no
  // longer has is passed over.
  const key = open ? (shown?.key ?? null) : null;
  const history = useSessionHistory(key, known, HISTORY_KEY);

  // The way through what waits on you (useWaitingQueue.ts), over the bar's own list: the pills
  // that wait, with ⌥↑ ⌥↓ along all of them. A session that leaves the bar takes the notch on to
  // the next whether it waited or not, since it has no pill to stay on. `show` is below: the
  // queue calls it only from a key or an answer.
  const item = key ? items.find((i) => i.key === key) : undefined;
  const queue = useWaitingQueue<BarItem>({
    waiting: waitingItems(items),
    along: items,
    reads: ITEMS,
    // One the bar's groups no longer list (it went idle, or it's another project's, come to
    // through ⌃- or from a session it started) is still shown until you leave it. The bar has
    // no pill to say whether there's anything in it to mark seen, so ⌘E goes on from it.
    shown: key ? { key, title: item?.title ?? 'The last one', since: item?.since, seenable: !!item && seenable(item) } : null,
    show: (to) => show(to.key),
    travel: (dir) => {
      const to = dir === 'back' ? history.goBack() : history.goForward();
      if (to) show(to);
      return to !== null;
    },
  });
  const { fresh, onKey: onQueueKey, run, answered, left: leftQueue, say } = queue;

  const show = useCallback(
    (key: string, rename = false) => {
      if (linger.current != null) window.clearTimeout(linger.current);
      linger.current = null;
      // Opening on one starts afresh; going on from one to the next keeps what's been dealt with.
      if (shownNow.current === null) fresh();
      shownNow.current = key;
      last.current = key;
      setShown({ key, rename });
      setOpen(true);
      setKeys(true);
      void notchKeys(true);
    },
    [fresh],
  );

  const close = useCallback(() => {
    if (shownNow.current === null) return;
    shownNow.current = null;
    setOpen(false);
    setKeys(false);
    say(null);
    void notchKeys(false);
    if (linger.current != null) window.clearTimeout(linger.current);
    linger.current = window.setTimeout(() => {
      linger.current = null;
      setShown(null);
    }, LINGER_MS);
  }, [say]);

  const toggle = useCallback((key: string) => (shownNow.current === key ? close() : show(key)), [close, show]);

  const press = useCallback(() => {
    if (shownNow.current !== null) {
      close();
      return true;
    }
    const to = currentItem(itemsNow.current, last.current);
    if (to) show(to.key);
    return to !== null;
  }, [close, show]);

  const takeKeys = useCallback(() => {
    if (shownNow.current === null) return;
    setKeys(true);
    void notchKeys(true);
  }, []);

  const next = useCallback((done: string, from: string) => void answered(from, done), [answered]);
  /** The session left the bar: the next that waits takes its place, and with none the notch
   *  closes, since there's nothing left under it to show. */
  const left = useCallback(
    (key: string, done: Left) => {
      if (shownNow.current === key && leftQueue(key, done) === 'last') close();
    },
    [close, leftQueue],
  );

  // The bar left the notch: there's nowhere to show one.
  useEffect(() => {
    if (!docked) close();
  }, [docked, close]);

  // The keyboard went without being given up: the shell puts the session away unless the pointer
  // is still on it (Notch.tsx).
  useEffect(() => onNotchKeys((has) => setKeys(has)), []);

  useEffect(
    () => () => {
      if (linger.current != null) window.clearTimeout(linger.current);
    },
    [],
  );

  // Its keys, while it's up. Escape closes it, and ⌘Q, ⌘H and ⌘W close it rather than going to
  // Codealong, since the menu bar is the other app's. The rest are the queue's, the ones a
  // session's panel has in Sessions (queue.ts `QUEUE_CHORDS`). Typing with nothing focused goes
  // to the box.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      const mod = e.metaKey && !e.ctrlKey && !e.altKey;
      if (e.key === 'Escape' || (mod && !e.shiftKey && ['q', 'h', 'w'].includes(e.key.toLowerCase()))) {
        e.preventDefault();
        close();
        return;
      }
      // ⌘E on a session that can be marked seen isn't taken here: it's its card's
      // (SessionCard.tsx), which marks it seen and says it left.
      if (onQueueKey(e)) {
        e.preventDefault();
        return;
      }
      const el = document.activeElement;
      if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && (!el || el === document.body)) {
        document.querySelector<HTMLTextAreaElement>('.ca-notch-session .ca-composer textarea')?.focus();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, close, onQueueKey]);

  const back = history.back ? () => void run('historyBack') : null;

  return { key: shown?.key ?? null, open, keys, rename: shown?.rename ?? false, queue: queue.queue, notice: queue.notice, back, show, toggle, press, close, takeKeys, next, left };
}

interface NotchSessionProps {
  /** The session to show. */
  sessionKey: string;
  /** Its name is ready to change. */
  rename: boolean;
  now: number;
  queue: Queue | undefined;
  notice: Notice | null;
  /** Back to the session looked at before (⌃-); null with none. */
  onBack: (() => void) | null;
  /** It has the keyboard; a click in it takes it. */
  keys: boolean;
  onKeys: () => void;
  /** Shows another session in its place: one it came from, or that came of it. */
  onShow: (key: string) => void;
  onClose: () => void;
  /** Sent or allowed with ⌘⏎, and what took it off the bar: on to the next. */
  onNext: (done: string, from: string) => void;
  onLeft: (key: string, done: Left) => void;
  /** The board no longer has it. */
  onGone: (key: string) => void;
}

export function NotchSession({ sessionKey, rename, now, queue, notice, onBack, keys, onKeys, onShow, onClose, onNext, onLeft, onGone }: NotchSessionProps) {
  // The session as the engine has it, asked for as it's shown; the one before stays until it
  // comes, so going from one to the next never shows an empty notch.
  const [told, setTold] = useState<(Told & { apps: Board['apps'] }) | null>(null);
  const gone = useRef(onGone);
  gone.current = onGone;
  useEffect(() => {
    let over = false;
    sessionAt(sessionKey).then(
      (at) => {
        if (over) return;
        if (at) setTold(at);
        else gone.current(sessionKey);
      },
      () => {
        if (!over) gone.current(sessionKey);
      },
    );
    return () => {
      over = true;
    };
  }, [sessionKey]);

  const live = useLiveSession(told, true);
  const host = useCardHost({ sessionKey: told?.session.key ?? null, live, apps: told?.apps, terminal: told?.apps.terminal ?? 'Terminal', away: onClose });
  // The session whose name its own menu asked to change.
  const [renameAsked, setRenameAsked] = useState<string | null>(null);
  // A name asked for from a pill's menu is asked once.
  const [renamed, setRenamed] = useState<string | null>(null);
  const card = useRef<HTMLDivElement>(null);

  const { lane, session } = live;
  if (!told || !lane || !session) return null;
  const { report } = host;

  /** Renames the session, or leaves its name. Asked for from a pill's menu, that's what the notch
   *  opened for, and it closes; from the session's own menu, it's back to the conversation. */
  const rename_ = (name: string | null) => {
    const here = renameAsked === session.key;
    setRenameAsked(null);
    setRenamed(session.key);
    if (name) renameAhead(session.key, name).catch((e: unknown) => report.error(errorText(e)));
    if (!here) onClose();
  };

  /** What the notch does for its session's actions (SessionCard.tsx has the rest): what takes the
   *  session off the bar goes on to the next that waits on you. */
  const can: WindowCan = { report, open: host.open, reveal: () => host.showSession(lane, session), rename: () => setRenameAsked(session.key), left: (done) => onLeft(session.key, done) };

  /** Keeps the session on the desktop, in a window of its own where its card is, and the notch
   *  closes over it. */
  const keep = () => {
    const r = card.current?.getBoundingClientRect();
    if (!r) return;
    keepNotchSession(session.key, { x: r.left, y: r.top, w: r.width, h: r.height }).then(onClose, (e: unknown) => report.error(errorText(e)));
  };

  return (
    <div
      ref={card}
      className="ca-notch-card"
      onPointerDown={() => {
        // One that lost the keyboard to another app takes it back on a click.
        if (!keys) onKeys();
      }}
    >
      <SessionCard
        lane={lane}
        session={session}
        apps={told.apps}
        now={now}
        board={live.board}
        can={can}
        // A session it came from, or that came of it: its menu shows it in Codealong.
        canFor={(l, s) => ({ report, open: host.open, reveal: () => host.showSession(l, s) })}
        outgoing={session.key !== sessionKey}
        onShowSession={(_l, s) => onShow(s.key)}
        onShowLane={host.showLane}
        onShowTodo={host.showTodo}
        onShowPr={host.showPr}
        autoFocus
        keys
        queue={queue}
        notice={notice}
        hotkeys={{ onBack, onClose }}
        renaming={(rename && renamed !== session.key) || renameAsked === session.key}
        onRename={rename_}
        onKeep={keep}
        keepTitle="Keep on the desktop: it gets a window of its own, here, that stays up until you close it."
        onDone={(done) => onNext(done, session.key)}
        onSent={(sent) => {
          // What you'd want to know goes on the bar: the prompt is on the clipboard, say, or it
          // went as a message from Codealong because its terminal couldn't be typed into.
          if (sent.opened || sent.fellBack) report.info(sent.message);
        }}
        onFailed={(why) => report.error(why)}
      />
    </div>
  );
}

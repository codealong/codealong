// The session the bar docked in the notch shows under its pills (Notch.tsx): a click on a
// session's pill opens the notch further, into that session with its header, its conversation and
// the box that answers it, the card the peek shows under a floating bar (SessionCard.tsx), dark
// as the notch is. There's no peek beside a docked bar: the notch itself is where a session is
// read and answered. A click on the notch itself, off its pills and buttons, opens it on the
// current one: the session it showed last while that still waits on you, else the first that
// does. A click on the pill again or on the notch, Escape, or a click anywhere else puts it away.
//
// It has the keyboard from the click, and works through what waits on you as a session's panel
// in Sessions does, with the same keys: ⌘⏎ in the box sends (or allows the call the session waits
// on) and goes on to the next that waits on you, ⌘E marks a finished turn seen and goes on, ⌘] or
// ⌥⇧↓ skips to it and ⌥⇧↑ goes back, ⌥↑ ⌥↓ step along the bar's sessions, the running ones too,
// and ⌃- and ⌃⇧- go back and forward through the ones you looked at here (history.ts). The pill
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
import { currentItem, stepItem, waitingAfter, waitingItems } from '../bar';
import { historyChord, useSessionHistory } from '../history';
import type { Left } from '../menus';
import type { Handled } from '../quick';
import { errorText } from '../report';
import type { BarItem, Board } from '../types';
import { useCardHost, useLiveSession, type Told } from './cardHost';
import { SessionCard, type WindowCan } from './SessionCard';
import type { Notice, Queue } from './SessionPanel';

/** How long a session's card stays after it's put away, while the notch closes over it. */
const LINGER_MS = 360;
/** How long the strip says how the last one went. */
const NOTICE_MS = 4000;
/** Where the sessions looked at in the notch are kept, apart from the main window's. */
const HISTORY_KEY = 'codealong.sessionHistory:notch';


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
  /** Sent or allowed with ⌘⏎: on to the next that waits on you, saying what was done. */
  next: (done: string) => void;
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
  const [notice, setNotice] = useState<Notice | null>(null);
  // The sessions dealt with since the notch opened on one, each with the time of the status it
  // had then: left out of the queue until they wait on you anew (quick.ts `Handled`).
  const handled = useRef<Handled>(new Map());
  const linger = useRef<number | null>(null);
  const itemsNow = useRef(items);
  itemsNow.current = items;
  // The session showing, as of the last call rather than the last render: two clicks can come
  // before the page draws again.
  const shownNow = useRef<string | null>(null);
  // The one shown last, which a click on the notch comes back to while it still waits on you.
  const last = useRef<string | null>(null);

  const show = useCallback((key: string, rename = false) => {
    if (linger.current != null) window.clearTimeout(linger.current);
    linger.current = null;
    // Opening on one starts afresh; going on from one to the next keeps what's been dealt with.
    if (shownNow.current === null) {
      handled.current = new Map();
      setNotice(null);
    }
    shownNow.current = key;
    last.current = key;
    setShown({ key, rename });
    setOpen(true);
    setKeys(true);
    void notchKeys(true);
  }, []);

  const close = useCallback(() => {
    if (shownNow.current === null) return;
    shownNow.current = null;
    setOpen(false);
    setKeys(false);
    setNotice(null);
    void notchKeys(false);
    if (linger.current != null) window.clearTimeout(linger.current);
    linger.current = window.setTimeout(() => {
      linger.current = null;
      setShown(null);
    }, LINGER_MS);
  }, []);

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

  /** On from the session shown to the next that waits on you; `done` says what was done to it,
   *  and without it it's a skip, which comes round again after the rest. */
  const goOn = useCallback(
    (done: string | null, orClose = false) => {
      const key = shownNow.current;
      if (!key) return;
      const item = itemsNow.current.find((i) => i.key === key);
      if (done) handled.current.set(key, item?.since ?? Date.now());
      const next = waitingAfter(itemsNow.current, key, handled.current)[0];
      if (!next) {
        if (orClose) close();
        return;
      }
      setNotice(done ? { text: `${item?.title ?? 'The last one'}: ${done}`, error: false } : null);
      show(next.key);
    },
    [close, show],
  );

  /** Back to the one before it that waits on you, the way a skip goes on. */
  const goBack = useCallback(() => {
    const key = shownNow.current;
    if (!key) return;
    const before = waitingAfter(itemsNow.current, key, handled.current);
    const prev = before[before.length - 1];
    if (!prev) return;
    setNotice(null);
    show(prev.key);
  }, [show]);

  const next = useCallback((done: string) => goOn(done), [goOn]);
  const left = useCallback(
    (key: string, done: Left) => {
      if (shownNow.current === key) goOn(`${done}.`, true);
    },
    [goOn],
  );

  // Back and forward through the sessions looked at here, as in the main window. One the bar no
  // longer has is passed over.
  const history = useSessionHistory(open ? (shown?.key ?? null) : null, known, HISTORY_KEY);
  const travel = useRef(history);
  travel.current = history;

  // The bar left the notch: there's nowhere to show one.
  useEffect(() => {
    if (!docked) close();
  }, [docked, close]);

  // The keyboard went without being given up: the shell puts the session away unless the pointer
  // is still on it (Notch.tsx).
  useEffect(() => onNotchKeys((has) => setKeys(has)), []);

  useEffect(() => {
    if (!notice) return;
    const t = window.setTimeout(() => setNotice(null), NOTICE_MS);
    return () => window.clearTimeout(t);
  }, [notice]);

  useEffect(
    () => () => {
      if (linger.current != null) window.clearTimeout(linger.current);
    },
    [],
  );

  // Its keys, while it's up: the ones a session's panel has in Sessions. Escape closes it, and ⌘Q,
  // ⌘H and ⌘W close it rather than going to Codealong, since the menu bar is the other app's.
  // Typing with nothing focused goes to the box.
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
      if (historyChord(e)) {
        e.preventDefault();
        const to = e.shiftKey ? travel.current.goForward() : travel.current.goBack();
        if (to) {
          setNotice(null);
          show(to);
        }
        return;
      }
      // ] goes by its key's code, which layouts that type it with ⌥ keep.
      if (mod && !e.shiftKey && e.code === 'BracketRight') {
        e.preventDefault();
        goOn(null);
        return;
      }
      // ⌘E from a session that doesn't wait on you (it runs, say) goes on as ⌘] does: there's
      // nothing to mark seen. Marking one seen is its card's (SessionCard.tsx), and goes on from
      // there; one that waits on a call to allow stays.
      if (mod && !e.shiftKey && e.key.toLowerCase() === 'e') {
        if (!waitingItems(itemsNow.current).some((i) => i.key === shownNow.current)) {
          e.preventDefault();
          goOn(null);
        }
        return;
      }
      const vertical = e.key === 'ArrowUp' || e.key === 'ArrowDown';
      if (vertical && e.altKey && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        const key = shownNow.current;
        if (e.shiftKey) {
          if (e.key === 'ArrowDown') goOn(null);
          else goBack();
        } else if (key) {
          const to = stepItem(itemsNow.current, key, e.key === 'ArrowDown' ? 1 : -1);
          if (to && to.key !== key) {
            setNotice(null);
            show(to.key);
          }
        }
        return;
      }
      const el = document.activeElement;
      if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && (!el || el === document.body)) {
        document.querySelector<HTMLTextAreaElement>('.ca-notch-session .ca-composer textarea')?.focus();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, close, goOn, goBack, show]);

  // What the strip over the header goes by, as in Sessions: what's left after the one shown,
  // and whether it's one of those waiting itself.
  const key = open ? (shown?.key ?? null) : null;
  const after = key ? waitingAfter(items, key, handled.current) : [];
  const queue: Queue | undefined = key !== null ? { left: after.length, next: after[0] ?? null, onNext: () => goOn(null), waiting: waitingItems(items).some((i) => i.key === key) } : undefined;
  const back = history.back
    ? () => {
        const to = travel.current.goBack();
        if (!to) return;
        setNotice(null);
        show(to);
      }
    : null;

  return { key: shown?.key ?? null, open, keys, rename: shown?.rename ?? false, queue, notice, back, show, toggle, press, close, takeKeys, next, left };
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
  onNext: (done: string) => void;
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
        onDone={onNext}
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

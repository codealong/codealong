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
// you, ⌘E marks a finished turn seen and goes on, ⌘] or ⌥⇧↓ skips to it and ⌘[ or ⌥⇧↑ goes back,
// ⌥↑ ⌥↓ step along the bar's sessions, the running ones too, and ⌃- and ⌃⇧- go back and forward
// through the ones you looked at here (history.ts). The pill
// of the one shown is marked, and a strip over its header has what's commonly done to it as
// buttons that say their keys (SessionPanel.tsx `Hotkeys`) and says how many more wait
// and how the last one went. Marked seen, snoozed or archived, a session leaves the bar, and the
// next that waits takes its place; with none left, the end of what waits does (QueueEnd.tsx),
// saying how the last one went, as the waiting window and the iPhone say it. (⌘⏎ goes on only
// while something's next: on the last one it sends and stays, as ⏎ does.) So does the shortcut with nothing waiting, though some run, and a click on the notch
// with nothing on the bar. The end keeps the
// keyboard until it's put away as a session is, and what comes to wait on you meanwhile takes its
// place by itself. The session's own shortcuts (⌃⌘A, ⌃⌘R, ⌃⌘P) are its card's.
//
// A PR waiting on your review takes the same place (NotchReview.tsx), among the sessions in the
// bar's order (bar.ts `notchRows`): the keys go from a session to a review and on as one list.
// So does any PR a click on a session's PR chip, or on a link to the PR in its conversation,
// shows: its card leads back to the session, and once the PR is merged, reviewed or snoozed the
// notch goes back there rather than on (PrCard.tsx `from`), as the main window's panel does.
//
// `useNotchStage` is the bar's part: which session or review shows, the queue and the keys.
// `NotchSession` is a session's card, which reads its session from the board while it's up
// (cardHost.ts).

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { PrTab } from '../agentReview';
import { keepNotchSession, notchKeys, onNotchKeys, sessionAt } from '../api';
import { renameAhead, sentWorthSaying } from '../ahead';
import { currentRow, notchRows, ROW_READS, waitingRow, waitingRows, type NotchRow } from '../bar';
import { useSessionHistory } from '../history';
import { seenable } from '../lanes';
import type { Left } from '../menus';
import { errorText } from '../report';
import type { BarItem, BarReview, Board, PrItem } from '../types';
import { useWaitingQueue } from '../useWaitingQueue';
import { useCardHost, useLiveSession, type Told } from './cardHost';
import { dialogUp } from './focusTrap';
import type { CameFrom } from './PrCard';
import { showPrTab } from './PrInspector';
import { SessionCard, type WindowCan } from './SessionCard';
import type { Notice, Queue } from './SessionPanel';
import { chordMod, held } from '../platform';

/** How long a session's card stays after it's put away, while the notch closes over it. */
const LINGER_MS = 360;
/** Where the sessions looked at in the notch are kept, apart from the main window's. */
const HISTORY_KEY = 'codealong.sessionHistory:notch';
/** The end of what waits shows as this key, which no session's or PR's can be. */
const END = 'end';
/** What shows under the pills: a session, a review, or the end of what waits. */
type ShownKind = NotchRow['kind'] | 'end';
/** What a key the bar no longer lists is: a PR's key is `owner/name#12`, a session's has its
 *  harness and account in front, with colons. */
function kindOf(key: string): NotchRow['kind'] {
  return key.includes(':') ? 'session' : 'review';
}

export interface NotchStageState {
  /** The session or review shown, or the one being put away while its card lingers; with `kind`
   *  `end`, the end of what waits. */
  key: string | null;
  kind: ShownKind;
  /** It shows; false as it's put away. */
  open: boolean;
  /** It has the keyboard. */
  keys: boolean;
  /** Asked for with its name ready to change (Rename… in a pill's menu). */
  rename: boolean;
  /** What's left to work through after it, and how the last one went. */
  queue: Queue | undefined;
  notice: Notice | null;
  /** At the end of what waits, how the last one went (useWaitingQueue.ts `ended`). */
  ended: string | null;
  /** Back to the session looked at before it; null with none. */
  back: (() => void) | null;
  /** The session the PR shown was shown in place of (`showPr`); null for any other. */
  from: CameFrom | null;
  /** Shows a session or a review by its key, with the keyboard; `rename` with a session's name
   *  ready to change. */
  show: (key: string, rename?: boolean) => void;
  /** Shows the PR `key` in place of the session `from`: a click on its PR chip, or on a link to
   *  it in the session's conversation. */
  showPr: (key: string, from: CameFrom) => void;
  /** Back from the PR `key` shown that way to its session, which says `said` on its strip: how
   *  the PR went. Nothing once another shows. */
  returnFrom: (key: string, said?: string) => void;
  /** Shows it, or puts it away if it's the one showing: a click on its pill. */
  toggle: (key: string) => void;
  /** A click on the notch itself: it opens on the current one, or on the end of what waits with
   *  nothing on the bar, or puts the one showing away. */
  press: () => void;
  /** The shortcut that works through what waits: it opens on what waits on you, or on the end of
   *  what waits with nothing waiting, though some run, or puts the one showing away. */
  workThrough: () => void;
  close: () => void;
  /** A click in the card of a session that lost the keyboard takes it back. */
  takeKeys: () => void;
  /** The session `from` was sent a message or allowed its call with ⌘⏎: on to the next that
   *  waits on you, saying what was done. */
  next: (done: string, from: string) => void;
  /** The session or review left the bar (marked seen, snoozed, archived; approved, reviewed): on
   *  to the next, or the end of what waits when none does. */
  left: (key: string, done: Left) => void;
  /** The review `key` left on didn't go: it's back among what waits, the notch opens on it again
   *  when it had closed, and the strip says why. */
  failed: (key: string, why: string) => void;
  /** A message to the session `key` didn't go, after the notch went on from it: it's back among
   *  what waits, and the strip of the one shown says `said`. False when the notch has closed, for
   *  the bar to say it. */
  unsent: (key: string, said: string) => boolean;
}

/** What the docked bar shows under its pills: which session or review, what it works through,
 *  and its keys. `items` are the bar's sessions as it shows them, in its order, and `reviews` the
 *  PRs waiting on your review; `known` says whether the bar still has a key, in any of its pills;
 *  `docked` while the bar is in the notch. */
export function useNotchStage(items: BarItem[], reviews: BarReview[], known: (key: string) => boolean, docked: boolean): NotchStageState {
  const [shown, setShown] = useState<{ key: string; kind: ShownKind; rename: boolean; from: CameFrom | null } | null>(null);
  const [open, setOpen] = useState(false);
  const [keys, setKeys] = useState(false);
  const linger = useRef<number | null>(null);
  const rows = useMemo(() => notchRows(items, reviews), [items, reviews]);
  const rowsNow = useRef(rows);
  rowsNow.current = rows;
  // The session showing, as of the last call rather than the last render: two clicks can come
  // before the page draws again.
  const shownNow = useRef<string | null>(null);
  // The one shown last, which a click on the notch comes back to while it still waits on you.
  const last = useRef<string | null>(null);
  // The session the PR showing was shown in place of, as of the last call.
  const fromNow = useRef<CameFrom | null>(null);
  // The PRs shown from a session's chip, with the session each was shown from: the bar lists only
  // those waiting on your review, and these are still there to go back and forward to, leading
  // back to their session as they did. One shown since some other way (a review the queue went
  // on to) leads nowhere.
  const fromChips = useRef(new Map<string, CameFrom>());

  // Back and forward through the sessions looked at here, as in the main window. One the bar no
  // longer has is passed over. The end of what waits shows no session.
  const key = open && shown?.kind !== 'end' ? (shown?.key ?? null) : null;
  const history = useSessionHistory(key, (k) => known(k) || fromChips.current.has(k), HISTORY_KEY);

  // The way through what waits on you (useWaitingQueue.ts), over the bar's own list: the pills
  // and reviews that wait, with ⌥↑ ⌥↓ along all of them. One that leaves the bar takes the notch
  // on to the next whether it waited or not, since it has no pill to stay on. `show` is below:
  // the queue calls it only from a key or an answer.
  const row = key ? rows.find((r) => r.key === key) : undefined;
  const queue = useWaitingQueue<NotchRow>({
    waiting: waitingRows(rows),
    along: rows,
    reads: ROW_READS,
    // One the bar's groups no longer list (it went idle, or it's another project's, come to
    // through ⌃- or from a session it started) is still shown until you leave it. The bar has
    // no pill to say whether there's anything in it to mark seen, so ⌘E goes on from it. A
    // review has nothing to mark seen either: ⌘E skips it.
    shown: key ? { key, title: row ? ROW_READS.titleOf(row) : 'The last one', since: row ? ROW_READS.sinceOf(row) : undefined, seenable: row?.kind === 'session' && seenable(row.item), skipsOnSeen: row?.kind === 'review' } : null,
    show: (to) => show(to.key),
    travel: (dir) => {
      const to = dir === 'back' ? history.goBack() : history.goForward();
      if (to) place(to, false, fromChips.current.get(to) ?? null);
      return to !== null;
    },
  });
  const { fresh, onKey: onQueueKey, run, answered, left: leftQueue, say, again, first } = queue;

  const place = useCallback(
    (key: string, rename: boolean, from: CameFrom | null) => {
      if (linger.current != null) window.clearTimeout(linger.current);
      linger.current = null;
      // Opening on one starts afresh; going on from one to the next keeps what's been dealt with.
      if (shownNow.current === null) fresh();
      shownNow.current = key;
      last.current = key;
      fromNow.current = from;
      if (from) fromChips.current.set(key, from);
      else fromChips.current.delete(key);
      setShown({ key, kind: rowsNow.current.find((r) => r.key === key)?.kind ?? kindOf(key), rename, from });
      setOpen(true);
      setKeys(true);
      void notchKeys(true);
    },
    [fresh],
  );
  const show = useCallback((key: string, rename = false) => place(key, rename, null), [place]);
  /** The end of what waits, with the keyboard: opened on with nothing to show, or gone on to from
   *  the last one, which it says how went. */
  const end = useCallback(() => {
    if (linger.current != null) window.clearTimeout(linger.current);
    linger.current = null;
    if (shownNow.current === null) fresh();
    shownNow.current = END;
    fromNow.current = null;
    setShown({ key: END, kind: 'end', rename: false, from: null });
    setOpen(true);
    setKeys(true);
    void notchKeys(true);
  }, [fresh]);
  const showPr = useCallback((key: string, from: CameFrom) => place(key, false, from), [place]);
  const returnFrom = useCallback(
    (key: string, said?: string) => {
      // Only while that PR shows: a review started from it can say so after you've moved on, or
      // after the notch has closed.
      const from = fromNow.current;
      if (!from || shownNow.current !== key) return;
      show(from.key);
      if (said) say({ text: said, error: false }, from.key);
    },
    [show, say],
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

  /** Opens on the row `pick` gives, or the end of what waits with none; or puts away the one
   *  showing. */
  const opens = useCallback(
    (pick: typeof currentRow) => {
      if (shownNow.current !== null) return close();
      const to = pick(rowsNow.current, last.current);
      if (to) show(to.key);
      else end();
    },
    [close, show, end],
  );
  const press = useCallback(() => opens(currentRow), [opens]);
  const workThrough = useCallback(() => opens(waitingRow), [opens]);

  const takeKeys = useCallback(() => {
    if (shownNow.current === null) return;
    setKeys(true);
    void notchKeys(true);
  }, []);

  const next = useCallback(
    (done: string, from: string) => {
      if (answered(from, done) === 'last') end();
    },
    [answered, end],
  );
  /** The session or review left the bar: the next that waits takes its place, and with none the
   *  end of what waits does. */
  const left = useCallback(
    (key: string, done: Left) => {
      if (shownNow.current === key && leftQueue(key, done) === 'last') end();
    },
    [end, leftQueue],
  );

  // At the end of what waits, the first that comes to wait on you takes its place, as in the
  // waiting window; not one just dealt with that the bar still shows waiting (queue.ts
  // `firstLeft`).
  const atEnd = open && shown?.kind === 'end';
  const firstKey = first ? ROW_READS.keyOf(first) : null;
  useEffect(() => {
    if (atEnd && firstKey !== null) show(firstKey);
  }, [atEnd, firstKey, show]);

  /** A review sent from the notch didn't go: the PR comes round again, and where the notch closed
   *  or ended on it as the last one, it opens on it again, saying why. */
  const failed = useCallback(
    (key: string, why: string) => {
      again(key);
      if (shownNow.current === null || shownNow.current === END) {
        show(key);
        say({ text: why, error: true }, key);
      } else say({ text: why, error: true });
    },
    [again, say, show],
  );
  const unsent = useCallback(
    (key: string, said: string) => {
      again(key);
      // At the end of what waits (⏎, then the last one marked seen before the message failed),
      // the notch goes back to it, saying why, as for a review: it may not wait on you, so
      // nothing else would bring it back.
      if (shownNow.current === END) show(key);
      return say({ text: said, error: true }, shownNow.current === key ? key : undefined);
    },
    [again, say, show],
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
      const mod = held(e, 'Cmd') || held(e, 'Shift', 'Cmd');
      if (e.key === 'Escape' || (mod && !e.shiftKey && ['q', 'h', 'w'].includes(e.key.toLowerCase()))) {
        e.preventDefault();
        close();
        return;
      }
      // A dialog over the card keeps the rest (focusTrap.ts `dialogUp`).
      if (dialogUp()) return;
      // ⌘E on a session that can be marked seen isn't taken here: it's its card's
      // (SessionCard.tsx), which marks it seen and says it left.
      if (onQueueKey(e)) {
        e.preventDefault();
        return;
      }
      const el = document.activeElement;
      if (e.key.length === 1 && !chordMod(e) && (!el || el === document.body)) {
        document.querySelector<HTMLTextAreaElement>('.ca-notch-session .ca-composer textarea')?.focus();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, close, onQueueKey]);

  const back = history.back ? () => void run('historyBack') : null;

  return { key: shown?.key ?? null, kind: shown?.kind ?? 'session', open, keys, rename: shown?.rename ?? false, queue: queue.queue, notice: queue.notice, ended: queue.ended, back, from: shown?.from ?? null, show, showPr, returnFrom, toggle, press, workThrough, close, takeKeys, next, left, failed, unsent };
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
  /** Shows a PR the board lists in its place, from its chip or a link to it (NotchStageState
   *  `showPr`). */
  onShowPr: (key: string, from: CameFrom) => void;
  onClose: () => void;
  /** Sent or allowed with ⌘⏎, and what took it off the bar: on to the next. */
  onNext: (done: string, from: string) => void;
  onLeft: (key: string, done: Left) => void;
  /** A message to the session `key` didn't go after the notch went on from it; false when there's
   *  no strip to say `said` on (NotchStageState `unsent`). */
  onUnsent: (key: string, said: string) => boolean;
  /** The board no longer has it. */
  onGone: (key: string) => void;
  /** The zoom the card draws at, in percent, from the bar's feed (zoom.ts). */
  zoom?: number;
}

export function NotchSession({ sessionKey, rename, now, queue, notice, onBack, keys, onKeys, onShow, onShowPr, onClose, onNext, onLeft, onUnsent, onGone, zoom }: NotchSessionProps) {
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
  // A click on a PR's chip, or on a link to it in the conversation: the PR in the session's
  // place, on the tab the link goes to.
  const followPr = (pr: PrItem, tab: PrTab | null = null) => {
    const s = live.session;
    if (!s) return;
    if (tab) showPrTab(pr.key, tab);
    onShowPr(pr.key, { key: s.key, title: s.title });
  };
  const host = useCardHost({ sessionKey: told?.session.key ?? null, live, apps: told?.apps, terminal: told?.apps.terminal ?? 'Terminal', away: onClose, followPr });
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
        zoom={zoom}
        board={live.board}
        can={can}
        // A session it came from, or that came of it: its menu shows it in Codealong.
        canFor={(l, s) => ({ report, open: host.open, reveal: () => host.showSession(l, s) })}
        outgoing={session.key !== sessionKey}
        onShowSession={(_l, s) => onShow(s.key)}
        onShowLane={host.showLane}
        onShowTodo={host.showTodo}
        onShowPr={host.showPr}
        onFollowPr={followPr}
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
          // may not have gone in.
          if (sentWorthSaying(sent)) report.info(sent.message);
        }}
        // It didn't go, after the notch went on from it: the strip of the one shown says why, as
        // the waiting window's does. Where the notch closed on it, the bar says so by its pill.
        onFailed={(why) => {
          if (!onUnsent(session.key, `Didn’t send to ${session.title}: ${why}`)) report.error(why);
        }}
      />
    </div>
  );
}

// A window's way through what waits on you (queue.ts has the rules): the sessions dealt with,
// how the last one went, and the steps a key, a button or an answer takes. Sessions' panel in
// the main window, the session in the notch and the waiting window all work through
// their lists with it, so a key for getting about means one thing in all three. A window passes
// only what differs: its rows, how it shows a session, and what's its own to do.

import { useCallback, useEffect, useRef, useState } from 'react';
import { seenable } from './lanes';
import type { Left } from './menus';
import * as rules from './queue';
import type { Handled, QueueAction, QueueView, Reads, Shown, Step, Went } from './queue';
import { statusTime } from './quick';
import type { Session } from './types';
import type { Notice, Queue } from './ui/SessionPanel';

/** How long the strip says how the last one went. Why one failed stays until something else is
 *  said or the window moves on. */
const NOTICE_MS = 4000;

/** A session shown, from the session itself. */
export function shownSession(s: Session): Shown {
  return { key: s.key, title: s.title, since: statusTime(s), seenable: seenable(s) };
}

/** How the queue reads rows that carry their session: Sessions'. */
export const SESSION_READS: Reads<{ session: Session }> = {
  keyOf: (r) => r.session.key,
  sinceOf: (r) => statusTime(r.session),
  titleOf: (r) => r.session.title,
};

/** What a window says of itself: its lists and the session shown (queue.ts `QueueView`), and how
 *  it does what the rules ask of it. What it leaves out stays its own. */
export interface WaitingQueueOptions<T, H = undefined> extends Omit<QueueView<T>, 'travels' | 'marksSeen'> {
  /** Shows a row's session in place of the one shown. `how` is whatever the window's own caller
   *  said about the step (the main window: whether the next one's box takes the cursor). */
  show: (row: T, how?: H) => void;
  /** Goes back or forward through the sessions looked at here, and says whether it went. Without
   *  it ⌃- and ⌃⇧- are the window's own (`QueueView.travels`). */
  travel?: (dir: 'back' | 'forward') => boolean;
  /** Marks a session seen, where ⌘E is the window's to run (`QueueView.marksSeen`). */
  markSeen?: (key: string) => void;
}

export interface WaitingQueue<H = undefined, T = unknown> {
  /** What the strip over the header goes by: what's left after the session shown, the next, and
   *  whether it waits itself. Undefined with no session shown. */
  queue: Queue | undefined;
  /** How the last one went, on the session gone on to, or what the window said (`say`). */
  notice: Notice | null;
  /** With nothing shown, the first that waits on you and hasn't been dealt with here (queue.ts
   *  `firstLeft`): what the waiting window goes to. Null while something shows. */
  first: T | null;
  /** Runs a key's action on the session shown (queue.ts `keyStep`). True when the queue took it;
   *  false when it's the window's own or the card's, or nothing shows. */
  run: (action: QueueAction, how?: H) => boolean;
  /** `run` for a key event's action (queue.ts `queueKey`); false for a key that isn't one. The
   *  window's own listener calls it, after the keys that are its own, and keeps the event from
   *  going further when it answers true. */
  onKey: (e: KeyboardEvent, how?: H) => boolean;
  /** ⌘⏎ sent the session `from` a message, allowed its call or answered its questions (queue.ts
   *  `answered`). */
  answered: (from: string, done: string, how?: H) => Went;
  /** The session `from` left what waits on you: marked seen, snoozed, closed or archived
   *  (queue.ts `left`). */
  left: (from: string, done: Left, how?: H) => Went;
  /** Not dealt with after all (its message didn't go): it comes round again. */
  again: (key: string) => void;
  /** Says something on the strip of the session shown, or of the one `on` that the window is
   *  about to show, in place of how the last one went; null takes back what it says. */
  say: (notice: Notice | null, on?: string) => void;
  /** Starts afresh, as a window opening on a session does: nothing dealt with, nothing said. */
  fresh: () => void;
}

export function useWaitingQueue<T, H = undefined>(options: WaitingQueueOptions<T, H>): WaitingQueue<H, T> {
  const handled = useRef<Handled>(new Map());
  // What the strip says, and the session it says it on: how one went is for the one gone on to,
  // not for whichever shows after a click elsewhere.
  const [note, setNote] = useState<(Notice & { on: string }) | null>(null);
  // The window as of its last render, for a key pressed or an answer that comes in later.
  const view: QueueView<T> = { ...options, travels: !!options.travel, marksSeen: !!options.markSeen };
  const now = useRef({ options, view });
  now.current = { options, view };

  useEffect(() => {
    if (!note || note.error) return;
    const t = window.setTimeout(() => setNote(null), NOTICE_MS);
    return () => window.clearTimeout(t);
  }, [note]);

  /** Does what a step came to: what's dealt with is kept, and the row gone on to shows, the strip
   *  saying on it how the last one went. */
  const take = useCallback((step: Step<T>, how?: H): Went => {
    const { options: o } = now.current;
    handled.current = step.handled;
    if (step.to !== undefined) {
      setNote(step.words ? { on: o.reads.keyOf(step.to), text: step.words, error: false } : null);
      o.show(step.to, how);
    }
    return step.went;
  }, []);

  const answered = useCallback((from: string, done: string, how?: H) => take(rules.answered(now.current.view, handled.current, from, done, Date.now()), how), [take]);
  const left = useCallback((from: string, done: Left, how?: H) => take(rules.left(now.current.view, handled.current, from, done, Date.now()), how), [take]);

  const run = useCallback(
    (action: QueueAction, how?: H): boolean => {
      const { options: o, view } = now.current;
      const step = rules.keyStep(view, handled.current, action, Date.now());
      if (step.markSeen) o.markSeen?.(step.markSeen);
      take(step, how);
      // The window's history shows the session itself; the strip has nothing to say on it.
      if (step.travel && o.travel?.(step.travel)) setNote(null);
      return step.taken;
    },
    [take],
  );

  const onKey = useCallback(
    (e: KeyboardEvent, how?: H) => {
      const action = rules.queueKey(e);
      return action !== null && run(action, how);
    },
    [run],
  );

  const again = useCallback((key: string) => void handled.current.delete(key), []);
  const say = useCallback((notice: Notice | null, onKey?: string) => {
    const on = onKey ?? now.current.view.shown?.key;
    setNote(notice && on ? { ...notice, on } : null);
  }, []);
  const fresh = useCallback(() => {
    handled.current = new Map();
    setNote(null);
  }, []);
  const next = useCallback(() => void run('next'), [run]);

  const stands = rules.standing(view, handled.current);
  const after = stands?.left[0];
  const queue: Queue | undefined = stands ? { left: stands.left.length, next: after !== undefined ? { title: view.reads.titleOf(after) } : null, onNext: next, waiting: stands.waiting } : undefined;
  const notice = note && note.on === view.shown?.key ? { text: note.text, error: note.error } : null;
  const first = view.shown ? null : (rules.firstLeft(view, handled.current) ?? null);

  return { queue, notice, first, run, onKey, answered, left, again, say, fresh };
}

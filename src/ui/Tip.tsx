// A tip on using Codealong (tips.ts), under what a window says it's reading while it has nothing
// else to show: the board before the engine's first scan, the PRs view before GitHub's PRs, a
// conversation, a PR's description. At launch it shows with the line over it; a conversation or a
// description, which often come at once, gets one only once its wait has gone on a moment, so
// switching between them doesn't flash one. While the wait goes on another comes round every so
// often, and Next tip brings one at once. Which tips have shown is kept for every window together, so the next wait, here or in
// another window, starts on one not seen yet. A window that can't be seen shows none, so none counts as seen.
// The shortcuts that work in every app are asked of the engine as the wait starts, so a tip names
// them only while they're on and work (a clash with another app's leaves them off). A working
// session's conversation shows one now and then too, its own way (Working.tsx).

import { Fragment, useCallback, useEffect, useRef, useState } from 'react';
import { quickShortcut } from '../api';
import { useLatest } from '../latest';
import { chordKeys } from '../menus';
import { readStored, strings, writeStored } from '../stored';
import { nextTip, shortcutCaps, shownNow, tipShows, TIPS, type Tip as TipData, type TipPlace, type TipShortcuts } from '../tips';
import { MarkLive } from './MarkLive';
import { cx, Icon, Kbd } from './primitives';

/** The tips that have shown, the latest last (tips.ts `shownNow`). */
const SHOWN_KEY = 'codealong.tips.shown';
/** Where a tip shows while a window reads; a working conversation's comes its own way (Working.tsx). */
type ReadingPlace = Exclude<TipPlace, 'working'>;
/** How long a wait goes on before it gets a tip: none at launch, where it's always seconds. */
const TIP_AFTER_MS: Record<ReadingPlace, number> = { board: 0, waiting: 0, quick: 0, session: 700, pr: 700 };
/** How long a tip shows before the next, while the wait goes on: time to read two sentences. */
const TIP_FOR_MS = 12_000;
const NO_SHORTCUTS: TipShortcuts = {};

/** The shortcuts that work in every app, asked of the engine; null until it has said them. A tip
 *  waits for them (a few milliseconds), or it would pass over the tips that name them. */
export function useTipShortcuts(): TipShortcuts | null {
  const [shortcuts, setShortcuts] = useState<TipShortcuts | null>(null);
  useEffect(() => {
    let on = true;
    const ask = (which: keyof TipShortcuts) =>
      quickShortcut(which).then(
        (s) => (s.error ? [] : [[which, s.shortcut] as const]),
        // Unasked, the tips that name it don't show: the rest still do.
        () => [],
      );
    void Promise.all([ask('quick'), ask('chat'), ask('waiting'), ask('improve'), ask('main')]).then((found) => on && setShortcuts(Object.fromEntries(found.flat())));
    return () => {
      on = false;
    };
  }, []);
  return shortcuts;
}

/** The next tip for `place`, counted as shown in every window. */
export function takeTip(place: TipPlace, shortcuts: TipShortcuts): TipData | null {
  const shown = readStored(SHOWN_KEY, strings, []);
  const next = nextTip(place, shown, shortcuts);
  if (next) writeStored(SHOWN_KEY, strings, shownNow(shown, next.id));
  return next;
}

/** A tip's words, its keys as caps. */
export function TipWords({ tip, shortcuts }: { tip: TipData; shortcuts: TipShortcuts }) {
  return tip.says.map((p, i) =>
    typeof p === 'string' ? (
      <Fragment key={i}>{p}</Fragment>
    ) : 'chord' in p ? (
      <Kbd key={i} keys={chordKeys(p.chord)} />
    ) : 'shortcut' in p ? (
      <Kbd key={i} keys={shortcutCaps(shortcuts[p.shortcut])} />
    ) : (
      <code key={i} className="ca-ic">
        {p.code}
      </code>
    ),
  );
}

export function Tip({ place, className }: { place: ReadingPlace; className?: string }) {
  const [tip, setTip] = useState<TipData | null>(null);
  const shortcuts = useTipShortcuts();
  const known = shortcuts ?? NO_SHORTCUTS;
  const cuts = useLatest(known);
  const asked = shortcuts !== null;
  const turn = useRef<number | undefined>(undefined);

  const show = useCallback(() => {
    if (document.hidden) return;
    setTip(takeTip(place, cuts.current));
  }, [place, cuts]);

  // The first once it's due, then one every so often; Next tip starts the count again. A window
  // that was hidden when its first was due (the quick panel's page is made hidden) shows one as
  // it comes up.
  const keepTurning = useCallback(() => {
    window.clearInterval(turn.current);
    turn.current = window.setInterval(show, TIP_FOR_MS);
  }, [show]);
  const due = useRef(false);
  const shownOne = useLatest(tip !== null);
  useEffect(() => {
    if (!asked) return;
    const first = window.setTimeout(() => {
      due.current = true;
      show();
      keepTurning();
    }, TIP_AFTER_MS[place]);
    const onVisible = () => {
      if (due.current && !shownOne.current && !document.hidden) show();
    };
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(turn.current);
      document.removeEventListener('visibilitychange', onVisible);
    };
  }, [asked, place, show, keepTurning, shownOne]);

  if (!tip) return null;
  const more = TIPS.filter((t) => t.places.includes(place) && tipShows(t, known)).length > 1;
  return (
    <div className={cx('ca-tip', className)} role="note" aria-label="Tip" data-tip={tip.id}>
      <div className="ca-tip-head">
        <span className="ca-label">Tip</span>
        {more && (
          <button
            type="button"
            className="ca-linkbtn ca-tip-next"
            onClick={() => {
              show();
              keepTurning();
            }}
          >
            Next tip
            <Icon name="chevron-right" size={12} />
          </button>
        )}
      </div>
      <p key={tip.id} className="ca-tip-text">
        <TipWords tip={tip} shortcuts={known} />
      </p>
    </div>
  );
}

/**
 * What a window or a pane shows while everything it would show is still being read: the mark at
 * work and what's being read, with the tip under them, in the middle of the room it has (Tip.css
 * sets the tip larger there, its Next tip under it). The main window's board, which takes the
 * panel's room too while it reads, and its PRs view while GitHub's PRs come (App.tsx), the
 * waiting window before the board comes, and a conversation that takes a moment, which `late`
 * keeps from flashing in for a read that doesn't.
 */
export function Reading({ what, place, late = false, className }: { what: string; place: ReadingPlace; late?: boolean; className?: string }) {
  return (
    <div className={cx('ca-reading', late && 'is-late', className)}>
      <MarkLive size={36} status="working" moment={null} />
      <p className="ca-reading-what" role="status">
        {what}
      </p>
      <Tip place={place} />
    </div>
  );
}

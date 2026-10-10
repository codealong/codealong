// A tip on using Codealong (tips.ts), under what a window says it's reading while it has nothing
// else to show: the board before the engine's first scan, a conversation, a PR's description. It
// shows only once the wait has gone on a moment, so a quick read doesn't flash one; while the
// wait goes on, another comes round every so often, and Next tip brings one at once. Which tips
// have shown is kept for every window together, so the next wait, here or in another window,
// starts on one not seen yet. A window that can't be seen shows none, so none counts as seen.
// The shortcuts that work in every app are asked of the engine as the wait starts, so a tip names
// them only while they're on and work (a clash with another app's leaves them off).

import { Fragment, useCallback, useEffect, useRef, useState } from 'react';
import { quickShortcut } from '../api';
import { useLatest } from '../latest';
import { chordKeys } from '../menus';
import { readStored, strings, writeStored } from '../stored';
import { nextTip, shortcutCaps, shownNow, tipShows, TIPS, type Tip as TipData, type TipPlace, type TipShortcuts } from '../tips';
import { cx, Icon, Kbd } from './primitives';

/** The tips that have shown, the latest last (tips.ts `shownNow`). */
const SHOWN_KEY = 'codealong.tips.shown';
/** How long a wait goes on before it gets a tip. */
export const TIP_AFTER_MS = 700;
/** How long a tip shows before the next, while the wait goes on: time to read two sentences. */
const TIP_FOR_MS = 12_000;

export function Tip({ place, className }: { place: TipPlace; className?: string }) {
  const [tip, setTip] = useState<TipData | null>(null);
  const [shortcuts, setShortcuts] = useState<TipShortcuts>({});
  const cuts = useLatest(shortcuts);
  useEffect(() => {
    let on = true;
    for (const which of ['quick', 'waiting'] as const) {
      quickShortcut(which).then(
        (s) => on && !s.error && setShortcuts((was) => ({ ...was, [which]: s.shortcut })),
        // Unasked, the tips that name it don't show: the rest still do.
        () => {},
      );
    }
    return () => {
      on = false;
    };
  }, []);
  const turn = useRef<number | undefined>(undefined);

  const show = useCallback(() => {
    if (document.hidden) return;
    const shown = readStored(SHOWN_KEY, strings, []);
    const next = nextTip(place, shown, cuts.current);
    setTip(next);
    if (next) writeStored(SHOWN_KEY, strings, shownNow(shown, next.id));
  }, [place, cuts]);

  // The first after a moment, then one every so often; Next tip starts the count again. A window
  // that was hidden when its first was due (the quick panel's page is made hidden) shows one as
  // it comes up.
  const keepTurning = useCallback(() => {
    window.clearInterval(turn.current);
    turn.current = window.setInterval(show, TIP_FOR_MS);
  }, [show]);
  const due = useRef(false);
  const shownOne = useLatest(tip !== null);
  useEffect(() => {
    const first = window.setTimeout(() => {
      due.current = true;
      show();
      keepTurning();
    }, TIP_AFTER_MS);
    const onVisible = () => {
      if (due.current && !shownOne.current && !document.hidden) show();
    };
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(turn.current);
      document.removeEventListener('visibilitychange', onVisible);
    };
  }, [show, keepTurning, shownOne]);

  if (!tip) return null;
  const more = TIPS.filter((t) => t.places.includes(place) && tipShows(t, shortcuts)).length > 1;
  return (
    <div className={cx('ca-tip', className)} role="note" aria-label="Tip" data-tip={tip.id}>
      <div className="ca-tip-head">
        <Icon name="bulb" size={13} />
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
          </button>
        )}
      </div>
      <p key={tip.id} className="ca-tip-text">
        {tip.says.map((p, i) =>
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
        )}
      </p>
    </div>
  );
}

// The line at the end of a conversation while its agent works (working.ts says what it says, and
// when a tip comes): the mark breathing, a word for what the agent's up to and how long the turn
// has gone on, quiet, where its words will come. Under the word, now and then, a tip; only in the
// main window's panel, since a card is read at a glance.

import { useEffect, useLayoutEffect, useState } from 'react';
import { duration } from '../format';
import { readStored, writeStored, type Codec } from '../stored';
import { TIPS, tipShows, type Tip } from '../tips';
import type { Session } from '../types';
import { turnOf, workingTip, workingWord, type TipClaim } from '../working';
import { MarkLive } from './MarkLive';
import { takeTip, TipWords, useTipShortcuts } from './Tip';

/** The last tip a turn got, for every window (working.ts `workingTip`). */
const CLAIM_KEY = 'codealong.tips.working';
const CLAIM: Codec<TipClaim> = {
  parse: (raw) => {
    const v: unknown = JSON.parse(raw);
    if (!v || typeof v !== 'object') return undefined;
    const { turn, id, at } = v as Record<string, unknown>;
    return typeof turn === 'string' && typeof id === 'string' && typeof at === 'number' ? { turn, id, at } : undefined;
  },
  format: (claim) => JSON.stringify(claim),
};
/** How often the line looks at the clock: on the clock's own beat, so windows change word together. */
const TICK_MS = 10_000;

/** The time, again at each multiple of `everyMs` on the clock. */
function useClock(everyMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    let id = 0;
    const next = () => {
      id = window.setTimeout(
        () => {
          setNow(Date.now());
          next();
        },
        everyMs - (Date.now() % everyMs) + 20,
      );
    };
    next();
    return () => window.clearTimeout(id);
  }, [everyMs]);
  return now;
}

export function Working({ session: s, tips, onGrow }: { session: Session; tips: boolean; onGrow: () => void }) {
  const now = useClock(TICK_MS);
  const since = s.statusSince ?? s.lastPromptAt;
  const word = workingWord(s.key, since, now);
  const turn = turnOf(s.key, s.lastPromptAt);
  // Read again at each beat: another window may have given this turn its tip. A window that can't
  // be seen takes none, so none counts as seen.
  const last = tips ? readStored<TipClaim, null>(CLAIM_KEY, CLAIM, null) : null;
  const due = tips ? workingTip(last, turn, now - since, now) : null;
  useLayoutEffect(onGrow, [onGrow]);
  return (
    <div className="ca-working">
      <MarkLive size={14} status="working" moment={null} />
      <span className="ca-working-line">
        <span key={word} className="ca-working-word">
          {word}…
        </span>
        <span className="ca-working-for">{duration(since, now)}</span>
      </span>
      {(due === 'keep' || (due === 'take' && !document.hidden)) && <WorkingTip kept={due === 'keep' ? last : null} turn={turn} onGrow={onGrow} />}
    </div>
  );
}

/** The turn's tip: the one it got, or a new one once the shortcuts are known. */
function WorkingTip({ kept, turn, onGrow }: { kept: TipClaim | null; turn: string; onGrow: () => void }) {
  const shortcuts = useTipShortcuts();
  const [taken, setTaken] = useState<Tip | null>(null);
  useEffect(() => {
    if (kept || taken || !shortcuts) return;
    const tip = takeTip('working', shortcuts);
    if (!tip) return;
    writeStored(CLAIM_KEY, CLAIM, { turn, id: tip.id, at: Date.now() });
    setTaken(tip);
  }, [kept, taken, shortcuts, turn]);
  // Two windows that took one at once show the one kept, once they read it.
  const tip = kept ? TIPS.find((t) => t.id === kept.id) : taken;
  const shows = !!tip && !!shortcuts && tipShows(tip, shortcuts);
  useLayoutEffect(() => {
    if (shows) onGrow();
  }, [shows, tip, onGrow]);
  if (!shows) return null;
  return (
    <p key={tip.id} className="ca-working-tip" role="note" aria-label="Tip" data-tip={tip.id}>
      <span className="ca-label">Tip</span> <TipWords tip={tip} shortcuts={shortcuts} />
    </p>
  );
}

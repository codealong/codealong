// The line at the end of a conversation while its agent works (working.ts says what it says, and
// when a tip comes): the mark, quiet (gray, its color gliding round it), a word for what the agent's up to and how long it has
// worked on what you last asked, quiet, where its words will come. Now and then a tip, on the
// line's right, out of the way of the agent's words; only in the main window's panel, since a
// card is read at a glance. A session whose turn is over that only waits on a command it left
// running in the background has another line in its place (`Waiting`): its clock and the command.

import { useEffect, useLayoutEffect, useState } from 'react';
import { duration } from '../format';
import { workingSince } from '../lanes';
import { readStored, writeStored, type Codec } from '../stored';
import { TIPS, tipShows } from '../tips';
import type { Session } from '../types';
import { claimed, turnOf, workingTip, workingWord, type TipClaim } from '../working';
import { MarkLive } from './MarkLive';
import { SessionGlyph } from './primitives';
import { takeTip, TipWords, useTipShortcuts } from './Tip';

/** The tips turns got, the latest last, for every window (working.ts `workingTip`). */
const CLAIMS_KEY = 'codealong.tips.working';
/** As JSON; a claim that doesn't read is left out. */
const CLAIMS: Codec<TipClaim[]> = {
  parse: (raw) => {
    const v: unknown = JSON.parse(raw);
    if (!Array.isArray(v)) return undefined;
    return v.flatMap((c: unknown) => {
      if (!c || typeof c !== 'object') return [];
      const { turn, id, at } = c as Record<string, unknown>;
      return typeof turn === 'string' && typeof id === 'string' && typeof at === 'number' ? [{ turn, id, at }] : [];
    });
  },
  format: (claims) => JSON.stringify(claims),
};
const readClaims = () => readStored<TipClaim[]>(CLAIMS_KEY, CLAIMS, []);
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
  const since = workingSince(s) ?? s.lastPromptAt;
  const word = workingWord(s.key, since, now);
  const turn = turnOf(s.key, s.lastPromptAt);
  // Read again at each beat: another window may have given this turn its tip. A window that can't
  // be seen takes none, so none counts as seen.
  const due = tips ? workingTip(readClaims(), turn, now - since, now) : null;
  // A longer word can leave the tip no room beside it, and put it under the word.
  useLayoutEffect(onGrow, [onGrow, word]);
  return (
    <div className="ca-working">
      <span className="ca-working-head">
        <MarkLive size={14} status="working" moment={null} quiet />
        <span className="ca-working-line">
          <span key={word} className="ca-working-word">
            {word}…
          </span>
          <span className="ca-working-for">{duration(since, now)}</span>
        </span>
      </span>
      {due && (due !== 'take' || !document.hidden) && <WorkingTip kept={due === 'take' ? null : due.keep} turn={turn} since={since} onGrow={onGrow} />}
    </div>
  );
}

/** In the working line's place, for a session whose turn is over that waits on a command it left
 *  running in the background (lanes.ts `waitsInBackground`): the clock the list and its header
 *  draw, and what it waits on, in the engine's words for its excerpt; the command on one line, all
 *  of it on hover. */
export function Waiting({ session: s, onGrow }: { session: Session; onGrow: () => void }) {
  useLayoutEffect(onGrow, [onGrow]);
  // "" when the engine couldn't read the command. Shown on one line; on hover as it was written.
  const command = s.background?.replace(/\s+/g, ' ').trim();
  return (
    <div className="ca-working is-waiting">
      <span className="ca-working-head">
        <SessionGlyph s={s} size={14} />
        <span className="ca-working-line">
          {command ? (
            <>
              <span className="ca-working-word is-lead">Waiting on</span>
              <code className="ca-ic" title={s.background}>
                {command}
              </code>
            </>
          ) : (
            <span className="ca-working-word">Waiting on a command in the background</span>
          )}
        </span>
      </span>
    </div>
  );
}

/** The turn's tip: the one it got, or a new one once the shortcuts are known. */
function WorkingTip({ kept, turn, since, onGrow }: { kept: TipClaim | null; turn: string; since: number; onGrow: () => void }) {
  const shortcuts = useTipShortcuts();
  const [taken, setTaken] = useState<TipClaim | null>(null);
  useEffect(() => {
    if (kept || taken || !shortcuts) return;
    // Asked again: another window may have given one, to this turn or another, while this one
    // waited on the shortcuts.
    const claims = readClaims();
    const now = Date.now();
    const due = workingTip(claims, turn, now - since, now);
    if (due === null) return;
    if (due !== 'take') {
      setTaken(due.keep);
      return;
    }
    const tip = takeTip('working', shortcuts);
    if (!tip) return;
    const claim = { turn, id: tip.id, at: now };
    writeStored(CLAIMS_KEY, CLAIMS, claimed(claims, claim));
    setTaken(claim);
  }, [kept, taken, shortcuts, turn, since]);
  // Two windows that took one at once show the one kept, once they read it.
  const id = (kept ?? taken)?.id;
  const tip = id ? TIPS.find((t) => t.id === id) : undefined;
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

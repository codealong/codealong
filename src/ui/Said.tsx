// What the bar shows of voice (said.ts): a key held to talk, and a listening session's cue.
// Docked in the notch it's a card under the pills, by the camera, where a glance at it in a
// meeting looks like looking at the people in it; floating, a line in the bar.

import { useEffect, useRef, useState } from 'react';
import { onTalk } from '../api';
import { clock } from '../listening';
import { latestTalk, saidNow, saidUntil, talkSays, type Said } from '../said';
import type { BarListening, Talk } from '../types';
import { cx, Icon } from './primitives';

const NONE: readonly BarListening[] = [];

/** What the bar shows of voice now, kept up with the key held to talk and the feed's listenings.
 *  `dismiss` puts a cue away before its time. */
export function useSaid(listenings: readonly BarListening[] | undefined): { said: Said | null; dismiss: () => void } {
  const [talk, setTalk] = useState<Talk | null>(null);
  const talkedAt = useRef(0);
  const [dismissed, setDismissed] = useState(0);
  // Drawn again when what shows runs out.
  const [, setTick] = useState(0);
  useEffect(
    () =>
      onTalk((t) =>
        setTalk((shown) => {
          const next = latestTalk(shown, t);
          if (next === t) talkedAt.current = Date.now();
          return next;
        }),
      ),
    [],
  );
  const said = saidNow(talk, talkedAt.current, listenings ?? NONE, dismissed, Date.now());
  const until = saidUntil(said, talkedAt.current);
  useEffect(() => {
    if (until === null) return;
    const timer = window.setTimeout(() => setTick((n) => n + 1), Math.max(0, until - Date.now()) + 20);
    return () => window.clearTimeout(timer);
  }, [until]);
  const cueAt = said?.kind === 'cue' ? said.listening.cueAt : 0;
  return { said, dismiss: () => setDismissed(cueAt) };
}

/** How loud the microphone is, as three bars. */
function Level({ level }: { level: number }) {
  // Speech sits low on the scale: a square root spreads it over the bars.
  const loud = Math.min(1, Math.sqrt(level) * 1.6);
  return (
    <span className="ca-spoken-level" aria-hidden="true">
      {[0.15, 0.4, 0.7].map((at) => (
        <span key={at} className={cx('ca-spoken-bar', loud >= at && 'is-on')} />
      ))}
    </span>
  );
}

/** The card under the notch's pills. */
export function SaidCard({ said, now, onDismiss }: { said: Said; now: number; onDismiss: () => void }) {
  if (said.kind === 'cue') {
    const l = said.listening;
    return (
      <button
        type="button"
        className="ca-spoken is-cue"
        title="Put it away"
        onClick={(e) => {
          // Not a click on the notch too, which would open the session that waits.
          e.stopPropagation();
          onDismiss();
        }}
      >
        <span className="ca-spoken-head">
          <span className="ca-live-dot" aria-hidden="true" />
          <span className="ca-spoken-title">{l.title}</span>
          <span className="ca-spoken-meta">{clock(now - l.startedAt)}</span>
        </span>
        <span className="ca-spoken-cue">{l.cue}</span>
      </button>
    );
  }
  const t = said.talk;
  const going = t.phase === 'listening';
  return (
    <div className={cx('ca-spoken', `is-${t.phase}`)} role="status">
      <span className="ca-spoken-head">
        {going ? <Level level={t.level} /> : <Icon name={t.phase === 'failed' || t.phase === 'nothing' ? 'microphone' : 'check'} size={13} stroke={2} />}
        <span className="ca-spoken-title">{talkSays(t)}</span>
      </span>
      {t.text && <span className="ca-spoken-text">{t.text}</span>}
      {t.message && <span className="ca-spoken-message">{t.message}</span>}
    </div>
  );
}

/** How much of what's said the floating bar's line holds. */
const LINE = 44;

/** The same, as a line in the floating bar. */
export function SaidLine({ said }: { said: Said }) {
  const all = (said.kind === 'cue' ? said.listening.cue : said.talk.text || said.talk.message) ?? '';
  // The end of what's heard is what's new: it's what stays when there's too much for a line.
  const words = all.length > LINE ? `…${all.slice(-LINE)}` : all;
  return (
    <span className={cx('ca-hud-spoken', said.kind === 'talk' && `is-${said.talk.phase}`)} role="status">
      <span className="ca-live-dot" aria-hidden="true" />
      <span className="ca-hud-spoken-title">{said.kind === 'cue' ? said.listening.title : talkSays(said.talk)}</span>
      {words && <span className="ca-hud-spoken-text">{words}</span>}
    </span>
  );
}

/** That a session is hearing now, beside the notch's mark for as long as one is: nothing is ever
 *  heard without it showing. */
export function HearingMark({ listenings }: { listenings: readonly BarListening[] | undefined }) {
  if (!listenings?.length) return null;
  const troubled = listenings.some((l) => l.problem);
  const title = listenings.map((l) => `${l.title} is listening${l.problem ? `: ${l.problem}` : ''}`).join('\n');
  return <span className={cx('ca-hearing-mark', troubled && 'is-troubled')} title={title} role="img" aria-label={title} />;
}

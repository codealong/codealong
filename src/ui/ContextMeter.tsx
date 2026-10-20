// How full a session's context window is, in its reply box's foot before its account's chip, as
// Claude Code and Codex show theirs: a small pie and its share of the window (or its tokens, where
// the window isn't known). Hovering it, or giving it the keyboard, shows a card with the numbers
// and what the session has cost so far, from the token ledger (ledger.rs). The figure comes with
// the session (`Session.context`), as its harness counted its latest call; another device's
// sessions show it too, and their cost is that device's to say.

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { sessionTokens } from '../api';
import { shortOf } from '../devices';
import { cacheShare, contextFigure, contextLevel, contextShare, contextWords, money, tokenCount, totalTokens } from '../spend';
import type { Session, SessionSpent } from '../types';
import { cx } from './primitives';

/** A pie of `share` (0 to 1), drawn as a wedge from the top, clockwise, on a ring. */
function Pie({ share }: { share: number | null }) {
  const r = 6;
  const c = 7;
  const s = share ?? 0;
  const end = s * 2 * Math.PI;
  const x = c + r * Math.sin(end);
  const y = c - r * Math.cos(end);
  const wedge = s >= 1 ? null : `M${c},${c} L${c},${c - r} A${r},${r} 0 ${s > 0.5 ? 1 : 0} 1 ${x.toFixed(2)},${y.toFixed(2)} Z`;
  return (
    <svg className="ca-context-pie" viewBox="0 0 14 14" width="14" height="14" aria-hidden="true">
      <circle cx={c} cy={c} r={r} className="ca-context-ring" />
      {s >= 1 ? <circle cx={c} cy={c} r={r} className="ca-context-fill" /> : s > 0 && wedge && <path d={wedge} className="ca-context-fill" />}
    </svg>
  );
}

export function ContextMeter({ session: s }: { session: Session }) {
  // The meter the card is open over, while it is.
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const c = s.context;
  if (!c) return null;
  const share = contextShare(c);
  const level = contextLevel(share);
  return (
    <>
      <button
        type="button"
        className={cx('ca-context', `is-${level}`)}
        aria-label={`Context: ${contextWords(c)}${share !== null ? `, ${Math.round(share * 100)}% of its window` : ''}`}
        onMouseEnter={(e) => setAnchor(e.currentTarget)}
        onMouseLeave={() => setAnchor(null)}
        onFocus={(e) => setAnchor(e.currentTarget)}
        onBlur={() => setAnchor(null)}
      >
        <Pie share={share} />
        <span className="ca-context-figure">{contextFigure(c)}</span>
      </button>
      {anchor && <ContextCard session={s} anchor={anchor} />}
    </>
  );
}

function ContextCard({ session: s, anchor }: { session: Session; anchor: HTMLElement }) {
  const ref = useRef<HTMLDivElement>(null);
  const [spent, setSpent] = useState<SessionSpent | null>(null);
  const [failed, setFailed] = useState(false);
  const here = !shortOf(s.key);
  // Asked as it opens: what the session has cost moves as it works.
  useEffect(() => {
    if (!here) return;
    let live = true;
    sessionTokens(s.key)
      .then((r) => live && setSpent(r))
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [s.key, here]);
  // Above the meter, which sits at the foot of its window toward its right, ending at its right
  // edge, kept inside the window.
  useLayoutEffect(() => {
    const card = ref.current;
    if (!card) return;
    const r = anchor.getBoundingClientRect();
    card.style.left = `${Math.max(8, Math.min(r.right - card.offsetWidth, window.innerWidth - card.offsetWidth - 8))}px`;
    card.style.top = `${Math.max(8, r.top - card.offsetHeight - 6)}px`;
  });
  const c = s.context;
  if (!c) return null;
  const share = contextShare(c);
  const t = spent?.total;
  return createPortal(
    <div ref={ref} className="ca-context-card" role="tooltip">
      <div className="ca-context-card-head">
        <span>Context</span>
        {share !== null && <span className="ca-context-card-share">{Math.round(share * 100)}%</span>}
      </div>
      <div className={cx('ca-context-track', `is-${contextLevel(share)}`)}>
        <span style={{ width: `${(share ?? 0) * 100}%` }} />
      </div>
      <div className="ca-context-card-line">{contextWords(c)}</div>
      {s.model && <div className="ca-context-card-note">{s.model}</div>}
      {here && (
        <dl className="ca-context-card-cost">
          {t && t.calls > 0 ? (
            <>
              <dt>Cost so far</dt>
              <dd>
                {money(t.api)} at API prices{t.paid > 0 ? ` · ${money(t.paid)} paid` : ''}
                {t.unpriced > 0 ? ' · some unpriced' : ''}
              </dd>
              <dt>Tokens</dt>
              <dd>
                {tokenCount(totalTokens(t.tokens))} · {Math.round(cacheShare(t.tokens) * 100)}% read from cache
              </dd>
              <dt>Calls</dt>
              <dd>{t.calls.toLocaleString('en-US')}</dd>
            </>
          ) : (
            <dd className="ca-context-card-note">{failed ? 'What it cost couldn’t be read.' : !spent ? 'Reading what it cost…' : 'Nothing recorded yet: the ledger reads it within a minute.'}</dd>
          )}
        </dl>
      )}
    </div>,
    document.body,
  );
}

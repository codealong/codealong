// A session's side conversation (side.ts), above the box that asks it with `/side`: each
// question and its answer as the throwaway copy writes it, the tool it's looking with meanwhile,
// and Close, which ends it. Nothing here reaches the session, and nothing of it is kept.

import { useLayoutEffect, useRef } from 'react';
import { sideClose } from '../api';
import { noteSide, turnState, useSide } from '../side';
import type { Session } from '../types';
import { Markdown } from './Markdown';
import { Icon, Rich } from './primitives';

export function SideStrip({ session }: { session: Session }) {
  const side = useSide(session.key);
  const scroller = useRef<HTMLDivElement>(null);
  // The end stays in view as the answer grows, unless you've scrolled back to read.
  const atEnd = useRef(true);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && atEnd.current) el.scrollTop = el.scrollHeight;
  }, [side]);
  if (!side) return null;
  const current = side.turns.findIndex((t) => !t.done);
  const close = () => {
    noteSide({ key: side.key, turns: [], running: false, ended: true });
    void sideClose(side.key).catch(() => {});
  };
  return (
    <section className="ca-side" aria-label="Side conversation">
      <header className="ca-side-head">
        <Icon name="help" size={14} />
        <span className="ca-side-title">On the side</span>
        <span className="ca-side-about">{side.ended ? 'Its copy has ended. Another question starts a new one.' : 'A throwaway copy answers. The session never sees this.'}</span>
        <button type="button" className="ca-linkbtn" title="End the side conversation. Nothing of it is kept." onClick={close}>
          Close
        </button>
      </header>
      <div
        className="ca-side-turns"
        ref={scroller}
        onScroll={(e) => {
          const el = e.currentTarget;
          atEnd.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
      >
        {side.turns.map((t, i) => {
          const state = turnState(t, i === current);
          return (
            <div key={i} className="ca-side-turn">
              <div className="ca-side-q">{t.question}</div>
              {t.answer && <Markdown text={t.answer} className="ca-side-a" />}
              {t.tool && (
                <div className="ca-side-doing" role="status">
                  <Rich text={t.tool} now={0} />
                </div>
              )}
              {state && (
                <div className="ca-side-doing" role="status">
                  {state}
                </div>
              )}
              {t.error && <div className="ca-side-error">{t.error}</div>}
            </div>
          );
        })}
      </div>
    </section>
  );
}

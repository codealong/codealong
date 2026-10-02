// A session's side conversations (side.ts), above the box that asks them with `/side`. The open
// one shows each question and its answer as the throwaway copy writes it, the tool it's using
// meanwhile, and Close, which ends the copy. Codealong keeps what was asked and answered, with
// what the copy did to answer, so `/side` alone (or Earlier, beside an open one) lists the ones
// from before, and picking one shows it to read, or to delete. None of it reaches the session.
// The session's header says how many it has (`SidesChip`), which is the way to them that shows.

import { useLayoutEffect, useRef, type ReactNode } from 'react';
import { sideClose } from '../api';
import { agoPhrase } from '../format';
import { forgetSide, keptLine, keptSummary, noteSide, showSides, turnState, useKept, useSide, useSideView } from '../side';
import type { Session, SideTurn } from '../types';
import { Markdown } from './Markdown';
import { Icon, Rich } from './primitives';

/** The strip's top line: what this is, a few words about it, and what can be done with it. */
function Head({ title, about, children }: { title: string; about: string; children: ReactNode }) {
  return (
    <header className="ca-sidestrip-head">
      <Icon name="help" size={14} />
      <span className="ca-sidestrip-title">{title}</span>
      <span className="ca-sidestrip-about">{about}</span>
      {children}
    </header>
  );
}

/** Questions and their answers. `current` is the turn the copy is on, in an open one. */
function Turns({ turns, current }: { turns: SideTurn[]; current: number }) {
  return (
    <>
      {turns.map((t, i) => {
        const state = turnState(t, i === current);
        const steps = t.tools ?? [];
        return (
          <div key={i} className="ca-sidestrip-turn">
            <div className="ca-sidestrip-q">{t.question}</div>
            {/* What it did to answer: the one it's on while it works, all of them to unfold after. */}
            {t.done && steps.length > 0 && (
              <details className="ca-sidestrip-steps">
                <summary>{steps.length === 1 ? '1 step' : `${steps.length} steps`}</summary>
                {steps.map((line, n) => (
                  <div key={n}>
                    <Rich text={line} now={0} />
                  </div>
                ))}
              </details>
            )}
            {t.answer && <Markdown text={t.answer} className="ca-sidestrip-a" />}
            {t.tool && (
              <div className="ca-sidestrip-doing" role="status">
                <Rich text={t.tool} now={0} />
              </div>
            )}
            {state && (
              <div className="ca-sidestrip-doing" role="status">
                {state}
              </div>
            )}
            {t.error && <div className="ca-sidestrip-error">{t.error}</div>}
          </div>
        );
      })}
    </>
  );
}

export function SideStrip({ session }: { session: Session }) {
  const key = session.key;
  const side = useSide(key);
  const view = useSideView(key);
  const past = useKept(key);
  const earlier = past.filter((k) => k.id !== side?.id);
  const reading = view?.kind === 'past' ? past.find((k) => k.id === view.id) : undefined;
  const scroller = useRef<HTMLDivElement>(null);
  // The end stays in view as the answer grows, unless you've scrolled back to read.
  const atEnd = useRef(true);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && atEnd.current && !view) el.scrollTop = el.scrollHeight;
  }, [side, view]);
  const now = Date.now();
  const many = (n: number, one: string) => (n === 1 ? `1 ${one}` : `${n} ${one}s`);
  const hide = (
    <button type="button" className="ca-linkbtn" onClick={() => showSides(key, null)}>
      Close
    </button>
  );

  if (reading) {
    return (
      <section className="ca-sidestrip" aria-label="Side conversation">
        <Head title="On the side" about={`${agoPhrase(reading.lastAt, now)} · closed. A new question starts a new copy, which knows none of this.`}>
          <button type="button" className="ca-linkbtn" onClick={() => showSides(key, { kind: 'list' })}>
            Earlier
          </button>
          <button
            type="button"
            className="ca-linkbtn"
            title="Forget this side conversation"
            onClick={() => {
              // Back to the others, or away with none left.
              showSides(key, earlier.length > 1 ? { kind: 'list' } : null);
              forgetSide(key, reading.id);
            }}
          >
            Delete
          </button>
          {hide}
        </Head>
        <div className="ca-sidestrip-turns">
          <Turns turns={reading.turns} current={-1} />
        </div>
      </section>
    );
  }

  if (view) {
    return (
      <section className="ca-sidestrip" aria-label="Side conversations">
        <Head title="Side conversations" about="Kept by Codealong. The session never saw them.">
          {hide}
        </Head>
        <div className="ca-sidestrip-turns">
          {earlier.length === 0 && <div className="ca-sidestrip-doing">{side ? 'None before the one that’s open.' : 'None with this session yet. /side, then a question, starts one.'}</div>}
          {earlier.map((k) => (
            <button key={k.id} type="button" className="ca-sidestrip-kept" onClick={() => showSides(key, { kind: 'past', id: k.id })}>
              <span className="ca-sidestrip-kept-q">{keptLine(k)}</span>
              <span className="ca-sidestrip-kept-when">
                {agoPhrase(k.lastAt, now)} · {many(k.turns.length, 'question')}
              </span>
            </button>
          ))}
        </div>
      </section>
    );
  }

  if (!side) return null;
  const close = () => {
    noteSide({ id: side.id, key, turns: [], running: false, ended: true });
    void sideClose(key).catch(() => {});
  };
  return (
    <section className="ca-sidestrip" aria-label="Side conversation">
      <Head title="On the side" about={side.ended ? 'Its copy has ended. Another question starts a new one.' : 'A throwaway copy answers. The session never sees this.'}>
        {earlier.length > 0 && (
          <button type="button" className="ca-linkbtn" title={`${many(earlier.length, 'side conversation')} from before`} onClick={() => showSides(key, { kind: 'list' })}>
            Earlier
          </button>
        )}
        <button type="button" className="ca-linkbtn" title="Ends its copy. What was said is kept: /side shows the earlier ones." onClick={close}>
          Close
        </button>
      </Head>
      <div
        className="ca-sidestrip-turns"
        ref={scroller}
        onScroll={(e) => {
          const el = e.currentTarget;
          atEnd.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
      >
        <Turns turns={side.turns} current={side.turns.findIndex((t) => !t.done)} />
      </div>
    </section>
  );
}

/** In a session's header: how many side conversations it has, kept or open. A click shows them
 *  above its box (the list, or the only one), and another puts them away. */
export function SidesChip({ session }: { session: Session }) {
  const key = session.key;
  const side = useSide(key);
  const view = useSideView(key);
  const summary = keptSummary(useKept(key), side);
  if (!summary) return null;
  const { words, show } = summary;
  const showing = !!view && !!show;
  return (
    <button
      type="button"
      className="ca-sessinsp-sides"
      aria-pressed={showing}
      title={show ? (showing ? 'Put them away' : `${words} with this session, kept by Codealong. Show ${show.kind === 'past' ? 'it' : 'them'} above the box.`) : `${words} with this session: the one that’s open above the box.`}
      onClick={() => showSides(key, showing ? null : show)}
    >
      <Icon name="help" size={12} />
      <span>{words}</span>
    </button>
  );
}

// A session's side conversations (side.ts), above the box that asks them with `/side`. The open
// one shows each question and its answer as the throwaway copy writes it, the tool it's using
// meanwhile, and Close, which ends the copy. Codealong keeps what was asked and answered, with
// what the copy did to answer, so `/side` alone (or Earlier, beside an open one) lists the ones
// from before, and picking one shows it to read, or to delete. None of it reaches the session.
// The session's header says how many it has (`SidesChip`), which is the way to them that shows.
//
// A PR's questions (PrCard.tsx, its box's Ask) are the same strip over the same store, by the
// PR's key: what differs is what it says of them (`SideWords`) and how one is closed.

import { useLayoutEffect, useRef, type ReactNode } from 'react';
import { prSideClose, sideClose } from '../api';
import { agoPhrase } from '../format';
import { forgetSide, keptLine, keptSummary, noteSide, showSides, takesBox, turnState, useKept, useSide, useSideView } from '../side';
import type { PrItem, Session, SideTurn } from '../types';
import { Markdown } from './Markdown';
import { Icon, Rich } from './primitives';

/** What a strip says of its conversations, which are about a session or a PR, and how it ends
 *  the open one. */
export interface SideWords {
  /** Over an open one, or one kept to read. */
  title: string;
  /** Beside the title of an open one, and of one whose process has ended. */
  open: string;
  ended: string;
  /** While the process that answers the first question starts. */
  starting: string;
  /** Over the list of the kept ones, and beside it. */
  listTitle: string;
  listAbout: string;
  /** In the list, with none kept and none open. */
  none: string;
  /** Beside the title of a kept one, after when it was. */
  closed: string;
  /** Close's tooltip, on an open one. */
  closeTitle: string;
  close: (key: string) => Promise<void>;
}

export const SESSION_SIDES: SideWords = {
  title: 'On the side',
  open: 'A throwaway copy answers. The session never sees this.',
  ended: 'Its copy has ended. Another question starts a new one.',
  starting: 'Starting a copy of the conversation…',
  listTitle: 'Side conversations',
  listAbout: 'Kept by Codealong. The session never saw them.',
  none: 'None with this session yet. /side, then a question, starts one.',
  closed: 'closed. A new question starts a new copy, which knows none of this.',
  closeTitle: 'Ends its copy. What was said is kept: /side shows the earlier ones.',
  close: sideClose,
};

/** What a PR's strip says, its conversations answered by `account`'s agent. */
export function prSides(pr: Pick<PrItem, 'number'>, account: string | null): SideWords {
  return {
    title: `About #${pr.number}`,
    open: `${account ?? 'An agent'} reads the PR and its code to answer, and changes nothing. GitHub never sees this.`,
    ended: 'It has ended. Another question starts a new conversation.',
    starting: 'Reading the PR…',
    listTitle: `Questions about #${pr.number}`,
    listAbout: 'Kept by Codealong. GitHub never saw them.',
    none: 'None about this PR yet. Ask, and an agent reads it to answer.',
    closed: 'closed. A new question starts a new conversation, which knows none of this.',
    closeTitle: 'Ends it. What was said is kept: Earlier lists it.',
    close: prSideClose,
  };
}

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

/** The side conversations of `sideKey`: a session's, or a PR's. */
export function SideStrip({ sideKey: key, words }: { sideKey: string; words: SideWords }) {
  const side = useSide(key);
  const shown = useSideView(key);
  // A question starting a copy shows the open one, as nothing else does, once there is one.
  const starting = shown?.kind === 'starting';
  const view = starting ? null : shown;
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
        <Head title={words.title} about={`${agoPhrase(reading.lastAt, now)} · ${words.closed}`}>
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
        <Head title={words.listTitle} about={words.listAbout}>
          {hide}
        </Head>
        <div className="ca-sidestrip-turns">
          {earlier.length === 0 && <div className="ca-sidestrip-doing">{side ? 'None before the one that’s open.' : words.none}</div>}
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

  if (!side) {
    if (!starting || !takesBox(shown, now)) return null;
    // Asked, and the engine is still starting its copy.
    return (
      <section className="ca-sidestrip" aria-label="Side conversation">
        <Head title={words.title} about={words.open}>
          {hide}
        </Head>
        <div className="ca-sidestrip-turns">
          <div className="ca-sidestrip-doing" role="status">
            {words.starting}
          </div>
        </div>
      </section>
    );
  }
  const close = () => {
    noteSide({ id: side.id, key, turns: [], running: false, ended: true });
    // The strip lets it go either way. Closing one here can't fail (side.rs); only another device
    // that doesn't answer can, and its copy goes on there whatever this window says.
    void words.close(key).catch(() => {});
  };
  return (
    <section className="ca-sidestrip" aria-label="Side conversation">
      <Head title={words.title} about={side.ended ? words.ended : words.open}>
        {earlier.length > 0 && (
          <button type="button" className="ca-linkbtn" title={`${many(earlier.length, 'side conversation')} from before`} onClick={() => showSides(key, { kind: 'list' })}>
            Earlier
          </button>
        )}
        <button type="button" className="ca-linkbtn" title={words.closeTitle} onClick={close}>
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
  const showing = !!view && view.kind !== 'starting' && !!show;
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

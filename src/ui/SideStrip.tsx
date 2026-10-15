// A session's side conversations (side.ts), above the box that asks them with `/side`. The open
// one shows each question and its answer as the throwaway copy writes it, the tool it's using
// meanwhile, and Close, which gives the box back to the session: a copy that's answering answers
// what it was asked first and then ends, listed with the ones from before and read as it answers.
// Codealong keeps what was asked and answered, with what the copy did to answer, so `/side` alone
// (or Earlier, beside an open one) lists the ones from before, and picking one shows it to read,
// or to delete. None of it reaches the session.
// The session's header names each under its title, with what came of it (SessionHeader.tsx), and
// a click on one shows it here; its row in Sessions lists the one answering (LaneRow.tsx `SideRow`).
// What can be done with them is a row of icons at the strip's top right, each saying what it does
// on hover.
//
// A session's pops out (`SideWords.window`) into a window of its own (kept.rs `pop_side`, drawn
// by Peek.tsx), where the strip is the window: the same views, filling it, over a box that only
// ever asks the side conversation. Meanwhile it's left out above the session's boxes, which go
// back to the session. Put back closes the window and it shows above the box again; Close closes
// it there, as here, and the window too.
//
// A PR's questions (PrCard.tsx, its box's Ask) are the same strip over the same store, by the
// PR's key: what differs is what it says of them (`SideWords`) and how one is closed.

import { useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { prSideClose, sideClose, sideWindow } from '../api';
import { agoPhrase } from '../format';
import { errorText } from '../report';
import { endSide, forgetSide, keptLine, showSides, takesBox, turnState, useClosing, useKept, usePopped, useSide, useSideView } from '../side';
import type { PrItem, SideTurn } from '../types';
import { Markdown } from './Markdown';
import { Button, cx, Icon, Rich } from './primitives';

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
  /** Beside the title of one closed and still answering. */
  answering: string;
  /** Close's tooltip, on an open one. */
  closeTitle: string;
  close: (key: string) => Promise<void>;
  /** Where they pop out into a window of their own: how, and what the window says with nothing
   *  open. */
  window?: { pop: (key: string) => Promise<void>; empty: string };
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
  answering: 'Closed, and still answering. A new question starts a new copy, which knows none of this.',
  closeTitle: 'Close: the box goes back to the session. An answer on its way still comes, and what was said is kept: /side shows it.',
  close: sideClose,
  window: { pop: (key) => sideWindow(key, true), empty: 'Nothing open on the side. Ask below, and a throwaway copy of the conversation answers. The session never sees it.' },
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
    answering: 'Closed, and still answering. A new question starts a new conversation, which knows none of this.',
    closeTitle: 'Close: an answer on its way still comes, and what was said is kept: Earlier lists it.',
    close: prSideClose,
  };
}

/** The strip as a side conversation's own window (kept.rs `pop_side`): what it's about, there
 *  being no session beside it, and what its window does. */
export interface SideWindowCan {
  /** The session's title. */
  about: string;
  /** Shows the session in Codealong's main window, as a click on its title does. */
  show: () => void;
  /** Closes the window: the side conversation shows above the session's box again. */
  putBack: () => void;
  /** Closes the window and the open side conversation, as Close does above a box. */
  close: () => void;
}

/** The strip's top line: what this is, a few words about it, and what can be done with it. In a
 *  window of its own the words are the session's title, which `onAbout` shows. */
function Head({ title, about, aboutTitle, onAbout, children }: { title: string; about: string; aboutTitle?: string; onAbout?: () => void; children: ReactNode }) {
  return (
    <header className="ca-sidestrip-head">
      <Icon name="help" size={14} />
      <span className="ca-sidestrip-title">{title}</span>
      {onAbout ? (
        <span className="ca-sidestrip-about has-link">
          <button type="button" className="ca-sidestrip-session" title={aboutTitle ?? about} onClick={onAbout}>
            {about}
          </button>
        </span>
      ) : (
        <span className="ca-sidestrip-about" title={aboutTitle ?? about}>
          {about}
        </span>
      )}
      <span className="ca-sidestrip-acts">{children}</span>
    </header>
  );
}

/** One of the strip's actions: an icon, its words on hover. */
function Act({ icon, label, title, onClick }: { icon: string; label: string; title?: string; onClick: () => void }) {
  return <Button variant="ghost" size="sm" icon={icon} aria-label={label} title={title ?? label} onClick={onClick} />;
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

/** The side conversations of `sideKey`: a session's, or a PR's. Above a box, or with `inWindow`
 *  the whole of a window of their own. */
export function SideStrip({ sideKey: key, words, inWindow }: { sideKey: string; words: SideWords; inWindow?: SideWindowCan }) {
  const side = useSide(key);
  const shown = useSideView(key);
  // Out in a window of its own, it isn't above the box as well.
  const popped = usePopped(key);
  // Why it didn't pop out.
  const [failed, setFailed] = useState<string | null>(null);
  // A question starting a copy shows the open one, as nothing else does, once there is one.
  const starting = shown?.kind === 'starting';
  const view = starting ? null : shown;
  const past = useKept(key);
  const earlier = past.filter((k) => k.id !== side?.id);
  const reading = view?.kind === 'past' ? past.find((k) => k.id === view.id) : undefined;
  // One closed while its copy answers reads as it answers.
  const closing = useClosing();
  const answering = view?.kind === 'past' ? closing.get(view.id) : undefined;
  const scroller = useRef<HTMLDivElement>(null);
  // The end stays in view as the answer grows, unless you've scrolled back to read.
  const atEnd = useRef(true);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && atEnd.current && !view) el.scrollTop = el.scrollHeight;
  }, [side, view]);
  if (popped && !inWindow) return null;

  const now = Date.now();
  const many = (n: number, one: string) => (n === 1 ? `1 ${one}` : `${n} ${one}s`);
  const pop = words.window;
  /** Out into a window of its own, or back above the box from one. */
  const place = inWindow ? (
    <Act icon="picture-in-picture-off" label="Put back" title="Put back above the session’s box: this window closes, and the conversation goes on there" onClick={inWindow.putBack} />
  ) : pop ? (
    <Act
      icon="picture-in-picture-on"
      label="Pop out"
      title="Pop out into a window of its own. The box here goes back to the session."
      onClick={() => {
        setFailed(null);
        pop.pop(key).catch((e: unknown) => setFailed(errorText(e)));
      }}
    />
  ) : null;
  /** A window's ×, where nothing's open to end. */
  const closeWindow = inWindow && <Act icon="x" label="Close" title="Close this window" onClick={inWindow.close} />;
  /** Away from the list or one being read: back to the open one, if there is one. Above a box
   *  that's out of sight; in a window it's back to what the window shows, and × closes it. */
  const hide = inWindow ? (
    <>
      <Act icon="chevron-left" label="Back" title={side ? 'Back to the open one' : 'Back'} onClick={() => showSides(key, null)} />
      {place}
      {closeWindow}
    </>
  ) : (
    <>
      {place}
      <Act icon="x" label="Close" onClick={() => showSides(key, null)} />
    </>
  );
  const about = (said: string) => (inWindow ? { about: inWindow.about, aboutTitle: `Show “${inWindow.about}” in Codealong. ${said}`, onAbout: inWindow.show } : { about: said });
  const section = (label: string, head: ReactNode, body: ReactNode, scrolls?: boolean) => (
    <section className={cx('ca-sidestrip', inWindow && 'is-window')} aria-label={label}>
      {head}
      {failed && <div className="ca-sidestrip-error">{failed}</div>}
      <div
        className="ca-sidestrip-turns"
        ref={scrolls ? scroller : undefined}
        onScroll={
          scrolls
            ? (e) => {
                const el = e.currentTarget;
                atEnd.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
              }
            : undefined
        }
      >
        {body}
      </div>
    </section>
  );

  const read = answering
    ? { id: answering.id, turns: answering.turns, current: answering.turns.findIndex((t) => !t.done), said: words.answering }
    : reading && { id: reading.id, turns: reading.turns, current: -1, said: `${agoPhrase(reading.lastAt, now)} · ${words.closed}` };
  if (read) {
    return section(
      'Side conversation',
      <Head title={words.title} {...about(read.said)}>
        <Act icon="history" label="Earlier" title="The others from before" onClick={() => showSides(key, { kind: 'list' })} />
        <Act
          icon="trash"
          label="Delete"
          title={answering ? 'Stop it answering, and forget this side conversation' : 'Forget this side conversation'}
          onClick={() => {
            // Back to the others, or away with none left.
            showSides(key, earlier.length > 1 ? { kind: 'list' } : null);
            forgetSide(key, read.id);
          }}
        />
        {hide}
      </Head>,
      <Turns turns={read.turns} current={read.current} />,
    );
  }

  if (view) {
    return section(
      'Side conversations',
      <Head title={words.listTitle} {...about(words.listAbout)}>
        {hide}
      </Head>,
      <>
        {earlier.length === 0 && <div className="ca-sidestrip-doing">{side ? 'None before the one that’s open.' : words.none}</div>}
        {earlier.map((k) => (
          <button key={k.id} type="button" className="ca-sidestrip-kept" onClick={() => showSides(key, { kind: 'past', id: k.id })}>
            <span className="ca-sidestrip-kept-q">{keptLine(k)}</span>
            <span className="ca-sidestrip-kept-when">
              {closing.has(k.id) ? 'answering' : agoPhrase(k.lastAt, now)} · {many(k.turns.length, 'question')}
            </span>
          </button>
        ))}
      </>,
    );
  }

  const earlierAct = earlier.length > 0 && <Act icon="history" label="Earlier" title={`${many(earlier.length, 'side conversation')} from before`} onClick={() => showSides(key, { kind: 'list' })} />;

  if (!side) {
    const asked = starting && takesBox(shown, now);
    // Above a box there's nothing to show until a question's asked; a window of its own says
    // so, over the box that asks one.
    if (!asked && !inWindow) return null;
    return section(
      'Side conversation',
      <Head title={words.title} {...about(words.open)}>
        {asked ? (inWindow ? <>{place}{closeWindow}</> : hide) : <>{earlierAct}{closeWindow}</>}
      </Head>,
      <div className="ca-sidestrip-doing" role="status">
        {asked ? words.starting : (pop?.empty ?? words.none)}
      </div>,
    );
  }
  return section(
    'Side conversation',
    <Head title={words.title} {...about(side.ended ? words.ended : words.open)}>
      {earlierAct}
      {place}
      <Act icon="x" label="Close" title={inWindow ? `${words.closeTitle} The window closes too.` : words.closeTitle} onClick={inWindow ? inWindow.close : () => endSide(key, words.close)} />
    </Head>,
    <Turns turns={side.turns} current={side.turns.findIndex((t) => !t.done)} />,
    true,
  );
}

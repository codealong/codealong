// Questions a session asks you (Claude Code's AskUserQuestion), laid out to read: each with its
// choices, numbered as Claude Code's own prompt numbers them, and, once answered, what you picked.
// The conversation shows a question call this way in place of its input, and the reply area does
// while the questions can only be answered where the session runs. Your answers also show in the
// conversation as a message of yours (`answeredOf`). Answering them is `QuestionSteps`, one at a
// time, with your picks in `useQuestionPicks`: the reply area's box answers a session that waits
// on them (SessionReply.tsx), and Rewind to this question answers them again (EditFrom.tsx).

import { useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { bare, chordPressed, cmd } from '../platform';
import type { Answers, Question, Suggestions } from '../types';
import { Chip, cx, Icon, Rich, Seg } from './primitives';

/** What an answer picked among a question's choices, and any words of your own. Claude Code
 *  records several picks as a list, or, in older versions, as one string joined with commas. */
export function picksOf(q: Question, given: string[] | undefined): { picked: Set<string>; own: string[] } {
  const labels = new Set(q.options.map((o) => o.label));
  const picked = new Set<string>();
  const own: string[] = [];
  for (const a of given ?? []) {
    if (labels.has(a)) {
      picked.add(a);
      continue;
    }
    const parts = q.multiSelect ? a.split(', ') : [];
    const known = parts.filter((p) => labels.has(p));
    if (!known.length) {
      own.push(a);
      continue;
    }
    for (const p of known) picked.add(p);
    const rest = parts.filter((p) => !labels.has(p)).join(', ');
    if (rest) own.push(rest);
  }
  return { picked, own };
}

/** Your answers to a call's questions as the conversation shows them, a message of yours: in the
 *  order it asked them, each by its header where it has one, with what you picked or wrote. An
 *  answer to a question the call doesn't list (its call is on a page not read yet) comes last, by
 *  its question's words; a question you left unanswered isn't there. */
export function answeredOf(questions: Question[], answers: Record<string, string[]>): { question: string; header?: string; answer: string }[] {
  const asked = new Set(questions.map((q) => q.question));
  const listed = questions.filter((q) => answers[q.question]?.length).map((q) => ({ question: q.question, header: q.header, answer: answers[q.question].join(', ') }));
  const rest = Object.entries(answers)
    .filter(([q, a]) => !asked.has(q) && a.length)
    .map(([question, a]) => ({ question, answer: a.join(', ') }));
  return [...listed, ...rest];
}

/** Where the box that answers several questions, one at a time, goes on to from the one it
 *  shows (`at`): the next that has no answer yet, after it and then from the first. None once
 *  every other question has one, when what's left is to answer them. */
export function nextQuestion(answered: readonly boolean[], at: number): number | null {
  const n = answered.length;
  for (let k = 1; k < n; k++) {
    const i = (at + k) % n;
    if (!answered[i]) return i;
  }
  return null;
}

/** Why another session picked a question's choice for you (`Suggestions.why`), under it. */
export function QuestionWhy({ why }: { why: string }) {
  return (
    <span className="ca-question-why">
      <Icon name="sparkles" size={12} />
      <span>{why}</span>
    </span>
  );
}

/** `suggested` marks the choices another session picked for them (`Suggestions.picks`), and
 *  `why` says why, while they wait to be answered where the session runs. */
export function QuestionList({ questions, answers, suggested, why }: { questions: Question[]; answers?: Record<string, string[]>; suggested?: Record<string, string[]>; why?: Record<string, string> }) {
  return (
    <div className="ca-questions">
      {questions.map((q) => {
        const given = answers?.[q.question];
        const { picked, own } = picksOf(q, given);
        return (
          <div key={q.question} className="ca-question">
            <div className="ca-approval-label">
              {q.header && <Chip className="ca-question-header">{q.header}</Chip>}
              <Rich text={q.question} now={0} />
            </div>
            {q.options.length > 0 && (
              <ol className={cx('ca-question-list', given && 'is-answered')}>
                {q.options.map((o) => (
                  <li key={o.label} className={picked.has(o.label) ? 'is-picked' : undefined}>
                    <span className="ca-question-opt">
                      <Rich text={o.label} now={0} />
                      {picked.has(o.label) && <Icon name="check" size={12} stroke={2.25} title="Your pick" />}
                      {suggested?.[q.question]?.includes(o.label) && <Icon name="sparkles" size={12} title="Suggested" />}
                    </span>
                    {o.description && (
                      <span className="ca-question-desc">
                        <Rich text={o.description} now={0} />
                      </span>
                    )}
                    {why?.[q.question] && suggested?.[q.question]?.[0] === o.label && <QuestionWhy why={why[q.question]} />}
                  </li>
                ))}
              </ol>
            )}
            {own.length > 0 && (
              <div className="ca-question-yours">
                <span>Your answer</span> {own.join(', ')}
              </div>
            )}
            {!given && answers && <div className="ca-question-yours is-none">Not answered</div>}
          </div>
        );
      })}
    </div>
  );
}

/** The picks and words of answers given before, for answering the questions again: a choice
 *  given as its own label is a pick, and anything else is your own words, never split. */
export function givenBack(questions: Question[], given: Answers | undefined): { picks: Record<string, string[]>; own: Record<string, string> } {
  const picks: Record<string, string[]> = {};
  const own: Record<string, string> = {};
  for (const q of questions) {
    const a = given?.[q.question];
    if (a === undefined) continue;
    const labels = new Set(q.options.map((o) => o.label));
    const values = Array.isArray(a) ? a : [a];
    picks[q.question] = values.filter((v) => labels.has(v));
    const words = values.filter((v) => !labels.has(v));
    if (words.length) own[q.question] = words.join(', ');
  }
  return { picks, own };
}

/** Picks that are still among their questions' choices, one for a question that takes one. A
 *  call's questions can change as it waits (a background job's take their headers and whether
 *  they take several from its transcript once it has them), and picks made among the earlier
 *  ones would answer with choices that don't show. A question whose picks are all gone has none,
 *  so what was suggested for it shows again. */
export function stillOffered(questions: Question[], picks: Record<string, string[]>): Record<string, string[]> {
  const kept: Record<string, string[]> = {};
  for (const q of questions) {
    const labels = picks[q.question];
    if (!labels) continue;
    const offered = labels.filter((l) => q.options.some((o) => o.label === l));
    if (offered.length || !labels.length) kept[q.question] = q.multiSelect ? offered : offered.slice(0, 1);
  }
  return kept;
}

/** Where answering questions stands: your picks and your own words, by question, and the one
 *  showing. */
export interface Picking {
  picks: Record<string, string[]>;
  own: Record<string, string>;
  at: number;
}

/** Your answers as the conversation has them (`TranscriptItem.answers`), to answer the questions
 *  again from, at the first: what you picked among the choices, the first only for a question
 *  that takes one, and words of your own. */
export function answeredBefore(questions: Question[], answers: Record<string, string[]>): Picking {
  const picks: Record<string, string[]> = {};
  const own: Record<string, string> = {};
  for (const q of questions) {
    const given = answers[q.question];
    if (!given?.length) continue;
    const { picked, own: words } = picksOf(q, given);
    picks[q.question] = q.multiSelect ? [...picked] : [...picked].slice(0, 1);
    if (words.length) own[q.question] = words.join(', ');
  }
  return { picks, own, at: 0 };
}

const offeredNone = (): string[] => [];

/** Your picks among questions' choices, and words of your own, as you answer them one at a time
 *  (`QuestionSteps`), from `first`, and the answers they make: each choice its own label, your
 *  words one more. A question you haven't touched has the choices `offered` picks for it (another
 *  session's suggestions). */
export function useQuestionPicks(questions: Question[], first: () => Picking, offered: (q: Question) => string[] = offeredNone) {
  const [start] = useState(first);
  // Your picks, by question; a question you haven't touched has the ones offered for it.
  const [picks, setPicks] = useState(start.picks);
  // The questions as they were when you picked, so picks no longer offered go as they change.
  const shape = JSON.stringify(questions.map((q) => [q.question, !!q.multiSelect, q.options.map((o) => o.label)]));
  const [pickedAmong, setPickedAmong] = useState(shape);
  if (pickedAmong !== shape) {
    setPickedAmong(shape);
    setPicks((all) => stillOffered(questions, all));
  }
  const [own, setOwn] = useState(start.own);
  const chosenFor = (q: Question): string[] => picks[q.question] ?? offered(q);
  // The question showing, for the questions it was picked among: another set starts at the first.
  const asked = questions.map((q) => q.question).join('\n');
  const [step, setStep] = useState({ asked, at: start.at });
  const at = step.asked === asked ? Math.min(step.at, questions.length - 1) : 0;
  const go = (i: number) => setStep({ asked, at: i });
  const answers: Answers = {};
  for (const q of questions) {
    const words = (own[q.question] ?? '').trim();
    const chosen = chosenFor(q);
    if (q.multiSelect) {
      const all = words ? [...chosen, words] : chosen;
      if (all.length) answers[q.question] = all;
    } else if (words || chosen.length) {
      answers[q.question] = words || chosen[0];
    }
  }
  const ready = Object.keys(answers).length > 0;
  const answered = questions.map((q) => q.question in answers);
  // Where Next goes, while another question has no answer; with none, what's left is to answer.
  const next = nextQuestion(answered, at);
  const pick = (q: Question, label: string) => {
    setPicks((all) => {
      const chosen = all[q.question] ?? offered(q);
      const next = q.multiSelect ? (chosen.includes(label) ? chosen.filter((l) => l !== label) : [...chosen, label]) : [label];
      return { ...all, [q.question]: next };
    });
    // One answer to a question that takes one: a choice clears what was typed, and is all the
    // question takes, so the next one without an answer shows.
    if (!q.multiSelect) {
      setOwn((all) => ({ ...all, [q.question]: '' }));
      if (next !== null) go(next);
    }
  };
  const type = (q: Question, text: string) => {
    setOwn((all) => ({ ...all, [q.question]: text }));
    if (!q.multiSelect && text.trim()) setPicks((all) => ({ ...all, [q.question]: [] }));
    // Words of yours to go with picks are an answer you've begun: picks suggested later don't
    // join it, and the ones it went with stay.
    else if (q.multiSelect) setPicks((all) => (q.question in all ? all : { ...all, [q.question]: offered(q) }));
  };
  return { picks, own, at, asked, go, setPicks, setOwn, setStep, offered, chosenFor, answers, ready, answered, next, pick, type };
}

export type QuestionPicks = ReturnType<typeof useQuestionPicks>;

interface QuestionStepsProps {
  questions: Question[];
  picks: QuestionPicks;
  busy: boolean;
  /** Its choices are all it takes (`Pending.exiting`): no box for your own answer. */
  exiting?: boolean;
  /** Choices another session picked for them, each marked Suggested, with why under the first. */
  suggested?: Suggestions | null;
  /** What goes under the choices: the suggestions' head, to dismiss them. */
  after?: ReactNode;
  /** ⌘⏎ in the box for your own answer. */
  onAnswer: () => void;
}

/** Questions to answer: pick a choice, several where a question takes more, or type your own
 *  answer. Several show one at a time, as Claude Code's own prompt shows them, with a strip of
 *  their headers to go between them: all at once, a few long ones ran off the bottom of the
 *  panel. Enter in your own answer goes on to the next question; it never answers, so a
 *  half-typed answer isn't sent. */
export function QuestionSteps({ questions, picks: p, busy, exiting = false, suggested, after, onAnswer }: QuestionStepsProps) {
  // The answer box to take the keyboard once Enter there went on to the next question.
  const ownRef = useRef<HTMLInputElement>(null);
  const typing = useRef(false);
  useLayoutEffect(() => {
    if (!typing.current) return;
    typing.current = false;
    ownRef.current?.focus();
  }, [p.at]);
  const q = questions[p.at];
  return (
    <>
      {questions.length > 1 && (
        <Seg
          className="ca-question-steps"
          label="Questions"
          options={questions.map((q, i) => ({ id: i, label: q.header || `Question ${i + 1}`, title: q.question, icon: p.answered[i] ? 'check' : undefined }))}
          value={p.at}
          onPick={p.go}
        />
      )}
      <div key={q.question} className="ca-question">
        {/* The question and its choices scroll as one where the card is short for them; the box
            for your own answer and the buttons stay under them (QuestionList.css). */}
        <div className="ca-question-body">
          <div className="ca-approval-label">
            {/* With several, the strip above names each by its header. */}
            {q.header && questions.length === 1 && <Chip className="ca-question-header">{q.header}</Chip>}
            <Rich text={q.question} now={0} />
          </div>
          {/* Its choices as Claude Code's own prompt lists them, each with what it says about it.
              As buttons, a picked one looked like Answer and the strip's question, and the words
              that say what a choice means were hidden in a tooltip. */}
          <div className="ca-question-choices" role={q.multiSelect ? 'group' : 'radiogroup'} aria-label={q.question}>
            {q.options.map((o) => {
              const on = p.chosenFor(q).includes(o.label);
              // Why it was suggested goes with the first choice suggested: one for each question.
              const why = p.offered(q)[0] === o.label ? suggested?.why?.[q.question] : undefined;
              return (
                <button key={o.label} type="button" className={cx('ca-question-choice', on && 'is-on')} role={q.multiSelect ? 'checkbox' : 'radio'} aria-checked={on} disabled={busy} onClick={() => p.pick(q, o.label)}>
                  <span className={cx('ca-question-mark', q.multiSelect && 'is-multi')} aria-hidden="true">
                    {on && q.multiSelect && <Icon name="check" size={10} stroke={2.5} />}
                  </span>
                  <span className="ca-question-choice-text">
                    <span className="ca-question-choice-label">
                      <span>
                        <Rich text={o.label} now={0} />
                      </span>
                      {/* The suggested one keeps its chip once you pick another. */}
                      {p.offered(q).includes(o.label) && (
                        <Chip className="ca-suggestedchip" title={suggested?.byTitle ? `Suggested by ${suggested.byTitle}` : 'Suggested'}>
                          <Icon name="sparkles" size={12} />
                          Suggested
                        </Chip>
                      )}
                    </span>
                    {o.description && (
                      <span className="ca-question-desc">
                        <Rich text={o.description} now={0} />
                      </span>
                    )}
                    {why && <QuestionWhy why={why} />}
                  </span>
                </button>
              );
            })}
          </div>
          {after}
        </div>
        {/* What an exit asks takes nothing but its choices. */}
        {!exiting && (
          <input
            ref={ownRef}
            className="ca-question-own"
            type="text"
            placeholder={q.options.length ? 'Or type your own answer' : 'Type your answer'}
            value={p.own[q.question] ?? ''}
            disabled={busy}
            onChange={(e) => p.type(q, e.target.value)}
            onKeyDown={(e) => {
              if (e.key !== 'Enter' || e.nativeEvent.isComposing || !(bare(e) || chordPressed('Cmd+Enter', e))) return;
              if (cmd(e)) {
                // ⌘⏎ from an answer you typed, in every window: on to the next question, or, with
                // none left, it answers and goes on.
                e.preventDefault();
                onAnswer();
              } else if (p.next !== null) {
                // Enter goes on to the next question and its box.
                e.preventDefault();
                typing.current = true;
                p.go(p.next);
              }
            }}
          />
        )}
      </div>
    </>
  );
}

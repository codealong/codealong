// Questions a session asks you (Claude Code's AskUserQuestion), laid out to read: each with its
// choices, numbered as Claude Code's own prompt numbers them, and, once answered, what you picked.
// The conversation shows a question call this way in place of its input, and the reply area does
// while the questions can only be answered where the session runs.

import type { Question } from '../types';
import { Chip, cx, Icon, Rich } from './primitives';

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

/** `suggested` marks the choices another session picked for them (`Suggestions.picks`), while
 *  they wait to be answered where the session runs. */
export function QuestionList({ questions, answers, suggested }: { questions: Question[]; answers?: Record<string, string[]>; suggested?: Record<string, string[]> }) {
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

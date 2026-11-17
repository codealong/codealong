// Edit from here: a prompt of yours in a session's conversation, rewritten, starts a copy of the
// session that goes on from just before it (start.rs `fork_from`), which a right-click on the
// prompt offers (SessionThread.tsx). Rewind to this question does the same from your answers to
// questions the agent asked: the copy goes on from just after it asked them, with the answers you
// give again. SessionPanel.tsx holds the dialogs, so every window showing a session asks the same.
// The session itself carries on as it was, and the files stay as they are: only the conversation
// goes back.

import { useEffect, useRef, useState } from 'react';
import { chordPressed, cmd, keysIn } from '../platform';
import { splitAttachments, withAttachments } from '../todos';
import type { Answers, Question, Session, TranscriptItem } from '../types';
import { AttachmentStrip } from './DescriptionEditor';
import { Button, Dialog } from './primitives';
import { answeredBefore, QuestionSteps, useQuestionPicks } from './QuestionList';

/** What the copy is, for the dialog's lede. */
export function editWords(s: Pick<Session, 'title'>): string {
  return `A copy of “${s.title}” goes on from just before this message, with your edit in its place. This session stays as it is, and so do the files: only the conversation goes back.`;
}

interface EditDialogProps {
  session: Session;
  /** The prompt of yours it goes back to. */
  prompt: TranscriptItem;
  /** Forks the session from before the prompt at `cursor`, `text` in its place (menus.ts `edit`). */
  onEdit: (cursor: number, text: string) => void;
  onClose: () => void;
}

/** Asks for your edit of a prompt before a copy goes on from just before it. The images and files
 *  it came with show in a strip under its words, as the reply box shows them, and go with the
 *  edit unless taken out: the prompt's text refers to them as kept attachments (transcript.rs
 *  `shown_in`), which the copy's first prompt carries as any sent from here does. */
export function EditDialog({ session: s, prompt, onEdit, onClose }: EditDialogProps) {
  const [{ text: words, attachments }] = useState(() => splitAttachments(prompt.text));
  const [text, setText] = useState(words);
  const [attached, setAttached] = useState(attachments);
  const full = withAttachments(text, attached);
  const box = useRef<HTMLTextAreaElement>(null);
  // At the end of what you wrote, to go on from there: WebKit puts the caret at the start of a box
  // focused for the first time (primitives.tsx `BoxText`).
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, []);
  const fork = () => {
    if (!full.trim()) return;
    onEdit(prompt.cursor, full);
    onClose();
  };
  return (
    <Dialog
      className="ca-editdlg"
      title="Edit from here"
      lede={editWords(s)}
      onClose={onClose}
      onSubmit={fork}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" icon="git-fork" disabled={!full.trim()} title={keysIn('⌘⏎')}>
            Fork and send
          </Button>
        </>
      }
    >
      <label className="ca-field ca-editdlg-text">
        <span>Your message</span>
        <textarea
          ref={box}
          value={text}
          rows={Math.min(14, Math.max(4, text.split('\n').length + 1))}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && cmd(e)) {
              e.preventDefault();
              fork();
            }
          }}
        />
      </label>
      {attached.length > 0 && (
        <div className="ca-editdlg-attached">
          <AttachmentStrip items={attached} onRemove={(a) => setAttached(attached.filter((x) => x.path !== a.path))} />
        </div>
      )}
    </Dialog>
  );
}

/** What the copy goes on with once you've answered its questions again, as your message: your
 *  answers, a line each, as Claude Code hands an agent answers (`"question"="answer"`, several
 *  joined). Its last call, the one that asked, ends in the copy with no answer ("interrupted"),
 *  so the message says what they answer. A question left unanswered isn't there. */
export function answersPrompt(questions: Question[], answers: Answers): string {
  const lines = questions.flatMap((q) => {
    const a = answers[q.question];
    if (a === undefined) return [];
    return [`"${q.question}"="${Array.isArray(a) ? a.join(', ') : a}"`];
  });
  return `My answers to the ${questions.length > 1 ? 'questions' : 'question'} you just asked:\n${lines.join('\n')}`;
}

/** What the copy is, for the rewind dialog's lede. */
export function rewindWords(s: Pick<Session, 'title'>, several: boolean): string {
  return `A copy of “${s.title}” goes back to ${several ? 'these questions' : 'this question'} and goes on from the answers you give here. This session stays as it is, and so do the files: only the conversation goes back.`;
}

/** Its title, and the menu item's words without the ellipsis. */
export function rewindTitle(several: boolean): string {
  return several ? 'Rewind to these questions' : 'Rewind to this question';
}

interface RewindDialogProps {
  session: Session;
  /** Your answers: the result of the call that asked them. */
  answered: TranscriptItem;
  /** The questions it asked. */
  questions: Question[];
  /** Forks the session from before your answers at `cursor`, `text` going to the copy (menus.ts
   *  `edit`). */
  onRewind: (cursor: number, text: string) => void;
  onClose: () => void;
}

/** Asks the questions again, your answers to them picked, before a copy goes on from just after
 *  they were asked with the answers you give. The copy can't ask them itself: Claude Code turns
 *  its questions off where Codealong runs it (claude/mod.rs `fork_from`). */
export function RewindDialog({ session: s, answered, questions, onRewind, onClose }: RewindDialogProps) {
  const p = useQuestionPicks(questions, () => answeredBefore(questions, answered.answers ?? {}));
  const several = questions.length > 1;
  // On to a question with no answer yet, as the box that answers them goes; with none, the fork.
  const answer = () => {
    if (p.next !== null) return p.go(p.next);
    if (!p.ready) return;
    onRewind(answered.cursor, answersPrompt(questions, p.answers));
    onClose();
  };
  return (
    <Dialog
      className="ca-rewinddlg"
      title={rewindTitle(several)}
      lede={rewindWords(s, several)}
      onClose={onClose}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          {p.next !== null ? (
            <Button variant="primary" size="sm" title={keysIn('⌘⏎')} onClick={answer}>
              Next
            </Button>
          ) : (
            <Button variant="primary" size="sm" icon="git-fork" disabled={!p.ready} title={keysIn('⌘⏎')} onClick={answer}>
              Fork and answer
            </Button>
          )}
        </>
      }
    >
      {/* ⌘⏎ from a choice as from the box for your own answer, which takes it first. */}
      <div
        className="ca-rewinddlg-questions"
        onKeyDown={(e) => {
          if (e.defaultPrevented || !chordPressed('Cmd+Enter', e)) return;
          e.preventDefault();
          answer();
        }}
      >
        <QuestionSteps questions={questions} picks={p} busy={false} onAnswer={answer} />
      </div>
    </Dialog>
  );
}

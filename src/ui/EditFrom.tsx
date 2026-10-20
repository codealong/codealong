// Edit from here: a prompt of yours in a session's conversation, rewritten, starts a copy of the
// session that goes on from just before it (start.rs `fork_from`), which a right-click on the
// prompt offers (SessionThread.tsx). SessionPanel.tsx holds the dialog, so every window showing a
// session asks the same. The session itself carries on as it was, and the files stay as they are:
// only the conversation goes back.

import { useEffect, useRef, useState } from 'react';
import { cmd, keysIn } from '../platform';
import type { Session, TranscriptItem } from '../types';
import { Button, Dialog } from './primitives';

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

/** Asks for your edit of a prompt before a copy goes on from just before it. */
export function EditDialog({ session: s, prompt, onEdit, onClose }: EditDialogProps) {
  const [text, setText] = useState(prompt.text);
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
    if (!text.trim()) return;
    onEdit(prompt.cursor, text);
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
          <Button variant="primary" size="sm" type="submit" icon="git-fork" disabled={!text.trim()} title={keysIn('⌘⏎')}>
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
    </Dialog>
  );
}

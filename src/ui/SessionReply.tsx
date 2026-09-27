// Answering a session from Codealong: a box that sends it a message where it runs (send.rs), or,
// when it waits on you to allow a tool call, that call with Allow and Deny, if its harness can be
// answered from here (harness.rs: OpenCode through its service, Claude Code by a keypress in its
// Ghostty or tmux tab, once its own record says the prompt is up). Otherwise it offers to open the
// session to answer there. The session panel, the floating bar's peek and the quick panel's reply
// view all end with it.
//
// A session that needs you only because its last reply asks you something sits at its prompt, so
// it gets the box: what you type is the answer.

import { useEffect, useLayoutEffect, useRef, useState, type ClipboardEvent } from 'react';
import { sendToSession, sessionAnswer, sessionPending } from '../api';
import { filesToAttach, keepFiles } from '../attachInput';
import { sessionPlace } from '../harness';
import { attachmentsIn, insertAttachments, withoutAttachment } from '../todos';
import type { Pending, Sent, Session } from '../types';
import { CodeBlock } from './CopyButton';
import { AttachmentStrip } from './DescriptionEditor';
import { Button, Icon, Rich } from './primitives';

/** The box grows to this many lines, then scrolls. */
const LINES = 8;

/** What you wrote to each session and haven't sent, by session: going to another session and
 *  back, or closing the peek, keeps it. */
const drafts = new Map<string, string>();

function plain(e: unknown): string {
  return String(e).replace(/^Error: /, '');
}

/** Whether typing reaches the session now: anything but one waiting on you to answer a prompt
 *  (a permission, or a turn stuck on a command). */
export function takesReply(s: Session): boolean {
  return s.status !== 'needs' || !!s.asks;
}

/** What the box asks for, and where a message goes when that isn't where the session runs. */
export function replyPlaceholder(s: Session, terminal: string): string {
  const to = s.asks ? `Answer ${s.account}` : `Send to ${s.account}`;
  if (s.harness === 'antigravity') return `${to}: Antigravity opens with this on the clipboard`;
  if (!s.live && s.harness === 'claude') return s.surface === 'app' ? `${to}: it isn’t running, so the Claude app opens it with this on the clipboard` : `${to}: it isn’t running, so this resumes it in ${terminal}`;
  if (!s.live && s.harness === 'codex' && s.surface === 'cli') return `${to}: it isn’t running, so this resumes it in ${terminal}`;
  if (s.status === 'working') return `${to}: it takes this in as it works`;
  return to;
}

interface ComposerProps {
  session: Session;
  terminal: string;
  /** Puts the cursor in the box as it shows. */
  autoFocus?: boolean;
  /** After a message went through, with what happened. */
  onSent?: (sent: Sent) => void;
}

function Composer({ session: s, terminal, autoFocus, onSent }: ComposerProps) {
  const [text, setTextNow] = useState(() => drafts.get(s.key) ?? '');
  const [sending, setSending] = useState(false);
  const [note, setNote] = useState<{ text: string; error: boolean } | null>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  const latest = useRef(text);
  latest.current = text;
  const attached = attachmentsIn(text);

  const setText = (t: string) => {
    setTextNow(t);
    latest.current = t;
    if (t) drafts.set(s.key, t);
    else drafts.delete(s.key);
  };

  useLayoutEffect(() => {
    const el = box.current;
    if (!autoFocus || !el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, [autoFocus]);

  const send = () => {
    const t = text.trim();
    if (!t || sending) return;
    setSending(true);
    setNote(null);
    sendToSession(s.key, t)
      .then((sent) => {
        setText('');
        setNote({ text: sent?.message ?? 'Sent.', error: false });
        onSent?.(sent);
      })
      .catch((e: unknown) => setNote({ text: plain(e), error: true }))
      .finally(() => {
        setSending(false);
        box.current?.focus();
      });
  };

  // A pasted screenshot or file goes in at the cursor, on a line of its own; the agent gets its
  // path (send.rs expands it).
  const paste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const files = filesToAttach(e);
    if (files.length === 0) return;
    e.preventDefault();
    const at = e.currentTarget.selectionStart;
    keepFiles(files)
      .then((kept) => {
        const r = insertAttachments(latest.current, at, kept);
        setText(r.text);
        window.requestAnimationFrame(() => {
          box.current?.focus();
          box.current?.setSelectionRange(r.cursor, r.cursor);
        });
      })
      .catch((err: unknown) => setNote({ text: plain(err), error: true }));
  };

  return (
    <div className="ca-composer">
      <textarea
        ref={box}
        value={text}
        rows={Math.min(LINES, Math.max(2, text.split('\n').length))}
        placeholder={replyPlaceholder(s, terminal)}
        onChange={(e) => {
          setText(e.target.value);
          if (note && !note.error) setNote(null);
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
            e.preventDefault();
            send();
          }
        }}
        onPaste={paste}
        aria-label={`Message to ${s.title}`}
        disabled={sending}
      />
      {attached.length > 0 && <AttachmentStrip items={attached} onRemove={(a) => setText(withoutAttachment(latest.current, a.path))} />}
      <div className="ca-composer-foot">
        <span className={['ca-composer-note', note?.error && 'is-error'].filter(Boolean).join(' ')}>{sending ? 'Sending…' : (note?.text ?? '⏎ sends · ⇧⏎ new line')}</span>
        <Button variant="primary" size="sm" icon="send" disabled={!text.trim() || sending} onClick={send}>
          Send
        </Button>
      </div>
    </div>
  );
}

interface ApprovalProps {
  session: Session;
  terminal: string;
  onOpen: () => void;
  /** After an answer went through, with what happened. */
  onAnswered?: (said: string) => void;
  /** ⌘⏎ allows, where the keyboard is the way in (the quick panel). */
  keys?: boolean;
}

/** A session waiting on you: what it wants to do, with Allow and Deny when it can be answered here. */
function Approval({ session: s, terminal, onOpen, onAnswered, keys }: ApprovalProps) {
  const [pending, setPending] = useState<Pending | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<{ text: string; error: boolean } | null>(null);
  useEffect(() => {
    let live = true;
    sessionPending(s.key)
      .then((p) => live && setPending(p))
      .catch(() => live && setPending(null));
    return () => {
      live = false;
    };
  }, [s.key, s.statusSince]);

  const answer = (a: 'allow' | 'deny') => {
    if (!pending || busy) return;
    setBusy(true);
    setNote(null);
    sessionAnswer(s.key, pending.id, a)
      .then((said) => {
        setNote({ text: said, error: false });
        setPending(null);
        onAnswered?.(said);
      })
      .catch((e: unknown) => setNote({ text: plain(e), error: true }))
      .finally(() => setBusy(false));
  };

  const answerable = !!pending && !pending.cannot;
  const allowRef = useRef(answer);
  allowRef.current = answer;
  useEffect(() => {
    if (!keys || !answerable) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || !e.metaKey || e.altKey || e.ctrlKey || e.shiftKey || e.key !== 'Enter') return;
      e.preventDefault();
      allowRef.current('allow');
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [keys, answerable]);

  const where = sessionPlace(s, terminal);
  if (!pending || pending.cannot) {
    const why = s.stalledSince ? `It looks stuck. Open it in ${where} to see what it’s waiting for.` : `It’s waiting on you. Answer it in ${where}.`;
    return (
      <div className="ca-composer is-waiting">
        <span>
          {note?.text ?? why}
          {pending?.cannot ? ` ${pending.cannot}` : ''}
        </span>
        <Button size="sm" icon="arrow-up-right" onClick={onOpen}>
          Open
        </Button>
      </div>
    );
  }
  return (
    <div className="ca-composer is-approval">
      <div className="ca-approval-head">
        <Icon name="alert" size={14} />
        <span>Wants to run</span>
        <button type="button" className="ca-linkbtn" onClick={onOpen}>
          Open in {where}
        </button>
      </div>
      <div className="ca-approval-label">
        <Rich text={pending.label} now={0} />
      </div>
      {pending.input && pending.input !== pending.label && <CodeBlock text={pending.input} className="ca-approval-input" />}
      <div className="ca-composer-foot">
        <span className={['ca-composer-note', note?.error && 'is-error'].filter(Boolean).join(' ')}>{busy ? 'Answering…' : (note?.text ?? 'Allows this call once. Deny stops the turn.')}</span>
        <Button size="sm" disabled={busy} onClick={() => answer('deny')}>
          Deny
        </Button>
        <Button variant="primary" size="sm" icon="check" kbd={keys ? ['⌘', '⏎'] : undefined} disabled={busy} onClick={() => answer('allow')}>
          Allow
        </Button>
      </div>
    </div>
  );
}

export interface ReplyProps {
  session: Session;
  terminal: string;
  /** Opens the session where it lives, to answer what can't be answered here. */
  onOpen: () => void;
  autoFocus?: boolean;
  keys?: boolean;
  onSent?: (sent: Sent) => void;
  onAnswered?: (said: string) => void;
}

/** The bottom of a session's panel: the box that sends it a message, or what it waits on you for. */
export function Reply({ session, terminal, onOpen, autoFocus, keys, onSent, onAnswered }: ReplyProps) {
  if (!takesReply(session)) return <Approval key={session.key} session={session} terminal={terminal} onOpen={onOpen} onAnswered={onAnswered} keys={keys} />;
  return <Composer key={session.key} session={session} terminal={terminal} autoFocus={autoFocus} onSent={onSent} />;
}

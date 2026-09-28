// Answering a session from Codealong: a box that sends it a message where it runs (send.rs), and,
// when it waits on you to allow a tool call, that call above the box with Allow and Deny, if its
// harness can be answered from here (harness.rs: OpenCode through its service, Claude Code by a
// keypress in its Ghostty or tmux tab, once its own record says the prompt is up). Otherwise it
// offers to open the session to answer there. The session panel, the floating bar's peek and the
// quick panel's reply view all end with it.
//
// The box stays while the session waits on you, so a session that starts waiting as you type
// doesn't take what you wrote away; you can send it once the session is no longer waiting. What
// you wrote is kept per session, for every window (drafts.ts), until you send it.
//
// Sending clears the box at once and the thread shows the message on its way (ahead.ts), so you
// can go on writing the next; messages to one session go in the order you sent them. One that
// doesn't go is back in the box, with why.
//
// A session that needs you only because its last reply asks you something sits at its prompt, so
// what you type is the answer.
//
// Where there's a next session waiting on you (the Sessions view's panel), ⌘⏎ sends and goes on to
// it, or, while the session waits on a call that can be allowed here, allows it and goes on.

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ClipboardEvent } from 'react';
import { sendAhead, sending, useAhead } from '../ahead';
import { sessionAnswer, sessionCancel, sessionPending } from '../api';
import { filesToAttach, keepFiles } from '../attachInput';
import { onDraftChange, readDraft, writeDraft } from '../drafts';
import { sessionPlace } from '../harness';
import { attachmentsIn, insertAttachments, withoutAttachment } from '../todos';
import type { Pending, Sent, Session } from '../types';
import { CodeBlock } from './CopyButton';
import { AttachmentStrip } from './DescriptionEditor';
import { Button, Icon, Rich } from './primitives';

/** The box grows to this many lines, then scrolls. */
const LINES = 8;

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
  if (s.hosted) return s.status === 'working' || s.status === 'needs' ? `${to}: it takes this after its current turn` : to;
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
  /** The session waits on you (Approval above says for what): what you write waits too. */
  waiting?: boolean;
  /** As a message goes on its way: the box is clear and the thread shows it. */
  onSending?: () => void;
  /** After a message went through, with what happened. */
  onSent?: (sent: Sent) => void;
  /** A message didn't go and no box on its session was showing to say so. It's back in the
   *  session's draft. */
  onFailed?: (why: string) => void;
  /** ⌘⏎ sends, then goes on to the next session that waits on you, saying what was done. */
  onNext?: (done: string) => void;
  /** ⌘⏎ allows the call the session waits on, then goes on: with nothing to send, or while what
   *  you write has to wait. */
  onAllowNext?: () => void;
}

type Note = { text: string; error: boolean };

/** How a send went, by session, for the box showing on it now or, with none, the next. */
const landed = new Map<string, Note>();
/** The boxes showing, by session, each hearing how its session's sends go. */
const hearing = new Map<string, Set<(note: Note) => void>>();

/** Tells the boxes showing on a session how a send went, or keeps it for the next one. Says
 *  whether a box heard. */
function land(key: string, note: Note): boolean {
  const boxes = hearing.get(key);
  if (!boxes || boxes.size === 0) {
    landed.set(key, note);
    return false;
  }
  landed.delete(key);
  for (const hear of boxes) hear(note);
  return true;
}

function Composer({ session: s, terminal, autoFocus, waiting, onSending, onSent, onFailed, onNext, onAllowNext }: ComposerProps) {
  const [text, setTextNow] = useState(() => readDraft(s.key));
  // A message to this session on its way, from this box or any other.
  const going = sending(useAhead(), s.key);
  const [note, setNote] = useState<Note | null>(() => landed.get(s.key) ?? null);
  const box = useRef<HTMLTextAreaElement>(null);
  const latest = useRef(text);
  latest.current = text;
  const attached = attachmentsIn(text);

  const setText = (t: string) => {
    setTextNow(t);
    latest.current = t;
    writeDraft(s.key, t);
  };

  // Another window wrote to this session too, or sent what was written.
  useEffect(
    () =>
      onDraftChange(s.key, (t) => {
        setTextNow(t);
        latest.current = t;
      }),
    [s.key],
  );

  // How this session's sends go, whichever box sent them. A message that didn't go is back in the
  // draft, so the box takes the draft again.
  useEffect(() => {
    // Said here, so no later box says it again. (An effect, not the initializer: StrictMode
    // mounts a box twice in development and keeps the second.)
    landed.delete(s.key);
    const hear = (n: Note) => {
      setNote(n);
      if (!n.error) return;
      const t = readDraft(s.key);
      setTextNow(t);
      latest.current = t;
      box.current?.focus();
    };
    const boxes = hearing.get(s.key) ?? new Set<(note: Note) => void>();
    boxes.add(hear);
    hearing.set(s.key, boxes);
    return () => {
      boxes.delete(hear);
      if (boxes.size === 0) hearing.delete(s.key);
    };
  }, [s.key]);

  useLayoutEffect(() => {
    const el = box.current;
    if (!autoFocus || !el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, [autoFocus]);

  // A session Codealong runs can be stopped mid-turn from here.
  const [stopping, setStopping] = useState(false);
  const stop = () => {
    setStopping(true);
    sessionCancel(s.key)
      .catch((e: unknown) => setNote({ text: plain(e), error: true }))
      .finally(() => setStopping(false));
  };

  /** Sends what's in the box; says whether there was anything to send. */
  const send = (): boolean => {
    const t = text.trim();
    if (!t || waiting) return false;
    setNote(null);
    setText('');
    // What happens next is told to whichever box is showing then: this one may be gone by then
    // (the peek closes as it sends).
    const key = s.key;
    sendAhead(s, t).then(
      (sent) => {
        land(key, { text: sent?.message ?? 'Sent.', error: false });
        onSent?.(sent);
      },
      (e: unknown) => {
        const why = plain(e);
        if (!land(key, { text: why, error: true })) onFailed?.(why);
      },
    );
    onSending?.();
    return true;
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
          if (e.key !== 'Enter' || e.shiftKey || e.nativeEvent.isComposing) return;
          if (e.metaKey && onAllowNext && (waiting || !text.trim())) {
            e.preventDefault();
            onAllowNext();
            return;
          }
          // While it waits on you ⏎ sends nothing, and ⌘⏎ goes on to Allow (the quick panel's).
          if (waiting && e.metaKey) return;
          e.preventDefault();
          if (send() && e.metaKey) onNext?.('sent.');
        }}
        onPaste={paste}
        aria-label={`Message to ${s.title}`}
      />
      {attached.length > 0 && <AttachmentStrip items={attached} onRemove={(a) => setText(withoutAttachment(latest.current, a.path))} />}
      <div className="ca-composer-foot">
        <span className={['ca-composer-note', note?.error && 'is-error'].filter(Boolean).join(' ')}>
          {note?.error ? note.text : going ? 'Sending…' : (note?.text ?? hint(waiting, !!onNext, !!onAllowNext))}
        </span>
        {s.hosted && s.status === 'working' && (
          <Button size="sm" icon="pause" disabled={stopping} onClick={stop}>
            Stop
          </Button>
        )}
        <Button variant="primary" size="sm" icon="send" disabled={!text.trim() || waiting} onClick={send}>
          Send
        </Button>
      </div>
    </div>
  );
}

/** What the box's keys do, below it. */
function hint(waiting: boolean | undefined, next: boolean, allowNext: boolean): string {
  if (waiting) return allowNext ? '⌘⏎ allow + next · send this once it isn’t waiting on you' : 'Send this once it isn’t waiting on you';
  return next ? '⏎ send · ⌘⏎ send + next · ⇧⏎ new line' : '⏎ sends · ⇧⏎ new line';
}

interface ApprovalProps {
  session: Session;
  terminal: string;
  onOpen: () => void;
  /** After an answer went through, with what happened. */
  onAnswered?: (said: string) => void;
  /** ⌘⏎ allows, where the keyboard is the way in (the quick panel). */
  keys?: boolean;
  /** Hands the box a way to allow, then go on to the next with `onNext` (null while there's
   *  nothing to allow here), for its ⌘⏎. */
  onAllowNext?: (allow: (() => void) | null) => void;
  onNext?: (done: string) => void;
}

/** A session waiting on you: what it wants to do, with Allow and Deny when it can be answered here. */
function Approval({ session: s, terminal, onOpen, onAnswered, keys, onAllowNext, onNext }: ApprovalProps) {
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

  const answer = (a: 'allow' | 'deny', then?: (said: string) => void) => {
    if (!pending || busy) return;
    setBusy(true);
    setNote(null);
    sessionAnswer(s.key, pending.id, a)
      .then((said) => {
        setNote({ text: said, error: false });
        setPending(null);
        onAnswered?.(said);
        then?.(said);
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
  // Where it goes on to next is read once the answer is in, as it is then.
  const nextRef = useRef(onNext);
  nextRef.current = onNext;
  useEffect(() => {
    if (!onAllowNext) return;
    onAllowNext(answerable ? () => allowRef.current('allow', (said) => nextRef.current?.(said)) : null);
    return () => onAllowNext(null);
  }, [onAllowNext, answerable]);

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
        {!s.hosted && (
          <button type="button" className="ca-linkbtn" onClick={onOpen}>
            Open in {where}
          </button>
        )}
      </div>
      <div className="ca-approval-label">
        <Rich text={pending.label} now={0} />
      </div>
      {pending.input && pending.input !== pending.label && <CodeBlock text={pending.input} className="ca-approval-input" />}
      <div className="ca-composer-foot">
        <span className={['ca-composer-note', note?.error && 'is-error'].filter(Boolean).join(' ')}>{busy ? 'Answering…' : (note?.text ?? (s.hosted ? 'Allows this call once. Deny tells it no, and it carries on.' : 'Allows this call once. Deny stops the turn.'))}</span>
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
  onSending?: () => void;
  onSent?: (sent: Sent) => void;
  onFailed?: (why: string) => void;
  onAnswered?: (said: string) => void;
  /** Going on to the next session that waits on you: ⌘⏎ in the box sends, or allows the call this
   *  one waits on, then goes on, saying what was done. */
  onNext?: (done: string) => void;
}

/** The bottom of a session's panel: what it waits on you for, if anything, and the box that sends
 *  it a message. The box is the same one throughout, so it keeps what's in it as the session
 *  starts and stops waiting. */
export function Reply({ session, terminal, onOpen, autoFocus, keys, onSending, onSent, onFailed, onAnswered, onNext }: ReplyProps) {
  const waiting = !takesReply(session);
  // The Allow the box's ⌘⏎ presses, once there's a call to allow here.
  const [allow, setAllow] = useState<{ run: () => void } | null>(null);
  const takeAllow = useCallback((run: (() => void) | null) => setAllow(run ? { run } : null), []);
  const allowNext = waiting && onNext && allow ? allow.run : undefined;
  return (
    <>
      {waiting && <Approval key={`wait:${session.key}`} session={session} terminal={terminal} onOpen={onOpen} onAnswered={onAnswered} keys={keys} onAllowNext={onNext ? takeAllow : undefined} onNext={onNext} />}
      {/* A session Codealong runs takes a message while it waits, after its current turn. */}
      <Composer key={`box:${session.key}`} session={session} terminal={terminal} autoFocus={autoFocus} waiting={waiting && !session.hosted} onSending={onSending} onSent={onSent} onFailed={onFailed} onNext={onNext} onAllowNext={allowNext} />
    </>
  );
}

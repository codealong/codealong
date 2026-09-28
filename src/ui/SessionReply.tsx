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
// A session that needs you only because its last reply asks you something sits at its prompt, so
// what you type is the answer.

import { useEffect, useLayoutEffect, useRef, useState, type ClipboardEvent } from 'react';
import { sendToSession, sessionAnswer, sessionCancel, sessionPending } from '../api';
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
  /** After a message went through, with what happened. */
  onSent?: (sent: Sent) => void;
}

type Note = { text: string; error: boolean };

/** Sends on their way, by session, in this window. The box goes when the peek closes, and a send
 *  can take a while on a busy Mac: a box that shows again while its session's send is still on its
 *  way shows it sending and hears how it went, rather than offering the same text to send twice. */
const flights = new Map<string, Promise<Sent>>();
/** How a send went that landed with no box showing, for the next box on its session to say. */
const landed = new Map<string, Note>();

/** Sends `text` to a session; its draft goes once it's through. */
function fly(key: string, text: string): Promise<Sent> {
  const flight = sendToSession(key, text).then((sent) => {
    writeDraft(key, '');
    return sent;
  });
  flights.set(key, flight);
  const land = (note: Note) => {
    if (flights.get(key) !== flight) return;
    flights.delete(key);
    landed.set(key, note);
  };
  flight.then(
    (sent) => land({ text: sent?.message ?? 'Sent.', error: false }),
    (e: unknown) => land({ text: plain(e), error: true }),
  );
  return flight;
}

function Composer({ session: s, terminal, autoFocus, waiting, onSent }: ComposerProps) {
  const [text, setTextNow] = useState(() => readDraft(s.key));
  // This session's send on its way, from this box or from one showing before it.
  const [flight, setFlight] = useState<Promise<Sent> | null>(() => flights.get(s.key) ?? null);
  const sending = flight !== null;
  const [note, setNote] = useState<Note | null>(() => {
    const n = flights.has(s.key) ? undefined : landed.get(s.key);
    landed.delete(s.key);
    return n ?? null;
  });
  const box = useRef<HTMLTextAreaElement>(null);
  const sentNow = useRef(onSent);
  sentNow.current = onSent;
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

  // The box says how its session's send went, whichever box sent it.
  useEffect(() => {
    if (!flight) return;
    let live = true;
    flight
      .then((sent) => {
        if (!live) return;
        setTextNow('');
        latest.current = '';
        setNote({ text: sent?.message ?? 'Sent.', error: false });
        sentNow.current?.(sent);
      })
      .catch((e: unknown) => live && setNote({ text: plain(e), error: true }))
      .finally(() => {
        if (!live) return;
        // Heard here, so no later box says it again.
        landed.delete(s.key);
        setFlight(null);
        box.current?.focus();
      });
    return () => {
      live = false;
    };
  }, [flight, s.key]);

  const send = () => {
    const t = text.trim();
    if (!t || sending || waiting || flights.has(s.key)) return;
    setNote(null);
    setFlight(fly(s.key, t));
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
          // While it waits on you ⏎ sends nothing, and ⌘⏎ goes on to Allow (the quick panel's).
          if (waiting && e.metaKey) return;
          e.preventDefault();
          send();
        }}
        onPaste={paste}
        aria-label={`Message to ${s.title}`}
        disabled={sending}
      />
      {attached.length > 0 && <AttachmentStrip items={attached} onRemove={(a) => setText(withoutAttachment(latest.current, a.path))} />}
      <div className="ca-composer-foot">
        <span className={['ca-composer-note', note?.error && 'is-error'].filter(Boolean).join(' ')}>
          {sending ? 'Sending…' : (note?.text ?? (waiting ? 'Send this once it isn’t waiting on you' : '⏎ sends · ⇧⏎ new line'))}
        </span>
        {s.hosted && s.status === 'working' && (
          <Button size="sm" icon="pause" disabled={stopping} onClick={stop}>
            Stop
          </Button>
        )}
        <Button variant="primary" size="sm" icon="send" disabled={!text.trim() || sending || waiting} onClick={send}>
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
  onSent?: (sent: Sent) => void;
  onAnswered?: (said: string) => void;
}

/** The bottom of a session's panel: what it waits on you for, if anything, and the box that sends
 *  it a message. The box is the same one throughout, so it keeps what's in it as the session
 *  starts and stops waiting. */
export function Reply({ session, terminal, onOpen, autoFocus, keys, onSent, onAnswered }: ReplyProps) {
  const waiting = !takesReply(session);
  return (
    <>
      {waiting && <Approval key={`wait:${session.key}`} session={session} terminal={terminal} onOpen={onOpen} onAnswered={onAnswered} keys={keys} />}
      {/* A session Codealong runs takes a message while it waits, after its current turn. */}
      <Composer key={`box:${session.key}`} session={session} terminal={terminal} autoFocus={autoFocus} waiting={waiting && !session.hosted} onSent={onSent} />
    </>
  );
}

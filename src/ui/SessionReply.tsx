// Answering a session from Codealong: a box that sends it a message where it runs (send.rs), and,
// when it waits on you to allow a tool call, that call above the box with Allow and Deny, if its
// harness can be answered from here (harness.rs: OpenCode through its service, Claude Code by a
// keypress in its Ghostty, iTerm2 or tmux tab, once its own record says the prompt is up). Otherwise it
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
//
// The box is laid out as chat apps' are: the send button sits inside it, at its corner, and the
// placeholder says how a message gets there (typed into its Ghostty tab, a message from
// Codealong, its app: send.rs `way_in`). What else it has to say (how a send went, the keys that
// aren't the usual ones) goes on a line below it, there only while there's something to say.
//
// While its turn runs, Escape in the box stops it where Codealong can reach the session (the
// engine's `stoppable`), as Escape does in Claude Code and Codex, and so does the button while
// the box is empty, whose placeholder says so (⌘. too, from anywhere in the window). It sends what the harness's own Stop
// sends, or presses Escape in its terminal. The session carries on, waiting for you.

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ClipboardEvent } from 'react';
import { sendAhead, sending, useAhead } from '../ahead';
import { sessionAnswer, sessionCancel, sessionPending, sessionWay } from '../api';
import { filesToAttach, keepFiles } from '../attachInput';
import { onDraftChange, readDraft, writeDraft } from '../drafts';
import { sessionPlace } from '../harness';
import { stoppable } from '../lanes';
import { attachmentsIn, insertAttachments, withoutAttachment } from '../todos';
import type { Answers, Pending, Question, Sent, Session, Way } from '../types';
import { CodeBlock } from './CopyButton';
import { AttachmentStrip } from './DescriptionEditor';
import { Button, Icon, Rich } from './primitives';
import { errorText } from '../report';

/** The box grows to this many lines, then scrolls. */
const LINES = 8;
/** How long a Stop waits for the turn to end before saying it hasn't. */
const STOP_WAIT_MS = 8000;

/** Whether typing reaches the session now: anything but one waiting on you to answer a prompt
 *  (a permission, or a turn stuck on a command). */
export function takesReply(s: Session): boolean {
  return s.status !== 'needs' || !!s.asks;
}

/** What the box asks for, and how a message gets there (`way`, once the engine has said): typed
 *  into its terminal, pasted into its background job, a message from Codealong, or its app; or,
 *  where it can't reach the session itself, what happens instead. */
export function replyPlaceholder(s: Session, terminal: string, way?: Way | null): string {
  const to = s.asks ? `Answer ${s.account}` : `Send to ${s.account}`;
  if (s.hosted) return s.status === 'working' || s.status === 'needs' ? `${to}: it takes this after its current turn` : to;
  const working = s.status === 'working' ? ': it takes this in as it works' : '';
  switch (way?.kind) {
    case 'typed':
      return `${to} via ${way.app}${working}`;
    case 'background':
      return `${to} via its background job${working}`;
    case 'message':
      return `${to} as a message from Codealong${working}`;
    case 'codex-app':
      return `${to} via the Codex app${working}`;
    case 'opencode':
      return `${to} via OpenCode${working}`;
    case 'resume':
      return `${to}: it isn’t running, so this resumes it in ${way.app}`;
    case 'claude-app':
      return `${to}: it isn’t running, so the Claude app opens it with this on the clipboard`;
  }
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

/** How a message to each session would go, as the engine last said, so a box that shows again
 *  says it at once. */
const ways = new Map<string, Way | null>();

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
  // How what's typed gets to the session, asked again as the session starts or stops running
  // somewhere.
  const [way, setWay] = useState<Way | null>(() => ways.get(s.key) ?? null);
  useEffect(() => {
    let live = true;
    sessionWay(s.key).then(
      (w) => {
        ways.set(s.key, w);
        if (live) setWay(w);
      },
      () => {},
    );
    return () => {
      live = false;
    };
  }, [s.key, s.live, s.pid, s.surface, s.attachCommand]);

  const setText = (t: string) => {
    setTextNow(t);
    latest.current = t;
    writeDraft(s.key, t);
  };

  // Another window wrote to this session too, or sent what was written, or a prompt the session
  // took back was put back here.
  useEffect(
    () =>
      onDraftChange(s.key, (t, focus) => {
        setTextNow(t);
        latest.current = t;
        const el = box.current;
        if (!focus || !el) return;
        el.focus();
        window.requestAnimationFrame(() => el.setSelectionRange(t.length, t.length));
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

  // A session whose turn is running can be stopped from here, where Codealong can reach it. The
  // button says Stopping until the turn ends, and says so if it hasn't a while later.
  const canStop = stoppable(s);
  const [stopping, setStopping] = useState(false);
  useEffect(() => {
    if (!stopping) return;
    if (!canStop) {
      setStopping(false);
      return;
    }
    const t = window.setTimeout(() => {
      setStopping(false);
      setNote({ text: 'It still says it’s working. Open it to stop it there.', error: true });
    }, STOP_WAIT_MS);
    return () => window.clearTimeout(t);
  }, [stopping, canStop]);
  const stop = () => {
    if (stopping) return;
    setStopping(true);
    setNote(null);
    sessionCancel(s.key).then(
      (said) => setNote({ text: said, error: false }),
      (e: unknown) => {
        setStopping(false);
        setNote({ text: errorText(e), error: true });
      },
    );
  };
  const stopNow = useRef(stop);
  stopNow.current = stop;
  // ⌘. stops it too, as it stops things in the Mac's own apps, wherever the focus is in the window.
  useEffect(() => {
    if (!canStop) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== '.' || !e.metaKey || e.shiftKey || e.altKey || e.ctrlKey || e.defaultPrevented) return;
      e.preventDefault();
      stopNow.current();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [canStop]);

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
        const why = errorText(e);
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
      .catch((err: unknown) => setNote({ text: errorText(err), error: true }));
  };

  const empty = !text.trim();
  // With nothing in the box, the button stops a turn that can be stopped; with something, it
  // sends, the turn taking the message in as it works.
  const stopButton = canStop && empty;
  const said = note?.error ? note.text : going ? 'Sending…' : (note?.text ?? (stopping ? 'Stopping…' : hint(waiting, !!onNext, !!onAllowNext)));
  return (
    <div className="ca-composer">
      <div
        className="ca-composer-field"
        onMouseDown={(e) => {
          // A press anywhere in the box but its buttons puts the cursor in it.
          if ((e.target as Element).closest('button, textarea')) return;
          e.preventDefault();
          box.current?.focus();
        }}
      >
        {attached.length > 0 && <AttachmentStrip items={attached} onRemove={(a) => setText(withoutAttachment(latest.current, a.path))} />}
        <textarea
          ref={box}
          value={text}
          rows={Math.min(LINES, Math.max(2, text.split('\n').length))}
          placeholder={replyPlaceholder(s, terminal, way) + (canStop ? ' · esc stops its turn' : '')}
          onChange={(e) => {
            setText(e.target.value);
            if (note && !note.error) setNote(null);
          }}
          onKeyDown={(e) => {
            // Escape stops a running turn; once it's stopping, Escape does what it does in the
            // window (closes the peek, gives Sessions' list its keys back).
            if (e.key === 'Escape' && canStop && !stopping && !e.nativeEvent.isComposing) {
              e.preventDefault();
              stop();
              return;
            }
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
        {stopButton ? (
          <button type="button" className="ca-composer-send is-stop" aria-label={stopping ? 'Stopping' : 'Stop'} title="Stop its turn (esc)" disabled={stopping} onClick={stop}>
            <Icon name="player-stop" size={12} stroke={2} />
          </button>
        ) : (
          <button type="button" className="ca-composer-send" aria-label="Send" title={waiting ? 'Send this once it isn’t waiting on you' : 'Send (⏎)'} disabled={empty || waiting} onClick={send}>
            <Icon name="arrow-up" size={16} stroke={2} />
          </button>
        )}
      </div>
      {said && (
        <span className={['ca-composer-note', note?.error && 'is-error'].filter(Boolean).join(' ')} role="status">
          {said}
        </span>
      )}
    </div>
  );
}

/** What the box has to say when nothing's happened: only the keys that aren't a chat box's usual
 *  ones. ⏎ sends and ⇧⏎ starts a new line, as everywhere, and the placeholder says Escape stops a
 *  turn while it can. */
function hint(waiting: boolean | undefined, next: boolean, allowNext: boolean): string {
  if (waiting) return allowNext ? '⌘⏎ allow + next · send this once it isn’t waiting on you' : 'Send this once it isn’t waiting on you';
  return next ? '⌘⏎ send + next' : '';
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

  const answer = (a: 'allow' | 'deny' | 'always', then?: (said: string) => void, answers?: Answers) => {
    if (!pending || busy) return;
    setBusy(true);
    setNote(null);
    sessionAnswer(s.key, pending.id, a, answers)
      .then((said) => {
        setNote({ text: said, error: false });
        setPending(null);
        onAnswered?.(said);
        then?.(said);
      })
      .catch((e: unknown) => setNote({ text: errorText(e), error: true }))
      .finally(() => setBusy(false));
  };

  const asks = !!pending?.questions?.length;
  // Questions take picks, so ⌘⏎ doesn't allow them from the box.
  const answerable = !!pending && !pending.cannot && !asks;
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
  if (pending.questions?.length) {
    return (
      <Questions
        questions={pending.questions}
        where={s.hosted ? null : where}
        onOpen={onOpen}
        keys={keys}
        busy={busy}
        note={note}
        onAnswer={(answers) => answer('allow', undefined, answers)}
        onDecline={() => answer('deny')}
      />
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
        {pending.always && (
          <Button size="sm" disabled={busy} title={pending.alwaysRule ?? 'Allows this call, and calls like it from now on.'} onClick={() => answer('always')}>
            Always
          </Button>
        )}
        <Button variant="primary" size="sm" icon="check" kbd={keys ? ['⌘', '⏎'] : undefined} disabled={busy} onClick={() => answer('allow')}>
          Allow
        </Button>
      </div>
    </div>
  );
}

interface QuestionsProps {
  questions: Question[];
  /** Where the session runs, for its Open link; none for one Codealong runs. */
  where: string | null;
  onOpen: () => void;
  /** ⌘⏎ answers, where the keyboard is the way in (the quick panel). */
  keys?: boolean;
  busy: boolean;
  note: { text: string; error: boolean } | null;
  onAnswer: (answers: Answers) => void;
  onDecline: () => void;
}

/** Questions a session asks you (Claude Code's AskUserQuestion): pick a choice, several where a
 *  question takes more, or type your own answer; your picks go back as its answer. */
function Questions({ questions, where, onOpen, keys, busy, note, onAnswer, onDecline }: QuestionsProps) {
  const [picks, setPicks] = useState<Record<string, string[]>>({});
  const [own, setOwn] = useState<Record<string, string>>({});
  const answers: Answers = {};
  for (const q of questions) {
    const words = (own[q.question] ?? '').trim();
    const chosen = picks[q.question] ?? [];
    if (q.multiSelect) {
      const all = words ? [...chosen, words] : chosen;
      if (all.length) answers[q.question] = all;
    } else if (words || chosen.length) {
      answers[q.question] = words || chosen[0];
    }
  }
  const ready = Object.keys(answers).length > 0;
  const pick = (q: Question, label: string) => {
    setPicks((all) => {
      const chosen = all[q.question] ?? [];
      const next = q.multiSelect ? (chosen.includes(label) ? chosen.filter((l) => l !== label) : [...chosen, label]) : [label];
      return { ...all, [q.question]: next };
    });
    // One answer to a question that takes one: a choice clears what was typed.
    if (!q.multiSelect) setOwn((all) => ({ ...all, [q.question]: '' }));
  };
  const type = (q: Question, text: string) => {
    setOwn((all) => ({ ...all, [q.question]: text }));
    if (!q.multiSelect && text.trim()) setPicks((all) => ({ ...all, [q.question]: [] }));
  };
  const answerRef = useRef(() => {});
  answerRef.current = () => {
    if (ready && !busy) onAnswer(answers);
  };
  useEffect(() => {
    if (!keys) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || !e.metaKey || e.altKey || e.ctrlKey || e.shiftKey || e.key !== 'Enter') return;
      e.preventDefault();
      answerRef.current();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [keys]);
  return (
    <div className="ca-composer is-approval is-questions">
      <div className="ca-approval-head">
        <Icon name="help" size={14} />
        <span>Asks you</span>
        {where && (
          <button type="button" className="ca-linkbtn" onClick={onOpen}>
            Open in {where}
          </button>
        )}
      </div>
      {questions.map((q) => (
        <div key={q.question} className="ca-question">
          <div className="ca-approval-label">
            {q.header && <span className="ca-chip ca-question-header">{q.header}</span>}
            <Rich text={q.question} now={0} />
          </div>
          <div className="ca-question-choices" role={q.multiSelect ? 'group' : 'radiogroup'} aria-label={q.question}>
            {q.options.map((o) => {
              const on = (picks[q.question] ?? []).includes(o.label);
              return (
                <Button key={o.label} size="sm" variant={on ? 'primary' : 'secondary'} role={q.multiSelect ? 'checkbox' : 'radio'} aria-checked={on} title={o.description} disabled={busy} onClick={() => pick(q, o.label)}>
                  {o.label}
                </Button>
              );
            })}
          </div>
          <input className="ca-question-own" type="text" placeholder={q.options.length ? 'Or type your own answer' : 'Type your answer'} value={own[q.question] ?? ''} disabled={busy} onChange={(e) => type(q, e.target.value)} />
        </div>
      ))}
      <div className="ca-composer-foot">
        <span className={['ca-composer-note', note?.error && 'is-error'].filter(Boolean).join(' ')}>{busy ? 'Answering…' : (note?.text ?? (questions.some((q) => q.multiSelect) ? 'Pick one or more. Decline stops the turn.' : 'Pick one, or type your own. Decline stops the turn.'))}</span>
        <Button size="sm" disabled={busy} onClick={onDecline}>
          Decline
        </Button>
        <Button variant="primary" size="sm" icon="check" kbd={keys ? ['⌘', '⏎'] : undefined} disabled={busy || !ready} onClick={() => answerRef.current()}>
          Answer
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

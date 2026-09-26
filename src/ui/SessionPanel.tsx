// A session's own panel, on the canvas and in Sessions: where it stands and how to open it at
// the top, its whole conversation in the middle (SessionThread.tsx), and a box at the bottom
// that sends it a message where it runs (send.rs), as the quick panel does.
//
// When it waits on you to allow a tool call, the bottom of the panel says what and answers it,
// if its harness can be answered from here (harness.rs: OpenCode through its service, Claude
// Code by a keypress in its Ghostty or tmux tab, once its own record says the prompt is up).
// Otherwise it opens the session to answer there.

import { useEffect, useRef, useState } from 'react';
import { sendToSession, sessionAnswer, sessionPending } from '../api';
import { below } from '../contextMenu';
import { absolute, agoPhrase, duration, middleTruncate, tildePath } from '../format';
import { HARNESS_WORD, sessionPlace } from '../harness';
import type { Board, Lane, Pending, Session, Todo } from '../types';
import { STATUS_WORD as TODO_WORD } from '../todos';
import { InsightChips } from './InsightBits';
import { OpenInMenu } from './Inspector';
import { BranchLabel, Button, HarnessChip, Icon, Rich, STATUS_WORD, StatusGlyph } from './primitives';
import { SessionThread } from './SessionThread';
import { TodoGlyph } from './TodoBits';

function Composer({ session: s, terminal, onOpen }: { session: Session; terminal: string; onOpen: () => void }) {
  const [text, setText] = useState('');
  const [sending, setSending] = useState(false);
  const [note, setNote] = useState<{ text: string; error: boolean } | null>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  const waiting = s.status === 'needs';

  const send = () => {
    const t = text.trim();
    if (!t || sending || waiting) return;
    setSending(true);
    setNote(null);
    sendToSession(s.key, t)
      .then((sent) => {
        setText('');
        setNote({ text: sent?.message ?? 'Sent.', error: false });
      })
      .catch((e: unknown) => setNote({ text: String(e).replace(/^Error: /, ''), error: true }))
      .finally(() => {
        setSending(false);
        box.current?.focus();
      });
  };

  if (waiting) return <Approval session={s} terminal={terminal} onOpen={onOpen} />;
  return (
    <div className="ca-composer">
      <textarea
        ref={box}
        value={text}
        rows={Math.min(8, Math.max(2, text.split('\n').length))}
        placeholder={s.status === 'working' ? `Send to ${s.account}: it takes this in as it works` : `Send to ${s.account}`}
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
        aria-label={`Message to ${s.title}`}
        disabled={sending}
      />
      <div className="ca-composer-foot">
        <span className={['ca-composer-note', note?.error && 'is-error'].filter(Boolean).join(' ')}>{sending ? 'Sending…' : (note?.text ?? '⏎ sends · ⇧⏎ new line')}</span>
        <Button variant="primary" size="sm" icon="send" disabled={!text.trim() || sending} onClick={send}>
          Send
        </Button>
      </div>
    </div>
  );
}

/** A session waiting on you: what it wants to do, with Allow and Deny when it can be answered here. */
function Approval({ session: s, terminal, onOpen }: { session: Session; terminal: string; onOpen: () => void }) {
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
    if (!pending) return;
    setBusy(true);
    setNote(null);
    sessionAnswer(s.key, pending.id, a)
      .then((said) => {
        setNote({ text: said, error: false });
        setPending(null);
      })
      .catch((e: unknown) => setNote({ text: String(e).replace(/^Error: /, ''), error: true }))
      .finally(() => setBusy(false));
  };

  const where = sessionPlace(s, terminal);
  if (!pending || pending.cannot) {
    return (
      <div className="ca-composer is-waiting">
        <span>
          {note?.text ?? `It’s waiting on you. Answer it in ${where}.`}
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
      {pending.input && pending.input !== pending.label && <pre className="ca-approval-input">{pending.input}</pre>}
      <div className="ca-composer-foot">
        <span className={['ca-composer-note', note?.error && 'is-error'].filter(Boolean).join(' ')}>{busy ? 'Answering…' : (note?.text ?? 'Allows this call once. Deny stops the turn.')}</span>
        <Button size="sm" disabled={busy} onClick={() => answer('deny')}>
          Deny
        </Button>
        <Button variant="primary" size="sm" icon="check" disabled={busy} onClick={() => answer('allow')}>
          Allow
        </Button>
      </div>
    </div>
  );
}

interface SessionPanelProps {
  lane: Lane;
  session: Session;
  apps: Board['apps'];
  now: number;
  onOpen: (targetId: string) => void;
  /** A new session in this one's worktree. */
  onNewSession: () => void;
  onShowLane: (lane: Lane) => void;
  todos?: Todo[];
  onShowTodo?: (todo: Todo) => void;
  /** The session's right-click menu, from its ⋯ button. */
  onMoreMenu: (at: { x: number; y: number }) => void;
}

export function SessionPanel({ lane, session: s, apps, now, onOpen, onNewSession, onShowLane, todos, onShowTodo, onMoreMenu }: SessionPanelProps) {
  const [details, setDetails] = useState(false);
  const targets = lane.targets.filter((t) => t.id.startsWith(`s|${s.key}|`));
  const since = s.statusSince ? (s.status === 'working' ? `for ${duration(s.statusSince, now)}` : agoPhrase(s.statusSince, now)) : agoPhrase(s.lastActivity, now);

  return (
    <aside className="ca-insp ca-sessinsp" aria-label={`${s.title} details`}>
      <header className="ca-sessinsp-head">
        <div className="ca-insp-title">
          <StatusGlyph status={s.status} size={14} />
          <span className="ca-tag">{HARNESS_WORD[s.harness]}</span>
          <Button variant="ghost" size="sm" icon="dots" aria-label="More" title="Pin, archive, copy and more" className="ca-todoinsp-more" onClick={(e) => onMoreMenu(below(e.currentTarget))} />
        </div>
        <h3 className="ca-sessinsp-title">{s.title}</h3>
        <div className="ca-sessinsp-status">{s.stalledSince ? `Quiet since ${agoPhrase(s.stalledSince, now)}` : `${STATUS_WORD[s.status]} · ${since}`}</div>
        <button type="button" className="ca-insp-path" title={`Show ${lane.callsign}`} onClick={() => onShowLane(lane)}>
          <Icon name="folder" size={12} />
          <span>
            {lane.callsign} · {middleTruncate(tildePath(lane.path), 36)}
          </span>
        </button>
        {(s.branch ?? lane.branch) && (
          <div className="ca-insp-branch">
            <BranchLabel branch={(s.branch ?? lane.branch) as string} />
          </div>
        )}
        <div className="ca-insp-actions">
          {targets.length > 0 ? (
            <OpenInMenu targets={targets} defaultId={s.openTarget} onOpen={onOpen} />
          ) : (
            <Button variant="primary" icon="arrow-up-right" onClick={() => onOpen(s.openTarget)}>
              Open in {sessionPlace(s, apps.terminal)}
            </Button>
          )}
          <Button icon="plus" title={`New session in ${lane.callsign} (⌘N)`} aria-label={`New session in ${lane.callsign}`} onClick={onNewSession} />
          <button type="button" className="ca-linkbtn ca-sessinsp-detailsbtn" aria-expanded={details} onClick={() => setDetails(!details)}>
            <Icon name={details ? 'chevron-down' : 'chevron-right'} size={12} />
            Details
          </button>
        </div>
        <InsightChips session={s} />
        {todos && todos.length > 0 && (
          <div className="ca-insp-todos">
            {todos.map((t) => (
              <button key={t.id} type="button" className="ca-insp-todo" title="Show the todo" onClick={() => onShowTodo?.(t)}>
                <TodoGlyph status={t.status} />
                <span className="ca-insp-todo-title">{t.title || 'Untitled'}</span>
                <span className="ca-meta">{TODO_WORD[t.status]}</span>
              </button>
            ))}
          </div>
        )}
        {details && (
          <dl className="ca-kv">
            <dt>Account</dt>
            <dd>
              <HarnessChip session={s} withStatus={false} />
            </dd>
            <dt>ID</dt>
            <dd className="ca-code" title={s.id}>
              {s.id}
            </dd>
            {s.startedAt != null && (
              <>
                <dt>Started</dt>
                <dd title={absolute(s.startedAt)}>{agoPhrase(s.startedAt, now)}</dd>
              </>
            )}
            <dt>Last prompt</dt>
            <dd title={absolute(s.lastPromptAt)}>{agoPhrase(s.lastPromptAt, now)}</dd>
            {s.live && s.pid ? (
              <>
                <dt>Process</dt>
                <dd>pid {s.pid}</dd>
              </>
            ) : null}
            {s.resumeCommand && (
              <>
                <dt>Resume</dt>
                <dd className="ca-code" title={s.resumeCommand}>
                  {s.resumeCommand}
                </dd>
              </>
            )}
          </dl>
        )}
      </header>
      <SessionThread
        key={s.key}
        session={s}
        now={now}
        onOpen={() => onOpen(s.openTarget)}
        fallback={
          s.excerpt ? (
            <p className="ca-sessinsp-now">
              <Rich text={s.excerpt} now={now} />
            </p>
          ) : null
        }
      />
      <Composer session={s} terminal={apps.terminal} onOpen={() => onOpen(s.openTarget)} />
    </aside>
  );
}

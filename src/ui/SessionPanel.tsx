// A session's own panel, on the canvas: where it stands, how to open it, your last prompt and
// its last response in full (from the text search's index), the todos it's on, and the rest of
// what's known about it. The worktree inspector lists a worktree's sessions; this is one of them.

import { useEffect, useState } from 'react';
import { sessionMessages } from '../api';
import { below } from '../contextMenu';
import { absolute, agoPhrase, duration, middleTruncate, tildePath } from '../format';
import { HARNESS_WORD, sessionPlace } from '../harness';
import type { Board, Lane, Said, Session, Todo } from '../types';
import { STATUS_WORD as TODO_WORD } from '../todos';
import { InsightChips } from './InsightBits';
import { Markdown } from './Markdown';
import { OpenInMenu } from './Inspector';
import { BranchLabel, Button, HarnessChip, Icon, Rich, STATUS_WORD, StatusGlyph } from './primitives';
import { TodoGlyph } from './TodoBits';

/** Lines a message shows before "Show all". */
const CLAMP = 14;

function Message({ said, label, now }: { said: Said; label: string; now: number }) {
  const [all, setAll] = useState(false);
  const long = said.text.split('\n').length > CLAMP || said.text.length > 1200;
  return (
    <div className={['ca-sessinsp-msg', said.you ? 'is-you' : 'is-agent'].join(' ')}>
      <div className="ca-sessinsp-msghead">
        <span>{label}</span>
        <span className="ca-meta" title={absolute(said.at)}>
          {agoPhrase(said.at, now)}
        </span>
      </div>
      {/* Yours shows as you typed it; the agent's as the Markdown it writes. */}
      {said.you ? (
        <div className={['ca-sessinsp-text', long && !all && 'is-clamped'].filter(Boolean).join(' ')}>{said.text}</div>
      ) : (
        <Markdown text={said.text} className={['ca-sessinsp-text', 'ca-sessinsp-reply', long && !all && 'is-clamped'].filter(Boolean).join(' ')} />
      )}
      {long && (
        <button type="button" className="ca-linkbtn" onClick={() => setAll(!all)}>
          {all ? 'Show less' : 'Show all'}
        </button>
      )}
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
  // Its last messages, read again when it does something.
  const [said, setSaid] = useState<Said[] | null>(null);
  useEffect(() => {
    let live = true;
    sessionMessages(s.key)
      .then((m) => live && setSaid(m))
      .catch(() => live && setSaid([]));
    return () => {
      live = false;
    };
  }, [s.key, s.lastActivity]);

  const targets = lane.targets.filter((t) => t.id.startsWith(`s|${s.key}|`));
  // The engine sends the last exchange: your prompt, then the agent's messages since.
  const prompt = said?.find((m) => m.you);
  const turn = said?.filter((m) => !m.you) ?? [];
  const reply = turn[turn.length - 1];
  const earlier = turn.slice(0, -1);
  const [showEarlier, setShowEarlier] = useState(false);
  const busy = s.status === 'working' || s.status === 'needs' || s.status === 'error';
  const since = s.statusSince ? (s.status === 'working' ? `for ${duration(s.statusSince, now)}` : agoPhrase(s.statusSince, now)) : agoPhrase(s.lastActivity, now);
  const agent = s.account;

  return (
    <aside className="ca-insp ca-sessinsp" aria-label={`${s.title} details`}>
      <header>
        <div className="ca-insp-title">
          <StatusGlyph status={s.status} size={14} />
          <span className="ca-tag">{HARNESS_WORD[s.harness]}</span>
          <Button variant="ghost" size="sm" icon="dots" aria-label="More" title="Pin, archive, copy and more" className="ca-todoinsp-more" onClick={(e) => onMoreMenu(below(e.currentTarget))} />
        </div>
        <h3 className="ca-sessinsp-title">{s.title}</h3>
        <div className="ca-sessinsp-status">
          {s.stalledSince ? `Quiet since ${agoPhrase(s.stalledSince, now)}` : `${STATUS_WORD[s.status]} · ${since}`}
        </div>
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
      </header>
      <div className="ca-insp-actions">
        {targets.length > 0 ? (
          <OpenInMenu targets={targets} defaultId={s.openTarget} onOpen={onOpen} />
        ) : (
          <Button variant="primary" icon="arrow-up-right" onClick={() => onOpen(s.openTarget)}>
            Open in {sessionPlace(s, apps.terminal)}
          </Button>
        )}
        <Button icon="plus" title={`New session in ${lane.callsign} (⌘N)`} aria-label={`New session in ${lane.callsign}`} onClick={onNewSession} />
      </div>
      <InsightChips session={s} />
      {busy && s.excerpt && (
        <section className="ca-insp-sec">
          <h4 className="ca-label">{s.status === 'needs' ? 'Asks you' : s.status === 'error' ? 'Error' : 'Now'}</h4>
          <p className="ca-sessinsp-now">
            <Rich text={s.excerpt} now={now} />
          </p>
        </section>
      )}
      {prompt && (
        <section className="ca-insp-sec">
          <Message said={prompt} label="Your last prompt" now={now} />
        </section>
      )}
      {earlier.length > 0 && (
        <section className="ca-insp-sec ca-sessinsp-earlier">
          <button type="button" className="ca-linkbtn" onClick={() => setShowEarlier(!showEarlier)}>
            {showEarlier ? 'Hide' : 'Show'} {earlier.length} earlier {earlier.length === 1 ? 'message' : 'messages'} from this turn
          </button>
          {showEarlier && earlier.map((m) => <Message key={m.at} said={m} label={agent} now={now} />)}
        </section>
      )}
      <section className="ca-insp-sec">
        {reply ? (
          <Message said={reply} label={busy ? `Latest · ${agent}` : `Last response · ${agent}`} now={now} />
        ) : said === null ? (
          <p className="ca-empty">Reading…</p>
        ) : s.excerpt && !busy ? (
          <>
            <h4 className="ca-label">Last response</h4>
            <p className="ca-sessinsp-now">
              <Rich text={s.excerpt} now={now} />
            </p>
          </>
        ) : (
          !prompt && <p className="ca-empty">No messages to show. Text search may be off, or the session is older than it reaches.</p>
        )}
      </section>
      {todos && todos.length > 0 && (
        <section className="ca-insp-sec">
          <h4 className="ca-label">Todos · {todos.length}</h4>
          <div className="ca-insp-todos">
            {todos.map((t) => (
              <button key={t.id} type="button" className="ca-insp-todo" title="Show the todo" onClick={() => onShowTodo?.(t)}>
                <TodoGlyph status={t.status} />
                <span className="ca-insp-todo-title">{t.title || 'Untitled'}</span>
                <span className="ca-meta">{TODO_WORD[t.status]}</span>
              </button>
            ))}
          </div>
        </section>
      )}
      <section className="ca-insp-sec">
        <h4 className="ca-label">Session</h4>
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
      </section>
    </aside>
  );
}

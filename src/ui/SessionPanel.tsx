// A session's own panel, on the canvas and in Sessions: where it stands and how to open it at
// the top, its whole conversation in the middle (SessionThread.tsx), and at the bottom a box that
// sends it a message where it runs, or what it waits on you to allow (SessionReply.tsx).

import { useState } from 'react';
import { below } from '../contextMenu';
import { absolute, agoPhrase, duration, middleTruncate, tildePath } from '../format';
import { HARNESS_WORD, sessionPlace } from '../harness';
import type { Board, DeviceView, Lane, Session, Todo } from '../types';
import { STATUS_WORD as TODO_WORD } from '../todos';
import { InsightChips } from './InsightBits';
import { OpenInMenu } from './Inspector';
import { BranchLabel, Button, HarnessChip, Icon, Rich, STATUS_WORD, StatusGlyph } from './primitives';
import { Reply } from './SessionReply';
import { SessionThread } from './SessionThread';
import { TodoGlyph } from './TodoBits';

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
  /** The Mac it's on, when that's another of yours: it opens there, and a message goes there. */
  device?: DeviceView | null;
}

export function SessionPanel({ lane, session: s, apps, now, onOpen, onNewSession, onShowLane, todos, onShowTodo, onMoreMenu, device }: SessionPanelProps) {
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
          {device ? (
            <span className="ca-tag ca-device-tag" title={device.online ? `Connected ${device.via === 'iroh' ? 'through iroh' : 'on your tailnet'}` : 'Not connected'}>
              <span className={device.online ? 'ca-device-dot is-on' : 'ca-device-dot'} aria-hidden="true" />
              On {device.name}
            </span>
          ) : targets.length > 0 ? (
            <OpenInMenu targets={targets} defaultId={s.openTarget} onOpen={onOpen} />
          ) : (
            <Button variant="primary" icon="arrow-up-right" onClick={() => onOpen(s.openTarget)}>
              Open in {sessionPlace(s, apps.terminal)}
            </Button>
          )}
          {!device && <Button icon="plus" title={`New session in ${lane.callsign} (⌘N)`} aria-label={`New session in ${lane.callsign}`} onClick={onNewSession} />}
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
      {device && !device.online ? (
        <p className="ca-sessinsp-remote" role="status">
          {device.name} isn't connected, so nothing can reach this session from here right now.
        </p>
      ) : device && !device.remoteControl ? (
        <p className="ca-sessinsp-remote" role="status">
          {device.name} doesn't take prompts from your other Macs. To send from here, turn on <b>Let my linked Macs act on sessions here</b> in Settings › Devices on {device.name}.
        </p>
      ) : (
        <Reply session={s} terminal={apps.terminal} onOpen={() => onOpen(s.openTarget)} />
      )}
    </aside>
  );
}

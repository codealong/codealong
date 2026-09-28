// A session's own panel, on the canvas and in Sessions: where it stands and how to open it at
// the top, its whole conversation in the middle (SessionThread.tsx), and at the bottom a box that
// sends it a message where it runs, or what it waits on you to allow (SessionReply.tsx).
//
// In Sessions it works through what waits on you: ⌘⏎ in the box sends (or allows the call the
// session waits on) and goes on to the next, ⌘] skips to it, and a strip above the header says how
// many more there are and how the last one went (ReplyView.tsx's).

import { useState } from 'react';
import { below } from '../contextMenu';
import { folderTargets, opensInEditor, useEditorIcons } from '../editors';
import { absolute, agoPhrase, duration, middleTruncate, tildePath } from '../format';
import { HARNESS_WORD, sessionPlace } from '../harness';
import { seenable } from '../lanes';
import type { Board, DeviceView, Lane, Session, Target, Todo } from '../types';
import { STATUS_WORD as TODO_WORD } from '../todos';
import { InsightChips } from './InsightBits';
import { QueueStrip, type Notice, type Queue } from './ReplyView';
import { OpenInMenu } from './Inspector';
import { BranchLabel, Button, HarnessChip, Icon, Rich, STATUS_WORD, StatusGlyph } from './primitives';
import { Reply } from './SessionReply';
import { DeviceBadge } from './DeviceBadge';
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
  /** Marks a finished turn seen, as the bar's peek does: the header offers it for Your turn and a
   *  reply that asks you something. */
  onSeen: () => void;
  /** The device it's on, when that's another of yours: it opens there, and a message goes there. */
  device?: DeviceView | null;
  /** Shown from another panel (a PR's, a todo's): the way back to it. */
  back?: { label: string; onBack: () => void };
  /** A message sent from here didn't go, and its panel had gone by then to say so. */
  onSendFailed?: (why: string) => void;
  /** Working through what waits on you (Sessions): what's left after this one, and how the one
   *  before went. */
  queue?: Queue;
  notice?: Notice | null;
  /** Sent or allowed with ⌘⏎: on to the next, with what was done. */
  onDone?: (done: string) => void;
  /** Puts the cursor in the box, for a session gone on to from the one before. */
  autoFocus?: boolean;
}

export function SessionPanel({ lane, session: s, apps, now, onOpen, onNewSession, onShowLane, todos, onShowTodo, onMoreMenu, onSeen, device, back, onSendFailed, queue, notice, onDone, autoFocus }: SessionPanelProps) {
  const [details, setDetails] = useState(false);
  const icons = useEditorIcons(apps);
  const own = lane.targets.filter((t) => t.id.startsWith(`s|${s.key}|`));
  // Under the ways to open the session, its worktree's folder in any editor installed here. A
  // session past the few its worktree lists opens by its own target alone.
  const place: Target = { id: s.openTarget, icon: 'arrow-up-right', label: `Open in ${sessionPlace(s, apps.terminal)}`, detail: null, kbd: null, disabled: false, group: 'session' };
  const targets = opensInEditor(lane) ? [...(own.length > 0 ? own : [place]), ...folderTargets(apps, icons)] : own;
  const since = s.statusSince ? (s.status === 'working' ? `for ${duration(s.statusSince, now)}` : agoPhrase(s.statusSince, now)) : agoPhrase(s.lastActivity, now);

  return (
    <aside className="ca-insp ca-sessinsp" aria-label={`${s.title} details`}>
      <QueueStrip queue={queue} notice={notice} />
      <header className="ca-sessinsp-head">
        {back && (
          <button type="button" className="ca-linkbtn ca-sessinsp-back" title={`Back to ${back.label}`} onClick={back.onBack}>
            <Icon name="chevron-left" size={12} />
            {back.label}
          </button>
        )}
        <div className="ca-insp-title">
          <StatusGlyph status={s.status} size={14} />
          <span className="ca-tag">{HARNESS_WORD[s.harness]}</span>
          {seenable(s) && <Button variant="ghost" size="sm" icon="check" aria-label="Mark as seen" title="Mark as seen: it goes idle and leaves the bar" className="ca-sessinsp-seen" onClick={onSeen} />}
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
            <DeviceBadge device={device} now={now} full />
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
          {device.name} doesn't take prompts from your other devices. To send from here, turn on <b>Let my linked devices act on sessions here</b> in Settings › Devices on {device.name}.
        </p>
      ) : (
        <Reply session={s} terminal={apps.terminal} onOpen={() => onOpen(s.openTarget)} onFailed={onSendFailed} autoFocus={autoFocus} onNext={queue?.next ? onDone : undefined} />
      )}
    </aside>
  );
}

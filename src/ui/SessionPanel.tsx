// A session's own panel, on the canvas and in Sessions: where it stands and how to open it at
// the top, its whole conversation in the middle (SessionThread.tsx), and at the bottom a box that
// sends it a message where it runs, or what it waits on you to allow (SessionReply.tsx).
//
// The top keeps to a few lines so the conversation has the room: the title with the ways to open
// it and act on it, one line of where it stands and where it works, and one of the PRs it worked
// on (each one's card on hover) and the todo it's on. The rest is in Details and the ⋯ menu.
// A session that came of another (a fork, or started by its agent) names it in a line over the
// title, and the sessions that came of it hang under the title, a few of them, each with its
// status: a click shows one, a right-click gives its menu.
//
// In Sessions it works through what waits on you: ⌘⏎ in the box sends (or allows the call the
// session waits on) and goes on to the next, the check (⌘E) marks a finished turn seen and goes
// on, ⌘] skips to it, and a strip above the header says how many more there are and how the last
// one went (ReplyView.tsx's). ⇧⌘F expands it to the whole window, so the list is out of the way
// while you do.

import { useMemo, useState } from 'react';
import { below } from '../contextMenu';
import { shortOf } from '../devices';
import { folderTargets, opensInEditor, useEditorIcons } from '../editors';
import { absolute, ago, agoPhrase, duration, middleTruncate, tildePath } from '../format';
import { HARNESS_WORD, sessionPlace } from '../harness';
import { seenable, type Placed } from '../lanes';
import { originWord } from '../lineage';
import type { Board, DeviceView, Lane, Pr, Session, SessionOrigin, Target, Todo } from '../types';
import { panelTodos, STATUS_WORD as TODO_WORD } from '../todos';
import { InsightChips } from './InsightBits';
import { QueueStrip, type Notice, type Queue } from './ReplyView';
import { OpenInMenu } from './Inspector';
import { PreviewLink } from './PreviewBits';
import { PrChipCard } from './PrLink';
import { BranchLabel, Button, HarnessChip, Icon, Rich, STATUS_WORD, StatusGlyph } from './primitives';
import { runFolder } from './RunButton';
import { Reply } from './SessionReply';
import { DeviceBadge } from './DeviceBadge';
import { SessionThread } from './SessionThread';
import { TodoGlyph } from './TodoBits';

/** PRs shown before a "+N": a long session can have made many. */
const PRS_SHOWN = 3;
/** Sessions that came of this one listed under the title before "and N more": an agent can
 *  start many, and the conversation needs the room. */
const CAME_OF_SHOWN = 3;

/** A session on the board and how it came of another. */
export type Kin = Placed & { how: SessionOrigin['how'] };

/** Where a session came from and what came of it, each showing its own panel. */
export interface Lineage {
  from: Kin | null;
  to: Kin[];
}

interface SessionPanelProps {
  lane: Lane;
  session: Session;
  apps: Board['apps'];
  now: number;
  onOpen: (targetId: string) => void;
  onShowLane: (lane: Lane) => void;
  /** The PRs it worked on (lanes.ts `sessionPrs`), each opening on GitHub, with its menu on a
   *  right-click. */
  prs?: Pr[];
  onOpenPr?: (pr: Pr) => void;
  onPrMenu?: (pr: Pr) => void;
  todos?: Todo[];
  onShowTodo?: (todo: Todo) => void;
  /** The session it came from and the ones that came of it (`Session.origin`), those on the board:
   *  the one it came from over the title, the ones that came of it under it, each showing its own
   *  panel on a click and its own menu on a right-click. */
  lineage?: Lineage;
  onShowSession?: (lane: Lane, session: Session) => void;
  onSessionMenu?: (lane: Lane, session: Session) => void;
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
  /** In Sessions: whether the panel takes the whole window, and the way to switch. */
  maximized?: boolean;
  onMaximize?: () => void;
}

export function SessionPanel({ lane, session: s, apps, now, onOpen, onShowLane, prs = [], onOpenPr, onPrMenu, todos, onShowTodo, lineage, onShowSession, onSessionMenu, onMoreMenu, onSeen, device, back, onSendFailed, queue, notice, onDone, autoFocus, maximized, onMaximize }: SessionPanelProps) {
  const [details, setDetails] = useState(false);
  const [allPrs, setAllPrs] = useState(false);
  const [allTodos, setAllTodos] = useState(false);
  const [allKin, setAllKin] = useState(false);
  // In the order they started, so the list holds still as they work.
  const cameOf = useMemo(() => [...(lineage?.to ?? [])].sort((a, b) => (a.session.startedAt ?? a.session.lastPromptAt) - (b.session.startedAt ?? b.session.lastPromptAt)), [lineage]);
  const listed = useMemo(() => panelTodos(todos ?? []), [todos]);
  // The todo it's on: one in progress, else one still to do. Done ones wait behind "+N".
  const current = listed.find((t) => t.status !== 'done');

  const icons = useEditorIcons(apps);
  const own = lane.targets.filter((t) => t.id.startsWith(`s|${s.key}|`));
  // Under the ways to open the session, its worktree's folder in any editor installed here. A
  // session past the few its worktree lists opens by its own target alone.
  const place: Target = { id: s.openTarget, icon: 'arrow-up-right', label: `Open in ${sessionPlace(s, apps.terminal)}`, detail: null, kbd: null, disabled: false, group: 'session' };
  const targets = opensInEditor(lane) ? [...(own.length > 0 ? own : [place]), ...folderTargets(apps, icons)] : own;
  const since = s.statusSince ? (s.status === 'working' ? `for ${duration(s.statusSince, now)}` : agoPhrase(s.statusSince, now)) : agoPhrase(s.lastActivity, now);
  // Its shell blocks run in a terminal here, so not for a session on another device.
  const run = useMemo(() => (shortOf(s.key) ? undefined : { folder: runFolder(lane, s), terminal: apps.terminal }), [s.key, s.cwd, lane.path, apps.terminal]);

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
        {lineage?.from && <KinRow kin={lineage.from} now={now} parent onShow={onShowSession} onMenu={onSessionMenu} />}
        <div className="ca-sessinsp-top">
          <StatusGlyph status={s.status} size={14} />
          <h3 className="ca-sessinsp-title" title={s.title}>
            {s.title}
          </h3>
          <div className="ca-sessinsp-tools">
            {device ? (
              <DeviceBadge device={device} now={now} full />
            ) : targets.length > 0 ? (
              <OpenInMenu targets={targets} defaultId={s.openTarget} onOpen={onOpen} size="sm" />
            ) : (
              <Button variant="primary" size="sm" icon="arrow-up-right" onClick={() => onOpen(s.openTarget)}>
                Open in {sessionPlace(s, apps.terminal)}
              </Button>
            )}
            {seenable(s) && <Button variant="ghost" size="sm" icon="check" aria-label="Mark as seen" title={queue?.next ? 'Mark as seen and go on to the next (⌘E)' : 'Mark as seen: it goes idle and leaves the bar'} className="ca-sessinsp-seen" onClick={onSeen} />}
            {onMaximize && (
              <Button
                variant="ghost"
                size="sm"
                icon={maximized ? 'minimize' : 'maximize'}
                aria-label={maximized ? 'Show the list' : 'Expand to the window'}
                aria-pressed={maximized}
                title={maximized ? 'Show the list again (⇧⌘F)' : 'Expand to the whole window (⇧⌘F)'}
                className="ca-sessinsp-max"
                onClick={onMaximize}
              />
            )}
            <Button variant="ghost" size="sm" icon="info-circle" aria-label="Details" aria-expanded={details} title={details ? 'Hide the details' : 'Details: its account, ID, folder and times'} className={details ? 'ca-sessinsp-detailsbtn is-on' : 'ca-sessinsp-detailsbtn'} onClick={() => setDetails(!details)} />
            <Button variant="ghost" size="sm" icon="dots" aria-label="More" title="New session here, pin, archive, copy and more" onClick={(e) => onMoreMenu(below(e.currentTarget))} />
          </div>
        </div>
        {cameOf.length > 0 && (
          <div className="ca-sessinsp-kids" role="list" aria-label="Sessions that came of this one">
            {(allKin ? cameOf : cameOf.slice(0, CAME_OF_SHOWN)).map((k) => (
              <KinRow key={k.session.key} kin={k} now={now} onShow={onShowSession} onMenu={onSessionMenu} />
            ))}
            {!allKin && cameOf.length > CAME_OF_SHOWN && (
              <button
                type="button"
                className="ca-linkbtn ca-sessinsp-kids-more"
                title={cameOf
                  .slice(CAME_OF_SHOWN)
                  .map((k) => `${k.how === 'fork' ? 'Forked into' : 'Started'} ${k.session.title}`)
                  .join('\n')}
                onClick={() => setAllKin(true)}
              >
                and {cameOf.length - CAME_OF_SHOWN} more
              </button>
            )}
          </div>
        )}
        <div className="ca-sessinsp-meta">
          <span>{s.stalledSince ? `Quiet since ${agoPhrase(s.stalledSince, now)}` : `${STATUS_WORD[s.status]} · ${since}`}</span>
          <span aria-hidden="true">·</span>
          <span>{HARNESS_WORD[s.harness]}</span>
          <span aria-hidden="true">·</span>
          <button type="button" className="ca-sessinsp-lane" title={`Show ${lane.callsign}: ${tildePath(lane.path)}`} onClick={() => onShowLane(lane)}>
            <Icon name="folder" size={12} />
            <span>{lane.callsign}</span>
          </button>
          {(s.branch ?? lane.branch) && (
            <>
              <span aria-hidden="true">·</span>
              <BranchLabel branch={(s.branch ?? lane.branch) as string} />
            </>
          )}
        </div>
        {/* Hidden when it has nothing to show (app.css), Jev's chip included. */}
        <div className="ca-sessinsp-links">
          {(allPrs ? prs : prs.slice(0, PRS_SHOWN)).map((pr) => (
            <PrChipCard key={pr.url} pr={pr} onOpen={() => onOpenPr?.(pr)} onMenu={onPrMenu && (() => onPrMenu(pr))} />
          ))}
          {!allPrs && prs.length > PRS_SHOWN && (
            <button
              type="button"
              className="ca-linkbtn ca-sessinsp-more"
              title={prs
                .slice(PRS_SHOWN)
                .map((p) => `#${p.number} ${p.title}`)
                .join('\n')}
              onClick={() => setAllPrs(true)}
            >
              +{prs.length - PRS_SHOWN}
            </button>
          )}
          <PreviewLink preview={lane.preview} now={now} onOpen={() => onOpen('l|preview')} />
          <InsightChips session={s} />
          {!allTodos && current && (
            <button type="button" className="ca-sessinsp-todo" title={`${current.title || 'Untitled'} · ${TODO_WORD[current.status]}. Show the todo.`} onClick={() => onShowTodo?.(current)}>
              <TodoGlyph status={current.status} />
              <span>{current.title || 'Untitled'}</span>
            </button>
          )}
          {!allTodos && listed.length > (current ? 1 : 0) && (
            <button
              type="button"
              className="ca-linkbtn ca-sessinsp-more"
              title={listed
                .filter((t) => t !== current)
                .map((t) => `${t.title || 'Untitled'} · ${TODO_WORD[t.status]}`)
                .join('\n')}
              onClick={() => setAllTodos(true)}
            >
              {current ? `+${listed.length - 1}` : `${listed.length} done ${listed.length === 1 ? 'todo' : 'todos'}`}
            </button>
          )}
        </div>
        {allTodos && (
          <div className="ca-insp-todos">
            {listed.map((t) => (
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
            <dt>Folder</dt>
            <dd className="ca-code" title={lane.path}>
              {middleTruncate(tildePath(lane.path), 60)}
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
        answeredBelow={!device || (device.online && device.remoteControl)}
        run={run}
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

/** A session this one came from (`parent`, over the title) or that came of it (under the title,
 *  hanging from it): its status, its title and how long since it last moved. A click shows it, a
 *  right-click gives its menu. */
function KinRow({ kin, now, parent = false, onShow, onMenu }: { kin: Kin; now: number; parent?: boolean; onShow?: (lane: Lane, session: Session) => void; onMenu?: (lane: Lane, session: Session) => void }) {
  const t = kin.session;
  const phrase = parent ? originWord(kin.how) : kin.how === 'fork' ? 'Forked into' : 'Started';
  return (
    <button
      type="button"
      role={parent ? undefined : 'listitem'}
      className={parent ? 'ca-sessinsp-kin is-parent' : 'ca-sessinsp-kin'}
      title={`${phrase} ${t.title}, in ${kin.lane.callsign}: ${STATUS_WORD[t.status].toLowerCase()}. Click to show it, right-click for its menu.`}
      onClick={() => onShow?.(kin.lane, t)}
      onContextMenu={
        onMenu &&
        ((e) => {
          e.preventDefault();
          e.stopPropagation();
          onMenu(kin.lane, t);
        })
      }
    >
      {parent ? <span className="ca-sessinsp-kin-phrase">{phrase}</span> : <span className="ca-sessinsp-elbow" aria-hidden="true" />}
      <StatusGlyph status={t.status} size={11} />
      <span className="ca-sessinsp-kin-title">{t.title}</span>
      {kin.how === 'fork' && !parent && <Icon name="git-fork" size={11} />}
      <span className="ca-sessinsp-kin-age">{t.status === 'working' && t.statusSince ? duration(t.statusSince, now) : ago(t.lastActivity, now)}</span>
    </button>
  );
}

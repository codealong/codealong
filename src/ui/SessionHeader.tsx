// A session's header, the same wherever the session is shown with its conversation: its panel in
// the main window, the floating bar's peek and the quick panel's reply view (SessionPanel.tsx puts
// it over the conversation in all three). It keeps to a few lines so the conversation has the room:
// the title with the ways to open it and act on it (its folder in your terminal and your editors
// among them), one line of where it stands and where it works, and one of the PRs it worked on
// (each one's card on hover), the todo it's on, and how many side conversations it has
// (SideStrip.tsx), which shows them above the box. The rest is in Details and the ⋯ menu.
//
// A session that came of another (a fork, or started by its agent) names it in a line over the
// title, and the latest session that came of it hangs under the title with its status; a quiet
// "+N more" beside it lists the rest. A click shows one, a right-click gives its menu.
//
// A window says what it can do by what it passes: the main window can expand the panel, the peek
// can keep the session on the desktop and a session kept there can be closed, and Rename in a
// menu shows the name as a field. The rest of what can be done to it (snoozing, archiving,
// pinning, showing it in Codealong) is in its ⋯ menu, which a right-click anywhere on the header
// gives too, so the row of buttons stays short and is the same in every window. A PR and a
// session the header names keep their own menus, and the name's field its copy and paste. What a
// window doesn't know (the PRs, the todos, the sessions around it) it leaves out, and that line
// isn't there.

import { useMemo, useRef, useState } from 'react';
import { below } from '../contextMenu';
import { folderTargets, opensInEditor, useEditorIcons } from '../editors';
import { absolute, ago, agoPhrase, duration, middleTruncate, tildePath } from '../format';
import { HARNESS_WORD, sessionName, sessionPlace } from '../harness';
import { seenable } from '../lanes';
import { cameWord, originWord, type Kin, type Lineage } from '../lineage';
import type { SessionActions } from '../menus';
import { untilWords } from '../snooze';
import type { Board, DeviceView, Lane, Pr, Session, Target, Todo } from '../types';
import { panelTodos, STATUS_WORD as TODO_WORD } from '../todos';
import { DeviceBadge } from './DeviceBadge';
import { InsightChips } from './InsightBits';
import { OpenInMenu } from './Inspector';
import { PreviewLink } from './PreviewBits';
import { PrChipCard } from './PrLink';
import { BranchLabel, Button, HarnessChip, Icon, STATUS_WORD, StatusGlyph } from './primitives';
import { runFolder } from './RunButton';
import { SidesChip } from './SideStrip';
import { TodoGlyph } from './TodoBits';

/** PRs shown before a "+N": a long session can have made many. */
const PRS_SHOWN = 3;

export interface SessionHeaderProps {
  lane: Lane;
  session: Session;
  /** The terminal's name and the editors installed, for the ways to open it. */
  apps: Board['apps'];
  now: number;
  /** Opens one of its worktree's targets: the session where it lives, or its folder in an editor. */
  onOpen: (targetId: string) => void;
  /** Shows its worktree; without it the worktree's name is only said. */
  onShowLane?: (lane: Lane) => void;
  /** The PRs it worked on (lanes.ts `sessionPrs`), each opening on GitHub, with its menu on a
   *  right-click. */
  prs?: Pr[];
  onOpenPr?: (pr: Pr) => void;
  onPrMenu?: (pr: Pr) => void;
  todos?: Todo[];
  onShowTodo?: (todo: Todo) => void;
  /** The session it came from and the ones that came of it (`Session.origin`), those on the board:
   *  the one it came from over the title, the ones that came of it under it, each showing itself
   *  on a click and its own menu on a right-click. */
  lineage?: Lineage;
  onShowSession?: (lane: Lane, session: Session) => void;
  onSessionMenu?: (lane: Lane, session: Session) => void;
  /** The session's menu (menus.ts): from the ⋯ button, below it, and at the pointer from a
   *  right-click on the header. */
  onMenu?: (at?: { x: number; y: number }) => void;
  /** Marks a finished turn seen, where the window does more than the action alone (the main
   *  window goes on to the next); else `actions.seen` does. */
  onSeen?: () => void;
  /** The session's actions (menus.ts), as its menu has them: the check marks it seen with them
   *  where the window doesn't say how. */
  actions?: SessionActions;
  /** Keeps the session on the desktop, from the bar's peek or the notch (kept.rs); `keepTitle`
   *  says how, where it isn't the peek's way. */
  onKeep?: () => void;
  keepTitle?: string;
  /** Puts the card away: the peek, or a session kept on the desktop, which leaves it.
   *  `closeTitle` says how, where it isn't a kept session's way. */
  onClose?: () => void;
  closeTitle?: string;
  /** Its name is a field, ready to change; `onRename` hears the new one, or null when it's left
   *  as it was. */
  renaming?: boolean;
  onRename?: (name: string | null) => void;
  /** The device it's on, when that's another of yours: it opens there. */
  device?: DeviceView | null;
  /** Shown from somewhere to go back to (a PR's or a todo's panel, the quick panel's list). */
  back?: { label: string; onBack: () => void; title?: string };
  /** There's another to go on to once this one is seen. */
  next?: boolean;
  /** In Sessions: whether the panel takes the whole window, and the way to switch. */
  maximized?: boolean;
  onMaximize?: () => void;
  /** In a card short of room (the peek, the quick panel): Details is left to the main window, so
   *  the title keeps its place beside the buttons. */
  compact?: boolean;
}

/** A session's name as a field: ⏎ keeps what's typed, Escape leaves the name as it was, and so
 *  does clicking away without changing it. Clicking away after changing it keeps the change, as
 *  renaming a file in the Finder does. */
function NameField({ title, onDone }: { title: string; onDone: (name: string | null) => void }) {
  // The title it opened on: one the board changes meanwhile isn't a change of yours.
  const [opened] = useState(title);
  const [name, setName] = useState(title);
  const done = useRef(false);
  const finish = (next: string | null) => {
    if (done.current) return;
    done.current = true;
    onDone(next);
  };
  // Left as it was, a title the board cut short isn't taken for a new name.
  const keep = () => finish(name !== opened ? sessionName(name) || null : null);
  return (
    <input
      className="ca-sessinsp-name"
      aria-label="Name"
      value={name}
      autoFocus
      spellCheck={false}
      onFocus={(e) => e.currentTarget.select()}
      onChange={(e) => setName(e.target.value)}
      onKeyDown={(e) => {
        if (e.nativeEvent.isComposing) return;
        if (e.key === 'Enter') {
          e.preventDefault();
          keep();
        } else if (e.key === 'Escape') {
          e.preventDefault();
          finish(null);
        }
      }}
      onBlur={keep}
    />
  );
}

/** A session this one came from (`parent`, over the title) or that came of it (under the title,
 *  hanging from it): its status, its title and how long since it last moved. A click shows it, a
 *  right-click gives its menu. */
function KinRow({ kin, now, parent = false, onShow, onMenu }: { kin: Kin; now: number; parent?: boolean; onShow?: (lane: Lane, session: Session) => void; onMenu?: (lane: Lane, session: Session) => void }) {
  const t = kin.session;
  const phrase = parent ? originWord(kin.how) : cameWord(kin.how);
  return (
    <button
      type="button"
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
      {kin.how !== 'started' && !parent && <Icon name={kin.how === 'fork' ? 'git-fork' : 'user'} size={11} />}
      <span className="ca-sessinsp-kin-age">{t.status === 'working' && t.statusSince ? duration(t.statusSince, now) : ago(t.lastActivity, now)}</span>
    </button>
  );
}

export function SessionHeader({ lane, session: s, apps, now, onOpen, onShowLane, prs = [], onOpenPr, onPrMenu, todos, onShowTodo, lineage, onShowSession, onSessionMenu, onMenu, onSeen, actions, onKeep, keepTitle, onClose, closeTitle, renaming, onRename, device, back, next, maximized, onMaximize, compact }: SessionHeaderProps) {
  const [details, setDetails] = useState(false);
  const [allPrs, setAllPrs] = useState(false);
  const [allTodos, setAllTodos] = useState(false);
  const [allKin, setAllKin] = useState(false);
  // The latest started first: it's the one shown, and the rest open under it in the order they
  // started, so the list holds still as they work.
  const cameOf = useMemo(() => [...(lineage?.to ?? [])].sort((a, b) => (b.session.startedAt ?? b.session.lastPromptAt) - (a.session.startedAt ?? a.session.lastPromptAt)), [lineage]);
  const listed = useMemo(() => panelTodos(todos ?? []), [todos]);
  // The todo it's on: one in progress, else one still to do. Done ones wait behind "+N".
  const current = listed.find((t) => t.status !== 'done');

  const icons = useEditorIcons(apps);
  const own = lane.targets.filter((t) => t.id.startsWith(`s|${s.key}|`));
  // Under the ways to open the session, its folder: a new terminal there, and its worktree's in
  // any editor installed here. A session past the few its worktree lists opens by its own target
  // alone.
  const place: Target = { id: s.openTarget, icon: 'arrow-up-right', label: `Open in ${sessionPlace(s, apps.terminal)}`, detail: null, kbd: null, disabled: false, group: 'session' };
  const targets = opensInEditor(lane) ? [...(own.length > 0 ? own : [place]), ...folderTargets(apps, icons, { name: apps.terminal, key: s.key, folder: middleTruncate(tildePath(runFolder(lane, s)), 44) })] : own;
  const since = s.statusSince ? (s.status === 'working' ? `for ${duration(s.statusSince, now)}` : agoPhrase(s.statusSince, now)) : agoPhrase(s.lastActivity, now);
  const snoozed = s.snoozedUntil ? ` · snoozed until ${untilWords(s.snoozedUntil, now)}` : '';
  const status = (s.stalledSince ? `Quiet since ${agoPhrase(s.stalledSince, now)}` : `${s.asks ? 'Asks you' : STATUS_WORD[s.status]} · ${since}`) + snoozed;
  const naming = !!renaming && !!onRename;
  // Seen where the window says how, else by the session's own action, when it has a turn to see.
  const seen = onSeen ? (seenable(s) ? onSeen : undefined) : actions?.seen?.run;
  const branch = s.branch ?? lane.branch;

  return (
    <header
      className="ca-sessinsp-head"
      onContextMenu={
        onMenu &&
        ((e) => {
          // The name's field keeps its own menu, for copy and paste, as does anything inside with one.
          if (e.defaultPrevented || (e.target instanceof Element && e.target.closest('input, textarea, [contenteditable="true"]'))) return;
          e.preventDefault();
          onMenu();
        })
      }
    >
      {back && (
        <button type="button" className="ca-linkbtn ca-sessinsp-back" title={back.title ?? `Back to ${back.label}`} onClick={back.onBack}>
          <Icon name="chevron-left" size={12} />
          {back.label}
        </button>
      )}
      {lineage?.from && <KinRow kin={lineage.from} now={now} parent onShow={onShowSession} onMenu={onSessionMenu} />}
      <div className="ca-sessinsp-top">
        <StatusGlyph status={s.status} size={14} />
        {naming ? (
          <NameField title={s.title} onDone={onRename} />
        ) : (
          <h3 className="ca-sessinsp-title" title={s.title}>
            {s.title}
          </h3>
        )}
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
          {seen && <Button variant="ghost" size="sm" icon="check" aria-label="Mark as seen" title={next ? 'Mark as seen and go on to the next (⌘E)' : 'Mark as seen: it goes idle and leaves the bar (⌘E)'} className="ca-sessinsp-seen" onClick={seen} />}
          {onKeep && <Button variant="ghost" size="sm" icon="pin" aria-label="Keep on the desktop" title={keepTitle ?? 'Keep on the desktop: it stays up, here, until you close it. Dragging it by its header, or its edge to make it bigger, keeps it too.'} onClick={onKeep} />}
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
          {!compact && <Button variant="ghost" size="sm" icon="info-circle" aria-label="Details" aria-expanded={details} title={details ? 'Hide the details' : 'Details: its account, ID, folder and times'} className={details ? 'ca-sessinsp-detailsbtn is-on' : 'ca-sessinsp-detailsbtn'} onClick={() => setDetails(!details)} />}
          {onMenu && <Button variant="ghost" size="sm" icon="dots" aria-label="More" aria-haspopup="menu" title="Snooze, archive, pin, rename, fork, copy and more" onClick={(e) => onMenu(below(e.currentTarget))} />}
          {onClose && <Button variant="ghost" size="sm" icon="x" aria-label="Close" title={closeTitle ?? 'Close: it leaves the desktop (⌘W)'} onClick={onClose} />}
        </div>
      </div>
      {cameOf.length > 0 && (
        <div className="ca-sessinsp-kids" role="list" aria-label="Sessions that came of this one">
          {(allKin ? cameOf : cameOf.slice(0, 1)).map((k, i) => (
            <div key={k.session.key} className="ca-sessinsp-kid" role="listitem">
              <KinRow kin={k} now={now} onShow={onShowSession} onMenu={onSessionMenu} />
              {i === 0 && cameOf.length > 1 && (
                <button
                  type="button"
                  className="ca-sessinsp-kids-more"
                  aria-expanded={allKin}
                  title={
                    allKin
                      ? 'Show only the latest'
                      : cameOf
                          .slice(1)
                          .map((x) => `${cameWord(x.how)} ${x.session.title}`)
                          .join('\n')
                  }
                  onClick={() => setAllKin(!allKin)}
                >
                  {allKin ? 'fewer' : `+${cameOf.length - 1} more`}
                </button>
              )}
            </div>
          ))}
        </div>
      )}
      <div className="ca-sessinsp-meta">
        {naming ? (
          <span>⏎ renames it · Esc keeps the old name</span>
        ) : (
          <>
            <span>{status}</span>
            <span aria-hidden="true">·</span>
            <span>{HARNESS_WORD[s.harness]}</span>
            <span aria-hidden="true">·</span>
            {onShowLane ? (
              <button type="button" className="ca-sessinsp-lane" title={`Show ${lane.callsign}: ${tildePath(lane.path)}`} onClick={() => onShowLane(lane)}>
                <Icon name="folder" size={12} />
                <span>{lane.callsign}</span>
              </button>
            ) : (
              <span className="ca-sessinsp-lane is-plain" title={tildePath(lane.path)}>
                <Icon name="folder" size={12} />
                <span>{lane.callsign}</span>
              </span>
            )}
            {branch && (
              <>
                <span aria-hidden="true">·</span>
                <BranchLabel branch={branch} />
              </>
            )}
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
          <button type="button" className="ca-sessinsp-todo" title={`${current.title || 'Untitled'} · ${TODO_WORD[current.status]}.${onShowTodo ? ' Show the todo.' : ''}`} onClick={() => onShowTodo?.(current)}>
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
        <SidesChip session={s} />
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
  );
}

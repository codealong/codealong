// A session's header, the same wherever the session is shown with its conversation: its panel in
// the main window, the floating bar's peek and the waiting window (SessionPanel.tsx puts
// it over the conversation in all three). It keeps to a few lines so the conversation has the room:
// the title with the ways to open it and act on it (its folder in your terminal and your editors
// among them), one line of where it stands and where it works, and one of the PRs it worked on
// (each one's card on hover), the latest page on the web it made (ArtifactBits.tsx) and the todo
// it's on. The rest is in Details and the ⋯ menu.
//
// Its account (the harness's name alone didn't say which of its accounts) is under the reply box,
// beside where you write to it (SessionReply.tsx); where there's no box, as for a session on a
// device that can't be reached, it ends the line of where it stands, as it does on a row.
//
// A session that came of another (a fork, or started by its agent) names it in a line over the
// title, and the latest of what came of it hangs under the title with its status; a quiet
// "+N more" beside it lists the rest (lineage.ts `cameLines`). What came of it is the sessions
// it was forked into and the ones its agent started, its helpers among them; its headless runs,
// together on one line with how many; and its side conversations (SideStrip.tsx). A click shows
// a session, a right-click gives its menu; a side conversation shows above the box. The pages it
// made go the same way under its links: the latest one's chip, and "+N more" for the rest.
//
// A session Codealong runs lives in the main window's panel, so there its Open button doesn't
// offer to show what's already showing: it leads with its folder in your editor. Elsewhere it
// says Show in Codealong, which brings the panel up.
//
// A window says what it can do by what it passes: the main window can expand the panel, the peek
// can keep the session on the desktop and a session kept there can be closed, and Rename in a
// menu shows the name as a field. The rest of what can be done to it (snoozing, archiving,
// pinning, showing it in Codealong) is in its ⋯ menu, which a right-click anywhere on the header
// gives too, so the row of buttons stays short and is the same in every window. A PR and a
// session the header names keep their own menus, and the name's field its copy and paste. What a
// window doesn't know (the PRs, the todos, the sessions around it) it leaves out, and that line
// isn't there.

import { useEffect, useMemo, useRef, useState } from 'react';
import { below } from '../contextMenu';
import { folderTargets, opensInEditor, useEditorIcons } from '../editors';
import { absolute, ago, agoPhrase, duration, middleTruncate, tildePath } from '../format';
import { sessionName, sessionPlace } from '../harness';
import { cutOffWords, limitWords, seenable, sideTitle } from '../lanes';
import { cameLines, cameLineWords, cameWord, originWord, type CameLine, type Kin, type Lineage } from '../lineage';
import { CHORDS, chordKeys, type SessionActions } from '../menus';
import { showSide } from '../side';
import { untilWords } from '../snooze';
import { sessionTokens } from '../api';
import { cacheShare, money, tokenCount, totalTokens } from '../spend';
import type { Board, DeviceView, Lane, Pr, Session, SessionSpent, SideGlance, Target, Todo } from '../types';
import { panelTodos, STATUS_WORD as TODO_WORD } from '../todos';
import { ArtifactChip, ArtifactList } from './ArtifactBits';
import { DeviceBadge } from './DeviceBadge';
import { InsightChips } from './InsightBits';
import { ModeLabel } from './ModeLabel';
import { OpenInMenu } from './Inspector';
import { PreviewChip } from './PreviewBits';
import { PrChipCard } from './PrLink';
import { AccountLabel, BranchLabel, Button, HarnessChip, headerMenu, Icon, PanelBack, STATUS_WORD, SessionEmblem, SessionGlyph, type PanelBackTo } from './primitives';
import { runFolder } from './RunButton';
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
  /** The PRs it worked on (lanes.ts `sessionPrs`), each a chip that shows it in Codealong
   *  (`onShowPr`) and opens it on GitHub on a double-click (`onOpenPr`), or on a click without
   *  `onShowPr`, with its menu on a right-click. */
  prs?: Pr[];
  onShowPr?: (pr: Pr) => void;
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
  /** The strip above has Seen (SessionPanel.tsx `QueueStrip`), so the header leaves out its check:
   *  where there's no strip (the peek, a kept session, the canvas) the check is the one button. */
  seenAbove?: boolean;
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
  /** Shown from somewhere to go back to (a PR's or a todo's panel). */
  back?: PanelBackTo;
  /** In Sessions: whether the panel takes the whole window, and the way to switch. */
  maximized?: boolean;
  onMaximize?: () => void;
  /** In a card short of room (the peek, the waiting window): Details is left to the main window, so
   *  the title keeps its place beside the buttons. */
  compact?: boolean;
  /** The reply box under the conversation names its account, so the header doesn't. */
  accountBelow?: boolean;
  /** In the main window's panel, which is where a session Codealong runs lives. */
  inMain?: boolean;
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
      <SessionGlyph s={t} size={11} />
      <span className="ca-sessinsp-kin-title">{t.title}</span>
      {kin.how !== 'started' && !parent && <Icon name={kin.how === 'fork' ? 'git-fork' : kin.how === 'helper' ? 'subtask' : 'user'} size={11} />}
      <span className="ca-sessinsp-kin-age">{t.status === 'working' && t.statusSince ? duration(t.statusSince, now) : ago(t.lastActivity, now)}</span>
    </button>
  );
}

/** What keeps a line of what came of a session apart from the others. */
const cameKey = (line: CameLine) => (line.kind === 'kin' ? line.kin.session.key : line.kind === 'side' ? `side:${line.side.id}` : 'runs');

/** The headless runs a session's commands started, on one line under its title: the glyph of
 *  the latest, or the working one while any works, how many, and how long since the latest moved.
 *  A click shows the latest, which lists them all under the session in Sessions. */
function RunsRow({ runs, now, onShow }: { runs: Kin[]; now: number; onShow?: (lane: Lane, session: Session) => void }) {
  const latest = runs[0];
  const working = runs.filter((k) => k.session.status === 'working').length;
  const t = latest.session;
  const what = runs.length === 1 ? 'A headless run' : `${runs.length} headless runs`;
  return (
    <button
      type="button"
      className="ca-sessinsp-kin is-runs"
      title={`${what} its commands started (claude -p, codex exec)${working > 0 ? `, ${working} at work` : ''}: what ran them reads what they say. Click to show the latest, “${t.title}”, with the rest listed under this session.`}
      onClick={() => onShow?.(latest.lane, t)}
    >
      <span className="ca-sessinsp-elbow" aria-hidden="true" />
      <SessionGlyph s={working > 0 ? { status: 'working' } : t} size={11} />
      <span className="ca-sessinsp-kin-title">{runs.length === 1 ? '1 headless run' : `${runs.length} headless runs`}</span>
      <Icon name="robot" size={11} />
      <span className="ca-sessinsp-kin-age">{ago(t.lastActivity, now)}</span>
    </button>
  );
}

/** A side conversation asked about the session (`Board.sides`), under its title as what came of
 *  it: answering or done, what it was first asked, and when it was last asked. A click shows it
 *  above the box (or brings its window forward, out in one). */
function SideKinRow({ sessionKey, side: g, now }: { sessionKey: string; side: SideGlance; now: number }) {
  const asked = g.asked === 1 ? '1 question' : `${g.asked} questions`;
  return (
    <button
      type="button"
      className="ca-sessinsp-kin is-side"
      title={`On the side: ${g.question}\n${asked}, ${g.running ? 'answering now' : 'answered'} by a throwaway copy of this conversation, which the session never saw. Click to show it above the box.`}
      onClick={() => showSide(sessionKey, g)}
    >
      <span className="ca-sessinsp-elbow" aria-hidden="true" />
      <SessionGlyph s={{ status: g.running ? 'working' : 'idle' }} size={11} />
      <span className="ca-sessinsp-kin-title">{sideTitle(g)}</span>
      <Icon name="help" size={11} />
      <span className="ca-sessinsp-kin-age">{ago(g.lastAt, now)}</span>
    </button>
  );
}

/** The PR an automatic review's session reviews (`Session.helper`), over its title where a
 *  helper's session names the one it works for (`KinRow`): a click opens the PR on GitHub. */
function ReviewOf({ pr, prs, onOpenPr }: { pr: string; prs: Pr[]; onOpenPr?: (pr: Pr) => void }) {
  const [slug, number] = pr.split('#');
  // reviews.json keys a PR in lower case, as GitHub's names compare.
  const found = prs.find((p) => p.url.toLowerCase().includes(`github.com/${slug}/pull/${number}`));
  return (
    <button
      type="button"
      className="ca-sessinsp-kin is-parent"
      title={`An automatic review of #${number}: Codealong posts what it writes. Its turns don't wait on you; it shows when it needs you.${found && onOpenPr ? ' Click to open the PR.' : ''}`}
      disabled={!found || !onOpenPr}
      onClick={() => found && onOpenPr?.(found)}
    >
      <span className="ca-sessinsp-kin-phrase">Review of</span>
      <Icon name="pr" size={11} />
      <span className="ca-sessinsp-kin-title">{found ? `#${number} ${found.title}` : `#${number}`}</span>
    </button>
  );
}

/** What the session used and cost, its subagents' calls included, from the token ledger
 *  (ledger.rs), in its details: at API prices, and what was paid by the token where that's known.
 *  Another device's sessions are in that device's ledger, so they show none. */
function SessionCost({ sessionKey }: { sessionKey: string }) {
  const [spent, setSpent] = useState<SessionSpent | null>(null);
  useEffect(() => {
    let live = true;
    sessionTokens(sessionKey)
      .then((s) => live && setSpent(s))
      // A ledger that can't be read only leaves these lines out of the details.
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [sessionKey]);
  if (!spent || spent.total.calls === 0) return null;
  const t = spent.total;
  const byModel = spent.models.map((m) => `${m.key}: ${money(m.api)}, ${tokenCount(totalTokens(m.tokens))} tokens`).join('\n');
  return (
    <>
      <dt>Cost</dt>
      <dd title={byModel}>
        {money(t.api)} at API prices
        {t.paid > 0 ? ` · ${money(t.paid)} paid` : ''}
        {t.unpriced > 0 ? ' · some unpriced' : ''}
      </dd>
      <dt>Tokens</dt>
      <dd>
        {tokenCount(totalTokens(t.tokens))} · {Math.round(cacheShare(t.tokens) * 100)}% read from cache
      </dd>
    </>
  );
}

export function SessionHeader({ lane, session: s, apps, now, onOpen, onShowLane, prs = [], onShowPr, onOpenPr, onPrMenu, todos, onShowTodo, lineage, onShowSession, onSessionMenu, onMenu, onSeen, actions, seenAbove, onKeep, keepTitle, onClose, closeTitle, renaming, onRename, device, back, maximized, onMaximize, compact, accountBelow, inMain }: SessionHeaderProps) {
  const [details, setDetails] = useState(false);
  const [allPrs, setAllPrs] = useState(false);
  const [allTodos, setAllTodos] = useState(false);
  const [allKin, setAllKin] = useState(false);
  const [allArtifacts, setAllArtifacts] = useState(false);
  const artifacts = s.artifacts ?? [];
  // The latest started first: it's the one shown, and the rest open under it in the order they
  // started, so the list holds still as they work.
  const came = useMemo(() => cameLines(lineage), [lineage]);
  const listed = useMemo(() => panelTodos(todos ?? []), [todos]);
  // The todo it's on: one in progress, else one still to do. Done ones wait behind "+N".
  const current = listed.find((t) => t.status !== 'done');

  const icons = useEditorIcons(apps);
  // Shown in the panel it lives in, it has nowhere of its own to open.
  const shownHere = !!inMain && !!s.hosted;
  const own = lane.targets.filter((t) => t.id.startsWith(`s|${s.key}|`) && !(shownHere && t.id === s.openTarget));
  // Under the ways to open the session, its folder: a new terminal there (⌃`, in every window that
  // shows a session), and its worktree's in any editor installed here. A session past the few its
  // worktree lists opens by its own target alone.
  const place: Target = { id: s.openTarget, icon: 'arrow-up-right', label: `Open in ${sessionPlace(s, apps.terminal)}`, detail: null, kbd: null, disabled: false, group: 'session' };
  const terminal = { name: apps.terminal, key: s.key, folder: middleTruncate(tildePath(runFolder(lane, s)), 44), kbd: chordKeys(CHORDS.terminal ?? '') };
  const folder = opensInEditor(lane) ? folderTargets(apps, icons, terminal) : [];
  const targets = folder.length > 0 ? [...(own.length > 0 || shownHere ? own : [place]), ...folder] : own;
  // Your editor, which comes first of them, when it's shown where it lives.
  const leads = shownHere ? (folder.find((t) => t.id.startsWith('l|editor|'))?.id ?? null) : s.openTarget;
  const since = s.statusSince ? (s.status === 'working' ? `for ${duration(s.statusSince, now)}` : agoPhrase(s.statusSince, now)) : agoPhrase(s.lastActivity, now);
  const snoozed = s.snoozedUntil ? ` · snoozed until ${untilWords(s.snoozedUntil, now)}` : '';
  const quiet = s.quiet ? ' · quiet' : '';
  const status = (s.stalledSince ? `Silent since ${agoPhrase(s.stalledSince, now)}` : `${s.asks ? 'Asks you' : STATUS_WORD[s.status]} · ${since}`) + snoozed + quiet;
  const naming = !!renaming && !!onRename;
  // Seen where the window says how, else by the session's own action, when it has a turn to see.
  const seen = seenAbove ? undefined : onSeen ? (seenable(s) ? onSeen : undefined) : actions?.seen?.run;
  const branch = s.branch ?? lane.branch;

  return (
    <header className="ca-sessinsp-head" onContextMenu={headerMenu(onMenu && (() => onMenu()))}>
      {back && <PanelBack back={back} />}
      {lineage?.from && <KinRow kin={lineage.from} now={now} parent onShow={onShowSession} onMenu={onSessionMenu} />}
      {!lineage?.from && s.helper?.pr && <ReviewOf pr={s.helper.pr} prs={prs} onOpenPr={onOpenPr} />}
      <div className="ca-sessinsp-top">
        <SessionGlyph s={s} size={14} />
        <SessionEmblem emblem={s.emblem} size={20} />
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
            <OpenInMenu targets={targets} defaultId={leads} onOpen={onOpen} size="sm" variant="secondary" />
          ) : shownHere ? null : (
            <Button size="sm" icon="arrow-up-right" onClick={() => onOpen(s.openTarget)}>
              Open in {sessionPlace(s, apps.terminal)}
            </Button>
          )}
          {seen && <Button variant="ghost" size="sm" icon="check" aria-label="Mark as seen" title="Mark as seen: it goes idle and leaves the bar (⌘E)" className="ca-sessinsp-seen" onClick={seen} />}
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
          {!compact && <Button variant="ghost" size="sm" icon="info-circle" aria-label="Details" aria-expanded={details} title={details ? 'Hide the details' : 'Details: its account, ID, folder, times and what it cost'} className={details ? 'ca-sessinsp-detailsbtn is-on' : 'ca-sessinsp-detailsbtn'} onClick={() => setDetails(!details)} />}
          {onMenu && <Button variant="ghost" size="sm" icon="dots" aria-label="More" aria-haspopup="menu" title="Snooze, archive, pin, rename, fork, copy and more" onClick={(e) => onMenu(below(e.currentTarget))} />}
          {onClose && <Button variant="ghost" size="sm" icon="x" aria-label="Close" title={closeTitle ?? 'Close: it leaves the desktop (⌘W)'} onClick={onClose} />}
        </div>
      </div>
      {came.length > 0 && (
        <div className="ca-sessinsp-kids" role="list" aria-label="What came of this session">
          {(allKin ? came : came.slice(0, 1)).map((line, i) => (
            <div key={cameKey(line)} className="ca-sessinsp-kid" role="listitem">
              {line.kind === 'kin' ? (
                <KinRow kin={line.kin} now={now} onShow={onShowSession} onMenu={onSessionMenu} />
              ) : line.kind === 'runs' ? (
                <RunsRow runs={line.runs} now={now} onShow={onShowSession} />
              ) : (
                <SideKinRow sessionKey={s.key} side={line.side} now={now} />
              )}
              {i === 0 && came.length > 1 && (
                <button
                  type="button"
                  className="ca-sessinsp-kids-more"
                  aria-expanded={allKin}
                  title={allKin ? 'Show only the latest' : came.slice(1).map(cameLineWords).join('\n')}
                  onClick={() => setAllKin(!allKin)}
                >
                  {allKin ? 'fewer' : `+${came.length - 1} more`}
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
            {!accountBelow && (
              <>
                <span aria-hidden="true">·</span>
                <AccountLabel session={s} />
              </>
            )}
          </>
        )}
      </div>
      {s.limit && s.status !== 'working' && (
        <div className="ca-sessinsp-limit" role="note">
          <Icon name="alert" size={12} />
          <span className="ca-sessinsp-limit-words">{limitWords(s.account, s.limit, now, !!actions?.moveOn)}</span>
          {actions?.moveOn && (
            <Button size="sm" title="Its conversation goes on under that account, told to carry on where it stopped; this one is closed and archived" onClick={() => actions.moveOn?.run()}>
              {actions.moveOn.label}
            </Button>
          )}
        </div>
      )}
      {s.cutOff != null && s.status !== 'working' && (
        <div className="ca-sessinsp-limit" role="note">
          <Icon name="alert" size={12} />
          <span className="ca-sessinsp-limit-words">{cutOffWords(!!actions?.carryOn)}</span>
          {actions?.carryOn && (
            <Button size="sm" title="It goes on where its account’s last session did, told to carry on the turn that was cut off" onClick={() => actions.carryOn?.run()}>
              {actions.carryOn.label}
            </Button>
          )}
        </div>
      )}
      {/* Hidden when it has nothing to show (SessionHeader.css), Jev's chip included. */}
      <div className="ca-sessinsp-links">
        {(allPrs ? prs : prs.slice(0, PRS_SHOWN)).map((pr) => (
          <PrChipCard key={pr.url} pr={pr} onShow={onShowPr && (() => onShowPr(pr))} onOpen={onOpenPr && (() => onOpenPr(pr))} onMenu={onPrMenu && (() => onPrMenu(pr))} />
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
        {artifacts[0] && <ArtifactChip artifact={artifacts[0]} now={now} />}
        {artifacts.length > 1 && (
          <button
            type="button"
            className="ca-sessinsp-artifacts-more"
            aria-expanded={allArtifacts}
            title={
              allArtifacts
                ? 'Show only the latest'
                : artifacts
                    .slice(1)
                    .map((a) => a.title)
                    .join('\n')
            }
            onClick={() => setAllArtifacts(!allArtifacts)}
          >
            {allArtifacts ? 'fewer' : `+${artifacts.length - 1} more`}
          </button>
        )}
        <PreviewChip preview={lane.preview} now={now} onOpen={() => onOpen('l|preview')} />
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
      </div>
      {allArtifacts && artifacts.length > 1 && <ArtifactList artifacts={artifacts.slice(1)} now={now} />}
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
          {s.live && s.mode && (
            <>
              <dt>Mode</dt>
              <dd>
                <ModeLabel mode={s.mode} /> <span className="ca-meta">{s.mode.detail}</span>
              </dd>
            </>
          )}
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
          {!device && <SessionCost sessionKey={s.key} />}
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

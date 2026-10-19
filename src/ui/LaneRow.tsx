// One worktree as a flight strip, read left to right as where, then who. On the left, shaded as
// one block, the worktree and everything that belongs to it: its name, its repo and branch, what
// it holds (its app, its PR, and whether new work can start there) and its lines and commits
// against the default branch. On the right its agents, a line each, every line a session built the same way: the
// ones that need you, are working or just finished, else the one that last worked there. A
// worktree shows once however many agents run in it.

import { REUSE_WORDS } from '../reusable';
import { DeviceBadge } from './DeviceBadge';
import { memoRow } from './memoRow';
import type { MouseEvent as ReactMouseEvent, ReactNode } from 'react';
import { absolute, ago, agoPhrase, count, middleTruncate, untilShort } from '../format';
import { branchLabel, foldWords, helperLine, sideTitle, startedOf, worktreeName, type AgentItem, type Fold, type WorktreeRow } from '../lanes';
import { selectHow, type SelectHow } from '../picks';
import { untilWords } from '../snooze';
import type { HelperCount, RunCount, SideCount } from '../boardLists';
import type { SaidLine } from '../quick';
import type { GitInfo, Lane, Session, SideGlance, Suggestions, Todo } from '../types';
import { Chip, cx, driftWords, HarnessChip, Icon, LaneGlyph, PrChip, Rich, Row, SaidText, selects, STATUS_WORD, SessionEmblem, SessionGlyph, WorktreeChip } from './primitives';
import type { DragProps } from './useRowDrag';
import { FlagChip, InsightChips, LooksFinished } from './InsightBits';
import { HelpersCard, type HelperActions } from './HelpersCard';
import { useHoverCard } from './HoverCard';
import { PreviewChip } from './PreviewBits';
import { TodoChips } from './TodoBits';
import { cmd } from '../platform';

interface LaneRowProps {
  row: WorktreeRow;
  now: number;
  /** The selected item: one of the row's lines, or the worktree when it has none. */
  selectedKey: string | null;
  /** Picked whole together with other worktrees (picks.ts). */
  picked?: boolean;
  /** The element id of an item, for the list's active descendant. */
  domId: (key: string) => string;
  /** A press on one of its items: alone, or with ⌘ or ⇧ held, picking the worktree with others. */
  onSelect: (key: string, how: SelectHow) => void;
  /** Double-click on the worktree: opens its most recent session. */
  onOpen: (lane: Lane) => void;
  /** Opens one session where it lives: a click on its chip, or a double-click on its line. */
  onOpenTarget: (lane: Lane, targetId: string) => void;
  pinned?: boolean;
  /** Right-click on the worktree: its menu (open, pin, copy…). */
  onContextMenu?: () => void;
  /** Right-click on one of its agents: the session's menu. */
  onAgentMenu?: (session: Session) => void;
  /** Right-click on its PR: the PR's menu (open it, copy its link…). */
  onPrMenu?: () => void;
  /** A click on its PR: shows it in PRs. */
  onPrShow?: () => void;
  /** A double-click on its PR: opens it on GitHub. */
  onPrOpen?: () => void;
  /** Dragging the row (useRowDrag.tsx): among the pinned ones, or out of the window. */
  drag?: DragProps;
  /** Todos a session is linked to, for its line. */
  todosFor?: (sessionKey: string) => Todo[] | undefined;
  onShowTodo?: (todo: Todo) => void;
}

/** `web · billing`: the repo, and the branch when the worktree's name doesn't already say it. */
function laneSub(lane: Lane): { sub: string; branch: string | null } {
  const branch = branchLabel(lane);
  return { branch, sub: branch && branch !== worktreeName(lane) ? `${lane.repo} · ${middleTruncate(branch, 26)}` : lane.repo };
}

/** A worktree on another of your devices, a device that's away included: its rows are dimmed. */
function away(lane: Lane): boolean {
  return lane.device ? !lane.device.online : false;
}

/** Lines added and removed against the default branch, and a dot for uncommitted files. */
function Diff({ git: g }: { git: GitInfo | null }) {
  if (!g || ((g.added ?? 0) === 0 && (g.removed ?? 0) === 0 && g.dirty === 0)) return null;
  return (
    <span className="ca-diff">
      {g.added != null && ((g.added ?? 0) > 0 || (g.removed ?? 0) > 0) ? `+${g.added.toLocaleString('en-US')} −${(g.removed ?? 0).toLocaleString('en-US')}` : ''}
      {g.dirty > 0 ? <span title={`${g.dirty} uncommitted files`}> ●</span> : null}
    </span>
  );
}

/** A count in at most five characters with its sign: `987`, `5.4k`, `12k`. Tooltips have it
 *  exactly. */
function shortCount(n: number): string {
  if (n < 1000) return String(n);
  if (n < 10_000) return `${(n / 1000).toFixed(1).replace(/\.0$/, '')}k`;
  if (n < 1_000_000) return `${Math.round(n / 1000)}k`;
  return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, '')}M`;
}

/**
 * Where the worktree stands against the default branch, in a column beside its name: lines added
 * and removed (with a dot for uncommitted files) over commits ahead and behind. What the branch
 * adds lines up in one column and what it lacks in the next, so rows read as a table.
 */
function DeltaSeg({ git: g, onMouseDown, onDoubleClick }: { git: GitInfo | null; onMouseDown?: (e: ReactMouseEvent) => void; onDoubleClick?: () => void }) {
  const ahead = g?.baseAhead ?? 0;
  const behind = g?.baseBehind ?? 0;
  const added = g?.added ?? 0;
  const removed = g?.removed ?? 0;
  const dirty = g?.dirty ?? 0;
  const lines = added > 0 || removed > 0;
  const words = g
    ? [
        lines ? `${count(added, 'line')} added and ${removed.toLocaleString('en-US')} removed against ${g.base ?? 'the default branch'}, uncommitted changes included` : null,
        dirty > 0 ? `${count(dirty, 'uncommitted file')}` : null,
        driftWords(g),
      ].filter(Boolean)
    : [];
  return (
    <div className="ca-seg ca-seg-call ca-seg-delta" title={words.length ? `${words.join('. ')}.` : undefined} onMouseDown={onMouseDown} onDoubleClick={onDoubleClick}>
      {words.length > 0 && (
        <>
          <span className="ca-delta-row">
            <span className="is-gain">{added > 0 ? `+${shortCount(added)}` : ''}</span>
            <span>{removed > 0 ? `−${shortCount(removed)}` : ''}</span>
            <span>{dirty > 0 ? '●' : ''}</span>
          </span>
          <span className="ca-delta-row">
            <span className="is-gain">{ahead > 0 ? `↑${shortCount(ahead)}` : ''}</span>
            <span>{behind > 0 ? `↓${shortCount(behind)}` : ''}</span>
            <span />
          </span>
        </>
      )}
    </div>
  );
}

interface CellProps {
  lane: Lane;
  now: number;
  /** Gives the name a tooltip of its own, where the column has none. */
  titled: boolean;
  onOpenTarget: (lane: Lane, targetId: string) => void;
  onPrMenu?: () => void;
  onPrShow?: () => void;
  onPrOpen?: () => void;
}

/**
 * The worktree's column, with what belongs to the worktree at the end of each of its lines: its
 * name, then finished as Jev reads it and its app (a globe, green while it runs); its repo and
 * branch, after its reuse mark (green when new work can start there), then its PR. Each line is laid from its end,
 * so apps and PRs line up down the board. Short of room, the name keeps its line and the chips
 * beside it give way, the app last; the repo and branch give way before the PR does.
 */
function WorktreeCell({ lane, now, titled, onOpenTarget, onPrMenu, onPrShow, onPrOpen }: CellProps) {
  const { sub, branch } = laneSub(lane);
  return (
    <>
      <span className="ca-lane-cellline">
        <span className="ca-lane-call" title={titled ? (lane.origin === 'main' ? `The main worktree of ${lane.repo}` : lane.callsign) : undefined}>
          {worktreeName(lane)}
        </span>
        <span className="ca-chips ca-lane-own">
          <PreviewChip preview={lane.preview} now={now} onOpen={() => onOpenTarget(lane, 'l|preview')} inRow bare />
          <LooksFinished lane={lane} />
        </span>
      </span>
      <span className="ca-lane-cellline">
        <span className="ca-lane-sub" title={branch ?? lane.path}>
          {lane.device && <DeviceBadge device={lane.device} now={now} />}
          <ReuseMark lane={lane} />
          {sub}
        </span>
        <span className="ca-chips ca-lane-own">{lane.pr && <PrChip pr={lane.pr} compact onMenu={onPrMenu} onShow={onPrShow} onOpen={onPrOpen} inRow />}</span>
      </span>
    </>
  );
}

/** A reusable worktree's mark, before its repo and branch: it takes one piece of work after
 *  another. Green while it's ready for new work, as an app that runs is; its tooltip says what it
 *  holds now. */
export function ReuseMark({ lane }: { lane: Lane }) {
  if (!lane.reuse) return null;
  return (
    <span className={cx('ca-reusemark', lane.reuse === 'free' && 'is-ready')} title={`Reusable. ${REUSE_WORDS[lane.reuse]}`}>
      <Icon name="repeat" size={11} />
    </span>
  );
}

function LaneRowView(props: LaneRowProps) {
  return props.row.lines.length > 0 ? <GroupRow {...props} /> : <EmptyRow {...props} />;
}

// `domId` and `todosFor` answer while the row renders, so they're compared as values.
export const LaneRow = memoRow(LaneRowView, ['domId', 'todosFor']);

/** A worktree no agent has worked in: on the right, only that it has none, and its age. You
 *  select it whole. */
function EmptyRow({ row, now, selectedKey, picked = false, domId, onSelect, onOpen, onOpenTarget, pinned, onContextMenu, onPrMenu, onPrShow, onPrOpen, drag }: LaneRowProps) {
  const { lane } = row;
  const selected = picked || selectedKey === lane.id;
  return (
    <Row
      id={domId(lane.id)}
      selected={selected}
      className={cx('ca-lane', 'is-worktree', 'is-empty', `is-${lane.bay}`, pinned && 'is-pinned', away(lane) && 'is-away')}
      drag={drag}
      onSelect={(e) => onSelect(lane.id, selectHow(e))}
      onOpen={(e) => {
        if (!cmd(e) && !e.shiftKey) onOpen(lane);
      }}
      onMenu={onContextMenu}
    >
      <div className="ca-seg ca-seg-glyph">
        <LaneGlyph lane={lane} />
      </div>
      <div className="ca-seg ca-seg-call">
        <WorktreeCell lane={lane} now={now} titled onOpenTarget={onOpenTarget} onPrMenu={onPrMenu} onPrShow={onPrShow} onPrOpen={onPrOpen} />
      </div>
      <DeltaSeg git={lane.git} />
      <div className="ca-seg-lines">
        <div className="ca-agentline">
          <span className="ca-agentline-glyph" />
          <span className="ca-agentline-main">
            <span className="ca-none" title={lane.sessions.length ? `Its ${count(lane.sessions.length, 'earlier session')}, archived or from before it last took new work, are in its panel.` : undefined}>
              No agents
            </span>
          </span>
          <span className="ca-agentline-age" title={lane.lastActivity ? `Last active ${absolute(lane.lastActivity)}` : undefined}>
            {lane.lastActivity ? ago(lane.lastActivity, now) : ''}
          </span>
        </div>
      </div>
    </Row>
  );
}

/**
 * A worktree with agents: the worktree once on the left, and a line for each agent, which you
 * select, open and right-click like a row of its own.
 */
function GroupRow({ row, now, selectedKey, picked = false, domId, onSelect, onOpen, onOpenTarget, pinned, onContextMenu, onAgentMenu, onPrMenu, onPrShow, onPrOpen, drag, todosFor, onShowTodo }: LaneRowProps) {
  const { lane, lines } = row;
  // Picked, the worktree is selected whole: each of its lines.
  const isSelected = (key: string) => picked || key === selectedKey;
  const hasSelection = lines.some((a) => isSelected(a.key));
  // The worktree's own part selects its first line; with ⌘ or ⇧ it picks the worktree.
  const pressFirst = (e: ReactMouseEvent) => {
    if (selects(e)) onSelect(lines[0].key, selectHow(e));
  };
  return (
    <Row
      role="group"
      label={`${lane.callsign}, ${count(lines.length, 'agent')}`}
      className={cx('ca-lane', 'is-worktree', 'is-group', `is-${lane.bay}`, hasSelection && 'has-selection', picked && 'is-picked', pinned && 'is-pinned', away(lane) && 'is-away')}
      drag={drag}
      onMenu={onContextMenu}
    >
      <div className="ca-seg ca-seg-glyph" onMouseDown={pressFirst} onDoubleClick={() => onOpen(lane)}>
        <LaneGlyph lane={lane} />
      </div>
      <div className="ca-seg ca-seg-call" onMouseDown={pressFirst} onDoubleClick={() => onOpen(lane)} title={`${lane.path}. Double-click to open its most recent session.`}>
        <WorktreeCell lane={lane} now={now} titled={false} onOpenTarget={onOpenTarget} onPrMenu={onPrMenu} onPrShow={onPrShow} onPrOpen={onPrOpen} />
      </div>
      <DeltaSeg git={lane.git} onMouseDown={pressFirst} onDoubleClick={() => onOpen(lane)} />
      <div className="ca-seg-lines">
        {lines.map((a) => (
          <AgentLine
            key={a.key}
            item={a}
            now={now}
            domId={domId(a.key)}
            selected={isSelected(a.key)}
            onSelect={(how) => onSelect(a.key, how)}
            onOpen={() => onOpenTarget(lane, a.session.openTarget)}
            onContextMenu={onAgentMenu ? () => onAgentMenu(a.session) : undefined}
            todos={todosFor?.(a.session.key)}
            onShowTodo={onShowTodo}
          />
        ))}
      </div>
    </Row>
  );
}

/**
 * A session's line: its emblem and title first, then a label for what it waits on you for (or at
 * the line's end with the other chips when `flag` is false), then what it's doing or said last,
 * which gives way first when the line is short. A session another worktree has, on this one's row,
 * says what it did here first: made it, or worked in it.
 */
function SessionText({ s, now, flag = true }: { s: Session; now: number; flag?: boolean }) {
  return (
    <span className="ca-agentline-text">
      <SessionEmblem emblem={s.emblem} />
      <span className="ca-lane-headline ca-agentline-title">
        <Rich text={s.title} now={now} />
      </span>
      {flag && <FlagChip session={s} />}
      {(s.detail || s.visitor) && (
        <span className="ca-lane-detail ca-agentline-detail">
          {s.visitor && (s.detail ? `${s.visitor} · ` : s.visitor)}
          {s.detail && <Rich text={s.detail} now={now} />}
        </span>
      )}
    </span>
  );
}

interface SessionLinesProps {
  s: Session;
  now: number;
  /** Where it is, first on the second line: its worktree in Sessions; nothing on its worktree's row. */
  where?: ReactNode;
  /** What shows at the second line's end: in Sessions, its worktree's PR and diff. */
  end?: ReactNode;
  todos?: Todo[];
  onShowTodo?: (todo: Todo) => void;
  /** Opens it where it lives: a double-click on its account's chip. */
  onOpen: () => void;
  /** The helpers its agent started, in Sessions, and whether all of them are listed under it,
   *  which `onToggleHelpers` turns on and off; what their chip's card does with one. */
  helpers?: HelperCount;
  helpersShown?: boolean;
  onToggleHelpers?: () => void;
  helperActions?: HelperActions;
  /** The headless runs its commands started, and whether all of them are listed under it, which
   *  `onToggleRuns` turns on and off. */
  runs?: RunCount;
  runsShown?: boolean;
  onToggleRuns?: () => void;
  /** Its side conversations, and whether all of them are listed under it, which `onToggleSides`
   *  turns on and off; what their chip's card does with one. */
  sides?: SideCount;
  sidesShown?: boolean;
  onToggleSides?: () => void;
  sideActions?: HelperActions;
  /** What a search found it by when that's what was said in it, under the first line. */
  said?: SaidLine;
}

/**
 * A session on two lines, wherever a list gives it two: what it's about first (its title, what it
 * waits on you for, and what it's doing or said last), then where and why (`where`, its snooze,
 * quiet, todo and Jev's read, then its account), with `end` at the second line's end. A search
 * that found it by what was said puts the line that has the words between them.
 */
function SessionLines({ s, now, where, end, todos, onShowTodo, onOpen, helpers, helpersShown, onToggleHelpers, helperActions, runs, runsShown, onToggleRuns, sides, sidesShown, onToggleSides, sideActions, said }: SessionLinesProps) {
  return (
    <>
      <div className="ca-agentrow-line1">
        <SessionText s={s} now={now} />
      </div>
      {said && <SaidText said={said} className="ca-agentrow-said" />}
      <div className="ca-lane-line2">
        <span className="ca-chips ca-lane-lead ca-agentrow-chips">
          {where}
          <HelperChip session={s} />
          <HelpersChip helpers={helpers} now={now} shown={helpersShown} onToggle={onToggleHelpers} actions={helperActions} />
          <RunsChip runs={runs} shown={runsShown} onToggle={onToggleRuns} actions={helperActions} now={now} />
          <SidesChip sides={sides} shown={sidesShown} onToggle={onToggleSides} actions={sideActions} now={now} />
          <SnoozeChip until={s.snoozedUntil} now={now} />
          <QuietChip quiet={s.quiet} />
          <SuggestedChip suggestions={s.suggestions} />
          <TodoChips todos={todos} onShow={onShowTodo} inRow />
          <InsightChips session={s} />
          <HarnessChip session={s} withStatus={false} onOpen={onOpen} inRow />
        </span>
        {end && <span className="ca-lane-right">{end}</span>}
      </div>
    </>
  );
}

interface AgentLineProps {
  item: AgentItem;
  now: number;
  domId: string;
  selected: boolean;
  onSelect: (how: SelectHow) => void;
  onOpen: () => void;
  onContextMenu?: () => void;
  /** Todos the agent's session is linked to. */
  todos?: Todo[];
  onShowTodo?: (todo: Todo) => void;
}

/** One agent on its worktree's row, on the two lines Sessions gives it (`SessionLines`) less its
 *  worktree, which the row has on its left: its status, its title and what it's doing, then its
 *  todo, Jev's read and its account's chip (double-click to open); and its age. */
function AgentLine({ item, now, domId, selected, onSelect, onOpen, onContextMenu, todos, onShowTodo }: AgentLineProps) {
  const s = item.session;
  return (
    <Row
      id={domId}
      selected={selected}
      className={cx('ca-agentline', `is-${s.status === 'error' ? 'needs' : s.status}`)}
      onSelect={(e) => onSelect(selectHow(e))}
      onOpen={(e) => {
        if (!cmd(e) && !e.shiftKey) onOpen();
      }}
      onMenu={onContextMenu}
    >
      <span className="ca-agentline-glyph">
        <SessionGlyph s={s} size={10} />
      </span>
      <span className="ca-agentline-main">
        <SessionLines s={s} now={now} todos={todos} onShowTodo={onShowTodo} onOpen={onOpen} />
      </span>
      <span className="ca-agentline-age" title={`Last active ${absolute(s.lastActivity)}. You last sent it a prompt ${agoPhrase(s.lastPromptAt, now)}, which sets its place in the list.`}>
        {ago(s.lastActivity, now)}
      </span>
    </Row>
  );
}

interface AgentRowProps {
  lane: Lane;
  session: Session;
  domId: string;
  now: number;
  /** What its age says: when it was last active, or when it started (Sessions sorted by start). */
  age?: 'active' | 'started';
  /** On one line: what it's about, then its worktree and PR. */
  slim?: boolean;
  selected: boolean;
  /** A press: alone, or with ⌘ or ⇧ held, picking it with others (picks.ts). */
  onSelect: (how: SelectHow) => void;
  onOpen: () => void;
  pinned?: boolean;
  /** Right-click: the session's menu (open, pin, archive…). */
  onContextMenu?: () => void;
  /** Right-click on its worktree's PR: the PR's menu (open it, copy its link…). */
  onPrMenu?: () => void;
  /** A click on its worktree's PR: shows it in PRs. */
  onPrShow?: () => void;
  /** A double-click on it: opens it on GitHub. */
  onPrOpen?: () => void;
  /** Dragging the row (useRowDrag.tsx): among the pinned ones, or out of the window. */
  drag?: DragProps;
  /** Todos the session is linked to. */
  todos?: Todo[];
  onShowTodo?: (todo: Todo) => void;
  /** The helpers its agent started (boardLists.ts `helperCounts`), and whether all of them are
   *  listed under it, which its chip's double-click turns on and off (`onToggleHelpers`); what
   *  their chip's card does with one. */
  helpers?: HelperCount;
  helpersShown?: boolean;
  onToggleHelpers?: () => void;
  helperActions?: HelperActions;
  /** The headless runs its commands started (boardLists.ts `runCounts`), and whether all of them
   *  are listed under it, which their chip's double-click turns on and off (`onToggleRuns`). */
  runs?: RunCount;
  runsShown?: boolean;
  onToggleRuns?: () => void;
  /** Its side conversations (boardLists.ts `sideCounts`), and whether all of them are listed
   *  under it, which their chip's double-click turns on and off (`onToggleSides`); what their
   *  chip's card does with one. */
  sides?: SideCount;
  sidesShown?: boolean;
  onToggleSides?: () => void;
  sideActions?: HelperActions;
  /** A helper or headless run listed under the session it works for: set in from the left, on
   *  one line. */
  nested?: boolean;
  /** A nested helper in another worktree than its session's, which its row names. */
  elsewhere?: boolean;
  /** What a search found it by when that's what was said in it (boardLists.ts `saidLinesOf`). */
  said?: SaidLine;
}

/** A snooze's chip: how long until it's back, as short as the age beside it, and the time and
 *  what the snooze keeps it out of (`away`) in its tooltip. A snoozed review's is this too
 *  (PrsView.tsx). */
export function SnoozeChip({ until, now, away = 'off the floating bar' }: { until: number | null | undefined; now: number; away?: string }) {
  if (until == null) return null;
  return (
    <Chip className="ca-snoozechip" title={`Snoozed until ${untilWords(until, now)}: ${away} until then, then back`}>
      <Icon name="zzz" size={12} />
      {untilShort(until, now)}
    </Chip>
  );
}

/** Quiet's chip: the session's turns don't wait on you, and its tooltip says what still does. */
export function QuietChip({ quiet }: { quiet: boolean | undefined }) {
  if (!quiet) return null;
  return (
    <Chip className="ca-quietchip" title="Quiet: its turns don’t wait on you. It does when it needs you: a question, a call to allow, an error">
      <Icon name="bell-off" size={12} />
      Quiet
    </Chip>
  );
}

/** A helper's or a headless run's chip, where it isn't under the session it works for
 *  (SessionsView.tsx): what it helps, or that it's a run, and in its tooltip why it's out of
 *  your lists. */
export function HelperChip({ session: s, nested }: { session: Session; nested?: boolean }) {
  if (nested) return null;
  if (!s.helper && s.headless) {
    return (
      <Chip className="ca-helperchip" title="A headless run: a script or an agent ran it on a prompt with nobody at it (claude -p, codex exec) and reads what comes back itself, so its turns don’t wait on you. It’s counted on the row of the session whose command ran it; it shows here when it needs you, or when no session here ran it.">
        <Icon name="robot" size={12} />
        Headless
      </Chip>
    );
  }
  if (!s.helper) return null;
  const pr = s.helper.pr?.split('#')[1];
  const [label, title] = pr
    ? [`Review of #${pr}`, `An automatic review of #${pr}: its turns are for Codealong to post, not for you, so it's listed on the PR. It shows here when it needs you.`]
    : ['Helper', 'A helper another session’s agent started: its turns are for that agent, not for you, so it’s listed under that session. It shows here when it needs you.'];
  return (
    <Chip className="ca-helperchip" title={title}>
      <Icon name="subtask" size={12} />
      {label}
    </Chip>
  );
}

/** The chip on a session's row for the headless runs its commands started (boardLists.ts
 *  `runCounts`, an eval's judge calls, say): how many, with the working glyph while any is at
 *  work. They're counted here rather than listed, a row a call. Resting the pointer on it brings
 *  up the helpers' card for them (HelpersCard.tsx, `kind` runs), each to show or open; where
 *  there's no card (`actions`), how many work is in its tooltip. A double-click lists them all
 *  under the row, or none again (`shown`). */
export function RunsChip({ runs: r, shown = false, onToggle, actions, now }: { runs: RunCount | undefined; shown?: boolean; onToggle?: () => void; actions?: HelperActions; now: number }) {
  const card = useHoverCard();
  if (!r || r.count === 0) return null;
  const what = r.count === 1 ? 'A headless run' : `${r.count} headless runs`;
  const at = r.working > 0 ? `, ${r.working} at work` : '';
  const how = onToggle ? `\nDouble-click to list ${shown ? 'none of them' : 'them all'} under it.` : '';
  const chip = (
    <Chip className={cx('ca-runschip', shown && 'is-shown')} title={actions ? undefined : `${what} its commands started${at}: claude -p or codex exec, which what ran them reads, so they're counted here rather than listed.${how}`} act={onToggle} inRow>
      <Icon name="robot" size={12} />
      {r.working > 0 && <SessionGlyph s={{ status: 'working' }} size={10} />}
      {r.count === 1 ? '1 run' : `${r.count} runs`}
    </Chip>
  );
  if (!actions) return chip;
  return (
    <span ref={(el) => void (card.anchor.current = el)} className="ca-helperschip-anchor" onMouseEnter={card.show} onMouseLeave={card.hide}>
      {chip}
      {card.open && <HelpersCard card={card} helpers={r} now={now} shown={shown} onToggle={onToggle} actions={actions} kind="runs" />}
    </span>
  );
}

/** The chip on a session's row for its side conversations (boardLists.ts `sideCounts`): how many,
 *  with the working glyph while one answers. Resting the pointer on it brings up the helpers' card
 *  for them (HelpersCard.tsx, `kind` sides), each to show above the session's box; where there's
 *  no card (`actions`), how many and whether one answers is in its tooltip. A double-click lists
 *  them all under the row, or only the one answering again (`shown`). */
export function SidesChip({ sides: c, shown = false, onToggle, actions, now }: { sides: SideCount | undefined; shown?: boolean; onToggle?: () => void; actions?: HelperActions; now: number }) {
  const card = useHoverCard();
  if (!c || c.count === 0) return null;
  const what = c.count === 1 ? 'A side conversation' : `${c.count} side conversations`;
  const at = c.working > 0 ? ', one answering' : '';
  const how = onToggle ? `\nDouble-click to list ${shown ? 'only the one answering' : 'them all'} under it.` : '';
  const chip = (
    <Chip className={cx('ca-sideschip', shown && 'is-shown')} title={actions ? undefined : `${what}${at}: questions about it that a throwaway copy answered, which the session never saw.${how}`} act={onToggle} inRow>
      <Icon name="help" size={12} />
      {c.working > 0 && <SessionGlyph s={{ status: 'working' }} size={10} />}
      {c.count}
    </Chip>
  );
  if (!actions) return chip;
  return (
    <span ref={(el) => void (card.anchor.current = el)} className="ca-helperschip-anchor" onMouseEnter={card.show} onMouseLeave={card.hide}>
      {chip}
      {card.open && <HelpersCard card={card} helpers={c} now={now} shown={shown} onToggle={onToggle} actions={actions} kind="sides" />}
    </span>
  );
}

/** How many helpers' glyphs a session's chip draws before it says how many more there are. */
const HELPER_GLYPHS = 3;

/** The chip on a session's row for its helpers (the ones its agent started, and the automatic
 *  reviews of a PR it opened): each one's status glyph, the most urgent first, in the attention
 *  color while one waits on you. Resting the pointer on it brings up its card (HelpersCard.tsx),
 *  every helper in full, to show or open; where there's no card (`actions`), each one's line is
 *  in its tooltip (lanes.ts `helperLine`). A double-click lists them all under the row, finished
 *  ones too, or only the ones at work again (`shown`). */
export function HelpersChip({ helpers: h, now, shown = false, onToggle, actions }: { helpers: HelperCount | undefined; now: number; shown?: boolean; onToggle?: () => void; actions?: HelperActions }) {
  const card = useHoverCard();
  if (!h || h.count === 0) return null;
  const more = h.count - HELPER_GLYPHS;
  const lines = h.each.map((g) => `· ${helperLine(g, now)}`).join('\n');
  const how = onToggle ? `\nDouble-click to list ${shown ? 'only the ones at work or waiting on you' : 'them all'} under it.` : '';
  const chip = (
    <Chip className={cx('ca-helperschip', h.waiting > 0 && 'is-waiting', shown && 'is-shown')} title={actions ? undefined : `${h.count === 1 ? 'Its helper' : `Its ${h.count} helpers`}:\n${lines}${how}`} act={onToggle} inRow>
      <Icon name="subtask" size={12} />
      <span className="ca-helperschip-glyphs">
        {h.each.slice(0, HELPER_GLYPHS).map((g) => (
          <SessionGlyph key={g.key} s={g} size={10} />
        ))}
      </span>
      {more > 0 && `+${more}`}
    </Chip>
  );
  if (!actions) return chip;
  return (
    <span ref={(el) => void (card.anchor.current = el)} className="ca-helperschip-anchor" onMouseEnter={card.show} onMouseLeave={card.hide}>
      {chip}
      {card.open && <HelpersCard card={card} helpers={h} now={now} shown={shown} onToggle={onToggle} actions={actions} />}
    </span>
  );
}

/** A chip for the replies another session suggested you send it, which its box shows: its tooltip
 *  says who suggested them and what they are. */
export function SuggestedChip({ suggestions: g }: { suggestions: Suggestions | undefined }) {
  const picks = Object.entries(g?.picks ?? {});
  if (!g || (g.replies.length === 0 && picks.length === 0)) return null;
  const cut = (t: string) => (t.length > 80 ? `${t.slice(0, 79)}…` : t);
  const by = g.byTitle ? `Suggested by ${g.byTitle}` : 'Suggested';
  // Replies to send from its box, or choices picked in its question box.
  const title = g.replies.length ? `${by}, to send from its box:\n${g.replies.map((r) => `· ${cut(r)}`).join('\n')}` : `${by}, picked to answer its ${picks.length > 1 ? 'questions' : 'question'} with:\n${picks.map(([q, labels]) => `· ${cut(q)} ${labels.join(', ')}${g.why?.[q] ? ` (${g.why[q]})` : ''}`).join('\n')}`;
  return (
    <Chip className="ca-suggestedchip" title={title}>
      <Icon name="sparkles" size={12} />
      Suggested
    </Chip>
  );
}

/**
 * One session in the Sessions view. On two lines: what it's about first (its title, what it waits
 * on you for, and what it's doing or said last), then where and why (its worktree, todo and
 * insights, its account, and the worktree's PR and diff). On one line (`slim`): what it's about,
 * then its status, worktree and PR chips at the line's end, the worktree giving way before the
 * status chips. A helper under the session it works for is on one line either way, and says only
 * what the session's row doesn't: its worktree only when it works in another, and no PR or diff.
 * A search that found it by what was said in it adds the line that has the words, under the
 * first.
 */
function AgentRowView({ lane, session: s, domId, now, age, slim = false, selected, onSelect, onOpen, pinned, onContextMenu, onPrMenu, onPrShow, onPrOpen, drag, todos, onShowTodo, helpers, helpersShown, onToggleHelpers, helperActions, runs, runsShown, onToggleRuns, sides, sidesShown, onToggleSides, sideActions, nested, elsewhere, said }: AgentRowProps) {
  const started = age === 'started';
  const oneLine = slim || nested;
  const pr = lane.pr && <PrChip pr={lane.pr} compact onMenu={onPrMenu} onShow={onPrShow} onOpen={onPrOpen} inRow />;
  return (
    <Row
      id={domId}
      selected={selected}
      className={cx('ca-lane', 'is-item', 'is-agent', oneLine && 'is-slim', `is-${s.status === 'error' ? 'needs' : s.status}`, s.snoozedUntil != null && 'is-snoozed', pinned && 'is-pinned', away(lane) && 'is-away', nested && 'is-nested')}
      drag={drag}
      // ⌘ adds it to the ones picked or leaves it out, ⇧ picks the rows up to it.
      onSelect={(e) => onSelect(selectHow(e))}
      onOpen={(e) => {
        if (!cmd(e) && !e.shiftKey) onOpen();
      }}
      onMenu={onContextMenu}
    >
      {/* One line leaves the account out: the status says it when pointed at. */}
      <div className="ca-seg ca-seg-glyph" title={oneLine ? `${STATUS_WORD[s.status]} · ${s.account}` : undefined}>
        <SessionGlyph s={s} />
      </div>
      {oneLine ? (
        <div className="ca-seg ca-seg-main">
          <div className="ca-agentrow-line1">
            <SessionText s={s} now={now} flag={false} />
            <span className="ca-chips ca-lane-tags">
              <FlagChip session={s} />
              <HelperChip session={s} nested={nested} />
              <HelpersChip helpers={helpers} now={now} shown={helpersShown} onToggle={onToggleHelpers} actions={helperActions} />
              <RunsChip runs={runs} shown={runsShown} onToggle={onToggleRuns} actions={helperActions} now={now} />
              <SidesChip sides={sides} shown={sidesShown} onToggle={onToggleSides} actions={sideActions} now={now} />
              <SnoozeChip until={s.snoozedUntil} now={now} />
              <QuietChip quiet={s.quiet} />
              <SuggestedChip suggestions={s.suggestions} />
              <InsightChips session={s} />
              {(!nested || elsewhere) && <WorktreeChip lane={lane} short />}
            </span>
            {!nested && pr}
          </div>
          {said && <SaidText said={said} className="ca-agentrow-said" />}
        </div>
      ) : (
        <div className="ca-seg ca-seg-main">
          <SessionLines
            s={s}
            now={now}
            where={<WorktreeChip lane={lane} />}
            end={
              <>
                {pr}
                <Diff git={lane.git} />
              </>
            }
            todos={todos}
            onShowTodo={onShowTodo}
            onOpen={onOpen}
            helpers={helpers}
            helpersShown={helpersShown}
            onToggleHelpers={onToggleHelpers}
            helperActions={helperActions}
            runs={runs}
            runsShown={runsShown}
            onToggleRuns={onToggleRuns}
            sides={sides}
            sidesShown={sidesShown}
            onToggleSides={onToggleSides}
            sideActions={sideActions}
            said={said}
          />
        </div>
      )}
      <div
        className="ca-seg ca-seg-age"
        title={
          started
            ? `Started ${absolute(startedOf(s))}, which sets its place in the list. Last active ${agoPhrase(s.lastActivity, now)}.`
            : `Last active ${absolute(s.lastActivity)}. You last sent it a prompt ${agoPhrase(s.lastPromptAt, now)}, which sets its place in the list.`
        }
      >
        {ago(started ? startedOf(s) : s.lastActivity, now)}
      </div>
    </Row>
  );
}

export const AgentRow = memoRow(AgentRowView);

interface SideRowProps {
  /** The session it's about. */
  sessionKey: string;
  side: SideGlance;
  now: number;
  /** Shows it: its session selected, and it above the session's box. */
  onShow: (sessionKey: string, side: SideGlance) => void;
}

/** A side conversation listed under its session (lanes.ts `sidesListed`), as a helper is: on
 *  one line, its status (answering, or done), what it was first asked and how many questions it's
 *  had, and when it was last asked. It's no session of its own, so a press shows it above its
 *  session's box, its session selected; it has no menu and isn't picked or dragged. */
function SideRowView({ sessionKey, side: g, now, onShow }: SideRowProps) {
  const asked = g.asked === 1 ? '1 question' : `${g.asked} questions`;
  return (
    <Row
      label={`On the side: ${sideTitle(g)}`}
      className={cx('ca-lane', 'is-item', 'is-agent', 'is-slim', 'is-nested', 'is-side', g.running ? 'is-working' : 'is-idle')}
      onSelect={(e) => {
        if (!cmd(e) && !e.shiftKey) onShow(sessionKey, g);
      }}
    >
      <div className="ca-seg ca-seg-glyph" title={g.running ? 'Answering' : 'Answered'}>
        <SessionGlyph s={{ status: g.running ? 'working' : 'idle' }} />
      </div>
      <div className="ca-seg ca-seg-main">
        <div className="ca-agentrow-line1" title={`On the side: ${g.question}\nA throwaway copy of the session's conversation ${g.running ? 'is answering' : 'answered'}; the session never sees it. Click to show it above the session's box.`}>
          <Icon name="help" size={14} />
          <span className="ca-agentline-title">{sideTitle(g)}</span>
          <span className="ca-sideline-meta">{g.running ? `answering · ${asked}` : asked}</span>
        </div>
      </div>
      <div className="ca-seg ca-seg-age" title={`Last asked ${absolute(g.lastAt)}`}>
        {ago(g.lastAt, now)}
      </div>
    </Row>
  );
}

export const SideRow = memoRow(SideRowView);

/** The end of what's listed under a session where you asked to see every one of a kind (`Fold`,
 *  boardLists.ts `foldsOf`): a click lists only the ones at work again, as the kind's chip's
 *  double-click does, so the way back is in sight. `head` is whether it's for the session at the
 *  family's head, which needs no name. */
export function FoldRow({ fold, head, onFold }: { fold: Fold; head: boolean; onFold: (fold: Fold) => void }) {
  return (
    <div className="ca-lane is-item is-agent is-slim is-nested is-fold" role="presentation">
      <div className="ca-seg ca-seg-glyph" aria-hidden="true">
        <Icon name="chevron-up" size={12} />
      </div>
      <div className="ca-seg ca-seg-main">
        <button type="button" className="ca-linkbtn ca-foldrow-btn" onClick={() => onFold(fold)}>
          {foldWords(fold, head)}
        </button>
      </div>
    </div>
  );
}

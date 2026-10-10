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
import { branchLabel, startedOf, worktreeName, type AgentItem, type WorktreeRow } from '../lanes';
import { selectHow, type SelectHow } from '../picks';
import { untilWords } from '../snooze';
import type { GitInfo, Lane, Session, Suggestions, Todo } from '../types';
import { Chip, cx, driftWords, HarnessChip, Icon, LaneGlyph, PrChip, Rich, Row, selects, STATUS_WORD, SessionGlyph, WorktreeChip } from './primitives';
import type { DragProps } from './useRowDrag';
import { FlagChip, InsightChips, LooksFinished } from './InsightBits';
import { PreviewChip } from './PreviewBits';
import { TodoChips } from './TodoBits';

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
  /** A click on its PR: opens it on GitHub. */
  onPrOpen?: () => void;
  /** Dragging the row (useRowDrag.tsx): among the pinned ones, or out of the window. */
  drag?: DragProps;
  /** Todos a session is linked to, for its line. */
  todosFor?: (sessionKey: string) => Todo[] | undefined;
  onShowTodo?: (todo: Todo) => void;
}

/** `atlas · quarry`: the repo, and the branch when the worktree's name doesn't already say it. */
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
  onPrOpen?: () => void;
}

/**
 * The worktree's column, with what belongs to the worktree at the end of each of its lines: its
 * name, then finished as Jev reads it and its app (a globe, green while it runs); its repo and
 * branch, after its reuse mark (green when new work can start there), then its PR. Each line is laid from its end,
 * so apps and PRs line up down the board. Short of room, the name keeps its line and the chips
 * beside it give way, the app last; the repo and branch give way before the PR does.
 */
function WorktreeCell({ lane, now, titled, onOpenTarget, onPrMenu, onPrOpen }: CellProps) {
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
        <span className="ca-chips ca-lane-own">{lane.pr && <PrChip pr={lane.pr} compact onMenu={onPrMenu} onOpen={onPrOpen} inRow />}</span>
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

function PinMark() {
  return (
    <span className="ca-pinmark" title="Pinned: drag, or press ⌃⌘↑ ⌃⌘↓, to reorder. Drag it down to the list to unpin it.">
      <Icon name="pinned" size={12} />
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
function EmptyRow({ row, now, selectedKey, picked = false, domId, onSelect, onOpen, onOpenTarget, pinned, onContextMenu, onPrMenu, onPrOpen, drag }: LaneRowProps) {
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
        if (!e.metaKey && !e.shiftKey) onOpen(lane);
      }}
      onMenu={onContextMenu}
    >
      <div className="ca-seg ca-seg-glyph">
        <LaneGlyph lane={lane} />
      </div>
      <div className="ca-seg ca-seg-call">
        <WorktreeCell lane={lane} now={now} titled onOpenTarget={onOpenTarget} onPrMenu={onPrMenu} onPrOpen={onPrOpen} />
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
            {pinned && <PinMark />}
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
function GroupRow({ row, now, selectedKey, picked = false, domId, onSelect, onOpen, onOpenTarget, pinned, onContextMenu, onAgentMenu, onPrMenu, onPrOpen, drag, todosFor, onShowTodo }: LaneRowProps) {
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
        <WorktreeCell lane={lane} now={now} titled={false} onOpenTarget={onOpenTarget} onPrMenu={onPrMenu} onPrOpen={onPrOpen} />
      </div>
      <DeltaSeg git={lane.git} onMouseDown={pressFirst} onDoubleClick={() => onOpen(lane)} />
      <div className="ca-seg-lines">
        {lines.map((a, i) => (
          <AgentLine
            key={a.key}
            item={a}
            now={now}
            domId={domId(a.key)}
            selected={isSelected(a.key)}
            pinned={pinned === true && i === 0}
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
 * A session's line: its title first, then a label for what it waits on you for, then what it's
 * doing or said last, which gives way first when the line is short. A session another worktree
 * has, on this one's row, says what it did here first: made it, or worked in it.
 */
function SessionText({ s, now }: { s: Session; now: number }) {
  return (
    <span className="ca-agentline-text">
      <span className="ca-lane-headline ca-agentline-title">
        <Rich text={s.title} now={now} />
      </span>
      <FlagChip session={s} />
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
}

/**
 * A session on two lines, wherever a list gives it two: what it's about first (its title, what it
 * waits on you for, and what it's doing or said last), then where and why (`where`, its snooze,
 * quiet, todo and Jev's read, then its account), with `end` at the second line's end.
 */
function SessionLines({ s, now, where, end, todos, onShowTodo, onOpen }: SessionLinesProps) {
  return (
    <>
      <div className="ca-agentrow-line1">
        <SessionText s={s} now={now} />
      </div>
      <div className="ca-lane-line2">
        <span className="ca-chips ca-lane-lead ca-agentrow-chips">
          {where}
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
  /** Shows the pin mark under its age: the first line of a pinned worktree. */
  pinned: boolean;
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
function AgentLine({ item, now, domId, selected, pinned, onSelect, onOpen, onContextMenu, todos, onShowTodo }: AgentLineProps) {
  const s = item.session;
  return (
    <Row
      id={domId}
      selected={selected}
      className={cx('ca-agentline', `is-${s.status === 'error' ? 'needs' : s.status}`)}
      onSelect={(e) => onSelect(selectHow(e))}
      onOpen={(e) => {
        if (!e.metaKey && !e.shiftKey) onOpen();
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
        {pinned && <PinMark />}
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
  /** A click on its worktree's PR: opens it on GitHub. */
  onPrOpen?: () => void;
  /** Dragging the row (useRowDrag.tsx): among the pinned ones, or out of the window. */
  drag?: DragProps;
  /** Todos the session is linked to. */
  todos?: Todo[];
  onShowTodo?: (todo: Todo) => void;
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

/** A chip for the replies another session suggested you send it, which its box shows: its tooltip
 *  says who suggested them and what they are. */
export function SuggestedChip({ suggestions: g }: { suggestions: Suggestions | undefined }) {
  if (!g || g.replies.length === 0) return null;
  const replies = g.replies.map((r) => `· ${r.length > 80 ? `${r.slice(0, 79)}…` : r}`).join('\n');
  return (
    <Chip className="ca-suggestedchip" title={`${g.byTitle ? `Suggested by ${g.byTitle}` : 'Suggested replies'}, to send from its box:\n${replies}`}>
      <Icon name="sparkles" size={12} />
      Suggested
    </Chip>
  );
}

/**
 * One session in the Sessions view. On two lines: what it's about first (its title, what it waits
 * on you for, and what it's doing or said last), then where and why (its worktree, todo and
 * insights, its account, and the worktree's PR and diff). On one line (`slim`): what it's about,
 * then its worktree and PR at the line's end.
 */
function AgentRowView({ lane, session: s, domId, now, age, slim = false, selected, onSelect, onOpen, pinned, onContextMenu, onPrMenu, onPrOpen, drag, todos, onShowTodo }: AgentRowProps) {
  const started = age === 'started';
  const pr = lane.pr && <PrChip pr={lane.pr} compact onMenu={onPrMenu} onOpen={onPrOpen} inRow />;
  return (
    <Row
      id={domId}
      selected={selected}
      className={cx('ca-lane', 'is-agent', slim && 'is-slim', `is-${s.status === 'error' ? 'needs' : s.status}`, s.snoozedUntil != null && 'is-snoozed', pinned && 'is-pinned', away(lane) && 'is-away')}
      drag={drag}
      // ⌘ adds it to the ones picked or leaves it out, ⇧ picks the rows up to it.
      onSelect={(e) => onSelect(selectHow(e))}
      onOpen={(e) => {
        if (!e.metaKey && !e.shiftKey) onOpen();
      }}
      onMenu={onContextMenu}
    >
      {/* One line leaves the account out: the status says it when pointed at. */}
      <div className="ca-seg ca-seg-glyph" title={slim ? `${STATUS_WORD[s.status]} · ${s.account}` : undefined}>
        <SessionGlyph s={s} />
      </div>
      {slim ? (
        <div className="ca-seg ca-seg-main">
          <div className="ca-agentrow-line1">
            <SessionText s={s} now={now} />
            <span className="ca-chips ca-agentline-tags">
              <SnoozeChip until={s.snoozedUntil} now={now} />
              <QuietChip quiet={s.quiet} />
              <SuggestedChip suggestions={s.suggestions} />
              <InsightChips session={s} />
              <WorktreeChip lane={lane} short />
            </span>
            {pr}
          </div>
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
        {pinned && <PinMark />}
      </div>
    </Row>
  );
}

export const AgentRow = memoRow(AgentRowView);

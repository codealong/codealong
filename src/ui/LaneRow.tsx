// One worktree as a flight strip: status; the worktree, with its repo and branch; its lines and
// commits against the default branch; what's happening, its agents, todo, app and PR; age. Each agent that needs you, is working or just finished gets a line of its own
// on its worktree's row, so a worktree shows once however many agents run in it.

import { deviceLabel } from '../devices';
import { REUSE_WORDS } from '../reusable';
import { DeviceBadge } from './DeviceBadge';
import { memoRow } from './memoRow';
import type { MouseEvent as ReactMouseEvent, PointerEvent as ReactPointerEvent } from 'react';
import { absolute, ago, agoPhrase, count, middleTruncate, untilShort } from '../format';
import { startedOf, worktreeName, type AgentItem, type WorktreeRow } from '../lanes';
import { untilWords } from '../snooze';
import type { GitInfo, Lane, Session, Todo } from '../types';
import { driftWords, HarnessChip, Icon, LaneGlyph, PrChip, Rich, StatusGlyph } from './primitives';
import { FlagChip, InsightChips, LooksFinished } from './InsightBits';
import { PreviewChip } from './PreviewBits';
import { TodoChips } from './TodoBits';

interface LaneRowProps {
  row: WorktreeRow;
  now: number;
  /** The selected item: one of the row's agents, or the worktree when it has none. */
  selectedKey: string | null;
  /** The element id of an item, for the list's active descendant. */
  domId: (key: string) => string;
  onSelect: (key: string) => void;
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
  /** Position among pinned rows, for dragging them into a new order. */
  pinIndex?: number;
  /** Starts a drag of the row (useRowDrag.tsx): among the pinned ones, or out of the window. */
  onPointerDown?: (e: ReactPointerEvent<HTMLDivElement>) => void;
  extraClass?: string;
  /** Todos the worktree or its sessions are linked to: a row without agent lines shows them. */
  todos?: Todo[];
  /** Todos linked to the worktree itself: a row with agent lines shows them at its foot. */
  worktreeTodos?: Todo[];
  /** Todos a session is linked to, for its line. */
  todosFor?: (sessionKey: string) => Todo[] | undefined;
  onShowTodo?: (todo: Todo) => void;
}

const DAY = 86_400_000;

/** Sessions worth showing at a glance: live ones, ones with news, and anything from the last
 *  day, but not a reusable worktree's from before its latest work. The inspector lists every
 *  session from the last 14 days. */
export function currentSessions(lane: Lane, now: number) {
  return lane.sessions.filter((s) => !s.earlier && (s.live || s.status === 'needs' || s.status === 'error' || s.status === 'working' || s.status === 'unread' || now - s.lastActivity < DAY));
}

/** `atlas · quarry`: the repo, and the branch when the worktree's name doesn't already say it. */
function laneSub(lane: Lane): { sub: string; branch: string | null } {
  const branch = lane.branch ?? (lane.detached ? `detached @ ${lane.detached}` : null);
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
function DeltaSeg({ git: g, onMouseDown, onDoubleClick }: { git: GitInfo | null; onMouseDown?: () => void; onDoubleClick?: () => void }) {
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
          <span className="ca-delta-row is-lines">
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

/** The worktree's column: its name, and its repo and branch. `titled` gives the name a tooltip
 *  of its own, where the column has none. */
function WorktreeCell({ lane, now, titled }: { lane: Lane; now: number; titled: boolean }) {
  const { sub, branch } = laneSub(lane);
  return (
    <>
      <span className="ca-lane-call" title={titled ? (lane.origin === 'main' ? `The main worktree of ${lane.repo}` : lane.callsign) : undefined}>
        {worktreeName(lane)}
      </span>
      <span className="ca-lane-sub" title={branch ?? lane.path}>
        {lane.device && <DeviceBadge device={lane.device} now={now} />}
        <ReuseMark lane={lane} />
        {sub}
      </span>
    </>
  );
}

/** A reusable worktree's mark, before its repo and branch: it takes one piece of work after
 *  another. Its tooltip says what it holds now. */
export function ReuseMark({ lane }: { lane: Lane }) {
  if (!lane.reuse) return null;
  return (
    <span className="ca-reusemark" title={`Reusable. ${REUSE_WORDS[lane.reuse]}`}>
      <Icon name="repeat" size={11} />
    </span>
  );
}

/** Free, on a reusable worktree that holds no work: new work can start there. */
function FreeChip({ lane }: { lane: Lane }) {
  if (lane.reuse !== 'free') return null;
  return (
    <span className="ca-chip ca-reusechip" title={REUSE_WORDS.free}>
      Free
    </span>
  );
}

/**
 * Recent sessions as chips that open them: `max` at most, one on a narrow board, then how many
 * more (the panel lists them all).
 */
function SessionChips({ sessions, max, onOpen }: { sessions: Session[]; max: number; onOpen: (s: Session) => void }) {
  const more = (from: number, cls: string) =>
    sessions.length > from && (
      <span className={`ca-chip ca-chip--more ${cls}`} title={sessions.slice(from).map((s) => `${s.account} · ${s.title}`).join('\n')}>
        +{sessions.length - from}
      </span>
    );
  return (
    <>
      {sessions.slice(0, max).map((s, i) => (
        <HarnessChip key={s.key} session={s} className={i > 0 ? 'is-extra' : undefined} onOpen={() => onOpen(s)} inRow />
      ))}
      {more(max, 'is-wide')}
      {max > 1 && more(1, 'is-narrow')}
    </>
  );
}

function PinMark() {
  return (
    <span className="ca-pinmark" title="Pinned: drag, or press ⌃⌘↑ ⌃⌘↓, to reorder. Drag it down to the list to unpin it.">
      <Icon name="pinned" size={12} />
    </span>
  );
}

/** Right-click handler that shows our menu instead of the webview's. */
function menu(show: (() => void) | undefined) {
  return show
    ? (e: { preventDefault: () => void; stopPropagation: () => void }) => {
        e.preventDefault();
        e.stopPropagation();
        show();
      }
    : undefined;
}

function LaneRowView(props: LaneRowProps) {
  return props.row.agents.length > 0 ? <GroupRow {...props} /> : <WorktreeOnlyRow {...props} />;
}

// `domId` and `todosFor` answer while the row renders, so they're compared as values.
export const LaneRow = memoRow(LaneRowView, ['domId', 'todosFor']);

/** A worktree with no agent that needs you, is working or just finished: its sentence, and chips for its recent sessions. */
function WorktreeOnlyRow({ row, now, selectedKey, domId, onSelect, onOpen, onOpenTarget, pinned, onContextMenu, onPrMenu, onPrOpen, pinIndex, onPointerDown, extraClass, todos, onShowTodo }: LaneRowProps) {
  const { lane } = row;
  const chips = currentSessions(lane, now);
  const selected = selectedKey === lane.id;
  return (
    <div
      id={domId(lane.id)}
      role="option"
      aria-selected={selected}
      className={['ca-lane', 'is-worktree', `is-${lane.bay}`, selected && 'is-selected', pinned && 'is-pinned', away(lane) && 'is-away', extraClass].filter(Boolean).join(' ')}
      data-pin-index={pinIndex}
      onPointerDown={onPointerDown}
      onMouseDown={() => onSelect(lane.id)}
      onDoubleClick={() => onOpen(lane)}
      onContextMenu={menu(onContextMenu)}
    >
      {onPointerDown && (
        <span className="ca-grip" aria-hidden="true">
          <Icon name="grip-vertical" size={10} />
        </span>
      )}
      <div className="ca-seg ca-seg-glyph">
        <LaneGlyph lane={lane} />
      </div>
      <div className="ca-seg ca-seg-call">
        <WorktreeCell lane={lane} now={now} titled />
      </div>
      <DeltaSeg git={lane.git} />
      <div className="ca-seg ca-seg-main">
        <div className="ca-lane-line1">
          <span className="ca-lane-headline">
            <Rich text={lane.headline} now={now} />
          </span>
          {lane.detail && (
            <span className="ca-lane-detail">
              <Rich text={lane.detail} now={now} />
            </span>
          )}
        </div>
        <div className="ca-lane-line2">
          <span className="ca-lane-lead">
            <SessionChips
              sessions={chips}
              max={2}
              onOpen={(s) => {
                onSelect(lane.id);
                onOpenTarget(lane, s.openTarget);
              }}
            />
            {chips.length === 0 && lane.reuse !== 'free' && (
              <span className="ca-none" title={lane.sessions.length ? `${count(lane.sessions.length, 'session')} before today. The panel lists them.` : undefined}>
                {lane.sessions.length ? 'No agents today' : 'No agents'}
              </span>
            )}
            <FreeChip lane={lane} />
            <TodoChips todos={todos} onShow={onShowTodo} inRow />
            <LooksFinished lane={lane} />
          </span>
          <LaneLinks lane={lane} now={now} onOpenTarget={onOpenTarget} onPrMenu={onPrMenu} onPrOpen={onPrOpen} />
        </div>
      </div>
      <div className="ca-seg ca-seg-age" title={lane.lastActivity ? absolute(lane.lastActivity) : undefined}>
        {lane.lastActivity ? ago(lane.lastActivity, now) : ''}
        {pinned && <PinMark />}
      </div>
    </div>
  );
}

/** The worktree's app and PR, at the right end of its line, after its agents and todo. */
function LaneLinks({ lane, now, onOpenTarget, onPrMenu, onPrOpen }: { lane: Lane; now: number; onOpenTarget: (lane: Lane, targetId: string) => void; onPrMenu?: () => void; onPrOpen?: () => void }) {
  return (
    <>
      <PreviewChip preview={lane.preview} now={now} onOpen={() => onOpenTarget(lane, 'l|preview')} />
      {lane.pr && (
        <span className="ca-lane-right">
          <PrChip pr={lane.pr} compact onMenu={onPrMenu} onOpen={onPrOpen} inRow />
        </span>
      )}
    </>
  );
}

/**
 * A worktree with agents that need you, are working or just finished: the worktree once on the
 * left, and a line for each agent, which you select, open and right-click like a row of its own.
 */
function GroupRow({ row, now, selectedKey, domId, onSelect, onOpen, onOpenTarget, pinned, onContextMenu, onAgentMenu, onPrMenu, onPrOpen, pinIndex, onPointerDown, extraClass, worktreeTodos, todosFor, onShowTodo }: LaneRowProps) {
  const { lane, agents } = row;
  const listed = new Set(agents.map((a) => a.session.key));
  // Recent sessions without a line of their own: finished a while ago, or visiting.
  const others = currentSessions(lane, now).filter((s) => !listed.has(s.key));
  const hasSelection = agents.some((a) => a.key === selectedKey);
  // The worktree's own todos, less the ones its agents' lines already show.
  const onLines = new Set(agents.flatMap((a) => todosFor?.(a.session.key) ?? []));
  const footTodos = (worktreeTodos ?? []).filter((t) => !onLines.has(t));
  const foot = others.length > 0 || footTodos.length > 0 || lane.pr != null || lane.preview != null || lane.reuse === 'free';
  const selectFirst = () => onSelect(agents[0].key);
  return (
    <div
      role="group"
      aria-label={`${lane.callsign}, ${agents.length} ${agents.length === 1 ? 'agent' : 'agents'}`}
      className={['ca-lane', 'is-worktree', 'is-group', `is-${lane.bay}`, agents.length > 1 && 'is-tall', hasSelection && 'has-selection', pinned && 'is-pinned', away(lane) && 'is-away', extraClass]
        .filter(Boolean)
        .join(' ')}
      data-pin-index={pinIndex}
      onPointerDown={onPointerDown}
      onContextMenu={menu(onContextMenu)}
    >
      {onPointerDown && (
        <span className="ca-grip" aria-hidden="true">
          <Icon name="grip-vertical" size={10} />
        </span>
      )}
      <div className="ca-seg ca-seg-glyph" onMouseDown={selectFirst} onDoubleClick={() => onOpen(lane)}>
        <LaneGlyph lane={lane} />
      </div>
      <div className="ca-seg ca-seg-call" onMouseDown={selectFirst} onDoubleClick={() => onOpen(lane)} title={`${lane.path}. Double-click to open its most recent session.`}>
        <WorktreeCell lane={lane} now={now} titled={false} />
      </div>
      <DeltaSeg git={lane.git} onMouseDown={selectFirst} onDoubleClick={() => onOpen(lane)} />
      <div className="ca-seg-lines">
        {agents.map((a, i) => (
          <AgentLine
            key={a.key}
            item={a}
            now={now}
            domId={domId(a.key)}
            selected={a.key === selectedKey}
            pinned={pinned === true && i === 0}
            onSelect={() => onSelect(a.key)}
            onOpen={() => onOpenTarget(lane, a.session.openTarget)}
            onContextMenu={onAgentMenu ? () => onAgentMenu(a.session) : undefined}
            todos={todosFor?.(a.session.key)}
            onShowTodo={onShowTodo}
          />
        ))}
        {foot && (
          <div className="ca-agentline ca-agentline--foot" onMouseDown={selectFirst} onDoubleClick={() => onOpen(lane)}>
            <span className="ca-agentline-main">
              <span className="ca-lane-lead">
                <SessionChips
                  sessions={others}
                  max={3}
                  onOpen={(s) => {
                    selectFirst();
                    onOpenTarget(lane, s.openTarget);
                  }}
                />
                <TodoChips todos={footTodos} onShow={onShowTodo} inRow />
                <FreeChip lane={lane} />
              </span>
              <LaneLinks lane={lane} now={now} onOpenTarget={onOpenTarget} onPrMenu={onPrMenu} onPrOpen={onPrOpen} />
            </span>
            <span className="ca-agentline-age" />
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * A session's line: its title first, then a label for what it waits on you for, then what it's
 * doing or said last, which gives way first when the line is short.
 */
function SessionText({ s, now }: { s: Session; now: number }) {
  return (
    <span className="ca-agentline-text">
      <span className="ca-lane-headline ca-agentline-title">
        <Rich text={s.title} now={now} />
      </span>
      <FlagChip session={s} />
      {s.detail && (
        <span className="ca-lane-detail ca-agentline-detail">
          <Rich text={s.detail} now={now} />
        </span>
      )}
    </span>
  );
}

interface AgentLineProps {
  item: AgentItem;
  now: number;
  domId: string;
  selected: boolean;
  /** Shows the pin mark under its age: the first line of a pinned worktree. */
  pinned: boolean;
  onSelect: () => void;
  onOpen: () => void;
  onContextMenu?: () => void;
  /** Todos the agent's session is linked to. */
  todos?: Todo[];
  onShowTodo?: (todo: Todo) => void;
}

/** One agent on its worktree's row: its chip (double-click to open), its title and what it's doing, its
 *  todo and Jev's read at the end of the line, and its age. */
function AgentLine({ item, now, domId, selected, pinned, onSelect, onOpen, onContextMenu, todos, onShowTodo }: AgentLineProps) {
  const s = item.session;
  return (
    <div
      id={domId}
      role="option"
      aria-selected={selected}
      className={['ca-agentline', `is-${s.status === 'error' ? 'needs' : s.status}`, selected && 'is-selected'].filter(Boolean).join(' ')}
      onMouseDown={onSelect}
      onDoubleClick={onOpen}
      onContextMenu={menu(onContextMenu)}
    >
      <span className="ca-agentline-main">
        <HarnessChip session={s} onOpen={onOpen} inRow />
        <SessionText s={s} now={now} />
        <span className="ca-agentline-tags">
          <TodoChips todos={todos} onShow={onShowTodo} inRow />
          <InsightChips session={s} />
        </span>
      </span>
      <span className="ca-agentline-age" title={`Last active ${absolute(s.lastActivity)}. You last sent it a prompt ${agoPhrase(s.lastPromptAt, now)}, which sets its place in the list.`}>
        {ago(s.lastActivity, now)}
        {pinned && <PinMark />}
      </span>
    </div>
  );
}

interface AgentRowProps {
  lane: Lane;
  session: Session;
  domId: string;
  now: number;
  /** What its age says: when it was last active, or when it started (Sessions sorted by start). */
  age?: 'active' | 'started';
  selected: boolean;
  onSelect: (e: ReactMouseEvent) => void;
  onOpen: () => void;
  pinned?: boolean;
  /** Right-click: the session's menu (open, pin, archive…). */
  onContextMenu?: () => void;
  /** Right-click on its worktree's PR: the PR's menu (open it, copy its link…). */
  onPrMenu?: () => void;
  /** A click on its worktree's PR: opens it on GitHub. */
  onPrOpen?: () => void;
  /** Position among pinned rows, for dragging them into a new order. */
  pinIndex?: number;
  /** Starts a drag of the row (useRowDrag.tsx): among the pinned ones, or out of the window. */
  onPointerDown?: (e: ReactPointerEvent<HTMLDivElement>) => void;
  extraClass?: string;
  /** Todos the session is linked to. */
  todos?: Todo[];
  onShowTodo?: (todo: Todo) => void;
}

/** Where a session runs, as a chip: its worktree, after its repo when the worktree has a name of
 *  its own, and after its device, in that device's color, when it's on another of yours. */
function WorktreeChip({ lane }: { lane: Lane }) {
  const { branch } = laneSub(lane);
  const name = worktreeName(lane);
  const own = name !== lane.repo;
  return (
    <span className="ca-chip ca-wtchip" title={`${lane.device ? `On ${lane.device.name}: ` : ''}${lane.path}${branch ? `\nOn ${branch}` : ''}`}>
      <Icon name="folder" size={12} />
      {lane.device && (
        <span className="ca-wtchip-part ca-wtchip-device">
          <i className={`ca-labeldot is-${lane.device.color}`} aria-hidden="true" />
          {deviceLabel(lane.device.name)}
        </span>
      )}
      {own && <span className="ca-wtchip-part">{lane.repo}</span>}
      <span className="ca-wtchip-name">{name}</span>
    </span>
  );
}

/** A snoozed session's chip: how long until it's back on the floating bar, as short as the age
 *  beside it, and the time in its tooltip. */
function SnoozeChip({ session: s, now }: { session: Session; now: number }) {
  if (s.snoozedUntil == null) return null;
  return (
    <span className="ca-chip ca-snoozechip" title={`Snoozed until ${untilWords(s.snoozedUntil, now)}: off the floating bar until then, then back`}>
      <Icon name="zzz" size={12} />
      {untilShort(s.snoozedUntil, now)}
    </span>
  );
}

/**
 * One session in the Sessions view: who and what on the first line (its agent, its title, what it
 * waits on you for, and what it's doing or said last), where and why on the second (its worktree,
 * todo and insights, and the worktree's PR and diff).
 */
function AgentRowView({ lane, session: s, domId, now, age, selected, onSelect, onOpen, pinned, onContextMenu, onPrMenu, onPrOpen, pinIndex, onPointerDown, extraClass, todos, onShowTodo }: AgentRowProps) {
  const started = age === 'started';
  return (
    <div
      id={domId}
      role="option"
      aria-selected={selected}
      className={['ca-lane', 'is-agent', `is-${s.status === 'error' ? 'needs' : s.status}`, s.snoozedUntil != null && 'is-snoozed', selected && 'is-selected', pinned && 'is-pinned', away(lane) && 'is-away', extraClass]
        .filter(Boolean)
        .join(' ')}
      data-pin-index={pinIndex}
      onPointerDown={onPointerDown}
      onMouseDown={onSelect}
      onDoubleClick={onOpen}
      onContextMenu={menu(onContextMenu)}
    >
      {onPointerDown && (
        <span className="ca-grip" aria-hidden="true">
          <Icon name="grip-vertical" size={10} />
        </span>
      )}
      <div className="ca-seg ca-seg-glyph">
        <StatusGlyph status={s.status} />
      </div>
      <div className="ca-seg ca-seg-main">
        <div className="ca-agentrow-line1">
          <HarnessChip session={s} withStatus={false} onOpen={onOpen} inRow />
          <SessionText s={s} now={now} />
        </div>
        <div className="ca-lane-line2">
          <span className="ca-lane-agents">
            <WorktreeChip lane={lane} />
            <SnoozeChip session={s} now={now} />
            <TodoChips todos={todos} onShow={onShowTodo} inRow />
            <InsightChips session={s} />
          </span>
          <span className="ca-lane-right">
            {lane.pr && <PrChip pr={lane.pr} compact onMenu={onPrMenu} onOpen={onPrOpen} inRow />}
            <Diff git={lane.git} />
          </span>
        </div>
      </div>
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
    </div>
  );
}

export const AgentRow = memoRow(AgentRowView);

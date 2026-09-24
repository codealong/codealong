// One worktree as a flight strip: status, callsign, its agents or what's happening, PR, diff,
// age. Each agent that needs you, is working or just finished gets a line of its own on its
// worktree's row, so a worktree shows once however many agents run in it.

import type { PointerEvent as ReactPointerEvent } from 'react';
import { absolute, ago, agoPhrase, middleTruncate } from '../format';
import { AGENT_BAY, withoutAccount, type AgentItem, type WorktreeRow } from '../lanes';
import type { GitInfo, Lane, Pr, Session, Todo } from '../types';
import { HarnessChip, Icon, LaneGlyph, PrChip, Rich, StatusGlyph } from './primitives';
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
  /** Position among pinned rows, for dragging them into a new order. */
  pinIndex?: number;
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

/** `#1044: 1 failing (unit-test)`. */
export function failingTitle(pr: Pr): string {
  const names = pr.checks.failing.slice(0, 3).join(', ');
  return `#${pr.number}: ${pr.checks.failed} failing${names ? ` (${names})` : ''}`;
}

/** Sessions worth showing at a glance: live ones, ones with news, and anything from the last
 *  day. The inspector lists every session from the last 14 days. */
export function currentSessions(lane: Lane, now: number) {
  return lane.sessions.filter((s) => s.live || s.status === 'needs' || s.status === 'error' || s.status === 'working' || s.status === 'unread' || now - s.lastActivity < DAY);
}

/** `atlas · quarry`: the repo, and the branch when the callsign doesn't already say it. */
function laneSub(lane: Lane): { sub: string; branch: string | null } {
  const branch = lane.branch ?? (lane.detached ? `detached @ ${lane.detached}` : null);
  return { branch, sub: branch && branch !== lane.callsign ? `${lane.repo} · ${middleTruncate(branch, 26)}` : lane.repo };
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

function PinMark() {
  return (
    <span className="ca-pinmark" title="Pinned">
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

export function LaneRow(props: LaneRowProps) {
  return props.row.agents.length > 0 ? <GroupRow {...props} /> : <WorktreeOnlyRow {...props} />;
}

/** A worktree with no agent that needs you, is working or just finished: its sentence, and chips for its recent sessions. */
function WorktreeOnlyRow({ row, now, selectedKey, domId, onSelect, onOpen, onOpenTarget, pinned, onContextMenu, pinIndex, onPointerDown, extraClass, todos, onShowTodo }: LaneRowProps) {
  const { lane } = row;
  const chips = currentSessions(lane, now);
  const shown = chips.slice(0, 2);
  const extra = chips.length - shown.length;
  const { sub, branch } = laneSub(lane);
  const selected = selectedKey === lane.id;
  return (
    <div
      id={domId(lane.id)}
      role="option"
      aria-selected={selected}
      className={['ca-lane', `is-${lane.bay}`, selected && 'is-selected', pinned && 'is-pinned', extraClass].filter(Boolean).join(' ')}
      data-pin-index={pinIndex}
      onPointerDown={onPointerDown}
      onMouseDown={() => onSelect(lane.id)}
      onDoubleClick={() => onOpen(lane)}
      onContextMenu={menu(onContextMenu)}
    >
      {pinIndex != null && (
        <span className="ca-grip" aria-hidden="true">
          <Icon name="grip-vertical" size={10} />
        </span>
      )}
      <div className="ca-seg ca-seg-glyph">
        <LaneGlyph lane={lane} />
      </div>
      <div className="ca-seg ca-seg-call">
        <span className="ca-lane-call" title={lane.callsign}>
          {lane.callsign}
        </span>
        <span className="ca-lane-sub" title={branch ?? lane.path}>
          {sub}
        </span>
      </div>
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
          <span className="ca-lane-agents">
            {shown.map((s) => (
              <HarnessChip
                key={s.key}
                session={s}
                onOpen={() => {
                  onSelect(lane.id);
                  onOpenTarget(lane, s.openTarget);
                }}
              />
            ))}
            {extra > 0 && <span className="ca-chip ca-chip--more">+{extra}</span>}
            {chips.length === 0 && <span className="ca-none">{lane.sessions.length ? `No agents today · ${lane.sessions.length} earlier` : 'No agents'}</span>}
            <TodoChips todos={todos} onShow={onShowTodo} />
          </span>
          <span className="ca-lane-right">
            {lane.pr && <PrChip pr={lane.pr} compact />}
            <Diff git={lane.git} />
          </span>
        </div>
      </div>
      <div className="ca-seg ca-seg-age" title={lane.lastActivity ? absolute(lane.lastActivity) : undefined}>
        {lane.lastActivity ? ago(lane.lastActivity, now) : ''}
        {pinned && <PinMark />}
      </div>
    </div>
  );
}

/**
 * A worktree with agents that need you, are working or just finished: the worktree once on the
 * left, and a line for each agent, which you select, open and right-click like a row of its own.
 */
function GroupRow({ row, now, selectedKey, domId, onSelect, onOpen, onOpenTarget, pinned, onContextMenu, onAgentMenu, pinIndex, onPointerDown, extraClass, worktreeTodos, todosFor, onShowTodo }: LaneRowProps) {
  const { lane, agents } = row;
  const { sub, branch } = laneSub(lane);
  const listed = new Set(agents.map((a) => a.session.key));
  // Recent sessions without a line of their own: finished a while ago, or visiting.
  const others = currentSessions(lane, now).filter((s) => !listed.has(s.key));
  const shown = others.slice(0, 3);
  const extra = others.length - shown.length;
  const hasSelection = agents.some((a) => a.key === selectedKey);
  // A worktree can need you for its PR alone (failing checks, changes requested): say so above its agents.
  const prNeeds = lane.bay === 'needs' && !agents.some((a) => AGENT_BAY[a.session.status] === 'needs');
  // The worktree's own todos, less the ones its agents' lines already show.
  const onLines = new Set(agents.flatMap((a) => todosFor?.(a.session.key) ?? []));
  const footTodos = (worktreeTodos ?? []).filter((t) => !onLines.has(t));
  const foot = shown.length > 0 || footTodos.length > 0 || lane.pr != null || (lane.git != null && ((lane.git.added ?? 0) > 0 || (lane.git.removed ?? 0) > 0 || lane.git.dirty > 0));
  const selectFirst = () => onSelect(agents[0].key);
  return (
    <div
      role="group"
      aria-label={`${lane.callsign}, ${agents.length} ${agents.length === 1 ? 'agent' : 'agents'}`}
      className={['ca-lane', 'is-group', `is-${lane.bay}`, agents.length + (prNeeds ? 1 : 0) > 1 && 'is-tall', hasSelection && 'has-selection', pinned && 'is-pinned', extraClass]
        .filter(Boolean)
        .join(' ')}
      data-pin-index={pinIndex}
      onPointerDown={onPointerDown}
      onContextMenu={menu(onContextMenu)}
    >
      {pinIndex != null && (
        <span className="ca-grip" aria-hidden="true">
          <Icon name="grip-vertical" size={10} />
        </span>
      )}
      <div className="ca-seg ca-seg-glyph" onMouseDown={selectFirst} onDoubleClick={() => onOpen(lane)}>
        <LaneGlyph lane={lane} />
      </div>
      <div className="ca-seg ca-seg-call" onMouseDown={selectFirst} onDoubleClick={() => onOpen(lane)} title={`${lane.path}. Double-click to open its most recent session.`}>
        <span className="ca-lane-call">{lane.callsign}</span>
        <span className="ca-lane-sub" title={branch ?? lane.path}>
          {sub}
        </span>
      </div>
      <div className="ca-seg-lines">
        {prNeeds && (
          <div className="ca-agentline ca-agentline--lane" onMouseDown={selectFirst} onDoubleClick={() => onOpen(lane)}>
            <span className="ca-agentline-main">
              <span className="ca-lane-headline">
                <Rich text={lane.headline} now={now} />
              </span>
              {lane.detail && (
                <span className="ca-lane-detail">
                  <Rich text={lane.detail} now={now} />
                </span>
              )}
            </span>
            <span className="ca-agentline-age" />
          </div>
        )}
        {agents.map((a, i) => (
          <AgentLine
            key={a.key}
            item={a}
            now={now}
            domId={domId(a.key)}
            selected={a.key === selectedKey}
            pinned={pinned === true && i === 0 && !prNeeds}
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
              <span className="ca-lane-agents">
                {shown.map((s) => (
                  <HarnessChip
                    key={s.key}
                    session={s}
                    onOpen={() => {
                      selectFirst();
                      onOpenTarget(lane, s.openTarget);
                    }}
                  />
                ))}
                {extra > 0 && <span className="ca-chip ca-chip--more">+{extra}</span>}
                <TodoChips todos={footTodos} onShow={onShowTodo} />
              </span>
              <span className="ca-lane-right">
                {lane.pr && <PrChip pr={lane.pr} compact />}
                <Diff git={lane.git} />
              </span>
            </span>
            <span className="ca-agentline-age" />
          </div>
        )}
      </div>
    </div>
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

/** One agent on its worktree's row: its chip (click to open), its todo, its sentence, and its age. */
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
        <HarnessChip
          session={s}
          onOpen={() => {
            onSelect();
            onOpen();
          }}
        />
        <TodoChips todos={todos} onShow={onShowTodo} />
        <span className="ca-agentline-text">
          <span className="ca-lane-headline">
            <Rich text={withoutAccount(s.headline)} now={now} />
          </span>
          {s.detail && (
            <span className="ca-lane-detail">
              <Rich text={s.detail} now={now} />
            </span>
          )}
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
  selected: boolean;
  onSelect: () => void;
  onOpen: () => void;
  pinned?: boolean;
  /** Right-click: the session's menu (open, pin, archive…). */
  onContextMenu?: () => void;
  /** Position among pinned rows, for dragging them into a new order. */
  pinIndex?: number;
  onPointerDown?: (e: ReactPointerEvent<HTMLDivElement>) => void;
  extraClass?: string;
  /** Todos the session is linked to. */
  todos?: Todo[];
  onShowTodo?: (todo: Todo) => void;
}

/** One session in the Sessions view, with its worktree as context. */
export function AgentRow({ lane, session: s, domId, now, selected, onSelect, onOpen, pinned, onContextMenu, pinIndex, onPointerDown, extraClass, todos, onShowTodo }: AgentRowProps) {
  const { sub, branch } = laneSub(lane);
  return (
    <div
      id={domId}
      role="option"
      aria-selected={selected}
      className={['ca-lane', 'is-agent', `is-${s.status === 'error' ? 'needs' : s.status}`, selected && 'is-selected', pinned && 'is-pinned', extraClass]
        .filter(Boolean)
        .join(' ')}
      data-pin-index={pinIndex}
      onPointerDown={onPointerDown}
      onMouseDown={onSelect}
      onDoubleClick={onOpen}
      onContextMenu={menu(onContextMenu)}
    >
      {pinIndex != null && (
        <span className="ca-grip" aria-hidden="true">
          <Icon name="grip-vertical" size={10} />
        </span>
      )}
      <div className="ca-seg ca-seg-glyph">
        <StatusGlyph status={s.status} />
      </div>
      <div className="ca-seg ca-seg-call">
        <span className="ca-lane-call" title={lane.path}>
          {lane.callsign}
        </span>
        <span className="ca-lane-sub" title={branch ?? lane.path}>
          {sub}
        </span>
      </div>
      <div className="ca-seg ca-seg-main">
        <div className="ca-lane-line1">
          <span className="ca-lane-headline">
            <Rich text={s.headline} now={now} />
          </span>
          {s.detail && (
            <span className="ca-lane-detail">
              <Rich text={s.detail} now={now} />
            </span>
          )}
        </div>
        <div className="ca-lane-line2">
          <span className="ca-lane-agents">
            <HarnessChip session={s} onOpen={onOpen} />
            <TodoChips todos={todos} onShow={onShowTodo} />
          </span>
          <span className="ca-lane-right">
            {lane.pr && <PrChip pr={lane.pr} compact />}
            <Diff git={lane.git} />
          </span>
        </div>
      </div>
      <div className="ca-seg ca-seg-age" title={absolute(s.lastActivity)}>
        {ago(s.lastActivity, now)}
        {pinned && <PinMark />}
      </div>
    </div>
  );
}

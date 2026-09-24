// One worktree as a flight strip: status, callsign, what's happening, agents, PR, diff, age.

import type { PointerEvent as ReactPointerEvent } from 'react';
import { absolute, ago, middleTruncate } from '../format';
import type { Lane, Pr, Session } from '../types';
import { HarnessChip, Icon, LaneGlyph, PrChip, Rich, StatusGlyph } from './primitives';

interface LaneRowProps {
  lane: Lane;
  domId: string;
  now: number;
  selected: boolean;
  onSelect: (id: string) => void;
  onOpen: (lane: Lane) => void;
  /** Opens one session where it lives. */
  onOpenTarget: (lane: Lane, targetId: string) => void;
  pinned?: boolean;
  /** Right-click: the worktree's menu (open, pin, copy…). */
  onContextMenu?: () => void;
  /** Position among pinned rows, for dragging them into a new order. */
  pinIndex?: number;
  onPointerDown?: (e: ReactPointerEvent<HTMLDivElement>) => void;
  extraClass?: string;
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

export function LaneRow({ lane, domId, now, selected, onSelect, onOpen, onOpenTarget, pinned, onContextMenu, pinIndex, onPointerDown, extraClass }: LaneRowProps) {
  const chips = currentSessions(lane, now);
  const shown = chips.slice(0, 2);
  const extra = chips.length - shown.length;
  const branch = lane.branch ?? (lane.detached ? `detached @ ${lane.detached}` : null);
  const sub = branch && branch !== lane.callsign ? `${lane.repo} · ${middleTruncate(branch, 26)}` : lane.repo;
  const g = lane.git;
  const showDiff = g && ((g.added ?? 0) > 0 || (g.removed ?? 0) > 0 || g.dirty > 0);
  return (
    <div
      id={domId}
      role="option"
      aria-selected={selected}
      className={['ca-lane', `is-${lane.bay}`, selected && 'is-selected', pinned && 'is-pinned', extraClass].filter(Boolean).join(' ')}
      data-pin-index={pinIndex}
      onPointerDown={onPointerDown}
      onMouseDown={() => onSelect(lane.id)}
      onDoubleClick={() => onOpen(lane)}
      onContextMenu={
        onContextMenu
          ? (e) => {
              e.preventDefault();
              onContextMenu();
            }
          : undefined
      }
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
              <HarnessChip key={s.key} session={s} onOpen={() => onOpenTarget(lane, s.openTarget)} />
            ))}
            {extra > 0 && <span className="ca-chip ca-chip--more">+{extra}</span>}
            {chips.length === 0 && <span className="ca-none">{lane.sessions.length ? `No agents today · ${lane.sessions.length} earlier` : 'No agents'}</span>}
          </span>
          <span className="ca-lane-right">
            {lane.pr && <PrChip pr={lane.pr} compact />}
            {showDiff && (
              <span className="ca-diff">
                {g.added != null && ((g.added ?? 0) > 0 || (g.removed ?? 0) > 0) ? `+${g.added.toLocaleString('en-US')} −${(g.removed ?? 0).toLocaleString('en-US')}` : ''}
                {g.dirty > 0 ? <span title={`${g.dirty} uncommitted files`}> ●</span> : null}
              </span>
            )}
          </span>
        </div>
      </div>
      <div className="ca-seg ca-seg-age" title={lane.lastActivity ? absolute(lane.lastActivity) : undefined}>
        {lane.lastActivity ? ago(lane.lastActivity, now) : ''}
        {pinned && (
          <span className="ca-pinmark" title="Pinned">
            <Icon name="pinned" size={12} />
          </span>
        )}
      </div>
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
}

/** One agent in the Needs you, Working or Your turn bays, with its worktree as context. */
export function AgentRow({ lane, session: s, domId, now, selected, onSelect, onOpen, pinned, onContextMenu, pinIndex, onPointerDown, extraClass }: AgentRowProps) {
  const branch = lane.branch ?? (lane.detached ? `detached @ ${lane.detached}` : null);
  const sub = branch && branch !== lane.callsign ? `${lane.repo} · ${middleTruncate(branch, 26)}` : lane.repo;
  const g = lane.git;
  const showDiff = g && ((g.added ?? 0) > 0 || (g.removed ?? 0) > 0 || g.dirty > 0);
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
      onContextMenu={
        onContextMenu
          ? (e) => {
              e.preventDefault();
              onContextMenu();
            }
          : undefined
      }
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
          </span>
          <span className="ca-lane-right">
            {lane.pr && <PrChip pr={lane.pr} compact />}
            {showDiff && (
              <span className="ca-diff">
                {g.added != null && ((g.added ?? 0) > 0 || (g.removed ?? 0) > 0) ? `+${g.added.toLocaleString('en-US')} −${(g.removed ?? 0).toLocaleString('en-US')}` : ''}
                {g.dirty > 0 ? <span title={`${g.dirty} uncommitted files`}> ●</span> : null}
              </span>
            )}
          </span>
        </div>
      </div>
      <div className="ca-seg ca-seg-age" title={absolute(s.lastActivity)}>
        {ago(s.lastActivity, now)}
        {pinned && (
          <span className="ca-pinmark" title="Pinned">
            <Icon name="pinned" size={12} />
          </span>
        )}
      </div>
    </div>
  );
}

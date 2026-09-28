// The sidebar tree: the project's worktrees, pinned ones first in your order and the rest
// grouped by repo, each opening to its own agents.

import { useState } from 'react';
import { ago } from '../format';
import { failingPr } from '../lanes';
import type { Lane, Session } from '../types';
import { currentSessions, failingTitle } from './LaneRow';
import { memoRow } from './memoRow';
import { Icon, LaneGlyph, StatusGlyph } from './primitives';

const EXPANDED_KEY = 'codealong.tree';
const ACTIVE = new Set(['needs', 'error', 'working', 'unread']);

function loadOverrides(): Record<string, boolean> {
  try {
    return JSON.parse(window.localStorage.getItem(EXPANDED_KEY) ?? '{}') as Record<string, boolean>;
  } catch {
    return {};
  }
}

/** A repo's worktrees, or the pinned ones (`pinned`), which can come from any repo. */
export interface TreeGroup {
  repo: string;
  lanes: Lane[];
  pinned?: boolean;
}

interface WorktreeTreeProps {
  groups: TreeGroup[];
  now: number;
  selectedLaneId: string | null;
  selectedSessionKey: string | null;
  onSelectLane: (lane: Lane) => void;
  /** Double-click on a worktree: opens its most recent session. */
  onOpenLane: (lane: Lane) => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  /** Right-click on a worktree. */
  onLaneMenu: (lane: Lane) => void;
  /** Right-click on a session. */
  onSessionMenu: (lane: Lane, session: Session) => void;
}

export function WorktreeTree({ groups, now, selectedLaneId, selectedSessionKey, onSelectLane, onOpenLane, onOpenSession, onLaneMenu, onSessionMenu }: WorktreeTreeProps) {
  // Worktrees with an agent that needs you, is working or just finished start open; the rest
  // start closed. Toggles are remembered.
  const [overrides, setOverrides] = useState<Record<string, boolean>>(loadOverrides);
  const [showOlder, setShowOlder] = useState<Set<string>>(() => new Set());
  const isOpen = (lane: Lane) => overrides[lane.id] ?? lane.sessions.some((s) => !s.visitor && ACTIVE.has(s.status));
  const toggle = (lane: Lane) => {
    const next = { ...overrides, [lane.id]: !isOpen(lane) };
    setOverrides(next);
    try {
      window.localStorage.setItem(EXPANDED_KEY, JSON.stringify(next));
    } catch {
      // Storage can be unavailable; the tree just forgets.
    }
  };
  return (
    <div className="ca-tree" role="tree" aria-label="Worktrees">
      {groups.map((g) => (
        <div key={g.pinned ? ':pinned' : g.repo} className="ca-wt-group" role="group" aria-label={g.pinned ? 'Pinned' : g.repo}>
          {groups.length > 1 &&
            (g.pinned ? (
              <div className="ca-wt-repo ca-wt-pinned">
                <Icon name="pinned" size={11} />
                Pinned
              </div>
            ) : (
              <div className="ca-wt-repo">{g.repo}</div>
            ))}
          {g.lanes.map((lane) => {
            const selected = selectedLaneId === lane.id;
            return (
              <TreeLane
                key={lane.id}
                lane={lane}
                now={now}
                open={isOpen(lane)}
                all={showOlder.has(lane.id)}
                selected={selected}
                selectedSessionKey={selected ? selectedSessionKey : null}
                onToggle={toggle}
                onShowOlder={(l) => setShowOlder((had) => new Set(had).add(l.id))}
                onSelectLane={onSelectLane}
                onOpenLane={onOpenLane}
                onOpenSession={onOpenSession}
                onLaneMenu={onLaneMenu}
                onSessionMenu={onSessionMenu}
              />
            );
          })}
        </div>
      ))}
    </div>
  );
}

interface TreeLaneProps {
  lane: Lane;
  now: number;
  open: boolean;
  /** Its older sessions are listed too. */
  all: boolean;
  selected: boolean;
  /** The selected session, when it's one of this worktree's. */
  selectedSessionKey: string | null;
  onToggle: (lane: Lane) => void;
  onShowOlder: (lane: Lane) => void;
  onSelectLane: (lane: Lane) => void;
  onOpenLane: (lane: Lane) => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  onLaneMenu: (lane: Lane) => void;
  onSessionMenu: (lane: Lane, session: Session) => void;
}

/** One worktree in the tree, with its sessions when it's open. */
const TreeLane = memoRow(function TreeLane({ lane, now, open, all, selected, selectedSessionKey, onToggle, onShowOlder, onSelectLane, onOpenLane, onOpenSession, onLaneMenu, onSessionMenu }: TreeLaneProps) {
  const recent = currentSessions(lane, now);
  const listed = all ? lane.sessions : recent;
  const older = lane.sessions.length - recent.length;
  const active = lane.sessions.filter((s) => !s.visitor && ACTIVE.has(s.status)).length;
  const failing = failingPr(lane);
  const opens = lane.defaultTarget?.startsWith('s|') ? 'its most recent session' : 'a new terminal here';
  return (
    <div key={lane.id} role="treeitem" aria-expanded={lane.sessions.length ? open : undefined} aria-selected={selected}>
      <div
        className={['ca-tree-node', lane.parked && 'is-parked', selected && !selectedSessionKey && 'is-active', lane.device && !lane.device.online && 'is-away'].filter(Boolean).join(' ')}
        onContextMenu={(e) => {
          e.preventDefault();
          onLaneMenu(lane);
        }}
      >
        <button
          type="button"
          className={['ca-tree-toggle', open && 'is-open'].filter(Boolean).join(' ')}
          aria-label={open ? `Collapse ${lane.callsign}` : `Expand ${lane.callsign}`}
          disabled={lane.sessions.length === 0}
          onClick={() => onToggle(lane)}
        >
          {lane.sessions.length > 0 && <Icon name="chevron-right" size={12} stroke={2} />}
        </button>
        <button
          type="button"
          className="ca-tree-label"
          title={`${lane.callsign} · ${lane.branch ?? lane.detached ?? ''}. Double-click to open ${opens}.`}
          onClick={() => onSelectLane(lane)}
          onDoubleClick={() => onOpenLane(lane)}
        >
          <LaneGlyph lane={lane} />
          <span className="ca-nav-label">{lane.callsign}</span>
          {lane.device && <i className={`ca-labeldot ca-tree-devdot is-${lane.device.color}`} title={`On ${lane.device.name}`} aria-hidden="true" />}
          {failing && (
            <span className="ca-tree-fail" title={failingTitle(failing)}>
              <Icon name="x" size={11} stroke={2.25} />#{failing.number}
            </span>
          )}
          {active > 0 ? (
            <span className={['ca-nav-count', lane.bay === 'needs' && 'is-attn'].filter(Boolean).join(' ')}>{active}</span>
          ) : lane.pr && !failing ? (
            <span className="ca-nav-count">#{lane.pr.number}</span>
          ) : null}
        </button>
      </div>
      {open && lane.sessions.length > 0 && (
        <div className="ca-tree-children" role="group">
          {listed.map((s) => (
            <button
              key={s.key}
              type="button"
              role="treeitem"
              className={['ca-tree-session', s.visitor && 'is-visitor', selectedSessionKey === s.key && 'is-active'].filter(Boolean).join(' ')}
              title={`${s.account} · ${s.title}${s.visitor ? ` (${s.visitor})` : ''}. Click to open it where it lives.`}
              // A double-click here is two clicks: open once, not in two terminals.
              onClick={(e) => {
                if (e.detail < 2) onOpenSession(lane, s);
              }}
              onContextMenu={(e) => {
                e.preventDefault();
                onSessionMenu(lane, s);
              }}
            >
              <StatusGlyph status={s.status} size={10} />
              <span className="ca-tree-acct">{s.account}</span>
              <span className="ca-tree-title">{s.title}</span>
              <span className="ca-tree-age">{ago(s.lastActivity, now)}</span>
            </button>
          ))}
          {!all && older > 0 && (
            <button type="button" className="ca-tree-more" onClick={() => onShowOlder(lane)}>
              {recent.length === 0 ? `${older} from the last 14 days` : `${older} older`}
            </button>
          )}
        </div>
      )}
    </div>
  );
});

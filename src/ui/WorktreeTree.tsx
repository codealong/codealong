// The sidebar tree: the project's worktrees, grouped by repo, each opening to its own agents.

import { useState } from 'react';
import { ago } from '../format';
import type { Lane, Session } from '../types';
import { currentSessions, failingPr, failingTitle } from './LaneRow';
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

interface WorktreeTreeProps {
  groups: { repo: string; lanes: Lane[] }[];
  now: number;
  selectedLaneId: string | null;
  selectedSessionKey: string | null;
  onSelectLane: (lane: Lane) => void;
  onOpenSession: (lane: Lane, session: Session) => void;
}

export function WorktreeTree({ groups, now, selectedLaneId, selectedSessionKey, onSelectLane, onOpenSession }: WorktreeTreeProps) {
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
        <div key={g.repo} className="ca-wt-group" role="group" aria-label={g.repo}>
          {groups.length > 1 && <div className="ca-wt-repo">{g.repo}</div>}
          {g.lanes.map((lane) => {
            const open = isOpen(lane);
            const recent = currentSessions(lane, now);
            const all = showOlder.has(lane.id);
            const listed = all ? lane.sessions : recent;
            const older = lane.sessions.length - recent.length;
            const active = lane.sessions.filter((s) => !s.visitor && ACTIVE.has(s.status)).length;
            const failing = failingPr(lane);
            return (
              <div key={lane.id} role="treeitem" aria-expanded={lane.sessions.length ? open : undefined} aria-selected={selectedLaneId === lane.id}>
                <div className={['ca-tree-node', lane.parked && 'is-parked', selectedLaneId === lane.id && !selectedSessionKey && 'is-active'].filter(Boolean).join(' ')}>
                  <button
                    type="button"
                    className={['ca-tree-toggle', open && 'is-open'].filter(Boolean).join(' ')}
                    aria-label={open ? `Collapse ${lane.callsign}` : `Expand ${lane.callsign}`}
                    disabled={lane.sessions.length === 0}
                    onClick={() => toggle(lane)}
                  >
                    {lane.sessions.length > 0 && <Icon name="chevron-right" size={12} stroke={2} />}
                  </button>
                  <button type="button" className="ca-tree-label" title={`${lane.callsign} · ${lane.branch ?? lane.detached ?? ''}`} onClick={() => onSelectLane(lane)}>
                    <LaneGlyph lane={lane} />
                    <span className="ca-nav-label">{lane.callsign}</span>
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
                        className={['ca-tree-session', s.visitor && 'is-visitor', selectedSessionKey === s.key && selectedLaneId === lane.id && 'is-active'].filter(Boolean).join(' ')}
                        title={`${s.account} · ${s.title}${s.visitor ? ` (${s.visitor})` : ''}. Click to open it where it lives.`}
                        onClick={() => onOpenSession(lane, s)}
                      >
                        <StatusGlyph status={s.status} size={10} />
                        <span className="ca-tree-acct">{s.account}</span>
                        <span className="ca-tree-title">{s.title}</span>
                        <span className="ca-tree-age">{ago(s.lastActivity, now)}</span>
                      </button>
                    ))}
                    {!all && older > 0 && (
                      <button type="button" className="ca-tree-more" onClick={() => setShowOlder(new Set(showOlder).add(lane.id))}>
                        {recent.length === 0 ? `${older} from the last 14 days` : `${older} older`}
                      </button>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}

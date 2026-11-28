// "Who's doing what" brief for a cluster on the Working Set canvas (WorkingSetCanvas.tsx): lists
// every session and schedule hub in the selected neighborhood sorted by urgency, with its full
// summary (orchestrator insight.note or status), action badge, deadline, PRs and upcoming timers.

import { useState } from 'react';
import { ago, duration } from '../format';
import type { WorkingCluster, WorkingNode } from '../canvas/workingSet';
import { cx, Icon, PrChip, SessionEmblem, SessionGlyph } from './primitives';

interface WorkingSetClusterPanelProps {
  cluster: WorkingCluster;
  nodes: WorkingNode[];
  now: number;
  onSelectSession: (key: string) => void;
  onOpenSession: (key: string) => void;
  onShelve: (id: string) => void;
  onRename: (clusterId: string, title: string) => void;
  onClose: () => void;
}

export function WorkingSetClusterPanel({
  cluster,
  nodes,
  now,
  onSelectSession,
  onOpenSession,
  onShelve,
  onRename,
  onClose,
}: WorkingSetClusterPanelProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(cluster.title);

  const sorted = [...nodes].sort((a, b) => a.action.rank - b.action.rank);

  return (
    <aside className={cx('ca-ws-panel', `is-${cluster.color}`)} aria-label={`${cluster.title} overview`}>
      <div className="ca-ws-panel-head">
        <span className="ca-ws-panel-dot" aria-hidden="true" />
        {editing ? (
          <input
            className="ca-ws-panel-input"
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.currentTarget.value)}
            onBlur={() => {
              onRename(cluster.id, draft);
              setEditing(false);
            }}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === 'Enter') e.currentTarget.blur();
              if (e.key === 'Escape') {
                setDraft(cluster.title);
                setEditing(false);
              }
            }}
            aria-label="Cluster name"
            spellCheck={false}
          />
        ) : (
          <button
            type="button"
            className="ca-ws-panel-title"
            title="Click to rename cluster"
            onClick={() => {
              setDraft(cluster.title);
              setEditing(true);
            }}
          >
            <span>{cluster.title}</span>
            <Icon name="pencil" size={12} />
          </button>
        )}
        <button type="button" className="ca-ws-panel-close" title="Close overview" aria-label="Close overview" onClick={onClose}>
          <Icon name="x" size={14} />
        </button>
      </div>

      <div className="ca-ws-panel-stats">
        <span>{sorted.length} {sorted.length === 1 ? 'item' : 'items'}</span>
        {cluster.actionCount > 0 && (
          <span className={cx('ca-ws-panel-stat', cluster.urgentCount > 0 ? 'is-urgent' : 'is-handoff')}>
            {cluster.actionCount} need action
          </span>
        )}
        {cluster.workingCount > 0 && <span className="ca-ws-panel-stat is-working">{cluster.workingCount} active</span>}
      </div>

      <div className="ca-ws-panel-list">
        {sorted.map((node) => {
          if (node.kind === 'schedule') {
            return (
              <div key={node.id} className="ca-ws-panel-row is-hub">
                <div className="ca-ws-panel-row-top">
                  <Icon name={node.schedule.when.kind === 'watch' ? 'eye' : 'clock'} size={13} />
                  <span className="ca-ws-panel-row-title">{node.schedule.name}</span>
                  {node.quietRuns > 0 && <span className="ca-ws-chip is-muted">{node.quietRuns} folded</span>}
                </div>
                <p className="ca-ws-panel-row-note">{node.nextLabel} · {node.schedule.whenWords}</p>
              </div>
            );
          }

          const s = node.session;
          return (
            <div key={node.id} className={cx('ca-ws-panel-row', `tier-${node.action.tier}`)}>
              <div className="ca-ws-panel-row-top">
                <span className={cx('ca-ws-beacon', `tier-${node.action.tier}`)}>{node.action.label}</span>
                {node.action.deadline && <span className="ca-ws-deadline">{node.action.deadline}</span>}
                <span className="ca-ws-panel-age">
                  {s.status === 'working' && s.statusSince ? duration(s.statusSince, now) : ago(s.lastActivity, now)}
                </span>
                <button
                  type="button"
                  className="ca-ws-shelve-btn"
                  title="Shelve from working set"
                  aria-label={`Shelve ${s.title}`}
                  onClick={() => onShelve(node.id)}
                >
                  <Icon name="x" size={12} />
                </button>
              </div>
              <button
                type="button"
                className="ca-ws-panel-row-open"
                onClick={() => onSelectSession(s.key)}
                onDoubleClick={() => onOpenSession(s.key)}
              >
                <SessionGlyph s={s} size={13} />
                <SessionEmblem emblem={s.emblem} of={s.key} size={15} />
                <span className="ca-ws-panel-row-title">{s.title}</span>
              </button>
              <p className="ca-ws-panel-row-note">{node.summary}</p>
              <div className="ca-ws-panel-row-foot">
                <span className="ca-ws-chip">{node.lane.repo || node.lane.callsign}</span>
                {node.prs.slice(0, 2).map((pr) => (
                  <PrChip key={pr.url} pr={pr} compact />
                ))}
                {node.timers.slice(0, 2).map((t) => (
                  <span key={t.scheduleId} className="ca-ws-timer" title={t.name}>
                    <Icon name="alarm" size={11} />
                    {t.words}
                  </span>
                ))}
                {node.helpers.length > 0 && (
                  <span className="ca-ws-chip is-muted">+{node.helpers.length} {node.helpers.length === 1 ? 'helper' : 'helpers'}</span>
                )}
                <span className="ca-ws-account">{s.account}</span>
              </div>
            </div>
          );
        })}
      </div>
    </aside>
  );
}

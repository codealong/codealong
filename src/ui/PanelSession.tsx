// A worktree and its sessions as the PR and todo panels list them, drawn like the sidebar's tree
// (`PanelPlace`), and a session under its worktree there (`PanelSession`): its title, then who runs it and
// what it's doing, as the Sessions view says it. A click shows it in the panel (with a way back);
// double-click or the arrow opens it where it runs, as double-click does in every list.

import { absolute, ago } from '../format';
import { sessionWhy } from '../lanes';
import type { Lane, Session } from '../types';
import type { ReactNode } from 'react';
import { cx, Icon, LaneGlyph, SessionGlyph } from './primitives';

/** What the Primary tag on a todo's session means. */
export const TODO_PRIMARY = "The todo's primary session: where the work on it goes on. Sessions started for it while this one runs work alongside it.";

/** What it means on a PR's (board.rs `pr_primary`). */
export const PR_PRIMARY = 'The PR’s primary session: the one that opened it, unless another was made it (codealong pr nudge --session). When the PR is nudged, only this one is told what happens on it.';

interface PanelSessionProps {
  lane: Lane;
  session: Session;
  now: number;
  onShow: (lane: Lane, session: Session) => void;
  onOpen: (lane: Lane, session: Session) => void;
  onMenu: (lane: Lane, session: Session) => void;
  /** It's the todo's or the PR's primary session: what its Primary tag says that means. */
  primary?: string;
}

export function PanelSession({ lane, session: s, now, onShow, onOpen, onMenu, primary }: PanelSessionProps) {
  return (
    <div
      role="treeitem"
      className={cx('ca-psess', s.visitor && 'is-visitor')}
      onContextMenu={(e) => {
        e.preventDefault();
        onMenu(lane, s);
      }}
    >
      <button type="button" className="ca-psess-main" title={`${s.title}\nClick to show it here · double-click to open it where it runs`} onClick={() => onShow(lane, s)} onDoubleClick={() => onOpen(lane, s)}>
        <SessionGlyph s={s} size={10} />
        <span className="ca-psess-text">
          <span className="ca-psess-line1">
            <span className="ca-psess-title">{s.title}</span>
            {primary && (
              <span className="ca-tag ca-psess-primary" title={primary}>
                Primary
              </span>
            )}
            <span className="ca-tree-age" title={absolute(s.lastActivity)}>
              {ago(s.lastActivity, now)}
            </span>
          </span>
          <span className="ca-psess-line2">
            <span className="ca-tree-acct">{s.account}</span>
            <span className="ca-psess-headline">{s.visitor ?? sessionWhy(s.detail, s.headline, now)}</span>
          </span>
        </span>
      </button>
      <button type="button" className="ca-psess-open" aria-label="Open where it runs" title="Open it where it runs" onClick={() => onOpen(lane, s)}>
        <Icon name="arrow-up-right" size={12} />
      </button>
    </div>
  );
}

interface PanelPlaceProps {
  lane: Lane;
  /** The sessions listed under it. */
  sessions: Session[];
  /** What the panel says after the worktree's name: its PR or branch, how it stands. */
  note?: ReactNode;
  now: number;
  onShowLane: (lane: Lane) => void;
  /** A right-click on the worktree. */
  onLaneMenu: (lane: Lane) => void;
  onShowSession: (lane: Lane, session: Session) => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  onSessionMenu: (lane: Lane, session: Session) => void;
  /** The todo's primary session, while it runs, or the PR's: its key, and what its Primary tag
   *  says that means. */
  primary?: { key: string; means: string } | null;
}

/** A worktree and its sessions in a panel, as the sidebar's tree draws them. */
export function PanelPlace({ lane, sessions, note, now, onShowLane, onLaneMenu, onShowSession, onOpenSession, onSessionMenu, primary = null }: PanelPlaceProps) {
  return (
    <div role="treeitem" aria-expanded={sessions.length > 0 ? true : undefined}>
      <div
        className="ca-tree-node"
        onContextMenu={(e) => {
          e.preventDefault();
          onLaneMenu(lane);
        }}
      >
        <button type="button" className="ca-tree-label" title={`${lane.path}. Click to show it in Worktrees.`} onClick={() => onShowLane(lane)}>
          <LaneGlyph lane={lane} />
          <span className="ca-nav-label">{lane.callsign}</span>
          {note}
        </button>
      </div>
      {sessions.length > 0 && (
        <div className="ca-tree-children" role="group">
          {sessions.map((s) => (
            <PanelSession key={s.key} lane={lane} session={s} now={now} onShow={onShowSession} onOpen={onOpenSession} onMenu={onSessionMenu} primary={s.key === primary?.key ? primary.means : undefined} />
          ))}
        </div>
      )}
    </div>
  );
}

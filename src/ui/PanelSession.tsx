// The worktrees and sessions behind a PR or a todo, as their panels show them (`PanelPlaces`): as
// chips at first, a worktree's chip and its sessions' after it, as the lists' rows name them, the
// primary session's worktree and that session leading (`primaryFirst`); and
// with the panel's details open, drawn like the sidebar's tree (`PanelPlace`), each session under
// its worktree (`PanelSession`) with its title, then who runs it and what it's doing, as the
// Sessions view says it. A click on a session, chip or row, shows it in the panel (with a way
// back); on a row, double-click or the arrow opens it where it runs, as double-click does in every
// list. A click on a worktree shows it in Worktrees; a right-click on either gives its menu.

import { absolute, ago } from '../format';
import { sessionWhy } from '../lanes';
import { flag, useStored } from '../stored';
import type { Lane, Session } from '../types';
import { Button, cx, HarnessChip, Icon, LaneGlyph, SessionEmblem, SessionGlyph, WorktreeChip } from './primitives';

/** What the Primary tag on a todo's session means. */
export const TODO_PRIMARY = "The todo's primary session: where the work on it goes on. Sessions started for it while this one runs work alongside it.";

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
            <SessionEmblem emblem={s.emblem} size={14} />
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
  note?: string | null;
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
          {note && <span className="ca-tree-note">{note}</span>}
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

/** A worktree and the sessions listed under it, in a PR's or a todo's panel. */
export interface Place {
  lane: Lane;
  sessions: Session[];
  /** What the panel says of the worktree: its PR or branch, how it stands. */
  note?: string | null;
}

interface PanelPlacesProps extends Omit<PanelPlaceProps, 'lane' | 'sessions' | 'note'> {
  places: Place[];
  /** The tree, each session's title and what it's doing said; else the chips. */
  open: boolean;
  /** What a screen reader calls the list. */
  label: string;
}

/** The places with the primary session's worktree first and that session first in it, the one the
 *  work goes on in; the rest keep their order. */
export function primaryFirst(places: Place[], key: string | null | undefined): Place[] {
  const at = places.findIndex((p) => p.sessions.some((s) => s.key === key));
  if (at < 0) return places;
  const p = places[at];
  const lifted = { ...p, sessions: [...p.sessions.filter((s) => s.key === key), ...p.sessions.filter((s) => s.key !== key)] };
  return [lifted, ...places.slice(0, at), ...places.slice(at + 1)];
}

/** The worktrees and sessions behind a PR or a todo: chips, or with `open` the tree. */
export function PanelPlaces({ places: given, open, label, now, onShowLane, onLaneMenu, onShowSession, onOpenSession, onSessionMenu, primary = null }: PanelPlacesProps) {
  const places = primaryFirst(given, primary?.key);
  if (open) {
    return (
      <div className="ca-tree" role="tree" aria-label={label}>
        {places.map((p) => (
          <PanelPlace
            key={p.lane.id}
            lane={p.lane}
            sessions={p.sessions}
            note={p.note}
            now={now}
            onShowLane={onShowLane}
            onLaneMenu={onLaneMenu}
            onShowSession={onShowSession}
            onOpenSession={onOpenSession}
            onSessionMenu={onSessionMenu}
            primary={primary}
          />
        ))}
      </div>
    );
  }
  return (
    <div className="ca-places" role="list" aria-label={label}>
      {places.map(({ lane, sessions, note }) => (
        <span key={lane.id} className="ca-places-group" role="listitem">
          <WorktreeChip lane={lane} short note={note} onShow={() => onShowLane(lane)} onMenu={() => onLaneMenu(lane)} />
          {sessions.map((s) => {
            const main = s.key === primary?.key;
            return (
              <HarnessChip
                key={s.key}
                session={s}
                named
                className={cx(s.visitor && 'is-visitor')}
                detail={`${s.visitor ?? sessionWhy(s.detail, s.headline, now)}${main ? `\nPrimary: ${primary.means}` : ''}`}
                onShow={() => onShowSession(lane, s)}
                onMenu={() => onSessionMenu(lane, s)}
              >
                {main && <span className="ca-places-primary">primary</span>}
              </HarnessChip>
            );
          })}
        </span>
      ))}
    </div>
  );
}

/** Whether a panel's details are open, kept for every PR or todo as you left it, under `key`. */
export function useDetailsOpen(key: string): [boolean, () => void] {
  const [open, setOpen] = useStored(key, flag, false);
  return [open, () => setOpen((o) => !o)];
}

/** The button that opens a panel's details and closes them again. `says` is what they add, for
 *  its tooltip: "each session's title and what it's doing". */
export function DetailsButton({ open, says, onToggle, className }: { open: boolean; says: string; onToggle: () => void; className?: string }) {
  return (
    <Button
      variant="ghost"
      size="sm"
      icon={open ? 'chevron-up' : 'chevron-down'}
      aria-label="Details"
      aria-expanded={open}
      title={open ? 'Fewer details' : `More details: ${says}`}
      className={cx('ca-detailsbtn', open && 'is-on', className)}
      onClick={onToggle}
    />
  );
}

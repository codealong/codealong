// A session as the PR and todo panels list it under its worktree: its title, then who runs it and
// what it's doing, as the Sessions view says it. A click shows it in the panel (with a way back);
// double-click or the arrow opens it where it runs, as double-click does in every list.

import { absolute, ago, withTimes } from '../format';
import { withoutAccount } from '../lanes';
import type { Lane, Session } from '../types';
import { Icon, StatusGlyph } from './primitives';

/** What a session is doing or said last, for a row under its title. Its time is in the row's
 *  own column, so the detail's running time is left out. */
export function sessionWhy(detail: string | null, headline: string, now: number): string {
  const text = detail ? detail.replace(/^\{for:-?\d+\}( · )?/, '') : withoutAccount(headline);
  return withTimes(text.replace(/`/g, ''), now);
}

interface PanelSessionProps {
  lane: Lane;
  session: Session;
  now: number;
  onShow: (lane: Lane, session: Session) => void;
  onOpen: (lane: Lane, session: Session) => void;
  onMenu: (lane: Lane, session: Session) => void;
}

export function PanelSession({ lane, session: s, now, onShow, onOpen, onMenu }: PanelSessionProps) {
  return (
    <div
      role="treeitem"
      className={['ca-psess', s.visitor && 'is-visitor'].filter(Boolean).join(' ')}
      onContextMenu={(e) => {
        e.preventDefault();
        onMenu(lane, s);
      }}
    >
      <button type="button" className="ca-psess-main" title={`${s.title}\nClick to show it here · double-click to open it where it runs`} onClick={() => onShow(lane, s)} onDoubleClick={() => onOpen(lane, s)}>
        <StatusGlyph status={s.status} size={10} />
        <span className="ca-psess-text">
          <span className="ca-psess-line1">
            <span className="ca-psess-title">{s.title}</span>
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

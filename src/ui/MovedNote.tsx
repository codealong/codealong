// A session that moved to another account or harness goes on as its copy (`Session.movedTo`):
// the engine neither sends to it nor resumes it, so its panel's box is the copy's
// (SessionPanel.tsx), and this line above the box says so, its chip showing the copy. Not for one
// handed off with the original kept open, which goes on as itself.

import { useSyncExternalStore } from 'react';
import { sessionNamed } from '../codealongLinks';
import { shortOf } from '../devices';
import type { Placed } from '../lanes';
import type { Lane, Session } from '../types';
import { MovedChip } from './LaneRow';
import { linkHost } from './PrLink';

/** The copy `s` moved to and goes on as, where this window lists it: on the same device as `s`,
 *  whose own key for it `movedTo` is. Null for one that didn't move, or whose copy isn't here. */
export function useMovedCopy(s: Pick<Session, 'key' | 'movedTo'>): Placed | null {
  const names = useSyncExternalStore(linkHost.subscribe, linkHost.sessions, linkHost.sessions);
  return s.movedTo ? sessionNamed(names, s.movedTo, shortOf(s.key)) : null;
}

export function MovedNote({ to, onShow }: { to: Placed; onShow?: (lane: Lane, session: Session) => void }) {
  const show = onShow ?? linkHost.actions()?.showSession;
  return (
    <div className="ca-movednote" role="status">
      <MovedChip to={to} onShow={show && ((p) => show(p.lane, p.session))} />
      <span>What you write here goes to it there.</span>
    </div>
  );
}

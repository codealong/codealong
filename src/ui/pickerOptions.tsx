// Worktrees and sessions as a picker's options (Picker.tsx), wherever one is picked: the quick
// panel's worktree menu, and the schedule dialog's worktree and session fields. A worktree reads
// as the quick panel has it, by name and branch with its status; a session by its title, where it
// works and its account (said last), with its status and how long since it last did anything.

import { ago } from '../format';
import { laneWaits } from '../lanes';
import { laneBranch, NO_PROJECT } from '../quick';
import type { Lane, Session } from '../types';
import type { PickerOption } from './Picker';
import { Icon, SessionGlyph, StatusGlyph } from './primitives';

/** A worktree, picked by `id` (its lane id in the quick panel, its folder in a schedule). */
export function laneOption(l: Lane, id: string = l.id): PickerOption {
  return {
    id,
    text: [l.callsign, l.branch ?? '', l.repo, l.project, l.origin === 'main' ? 'main checkout' : ''].join(' '),
    label: l.origin === 'main' ? `${l.callsign} (main checkout)` : l.callsign,
    detail: laneBranch(l),
    lead: <StatusGlyph status={l.glyph} size={10} title="" waiting={laneWaits(l)} />,
  };
}

/** A chat of its own, in no project: Codealong's chats, apart from every worktree. */
export function chatOption(id: string): PickerOption {
  return { id, text: 'no project chat question', label: NO_PROJECT, detail: 'A chat of its own', lead: <Icon name="message-circle" size={12} /> };
}

/** A session, with the worktree it works in. */
export function sessionOption(lane: Lane, s: Session, now: number): PickerOption {
  const where = lane.startsChats ? NO_PROJECT : `${lane.project} · ${lane.callsign}`;
  return {
    id: s.key,
    text: [s.title, lane.project, lane.repo, lane.callsign, lane.branch ?? '', s.account, lane.startsChats ? 'chat' : ''].join(' '),
    label: s.title,
    detail: `${where} · ${s.account}`,
    lead: <SessionGlyph s={s} size={10} title="" />,
    trail: (
      <span className="ca-picker-trail" title={`Last active ${new Date(s.lastActivity).toLocaleString()}`}>
        {s.archived ? 'Archived' : ago(s.lastActivity, now)}
      </span>
    ),
  };
}

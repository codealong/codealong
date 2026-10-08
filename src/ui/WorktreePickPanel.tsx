// Several worktrees picked together on the Worktrees board, each whole (picks.ts), in the pick
// panel (PickPanel.tsx): marking their sessions seen and pinning them, as buttons that say their
// keys, and each of them. The actions are the menu's (menus.ts `lanesActions`).

import { count } from '../format';
import { worktreeName, type WorktreeRow } from '../lanes';
import { KEYS, type LanesActions } from '../menus';
import { PickKeys, PickList, PickPanel, PickRow } from './PickPanel';
import { LaneGlyph } from './primitives';
import { Hotkey } from './SessionPanel';

interface WorktreePickPanelProps {
  /** The worktrees picked, in the board's order. */
  items: WorktreeRow[];
  actions: LanesActions;
  /** Selects only this one. */
  onShow: (item: WorktreeRow) => void;
  /** Takes it out of the ones picked. */
  onLeaveOut: (item: WorktreeRow) => void;
  /** The ⋯ button and a right-click on the header: everything that can be done to them. */
  onMoreMenu: (at?: { x: number; y: number }) => void;
}

export function WorktreePickPanel({ items, actions, onShow, onLeaveOut, onMoreMenu }: WorktreePickPanelProps) {
  const { seen, pin } = actions;
  return (
    <PickPanel
      label="Selected worktrees"
      icon="branch"
      head={count(items.length, 'worktree')}
      help="⌘-click adds a worktree or leaves it out, ⇧-click adds a run of them. Esc goes back to one."
      moreTitle="Everything that can be done to them"
      onMoreMenu={onMoreMenu}
      actions={
        <PickKeys>
          <Hotkey label="Seen" keys={KEYS.seen} title={seen ? `${seen.label}: their turns that ended go idle and leave what waits on you` : 'None of their sessions has anything to mark as seen'} run={seen && (() => seen.run())} />
          <Hotkey label={pin?.label.startsWith('Unpin') ? 'Unpin' : 'Pin'} keys={KEYS.pin} title={pin?.label ?? 'Pin'} run={pin && (() => pin.run())} />
        </PickKeys>
      }
    >
      <PickList>
        {items.map((r) => (
          <PickRow key={r.key} lead={<LaneGlyph lane={r.lane} />} name={worktreeName(r.lane)} title={`${r.lane.path}. Click to show only this worktree.`} onShow={() => onShow(r)} onLeaveOut={() => onLeaveOut(r)}>
            {r.lane.repo !== worktreeName(r.lane) && <span className="ca-tag">{r.lane.repo}</span>}
          </PickRow>
        ))}
      </PickList>
    </PickPanel>
  );
}

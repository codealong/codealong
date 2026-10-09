// Several PRs picked together in PRs (picks.ts), in the pick panel (PickPanel.tsx): what's
// commonly done to all of them at once, as buttons that say their keys, and each of them. The
// actions are the menu's (menus.ts `prsActions`): Approve takes the open ones that aren't yours
// and Snooze the ones that ask for your review, and each says how many where that's fewer than
// all.

import { below, showContextMenu } from '../contextMenu';
import { KEYS, type PrsActions } from '../menus';
import { PickKeys, PickList, PickPanel, PickRow } from './PickPanel';
import { PrGlyph, prIsOpen, type PrRowItem } from './PrsView';
import { Hotkey, snoozeKeys, snoozeRange } from './SessionPanel';

interface PrPickPanelProps {
  /** The PRs picked, in the list's order. */
  items: PrRowItem[];
  actions: PrsActions;
  /** Selects only this one. */
  onShow: (item: PrRowItem) => void;
  /** Takes it out of the ones picked. */
  onLeaveOut: (item: PrRowItem) => void;
  /** The ⋯ button and a right-click on the header: everything that can be done to them. */
  onMoreMenu: (at?: { x: number; y: number }) => void;
}

export function PrPickPanel({ items, actions, onShow, onLeaveOut, onMoreMenu }: PrPickPanelProps) {
  const n = items.length;
  const { github, approve, snooze, pin, takes } = actions;
  // How many it takes, where that's fewer than all of them: Approve 2 of 5.
  const some = (name: keyof typeof takes) => (takes[name] > 0 && takes[name] < n ? ` ${takes[name]}` : '');
  return (
    <PickPanel
      label="Selected PRs"
      icon="pr"
      head={`${n} PRs`}
      help="⌘-click adds a PR or leaves it out, ⇧-click adds a run of them. Esc goes back to one."
      moreTitle="Everything that can be done to them"
      onMoreMenu={onMoreMenu}
      actions={
        <PickKeys>
          <Hotkey label={`Approve${some('approve')}`} title={approve ? `${approve.label}, as you` : 'They’re yours or closed: GitHub doesn’t let you approve your own'} run={approve && (() => approve.run())} />
          {/* A menu that didn't pop up leaves nothing half done, and the snooze keys beside it still work. */}
          <Hotkey
            label={`Snooze${some('snooze')}`}
            keys={snoozeRange()}
            title={snooze ? `${snooze.label}: they're out of Review requested and off the floating bar until their time comes. ${snoozeKeys(snooze)}` : 'None of them asks for your review'}
            run={snooze && ((el) => void showContextMenu(snooze.items, below(el)).catch(() => {}))}
          />
          <Hotkey label="Open" keys="⌘O" title={github?.label ?? 'Open on GitHub'} run={github && (() => github.run())} />
          <Hotkey label={pin?.label.startsWith('Unpin') ? 'Unpin' : 'Pin'} keys={KEYS.pin} title={pin?.label ?? 'Pin'} run={pin && (() => pin.run())} />
        </PickKeys>
      }
    >
      <PickList>
        {items.map((r) => (
          <PickRow
            key={r.key}
            lead={<PrGlyph pr={r.pr} />}
            name={r.pr.title}
            title={`#${r.pr.number} ${r.pr.title}. Click to show only this PR.`}
            onShow={() => onShow(r)}
            onLeaveOut={() => onLeaveOut(r)}
            dim={!prIsOpen(r.pr)}
          >
            <span className="ca-tag">
              {r.pr.repo} #{r.pr.number}
            </span>
          </PickRow>
        ))}
      </PickList>
    </PickPanel>
  );
}

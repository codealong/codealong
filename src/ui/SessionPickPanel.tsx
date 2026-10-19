// Several sessions picked together in Sessions (picks.ts), in the pick panel (PickPanel.tsx):
// what's commonly done to all of them at once, the strip a session's panel has
// (SessionPanel.tsx `Hotkey`) with Pin beside it, led by Carry on where any was cut off (a restart
// leaves many so), and each of them. The actions are the menu's
// (menus.ts `sessionsActions`), and each acts on the ones it applies to: a button says how many
// where that's fewer than all.

import { below, showContextMenu } from '../contextMenu';
import { count } from '../format';
import { CHORDS, chordSigns, type SessionsActions } from '../menus';
import { PickKeys, PickList, PickPanel, PickRow } from './PickPanel';
import { AccountLabel, SessionGlyph, STATUS_WORD, WorktreeChip } from './primitives';
import { Hotkey, snoozeKeys, snoozeRange } from './SessionPanel';
import type { SessionItem } from './SessionsView';

interface SessionPickPanelProps {
  /** The sessions picked, in the list's order. */
  items: SessionItem[];
  actions: SessionsActions;
  /** Selects only this one. */
  onShow: (item: SessionItem) => void;
  /** Takes it out of the ones picked. */
  onLeaveOut: (item: SessionItem) => void;
  /** The ⋯ button and a right-click on the header: everything that can be done to them. */
  onMoreMenu: (at?: { x: number; y: number }) => void;
}

export function SessionPickPanel({ items, actions, onShow, onLeaveOut, onMoreMenu }: SessionPickPanelProps) {
  const n = items.length;
  const { carryOn, seen, done, snooze, close, archive, pin, takes } = actions;
  const sign = (name: keyof typeof CHORDS) => chordSigns(CHORDS[name] ?? '');
  // How many it takes, where that's fewer than all of them: Close 2 of 5.
  const some = (name: keyof typeof takes) => (takes[name] > 0 && takes[name] < n ? ` ${takes[name]}` : '');
  return (
    <PickPanel
      label="Selected sessions"
      icon="stack"
      head={count(n, 'session')}
      help="⌘-click adds a session or leaves it out, ⇧-click adds a run of them. Esc goes back to one."
      moreTitle="Everything that can be done to them"
      onMoreMenu={onMoreMenu}
      actions={
        <PickKeys>
          {carryOn && (
            <Hotkey
              label={`Carry on${some('carryOn')}`}
              title={`${carryOn.label}: each resumes where its account’s sessions last went on, told to carry on the turn that was cut off`}
              run={() => carryOn.run()}
            />
          )}
          <Hotkey label={`Seen${some('seen')}`} keys={sign('seen')} title={seen ? `${seen.label}: they go idle and leave what waits on you` : 'None of them has anything to mark as seen'} run={seen && (() => seen.run())} />
          <Hotkey
            label={`Done${some('done')}`}
            keys={sign('done')}
            title={done ? `${done.label}: each closes where Codealong can close it, and is marked seen and archived` : 'They’re archived already'}
            run={done && (() => done.run())}
          />
          {/* A menu that didn't pop up leaves nothing half done, and the snooze keys beside it still work. */}
          <Hotkey
            label={`Snooze${some('snooze')}`}
            keys={snoozeRange()}
            title={snooze ? `${snooze.label}: they wait apart until their time comes. ${snoozeKeys(snooze)}` : 'None of them can be snoozed'}
            run={snooze && ((el) => void showContextMenu(snooze.items, below(el)).catch(() => {}))}
          />
          <Hotkey label={`Close${some('close')}`} keys={sign('close')} title={close ? `${close.label}: the conversations stay, to go on with later` : 'None of them has a terminal of its own to close'} run={close && (() => close.run())} />
          <Hotkey
            label={`${archive?.label.startsWith('Unarchive') ? 'Unarchive' : 'Archive'}${some('archive')}`}
            keys={sign('archive')}
            title={archive?.label ?? 'Archive'}
            run={archive && (() => archive.run())}
          />
          <Hotkey label={pin?.label.startsWith('Unpin') ? 'Unpin' : 'Pin'} keys={sign('pin')} title={pin?.label ?? 'They’re archived'} run={pin && (() => pin.run())} />
        </PickKeys>
      }
    >
      <PickList>
        {items.map((r) => (
          <PickRow
            key={r.key}
            lead={<SessionGlyph s={r.session} size={12} title={STATUS_WORD[r.session.status]} />}
            name={r.session.title}
            title={`${r.session.title}. Click to show only this session.`}
            onShow={() => onShow(r)}
            onLeaveOut={() => onLeaveOut(r)}
            dim={r.session.archived}
          >
            <span className="ca-chips ca-pick-chips">
              <WorktreeChip lane={r.lane} short />
            </span>
            <AccountLabel session={r.session} />
          </PickRow>
        ))}
      </PickList>
    </PickPanel>
  );
}

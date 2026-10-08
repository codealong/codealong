// Several sessions picked together in Sessions, or lines on the Worktrees board (picks.ts), in
// the pick panel (PickPanel.tsx): what's commonly done to all of them at once, the strip a
// session's panel has (SessionPanel.tsx `Hotkey`) with Pin beside it, and each of them. On the
// board a worktree with no agent line is picked whole, and Pin pins the worktrees. The actions
// are the menu's (menus.ts `sessionsActions`), and each acts on the ones it applies to: a button
// says how many where that's fewer than all.

import { showContextMenu, below } from '../contextMenu';
import { count } from '../format';
import { worktreeName, type BoardRow } from '../lanes';
import { CHORDS, chordSigns, type SessionsActions } from '../menus';
import { PickKeys, PickList, PickPanel, PickRow } from './PickPanel';
import { AccountLabel, LaneGlyph, SessionGlyph, STATUS_WORD, WorktreeChip } from './primitives';
import { Hotkey, snoozeKeys, snoozeRange } from './SessionPanel';

interface SessionPickPanelProps {
  /** The rows picked, in the list's order: sessions, and on the board worktrees with no line. */
  items: BoardRow[];
  actions: SessionsActions;
  /** What Pin pins: the sessions (Sessions), or their worktrees (the board). */
  pins: 'session' | 'worktree';
  /** Selects only this one. */
  onShow: (item: BoardRow) => void;
  /** Takes it out of the ones picked. */
  onLeaveOut: (item: BoardRow) => void;
  /** The ⋯ button: everything that can be done to them. */
  onMoreMenu: (at: { x: number; y: number }) => void;
}

export function SessionPickPanel({ items, actions, pins, onShow, onLeaveOut, onMoreMenu }: SessionPickPanelProps) {
  const sessions = items.filter((r) => r.kind === 'agent').length;
  const worktrees = items.length - sessions;
  const { seen, done, snooze, close, archive, pin, takes } = actions;
  const sign = (name: keyof typeof CHORDS) => chordSigns(CHORDS[name] ?? '');
  // How many it takes, where that's fewer than all of the sessions: Close 2 of 5.
  const some = (name: keyof typeof takes) => (takes[name] > 0 && takes[name] < sessions ? ` ${takes[name]}` : '');
  const head = [sessions > 0 && count(sessions, 'session'), worktrees > 0 && count(worktrees, 'worktree')].filter(Boolean).join(' and ');
  return (
    <PickPanel
      label={pins === 'worktree' ? 'Selected on the board' : 'Selected sessions'}
      icon="stack"
      head={head}
      help={`⌘-click adds a ${pins === 'worktree' ? 'line' : 'session'} or leaves it out, ⇧-click adds a run of them. Esc goes back to one.`}
      moreTitle="Everything that can be done to them"
      onMoreMenu={onMoreMenu}
      actions={
        <PickKeys>
          {sessions > 0 && (
            <>
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
            </>
          )}
          <Hotkey label={pin?.label.startsWith('Unpin') ? 'Unpin' : 'Pin'} keys={sign('pin')} title={pin?.label ?? 'They’re archived'} run={pin && (() => pin.run())} />
        </PickKeys>
      }
    >
      <PickList>
        {items.map((r) =>
          r.kind === 'agent' ? (
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
          ) : (
            <PickRow key={r.key} lead={<LaneGlyph lane={r.lane} />} name={worktreeName(r.lane)} title={`${r.lane.path}. Click to show only this worktree.`} onShow={() => onShow(r)} onLeaveOut={() => onLeaveOut(r)}>
              {r.lane.repo !== worktreeName(r.lane) && <span className="ca-tag">{r.lane.repo}</span>}
            </PickRow>
          ),
        )}
      </PickList>
    </PickPanel>
  );
}

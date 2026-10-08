// Several sessions picked together in Sessions (⌘-click, ⇧-click, ⇧↑ ⇧↓; picks.ts), shown in the
// panel in place of one session's conversation: what's commonly done to all of them at once, as
// buttons that say their keys, the strip a session's panel has (SessionPanel.tsx `Hotkey`), and
// each of them, to show alone or leave out. The actions are the menu's (menus.ts
// `sessionsActions`), and each acts on the ones it applies to: a button says how many where
// that's fewer than all.

import { below, showContextMenu } from '../contextMenu';
import { CHORDS, chordSigns, type SessionsActions } from '../menus';
import { AccountLabel, Button, cx, Icon, SessionGlyph, STATUS_WORD, WorktreeChip } from './primitives';
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
  /** The ⋯ button: everything that can be done to them. */
  onMoreMenu: (at: { x: number; y: number }) => void;
}

export function SessionPickPanel({ items, actions, onShow, onLeaveOut, onMoreMenu }: SessionPickPanelProps) {
  const n = items.length;
  const { seen, done, snooze, close, archive, takes } = actions;
  const sign = (name: keyof typeof CHORDS) => chordSigns(CHORDS[name] ?? '');
  // How many it takes, where that's fewer than all of them: Close 2 of 5.
  const some = (name: keyof typeof takes) => (takes[name] > 0 && takes[name] < n ? ` ${takes[name]}` : '');
  return (
    <aside className="ca-insp ca-sesspick" aria-label="Selected sessions">
      <header className="ca-sesspick-header">
        <div className="ca-insp-title">
          <Icon name="stack" size={14} />
          <span className="ca-tag">Selected</span>
          <Button variant="ghost" size="sm" icon="dots" aria-label="More" title="Everything that can be done to them" className="ca-sesspick-more" onClick={(e) => onMoreMenu(below(e.currentTarget))} />
        </div>
        <h2 className="ca-sesspick-head">{n} sessions</h2>
        <p className="ca-sesspick-help">⌘-click adds a session or leaves it out, ⇧-click adds a run of them. Esc goes back to one.</p>
      </header>
      <div className="ca-hotkeys ca-sesspick-keys" role="toolbar" aria-label="What’s commonly done to them">
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
      </div>
      <section className="ca-insp-sec" aria-label="The sessions selected">
        <ul className="ca-sesspick-list">
          {items.map((r) => {
            const s = r.session;
            return (
              <li key={r.key} className={cx('ca-sesspick-row', s.archived && 'is-archived')}>
                <SessionGlyph s={s} size={12} title={STATUS_WORD[s.status]} />
                <button type="button" className="ca-sesspick-name" title={`${s.title}. Click to show only this session.`} onClick={() => onShow(r)}>
                  {s.title}
                </button>
                <span className="ca-chips ca-sesspick-chips">
                  <WorktreeChip lane={r.lane} short />
                </span>
                <AccountLabel session={s} />
                <button type="button" className="ca-sesspick-out" aria-label={`Leave out ${s.title}`} title="Leave it out" onClick={() => onLeaveOut(r)}>
                  <Icon name="x" size={10} stroke={2.25} />
                </button>
              </li>
            );
          })}
        </ul>
      </section>
    </aside>
  );
}

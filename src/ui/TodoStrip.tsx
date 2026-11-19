// The strip over a todo, wherever a window shows one: its card in the notch (TodoCard.tsx) and its
// panel in the main window's Todo and Kanban views (MainPanel.tsx). It's the strip a session's
// card and panel have (SessionPanel.tsx `QueueStrip`), with what's commonly done to a todo in
// place of a session's: Start, Done (⇧⌘E), Snooze (⌃⌘1 to ⌃⌘7, out of what waits until then, its
// due date as it is) and Due…, each a button that says its key. Done and the snooze times are the
// strip's keys while it shows, as a PR's are its strip's (PrStrip.tsx); Start's is the window's:
// ⌘⏎ in a card, which starts from its box, and ⌘N in the main window, which starts a session for
// the todo selected (mainKeys.ts).
//
// A card's window is worked from the keyboard, so there its keys listen as the event comes back up,
// after what in the card has them for itself. In the main window they go first, as the event comes
// down, ahead of the list's keys for the row selected (PrStrip.tsx says why).

import { useEffect } from 'react';
import { useLatest } from '../latest';
import type { MenuGroup } from '../contextMenu';
import { PLACE_CHORDS } from '../mainKeys';
import * as menus from '../menus';
import { chordPressed, held, keysSaid } from '../platform';
import { snoozeChorded } from '../snooze';
import type { Lane, Todo } from '../types';
import { dialogUp } from './focusTrap';
import type { PanelBackTo } from './primitives';
import { QueueStrip, snoozeKeys, snoozeRange, type Hotkeys, type Notice, type Queue, type StripAction } from './SessionPanel';

/** What's commonly done to a todo shown, by name: what its strip and its keys run. */
export interface TodoActions {
  /** Starts a session on it: from the card's box in its worktree, else the quick panel. */
  start: () => void;
  /** Marks it done; none while that can't be (it's done, or a start is on its way). */
  done?: () => void;
  snooze: MenuGroup;
  due: MenuGroup;
}

interface TodoStripProps {
  todo: Todo;
  /** Where a start goes: the worktree its actions use. */
  lane: Lane | null;
  actions: TodoActions;
  /** A session is starting on it, from a card. */
  sending?: boolean;
  queue?: Queue;
  notice?: Notice | null;
  hotkeys: Hotkeys;
  /** Over its panel in the main window, where the strip's keys go ahead of the list's. */
  panel?: boolean;
  /** In the main window, ⌘N (the window's key) starts a session for the todo selected in the list,
   *  which is this one: the strip says it. Not on the canvas, whose todo isn't the list's. */
  startKey?: boolean;
  back?: PanelBackTo;
}

export function TodoStrip({ todo, lane, actions, sending = false, queue, notice, hotkeys, panel = false, startKey = false, back }: TodoStripProps) {
  const latest = useLatest(actions);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing || dialogUp()) return;
      if (chordPressed(menus.CHORDS.done ?? '', e)) {
        // Taken whether it applies or not, so that the row selected behind it doesn't take it.
        e.preventDefault();
        latest.current.done?.();
        return;
      }
      const time = held(e, 'Ctrl', 'Cmd') ? snoozeChorded(latest.current.snooze, e.key) : null;
      if (time) {
        e.preventDefault();
        time.run();
      }
    };
    window.addEventListener('keydown', onKey, panel);
    return () => window.removeEventListener('keydown', onKey, panel);
  }, [latest, panel]);

  const { start, done, snooze, due } = actions;
  const startKeys = panel ? (startKey ? menus.chordSigns(PLACE_CHORDS.newSession) : undefined) : keysSaid('⌘⏎');
  const buttons: StripAction[] = [
    {
      label: 'Start',
      keys: startKeys,
      title: sending ? 'A session is starting on it' : panel ? `Start a session on it${startKeys ? ` (${startKeys})` : ''}` : lane ? `Start a session on it in ${lane.callsign}, with what’s written in the box at its foot` : 'It’s linked to no worktree: the quick panel asks where to start it',
      run: sending ? undefined : start,
    },
    { label: 'Done', chord: menus.CHORDS.done, title: sending ? 'A session is starting on it: mark it done once it has' : todo.status === 'done' ? 'It’s done already' : 'Mark it done: it leaves what waits on you', run: done },
    { label: 'Snooze', keys: snoozeRange(), title: `Snooze it: it waits apart until its time comes, its due date as it is. ${snoozeKeys(snooze)}`, menu: snooze.items },
    { label: 'Due…', title: 'When it’s due', menu: due.items },
  ];
  return <QueueStrip queue={queue} notice={notice} hotkeys={hotkeys} buttons={buttons} of="this todo" back={back} />;
}

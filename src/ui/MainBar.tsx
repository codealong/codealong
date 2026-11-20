// The main window's title bar (App.tsx; its styles are App.css's): the sidebar's toggle, back and
// forward through the sessions you looked at, and the project switcher at its start; the box that
// opens the quick panel in the middle; and at its end the sessions hearing now, Meetings, where
// the agents bar shows, New session and Improve Codealong. It keeps nothing itself: what each
// control does is the window's.

import type { ReactNode } from 'react';
import type { BarMode } from '../api';
import { improveTitle } from '../improve';
import { hearingNow } from '../listening';
import { meetingWhen } from '../meetings';
import * as menus from '../menus';
import { keysIn } from '../platform';
import { QUEUE_CHORDS } from '../queue';
import { QUICK_PLACEHOLDER, shortcutKeys, shortcutLabel } from '../quick';
import type { BarListening, Board } from '../types';
import { ListeningNow } from './Listening';
import { MeetingCountdown } from './Meeting';
import { Button, Icon, Kbd, Seg } from './primitives';

interface Props {
  board: Board | null;
  now: number;
  sideCollapsed: boolean;
  onToggleSidebar: () => void;
  /** The sessions back and forward go to (history.ts), by key: none where there's none to go to. */
  history: { back: string | null; forward: string | null };
  /** What a history button says: where it goes, and its key. */
  historyTitle: (to: string | null, word: string, none: string, keys: string) => string;
  onTravel: (dir: 'back' | 'forward') => void;
  /** The project switcher (ProjectSwitcher.tsx). */
  switcher: ReactNode;
  /** Opens the quick panel, as ⌘K does. */
  onQuick: () => void;
  /** Shows a session that's hearing; and Listen now, while voice is on. */
  onShowListening: (l: BarListening) => void;
  onListen: () => void;
  onMeetings: () => void;
  /** Where the agents bar shows, and a pick of one of its places (the one it's in turns it off). */
  barPlace: BarMode;
  onBarPlace: (picked: Exclude<BarMode, 'off'>) => void;
  /** What New session starts on: how many todos are picked together, or whether one is selected. */
  pickedTodos: number;
  todoSelected: boolean;
  onNew: () => void;
  onImprove: () => void;
}

export function MainBar({ board, now, sideCollapsed, onToggleSidebar, history, historyTitle, onTravel, switcher, onQuick, onShowListening, onListen, onMeetings, barPlace, onBarPlace, pickedTodos, todoSelected, onNew, onImprove }: Props) {
  // The quick panel's shortcut from any app, which the box in the title bar shows; ⌘K without one.
  const quickKeys = shortcutKeys(board?.quickShortcut);
  return (
    <div className="ca-win-bar" data-tauri-drag-region>
      <div className="ca-win-bar-start" data-tauri-drag-region>
        <button
          type="button"
          className={sideCollapsed ? 'ca-sidetoggle' : 'ca-sidetoggle is-on'}
          aria-label={sideCollapsed ? 'Show sidebar' : 'Hide sidebar'}
          aria-pressed={!sideCollapsed}
          title={keysIn(`${sideCollapsed ? 'Show' : 'Hide'} the sidebar (⌃⌘S)`)}
          onClick={onToggleSidebar}
        >
          <Icon name="layout-sidebar" size={16} />
        </button>
        <div className="ca-histnav" role="group" aria-label="Sessions you looked at">
          <button
            type="button"
            aria-label="Back"
            aria-disabled={!history.back}
            title={historyTitle(history.back, 'Back', 'Back to the session you looked at before', menus.chordSigns(QUEUE_CHORDS.historyBack[0]))}
            onClick={() => onTravel('back')}
          >
            <Icon name="chevron-left" size={16} />
          </button>
          <button
            type="button"
            aria-label="Forward"
            aria-disabled={!history.forward}
            title={historyTitle(history.forward, 'Forward', 'Forward again, after going back', menus.chordSigns(QUEUE_CHORDS.historyForward[0]))}
            onClick={() => onTravel('forward')}
          >
            <Icon name="chevron-right" size={16} />
          </button>
        </div>
        {switcher}
      </div>
      {/* The way into the app: the quick panel (ui/Quick.tsx), as ⌘K and its own shortcut open it,
          to find a session, a todo or a PR anywhere or to start new work where ⌘N would. It says
          what the panel's box says, and the panel's shortcut. Finding in the window is ⌘F's
          (ui/WindowFind.tsx), and filtering the list ⌥⌘F's (ui/ListBar.tsx). */}
      <button
        type="button"
        className="ca-search"
        title={keysIn(`Find a session, a todo or a PR, or describe new work (⌘K${quickKeys ? `, or ${shortcutLabel(board?.quickShortcut)} from any app` : ''})`)}
        onClick={onQuick}
      >
        <Icon name="search" size={14} />
        <span>{QUICK_PLACEHOLDER}</span>
        <Kbd keys={quickKeys ?? ['⌘', 'K']} />
      </button>
      <div className="ca-win-bar-end" data-tauri-drag-region>
        {/* The sessions hearing now (listen.rs), whatever the window shows: a click shows one.
            While none hears, and voice is on, Listen now is in their place (meetings.rs `now`):
            the button that starts a listening is where the light that says one is going shows. */}
        <ListeningNow
          listenings={hearingNow(board?.lanes ?? [], board?.listenings)}
          onShow={onShowListening}
          onListen={board?.experimental?.voice ? onListen : undefined}
          shortcut={shortcutLabel(board?.listenShortcut)}
        />
        {/* With a meeting counted down to (meetings.rs `counting_down`), the minutes to go are
            on it, and its dialog opens on that meeting (meetings.rs `Ahead::takeable`). */}
        {board?.experimental?.voice && (
          <Button
            icon="calendar"
            aria-label="Meetings"
            title={board.meetingSoon ? `${board.meetingSoon.title.trim() || 'A meeting'}, ${meetingWhen(board.meetingSoon, now)}: take its notes from Meetings` : 'Meetings: the notes and transcripts of the ones Codealong took'}
            onClick={onMeetings}
          >
            {board.meetingSoon ? <MeetingCountdown meeting={board.meetingSoon} /> : undefined}
          </Button>
        )}
        <Seg
          className="ca-barplace"
          label="Agents bar"
          toggles
          look="icon"
          iconSize={15}
          options={[
            { id: 'floating', label: 'Floating bar', icon: 'capsule-horizontal', title: barPlace === 'floating' ? 'Hide the floating bar' : 'Floating bar: the agents that need you, on top of every window, where you put it' },
            { id: 'notch', label: 'Bar in the notch', icon: 'notch', title: barPlace === 'notch' ? 'Hide the bar in the notch' : 'In the notch: the agents that need you, around the notch at the top of the screen, opening when you point at it' },
            { id: 'menubar', label: 'Menu bar icon', icon: 'menu-bar', title: barPlace === 'menubar' ? 'Take Codealong out of the menu bar' : 'In the menu bar: an icon with the agents that need you in its menu, the server, and quitting with it or without it' },
          ]}
          value={barPlace === 'off' ? null : barPlace}
          onPick={onBarPlace}
        />
        <Button
          icon="plus"
          aria-label="New session"
          title={keysIn(pickedTodos > 0 ? `Start one session for the ${pickedTodos} selected todos (⌘N)` : todoSelected ? 'Start a session for the selected todo (⌘N)' : 'Start a session in the selected worktree (⌘N)')}
          onClick={onNew}
        />
        {/* Improve Codealong (improve.ts): the quick panel on Codealong's own project, with pictures
            of its windows as they are, for a session that changes Codealong itself. */}
        <Button icon="bulb" className="ca-improve" aria-label="Improve Codealong" title={improveTitle(shortcutLabel(board?.improveShortcut))} onClick={onImprove} />
      </div>
    </div>
  );
}

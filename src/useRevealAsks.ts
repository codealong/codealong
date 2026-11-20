// What the main window is asked to show from outside it (App.tsx `reveal`): by the floating bar,
// the quick panel or a card on the desktop (`onReveal`), and by a link to a session or a todo
// clicked in another app, which macOS opens with Codealong (front.rs `open_links`). A link is
// taken when the app says there's one, and once this window hears what it's told to show, since a
// click that launched the app came before there was a page to tell.

import { useCallback, useEffect, useRef, useState } from 'react';
import { onReveal, takeOpenedLink, type Reveal } from './api';
import { mayYetCome, opened } from './codealongLinks';
import type { Board, DevicesView, Lane, Todo } from './types';
import { linkHost } from './ui/PrLink';

/** How long a link macOS opened waits for your other devices to be read, and to have read their
 *  own sessions, before its session is said to be missing (useDevices.ts): they come a moment
 *  after the board. */
export const DEVICES_WAIT_MS = 5000;

/** What the window has for them. */
export interface RevealAsksInput {
  board: Board | null;
  /** Your other devices, and their worktrees: a link's session may be on one of them. */
  devices: DevicesView | null;
  deviceLanes: Lane[];
  /** Shows what's asked for, as the latest render has it. */
  reveal: { readonly current: (r: Reveal) => void };
  setToast: (message: string) => void;
  todoStore: { todos: Todo[] };
}

/** Hears what's asked to be shown, and shows the link macOS opened. Answers the way to take a
 *  link the app says there is (a reveal of kind `link`). */
export function useRevealAsks({ board, devices, deviceLanes, reveal, setToast, todoStore }: RevealAsksInput): () => void {
  const [openedLink, setOpenedLink] = useState<string | null>(null);
  // Until when a link's session is looked for among your other devices, which come after the
  // board; 0 while no link waits for them.
  const linkWaits = useRef(0);
  const [linkWaited, setLinkWaited] = useState(0);
  const takeLink = useCallback(() => {
    // One that can't be asked for is as if none was clicked: there's nothing to say it about.
    void takeOpenedLink().then((url) => {
      if (!url) return;
      linkWaits.current = 0;
      setOpenedLink(url);
    }, () => {});
  }, []);

  // A link that came before this window could hear of it is asked for once it can.
  useEffect(() => onReveal((r) => reveal.current(r), takeLink), [reveal, takeLink]);

  // The link macOS opened, shown once the board has been read, as a click on one in a conversation
  // shows it (SessionLink.tsx): its session among those this window's links name (`linkHost`),
  // here or on your other devices, or its todo. It only ever shows something. A session that
  // isn't listed may be on another of your devices, which this window reads after the board
  // (useDevices.ts) and which may itself still be reading its sessions: it's looked for again as
  // they come, and said to be missing only once they have, or haven't for a while.
  useEffect(() => {
    if (!openedLink || !board || board.loading) return;
    const went = opened(openedLink, linkHost.sessions(), todoStore.todos);
    if ('none' in went && went.awaits !== null && mayYetCome(went.awaits, devices)) {
      linkWaits.current ||= Date.now() + DEVICES_WAIT_MS;
      const left = linkWaits.current - Date.now();
      if (left > 0) {
        const again = window.setTimeout(() => setLinkWaited((n) => n + 1), left);
        return () => window.clearTimeout(again);
      }
    }
    linkWaits.current = 0;
    setOpenedLink(null);
    if ('session' in went) reveal.current({ kind: 'session', laneId: went.session.lane.id, key: went.session.session.key });
    else if ('todo' in went) reveal.current({ kind: 'todo', id: went.todo.id });
    else setToast(went.none);
  }, [openedLink, board, devices, deviceLanes, linkWaited, reveal, setToast, todoStore.todos]);

  return takeLink;
}

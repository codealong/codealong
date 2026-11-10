// A new session that waits for its first prompt (start.ts `startsOnFirstSend`): a start from the
// quick panel with nothing typed, in the background or in Codealong, where nothing of its own
// opens to write the prompt in. The panel hands it to the main window, whose Sessions panel shows
// it in place of the selected row's (NewSession.tsx) with its box ready, as the Claude app's
// new-session page and Codex's new-thread page are: nothing starts until you send. The send
// starts it as the quick panel would have, and the panel stays, saying it's starting, until the
// board lists the session, which is then selected with the cursor in its box.

import type { Harness, Lane, StartSpec } from './types';

/** A start left for its first prompt: the start as it goes, on no prompt yet, what sending does,
 *  in the start row's words (start.ts `startDoes`), the place as the start row's chip says it
 *  (Background), and the account's harness, which a skill's name is written for as it's sent
 *  (skills.ts `writtenFor`). */
export interface NewSession {
  spec: StartSpec;
  does: string;
  placeLabel: string;
  harness: Harness | null;
}

/** A new session as the main window shows it: over the row that was selected as it opened, which
 *  it shows in place of (`newSessionAfter`), and once sent, the session the send started, by
 *  key, which it gives way to once the board lists it (`NEW_SESSION_WAIT_MS`). */
export interface NewSessionShown {
  draft: NewSession;
  over: string | null;
  /** When it opened: each open of it is a new panel, which takes the keyboard. */
  opened: number;
  started: { key: string; at: number; message: string | null } | null;
}

/** How long a session the send started has to show up on the board before the panel stops
 *  waiting for it and says what the start said instead. A background job is listed within a
 *  second or two; one on another Mac comes with that Mac's next board. */
export const NEW_SESSION_WAIT_MS = 30_000;

/** Where what's written to it is kept (drafts.ts): one per worktree, so starting another there
 *  brings back what you'd written, whichever account it's for. */
export function newSessionDraft(spec: Pick<StartSpec, 'laneId'>): string {
  return `new:${spec.laneId}`;
}

/** The new session once the selection is `selected`: it stays while that's the row it opened
 *  over, and goes once another row is picked. One that opened with nothing selected takes the row
 *  the window selects by itself as its list comes in (App.tsx selects the top one). */
export function newSessionAfter(shown: NewSessionShown | null, selected: string | null): NewSessionShown | null {
  if (!shown || shown.over === selected) return shown;
  return shown.over === null ? { ...shown, over: selected } : null;
}

/** The worktree that lists the session `key`, among `lanes`: where the board shows the session
 *  a send started, once it does. */
export function listedIn(lanes: Lane[], key: string): Lane | null {
  return lanes.find((l) => l.sessions.some((s) => s.key === key)) ?? null;
}

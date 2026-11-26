// A start the quick panel left for its first prompt, as the main window keeps it (App.tsx;
// newSession.ts has the rules): Sessions' panel shows it in place of the selected row's until
// another row is picked, and once sent, until the board lists the session it started, which then
// takes its place with the cursor in its box. One the quick panel sent already starts as it opens.

import { useEffect, useState } from 'react';
import { startFromQuick, type Reveal } from './api';
import { readDraft, writeDraft } from './drafts';
import { useLatest } from './latest';
import { forOpen, listedIn, NEW_SESSION_WAIT_MS, newSessionAfter, newSessionDraft, type NewSession, type NewSessionShown } from './newSession';
import { errorText } from './report';
import type { Board, Lane, Started } from './types';

/** What the window has for it. */
export interface NewSessionInput {
  /** The row selected, by its key: the new session shows in its place. */
  selectedKey: string | null;
  board: Board | null;
  /** Other devices' worktrees, where a session started there is listed. */
  deviceLanes: Lane[];
  /** Shows a session in Sessions, as another window asks for one. */
  reveal: { readonly current: (r: Reveal) => void };
  /** The row whose box takes the cursor. */
  setBoxFocus: (key: string) => void;
  setToast: (message: string) => void;
}

export function useNewSession({ selectedKey, board, deviceLanes, reveal, setBoxFocus, setToast }: NewSessionInput) {
  const [newShown, setNewShown] = useState<NewSessionShown | null>(null);
  useEffect(() => setNewShown((n) => newSessionAfter(n, selectedKey)), [selectedKey]);
  const showStarted = useLatest(() => {
    const started = newShown?.started;
    if (!started) return;
    const lane = listedIn([...(board?.lanes ?? []), ...deviceLanes], started.key);
    if (lane) {
      setNewShown(null);
      reveal.current({ kind: 'session', laneId: lane.id, key: started.key });
      setBoxFocus(`${lane.id}::${started.key}`);
    } else if (Date.now() - started.at >= NEW_SESSION_WAIT_MS) {
      setNewShown(null);
      setToast(started.message ?? 'It started, and shows here once it’s listed.');
    }
  });
  useEffect(() => showStarted.current(), [board, deviceLanes, newShown, showStarted]);
  /** Starts the new session `shown` on `prompt` (`written` is what was typed, shown while it
   *  goes): what the start answers goes to that open alone. Rejects when it fails. */
  const sendNew = (shown: NewSessionShown) => async (prompt: string, written: string) => {
    const mine = (change: (n: NewSessionShown) => NewSessionShown | null) => setNewShown((n) => forOpen(n, shown.opened, change));
    mine((n) => ({ ...n, sending: written, error: null }));
    let started: Started | null;
    try {
      started = await startFromQuick({ ...shown.draft.spec, prompt });
    } catch (e) {
      mine((n) => ({ ...n, sending: null, error: errorText(e) }));
      throw e;
    }
    // A harness that names its own sessions, or one that hasn't said yet, gives no key to wait
    // for: what the start said goes instead.
    const key = started?.key;
    if (!key) {
      mine(() => null);
      if (started?.message) setToast(started.message);
      return;
    }
    const message = started?.message ?? null;
    mine((n) => ({ ...n, started: { key, at: Date.now(), message } }));
    window.setTimeout(() => showStarted.current(), NEW_SESSION_WAIT_MS + 100);
  };
  /** Shows the new session `draft` over the row `over`: waiting for its first prompt, or, sent on
   *  one in the quick panel already (`sent`), starting on it there and then. */
  const openNew = (draft: NewSession, over: string | null) => {
    const shown: NewSessionShown = { draft, over, opened: Date.now(), sending: null, error: null, started: null };
    setNewShown(shown);
    if (draft.sent === undefined) return;
    const sent = draft.sent;
    // A failure stays in the panel, saying why, with what was written in its box to send again,
    // and kept for the worktree as the panel's own send keeps it, in case another view shows.
    sendNew(shown)(draft.spec.prompt, sent).catch(() => {
      if (!readDraft(newSessionDraft(draft.spec))) writeDraft(newSessionDraft(draft.spec), sent);
    });
  };
  return { newShown, setNewShown, sendNew, openNew };
}

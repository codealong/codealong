// The box of questions under a session's conversation (SessionReply.tsx `Approval`), as the
// conversation above it sees it. While the box shows the questions a session waits on you for,
// the conversation's line for that call (SessionThread.tsx `ToolLine`) opens no copy of them, which
// couldn't be answered: once the box is minimized, a click on the line shows the box again. The
// line looks like the box's own head, and a copy opened from it read as the box losing its choices
// (asked 2026-11-04).
//
// Each window keeps its own: a box and the conversation above it are in the same window.

import { useSyncExternalStore } from 'react';

/** The call whose questions you minimized, by session, so its box stays minimized as you go to
 *  another session and back. The next call it waits on shows whole, whatever it asks. */
const minimized = new Map<string, string>();
/** The call whose questions each session's box shows, while it shows some. */
const shown = new Map<string, string>();
const heard = new Set<() => void>();

function tell() {
  for (const h of heard) h();
}

function hear(h: () => void): () => void {
  heard.add(h);
  return () => heard.delete(h);
}

/** Says a session's box shows `call`'s questions, until the function it returns is called. */
export function showQuestions(key: string, call: string): () => void {
  shown.set(key, call);
  tell();
  return () => {
    // A box that has gone on to the next call already said so.
    if (shown.get(key) !== call) return;
    shown.delete(key);
    tell();
  };
}

/** Minimizes the box that shows `call`'s questions to its head, or shows it whole again. */
export function foldQuestions(key: string, call: string, on: boolean) {
  if (on) minimized.set(key, call);
  else if (minimized.get(key) === call) minimized.delete(key);
  else return;
  tell();
}

/** Shows a session's box of questions whole again, where one shows them minimized. */
export function unfoldQuestions(key: string) {
  const call = shown.get(key);
  if (call !== undefined) foldQuestions(key, call, false);
}

/** What a session's box does with the questions it waits on you for: shows them (`open`), shows
 *  them minimized (`folded`), or shows none (null). */
export function questionsBox(key: string): 'open' | 'folded' | null {
  const call = shown.get(key);
  if (call === undefined) return null;
  return minimized.get(key) === call ? 'folded' : 'open';
}

/** Whether `call`'s questions are minimized in a session's box. */
export function useQuestionsFolded(key: string, call: string | null): boolean {
  return useSyncExternalStore(hear, () => call !== null && minimized.get(key) === call);
}

/** `questionsBox`, as it changes. */
export function useQuestionsBox(key: string): 'open' | 'folded' | null {
  return useSyncExternalStore(hear, () => questionsBox(key));
}

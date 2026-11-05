// The box of questions under a session's conversation (SessionReply.tsx `Approval`), as the
// conversation above it sees it. While the box shows the questions a session waits on you for,
// the conversation's line for that call (SessionThread.tsx `ToolLine`) opens no copy of them, which
// couldn't be answered: once the box is minimized, a click on the line shows the box again. The
// line looks like the box's own head, and a copy opened from it read as the box losing its choices
// (asked 2026-11-04).
//
// The line is known for the box's by the questions it asks, not only by its session: a session
// can wait on two calls at once (a subagent's and its own), and the box shows the one asked first
// where the conversation marks the latest. Each window keeps its own: a box and the conversation
// above it are in the same window.

import { useSyncExternalStore } from 'react';
import type { Question } from './types';

/** The questions a session's box shows: its call, and what it asks (`askedOf`). */
export interface ShownQuestions {
  call: string;
  asked: string;
}

/** The call whose questions you minimized, by session, so its box stays minimized as you go to
 *  another session and back. The next call it waits on shows whole, whatever it asks. */
const minimized = new Map<string, string>();
/** What each session's box shows, while it shows questions. */
const shown = new Map<string, ShownQuestions>();
const heard = new Set<() => void>();

function tell() {
  for (const h of heard) h();
}

function hear(h: () => void): () => void {
  heard.add(h);
  return () => heard.delete(h);
}

/** What a set of questions asks, to know a call's line in the conversation for the box's: the
 *  questions' own words, which the call's record and what the box was handed have alike. */
export function askedOf(questions: Pick<Question, 'question'>[]): string {
  return questions.map((q) => q.question.trim()).join('\n');
}

/** Says a session's box shows `call`'s questions, until the function it returns is called. */
export function showQuestions(key: string, call: string, asked: string): () => void {
  const showing = { call, asked };
  shown.set(key, showing);
  tell();
  return () => {
    // A box that has gone on to the next call already said so.
    if (shown.get(key) !== showing) return;
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

/** What a session's box shows, while it shows questions. */
export function shownQuestions(key: string): ShownQuestions | null {
  return shown.get(key) ?? null;
}

/** Whether `call`'s questions are minimized in a session's box. */
export function questionsFolded(key: string, call: string | null): boolean {
  return call !== null && minimized.get(key) === call;
}

/** `shownQuestions`, as it changes. */
export function useShownQuestions(key: string): ShownQuestions | null {
  return useSyncExternalStore(hear, () => shownQuestions(key));
}

/** `questionsFolded`, as it changes. */
export function useQuestionsFolded(key: string, call: string | null): boolean {
  return useSyncExternalStore(hear, () => questionsFolded(key, call));
}

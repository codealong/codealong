// The call a session waits on you for, as the box under its conversation shows it (SessionReply.tsx
// `Approval`: questions to answer or a call to allow), and as the conversation above it sees it.
// While the box shows the call, the conversation leaves out its line (SessionThread.tsx), which
// said it again just above the box: the box stands in its place at the conversation's end, and
// minimized, its head still says the question and shows the box again (asked 2026-11-13). Before,
// the line stayed and opened no copy of the questions, since a copy read as the box losing its
// choices (asked 2026-11-04). Once it's answered, the call is a line of the conversation as any
// other is.
//
// The line is known for the box's by what the call asks (`askedOf`, `calledOf`), not only by its
// session: a session can wait on two calls at once (a subagent's and its own), and the box shows
// the one asked first where the conversation marks the latest. One the box doesn't show keeps its
// line. Each window keeps its own: a box and the conversation above it are in the same window.

import { useSyncExternalStore } from 'react';
import type { Question } from './types';

/** The call a session's box shows: its id there, and what it asks (`askedOf`, `calledOf`). */
export interface ShownCall {
  call: string;
  asked: string;
}

/** The call whose questions you minimized, by session, so its box stays minimized as you go to
 *  another session and back. The next call it waits on shows whole, whatever it asks. */
const minimized = new Map<string, string>();
/** What each session's box shows, while it shows a call. */
const shown = new Map<string, ShownCall>();
const heard = new Set<() => void>();

function tell() {
  for (const h of heard) h();
}

function hear(h: () => void): () => void {
  heard.add(h);
  return () => heard.delete(h);
}

/** What a set of questions asks, to know a call's line in the conversation for the box's: each
 *  question's words and its choices' labels, which the call's record and what the box was handed
 *  have alike. Not its header nor whether it takes several, which a background job's record
 *  doesn't say (claude/registry.rs `job_questions`). */
export function askedOf(questions: Pick<Question, 'question' | 'options'>[]): string {
  return JSON.stringify(questions.map((q) => [q.question.trim(), q.options.map((o) => o.label.trim())]));
}

/** What a call to allow asks, to know its line in the conversation for the box's: its label, which
 *  the conversation's line has too where the harness makes both alike (Claude Code's
 *  `tool_request`). Where they differ, the line stays. */
export function calledOf(label: string): string {
  return JSON.stringify(label.trim());
}

/** Says a session's box shows `call`, until the function it returns is called. */
export function showCall(key: string, call: string, asked: string): () => void {
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

/** What a session's box shows, while it shows a call. */
export function shownCall(key: string): ShownCall | null {
  return shown.get(key) ?? null;
}

/** Whether `call`'s questions are minimized in a session's box. */
export function questionsFolded(key: string, call: string | null): boolean {
  return call !== null && minimized.get(key) === call;
}

/** `shownCall`, as it changes. */
export function useShownCall(key: string): ShownCall | null {
  return useSyncExternalStore(hear, () => shownCall(key));
}

/** `questionsFolded`, as it changes. */
export function useQuestionsFolded(key: string, call: string | null): boolean {
  return useSyncExternalStore(hear, () => questionsFolded(key, call));
}

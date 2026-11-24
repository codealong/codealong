// The call a session waits on you for, as the box under its conversation shows it (SessionReply.tsx
// `Approval`: questions to answer or a call to allow), and as the conversation above it sees it.
// While the box shows the call, the conversation leaves out its line (SessionThread.tsx), which
// said it again just above the box: the box stands in its place at the conversation's end, and
// minimized, its head still says the question and shows the box again (asked 2026-11-13). Before,
// the line stayed and opened no copy of the questions, since a copy read as the box losing its
// choices (asked 2026-11-04). Once it's answered, the call is a line of the conversation as any
// other is. Where the conversation hasn't the call yet (Claude Code writes a question to its
// record once it's answered), the line it ends on in its place, the session's excerpt, is left out
// too while the box shows a call (asked 2026-11-15).
//
// The line is known for the box's by its call id when both sides keep the same one (ACP's
// `toolCallId` on Claude Code's `toolu_…`, or a step index), else by what the call asks
// (`askedOf`, `calledOf`), not only by its session: a session can wait on two calls at once (a
// subagent's and its own), and the box shows the one asked first where the conversation marks
// the latest. One the box doesn't show keeps its line. Each window keeps its own: a box and the
// conversation above it are in the same window.

import { useSyncExternalStore } from 'react';
import type { Question, TranscriptItem } from './types';

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

/** A tool's name from its line, without the summary or inline-code detail after ` · `, so
 *  `run_command · `…`` and `Run command · `…`` name the same tool while `linear · create_issue`
 *  and `linear · delete_issue` keep their tool names distinct. */
function toolName(label: string): string {
  const trimmed = label.trim();
  const sep = trimmed.indexOf(' · ');
  const base = sep >= 0 && !/^[A-Za-z0-9_-]+ · [A-Za-z0-9_-]+$/.test(trimmed) ? trimmed.slice(0, sep) : trimmed;
  const head = base
    .replace(/^mcp__/i, '')
    .replace(/__/g, ' · ')
    .replace(/[_-]+/g, ' ')
    .replace(/\s+/g, ' ')
    .toLowerCase();
  return head === 'bash' || head === 'shell' || head === 'run command' || head === 'exec command' ? 'shell' : head;
}

/** What a call to allow asks, to know its line in the conversation for the box's: its tool name
 *  and its input when it has input (since a permission prompt and the session's record can word
 *  or clip the line's label differently: ACP's `run_command · `…`` vs Antigravity's
 *  `Run command · `…``, or `Bash` without `cd … &&`), and its whole label when it has none. The
 *  input as well, since a label can leave out what the call was given (an MCP tool's is its name
 *  alone): two calls of one tool waiting at once have the same label, and the box shows the one
 *  asked first. Where they differ, the line stays. */
export function calledOf(label: string, input: string | null | undefined): string {
  const body = (input ?? '').trim();
  return JSON.stringify([body ? toolName(label) : label.trim(), body]);
}

/** Whether the box shows the call a line of the conversation is for, by its call id when both
 *  sides keep the same one, else by what each asks. */
export function showsCall(box: ShownCall | null, call: Pick<TranscriptItem, 'id' | 'text' | 'input' | 'questions'>): boolean {
  if (!box) return false;
  if (call.questions?.length) return box.asked === askedOf(call.questions);
  if (box.asked.startsWith('[[')) return false;
  if (call.id && box.call === call.id) return true;
  return box.asked === calledOf(call.text, call.input);
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

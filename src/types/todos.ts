// Todos and the labels they wear (src-tauri/src/todos.rs), what Jev says of one, and the
// attachments kept for a description or a prompt (attachments.rs).

import type { Harness } from './session';

/** Why a todo looks done: how sure Jev is, and what showed it. */
export interface TodoHint {
  p: number;
  why: string;
}

/** What Jev suggests for a todo, each with how likely it is, the likeliest first. */
export interface TodoSuggested {
  projects: [string, number][];
  labels: [string, number][];
  dependsOn: [string, number][];
}

export type TodoStatus = 'todo' | 'in-progress' | 'done';

/** A kept attachment (src-tauri/src/attachments.rs): an image shown in place, or any other file. */
export interface Attachment {
  kind: 'image' | 'file';
  /** How text refers to it: `images/<hash>.<ext>` or `files/<hash>/<name>`. */
  path: string;
  /** The file's own name. */
  name: string;
}

/** The label colors; each is a `--label-<color>` token. */
export const LABEL_COLORS = ['gray', 'red', 'pink', 'purple', 'indigo', 'blue', 'teal', 'green'] as const;
export type LabelColor = (typeof LABEL_COLORS)[number];

export interface Label {
  id: string;
  name: string;
  color: LabelColor;
}

/** A start in the Claude or Codex app, waiting for its session to show up. */
export interface PendingStart {
  lane: string;
  harness: Harness;
  account: string;
  at: number;
}

/** A todo marked as waiting, on something outside the list. */
export interface Waiting {
  /** What it waits for, in a few words; can be empty. */
  note: string;
  /** When it was marked; 0 when written by hand without a time. */
  since: number;
}

/** A todo marked as ready for review (todos.rs `TodoReview`): its agent's part is done, and it
 *  waits on your verdict. */
export interface TodoReview {
  /** What to look at, in a few words; can be empty. */
  note: string;
  /** When it was marked; 0 when written by hand without a time. */
  since: number;
}

/** Work you mean to hand an agent. */
export interface Todo {
  id: string;
  /** Empty until you pick a project or link a worktree. */
  project: string;
  title: string;
  /** Plain text; images are `![…](images/<name>)`. */
  description: string;
  status: TodoStatus;
  /** Label ids. */
  labels: string[];
  /** Linked worktrees (lane ids). */
  worktrees: string[];
  /** Linked session keys. */
  sessions: string[];
  createdAt: number;
  updatedAt: number;
  doneAt: number | null;
  pending: PendingStart[];
  /** Ids of the todos this one waits on, in the order added. Missing on older boards. An id
   *  whose todo was deleted stays (so the delete can be undone) and is ignored. */
  dependsOn: string[];
  /** Marked as waiting, on something outside the list. Missing or null when it isn't. */
  waiting?: Waiting | null;
  /** Marked as ready for review: only an in-progress todo is. Missing or null when it isn't. */
  review?: TodoReview | null;
  /** The day it's due, as `2026-10-03` (src/due.ts). Missing or null when it has none. */
  due?: string | null;
  /** Its primary session: where the work on it goes on, which other sessions send what they
   *  find instead of taking it on too (todos.rs `Primary`). Kept after that session stops, as
   *  the one that had it last. Missing or null when no session has been on it. */
  primary?: TodoPrimary | null;
  /** When it was archived: it's under Archived and in no other list, keeping its status
   *  (todos.rs). Missing or null when it isn't. */
  archivedAt?: number | null;
  /** Out of what waits on you until then, epoch milliseconds, its due date as it is (todos.rs
   *  `snoozed_until`). Missing when it isn't snoozed. */
  snoozedUntil?: number | null;
}

export interface TodoPrimary {
  /** Its key. */
  session: string;
  /** When it became the primary one. */
  since: number;
}

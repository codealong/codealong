// What Close idle sessions and Close stale terminals would close, and what closing did. Mirrors
// src-tauri/src/idle.rs and stale.rs.

import type { Harness } from './session';

/** How a session Close idle sessions lists ends: its terminal tab closes, its background job
 *  stops, or nothing Codealong can do ends it and it's only archived (idle.rs `Ends`). */
export type IdleEnds = 'terminal' | 'job' | 'archive';

/** A session that Close idle sessions would close, or a terminal, named by the session it shows
 *  (idle.rs). */
export interface IdleSession {
  key: string;
  title: string;
  account: string;
  harness: Harness;
  ends: IdleEnds;
  pid: number | null;
  cwd: string;
  worktree: string;
  callsign: string;
  lastActivity: number;
  /** Why it can close, in words, and where it stays open when it's only archived. */
  why: string;
  /** The other sessions its terminal shows, which close with it. */
  with?: string[];
}

/** A live session that stays open, and why. */
export interface KeptSession {
  key: string;
  title: string;
  account: string;
  why: string;
}

export interface IdlePlan {
  afterMs: number;
  /** Whether what closes is archived too, and what can't close is archived only. */
  archive: boolean;
  close: IdleSession[];
  kept: KeptSession[];
}

/** What a tab Close stale terminals lists shows: its shell at its prompt, sessions on the board,
 *  or another program in its foreground (stale.rs `Shows`). */
export type StaleShows = 'shell' | 'session' | 'program';

/** A terminal tab nothing was typed or printed in for the wait (stale.rs). */
export interface StaleTab {
  /** Its terminal device and its shell, `ttys004:4821`: what closing it goes by. */
  key: string;
  /** Ghostty, iTerm2 or Terminal. */
  terminal: string;
  shows: StaleShows;
  /** The session it shows, the program in its foreground, or its folder. */
  title: string;
  /** The sessions it shows, newest first. */
  sessions?: string[];
  pid: number;
  cwd: string;
  opened: number;
  used: number;
  /** Ticked to close at first: closing it loses nothing. */
  ticked: boolean;
  /** Why it may close, or what closing it ends. */
  why: string;
}

export interface StalePlan {
  afterMs: number;
  /** The longest unused first. */
  tabs: StaleTab[];
  /** How many tabs were used since. */
  fresh: number;
}

/** What closing one tab did. */
export interface StaleOutcome {
  key: string;
  title: string;
  closed: boolean;
  message: string;
}

/** What closing one session did. */
export interface CloseOutcome {
  key: string;
  title: string;
  /** What was to end it, when the plan still listed it. */
  ends?: IdleEnds;
  /** Its terminal closed or its job stopped. */
  closed: boolean;
  archived: boolean;
  /** Everything asked of it was done. */
  ok: boolean;
  message: string;
}

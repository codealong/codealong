// Side conversations: questions about a session or a PR, answered by a throwaway copy while the
// session carries on. Mirrors src-tauri/src/side.rs. `SideAnswer`, a session's field, is in
// session.ts with the rest of the model.

/** One question asked on the side, and its answer (side.rs). */
export interface SideTurn {
  question: string;
  /** The answer so far. */
  answer: string;
  /** What the copy did to answer: its tool calls, a line each, in order. */
  tools?: string[];
  /** The tool it's running now, as a line. */
  tool?: string;
  done: boolean;
  /** Why it ended without an answer, or with half of one. */
  error?: string;
  /** When it was asked. */
  at?: number;
}

/** A session's side conversation (side.rs): questions about it, answered by a throwaway copy of
 *  its conversation while the session carries on, unaware. A PR's questions are one too, answered
 *  by an agent of their own. With no turns there is none. */
export interface Side {
  /** Which of the session's side conversations: its record's id (`SideKept`). */
  id: string;
  /** The session it's about, or the PR (`owner/name#number`). */
  key: string;
  turns: SideTurn[];
  /** The copy is answering. */
  running: boolean;
  /** The copy is gone: what was said can still be read, and the next question goes to a new copy,
   *  told what was said. */
  ended: boolean;
  /** The account whose agent answers, where that was chosen: a PR's conversation's, with the model
   *  and effort picked with it (none for the account's own). */
  account?: string;
  model?: string;
  effort?: string;
  /** It was closed while its copy answered, and the copy answers what it was asked before it ends:
   *  no longer the open one, it's read where the kept ones are. */
  closed?: boolean;
}

/** A side conversation as the board lists it (side.rs `Glance`): what it was first asked, how far
 *  it went and whether its copy is answering, without what was said. */
export interface SideGlance {
  /** Its record's id (`Side.id`, `SideKept.id`). */
  id: string;
  /** The first line of what it was first asked. */
  question: string;
  /** How many questions it's been asked. */
  asked: number;
  /** Its copy is answering. */
  running: boolean;
  /** The session's open one, which shows above its box until it's closed. */
  open: boolean;
  startedAt: number;
  /** When it was last asked, or what was kept of it was written. */
  lastAt: number;
  /** When it finished what it was asked, until it's read (side.rs `Open::answered_at`). */
  answeredAt?: number;
}

/** A side conversation as Codealong keeps it (`sides.json`), to read once its copy is gone: the
 *  turns that ended, with or without an answer. */
export interface SideKept {
  id: string;
  key: string;
  /** The session's title when it was asked. */
  title: string;
  startedAt: number;
  /** When it was last asked or answered. */
  lastAt: number;
  turns: SideTurn[];
  /** It was closed while its copy answered, and the copy is answering still: read as it is. */
  answering?: boolean;
  /** When it finished answering, until it's asked again or closed with nothing left to answer:
   *  what its session waits on you for (`Session.sideAnswer`). */
  answeredAt?: number;
}

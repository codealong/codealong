// What model calls used and cost, and your part in the work, for the Analytics window and
// Settings › Decisions: ledger.rs, turns.rs, dismissed.rs and analytics.rs.

/** Tokens of each kind (ledger.rs `Tokens`): input read neither from nor into the cache, cache
 *  reads, cache writes, and output with its thinking among it. */
export interface TokenCounts {
  input: number;
  cacheRead: number;
  cacheWrite: number;
  output: number;
  reasoning: number;
}

/** What a group of model calls used and cost (ledger.rs `Spent`): an account's, a model's, a
 *  folder's, a session's or all of them. */
export interface Spent {
  /** The account, model, folder or session key; empty for a total. */
  key: string;
  /** At API prices, in dollars, its unpriced calls left out. */
  api: number;
  /** Paid by the token: what the harness recorded, or a `per_token` account at API prices. */
  paid: number;
  tokens: TokenCounts;
  calls: number;
  /** Tokens of calls no price was found for. */
  unpriced: number;
  lastAt: number;
  /** A session's account and folder. */
  account?: string;
  folder?: string;
}

/** One local day of a report. */
export interface SpendDay {
  /** `2026-10-12`. */
  day: string;
  /** At API prices, by account. */
  api: Record<string, number>;
  /** Really paid: plans' daily share, extra usage, and by the token. */
  paid: number;
  tokens: number;
}

/** What an account paid in a report's days, by kind (ledger.rs `Paid`). */
export interface AccountPaid {
  account: string;
  monthly: number | null;
  perToken: boolean;
  plan: number;
  /** Claude's extra usage in the days. */
  extra: number;
  /** The latest extra usage reading: this period's total and its monthly limit. */
  extraNow?: [number, number | null];
  tokens: number;
}

/** What model calls used and cost over some days (ledger.rs `Report`), for the Analytics window. */
export interface TokenReport {
  days: SpendDay[];
  total: Spent;
  /** Really paid in all, plans and extra usage included. */
  paid: number;
  accounts: Spent[];
  models: Spent[];
  folders: Spent[];
  /** The sessions that cost the most. */
  sessions: Spent[];
  paying: AccountPaid[];
  /** Models with calls no price was found for. */
  unpriced: string[];
  /** Accounts whose harness records no tokens Codealong can read. */
  untracked: string[];
  /** When the ledger last caught up; 0 before its first read ends. */
  asOf: number;
  filling: boolean;
  /** What the last read of the records couldn't read, which the numbers leave out. */
  problem?: string;
  /** By how each session began (began.rs `Began::word`), an empty key for the ones nothing was
   *  noted of. Missing while `[experimental] analytics` is off, as the two below are. */
  origins?: Spent[];
  /** By who began the turn each call was made in (turns.rs `Source::word`: you, agent,
   *  codealong), an empty key for calls whose turn isn't known. */
  hands?: Spent[];
  /** What subagents' calls came to. */
  subagents?: Spent;
}

/** How many of something, by a word (turns.rs `Count`). */
export interface Count {
  key: string;
  n: number;
}

/** One day of what came in (turns.rs `PartDay`): by who it was from, and your prompts by what
 *  they did, an empty key for the ones not read. */
export interface PartDay {
  day: string;
  sources: Record<string, number>;
  stances: Record<string, number>;
}

/** What the records say for sure of your prompts (turns.rs `Marks`). */
export interface PartMarks {
  mid: number;
  afterStop: number;
  pickedFirst: number;
  pickedOffered: number;
  ownWords: number;
  allowed: number;
  denied: number;
  suggested: number;
  chars: number;
}

/** A group's turns: an account's, a folder's or a session's (turns.rs `PartRow`). */
export interface PartRow {
  key: string;
  title?: string;
  account?: string;
  folder?: string;
  /** How the session began, for a session's row, when noted. */
  began?: string;
  /** What came in, by what it counts as: you, agent, codealong. */
  counts: Record<string, number>;
  /** Your prompts by stance, an empty key for the ones not read. */
  stances: Record<string, number>;
  lastAt: number;
}

/** A prompt of yours, for a list (turns.rs `Prompt`). */
export interface PartPrompt {
  id: string;
  at: number;
  session: string;
  title: string;
  opening: string;
  stance?: string;
  /** How sure the read was, 0 to 1, when a read says. */
  sure?: number;
  /** What read it: `rule`, a Jev model, or `agent` and its session. */
  readBy?: string;
  mid: boolean;
  afterStop: boolean;
}

/** Your part over some days (turns.rs `Part`). */
export interface Part {
  days: PartDay[];
  /** Everything that came in, by who its record says it was from. */
  sources: Count[];
  /** By what it counts as: a turn the session began itself with the one it follows. */
  hands: Count[];
  stances: Count[];
  marks: PartMarks;
  accounts: PartRow[];
  folders: PartRow[];
  sessions: PartRow[];
  /** Your latest prompts that steered or redirected. */
  prompts: PartPrompt[];
  /** The earliest thing known to have come in. */
  since?: number;
  /** Prompts of yours still waiting on a read, and whether Jev reads them. */
  unread: number;
  reading: boolean;
}

/** A session you left (dismissed.rs `Left`). */
export interface LeftSession {
  at: number;
  session: string;
  title: string;
  account: string;
  folder: string;
  /** `done`, `archive` or `close-idle`. */
  what: string;
  standing: string;
  said: string;
  unread: boolean;
  reopened: boolean;
}

/** A session still open with work left undone (dismissed.rs `Hanging`). */
export interface HangingSession {
  session: string;
  title: string;
  account: string;
  folder: string;
  standing: string;
  said: string;
  lastActivity: number;
}

/** How sessions began and how you left them (dismissed.rs `Outcomes`). */
export interface Outcomes {
  began: Count[];
  left: number;
  finished: number;
  lookedFinished: number;
  unfinished: number;
  unknown: number;
  reopened: number;
  standings: Count[];
  /** The ones left unfinished, the newest first. */
  list: LeftSession[];
  /** Sessions an agent marked done from the command, not counted above. */
  byAgents: number;
  hanging: HangingSession[];
  since?: number;
}

/** Your part in the work (analytics.rs `Analytics`), for the Analytics window's Your part and
 *  Sessions. `on` is false, and the rest empty, while `[experimental] analytics` is off. */
export interface Analytics {
  on: boolean;
  part: Part;
  outcomes: Outcomes;
  asOf: number;
  filling: boolean;
}

/** What one session used and cost, by model (ledger.rs `SessionSpent`). */
export interface SessionSpent {
  total: Spent;
  models: Spent[];
}

/** What decision models' requests for one feature, or on one model, used on a day: input tokens,
 *  which are what the providers charge for, and what they cost at each one's prices. */
export interface JevUsed {
  requests: number;
  tokens: number;
  /** Dollars. */
  cost: number;
}

/** What Codealong's requests to decision models used each day (ledger.rs `JevReport`), for
 *  Settings › Decisions. */
export interface JevReport {
  /** Oldest first, today last; each day's `features` by the switch in `[decisions]` that asked,
   *  or `test` for Settings' Test button, and `models` by the model that answered, as
   *  `<provider>:<model>` (missing from an older engine's). */
  days: { day: string; features: Record<string, JevUsed>; models?: Record<string, JevUsed> }[];
  /** When the ledger kept its first request: none before it were counted. */
  since: number | null;
}

// Schedules: when one runs, what it does, how its runs went, and the ones the harnesses keep
// themselves. Mirrors src-tauri/src/schedules/ (mod.rs, harnesses.rs).

import type { Emblem, Harness } from './session';
import type { StartPlace } from './start';

/** When a schedule runs (schedules/mod.rs `When`): at the times a rule names, when a command it
 *  runs prints something new, when something happens on the board, or only when it's fired. */
export type ScheduleWhen =
  | { kind: 'time'; rule: string }
  | { kind: 'watch'; command: string; every: string; mode: 'lines' | 'value' }
  | { kind: 'event'; event: ScheduleEvent; repo: string; label: string; mine: boolean }
  | { kind: 'fired' };

/** `changes`: something an orchestrator keeps watch over needs a look (schedules/orchestrate.rs). */
export type ScheduleEvent = 'pr-merged' | 'checks-failed' | 'todo-ready' | 'changes';

/** What a schedule does when it runs (schedules/mod.rs `Run`): start a new session, keep one
 *  going (`thread`), or send to one that's there (`session`). */
export interface ScheduleRun {
  to: 'new' | 'thread' | 'session';
  session: string;
  /** Where a new session starts; empty for a chat in no project. */
  folder: string;
  worktree: boolean;
  /** An account, a routing group, or empty for Auto. */
  account: string;
  model?: string | null;
  effort?: string | null;
  /** None for out of sight. */
  place?: StartPlace | null;
  prompt: string;
  freshAfter: string;
}

/** A schedule, as schedules.json has it. */
export interface Schedule {
  id: string;
  name: string;
  paused: boolean;
  when: ScheduleWhen;
  onlyIf: string;
  run: ScheduleRun;
  quiet: boolean;
  overlap: 'skip' | 'allow';
  catchUp: 'once' | 'skip';
  perDay?: number | null;
  notes: boolean;
  createdAt: number;
  updatedAt: number;
  /** The session whose agent made it, from the command; none when you did. */
  createdBy?: string;
  /** Which of your machines run it; none for this machine alone. */
  runsOn?: ScheduleRunsOn;
  /** Then stop: after its first run that goes it's finished (Run now doesn't count). */
  once?: boolean;
  /** When the scheduler noted a `once` schedule finished. Saves keep the engine's, whatever they send. */
  finishedAt?: number;
  /** When it was last resumed, or changed to run at other times or on something else: what was
   *  due before then isn't run, on any machine. */
  resumedAt?: number;
  /** The repo its folder is a checkout of, and a linked worktree's branch, for your other
   *  machines, which run it in a checkout of their own (schedules/mod.rs `Checkout`). */
  checkout?: { repo: string; branch?: string };
  /** Its folder was picked where its worktree had no repo another machine could tell: it goes
   *  by its folder alone. */
  folderOnly?: boolean;
  /** It's an orchestrator: it keeps watch over a project's sessions (`project`), or every
   *  project's without one of its own (none), and its session is listed first in their Sessions. */
  orchestrates?: { project?: string; send?: boolean; start?: boolean };
}

/** Which of your machines run a schedule (schedules/mod.rs `RunsOn`): this one alone; one of them
 *  each time, the one it prefers (a device key) while it's around and can, else the next; or each. */
export type ScheduleRunsOn = { kind: 'this' } | { kind: 'one'; prefer?: string } | { kind: 'every' };

export type ScheduleOutcome = 'started' | 'sent' | 'resumed' | 'queued' | 'skipped' | 'missed' | 'failed';

/** One run of a schedule, or one that didn't go. */
export interface ScheduleRunRecord {
  at: number;
  due?: number;
  why: string;
  outcome: ScheduleOutcome;
  said: string;
  session?: string;
  /** A start whose session hasn't shown yet. */
  pending?: { lane: string; account: string; at: number };
  /** The machine it ran on, when that's another of yours; its session is a key from there. */
  machine?: string;
}

/** A schedule as the Schedules view shows it (schedules/mod.rs `Item`). */
export interface ScheduleItem extends Schedule {
  whenWords: string;
  /** Why it can't run as it is. */
  problem?: string;
  /** When it runs next: a time, or a watch's next look. */
  nextAt?: number;
  /** The session it keeps going, followed to where it moved; none once it's archived, when the
   *  next run starts a new one. */
  thread?: string;
  /** Runs waiting for its session's turn to end. */
  queued?: { at: number; why: string[]; files: string[] };
  /** How its watch's latest look went. */
  check?: { at: number; ok: boolean; said: string; failures: number };
  /** Its latest runs, the newest first. */
  runs: ScheduleRunRecord[];
  /** Runs asked for that haven't gone yet. */
  asked: number;
  /** Where it runs, for one your machines share: "Next on Mini", "Next on this machine, in Mini's place". */
  runsOnWords?: string;
  /** Why this machine can't run it, for one your machines share that another can. */
  here?: string;
  /** The folder it runs in here, when that's another than its own: this machine's checkout of its repo. */
  runsIn?: string;
  /** The worktree it runs in here, by its id, its folder read as this machine has it (`~` your
   *  home, a link followed): what Schedules groups it under (schedules.ts `scheduleLane`). */
  worktree?: string;
  /** When it finished: one that runs once and has, or one whose times are over (schedules/mod.rs `spent`). */
  finished?: number;
  /** What its sessions show beside their titles (emblems.rs). Missing until it has one, and for
   *  one that sends to a session, which has its own. */
  emblem?: Emblem;
}

/** A schedule a harness keeps itself, outside Codealong (schedules/harnesses.rs `Kept`): a Claude
 *  Code session's loop or cron job, a Codex app automation. Listed, never changed. */
export interface HarnessSchedule {
  /** Unique among them: the harness, the account and its own id. */
  key: string;
  harness: Harness;
  account: string;
  id: string;
  /** What the harness calls it: Loop, Cron job, Heartbeat, Automation. */
  kind: string;
  name: string;
  prompt: string;
  whenWords: string;
  /** When it runs next, where that can be told. */
  nextAt?: number;
  paused: boolean;
  /** The session it wakes, or sends its runs to, by key. */
  session?: string;
  /** The folders its runs start sessions in, for one that starts its own. */
  folders?: string[];
  /** A link that opens it where it lives (`codex://…`). */
  link?: string;
  /** What keeps it going, in a sentence. */
  lasts: string;
  /** Why it can't run, where its harness would skip it every time. */
  problem?: string;
}

/** A schedule's time as it's being written: in words, and its next few times (`schedule_times`). */
export interface ScheduleTimes {
  words: string;
  next: number[];
  problem?: string;
}

/** A change to the schedules (schedules/mod.rs `Op`). */
export type ScheduleOp =
  | { op: 'save'; schedule: Schedule }
  | { op: 'remove'; id: string }
  | { op: 'pause'; id: string; paused: boolean }
  | { op: 'run'; id: string }
  /** Runs it now in a new session, the one it kept going marked done (an orchestrator's). */
  | { op: 'fresh'; id: string }
  /** Starts the orchestrator for a project, or All projects' with none: resumes it, else adds it,
   *  and runs it now (schedules/mod.rs `Op::Orchestrate`). */
  | { op: 'orchestrate'; project?: string | null; account?: string; model?: string | null; send?: boolean | null; start?: boolean | null };

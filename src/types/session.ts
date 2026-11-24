// A session as the board lists it under its worktree, with what hangs off it: how it stands, what
// its last turn ended on, what it holds and waits on, its mode, its emblem, and how its turn was
// read. Mirrors `Session` and its parts in src-tauri/peer/src/model.rs.

import type { Label } from './todos';

export type Harness = 'claude' | 'codex' | 'opencode' | 'antigravity' | 'cursor';
export type Surface = 'cli' | 'app';
export type Status = 'needs' | 'error' | 'working' | 'unread' | 'idle' | 'stopped';

/** A turn a lost connection ended (peer/src/model.rs `ConnectionLost`). */
export interface ConnectionLost {
  /** When the turn ended. */
  at: number;
  /** Codealong on its Mac tells it to carry on once its API can be reached again (config.toml's
   *  `[sessions] continue_after_network_error`, reconnect.rs). */
  continues?: boolean;
}

/** An error from its API that ended a session's turn (peer/src/model.rs `ApiError`). */
export interface ApiError {
  /** When the turn ended. */
  at: number;
  /** What went wrong, in a line, in the harness's words. */
  said: string;
}

/** A turn its account's sign-in was turned down for (peer/src/model.rs `SignedOut`). */
export interface SignedOut {
  /** When the turn ended. */
  at: number;
  /** What the harness said, in a line. */
  said: string;
  /** Codealong on its Mac tells it to carry on once its account reads signed in again
   *  (config.toml's `[sessions] continue_after_sign_in`, reconnect.rs). */
  continues?: boolean;
}

/** A message a session's harness holds until the turn it's taking ends (peer/src/model.rs
 *  `QueuedMessage`). */
export interface QueuedMessage {
  /** Its id in its harness's queue: for one Codealong queued, the one its send said
   *  (`Sent.queued`). */
  id: string;
  /** When it was queued. */
  at: number;
  /** What it says, each image in it as `[Image #n]`. */
  text: string;
}

/** The usage limit a session's last turn ended on (peer model.rs `SessionLimit`). */
export interface SessionLimit {
  /** The limit, as its account's meters key it (`session`, `week`, `model:<name>`), when said. */
  meter: string | null;
  /** When it lifts, when said. */
  resetsAt: number | null;
  /** The account's extra usage was refused too, so raising its spend limit would also do. */
  spendCapped?: boolean;
  /** When it was refused. */
  at: number;
  /** Another account of its harness with room, to move it to and go on (start.rs `limit_moves`). */
  moveTo: string | null;
  /** Codealong tells it to carry on once the limit lifts (`[limits] continue_when_lifted`,
   *  reconnect.rs). Missing from Macs before 2026-10-15. */
  continues?: boolean;
}

/** A session moved to the top of what waits on you (`Session.raised`): when, and by whom. The
 *  ones you moved go first, then the ones agents did, each the latest first (queue.ts
 *  `byRaise`). */
export interface Raised {
  at: number;
  /** An agent moved it (`codealong session raise`), not you. */
  agent?: boolean;
  by?: string;
  /** The agent's session's title when it moved it. */
  byTitle?: string;
  /** Why, when the agent said. */
  note?: string;
}

/** A finished turn its own agent marked seen (`Session.agentSeen`): when it said so, and why. */
export interface AgentSeen {
  at: number;
  /** What the turn came to, when the agent said. */
  note?: string;
}

/** Replies suggested for a session by another session that read it (peer/src/model.rs
 *  `Suggestions`): each goes as it is, as yours, once you pick it. */
export interface Suggestions {
  /** The best first; none where it picks among the choices of questions it asks. */
  replies: string[];
  /** The labels of the choices picked for each question it asks, by its question. */
  picks?: Record<string, string[]>;
  /** Why each question's pick, in a few words, by its question, where the suggester said. */
  why?: Record<string, string>;
  /** Why, or what to weigh, when the suggester said. */
  note?: string;
  /** The session that suggested them, by its key on its own device. */
  by?: string;
  /** That session's title when it suggested them. */
  byTitle?: string;
  /** When they were suggested, by the session's Mac's clock. */
  at: number;
  /** The session's last prompt as the suggester read it: they answer the reply to that one. */
  promptAt?: number;
}

/** A command a session's agent started in the background: a Claude Code shell or Monitor, or a
 *  Codex background terminal (model.rs `BackgroundTask`). */
export interface BackgroundTask {
  /** Its harness's id for it: Claude Code's task id, Codex's process id. */
  id: string;
  /** The command, or "" where its harness's record didn't say. */
  command: string;
  /** What the agent said it's for. Missing when it didn't say. */
  about?: string;
  /** A Claude Code Monitor: every line it prints wakes the agent. */
  monitor?: boolean;
  startedAt: number;
  /** When it ended, once its harness said so. Missing while it runs. */
  endedAt?: number;
  /** How it ended, as its harness put it (`completed`, `failed`, `killed`, `stopped`). */
  status?: string;
  exitCode?: number;
  /** The file its harness writes its output to, where it keeps one. */
  output?: string;
}

/** A command running in a terminal that a session waits on: one you allowed, which its agent
 *  waits on mid-turn, or one you ran for it (`yours`) (model.rs `RunningIn`). */
export interface RunningIn {
  /** The terminal app its tab is in (Ghostty, Rex…), or Codealong for a terminal under the session. */
  terminal: string;
  /** The command, less a leading `cd` to its folder. */
  command: string;
  /** You ran it for the session (`!` in its box, Run on a block): its turn is over, and what the
   *  command prints goes to it when it ends, so it waits on it (`waitsInBackground`). Missing for
   *  an agent's run, and from a board an older app sent. */
  yours?: boolean;
}

/** Why a session needs you (model.rs `Need`): waiting for you to allow something, for your
 *  answer to a question it holds its turn on, on a reply that asks you something, holding
 *  another session's message, stuck, on a command in its terminal that waits for input, or on a
 *  helper it started that waits in its terminal before it starts. */
export type Need = 'approval' | 'question' | 'reply' | 'message' | 'stuck' | 'input' | 'start' | 'other';

export interface Session {
  key: string;
  id: string;
  shortId: string;
  harness: Harness;
  /** The account name as you type it: claude, claude2, codex, opencode, antigravity. */
  account: string;
  surface: Surface;
  status: Status;
  live: boolean;
  pid: number | null;
  title: string;
  cwd: string;
  name: string | null;
  startedAt: number | null;
  statusSince: number | null;
  lastActivity: number;
  /** When you last sent it a prompt, or when it started if its log doesn't say. It holds still
   *  while the session works, so lists sort by it. */
  lastPromptAt: number;
  /** Backtick spans render as code. */
  excerpt: string | null;
  /** The branch it worked on. */
  branch: string | null;
  /** Archived from Codealong, in Codex, in OpenCode, or in the Claude app. */
  archived: boolean;
  /** A background job's `claude attach <job>`: it has no terminal tab of its own. */
  attachCommand: string | null;
  /** While it runs, the id its harness's app reaches it by: a Claude Code session's Remote
   *  Control session, which the Claude app shows and sends to. Missing when it has none. */
  remoteControl?: string;
  /** The session whose record holds this one's conversation until it writes its own: a Claude
   *  Code conversation moved to the background goes on in a job that writes nothing until its
   *  next prompt, and then starts its own with a copy of the conversation. */
  continues?: string;
  /** The session it came from, when its harness or Codealong wrote that down; a headless run's,
   *  the session whose command ran it, when Codealong saw it running (board.rs `run_started_by`). */
  origin?: SessionOrigin;
  /** Busy but silent for a long while: stuck mid-turn. When it went quiet. */
  stalledSince: number | null;
  /** Why it needs you, as the engine reads it (`Session::need`): the one thing a headline, a
   *  row's label and `waitsOnYou` tell one need from another by. */
  need?: Need;
  /** When the latest of the messages its harness holds until its turn ends came: typed while it
   *  works, one waits there, and its record has it only once the turn ends (Antigravity's
   *  terminal UI, the Codex app's follow-up queue). Missing when it holds none, and from a board
   *  an older app sent. */
  queuedAt?: number;
  /** The messages it holds, in the order they'll go, where its harness says what they are (the
   *  Codex app's follow-up queue), whoever sent them: shown at the end of its conversation until
   *  they go in. Missing when it holds none or its harness doesn't say. */
  queued?: QueuedMessage[];
  /** A session whose turn left a command running in the background (Claude Code's, `agy`'s): the
   *  command, or "" when it couldn't be read. Working, it waits on it (`waitsInBackground`).
   *  Missing from a board an older app sent. */
  background?: string;
  /** A Claude session whose turn ended with subagents or workflows it started in the background
   *  still running: what each is for, oldest first ("" where its call didn't say). Working, it
   *  waits on them (`waitsInBackground`). Missing when there are none, and from a board an older
   *  app sent. */
  agents?: string[];
  /** The commands its agent started in the background, as its harness's own list has them: the
   *  ones running, then the ones that ended a little while ago, oldest first in each (Claude Code
   *  and Codex). Missing when there are none, and from a board an older app sent. */
  tasks?: BackgroundTask[];
  /** While its agent waits on a command you allowed in your terminal (`codealong terminal run`),
   *  or while a command you ran for it goes (`RunningIn.yours`): where it runs and what, which
   *  the end of its conversation says (`Waiting`). Missing from a board an older app sent. */
  running?: RunningIn;
  /** The decision model's read of how its turn ended, when `[decisions] sessions` is on (insights.rs). */
  insight?: SessionInsight;
  /** Its work is done and nothing is left for you, and why (board.rs `finished_of`). It stays
   *  Your turn until you've seen it, after the turns that want something of you. Missing while
   *  it isn't, and from a board an older app sent. */
  finished?: Finished;
  /** Needs you only because its last reply asks you something: it sits at its prompt, so a
   *  reply is the answer. Not set while it waits on a permission prompt. */
  asks?: boolean;
  /** Codealong runs it itself, over the Agent Client Protocol (acp.rs): its panel sends to it
   *  and answers what it asks, and opening it shows the panel. */
  hosted?: boolean;
  /** Its turn is running and Codealong can stop it (harness::stoppable): Stop sends its harness
   *  what its own Stop sends, or presses Escape in its terminal. */
  stoppable?: boolean;
  /** Its turn is running and what you send can reach the agent in that turn at once, rather than
   *  once the step it's on ends (harness::steerable): its reply box's Send now, Claude Code's
   *  send-now key and Codex's steer. */
  steerable?: boolean;
  /** Snoozed from Codealong until then: the floating bar and the mark leave it out of Needs you
   *  and Your turn until the time comes (board::apply_snooze). */
  snoozedUntil?: number;
  /** Quieted from Codealong: a turn it ends is idle rather than Your turn, so it waits on you
   *  only when it needs you (board::apply_quiet), until you make it heard again. Missing from
   *  older Macs, whose statuses say nothing of it. */
  quiet?: boolean;
  /** Moved to the top of what waits on you (Move to top, `codealong session raise`): every queue
   *  goes to it first until you take it up (board.rs `apply_raise`, queue.ts `byPlace`). Missing
   *  from older Macs. */
  raised?: Raised;
  /** Its own agent said the turn it just ended needs nothing from you (`codealong session seen
   *  me`): idle rather than Your turn until you look at it or another turn of its starts (board.rs
   *  `apply_agent_seen`). Missing from older Macs, which show such a turn as Your turn. */
  agentSeen?: AgentSeen;
  /** What a side conversation answered that you haven't seen (board.rs `apply_side_answer`): a
   *  session whose turn you'd seen is Your turn for it, and a window shows it above the box as it
   *  comes to the session (side.ts `useSideAnswer`). Missing from older Macs. */
  sideAnswer?: SideAnswer;
  /** When you set it aside for its side conversation (board.rs `apply_aside`): it still needs
   *  you, but waits on the side conversation first, listed with what's working, out of every
   *  queue, until that's done. */
  aside?: number;
  /** Replies another session suggested you send it (`codealong session suggest`), shown above its
   *  box to send with a click, while they still answer where it stands: nothing sent to it since,
   *  no turn of its started since (board::suggestions_hold). Missing from older Macs. */
  suggestions?: Suggestions;
  /** Labels you put on it, from the labels todos wear on its device, in the order you put them
   *  (board.rs `apply_labels`). Unlike its `insight`, a read of one turn, they stay until taken
   *  off. Missing when it has none, and from older Macs. */
  labels?: Label[];
  /** Its place among the sessions pinned on the device it's on, the first 0, as that device
   *  tells the others; missing when it isn't pinned. This device's own windows go by the
   *  board's list (`Board.pinned`). */
  pin?: number;
  /** It worked in a reusable worktree before that worktree last took new work: listed after the
   *  current work's sessions, and out of the worktree's status (board::earlier). */
  earlier?: boolean;
  /** The Chrome tabs it drives through Claude in Chrome, to bring back (chrometabs.rs). */
  chrome?: ChromeTabs;
  /** Its last turn ended on its account's usage limit, and nothing has been said since. */
  limit?: SessionLimit;
  /** Its last turn ended because its harness couldn't reach its API (the Mac slept, the network
   *  dropped, or the connection was refused), and nothing has been said since. Missing from older
   *  Macs. */
  connectionLost?: ConnectionLost;
  /** Its last turn ended on an error from its API, a lost connection included, and nothing has
   *  been said since: what the harness said. Missing from older Macs. */
  apiError?: ApiError;
  /** Its last turn ended because its account's sign-in was turned down (expired, revoked or
   *  signed out), and nothing has been said since. Missing from older Macs. */
  signedOut?: SignedOut;
  /** Its last turn was cut off: what ran it ended partway through (its terminal closed, the Mac
   *  restarted, it was killed), and nothing has gone on since. When the turn last did something.
   *  Missing from older Macs. */
  cutOff?: number;
  /** The provider recorded a refusal or content-policy error in this conversation. */
  flagged?: boolean;
  /** The model it's on, as its harness names it (`claude-opus-5-5`): its latest reply's, or one
   *  it was switched to since. Missing where the harness doesn't say, and from older Macs. */
  model?: string;
  /** The reasoning effort it's on, as its harness names the level (`xhigh`): its latest reply's,
   *  or one it was set to since. Missing where the harness doesn't say, and from older Macs. */
  effort?: string;
  /** The mode it runs in, as its harness names it (peer model.rs `SessionMode`): what it does
   *  without asking you. Missing where the harness doesn't say, and from older Macs. */
  mode?: SessionMode;
  /** The modes it can be switched to here, each with its `id`, in its agent's order: for a
   *  session Codealong runs (acp.rs `overlay`), and for one in a terminal Codealong presses keys
   *  in whose harness steps through its modes with Shift+Tab there, in that order (cycle.rs).
   *  Missing for any other, whose mode is changed where it runs, and from older Macs. */
  modes?: SessionMode[];
  /** How full its model's context window is, as its latest call sent it (peer model.rs
   *  `ContextUse`). Missing where the harness doesn't say, before its first reply, and from
   *  older Macs. */
  context?: ContextUse;
  /** The worktree its harness made for it and holds while it runs (Claude Code's `--worktree`),
   *  which the harness removes as the session exits its own way: Mark as done exits it so
   *  (idle.rs `ending`). Missing where it has none, and from older Macs. */
  ownWorktree?: string;
  /** It works for an agent, not for you: a helper another session's agent started (`codealong
   *  start --helper`), or an automatic review's session. While it doesn't need you it's out of
   *  the lists and queues, under what it helps (lanes.ts `unattended`). Missing from older Macs. */
  helper?: Helper;
  /** The orchestrator whose session it is: it keeps watch over a project's sessions, or every
   *  project's without one of its own, and is listed first in their Sessions rather than among
   *  them (boardLists.ts `orchestrators`). Missing for every other session, and from older Macs. */
  orchestrator?: Orchestrator;
  /** A run nobody is at (`claude -p`, `codex exec`): a script or an agent ran it and reads what
   *  comes back, so a turn it ends is idle (board.rs `apply_headless`). While it doesn't need you
   *  it's out of the lists and queues, counted on the row of the session it came from
   *  (`origin`, lanes.ts `workingFor`). Missing otherwise. */
  headless?: boolean;
  /** Hearing a meeting now, on its own Mac (listen.rs, board.rs `apply_listening`): shown where
   *  its status is (ui/primitives.tsx `SessionGlyph`). Missing otherwise, and from older Macs. */
  listening?: boolean;
  /** The pages on the web it made or showed you (artifacts Claude Code published), the one it
   *  last published, updated or opened first (artifacts.rs). Missing when it has none, and from
   *  older Macs. */
  artifacts?: Artifact[];
  /** Something to know it by beside its title, an emoji or a small picture (emblems.rs, when
   *  `[emblems]` is on). Missing until it has one, and from older Macs. */
  emblem?: Emblem;
  /** Other non-helper sessions this session has been actively messaging back and forth with
   *  recently (chatter.rs). Missing when there is no recent peer chatter, and from older Macs. */
  peers?: PeerTalk[];
  resumeCommand: string | null;
  /** The Claude app is signed into this session's account. */
  appAccount: boolean;
  /** Opens this session where it lives: its terminal tab, the Claude app, Codex, or Antigravity. */
  openTarget: string;
  /** The terminal app a live session runs in (Ghostty, Rex…), where Show in takes it whatever
   *  terminal config.toml names; absent in none Codealong knows as an app, or from older engines. */
  terminal?: string;
  /** Why a session from another lane shows here: "made this worktree" or "worked here". */
  visitor: string | null;
  /** This session's own sentence; backtick spans and time tokens as in lane headlines. */
  headline: string;
  detail: string | null;
}

/** Recent peer-to-peer message activity between this session and another non-helper session
 *  (peer model.rs `PeerTalk`, chatter.rs). */
export interface PeerTalk {
  key: string;
  title: string;
  count: number;
  flips: number;
  lastAt: number;
}

/** A session's emblem (peer model.rs `Emblem`): something you can name that stands for what it
 *  works on, an emoji Jev picked or a picture an account drew, different from the sessions'
 *  beside it where it can be. */
export interface Emblem {
  /** The emoji, when it's one, or when it has one beside its picture, shown in place of a picture
   *  that doesn't load. */
  emoji?: string;
  /** The picture, when it's one: a kept image's name (`<hash>.png`), loaded from the Mac the
   *  session is on (api.ts `imageUrl`). */
  picture?: string;
  /** What it shows, in a word or two, for its tooltip. */
  name: string;
}

/** A page on the web a session made or showed you: an artifact Claude Code published, made from a
 *  type or opened (peer model.rs `Artifact`), as it last stood. */
export interface Artifact {
  url: string;
  /** What the page is called. */
  title: string;
  /** What the session last did with it, in a word or two: "Updated · version 9". */
  note?: string;
  /** When it last did, or 0 where its record doesn't say. */
  at: number;
}

/** A session's tabs in Chrome, as its record last listed them. */
export interface ChromeTabs {
  /** Chrome's ids for its tab group's tabs, the one it last acted on first; empty once closed. */
  tabs: number[];
  /** The page it last acted on, kept after its tabs close to open again. */
  url: string;
  title: string;
  /** The Chrome profile they're in, as its picker names it, when Chrome has more than one. */
  profile?: string;
  profileDir?: string;
  at: number;
}

/** A session's mode: how much it does without asking you, from the least (`plan` changes nothing,
 *  `skips` asks nothing), its harness's own name for it, and what that means in a sentence. A kind
 *  a later Mac adds comes as `other`. */
export type ModeKind = 'plan' | 'read-only' | 'asks' | 'edits' | 'auto' | 'skips' | 'other';

export interface SessionMode {
  kind: ModeKind;
  label: string;
  detail: string;
  /** Its harness's id for it (`acceptEdits`), which a switch names: on the modes a session can
   *  be switched to (`Session.modes`) and the one such a session is in. */
  id?: string;
  /** The mode Codealong expects it to be in after pressing Shift+Tab in its terminal, where its
   *  harness writes the mode only with the next prompt (Claude Code): it holds until its record
   *  says (cycle.rs). */
  expected?: boolean;
}

/** How full a session's context window is: the tokens its latest call sent, counted as its
 *  harness counts them for its own context meter, and the window where it's known. */
export interface ContextUse {
  used: number;
  window?: number;
}

/** Jev's read of a session (insights.rs). Each part is there only when it was asked. */
/** How a session's turn ended: Jev's read (insights.rs), or an agent's tag of it over Jev's
 *  (`codealong session tag`, tags.rs), which names the agent's session in `by`. */
export interface SessionInsight {
  asksYou?: number;
  landing?: 'landed' | 'pushed' | 'committed' | 'uncommitted' | 'blocked' | 'none';
  finished?: number;
  /** 0 reads only … 3 destructive. */
  risk?: number;
  waitsForInput?: number;
  background?: 'polling' | 'job' | 'server' | 'stalled';
  backgroundP?: number;
  /** How sure Jev was of its least sure answer, 0 to 1; none for an agent's tag. */
  confidence?: number;
  /** The tagging agent's session, by its key, and its title; none for Jev's own read. */
  by?: string;
  byTitle?: string;
  /** Why, in a sentence or two, when the agent said. */
  note?: string;
  /** How its last reply leaves things with you, by the likeliest way a reply can end
   *  (insights.rs `ENDINGS`). `asksYou` is the chance of the first two together. */
  ending?: Ending;
  /** When its last reply says the work goes on, with nothing for you or it to do before then: a
   *  snooze is offered until then (snooze.ts `suggestedSnooze`). Jev reads a day, which is that
   *  morning; an agent's tag says any time. It stays once the time has come, when nothing is
   *  offered. Missing where the reply names none, and from older Macs. */
  waitsUntil?: number;
  /** That `waitsUntil` is Jev's read under an agent's tag that didn't say how the reply ends:
   *  the agent in `by` didn't read it (insights.rs `merged`). */
  waitsByJev?: boolean;
}

/** How a reply can leave things with you: it asks you something, can't go on until you do
 *  something, offers more, hands you a next step, waits on what it started, or only reports. */
export type Ending = 'question' | 'blocked' | 'offer' | 'handoff' | 'waiting' | 'report';

/** That a session's work is done with nothing left for you (`Session.finished`). */
export interface Finished {
  /** Something besides its last reply says so: a PR of its merged, a todo of its done, or its
   *  own status line. False when only a read of that reply does: it looks finished. */
  sure: boolean;
  /** Why, each in a few words, what's known first. */
  why: string[];
}

/** What a helper session works for: the session whose agent started it, or the PR it reviews. */
/** An orchestrator a session is the session of (`Session.orchestrator`). */
export interface Orchestrator {
  /** Its schedule's id, which it's paused, resumed, run and edited by. */
  schedule: string;
  /** The project it keeps watch over; missing for All projects. */
  project?: string;
  /** Paused: it keeps watch over nothing until it's resumed. */
  paused?: boolean;
  /** When it last looked: its latest run that went to its session. Missing before its first. */
  lookedAt?: number;
}

export interface Helper {
  parent?: string;
  /** The PR's key as reviews.json has it, which ignores case (`owner/name#7`, lowercased). */
  pr?: string;
  /** Its session is told when its turn ends or it waits on you (`start --helper --report`). */
  report?: boolean;
  /** You sent it a message after an agent last did, so what it says now is for you: listed, and
   *  its turns wait on you (lanes.ts `helpsAnAgent`), until an agent's message or a review's
   *  next round. Missing otherwise, and from older Macs. */
  yours?: boolean;
}

/** Where a session came from: a fork of another session (its conversation copied as far as it
 *  went), or started by another session's agent (`codealong start`, Codex's `create_thread`). */
export interface SessionOrigin {
  key: string;
  how: 'fork' | 'started';
}

/** The side conversation that answered last since you saw a session (`Session.sideAnswer`). */
export interface SideAnswer {
  /** Its record's id (`Side.id`, `SideKept.id`). */
  id: string;
  /** It's the session's open one; closed, it's read where the kept ones are. */
  open?: boolean;
  /** When it finished answering. */
  at: number;
}

// A session's conversation: the pages of its transcript (transcript.rs), what it waits on you to
// allow or answer (harness.rs), how a message reaches it and what sending one did (send.rs), the
// reply it's writing, and the search of what was said (search.rs).

/** How a message sent to a session would reach it (send.rs `way_in`): typed into its terminal,
 *  pasted into its background job, a message on Claude Code's socket, a new terminal resuming it,
 *  or its app. */
export type Way =
  | { kind: 'typed'; app: string }
  | { kind: 'background' }
  | { kind: 'message' }
  | { kind: 'resume'; app: string }
  | { kind: 'claude-app' }
  | { kind: 'codex-app' }
  | { kind: 'opencode' }
  | { kind: 'antigravity' }
  | { kind: 'cursor' };

/** What a path a conversation names is on this Mac (launch.rs `PathKind`). */
export type PathKind = 'file' | 'folder';

/** What a window can do with a file a session sent you (sentfiles.rs `State`): show it, show it
 *  once you ask, or name it alone. */
export type SentState = 'here' | 'asksFirst' | 'gone';

/** What reading a local Markdown file for an in-app preview gives back (sentfiles.rs `MarkdownRead`). */
export type MarkdownRead = { kind: 'text'; text: string } | { kind: 'asksFirst' };

/** What switching a session's mode did (send.rs `switch_mode`): what happened, and the mode it's in
 *  now by its id, which isn't always the one asked for (its agent may put it in another). */
export interface ModeSwitched {
  message: string;
  mode: string;
}

/** What sending to a session did (send.rs). */
export interface Sent {
  message: string;
  /** The session or its app was opened instead, with the prompt filled in or copied. */
  opened: boolean;
  /** It couldn't be typed into the session's terminal, so it went another way, as `message` says.
   *  Missing from other devices that don't know it yet. */
  fellBack?: boolean;
  /** The way in failed after it may have put it in the session (its terminal didn't confirm the
   *  typing, say), so it counts as sent without being sure, and `message` says where to look.
   *  Missing from other devices that don't know it yet. */
  uncertain?: boolean;
  /** The id its harness holds it under until the turn the session is taking ends (the Codex
   *  app's queue), as `Session.queued` lists it. Missing where it wasn't queued. */
  queued?: string;
}

/** What taking a held message out of what a session's harness holds did (`session-unqueue`). */
export interface Unqueued {
  message: string;
  /** The others it held that went back in after it, where its harness gives back everything it
   *  holds at once (Claude Code): the id each was held under before, and the one it has now. */
  requeued?: { was: string; now: string }[];
}

/** A session whose prompts and replies have every word typed (search.rs). */
export interface TextHit {
  key: string;
  /** A line of the message that matched best, around the first match. */
  snippet: string;
  /** You wrote it; otherwise the agent did. */
  you: boolean;
  at: number;
  /** How well that message matched: the words as typed beat them apart, what you wrote beats
   *  the agent's. Higher is better; missing from an app of an earlier build. */
  score?: number;
}

export interface TextFound {
  hits: TextHit[];
  /** Still reading sessions for the first time, so more may match soon. */
  building: boolean;
  /** `[search] days` is 0. */
  off: boolean;
}

/** What a message Codealong sent by itself is about, for its card's icon (codealong-peer's
 *  glance.rs `About`). */
export type GlanceAbout = 'pr' | 'helper' | 'schedule' | 'review' | 'carry-on' | 'other';

/** A message Codealong sent by itself, in a line (codealong-peer's glance.rs `Glance`): what it's
 *  about, the PR it names, and what happened, in a few words a part after it. */
export interface Glance {
  about: GlanceAbout;
  /** The PR it's about, its address where the message gave it. */
  pr?: { number: number; title: string; url?: string };
  /** Each thing it tells of (each of a PR's happenings, each helper's news), with the session
   *  it's about (a helper) by its key and title, where it names one. */
  parts: { session?: { key: string; title: string }; line: string }[];
}

/** A step of a session's conversation, from its own record (transcript.rs). */
export interface TranscriptItem {
  at: number;
  /** `result` is a Claude Code tool's output, matched to its call by `id`. */
  /** `codealong`: what Codealong sent the session by itself (a PR's news, a review handed back,
   *  a helper's news), never yours. `peer`: another session's message, whole. */
  kind: 'you' | 'agent' | 'tool' | 'result' | 'note' | 'codealong' | 'peer';
  text: string;
  /** Who sent a `peer` message: the sending session's name, where its record says. */
  from?: string;
  /** That session's key, where the message said it: its heading names the sender as a link to
   *  its session (SessionLink.tsx). */
  fromKey?: string;
  /** The id of this session's side conversation that sent a `peer` message (`Side.id`), where
   *  the message said it: its heading links to that side conversation (MessageCard.tsx). */
  fromSide?: string;
  /** What a `codealong` message comes to in a line, for its card (MessageCard.tsx), which a
   *  click opens to the whole of it (glance.rs). Missing from an older engine's. */
  glance?: Glance;
  id?: string;
  input?: string;
  output?: string;
  failed: boolean;
  /** A shell command's call whose `input` is the whole command: it can run again in a new terminal. */
  shell?: boolean;
  /** The folder a shell command ran in, when its record says. */
  cwd?: string;
  /** A prompt of yours the session took back before its agent saw it: Claude Code does that when
   *  a turn is stopped before any reply. */
  withdrawn?: boolean;
  /** A note where the turn was stopped, set apart from the other notes; its text says who stopped
   *  it when Codealong did (stops.rs). */
  stop?: boolean;
  /** The questions a call asks you (Claude Code's AskUserQuestion), shown in place of its input. */
  questions?: Question[];
  /** Your answers to them, on the call's result: each question's text to what you picked or
   *  wrote, several where it took more. */
  answers?: Record<string, string[]>;
  /** A page on the web the call made or opened (an artifact Claude Code published), on its
   *  result: its title, and what the call did with it ("Updated · version 9"). On a prompt of
   *  yours, the artifact it was about: the one you were looking at as you sent it ("Viewing
   *  Activity"), or the one whose comment it hands on ("Comment on Activity"). */
  link?: { url: string; title: string; note?: string };
  /** The files the call handed you to look at (Claude Code's `SendUserFile`), on the call: each a
   *  full path on the session's Mac, with what the agent said of them. */
  sent?: { files: string[]; caption?: string };
  /** The file the call read or changed (a read, a write, an edit), as a full path on the
   *  session's Mac: its line opens it on a click and offers the file's menu. */
  file?: string;
  /** Where its line starts (or its message number in OpenCode); with `n`, unique in the session. */
  cursor: number;
  n: number;
}

export interface TranscriptPage {
  items: TranscriptItem[];
  /** Where the next page back ends; null at the session's start. */
  before: number | null;
  /** Where reading on starts. */
  after: number;
}

/** A tool call a session waits on you to allow (harness.rs). */
export interface Pending {
  id: string;
  label: string;
  input: string | null;
  /** Why Codealong can't answer it here; the panel offers to open it instead. */
  cannot: string | null;
  /** It can also be allowed from now on, not just this once. */
  always?: boolean;
  /** It can't be allowed just this once (its agent offers no such answer): no Allow, only Always
   *  where `always` says so. */
  noOnce?: boolean;
  /** What allowing it from now on adds, in a sentence, when Codealong can say. */
  alwaysRule?: string;
  /** It's a plan to approve (plan mode): `input` is the plan, in Markdown. */
  plan?: boolean;
  /** It's another session's message it holds for you (Claude Code): `input` is the message, as
   *  prose. Allow delivers it; Deny drops it, and its sender is told it was declined. */
  message?: boolean;
  /** The session a held message is from, by its key, where its byline names one: the label
   *  names the sender as a link to it. */
  fromKey?: string;
  /** Questions it asks you: answered with your picks rather than allowed. */
  questions?: Question[];
  /** It's a command its agent asks to run in a terminal of yours (`codealong terminal run`):
   *  `input` is the command, which Allow runs in a new tab of `app` in `cwd`, as you. It's asked
   *  every time, so there's no Always. */
  terminal?: { cwd: string; app: string } | null;
  /** It's its harness asking, as the session exits, whether to keep the worktree it made for it
   *  (Claude Code's "Exiting worktree session"): `questions` has the one question, whose choices
   *  are all it takes, and Deny calls the exit off, leaving the session at its prompt. */
  exiting?: boolean;
  /** It's a dialog of its harness's own, read off the terminal it runs in (Claude Code's offer to
   *  make auto mode the default, a command's list): `questions` has the one question, whose
   *  choices are all it takes, picked there as you'd pick one, and Deny presses Escape there. */
  dialog?: boolean;
}

/** A question an agent asks you, with choices to pick from. */
export interface Question {
  /** Its text, which its answer is keyed by. */
  question: string;
  header?: string;
  options: { label: string; description?: string }[];
  /** More than one choice can be picked. */
  multiSelect?: boolean;
  /** It's a field of a form its agent asks you to fill in (a session Codealong runs), where what
   *  it takes is more than a pick or words of your own. */
  form?: FormField;
}

/** What a form's field takes (`Question.form`). */
export interface FormField {
  /** Its choices are all it takes: no box for words of your own. */
  choicesOnly?: boolean;
  /** The form doesn't go without an answer to it. */
  required?: boolean;
  /** What words it takes, where it takes some and not any: "A whole number". */
  hint?: string;
  /** What's written for it is hidden as it's typed. */
  secret?: boolean;
}

/** Answers to questions, each by its question's text: a choice's label or your own words, or
 *  several where the question takes more. */
export type Answers = Record<string, string | string[]>;

/** The reply a session is writing, ahead of its record: one Codealong runs (acp.rs), or a Claude
 *  Code session whose plugin hands it over (claude/live.rs), which has no tool. */
export interface HostedStream {
  key: string;
  /** The reply so far since its last tool call; empty when there's none. */
  text: string;
  /** The tool it's running, as a line. */
  tool: string | null;
}

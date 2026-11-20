// Starting a session: what the quick panel and the new-session sheet offer and ask for
// (start.rs), Jev's suggestions for what's typed (route.rs, routelog.rs), a folder typed there
// (newfolder.rs), and a start whose session hasn't shown yet (starting.rs).

import type { RoutingStrategy } from './config';
import type { Harness } from './session';

/** A folder typed in the quick panel's worktree menu, as the engine finds it (newfolder.rs
 *  `Look`). */
export interface FolderLook {
  /** What was typed: the answer goes with that, not with what's typed since. */
  typed: string;
  /** The folder, `~` expanded. */
  path: string;
  /** As config.toml writes it, `~` for your home folder. */
  shown: string;
  /** Its last part: what a project of its own is named. */
  name: string;
  /** It's there; adding one that isn't makes it. */
  exists: boolean;
  /** Why it can't be added. */
  refused?: string | null;
  /** The worktree on the board it is or is in, started in as it is. */
  lane?: string | null;
  /** The project config.toml lists it in already. */
  project?: string | null;
  /** Folders in the one typed whose names start with what's typed after its last `/`. */
  completions: string[];
}

export type RouteAction = 'send' | 'start' | 'find' | 'todo';

/** One option Jev weighed: a session key, lane id or `project/repo`, and its probability. */
export interface RoutePick {
  id: string;
  p: number;
}

/** Jev's answer for what's typed in the quick panel (route.rs). */
export interface Route {
  action: RouteAction;
  actions: [RouteAction, number][];
  sessions: RoutePick[];
  noSession: number;
  lanes: RoutePick[];
  /** How likely it is that the work needs none of the worktrees: a chat in no project. Missing
   *  from Macs of earlier builds. */
  noRepo?: number;
  repos: RoutePick[];
  /** How likely new work wants a worktree of its own: Jev's answer weighed by your habit. */
  ownWorktree: number;
  /** Jev's answer alone. */
  ownWorktreeAsked: number;
  /** Your latest starts in the likeliest repo: how many went into a new worktree, of how many. */
  habit: { new: number; of: number };
  /** A live session that may already be doing this work. */
  duplicate: RoutePick | null;
  /** Agent or model picks Jev weighed when `[routing] prompt` is set, the likeliest first. */
  agents?: AgentPick[];
  /** The service the work needs, when only some accounts reach it (route.rs `needs`). Missing
   *  from Macs of earlier builds. */
  needs?: NeedPick | null;
  model: string;
  ms: number;
  inputTokens: number;
}

/** What a quick panel suggestion does, or what you did with what you typed: to which session
 *  (its key) or worktree (its lane id). Kept beside Jev's suggestions to measure them
 *  (routelog.rs). */
export interface RouteChoice {
  kind: 'send' | 'open' | 'start' | 'todo' | 'other';
  target: string | null;
  newWorktree?: boolean;
  p?: number;
}

/** What you did with what you typed, as the quick panel tells the engine (routelog.rs `Note`):
 *  the text as it went to Jev, its suggestions as the panel listed them (none when you acted
 *  before Jev answered), and how you got to what you did. */
export interface RouteNote {
  project: string;
  text: string;
  suggested: RouteChoice[] | null;
  did: RouteChoice;
  via: 'suggested' | 'start-row' | 'send-to' | 'found';
}

/** A service the work needs, reached through an MCP server or connector, with the accounts that
 *  reach it: the one Auto would pick of them first. */
export interface NeedPick {
  name: string;
  accounts: string[];
  /** The model Auto would start the first on, where its routing group names one. */
  model?: string | null;
  p: number;
}

/** An agent, model or routing group Jev weighed when `[routing] prompt` is set (route.rs). */
export interface AgentPick {
  account: string;
  model?: string | null;
  p: number;
}

/** Where a new session runs: the terminal, or the Claude or Codex app. */
export type StartPlace = 'terminal' | 'background' | 'claude-app' | 'codex-app' | 'codealong';

/** An account as the new-session sheet offers it (src-tauri/src/start.rs). */
export interface StartAccount {
  name: string;
  harness: Harness;
  /** Where this account can start a session, the usual place first: the one `start` names, when set.
   *  `behind`, on Background: its harness has no background service of its own, so the session
   *  starts in the terminal, behind your windows (missing from Macs before 2026-10-03); false
   *  where it starts in tmux with no window instead. `service`, on Background: it goes under its
   *  harness's own background service (missing from Macs before 2026-10-23). `isolatesIn`: the
   *  main checkouts on that Mac's board that a session started here moves out of into a worktree
   *  of its own before it changes anything (Claude Code's background jobs), so it can start in
   *  one holding other work (missing from Macs before 2026-11-16). */
  places: { place: StartPlace; label: string; behind?: boolean; service?: boolean; isolatesIn?: string[] }[];
  /** The place its `start` setting names; starts go there rather than where the last one went. */
  chosen: StartPlace | null;
  /** Why there's nowhere, when `places` is empty. */
  unavailable: string | null;
  /** Its `usage_cap` setting (1 to 100); absent uses 90. */
  usageCap?: number | null;
  /** The room below `usageCap` Auto and routing groups leave for the sessions already running on
   *  it (`[routing] buffer`); absent uses 5. Missing from Macs before 2026-10-19. */
  buffer?: number | null;
  /** The model a session starts on without one chosen: the account's settings.json `model`. */
  defaultModel: string | null;
  /** The models a start on its command line can ask for, as its harness takes them; none when
   *  it takes none. Missing from devices that list only Claude Code's (`claudeModels`). */
  models?: { id: string; label: string }[];
  /** The effort levels a start on its command line can ask for, lowest first. */
  efforts?: { id: string; label: string }[];
  /** Its command line makes a new worktree for a session when asked. */
  worktree?: boolean;
  /** Codealong itself can make a new worktree on this device when the harness or place doesn't.
   *  Missing from Macs before 2026-11-20. */
  codealongWorktree?: boolean;
  /** Of `places`, where one of its sessions that isn't running can go on, in the same order.
   *  Missing from Macs before 2026-10-06, which resume one only in a terminal. */
  resumes?: StartPlace[];
  /** Its running sessions that Codealong types into can be switched to another of `models`;
   *  those Codealong runs can be, whatever their harness. */
  switchesModel?: boolean;
  /** Its running sessions that Codealong types into can be switched to another of `efforts`;
   *  those Codealong runs can be where their agent offers the level. */
  switchesEffort?: boolean;
  /** Its harness takes those switches while a turn runs, for the turn's next step, where others
   *  wait for the turn to end. Missing from Macs before 2026-11-10. */
  switchesMidTurn?: boolean;
  /** Its sessions that go on in its app can be switched to another of `models` and `efforts`
   *  there, running or not, from their next turn. Missing from Macs before 2026-10-17. */
  switchesInApp?: boolean;
}

/** Where a session that isn't running goes on, and on what (start.rs `resume`): one of its
 *  account's `resumes`, with the model and effort where that place takes them; null for the
 *  account's own. */
export interface ResumeIn {
  place: StartPlace;
  model: string | null;
  effort: string | null;
}

/** One member of a routing group as `StartOptions` sends it (peer/src/api.rs `GroupMember`). */
export interface GroupMember {
  account: string;
  model?: string;
  /** Missing from Macs before 2026-10-19. */
  effort?: string;
}

/** A routing group as `StartOptions` sends it (peer/src/api.rs `RoutingGroup`). */
export interface RoutingGroup {
  name: string;
  strategy: RoutingStrategy | string;
  members: GroupMember[];
}

export interface StartOptions {
  /** In config order. */
  accounts: StartAccount[];
  /** Folders the Codex app has as projects. A thread started anywhere else adds one. */
  codexProjects: string[];
  /** Models a Claude Code start in the terminal can ask for, as `--model` takes them. */
  claudeModels: { id: string; label: string }[];
  /** Effort levels a Claude Code start can ask for, lowest first, as `--effort` takes them.
   *  Missing from devices that don't know them yet: they start on the account's own. */
  claudeEfforts?: { id: string; label: string }[];
  /** Routing groups from `[routing]`. */
  groups?: RoutingGroup[];
  /** The routing group `[routing] auto` names, or empty. */
  autoGroup?: string;
  /** The routing group Auto starts from in a project, by its name, over `autoGroup` there
   *  (`[[projects]] default`; start.ts `autoGroupIn`). */
  projectDefaults?: Record<string, string>;
}

export interface StartSpec {
  laneId: string;
  account: string;
  place: StartPlace;
  prompt: string;
  /** `--model` for a terminal start; null starts on the account's default. */
  model: string | null;
  /** `--effort` for a Claude Code start; null starts on the account's own. */
  effort: string | null;
  /** A terminal start in a new worktree Claude Code makes (`--worktree`). */
  worktree: boolean;
  /** The todos the session is for, one or several on one prompt: each is linked once it starts. */
  todos: string[];
  /** The session that asked for the start, by key: the new one is marked as started by it. */
  from?: string | null;
  /** A helper of `from` (`codealong start --helper`), which `report` says `from` is told of. */
  helper?: boolean;
  report?: boolean;
}

export interface Started {
  /** The new session's key when Codealong chose its ID, so the board can select it. */
  key: string | null;
  /** What to do next, when the app is waiting on you. */
  message: string | null;
}

/** A start a session's agent asked for whose session isn't on the board yet (starting.rs
 *  `Starting`), listed under that session until its session shows. */
export interface Starting {
  /** When it was started: which start it is. */
  at: number;
  account: string;
  /** Its harness's word (Codex). */
  harness: string;
  /** The worktree it was started in (a lane id), and its name. */
  lane: string;
  place: string;
  /** Started as a helper of the session. */
  helper: boolean;
  /** How its prompt opens: its title until its session has one. */
  opens: string | null;
  /** The terminal it was started in, where it's found, when it was. */
  terminal: string | null;
  /** It's gone a few minutes without its session: its terminal likely waits on a screen of its
   *  own. */
  stuck: boolean;
  /** What its terminal shows at its foot, when that was read. */
  screen: string | null;
  /** Its harness's process, found in its terminal as its screen was last read: what shows its
   *  tab, answers it and stops it. */
  pid: number | null;
  /** What its screen offers to pick from, once it's stuck, to answer it with (starting.rs
   *  `choices`). */
  choices: Choice[];
}

/** One of the choices a terminal UI's screen lists, as it numbers it (starting.rs `Choice`). */
export interface Choice {
  number: number;
  words: string;
}

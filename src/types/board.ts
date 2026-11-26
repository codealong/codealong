// The board itself: everything the engine publishes to the windows at once, with its projects,
// its notices and how busy the machine is. Mirrors src-tauri/src/model.rs; `Load` is
// src-tauri/peer/src/model.rs's.

import type { Mcp, Resets, SignIn, Training, Usage } from './accounts';
import type { Improve } from './app';
import type { BackgroundsCfg, DecisionsCfg, DevicesCfg, ExperimentalCfg, NewTerminal, NudgeAbout, Profile, QueueCfg, RepoCfg, ReviewsCfg, SessionsCfg, TeamFile, Theme, ToolDisplay, WhenLimited } from './config';
import type { TodoQueue } from './feeds';
import type { Lane } from './lane';
import type { PrItem, PrStopped } from './pr';
import type { ScheduleItem } from './schedules';
import type { SideGlance } from './side';
import type { Starting } from './start';
import type { TerminalGlance } from './terminals';
import type { Label, Todo, TodoHint } from './todos';
import type { ListeningGlance, Meeting, MeetingOffer } from './voice';

export type LogoStatus = 'error' | 'needs' | 'unread' | 'working' | 'idle';

export interface ProjectSummary {
  name: string;
  lanes: number;
  repos: { name: string; lanes: number; folder: boolean }[];
  /** Cached GitHub organization avatar as a `data:image/…` URL, when the project belongs to one org. */
  icon?: string | null;
}

/** How busy a machine is, as it reads itself every half a minute (load.rs): what a session
 *  started there would share it with. Its numbers are rounded. */
export interface Load {
  level: LoadLevel;
  /** Its load average over five minutes for each CPU: at 1 every CPU is busy. Null where the OS
   *  keeps none (Windows). */
  perCpu: number | null;
  cpus: number;
  /** How much of its memory is free, in percent. */
  memoryFree: number | null;
  /** Memory is getting short (macOS's memory pressure isn't normal). */
  memoryShort: boolean;
}

/** calm: CPUs to spare; busy: every CPU busy or memory getting short; strained: far more work
 *  than CPUs, or memory critical, so everything there waits. */
export type LoadLevel = 'calm' | 'busy' | 'strained';

/** Something in the background that isn't working, said until it's put right or dismissed. */
export interface Notice {
  /** What it's about (`config`, `github`, `todos`…), one of each at most. */
  key: string;
  /** A few words. */
  title: string;
  /** The whole of it. */
  text: string;
  /** When what it tells of ends, where that's known: the hour of a rate limit. */
  until?: number | null;
  /** The PR (`owner/name#number`) whose review didn't start or wasn't posted, which the notice
   *  offers to try again (review.ts `tryAgain`). */
  reviewAgain?: string | null;
  /** The account whose sign-in was turned down, which the notice offers to sign in
   *  (board.rs `signed_out_notices`). */
  signIn?: string;
  /** What a terminal shows that the notice is about, drawn as it's laid out: the screen a start
   *  that never made its session waits on (origins.rs `stuck_notices`). */
  screen?: string;
}

export interface Board {
  generatedAt: number;
  /** Which of the engine's published boards it is, counted from 1 at launch: what a push
   *  changes (boardPush.ts). 0, or missing from a snapshot's, for one never published. */
  rev?: number;
  /** App-wide status from the engine; view and project filters do not change the logo. */
  logoStatus: LogoStatus;
  /** `editors`: the editors installed here, in menu order (editors.ts). */
  apps: { terminal: string; terminalId: string; editor: string; editorId: string; editors?: Profile[]; diffEditor?: Profile | null; newTerminal?: NewTerminal; runIn?: 'codealong' | 'terminal' };
  projects: ProjectSummary[];
  lanes: Lane[];
  /** What isn't working in the background (notices.ts). */
  notices: Notice[];
  loading: boolean;
  githubAt: number | null;
  /** GitHub's first poll has ended: until then the PRs are what's come so far. */
  githubPolled: boolean;
  /** When each polled repo's PRs were last as GitHub had them, by slug; null for one asked about
   *  but never read. Absent when there's none. */
  githubCurrent?: Record<string, number | null>;
  usage: Usage[];
  /** Whether each account's provider may train on what it sends (training.rs), for the accounts
   *  read so far. Absent from engines of older builds. */
  training?: Training[];
  /** The limit resets each account holds (resets.rs), for the accounts read so far. Absent from
   *  engines of older builds. */
  resets?: Resets[];
  /** Where each boundary group member's sign-in stands (signin.rs). Absent when no boundary
   *  group lists an account, and from engines of older builds. */
  signIns?: SignIn[];
  /** The MCP servers each account's agents have (mcp.rs), for the accounts read so far. Absent
   *  from engines of older builds. */
  mcp?: Mcp[];
  /** The side conversations asked on this Mac (side.rs `glances`), kept and open, by the key of
   *  the session or PR they're about, the latest started first. Absent from engines of older
   *  builds. */
  sides?: Record<string, SideGlance[]>;
  /** The starts a session's agent asked for whose sessions aren't here yet (starting.rs
   *  `by_session`), by the key of the session that asked, the latest first. This Mac's own.
   *  Absent from engines of older builds, and when there are none. */
  starting?: Record<string, Starting[]>;
  /** Codealong's own terminals (terminals.rs `glances`), by the key of the session each is for,
   *  in the order they were opened. Absent from engines of older builds, and when there are none. */
  terminals?: Record<string, TerminalGlance[]>;
  /** The sessions that are listening (listen.rs `glances`), by session key. This Mac's own.
   *  Absent from engines of older builds, and when there are none. */
  listenings?: Record<string, ListeningGlance>;
  /** The meeting in your calendar that's about to start or just has, on offer to take its notes
   *  (meetings.rs). This Mac's own. Absent when none is. */
  meeting?: MeetingOffer;
  /** The meeting counted down to before it's offered (meetings.rs `counting_down`), from
   *  `[voice] meeting_countdown` minutes before it starts. This Mac's own. Absent when none is. */
  meetingSoon?: Meeting;
  /** Pinned session keys, in order. */
  pinned: string[];
  /** Pinned worktrees (lane ids), in order. */
  pinnedWorktrees: string[];
  /** Pinned PRs (`owner/name#number`), in order. */
  pinnedPrs?: string[];
  /** Pinned repos (boards/rules.ts `repoOf`), in order. Absent from engines of older builds. */
  pinnedRepos?: string[];
  /** Every PR in the repos the projects cover, newest activity first. */
  prs: PrItem[];
  /** Who `gh` is signed in as. */
  githubLogin: string | null;
  /** How busy this machine is (load.rs), none until it's first read. Absent from engines of
   *  older builds. */
  load?: Load | null;
  /** True while `[debug] perf_log` is on: the window reports its timings to the perf log. */
  perfLog?: boolean;
  /** From config.toml. */
  theme: Theme;
  /** How big the board's windows draw, in percent, from config.toml (zoom.ts); actual size when
   *  it's missing. */
  zoom?: number;
  /** Whether the board's windows show what's behind them through macOS's glass, from
   *  `[appearance] glass` (src/glass.ts); off when it's missing. */
  glass?: boolean;
  /** Whether keyboard shortcuts show all the time, from `[appearance] show_keys` (keysShown.ts);
   *  on when it's missing, as by default. Off, they show while ⌘ or ⌃ is held. */
  showKeys?: boolean;
  /** Whether a session the quick panel starts in the background or in Codealong is shown in the
   *  main window as it starts, from `[sessions] show_started` (newSession.ts); off when missing. */
  showStarted?: boolean;
  /** How tool calls appear in conversations, from `[appearance] tools` (toolDisplay.ts);
   *  `folded` when missing, as by default. */
  tools?: ToolDisplay;
  /** Every todo, in your order (src-tauri/src/todos.rs). */
  todos: Todo[];
  /** todos.json didn't read the last time it was read: `todos` is what it held before, and a
   *  change is turned away until it's fixed. Missing while it reads. */
  todosUnreadable?: boolean;
  /** Today on the engine's Mac, as a due date is written (`2026-11-18`): which todos are due goes
   *  by it (barfeed.rs `todo_lists`). Missing on older boards. */
  today?: string;
  /** The todos that wait on you, due and ready to start, as of `today` (barfeed.rs `todo_queue`):
   *  the main window's Todo panel and the waiting window go through them, as the notch does
   *  through the bar's copy. Missing on older boards. */
  todoQueue?: TodoQueue;
  /** The next time a todo's standing in what waits on you changes by the clock alone (todos.rs
   *  `todo_clock`), which the board changes by. Missing with nothing ahead, and on older boards. */
  todoClock?: number;
  /** Todo labels, in your order. */
  labels: Label[];
  /** The schedules (src-tauri/src/schedules/), with what happened to each. Missing on older
   *  boards: none. */
  schedules?: ScheduleItem[];
  /** Why schedules.json or schedule-runs.json doesn't read, while it doesn't. */
  schedulesProblem?: string;
  /** The quick panel's shortcut from config.toml, as it writes it; empty when it's off. Missing on older boards. */
  quickShortcut?: string;
  /** The shortcut that opens the quick panel set to a chat in no project, as config.toml writes it; empty when off. Missing on older boards. */
  chatShortcut?: string;
  /** The shortcut that goes through what waits on you, as config.toml writes it; empty when off. */
  waitingShortcut?: string;
  /** Improve Codealong's shortcut, as config.toml writes it; empty when off. Missing on older boards. */
  improveShortcut?: string;
  /** The shortcut that brings the board's window forward, as config.toml writes it; empty when off. Missing on older boards. */
  mainWindowShortcut?: string;
  /** The shortcuts held to talk, to the session you're looking at and to your orchestrator, as
   *  config.toml writes them; empty when off, as they are until voice is turned on. Missing on
   *  older boards. */
  talkShortcut?: string;
  talkOrchestratorShortcut?: string;
  /** The shortcut that opens the quick panel set to listen to a meeting, as config.toml writes
   *  it; empty when off, as it is until voice is turned on. Missing on older boards. */
  listenShortcut?: string;
  /** Where a session that improves Codealong goes (improve.ts). Missing on older boards. */
  improve?: Improve;
  /** What Codealong asks decision models, from `[decisions]` in config.toml. Missing on older
   *  boards. */
  decisions?: DecisionsCfg;
  /** The experiments turned on, from `[experimental]` in config.toml. Missing on older boards:
   *  none. */
  experimental?: ExperimentalCfg;
  /** `[backgrounds]` from config.toml: per-session AI background settings. Missing on older boards. */
  backgrounds?: BackgroundsCfg;
  /** `[queue]` from config.toml as the windows go by it: its `order` the kinds in your order
   *  (config.rs `QueueCfg::settled`). Missing on older boards: the default. */
  queue?: QueueCfg;
  /** `[reviews]` from config.toml. Missing on older boards. */
  reviews?: ReviewsCfg;
  /** Open todos whose work Jev says looks done, by todo id. Missing on older boards. */
  todoHints?: Record<string, TodoHint>;
  /** `[devices]` from config.toml. Missing on older boards. */
  devices?: DevicesCfg;
  /** `[limits] when_hit`: whether a session a usage limit stopped is moved by itself. */
  whenLimited?: WhenLimited;
  /** Session automation settings on this device. Missing on older boards. */
  sessions?: SessionsCfg;
  /** `[pull_requests] nudge_about`: what a PR's nudge tells its session about. Missing on older
   *  boards: everything. */
  nudgeAbout?: NudgeAbout;
  /** What becomes of each nudged PR's news while its primary session, on this Mac, isn't running,
   *  by the PR's key (nudges.rs `stopped_primaries`). Missing on older boards. */
  prStopped?: Record<string, PrStopped>;
  /** Your `[repos."github.com/…"]` tables, by key. Missing on older boards. */
  repos?: Record<string, RepoCfg>;
  /** The repos' team files read from their branches. Missing on older boards. */
  teamFiles?: TeamFile[];
}

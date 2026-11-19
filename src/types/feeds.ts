// What the windows that don't get the board are sent in its place: the floating bar's and the
// notch's feed (barfeed.rs) and a PR for the notch to show, a desktop pin's (desktop.rs) and the
// lines between pins (lineage.rs), and what the peek is asked to show (peek.rs).

import type { Board, LogoStatus } from './board';
import type { Profile, Theme } from './config';
import type { DeviceTag, Lane } from './lane';
import type { PrItem } from './pr';
import type { Emblem, Raised, Session, Status } from './session';
import type { Label, Todo } from './todos';
import type { ListeningGlance, Meeting, MeetingOffer } from './voice';

/** A session that's listening, as the bar shows it (barfeed.rs `BarListening`). */
export interface BarListening extends ListeningGlance {
  key: string;
  /** The worktree its session is in, to show it there. */
  laneId: string;
  title: string;
}

/** The floating bar's groups, most urgent first. */
export type BarGroup = 'needs' | 'unread' | 'working';

/** What the floating bar shows (barfeed.rs): the engine sends it instead of the board. */
export interface BarFeed {
  loading: boolean;
  /** The board's time: a change the bar shows ahead of the engine goes once a feed this new
   *  arrives (ahead.ts). Missing from a feed an older app sent. */
  generatedAt?: number;
  theme: Theme;
  /** The zoom, in percent: the session or PR the notch shows draws at it (zoom.ts). */
  zoom?: number;
  /** Whether keyboard shortcuts show all the time (keysShown.ts); missing from an older app's. */
  showKeys?: boolean;
  /** What the mark's colors say, as on the board, with your other devices' sessions it lists. */
  logoStatus: LogoStatus;
  perfLog: boolean;
  /** The terminal app's name, for menu items such as "Show in Ghostty". */
  terminal: string;
  /** The projects, in your order, for the bar's project menu. */
  projects: string[];
  /** In bar order: by group, then each the longest first. */
  items: BarItem[];
  /** The other sessions started in the last day, the newest status first. */
  recent: BarRecent[];
  /** Sessions you snoozed, out of Needs you and Your turn until their time comes (running ones
   *  stay in Working), the soonest back first. */
  snoozed: BarSnoozed[];
  /** Open PRs that ask for your review, the longest wait first, as Needs you is. */
  reviews: BarReview[];
  /** Todos ready to start (not started, and waiting on nothing), in your order, but those in
   *  `due` and those snoozed. */
  todos: BarTodo[];
  /** Todos that wait on you by their due date, the most overdue first (barfeed.rs
   *  `todo_lists`). Missing on older feeds. */
  due?: BarTodo[];
  /** What waits on you in your order (`[queue] order`, queue.ts `queueKinds`): the notch works
   *  through its rows and counts by it. The reviews and todos of a kind left out aren't on the
   *  feed. Missing on older feeds: the default order. */
  queueOrder?: string[];
  /** The other devices whose sessions it lists (`[devices] in_bar`), by short id: those connected
   *  now. What one that wasn't on the feed before brings isn't news (mark.ts). Missing from a feed
   *  an older app sent. */
  devices?: string[];
  /** The sessions that are listening, the first to begin first. Missing while there are none,
   *  and from a feed an older app sent. */
  listenings?: BarListening[];
  /** The meeting on offer (`Board.meeting`): the bar asks whether to take its notes. */
  meeting?: MeetingOffer;
  /** The meeting counted down to before its offer (`Board.meetingSoon`). */
  meetingSoon?: Meeting;
  /** Voice is turned on: the menu bar item's menu has Listen now. Missing while it's off. */
  voice?: boolean;
}

export interface BarItem {
  key: string;
  group: BarGroup;
  status: Status;
  /** Working only in that it waits on a command left running in the background: drawn with a
   *  clock. Missing from a feed an older app sent. */
  waiting?: boolean;
  /** Needs you only because its last reply asks you something: marking it seen settles it.
   *  Missing from a feed an older app sent. */
  asks?: boolean;
  /** Its last turn was cut off (Session.cutOff): marking it seen settles it too. */
  cutOff?: number;
  title: string;
  /** Its emblem (Session.emblem), drawn on its pill beside its title. */
  emblem?: Emblem;
  account: string;
  laneId: string;
  callsign: string;
  project: string;
  /** In a worktree the board keeps out of sight (Elsewhere's, while it holds no session you
   *  haven't archived). */
  hidden: boolean;
  detail: string | null;
  /** When its status began, for its age. */
  since: number;
  openTarget: string;
  /** The lane's own entry for `openTarget`, when it lists one. */
  target: { label: string; detail: string | null; disabled: boolean } | null;
  /** The terminal app it runs in (`Session.terminal`), for its words where `target` is null. */
  terminal?: string;
  /** When it's back, while it's snoozed. */
  snoozedUntil: number | null;
  /** Quieted: its turns don't wait on you. Missing from a feed an older app sent. */
  quiet?: boolean;
  /** Moved to the top of what waits on you (`Session.raised`): first in its group and the
   *  notch's queue. */
  raised?: Raised;
  /** Your turn only to be read, its work being finished (`Session.finished`): last in its group
   *  and in the notch's queue. */
  finished?: boolean;
  /** Its turn is running and Codealong can stop it. Missing from a feed an older app sent. */
  stoppable?: boolean;
  /** The device it's on, when that's another of yours: its key starts `@<short>:`. */
  device?: DeviceTag;
}

/** A session on the bar's recent pill: started in the last day, and in none of its groups. */
export type BarRecent = Omit<BarItem, 'group'> & { group: 'recent' };

/** A session on the bar's snoozed pill: snoozed, and not running. */
export type BarSnoozed = Omit<BarItem, 'group' | 'snoozedUntil'> & { group: 'snoozed'; snoozedUntil: number };

export interface BarReview {
  key: string;
  number: number;
  title: string;
  repo: string;
  /** `owner/name`, for asking GitHub about it. */
  repoSlug: string;
  url: string;
  project: string;
  /** Who asked for your review, when GitHub says. */
  requestedBy: string | null;
  /** Since when it waits on you: the latest request to you, else the PR's last update. The queue
   *  keeps one you dealt with out until this moves on (queue.ts `Handled`). */
  since: number;
  /** The PR's last update: the card showing it reads it again when this moves. */
  updatedAt: number;
  /** Codealong's agent approves it: what's asked of you is a sign-off (prStatus.ts `agentApproves`). */
  approves?: boolean;
  /** It goes ahead of the sessions that need you in the queue (`PrItem.reviewFirst`). */
  first?: boolean;
  /** It's yours, and waits on you to merge it rather than on your review (`PrItem.waitsToMerge`). */
  merge?: boolean;
}

/** A PR the board lists, with the worktrees that hold it and the ones its sessions are listed
 *  under, for a window without the board (the notch) to show its card (PrCard.tsx). */
export interface PrAt {
  pr: PrItem;
  lanes: Lane[];
  apps: Board['apps'];
}

/** The todos that wait on you (barfeed.rs `TodoQueue`). */
export interface TodoQueue {
  /** Due by `[queue] due_within` days from today: the most overdue first. */
  due: BarTodo[];
  /** Ready to start but for those, in your order. */
  ready: BarTodo[];
}

export interface BarTodo {
  id: string;
  title: string;
  /** Empty until you pick one or link a worktree. */
  project: string;
  /** The worktree a session for it starts in, when it has one. */
  callsign: string | null;
  /** The day it's due, as `2026-10-03`. Missing on older feeds. */
  due?: string | null;
  /** Since when it waits on you: a todo due from the start of its day, a ready one from when it
   *  was made. Missing on older feeds. */
  since?: number;
  /** Its place in your order, which the queue goes by for ready todos. Missing on older feeds. */
  order?: number;
}

/** A session, worktree, todo or PR pinned to the desktop (desktop.rs), by its session key, lane
 *  id, todo id or PR key. */
export interface DesktopPinned {
  ref: 'session' | 'worktree' | 'todo' | 'pr';
  key: string;
}

/** Lines between the cards of sessions pinned to the desktop, one of which came of the other,
 *  drawn while one is hovered (lineage.rs): each card in the window's points, y down. */
export interface LineageFeed {
  theme: Theme;
  lines: { from: { x: number; y: number; w: number; h: number }; to: { x: number; y: number; w: number; h: number }; side: 'left' | 'right' }[];
}

/** What a card pinned to the desktop shows, cut from the board: the engine sends it instead of
 *  the board. */
export interface DesktopFeed {
  pin: DesktopPinned;
  /** Its title now, or when last seen once the board no longer has it. */
  title: string | null;
  /** True until the engine's first full scan has finished. */
  loading: boolean;
  generatedAt: number;
  theme: Theme;
  /** Whether keyboard shortcuts show all the time (keysShown.ts); missing from an older app's. */
  showKeys?: boolean;
  /** The terminal app's name, for menu items such as "Show in Ghostty". */
  terminal: string;
  /** Your editor and the others installed, as the board has them, for the Open in menus. */
  editor: string;
  editorId: string;
  editors: Profile[];
  /** The worktree's own lane; the session's; the todo's linked worktrees and its sessions'; the
   *  worktrees that hold the PR and its sessions'. A worktree keeps all its sessions, the others
   *  only the ones the card shows. */
  lanes: Lane[];
  /** The PR, for a PR's card; for a worktree's or a session's, its worktree's PR when the board
   *  lists it (desktop.rs `listed_pr`). Missing on feeds from before PRs could be pinned. */
  prs?: PrItem[];
  /** The todo, for a todo's card. */
  todos: Todo[];
  labels: Label[];
}

/** What the floating bar's peek is asked to show (peek.rs). */
export interface PeekOpen {
  /** Answered with the page's height once it's laid out. */
  id: number;
  lane: Lane;
  session: Session;
  terminal: string;
  /** The editors installed here, for the session menu's Open folder in (menus.ts). Missing from
   *  an app older than the menu. */
  apps?: Board['apps'];
  theme: Theme;
  /** The zoom the card draws at, in percent (zoom.ts). */
  zoom?: number;
  /** The tallest the peek's window can be where it opens. */
  room: number;
  /** Under the floating bar; otherwise over it. */
  below: boolean;
  /** It takes the keyboard as it shows, rather than on a click. */
  focus: boolean;
  /** Its name is ready to change (Rename in a pill's menu). */
  rename: boolean;
  /** The words its conversation's find bar opens on: what a search in the main window found it
   *  by. */
  find?: string | null;
  /** The window it opened beside (the main window's PR or todo panel), by its label: what goes
   *  wrong is said there, as a toast, rather than on the bar, which may not be up. */
  beside?: string | null;
  /** `[debug] perf_log` is on, so the page reports its timings. */
  perfLog?: boolean;
}

/** What the peek shows, as the bar and the peek's page hear it. */
export interface PeekShown {
  key: string | null;
  /** It has the keyboard. */
  engaged: boolean;
}

/** A note the bar shows in the place of a session's pill: a failed open, or what a send from the
 *  peek did that you should know about (the prompt is on the clipboard, say). */
export interface BarNote {
  key: string;
  message: string;
  tone: 'error' | 'info';
}

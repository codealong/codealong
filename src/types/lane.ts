// A worktree (a lane) as the board lists it: its git, whether it can be cleaned up or reused, the
// ways to open it, its app's preview, and the device it's on. Mirrors `Lane` and its parts in
// src-tauri/peer/src/model.rs.

import type { Pr } from './pr';
import type { Session, Status } from './session';
import type { LabelColor } from './todos';

export type Bay = 'needs' | 'working' | 'unread' | 'review' | 'idle' | 'merged';
export type Origin = 'main' | 'script' | 'codex' | 'claude-app' | 'scratchpad' | 'temp' | 'folder' | 'other';

export interface GitInfo {
  upstream: string | null;
  ahead: number;
  behind: number;
  dirty: number;
  lastCommit: string | null;
  lastCommitAt: number | null;
  added: number | null;
  removed: number | null;
  /** The default branch on the remote (`origin/main`), and commits only HEAD has
   *  (`baseAhead`) and only it has (`baseBehind`), whatever the branch tracks. Missing from
   *  other Macs on older versions. */
  base?: string | null;
  baseAhead?: number | null;
  baseBehind?: number | null;
  /** When the worktree last took new work, from its HEAD's reflog: another branch checked out,
   *  or its branch reset or made afresh. */
  resetAt?: number;
  /** How many times it took new work, as far back as its reflog goes. */
  newWorkCount?: number;
  /** Commits HEAD has that no remote branch has: they'd go with the worktree. */
  unpushed?: number;
  /** Why the worktree is locked (`git worktree lock`), when it is. */
  locked?: string;
}

/** Whether a worktree can be removed with nothing in it lost (board.rs `cleanup`), and why not
 *  when it can't, in a few words: its menu's Clean up worktree. */
export interface Cleanup {
  ok: boolean;
  why?: string;
}

export interface Target {
  id: string;
  icon: string;
  /** An app's own icon (a data URL) to show in place of `icon`: the editors' in Open menus. */
  image?: string;
  label: string;
  detail: string | null;
  kbd: string[] | null;
  disabled: boolean;
  /** `browser` is a session's tabs in Chrome: listed apart from the ways to open it. */
  group: 'session' | 'browser' | 'lane' | 'pr';
}

export interface Lane {
  id: string;
  project: string;
  repo: string;
  repoSlug: string | null;
  /** What GitHub calls the repo now, when that isn't `repoSlug`: it, or its owner, was renamed. */
  repoSlugNow?: string;
  path: string;
  callsign: string;
  origin: Origin;
  branch: string | null;
  detached: string | null;
  bay: Bay;
  glyph: Status;
  parked: boolean;
  hidden: boolean;
  /** Backtick spans render as code; {ago:ms} and {for:ms} render as live times. */
  headline: string;
  detail: string | null;
  sessions: Session[];
  pr: Pr | null;
  git: GitInfo | null;
  lastActivity: number;
  /** Jev reads every session here as finished and nothing runs: maybe ready to clean up. */
  looksFinished?: boolean;
  targets: Target[];
  defaultTarget: string | null;
  /** The device it's on, when that's another of your devices; missing for this device's own. */
  device?: DeviceTag;
  /** Its app, to open in the browser: the URL set for it, or a page a server in its folder
   *  serves. This device's worktrees only. */
  preview?: Preview;
  /** What it holds, for a worktree that takes one piece of work after another (its menu's Reuse
   *  for new work, or a main checkout left as it is). Missing for one kept for a single piece of
   *  work (board::reuse_of). */
  reuse?: Reuse;
  /** Marked neither way, but its branch has carried several merged PRs: Settings suggests
   *  making it reusable. */
  looksReusable?: boolean;
  /** Codealong's own chats: a session started here gets a folder of its own, for a question
   *  that belongs to no project. It's on the board with no sessions too, hidden. */
  startsChats?: boolean;
  /** The folder an app keeps the chats it starts with no folder of yours in, Codealong's own
   *  among them: listed together for having no folder, so its sessions don't share a worktree. */
  chats?: boolean;
  /** Codealong's own source: the worktree holds the app's tauri.conf.json (the engine's
   *  improve.rs). Improve Codealong starts its sessions here. */
  codealong?: boolean;
  /** Whether it can be removed with nothing in it lost. Missing until its git is read, and from
   *  another Mac's older Codealong. */
  cleanup?: Cleanup;
}

/** What a reusable worktree holds: no work, so new work can start there; work under way; its
 *  PR in review; or its PR merged, so new work can start once it's back on the default branch. */
export type Reuse = 'free' | 'open' | 'review' | 'landed';

/** A worktree's app: where it opens, and the server behind it while one runs. */
export interface Preview {
  /** The URL set for it (`codealong preview set`, or its menu), or `http://localhost:<port>`
   *  for a page a server in its folder serves. */
  url: string;
  /** The URL was set; false when it's a page found serving there. */
  set: boolean;
  /** The port set with the URL: which of the worktree's servers is the app. */
  port: number | null;
  /** The server serving it; null when its app isn't running. */
  server: PreviewServer | null;
  /** The Chrome profile it opens in: the worktree's own, or else its project's. Missing opens
   *  it in the default browser. */
  chromeProfile?: string;
  /** The profile was set for the worktree, not taken from its project. */
  chromeProfileSet: boolean;
}

/** A process listening on a TCP port, whose folder is in the worktree. */
export interface PreviewServer {
  pid: number;
  port: number;
  /** The process's name, such as `node`. */
  name: string;
  title: string | null;
  startedAt: number;
  /** The key of the session whose command started it. */
  session: string | null;
}

/** Which of your other devices a lane is on, as its badge shows it. */
export interface DeviceTag {
  name: string;
  short: string;
  color: LabelColor;
  online: boolean;
  lastSeen: number | null;
}

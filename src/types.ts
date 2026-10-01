// The board as the Rust engine serializes it (src-tauri/src/model.rs). Option<T> arrives as null.

export type Harness = 'claude' | 'codex' | 'opencode' | 'antigravity';
export type Surface = 'cli' | 'app';
export type Status = 'needs' | 'error' | 'working' | 'unread' | 'idle' | 'stopped';
export type LogoStatus = 'error' | 'needs' | 'unread' | 'working' | 'idle';
export type Bay = 'needs' | 'working' | 'unread' | 'review' | 'idle' | 'merged';
export type Origin = 'main' | 'script' | 'codex' | 'claude-app' | 'scratchpad' | 'temp' | 'folder' | 'other';
export type PrState = 'draft' | 'open' | 'approved' | 'changes' | 'merged' | 'closed';
/** Whether GitHub would merge an open PR now (its `mergeStateStatus`); `unknown` once it's closed. */
export type Merge = 'clean' | 'conflicts' | 'behind' | 'blocked' | 'unstable' | 'unknown';

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
  /** The session whose record holds this one's conversation until it writes its own: a Claude
   *  Code conversation moved to the background goes on in a job that writes nothing until its
   *  next prompt, and then starts its own with a copy of the conversation. */
  continues?: string;
  /** The session it came from, when its harness or Codealong wrote that down. */
  origin?: SessionOrigin;
  /** Busy but silent for a long while: stuck mid-turn. When it went quiet. */
  stalledSince: number | null;
  /** Jev's read of how its turn ended, when `[jev] sessions` is on (insights.rs). */
  insight?: SessionInsight;
  /** Needs you only because its last reply asks you something: it sits at its prompt, so a
   *  reply is the answer. Not set while it waits on a permission prompt. */
  asks?: boolean;
  /** Codealong runs it itself, over the Agent Client Protocol (acp.rs): its panel sends to it
   *  and answers what it asks, and opening it shows the panel. */
  hosted?: boolean;
  /** Its turn is running and Codealong can stop it (harness::stoppable): Stop sends its harness
   *  what its own Stop sends, or presses Escape in its terminal. */
  stoppable?: boolean;
  /** Snoozed from Codealong until then: the floating bar and the mark leave it out of Needs you
   *  and Your turn until the time comes (board::apply_snooze). */
  snoozedUntil?: number;
  /** It worked in a reusable worktree before that worktree last took new work: listed after the
   *  current work's sessions, and out of the worktree's status (board::earlier). */
  earlier?: boolean;
  /** The Chrome tabs it drives through Claude in Chrome, to bring back (chrometabs.rs). */
  chrome?: ChromeTabs;
  resumeCommand: string | null;
  /** The Claude app is signed into this session's account. */
  appAccount: boolean;
  /** Opens this session where it lives: its terminal tab, the Claude app, Codex, or Antigravity. */
  openTarget: string;
  /** Why a session from another lane shows here: "made this worktree" or "worked here". */
  visitor: string | null;
  /** This session's own sentence; backtick spans and time tokens as in lane headlines. */
  headline: string;
  detail: string | null;
}

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
  | { kind: 'antigravity' };

export interface Checks {
  passed: number;
  failed: number;
  pending: number;
  skipped: number;
  failing: string[];
}

export interface Pr {
  number: number;
  title: string;
  url: string;
  state: PrState;
  checks: Checks;
  merge: Merge;
  updatedAt: number;
  mergedAt: number | null;
  closedAt: number | null;
}

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

export interface ProjectSummary {
  name: string;
  lanes: number;
  repos: { name: string; lanes: number; folder: boolean }[];
}

export type Severity = 'normal' | 'warning' | 'critical';

/** One plan limit: the 5-hour session, the week, or a per-model week such as Fable's. */
export interface Meter {
  key: string;
  label: string;
  window: string;
  percent: number;
  resetsAt: number | null;
  severity: Severity;
  model: string | null;
}

export interface Usage {
  account: string;
  harness: Harness;
  plan: string | null;
  meters: Meter[];
  asOf: number | null;
  error: string | null;
  needsSignIn: boolean;
  loading: boolean;
  /** Its `preferred` setting: Auto starts it over accounts without one while it has room. */
  preferred?: boolean;
}

/** An `[[accounts]]` table in config.toml. */
export interface AccountCfg {
  name: string;
  harness: Harness;
  home: string;
  command: string | null;
  /** Where a new session starts unless you pick another; absent leaves it to the usual place. */
  start?: StartPlace | null;
  /** Auto starts it over accounts that aren't preferred while it has room. */
  preferred?: boolean;
}

/** A `[[projects]]` table in config.toml. */
export interface ProjectCfg {
  name: string;
  /** `github.com/owner/*` or `github.com/owner/name`. */
  repos: string[];
  /** Folders that aren't repos but belong to the project. */
  folders: string[];
  /** The Chrome profile its worktrees' previews open in; missing opens them in the default
   *  browser. */
  chrome_profile?: string;
}

/** config.toml, as the engine parses it (src-tauri/src/config.rs). Keys are the file's own. */
export interface Config {
  scan: string[];
  apps: { terminal: string; editor: string; new_terminal?: NewTerminal; move_pointer?: boolean };
  appearance: { theme: Theme };
  /** Shortcuts that work in any app, as config.toml writes them (`Option+Cmd+/`); empty is off. */
  shortcuts: { quick_panel: string };
  /** What Codealong asks TypeSafe's Jev, feature by feature. Older files have none: all off. */
  jev?: JevCfg;
  accounts: AccountCfg[];
  projects: ProjectCfg[];
  /** How many days of sessions have what was said in them searched; 0 is off. */
  search?: { days: number };
  /** Your other Macs. Older files have none: off. */
  devices?: DevicesCfg;
  /** Automatic updates. Older files have none: on. */
  updates?: { check: boolean };
  debug: { perf_log: boolean };
}

/** Where automatic updates stand (src-tauri/src/updates.rs): this copy's version, and a state. */
export type UpdateView = { current: string } & (
  | { state: 'off' } // a development build, which never updates itself
  | { state: 'idle' }
  | { state: 'checking' }
  | { state: 'upToDate'; checkedAt: number }
  | { state: 'installing'; version: string }
  | { state: 'ready'; version: string; notes: string | null } // starts at the next launch
  | { state: 'failed'; error: string; at: number }
);

/** `[devices]` in config.toml: Codealong on your other devices (src-tauri/src/peers.rs). */
export interface DevicesCfg {
  enabled: boolean;
  /** This device's name on the others; empty is the computer's name. */
  name: string;
  /** Find your devices on your tailnet. */
  tailscale: boolean;
  /** Reach linked devices anywhere, through iroh. */
  iroh: boolean;
  /** Let linked devices send prompts, answer approvals and start sessions here. */
  remote_control: boolean;
  port: number;
}

export const DEVICES_OFF: DevicesCfg = { enabled: false, name: '', tailscale: true, iroh: true, remote_control: false, port: 47471 };

export type DeviceVia = 'tailscale' | 'iroh' | 'test';

/** Which of your other devices a lane is on, as its badge shows it. */
export interface DeviceTag {
  name: string;
  short: string;
  color: LabelColor;
  online: boolean;
  lastSeen: number | null;
}

/** Your other devices, as the engine hands them to the windows (`devices` event). */
export interface DevicesView {
  enabled: boolean;
  me: DevicesMe;
  /** Linked devices with boards of their own (Macs), in the order they were linked. */
  devices: DeviceView[];
  /** Linked phones and tablets: they only watch, so they have no worktrees and are in no view. */
  phones: DeviceView[];
  /** Your devices on your tailnet running Codealong that aren't linked yet. */
  found: FoundDevice[];
  /** Devices asking to link, waiting for your answer here. */
  asks: DeviceAsk[];
  /** Every linked device's worktrees; each lane id and session key starts `@<short>:`. */
  lanes: Lane[];
}

export interface DevicesMe {
  key: string;
  short: string;
  name: string;
  tailscale: 'off' | 'missing' | 'stopped' | 'running';
  tailnet: string | null;
  address: string | null;
  /** "off", "starting", "online", "offline", or why it didn't start. */
  iroh: string;
  remoteControl: boolean;
  error: string | null;
}

export interface DeviceView {
  key: string;
  short: string;
  name: string;
  /** Its badge's color: the one you gave it, or the one its key picks. */
  color: LabelColor;
  online: boolean;
  via: DeviceVia | null;
  since: number | null;
  lastSeen: number | null;
  remoteControl: boolean;
  app: string;
  /** It has this device linked too. */
  linkedBack: boolean;
  loading: boolean;
  usage: Usage[];
  kind: 'mac' | 'phone' | 'tablet';
}

export interface FoundDevice {
  key: string;
  short: string;
  name: string;
  /** The six digits both devices show while linking. */
  code: string;
  /** You asked it to link; it's waiting for an answer there. */
  asking: boolean;
}

export interface DeviceAsk {
  key: string;
  short: string;
  name: string;
  code: string;
}

/** A linked device's answer to a search of what was said. */
export interface DeviceFound {
  device: string;
  name: string;
  found: TextFound | null;
  error: string | null;
}

/** Where a terminal Codealong opens goes: its own window, a tab in the front window, or a tab
 *  beside the same worktree, repo or project. */
export type NewTerminal = 'window' | 'tab' | 'project';

/** A terminal or editor profile `[apps]` can name. */
export interface Profile {
  id: string;
  name: string;
}

/** An installed editor's app icon (editors.rs): `size` pixels square, straight RGBA, top row
 *  first. */
export interface EditorIcon {
  id: string;
  size: number;
  rgba: number[];
}

/** What Settings works from: the settings as parsed, the file as written, and its error. */
export interface ConfigView {
  config: Config;
  text: string;
  path: string;
  /** Why the file can't be parsed; then `config` is the defaults and the form is read-only. */
  error: string | null;
  terminals: Profile[];
  editors: Profile[];
  /** Chrome's profiles, in its profile picker's order; empty when Chrome isn't set up here. */
  chromeProfiles: ChromeProfile[];
}

/** A Chrome profile (src-tauri/src/chrome.rs). */
export interface ChromeProfile {
  /** What config.toml names it by: its name, or its folder when another profile has that name. */
  id: string;
  name: string;
  /** Its folder in Chrome's data folder: `Default`, `Profile 12`. */
  dir: string;
  /** The Google account signed in to it. */
  email: string | null;
}

export interface AccountInfo extends AccountCfg {
  folderExists: boolean;
  signedIn: boolean;
  email: string | null;
}

/** An account on this Mac that config.toml doesn't list (detect.rs). */
export interface FoundAccount extends AccountCfg {
  /** The file the command comes from, when it's your own wrapper or alias. */
  via: string | null;
  /** active: sessions in the last 30 days; signedIn: signed in but quiet; idle: installed or set
   * up in your shell, neither signed in nor used lately. */
  state: 'active' | 'signedIn' | 'idle';
  signedIn: boolean;
  folderExists: boolean;
  email: string | null;
  lastUsed: number | null;
  /** Codealong has never read this folder, and nobody took it out or dismissed it. */
  new: boolean;
}

/** The `codealong` command link and the agent skill per account (install.rs). */
export interface AgentsStatus {
  command: { link: string | null; target: string; onPath: boolean };
  /** Whether the skill folder ships with this build at all. */
  skillFound: boolean;
  /** The Claude Code plugin's version in this build, when it ships with it. */
  pluginVersion: string | null;
  accounts: AccountSkill[];
  /** One line per failure of the last install; empty when only read. */
  errors: string[];
}

export interface AccountSkill {
  name: string;
  harness: Harness;
  /** The skill's folder for this account; null when the harness has none Codealong knows. */
  path: string | null;
  installed: boolean;
  current: boolean;
  /** Someone changed this copy, so updates leave it alone until it's replaced from Settings. */
  edited: boolean;
  /** For a Claude Code account, the plugin, which is what Codealong installs there; the skill
   *  fields then say whether a copy Codealong made before is still in its skills folder. */
  plugin: PluginState | null;
}

/** The Codealong plugin in a Claude Code account. */
export interface PluginState {
  /** The version installed; null when it isn't. */
  version: string | null;
  /** Installed but not turned on in Claude Code: its hooks and skill don't run. */
  disabled: boolean;
  /** Where its marketplace is when it isn't Codealong's own (a GitHub repo, say). */
  from: string | null;
}

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
}

export interface Board {
  generatedAt: number;
  /** App-wide status from the engine; view and project filters do not change the logo. */
  logoStatus: LogoStatus;
  /** `editors`: the editors installed here, in menu order (editors.ts). */
  apps: { terminal: string; terminalId: string; editor: string; editorId: string; editors?: Profile[]; newTerminal?: NewTerminal };
  projects: ProjectSummary[];
  lanes: Lane[];
  /** What isn't working in the background (notices.ts). */
  notices: Notice[];
  loading: boolean;
  githubAt: number | null;
  usage: Usage[];
  /** Pinned session keys, in order. */
  pinned: string[];
  /** Pinned worktrees (lane ids), in order. */
  pinnedWorktrees: string[];
  /** Pinned PRs (`owner/name#number`), in order. */
  pinnedPrs?: string[];
  /** Every PR in the repos the projects cover, newest activity first. */
  prs: PrItem[];
  /** Who `gh` is signed in as. */
  githubLogin: string | null;
  /** True while `[debug] perf_log` is on: the window reports its timings to the perf log. */
  perfLog?: boolean;
  /** From config.toml. */
  theme: Theme;
  /** Every todo, in your order (src-tauri/src/todos.rs). */
  todos: Todo[];
  /** Todo labels, in your order. */
  labels: Label[];
  /** The quick panel's shortcut from config.toml, as it writes it; empty when it's off. Missing on older boards. */
  quickShortcut?: string;
  /** What Codealong asks Jev, from `[jev]` in config.toml. Missing on older boards. */
  jev?: JevCfg;
  /** Open todos whose work Jev says looks done, by todo id. Missing on older boards. */
  todoHints?: Record<string, TodoHint>;
  /** `[devices]` from config.toml. Missing on older boards. */
  devices?: DevicesCfg;
}

/** Why a todo looks done: how sure Jev is, and what showed it. */
export interface TodoHint {
  p: number;
  why: string;
}

/** What Jev suggests for a todo, each with how likely it is, the likeliest first. */
export interface TodoSuggested {
  projects: [string, number][];
  labels: [string, number][];
  dependsOn: [string, number][];
}

/** `[jev]` in config.toml: the model, and each feature's switch. */
export interface JevCfg {
  model: string;
  quick_panel: boolean;
  sessions: boolean;
  todos: boolean;
  pull_requests: boolean;
}

/** Jev's read of a session (insights.rs). Each part is there only when it was asked. */
export interface SessionInsight {
  asksYou?: number;
  landing?: 'landed' | 'pushed' | 'committed' | 'uncommitted' | 'blocked' | 'none';
  finished?: number;
  /** 0 reads only … 3 destructive. */
  risk?: number;
  waitsForInput?: number;
  background?: 'polling' | 'job' | 'server' | 'stalled';
  backgroundP?: number;
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
  repos: RoutePick[];
  /** How likely new work wants a worktree of its own: Jev's answer weighed by your habit. */
  ownWorktree: number;
  /** Jev's answer alone. */
  ownWorktreeAsked: number;
  /** Your latest starts in the likeliest repo: how many went into a new worktree, of how many. */
  habit: { new: number; of: number };
  /** A live session that may already be doing this work. */
  duplicate: RoutePick | null;
  model: string;
  ms: number;
  inputTokens: number;
}

/** What sending to a session did (send.rs). */
export interface Sent {
  message: string;
  /** The session or its app was opened instead, with the prompt filled in or copied. */
  opened: boolean;
  /** It couldn't be typed into the session's terminal, so it went another way, as `message` says.
   *  Missing from other devices that don't know it yet. */
  fellBack?: boolean;
}

export type TodoStatus = 'todo' | 'in-progress' | 'done';

/** A kept attachment (src-tauri/src/attachments.rs): an image shown in place, or any other file. */
export interface Attachment {
  kind: 'image' | 'file';
  /** How text refers to it: `images/<hash>.<ext>` or `files/<hash>/<name>`. */
  path: string;
  /** The file's own name. */
  name: string;
}

/** The label colors; each is a `--label-<color>` token. */
export const LABEL_COLORS = ['gray', 'red', 'pink', 'purple', 'indigo', 'blue', 'teal', 'green'] as const;
export type LabelColor = (typeof LABEL_COLORS)[number];

export interface Label {
  id: string;
  name: string;
  color: LabelColor;
}

/** A start in the Claude or Codex app, waiting for its session to show up. */
export interface PendingStart {
  lane: string;
  harness: Harness;
  account: string;
  at: number;
}

/** A todo marked as waiting, on something outside the list. */
export interface Waiting {
  /** What it waits for, in a few words; can be empty. */
  note: string;
  /** When it was marked; 0 when written by hand without a time. */
  since: number;
}

/** Work you mean to hand an agent. */
export interface Todo {
  id: string;
  /** Empty until you pick a project or link a worktree. */
  project: string;
  title: string;
  /** Plain text; images are `![…](images/<name>)`. */
  description: string;
  status: TodoStatus;
  /** Label ids. */
  labels: string[];
  /** Linked worktrees (lane ids). */
  worktrees: string[];
  /** Linked session keys. */
  sessions: string[];
  createdAt: number;
  updatedAt: number;
  doneAt: number | null;
  pending: PendingStart[];
  /** Ids of the todos this one waits on, in the order added. Missing on older boards. An id
   *  whose todo was deleted stays (so the delete can be undone) and is ignored. */
  dependsOn: string[];
  /** Marked as waiting, on something outside the list. Missing or null when it isn't. */
  waiting?: Waiting | null;
  /** The day it's due, as `2026-10-03` (src/due.ts). Missing or null when it has none. */
  due?: string | null;
}

/** Light, dark, or following macOS. */
export type Theme = 'system' | 'light' | 'dark';

/** A PR in the PRs view, with the worktrees and sessions that worked on it. */
export interface PrItem {
  /** `owner/name#number`. */
  key: string;
  repoSlug: string;
  repo: string;
  project: string;
  number: number;
  title: string;
  url: string;
  state: PrState;
  author: string | null;
  mine: boolean;
  reviewRequested: boolean;
  /** Who asked for your review, when GitHub says; often not the author. */
  requestedBy?: string | null;
  head: string;
  base: string | null;
  checks: Checks;
  merge: Merge;
  additions: number;
  deletions: number;
  changedFiles: number;
  createdAt: number;
  updatedAt: number;
  mergedAt: number | null;
  closedAt: number | null;
  /** Worktrees (lane ids) that hold the PR: the ones the Worktrees view shows it on. */
  lanes: string[];
  /** The sessions that worked on the PR while it was open, each with the worktree it's listed under. */
  sessions: PrSession[];
}

/** A session that worked on a PR. */
export interface PrSession {
  key: string;
  /** The lane it's listed under: its own, or the worktree it made or worked in. */
  lane: string;
}

export interface PrReview {
  author: string;
  state: 'APPROVED' | 'CHANGES_REQUESTED' | 'COMMENTED' | 'DISMISSED' | string;
  at: number | null;
}

export interface PrFile {
  path: string;
  status: string;
  additions: number;
  deletions: number;
  patch: string | null;
}

export interface PrDetail {
  /** The description as GitHub renders it. */
  bodyHtml: string;
  mergeable: string | null;
  commits: number;
  reviews: PrReview[];
  requested: string[];
  /** Comments with something written in them, newest first. Missing from older engines. */
  comments?: PrComment[];
  files: PrFile[];
  moreFiles: boolean;
}

/** A comment on a PR: on the conversation, a review's text, or a thread's first comment. */
export interface PrComment {
  id: string;
  author: string;
  body: string;
  at: number | null;
  url: string | null;
  /** The file a thread on the code is about. */
  path: string | null;
  resolved: boolean;
  /** Written by you. */
  mine: boolean;
}

/** Where a new session runs: the terminal, or the Claude or Codex app. */
export type StartPlace = 'terminal' | 'background' | 'claude-app' | 'codex-app' | 'codealong';

/** An account as the new-session sheet offers it (src-tauri/src/start.rs). */
export interface StartAccount {
  name: string;
  harness: Harness;
  /** Where this account can start a session, the usual place first: the one `start` names, when set. */
  places: { place: StartPlace; label: string }[];
  /** The place its `start` setting names; starts go there rather than where the last one went. */
  chosen: StartPlace | null;
  /** Why there's nowhere, when `places` is empty. */
  unavailable: string | null;
  /** Its `preferred` setting: Auto starts it over accounts without one while it has room. */
  preferred: boolean;
  /** The model a session starts on without one chosen: the account's settings.json `model`. */
  defaultModel: string | null;
  /** The models a start on its command line can ask for, as its harness takes them; none when
   *  it takes none. Missing from devices that list only Claude Code's (`claudeModels`). */
  models?: { id: string; label: string }[];
  /** The effort levels a start on its command line can ask for, lowest first. */
  efforts?: { id: string; label: string }[];
  /** Its command line makes a new worktree for a session when asked. */
  worktree?: boolean;
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
}

/** Where a session came from: a fork of another session (its conversation copied as far as it
 *  went), or started by another session's agent (`codealong start`, Codex's `create_thread`). */
export interface SessionOrigin {
  key: string;
  how: 'fork' | 'started';
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
}

/** The quick panel's shortcut, and why it isn't working when it isn't (src-tauri/src/quick.rs). */
export interface ShortcutStatus {
  /** As config.toml writes it; empty when it's off. */
  shortcut: string;
  error: string | null;
}

export interface Started {
  /** The new session's key when Codealong chose its ID, so the board can select it. */
  key: string | null;
  /** What to do next, when the app is waiting on you. */
  message: string | null;
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
  /** What the mark's colors say, as on the board. */
  logoStatus: LogoStatus;
  perfLog: boolean;
  /** The terminal app's name, for menu items such as "Show in Ghostty". */
  terminal: string;
  /** The projects, in your order, for the bar's project menu. */
  projects: string[];
  /** In bar order: by group, then Needs you and Working the longest first, Your turn the newest
   *  status first. */
  items: BarItem[];
  /** The other sessions started in the last day, the newest status first. */
  recent: BarRecent[];
  /** Sessions you snoozed, out of Needs you and Your turn until their time comes (running ones
   *  stay in Working), the soonest back first. */
  snoozed: BarSnoozed[];
  /** Open PRs that ask for your review, newest first. */
  reviews: BarReview[];
  /** Todos ready to start (not started, and waiting on nothing), in your order. */
  todos: BarTodo[];
}

export interface BarItem {
  key: string;
  group: BarGroup;
  status: Status;
  /** Needs you only because its last reply asks you something: marking it seen settles it.
   *  Missing from a feed an older app sent. */
  asks?: boolean;
  title: string;
  account: string;
  laneId: string;
  callsign: string;
  project: string;
  /** In a worktree the board keeps out of sight (Elsewhere's, while nothing there needs you). */
  hidden: boolean;
  detail: string | null;
  /** When its status began, for its age. */
  since: number;
  openTarget: string;
  /** The lane's own entry for `openTarget`, when it lists one. */
  target: { label: string; detail: string | null; disabled: boolean } | null;
  /** When it's back, while it's snoozed. */
  snoozedUntil: number | null;
  /** Its turn is running and Codealong can stop it. Missing from a feed an older app sent. */
  stoppable?: boolean;
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
  project: string;
  /** Who asked for your review, when GitHub says. */
  requestedBy: string | null;
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
  /** The PR, for a PR's card. Missing on feeds from before PRs could be pinned. */
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
  /** The tallest the peek's window can be where it opens. */
  room: number;
  /** Under the floating bar; otherwise over it. */
  below: boolean;
  /** It takes the keyboard as it shows, rather than on a click. */
  focus: boolean;
  /** Its name is ready to change (Rename in a pill's menu). */
  rename: boolean;
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

/** A step of a session's conversation, from its own record (transcript.rs). */
export interface TranscriptItem {
  at: number;
  /** `result` is a Claude Code tool's output, matched to its call by `id`. */
  kind: 'you' | 'agent' | 'tool' | 'result' | 'note';
  text: string;
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
  /** The questions a call asks you (Claude Code's AskUserQuestion), shown in place of its input. */
  questions?: Question[];
  /** Your answers to them, on the call's result: each question's text to what you picked or
   *  wrote, several where it took more. */
  answers?: Record<string, string[]>;
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
  /** What allowing it from now on adds, in a sentence, when Codealong can say. */
  alwaysRule?: string;
  /** Questions it asks you: answered with your picks rather than allowed. */
  questions?: Question[];
}

/** A question an agent asks you, with choices to pick from. */
export interface Question {
  /** Its text, which its answer is keyed by. */
  question: string;
  header?: string;
  options: { label: string; description?: string }[];
  /** More than one choice can be picked. */
  multiSelect?: boolean;
}

/** Answers to questions, each by its question's text: a choice's label or your own words, or
 *  several where the question takes more. */
export type Answers = Record<string, string | string[]>;

/** A terminal that Close idle terminals would close, named by its session (idle.rs). */
export interface IdleTerminal {
  key: string;
  title: string;
  account: string;
  harness: Harness;
  pid: number;
  cwd: string;
  worktree: string;
  callsign: string;
  lastActivity: number;
  /** Why it can close, in words. */
  why: string;
}

/** A live terminal session that stays open, and why. */
export interface KeptTerminal {
  key: string;
  title: string;
  account: string;
  why: string;
}

export interface IdlePlan {
  afterMs: number;
  close: IdleTerminal[];
  kept: KeptTerminal[];
}

/** What closing one terminal did. */
export interface CloseOutcome {
  key: string;
  title: string;
  closed: boolean;
  message: string;
}

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
 *  its conversation while the session carries on, unaware. With no turns there is none. */
export interface Side {
  /** Which of the session's side conversations: its record's id (`SideKept`). */
  id: string;
  /** The session it's about. */
  key: string;
  turns: SideTurn[];
  /** The copy is answering. */
  running: boolean;
  /** The copy is gone: what was said can still be read, and the next question goes to a new copy,
   *  which knows none of it. */
  ended: boolean;
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
}

/** The reply a session Codealong runs is writing (acp.rs). */
export interface HostedStream {
  key: string;
  /** The reply so far since its last tool call; empty when there's none. */
  text: string;
  /** The tool it's running, as a line. */
  tool: string | null;
}

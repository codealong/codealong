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
  /** Busy but silent for a long while: stuck mid-turn. When it went quiet. */
  stalledSince: number | null;
  /** Jev's read of how its turn ended, when `[jev] sessions` is on (insights.rs). */
  insight?: SessionInsight;
  resumeCommand: string | null;
  /** The Claude app is signed into this session's account. */
  appAccount: boolean;
  /** Opens this session where it lives: Ghostty, the Claude app, Codex, or Antigravity. */
  openTarget: string;
  /** Why a session from another lane shows here: "made this worktree" or "worked here". */
  visitor: string | null;
  /** This session's own sentence; backtick spans and time tokens as in lane headlines. */
  headline: string;
  detail: string | null;
}

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
}

export interface Target {
  id: string;
  icon: string;
  label: string;
  detail: string | null;
  kbd: string[] | null;
  disabled: boolean;
  group: 'session' | 'lane' | 'pr';
}

export interface Lane {
  id: string;
  project: string;
  repo: string;
  repoSlug: string | null;
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
}

/** config.toml, as the engine parses it (src-tauri/src/config.rs). Keys are the file's own. */
export interface Config {
  scan: string[];
  apps: { terminal: string; editor: string; new_terminal?: NewTerminal; move_pointer?: boolean };
  appearance: { theme: Theme };
  /** Shortcuts that work in any app, as config.toml writes them (`Option+Shift+Space`); empty is off. */
  shortcuts: { quick_panel: string };
  /** What Codealong asks TypeSafe's Jev, feature by feature. Older files have none: all off. */
  jev?: JevCfg;
  accounts: AccountCfg[];
  projects: ProjectCfg[];
  /** How many days of sessions have what was said in them searched; 0 is off. */
  search?: { days: number };
  debug: { perf_log: boolean };
}

/** Where a terminal Codealong opens goes: its own window, a tab in the front window, or a tab
 *  beside the same worktree, repo or project. */
export type NewTerminal = 'window' | 'tab' | 'project';

/** A terminal or editor profile `[apps]` can name. */
export interface Profile {
  id: string;
  name: string;
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
}

export interface Board {
  generatedAt: number;
  /** App-wide status from the engine; view and project filters do not change the logo. */
  logoStatus: LogoStatus;
  apps: { terminal: string; terminalId: string; editor: string; editorId: string; newTerminal?: NewTerminal };
  projects: ProjectSummary[];
  lanes: Lane[];
  warnings: string[];
  loading: boolean;
  githubAt: number | null;
  usage: Usage[];
  /** Pinned session keys, in order. */
  pinned: string[];
  /** Pinned worktrees (lane ids), in order. */
  pinnedWorktrees: string[];
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
  background?: 'polling' | 'job' | 'stalled';
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
  ownWorktree: number;
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
export type StartPlace = 'terminal' | 'background' | 'claude-app' | 'codex-app';

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
}

export interface StartOptions {
  /** In config order. */
  accounts: StartAccount[];
  /** Folders the Codex app has as projects. A thread started anywhere else adds one. */
  codexProjects: string[];
  /** Models a Claude Code start in the terminal can ask for, as `--model` takes them. */
  claudeModels: { id: string; label: string }[];
}

export interface StartSpec {
  laneId: string;
  account: string;
  place: StartPlace;
  prompt: string;
  /** `--model` for a terminal start; null starts on the account's default. */
  model: string | null;
  /** A terminal start in a new worktree Claude Code makes (`--worktree`). */
  worktree: boolean;
  /** The todos the session is for, one or several on one prompt: each is linked once it starts. */
  todos: string[];
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
  theme: Theme;
  /** What the mark's colors say, as on the board. */
  logoStatus: LogoStatus;
  perfLog: boolean;
  /** The terminal app's name, for menu items such as "Show in Ghostty". */
  terminal: string;
  /** The projects, in your order, for the bar's project menu. */
  projects: string[];
  /** In bar order: by group, then the newest status first. */
  items: BarItem[];
  /** Open PRs that ask for your review, newest first. */
  reviews: BarReview[];
  /** Todos ready to start (not started, and waiting on nothing), in your order. */
  todos: BarTodo[];
}

export interface BarItem {
  key: string;
  group: BarGroup;
  status: Status;
  title: string;
  account: string;
  laneId: string;
  callsign: string;
  project: string;
  /** In a worktree the board keeps out of sight (Elsewhere's, while nothing there needs you). */
  hidden: boolean;
  detail: string | null;
  openTarget: string;
  /** The lane's own entry for `openTarget`, when it lists one. */
  target: { label: string; detail: string | null; disabled: boolean } | null;
}

export interface BarReview {
  key: string;
  number: number;
  title: string;
  repo: string;
  project: string;
}

export interface BarTodo {
  id: string;
  title: string;
  /** Empty until you pick one or link a worktree. */
  project: string;
  /** The worktree a session for it starts in, when it has one. */
  callsign: string | null;
}

/** A session whose prompts and replies have every word typed (search.rs). */
export interface TextHit {
  key: string;
  /** A line of the message that matched best, around the first match. */
  snippet: string;
  /** You wrote it; otherwise the agent did. */
  you: boolean;
  at: number;
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
}

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

// The board as the Rust engine serializes it (src-tauri/src/model.rs). Option<T> arrives as null.

export type Harness = 'claude' | 'codex' | 'opencode' | 'antigravity';
export type Surface = 'cli' | 'app';
export type Status = 'needs' | 'error' | 'working' | 'unread' | 'idle' | 'stopped';
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
  apps: { terminal: string; editor: string };
  appearance: { theme: Theme };
  accounts: AccountCfg[];
  projects: ProjectCfg[];
  debug: { perf_log: boolean };
}

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

export interface Board {
  generatedAt: number;
  apps: { terminal: string; terminalId: string; editor: string; editorId: string };
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
}

export type TodoStatus = 'todo' | 'in-progress' | 'done';

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
  files: PrFile[];
  moreFiles: boolean;
}

/** Where a new session runs: the terminal, or the Claude or Codex app. */
export type StartPlace = 'terminal' | 'claude-app' | 'codex-app';

/** An account as the new-session sheet offers it (src-tauri/src/start.rs). */
export interface StartAccount {
  name: string;
  harness: Harness;
  /** Where this account can start a session, the usual place first. */
  places: { place: StartPlace; label: string }[];
  /** Why there's nowhere, when `places` is empty. */
  unavailable: string | null;
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
  /** The todo the session is for: it's linked once it starts. */
  todo: string | null;
}

export interface Started {
  /** The new session's key when Codealong chose its ID, so the board can select it. */
  key: string | null;
  /** What to do next, when the app is waiting on you. */
  message: string | null;
}

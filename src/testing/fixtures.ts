// What the tests build their boards from: one builder a type, each returning a whole value of
// its type from types.ts with nothing cast. A test says only what it's about
// (`session({ status: 'needs' })`) and the rest comes from here, so a field added to Session or
// Lane fails to compile in this file and nowhere else in the tests, where a partial object cast
// to its type would have gone on compiling without it.
//
// Everything here is made up: no real project, repo, account or person is named, and a test
// shouldn't name one either.
//
// Only tests import this file. Nothing the app's pages load reaches it, so it isn't in the
// bundle.

import type {
  BarFeed,
  BarItem,
  BarReview,
  Board,
  Checks,
  DevicesView,
  DeviceView,
  GitInfo,
  Lane,
  Meter,
  Pr,
  PrItem,
  Session,
  StartAccount,
  Target,
  Todo,
  Usage,
} from '../types';

/** The time the builders' own times are told from: a session's last activity is a second before
 *  it, a lane's is it. A test that goes by time imports it, or sets its own times. */
export const NOW = 1_790_000_000_000;

/** A session that isn't running, idle, in a terminal. Its key is `harness:account:id`, and its
 *  id, title and the target that opens it follow from the key. */
export function session(over: Partial<Session> = {}): Session {
  const key = over.key ?? 'claude:claude:s1';
  const id = key.split(':')[2] ?? key;
  return {
    key,
    id,
    shortId: id.slice(0, 8),
    harness: 'claude',
    account: 'claude',
    surface: 'cli',
    status: 'idle',
    live: false,
    pid: null,
    title: `Session ${key}`,
    cwd: '/r/app',
    name: null,
    startedAt: NOW - 1000,
    statusSince: null,
    lastActivity: NOW - 1000,
    lastPromptAt: NOW - 1000,
    excerpt: null,
    branch: null,
    archived: false,
    attachCommand: null,
    stalledSince: null,
    resumeCommand: null,
    appAccount: false,
    openTarget: `s|${key}|focus`,
    visitor: null,
    headline: '',
    detail: null,
    ...over,
  };
}

/** A worktree a script made, on `main`, with no sessions, PR or git state. Its path is its id
 *  and its callsign the id's last part. */
export function lane(over: Partial<Lane> = {}): Lane {
  const id = over.id ?? '/r/app';
  return {
    id,
    project: 'Acme',
    repo: 'app',
    repoSlug: 'acme/app',
    path: id,
    callsign: id.split('/').pop() || id,
    origin: 'script',
    branch: 'main',
    detached: null,
    bay: 'idle',
    glyph: 'idle',
    parked: false,
    hidden: false,
    headline: '',
    detail: null,
    sessions: [],
    pr: null,
    git: null,
    lastActivity: NOW,
    targets: [],
    defaultTarget: null,
    ...over,
  };
}

/** A way to open something. Its group follows from its id: `s|…` a session's, else a lane's. */
export function target(over: Partial<Target> = {}): Target {
  const id = over.id ?? 'l|terminal';
  return {
    id,
    icon: 'terminal',
    label: id,
    detail: null,
    kbd: null,
    disabled: false,
    group: id.startsWith('s|') ? 'session' : 'lane',
    ...over,
  };
}

/** No checks at all. */
export function checks(over: Partial<Checks> = {}): Checks {
  return { passed: 0, failed: 0, pending: 0, skipped: 0, failing: [], ...over };
}

/** A worktree's PR: open, with no checks, and GitHub yet to say whether it merges. */
export function pr(over: Partial<Pr> = {}): Pr {
  const number = over.number ?? 7;
  return {
    number,
    title: 'Fix it',
    url: `https://github.com/acme/app/pull/${number}`,
    state: 'open',
    checks: checks(),
    merge: 'unknown',
    updatedAt: NOW,
    mergedAt: null,
    closedAt: null,
    ...over,
  };
}

/** A PR as the PRs view lists it: someone else's, open, held by no worktree. Its key and URL
 *  follow from its repo and number. */
export function prItem(over: Partial<PrItem> = {}): PrItem {
  const repoSlug = over.repoSlug ?? 'acme/app';
  const number = over.number ?? 7;
  return {
    key: `${repoSlug}#${number}`,
    repoSlug,
    repo: repoSlug.split('/').pop() || repoSlug,
    project: 'Acme',
    number,
    title: 'Fix it',
    url: `https://github.com/${repoSlug}/pull/${number}`,
    state: 'open',
    author: 'someone',
    mine: false,
    reviewRequested: false,
    head: `branch-${number}`,
    base: 'main',
    checks: checks(),
    merge: 'unknown',
    additions: 0,
    deletions: 0,
    changedFiles: 0,
    createdAt: NOW,
    updatedAt: NOW,
    mergedAt: null,
    closedAt: null,
    lanes: [],
    sessions: [],
    ...over,
  };
}

/** A clean worktree with no upstream and no commits git told of. */
export function git(over: Partial<GitInfo> = {}): GitInfo {
  return { upstream: null, ahead: 0, behind: 0, dirty: 0, lastCommit: null, lastCommitAt: null, added: null, removed: null, ...over };
}

/** A todo not yet started, in no project, linked to nothing. */
export function todo(over: Partial<Todo> = {}): Todo {
  const id = over.id ?? 't1';
  return {
    id,
    project: '',
    title: `Todo ${id}`,
    description: '',
    status: 'todo',
    labels: [],
    worktrees: [],
    sessions: [],
    createdAt: NOW,
    updatedAt: NOW,
    doneAt: null,
    pending: [],
    dependsOn: [],
    waiting: null,
    review: null,
    due: null,
    ...over,
  };
}

/** A board with nothing on it, once the engine's first scan is done. */
export function board(over: Partial<Board> = {}): Board {
  return {
    generatedAt: NOW,
    logoStatus: 'idle',
    apps: { terminal: 'Ghostty', terminalId: 'ghostty', editor: 'VS Code', editorId: 'vscode' },
    projects: [],
    lanes: [],
    notices: [],
    loading: false,
    githubAt: null,
    githubPolled: false,
    usage: [],
    pinned: [],
    pinnedWorktrees: [],
    prs: [],
    githubLogin: null,
    theme: 'system',
    todos: [],
    labels: [],
    ...over,
  };
}

/** One plan limit, unused. Its label and window are its key. */
export function meter(over: Partial<Meter> = {}): Meter {
  const key = over.key ?? 'session';
  return { key, label: key, window: key, percent: 0, resetsAt: null, severity: 'normal', model: null, ...over };
}

/** An account's plan usage, read just now, with no meters. */
export function usage(over: Partial<Usage> = {}): Usage {
  return { account: 'claude', harness: 'claude', plan: null, meters: [], asOf: NOW, error: null, needsSignIn: false, loading: false, ...over };
}

/** An account as the new-session sheet offers it, with nowhere to start yet. */
export function startAccount(over: Partial<StartAccount> = {}): StartAccount {
  return { name: 'claude', harness: 'claude', places: [], chosen: null, unavailable: null, defaultModel: null, ...over };
}

/** A session's pill on the floating bar, in Needs you. Its status is its group, and its title,
 *  lane and callsign its key. */
export function barItem(over: Partial<BarItem> = {}): BarItem {
  const key = over.key ?? 'claude:claude:s1';
  const group = over.group ?? 'needs';
  return {
    key,
    group,
    status: group,
    title: key,
    account: 'claude',
    laneId: key,
    callsign: key,
    project: 'Acme',
    hidden: false,
    detail: null,
    since: NOW,
    openTarget: `s|${key}|focus`,
    target: null,
    snoozedUntil: null,
    ...over,
  };
}

/** A PR that asks for your review, on the floating bar. */
export function barReview(over: Partial<BarReview> = {}): BarReview {
  return { key: 'acme/app#7', number: 7, title: 'Fix it', repo: 'app', repoSlug: 'acme/app', url: 'https://github.com/acme/app/pull/7', project: 'Acme', requestedBy: null, since: NOW, updatedAt: NOW, ...over };
}

/** What the floating bar shows with nothing on it. */
export function barFeed(over: Partial<BarFeed> = {}): BarFeed {
  return { loading: false, theme: 'system', logoStatus: 'idle', perfLog: false, terminal: 'Ghostty', projects: [], items: [], recent: [], snoozed: [], reviews: [], todos: [], ...over };
}

/** A linked Mac that isn't connected. Its key is its short name. */
export function deviceView(over: Partial<DeviceView> = {}): DeviceView {
  const short = over.short ?? 'a1b2c3d4e5';
  return {
    key: short,
    short,
    name: 'Mac mini',
    color: 'blue',
    online: false,
    via: null,
    since: null,
    lastSeen: null,
    remoteControl: false,
    app: '0.1.0',
    linkedBack: true,
    loading: false,
    usage: [],
    kind: 'mac',
    ...over,
  };
}

/** Your other devices turned on, with none linked, found or asking. */
export function devicesView(over: Partial<DevicesView> = {}): DevicesView {
  return {
    enabled: true,
    me: { key: 'f0e1d2c3b4', short: 'f0e1d2c3b4', name: 'MacBook Pro', tailscale: 'off', tailnet: null, address: null, iroh: 'off', remoteControl: false, error: null },
    devices: [],
    phones: [],
    found: [],
    asks: [],
    lanes: [],
    ...over,
  };
}

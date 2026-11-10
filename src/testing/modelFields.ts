// Written by src-tauri/src/wiretypes.rs, whose test keeps it current
// (`CODEALONG_WRITE_FIXTURES=1 cargo test wiretypes` writes it again): the names the engine
// writes each struct of its model by, held against the interface of types.ts that mirrors it.
// `npm run typecheck` fails on the line of one that names other fields than the engine sends,
// saying which only one side has: name the field on the side that lacks it, or say why the two
// are apart in wiretypes.rs `APART`.
import type * as Types from '../types';

type Same<Engine, Windows> = [Exclude<Engine, Windows>, Exclude<Windows, Engine>] extends [never, never]
  ? true
  : { onlyTheEngineNames: Exclude<Engine, Windows>; onlyTheWindowsName: Exclude<Windows, Engine> };
type Holds<T extends true> = T;
/** A type's names as JSON has them: a number's as text, and those of every member of a union. */
type Names<T> = T extends unknown ? Text<keyof T> : never;
type Text<Key> = Key extends string | number ? `${Key}` : Key;

// The types above, held to what they're for: a union names what any member of it does, a
// number is its text, and a name only one side has doesn't hold.
export type Helpers = [
  Holds<Same<"id" | "more", Names<{ id: string } | { id: string; more: string }>>>,
  Holds<Same<"0", Names<{ 0: string }>>>,
  // @ts-expect-error: `more` is only the windows'.
  Holds<Same<"id", Names<{ id: string; more?: string }>>>,
];
type Has<Theirs, Name> = Name extends Theirs ? true : { theWindowsNoLongerName: Name };

export type Session = Holds<Same<
  | "account"
  | "agentSeen"
  | "appAccount"
  | "archived"
  | "artifacts"
  | "asks"
  | "attachCommand"
  | "background"
  | "branch"
  | "chrome"
  | "connectionLost"
  | "context"
  | "continues"
  | "cutOff"
  | "cwd"
  | "detail"
  | "earlier"
  | "effort"
  | "emblem"
  | "excerpt"
  | "flagged"
  | "harness"
  | "headless"
  | "headline"
  | "helper"
  | "hosted"
  | "id"
  | "insight"
  | "key"
  | "labels"
  | "lastActivity"
  | "lastPromptAt"
  | "limit"
  | "live"
  | "mode"
  | "model"
  | "modes"
  | "name"
  | "need"
  | "openTarget"
  | "origin"
  | "ownWorktree"
  | "pid"
  | "pin"
  | "queued"
  | "queuedAt"
  | "quiet"
  | "raised"
  | "remoteControl"
  | "resumeCommand"
  | "running"
  | "shortId"
  | "sideAnswer"
  | "snoozedUntil"
  | "stalledSince"
  | "startedAt"
  | "status"
  | "statusSince"
  | "steerable"
  | "stoppable"
  | "suggestions"
  | "surface"
  | "terminal"
  | "title"
  | "visitor",
  Names<Types.Session>
>>;

export type Emblem = Holds<Same<
  | "emoji"
  | "name"
  | "picture",
  Names<Types.Emblem>
>>;

export type Artifact = Holds<Same<
  | "at"
  | "note"
  | "title"
  | "url",
  Names<Types.Artifact>
>>;

export type Helper = Holds<Same<
  | "parent"
  | "pr"
  | "report"
  | "yours",
  Names<Types.Helper>
>>;

export type SessionMode = Holds<Same<
  | "detail"
  | "expected"
  | "id"
  | "kind"
  | "label",
  Names<Types.SessionMode>
>>;

export type ContextUse = Holds<Same<
  | "used"
  | "window",
  Names<Types.ContextUse>
>>;

export type QueuedMessage = Holds<Same<
  | "at"
  | "id"
  | "text",
  Names<Types.QueuedMessage>
>>;

export type SessionLimit = Holds<Same<
  | "at"
  | "continues"
  | "meter"
  | "moveTo"
  | "resetsAt"
  | "spendCapped",
  Names<Types.SessionLimit>
>>;

export type ConnectionLost = Holds<Same<
  | "at"
  | "continues",
  Names<Types.ConnectionLost>
>>;

export type Suggestions = Holds<Same<
  | "at"
  | "by"
  | "byTitle"
  | "note"
  | "picks"
  | "promptAt"
  | "replies"
  | "why",
  Names<Types.Suggestions>
>>;

export type SideAnswer = Holds<Same<
  | "at"
  | "id"
  | "open",
  Names<Types.SideAnswer>
>>;

export type Raised = Holds<Same<
  | "agent"
  | "at"
  | "by"
  | "byTitle"
  | "note",
  Names<Types.Raised>
>>;

export type AgentSeen = Holds<Same<
  | "at"
  | "note",
  Names<Types.AgentSeen>
>>;

export type RunningIn = Holds<Same<
  | "command"
  | "terminal",
  Names<Types.RunningIn>
>>;

export type SessionOrigin = Holds<Same<
  | "how"
  | "key",
  Names<Types.SessionOrigin>
>>;

export type ChromeTabs = Holds<Same<
  | "at"
  | "profile"
  | "profileDir"
  | "tabs"
  | "title"
  | "url",
  Names<Types.ChromeTabs>
>>;

export type SessionInsight = Holds<Same<
  | "asksYou"
  | "background"
  | "backgroundP"
  | "by"
  | "byTitle"
  | "confidence"
  | "finished"
  | "landing"
  | "note"
  | "risk"
  | "waitsForInput",
  Names<Types.SessionInsight>
>>;

export type Checks = Holds<Same<
  | "failed"
  | "failing"
  | "passed"
  | "pending"
  | "skipped",
  Names<Types.Checks>
>>;

export type Label = Holds<Same<
  | "color"
  | "id"
  | "name",
  Names<Types.Label>
>>;

export type PrLabel = Holds<Same<
  | "color"
  | "description"
  | "name",
  Names<Types.PrLabel>
>>;

export type Pr = Holds<Same<
  | "checks"
  | "closedAt"
  | "merge"
  | "mergedAt"
  | "number"
  | "staleApproval"
  | "state"
  | "title"
  | "updatedAt"
  | "url",
  Names<Types.Pr>
>>;

export type StaleApproval = Holds<Same<
  | "by"
  | "commit",
  Names<Types.StaleApproval>
>>;

export type GitInfo = Holds<Same<
  | "added"
  | "ahead"
  | "base"
  | "baseAhead"
  | "baseBehind"
  | "behind"
  | "dirty"
  | "lastCommit"
  | "lastCommitAt"
  | "locked"
  | "newWorkCount"
  | "removed"
  | "resetAt"
  | "unpushed"
  | "upstream",
  Names<Types.GitInfo>
>>;

export type Target = Holds<Same<
  | "detail"
  | "disabled"
  | "group"
  | "icon"
  | "id"
  | "kbd"
  | "label",
  Exclude<Names<Types.Target>, "image">
>>;
/** An editor's own icon, which the windows put on its target from the icons they ask for apart (src/editors.ts). */
export type TargetApart = [Holds<Has<Names<Types.Target>, "image">>];

export type Lane = Holds<Same<
  | "bay"
  | "branch"
  | "callsign"
  | "chats"
  | "cleanup"
  | "codealong"
  | "defaultTarget"
  | "detached"
  | "detail"
  | "device"
  | "git"
  | "glyph"
  | "headline"
  | "hidden"
  | "id"
  | "lastActivity"
  | "looksFinished"
  | "looksReusable"
  | "origin"
  | "parked"
  | "path"
  | "pr"
  | "preview"
  | "project"
  | "repo"
  | "repoSlug"
  | "repoSlugNow"
  | "reuse"
  | "sessions"
  | "startsChats"
  | "targets",
  Names<Types.Lane>
>>;

export type Cleanup = Holds<Same<
  | "ok"
  | "why",
  Names<Types.Cleanup>
>>;

export type Preview = Holds<Same<
  | "chromeProfile"
  | "chromeProfileSet"
  | "port"
  | "server"
  | "set"
  | "url",
  Names<Types.Preview>
>>;

export type PreviewServer = Holds<Same<
  | "name"
  | "pid"
  | "port"
  | "session"
  | "startedAt"
  | "title",
  Names<Types.PreviewServer>
>>;

export type DeviceTag = Holds<Same<
  | "color"
  | "lastSeen"
  | "name"
  | "online"
  | "short",
  Names<Types.DeviceTag>
>>;

export type Meter = Holds<Same<
  | "key"
  | "label"
  | "model"
  | "percent"
  | "resetsAt"
  | "severity"
  | "window",
  Names<Types.Meter>
>>;

export type Usage = Holds<Same<
  | "account"
  | "asOf"
  | "error"
  | "harness"
  | "loading"
  | "meters"
  | "needsSignIn"
  | "noLimits"
  | "plan"
  | "preferred"
  | "rateLimited",
  Names<Types.Usage>
>>;

export type ProjectSummary = Holds<Same<
  | "lanes"
  | "name"
  | "repos",
  Names<Types.ProjectSummary>
>>;

export type AccountInfo = Holds<Same<
  | "acp"
  | "command"
  | "disabled"
  | "email"
  | "folderExists"
  | "harness"
  | "home"
  | "missingCli"
  | "monthly"
  | "name"
  | "per_token"
  | "signInNote"
  | "signedIn"
  | "start"
  | "usage_cap",
  Names<Types.AccountInfo>
>>;

export type Notice = Holds<Same<
  | "key"
  | "reviewAgain"
  | "text"
  | "title"
  | "until",
  Names<Types.Notice>
>>;

export type Board = Holds<Same<
  | "apps"
  | "chatShortcut"
  | "devices"
  | "experimental"
  | "generatedAt"
  | "githubAt"
  | "githubCurrent"
  | "githubLogin"
  | "githubPolled"
  | "improve"
  | "improveShortcut"
  | "jev"
  | "labels"
  | "lanes"
  | "loading"
  | "logoStatus"
  | "mainWindowShortcut"
  | "mcp"
  | "notices"
  | "nudgeAbout"
  | "perfLog"
  | "pinned"
  | "pinnedPrs"
  | "pinnedRepos"
  | "pinnedWorktrees"
  | "prStopped"
  | "projects"
  | "prs"
  | "quickShortcut"
  | "repos"
  | "rev"
  | "reviews"
  | "schedules"
  | "schedulesProblem"
  | "sessions"
  | "sides"
  | "teamFiles"
  | "terminals"
  | "theme"
  | "todoHints"
  | "todos"
  | "todosUnreadable"
  | "training"
  | "usage"
  | "waitingShortcut"
  | "whenLimited"
  | "zoom",
  Names<Types.Board>
>>;

export type TodoHint = Holds<Same<
  | "p"
  | "why",
  Names<Types.TodoHint>
>>;

export type PrItem = Holds<Same<
  | "additions"
  | "author"
  | "authoredBy"
  | "autoReview"
  | "base"
  | "changedFiles"
  | "checks"
  | "closedAt"
  | "createdAt"
  | "deletions"
  | "head"
  | "headOid"
  | "key"
  | "labels"
  | "lanes"
  | "lastReview"
  | "merge"
  | "mergedAt"
  | "mine"
  | "nudge"
  | "nudgeByDefault"
  | "number"
  | "primary"
  | "primaryBy"
  | "project"
  | "repo"
  | "repoSlug"
  | "requestedAt"
  | "requestedBy"
  | "reviewRequested"
  | "reviewersAsked"
  | "sessions"
  | "snoozedUntil"
  | "staleApproval"
  | "state"
  | "title"
  | "turn"
  | "updatedAt"
  | "url"
  | "watched",
  Names<Types.PrItem>
>>;

export type LastReview = Holds<Same<
  | "at"
  | "commit"
  | "state",
  Names<Types.LastReview>
>>;

export type AutoReview = Holds<Same<
  | "approves"
  | "at"
  | "behind"
  | "comment"
  | "head"
  | "holds"
  | "machine"
  | "note"
  | "own"
  | "parts"
  | "rounds"
  | "sendable"
  | "sent"
  | "sentAt"
  | "stage"
  | "submitted"
  | "submittedAt"
  | "suggests",
  Names<Types.AutoReview>
>>;

export type ReviewPart = Holds<Same<
  | "note"
  | "session"
  | "state"
  | "who",
  Names<Types.ReviewPart>
>>;

export type PrSession = Holds<Same<
  | "key"
  | "lane"
  | "primary"
  | "pushes",
  Names<Types.PrSession>
>>;

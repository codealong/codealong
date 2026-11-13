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
type Lacks<Theirs, Name> = Name extends Theirs ? { theWindowsNowName: Name } : true;

export type Session = Holds<Same<
  | "account"
  | "agentSeen"
  | "agents"
  | "apiError"
  | "appAccount"
  | "archived"
  | "artifacts"
  | "aside"
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
  | "finished"
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
  | "listening"
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
  | "tasks"
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

export type ApiError = Holds<Same<
  | "at"
  | "said",
  Names<Types.ApiError>
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

export type BackgroundTask = Holds<Same<
  | "about"
  | "command"
  | "endedAt"
  | "exitCode"
  | "id"
  | "monitor"
  | "output"
  | "startedAt"
  | "status",
  Names<Types.BackgroundTask>
>>;

export type RunningIn = Holds<Same<
  | "command"
  | "terminal"
  | "yours",
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
  | "ending"
  | "finished"
  | "landing"
  | "note"
  | "risk"
  | "waitsForInput"
  | "waitsUntil",
  Names<Types.SessionInsight>
>>;

export type Finished = Holds<Same<
  | "sure"
  | "why",
  Names<Types.Finished>
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
  | "chrome_profile"
  | "command"
  | "disabled"
  | "email"
  | "folderExists"
  | "harness"
  | "home"
  | "makesPages"
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
  | "listenShortcut"
  | "listenings"
  | "loading"
  | "logoStatus"
  | "mainWindowShortcut"
  | "mcp"
  | "meeting"
  | "meetingSoon"
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
  | "talkOrchestratorShortcut"
  | "talkShortcut"
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

export type Todo = Holds<Same<
  | "archivedAt"
  | "createdAt"
  | "dependsOn"
  | "description"
  | "doneAt"
  | "due"
  | "id"
  | "labels"
  | "pending"
  | "primary"
  | "project"
  | "review"
  | "sessions"
  | "status"
  | "title"
  | "updatedAt"
  | "waiting"
  | "worktrees",
  Names<Types.Todo>
>>;

export type Schedule = Holds<Same<
  | "catchUp"
  | "checkout"
  | "createdAt"
  | "createdBy"
  | "finishedAt"
  | "folderOnly"
  | "id"
  | "name"
  | "notes"
  | "once"
  | "onlyIf"
  | "overlap"
  | "paused"
  | "perDay"
  | "quiet"
  | "resumedAt"
  | "run"
  | "runsOn"
  | "updatedAt"
  | "when",
  Names<Types.Schedule>
>>;

export type JevReport = Holds<Same<
  | "days"
  | "since",
  Names<Types.JevReport>
>>;

export type JevUsed = Holds<Same<
  | "cost"
  | "requests"
  | "tokens",
  Names<Types.JevUsed>
>>;

export type SessionSpent = Holds<Same<
  | "models"
  | "total",
  Names<Types.SessionSpent>
>>;

export type Spent = Holds<Same<
  | "account"
  | "api"
  | "calls"
  | "folder"
  | "key"
  | "lastAt"
  | "paid"
  | "tokens"
  | "unpriced",
  Names<Types.Spent>
>>;

export type TeamCfg = Holds<Same<
  | "neverSubmit"
  | "nudgeAbout"
  | "ownReviewInstructions"
  | "reviewInstructions"
  | "suggests",
  Names<Types.TeamCfg>
>>;

export type TeamFile = Holds<Same<
  | "branch"
  | "commit"
  | "default"
  | "error"
  | "offers"
  | "repo"
  | "settings"
  | "skipped",
  Names<Types.TeamFile>
>>;

export type Listening = Holds<Same<
  | "done"
  | "endedAt"
  | "failures"
  | "folder"
  | "heardAt"
  | "id"
  | "lines"
  | "locale"
  | "meeting"
  | "prompt"
  | "role"
  | "session"
  | "sources"
  | "start"
  | "startedAt"
  | "told"
  | "toldAt"
  | "whyEnded",
  Names<Types.Listening>
>>;

export type Mcp = Holds<Same<
  | "account"
  | "asOf"
  | "note"
  | "servers"
  | "stale",
  Names<Types.Mcp>
>>;

export type Improve = Holds<Same<
  | "installed"
  | "laneId"
  | "project"
  | "source",
  Names<Types.Improve>
>>;

export type Training = Holds<Same<
  | "account"
  | "asOf"
  | "canStop"
  | "note"
  | "stale"
  | "state"
  | "url",
  Names<Types.Training>
>>;

export type DeviceView = Holds<Same<
  | "app"
  | "automations"
  | "color"
  | "key"
  | "kind"
  | "lastSeen"
  | "linkedBack"
  | "loading"
  | "name"
  | "online"
  | "projects"
  | "remoteControl"
  | "short"
  | "since"
  | "usage"
  | "via",
  Names<Types.DeviceView>
>>;

export type ProjectThere = Holds<Same<
  | "name"
  | "repos",
  Names<Types.ProjectThere>
>>;

export type AccountCfg = Holds<Same<
  | "acp"
  | "chrome_profile"
  | "command"
  | "disabled"
  | "harness"
  | "home"
  | "monthly"
  | "name"
  | "per_token"
  | "start"
  | "usage_cap",
  Names<Types.AccountCfg>
>>;

export type Config = Holds<Same<
  | "accounts"
  | "appearance"
  | "apps"
  | "debug"
  | "devices"
  | "emblems"
  | "experimental"
  | "jev"
  | "limits"
  | "prices"
  | "projects"
  | "pull_requests"
  | "repos"
  | "reviews"
  | "routing"
  | "scan"
  | "search"
  | "sessions"
  | "shortcuts"
  | "sounds"
  | "updates"
  | "voice",
  Names<Types.Config>
>>;

export type DevicesCfg = Holds<Same<
  | "automations"
  | "enabled"
  | "in_bar"
  | "iroh"
  | "name"
  | "port"
  | "remote_control"
  | "tailscale",
  Names<Types.DevicesCfg>
>>;

export type EmblemsCfg = Holds<Same<
  | "account"
  | "kind"
  | "prompt",
  Names<Types.EmblemsCfg>
>>;

export type ExperimentalCfg = Holds<Same<
  | "analytics"
  | "boards"
  | "voice",
  Names<Types.ExperimentalCfg>
>>;

export type GroupMemberCfg = Holds<Same<
  | "account"
  | "effort"
  | "model",
  Names<Types.GroupMemberCfg>
>>;

export type JevCfg = Holds<Same<
  | "model"
  | "prompts"
  | "pull_requests"
  | "quick_panel"
  | "sessions"
  | "todos",
  Names<Types.JevCfg>
>>;

export type OwnReviewsCfg = Holds<Same<
  | "account"
  | "auto"
  | "drafts"
  | "effort"
  | "model"
  | "other_harness"
  | "prompt"
  | "send",
  Names<Types.OwnReviewsCfg>
>>;

export type Profile = Holds<Same<
  | "id"
  | "name",
  Names<Types.Profile>
>>;

export type ProjectCfg = Holds<Same<
  | "chrome_profile"
  | "folders"
  | "name"
  | "repos",
  Names<Types.ProjectCfg>
>>;

export type PullRequestsCfg = Holds<Same<
  | "nudge_about"
  | "nudge_by_default"
  | "rerequest_after_fixes",
  Names<Types.PullRequestsCfg>
>>;

export type RepoCfg = Holds<Same<
  | "pull_requests"
  | "reviews",
  Names<Types.RepoCfg>
>>;

export type ReviewsCfg = Holds<Same<
  | "account"
  | "auto"
  | "drafts"
  | "effort"
  | "model"
  | "other_harness"
  | "own"
  | "post"
  | "prompt"
  | "start"
  | "submit",
  Names<Types.ReviewsCfg>
>>;

export type ReviewsPatch = Holds<Same<
  | "account"
  | "auto"
  | "drafts"
  | "effort"
  | "model"
  | "other_harness"
  | "own"
  | "post"
  | "prompt"
  | "submit",
  Names<Types.ReviewsPatch>
>>;

export type RoutingCfg = Holds<Same<
  | "auto"
  | "buffer"
  | "groups"
  | "prompt",
  Names<Types.RoutingCfg>
>>;

export type RoutingGroupCfg = Holds<Same<
  | "members"
  | "name"
  | "strategy",
  Names<Types.RoutingGroupCfg>
>>;

export type SessionsCfg = Holds<Same<
  | "block_flagged_sessions"
  | "continue_after_network_error"
  | "continue_after_restart"
  | "hibernate_after"
  | "live_replies"
  | "resume_for_any_session"
  | "resume_for_pr_news"
  | "resume_for_sessions_it_started",
  Names<Types.SessionsCfg>
>>;

export type SoundsCfg = Holds<Same<
  | "meeting"
  | "needs_you"
  | "review_requested"
  | "volume"
  | "your_turn",
  Names<Types.SoundsCfg>
>>;

export type VoiceCfg = Holds<Same<
  | "locale"
  | "meeting_account"
  | "meeting_calendars_off"
  | "meeting_countdown"
  | "meeting_lead"
  | "meeting_model"
  | "meetings"
  | "meetings_folder"
  | "send_on_release"
  | "tell_every",
  Names<Types.VoiceCfg>
>>;

export type GroupMember = Holds<Same<
  | "account"
  | "effort"
  | "model",
  Names<Types.GroupMember>
>>;

export type ModeSwitched = Holds<Same<
  | "message"
  | "mode",
  Names<Types.ModeSwitched>
>>;

export type Pending = Holds<Same<
  | "always"
  | "alwaysRule"
  | "cannot"
  | "exiting"
  | "id"
  | "input"
  | "label"
  | "message"
  | "noOnce"
  | "plan"
  | "questions"
  | "terminal"
  | "unwritten",
  Names<Types.Pending>
>>;

export type Question = Holds<Same<
  | "header"
  | "multiSelect"
  | "options"
  | "question",
  Names<Types.Question>
>>;

export type ResumeIn = Holds<Same<
  | "effort"
  | "model"
  | "place",
  Names<Types.ResumeIn>
>>;

export type Route = Holds<Same<
  | "action"
  | "actions"
  | "agents"
  | "duplicate"
  | "habit"
  | "inputTokens"
  | "lanes"
  | "model"
  | "ms"
  | "needs"
  | "noRepo"
  | "noSession"
  | "ownWorktree"
  | "ownWorktreeAsked"
  | "repos"
  | "sessions",
  Names<Types.Route>
>>;

export type RoutingGroup = Holds<Same<
  | "members"
  | "name"
  | "strategy",
  Names<Types.RoutingGroup>
>>;

export type Sent = Holds<Same<
  | "fellBack"
  | "message"
  | "opened"
  | "queued"
  | "uncertain",
  Names<Types.Sent>
>>;

export type Side = Holds<Same<
  | "account"
  | "closed"
  | "effort"
  | "ended"
  | "id"
  | "key"
  | "model"
  | "running"
  | "turns",
  Names<Types.Side>
>>;

export type SideKept = Holds<Same<
  | "answeredAt"
  | "answering"
  | "id"
  | "key"
  | "lastAt"
  | "startedAt"
  | "title"
  | "turns",
  Names<Types.SideKept>
>>;

export type SideTurn = Holds<Same<
  | "answer"
  | "at"
  | "done"
  | "error"
  | "question"
  | "tool"
  | "tools",
  Names<Types.SideTurn>
>>;

export type StartAccount = Holds<Same<
  | "buffer"
  | "chosen"
  | "defaultModel"
  | "efforts"
  | "harness"
  | "models"
  | "name"
  | "places"
  | "resumes"
  | "switchesEffort"
  | "switchesInApp"
  | "switchesMidTurn"
  | "switchesModel"
  | "unavailable"
  | "usageCap"
  | "worktree",
  Names<Types.StartAccount>
>>;
/** Kept for devices of older builds that read it here; no window does. */
export type StartAccountApart = [Holds<Lacks<Names<Types.StartAccount>, "preferred">>];

export type StartOptions = Holds<Same<
  | "accounts"
  | "autoGroup"
  | "claudeEfforts"
  | "claudeModels"
  | "codexProjects"
  | "groups",
  Names<Types.StartOptions>
>>;

export type Started = Holds<Same<
  | "key"
  | "message",
  Names<Types.Started>
>>;

export type Unqueued = Holds<Same<
  | "message"
  | "requeued",
  Names<Types.Unqueued>
>>;

export type CommitText = Holds<Same<
  | "body"
  | "title",
  Names<Types.CommitText>
>>;

export type MergeBox = Holds<Same<
  | "auto"
  | "branch"
  | "branchRef"
  | "canAuto"
  | "canBypass"
  | "canMerge"
  | "canReady"
  | "canStopAuto"
  | "canUpdateBranch"
  | "defaultEmail"
  | "defaultMethod"
  | "deletesBranch"
  | "draft"
  | "emails"
  | "head"
  | "id"
  | "mergeState"
  | "mergeText"
  | "mergeable"
  | "mergedBy"
  | "methods"
  | "queue"
  | "queued"
  | "reviewDecision"
  | "squashText"
  | "state",
  Names<Types.MergeBox>
>>;

export type Turn = Holds<Same<
  | "also"
  | "kind"
  | "since"
  | "who",
  Names<Types.Turn>
>>;

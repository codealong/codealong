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
type Has<Names, Name> = Name extends Names ? true : { theWindowsNoLongerName: Name };

export type Session = Holds<Same<
  | 'account'
  | 'agentSeen'
  | 'appAccount'
  | 'archived'
  | 'artifacts'
  | 'asks'
  | 'attachCommand'
  | 'background'
  | 'branch'
  | 'chrome'
  | 'connectionLost'
  | 'context'
  | 'continues'
  | 'cutOff'
  | 'cwd'
  | 'detail'
  | 'earlier'
  | 'effort'
  | 'emblem'
  | 'excerpt'
  | 'flagged'
  | 'harness'
  | 'headless'
  | 'headline'
  | 'helper'
  | 'hosted'
  | 'id'
  | 'insight'
  | 'key'
  | 'labels'
  | 'lastActivity'
  | 'lastPromptAt'
  | 'limit'
  | 'live'
  | 'mode'
  | 'model'
  | 'modes'
  | 'name'
  | 'need'
  | 'openTarget'
  | 'origin'
  | 'ownWorktree'
  | 'pid'
  | 'pin'
  | 'queued'
  | 'queuedAt'
  | 'quiet'
  | 'raised'
  | 'remoteControl'
  | 'resumeCommand'
  | 'running'
  | 'shortId'
  | 'sideAnswer'
  | 'snoozedUntil'
  | 'stalledSince'
  | 'startedAt'
  | 'status'
  | 'statusSince'
  | 'steerable'
  | 'stoppable'
  | 'suggestions'
  | 'surface'
  | 'terminal'
  | 'title'
  | 'visitor',
  keyof Types.Session
>>;

export type Emblem = Holds<Same<
  | 'emoji'
  | 'name'
  | 'picture',
  keyof Types.Emblem
>>;

export type Artifact = Holds<Same<
  | 'at'
  | 'note'
  | 'title'
  | 'url',
  keyof Types.Artifact
>>;

export type Helper = Holds<Same<
  | 'parent'
  | 'pr'
  | 'report'
  | 'yours',
  keyof Types.Helper
>>;

export type SessionMode = Holds<Same<
  | 'detail'
  | 'expected'
  | 'id'
  | 'kind'
  | 'label',
  keyof Types.SessionMode
>>;

export type ContextUse = Holds<Same<
  | 'used'
  | 'window',
  keyof Types.ContextUse
>>;

export type QueuedMessage = Holds<Same<
  | 'at'
  | 'id'
  | 'text',
  keyof Types.QueuedMessage
>>;

export type SessionLimit = Holds<Same<
  | 'at'
  | 'continues'
  | 'meter'
  | 'moveTo'
  | 'resetsAt'
  | 'spendCapped',
  keyof Types.SessionLimit
>>;

export type ConnectionLost = Holds<Same<
  | 'at'
  | 'continues',
  keyof Types.ConnectionLost
>>;

export type Suggestions = Holds<Same<
  | 'at'
  | 'by'
  | 'byTitle'
  | 'note'
  | 'picks'
  | 'promptAt'
  | 'replies'
  | 'why',
  keyof Types.Suggestions
>>;

export type SideAnswer = Holds<Same<
  | 'at'
  | 'id'
  | 'open',
  keyof Types.SideAnswer
>>;

export type Raised = Holds<Same<
  | 'agent'
  | 'at'
  | 'by'
  | 'byTitle'
  | 'note',
  keyof Types.Raised
>>;

export type AgentSeen = Holds<Same<
  | 'at'
  | 'note',
  keyof Types.AgentSeen
>>;

export type RunningIn = Holds<Same<
  | 'command'
  | 'terminal',
  keyof Types.RunningIn
>>;

export type SessionOrigin = Holds<Same<
  | 'how'
  | 'key',
  keyof Types.SessionOrigin
>>;

export type ChromeTabs = Holds<Same<
  | 'at'
  | 'profile'
  | 'profileDir'
  | 'tabs'
  | 'title'
  | 'url',
  keyof Types.ChromeTabs
>>;

export type SessionInsight = Holds<Same<
  | 'asksYou'
  | 'background'
  | 'backgroundP'
  | 'by'
  | 'byTitle'
  | 'confidence'
  | 'finished'
  | 'landing'
  | 'note'
  | 'risk'
  | 'waitsForInput',
  keyof Types.SessionInsight
>>;

export type Checks = Holds<Same<
  | 'failed'
  | 'failing'
  | 'passed'
  | 'pending'
  | 'skipped',
  keyof Types.Checks
>>;

export type Label = Holds<Same<
  | 'color'
  | 'id'
  | 'name',
  keyof Types.Label
>>;

export type PrLabel = Holds<Same<
  | 'color'
  | 'description'
  | 'name',
  keyof Types.PrLabel
>>;

export type Pr = Holds<Same<
  | 'checks'
  | 'closedAt'
  | 'merge'
  | 'mergedAt'
  | 'number'
  | 'staleApproval'
  | 'state'
  | 'title'
  | 'updatedAt'
  | 'url',
  keyof Types.Pr
>>;

export type StaleApproval = Holds<Same<
  | 'by'
  | 'commit',
  keyof Types.StaleApproval
>>;

export type GitInfo = Holds<Same<
  | 'added'
  | 'ahead'
  | 'base'
  | 'baseAhead'
  | 'baseBehind'
  | 'behind'
  | 'dirty'
  | 'lastCommit'
  | 'lastCommitAt'
  | 'locked'
  | 'newWorkCount'
  | 'removed'
  | 'resetAt'
  | 'unpushed'
  | 'upstream',
  keyof Types.GitInfo
>>;

export type Target = Holds<Same<
  | 'detail'
  | 'disabled'
  | 'group'
  | 'icon'
  | 'id'
  | 'kbd'
  | 'label',
  Exclude<keyof Types.Target, 'image'>
>>;
/** An editor's own icon, which the windows put on its target from the icons they ask for apart (src/editors.ts). */
export type TargetApart = [Holds<Has<keyof Types.Target, 'image'>>];

export type Lane = Holds<Same<
  | 'bay'
  | 'branch'
  | 'callsign'
  | 'chats'
  | 'cleanup'
  | 'codealong'
  | 'defaultTarget'
  | 'detached'
  | 'detail'
  | 'device'
  | 'git'
  | 'glyph'
  | 'headline'
  | 'hidden'
  | 'id'
  | 'lastActivity'
  | 'looksFinished'
  | 'looksReusable'
  | 'origin'
  | 'parked'
  | 'path'
  | 'pr'
  | 'preview'
  | 'project'
  | 'repo'
  | 'repoSlug'
  | 'repoSlugNow'
  | 'reuse'
  | 'sessions'
  | 'startsChats'
  | 'targets',
  keyof Types.Lane
>>;

export type Cleanup = Holds<Same<
  | 'ok'
  | 'why',
  keyof Types.Cleanup
>>;

export type Preview = Holds<Same<
  | 'chromeProfile'
  | 'chromeProfileSet'
  | 'port'
  | 'server'
  | 'set'
  | 'url',
  keyof Types.Preview
>>;

export type PreviewServer = Holds<Same<
  | 'name'
  | 'pid'
  | 'port'
  | 'session'
  | 'startedAt'
  | 'title',
  keyof Types.PreviewServer
>>;

export type DeviceTag = Holds<Same<
  | 'color'
  | 'lastSeen'
  | 'name'
  | 'online'
  | 'short',
  keyof Types.DeviceTag
>>;

export type Meter = Holds<Same<
  | 'key'
  | 'label'
  | 'model'
  | 'percent'
  | 'resetsAt'
  | 'severity'
  | 'window',
  keyof Types.Meter
>>;

export type Usage = Holds<Same<
  | 'account'
  | 'asOf'
  | 'error'
  | 'harness'
  | 'loading'
  | 'meters'
  | 'needsSignIn'
  | 'noLimits'
  | 'plan'
  | 'preferred'
  | 'rateLimited',
  keyof Types.Usage
>>;

export type ProjectSummary = Holds<Same<
  | 'lanes'
  | 'name'
  | 'repos',
  keyof Types.ProjectSummary
>>;

export type AccountInfo = Holds<Same<
  | 'acp'
  | 'command'
  | 'disabled'
  | 'email'
  | 'folderExists'
  | 'harness'
  | 'home'
  | 'missingCli'
  | 'monthly'
  | 'name'
  | 'per_token'
  | 'signInNote'
  | 'signedIn'
  | 'start'
  | 'usage_cap',
  keyof Types.AccountInfo
>>;

export type Notice = Holds<Same<
  | 'key'
  | 'reviewAgain'
  | 'text'
  | 'title'
  | 'until',
  keyof Types.Notice
>>;

export type Board = Holds<Same<
  | 'apps'
  | 'chatShortcut'
  | 'devices'
  | 'experimental'
  | 'generatedAt'
  | 'githubAt'
  | 'githubCurrent'
  | 'githubLogin'
  | 'githubPolled'
  | 'improve'
  | 'improveShortcut'
  | 'jev'
  | 'labels'
  | 'lanes'
  | 'loading'
  | 'logoStatus'
  | 'mainWindowShortcut'
  | 'mcp'
  | 'notices'
  | 'nudgeAbout'
  | 'perfLog'
  | 'pinned'
  | 'pinnedPrs'
  | 'pinnedRepos'
  | 'pinnedWorktrees'
  | 'prStopped'
  | 'projects'
  | 'prs'
  | 'quickShortcut'
  | 'repos'
  | 'rev'
  | 'reviews'
  | 'schedules'
  | 'schedulesProblem'
  | 'sessions'
  | 'sides'
  | 'teamFiles'
  | 'terminals'
  | 'theme'
  | 'todoHints'
  | 'todos'
  | 'todosUnreadable'
  | 'training'
  | 'usage'
  | 'waitingShortcut'
  | 'whenLimited'
  | 'zoom',
  keyof Types.Board
>>;

export type TodoHint = Holds<Same<
  | 'p'
  | 'why',
  keyof Types.TodoHint
>>;

export type PrItem = Holds<Same<
  | 'additions'
  | 'author'
  | 'authoredBy'
  | 'autoReview'
  | 'base'
  | 'changedFiles'
  | 'checks'
  | 'closedAt'
  | 'createdAt'
  | 'deletions'
  | 'head'
  | 'headOid'
  | 'key'
  | 'labels'
  | 'lanes'
  | 'lastReview'
  | 'merge'
  | 'mergedAt'
  | 'mine'
  | 'nudge'
  | 'nudgeByDefault'
  | 'number'
  | 'primary'
  | 'primaryBy'
  | 'project'
  | 'repo'
  | 'repoSlug'
  | 'requestedAt'
  | 'requestedBy'
  | 'reviewRequested'
  | 'reviewersAsked'
  | 'sessions'
  | 'snoozedUntil'
  | 'staleApproval'
  | 'state'
  | 'title'
  | 'turn'
  | 'updatedAt'
  | 'url'
  | 'watched',
  keyof Types.PrItem
>>;

export type LastReview = Holds<Same<
  | 'at'
  | 'commit'
  | 'state',
  keyof Types.LastReview
>>;

export type AutoReview = Holds<Same<
  | 'approves'
  | 'at'
  | 'behind'
  | 'comment'
  | 'head'
  | 'holds'
  | 'machine'
  | 'note'
  | 'own'
  | 'parts'
  | 'rounds'
  | 'sendable'
  | 'sent'
  | 'sentAt'
  | 'stage'
  | 'submitted'
  | 'submittedAt'
  | 'suggests',
  keyof Types.AutoReview
>>;

export type ReviewPart = Holds<Same<
  | 'note'
  | 'session'
  | 'state'
  | 'who',
  keyof Types.ReviewPart
>>;

export type PrSession = Holds<Same<
  | 'key'
  | 'lane'
  | 'primary'
  | 'pushes',
  keyof Types.PrSession
>>;

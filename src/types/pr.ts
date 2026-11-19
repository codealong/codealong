// Pull requests: a worktree's PR (src-tauri/peer/src/model.rs) and a PR in the PRs view
// (src-tauri/src/model.rs), its detail and comments and your review of it (github.rs),
// Codealong's own reviews (autoreview.rs), and its merge box (prmerge.rs).

import type { Harness } from './session';

export type PrState = 'draft' | 'open' | 'approved' | 'changes' | 'merged' | 'closed';
/** Whether GitHub would merge an open PR now (its `mergeStateStatus`); `unknown` once it's closed. */
export type Merge = 'clean' | 'conflicts' | 'behind' | 'blocked' | 'unstable' | 'unknown';

export interface Checks {
  passed: number;
  failed: number;
  pending: number;
  skipped: number;
  failing: string[];
}

/** A label on a PR, as its repo on GitHub defines it. */
export interface PrLabel {
  name: string;
  /** Six hex digits without the `#`, as GitHub keeps it; empty when GitHub gave none that reads. */
  color: string;
  /** What it's for, when the repo says. */
  description?: string;
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
  /** It's approved only by approvals of an earlier commit than the one it's at (github.rs
   *  `stale_approval_of`). Other Macs' older builds don't send it. */
  staleApproval?: StaleApproval;
}

/** Approvals a PR has from before its latest commits, which GitHub still counts where the repo
 *  doesn't dismiss them: a merge would take in commits none of them saw (model.rs
 *  `StaleApproval`). */
export interface StaleApproval {
  /** Who gave them, by login. */
  by: string[];
  /** The commit the latest of them was of. */
  commit: string;
}

/** What becomes of a PR's news while its primary session, on this Mac, isn't running (nudges.rs
 *  `Stopped`): it's resumed out of sight to be told, or told where its harness takes a message
 *  while it doesn't run; not while another of the PR's sessions runs; or not at all. */
export type PrStopped = 'resumes' | 'beside' | 'never';

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
  /** When your review was last asked for, when GitHub says: what the review waits on you since. */
  requestedAt?: number | null;
  /** Whose reviews it still waits on besides yours (`reviewRequested`): people by login, teams by
   *  name. Only an open PR's the board reads (github.rs `asked_of_whom`). */
  reviewersAsked?: string[];
  /** You snoozed your review until then: it's out of Review requested, the floating bar's
   *  reviews and the mark until the time comes (board.rs, `apply_review_snoozes`). */
  snoozedUntil?: number | null;
  /** Its primary session is told of what happens on it (nudges.rs): turned on for this PR, from
   *  its card or menu or by `codealong pr nudge`, or for every PR of yours by `[pull_requests]
   *  nudge_by_default` unless turned off for this one. Said only while it's open. */
  nudge?: boolean;
  /** The nudge is on through `[pull_requests] nudge_by_default`, not turned on for this PR. */
  nudgeByDefault?: boolean;
  /** Watched, from its card or by `codealong pr watch`: its nudge tells of everything, whatever
   *  `[pull_requests] nudge_about` says (nudges.rs `about_of`). */
  watched?: boolean;
  /** Your review of it goes ahead of the sessions that need you in what waits on you, as
   *  `[pull_requests] reviews_first` goes for its repo (queue.ts `byPlace`). */
  reviewFirst?: boolean;
  /** It's yours and GitHub would merge it now, and nothing else brings it to you: no session of
   *  it is at work, and its primary session doesn't wait on you with Merge and done (board.rs
   *  `merge_waits`). It waits on you among the reviews (quick.ts `reviewRows`) until you merge it
   *  or snooze it. */
  waitsToMerge?: boolean;
  /** Its primary session did none of the PR's work itself (board.rs `pr_item`): it took the PR
   *  over from a helper it started, or was picked. It's told of what happens on the PR, but merging
   *  it doesn't make that session done (menus.ts `mergesWith`), and its work elsewhere isn't the
   *  PR's (PrsView.tsx `prSessionsNow`). */
  takenOver?: boolean;
  /** The key of its primary session, of `sessions`: the one picked (`codealong pr nudge`, its
   *  menu's Primary session), else the one that pushed to its branch most, the one that opened it
   *  winning a tie, else the one that started first (board.rs `pr_primary`). Only it is nudged. */
  primary?: string;
  /** Why `primary` is the one it is. */
  primaryBy?: PrimaryBy;
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
  /** The sessions that worked on the PR while it was open, each with the worktree it's listed
   *  under: archived ones too, which are never its primary. */
  sessions: PrSession[];
  /** The commit its branch is at, when GitHub says. */
  headOid?: string;
  /** The harness a commit trailer or PR body says wrote it (github.rs `authored_by`). */
  authoredBy?: Harness | null;
  /** Its labels on GitHub, in the order of their names; left out when it has none. */
  labels?: PrLabel[];
  /** Where Codealong's own review of it stands, once it's had one (autoreview.rs). */
  autoReview?: AutoReview;
  /** The review you last submitted on it, once you have (model.rs `LastReview`). */
  lastReview?: LastReview;
  /** It's approved only by approvals of an earlier commit (`Pr.staleApproval`). */
  staleApproval?: StaleApproval;
  /** Whose move it is, where its review state doesn't say (board.rs `turn_of`). */
  turn?: Turn;
}

/** Whose move an open PR is, where GitHub's review state doesn't say: a request for changes
 *  stands until its reviewer approves or dismisses it, whatever comes after (model.rs `Turn`).
 *  `replyNeeded`: a PR of yours with commits after the reviewer's last word and nothing said of
 *  them; `rereviewAsked`: asked again or written to since, their move; `reviewerReplied`: they
 *  wrote again since; `authorReplied`: someone else's PR, whose author wrote after you;
 *  `assigned`: someone else's PR that `who` assigned you to after your last review, which waits on
 *  your review as a request would. */
export interface Turn {
  kind: 'replyNeeded' | 'rereviewAsked' | 'reviewerReplied' | 'authorReplied' | 'assigned';
  /** The reviewer or author it's about. */
  who: string;
  /** The other reviewers it holds for alike. */
  also?: string[];
  since?: number;
}

/** The review you last submitted on a PR, as GitHub has it (model.rs `LastReview`): a draft isn't
 *  one. A dismissed one no longer says what it said. */
export interface LastReview {
  state: 'approved' | 'changes' | 'commented' | 'dismissed';
  /** The commit it was of; null where GitHub doesn't say. */
  commit: string | null;
  at: number | null;
}

/** What changed on a PR's branch since a commit (github.rs `Since`): nothing to compare, with no
 *  files, where the branch was rebased or force-pushed since (`follows` false). */
export interface Since {
  follows: boolean;
  commits: number;
  files: PrFile[];
  /** A comparison gives its first 300 files. */
  moreFiles: boolean;
}

/** Where a round of Codealong's automatic review of a PR stands (model.rs `ReviewStage`):
 *  `queued` is the round to come for a commit no session has started on yet. */
export type ReviewStage = 'queued' | 'reviewing' | 'posted' | 'drafted' | 'kept' | 'skipped' | 'failed';

/** A round's review as its agent wrote it (autoreview.rs `WrittenReview`), for the PR's panel. */
export interface WrittenReview {
  /** The commit it reviewed. */
  head: string;
  stage: ReviewStage;
  /** When it was done. */
  at: number;
  session: string | null;
  /** Who wrote it: its harness, and its model where that's known. */
  agent: string | null;
  /** Where it went on GitHub: the PR's files, beside a draft of yours, or the comment. */
  link: string | null;
  summary: string;
  comments: LineComment[];
}

/** Your draft review of a PR as GitHub has it (github.rs `Draft`): its text, without
 *  Codealong's hidden marks, and its comments. Only you see it until it's submitted. */
export interface Draft {
  id: string;
  body: string;
  /** The commit it was begun on, which its comments' lines are numbered by. */
  commit: string | null;
  url: string | null;
  comments: DraftComment[];
  /** How many comments it has, of which `comments` are the first hundred. */
  count: number;
}

/** A comment of your draft review: on its line of the file's new version, or on the file. */
export interface DraftComment {
  id: string;
  path: string;
  line: number | null;
  /** The PR's changes since moved or changed its line. */
  outdated: boolean;
  body: string;
}

/** Your review of a PR on GitHub (github.rs `YourReview`): the draft you haven't submitted, and
 *  the agent's review you did submit, when that's the review you last gave. */
export interface YourReview {
  draft: Draft | null;
  submitted: { state: string; at: number | null; url: string | null } | null;
}

/** What a review did (github.rs `Reviewed`): submitted your draft, with this many comments, of
 *  the draft's commit. */
export interface Reviewed {
  draft: number | null;
  commit?: string;
}

/** A comment on one line of a PR's code: the file, and the line's number in its new version. */
export interface LineComment {
  path: string;
  line: number;
  body: string;
}

/** The reviewer's part in a round (model.rs `ReviewPart`). */
export interface ReviewPart {
  /** The reviewer, by its account and model (`claude opus`), or `auto`. */
  who: string;
  session?: string;
  state: 'reviewing' | 'done' | 'skipped' | 'failed';
  note?: string;
}

/** Codealong's automatic review of a PR: its latest round (model.rs `AutoReview`). */
export interface AutoReview {
  stage: ReviewStage;
  /** The commit it reviews. */
  head: string;
  /** The reviewer's part, with its session. */
  parts?: ReviewPart[];
  /** When the round began, or ended once it has. */
  at: number;
  /** The latest review it posted, by its link. */
  comment?: string;
  /** Why it was skipped, didn't get done, or wasn't posted. */
  note?: string;
  /** How many rounds the PR has had. */
  rounds: number;
  /** The PR has new commits since: another round follows while it waits on your review. */
  behind?: boolean;
  /** The agent's review of its commit isn't done, so the PR is out of what waits on you until
   *  it is (`heldForAgent`). */
  holds?: boolean;
  /** A review of a PR of yours (`[reviews.own]`), which goes back to its primary session. */
  own?: boolean;
  /** The session this round's review went back to, and when. */
  sent?: string | null;
  sentAt?: number | null;
  /** The session it goes back to, which can be sent it now, for the Reviews tab's Send. */
  sendable?: string | null;
  /** What this round's agent would do with the PR, which its windows suggest. */
  suggests?: Verdict | null;
  /** The commit the agent's latest written review approved, when that review approves someone
   *  else's PR: what's asked of you is a sign-off once it's of the PR's commit (prStatus.ts
   *  `agentApproves`); one of an earlier commit says why the PR is held (`approvedBefore`). */
  approves?: string | null;
  /** What its review was submitted as yours as (`[reviews] submit`), and when. */
  submitted?: Verdict | null;
  submittedAt?: number | null;
  /** The machine whose review this is, when it's another of yours: one of your machines reviews
   *  a PR (autoreview.rs `reviewer_of`), and the others show its round. */
  machine?: string | null;
}

/** A session that worked on a PR. */
export interface PrSession {
  key: string;
  /** The lane it's listed under: its own, or the worktree it made or worked in. */
  lane: string;
  /** How often its records show it pushed to the PR's branch in the PR's time. */
  pushes?: number;
  /** One on another of your machines is the PR's primary session there, which that machine's
   *  nudges tell, and this is why (peerfeed.rs `pr_sessions_for_windows`). This machine's own is
   *  the PR's `primary`. */
  primary?: PrimaryBy;
}

/** Why a PR's primary session is the one it is (codealong-peer's `PrimaryBy`): it was picked; it
 *  pushed to the branch more often than the session that opened it; it opened it; no session's
 *  records show either, and it started first; or every session that worked on it is done with,
 *  and it started the helper that made it (board.rs `pr_primary`). `other` is one a later version
 *  names, which only another machine says. */
export type PrimaryBy = 'picked' | 'pushed' | 'opened' | 'started' | 'helper' | 'other';

export interface PrReview {
  author: string;
  state: 'APPROVED' | 'CHANGES_REQUESTED' | 'COMMENTED' | 'DISMISSED' | string;
  at: number | null;
  /** Given by you. Missing from older engines. */
  mine?: boolean;
}

export interface PrFile {
  path: string;
  status: string;
  additions: number;
  deletions: number;
  patch: string | null;
  /** Its path before the PR renamed it. */
  previous?: string | null;
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
  /** A thread on the code: its line in the file's new version, or the old one's when `before`,
   *  none for one on the whole file or one the PR's changes since moved. Missing from older
   *  engines. */
  line?: number | null;
  /** A thread on the code on the file as it was before the PR: on a line it removes, or one
   *  beside them, numbered as the file was. */
  before?: boolean;
  /** A thread on the code whose line the PR's changes since moved or changed. */
  outdated?: boolean;
  /** A thread on the code: how many replies follow its first comment. */
  replies?: number;
  resolved: boolean;
  /** Written by you. */
  mine: boolean;
  /** A review's own text: that review's verdict (`PrReview.state`). */
  review?: string;
}

/** What a review of a PR says (github.rs `Verdict`). */
export type Verdict = 'approve' | 'comment' | 'request-changes';

/** How a PR's commits go onto its base: a merge commit, one squashed commit, or rebased (prmerge.rs
 *  `Method`). */
export type MergeMethod = 'merge' | 'squash' | 'rebase';

/** A commit's title and message, as GitHub would write them. */
export interface CommitText {
  title: string;
  body: string;
}

/** What a PR's merge box offers you, and where the PR stands for it, as GitHub's has it
 *  (prmerge.rs `MergeBox`). */
export interface MergeBox {
  /** The PR's node id, which every act names it by. */
  id: string;
  /** The head commit the box was read at: a merge or an update names it, and GitHub refuses
   *  one whose branch has moved since. */
  head: string;
  branch: string;
  state: 'OPEN' | 'MERGED' | 'CLOSED';
  draft: boolean;
  /** `MERGEABLE`, `CONFLICTING`, or `UNKNOWN` while GitHub works it out. */
  mergeable: string;
  /** GitHub's `mergeStateStatus`: `CLEAN`, `HAS_HOOKS`, `UNSTABLE`, `BLOCKED`, `BEHIND`, `DIRTY`
   *  or `UNKNOWN`. */
  mergeState: string;
  /** `APPROVED`, `CHANGES_REQUESTED` or `REVIEW_REQUIRED`; null where the repo needs no review. */
  reviewDecision: string | null;
  /** You can write to the repo, which merging needs. */
  canMerge: boolean;
  /** The methods the repo allows, in GitHub's order, and the one GitHub picks for you. */
  methods: MergeMethod[];
  defaultMethod: MergeMethod | null;
  mergeText: CommitText;
  squashText: CommitText;
  /** The emails a commit you make can be under, and the one GitHub picks. */
  emails: string[];
  defaultEmail: string | null;
  canReady: boolean;
  /** You can merge it before its requirements are met, past the rules. */
  canBypass: boolean;
  canAuto: boolean;
  canStopAuto: boolean;
  /** Auto-merge, while it's on. */
  auto: { method: MergeMethod | null; by: string | null; at: number | null } | null;
  /** Its branch is behind its base and you can bring it up to date. */
  canUpdateBranch: boolean;
  /** Its base branch merges through a queue, and where it is in it. */
  queue: boolean;
  queued: { position: number | null; state: string | null } | null;
  /** GitHub deletes the branch itself once it's merged. */
  deletesBranch: boolean;
  /** The branch's ref, while it's there and you can push to its repo: what deleting it names. */
  branchRef: string | null;
  mergedBy: string | null;
}

/** What a merge or auto-merge writes: for a rebase, only the method. */
interface MergeCommit {
  method: MergeMethod;
  title: string | null;
  body: string | null;
  email: string | null;
}

/** One thing the merge box does (prmerge.rs `Op`). */
export type MergeOp =
  | ({ op: 'merge'; head: string; deleteBranch: string | null } & MergeCommit)
  | ({ op: 'auto'; head: string } & MergeCommit)
  | { op: 'stop-auto' }
  | { op: 'enqueue'; head: string }
  | { op: 'dequeue' }
  | { op: 'update-branch'; rebase: boolean; head: string }
  | { op: 'ready' }
  | { op: 'delete-branch'; branchRef: string };

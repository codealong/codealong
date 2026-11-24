---
paths:
  - "src-tauri/src/clones.rs"
  - "src-tauri/src/github.rs"
  - "src-tauri/src/ghpoll.rs"
  - "src-tauri/src/prlinks.rs"
  - "src-tauri/src/prasks.rs"
  - "src-tauri/src/prmerge.rs"
  - "src-tauri/src/autoreview.rs"
  - "scripts/land.sh"
  - ".github/workflows/*.yml"
---

# GitHub

How Codealong asks GitHub through `gh` and what GitHub's answers mean, moved from CLAUDE.md's
Harness facts (from GitHub's answers on this Mac, 2026-10-01). A bare file name is under
`src-tauri/src/`.

- What's asked of `gh`, and how: `github.rs` (module header, `OPEN_QUERY`, `DONE_QUERY`,
  `REQUESTED_QUERY`, `PAGE`, `asker_of`, `detail`, `approve`) and `board/prs.rs` (`polled_slugs`).
- GraphQL points (5,000 an hour, charged for the connections a query could return) and how a
  poll glances first and reads in full only what changed, keeping answers in
  `~/Library/Caches/Codealong/github`: `github.rs` ("Asking only for what changed",
  `GLANCE_FIELDS`, `stands`, `busy`, `Points`). The points are shared with everything else that
  uses `gh` (you, the agents, your other Macs). A query costs the connections it could return
  over a hundred, at least 1, whatever comes back (the review search 2); `rateLimit { cost
  remaining resetAt }` in a query says what it cost, and is free. A repo's checks count as
  running, which has it read every minute, only while an open PR with pending checks changed
  within the hour (`BUSY_FOR_MS`): checks pending longer are waiting on someone, or stuck.
- When GitHub is asked, how long it's left alone after a refusal for want of points, and what
  `gh` failing is called: `ghpoll.rs` (`EVERY`, `ASKED_LATELY`, `HOLD`, `Schedule`,
  `Event::PollEnded`) and `github.rs` (`Failure`, `RESERVE`).
- `gh api graphql` exits 1 when any part of a query failed (a repo you can't see is `null` under
  its alias, with a `NOT_FOUND` error) and still prints the rest: `github.rs` (`glance_some`).
  A PR number a repo doesn't have is `{"pullRequest": null}` under its alias, `NOT_FOUND` too.
  `gh api rate_limit` isn't to go by: it disagreed with the queries' own `rateLimit`, a second
  counter that one query in ten lands on.
- PRs sessions worked on in repos whose PRs aren't read are read by their numbers, one alias each
  (`repository(owner:, name:) { pullRequest(number:) { ...pr mergeStateStatus ...asked } }`):
  `github.rs` ("The PRs sessions made…", `made_query`, `fetch_made_text`), `ghpoll.rs`
  (`made_to_read`). 26 of them in one query, 24 there to see, cost 2 points and took 7.9 s; the
  same as a glance (`number state updatedAt headRefOid isDraft reviewDecision statusCheckRollup {
  state }`), 1 point and 2.6 s (2026-11-03), so they have none and are read again on a timer.
- Whether a worktree's HEAD is the last commit of a PR that merged, for removing it with its
  commits on no remote (GitHub deletes a merged PR's branch): `object(oid:) { ... on Commit {
  associatedPullRequests(first: 5) { nodes { number merged headRefOid } } } }`, a repo's commits as
  aliases of one query (`github.rs` "Whether a commit is the last of a PR that merged",
  `merged_heads`; `ghpoll.rs` `Heads`). It still ties a commit to its squash-merged PR after the
  branch is deleted, and a PR's `headRefOid` is the commit it merged at: over 31 worktrees' HEADs
  in one repo, the 27 whose PR had merged were each their PR's `headRefOid` exactly, a commit
  GitHub never had was `null` under its alias with no error, and the query cost 1 point
  (2026-11-18).
- A PR's file opened as a diff in your editor (`prdiff.rs`) reads its two sides with REST, no
  GraphQL points: `pulls/<n>` for `.head.sha` and `.base.sha`, `compare/<base>...<head>` for
  `.merge_base_commit.sha` (the side before, as GitHub's own diff has it), and
  `contents/<path, each part percent-encoded>?ref=<sha>` with `Accept: application/vnd.github.raw`
  for a file's bytes as they are. `pulls/<n>/files` gives a renamed file's old path as
  `previous_filename`. Read for real on 2026-10-11 (a private repo's PR #1182: its head, its merge base, and a
  file of 474 lines before and 447 after, matching its +14 −41): `prdiff_reads_github`, an
  ignored test.
- REST `repos/<owner>/<name>` answers to a repo's old names with the repo as it is now:
  `github.rs` (`current_slug`), `ghpoll.rs` (`Names`), and the lane's `repo_slug_now`
  (`src-tauri/peer/src/model.rs`), which other devices line a checkout with an old remote up
  by (`peers.rs`).
- What a conversation about a PR is handed to read (it can ask `gh` for little or nothing
  itself; Claude Code's only its lookups, `claude/side.rs` `PR_LOOKUPS`): `github.rs`
  (`for_reading`, `reading_md`), `side.rs` (`pr_recipe`, `reading_dir`). `gh pr diff` is REST and
  costs no GraphQL points; it gave a 985 KB diff for 87 files whose `pulls/<n>/files` patches came
  to 290 KB, GitHub leaving out the patches of its largest files there (2026-10-07).
- What `mergeStateStatus` says: `src-tauri/peer/src/model.rs` (`Merge`) and `github.rs`
  (`merge_of`). It needs no preview header, and a PR approved with passing checks can still be
  `DIRTY`.
- Merging, as GitHub's merge box does: `prmerge.rs` (module header, `QUERY`, `Op`). The box's
  defaults are GitHub's own: `viewerMergeHeadlineText`/`viewerMergeBodyText(mergeType:)` give the
  title and message GitHub would write for a merge commit or a squash with the repo's settings
  applied (`Add teams (#7)`, `Merge pull request #7 from ana/teams`; empty for a rebase),
  `viewerDefaultMergeMethod` its pick for you, `viewerPossibleCommitEmails` your emails. Asking
  for `viewerCanDeleteHeadRef` beside `mergeStateStatus` and `viewerCanUpdateBranch` fails the
  whole query for a PR from a fork ("Something went wrong while executing your query", every
  time, an alias of its own no help; 2026-10-09), so it isn't asked. `viewerCanMergeAsAdmin` was
  false for a repo's admin on its clean PRs and drafts (2026-10-09), so the box offers merging
  past the rules only where GitHub says so. `gh pr merge --admin` is a plain `mergePullRequest`,
  which GitHub lets through for whoever may bypass the rules. A mutation GitHub refuses makes
  `gh api graphql` exit 1 with `gh: <message>` on stderr, and one that names a node that isn't
  there fails on `NOT_FOUND` only after its document is checked, which is how the mutations were
  checked without changing anything. A PR merged here shows merged at once (`ghpoll.rs`
  `on_merged`, `merged_here`), against an answer asked for before the merge too; the search for
  PRs that wait on your review lists only open ones, so one it found stays in it, merged, until
  GitHub is asked again.
- Automatic reviews (autoreview.rs) never submit a review unless `[reviews] submit` says to
  submit the agent's verdict (`Job::Submit`, through `github.rs` `review_with`, which submits your
  draft with it; not run for real): any review you submit, a comment one too, takes you off the
  PR's requested reviewers, and the review is otherwise still yours to give. Commits after the
  round's commit, while `viewerLatestReview` is still that comment or request for changes, bring
  the PR back to wait on you (`autoreview.rs` `asked_again`, from reviews.json's submission, not
  one from the PR's box), as an author who pushes the fixes doesn't always ask GitHub again. Not
  from the review's text: Codealong's marks stay in a draft you rewrote (`edit_draft`), and your
  words go before the agent's from the box. Not the review's commit either: a draft begun on an
  earlier commit keeps it. Only in repos whose PRs are read, or for a PR read by its number (one a
  session is linked to: `board::read_now`): the search (`review-requested:@me`) never lists it. By
  default they go into a pending review on your account (`github.rs` `draft_review`,
  `add_to_draft`, `pending_review`): GraphQL `addPullRequestReview` with no `event` leaves it
  `PENDING`, which only its author sees, its `threads` each a `path`, a `line` in the file's new
  version and `side: RIGHT`. GitHub keeps one pending review per person and PR, found under
  `reviews(states: PENDING)` with `viewerDidAuthor`, so a review goes into yours when you have
  one (`addPullRequestReviewThread`, then `updatePullRequestReview` for its text). A comment
  can only go on a line the PR's diff shows (`autoreview::commentable`); the rest are written
  into the text. Variables go as JSON on stdin (`gh api graphql --input -`), since a list of
  objects can't be given with `-f`. The documents were checked against GitHub's schema with
  made-up node ids, which it answers `NOT_FOUND` only after checking them, and `commitOID` takes
  a whole 40-character id (2026-10-09). The first real run (that PR #1182, 2026-10-10) left a
  `PENDING` review at the reviewed commit with its text and four comments, each on its line. A
  review whose PR moved on while it was written (`autoreview.rs` `post`, `moved_to`) begins its
  draft at the commit the PR is at by then, its comments in its text: their lines are numbered
  by a commit the PR's diff no longer shows, and a rewritten branch no longer has that commit to
  begin a review at (not run for real). With `post =
  "comment"` they post with REST `POST repos/<owner>/<name>/issues/<n>/comments` (`comment`). What changed between two commits is REST `repos/<owner>/<name>/compare/
  <a>...<b>` (`changes_since`), whose `status` is `ahead` only when the second follows from the
  first (`behind` the other way round; checked 2026-10-09, with `files[].filename` and `patch`);
  after a rebase or force push it's `diverged` and its files hold the base's changes too, so a
  review then reads the whole diff again. A posted review carries an HTML comment naming
  the PR and commit (`autoreview::marker`), which GitHub doesn't show, so a review already
  posted (from another Mac, or before a restart) is found among the PR's comments and not
  posted twice. A drafted one's mark line names its round after that (`autoreview::draft_mark`),
  so a second review of a commit, asked for by hand, goes into the draft beside the first.
- Your draft review, read for a PR's Review tab, and submitting it: `github.rs` (`DRAFT_QUERY`,
  `your_review`, `review`, `edit_draft`). The query asks the pending review's
  `comments(first: 100) { totalCount nodes { id path line outdated body } }` and the PR's
  `viewerLatestReview { state body submittedAt url }`, which was `null` before any review of
  yours; it cost 1 point (PR #1182 again, 2026-10-10, which read the agent's draft with its four
  comments, `line` the file's new version's as drafted). A verdict on a PR you have a draft of is
  `submitPullRequestReview(pullRequestReviewId, event: APPROVE | COMMENT | REQUEST_CHANGES, body)`,
  whose `body` sets the review's text: it's always given, the draft's own when you wrote nothing,
  since what GitHub does with none wasn't seen. A comment is changed with
  `updatePullRequestReviewComment(pullRequestReviewCommentId, body)` and taken out with
  `deletePullRequestReviewComment(id)`. These and the query were checked against GitHub's schema
  with made-up ids (`NOT_FOUND` after the document's checked; a made-up field is refused first),
  not run for real: submitting would reach the PR's people. Codealong's hidden mark is kept through
  an edit of the draft's text (`unmarked`), since the next round looks for it.
- A PR the search finds waiting on your review in a repo with no checkout here is left out of
  the board (`ghpoll.rs` `on_requested`, `not_here`), and its repo cloned when a project covers it
  and reviews are on for it (`clones.rs`). `gh repo clone <owner>/<name> <dir> -- --quiet` clones
  as `gh config get git_protocol -h github.com` says, which can differ from the global
  `git_protocol` (ssh for the host and https globally on this Mac, 2026-11-24, which cloned with
  an ssh `origin`). A fork's clone gets the repo it was forked from as `upstream` too, fetched
  at once; `origin` still names the checkout (`repos::load_checkout`). Nothing is cloned until
  GitHub has said what each repo here is called now (`GhPoll::names_known`): `gh api
  repos/<owner>/<name>` for a repo GitHub doesn't have for you exits 1 with `gh: Not Found (HTTP
  404)` on stderr (2026-11-24), which `github::current_slug` takes as the repo going by its own
  name; any other failure is no answer.
- Who's assigned to a PR and by whom: `ASSIGNED_EVENT` and `UNASSIGNED_EVENT` beside
  `REVIEW_REQUESTED_EVENT` in `ASKED_FIELDS`'s `timelineItems` (`assigned_of`, `events_of`), each
  node's `__typename` telling them apart, `assignee` a union whose `... on User { login }` names
  you; one connection, so a repo's query stayed at 8 points with them and `last: 20`
  (2026-10-28). Assigning moves the PR's `updatedAt` (to the assignment's second), so a glance
  sees it, and a review leaves the assignment standing, unlike a review
  request (`PrRaw::assigned_review`). The PRs assigned to you anywhere are a search of their own
  beside the requests' (`ASSIGNED_SEARCH`, `assignee:@me -author:@me`; `REQUESTED_QUERY` and the
  glance), so they never crowd requests out of its fifty and being found by the requests' search
  still says a team you're on was asked (`asked_of`). With the board's fragments the two cost 7
  points to the requests' 4 alone, the glance's two searches 1 (2026-10-28). One search for
  either, `(review-requested:@me OR assignee:@me)`, takes `type: ISSUE_ADVANCED` (the plain
  `ISSUE` search answered it with no PRs and no error) and cost 4, but can't tell which matched.
- The review you last submitted on a PR, on every PR the board reads: `viewerLatestReview { state
  submittedAt commit { oid } }` in `PR_FIELDS` (`last_review_of`), one object, so no points (a
  60-PR search with it cost 1, 2026-10-20). It leaves out a draft: with a pending review made on a
  PR whose latest was an approval, it still gave the approval (made and deleted again, 2026-10-20).
  Its states are `APPROVED`, `CHANGES_REQUESTED`, `COMMENTED` and `DISMISSED`; a dismissed one keeps
  its commit. Submitting a review moves the PR's `updatedAt`, so the next glance reads it; a review
  given here shows at once (`ghpoll.rs` `on_approved`). What changed since its commit is REST
  `compare/<commit>...<head>?per_page=1` (`since`): the files and `total_commits` come whole on the
  first page with one commit listed (105 files and 74 commits, 2026-10-20).
- The approvals that count on a PR and the commits they were of, for its stale approval:
  `latestOpinionatedReviews(first: 100, writersOnly: true) { pageInfo { hasNextPage } nodes { state
  submittedAt author { login } commit { oid } } }` in `ASKED_FIELDS`, asked only of open PRs
  (`stale_approval_of`), which says nothing of a PR with a page after it. It's each
  reviewer's latest review that approved or asked for changes, from people who may write to the
  repo. Where the repo doesn't dismiss approvals when commits come, `reviewDecision` stays
  `APPROVED` after a push and a rebase, and the approval's commit can be one the branch no longer
  has (a private repo's PR approved at one commit, then rebased and pushed twice: `mergeStateStatus`
  `CLEAN`, 2026-10-20). It's a connection: a repo's query went from 6 points to 7, at 20 a PR
  and at 100 alike.
- Who spoke last on an open PR, for whose move it is (`board/prs.rs` `turn_of`): `said:
  timelineItems(itemTypes: [ISSUE_COMMENT, PULL_REQUEST_REVIEW], last: 20)` in `ASKED_FIELDS`
  (`said_of`), beside the review requests' timeline and `latestOpinionatedReviews`, and the head
  commit's `committedDate` in `PR_FIELDS`. A reply on a thread is a `PullRequestReview` of its
  own; a bot's author has `__typename: Bot`; a commit's `pushedDate` is null (GitHub retired it), so
  a push is dated by its commit. A repo's query went from 7 points to 8 with it
  (2026-10-22). Asking a reviewer again, as Re-request review does, is REST `POST
  repos/<owner>/<name>/pulls/<n>/requested_reviewers` with `{"reviewers": [login]}` on stdin
  (`request_review`, `rerequest.rs`), no GraphQL points; checked only against a PR number that
  doesn't exist (`gh: Not Found (HTTP 404)`, exit 1), since a real one notifies the reviewer.
- `main`'s merge queue, as `scripts/land.sh` and `.github/workflows/ci.yml` rely on it
  (2026-11-20, from GitHub's documentation and from two pull requests made to try it, #5 and
  #6, each with a unit test that always fails so that neither could land):
  - A place in the queue is asked for one commit: `enqueuePullRequest(pullRequestId,
    expectedHeadOid)`, as `prmerge.rs` asks too; asked for a commit the branch isn't at,
    GitHub answers `UNPROCESSABLE`, "expected head oid does not match". It goes from `QUEUED`
    to `AWAITING_CHECKS` within half a minute. "Merge when ready" is something else, a
    standing request (`autoMergeRequest`) that GitHub offers on a queue's branch even with the
    repository's automatic merges off, and that holds for whatever is pushed afterwards:
    `land.sh` makes none, and pushes to no pull request that has one.
  - The queue's run is a `merge_group` event on a branch of its own
    (`gh-readonly-queue/main/pr-<number>-<commit>`), of the workflow as that commit has it.
    Why a pull request left the queue is a `RemovedFromMergeQueueEvent` in its timeline, its
    `reason` in words (`manual` for one taken out by hand or by `dequeuePullRequest`).
  - A queued pull request's branch can't be pushed to, by anyone, an organization's admin who
    may bypass the ruleset included: `git push` and `git push --force` are both turned down
    (`GH006: Protected branch update failed`, "Branches that are queued for merging cannot be
    updated. To modify this branch, dequeue the associated pull request"), and the contents
    API answers 409 with the same words. The pull request stayed queued as the commit it was
    queued at. So what's in the queue is what lands, and a branch moves again only once its
    pull request is out of it: `land.sh` takes one out (`dequeuePullRequest(input: {id})`, its
    document checked against GitHub's schema with a made-up id on 2026-11-21, not yet run on a
    real pull request) when the branch adds to the queued commit, and pushes once GitHub shows
    it out. GitHub's documentation says none of this, and one account
    (github.com/orgs/community/discussions/194832, 2026-05) has a force-push accepted and the
    entry showing the new commit: not what happened here.
  - A pull request that conflicts with its base (`mergeable: CONFLICTING`) gets no
    `pull_request` run at all, so a wait for its checks never ends.
  - GitHub can also drop the `pull_request` event on a clean, `MERGEABLE` pull request (seen on
    #14, 2026-11-20: zero check runs for three hours until `gh pr close` + `gh pr reopen` fired
    it), so `land.sh`'s `queue_it` gives up once empty answers persist rather than wait three
    hours, keeping `$sent` so reopening and rerunning picks up the same pull request.
  - A workflow run's `pull_requests` lists every open pull request whose branch or commit
    matches, not the one that started it: nothing says which pull request a run was for.

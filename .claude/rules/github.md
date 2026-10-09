---
paths:
  - "src-tauri/src/github.rs"
  - "src-tauri/src/ghpoll.rs"
  - "src-tauri/src/prlinks.rs"
  - "src-tauri/src/prasks.rs"
  - "src-tauri/src/prmerge.rs"
  - "src-tauri/src/autoreview.rs"
---

# GitHub

How Codealong asks GitHub through `gh` and what GitHub's answers mean, moved from CLAUDE.md's
Harness facts (from GitHub's answers on this Mac, 2026-10-01). A bare file name is under
`src-tauri/src/`.

- What's asked of `gh`, and how: `github.rs` (module header, `QUERY`, `REQUESTED_QUERY`,
  `asker_of`, `detail`, `approve`) and `board.rs` (`polled_slugs`).
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
  `gh api rate_limit` isn't to go by: it disagreed with the queries' own `rateLimit`, a second
  counter that one query in ten lands on.
- REST `repos/<owner>/<name>` answers to a repo's old names with the repo as it is now:
  `github.rs` (`current_slug`), `ghpoll.rs` (`Names`), and the lane's `repo_slug_now`
  (`src-tauri/peer/src/model.rs`), which other devices line a checkout with an old remote up
  by (`peers.rs`).
- What a conversation about a PR is handed to read (it can't ask `gh` itself): `github.rs`
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
  checked without changing anything.
- Automatic reviews (autoreview.rs) post with REST `POST repos/<owner>/<name>/issues/<n>/comments`
  (`github.rs` `comment`, the body as JSON on stdin), not `gh pr review --comment`: any review
  you submit, a comment one too, takes you off the PR's requested reviewers, and the review is
  still yours to give. What changed between two commits is REST `repos/<owner>/<name>/compare/
  <a>...<b>` (`changes_since`), whose `status` is `ahead` only when the second follows from the
  first (`behind` the other way round; checked 2026-10-09, with `files[].filename` and `patch`);
  after a rebase or force push it's `diverged` and its files hold the base's changes too, so a
  review then reads the whole diff again. A posted review carries an HTML comment naming
  the PR and commit (`autoreview::marker`), which GitHub doesn't show, so a review already
  posted (from another Mac, or before a restart) is found among the PR's comments and not
  posted twice. Neither the post nor its refusal was tried against a real PR (2026-10-09).

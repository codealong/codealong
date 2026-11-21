#!/usr/bin/env bash
# Lands this checkout's branch on origin/main: steps 2 and 3 of CLAUDE.md's *Landing work* as one
# command, for whoever has write access to the repository. The checks that keep main working
# run on GitHub (.github/workflows/ci.yml), not here: agents land dozens of changes a day from
# one busy Mac, where the same checks took from five minutes to forty and queued one landing
# behind another. This pushes the branch, opens its pull request and puts it in main's merge
# queue, which runs the checks on the change as it would land (on top of main and of what's
# queued before it) and merges it when they pass. So main never holds a commit they didn't see,
# and nothing waits here but this script.
#
#   scripts/land.sh --reviewed-by "<account>, <model>"   a change to code, after its review
#   scripts/land.sh --no-review "<why>"                   one no review could be had for
#   scripts/land.sh                                       docs and notes only
#   scripts/land.sh --no-wait …                           queue it and come back: GitHub lands
#                                                         it, and running this again says how
#                                                         it went and catches up
#   scripts/land.sh --check                               the checks alone, on GitHub: no landing
#   scripts/land.sh --plan [path…]                        only says which checks those paths (or
#                                                         this branch's commits) call for
#
# In order:
#   1. It fetches origin/main and sees that the branch applies on it. It rewrites nothing of
#      yours: the commit it lands is the one that's here when it's asked, and a branch that
#      doesn't apply is yours to rebase (it says how: with every commit written again, so
#      that git doesn't skip one of yours for one of main's that only looks the same),
#      resolve and test before running this again. A branch from before the history was
#      rewritten is turned away, with the rebase CLAUDE.md gives for it.
#   2. It pushes the branch to origin under its own name: adding to what's there, or, when
#      you've rebased it, replacing what this script itself pushed there before and nothing
#      else.
#   3. It opens the branch's pull request, or takes the one that's open for it (of this
#      repository's own branch; not one someone has set to merge by itself, nor one in the
#      queue as another commit), and, once that has passed its own run of CI, puts it in the
#      queue as the commit pushed here and no other. CI leaves a pull request this opened to
#      the queue's run (it knows it by a line of its description), so nothing runs twice and
#      its own run is moments. The queue writes the commits on top of main (a rebase, no merge
#      commit): what lands is copies of yours.
#   4. It waits for the queue, saying where the change stands, and stops when it has landed or
#      can't as it is: a check failed and the queue put it out (it says which run, and how to
#      read it), it no longer applies on main, or someone took it out of the queue (GitHub
#      lets nobody push to a queued pull request's branch, so what's queued is what lands).
#      It's still the pull request: fix it on this branch, commit, and run this again. A
#      landing that's with GitHub is noted, and running this again for the same commit takes
#      it up where it got to.
#   5. It brings this branch and the main checkout up to origin/main (catch-up-main.sh) and
#      prints the commit and where things stand, for the status line, saying so when this
#      checkout isn't at origin/main after all. Relaunching is still yours: scripts/relaunch.sh,
#      when what the app runs changed.
#
# It lands only what's committed: uncommitted changes here, yours or another session's, stop it
# before anything is touched. It's for a worktree that's yours alone (CLAUDE.md, *Landing
# work*). Nothing here rebases, merges or stashes: once its commits have landed the branch is
# set to main's commit by git's own change of branch, which refuses rather than write over a
# file. What another session does in the same checkout while this runs isn't landed as yours,
# but it can find the checkout moved under it, and a commit it makes on the branch in the
# moment that move is made is left off the branch (`git reflog show <branch>` has it, where
# the branch keeps a log): git can't be asked to move a branch only if it's where it was last
# seen. One landing of a branch runs
# at a time. `--check` runs CI on the
# commit as it is here, on a branch made for that run and removed after, so a landing under way
# isn't touched. There is no flag to skip the checks, and none to run them here: `npm test` and
# `cargo test` are still yours to run while you work.
set -uo pipefail

# Main as origin has it, by its whole name: `origin/main` alone is also what git calls a local
# branch someone named that, which it would take first.
MAIN=refs/remotes/origin/main
# The rebase an author is asked for when a branch doesn't apply on main: every commit written
# again, so that git doesn't skip one of theirs for one of main's that only looks the same.
REBASE="git rebase --reapply-cherry-picks $MAIN"

# Everything is in functions, read whole before any of it runs: catching up at the end can
# rewrite this file, and bash reads a script as it goes.

usage() {
  sed -n '2,/^set -uo/p' "$self" | sed '$d; s/^# \{0,1\}//'
}

# Which checks a changed file calls for, one word to a line: `ui` (build, lint, unit tests),
# `engine` (cargo test, on macOS and on Linux), `checks` (the browser checks on the made-up
# Mac, which run on main after a landing), `code` (it needs a review) and `install` (what's
# installed changed). CI asks this table (.github/workflows/ci.yml and checks.yml, through
# --plan), so it's the one place that says. The lists err
# towards running a check: each side's tests read some of the other's files, named here with
# what reads them.
calls_for() {
  case "$1" in
    # Docs nothing tests or builds. CLAUDE.md, the rules and the skills aren't among them.
    docs/* | design/*.md | README.md | CONTRIBUTING.md | SECURITY.md | .github/*.md | .github/CODEOWNERS | .claude/skills/*.md) ;;
    # Words the engine's tests hold: the snapshot modes (snapshot.rs) and the command's reference
    # and the plugin's copies of the skills (cli.rs, skills.rs, install.rs). Words, so no review.
    .claude/rules/snapshot.md | skills/*.md | plugin/*.md)
      echo engine
      ;;
    # Words checked by src/conventions.test.ts (CLAUDE.md's size and list of rules files, the
    # rules' frontmatter, and the files tauri.conf.json copies under bundle.resources). Checked
    # under `ui` in seconds instead of running the macOS+Linux engine builds. Words, so no review.
    CLAUDE.md | AGENTS.md | LICENSE | NOTICE | THIRD_PARTY_NOTICES.md | .claude/rules/*)
      echo ui
      ;;
    # The engine, and what the windows' tests read of it: the command tables (commands.test.ts)
    # and the fixtures it writes for them (src/testing/).
    src-tauri/* | .cargo/*)
      echo engine
      echo ui
      echo checks
      echo code
      ;;
    # What the engine's tests read of the windows': the fixtures it writes for them
    # (wiretypes.rs, board.rs, windowfeed.rs), the tokens (geometry.rs, quick.rs) and Settings
    # (emblems.rs). The rest of src/testing/ is the unit tests' own (their stand-in for the
    # engine, their page), which no engine test reads.
    src/testing/modelFields.ts | src/testing/ruleCases.json | src/testing/boardPushes.json | src/styles/tokens.css | src/ui/SettingsDialog.tsx | design/*)
      echo engine
      echo ui
      echo checks
      echo code
      ;;
    # The skill and the plugin as the bundle carries them and install.rs copies them.
    skills/* | plugin/* | .claude-plugin/*)
      echo engine
      echo code
      ;;
    # The checks themselves: all of them, to see they still run.
    .github/workflows/*)
      echo ui
      echo engine
      echo checks
      echo code
      ;;
    # The rest of what GitHub reads (a template, an action's settings) wants a second reader.
    .github/*) echo code ;;
    # What's installed: one changed without the other fails a clean install, as CI's is.
    package.json | package-lock.json)
      echo ui
      echo checks
      echo code
      echo install
      ;;
    # The unit tests and their stand-ins, which no page loads.
    src/*.test.ts | src/*.test.tsx | src/testing/*)
      echo ui
      echo code
      ;;
    # The browser checks and what runs them.
    scripts/preview/* | scripts/preview.sh | scripts/checks.sh | scripts/engine.sh)
      echo ui
      echo checks
      echo code
      ;;
    # The other scripts, which the unit tests cover some of.
    scripts/*)
      echo ui
      echo code
      ;;
    # Everything else is the windows'.
    *)
      echo ui
      echo checks
      echo code
      ;;
  esac
}

# The checks for a list of paths on stdin, a NUL after each as git writes them (a name can
# hold anything else), as a line of words: "ui engine checks code", or "" for none.
plan_of() {
  local path word words wants="" out=""
  while IFS= read -r -d '' path; do
    [ -n "$path" ] || continue
    # A table that fails for a path isn't a path that calls for nothing.
    words="$(calls_for "$path")" || return 1
    wants="$wants ${words//$'\n'/ }"
  done
  for word in ui engine checks code install; do
    case " $wants " in *" $word "*) out="$out $word" ;; esac
  done
  echo "${out# }"
}

# The files this branch's commits change, into the file `$list`: both names of one that moved
# (a rename shows only where it went, and the checks its old place called for would be skipped),
# each as it is with a NUL after it. Against main as `$2` has it, or origin/main. Fails when
# git can't say, so that no answer isn't taken for no changes.
changed() {
  git diff --no-renames --name-only -z "${2:-$MAIN}...${1:-HEAD}" >"$list"
}

fetch() { GIT_TERMINAL_PROMPT=0 git fetch -q origin main; }

die() {
  echo "$1" >&2
  exit "${2:-1}"
}

clean_or_die() {
  local dirty
  # No answer isn't a clean checkout, and a file git doesn't know of isn't in the commit, though
  # a test here would find it: asked for whatever `status.showUntrackedFiles` says.
  dirty="$(git status --porcelain --untracked-files=normal)" || die "Couldn't ask git whether this checkout has uncommitted changes."
  if [ -n "$dirty" ]; then
    echo "This checkout has uncommitted changes, and only what's committed lands:" >&2
    printf '%s\n' "$dirty" | head -10 >&2
    die "Commit yours first. If they're another session's, land from a worktree of your own."
  fi
}

# A branch made before the history was rewritten (CLAUDE.md, *Landing work*) stands on commits
# origin/main no longer has, and a plain rebase would replay those onto it.
refuse_old_history() {
  local ref base
  for ref in $(git for-each-ref --format='%(refname)' 'refs/pre-rewrite*/main'); do
    base="$(git merge-base HEAD "$ref" 2>/dev/null)" || continue
    if [ -n "$base" ] && ! git merge-base --is-ancestor "$base" "$MAIN"; then
      echo "This branch stands on commits from before the history was rewritten ($ref)." >&2
      echo "Move your own commits onto origin/main first, as CLAUDE.md says:" >&2
      die "  git rebase --onto origin/main \$(git merge-base HEAD $ref)"
    fi
  done
}

short() { git rev-parse --short "$1" 2>/dev/null || echo "${1:0:8}"; }

# Waits between two looks at GitHub: twenty seconds, or what a test asks for.
pause() { sleep "${CODEALONG_LAND_POLL:-20}"; }

# Whether the checkout is still what this landing is of: on its branch, at `$1`, with nothing
# uncommitted. Another session can share a checkout, and the waits here are long. A status git
# couldn't give isn't a clean one.
still_here() {
  local dirty
  [ "$(git symbolic-ref -q HEAD)" = "refs/heads/$branch" ] && [ "$(git rev-parse HEAD)" = "$1" ] || return 1
  dirty="$(git status --porcelain --untracked-files=normal)" || return 1
  [ -z "$dirty" ]
}

# One landing of a branch at a time in a repository: two would push and queue under each
# other. The turn is a lock the kernel holds on a file in the repository's .git, for a small
# helper that lives as long as this script does and no longer, so there's no owner's pid to
# go by, no lock left by one that was killed, and no two takers of one that was. It isn't
# waited for: a branch that's being landed is told so.
take_turn() {
  command -v python3 >/dev/null || die "python3 isn't here, which holds the landing's turn."
  local said waited=0
  mkdir -p "$notes" || die "Couldn't make $notes."
  said="$(mktemp)"
  python3 -I - "$turn" "$$" >"$said" 2>&1 <<'EOF' &
import fcntl, os, sys, time

path, owner = sys.argv[1], int(sys.argv[2])
held = os.open(path, os.O_RDWR | os.O_CREAT, 0o644)
try:
    fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
except OSError:
    print("taken by " + (os.read(held, 32).decode().strip() or "another process"), flush=True)
    sys.exit(0)
os.ftruncate(held, 0)
os.write(held, ("pid %d" % owner).encode())
print("held", flush=True)
while True:
    try:
        os.kill(owner, 0)
    except OSError:
        break
    time.sleep(1)
EOF
  holder=$!
  # Disowned, or bash says "Terminated" with the helper's whole text when it's told to go.
  disown "$holder" 2>/dev/null
  until [ -s "$said" ]; do
    if ! kill -0 "$holder" 2>/dev/null || [ "$waited" -ge 600 ]; then
      break
    fi
    sleep 0.1
    waited=$((waited + 1))
  done
  case "$(cat "$said")" in
    held)
      rm -f "$said"
      ;;
    "taken by "*)
      # The helper that found it taken has gone by itself.
      holder=""
      waited="$(cat "$said")"
      rm -f "$said"
      die "Another landing of $branch is under way here (${waited#taken by }): wait for it, or stop it, before starting one."
      ;;
    *)
      cat "$said" >&2
      rm -f "$said"
      kill "$holder" 2>/dev/null
      holder=""
      die "Couldn't take the turn to land $branch ($turn)."
      ;;
  esac
}

# Ends the turn: the helper that holds the lock is told to go.
let_go() {
  [ -z "${holder:-}" ] || kill "$holder" 2>/dev/null
}

# Which repository on GitHub this is: origin's, which is where git pushes, so that gh asks the
# same one whatever it has been told to take for its default (`GH_REPO`, `gh repo
# set-default`). Where origin isn't GitHub's by its address, gh says.
which_repository() {
  local url
  url="$(git remote get-url origin 2>/dev/null)"
  case "$url" in
    *github.com[:/]*/*)
      repo="${url#*github.com[:/]}"
      repo="${repo%/}"
      repo="${repo%.git}"
      export GH_REPO="$repo"
      ;;
    *) repo="$(gh repo view --json nameWithOwner --jq .nameWithOwner 2>"$log")" || { cat "$log" >&2; die "Couldn't ask GitHub which repository this is (gh): is it signed in? gh auth status."; } ;;
  esac
}

# Pushes `$head` to origin as this branch. What's there already is added to when this only adds
# to it. It's replaced (a rebase has rewritten it) only when it's what this script itself
# pushed there last, which it notes in a ref of its own when it has pushed: anything else is
# someone's work, or yours pushed by hand, and this doesn't throw either away.
push_branch() {
  local there mine note="refs/codealong/pushed/$branch"
  there="$(git ls-remote origin "refs/heads/$branch" 2>"$log" | cut -f1)" || { cat "$log" >&2; die "Couldn't ask origin for its $branch."; }
  [ "$there" != "$head" ] || return 0
  if [ -n "$there" ] && ! git merge-base --is-ancestor "$there" "$head" 2>/dev/null; then
    mine="$(git rev-parse -q --verify "$note" 2>/dev/null || true)"
    if [ "$there" != "$mine" ]; then
      echo "origin has a $branch already, at $(short "$there"), which isn't in what's being landed and isn't what this" >&2
      echo "script pushed there last: someone else's, or yours pushed by hand before a rebase." >&2
      echo "Look at it (git fetch origin $branch). If it's yours to replace:" >&2
      die "  git push --force-with-lease=$branch:$there origin $head:refs/heads/$branch"
    fi
  fi
  # The branch alone: with `push.followTags` a tag on these commits would go too, and a
  # version's tag starts a release.
  if ! git push -q --no-follow-tags --force-with-lease="refs/heads/$branch:$there" origin "$head:refs/heads/$branch" 2>"$log"; then
    echo "The push of $branch was turned down:" >&2
    cat "$log" >&2
    exit 1
  fi
  git update-ref "$note" "$head" || true
}

# The pull request open for this branch, of this repository's own branch and into main: its
# number, or nothing. A fork's branch of the same name is another pull request.
pull_request() {
  gh pr list --head "$branch" --base main --state open --json number,isCrossRepository \
    --jq '[.[] | select(.isCrossRepository == false)][0].number // empty' 2>"$log" ||
    { cat "$log" >&2; die "Couldn't ask GitHub for this branch's pull request (gh)."; }
}

# What the pull request says: the commits, and who reviewed them.
describe() {
  git log --reverse --format='- %s' "$main_at..$head"
  echo
  if [ -n "$reviewed" ]; then
    echo "Reviewed by: $reviewed."
  elif [ -n "$unreviewed" ]; then
    echo "No review: $unreviewed."
  else
    echo "Docs and notes alone: no review."
  fi
  echo
  # CI knows a landing's pull request by these words, and leaves its checks to the queue's run
  # (.github/workflows/ci.yml).
  echo "scripts/land.sh queues this for main's merge queue, whose run checks it."
}

open_pull_request() {
  local title body url
  title="$(git log --reverse --format=%s "$main_at..$head" | head -1)"
  body="$(mktemp)"
  describe >"$body"
  url="$(gh pr create --base main --head "$branch" --title "$title" --body-file "$body" 2>"$log")" || url=""
  rm -f "$body"
  if [ -z "$url" ]; then
    echo "Couldn't open the pull request:" >&2
    cat "$log" >&2
    exit 1
  fi
  echo "${url##*/}"
}

# Where a pull request stands, as seven words: its state (OPEN, MERGED, CLOSED), its place in
# the merge queue (QUEUED, AWAITING_CHECKS, MERGEABLE, UNMERGEABLE, LOCKED, or - when it isn't
# in it), the commit it merged as (or -), GitHub's id for it, whether someone has GitHub set to
# merge it by itself when it can (set, or -), whether it applies on main (MERGEABLE,
# CONFLICTING, or UNKNOWN while GitHub works it out), and the commit its branch is at.
standing() {
  gh api graphql -F owner="${repo%%/*}" -F name="${repo#*/}" -F number="$1" -f query='
    query($owner: String!, $name: String!, $number: Int!) {
      repository(owner: $owner, name: $name) {
        pullRequest(number: $number) {
          id
          state
          headRefOid
          mergeCommit { oid }
          mergeQueueEntry { state }
          autoMergeRequest { enabledAt }
          mergeable
        }
      }
    }' --jq '.data.repository.pullRequest | [.state, (.mergeQueueEntry.state // "-"), (.mergeCommit.oid // "-"), .id, (if .autoMergeRequest then "set" else "-" end), (.mergeable // "UNKNOWN"), .headRefOid] | join(" ")'
}

# The pull request's own run of CI on the commit being landed, as four words: its status, how
# it came out (or -), its id and its address; nothing while there's none yet. The run, and not
# the `gate` check by its name: a run started again is still the one run, where the checks of
# its first go stay listed until the new ones start. And this pull request's (`$1`, its
# number), which only the run's name says: ci.yml names a pull request's run for it, since a
# second pull request of this branch at this commit, into another branch than main, has runs
# of its own that nothing else tells apart.
own_run() {
  gh run list --workflow ci.yml --branch "$branch" --commit "$head" --event pull_request --limit 200 --json databaseId,status,conclusion,url,displayTitle \
    --jq "[.[] | select(.displayTitle | startswith(\"#$1 \"))][0] | select(. != null) | [.status, (if (.conclusion // \"\") == \"\" then \"-\" else .conclusion end), (.databaseId | tostring), .url] | join(\" \")" 2>"$log"
}

# A pull request that was there already is looked at before anything is pushed to it. One that
# someone has set to merge by itself would land whatever is pushed, on their say and not this
# landing's review; one that's in the queue as another commit is a landing under way.
may_take() {
  local number="$1"
  look "$number"
  [ "$state" = OPEN ] || die "Pull request #$number ($pr_url) is $state, so it isn't this landing's to take."
  if [ "$standing_request" = set ]; then
    echo "Pull request #$number ($pr_url) is set to merge by itself when it can, so whatever is pushed to it" >&2
    echo "would land on that say-so, checked but not as this landing. Nothing was pushed. Take that off, and run this again:" >&2
    die "  gh pr merge $number --disable-auto"
  fi
  if [ "$entry" != - ] && [ "$at" != "$head" ]; then
    die "Pull request #$number ($pr_url) is in main's merge queue as $(short "$at"), another commit than the one here. Nothing was pushed: wait for it to land, or take it out of the queue on its page, before landing this."
  fi
}

# The same as `standing`, asked until GitHub answers (a few times): into `state`, `entry`,
# `merged`, `id`, `standing_request`, `mergeable` and `at`. Nothing is done to a pull request
# on the strength of no answer.
look() {
  local line tries=0
  until line="$(standing "$1" 2>"$log")" && [ -n "$line" ]; do
    tries=$((tries + 1))
    if [ "$tries" -ge 5 ]; then
      cat "$log" >&2
      die "Couldn't read where pull request #$1 stands, so nothing was asked of it: run scripts/land.sh again." 3
    fi
    pause
  done
  read -r state entry merged id standing_request mergeable at <<EOF
$line
EOF
}

# Why the queue put a pull request out, as GitHub words it, when it has since this began, and
# nothing when it hasn't. Fails when GitHub doesn't answer, which isn't "it hasn't".
put_out() {
  gh api graphql -F owner="${repo%%/*}" -F name="${repo#*/}" -F number="$1" -f query='
    query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { pullRequest(number: $number) {
      timelineItems(last: 1, itemTypes: [REMOVED_FROM_MERGE_QUEUE_EVENT]) { nodes { ... on RemovedFromMergeQueueEvent { reason createdAt } } } } } }' \
    --jq ".data.repository.pullRequest.timelineItems.nodes[0] | select(. != null and .createdAt >= \"$since\") | .reason // \"it doesn't say why\"" 2>/dev/null
}

# The run of CI that failed for a pull request since this landing began: the queue's (its
# branch is named for the pull request), or the pull request's own.
failed_run() {
  gh run list --workflow ci.yml --limit 40 --json databaseId,headBranch,headSha,conclusion,url,createdAt \
    --jq "[.[] | select(.createdAt >= \"$since\") | select(.conclusion == \"failure\" or .conclusion == \"timed_out\" or .conclusion == \"startup_failure\" or .conclusion == \"cancelled\") | select((.headBranch | test(\"/pr-$1-\")) or .headSha == \"$head\")] | first | select(. != null) | (.databaseId | tostring) + \" \" + .url" 2>/dev/null
}

# What's said of a pull request that can't land as it is: the run that failed (`$2`, its id and
# address, when the caller has them; the one found otherwise), and that it's still the pull
# request.
say_failed() {
  local run="${2:-}"
  [ -n "$run" ] || run="$(failed_run "$1")"
  if [ -n "$run" ]; then
    echo "The run that failed: ${run#* }" >&2
    echo "Read why with: gh run view ${run%% *} --log-failed" >&2
  fi
  echo "It's still pull request #$1 ($pr_url): fix it on this branch, commit, and run scripts/land.sh again." >&2
}

# Puts the pull request in main's merge queue, as the commit pushed here and no other: a place
# in the queue is asked for by commit, where a standing request to merge the pull request when
# it can would go on to whatever was pushed after. Its own run of CI has to have passed first,
# which for a landing's pull request is moments (CI leaves its checks to the queue's run);
# nothing is asked of a pull request that isn't open at this commit. Once it's in the queue
# GitHub lets nobody push to its branch (.claude/rules/github.md), so what's queued is this
# commit until it lands or is put out.
queue_it() {
  local number="$1" said=0 began=$SECONDS run status conclusion run_id run_url looks=0 missing=0 quiet=0
  while :; do
    look "$number"
    if [ "$state" != OPEN ]; then
      rm -f "$sent"
      die "Pull request #$number ($pr_url) is $state, so it isn't this landing's to queue."
    fi
    if [ "$at" != "$head" ]; then
      rm -f "$sent"
      echo "Pull request #$number's branch is at $(short "$at"), not the commit this landing pushed ($(short "$head")):" >&2
      die "someone else pushed to it. Nothing was asked of it; look at it before landing again."
    fi
    # In the queue already, at this commit: a landing taken up again.
    [ "$entry" = - ] || return 0
    # Main moved under it since it was seen to apply: GitHub runs nothing on a pull request
    # that conflicts, so its own run of CI would be waited on for nothing.
    if [ "$mergeable" = CONFLICTING ]; then
      rm -f "$sent"
      die "Pull request #$number ($pr_url) no longer applies on main, which moved under it: $REBASE, resolve, rerun what you'd tested, and run scripts/land.sh again."
    fi
    if ! run="$(own_run "$number")"; then
      quiet=$((quiet + 1))
      if [ "$quiet" -ge 15 ]; then
        cat "$log" >&2
        die "Couldn't read CI's own run on pull request #$number ($pr_url) for a while, so it isn't queued: look at the repository's Actions, and run scripts/land.sh again." 3
      fi
    else
      quiet=0
      status="" conclusion="" run_id="" run_url=""
      read -r status conclusion run_id run_url <<EOF
$run
EOF
      if [ "$status" = completed ]; then
        [ "$conclusion" != success ] || break
        rm -f "$sent"
        echo "CI's own run on pull request #$number didn't pass ($conclusion), so it can't join the queue." >&2
        say_failed "$number" "$run_id $run_url"
        exit 1
      fi
      if [ -n "$status" ]; then
        missing=0
      else
        missing=$((missing + 1))
        # GitHub occasionally drops the pull_request event for a clean PR, so no run ever starts
        # (.claude/rules/github.md). Stop after 15 empty answers (~5 minutes, as check_on_github
        # does) rather than wait three hours; keep $sent so reopening and rerunning picks it up.
        if [ "$missing" -ge 15 ]; then
          die "CI's own run on pull request #$number ($pr_url) never showed up, so it isn't queued: look at the repository's Actions, or reopen the pull request (gh pr close $number && gh pr reopen $number) to start it, and run scripts/land.sh again." 3
        fi
      fi
    fi
    if [ "$said" = 0 ]; then
      echo "Pull request #$number ($pr_url): waiting for its own run of CI before it can join main's merge queue…"
      said=1
    fi
    if [ $((SECONDS - began)) -gt "${CODEALONG_LAND_WAIT:-10800}" ]; then
      die "Pull request #$number ($pr_url)'s own run of CI hasn't finished in three hours, so it isn't queued: look at the repository's Actions, and run scripts/land.sh again." 3
    fi
    pause
  done
  # A draft can't be queued.
  gh pr ready "$number" >/dev/null 2>&1 || true
  if ! gh api graphql -f pr="$id" -f head="$head" -f query='
    mutation($pr: ID!, $head: GitObjectID!) { enqueuePullRequest(input: {pullRequestId: $pr, expectedHeadOid: $head}) { mergeQueueEntry { id } } }' >"$said_no" 2>&1; then
    # No yes. GitHub's own no to the asking is an error of a kind that says so (what can't be
    # done, isn't allowed or isn't there): a refusal. Anything else, an error in answering
    # included, can be an answer lost on the way with the pull request queued or not, and
    # GitHub can take a moment to show what it was just asked: so where it stands is looked at
    # a few times, as the wait does, before anything is made of it. Queued as this commit, or
    # merged already, is a yes (whoever called this sees a merge through as any other;
    # `state` says which).
    while :; do
      look "$number"
      if [ "$state" = MERGED ] || { [ "$state" = OPEN ] && [ "$entry" != - ] && [ "$at" = "$head" ]; }; then
        return 0
      fi
      if grep -E '"type": ?"(UNPROCESSABLE|FORBIDDEN|NOT_FOUND|INSUFFICIENT_SCOPES)"' "$said_no" >/dev/null; then
        rm -f "$sent"
        echo "GitHub wouldn't queue pull request #$number ($pr_url):" >&2
        cat "$said_no" >&2
        echo "If that's a conflict with main: $REBASE, resolve, rerun what you'd tested, and run scripts/land.sh again." >&2
        exit 1
      fi
      looks=$((looks + 1))
      if [ "$looks" -ge 6 ]; then
        cat "$said_no" >&2
        die "GitHub didn't answer when asked to queue pull request #$number ($pr_url), and it isn't in the queue so far. It's still noted: run scripts/land.sh again, which sees where it stands and asks again if it has to." 3
      fi
      pause
    done
  fi
}

# Waits until the pull request has landed, and sets `landed` to the commit it landed as. Stops
# the script when it can't land as it is, and after three hours of a queue that hasn't got to
# it (it still lands then, without this: the note of it is kept, and running this again takes
# it up).
wait_landed() {
  local number="$1" was="" quiet=0 why began=$SECONDS line seen=0 unseen=0 unsaid=0 there=""
  while :; do
    if ! line="$(standing "$number" 2>"$log")" || [ -z "$line" ]; then
      # GitHub not answering for a moment isn't the landing's end; for five minutes it is ours.
      quiet=$((quiet + 1))
      if [ "$quiet" -ge 15 ]; then
        cat "$log" >&2
        die "GitHub hasn't answered for a while. The pull request is #$number ($pr_url): the queue lands it without this; run scripts/land.sh again to see where it stands." 3
      fi
      pause
      continue
    fi
    quiet=0
    read -r state entry merged id standing_request mergeable at <<EOF
$line
EOF
    case "$state" in
      MERGED)
        if [ "$at" != "$head" ]; then
          rm -f "$sent"
          echo "Pull request #$number ($pr_url) merged, but as $(short "$at"), not the commit this landing pushed ($(short "$head")):" >&2
          die "its branch was pushed to meanwhile. Look at what landed (git log origin/main) before anything else."
        fi
        landed="$merged"
        return 0
        ;;
      CLOSED)
        rm -f "$sent"
        die "Pull request #$number ($pr_url) was closed without landing."
        ;;
    esac
    if [ "$at" != "$head" ]; then
      rm -f "$sent"
      # Whoever pushed may have queued it themselves, which isn't this landing's to undo: said,
      # and left.
      [ "$entry" = - ] || there=" It's in main's merge queue: if that commit isn't meant to land, take it out on its page ($pr_url)."
      die "Pull request #$number's branch was pushed to while this waited (it's at $(short "$at") now), so it's no longer this landing's.$there"
    fi
    [ "$entry" = - ] || seen=1
    if [ "$entry" = - ]; then
      # No longer in the queue: it put the pull request out, or someone took it out. However
      # quickly that went by; but GitHub can take a moment to show what it was just asked, so
      # with no word of its being put out, one never yet seen queued is looked at a few times
      # more first.
      if ! why="$(put_out "$number")"; then
        unsaid=$((unsaid + 1))
        if [ "$unsaid" -ge 15 ]; then
          die "Pull request #$number ($pr_url) isn't in main's merge queue, and GitHub won't say whether it was put out. It's still noted: run scripts/land.sh again to see where it stands." 3
        fi
        pause
        continue
      fi
      if [ -z "$why" ] && [ "$seen" = 0 ] && [ "$unseen" -lt 6 ]; then
        unseen=$((unseen + 1))
        pause
        continue
      fi
      rm -f "$sent"
      if [ -n "$why" ]; then
        echo "The merge queue put pull request #$number out: $why." >&2
      else
        echo "Pull request #$number is no longer in main's merge queue: someone took it out, or it can't merge as it is." >&2
      fi
      say_failed "$number"
      exit 1
    fi
    if [ "$entry" != "$was" ]; then
      case "$entry" in
        AWAITING_CHECKS) echo "In the merge queue: its checks are running…" ;;
        QUEUED) echo "In the merge queue, behind others…" ;;
        MERGEABLE) echo "In the merge queue: its checks passed, and it merges once what's before it has…" ;;
        *) echo "In the merge queue ($entry)…" ;;
      esac
      was="$entry"
    fi
    if [ $((SECONDS - began)) -gt "${CODEALONG_LAND_WAIT:-10800}" ]; then
      die "Still waiting after three hours. The pull request is #$number ($pr_url): the queue lands it without this; run scripts/land.sh again to see where it stands." 3
    fi
    pause
  done
}

# `--check`: CI's whole run on the commit as it is here, without a pull request or a landing.
# It runs on a branch of this run's own (named for the commit and this process, and made only
# if there's none of that name), so a landing of this branch that's with GitHub is left as it
# is, and what's removed afterwards is what was made here.
check_on_github() {
  local on since run line status conclusion url tries=0 went began=$SECONDS
  on="check/$branch-$(short "$head")-$$"
  # Made here, as git says of this very branch (`*` for a new one): one already there under
  # that name, at this commit, would pass for pushed, and isn't this run's to remove. No tags
  # go with it, whose own lines would say `*` as well.
  if ! git push --porcelain --no-follow-tags --force-with-lease="refs/heads/$on:" origin "$head:refs/heads/$on" >"$log" 2>&1 ||
    ! grep -F -- "*"$'\t'"$head:refs/heads/$on"$'\t' "$log" >/dev/null; then
    cat "$log" >&2
    die "Couldn't make the branch $on on origin at $(short "$head"), for CI to run on."
  fi
  since="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  went=3
  if ! gh workflow run ci.yml --ref "$on" >/dev/null 2>"$log"; then
    cat "$log" >&2
    echo "Couldn't start CI on $on (gh workflow run ci.yml)." >&2
  else
    echo "CI is running on $(short "$head") (as $on)…"
    while :; do
      line="$(gh run list --workflow ci.yml --branch "$on" --event workflow_dispatch --limit 10 --json databaseId,headSha,createdAt,status,conclusion,url \
        --jq "[.[] | select(.headSha == \"$head\" and .createdAt >= \"$since\")] | first | select(. != null) | [(.databaseId | tostring), .status, (.conclusion // \"-\"), .url] | join(\" \")" 2>/dev/null)"
      if [ -z "$line" ]; then
        tries=$((tries + 1))
        if [ "$tries" -ge 15 ]; then
          echo "CI's run on $on never showed up: look at the repository's Actions." >&2
          break
        fi
        pause
        continue
      fi
      read -r run status conclusion url <<EOF
$line
EOF
      if [ "$status" = completed ]; then
        if [ "$conclusion" = success ]; then
          echo "Checked, not landed: $(short "$head") passed CI ($url)."
          went=0
        else
          echo "CI didn't pass on $(short "$head") ($conclusion): $url" >&2
          echo "Read why with: gh run view $run --log-failed" >&2
          went=1
        fi
        break
      fi
      if [ $((SECONDS - began)) -gt "${CODEALONG_LAND_WAIT:-10800}" ]; then
        echo "Still running after three hours: $url" >&2
        break
      fi
      pause
    done
  fi
  # The branch was only for the run, and is this run's own: made above, removed while it's
  # still this commit. No tags go with this push either: git picks them by the branch's old
  # commit even as it removes it.
  git push -q --no-follow-tags --force-with-lease="refs/heads/$on:$head" origin ":refs/heads/$on" 2>/dev/null || true
  exit "$went"
}

# Moves this checkout and its branch from `$1` to `$2`, a commit that has all `$1` has, with
# one command of git's: `git switch -C <the branch> <$2>`, which sets the branch that's checked
# out to that commit and the files with it, as a change of branch does. So the checks are
# git's own, made by the command that moves: what's uncommitted is carried along or it
# refuses; a merge or the like under way, it refuses; and with `--no-overwrite-ignore` it
# writes over no file it ignores (by default it would, and a file of the author's that main
# has begun to track, or to keep a directory where it is, would be gone with nothing kept of
# it). The branch stays the one checked out here throughout, so no other worktree can take it
# meanwhile. Its own commits are never written again or weighed as patches: they're left
# behind as landed, which is what's known of them.
#
# Whether it moved is read from where the checkout stands, not from what git returned (a hook
# can fail after the move). Git changes the files before the branch, so a move it couldn't
# finish (the branch locked, say) can leave main's changes staged on a branch that hasn't
# moved, which `why_not` says how to see. Nothing is done about a move that didn't happen:
# no second try, nothing put back.
move_to() {
  why_not=""
  git switch -q --no-overwrite-ignore -C "$branch" "$2" >"$log" 2>&1
  if ! still_here "$2"; then
    why_not="This checkout isn't at origin/main after git was asked to move it there. $(grep -v '^Aborting' "$log" | head -4 | tr -s '\t\n' '  ') Look at git status and git diff --cached before committing anything here: a move git began and couldn't finish leaves main's changes staged."
    return 1
  fi
}

# Brings this checkout and the main checkout up to main once the change is in it, and says
# where each stands. `$1` is the commit the branch was at, all of which is in `$2`, main's
# commit as it was fetched: landed there by the queue, or merging into it as nothing new. So
# the branch has nothing main lacks, and goes to `$2` (`move_to`). Where this checkout stands
# is read last, after main has been fetched for the last time (by catch-up-main.sh): on its
# branch, at origin/main as that is by then, with nothing uncommitted, or it isn't said to be
# at origin/main.
caught_up() {
  local here="" said now
  if ! still_here "$1"; then
    here="This checkout moved or changed while the landing waited, so it's left as it is: it isn't at origin/main."
  elif [ "$1" != "$2" ] && ! move_to "$1" "$2"; then
    here="$why_not"
  fi
  said="$(scripts/catch-up-main.sh)"
  if [ -z "$here" ]; then
    now="$(git rev-parse -q --verify "$MAIN")"
    # Main moved on while this ran: once more, from where the first move left this branch.
    if [ "$now" != "$2" ] && still_here "$2" && git merge-base --is-ancestor "$2" "$now"; then
      move_to "$2" "$now" || true
    fi
    if ! still_here "$now"; then
      here="${why_not:-This checkout isn't at origin/main ($(short "$now")): it has commits main doesn't, made here while this landed, or it changed meanwhile (git status; git log origin/main..HEAD).}"
    fi
  fi
  echo "$said${here:+ $here}"
  [ -z "$here" ]
}

# The end of a landing that's with GitHub: waited for, then this checkout and the main one
# caught up, and what happened said for the status line.
see_through() {
  local number="$1" at_main=1 where main_at=""
  landed=""
  wait_landed "$number"
  # Main as it's fetched now has to have what landed before anything here is moved onto it: a
  # main from before it would have the branch written again, as other commits than the ones
  # noted.
  fetch && main_at="$(git rev-parse -q --verify "$MAIN")"
  if [ -z "$main_at" ] || ! git merge-base --is-ancestor "$landed" "$main_at" 2>/dev/null; then
    echo "GitHub says pull request #$number ($pr_url) merged as $(short "$landed"), which origin/main here doesn't have:" >&2
    die "it couldn't be fetched, or isn't there. Nothing here was touched, and it's still noted as with GitHub: run scripts/land.sh again to see, or look (git fetch origin main; git log origin/main)." 3
  fi
  where="$(caught_up "$head" "$main_at")" || at_main=0
  # Landed, and seen to be: the note of it has done its work.
  rm -f "$sent"
  echo "Landed: $(short "$landed") is in origin/main. $where"
  echo "Checked on GitHub: $pr_url."
  if [ -n "$reviewed" ]; then
    echo "Reviewed by: $reviewed."
  elif [ -n "$unreviewed" ]; then
    echo "No review: $unreviewed."
  fi
  if [ "$at_main" = 1 ]; then
    echo "If what the app runs changed, relaunch it from here: scripts/relaunch.sh."
  else
    echo "If what the app runs changed, relaunch it from a checkout that's clean and at origin/main: scripts/relaunch.sh."
  fi
}

main() {
  # This file by its whole path, before the folder changes: run as ../scripts/land.sh, `$0`
  # names nothing from the checkout's top.
  self="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")" || exit 1
  cd "$(dirname "$self")/.." || exit 1

  reviewed=""
  unreviewed=""
  branch=""
  local check_only=0 no_wait=0 plan_only=0 plan_paths=()
  while [ $# -gt 0 ]; do
    case "$1" in
      --reviewed-by | --no-review)
        # Its value, and not the next option: `--reviewed-by --check` would have landed.
        if [ $# -lt 2 ] || [ -z "$2" ] || [ "${2#--}" != "$2" ]; then
          [ "$1" = --reviewed-by ] && die "--reviewed-by needs who reviewed it: the account and its model." 2
          die "--no-review needs why there was none." 2
        fi
        if [ "$1" = --reviewed-by ]; then reviewed="$2"; else unreviewed="$2"; fi
        shift 2
        ;;
      --check)
        check_only=1
        shift
        ;;
      --no-wait)
        no_wait=1
        shift
        ;;
      --plan)
        plan_only=1
        shift
        while [ $# -gt 0 ]; do
          plan_paths+=("$1")
          shift
        done
        ;;
      -h | --help)
        usage
        exit 0
        ;;
      *) die "Unknown argument $1; see scripts/land.sh --help." 2 ;;
    esac
  done

  if [ "$plan_only" = 1 ] && [ ${#plan_paths[@]} -gt 0 ]; then
    printf '%s\0' "${plan_paths[@]}" | plan_of
    exit
  fi

  git rev-parse --git-dir >/dev/null 2>&1 || die "Not in a git checkout." 2
  log="$(mktemp)"
  list="$(mktemp)"
  said_no="$(mktemp)"
  trap 'rm -f "$log" "$list" "$said_no"; let_go' EXIT
  if [ "$plan_only" = 1 ]; then
    changed || die "Couldn't list what this branch changes."
    plan_of <"$list"
    exit
  fi

  # What's landed is what was here when this was asked, read before anything else is: the
  # waits are long, and in a checkout that isn't ours alone another session can commit or
  # change branch meanwhile, which would go out under this one's review.
  # A pull request is of a branch, and main isn't one to land from. The branch is read first
  # and the commit from the branch, so the two are one branch's: by its whole name, since the
  # short one git gives for a branch that shares its name with a tag is another
  # (heads/<name>), and would be pushed as that.
  local asked_for
  branch="$(git symbolic-ref -q HEAD)" || die "This checkout is on no branch, and a landing is a branch's: git switch -c <topic>, then run this again."
  asked_for="$(git rev-parse -q --verify "$branch^{commit}")" || die "Couldn't read this checkout's branch."
  branch="${branch#refs/heads/}"
  clean_or_die
  if [ -d "$(git rev-parse --git-path rebase-merge)" ] || [ -d "$(git rev-parse --git-path rebase-apply)" ]; then
    die "A rebase is under way here: finish it (git rebase --continue) and run this again."
  fi
  # Nor a merge, a cherry-pick, a revert or a bisect: one can be under way with nothing
  # uncommitted to show for it, and the move to main at the end would be the end of it.
  # (`sequencer` is a run of cherry-picks or reverts with some still to go, which is there
  # between two of them with nothing else to say so.)
  local under_way
  for under_way in MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD BISECT_LOG sequencer; do
    [ ! -e "$(git rev-parse --git-path "$under_way")" ] || die "Something is under way in this checkout ($under_way): finish it or give it up (git status says how), and run this again."
  done
  command -v gh >/dev/null || die "gh isn't here, which is how this reaches GitHub: https://cli.github.com."
  [ "$branch" != main ] || die "This checkout is on main, and a landing is a branch's of its own (CLAUDE.md, *Landing work*): git switch -c <topic>, then run this again."
  # What this keeps of a branch's landings, in the repository's .git: the turn, and the note of
  # a landing that's with GitHub.
  notes="$(git rev-parse --path-format=absolute --git-common-dir)/codealong-landing" || die "Not in a git checkout." 2
  # Named for the branch by a hash of its whole name: fix/menu and fix_menu are two branches.
  local name
  name="$(printf %s "$branch" | git hash-object --stdin)" || die "Couldn't name this branch's notes."
  turn="$notes/$name.turn"
  sent="$notes/$name.sent"
  # Started by the land.sh from before 2026-11-20, as main's own copy after its rebase: that
  # one's turn was the whole repository's, kept for as long as this process lives, which is now
  # the queue's wait. Let go, or every landing still made with that script waits behind this.
  # (To go once no worktree has the old script.)
  if [ "${CODEALONG_LANDING_HELD:-}" = "$$" ] && [ -n "${CODEALONG_LANDING_HOLDER:-}" ]; then
    kill "$CODEALONG_LANDING_HOLDER" 2>/dev/null
  fi
  take_turn
  which_repository

  # The commit to land is the one asked for, all the way: nothing here rewrites it, so there's
  # no moment in which what another session commits in this checkout could take its place.
  head="$asked_for"
  [ "$check_only" = 0 ] || check_on_github

  # A landing of this very commit that's already with GitHub (sent with --no-wait, or by a run
  # of this that was stopped or gave up waiting) is taken up where it got to, and nothing is
  # pushed under it.
  local number wants noted noted_branch why ahead nothing=0
  if [ -f "$sent" ]; then
    read -r number noted since noted_branch <"$sent"
    if [ "$noted" = "$head" ] && [ -n "$number" ] && [ "$noted_branch" = "$branch" ]; then
      pr_url="https://github.com/$repo/pull/$number"
      look "$number"
      if [ "$state" = OPEN ] && [ "$at" = "$head" ] && [ "$entry" = - ]; then
        # Not in the queue. Put out of it meanwhile, by the queue or by someone, which the
        # wait says with why; or never yet in it, this having stopped while the pull request's
        # own run of CI was still going, and it's asked for now. Only GitHub's answer tells the
        # two apart: with none, nothing is asked.
        why="$(put_out "$number")" || die "GitHub wouldn't say whether pull request #$number ($pr_url) was put out of the queue, so nothing was asked of it. It's still noted: run scripts/land.sh again." 3
        if [ -z "$why" ]; then
          queue_it "$number"
          [ "$state" != OPEN ] || entry=QUEUED
        fi
      fi
      if [ "$no_wait" = 1 ] && [ "$state" = OPEN ] && [ "$at" = "$head" ] && [ "$entry" != - ]; then
        echo "Queued still: pull request #$number ($pr_url) lands $(short "$head") when main's merge queue has checked it."
        exit 0
      fi
      see_through "$number"
      exit 0
    fi
    # The branch has moved on since: what's here is a new landing.
    rm -f "$sent"
  fi

  fetch || die "Couldn't fetch origin/main, so there's no telling whether this is in it or applies on it."
  still_here "$head" || die "This checkout moved or changed while origin/main was fetched (it was $branch at $(short "$head")): look at what's here now, and run scripts/land.sh again."
  refuse_old_history
  # Main as fetched, read once: another worktree's fetch moves origin/main, and what's found of
  # one main mustn't be acted on against another.
  main_at="$(git rev-parse -q --verify "$MAIN")" || die "Couldn't read origin/main."

  # What merging it into main as main is now would make, asked without touching the checkout:
  # the queue writes the commits on top of main itself. Nothing new, and it's in main already:
  # never ahead of it, or landed before as the copies the queue wrote, and all that's left is
  # to catch up. That's asked of what the files would hold and not of the commits' patches,
  # which git compares without their whitespace, so that a change that differs only by that
  # from one in main is still a change to land. A conflict is yours to rebase and resolve. (A
  # git too old to say leaves that to GitHub, which won't queue a pull request that conflicts.)
  if git merge-base --is-ancestor "$head" "$main_at"; then
    nothing=1
  elif git merge-tree --write-tree "$main_at" "$head" >"$log" 2>/dev/null; then
    [ "$(head -1 "$log")" != "$(git rev-parse "$main_at^{tree}")" ] || nothing=1
  elif grep '^CONFLICT' "$log" >/dev/null; then
    echo "This branch doesn't apply on origin/main as it is:" >&2
    grep '^CONFLICT' "$log" | head -10 >&2
    die "Rebase it ($REBASE), resolve, rerun what you'd tested, and run scripts/land.sh again."
  fi
  if [ "$nothing" = 1 ]; then
    echo "Nothing to land: origin/main ($(short "$main_at")) already has all this branch holds. $(caught_up "$head" "$main_at")"
    exit 0
  fi
  # Some of its commits in main already, by their patches, under ones that aren't: a rebase
  # drops those, where the queue would be given them to write again. Only a reason to rebase,
  # which is yours to do and to look at; with every commit written again, so that one git
  # takes for main's by its patch and isn't (their whitespace aside) shows as the conflict it
  # is, and the ones that are come to nothing and drop out.
  ahead="$(git cherry "$main_at" "$head" 2>"$log")" || { cat "$log" >&2; die "Couldn't compare this branch with origin/main."; }
  case "$ahead" in
    *-*) die "Some of this branch's commits look to be in origin/main already, as the queue wrote them, under ones that aren't: $REBASE, where those come to nothing and drop out, and run scripts/land.sh again." ;;
  esac

  changed "$head" "$main_at" || die "Couldn't list what these commits change, so there's no telling whether they need a review."
  wants="$(plan_of <"$list")" || die "Couldn't tell what these commits call for (scripts/land.sh --plan)."
  case " $wants " in
    *" code "*)
      if [ -z "$reviewed$unreviewed" ]; then
        echo "These commits change code, which another provider's agent reviews before it lands" >&2
        echo "(CLAUDE.md, *Review before pushing*). Say who did: --reviewed-by \"<account>, <model>\"," >&2
        die "or, when none could be had, --no-review \"<why>\"." 2
      fi
      ;;
  esac

  # Without the queue nothing would check it: CI leaves a landing's pull request to the queue's
  # run.
  local queue
  queue="$(gh api graphql -F owner="${repo%%/*}" -F name="${repo#*/}" -f query='query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { mergeQueue(branch: "main") { id } } }' --jq '.data.repository.mergeQueue.id // empty' 2>"$log")" || { cat "$log" >&2; die "Couldn't ask GitHub for main's merge queue."; }
  [ -n "$queue" ] || die "main has no merge queue on $repo, so nothing would check this before it merged: docs/development.md, *Keeping main working*, says how it's set up."

  number="$(pull_request)" || exit 1
  if [ -n "$number" ]; then
    pr_url="https://github.com/$repo/pull/$number"
    may_take "$number"
  fi
  still_here "$head" || die "This checkout moved or changed before the branch was pushed: look at what's here now, and run scripts/land.sh again."
  push_branch
  if [ -z "$number" ]; then
    number="$(open_pull_request)" || exit 1
  fi
  pr_url="https://github.com/$repo/pull/$number"
  since="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  # Noted before it's queued, so a landing that reached GitHub is always one this can take up
  # again; taken back when it turns out not to have been queued.
  echo "$number $head $since $branch" >"$sent" || die "Couldn't note this landing ($sent), so it isn't started: nothing was queued."
  queue_it "$number"

  # Queued, and left to GitHub; unless it was seen merged already, which is seen through.
  if [ "$no_wait" = 1 ] && [ "$state" = OPEN ]; then
    echo "Queued: pull request #$number ($pr_url) lands $(git rev-parse --short "$head") when main's merge queue has checked it."
    echo "Run scripts/land.sh again to see whether it has, and to bring this branch and the main checkout up to it."
    exit 0
  fi
  see_through "$number"
}

main "$@"
exit

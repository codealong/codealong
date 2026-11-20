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
#   scripts/land.sh --no-wait …                           queue it and come back: GitHub lands it
#   scripts/land.sh --check                               the checks alone, on GitHub: no landing
#   scripts/land.sh --plan [path…]                        only says which checks those paths (or
#                                                         this branch's commits) call for
#
# In order:
#   1. It fetches, and rebases the branch onto origin/main. A conflict stops it there with the
#      rebase under way: finish it (`git rebase --continue`) and run this again. A branch from
#      before the history was rewritten is turned away, with the rebase CLAUDE.md gives for it.
#   2. It pushes the branch to origin under its own name, over what this checkout pushed there
#      before and over nothing else.
#   3. It opens the branch's pull request (or takes the one that's open), labelled `land`, which
#      tells CI that the queue's run is the one that counts, and asks GitHub to merge it when
#      it can. The queue writes the commits again on top of main (a rebase, no merge commit).
#   4. It waits for the queue, saying where the change stands, and stops when it has landed or
#      was put out of the queue: a check failed (it says which run, and how to read it) or it
#      no longer applies on main. It's still the pull request then: fix it on this branch,
#      commit, and run this again.
#   5. It brings this branch and the main checkout up to origin/main (catch-up-main.sh) and
#      prints the commit and where things stand, for the status line. Relaunching is still
#      yours: scripts/relaunch.sh, when what the app runs changed.
#
# It lands only what's committed: uncommitted changes here, yours or another session's, stop it
# before anything is touched. There is no flag to skip the checks, and none to run them here:
# `npm test` and `cargo test` are still yours to run while you work.
set -uo pipefail

# Everything is in functions, read whole before any of it runs: the rebase can rewrite this
# file, and bash reads a script as it goes.

usage() {
  sed -n '2,/^set -uo/p' "$self" | sed '$d; s/^# \{0,1\}//'
}

# Which checks a changed file calls for, one word to a line: `ui` (build, lint, unit tests),
# `engine` (cargo test, on macOS and on Linux), `checks` (the browser checks on the made-up Mac),
# `code` (it needs a review) and `install` (what's installed changed). CI asks this table
# (.github/workflows/ci.yml, through --plan), so it's the one place that says. The lists err
# towards running a check: each side's tests read some of the other's files, named here with
# what reads them.
calls_for() {
  case "$1" in
    # Docs nothing tests or builds. CLAUDE.md, the rules and the skills aren't among them.
    docs/* | design/*.md | README.md | CONTRIBUTING.md | SECURITY.md | .github/*.md | .github/CODEOWNERS | .claude/skills/*.md) ;;
    # Words the engine's build or tests hold: CLAUDE.md's size and its list of the rules files
    # (lib.rs), the snapshot modes (snapshot.rs), the command's reference and the plugin's copies
    # of the skills (cli.rs, skills.rs, install.rs), and the files the bundle copies
    # (tauri.conf.json `resources`). Words, so no review.
    CLAUDE.md | AGENTS.md | LICENSE | NOTICE | THIRD_PARTY_NOTICES.md | .claude/rules/* | skills/*.md | plugin/*.md)
      echo engine
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
  local path word wants="" out=""
  while IFS= read -r -d '' path; do
    [ -n "$path" ] || continue
    wants="$wants $(calls_for "$path" | tr '\n' ' ')"
  done
  for word in ui engine checks code install; do
    case " $wants " in *" $word "*) out="$out $word" ;; esac
  done
  echo "${out# }"
}

# The files this branch's commits change, into the file `$list`: both names of one that moved
# (a rename shows only where it went, and the checks its old place called for would be skipped),
# each as it is with a NUL after it. Fails when git can't say, so that no answer isn't taken for
# no changes.
changed() {
  git diff --no-renames --name-only -z "origin/main...${1:-HEAD}" >"$list"
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
    if [ -n "$base" ] && ! git merge-base --is-ancestor "$base" origin/main; then
      echo "This branch stands on commits from before the history was rewritten ($ref)." >&2
      echo "Move your own commits onto origin/main first, as CLAUDE.md says:" >&2
      die "  git rebase --onto origin/main \$(git merge-base HEAD $ref)"
    fi
  done
}

short() { git rev-parse --short "$1" 2>/dev/null || echo "${1:0:8}"; }

# Waits between two looks at GitHub: twenty seconds, or what a test asks for.
pause() { sleep "${CODEALONG_LAND_POLL:-20}"; }

# Pushes the branch to origin under its own name. What's there already is replaced only when
# it's this checkout's own earlier push (a commit this branch has been at, which a rebase since
# has rewritten): anything else is someone's work this would throw away.
push_branch() {
  local there
  there="$(git ls-remote origin "refs/heads/$branch" 2>"$log" | cut -f1)" || { cat "$log" >&2; die "Couldn't ask origin for its $branch."; }
  [ "$there" = "$head" ] && return 0
  if [ -n "$there" ]; then
    # Read whole before it's searched: a search that stops at its first match would end the
    # list's writer early, which `pipefail` takes for a failure.
    case "$(git reflog show --format=%H "$branch" 2>/dev/null)" in
      *"$there"*) ;;
      *)
        echo "origin has a $branch already, at $(short "$there"), where this checkout's $branch never was:" >&2
        die "someone else pushed it. Look at it (git fetch origin $branch) before landing over it, or land from a branch with another name."
        ;;
    esac
  fi
  if ! git push -q --force-with-lease="refs/heads/$branch:$there" origin "$head:refs/heads/$branch" 2>"$log"; then
    echo "The push of $branch was turned down:" >&2
    cat "$log" >&2
    exit 1
  fi
}

# The pull request this branch lands as: the one open for it, or a new one. Labelled `land`
# before anything is pushed to it, so its own run of CI leaves the checks to the queue's.
pull_request() {
  local number
  number="$(gh pr list --head "$branch" --base main --state open --json number --jq '.[0].number // empty' 2>"$log")" || { cat "$log" >&2; die "Couldn't ask GitHub for this branch's pull request (gh)."; }
  if [ -n "$number" ]; then
    gh pr edit "$number" --add-label land >/dev/null 2>"$log" || { cat "$log" >&2; die "Couldn't label pull request #$number."; }
    echo "$number"
    return 0
  fi
  echo ""
}

# What the pull request says: the commits, and who reviewed them.
describe() {
  git log --reverse --format='- %s' "origin/main..$head"
  echo
  if [ -n "$reviewed" ]; then
    echo "Reviewed by: $reviewed."
  elif [ -n "$unreviewed" ]; then
    echo "No review: $unreviewed."
  else
    echo "Docs and notes alone: no review."
  fi
  echo
  echo "Landed with \`scripts/land.sh\`: main's merge queue runs the checks and merges it."
}

open_pull_request() {
  local title body url
  title="$(git log --reverse --format=%s "origin/main..$head" | head -1)"
  body="$(mktemp)"
  describe >"$body"
  if ! url="$(gh pr create --base main --head "$branch" --title "$title" --body-file "$body" --label land 2>"$log")"; then
    # The label is made the first time it's wanted.
    if grep -q -i "label" "$log" && gh label create land --description "Opened by scripts/land.sh: the merge queue's run of CI is the one that counts" --color 1f883d >/dev/null 2>&1; then
      url="$(gh pr create --base main --head "$branch" --title "$title" --body-file "$body" --label land 2>"$log")" || url=""
    else
      url=""
    fi
  fi
  rm -f "$body"
  if [ -z "$url" ]; then
    echo "Couldn't open the pull request:" >&2
    cat "$log" >&2
    exit 1
  fi
  echo "${url##*/}"
}

# Where a pull request stands, as four words: its state (OPEN, MERGED, CLOSED), its place in
# the merge queue (QUEUED, AWAITING_CHECKS, MERGEABLE, UNMERGEABLE, LOCKED, or - when it isn't
# in it), the commit it merged as (or -), and how `gate` came out on its own head (or -).
standing() {
  gh api graphql -F owner="${repo%%/*}" -F name="${repo#*/}" -F number="$1" -f query='
    query($owner: String!, $name: String!, $number: Int!) {
      repository(owner: $owner, name: $name) {
        pullRequest(number: $number) {
          state
          mergeCommit { oid }
          mergeQueueEntry { state }
          commits(last: 1) { nodes { commit { statusCheckRollup { contexts(first: 100) { nodes { ... on CheckRun { name conclusion } } } } } } }
        }
      }
    }' --jq '.data.repository.pullRequest | [.state, (.mergeQueueEntry.state // "-"), (.mergeCommit.oid // "-"), ([.commits.nodes[0].commit.statusCheckRollup.contexts.nodes[]? | select(.name == "gate") | .conclusion // "PENDING"] | last // "-")] | join(" ")'
}

# The run of CI that failed for a pull request: the queue's (its branch is named for the pull
# request), or the pull request's own.
failed_run() {
  gh run list --workflow ci.yml --limit 40 --json databaseId,headBranch,headSha,conclusion,url \
    --jq "[.[] | select(.conclusion == \"failure\" or .conclusion == \"timed_out\" or .conclusion == \"startup_failure\") | select((.headBranch | test(\"/pr-$1-\")) or .headSha == \"$head\")] | first | select(. != null) | (.databaseId | tostring) + \" \" + .url" 2>/dev/null
}

say_failed() {
  local run
  run="$(failed_run "$1")"
  if [ -n "$run" ]; then
    echo "The run that failed: ${run#* }" >&2
    echo "Read why with: gh run view ${run%% *} --log-failed" >&2
  fi
  echo "It's still pull request #$1 ($pr_url): fix it on this branch, commit, and run scripts/land.sh again." >&2
}

# Waits until the pull request has landed, and sets `landed` to the commit it landed as. Stops
# the script when it can't land as it is.
wait_landed() {
  local number="$1" line state entry merged gate queued=0 was="" quiet=0 why
  while :; do
    if ! line="$(standing "$number" 2>"$log")" || [ -z "$line" ]; then
      # GitHub not answering for a moment isn't the landing's end; for five minutes it is ours.
      quiet=$((quiet + 1))
      if [ "$quiet" -ge 15 ]; then
        cat "$log" >&2
        die "GitHub hasn't answered for a while. The pull request is #$number ($pr_url): the queue lands it without this; run scripts/land.sh again to see where it stands."
      fi
      pause
      continue
    fi
    quiet=0
    read -r state entry merged gate <<EOF
$line
EOF
    case "$state" in
      MERGED)
        landed="$merged"
        return 0
        ;;
      CLOSED) die "Pull request #$number ($pr_url) was closed without landing." ;;
    esac
    if [ "$entry" != - ]; then
      queued=1
    elif [ "$queued" = 1 ]; then
      why="$(gh api graphql -F owner="${repo%%/*}" -F name="${repo#*/}" -F number="$number" -f query='
        query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { pullRequest(number: $number) {
          timelineItems(last: 1, itemTypes: [REMOVED_FROM_MERGE_QUEUE_EVENT]) { nodes { ... on RemovedFromMergeQueueEvent { reason } } } } } }' \
        --jq '.data.repository.pullRequest.timelineItems.nodes[0].reason // empty' 2>/dev/null)"
      echo "The merge queue put pull request #$number out${why:+: $why}." >&2
      say_failed "$number"
      exit 1
    else
      case "$gate" in
        FAILURE | TIMED_OUT | CANCELLED | STARTUP_FAILURE | ACTION_REQUIRED)
          echo "CI's own run on pull request #$number didn't pass ($gate), so it can't join the queue." >&2
          say_failed "$number"
          exit 1
          ;;
      esac
    fi
    if [ "$entry" != "$was" ]; then
      case "$entry" in
        -) echo "Pull request #$number ($pr_url) is waiting to join main's merge queue…" ;;
        AWAITING_CHECKS) echo "In the merge queue: its checks are running…" ;;
        QUEUED) echo "In the merge queue, behind others…" ;;
        MERGEABLE) echo "In the merge queue: its checks passed, and it merges once what's before it has…" ;;
        *) echo "In the merge queue ($entry)…" ;;
      esac
      was="$entry"
    fi
    pause
  done
}

# `--check`: CI's whole run on the branch as pushed, without a pull request or a landing.
check_on_github() {
  local since run line status conclusion url tries=0
  since="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  gh workflow run ci.yml --ref "$branch" >/dev/null 2>"$log" || { cat "$log" >&2; die "Couldn't start CI on $branch (gh workflow run ci.yml)."; }
  echo "CI is running on $branch at $(short "$head")…"
  while :; do
    line="$(gh run list --workflow ci.yml --branch "$branch" --event workflow_dispatch --limit 10 --json databaseId,headSha,createdAt,status,conclusion,url \
      --jq "[.[] | select(.headSha == \"$head\" and .createdAt >= \"$since\")] | first | select(. != null) | [(.databaseId | tostring), .status, (.conclusion // \"-\"), .url] | join(\" \")" 2>/dev/null)"
    if [ -z "$line" ]; then
      tries=$((tries + 1))
      [ "$tries" -lt 15 ] || die "CI's run on $branch never showed up: look at the repository's Actions."
      pause
      continue
    fi
    read -r run status conclusion url <<EOF
$line
EOF
    if [ "$status" = completed ]; then
      if [ "$conclusion" = success ]; then
        echo "Checked, not landed: $(short "$head") passed CI ($url)."
        exit 0
      fi
      echo "CI didn't pass on $(short "$head") ($conclusion): $url" >&2
      die "Read why with: gh run view $run --log-failed"
    fi
    pause
  done
}

main() {
  # This file by its whole path, before the folder changes: run as ../scripts/land.sh, `$0`
  # names nothing from the checkout's top.
  self="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")" || exit 1
  cd "$(dirname "$self")/.." || exit 1

  reviewed=""
  unreviewed=""
  local check_only=0 no_wait=0 plan_only=0 plan_paths=() again=("$@")
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
    exit 0
  fi

  git rev-parse --git-dir >/dev/null 2>&1 || die "Not in a git checkout." 2
  log="$(mktemp)"
  list="$(mktemp)"
  trap 'rm -f "$log" "$list"' EXIT
  if [ "$plan_only" = 1 ]; then
    changed || die "Couldn't list what this branch changes."
    plan_of <"$list"
    exit 0
  fi

  clean_or_die
  if [ -d "$(git rev-parse --git-path rebase-merge)" ] || [ -d "$(git rev-parse --git-path rebase-apply)" ]; then
    die "A rebase is under way here: finish it (git rebase --continue) and run this again."
  fi
  command -v gh >/dev/null || die "gh isn't here, which is how this reaches GitHub: https://cli.github.com."
  # A pull request is of a branch, and main isn't one to land from.
  branch="$(git symbolic-ref -q --short HEAD)" || die "This checkout is on no branch, and a landing is a branch's: git switch -c <topic>, then run this again."
  [ "$branch" != main ] || die "This checkout is on main, and a landing is a branch's of its own (CLAUDE.md, *Landing work*): git switch -c <topic>, then run this again."

  fetch || die "Couldn't fetch origin/main, so there's nothing to rebase onto."
  if git merge-base --is-ancestor HEAD origin/main; then
    echo "Nothing to land: origin/main ($(git rev-parse --short origin/main)) already has this branch's commits."
    exit 0
  fi
  local itself now
  if ! git merge-base --is-ancestor origin/main HEAD; then
    refuse_old_history
    itself="$(git hash-object "$self")" || die "Couldn't read $self."
    if ! git rebase -q origin/main >"$log" 2>&1; then
      echo "The rebase onto origin/main stopped on a conflict:" >&2
      git status --short | grep -E '^(UU|AA|DU|UD|AU|UA|DD) ' | head -20 >&2
      die "Resolve it, git rebase --continue, rerun what you'd tested, and run scripts/land.sh again."
    fi
    # main's own land.sh came in with the rebase: it's the one to land with, and bash has this
    # one in memory. Start again as it.
    now="$(git hash-object "$self")" || die "Couldn't read $self after the rebase."
    if [ "$now" != "$itself" ]; then
      rm -f "$log" "$list"
      exec "$self" ${again[@]+"${again[@]}"}
    fi
  fi
  head="$(git rev-parse HEAD)" || die "Couldn't read this checkout's HEAD."

  local wants
  changed "$head" || die "Couldn't list what these commits change, so there's no telling whether they need a review."
  wants="$(plan_of <"$list")"

  if [ "$check_only" = 1 ]; then
    push_branch
    check_on_github
  fi

  case " $wants " in
    *" code "*)
      if [ -z "$reviewed$unreviewed" ]; then
        echo "These commits change code, which another provider's agent reviews before it lands" >&2
        echo "(CLAUDE.md, *Review before pushing*). Say who did: --reviewed-by \"<account>, <model>\"," >&2
        die "or, when none could be had, --no-review \"<why>\"." 2
      fi
      ;;
  esac

  repo="$(gh repo view --json nameWithOwner --jq .nameWithOwner 2>"$log")" || { cat "$log" >&2; die "Couldn't ask GitHub which repository this is (gh): is it signed in? gh auth status."; }
  # Without the queue, asking GitHub to merge would merge at once, with no check run.
  local queue
  queue="$(gh api graphql -F owner="${repo%%/*}" -F name="${repo#*/}" -f query='query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { mergeQueue(branch: "main") { id } } }' --jq '.data.repository.mergeQueue.id // empty' 2>"$log")" || { cat "$log" >&2; die "Couldn't ask GitHub for main's merge queue."; }
  [ -n "$queue" ] || die "main has no merge queue on $repo, so nothing would check this before it merged: docs/development.md, *Keeping main working*, says how it's set up."

  # An open pull request is labelled before the push, a new one opened after it: either way
  # the label is there when CI's run on the pull request starts.
  local number
  number="$(pull_request)" || exit 1
  push_branch
  if [ -z "$number" ]; then
    number="$(open_pull_request)" || exit 1
  fi
  pr_url="https://github.com/$repo/pull/$number"
  # A draft can't merge.
  gh pr ready "$number" >/dev/null 2>&1 || true
  # Merged when it can be, and only as the commit pushed here.
  if ! gh pr merge "$number" --auto --match-head-commit "$head" >"$log" 2>&1; then
    echo "GitHub wouldn't queue pull request #$number ($pr_url):" >&2
    cat "$log" >&2
    exit 1
  fi

  if [ "$no_wait" = 1 ]; then
    echo "Queued: pull request #$number ($pr_url) lands $(git rev-parse --short "$head") when main's merge queue has checked it."
    echo "Run scripts/land.sh again to see whether it has, and to bring this branch and the main checkout up to it."
    exit 0
  fi

  landed=""
  wait_landed "$number"

  fetch || true
  # The queue wrote these commits again on top of main, so the ones here are copies of what
  # landed: rebased onto origin/main they drop out, and the branch is where main is.
  if [ "$(git rev-parse HEAD)" = "$head" ] && [ -z "$(git status --porcelain --untracked-files=no)" ]; then
    git rebase -q origin/main >"$log" 2>&1 || git rebase --abort >/dev/null 2>&1
  fi
  # GitHub removes a merged branch when the repository is set to; this is for when it isn't.
  git push -q origin --delete "$branch" >/dev/null 2>&1 || true
  echo "Landed: $(short "$landed") is in origin/main. $(scripts/catch-up-main.sh)"
  echo "Checked on GitHub: $pr_url."
  if [ -n "$reviewed" ]; then
    echo "Reviewed by: $reviewed."
  elif [ -n "$unreviewed" ]; then
    echo "No review: $unreviewed."
  fi
  case " $wants " in
    *" ui "* | *" engine "*) echo "If what the app runs changed, relaunch it from here: scripts/relaunch.sh." ;;
  esac
}

main "$@"
exit

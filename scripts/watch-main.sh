#!/usr/bin/env bash
# What a Codealong schedule watches so that a broken main is somebody's to fix (docs/development.md,
# *Keeping main working*): prints a line for each recent run on main that failed, of CI or of
# the browser checks, as a JSON object a watch takes for one thing (its `id`, a `title`, its `url`, some
# `text`). A watch starts its session on a line it hasn't printed before, so each failed run is
# told once, and a run that passes prints nothing. Failed is every way a finished run can be
# red: a step that failed, a run that timed out, one that couldn't start. A watch's first look
# only notes what's there, so a run already red when the schedule is made isn't told.
#
#   codealong schedule add "main is red" --watch scripts/watch-main.sh --every 15m --thread \
#     --fresh-after 1d --worktree <the main checkout> --prompt-file <what the session is told>
#
# The schedule's worktree is the main checkout, which stays: the script's path is taken from
# there, and a session's own worktree is removed once its work has landed.
#
# A change reaches main through its merge queue, whose run of CI (.github/workflows/ci.yml) it
# has to pass first, so CI's own failing runs on main are of what got there another way (a
# release's version commit, an admin's push) and the daily run of everything, which a toolchain or
# a runner's image that moved can fail with no commit to blame. (Merge-queue pushes may run
# non-failing Rust cache-warm builds on main, which `gate` lets through with a warning.) The
# browser checks
# (.github/workflows/checks.yml) run on main after what lands, so theirs is the run a landing
# can turn red. One that was cancelled only made way for a later one. Before there was a watch,
# a failed run emailed whoever last changed the workflow, and main stayed red until they read
# it: eight hours on 2026-11-18, with 83 commits landed on top.
#
#
# Asked a workflow, a way of starting and a way of failing at a time, the last ten of each: a
# list of every run on main is mostly the queue's own pushes and runs started by hand, and a
# browser run that took an hour to fail would be past its end by the time it did. A browser
# run checks main as it was when the run started, which its plan says and needn't be the
# commit it's listed under.
set -euo pipefail
for workflow in ci.yml checks.yml; do
  for event in push schedule; do
    for how in failure timed_out startup_failure; do
      gh run list --workflow "$workflow" --branch main --event "$event" --status "$how" --limit 10 \
        --json databaseId,headSha,url,createdAt,conclusion,event,workflowName \
        --jq '.[] | {id: (.databaseId | tostring), title: (.workflowName + " failed on main at " + .headSha[0:8]), url: .url, text: ("Run " + (.databaseId | tostring) + " of " + .workflowName + " (" + .conclusion + ", " + .event + "), started " + .createdAt + " by commit " + .headSha)}'
    done
  done
done

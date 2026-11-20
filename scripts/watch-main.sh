#!/usr/bin/env bash
# What a Codealong schedule watches so that a broken main is somebody's to fix (docs/development.md,
# *Keeping main working*): prints a line for each recent run of CI on main that failed, as a
# JSON object a watch takes for one thing (its `id`, a `title`, its `url`, some
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
# has to pass first, so main's own runs are of what got there another way (a release's version
# commit, an admin's push) and the daily run of everything, which a toolchain or a runner's
# image that moved can fail with no commit to blame. Before there was a watch, a failed run
# emailed whoever last changed the workflow, and main stayed red until they read it: eight hours
# on 2026-11-18, with 83 commits landed on top.
set -euo pipefail
gh run list --workflow ci.yml --branch main --status completed --limit 20 \
  --json databaseId,headSha,url,createdAt,conclusion,event \
  --jq '.[] | select(.event == "push" or .event == "schedule") | select(.conclusion == "failure" or .conclusion == "timed_out" or .conclusion == "startup_failure") | {id: (.databaseId | tostring), title: ("CI failed on main at " + .headSha[0:8]), url: .url, text: ("Run " + (.databaseId | tostring) + " (" + .conclusion + ", " + .event + "), started " + .createdAt + ", at commit " + .headSha)}'

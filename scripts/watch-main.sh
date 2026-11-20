#!/usr/bin/env bash
# What a Codealong schedule watches so that a broken main is somebody's to fix (docs/development.md,
# *Keeping main working*): prints a line for each recent run of the Linux workflow on main that
# failed, as a JSON object a watch takes for one thing (its `id`, a `title`, its `url`, some
# `text`). A watch starts its session on a line it hasn't printed before, so each failed run is
# told once, and a run that passes prints nothing.
#
#   codealong schedule add "main is red" --watch scripts/watch-main.sh --every 15m --thread \
#     --fresh-after 1d --prompt-file <what the session is told>
#
# That workflow (.github/workflows/linux.yml) is the only run of the checks that isn't an agent's
# own on a busy Mac: every four hours, the UI's build, linter and unit tests and cargo test with
# warnings as errors, on Linux. Before this a failure there emailed whoever last changed the
# workflow, and main stayed red until they read it: eight hours on 2026-11-18, with 83 commits
# landed on top.
set -euo pipefail
gh run list --workflow linux.yml --branch main --status failure --limit 5 \
  --json databaseId,headSha,url,createdAt \
  --jq '.[] | {id: (.databaseId | tostring), title: ("The Linux run on main failed at " + .headSha[0:8]), url: .url, text: ("Run " + (.databaseId | tostring) + ", started " + .createdAt + ", at commit " + .headSha)}'

#!/usr/bin/env bash
# Runs a command for so many seconds at most and prints what it printed, stdout and stderr
# together. It ends as the command ended, or with 124 when the command was stopped, as gtimeout
# says it. This is for a command that may wait on something nobody is there to answer, as
# check-release.sh's run of a downloaded app may.
#
#   scripts/within.sh 120 some-command --its-flags
#
# gtimeout isn't on every Mac, and it waits for the command to die, where this never waits past
# the limit: nothing says a process macOS is holding at a prompt dies when it's killed, so it's
# killed and left. Its output goes through a file so that it leaves no pipe open behind it, which
# a caller reading this script's output, as `$(…)` does, would go on waiting for.
set -uo pipefail

[ $# -ge 2 ] || { echo "Usage: scripts/within.sh <seconds> <command> [argument…]" >&2; exit 2; }
limit="$1"
shift
out="$(mktemp)"
trap 'rm -f "$out"' EXIT

"$@" </dev/null >"$out" 2>&1 &
run=$!
SECONDS=0
while kill -0 "$run" 2>/dev/null; do
  if [ "$SECONDS" -ge "$limit" ]; then
    kill -KILL "$run" 2>/dev/null
    cat "$out"
    echo "$1 was still running after $limit seconds, so it was stopped." >&2
    exit 124
  fi
  sleep 0.1
done
wait "$run"
status=$?
cat "$out"
exit "$status"

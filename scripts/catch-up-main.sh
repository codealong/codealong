#!/usr/bin/env bash
# Brings the main checkout (the one holding the repository's .git, the first entry of `git worktree
# list`) up to origin/main from any worktree: landing's step 3 in CLAUDE.md. A session Claude Code
# isolates in a worktree of its own is refused git that names another checkout (`git -C`), so it
# can't run that step's command itself; relaunch.sh runs this after every build, and a change with
# nothing to relaunch runs it alone. It only fast-forwards, and only a main checkout on `main`: git
# refuses when uncommitted edits there touch files the update changes, and a `main` with commits
# origin/main doesn't is left alone, so nothing there is lost. Prints one line saying where the main
# checkout stands, and exits 0 however that is, so a relaunch never fails over it.
set -uo pipefail
cd "$(dirname "$0")/.."

common="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || {
  echo "Not in a git checkout, so there's no main checkout to bring up to origin/main."
  exit 0
}
top="${common%/.git}"
if [ "$top" = "$common" ]; then
  echo "The repository is bare, so there's no main checkout to bring up to origin/main."
  exit 0
fi
GIT_TERMINAL_PROMPT=0 git fetch -q origin main 2>/dev/null || echo "Couldn't fetch origin/main; comparing with the last one fetched."

branch="$(git -C "$top" symbolic-ref -q --short HEAD || true)"
if [ "$branch" != main ]; then
  echo "The main checkout is on ${branch:-a detached HEAD}, not main, so it's left as it is."
elif [ "$(git -C "$top" rev-parse HEAD)" = "$(git rev-parse origin/main)" ]; then
  echo "The main checkout is at origin/main."
elif ! git merge-base --is-ancestor main origin/main; then
  echo "The main checkout's main has commits origin/main doesn't, so it's left as it is."
elif out="$(git -C "$top" merge -q --ff-only origin/main 2>&1)"; then
  echo "Fast-forwarded the main checkout to origin/main ($(git rev-parse --short origin/main))."
else
  # Git lists the files whose uncommitted edits stood in the way one to a line, indented by a tab.
  files="$(printf '%s\n' "$out" | sed -n 's/^	//p' | paste -s -d ' ' -)"
  if [ -n "$files" ]; then
    echo "Couldn't fast-forward the main checkout: its uncommitted edits touch files the update changes ($files)."
  else
    echo "Couldn't fast-forward the main checkout: $(printf '%s\n' "$out" | head -1)"
  fi
fi

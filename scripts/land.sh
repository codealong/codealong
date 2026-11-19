#!/usr/bin/env bash
# Lands this checkout's branch on origin/main: steps 2 and 3 of CLAUDE.md's *Landing work* as one
# command, for whoever has write access to the repository. It's a script so that the checks which
# keep main working run on exactly what is pushed, and not from memory on whatever a session last
# tested: agents push to main dozens of times a day, and nothing else looks at a commit before
# every other session builds on it.
#
#   scripts/land.sh --reviewed-by "<account>, <model>"   a change to code, after its review
#   scripts/land.sh --no-review "<why>"                   one no review could be had for
#   scripts/land.sh                                       docs, comments in them and notes only
#   scripts/land.sh --check                               everything but the push
#   scripts/land.sh --plan [path…]                        only says which checks those paths (or
#                                                         this branch's commits) call for
#
# In order:
#   1. It waits its turn: one landing at a time on this Mac, so nothing lands between the checks
#      and the push, and two sessions don't spend their checks racing each other to it. The lock
#      is a folder in the repository's .git holding its owner's pid, as relaunch.sh's is.
#   2. It fetches, and rebases the branch onto origin/main. A conflict stops it there with the
#      rebase under way: finish it (`git rebase --continue`) and run this again.
#   3. It runs the checks the commits call for, by the files they change (`calls_for` below): the
#      typecheck, the linter and the unit tests for the windows, cargo test for the engine, both
#      for what either reads of the other's, none for the docs alone. The engine has to build
#      without warnings, and builds in a folder of landings' own (`landing_target`), so it
#      doesn't queue behind other sessions' builds. A cargo test that fails is run again by
#      itself: one that passes then failed for this Mac's load, not for the change (some time
#      out while twenty agents build), so it's said, noted in <.git>/codealong-flaky.log and
#      doesn't stop the landing.
#   4. It pushes HEAD to main. A push turned down because main moved meanwhile (another Mac, a
#      pull request merged) goes back to 2, three times at most.
#   5. It brings the main checkout up to origin/main (catch-up-main.sh) and prints the commit and
#      where things stand, for the status line. Relaunching is still yours: scripts/relaunch.sh,
#      when what the app runs changed.
#
# It lands only what's committed: uncommitted changes here, yours or another session's, stop it
# before anything is touched. There is no flag to skip the checks.
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

usage() {
  sed -n '2,/^set -uo/p' "$0" | sed '$d; s/^# \{0,1\}//'
}

reviewed=""
unreviewed=""
check_only=0
plan_only=0
plan_paths=()
while [ $# -gt 0 ]; do
  case "$1" in
    --reviewed-by)
      [ $# -ge 2 ] && [ -n "$2" ] || { echo "--reviewed-by needs who reviewed it: the account and its model." >&2; exit 2; }
      reviewed="$2"
      shift 2
      ;;
    --no-review)
      [ $# -ge 2 ] && [ -n "$2" ] || { echo "--no-review needs why there was none." >&2; exit 2; }
      unreviewed="$2"
      shift 2
      ;;
    --check)
      check_only=1
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
    *)
      echo "Unknown argument $1; see scripts/land.sh --help." >&2
      exit 2
      ;;
  esac
done

# Which checks a changed file calls for, one word to a line: `ui` (typecheck, lint, unit tests),
# `engine` (cargo test) and `code` (it needs a review). The lists err towards running a check:
# each side's tests read some of the other's files, named here with what reads them.
calls_for() {
  case "$1" in
    # Docs nothing tests. CLAUDE.md, the rules and the skills aren't among them (below).
    docs/* | README.md | CONTRIBUTING.md | SECURITY.md | LICENSE | NOTICE | THIRD_PARTY_NOTICES.md | .github/*.md | .github/CODEOWNERS | .claude/skills/*.md) ;;
    # Notes the engine's tests hold: CLAUDE.md's size and its list of the rules files (lib.rs),
    # the snapshot modes (snapshot.rs), the command's reference and the plugin's copies of the
    # skills (cli.rs, skills.rs, install.rs). Words, so no review.
    CLAUDE.md | AGENTS.md | .claude/rules/* | skills/*.md | skills/*/*.md | skills/*/*/*.md | plugin/*.md | plugin/*/*.md | plugin/*/*/*.md | plugin/*/*/*/*.md)
      echo engine
      ;;
    # The engine, and what the windows' tests read of it: the command tables (commands.test.ts)
    # and the fixtures it writes for them (src/testing/).
    src-tauri/* | .cargo/*)
      echo engine
      echo ui
      echo code
      ;;
    # What the engine's tests read of the windows': the fixtures (wiretypes.rs, board.rs,
    # windowfeed.rs), the tokens (geometry.rs, quick.rs) and Settings (emblems.rs).
    src/testing/* | src/styles/tokens.css | src/ui/SettingsDialog.tsx | design/*)
      echo engine
      echo ui
      echo code
      ;;
    skills/* | plugin/*)
      echo engine
      echo code
      ;;
    # A workflow runs on GitHub, which nothing here can try; it still wants a second reader.
    .github/*) echo code ;;
    # Everything else is the windows' or a script, which the unit tests cover some of.
    *)
      echo ui
      echo code
      ;;
  esac
}

# The checks for a list of paths on stdin, as a line of words: "ui engine code", or "" for none.
plan_of() {
  local path wants=""
  while IFS= read -r path; do
    [ -n "$path" ] || continue
    wants="$wants $(calls_for "$path" | tr '\n' ' ')"
  done
  local out=""
  for word in ui engine code; do
    case " $wants " in *" $word "*) out="$out $word" ;; esac
  done
  echo "${out# }"
}

if [ "$plan_only" = 1 ] && [ ${#plan_paths[@]} -gt 0 ]; then
  printf '%s\n' "${plan_paths[@]}" | plan_of
  exit 0
fi

common="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || {
  echo "Not in a git checkout." >&2
  exit 2
}
changed() { git diff --name-only origin/main...HEAD; }

if [ "$plan_only" = 1 ]; then
  changed | plan_of
  exit 0
fi

if [ -n "$(git status --porcelain)" ]; then
  echo "This checkout has uncommitted changes, and only what's committed lands:" >&2
  git status --short | head -10 >&2
  echo "Commit yours first. If they're another session's, land from a worktree of your own." >&2
  exit 1
fi
if [ -d "$(git rev-parse --git-path rebase-merge)" ] || [ -d "$(git rev-parse --git-path rebase-apply)" ]; then
  echo "A rebase is under way here: finish it (git rebase --continue) and run this again." >&2
  exit 1
fi

fetch() { GIT_TERMINAL_PROMPT=0 git fetch -q origin main; }

lock="$common/codealong-landing.lock"
held=0
release() { [ "$held" = 1 ] && rm -rf "$lock"; }
trap release EXIT
take_turn() {
  [ "$check_only" = 1 ] && return 0
  # One whose owner is gone is taken over.
  while ! mkdir "$lock" 2>/dev/null; do
    local holder
    holder="$(cat "$lock/pid" 2>/dev/null || true)"
    if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
      rm -rf "$lock"
      continue
    fi
    echo "Waiting for the landing in progress (pid ${holder:-?}, $(cat "$lock/what" 2>/dev/null || echo "which hasn't said what"))…"
    sleep 15
  done
  held=1
  echo $$ >"$lock/pid"
  echo "$(git rev-parse --abbrev-ref HEAD) in $PWD" >"$lock/what"
}

# What each check took and came to, for the last lines.
ran=""
note() { ran="$ran${ran:+, }$1"; }
log="$(mktemp -t codealong-land)"
cleanup() {
  rm -f "$log"
  release
}
trap cleanup EXIT

# Runs one of the windows' checks, quietly unless it fails.
ui_check() {
  local name="$1"
  shift
  SECONDS=0
  if "$@" >"$log" 2>&1; then
    note "$name (${SECONDS} s)"
    return 0
  fi
  echo "$name failed:" >&2
  tail -60 "$log" >&2
  return 1
}

# Where a landing's engine tests build: a folder of landings' own beside the shared one
# (<shared>-landing), so they don't queue on cargo's lock behind every session's build, which on
# 2026-11-19 held a cargo test for 20 minutes before it started. Only one landing runs at a time,
# so nothing waits in it. It starts as a copy-on-write clone of the shared folder's debug build
# (APFS: seconds, and no room taken until something is rebuilt), so the dependencies are built
# already, and is made afresh every few days, which sheds what it has gathered; deleting it is
# always safe. Where a clone can't be made (not APFS), with --check (several can run at once)
# and with CARGO_TARGET_DIR set, it's the folder everything else builds in.
landing_target() {
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    echo "$CARGO_TARGET_DIR"
    return
  fi
  local shared own
  shared="$(scripts/target-dir.sh 2>/dev/null)" || return 1
  own="${shared%/}-landing"
  if [ "$check_only" = 1 ] || [ ! -d "$shared/debug" ]; then
    echo "$shared"
    return
  fi
  if [ -n "$(find "$own/cloned" -mtime +3 2>/dev/null)" ]; then
    rm -rf "$own"
  fi
  if [ ! -f "$own/cloned" ]; then
    rm -rf "$own"
    mkdir -p "$own"
    if ! cp -c -R "$shared/debug" "$own/" 2>/dev/null; then
      rm -rf "$own"
      echo "$shared"
      return
    fi
    cp "$shared/CACHEDIR.TAG" "$own/" 2>/dev/null || true
    touch "$own/cloned"
  fi
  echo "$own"
}

engine_check() {
  # Tauri's build reads dist/, whatever is in it.
  if [ ! -f dist/index.html ]; then
    ui_check "the UI's build" npm run --silent build || return 1
  fi
  local target
  target="$(landing_target)" || { echo "Couldn't tell where cargo builds (scripts/target-dir.sh)." >&2; return 1; }
  SECONDS=0
  (cd src-tauri && CARGO_TARGET_DIR="$target" cargo test) >"$log" 2>&1
  local status=$?
  if grep -q -E '^warning: .* generated [0-9]+ warnings?' "$log"; then
    echo "The engine builds with warnings, which CI takes for errors:" >&2
    grep -E -A12 '^warning' "$log" | head -80 >&2
    return 1
  fi
  if [ "$status" = 0 ]; then
    note "cargo test (${SECONDS} s)"
    return 0
  fi
  # A build that failed has no tests to name; neither has a run that died.
  local failed
  failed="$(sed -n 's/^test \(.*\) \.\.\. FAILED$/\1/p' "$log" | sort -u)"
  if [ -z "$failed" ]; then
    echo "cargo test failed before any test did:" >&2
    grep -E -B2 -A14 '^error' "$log" | head -80 >&2
    [ -s "$log" ] && ! grep -q '^error' "$log" && tail -30 "$log" >&2
    return 1
  fi
  # Each by itself: a test that only fails with the rest, on a Mac this busy, isn't the change's.
  local name alone="" real="" whole
  whole="$(mktemp -t codealong-land)"
  cp "$log" "$whole"
  while IFS= read -r name; do
    if (cd src-tauri && CARGO_TARGET_DIR="$target" cargo test "$name" -- --exact) >"$log" 2>&1 && grep -q -F "test $name ... ok" "$log"; then
      alone="$alone${alone:+, }$name"
    else
      real="$real${real:+, }$name"
    fi
  done <<EOF
$failed
EOF
  if [ -n "$real" ]; then
    echo "cargo test failed, and these fail by themselves too: $real" >&2
    for name in $(echo "$real" | tr -d ','); do
      grep -F -A25 -- "---- $name stdout ----" "$whole" | head -40 >&2
    done
    rm -f "$whole"
    return 1
  fi
  rm -f "$whole"
  echo "Failed with the rest and passed alone, so taken for this Mac's load: $alone"
  echo "$(date '+%Y-%m-%d %H:%M') $(git rev-parse --short HEAD) load $(sysctl -n vm.loadavg 2>/dev/null | awk '{print $2}'): $alone" >>"$common/codealong-flaky.log"
  note "cargo test (${SECONDS} s; passed alone: $alone)"
  return 0
}

take_turn

tries=0
while :; do
  tries=$((tries + 1))
  fetch || { echo "Couldn't fetch origin/main, so there's nothing to rebase onto or push to." >&2; exit 1; }
  if git merge-base --is-ancestor HEAD origin/main; then
    echo "Nothing to land: origin/main ($(git rev-parse --short origin/main)) already has this branch's commits."
    exit 0
  fi
  if ! git merge-base --is-ancestor origin/main HEAD; then
    if ! git rebase -q origin/main >"$log" 2>&1; then
      echo "The rebase onto origin/main stopped on a conflict:" >&2
      git status --short | grep -E '^(UU|AA|DU|UD|AU|UA|DD) ' | head -20 >&2
      echo "Resolve it, git rebase --continue, rerun what you'd tested, and run scripts/land.sh again." >&2
      exit 1
    fi
  fi

  wants="$(changed | plan_of)"
  case " $wants " in
    *" code "*)
      if [ -z "$reviewed$unreviewed" ]; then
        echo "These commits change code, which another provider's agent reviews before it's pushed" >&2
        echo "(CLAUDE.md, *Review before pushing*). Say who did: --reviewed-by \"<account>, <model>\"," >&2
        echo "or, when none could be had, --no-review \"<why>\"." >&2
        exit 2
      fi
      ;;
  esac

  ran=""
  [ -d node_modules ] || { echo "No node_modules here: run npm ci first." >&2; exit 1; }
  case " $wants " in
    *" ui "*)
      ui_check "typecheck" npm run --silent typecheck || exit 1
      ui_check "lint" npm run --silent lint || exit 1
      # A test that fails once and passes the second time is counted as passing: several time
      # their work, and this Mac is often too busy for them.
      ui_check "unit tests" npx vitest run --retry=1 || exit 1
      ;;
  esac
  case " $wants " in
    *" engine "*) engine_check || exit 1 ;;
  esac
  [ -n "$ran" ] || ran="no checks: nothing these commits change is tested"

  if [ "$check_only" = 1 ]; then
    echo "Checked, not pushed: $(git rev-parse --short HEAD) on origin/main $(git rev-parse --short origin/main)."
    echo "Ran: $ran."
    exit 0
  fi

  if git push -q origin HEAD:main 2>"$log"; then
    break
  fi
  # Main moved while the checks ran, from somewhere this Mac's lock doesn't reach.
  if grep -q -E 'non-fast-forward|fetch first|rejected' "$log" && [ "$tries" -lt 3 ]; then
    echo "origin/main moved while the checks ran; rebasing onto it and checking again."
    continue
  fi
  echo "The push was turned down:" >&2
  cat "$log" >&2
  exit 1
done

release
held=0
fetch || true
echo "Landed: $(git rev-parse --short HEAD) is in origin/main. $(scripts/catch-up-main.sh)"
echo "Ran: $ran."
if [ -n "$reviewed" ]; then
  echo "Reviewed by: $reviewed."
elif [ -n "$unreviewed" ]; then
  echo "No review: $unreviewed."
fi
case " $wants " in
  *" ui "* | *" engine "*) echo "If what the app runs changed, relaunch it from here: scripts/relaunch.sh." ;;
esac

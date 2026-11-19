#!/usr/bin/env bash
# Lands this checkout's branch on origin/main: steps 2 and 3 of CLAUDE.md's *Landing work* as one
# command, for whoever has write access to the repository. It's a script so that the checks which
# keep main working run on exactly what is pushed, and not from memory on whatever a session last
# tested: agents push to main dozens of times a day, and nothing else looks at a commit before
# every other session builds on it.
#
#   scripts/land.sh --reviewed-by "<account>, <model>"   a change to code, after its review
#   scripts/land.sh --no-review "<why>"                   one no review could be had for
#   scripts/land.sh                                       docs and notes only
#   scripts/land.sh --check                               everything but the push
#   scripts/land.sh --plan [path…]                        only says which checks those paths (or
#                                                         this branch's commits) call for
#
# In order:
#   1. It waits its turn: one landing at a time on this Mac, so nothing lands between the checks
#      and the push, and two sessions don't spend their checks racing each other to it. The turn
#      is a lock the kernel holds on a file in the repository's .git for as long as this runs,
#      so one that dies leaves nothing behind for the next to work out.
#   2. It fetches, and rebases the branch onto origin/main. A conflict stops it there with the
#      rebase under way: finish it (`git rebase --continue`) and run this again. A branch from
#      before the history was rewritten is turned away, with the rebase CLAUDE.md gives for it.
#   3. It runs the checks the commits call for, by the files they change (`calls_for` below): the
#      build (the typecheck and the bundle), the linter and the unit tests for the windows; cargo
#      test for the engine; both for what either reads of the other's; none for the docs alone.
#      The engine has to build without warnings, from Cargo.lock as committed, and builds in a
#      folder of landings' own (`landing_target`), so it doesn't queue behind other sessions'
#      builds. A cargo test that fails is run again by itself: one that passes then failed for
#      this Mac's load, not for the change (some time out while twenty agents build), so it's
#      said, noted in <.git>/codealong-flaky.log and doesn't stop the landing.
#   4. It pushes the commit it checked, and only if the checkout still is that commit with
#      nothing changed in it. A push turned down because main moved meanwhile (another Mac, a
#      pull request merged) goes back to 2, three times at most.
#   5. It brings the main checkout up to origin/main (catch-up-main.sh) and prints the commit and
#      where things stand, for the status line. Relaunching is still yours: scripts/relaunch.sh,
#      when what the app runs changed.
#
# It lands only what's committed: uncommitted changes here, yours or another session's, stop it
# before anything is touched. There is no flag to skip the checks.
set -uo pipefail

# Everything is in functions, read whole before any of it runs: the rebase can rewrite this
# file, and bash reads a script as it goes.

usage() {
  sed -n '2,/^set -uo/p' "$0" | sed '$d; s/^# \{0,1\}//'
}

# Which checks a changed file calls for, one word to a line: `ui` (build, lint, unit tests),
# `engine` (cargo test) and `code` (it needs a review). The lists err towards running a check:
# each side's tests read some of the other's files, named here with what reads them.
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
      echo code
      ;;
    # What the engine's tests read of the windows': the fixtures (wiretypes.rs, board.rs,
    # windowfeed.rs), the tokens (geometry.rs, quick.rs) and Settings (emblems.rs).
    src/testing/* | src/styles/tokens.css | src/ui/SettingsDialog.tsx | design/*)
      echo engine
      echo ui
      echo code
      ;;
    # The skill and the plugin as the bundle carries them and install.rs copies them.
    skills/* | plugin/* | .claude-plugin/*)
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
  local path word wants="" out=""
  while IFS= read -r path; do
    [ -n "$path" ] || continue
    wants="$wants $(calls_for "$path" | tr '\n' ' ')"
  done
  for word in ui engine code; do
    case " $wants " in *" $word "*) out="$out $word" ;; esac
  done
  echo "${out# }"
}

# The files this branch's commits change, both names of one that moved (a rename shows only
# where it went, and the checks its old place called for would be skipped), written as they are
# rather than quoted. Fails when git can't say, so that no answer isn't taken for no changes.
changed() {
  git -c core.quotePath=false diff --no-renames --name-only "origin/main...${1:-HEAD}"
}

fetch() { GIT_TERMINAL_PROMPT=0 git fetch -q origin main; }

die() {
  echo "$1" >&2
  exit "${2:-1}"
}

# One landing at a time: an exclusive lock on a file in the repository's .git, held through
# descriptor 9 for as long as this process (or anything it started) lives. The kernel lets go of
# it however this ends, so there's no owner's pid to read, no lock left by one that was killed,
# and no two waiters taking over the same dead one.
take_turn() {
  [ "$check_only" = 1 ] && return 0
  [ "${CODEALONG_LANDING_HELD:-}" = "$$" ] && return 0
  local lock="$common/codealong-landing.lock"
  exec 9>>"$lock" || die "Couldn't open $lock, which landings take turns by."
  until python3 -I -c 'import fcntl; fcntl.flock(9, fcntl.LOCK_EX | fcntl.LOCK_NB)' 2>/dev/null; do
    command -v python3 >/dev/null || die "python3 isn't here, which takes the landing's lock."
    echo "Waiting for the landing in progress ($(cat "$lock.who" 2>/dev/null || echo "which hasn't said whose"))…"
    sleep 15
  done
  echo "pid $$, $(git rev-parse --abbrev-ref HEAD) in $PWD" >"$lock.who"
  export CODEALONG_LANDING_HELD="$$"
}

clean_or_die() {
  if [ -n "$(git status --porcelain)" ]; then
    echo "This checkout has uncommitted changes, and only what's committed lands:" >&2
    git status --short | head -10 >&2
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
# already; the clone is made under cargo's own lock on that folder, so no build is halfway
# through writing it, which waits for the build under way, once. It's made afresh after a week,
# which sheds what it has gathered. Only a folder this made (it has the `landing-clone` file) is
# ever removed. Where no clone can be made (a volume that isn't APFS, where cp would copy all of
# it), with --check (several can run at once) and with CARGO_TARGET_DIR set, the engine builds
# where everything else does.
landing_target() {
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    echo "$CARGO_TARGET_DIR"
    return
  fi
  local shared own
  shared="$(scripts/target-dir.sh 2>/dev/null)" || return 1
  own="${shared%/}-landing"
  if [ "$check_only" = 1 ] || [ ! -f "$shared/debug/.cargo-lock" ] || [ "$(stat -f %T "$shared" 2>/dev/null)" != apfs ]; then
    echo "$shared"
    return
  fi
  if [ -e "$own" ] && [ ! -f "$own/landing-clone" ]; then
    # Something else's folder, by the same name: not ours to build in or to remove.
    echo "$shared"
    return
  fi
  if [ -n "$(find "$own/landing-clone" -mtime +7 2>/dev/null)" ]; then
    rm -rf "$own"
  fi
  if [ ! -f "$own/landing-clone" ]; then
    echo "Copying the shared build folder for landings to build in (it waits for the build under way, once a week)…" >&2
    mkdir "$own" || { echo "$shared"; return; }
    touch "$own/landing-clone"
    if ! python3 -I - "$shared/debug" "$own/" <<'EOF'; then
import fcntl, os, subprocess, sys

lock = os.open(os.path.join(sys.argv[1], ".cargo-lock"), os.O_RDWR)
fcntl.flock(lock, fcntl.LOCK_EX)
try:
    sys.exit(subprocess.call(["cp", "-c", "-R", sys.argv[1], sys.argv[2]]))
finally:
    fcntl.flock(lock, fcntl.LOCK_UN)
EOF
      rm -rf "$own"
      echo "$shared"
      return
    fi
    cp "$shared/CACHEDIR.TAG" "$own/" 2>/dev/null || true
  fi
  echo "$own"
}

engine_check() {
  # Tauri's build reads dist/, whatever is in it; the windows' checks have just built it when
  # they ran.
  if [ ! -f dist/index.html ]; then
    ui_check "the UI's build" npm run --silent build || return 1
  fi
  local target
  target="$(landing_target)" || { echo "Couldn't tell where cargo builds (scripts/target-dir.sh)." >&2; return 1; }
  SECONDS=0
  # Every test binary, though one fails: cargo stops at the first otherwise, and the ones after
  # it would count as passed. --locked: what's tested is what Cargo.lock says, and nothing
  # rewrites it under the commit.
  (cd src-tauri && CARGO_TARGET_DIR="$target" cargo test --locked --no-fail-fast) >"$log" 2>&1
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
  # A build that failed has no tests to name, and a test binary that died (a crash, an abort)
  # never says how it came out: neither is a test to try again.
  local failed started ended
  failed="$(sed -n 's/^test \(.*\) \.\.\. FAILED$/\1/p' "$log" | sort -u)"
  started="$(grep -c -E '^ +(Running|Doc-tests) ' "$log")"
  ended="$(grep -c -E '^test result: ' "$log")"
  if [ -z "$failed" ] || [ "$started" != "$ended" ]; then
    echo "cargo test failed, and not only by tests that said so ($ended of $started test binaries finished):" >&2
    grep -E -B2 -A14 '^error' "$log" | head -80 >&2
    return 1
  fi
  # Each by itself: a test that only fails with the rest, on a Mac this busy, isn't the change's.
  local name alone="" real="" whole
  whole="$(mktemp)"
  cp "$log" "$whole"
  while IFS= read -r name; do
    if (cd src-tauri && CARGO_TARGET_DIR="$target" cargo test --locked "$name" -- --exact) >"$log" 2>&1 && grep -q -F "test $name ... ok" "$log"; then
      alone="$alone${alone:+, }$name"
    else
      real="$real${real:+, }$name"
      grep -F -A25 -- "---- $name stdout ----" "$whole" | head -40 >&2
    fi
  done <<EOF
$failed
EOF
  rm -f "$whole"
  if [ -n "$real" ]; then
    echo "cargo test failed, and these fail by themselves too: $real" >&2
    return 1
  fi
  echo "Failed with the rest and passed alone, so taken for this Mac's load: $alone"
  echo "$(date '+%Y-%m-%d %H:%M') $(git rev-parse --short HEAD) load $(sysctl -n vm.loadavg 2>/dev/null | awk '{print $2}'): $alone" >>"$common/codealong-flaky.log"
  note "cargo test (${SECONDS} s; passed alone: $alone)"
  return 0
}

note() { ran="$ran${ran:+, }$1"; }

main() {
  cd "$(dirname "$0")/.." || exit 1
  export PATH="$HOME/.cargo/bin:$PATH"

  reviewed=""
  unreviewed=""
  check_only=0
  local plan_only=0 plan_paths=() again=("$@")
  while [ $# -gt 0 ]; do
    case "$1" in
      --reviewed-by | --no-review)
        # Its value, and not the next option: `--reviewed-by --check` would have pushed.
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
    printf '%s\n' "${plan_paths[@]}" | plan_of
    exit 0
  fi

  common="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || die "Not in a git checkout." 2
  local paths
  if [ "$plan_only" = 1 ]; then
    paths="$(changed)" || die "Couldn't list what this branch changes."
    printf '%s\n' "$paths" | plan_of
    exit 0
  fi

  clean_or_die
  if [ -d "$(git rev-parse --git-path rebase-merge)" ] || [ -d "$(git rev-parse --git-path rebase-apply)" ]; then
    die "A rebase is under way here: finish it (git rebase --continue) and run this again."
  fi

  ran=""
  log="$(mktemp)"
  trap 'rm -f "$log"' EXIT

  take_turn
  # The wait can be long, and the checkout isn't ours alone.
  clean_or_die

  local tries=0 itself base tested wants
  while :; do
    tries=$((tries + 1))
    fetch || die "Couldn't fetch origin/main, so there's nothing to rebase onto or push to."
    base="$(git rev-parse origin/main)"
    if git merge-base --is-ancestor HEAD origin/main; then
      echo "Nothing to land: origin/main ($(git rev-parse --short origin/main)) already has this branch's commits."
      exit 0
    fi
    if ! git merge-base --is-ancestor origin/main HEAD; then
      refuse_old_history
      itself="$(git hash-object "$0")"
      if ! git rebase -q origin/main >"$log" 2>&1; then
        echo "The rebase onto origin/main stopped on a conflict:" >&2
        git status --short | grep -E '^(UU|AA|DU|UD|AU|UA|DD) ' | head -20 >&2
        die "Resolve it, git rebase --continue, rerun what you'd tested, and run scripts/land.sh again."
      fi
      # main's own land.sh came in with the rebase: its checks are the ones to run, and bash has
      # this one in memory. Start again as it, still holding the turn.
      if [ "$(git hash-object "$0")" != "$itself" ]; then
        exec "$0" ${again[@]+"${again[@]}"}
      fi
    fi
    tested="$(git rev-parse HEAD)"

    paths="$(changed "$tested")" || die "Couldn't list what these commits change, so there's no telling which checks they need."
    wants="$(printf '%s\n' "$paths" | plan_of)"
    case " $wants " in
      *" code "*)
        if [ -z "$reviewed$unreviewed" ]; then
          echo "These commits change code, which another provider's agent reviews before it's pushed" >&2
          echo "(CLAUDE.md, *Review before pushing*). Say who did: --reviewed-by \"<account>, <model>\"," >&2
          die "or, when none could be had, --no-review \"<why>\"." 2
        fi
        ;;
    esac

    ran=""
    case " $wants " in
      *" ui "*)
        # What's installed is what package-lock.json says, which a rebase can have changed.
        if [ ! -d node_modules ] || [ package-lock.json -nt node_modules/.package-lock.json ]; then
          ui_check "npm ci" npm ci --no-audit --no-fund || exit 1
        fi
        # The typecheck and the bundle the app is built from.
        ui_check "build" npm run --silent build || exit 1
        ui_check "lint" npm run --silent lint || exit 1
        # A test that fails once and passes the second time is counted as passing: this Mac is
        # often too busy for one that waits on a timer.
        ui_check "unit tests" npx vitest run --retry=1 || exit 1
        ;;
    esac
    case " $wants " in
      *" engine "*)
        [ -d node_modules ] || die "No node_modules here, which the UI's build needs: run npm ci first."
        engine_check || exit 1
        ;;
    esac
    [ -n "$ran" ] || ran="no checks: nothing these commits change is tested"

    # What was checked is what's pushed: the commit, with nothing a check or anyone else left
    # changed in the checkout (a rewritten Cargo.lock, a commit made meanwhile).
    if [ "$(git rev-parse HEAD)" != "$tested" ] || [ -n "$(git status --porcelain)" ]; then
      echo "The checkout changed while the checks ran, so what they checked isn't what's here now:" >&2
      git status --short | head -10 >&2
      die "HEAD is $(git rev-parse --short HEAD), and $(git rev-parse --short "$tested") was checked. Run scripts/land.sh again."
    fi

    if [ "$check_only" = 1 ]; then
      echo "Checked, not pushed: $(git rev-parse --short "$tested") on origin/main $(git rev-parse --short "$base")."
      echo "Ran: $ran."
      exit 0
    fi

    if git push -q origin "$tested:refs/heads/main" 2>"$log"; then
      break
    fi
    # Turned down. Only main having moved, from somewhere this Mac's lock doesn't reach, is
    # worth another go; a hook's refusal would be the same again.
    cp "$log" "$log.push"
    if fetch && [ "$(git rev-parse origin/main)" != "$base" ] && [ "$tries" -lt 3 ]; then
      rm -f "$log.push"
      echo "origin/main moved while the checks ran; rebasing onto it and checking again."
      continue
    fi
    echo "The push was turned down:" >&2
    cat "$log.push" >&2
    rm -f "$log.push"
    exit 1
  done

  # The turn is over: the next landing can start while the main checkout catches up.
  exec 9>&-
  fetch || true
  echo "Landed: $(git rev-parse --short "$tested") is in origin/main. $(scripts/catch-up-main.sh)"
  echo "Ran: $ran."
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

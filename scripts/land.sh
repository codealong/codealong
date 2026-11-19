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
  sed -n '2,/^set -uo/p' "$self" | sed '$d; s/^# \{0,1\}//'
}

# Which checks a changed file calls for, one word to a line: `ui` (build, lint, unit tests),
# `engine` (cargo test), `code` (it needs a review) and `install` (npm ci first). The lists err towards running a check:
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
    # What the engine's tests read of the windows': the fixtures it writes for them
    # (wiretypes.rs, board.rs, windowfeed.rs), the tokens (geometry.rs, quick.rs) and Settings
    # (emblems.rs). The rest of src/testing/ is the unit tests' own (their stand-in for the
    # engine, their page), which no engine test reads.
    src/testing/modelFields.ts | src/testing/ruleCases.json | src/testing/boardPushes.json | src/styles/tokens.css | src/ui/SettingsDialog.tsx | design/*)
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
    # What's installed: one changed without the other fails a clean install, as CI's is.
    package.json | package-lock.json)
      echo ui
      echo code
      echo install
      ;;
    # Everything else is the windows' or a script, which the unit tests cover some of.
    *)
      echo ui
      echo code
      ;;
  esac
}

# The checks for a list of paths on stdin, a NUL after each as git writes them (a name can
# hold anything else), as a line of words: "ui engine code", or "" for none.
plan_of() {
  local path word wants="" out=""
  while IFS= read -r -d '' path; do
    [ -n "$path" ] || continue
    wants="$wants $(calls_for "$path" | tr '\n' ' ')"
  done
  for word in ui engine code install; do
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

# One landing at a time: an exclusive lock (flock) on a file in the repository's .git, held by a
# small helper for as long as this script lives. The kernel lets go of it when the helper ends,
# and the helper ends when it's told the landing is over or sees this process gone, however
# that came about: so there's no owner's pid to read, no lock left by one that was killed, and
# no two waiters taking over the same dead one. Only the helper has the lock's file open, so
# nothing this script starts (a check, git, a hook and whatever that leaves running) can keep
# later landings waiting by outliving it.
take_turn() {
  [ "$check_only" = 1 ] && return 0
  command -v python3 >/dev/null || die "python3 isn't here, which holds the landing's lock."
  local lock="$common/codealong-landing.lock" said waited=0
  said="$(mktemp)"
  python3 -I - "$lock" "$$" >"$said" 2>&1 <<'EOF' &
import fcntl, os, sys, time

lock, owner = sys.argv[1], int(sys.argv[2])


def owned():
    try:
        os.kill(owner, 0)
        return True
    except OSError:
        return False


held = os.open(lock, os.O_RDWR | os.O_CREAT, 0o644)
while True:
    try:
        fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
        break
    except OSError:
        if not owned():
            sys.exit(0)
        time.sleep(2)
print("held", flush=True)
while owned():
    time.sleep(1)
EOF
  holder=$!
  # Disowned, or bash says "Terminated" with the helper's whole text when it's told to go.
  disown "$holder" 2>/dev/null
  until grep -q held "$said" 2>/dev/null; do
    if ! kill -0 "$holder" 2>/dev/null; then
      cat "$said" >&2
      rm -f "$said"
      die "Couldn't take the landing's lock ($lock)."
    fi
    # Said once the helper has had a moment to take a lock that's free, then every so often.
    if [ "$waited" -ge 5 ] && [ $(((waited - 5) % 20)) = 0 ]; then
      echo "Waiting for the landing in progress ($(cat "$lock.who" 2>/dev/null || echo "which hasn't said whose"))…"
    fi
    sleep 1
    waited=$((waited + 1))
  done
  rm -f "$said"
  echo "pid $$, $(git rev-parse --abbrev-ref HEAD) in $PWD" >"$lock.who"
  # For this script started again as main's own (after a rebase brought a newer one): it's this
  # process still, which the helper watches, and so it has the turn.
  export CODEALONG_LANDING_HELD="$$" CODEALONG_LANDING_HOLDER="$holder"
}

# Ends the turn: the helper that holds the lock is told to go. Only this process's own: the
# two are in the environment for the script's restart, so a land.sh started under a landing
# (the unit tests run one, in a repository of their own) is handed the outer one's helper too,
# and mustn't end its turn for it.
let_go() {
  if [ "${CODEALONG_LANDING_HELD:-}" = "$$" ] && [ -n "${CODEALONG_LANDING_HOLDER:-}" ]; then
    kill "$CODEALONG_LANDING_HOLDER" 2>/dev/null
  fi
  CODEALONG_LANDING_HELD=""
  CODEALONG_LANDING_HOLDER=""
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
# which sheds what it has gathered, and when its copy was cut short. Only a folder this made (it
# has the `landing-clone` file) is ever removed. Where no clone can be made (a volume that isn't APFS, where cp would copy all of
# it), with --check (several can run at once) and with CARGO_TARGET_DIR set, the engine builds
# where everything else does.
landing_target() {
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    echo "$CARGO_TARGET_DIR"
    return
  fi
  local shared
  shared="$(scripts/target-dir.sh 2>/dev/null)" || return 1
  shared="${shared%/}"
  # Copy-on-write is APFS's, so macOS's: there `df -T apfs` lists the volume only when it is
  # APFS (GNU's df reads -T another way, and stat's %T is a file's type, not its volume's).
  if [ "$check_only" = 1 ] || [ ! -f "$shared/debug/.cargo-lock" ] || [ "$(uname -s)" != Darwin ] || ! df -T apfs "$shared" >/dev/null 2>&1 || [ -z "$(df -T apfs "$shared" 2>/dev/null | tail -n +2)" ]; then
    echo "$shared"
    return
  fi
  python3 -I - "$shared" <<'EOF' || echo "$shared"
import fcntl, os, shutil, subprocess, sys, time

shared = sys.argv[1]
own = shared + "-landing"
# `landing-clone` says the folder is this script's to remove; `whole` that its copy finished.
mine, whole = os.path.join(own, "landing-clone"), os.path.join(own, "whole")
# One at a time through here: two clones of the repository beside each other share this folder,
# each with a landing lock of its own.
with open(own + ".lock", "w") as turn:
    fcntl.flock(turn, fcntl.LOCK_EX)
    if os.path.lexists(own) and not os.path.isfile(mine):
        # Something else's folder, by the same name: not ours to build in or to remove.
        print(shared)
        sys.exit(0)
    # One whose copy was cut short is made again, and so is one a week old, unless a build is
    # under way in it (cargo's own lock on it is taken): a landing from another clone of the
    # repository may be testing there, and it's renewed the next time instead.
    old = os.path.isfile(whole) and time.time() - os.path.getmtime(whole) > 7 * 86400
    if old:
        try:
            busy = os.open(os.path.join(own, "debug", ".cargo-lock"), os.O_RDWR)
            fcntl.flock(busy, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            old = False
    if os.path.lexists(own) and (not os.path.isfile(whole) or old):
        shutil.rmtree(own)
    if not os.path.isfile(whole):
        print("Copying the shared build folder for landings to build in (it waits for the build under way, once a week)…", file=sys.stderr)
        os.mkdir(own)
        open(mine, "w").close()
        # Under cargo's own lock on the folder copied, so no build is halfway through writing it.
        lock = os.open(os.path.join(shared, "debug", ".cargo-lock"), os.O_RDWR)
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            copied = subprocess.call(["cp", "-c", "-R", os.path.join(shared, "debug"), own + "/"]) == 0
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)
        if not copied:
            shutil.rmtree(own, ignore_errors=True)
            print(shared)
            sys.exit(0)
        if os.path.isfile(os.path.join(shared, "CACHEDIR.TAG")):
            shutil.copy(os.path.join(shared, "CACHEDIR.TAG"), own)
        open(whole, "w").close()
    print(own)
EOF
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
  if [ -z "$failed" ] || [ "$started" != "$ended" ] || grep -q -E "process didn't exit successfully|\(signal: [0-9]+" "$log"; then
    echo "cargo test failed, and not only by tests that said so ($ended of $started test binaries finished; one that dies after its tests says so below):" >&2
    grep -E -B2 -A14 "^error|process didn't exit successfully" "$log" | head -80 >&2
    return 1
  fi
  # Each by itself: a test that only fails with the rest, on a Mac this busy, isn't the change's.
  # And once more a minute on when it fails alone too: at a load in the high hundreds the tests
  # that start a terminal or read the process table have failed alone and passed the next time
  # (eight of them in one landing on 2026-11-19, for a change with nothing of the engine's).
  local name alone="" real="" whole
  whole="$(mktemp)"
  cp "$log" "$whole"
  # 0 when it passed, 1 when it ran and failed, 2 when the run came to anything else: a
  # build that failed, or a test binary that died (a crash is never this Mac's load).
  by_itself() {
    (cd src-tauri && CARGO_TARGET_DIR="$target" cargo test --locked "$1" -- --exact) >"$log" 2>&1
    local status=$?
    if grep -q -E "process didn't exit successfully|\(signal: [0-9]+" "$log"; then
      return 2
    fi
    if [ "$status" = 0 ] && grep -q -F "test $1 ... ok" "$log"; then
      return 0
    fi
    # Ran and failed: cargo's own status for a failed test, its line for this one, and the
    # run's summary after it. A run cut off before its summary (killed, out of memory) is 2.
    if [ "$status" = 101 ] && grep -q -F "test $1 ... FAILED" "$log" && grep -q -E '^test result: FAILED' "$log"; then
      return 1
    fi
    return 2
  }
  local went
  while IFS= read -r name; do
    by_itself "$name"
    went=$?
    if [ "$went" = 1 ]; then
      sleep 60
      by_itself "$name"
      went=$?
    fi
    if [ "$went" = 2 ]; then
      echo "Run by itself, $name didn't fail as a test fails (a build that failed, or a test binary that died):" >&2
      tail -30 "$log" >&2
    fi
    if [ "$went" = 0 ]; then
      alone="$alone${alone:+, }$name"
    else
      real="$real${real:+, }$name"
      # What it said the first time, up to the next test's.
      awk -v head="---- $name stdout ----" '$0 == head { on = 1; print; next } on && /^---- .* stdout ----$/ { exit } on' "$whole" | head -40 >&2
    fi
  done <<EOF
$failed
EOF
  rm -f "$whole"
  if [ -n "$real" ]; then
    echo "cargo test failed, and these fail by themselves too: $real" >&2
    [ -z "$alone" ] || echo "(These failed with the rest and passed alone, so aren't why: $alone)" >&2
    return 1
  fi
  echo "Failed with the rest and passed alone, so taken for this Mac's load: $alone"
  echo "$(date '+%Y-%m-%d %H:%M') $(git rev-parse --short HEAD) load $(sysctl -n vm.loadavg 2>/dev/null | awk '{print $2}'): $alone" >>"$common/codealong-flaky.log"
  note "cargo test (${SECONDS} s; passed alone: $alone)"
  return 0
}

note() { ran="$ran${ran:+, }$1"; }

main() {
  # This file by its whole path, before the folder changes: run as ../scripts/land.sh, `$0`
  # names nothing from the checkout's top.
  self="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")" || exit 1
  cd "$(dirname "$self")/.." || exit 1
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
    printf '%s\0' "${plan_paths[@]}" | plan_of
    exit 0
  fi

  common="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || die "Not in a git checkout." 2
  log="$(mktemp)"
  list="$(mktemp)"
  trap 'rm -f "$log" "$list"; let_go' EXIT
  if [ "$plan_only" = 1 ]; then
    changed || die "Couldn't list what this branch changes."
    plan_of <"$list"
    exit 0
  fi

  clean_or_die
  if [ -d "$(git rev-parse --git-path rebase-merge)" ] || [ -d "$(git rev-parse --git-path rebase-apply)" ]; then
    die "A rebase is under way here: finish it (git rebase --continue) and run this again."
  fi

  ran=""

  # What's landed is what was here when this was asked: the wait can be long, and in a checkout
  # that isn't ours alone another session can have committed or changed branch meanwhile, which
  # would go out under this one's review.
  local asked
  asked="$(git rev-parse HEAD) $(git symbolic-ref -q --short HEAD || echo "no branch")" || die "Couldn't read this checkout's HEAD."
  # Not when this is the script started again as main's own, after its rebase: it has the turn
  # already, and HEAD is what that rebase made.
  if [ "${CODEALONG_LANDING_HELD:-}" != "$$" ]; then
    take_turn
    if [ "$asked" != "$(git rev-parse HEAD) $(git symbolic-ref -q --short HEAD || echo "no branch")" ]; then
      die "This checkout moved while it waited its turn (it was $asked): look at what's here now, and run scripts/land.sh again."
    fi
  fi
  clean_or_die

  # From here only this script moves the checkout: the commit it holds is the one asked for,
  # then the one its own rebase made of it, and anything else there stops the landing.
  local tries=0 itself now base tested wants dirty expected rebased
  # The commit as it was read before the wait, not read again now.
  expected="${asked%% *}"
  while :; do
    tries=$((tries + 1))
    fetch || die "Couldn't fetch origin/main, so there's nothing to rebase onto or push to."
    base="$(git rev-parse origin/main)"
    now="$(git rev-parse HEAD)" || die "Couldn't read this checkout's HEAD."
    [ "$now" = "$expected" ] || die "This checkout moved under the landing (it was $(git rev-parse --short "$expected"), and is $(git rev-parse --short "$now")): look at what's here now, and run scripts/land.sh again."
    if git merge-base --is-ancestor HEAD origin/main; then
      echo "Nothing to land: origin/main ($(git rev-parse --short origin/main)) already has this branch's commits."
      exit 0
    fi
    rebased=0
    if ! git merge-base --is-ancestor origin/main HEAD; then
      rebased=1
      refuse_old_history
      itself="$(git hash-object "$self")" || die "Couldn't read $self."
      if ! git rebase -q origin/main >"$log" 2>&1; then
        echo "The rebase onto origin/main stopped on a conflict:" >&2
        git status --short | grep -E '^(UU|AA|DU|UD|AU|UA|DD) ' | head -20 >&2
        die "Resolve it, git rebase --continue, rerun what you'd tested, and run scripts/land.sh again."
      fi
      # main's own land.sh came in with the rebase: its checks are the ones to run, and bash has
      # this one in memory. Start again as it, still holding the turn.
      now="$(git hash-object "$self")" || die "Couldn't read $self after the rebase."
      if [ "$now" != "$itself" ]; then
        rm -f "$log" "$list"
        exec "$self" ${again[@]+"${again[@]}"}
      fi
    fi
    tested="$(git rev-parse HEAD)" || die "Couldn't read this checkout's HEAD after the rebase."
    # Moved by this script's rebase, or not at all.
    if [ "$rebased" = 0 ] && [ "$tested" != "$expected" ]; then
      die "This checkout moved under the landing (it was $(git rev-parse --short "$expected"), and is $(git rev-parse --short "$tested")): look at what's here now, and run scripts/land.sh again."
    fi
    expected="$tested"

    changed "$tested" || die "Couldn't list what these commits change, so there's no telling which checks they need."
    wants="$(plan_of <"$list")"
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
        # What's installed is what package.json and package-lock.json say, which these commits
        # or a rebase can have changed: npm ci also refuses the two when they disagree, as CI's
        # clean install would.
        local install=0
        case " $wants " in *" install "*) install=1 ;; esac
        if [ "$install" = 1 ] || [ ! -f node_modules/.package-lock.json ] || [ package-lock.json -nt node_modules/.package-lock.json ] || [ package.json -nt node_modules/.package-lock.json ]; then
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
    now="$(git rev-parse HEAD)" || die "Couldn't read this checkout's HEAD after the checks."
    dirty="$(git status --porcelain --untracked-files=normal)" || die "Couldn't ask git whether the checks left this checkout as it was."
    if [ "$now" != "$tested" ] || [ -n "$dirty" ]; then
      echo "The checkout changed while the checks ran, so what they checked isn't what's here now:" >&2
      printf '%s\n' "$dirty" | head -10 >&2
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
  let_go
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

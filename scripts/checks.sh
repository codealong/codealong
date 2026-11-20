#!/usr/bin/env bash
# Runs the browser checks (scripts/preview/*-check.js) as one set: builds the UI and takes its
# snapshots once, runs every check against that, prints a line for each with its result, and
# exits 1 if any failed.
#
#   scripts/checks.sh --fixture       the gate: the runs scripts/preview/fixture-passes.txt lists,
#                                     against the engine's made-up Mac, the same on every Mac and
#                                     every day; a failure there is the code's
#   scripts/checks.sh --fixture queue queue-check.js against the made-up Mac, listed or not
#   scripts/checks.sh --fixture --all every check against it
#   scripts/checks.sh                 every check, on this Mac's real data
#   scripts/checks.sh queue rename    only queue-check.js and rename-check.js, on real data
#   scripts/checks.sh --base …        on real data, the runs that failed are tried again with the
#                                     UI and the checks of the commit this branch left origin/main
#                                     at (--base=<commit> for another), over the same snapshots:
#                                     the ones that fail there too are this Mac's data, not your
#                                     change, and only the others count as failures (one the
#                                     base's run gave no result for among them: run it again)
#   scripts/checks.sh --jobs=6 …      the checks in six sets side by side, each in a copy of the
#                                     build and the snapshots, for a Mac with cores to spare:
#                                     the gate in minutes
#   scripts/checks.sh --fixture --listed queue   only the runs of queue-check.js the gate lists
#   scripts/checks.sh -v …            also what each check noted for a pass, and the commands
#                                     the stub had no answer for
#   scripts/checks.sh --results=<file> …  also writes a line for each run to <file>: its result
#                                     (PASS, FAIL, or NONE for a run that failed before its check
#                                     said anything), its check, its run line and its label, with
#                                     tabs between
#
# How a check runs is in its own header: each comment line there that is a whole preview.sh
# command for that file,
#   //   PREVIEW_WAIT=60000 PREVIEW_PIN=session scripts/preview.sh desktop '' scripts/preview/pr-chip-check.js
# (three spaces in, any PREVIEW_* variables, the view, the project or '', the script) is one
# run, with that view and those variables. A line with something left to fill in (<key>, <doc>)
# is for running by hand, on data picked for it; it's listed as not run.
#
# On real data the checks read whatever this Mac has, so one can fail for want of data (no
# session that used Chrome, nothing in Your turn): it says what it was missing, and --base tells
# such a failure from one a change made. With --fixture they read the made-up Mac
# (PREVIEW_FIXTURE in preview.sh, src-tauri/src/fixture.rs), where nothing is missing by chance.
# scripts/preview/fixture-passes.txt lists the runs that pass there, by their run lines, and
# --fixture alone runs exactly those: the checks to run before a change to the UI lands. A check
# that isn't listed assumes something of a real board the made-up one doesn't have, and is still
# real data's to run; the list's own header says how a run gets onto it. A listed run that no
# check has any more fails, so the list can't go stale unseen.
#
# preview.sh's own variables carry through: PREVIEW_BIN for another engine than this checkout's,
# PREVIEW_DIR, PREVIEW_SCHEME=dark. PREVIEW_PREPARE=skip runs against the build and the
# snapshots an earlier run left in $PREVIEW_DIR, for several sets of checks side by side, each
# in a copy of that folder.
#
# Each run gets as long as its PREVIEW_WAIT and a minute and a half more (two and a half minutes
# on virtual time), then it's stopped and fails: headless Chrome can hang on a busy Mac.
# gtimeout (coreutils) does the stopping when it's installed. A run whose page Chrome never
# showed gets one more go at once, and a check with a run that gave no result (that, a run
# stopped, a preview.sh that failed) gets another once the rest are through, by itself.
set -uo pipefail
cd "$(dirname "$0")/.."

verbose=0
fixture=0
all=0
only_listed=0
jobs=1
base=""
results=""
wanted=()
for arg in "$@"; do
  case "$arg" in
    -v | --verbose) verbose=1 ;;
    --fixture) fixture=1 ;;
    --all) all=1 ;;
    --listed) only_listed=1 ;;
    --jobs=*)
      jobs="${arg#--jobs=}"
      case "$jobs" in
        "" | *[!0-9]* | 0)
          echo "--jobs takes how many sets to run side by side: --jobs=4." >&2
          exit 2
          ;;
      esac
      ;;
    --base) base="merge-base" ;;
    --base=*) base="${arg#--base=}" ;;
    --results=*) results="${arg#--results=}" ;;
    -h | --help)
      sed -n '2,/^set -uo/p' "$0" | sed '$d; s/^# \{0,1\}//'
      exit 0
      ;;
    -*)
      echo "Unknown option $arg; see scripts/checks.sh --help." >&2
      exit 2
      ;;
    *)
      # queue, queue-check, queue-check.js and a path to it all name one check.
      name="$(basename "$arg" .js)"
      name="${name%-check}"
      if [ ! -f "scripts/preview/$name-check.js" ]; then
        echo "No check named $name (scripts/preview/$name-check.js)." >&2
        exit 2
      fi
      wanted+=("$name")
      ;;
  esac
done
# The gate: with --fixture and no checks named, only the runs listed as passing there; and
# with --listed, only those of the checks named.
passes=scripts/preview/fixture-passes.txt
gate=0
named=${#wanted[@]}
if [ $fixture -eq 1 ] && [ $all -eq 0 ] && { [ $named -eq 0 ] || [ $only_listed -eq 1 ]; }; then
  gate=1
  if [ ! -f "$passes" ]; then
    echo "No $passes to say which runs pass on the made-up Mac; scripts/checks.sh --fixture --all runs every check there." >&2
    exit 2
  fi
fi
if [ ${#wanted[@]} -eq 0 ]; then
  for file in scripts/preview/*-check.js; do
    name="$(basename "$file" .js)"
    wanted+=("${name%-check}")
  done
fi
# Whether a run line is one the list has. The list is a run line to a line, `#` for a comment.
listed() {
  [ -f "$passes" ] && grep -qxF -- "$1" "$passes"
}

# Only what a check's own run line sets reaches it.
prepared="${PREVIEW_PREPARE:-}"
unset PREVIEW_WAIT PREVIEW_TODOS PREVIEW_INSIGHTS PREVIEW_ROUTE PREVIEW_CANVAS PREVIEW_USAGE PREVIEW_DEVICES \
  PREVIEW_NOTICES PREVIEW_SKILLS PREVIEW_FOUND PREVIEW_TRANSCRIPT PREVIEW_LOADING PREVIEW_PIN PREVIEW_SIZE PREVIEW_NOTCH \
  PREVIEW_INIT PREVIEW_NAME PREVIEW_PORT PREVIEW_PREPARE PREVIEW_BOARDS PREVIEW_BOARDS_OFF PREVIEW_GROUP PREVIEW_VOICE PREVIEW_KEYS \
  PREVIEW_FIXTURE
[ $fixture -eq 0 ] || export PREVIEW_FIXTURE=1

# Runs a command for so many seconds at most; 124 says it was stopped, as gtimeout says it.
# Stopped, preview.sh takes its browser and its server with it.
timeout_cmd="$(command -v gtimeout || true)"
limited() {
  local secs="$1" run watch status
  shift
  if [ -n "$timeout_cmd" ]; then
    "$timeout_cmd" -k 10 "$secs" "$@"
    return $?
  fi
  "$@" &
  run=$!
  (
    nap=""
    trap '[ -z "$nap" ] || kill $nap 2>/dev/null; exit 0' TERM
    sleep "$secs" &
    nap=$!
    wait $nap && echo stopped >"$out.stopped" && kill -TERM "$run" 2>/dev/null
  ) &
  watch=$!
  wait "$run"
  status=$?
  # The watch's sleep goes first: a watch stopped between starting it and noting it leaves it.
  pkill -P "$watch" 2>/dev/null
  kill "$watch" 2>/dev/null
  wait "$watch" 2>/dev/null
  if [ -f "$out.stopped" ]; then
    rm -f "$out.stopped"
    return 124
  fi
  return $status
}

out="$(mktemp)"
work=""
trap 'rm -f "$out" "$out.err" "$out.stopped" "$out.runs" "$out.ran" "$out.kept"; [ -z "$work" ] || rm -rf "$work"' EXIT
: >"$out.runs"
: >"$out.ran"

if [ "$prepared" = skip ]; then
  echo "Running against what ${PREVIEW_DIR:-.preview} holds…"
else
  echo "Building the UI and $([ $fixture -eq 1 ] && echo "making up a board" || echo "reading the board")…"
  if ! PREVIEW_PREPARE=only scripts/preview.sh; then
    echo "scripts/preview.sh couldn't build the UI or take its snapshots." >&2
    exit 1
  fi
fi

passed=0
failed=0
skipped=0
unanswered=""
# One run's result, for --results and --base: PASS or FAIL as the check said, or NONE for a run
# that failed without the check saying (`said` below); then its check, run line and label.
record() {
  printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" >>"$out.runs"
}
# Every check wanted, one after another.
run_here() {
  for name in "${wanted[@]}"; do
    file="scripts/preview/$name-check.js"
    runs=0
    # The header is the comment the file starts with.
    while IFS= read -r line; do
      case "$line" in
        "//   "*"scripts/preview.sh "*"$file") ;;
        *) continue ;;
      esac
      runs=$((runs + 1))
      asked="${line#//   }"
      if [ $gate -eq 1 ]; then
        listed "$asked" || continue
        echo "$asked" >>"$out.ran"
      fi
      case "$line" in
        *"<"*)
          printf 'SKIP  %-22s %s\n' "$name" "by hand, on data picked for it: $asked"
          skipped=$((skipped + 1))
          continue
          ;;
      esac
      vars=()
      view=""
      project=""
      wait_ms=0
      label=""
      seen=0
      for word in $asked; do
        if [ $seen -eq 0 ]; then
          case "$word" in
            scripts/preview.sh) seen=1 ;;
            PREVIEW_WAIT=*)
              vars+=("$word")
              wait_ms="${word#PREVIEW_WAIT=}"
              ;;
            PREVIEW_*=*)
              vars+=("$word")
              label="$label ${word#PREVIEW_}"
              ;;
            *) seen=-1 ;;
          esac
        elif [ $seen -eq 1 ]; then
          view="$word"
          seen=2
        elif [ $seen -eq 2 ]; then
          [ "$word" = "''" ] || project="$word"
          seen=3
        fi
      done
      if [ $seen -ne 3 ]; then
        printf 'FAIL  %-22s %s\n' "$name" "its header's run line doesn't read as a preview.sh command: $asked"
        failed=$((failed + 1))
        record FAIL "$name" "$asked" ""
        continue
      fi
      label="$view${project:+ $project}$(echo "$label" | sed 's#scripts/preview/##g; s#-sample\.json##g')"
      vars+=(PREVIEW_PREPARE=skip)
      # On virtual time a run is over in seconds, or in a couple of minutes on a busy Mac.
      limit=150
      [ "$wait_ms" -eq 0 ] || limit=$((wait_ms / 1000 + 90))
      tries=0
      while :; do
        limited $limit env "${vars[@]}" scripts/preview.sh "$view" "$project" "$file" >"$out" 2>"$out.err" </dev/null
        status=$?
        # preview.sh's 3 is a page Chrome never showed, which says nothing of the check.
        if [ $status -ne 3 ] || [ $tries -eq 1 ]; then
          break
        fi
        tries=1
      done
      result="$(sed -n '1p' "$out")"
      # Whether the check itself said how it went. A page Chrome never showed, a run stopped for
      # taking too long and a preview.sh that failed say nothing of the check: failures all the
      # same, but --base doesn't take one on the base commit for the base failing too.
      said=FAIL
      [ $status -ne 3 ] || said=NONE
      if [ $status -eq 124 ] || [ $status -eq 137 ]; then
        # Stopped after it had its result (Chrome can hang once it's printed), the result stands.
        if [ -z "$result" ]; then
          result="FAIL 0/0 stopped after $limit s"
          said=NONE
        fi
      elif [ -z "$result" ]; then
        result="FAIL 0/0 scripts/preview.sh exited $status: $(sed -n '1p' "$out.err")"
        said=NONE
      elif [ $status -ne 0 ]; then
        case "$result" in
          FAIL\ *) ;;
          *)
            result="FAIL 0/0 scripts/preview.sh exited $status: $result"
            said=NONE
            ;;
        esac
      fi
      case "$result" in "FAIL 0/0 no result:"*) said=NONE ;; esac
      count="${result#* }"
      count="${count%% *}"
      why="${result#* }"
      why="${why#"$count"}"
      printf '%s  %-22s %-28s %s%s\n' "${result%% *}" "$name" "$label" "$count" "$why"
      case "$result" in
        PASS\ *)
          passed=$((passed + 1))
          record PASS "$name" "$asked" "$label"
          ;;
        *)
          failed=$((failed + 1))
          record "$said" "$name" "$asked" "$label"
          ;;
      esac
      # What the check noted is always worth a line; what went unanswered is for a failure, or -v.
      sed -n '2,$p' "$out" | while IFS= read -r more; do
        case "$more" in
          unanswered:*)
            case "$result" in PASS\ *) [ $verbose -eq 1 ] || continue ;; esac
            ;;
        esac
        printf '      %s\n' "$more"
      done
      unanswered="$unanswered$(sed -n 's/^unanswered: //p' "$out" | tr ',' '\n' | sed 's/^ *//')"$'\n'
    done < <(awk '/^\/\//{print; next} {exit}' "$file")
    if [ $runs -eq 0 ]; then
      printf 'FAIL  %-22s %s\n' "$name" "its header has no run line (//   … scripts/preview.sh <view> '' $file)"
      failed=$((failed + 1))
      record FAIL "$name" "" ""
    fi
  done
}
# --jobs: the checks in sets side by side, each set a run of this script over its share of them
# in a copy of the folder that holds the build and the snapshots, so no two write one page.
# Their lines are printed once all are through, in the checks' order, and their results put
# together as one run's.
run_side_by_side() {
  local dir="${PREVIEW_DIR:-.preview}" sets k n name item args pids=()
  sets="$(mktemp -d)"
  n=0
  for name in "${wanted[@]}"; do
    echo "$name" >>"$sets/names.$((n % jobs))"
    n=$((n + 1))
  done
  echo "Running them in $jobs sets side by side…"
  for k in $(seq 0 $((jobs - 1))); do
    [ -f "$sets/names.$k" ] || continue
    mkdir "$sets/$k"
    # The page and the snapshots, without this checkout's engine, which they all run from here.
    for item in "$dir"/*; do
      case "$(basename "$item")" in engine | ear) ;; *) cp -R "$item" "$sets/$k/" ;; esac
    done
    args=("--results=$sets/runs.$k")
    [ $fixture -eq 0 ] || args+=(--fixture)
    [ $gate -eq 0 ] || args+=(--listed)
    [ $verbose -eq 0 ] || args+=(-v)
    # shellcheck disable=SC2046
    CHECKS_SET=1 PREVIEW_PREPARE=skip PREVIEW_DIR="$sets/$k" scripts/checks.sh "${args[@]}" $(cat "$sets/names.$k") >"$sets/out.$k" 2>&1 &
    pids+=($!)
  done
  # While they run, a word a minute on how far along they are.
  n=0
  while :; do
    item=0
    for k in "${pids[@]}"; do ! kill -0 "$k" 2>/dev/null || item=1; done
    [ $item -eq 1 ] || break
    sleep 5
    n=$((n + 5))
    [ $((n % 60)) -ne 0 ] || echo "  $(cat "$sets"/out.* 2>/dev/null | grep -c -E '^(PASS|FAIL)  ') runs through…" >&2
  done
  wait "${pids[@]}"
  for name in "${wanted[@]}"; do
    # A check's lines in its set's output: its results, and what's noted under each.
    awk -v name="$name" 'FNR == 1 { on = 0 } /^(PASS|FAIL|SKIP)  /{ on = ($2 == name) } /^$/{ on = 0 } on' "$sets"/out.*
  done
  cat "$sets"/runs.* >>"$out.runs" 2>/dev/null
  cut -f3 "$out.runs" >"$out.ran"
  # A set that didn't get to its count stopped short: what it said instead, and a failure.
  for item in $(grep -L -E '^[0-9]+ passed, ' "$sets"/out.* 2>/dev/null); do
    echo "FAIL  a set of checks stopped short ($(tr '\n' ' ' <"$sets/names.${item##*.}")):"
    sed 's/^/      /' "$item" | tail -5
    stopped_short=$((${stopped_short:-0} + 1))
  done
  skipped=$(($(cat "$sets"/out.* | grep -c '^SKIP  ')))
  unanswered="$(sed -n 's/^Commands the stub had no answer for: //p' "$sets"/out.* | tr ' ' '\n')"$'\n'
  rm -rf "$sets"
}
if [ "$jobs" -gt 1 ] && [ ${#wanted[@]} -gt 1 ]; then
  run_side_by_side
else
  run_here
fi
# A run that gave no result (`said` above) says nothing of its check: Chrome hung as it started,
# or the Mac was too busy for the page to come in time. Those checks get one more go, one at a
# time now that the rest are through, and what they say then is what counts. Not in a set of a
# run side by side, whose results the run that started it goes over.
again="$(awk -F'\t' '$1 == "NONE" { print $2 }' "$out.runs" | sort -u)"
if [ -n "$again" ] && [ -z "${CHECKS_SET:-}" ]; then
  echo
  echo "Trying again the checks with a run that gave no result: $(echo $again)"
  awk -F'\t' -v again="$(echo $again)" 'BEGIN { n = split(again, name, " "); for (i = 1; i <= n; i++) gone[name[i]] = 1 } !($2 in gone)' "$out.runs" >"$out.kept"
  mv "$out.kept" "$out.runs"
  kept=$skipped
  # shellcheck disable=SC2206
  wanted=($again)
  run_here
  skipped=$kept
  [ $gate -eq 0 ] || cut -f3 "$out.runs" >"$out.ran"
fi
# Counted from what's recorded, whichever way they ran; a set that stopped short is one more.
stopped_short=${stopped_short:-0}
passed=$(($(grep -c '^PASS' "$out.runs")))
failed=$(($(grep -c -v '^PASS' "$out.runs") + stopped_short))
# The gate's list names runs: one no check has any more (its check renamed, its run line
# changed) would otherwise stop being checked without a word.
if [ $gate -eq 1 ] && [ $named -eq 0 ]; then
  while IFS= read -r asked; do
    case "$asked" in "" | "#"*) continue ;; esac
    if ! grep -qxF -- "$asked" "$out.ran"; then
      printf 'FAIL  %-22s %s\n' "$(basename "${asked##* }" -check.js)" "$passes lists a run no check has: $asked"
      failed=$((failed + 1))
      record FAIL "$(basename "${asked##* }" -check.js)" "$asked" ""
    fi
  done <"$passes"
fi
[ -z "$results" ] || cp "$out.runs" "$results"

echo
echo "$passed passed, $failed failed$([ $skipped -eq 0 ] || echo ", $skipped not run")."
# Every command the stub answered null, across the runs: a new one whose answer the page shows
# belongs in fake-tauri.js.
names="$(printf '%s' "$unanswered" | sed '/^$/d' | sort -u | tr '\n' ' ')"
[ -z "$names" ] || echo "Commands the stub had no answer for: ${names% }"
# Against the made-up Mac with more than the gate asked for: where the list and the runs differ.
if [ $fixture -eq 1 ] && [ $gate -eq 0 ] && [ -f "$passes" ]; then
  broke="$(awk -F'\t' '$1 != "PASS" { print $3 }' "$out.runs" | grep -xFf "$passes" | wc -l | tr -d ' ')"
  fresh="$(awk -F'\t' '$1 == "PASS" { print $3 }' "$out.runs" | grep -vxFf "$passes" | wc -l | tr -d ' ')"
  [ "$broke" -eq 0 ] || echo "$broke of the failures are runs $passes lists: they passed on the made-up Mac before."
  [ "$fresh" -eq 0 ] || echo "$fresh of the passes are runs $passes doesn't list; one that passes every time belongs there."
fi
[ -n "$base" ] || [ $failed -eq 0 ] || exit 1
[ $failed -gt 0 ] || exit 0

# --base: each run that failed, again with the base commit's own UI and checks over the same
# snapshots. One that fails there too failed before the change; the rest are the change's.
[ "$base" != merge-base ] || base="$(git merge-base origin/main HEAD 2>/dev/null || true)"
commit="$(git rev-parse --verify --quiet "$base^{commit}" 2>/dev/null || true)"
if [ -z "$commit" ]; then
  echo "--base: there's no commit ${base:-where this branch left origin/main} to compare with." >&2
  exit 1
fi
short="$(git rev-parse --short "$commit")"
dir="${PREVIEW_DIR:-.preview}"
echo
echo "Trying the $failed that failed on $short's UI and checks, over the same snapshots…"
work="$(mktemp -d)"
# The base's page, its checks and its stub, without its engine: the snapshots are already taken.
if ! git archive "$commit" -- . ':!src-tauri' | tar -x -C "$work" ||
  ! ln -s "$PWD/node_modules" "$work/node_modules" ||
  ! (cd "$work" && npx vite build >"$work/build.log" 2>&1) ||
  ! mkdir -p "$work/.preview" ||
  ! cp -R "$work/dist/." "$work/.preview/" ||
  ! cp "$dir"/*.json "$work/.preview/"; then
  cat "$work/build.log" >&2 2>/dev/null
  echo "--base: couldn't build $short's UI to compare with." >&2
  exit 1
fi
[ ! -f "$dir/snapshot-of" ] || cp "$dir/snapshot-of" "$work/.preview/"
# This script runs them there, so whatever the base's own did, the two runs are read alike.
cp scripts/checks.sh "$work/scripts/checks.sh"
there=(env -u PREVIEW_DIR -u PREVIEW_BIN PREVIEW_PREPARE=skip)
# The engine the snapshots came from, for a run that asks it for more (a conversation): the one
# named, else this checkout's as the first run left it, not built again now.
engine="${PREVIEW_BIN:-$PWD/.preview/engine/codealong}"
[ ! -x "$engine" ] || there+=("PREVIEW_BIN=$engine")
there+=(scripts/checks.sh "--results=$work/runs.tsv" "--jobs=$jobs")
[ $fixture -eq 0 ] || there+=(--fixture)
again=()
while IFS= read -r name; do
  [ -z "$name" ] || [ ! -f "$work/scripts/preview/$name-check.js" ] || again+=("$name")
done < <(awk -F'\t' '$1 != "PASS" { print $2 }' "$out.runs" | sort -u)
: >"$work/runs.tsv"
if [ ${#again[@]} -gt 0 ]; then
  # Its lines, without the first that says what it runs against and the count at the end.
  (cd "$work" && "${there[@]}" "${again[@]}") | sed '1d; /^$/,$d; s/^/    /'
fi
# Each failure here by what the base made of the same run of the same check: failing there
# too, as its check said; passing there; not a run of the base's (a new check, a run line that
# changed); or not settled, where the base's run failed before its check said anything.
verdicts="$(awk -F'\t' -v there="$work/runs.tsv" '
  BEGIN { while ((getline run < there) > 0) { split(run, part, FS); on_base[part[2] FS part[3]] = part[1] } }
  $1 != "PASS" { was = on_base[$2 FS $3]; print (was == "FAIL" ? "too" : was == "PASS" ? "new" : was == "NONE" ? "unsettled" : "gone") FS $2 FS $4 }' "$out.runs")"
echo
mine=0
while IFS=$'\t' read -r verdict name label; do
  case "$verdict" in
    too) printf 'fails on %s too  %-22s %s\n' "$short" "$name" "$label" ;;
    new)
      printf 'FAIL here only    %-22s %s\n' "$name" "$label"
      mine=$((mine + 1))
      ;;
    gone)
      printf 'FAIL, not on %s  %-22s %s\n' "$short" "$name" "$label"
      mine=$((mine + 1))
      ;;
    unsettled)
      printf 'FAIL, no result on %s  %-22s %s\n' "$short" "$name" "$label"
      mine=$((mine + 1))
      ;;
  esac
done <<<"$verdicts"
echo
echo "$failed failed: $((failed - mine)) on $short too, $mine not."
[ $mine -eq 0 ]

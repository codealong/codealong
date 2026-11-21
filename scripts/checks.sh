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
#   scripts/checks.sh --base …        on real data (never with --fixture), the runs that failed
#                                     are tried again with the UI and the checks of the commit
#                                     this branch left origin/main at (--base=<commit> for
#                                     another), over the same snapshots: the ones that fail there
#                                     too, on the same things, are this Mac's data, not your
#                                     change, and only the others count as failures. Among the
#                                     others: one that fails there on something else, or on
#                                     fewer, one whose report is cut short, here or there (it
#                                     names only its first failures: look at it), and one that
#                                     gave no result here or there (run it again)
#   scripts/checks.sh --jobs=4 …      the checks across four persistent headless Chrome workers
#                                     (scripts/preview/runner.mjs, sharing one in-memory HTTP
#                                     server), or with CHECKS_LEGACY=1 four sets of preview.sh
#                                     spawns in copies of the build and snapshots: the gate in
#                                     minutes
#   scripts/checks.sh --fixture --listed queue   only the runs of queue-check.js the gate lists
#   scripts/checks.sh --prepared …    against the UI and the snapshots an earlier run left in the
#                                     folder, building and reading nothing again: its last line
#                                     says the UI wasn't built, so what passed isn't taken for
#                                     today's code. Only this asks for it: a PREVIEW_PREPARE left
#                                     exported does nothing here
#   scripts/checks.sh -v …            also what each check noted for a pass, and the commands
#                                     the stub had no answer for
#   scripts/checks.sh --results=<file> …  also writes a line for each run to <file>: its result
#                                     (PASS, FAIL, or NONE for a run that failed before its check
#                                     said anything), its check, its run line, its label, what
#                                     failed as the check worded it and how many passed of how
#                                     many (7/9), with tabs between
#
# How a check runs is in its own header: each comment line there that is a whole preview.sh
# command for that file,
#   //   PREVIEW_WAIT=60000 PREVIEW_PIN=session scripts/preview.sh desktop '' scripts/preview/pr-chip-check.js
# (three spaces in, any PREVIEW_* variables, the view, the project or '', the script) is one
# run, with that view and those variables. A line with something left to fill in (<key>, <doc>)
# is for running by hand, on data picked for it; it's listed as not run, and the gate fails on
# one in its list, which would pass every time by not being run.
#
# On real data the checks read whatever this Mac has, so one can fail for want of data (no
# session that used Chrome, nothing in Your turn): it says what it was missing, and --base tells
# such a failure from one a change made. With --fixture they read the made-up Mac
# (PREVIEW_FIXTURE in preview.sh, src-tauri/src/fixture.rs), where nothing is missing by chance.
# scripts/preview/fixture-passes.txt lists the runs that pass there, by their run lines, and
# --fixture alone runs exactly those. GitHub runs them on main after what lands
# (.github/workflows/checks.yml); before a change to the UI lands, run the ones that cover it,
# by name, rather than the list, which is half an hour of a busy Mac. A check
# that isn't listed assumes something of a real board the made-up one doesn't have, and is still
# real data's to run; the list's own header says how a run gets onto it. A listed run that no
# check has any more fails, so the list can't go stale unseen.
#
# On real data preview.sh's own variables carry through: PREVIEW_BIN for another engine than
# this checkout's, PREVIEW_DIR, PREVIEW_SCHEME=dark. With --fixture only PREVIEW_BIN and
# PREVIEW_DIR do, and every other PREVIEW_* variable is taken away, so the made-up Mac's runs
# are the same whoever starts them; its build and snapshots are in .preview/fixture, apart from
# this Mac's in .preview.
#
# By default, checks run over persistent headless Chrome workers (scripts/preview/runner.mjs) on
# the real clock with isolated BrowserContexts; set CHECKS_LEGACY=1 to fall back to per-check
# preview.sh spawns. Each run gets as long as its PREVIEW_WAIT and a minute and a half more (two
# and a half minutes when PREVIEW_WAIT is unset), then it's stopped and fails: headless Chrome can
# hang on a busy machine (gtimeout stops legacy runs when installed; runner.mjs enforces per-call
# CDP timeouts and a hard deadline before setup). A run whose page Chrome never showed gets one
# more go at once, and a run that gave no result (that, a run stopped, a preview.sh or runner.mjs
# that failed, one a set of --jobs never got to before it stopped short) gets another once the rest
# are through, by itself; every run that said how it went keeps what it said.
set -uo pipefail
cd "$(dirname "$0")/.."

verbose=0
fixture=0
all=0
only_listed=0
jobs=1
base=""
results=""
prepared=0
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
    --prepared) prepared=1 ;;
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
# --base tells what this Mac's data makes fail from what a change does. On the made-up Mac there's
# nothing to tell: a run that fails there fails for the code. And a base from before the made-up
# Mac's page was kept apart would put this Mac's images and conversations in it.
if [ $fixture -eq 1 ] && [ -n "$base" ]; then
  echo "--base goes with this Mac's data, not with --fixture: what fails on the made-up Mac fails for the code." >&2
  exit 2
fi
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
# The checks this run is of, whichever of them get another go later.
asked_for=("${wanted[@]}")
# Whether a run line is one the list has. The list is a run line to a line, `#` for a comment.
listed() {
  [ -f "$passes" ] && grep -qxF -- "$1" "$passes"
}

# The folder the build and the snapshots are in, as scripts/preview.sh has it: the made-up Mac's
# apart from this Mac's.
dir="${PREVIEW_DIR:-.preview}"
[ $fixture -eq 0 ] || dir="${PREVIEW_DIR:-.preview/fixture}"
# Only what a check's own run line sets reaches it. PREVIEW_PREPARE among them: left exported,
# it would have this run skip the build and pass on the UI of whenever that folder was built
# (--prepared asks for that, and the run says so).
unset PREVIEW_WAIT PREVIEW_DATE PREVIEW_TODOS PREVIEW_INSIGHTS PREVIEW_ROUTE PREVIEW_CANVAS PREVIEW_USAGE PREVIEW_DEVICES \
  PREVIEW_NOTICES PREVIEW_SKILLS PREVIEW_FOUND PREVIEW_TRANSCRIPT PREVIEW_LOADING PREVIEW_PIN PREVIEW_SIZE PREVIEW_NOTCH \
  PREVIEW_INIT PREVIEW_NAME PREVIEW_PORT PREVIEW_PREPARE PREVIEW_BOARDS PREVIEW_BOARDS_OFF PREVIEW_GROUP PREVIEW_VOICE PREVIEW_KEYS \
  PREVIEW_FIXTURE PREVIEW_POLL PREVIEW_WAIT_POLL
if [ $fixture -eq 1 ]; then
  # On the made-up Mac nothing else reaches one at all, so its runs are the same whoever runs
  # them: a PREVIEW_PLATFORM=linux left exported would have the gate's runs check Linux's keys,
  # and pass with macOS's broken. The engine and the folder are the two that say where, not what.
  for var in $(compgen -v PREVIEW_); do
    case "$var" in PREVIEW_BIN | PREVIEW_DIR) ;; *) unset "$var" ;; esac
  done
  export PREVIEW_FIXTURE=1
fi
export PREVIEW_DIR="$dir"

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
# What's said last of all, whichever way the run ends: that the UI wasn't built (--prepared).
unbuilt=""
trap 'rm -f "$out" "$out.err" "$out.stopped" "$out.runs" "$out.again" "$out.kept" "$out.plan.json" "$out.static" "$out.cdp.ndjson" "$out.cdp.err"; [ -z "$work" ] || rm -rf "$work"; [ -z "$unbuilt" ] || echo "$unbuilt"' EXIT
: >"$out.runs"
: >"$out.again"
# A set of a run side by side says each result as it has it (--results), so that one stopped
# part way has said all it found; any other run writes its results once, at the end.
publish=""
if [ -n "${CHECKS_SET:-}" ] && [ -n "$results" ]; then
  publish="$results"
  : >"$publish"
fi

if [ $prepared -eq 1 ]; then
  echo "Running against what $dir holds…"
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
# that failed without the check saying (`said` below); then its check, run line and label, for
# a failure what failed, as the check worded it (`<name>: <why>; <name>: <why>`), and how many
# passed of how many (`7/9`).
record() {
  local row
  row="$(printf '%s\t%s\t%s\t%s\t%s\t%s' "$1" "$2" "$3" "$4" "${5:-}" "${6:-}")"
  printf '%s\n' "$row" >>"$out.runs"
  [ -z "$publish" ] || printf '%s\n' "$row" >>"$publish"
}
# Whether a run has a row already: its check and its run line, the whole of each.
ran() {
  check="$1" asked="$2" awk -F'\t' '$2 == ENVIRON["check"] && $3 == ENVIRON["asked"] { found = 1 } END { exit !found }' "$out.runs"
}
# Whether a run is one to go again: with nothing to go again, every run is; else one that's
# named, and of a check whose set stopped short, which names none, each it left no row for.
going_again() {
  [ -s "$out.again" ] || return 0
  ! grep -qxF -- "$1"$'\t'"$2" "$out.again" || return 0
  grep -qxF -- "$1"$'\t' "$out.again" && ! ran "$1" "$2"
}
# Whether a results file has a row whose field `$2` is `$3`, the whole of it. (Not `cut | grep
# -q`: under pipefail a grep that has its answer fails the cut still writing to it.)
has_row() {
  [ -f "$1" ] && value="$3" awk -F'\t' -v field="$2" '$field == ENVIRON["value"] { found = 1 } END { exit !found }' "$1"
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
      fi
      going_again "$name" "$asked" || continue
      case "$line" in
        *"<"*)
          # Listed, it would pass the gate every time by not being run.
          if [ $gate -eq 1 ]; then
            printf 'FAIL  %-22s %s\n' "$name" "$passes lists a run that's made by hand, on data picked for it: $asked"
            failed=$((failed + 1))
            record FAIL "$name" "$asked" "" "listed, but made by hand"
            continue
          fi
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
        record FAIL "$name" "$asked" "" "its header's run line doesn't read as a preview.sh command"
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
          record PASS "$name" "$asked" "$label" "" "$count"
          ;;
        *)
          failed=$((failed + 1))
          record "$said" "$name" "$asked" "$label" "${why# }" "$count"
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
      record FAIL "$name" "" "" "its header has no run line"
    fi
  done
}
# --jobs: the checks in sets side by side, each set a run of this script over its share of them
# in a copy of the folder that holds the build and the snapshots, so no two write one page.
# Their lines are printed once all are through, in the checks' order, and their results put
# together as one run's.
run_side_by_side() {
  local sets k n name item args pids=()
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
    # The page and the snapshots, without this checkout's engine, which they all run from here,
    # and without the made-up Mac's folder, when this is this Mac's.
    for item in "$dir"/*; do
      case "$(basename "$item")" in engine | ear | fixture) ;; *) cp -R "$item" "$sets/$k/" ;; esac
    done
    args=("--results=$sets/runs.$k")
    [ $fixture -eq 0 ] || args+=(--fixture)
    [ $gate -eq 0 ] || args+=(--listed)
    [ $verbose -eq 0 ] || args+=(-v)
    # shellcheck disable=SC2046
    CHECKS_SET=1 PREVIEW_DIR="$sets/$k" scripts/checks.sh --prepared "${args[@]}" $(cat "$sets/names.$k") >"$sets/out.$k" 2>&1 &
    pids+=($!)
  done
  # While they run, a word a minute on how far along they are. Poll every 200 ms rather than
  # sleeping 5 s at a stretch so fast runs and tests don't sit idle after every set has ended.
  next=$((SECONDS + 60))
  while :; do
    item=0
    for k in "${pids[@]}"; do ! kill -0 "$k" 2>/dev/null || item=1; done
    [ $item -eq 1 ] || break
    sleep 0.2
    if [ "$SECONDS" -ge "$next" ]; then
      echo "  $(cat "$sets"/out.* 2>/dev/null | grep -c -E '^(PASS|FAIL)  ') runs through…" >&2
      next=$((SECONDS + 60))
    fi
  done
  wait "${pids[@]}"
  for name in "${wanted[@]}"; do
    # A check's lines in its set's output: its results, and what's noted under each.
    awk -v name="$name" 'FNR == 1 { on = 0 } /^(PASS|FAIL|SKIP)  /{ on = ($2 == name) } /^$/{ on = 0 } on' "$sets"/out.*
  done
  cat "$sets"/runs.* >>"$out.runs" 2>/dev/null
  # A set that didn't get to its count stopped short: what it said instead. It said each result
  # as it had it, and those stand: a failure it found isn't to be run away by another go. What
  # it never got to has no row, so each of its checks is recorded as having given no result,
  # with no run named, and the runs without a row go again: a failure that stands, and that
  # --base can see, unless its other go says otherwise.
  for item in $(grep -L -E '^[0-9]+ passed, ' "$sets"/out.* 2>/dev/null); do
    k="${item##*.}"
    echo "FAIL  a set of checks stopped short ($(tr '\n' ' ' <"$sets/names.$k")):"
    sed 's/^/      /' "$item" | tail -5
    while IFS= read -r name; do
      record NONE "$name" "" "" "its set of checks stopped short"
    done <"$sets/names.$k"
  done
  skipped=$(($(cat "$sets"/out.* | grep -c '^SKIP  ')))
  unanswered="$(sed -n 's/^Commands the stub had no answer for: //p' "$sets"/out.* | tr ' ' '\n')"$'\n'
  rm -rf "$sets"
}
# Persistent-Chrome CDP runner (scripts/preview/runner.mjs): runs all checks in one in-memory
# HTTP server and `$jobs` long-lived headless Chrome instances (one isolated BrowserContext per
# check) instead of spawning a cold Chrome, Python server and `ps` scan per check. Used whenever
# runner.mjs and node are present unless CHECKS_LEGACY=1 asks for per-check preview.sh spawns.
run_cdp() {
  local worker_jobs="$1"
  local plan="$out.plan.json"
  local cdp_out="$out.cdp.ndjson"
  local cdp_err="$out.cdp.err"
  local bin_arg=()
  local engine="${PREVIEW_BIN:-.preview/engine/codealong}"
  [ ! -x "$engine" ] || bin_arg=(--bin "$engine")
  local fix_arg=()
  [ $fixture -eq 0 ] || fix_arg=(--fixture)

  # Build the run plan JSON using the exact same header parsing and filtering rules as run_here,
  # and collect any static SKIP/FAIL rows (missing header, malformed line, <key> hand-run).
  node - "$plan" "$out.static" "$gate" "$passes" "$out.again" "$out.runs" "${wanted[@]}" <<'EOF'
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { basename } from 'node:path';

const [planPath, staticPath, gateStr, passesPath, againPath, runsPath, ...wanted] = process.argv.slice(2);
const gate = gateStr === '1';
const listedSet = new Set(
  gate && existsSync(passesPath)
    ? readFileSync(passesPath, 'utf8').split('\n').map((l) => l.replace(/\r$/, ''))
    : [],
);
const againLines = existsSync(againPath) ? readFileSync(againPath, 'utf8').split('\n').filter(Boolean) : [];
const hasAgain = againLines.length > 0;
const againExact = new Set(againLines);
const ranExact = new Set();
if (existsSync(runsPath)) {
  for (const line of readFileSync(runsPath, 'utf8').split('\n')) {
    if (!line) continue;
    const cols = line.split('\t');
    ranExact.add(`${cols[1]}\t${cols[2]}`);
  }
}
const goingAgain = (name, asked) => {
  if (!hasAgain) return true;
  if (againExact.has(`${name}\t${asked}`)) return true;
  return againExact.has(`${name}\t`) && !ranExact.has(`${name}\t${asked}`);
};

// Inherit exported PREVIEW_* variables (e.g. PREVIEW_SCHEME=dark on real data) that checks.sh
// intentionally kept; per-run header variables override these.
const inherited = {};
for (const [k, v] of Object.entries(process.env)) {
  if (!k.startsWith('PREVIEW_') || v === undefined) continue;
  if (['PREVIEW_DIR', 'PREVIEW_BIN', 'PREVIEW_FIXTURE', 'PREVIEW_PREPARE', 'PREVIEW_CHROME', 'PREVIEW_POLL', 'PREVIEW_WAIT_POLL'].includes(k)) continue;
  inherited[k] = v;
}

const plan = [];
const staticRows = [];
for (const name of wanted) {
  const file = `scripts/preview/${name}-check.js`;
  const lines = readFileSync(file, 'utf8').split('\n');
  let runs = 0;
  for (const line of lines) {
    if (!line.startsWith('//')) break;
    if (!line.startsWith('//   ') || !line.includes('scripts/preview.sh ') || !line.endsWith(file)) continue;
    runs++;
    const asked = line.slice(5);
    if (gate && !listedSet.has(asked)) continue;
    if (!goingAgain(name, asked)) continue;
    if (line.includes('<')) {
      if (gate) {
        staticRows.push({ kind: 'FAIL', name, asked, label: '', why: 'listed, but made by hand', print: `${passesPath} lists a run that's made by hand, on data picked for it: ${asked}` });
      } else {
        staticRows.push({ kind: 'SKIP', name, asked, label: '', why: '', print: `by hand, on data picked for it: ${asked}` });
      }
      continue;
    }
    const vars = { ...inherited };
    let view = '';
    let project = '';
    let labelExtra = '';
    let seen = 0;
    for (const word of asked.split(/\s+/).filter(Boolean)) {
      if (seen === 0) {
        if (word === 'scripts/preview.sh') seen = 1;
        else if (word.startsWith('PREVIEW_') && word.includes('=')) {
          const eq = word.indexOf('=');
          const k = word.slice(0, eq);
          const v = word.slice(eq + 1);
          vars[k] = v;
          if (k !== 'PREVIEW_WAIT') labelExtra += ` ${word.slice('PREVIEW_'.length)}`;
        } else {
          seen = -1;
        }
      } else if (seen === 1) {
        view = word;
        seen = 2;
      } else if (seen === 2) {
        project = word === "''" ? '' : word;
        seen = 3;
      }
    }
    if (seen !== 3) {
      staticRows.push({ kind: 'FAIL', name, asked, label: '', why: "its header's run line doesn't read as a preview.sh command", print: `its header's run line doesn't read as a preview.sh command: ${asked}` });
      continue;
    }
    const label = `${view}${project ? ` ${project}` : ''}${labelExtra.replace(/scripts\/preview\//g, '').replace(/-sample\.json/g, '')}`;
    plan.push({ id: plan.length, name, asked, view, project, check: file, label, vars });
  }
  if (runs === 0) {
    staticRows.push({ kind: 'FAIL', name, asked: '', label: '', why: 'its header has no run line', print: `its header has no run line (//   … scripts/preview.sh <view> '' ${file})` });
  }
}
writeFileSync(planPath, JSON.stringify(plan));
writeFileSync(staticPath, JSON.stringify(staticRows));
EOF

  # Emit static rows first (matching run_here).
  while IFS=$'\037' read -r kind name asked label why print_msg; do
    [ -n "$kind" ] || continue
    if [ "$kind" = SKIP ]; then
      printf 'SKIP  %-22s %s\n' "$name" "$print_msg"
      skipped=$((skipped + 1))
    else
      printf 'FAIL  %-22s %s\n' "$name" "$print_msg"
      failed=$((failed + 1))
      record FAIL "$name" "$asked" "$label" "$why"
    fi
  done < <(node -e '
    const rows = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8"));
    for (const r of rows) console.log([r.kind, r.name, r.asked, r.label, r.why, r.print].join("\x1f"));
  ' "$out.static")

  local count_plan
  count_plan="$(node -e 'console.log(JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8")).length)' "$plan")"
  [ "$count_plan" -gt 0 ] || return 0

  if [ "$worker_jobs" -gt 1 ]; then
    echo "Running $count_plan runs across $worker_jobs persistent Chrome workers…"
  fi
  : >"$cdp_out"
  : >"$cdp_err"
  node scripts/preview/runner.mjs --dir "$dir" --plan "$plan" --jobs "$worker_jobs" ${fix_arg[@]+"${fix_arg[@]}"} ${bin_arg[@]+"${bin_arg[@]}"} >"$cdp_out" 2>"$cdp_err" &
  local runner_pid=$!
  local next=$((SECONDS + 60))
  while kill -0 "$runner_pid" 2>/dev/null; do
    sleep 0.2
    if [ "$SECONDS" -ge "$next" ]; then
      echo "  $(wc -l <"$cdp_out" | tr -d ' ')/$count_plan runs through…" >&2
      next=$((SECONDS + 60))
    fi
  done
  wait "$runner_pid"
  local runner_status=$?
  if [ "$runner_status" -ne 0 ]; then
    [ ! -s "$cdp_err" ] || cat "$cdp_err" >&2
  fi
  # A fatal startup refusal (missing/mismatched snapshot-of, old engine, missing Chrome) exits
  # before producing any results: fail up front instead of retrying or printing bogus gate rows.
  if [ "$runner_status" -ne 0 ] && [ ! -s "$cdp_out" ]; then
    echo "runner.mjs couldn't start (exit $runner_status): $(sed -n '1p' "$cdp_err")" >&2
    exit 1
  fi

  # Render results in the checks' original order and record every run.
  while IFS=$'\037' read -r tag name asked label said count why extra_json; do
    [ -n "$tag" ] || continue
    printf '%s  %-22s %-28s %s%s\n' "$tag" "$name" "$label" "$count" "${why:+ $why}"
    if [ "$tag" = PASS ]; then
      passed=$((passed + 1))
      record PASS "$name" "$asked" "$label" "" "$count"
    else
      failed=$((failed + 1))
      record "$said" "$name" "$asked" "$label" "$why" "$count"
    fi
    while IFS= read -r more; do
      [ -n "$more" ] || continue
      case "$more" in
        unanswered:*)
          unanswered="$unanswered$(printf '%s' "${more#unanswered: }" | tr ',' '\n' | sed 's/^ *//')"$'\n'
          [ "$tag" != PASS ] || [ $verbose -eq 1 ] || continue
          ;;
      esac
      printf '      %s\n' "$more"
    done < <(node -e 'for (const s of JSON.parse(process.argv[1])) console.log(s);' "$extra_json")
  done < <(node - "$plan" "$cdp_out" <<'EOF'
import { readFileSync } from 'node:fs';
const plan = JSON.parse(readFileSync(process.argv[2], 'utf8'));
const byId = new Map();
for (const line of readFileSync(process.argv[3], 'utf8').split('\n')) {
  if (!line.trim()) continue;
  try {
    const obj = JSON.parse(line);
    byId.set(obj.id, obj);
  } catch {}
}
for (const r of plan) {
  const got = byId.get(r.id);
  let status = got ? got.status : 1;
  let result = got ? got.result : 'FAIL 0/0 no result: runner stopped before this check';
  const extraLines = got?.extraLines || [];
  let said = 'FAIL';
  if (!got || status === 3 || status === 124 || status === 137) {
    if (!result || status === 3 || result.startsWith('FAIL 0/0 stopped after ') || result.startsWith('FAIL 0/0 no result:')) said = 'NONE';
  }
  if (result.startsWith('FAIL 0/0 no result:')) said = 'NONE';
  const sp = result.indexOf(' ');
  const tag = sp > 0 ? result.slice(0, sp) : 'FAIL';
  const rest = sp > 0 ? result.slice(sp + 1) : '';
  const sp2 = rest.indexOf(' ');
  const count = sp2 > 0 ? rest.slice(0, sp2) : rest;
  const why = sp2 > 0 ? rest.slice(sp2 + 1) : '';
  console.log([tag, r.name, r.asked, r.label, said, count, why, JSON.stringify(extraLines)].join('\x1f'));
}
EOF
  )
  rm -f "$plan" "$out.static" "$cdp_out" "$cdp_err"
}

use_cdp=0
if [ -z "${CHECKS_LEGACY:-}" ] && [ -f scripts/preview/runner.mjs ] && command -v node >/dev/null 2>&1; then
  use_cdp=1
fi
if [ "$use_cdp" -eq 1 ]; then
  run_cdp "$jobs"
elif [ "$jobs" -gt 1 ] && [ ${#wanted[@]} -gt 1 ]; then
  run_side_by_side
else
  run_here
fi
# A run that gave no result (`said` above) says nothing of its check: Chrome hung as it started,
# or the Mac was too busy for the page to come in time. Those runs get one more go, one at a
# time now that the rest are through, and what they say then is what counts. Only those: a run
# of the same check that did say how it went keeps what it said. (A check whose set stopped
# short has no run to name: the ones it left no row for go again.) Not in a set of a run side by
# side, whose results the run that started it goes over.
awk -F'\t' '$1 == "NONE" { print $2 FS $3 }' "$out.runs" >"$out.again"
if [ -s "$out.again" ] && [ -z "${CHECKS_SET:-}" ]; then
  again="$(cut -f1 "$out.again" | sort -u)"
  echo
  echo "Trying again the runs that gave no result, of: $(echo $again)"
  awk -F'\t' '$1 != "NONE"' "$out.runs" >"$out.kept"
  mv "$out.kept" "$out.runs"
  kept=$skipped
  # shellcheck disable=SC2206
  wanted=($again)
  if [ "$use_cdp" -eq 1 ]; then
    run_cdp 1
  else
    run_here
  fi
  skipped=$kept
fi
# The gate's list names runs: one no check has any more (its check renamed, its run line
# changed) would otherwise stop being checked without a word. With checks named, their lines.
if [ $gate -eq 1 ]; then
  # (`read` fails at a last line with no newline after it, having read it.)
  while IFS= read -r asked || [ -n "$asked" ]; do
    case "$asked" in "" | "#"*) continue ;; esac
    name="$(basename "${asked##* }" -check.js)"
    [ $named -eq 0 ] || case " ${asked_for[*]} " in *" $name "*) ;; *) continue ;; esac
    if ! has_row "$out.runs" 3 "$asked"; then
      printf 'FAIL  %-22s %s\n' "$name" "$passes lists a run no check has: $asked"
      record FAIL "$name" "$asked" "" "listed, but no check has it"
    fi
  done <"$passes"
fi
# Counted from what's recorded, whichever way they ran.
passed=$(($(grep -c '^PASS' "$out.runs")))
failed=$(($(grep -c -v '^PASS' "$out.runs")))
[ -z "$results" ] || [ -n "$publish" ] || cp "$out.runs" "$results"

echo
echo "$passed passed, $failed failed$([ $skipped -eq 0 ] || echo ", $skipped not run")."
# Said last, by the run someone started and not by a set of it.
if [ $prepared -eq 1 ] && [ -z "${CHECKS_SET:-}" ]; then
  unbuilt="Not built (--prepared): these ran against the UI and the snapshots $dir already held, which may not be this checkout's code."
fi
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
echo
echo "Trying the $failed that failed on $short's UI and checks, over the same snapshots…"
work="$(mktemp -d)"
# What the base's UI is built with: this checkout's packages when the base asks for the same, and
# its own, installed for it, when it doesn't. A failure that an update to a package made would
# fail on both UIs otherwise, and be taken for the data's.
packages() {
  if cmp -s package-lock.json "$work/package-lock.json"; then
    ln -s "$PWD/node_modules" "$work/node_modules"
    return
  fi
  echo "  $short asks for other packages than this checkout has: installing its own…"
  (cd "$work" && npm ci --no-audit --no-fund >"$work/build.log" 2>&1)
}
# The base's page, its checks and its stub, without its engine: the snapshots are already taken.
if ! git archive "$commit" -- . ':!src-tauri' | tar -x -C "$work" ||
  ! packages ||
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
[ ! -f scripts/preview/runner.mjs ] || cp scripts/preview/runner.mjs "$work/scripts/preview/runner.mjs"
there=(env -u PREVIEW_DIR -u PREVIEW_BIN)
# The engine the snapshots came from, for a run that asks it for more (a conversation): the one
# named, else this checkout's as the first run left it, not built again now. By its whole path,
# as it's run from the base's folder.
engine="${PREVIEW_BIN:-.preview/engine/codealong}"
case "$engine" in /*) ;; *) engine="$PWD/$engine" ;; esac
[ ! -x "$engine" ] || there+=("PREVIEW_BIN=$engine")
there+=(scripts/checks.sh --prepared "--results=$work/runs.tsv" "--jobs=$jobs")
again=()
while IFS= read -r name; do
  [ -z "$name" ] || [ ! -f "$work/scripts/preview/$name-check.js" ] || again+=("$name")
done < <(awk -F'\t' '$1 != "PASS" { print $2 }' "$out.runs" | sort -u)
: >"$work/runs.tsv"
if [ ${#again[@]} -gt 0 ]; then
  # Its lines, without the first that says what it runs against and the count at the end.
  (cd "$work" && "${there[@]}" "${again[@]}") | sed '1d; /^$/,$d; s/^/    /'
fi
# Each failure here by what the base made of the same run of the same check. Only one whose
# check said what failed, and said the same there, is the data's (`too`): every `<name>: <why>`
# that fails here fails so on the base, and no more fail here than there. The rest count
# against the change: passing there; failing there on something else, which a new failure could
# hide behind (`other`), or on fewer (`more`); a report cut short, here or there (`cut`); not a
# run of the base's (a new check, a run line that changed); and a run that gave no result, here
# or there, which says nothing of the check either way.
#
# A report is cut short when its failures don't fit: prelude.js `done` keeps the first 2,400
# characters of the list, the last of them an ellipsis (`cut_len`; src/checks.test.ts holds the
# number here to the one there). What it leaves out can't be told from what the base's leaves out,
# whatever the two have in common: nine failures both have, then `opens` failing there and
# `saves` here, are the same words and the same count. So it's never the base's: it needs a
# look. (One `why` cut at its own, shorter length ends with an ellipsis too, with every
# failure still named: that one is read like any other.) Lengths are counted in bytes, which
# no cut list has fewer of than it has characters.
#
# (A verdict's parts are set apart by a character no label has, and not a tab: read takes tabs
# side by side for one, and a label can be empty.)
cut_len=2400
verdicts="$(LC_ALL=C awk -F'\t' -v there="$work/runs.tsv" -v sep=$'\037' -v cut_at="…" -v cut_len="$cut_len" '
  # How many failed, of a count as a row has it (7/9); -1 for a row without one.
  function failures(count,    part) { return split(count, part, "/") == 2 ? part[2] - part[1] : -1 }
  function is_cut(text) { return length(text) >= cut_len && substr(text, length(text) - length(cut_at) + 1) == cut_at }
  BEGIN { while ((getline run < there) > 0) { split(run, part, FS); k = part[2] FS part[3]; on_base[k] = part[1]; failed_there[k] = part[5]; count_there[k] = part[6] } }
  $1 == "PASS" { next }
  {
    k = $2 FS $3
    was = on_base[k]
    verdict = $1 == "NONE" ? "unsaid" : was == "PASS" ? "new" : was == "NONE" ? "unsettled" : was == "FAIL" ? "too" : "gone"
    only = ""
    if (verdict == "too") {
      counts = $6 " here, " count_there[k] " there"
      if (is_cut($5) || is_cut(failed_there[k])) {
        verdict = "cut"
        only = counts
      } else {
        split("", there_too)
        n = split(failed_there[k], part, "; ")
        for (i = 1; i <= n; i++) there_too[part[i]] = 1
        n = split($5, part, "; ")
        for (i = 1; i <= n; i++) if (!(part[i] in there_too)) only = only (only == "" ? "" : "; ") part[i]
        if (only != "") verdict = "other"
        else if (failures($6) > failures(count_there[k]) && failures(count_there[k]) >= 0) { verdict = "more"; only = counts }
      }
    }
    print verdict sep $2 sep $4 sep only
  }' "$out.runs")"
echo
mine=0
while IFS=$'\037' read -r verdict name label only; do
  case "$verdict" in
    too) printf 'fails on %s too  %-22s %s\n' "$short" "$name" "$label" ;;
    new)
      printf 'FAIL here only    %-22s %s\n' "$name" "$label"
      mine=$((mine + 1))
      ;;
    other)
      printf 'FAIL, not as on %s  %-22s %s\n' "$short" "$name" "$label"
      printf '      here only: %s\n' "$only"
      mine=$((mine + 1))
      ;;
    more)
      printf 'FAIL, more than on %s  %-22s %s\n' "$short" "$name" "$label"
      printf '      %s\n' "$only"
      mine=$((mine + 1))
      ;;
    cut)
      printf 'FAIL, its report cut short  %-22s %s\n' "$name" "$label"
      printf "      %s: only the first failures are named, so it can't be told from %s's: look at it\n" "$only" "$short"
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
    unsaid)
      printf 'FAIL, no result here  %-22s %s\n' "$name" "$label"
      mine=$((mine + 1))
      ;;
  esac
done <<<"$verdicts"
echo
echo "$failed failed: $((failed - mine)) on $short too, $mine not."
[ $mine -eq 0 ]

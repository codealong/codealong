#!/usr/bin/env bash
# Runs the browser checks (scripts/preview/*-check.js) as one set: builds the UI and snapshots
# your real data once, runs every check against that, prints a line for each with its result,
# and exits 1 if any failed.
#
#   scripts/checks.sh                 every check
#   scripts/checks.sh queue rename    only queue-check.js and rename-check.js
#   scripts/checks.sh -v …            also what each check noted for a pass, and the commands
#                                     the stub had no answer for
#
# How a check runs is in its own header: each comment line there that is a whole preview.sh
# command for that file,
#   //   PREVIEW_WAIT=60000 PREVIEW_PIN=session scripts/preview.sh desktop '' scripts/preview/pr-chip-check.js
# (three spaces in, any PREVIEW_* variables, the view, the project or '', the script) is one
# run, with that view and those variables. A line with something left to fill in (<key>, <doc>)
# is for running by hand, on data picked for it; it's listed as not run.
#
# The checks read whatever this Mac has, so one can fail for want of data (no session that used
# Chrome, nothing in Your turn): it says what it was missing. preview.sh's own variables carry
# through: PREVIEW_BIN for another engine than this checkout's, PREVIEW_DIR, PREVIEW_SCHEME=dark.
#
# Each run gets as long as its PREVIEW_WAIT and a minute and a half more (two and a half minutes
# on virtual time), then it's stopped and fails: headless Chrome can hang on a busy Mac.
# gtimeout (coreutils) does the stopping when it's installed. A run whose page Chrome never
# showed gets one more go.
set -uo pipefail
cd "$(dirname "$0")/.."

verbose=0
wanted=()
for arg in "$@"; do
  case "$arg" in
    -v | --verbose) verbose=1 ;;
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
if [ ${#wanted[@]} -eq 0 ]; then
  for file in scripts/preview/*-check.js; do
    name="$(basename "$file" .js)"
    wanted+=("${name%-check}")
  done
fi

# Only what a check's own run line sets reaches it.
unset PREVIEW_WAIT PREVIEW_TODOS PREVIEW_INSIGHTS PREVIEW_ROUTE PREVIEW_CANVAS PREVIEW_USAGE PREVIEW_DEVICES \
  PREVIEW_NOTICES PREVIEW_SKILLS PREVIEW_FOUND PREVIEW_TRANSCRIPT PREVIEW_LOADING PREVIEW_PIN PREVIEW_SIZE PREVIEW_NOTCH \
  PREVIEW_INIT PREVIEW_NAME PREVIEW_PORT PREVIEW_PREPARE PREVIEW_BOARDS PREVIEW_BOARDS_OFF PREVIEW_GROUP

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

echo "Building the UI and reading the board…"
if ! PREVIEW_PREPARE=only scripts/preview.sh; then
  echo "scripts/preview.sh couldn't build the UI or take its snapshots." >&2
  exit 1
fi

out="$(mktemp)"
trap 'rm -f "$out" "$out.err" "$out.stopped"' EXIT
passed=0
failed=0
skipped=0
unanswered=""
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
    case "$line" in
      *"<"*)
        printf 'SKIP  %-22s %s\n' "$name" "by hand, on data picked for it: ${line#//   }"
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
    for word in ${line#//   }; do
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
      printf 'FAIL  %-22s %s\n' "$name" "its header's run line doesn't read as a preview.sh command: ${line#//   }"
      failed=$((failed + 1))
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
    if [ $status -eq 124 ] || [ $status -eq 137 ]; then
      # Stopped after it had its result (Chrome can hang once it's printed), the result stands.
      [ -n "$result" ] || result="FAIL 0/0 stopped after $limit s"
    elif [ -z "$result" ]; then
      result="FAIL 0/0 scripts/preview.sh exited $status: $(sed -n '1p' "$out.err")"
    elif [ $status -ne 0 ]; then
      case "$result" in
        FAIL\ *) ;;
        *) result="FAIL 0/0 scripts/preview.sh exited $status: $result" ;;
      esac
    fi
    count="${result#* }"
    count="${count%% *}"
    why="${result#* }"
    why="${why#"$count"}"
    printf '%s  %-22s %-28s %s%s\n' "${result%% *}" "$name" "$label" "$count" "$why"
    case "$result" in
      PASS\ *) passed=$((passed + 1)) ;;
      *) failed=$((failed + 1)) ;;
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
    unanswered="$unanswered$(sed -n 's/^unanswered: //p' "$out" | tr ',' '\n' | sed 's/^ *//')
"
  done < <(awk '/^\/\//{print; next} {exit}' "$file")
  if [ $runs -eq 0 ]; then
    printf 'FAIL  %-22s %s\n' "$name" "its header has no run line (//   … scripts/preview.sh <view> '' $file)"
    failed=$((failed + 1))
  fi
done

echo
echo "$passed passed, $failed failed$([ $skipped -eq 0 ] || echo ", $skipped not run")."
# Every command the stub answered null, across the runs: a new one whose answer the page shows
# belongs in fake-tauri.js.
names="$(printf '%s' "$unanswered" | sed '/^$/d' | sort -u | tr '\n' ' ')"
[ -z "$names" ] || echo "Commands the stub had no answer for: ${names% }"
[ $failed -eq 0 ]

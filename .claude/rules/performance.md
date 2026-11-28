---
paths:
  - "src-tauri/src/perf.rs"
  - "src-tauri/src/throttle.rs"
  - "src/perf.ts"
  - "src/reuse.ts"
  - "src/ui/memoRow.ts"
  - "scripts/perf-summary.mjs"
  - "scripts/preview/*-bench.js"
  - "scripts/preview/rows-check.js"
---

# Checking performance

- `[debug] perf_log = true` in config.toml (no restart needed) makes the engine and every window
  append timings to `~/Library/Logs/Codealong/perf.log`; perf.rs's header says what's in it. It's
  cut at 200 MB, the older half kept as `perf.log.1` (perf.rs `CUT_AT`), so either reads whole in
  perf-summary.mjs; before 2026-11-14 nothing cut it, and it held 540 MB from seven weeks, too
  much for Node to read whole (cut out a week with `tail -n +<line>` and `--log` it). The server
  turns it on from config.toml and the app
  from the board's `perfLog` (lib.rs `on_board`), so the windows' `ui[<window>]` lines are the
  app's to write: from 2026-10-31, when the engine moved into the server, until 2026-11-06 only
  the server turned it on, and the log has no window lines from those days.
- Its `ui[<window>] box:` lines say what moved the caret in a session's reply box
  (src/typingTrace.ts): typing and where it went in, drafts taken from another window, focus,
  the page's own calls that place the caret with where in the bundle they came from (`file:line:col`
  of the minified build: build that commit with `sourcemap: true` to name them), and moves
  nothing in the page made.
- A main thread still stuck a second after the watcher's probe is sampled while it's stuck
  (`/usr/bin/sample`, two seconds, at most every five minutes, and again at ten seconds), and
  `stalls/` beside the log keeps the last 20 reports. The release build is stripped, so
  Codealong's own frames are offsets; a build of the same commit with
  `CARGO_PROFILE_RELEASE_STRIP=false` names them (`atos -o <binary> -l <load address>
  <address>`, the load address from the report's header).
- `node scripts/perf-summary.mjs` sums up the app's latest run (its header has the options). A
  window that can't be seen holds pushes and renders the latest when it's shown, so its push
  times are for boards someone saw. A window still counted as shown while the display sleeps
  isn't held, and WebKit slows it then: a `ui[<window>] stall: a 1 s timer ran ~1000 ms late`
  every two seconds (1,800 an hour a window) and main-window commits of 250 to 300 ms, which
  were all of 2026-11-25 22:00 to 11-26 07:00, the display off throughout (`pmset -g log`,
  "Display is turned off"). Those hours are WebKit's throttling, not load: leave them out of a
  week's window numbers.
- The windows are sent what changed in each board, a session at a time (windowfeed.rs), and the
  log's `windows sent` lines say how much, and its `feed to <device>` lines what each linked
  device was fed (peerfeed.rs `perf_line`); only the first push, and a read after a window missed
  one, carry the whole board. The PRs, schedules and todos go an item at a time too
  (windowfeed.rs `LISTS`), and any other field of the board whole when it changes: until
  2026-11-28 the schedules did (some 270 KB, changed by a check every few minutes), 1.5 GB of
  the 2.1 GB the windows were sent from 2026-11-21 to 11-28, a send of 230 to 290 KB with
  "0 sessions in 0 lanes" some thousand times a day. A whole board's cost in a window is
  `scripts/preview/push-bench.js` (forty pushes of the real board
  with one worktree changed, as most publishes are), the quick panel's open, keys and pushes
  `quick-bench.js`, and its keys after a prompt of up to 12,000 characters `quick-long-bench.js`
  (a key cost 48 ms there until the session search stopped marking titles it didn't match),
  typing in the notch's box `notch-keys-bench.js`, and a reply streaming into an open
  conversation, then pushes while it's open, `stream-bench.js`; each header has its command. Rows of the
  board and the sidebar skip rendering when nothing they show changed (`reuse.ts` keeps
  unchanged parts of a push as the same objects, `ui/memoRow.ts` compares rows), which
  `rows-check.js` checks, and so do a conversation's (SessionThread.tsx), its reply streaming in
  drawn on its own at most once a frame (`liveReply.ts`): 500 chunks of a 12,500-character reply
  over 160 rows took 2,000 ms of the page's work and 80,500 row renders before that, 625 ms and
  300 after, and a push 6 ms and 170 row renders, 3 ms and 1 (2026-11-18, in Chrome). The reply's
  Markdown is still drawn whole each frame.
  On this Mac's board (68 sessions, 102 PRs, 2026-11-18) the main
  window's lists (boardLists.ts `useBoardLists`) took 0.5 ms of a push's 8 ms commit in Chrome,
  0.2 ms of the 15 s tick's 6 ms and 0.3 ms of a find-box key's 4 ms, so their memos weren't worth
  narrowing further.
- Every window loads the one bundle (main.tsx imports every window's root). Splitting it was
  tried and left out (2026-11-18): giving the windows other than the board's a page of their own,
  with only their own root's code, took a kept session's first React commit in a WKWebView from 42
  to 34 ms, a desktop card's from 42 to 28 and the quick panel's from 49 to 39 (the bar, peek and
  quick panel are made at launch, so only kept, side, terminal, desktop, lineage and waiting
  windows would feel it), the main window's staying at 60: not worth a second page to keep. Loading
  the main window's dialogs and its Canvas and Schedules views only when opened saved it 3 ms and
  cost Settings 10 ms the first time it opened. Chrome compiles module chunks off the main thread
  and WebKit doesn't, so a split measured in Chrome looks better than it is in the app; in a
  WKWebView nobody sees, WebKit runs no frames, so its first commit is the number, not its paint.
- Every window's typed keys go to the log as `ui[<window>] keys:` lines (perf.ts `useKeyPerf`):
  from the time macOS had each key to the frame after it, and, from 2026-11-16, how much of that
  came before the page had the key. Before then only the quick panel and the peek wrote them, so
  the notch's box had none. A key's own work is small: in the notch 1 ms of script and 1 ms of
  layout in an offscreen WKWebView, in the quick panel 4 ms of script in Chrome (2026-11-16).
  The quick panel's median per second was 13 to 15 ms at a load under 100 and about 20 ms at 400
  (2026-11-10 to 11-14), and 29 ms at 300 on 2026-11-16. Most of it comes before the page has the
  key: at a load of 15 to 90 (2026-11-16) the notch's keys were drawn 18 to 24 ms after them, 13
  to 19 ms of it before the page had them, and the quick panel's 14 to 20 ms, 8 to 13 before. Real
  key events handed to an offscreen WKWebView (`keyDown:` on the web view, through WebKit's own
  handling and its message to the page's process) reached the page in 0 ms (median) and were
  drawn in 3, so that wait is before WebKit has the key: WindowServer handing it to the app, and
  the app's main thread taking it. A 5 s sample of the app's main thread while nobody typed found
  it 95% idle, its busiest work WebKit asking WindowServer which window is under the pointer on
  each mouse move (`windowNumberAtPoint`, a round trip to WindowServer), and WindowServer was
  using 60 to 76% of a core.
- The engine thread's own costs on this Mac's real data (composing the board, hashing it, a
  big batch from the file watcher) are `bench_engine_on_real_data` in engine.rs, an ignored test
  whose doc has the command; `/usr/bin/sample` on it names the hot functions, which the stripped
  release build's stall reports can't. It's a debug build, where the engine's own code isn't
  optimized, so compare its numbers with each other, not with the log's. The shared build
  directory's test binary is rebuilt by whichever checkout tests next: copy it aside to compare
  before and after.
- A new line in a session's record reaches the board only when the engine's thread gets to
  it, so nothing on that thread may wait on another program: at a load of 200 to 700, `ps`
  took 3 to 7 s and an `lsof` half a second (2026-10-16). The tick's process table is listed on
  a thread of its own (`engine/readers.rs` `Engine::list_processes`), the readers look at it as it lands
  (`Reader::processes`) and every look after has it, and open files come from the kernel
  (`procs::open_files`). The log's `engine lag` lines say the thread was held; the lines just
  before them say by what (`glance <harness>`, `tick <harness>`, `processes <harness>`, `engine
  tick`, `rescan repos`, each when it took 50 ms or more), and an engine stall with none of them
  is a sample's job.
- Git is the engine's biggest cost and starts during load by construction. `throttle.rs` spaces
  runs in one worktree two seconds apart and publishes git-only changes in two-second batches
  (session changes still go out at once); `procs.rs` runs git at nice 10
  (`run_background`) and puts the git threads in the utility QoS class (`utility_thread`). At
  launch, worktrees with the latest activity go first.
- The file watcher is one FSEvents stream for every folder it watches, and when a build's burst
  overflows macOS's queue for it, FSEvents marks every one of them to be looked at again
  ("rescan: user dropped"), idle repos too: 6,298 such events in five minutes on 47 folders
  (2026-10-28, a watcher of notify 8.2's beside the app). Those run git only as throttle.rs
  `Drops` allows, and the log says `watcher dropped events in N folders`. A git run that finds a
  worktree as it was asks for no publish, and `publish skipped` says what asked for it.
- The engine's own thread is user-initiated (`user_initiated_thread`), ahead of the agents'
  builds. In the utility class from 2026-09-24 it sat at priority 20 in `ps -M` and, at a load
  of 30 to 130 on ten cores, messages waited 15 to 40 s for it while it used ~4% of a core and
  composed a board in 66 ms (2026-10-20): its lag was waiting for the CPU, not work. A thread on
  macOS starts in the default class whatever its starter's, so the threads the engine spawns
  keep theirs.

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
  append timings to `~/Library/Logs/Codealong/perf.log`; perf.rs's header says what's in it. Set
  it back to false when done: the log grows.
- A main thread still stuck a second after the watcher's probe is sampled while it's stuck
  (`/usr/bin/sample`, two seconds, at most every five minutes, and again at ten seconds), and
  `stalls/` beside the log keeps the last 20 reports. The release build is stripped, so
  Codealong's own frames are offsets; a build of the same commit with
  `CARGO_PROFILE_RELEASE_STRIP=false` names them (`atos -o <binary> -l <load address>
  <address>`, the load address from the report's header).
- `node scripts/perf-summary.mjs` sums up the app's latest run (its header has the options). A
  window that can't be seen holds pushes and renders the latest when it's shown, so its push
  times are for boards someone saw.
- A push's cost in a window is `scripts/preview/push-bench.js` (forty pushes of the real board
  with one worktree changed, as most publishes are), the quick panel's open, keys and pushes
  `quick-bench.js`, and its keys after a prompt of up to 12,000 characters `quick-long-bench.js`
  (a key cost 48 ms there until the session search stopped marking titles it didn't match);
  each header has its command. Rows of the board and the sidebar skip
  rendering when nothing they show changed (`reuse.ts` keeps unchanged parts of a push as the
  same objects, `ui/memoRow.ts` compares rows), which `rows-check.js` checks.
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
  a thread of its own (`Engine::list_processes`), the readers look at it as it lands
  (`Reader::processes`) and every look after has it, and open files come from the kernel
  (`procs::open_files`). The log's `engine lag` lines say the thread was held; the lines just
  before them say by what (`glance <harness>`, `tick <harness>`, `processes <harness>`, `engine
  tick`, `rescan repos`, each when it took 50 ms or more), and an engine stall with none of them
  is a sample's job.
- Git is the engine's biggest cost and starts during load by construction. `throttle.rs` spaces
  runs in one worktree two seconds apart and publishes git-only changes in two-second batches
  (session changes still go out at once); `procs.rs` runs git at nice 10
  (`run_background`) and puts the engine and git threads in the utility QoS class
  (`utility_thread`). At launch, worktrees with the latest activity go first.

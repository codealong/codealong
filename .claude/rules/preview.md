---
paths:
  - "scripts/preview.sh"
  - "scripts/checks.sh"
  - "scripts/preview/**"
---

# Browser checks

`scripts/preview.sh`'s header lists the views and every `PREVIEW_*` variable, `scripts/checks.sh`'s
how a check's own header names its runs, `prelude.js`'s what a check gets (`say`, `wait`, `until`,
`check`, `note`, `calls`, `q`, `all`, `pointAt`, `rightClick` with `menuLabels` and `menuItem`,
`done`, `stopped`) and how its result is printed, and `fake-tauri.js`'s how commands are recorded
(`window.__calls`) and which went unanswered (`window.__unknown`). What they don't say:

- In check mode Chrome runs on virtual time, which stands still while scripts run, and draws no
  frames, so scroll events never arrive; and it resizes the page just before a screenshot, which
  closes whatever closes on resize. `PREVIEW_WAIT=<ms>` runs a check on the real clock with
  frames instead.
- A check's page is the size its view asks for (the bar's 1100 by 64, say), the window taller by
  Chrome's own frame; the prelude notes one that isn't (on virtual time a page is 500 wide at
  least).
- The engine the preview runs is this checkout's own: `scripts/engine.sh` builds it (debug) when
  src-tauri has changed and copies it to `.preview/engine/codealong`. `PREVIEW_BIN` runs another.
- `scripts/checks.sh --fixture` is the gate: it runs the runs `scripts/preview/fixture-passes.txt`
  lists against the engine's made-up Mac (`src-tauri/src/fixture.rs`, `codealong --snapshot
  --fixture`), which is the same everywhere, so a failure there is the code's. Run it before a
  change to the UI lands (`--jobs=6` runs its sets side by side, in minutes). A new check is
  written to pass there (`scripts/checks.sh --fixture <name>`) and its run lines added to the
  list; what it needs that the made-up Mac lacks (a session in some state, a PR, a todo) is
  added to fixture.rs, whose own test says what the board must have, rather than left to chance
  on a real board. A check the list doesn't have assumes something of a real board; it still
  runs on real data.
- On real data the checks read this Mac's board, so one can fail for want of data (no session
  that used Chrome) and says what it missed. `scripts/checks.sh --base <names>` tries each
  failure again with the base commit's UI and checks over the same snapshots, and counts only
  the ones that pass there: a failure it says the base has too isn't the change's.
- The made-up Mac's sessions have keys that read as what they are
  (`claude:claude:checkout-approval`, a session waiting on a call to allow;
  `claude:claude2:checkout-question`, on a question): `PREVIEW_FIXTURE=1
  PREVIEW_TRANSCRIPT=<key>` shows one's made-up conversation and what it waits on, so a run that
  names one needs no real session picked by hand.
- Chrome doesn't drop a hidden page to background priority as WebKit does, so a window slowed by
  that in the app looks fast here (`.claude/rules/windows.md`).
- `scripts/preview/chip-lanes-check.js` checks that no list cuts a chip off.
- After moving CSS rules or reordering `src/styles/app.css`, `scripts/preview/style-diff.mjs` (its
  header says how) records every scene's computed styles from the build before and the build
  after and compares them, element by element, light and dark.

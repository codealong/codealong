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
  runs on real data. The list holds only runs the gate can run: one made by hand (`<key>`), or
  one no check has any more, fails it.
- Nothing of this Mac's may reach the made-up Mac's page, and nothing of whoever runs the gate
  may change what it checks. Its build and snapshots are in `.preview/fixture`, apart from this
  Mac's in `.preview`; a folder says which Mac it's of (`snapshot-of`) only once every snapshot
  is in it, and `PREVIEW_PREPARE=skip` stops at one that says anything else; the engine is asked
  for the made-up Mac's name first (`--snapshot --fixture name`), since one built before it had
  one prints this Mac's board for `--fixture`; and `checks.sh --fixture` takes away every
  `PREVIEW_*` variable it was started with but `PREVIEW_BIN` and `PREVIEW_DIR`. fixture.rs reads
  nothing of the machine it runs on but its time zone (not its home, its temporary folder, its
  sounds or the day on its clock): a rule that would is given the made-up Mac's instead.
- `checks.sh` builds the UI every time, whatever is exported: `--prepared` is the one way to
  run against the folder as an earlier run left it, and such a run's last line says the UI
  wasn't built, so a pass on yesterday's build isn't taken for today's code.
- A check that reads a date holds on every day of the year, or it fails the gate on an unchanged
  UI when its day comes (due-check.js did from December 20, when a todo due in 12 days is next
  year's and its chip says the year). It says what the window's own rule gives, and has a run on
  the day that differs: `PREVIEW_DATE=2026-12-28` sets the page's clock to that day (only the
  page's: the board's times stay as snapshotted, so it's for what the page dates by itself, the
  sample todos' due dates). Only `due.ts` words a date by its year today.
- Two `preview.sh` runs at once in one folder share its page, its check and its snapshots
  (`preview.html`, `check.js`, `board.json`), and one can be given the other's: give each a
  `PREVIEW_DIR` of its own, as `checks.sh` gives its `--jobs` sets. That has always been so of
  `.preview`; two runs of `checks.sh` at once in one checkout meet there too.
- On real data the checks read this Mac's board, so one can fail for want of data (no session
  that used Chrome) and says what it missed. `scripts/checks.sh --base <names>` tries each
  failure again with the base commit's UI and checks over the same snapshots, and clears only a
  failure the base's check words the same: one that passes there, fails there on something
  else or on fewer, or gave no result here or there still counts, and so does one whose report
  is cut short (`prelude.js` ends a long one with an ellipsis and names only what fits) unless
  the two are the same word for word and count for count. It's for real data only (`--fixture
  --base` is refused), and builds the base's UI with the base's own packages when its
  `package-lock.json` isn't this checkout's.
- A run that gives no result (Chrome never showed the page, the run was stopped, a `--jobs` set
  stopped short before it got to it) goes again by itself once the rest are through; every run
  that said how it went keeps what it said, a set saying each as it has it. `src/checks.test.ts`
  runs both scripts over stand-ins and pins all of this, opening no socket, so it runs in a
  sandbox too (`preview.sh` picks its port only when it starts its server).
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

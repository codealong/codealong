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
- The engine the preview runs is whatever was last built into the shared directory, possibly
  another session's: `PREVIEW_BIN=<a copy of your build>` uses yours.
- The checks read this Mac's board, so one can fail for want of data (no session that used
  Chrome) and says what it missed.
- Chrome doesn't drop a hidden page to background priority as WebKit does, so a window slowed by
  that in the app looks fast here (`.claude/rules/windows.md`).
- `scripts/preview/chip-lanes-check.js` checks that no list cuts a chip off.
- After moving CSS rules or reordering `src/styles/app.css`, `scripts/preview/style-diff.mjs` (its
  header says how) records every scene's computed styles from the build before and the build
  after and compares them, element by element, light and dark.

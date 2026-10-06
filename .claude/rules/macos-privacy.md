---
paths:
  - "src-tauri/src/held.rs"
  - "src-tauri/src/repos.rs"
---

# macOS privacy prompts

How macOS's privacy prompts (TCC) hold Codealong's reads and how to see one, moved from
CLAUDE.md's Harness facts (macOS 26.6, checked 2026-09-27).

- Reads in a folder macOS protects (`~/Documents`, Desktop, Downloads, a removable volume) wait
  in `open()` until the prompt is answered, while stat and a file watch return at once, and
  macOS asks again after every build: `held.rs` (module header, `Held`). `git worktree list`
  only looks worktree paths up, so it doesn't wait.
- A process started from a terminal or an agent's shell gets that app's permissions. To see a
  prompt, start it as its own responsible process: `posix_spawn` with
  `responsibility_spawnattrs_setdisclaim(&attr, 1)` (looked up with `dlsym`), as Claude.app's
  `disclaimer` helper starts Claude Code. A file symlinked into `~/Documents` makes a read
  wait on the prompt, which gives a test a held folder anywhere. A read-only disk image mounted
  with `hdiutil` asked for nothing, though `diskutil` calls it removable.
- tccd logs each request: `log show --predicate 'process == "tccd"'` has `AUTHREQ_PROMPTING`
  and `AUTHREQ_RESULT` (authValue 2 allowed, 0 denied; authReason 2 is a person's answer).

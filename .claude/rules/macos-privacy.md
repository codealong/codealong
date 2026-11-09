---
paths:
  - "src-tauri/src/held.rs"
  - "src-tauri/src/repos.rs"
  - "scripts/check-release.sh"
  - "scripts/within.sh"
---

# macOS privacy prompts

How macOS's privacy prompts (TCC) hold Codealong's reads and how to see one, moved from
CLAUDE.md's Harness facts (macOS 26.6, checked 2026-09-27).

- Reads in a folder macOS protects (`~/Documents`, Desktop, Downloads, a removable volume) wait
  in `open()` until the prompt is answered, while stat and a file watch return at once, and
  macOS asks again after every build signed ad hoc: `held.rs` (module header, `Held`). `git worktree list`
  only looks worktree paths up, so it doesn't wait.
- A process started from a terminal or an agent's shell gets that app's permissions. To see a
  prompt, start it as its own responsible process: `posix_spawn` with
  `responsibility_spawnattrs_setdisclaim(&attr, 1)` (looked up with `dlsym`), as Claude.app's
  `disclaimer` helper starts Claude Code. A file symlinked into `~/Documents` makes a read
  wait on the prompt, which gives a test a held folder anywhere. A read-only disk image mounted
  with `hdiutil` asked for nothing, though `diskutil` calls it removable.
- tccd logs each request: `log show --predicate 'process == "tccd"'` has `AUTHREQ_PROMPTING`
  and `AUTHREQ_RESULT` (authValue 2 allowed, 0 denied; authReason 2 is a person's answer).

## Gatekeeper

- A quarantined app's first run waits on the "downloaded from the Internet" prompt with no limit,
  notarized and stapled or not, and started from a shell as much as from Finder: none of its code
  runs until someone answers, so even `--version` waits, and `ps` shows it asleep under its own
  command line. `spctl -a` gives the same verdict without asking anyone. A quarantined disk image
  mounts with the `quarantine` option, so an app copied off it with `ditto` is quarantined too:
  `scripts/check-release.sh`, which takes the quarantine off its copy before running it.
- syspolicyd logs it: `log show --predicate 'process == "syspolicyd" AND subsystem ==
  "com.apple.syspolicy.exec"'` has `Prompt shown (5, 0), waiting for response` with the app's team
  and id, and `Marked application as user approved: 1` when it's answered (macOS 26.6.2,
  2026-11-08: 10 h 16 min apart, on 0.17.0's check). This Mac kept those lines for about a day.
- Not known: why check-release.sh's runs up to 0.13.0 (2026-11-03) took about 25 s from the same
  kind of session, when from 0.14.0 on they took minutes to hours. The two builds have the same
  files and entitlements, so it isn't the build. Nor whether a process held at the prompt dies when
  killed (`within.sh` doesn't count on it): finding out puts a prompt on the user's screen.

---
paths:
  - "src-tauri/src/jev.rs"
  - "src-tauri/src/route.rs"
  - "src-tauri/src/todojev.rs"
  - "src-tauri/src/insights.rs"
---

# TypeSafe

What TypeSafe's Jev takes and what it's good at, moved from CLAUDE.md's Harness facts (from
docs.typesafe.ai, 2026-09-25).

- The endpoint, the request's body, reading answers and the errors (401, 422, 429, 529):
  `jev.rs` (`ENDPOINT`, `body`, `noul`, `choice`, `score`, `post`). Questions are `choice` (up
  to 255 options, each described by a string or an object), `score` (2 to 10 levels) or `noul`
  (yes/no); answers carry `probabilities` and a `confidence`. `jev-latest` is `jev-1.13.0`: 64k
  tokens per request, 32k for the state plus the longest question; 1,200 requests a minute;
  $0.042 per million input tokens, output free.
- It reads instructions literally and is weak at dates, counting and arithmetic, so times go in
  as words and each question asks one plain thing: `jev.rs` and `route.rs` (module headers).

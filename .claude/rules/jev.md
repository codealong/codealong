---
paths:
  - "src-tauri/src/jev.rs"
  - "src-tauri/src/route.rs"
  - "src-tauri/src/todojev.rs"
  - "src-tauri/src/insights.rs"
---

# TypeSafe

What TypeSafe's Jev takes and what it's good at, moved from CLAUDE.md's Harness facts (from
docs.typesafe.ai, 2026-09-25, and its models and images pages with requests tried here,
2026-10-14).

- The endpoint, the request's body, reading answers and the errors (400, 401, 413, 422, 429,
  529): `jev.rs` (`ENDPOINT`, `body`, `noul`, `choice`, `score`, `post`, `answer`). Questions are
  `choice` (up to 255 options, each described by a string or an object), `score` (2 to 10
  levels), `noul` (yes/no) or, on 1.14 with the images header, `box` (one bounding box); answers
  carry `probabilities` and a `confidence`. A 400's `detail` is `{error_type, message}`.
- Models: `jev-1.14.0` (from 2026-10-14) and `jev-1.13.0`; `jev-latest` is the newest stable one
  (1.13.0 until about 2026-10-21, then 1.14.0), `jev-preview` the newest of any (1.14.0), and
  Codealong's default (`config.rs` `default_jev_model`, asked 2026-10-15). `GET /v1/models`
  lists only those two aliases, not the versions. A reply's `model` names the version that
  answered. 60 days' notice before 1.13 goes, 30 for
  1.14. Both: 64k tokens per request, 32k for the state plus the longest question (262k each
  with `TypeSafe-Danger-Zone-Long-Context: true`, slower and less accurate); $0.042 per million
  input tokens, output free. The state is billed once however many questions there are. A 200's
  `usage` is `{input_tokens, output_tokens}` (a one-question noul: 281 and 20 on 1.14,
  2026-10-16); every answer's goes in the ledger by the feature that asked (`jev::ask`,
  `ledger::note_jev`), for Settings › Jev's usage.
- Images: `jev.rs` (`Images`, `fit`, `make_ready`, `MAX_BODY`). A top-level `assets` map, each
  `{type: "image", format: "png"|"jpg", data_base64}` (or a multipart `part`), named by its key
  anywhere in `state`, `instructions` or `criteria`, inside longer strings too; each place a key
  appears is another copy. Every request with one needs `TypeSafe-Danger-Zone-Images: true`
  ("Images are experimental…" 400 without). An asset no text names, a side under 256 or over 1568
  pixels, or fewer than 78,400 pixels is a 400 ("Invalid image request.", "Invalid image
  dimensions.") for the whole request; 413 for size. PNG or JPEG, no animation, EXIF orientation
  not applied, and `sips` neither reports nor applies it either, so Codealong reads it from the
  image's EXIF (a JPEG's APP1, a PNG's eXIf, a WebP's EXIF chunk) and turns the pixels
  (`exif_orientation`, `exif_of`, `upright_steps`, each of the eight checked by corner colors). In one run sips orders a turn and a flip against the command line (`-r 90 -f
  horizontal` flips first) and resamples before it turns, so each turn is a run of its own;
  `-z` then `-p` in one run scales, then pads around the middle. An image cost about a token per 370 pixels on 1.14 (1568 × 958: 3,934 tokens;
  784 × 479: 1,022; 280 × 280: 214). A quick panel request was about 36,000 tokens before images.
- curl takes the body on one line of its config (`-K -`), which curl 8.7 caps at 10 MiB
  (10,485,760 bytes: longer fails as "error encountered when reading a file").
- It reads instructions literally and is weak at dates, counting and arithmetic, so times go in
  as words and each question asks one plain thing: `jev.rs` and `route.rs` (module headers). The
  blog post for 1.14 says every model does worse on images than on text, and to ask the simplest
  questions about them.
- A search or a message (route.rs `actions`, 1.14, 46 real sessions, 2026-10-21): with `find`
  as "only names or searches for a session … asks for no work", 7 of 17 searches (a session's
  title or words of it, typed to find it; one an exact title, at send 0.65) came back send or
  start. Saying that a search asks and says nothing, and that words of a title are one, read all
  17 as find, and of 22 prompts none as a search under either wording (the misses were sends
  taken for starts and back, close calls both ways). A yes/no question of its own ("only words
  to find a session by?") did worse than the reworded option. With the request's other
  questions in, a two-word chat title still came back send 0.51, find 0.40: route.ts gives the
  other of the two a row.
- Which service a message needs (route.rs `needs_question`, `NEEDS_AT`): asked "what does my email
  say about the launch", "summarize what was said in slack today" and "implement the settings
  screen from the figma design", 1.14 gave the service 0.92 to 0.98; "fix the flaky test" gave
  GitHub 0.63 until "none" said code work needs none, then nothing,
  while "review the open pull requests on github" still gave GitHub 0.71, and "what did we decide
  in yesterday's meeting" gave none with a meeting-notes service offered (2026-10-20). A request
  with it was about 33,000 tokens.
- Lines of sessions' conversations (route.rs `related`, search.rs `related`, 1.14, 2026-10-22):
  prompts that named what was said in a session rather than its title, over 14 days of one
  Mac's sessions. Without lines, Jev put four of five on the right session and one ("Send now on
  a held message killed my running command…") on an unrelated session at 0.40. Matching whole
  sessions by the words they shared put long conversations first (they have every word
  somewhere), and lines from an unrelated one, sharing "since" and "last", took "the schedule
  watch command should say what's new since the last run" from the right session (0.75) to the
  wrong one at 0.65. Matching single messages, leaving out words in more than 5% of the 40,000
  messages and squaring how rare each is, put the held message on the right session at 0.91,
  and the others stayed right (0.98 to 1.00, 0.90 to 0.97, the schedule watch 0.75 to 0.96, 0.94
  to 0.93). A request was 1,000 to
  5,000 tokens more (33,000 to 35,000–38,000); the search took 85 to 410 ms in a debug build.

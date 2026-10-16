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

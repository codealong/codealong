---
paths:
  - "src-tauri/src/claude/conversation.rs"
  - "src-tauri/src/transcript.rs"
  - "src-tauri/src/attachments.rs"
  - "src-tauri/src/prlinks.rs"
---

# Claude Code's conversations

What a prompt and a call's result record in Claude Code's transcript, as a session's panel shows
them and as Codealong keeps them: a prompt taken back, the images a prompt came with, your
answers to a question, an artifact, the files handed to you, the PR a call made, and how its
terminal draws a reply's links. Found by reading its code and this Mac's transcripts; read it
before changing how a conversation reads (`claude/conversation.rs`) or what's kept from one
(`attachments.rs`, `prlinks.rs`). Paths are under `src-tauri/src/`.

- Escape before the agent has written anything back, with the input box empty, takes the prompt
  back instead of interrupting the turn, and nothing in the transcript says so (2.1.285's
  `auto_restore_cancel`): `claude/conversation.rs` (the comment above `fate_of`, and `fate_of`).
- A PR a session made or changed, from `toolUseResult.gitOperation.pr` or a `pr-link` line:
  `claude/transcript.rs` (`pr_made`); `prlinks.rs` links the session to it, subagents' logs
  included (`engine.rs`). `gitOperation` also has `push`, `commit` and `branch`; `push.branch`,
  the remote's branch, is how often a session pushed to a PR's (`claude/transcript.rs` `pushed`,
  board/prs.rs `pr_primary`), though a quiet push leaves none. Its `pr` is
  written only for commands Claude Code reads as acting on a PR (`gh pr create`…): a PR made
  with `gh api repos/<owner>/<name>/pulls`, or by a script, gets none. A `pr-link` line is
  `{"type":"pr-link","sessionId","prNumber","prUrl","prRepository","timestamp"}`, "last-wins"
  metadata re-stamped with the session's current PR every 32 KB of writes, so one PR has dozens
  of lines. Both marks go by what a command printed, so a command that only prints another
  session's PR-making command and its URL (a `jq` over a transcript) gets both marks too,
  `action: "created"` included (2.1.289, 2026-10-05).
- An image that came with a prompt is an `image` part (`source: {type: "base64", media_type,
  data}`; PNG, JPEG and WebP seen) in the prompt's own `message.content`, numbered by
  `imagePasteIds` and named `[Image #N]` in the text when pasted into the terminal:
  `claude/conversation.rs` (`images_of`); Codealong keeps each (`attachments.rs` module header,
  `keep_image_in`, `written_in`). Pasted, the text part comes first; attached in the Claude app,
  the image comes first. A prompt sent while it worked has the same list as its `queued_command`
  attachment's `prompt`, with `imagePasteIds` on the attachment. A `@"<path>"` mention stays in
  the text as typed, and the file it attached is the next line, `attachment: {type: "file",
  filename, displayPath, content}`, an image's content `{type: "image", file: {base64, type,
  originalSize, dimensions}}`; Codealong doesn't read that line (2.1.281 to 2.1.287).
- Your answers to an `AskUserQuestion`, on its result line's `toolUseResult.answers`:
  `claude/conversation.rs` (`answers_of`, `picks_in`). Several picks are one string joined with
  ", " in the order they were picked, words of your own where they were written (2.1.302 to
  2.1.326 here, 2026-11-17; only 2.1.219 wrote a list), and labels can have commas of their own. `toolUseResult` also has `questions` and
  `annotations`. Declined, the result is an error, "The user doesn't want to proceed…", with
  `toolUseResult` the string "User rejected tool use".
- An `Artifact` call's result line (`toolUseResult`) and the link the conversation shows for it:
  `claude/conversation.rs` (`artifact_of`), which the session's list of them for its header
  (`artifact_made`, artifacts.rs) goes by too, subagents' logs included. Across this Mac's
  transcripts on 2026-10-12, only `Artifact` calls had results with `artifact_id` or
  `created_from_type`: 169 publishes and one open, all with a `url`. The call's input is
  `file_path`, or `type_url` and `title`, with an `action` when it isn't a publish. The result
  also has `version`, `path`, `icon`, `audience`…; `url` is `https://claude.ai/artifact/<id>`,
  older versions write no `seq`, and `updated` is false on the first publish. One made from a
  type has no `artifact_id`; a read keeps `url` and `title` under `read` (beside
  `artifactRead`), and a list has `artifacts` (2026-10-02).
- A `SendUserFile` call and the files the conversation lists under it: `claude/conversation.rs`
  (`sent_of`), `claude/transcript.rs` (`SEND_FILE`). Its input is `files` (paths, absolute or
  from the folder it works in), `caption`, `status` (`normal`, or `proactive` to reach your
  phone) and `display` (`render` or `attach`, absent before about 2.1.280). Its result's
  `toolUseResult` is `{caption, display, rendered_locally, attachments: [{path, size, isImage,
  media_type, pathValidated, file_uuid}]}`, an attachment that didn't upload having
  `upload_error` in place of `file_uuid`; one failing among several doesn't make the result an
  error. Across this Mac's 71 calls (2.1.219 to 2.1.296, terminal and Claude app sessions, none
  in a subagent's log, 2026-10-16): 100 files, 82% images, then PDFs, Markdown, HTML and an MP4,
  and 61 of them already gone from disk, sent from scratchpads and background jobs' `tmp`
  folders. Claude Code uploads each to `POST /api/oauth/file_upload`; `GET
  https://api.anthropic.com/api/oauth/files/<file_uuid>/content` with the sending account's
  bearer token (and `anthropic-beta: oauth-2025-04-20`) gave the bytes back exactly, as
  `application/octet-stream`, for a file sent that day and one sent 22 days before whose copy on
  disk was gone (one `~/.claude` account's terminal sessions). Codealong doesn't fetch them: a
  click opens the file where it is (`launch::open_file`).
- Its terminal draws a Markdown link in a reply as its words alone, whatever the link's scheme:
  `[Fix the parser](codealong://session/<key>)` showed as `Fix the parser`, as
  `[the guide](https://example.com/guide)` showed as `the guide` (2.1.322 in tmux, 2026-11-10; the
  record keeps the Markdown as written). So a link to a session or a todo, as the skill has
  agents write one (`src/codealongLinks.ts`), costs nothing to read there. The words carry a
  terminal hyperlink to the link's target, whatever its scheme (`ESC ] 8 ; id=<id> ; codealong://session/<key> ESC \`,
  read with `tmux capture-pane -e`), so a terminal that opens hyperlinks hands a click on them
  to macOS, which opens Codealong's with Codealong (`front.rs` `open_links`). Not read: what
  Codex's and the other harnesses' terminals draw, and a click in a real terminal.

## Elsewhere

- Which lines are prompts, a slash command's lines and a message taken in mid-turn
  (`conversation_items`, `name_commands`) are in `claude-transcript.md`.
- Another session's message (`line_peer`) and a `!` command's block (`shell_items`) are in
  `claude-send.md`.
- Attaching a file to a prompt (an `@"<path>"` mention, `--add-dir`), a copy cut at a message
  (`fork_point_before`) and why a moved session's PRs are carried from the original (`prlinks.rs`
  `carry_moves`) are in `claude-start.md`.
- The artifact a prompt or a comment is about (`artifact_about`) is in `claude-code.md`, under
  Claude app.

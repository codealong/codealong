---
paths:
  - "src-tauri/src/terminals.rs"
  - "src-tauri/src/terminals/**"
  - "src/ui/terminal/**"
  - "src/terminals.ts"
---

# libghostty and Codealong's own terminals

What Ghostty's terminal library does that `terminals.rs` and `src/ui/terminal/` rely on, from the
`libghostty-vt` crate at the revision `src-tauri/Cargo.toml` pins (libghostty-vt 1.0.0, Ghostty's
`vt-1.0.0` tag, 0fa3d577, which needs Zig 0.17.0; the crate needs Rust 1.96) and its example
terminal (`example/ghostling_rs`, the best map of the API), checked by `cargo test terminals`
(2026-11-08). How to take a newer revision: `docs/development.md` (*Ghostty's terminal library*).

- 1.0's C API is opaque handles read and written through getters and setters, and the crate
  follows it: a formatter is made from its terminal and set up with `set_*` calls, each a
  `Result` (no options struct). A call into a feature the build compiled out is
  `Error::Unsupported`, as is a value the crate has no variant for (from a newer library). The mouse encoder's `set_size` refuses a cell with no width or height
  (`Error::InvalidValue`) and keeps the size it had. Text the C API takes (a terminfo name, a
  clipboard MIME type, a color to parse) goes in as bytes.

- Nothing in the library is thread-safe and every type is `!Send`: a terminal's emulator, render
  state, iterators and encoders are made and used on its own thread (`terminals.rs` `run`), and
  everything else reaches them as messages. Callbacks (`on_pty_write`, `on_title_changed`, …) run
  inside `vt_write` on that thread.
- `RenderState::update` takes the screen as it is; `Snapshot::dirty` says Clean, Partial or Full,
  and `RowIteration::next_dirty` gives only the changed rows (every row when Full). `clean()`
  marks them drawn. `set_dirty(Full)` before iterating gives a whole frame, which a window that
  just started showing a terminal needs (`screen.rs` `frame`). Rows not drawn while no window
  watches stay dirty until the next update.
- `CellIteration::fg_color`/`bg_color` resolve palette colors to RGB (None for the default);
  inverse and invisible are the caller's to apply, and bold isn't brightened. A wide character's
  second cell is `CellWide::SpacerTail`, and a wide character that didn't fit at a line's end
  leaves `SpacerHead`; neither is drawn.
- Programs that ask for the colors (OSC 10, 11, 4) are answered from the terminal's default
  colors and palette, which a window's theme sets (`Screen::theme`); CSI ? 996 n asks
  `on_color_scheme`. Device attributes, size reports and XTVERSION each need their callback, or
  the query goes unanswered.
- The key encoder takes the W3C key names `KeyboardEvent.code` uses (`terminals/keys.rs`) with
  the text the key typed and the modifiers the text already took in (`consumed_mods`); with
  `set_options_from_terminal` it encodes for the modes the program set (application cursor keys,
  the Kitty keyboard protocol, modifyOtherKeys). A release encodes to nothing unless the program
  asked for releases.
- `Terminal::paste` frames a paste as the program asked (bracketed, or a Kitty paste event) and
  writes it through `on_pty_write`; text that could run commands where pastes aren't bracketed is
  `Error::Rejected` unless `with_allow_unsafe`.
- Selection lives in the terminal: the gesture API (`PressEvent`, `DragEvent`, `ReleaseEvent`)
  counts clicks for words and lines, `set_selection` sets it, the render state marks selected
  cells, and a formatter made with `Formatter::new_selection` (None while nothing's selected)
  gives its text (`screen.rs` `selection_text`). The formatter's own `set_selection`, for a
  selection from elsewhere, is `unsafe`: nothing checks it came from that terminal's active screen.
- A program that marks a redraw as one (synchronized output, DEC mode 2026, which Claude Code and
  Codex wrap every draw in) calls `on_render_hold` with true as it starts and false as it ends (or
  on a reset or a resize); a renderer keeps the last finished frame meanwhile and gives up after a
  second. The server makes no frame while one's held (`screen.rs` `held`, `HOLD_MAX`).
- `Terminal::encode_snapshot` writes everything a terminal holds (size, palette, modes and mouse
  settings, both screens with their cursors and Kitty keyboard stacks, the scrollback, newest
  first) and `snapshot::Decoder::decode` makes a terminal of it. The format is versioned apart
  from the library and outside 1.0's ABI promise: its version 1 has no binary-compatibility
  guarantee yet (Ghostty's snapshots guide, vt-1.0.0). Encoding partway through a sequence needs continuation tracking on
  (`set_continuation_max_bytes`) before the bytes that started it, so every screen has it
  (`screen.rs` `CONTINUATION_BYTES`); the decoded terminal finishes the sequence from the next
  bytes. Callbacks aren't in a snapshot: they're set again on the decoded terminal (`Screen::around`).
- The formatter's `Vt` format writes a screen and its scrollback as escape sequences another
  terminal draws, with what `set_extras` adds (`Extras`: the modes, keyboard modes, scrolling
  region, tab stops, working directory, the cursor's place, pen and hyperlink, …; the palette,
  pen and hyperlink by default) (`screen.rs` `vt`, for `codealong terminal attach`). It
  trims trailing spaces and blank rows by default; untrimmed VT output keeps the blank rows, so it
  fills the rows the screen does.
- Claude Code turns the Kitty keyboard protocol on (`CSI > 5 u`) only once a terminal it doesn't
  know answers `CSI ? u` (2.1.317, by `TERM_PROGRAM`: Ghostty's it turns on unasked); Codex pushes
  `CSI > 7 u` itself. Without it the encoder sends Shift+Return as xterm's `CSI 27;2;13~`, as
  Ghostty does.
- The `libghostty-vt-sys` build script clones Ghostty into its `OUT_DIR` and runs `zig build
  -Demit-lib-vt`, Debug in a cargo build whose profile has debug info, else ReleaseFast; across
  targets it passes `-Dtarget` (x86_64 macOS builds from an arm64 Mac, 2026-11-03).

## Unchecked

- 2026-11-04, in the real app: a session started and resumed in Codealong's terminal
  (`[apps] terminal = "codealong"`) shown in its panel, Show in and the waiting window's answers
  there, and a server handing its terminals to a new build through a real update or relaunch
  (checked against a server of this build in a home of its own, which handed a terminal over to
  itself and took it back: same pid, screen, input and attach). Whether macOS's privacy grants
  hold for an agent across the change is reasoned (the same process), not seen.

- 2026-11-03, in the real app (the preview's Chrome showed them, with WebGL2 drawn by
  SwiftShader): the terminal drawn with WebGL2 in WKWebView and on WebKitGTK, and Canvas 2D where
  WebGL2 isn't there; an input method composing in the terminal's hidden field; ⌃` from a
  session's box and back from the terminal; a terminal popped out (`kept.rs` `pop_terminal`):
  where its window opens, that it takes the keyboard, that its strip drags it and that it comes
  back at launch; `[apps] run_in` runs from Run and `!` and an agent's allowed request.

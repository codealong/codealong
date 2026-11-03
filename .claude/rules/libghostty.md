---
paths:
  - "src-tauri/src/terminals.rs"
  - "src-tauri/src/terminals/**"
  - "src/ui/terminal/**"
  - "src/terminals.ts"
---

# libghostty and Codealong's own terminals

What Ghostty's terminal library does that `terminals.rs` and `src/ui/terminal/` rely on, from the
`libghostty-vt` crate at the revision `src-tauri/Cargo.toml` pins (its Ghostty commit of
2026-10-21, 1.3.2-dev, which needs Zig 0.17.0) and its example terminal (`example/ghostling_rs`,
the best map of the API), checked by `cargo test terminals` (2026-11-03). How to take a newer
revision: `docs/development.md` (*Ghostty's terminal library*).

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
  cells, and the formatter (`fmt`, Plain with `with_selection`) gives its text.
- The `libghostty-vt-sys` build script clones Ghostty into its `OUT_DIR` and runs `zig build
  -Demit-lib-vt`, Debug in a cargo build whose profile has debug info, else ReleaseFast; across
  targets it passes `-Dtarget` (x86_64 macOS builds from an arm64 Mac, 2026-11-03).

## Unchecked

- 2026-11-03, in the real app (the preview's Chrome showed them, with WebGL2 drawn by
  SwiftShader): the terminal drawn with WebGL2 in WKWebView and on WebKitGTK, and Canvas 2D where
  WebGL2 isn't there; an input method composing in the terminal's hidden field; ⌃` from a
  session's box and back from the terminal; a terminal popped out (`kept.rs` `pop_terminal`):
  where its window opens, that it takes the keyboard, that its strip drags it and that it comes
  back at launch; `[apps] run_in` runs from Run and `!` and an agent's allowed request.

---
paths:
  - "src-tauri/src/panel.rs"
  - "src-tauri/src/bar.rs"
  - "src-tauri/src/barfeed.rs"
  - "src-tauri/src/peek.rs"
  - "src-tauri/src/kept.rs"
  - "src-tauri/src/quick.rs"
  - "src-tauri/src/waiting.rs"
  - "src-tauri/src/desktop.rs"
  - "src-tauri/src/lineage.rs"
  - "src-tauri/src/front.rs"
  - "src-tauri/src/geometry.rs"
  - "src-tauri/src/procs.rs"
  - "src-tauri/src/zoom.rs"
  - "src-tauri/src/lib.rs"
  - "src-tauri/capabilities/**"
  - "src-tauri/tauri*.json"
  - "src/ui/Bar.tsx"
  - "src/ui/Notch*.tsx"
  - "src/ui/Peek.tsx"
  - "src/ui/Quick.tsx"
  - "src/ui/Waiting.tsx"
  - "src/ui/DesktopPin.tsx"
  - "src/ui/DesktopLines.tsx"
  - "src/bar.ts"
---

# Tauri and macOS

What Codealong's windows rely on in Tauri, AppKit and WebKit, moved from CLAUDE.md's "Tauri and
macOS" (from the Tauri 2.11 and tao 0.35 sources, and checks on this Mac). A bare file name is
under `src-tauri/src/`.

- Tauri doesn't say whether a window can be seen; AppKit's `NSWindow.occlusionState` (through
  objc2) does: visible means some part is on screen, so not minimized, hidden, on another Space
  or fully covered (`panel::on_screen`).
- Every window's level, and why each is where it is: `panel.rs` (`Level`; its test lists them as
  AppKit counts them, the menu bar being 24 and its status items 25).
- The notch: how it's found and how the docked bar's window keeps one size and takes clicks only
  on its shape: `bar.rs` (module header, `notch_of`, `watch_pointer`), `panel.rs` (`Top`),
  `src/bar.ts` (`NOTCH`, `notchSizes`). On a 16-inch MacBook Pro (2026-09-28) the safe area is
  32, the notch 185 wide and the menu bar 33 tall (`frame` top less `visibleFrame` top).
- The docked bar has the keyboard only while it shows a session, and gives it back through an
  unseen panel that takes it and is ordered out: a panel ordered out and in again, even in one
  turn of the run loop, has WebKit hide its page until it draws again, and the notch blinked
  out as it closed (2026-10-05). `bar.rs` (`set_keys`, `lost_keys`), `panel.rs`
  (`Keys::WhenAsked`, `drop_keys`, `relay`).
- Only a non-activating NSPanel goes onto another app's full-screen Space, so each window's class
  is swapped before it's first shown: `panel.rs` (module header, `builder`, `make`).
- A panel gets its old class back before its window is destroyed, or WebKit's key-value
  observing aborts the app (checked 2026-09-28): `panel.rs` (`unmake`, `make`).
- A click in a non-activating panel leaves the app inactive, so the bar asks macOS to bring the
  app it opened forward: `front.rs` (`bring_forward`).
- WebKit treats a page as active only while its window says it's key (re-read when the window is
  ordered on screen), and an inactive page gets mouse moves only for its scrollbars (WebKit's
  WebFrame.cpp, checked in an offscreen WKWebView on 2026-09-26): what each panel answers and
  why is `panel.rs` (`Keys`, `present`, `Config`'s `tooltips`). A WKWebView takes mouse moves
  from its own tracking area, active always with classic scroll bars and in the key window with
  overlay ones, and has no `mouseMoved:` to call: a test would send them with
  `_simulateMouseMove:`.
- `proc_pidinfo` answers only for your own processes, `sysctl` KERN_PROC_PID for any, and a
  killed process stuck in the kernel is reaped on a thread of its own: `procs.rs` (`listed`,
  `run`).
- A plain JS `listen()` hears events sent to any window, and a window's own `emit` goes to every
  window: `src/api.ts` (`onWindowEvent`), and `lib.rs` beside the Settings menu's `emit_to`.
- A window's label must be listed in `src-tauri/capabilities/default.json` (a `*` at the end
  matches the rest), or its page's `listen` is refused and it never hears an event (2026-09-30):
  `lib.rs` (`every_window_may_listen_for_events`, whose list a new window's label goes in too).
  The preview's stub allows everything, so it can't show this.
- A window ordered front is scaled in by AppKit over a fifth of a second unless its
  `animationBehavior` is none (`panel::no_animation`; `CGWindowListCopyWindowInfo` reports its
  bounds smaller meanwhile). What another process draws can be seen without the screen:
  `screencapture -x -o -l <window id>` captures one window, its clear parts clear, and its id
  comes from the window list (the app's windows by owner name, the panels at layers 24 to 27,
  `panel::Level`).
- A menu item's icon as raw RGBA, `{rgba, width, height}`, which needs none of Tauri's image
  features though its JS types don't list it, and muda's 18-point item images: `src/contextMenu.ts`
  (`MenuImage`, `options`), `editors.rs` (`ICON_PX`, `menus_take_the_icons_as_pages_send_them`).
- `show()` orders a window front without activating the app; `set_focus()` activates it.
  Commands without `async` run on the main thread; one marked `async` that can wait on something
  slow goes through `lib.rs` (`blocking`, which says why; tauri-macros 2.6), and errors go to a
  window as `util::words`.
- Transparent windows need `macOSPrivateApi` in tauri.conf.json and the `macos-private-api`
  feature. `data-tauri-drag-region="deep"` drags from anywhere inside except controls.
- WebKit holds back ResizeObserver and animation frames while a window is hidden.
- WebKit runs a hidden page's web process at background priority except while a script the app
  sends runs in it (`evaluateJavaScript`: Tauri's `eval`, and so every event it emits): what that
  cost the quick panel and how it opens inside one script is `quick.rs` (module header).
  `background_throttling` off doesn't change that priority: it only keeps the page from being
  suspended. The process log says when it changes (`log show --predicate 'subsystem ==
  "com.apple.WebKit" AND category == "ProcessSuspension"'`).
- WebKit repaints a rotating path inside an SVG every frame, so the working glyph turns its
  whole `<svg>`: `src/styles/glyphs.css` (`.ca-glyph--working`).
- WebKit slows a visible page's timers and drawing while its app isn't frontmost, and every
  panel opts out: `panel.rs` (`builder`, `background_throttling`).
- A scroller that isn't a stacking context, with positioned rows in it, gets a composited layer
  per row WebKit composites, each outside the scroller's own layer, wrapped in a clip layer, and
  moved by an "overflow scroll proxy" node from the scroll position; which rows those are
  changes as the list scrolls and changes. The board was 1,180 layers for Sessions' 155 rows, 43
  isolated (`src/App.css`, `.ca-board`). To count: a WKWebView in a small Swift program, on
  `preview.sh`'s page served from `.preview/`, and its testing SPI `_scrollingTreeAsText` (proxy
  nodes, and each scroll node's position on the drawing side) and `_caLayerTreeAsText` (layers),
  called with `perform`. Synthesized `NSEvent` clicks reach the page there; hover doesn't, with
  the window hidden (checked 2026-10-10). A hidden window gets no display refreshes either, so a
  scroll's rubber band never springs back there and the two sides look out of step: put the
  window on a screen (transparent, ignoring the mouse) before believing that (2026-10-12).
- Hover and clicks in a list landing rows below the pointer and staying there with it still
  (2026-10-12, Sessions) mean WebKit hit-tests somewhere other than what it draws: the page's
  pointer or a list's scroll position isn't the app side's. With `[debug] perf_log` on,
  `pointer.rs` compares the two each time the pointer rests and logs `pointer off:` when they
  disagree.
- The bar listens for `bar-feed`, never `board`, so Tauri doesn't send it the board:
  `barfeed.rs` (module header).
- The quick panel takes the keyboard without activating Codealong: `orderFrontRegardless`,
  `makeKeyWindow`, then the WKWebView made first responder, which a window made with
  `focused(false)` doesn't do by itself. It follows the board only while it's up, and an open
  brings the board in the script that opens the page: `quick.rs` (module header), `panel.rs`
  (`present`, `take_keys`, `Keys::Always`), `src/ui/Quick.tsx`.
- The waiting window is an ordinary window, not a panel: it activates Codealong (`set_focus`),
  sizes natively, and so doesn't go onto another app's full-screen Space. It's made on first open
  and hidden on close; its page reads what the latest open asked for as it comes up, and hears
  later opens as `waiting-open`. The shortcut that puts it away gives the keyboard back to the
  app that had it, by bundle ID (`front::frontmost_other`; NSRunningApplication's
  `processIdentifier` needs objc2-app-kit's `libc` feature, which isn't on): `waiting.rs`. It
  has no title bar (an Overlay one with its title hidden): the card's strip is one
  (`src/ui/SessionPanel.tsx` `Hotkeys.titleBar`).
- In a window with an Overlay title bar, macOS 26 leaves the window's buttons where it puts
  them: 14-point buttons 23 apart, 9 to 69 points in, centered 16 down in a 32-point title bar.
  tao's `traffic_light_position(x, y)` moves the close button to x and their middle to y − 2, and
  does it again on every redraw (`inset_traffic_lights`; measured in a plain AppKit window on
  2026-10-07). The page draws under them and gets the clicks there but on the buttons, so it
  drags the window itself (`data-tauri-drag-region`): `waiting.rs`, `src/ui/SessionPanel.css`
  (`.is-titlebar`), `src/App.tsx` (`ca-win-bar`).
- The peek: hover shows it without the keyboard, a click gives it the keyboard, it watches the
  pointer to close, keeps a thin margin on the bar's side, and is dragged from the engine:
  `peek.rs` (module header, `watch_pointer`, `NEAR`, `drag`), `panel.rs` (`Keys::OnClick`).
- A session kept on the desktop is a panel of the peek's kind: `kept.rs` (module header, `close`,
  which gives its class back, and `on_window_event`, which orders one that lost the keyboard in
  again saying it's key), `panel.rs` (`ENGAGED`).
- tauri-plugin-global-shortcut's `register` and `unregister` wait on the main thread, so they're
  called off it: `quick.rs` (`apply_shortcut`), `lib.rs`. Carbon's `RegisterEventHotKey`
  succeeds for keys another app already holds, so clashes can't be found by registering. Apps
  keep theirs in their preferences: the KeyboardShortcuts library as `KeyboardShortcuts_<name>`
  = `{carbonKeyCode, carbonModifiers}` (ChatGPT in `com.openai.chat`), macOS in
  `com.apple.symbolichotkeys`, which lists only the ones changed from their defaults.
- Tauri's `plugin:menu|popup` (2.11.6, the same in 2.12.0) keeps the page's resource table
  locked while a menu is up, which hung the app: `lib.rs` (`popup_menu`).
- A key equivalent (⌘ and a key) goes to the page before the menu bar, and to the menu only when
  the page doesn't prevent it; WKWebView's `pageZoom` shrinks the page's CSS pixels and is in
  its `devicePixelRatio` (2.5 on a Retina screen at 125%). Both checked in a WKWebView whose
  window was never on screen, on 2026-10-06: `zoom.rs` (module header, `add_menu`).
- CSS `zoom` on an element, as the cards in the windows that float take it, behaves as the
  standard has it: rects, hit testing and the pointer are on the screen's scale; `offset*`,
  `scroll*`, ResizeObserver and lengths written in CSS (a `position: fixed` element's `left`
  too) are in the element's own pixels; a box sized by its parent (a percent, flex) fills it.
  Checked in the same way on 2026-10-06: `src/zoom.ts` (module header, `scaleOf`).
- The background computer-use tools click through raw input: the app comes forward for a moment
  and the one before is put back, which takes the keyboard from the quick panel and hides it.
  `screencapture -l <window id>` captures a window of Codealong's alone, even ordered out.

## Unchecked

- Not yet checked by hand (2026-10-06): zooming in the real app. View's Actual size, Zoom in and
  Zoom out and their keys; every board window following a step, and a project window opening at
  the zoom; a menu dropped from a button (a session header's More) landing under it at 125%. The
  preview only shows the keys asking for it (`zoom-check.js`). In the windows that float: the
  peek opening wider and taller at a zoom (`peek::place`), the notch's open card following a
  step while it's up, and ⌘= in the notch reaching its card while it has the keyboard; the
  preview shows the cards drawn at it and their keys (`card-zoom-check.js`).

- Not yet checked by hand (2026-10-06): ⌃` in the real app. That WKWebView hands ⌃` to the page
  from a session's box as it does ⌃-, in the main window and the floating panels (the peek, the
  notch, the waiting window), and that a native menu shows `Ctrl+`` beside the folder's terminal
  (muda 0.19.3 parses it as Backquote). The preview shows the terminal it asks for
  (`terminal-key-check.js`).

- Not yet checked by hand (2026-10-12): pressing ⇧⌘S, ⇧⌘W, ⇧⌘P, ⇧⌘T and ⇧⌘B in the real app
  (`src/mainKeys.ts` `VIEW_CHORDS`). They're key equivalents, which reach the page before the
  menu bar (above), so they should behave as the preview shows (`view-keys-check.js`). The
  sidebar's keycaps on hover were seen in the real app (2026-10-12). Canvas isn't ⇧⌘C because
  clipboard managers take it for every app: Maccy's popup ships on it, as on this Mac
  (`defaults read org.p0deje.Maccy`, `KeyboardShortcuts_popup` = key 8 with ⇧⌘).

- ⌘1 sending a session's first suggested reply from its box was confirmed in the real app on
  2026-10-10 (`src/menus.ts` `SUGGESTION_CHORDS`): WKWebView hands ⌘ and a digit to the page.
  Not yet checked by hand: the floating panels (the notch and the waiting window, from anywhere in
  them), and a reply's native menu showing its key. The preview shows the keys sending, the keys
  showing only while the box or the list has the keyboard, and ⌘ and a digit from Sessions' list
  still opening the row's places (`suggest-check.js`).

- Not yet checked by hand (2026-10-06): the waiting window in the real app. Whether ⌥⌘. from
  another app brings only it forward or Codealong's other windows too (`set_focus` is tao's
  `makeKeyAndOrderFront` and `activateIgnoringOtherApps`); that ⌥⌘. again gives the keyboard back
  to that app; that Window › Waiting on you opens it; that where it was is kept across a relaunch.
  The preview only shows its page.

- Not yet checked by hand (2026-10-07): the waiting window without a title bar. That its buttons
  sit on the strip, clear of the note and centered on its first line; that the strip drags the
  window (and a double-click zooms it) while its buttons still click; that with nothing shown
  the window drags from anywhere. The preview shows where the strip's first line falls and that
  it asks to drag (`waiting-titlebar-check.js`).

- Not yet checked by hand (2026-10-05): that the app you were in gets its keys back at once when
  the unseen panel takes them from the docked bar and is ordered out, and that the notch no
  longer blinks as a session in it closes (it did while the bar itself was ordered out).
- Not yet checked by hand (2026-09-27): a click in the peek making it key under the
  `isKeyWindow` override.
- Not yet checked by hand (2026-10-02): dragging the peek by its header in the real app; the
  preview only shows that the page asks for it (`peek-drag-check.js`).
- Not yet checked by hand (2026-10-01): keeping, sizing, dragging and closing a kept session in
  the real app; the preview only shows the page and the commands it sends.
- Not yet checked by hand (2026-10-07): a side conversation popped out (kept.rs `pop_side`) in
  the real app: where its window opens beside the main window, the peek, the notch and the
  waiting window (`side_frame`), that it takes the keyboard as it shows, that its strip's top
  line drags it, and that it comes back at launch. The preview shows the window's page and the
  commands it and the box it left send (`side-window-check.js`).

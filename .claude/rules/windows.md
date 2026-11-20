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
  - "src-tauri/src/zoom.rs"
  - "src/ui/Bar.tsx"
  - "src/ui/Notch*.tsx"
  - "src/ui/Peek.tsx"
  - "src/ui/Quick.tsx"
  - "src/ui/Waiting.tsx"
  - "src/ui/DesktopPin.tsx"
  - "src/ui/DesktopLines.tsx"
  - "src/bar.ts"
---

# The app's windows

What Codealong's windows rely on in Tauri, AppKit and WebKit, moved from CLAUDE.md's "Tauri and
macOS" (from the Tauri 2.11 and tao 0.35 sources, and checks on this Mac). A bare file name is
under `src-tauri/src/`.

What the app relies on apart from its windows (commands and the main thread, events and which
windows hear them, menus, the menu bar item, global shortcuts, quitting, the process table, the
links macOS opens with it) is in `tauri.md`.

- Tauri doesn't say whether a window can be seen; AppKit's `NSWindow.occlusionState` (through
  objc2) does: visible means some part is on screen, so not minimized, hidden, on another Space
  or fully covered (`panel::on_screen`).
- Every window's level, and why each is where it is: `panel.rs` (`Level`; its test lists them as
  AppKit counts them, the menu bar being 24 and its status items 25).
- The notch: how it's found and how the docked bar's window keeps one size and takes clicks only
  on its shape: `bar.rs` (module header, `notch_of`, `watch_pointer`), `panel.rs` (`Top`),
  `src/bar.ts` (`NOTCH`, `notchSizes`). On a 16-inch MacBook Pro (2026-09-28) the safe area is
  32, the notch 185 wide and the menu bar 33 tall (`frame` top less `visibleFrame` top). The
  screen it's dragged to is kept by the display's UUID (`CGDisplayCreateUUIDFromDisplayID` on
  its `NSScreenNumber`): the number alone can change when a display is plugged in again, and two
  of one model share a name (`os/windows/macos.rs` `display_id`, `bar.rs` `notch_on`, `drag`).
- The docked bar has the keyboard only while it shows a session, and gives it back through an
  unseen panel that takes it and is ordered out: a panel ordered out and in again, even in one
  turn of the run loop, has WebKit hide its page until it draws again, and the notch blinked
  out as it closed (2026-10-05). `bar.rs` (`set_keys`, `lost_keys`), `panel.rs`
  (`Keys::WhenAsked`, `drop_keys`, `relay`).
- Only a non-activating NSPanel goes onto another app's full-screen Space, so each window's class
  is swapped before it's first shown: `panel.rs` (module header, `builder`, `make`).
- A panel gets its old class back before its window is destroyed, or WebKit's key-value
  observing aborts the app (checked 2026-09-28): `panel.rs` (`unmake`, `make`).
- A page closed while it draws can crash the app in macOS 26.6's WebKit (7624.5.1.11): a display
  refresh after the close reaches its torn-down scrolling coordinator, a SIGSEGV in
  `ScrollingTree::takePendingScrollUpdates` on the main thread (WebKit bug 327576, rdar://189825751;
  WebKit's own code checks for it since 2026-10, which the shipped one doesn't). It happened on
  2026-11-19 as a popped-out side conversation was put back: the system log
  (`log show --predicate 'process == "codealong"'`) has its `WebPageProxy::close` just before, and a
  register held that page. It didn't come in a Swift harness closing drawing WKWebViews 500 times
  (a request of the page's in flight, an event sent, the window ordered out the turn before), so
  it's a narrow race. A page ordered out draws no frames from then (rAF counted in the harness), and
  WebKit drops its display refreshes at the first with nothing to draw, so a panel is hidden a
  moment before it's destroyed: `panel.rs` (`dispose`, `SETTLE`).
- WebKit treats a page as active only while its window says it's key (re-read when the window is
  ordered on screen), and an inactive page gets mouse moves only for its scrollbars (WebKit's
  WebFrame.cpp, checked in an offscreen WKWebView on 2026-09-26): what each panel answers and
  why is `panel.rs` (`Keys`, `present`, `Config`'s `tooltips`). So a panel's page has
  `document.hasFocus()` true while the panel isn't key (Codex in a WKWebView, 2026-11-05); which
  window has the keyboard is `NSApp.keyWindow`, and Tauri's focus events, which come from
  AppKit's became-key and resigned-key notifications, follow it (os/windows `has_keys`,
  `src/terminals.ts` `hasKeys`). A WKWebView takes mouse moves
  from its own tracking area, active always with classic scroll bars and in the key window with
  overlay ones, and has no `mouseMoved:` to call: a test would send them with
  `_simulateMouseMove:`.
- A window ordered front is scaled in by AppKit over a fifth of a second unless its
  `animationBehavior` is none (`panel::no_animation`; `CGWindowListCopyWindowInfo` reports its
  bounds smaller meanwhile). What another process draws can be seen without the screen:
  `screencapture -x -o -l <window id>` captures one window, its clear parts clear, and its id
  comes from the window list (the app's windows by owner name, the panels at layers 24 to 27,
  `panel::Level`).
- WebKit holds back ResizeObserver and animation frames while a window is hidden.
- WebKit runs a hidden page's web process at background priority except while a script the app
  sends runs in it (`evaluateJavaScript`: Tauri's `eval`, and so every event it emits): what that
  cost the quick panel and how it opens inside one script is `quick.rs` (module header).
  `background_throttling` off doesn't change that priority: it only keeps the page from being
  suspended. The process log says when it changes (`log show --predicate 'subsystem ==
  "com.apple.WebKit" AND category == "ProcessSuspension"'`).
- WebKit repaints a rotating path inside an SVG every frame, so the working glyph turns its
  whole `<svg>`: `src/styles/glyphs.css` (`.ca-glyph--working`).
- WebKit draws CSS highlights (`CSS.highlights`) by checking every highlighted range for each
  piece of text it paints, so a repaint costs in proportion to the ranges: on Settings some 5 ms
  a range (50 took 0.3 s, 600 took 3.3 s), with StaticRange no cheaper, where Chrome's cost was
  too small to see (2026-10-22, in a WKWebView). Settings' search marks with boxes of its own
  instead (`src/ui/SettingsFind.tsx` `paint`), and so does the window's find
  (`src/ui/WindowFind.tsx` `paint`), which keeps a highlight only for the match you're on.
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
  app that had it, by bundle ID (`os::apps::frontmost_other`; NSRunningApplication's
  `processIdentifier` needs objc2-app-kit's `libc` feature, which isn't on): `waiting.rs`. It
  has no title bar (an Overlay one with its title hidden): the card's strip is one
  (`src/ui/SessionPanel.tsx` `Hotkeys.titleBar`).
- In a window with an Overlay title bar, macOS 26 leaves the window's buttons where it puts
  them: 14-point buttons 23 apart, 9 to 69 points in, centered 16 down in a 32-point title bar.
  tao's `traffic_light_position(x, y)` moves the close button to x and their middle to y − 2, and
  does it again on every redraw (`inset_traffic_lights`; measured in a plain AppKit window on
  2026-10-07). The page draws under them and gets the clicks there but on the buttons, so it
  drags the window itself (`data-tauri-drag-region`): `waiting.rs`, `src/ui/SessionPanel.css`
  (`.is-titlebar`), `src/ui/MainBar.tsx` (`ca-win-bar`).
- The peek: hover shows it without the keyboard, a click gives it the keyboard, it watches the
  pointer to close, keeps a thin margin on the bar's side, and is dragged from the engine:
  `peek.rs` (module header, `watch_pointer`, `NEAR`, `drag`), `panel.rs` (`Keys::OnClick`).
- A session kept on the desktop is a panel of the peek's kind: `kept.rs` (module header, `close`,
  which gives its class back, and `on_window_event`, which orders one that lost the keyboard in
  again saying it's key), `panel.rs` (`ENGAGED`).
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

- Not yet checked by hand (2026-10-12): pressing ⇧⌘S, ⇧⌘P, ⇧⌘T and ⇧⌘B in the real app
  (`src/mainKeys.ts` `VIEW_CHORDS`). They're key equivalents, which reach the page before the
  menu bar (above), so they should behave as the preview shows (`view-keys-check.js`). The
  sidebar's keycaps on hover were seen in the real app (2026-10-12). Nor (2026-10-21) the same
  keys in the windows that float bringing the main window forward on the view, with the notch
  giving the keyboard back first (`useViewKeys`); the preview shows each asking for it
  (`view-keys-elsewhere-check.js`). Canvas isn't ⇧⌘C because
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

- Not yet checked by hand (2026-10-27): dragging the notch to another screen in the real app
  (`bar.rs` `drag`): that it slides along the top of each screen the pointer crosses, docks on
  the one it's let go on, keeps it across a relaunch, falls back when that display is unplugged
  and returns when it's plugged in again; and that the UUID stays the same for a display plugged
  into another port. The preview shows only where a drag starts and what it asks for
  (`notch-move-check.js`); the engine's choice of screen is tested (`bar.rs`).
- Not yet checked by hand (2026-10-05): that the app you were in gets its keys back at once when
  the unseen panel takes them from the docked bar and is ordered out, and that the notch no
  longer blinks as a session in it closes (it did while the bar itself was ordered out).
- Not yet checked by hand (2026-09-27): a click in the peek making it key under the
  `isKeyWindow` override.
- Not yet checked by hand (2026-10-02): dragging the peek by its header in the real app; the
  preview only shows that the page asks for it (`peek-drag-check.js`).
- Not yet checked by hand (2026-11-13): the peek opened from the main window's PR or todo panel
  (peek.rs `beside`, `src/ui/MainPanel.tsx` `peekFromPanel`), on a hover since the same day. That
  it shows beside the panel, its card level with the row the pointer rests on, without taking the
  keyboard from the main window; that it stays up as the pointer crosses the panel along that
  row to it and closes once the pointer leaves both (the row counts across the panel's width);
  that a click in it takes the keyboard, and a click back in the main window, Escape and ⏎ put it
  away and the main window has the keyboard again after; that a click on the session puts it
  away as Sessions shows the session; that with the PR panel taking the whole window it opens
  under the row (or over it), leaving the row to click (`across`); and that it lands where the
  page measured at a zoom (`peekBeside` scales by the page zoom). On
  Linux and Windows it goes by the window's inside (`os::windows::content`), under the title bar
  the window manager draws, which hasn't run there. The engine's placement is tested and the
  preview shows what the page asks for and that a failure in it toasts in the main window
  (`panel-peek-check.js`).
- Not yet checked by hand (2026-10-01): keeping, sizing, dragging and closing a kept session in
  the real app; the preview only shows the page and the commands it sends.
- Not yet checked by hand (2026-10-30): the quick panel folding in the real app (quick.rs
  `put_aside`). That a click in another app folds it to the chip where its top was without the
  chip moving as the window shrinks round it; that a click on the chip (it's a non-activating
  panel that accepts the first mouse) both makes it key and reaches the page, which asks for the
  open that unfolds it, and that the caret comes back where it was; that ⌥⌘/ unfolds it from
  another app; that × puts it away without unfolding first; and that the chip follows onto
  another app's full-screen Space. The preview shows the page's side (`quick-fold-check.js`).
- Not yet checked by hand (2026-11-10): a file dragged onto the quick panel from the Finder in the
  real app (quick.rs `lost_keys`, `dragged`). That the panel stays up while the button is down
  after the click in the Finder takes the keyboard, and that the panel takes drops (wry's handler)
  while it isn't key and Codealong isn't active; that it takes the keyboard back once the file
  lands, the caret in the box; that a click elsewhere still folds or hides it once the button comes
  up; that a file let go on the folded chip opens it again; and that `SETTLE` is long enough for a
  file let go the instant it reaches the panel: its drag events come from the other app once it
  sees the button up, behind the look that saw it, and a panel put aside before they arrive hides
  if it's empty. The preview shows the page keeping files let go on the list, the shadow and the chip
  (`quick-drop-check.js`).
- Not yet checked by hand (2026-11-09): a session popped out of the main window's panel (kept.rs
  `pop_session`) in the real app: where its window opens beside the main window, or over its
  panel's side when there's no room (`beside`, `SESSION_SIZE`), that it takes the keyboard in its
  box, that Put it back here and its Main window close it, and that the main window's panel
  follows (`session-windows`). The preview shows the pages and the commands they send
  (`session-window-check.js`).
- Not yet checked by hand (2026-10-07): a side conversation popped out (kept.rs `pop_side`) in
  the real app: where its window opens beside the main window, the peek, the notch and the
  waiting window (`side_frame`), that it takes the keyboard as it shows, that its strip's top
  line drags it, and that it comes back at launch. The preview shows the window's page and the
  commands it and the box it left send (`side-window-check.js`).

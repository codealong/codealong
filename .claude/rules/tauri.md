---
paths:
  - "src-tauri/src/lib.rs"
  - "src-tauri/src/front.rs"
  - "src-tauri/src/procs.rs"
  - "src-tauri/capabilities/**"
  - "src-tauri/tauri*.json"
---

# Tauri and macOS under the app

What the app as a whole relies on in Tauri and macOS, apart from its floating windows
(`windows.md`): commands and the main thread, events and which windows hear them, menus, the
menu bar item, global shortcuts, quitting, the process table, and the links macOS opens with
it. From the Tauri 2.11 and tao 0.35 sources, and checks on this Mac, moved from `windows.md`;
read it before changing a command, an event or a menu in `lib.rs`. A bare file name is under
`src-tauri/src/`.

- A click in a non-activating panel leaves the app inactive, so the bar asks macOS to bring the
  app it opened forward: `front.rs` (`bring_forward`).
- `proc_pidinfo` answers only for your own processes, `sysctl` KERN_PROC_PID for any, and a
  killed process stuck in the kernel is reaped on a thread of its own: `procs.rs` (`listed`,
  `run`). KERN_PROCARGS2 gives your own processes' environments, but not those of Apple's own
  programs, which only root sees: `procs.rs` (`environment`).
- Only a plain Quit reaches `RunEvent::Exit`: a copy taking over ends the process itself (the
  server tells it to quit, view.rs `take_over`), relaunch.sh sends SIGTERM, and `AppHandle::restart` on the main
  thread skips the exit events (tauri 2.12.5), so what must survive a relaunch is written as it
  changes: `frames.rs` (module header). tao marks a window full screen in
  `windowWillEnterFullScreen`, before its animation, and clears that before the last resize on
  the way out, so `is_fullscreen` keeps the animation's frames from being noted.
- A plain JS `listen()` hears events sent to any window, and a window's own `emit` goes to every
  window: `src/api.ts` (`onWindowEvent`), and `lib.rs` beside the Settings menu's `emit_to`.
- A window's label must be listed in `src-tauri/capabilities/default.json` (a `*` at the end
  matches the rest), or its page's `listen` is refused and it never hears an event (2026-09-30):
  `lib.rs` (`every_window_may_listen_for_events`, whose list a new window's label goes in too).
  The preview's stub allows everything, so it can't show this.
- A menu item's icon as raw RGBA, `{rgba, width, height}`, which needs none of Tauri's image
  features though its JS types don't list it, and muda's 18-point item images: `src/contextMenu.ts`
  (`MenuImage`, `options`), `editors.rs` (`ICON_PX`, `menus_take_the_icons_as_pages_send_them`).
- `show()` orders a window front without activating the app; `set_focus()` activates it.
  Commands without `async` run on the main thread; one marked `async` that can wait on something
  slow goes through `lib.rs` (`blocking`, which says why; tauri-macros 2.6), and errors go to a
  window as `util::words`.
- Transparent windows need `macOSPrivateApi` in tauri.conf.json and the `macos-private-api`
  feature. `data-tauri-drag-region="deep"` drags from anywhere inside except controls.
- The menu bar item is Tauri's tray (tauri 2.12.5, tray-icon 0.25.1): `menubar.rs` (module
  header). Its calls wait for the main thread when made from another. tray-icon draws the image
  18 points tall whatever its size, and its `set_icon` turns the template off, so a template
  goes on with `set_icon_with_as_template`. Its menu's events reach every `on_menu_event`
  handler, the app's menus' too, so ids carry a prefix. On Linux the first item made loads
  libayatana-appindicator3, and a missing library panics there (`make`); with the library but
  nothing on the session bus to show items, it's made and never seen
  (`os::windows::tray_shown`).
- tauri-plugin-global-shortcut's `register` and `unregister` wait on the main thread, so they're
  called off it: `quick.rs` (`apply_shortcut`), `lib.rs`. Carbon's `RegisterEventHotKey`
  succeeds for keys another app already holds, so clashes can't be found by registering. Apps
  keep theirs in their preferences: the KeyboardShortcuts library as `KeyboardShortcuts_<name>`
  = `{carbonKeyCode, carbonModifiers}` (ChatGPT in `com.openai.chat`), macOS in
  `com.apple.symbolichotkeys`, which lists only the ones changed from their defaults.
- Tauri's `plugin:menu|popup` (2.11.6, the same in 2.12.0) keeps the page's resource table
  locked while a menu is up, which hung the app: `lib.rs` (`popup_menu`).
- A right-click's native menu (muda's `popUpMenuPositioningItem`) runs its loop while the button
  is still down, and the loop takes its release: WebKit then takes that button as held, so the
  next press of the left comes to the page as a `pointermove` naming button 0 (the Pointer Events'
  chorded press), not a `pointerdown`, every move of its drag names button 0 too (WebKit's last
  button stays the right's), and only its release comes as a `pointerup`. ⌃-click comes as a
  `pointerdown` of button 0 with `ctrlKey`, then `contextmenu`. Both seen in a WKWebView harness
  (Swift, events sent straight to the web view, the right button's release left out) on
  2026-11-14; that the real menu's loop takes the release is read from muda's code and AppKit's
  menu tracking, not seen. The terminal goes by both (`ui/terminal/controller.ts` `pointer`);
  anything else that reads presses from pointer events misses the first press after a menu.
- The links macOS opens with an app (LaunchServices, checked with two stub apps claiming a
  made-up scheme in `CFBundleURLTypes`, macOS 26.6, 2026-11-10). Left to itself it opens a
  scheme's links with the app it registered last. `NSWorkspace`'s
  `setDefaultApplication(at:toOpenURLsWithScheme:)` makes one app the scheme's from then on: it
  answered at once and asked nobody, for a scheme that's an app's own, from a program that
  wasn't either app. The choice is kept by bundle ID (`LSHandlers` in
  `com.apple.LaunchServices/com.apple.launchservices.secure`): it held when the other app
  registered again, went to the other while the chosen app was unregistered, and came back with
  it. A click that launches the app is delivered (`application:openURLs:`) before
  `applicationDidFinishLaunching`. An app under a temporary folder (`/private/tmp`) is
  registered `launch-disabled in-temp-dir` and opens no links, whatever it claims;
  `lsregister -dump` shows each app's flags and claimed schemes.
  `os/apps/macos.rs` (`opened_links`, `claim_links`), `front.rs` (`open_links`).

## Unchecked

- Not yet checked by hand (2026-11-10): a link to a session that launches Codealong, the app
  closed when it's clicked (`front.rs` `open_links`): that the main window takes the link once
  it's up and shows the session. With Codealong Dev running, `open codealong://session/<key>`
  and one with only the start of an id each showed its session (a capture of the window,
  2026-11-10), and the preview shows the page taking a link that came before it
  (`opened-link-check.js`). Nor a click on a link's words in a real terminal; nor a Codealong
  in Applications that claims the scheme beside Codealong Dev (the one installed then didn't
  yet), where the stub apps say Codealong Dev's claim holds.

## Elsewhere

- The windows that float (panels and their levels, which has the keyboard, the notch, the quick
  panel, the peek) and what WebKit does in a page are in `windows.md`.
- A terminal's app that macOS won't bring forward for a background process is in `terminals.md`
  (Ghostty) and `rex.md`.

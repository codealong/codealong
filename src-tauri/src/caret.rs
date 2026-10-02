// The text cursor lost in a window you're typing in. WebKit draws it only while it has the page
// as focused (its view AppKit's first responder) in a window it has as key, and learns both from
// AppKit's notifications; a page that gets typing while WebKit has it otherwise shows the text
// but no cursor. The page notices (`document.hasFocus()` false on a key typed into a text field,
// src/caret.ts) and asks for this: it writes to the perf log what AppKit says of the window, then
// hands the page its focus again, which makes WebKit ask AppKit afresh.

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSWindowDidBecomeKeyNotification};
use objc2_foundation::NSNotificationCenter;
use tauri::WebviewWindow;

use crate::{panel, perf};

fn yes(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// On the main thread.
pub fn refocus(w: &WebviewWindow) {
    let label = w.label().to_string();
    panel::with_page(w, move |ns, view| {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let first = match ns.firstResponder() {
            None => "nothing".to_string(),
            Some(f) if std::ptr::eq(&*f, view) => "the page".to_string(),
            Some(f) => format!("a {}", f.class().name().to_string_lossy()),
        };
        let key = ns.isKeyWindow();
        let state = format!("key window {}, Codealong active {}, first responder {first}", yes(key), yes(NSApplication::sharedApplication(mtm).isActive()));
        perf::log(&format!("ui[{label}] typing into a text field WebKit had as unfocused: {state}"));
        // Taking the page's focus and handing it back makes WebKit ask whether it's focused.
        ns.makeFirstResponder(None);
        ns.makeFirstResponder(Some(view));
        // And a key window WebKit missed becoming key is said to again, which makes it ask
        // whether the window is key; AppKit's own state doesn't change.
        if key {
            // SAFETY: an AppKit constant, read on the main thread.
            let name = unsafe { NSWindowDidBecomeKeyNotification };
            // SAFETY: posting a notification about the window, which is alive and key.
            unsafe { NSNotificationCenter::defaultCenter().postNotificationName_object(name, Some(ns.as_ref())) };
        }
    });
}

// The text cursor lost in a window you're typing in. WebKit draws it only while it has the page
// as focused (its view AppKit's first responder) in a window it has as key, and learns both from
// AppKit's notifications; a page that gets typing while WebKit has it otherwise shows the text
// but no cursor. The page notices (`document.hasFocus()` false on a key typed into a text field,
// src/caret.ts) and asks for this: it writes to the perf log what AppKit says of the window, then
// hands the page its focus again, which makes WebKit ask AppKit afresh.

use tauri::WebviewWindow;

use crate::perf;

/// On the main thread.
pub fn refocus(w: &WebviewWindow) {
    let label = w.label().to_string();
    native::refocus(w, move |state| perf::log(&format!("ui[{label}] typing into a text field WebKit had as unfocused: {state}")));
}

#[cfg(target_os = "macos")]
mod native {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSResponder, NSWindow, NSWindowDidBecomeKeyNotification};
    use objc2_foundation::NSNotificationCenter;
    use tauri::WebviewWindow;

    fn yes(b: bool) -> &'static str {
        if b {
            "yes"
        } else {
            "no"
        }
    }

    pub fn refocus(w: &WebviewWindow, log: impl FnOnce(String) + Send + 'static) {
        let window = w.clone();
        let _ = w.with_webview(move |webview| {
            let Some(mtm) = MainThreadMarker::new() else { return };
            let Ok(ptr) = window.ns_window() else { return };
            // SAFETY: Tauri hands out the window's own NSWindow and, on macOS, its webview's
            // WKWebView (an NSResponder), which live as long as the window; this is the main
            // thread, where AppKit expects to be called.
            let (Some(ns), Some(view)) = (unsafe { (ptr as *const NSWindow).as_ref() }, unsafe { (webview.inner() as *const NSResponder).as_ref() }) else {
                return;
            };
            let first = match ns.firstResponder() {
                None => "nothing".to_string(),
                Some(f) if std::ptr::eq(&*f, view) => "the page".to_string(),
                Some(f) => format!("a {}", f.class().name().to_string_lossy()),
            };
            let key = ns.isKeyWindow();
            log(format!("key window {}, Codealong active {}, first responder {first}", yes(key), yes(NSApplication::sharedApplication(mtm).isActive())));
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
}

#[cfg(not(target_os = "macos"))]
mod native {
    use tauri::WebviewWindow;

    pub fn refocus(_: &WebviewWindow, _: impl FnOnce(String) + Send + 'static) {}
}

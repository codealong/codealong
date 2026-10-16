// The text cursor lost in a window you're typing in. WebKit draws it only while it has the page
// as focused (its view AppKit's first responder) in a window it has as key, and learns both from
// AppKit's notifications; a page that gets typing while WebKit has it otherwise shows the text
// but no cursor. The page notices (`document.hasFocus()` false on a key typed into a text field,
// src/caret.ts) and asks for this: it writes to the perf log what AppKit says of the window, then
// hands the page its focus again, which makes WebKit ask AppKit afresh. GTK has no first
// responder to hand back, so on Linux the report is only written down.

use tauri::WebviewWindow;

use crate::{os, perf};

/// On the main thread.
pub fn refocus(w: &WebviewWindow) {
    let label = w.label().to_string();
    os::windows::refocus_page(w, move |state| {
        let said = state.map(|s| format!(": {s}")).unwrap_or_default();
        perf::log(&format!("ui[{label}] typing into a text field WebKit had as unfocused{said}"));
    });
}

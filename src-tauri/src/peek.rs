//! The floating bar's peek: hovering a session's pill shows that session in a panel under the bar
//! (over it, when the bar sits low on the screen): its conversation as its panel in the main
//! window has it, and a box to answer it (Peek.tsx). It's the quickest way to answer a session:
//! hover, click the box, type, ⏎.
//!
//! Like the bar and the quick panel it's a non-activating panel, so it shows over full-screen apps
//! and leaves the app you're in active. Hovering never gives it the keyboard, so passing the
//! pointer over the bar can't take what you're typing elsewhere; a click in it does. Until then it
//! closes once the pointer has been off both its pill and the peek for a moment, and hovering
//! another pill switches it to that session. With the keyboard it stays on its session until you
//! send, press Escape, click somewhere else or bring up another of Codealong's windows, whatever
//! the bar does meanwhile. Should another app take the keyboard while the pointer is still on the
//! peek, it goes back to how a hover has it rather than closing under you.
//!
//! Dragging its far edge or corner makes it bigger, and dragging its header moves it. Its Keep
//! button or letting go of either drag keeps it on the desktop: the session gets a window of its
//! own there (kept.rs), which stays up, and the peek goes back to being the peek.
//!
//! Opening is the quick panel's handshake: Codealong tells the page which session (`peek-open`),
//! the page reads the conversation and answers with its height (`peek_ready`), and only then does
//! the panel show, so it never flashes the session it showed before.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent};

use crate::geometry::{screen_for, PageRect, Rect, CARD_MARGIN};
use crate::config::Theme;
use crate::model::{Board, Lane, Session};
use crate::panel;

pub const LABEL: &str = "peek";
/// Room around the card for its shadow (`--shadow-peek`, drawn to fit it): the window is this
/// much bigger on its sides (Peek.tsx).
const MARGIN: f64 = 20.0;
/// And this much on the side away from the bar, where the shadow falls.
const FAR: f64 = 32.0;
/// On the side toward the bar the window is only this much bigger, so it never covers the bar's
/// pills (a window takes the clicks on its clear parts too). The shadow falls away from there.
const NEAR: f64 = 8.0;
/// The window's width: the card and its margins.
const WIDTH: f64 = 480.0 + 2.0 * MARGIN;
/// The tallest the window gets.
const MAX_HEIGHT: f64 = 580.0 + FAR + NEAR;
/// The smallest a peek can be dragged to, and a kept session's window (kept.rs): the window, its
/// margins included.
pub const MIN_SIZE: (f64, f64) = (360.0 + 2.0 * MARGIN, 240.0 + FAR + NEAR);
/// With less room than this under the bar, the peek opens over it when there's more room there.
const MIN_ROOM: f64 = 360.0;
/// The bar's page leaves this much room around its card (Bar.tsx), as a desktop card's page does
/// (DesktopPin.tsx); docked in the notch, the frame the peek goes by has it around the shape
/// (`bar::peek_frame`).
const BAR_MARGIN: f64 = CARD_MARGIN;
/// Space between the bar's card and the peek's: no less than `NEAR`, so the peek's window starts
/// where the bar's card ends.
const GAP: f64 = 8.0;
/// The pill counts this far past its edges, so the pointer can wobble on its way to the peek.
const PILL_SLACK: f64 = 6.0;
/// How long the pointer may be off the pill and the peek before the peek closes.
const LEAVE_AFTER: Duration = Duration::from_millis(350);
/// How often the pointer is looked at while the peek is up without the keyboard.
const WATCH_EVERY: Duration = Duration::from_millis(60);
/// How long an open waits for the page before showing anyway.
const READY_WAIT: Duration = Duration::from_millis(400);
/// The peek's panel: a click in it gives it the keyboard, and until then it says it has it, so
/// its page gets hover and tooltips. It's one level over the floating bar, on whichever Space is
/// in front, full-screen ones too, coming and going.
const PANEL: panel::Config = panel::Config { keys: panel::Keys::OnClick, level: panel::Level::Peek, spaces: panel::Spaces::Transient, unconstrained: false, tooltips: true, steps: panel::Steps::One };

/// Where the peek goes: its left edge, and the edge it keeps while its height changes (its top
/// under the bar, its bottom over it), in AppKit points (y grows up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub x: f64,
    pub edge: f64,
    pub below: bool,
    /// The tallest the window can be there.
    pub room: f64,
    /// The room there is to the screen's edge, for a peek dragged bigger than it opens.
    pub full: f64,
    /// The widest it can be dragged, to the screen's right edge.
    pub wide: f64,
}

impl Place {
    /// The window's frame at a height: hanging from its top edge under the bar, standing on its
    /// bottom edge over it.
    pub fn frame(&self, height: f64) -> Rect {
        let h = height.min(self.room).max(1.0);
        if self.below {
            Rect { x: self.x, y: self.edge - h, w: WIDTH, h }
        } else {
            Rect { x: self.x, y: self.edge, w: WIDTH, h }
        }
    }

    /// The window's frame dragged to a size: it keeps its left edge and the edge by the bar, no
    /// smaller than it opens and no bigger than the screen has room for.
    pub fn sized(&self, width: f64, height: f64) -> Rect {
        let w = width.min(self.wide).max(MIN_SIZE.0);
        let h = height.min(self.full).max(MIN_SIZE.1.min(self.full));
        Rect { x: self.x, y: if self.below { self.edge - h } else { self.edge }, w, h }
    }
}

/// Where the peek goes for a pill: under the bar's card (`bar` is its window, or a desktop card's),
/// centered on the pill and kept on the screen, or over it when there's too little room under it
/// and more over it.
pub fn place(bar: Rect, pill: Rect, screen: Rect) -> Place {
    let below_edge = bar.y + BAR_MARGIN - GAP + NEAR;
    let above_edge = bar.top() - BAR_MARGIN + GAP - NEAR;
    let room_below = below_edge - screen.y;
    let room_above = screen.top() - above_edge;
    let below = room_below >= MIN_ROOM || room_below >= room_above;
    let (cx, _) = pill.center();
    let x = (cx - WIDTH / 2.0).min(screen.right() - WIDTH).max(screen.x).round();
    let room = if below { room_below } else { room_above };
    Place { x, edge: if below { below_edge } else { above_edge }, below, room: room.min(MAX_HEIGHT), full: room, wide: screen.right() - x }
}

/// The peek's frame dragged by its header: moved as far as the pointer has gone since the drag
/// began, at the size it had.
pub fn dragged(from: Rect, began: (f64, f64), pointer: (f64, f64)) -> Rect {
    Rect { x: from.x + pointer.0 - began.0, y: from.y + pointer.1 - began.1, ..from }
}

/// Whether the pointer is on the peek or on the pill it came from.
pub fn over(p: (f64, f64), peek: Rect, pill: Option<Rect>) -> bool {
    peek.contains(p) || pill.is_some_and(|r| r.outset(PILL_SLACK).contains(p))
}

/// What the bar and the page hear about the peek.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Shown {
    /// The session it shows, while it's up.
    pub key: Option<String>,
    /// It has the keyboard.
    pub engaged: bool,
}

/// What an open tells the page.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Opening {
    id: u64,
    lane: Lane,
    session: Session,
    terminal: String,
    /// The editors installed here, for the session menu's Open folder in (menus.ts).
    apps: crate::model::AppNames,
    theme: Theme,
    /// The tallest the window can be where it opens.
    room: f64,
    /// Under the bar, so the card sits at the window's top; over it, at its bottom.
    below: bool,
    /// It takes the keyboard as it shows (Reply here in the pill's menu), not on a click.
    focus: bool,
    /// Its name is ready to change (Rename in the pill's menu).
    rename: bool,
    /// `[debug] perf_log` is on, so the page reports its timings.
    perf_log: bool,
}

#[derive(Default)]
struct Peek {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// The session shown, or about to be.
    key: Option<String>,
    /// The window its pill is in: the bar, or a card pinned to the desktop (desktop.rs).
    host: Option<Rect>,
    /// Its pill on the screen.
    pill: Option<Rect>,
    place: Option<Place>,
    opens: u64,
    /// The open the page hasn't answered yet.
    waiting: Option<u64>,
    /// The open waiting wants the keyboard.
    focus: bool,
    visible: bool,
    /// It has the keyboard.
    engaged: bool,
    /// Since when the pointer has been off the peek and its pill.
    away_since: Option<Instant>,
    /// Counts watches of the pointer, so an old one stops.
    watch: u64,
    /// You dragged it to a size, or somewhere else: it keeps the size it has, whatever its page
    /// measures.
    sized: bool,
    /// Its header is being dragged: its frame when the drag began, and where the pointer was when
    /// the button went down.
    drag: Option<(Rect, (f64, f64))>,
    /// Counts drags of its header, so an old one stops following the pointer.
    drags: u64,
}

/// Makes the peek's window, hidden, at launch, so a hover never waits for a page to load.
pub fn setup(app: &AppHandle) -> Result<()> {
    app.manage(Peek::default());
    let page = panel::Page { title: "Codealong", script: "window.__CODEALONG_PEEK__ = true;", size: (WIDTH, 320.0), focusable: true, first_mouse: true };
    let window = panel::builder(app, LABEL, page).build()?;
    // The peek is never destroyed, so the class it had isn't kept.
    let _ = panel::make(&window, PANEL);
    Ok(())
}

/// A session to show, with what the page needs besides it.
pub struct Target {
    pub lane: Lane,
    pub session: Session,
    pub terminal: String,
    pub apps: crate::model::AppNames,
    pub theme: Theme,
}

/// The session and its lane, as the board has them.
pub fn target(board: &Board, key: &str) -> Option<Target> {
    let (lane, session) = board.session(key).map(|(l, s)| (l.clone(), s.clone()))?;
    Some(Target { lane, session, terminal: board.apps.terminal.clone(), apps: board.apps.clone(), theme: board.theme })
}

/// Shows a session in the peek, for a hover over its pill (`pill`: the window it's in, the bar or
/// a card on the desktop, and the pill as that window's page measured it) or, with `focus`, ready
/// to type into, its name with `rename`. Without a pill it opens where it last was. A hover
/// doesn't take the peek from a session you're typing to. On the main thread.
pub fn open(app: &AppHandle, target: Target, pill: Option<(Rect, PageRect)>, focus: bool, rename: bool) -> Result<()> {
    let Some(state) = app.try_state::<Peek>() else { return Ok(()) };
    let bar = crate::bar::frame(app);
    let pill = pill.map(|(host, p)| (host, p.to_screen(host)));
    let key = target.session.key.clone();
    let (id, room, below) = {
        let mut s = state.inner.lock().map_err(|_| anyhow!("The peek is unavailable."))?;
        let same = s.key.as_deref() == Some(key.as_str());
        // Nor from one you're dragging, whose pointer may cross other pills on its way.
        if s.visible && (s.engaged || s.drag.is_some()) && !focus && !same {
            return Ok(());
        }
        if let Some((host, p)) = pill {
            s.host = Some(host);
            s.pill = Some(p);
        }
        if s.visible && same && !focus {
            s.away_since = None;
            return Ok(());
        }
        let screens = panel::screens();
        let host = s.host.or(bar);
        let anchor = s.pill.or(host);
        let screen = anchor.and_then(|a| screen_for(a, &screens));
        // The bar docked in the notch keeps room in its window for the shape's shadow, so the peek
        // goes by the shape (`bar::peek_frame`).
        let by = if host.is_some() && host == bar { crate::bar::peek_frame(app) } else { host };
        if let (Some(h), Some(a), Some(sc)) = (by, anchor, screen) {
            s.place = Some(place(h, a, sc));
        }
        s.opens += 1;
        s.waiting = Some(s.opens);
        s.focus = focus;
        s.key = Some(key);
        s.away_since = None;
        s.sized = false;
        s.drag = None;
        (s.opens, s.place.map_or(MAX_HEIGHT, |p| p.room), s.place.is_none_or(|p| p.below))
    };
    let Target { lane, session, terminal, apps, theme } = target;
    let _ = app.emit_to(LABEL, "peek-open", Opening { id, lane, session, terminal, apps, theme, room, below, focus, rename, perf_log: crate::perf::enabled() });
    // A page that doesn't answer doesn't keep the peek from showing.
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(READY_WAIT);
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || ready(&app, id, None));
    });
    Ok(())
}

/// The page has laid out the open `id` at this height: show the peek there, with the keyboard if
/// that open asked for it. A late answer to an older open is ignored. On the main thread.
pub fn ready(app: &AppHandle, id: u64, height: Option<f64>) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    let (frame, focus, watch) = {
        let Ok(mut s) = state.inner.lock() else { return };
        if s.waiting != Some(id) {
            // The page answered after the peek showed without it (`READY_WAIT`), at the height the
            // window had for the session before. The page won't say its height again until it
            // changes, so the peek takes it now, or it stays cut off (or too tall).
            let late = s.opens == id && s.visible;
            drop(s);
            if let Some(h) = height.filter(|_| late) {
                fit(app, h);
            }
            return;
        }
        let height = height.or_else(|| panel::frame(&window).map(|f| f.h)).unwrap_or(320.0);
        // Nowhere to put it (the bar isn't there), or the pointer moved on while the page read
        // the conversation: nothing to show it for, and the page hears so.
        let away = |f: &Rect| !s.focus && !s.visible && panel::pointer().is_some_and(|p| !over(p, *f, s.pill));
        let Some(frame) = s.place.map(|p| p.frame(height)).filter(|f| !away(f)) else {
            drop(s);
            hide(app);
            return;
        };
        s.waiting = None;
        s.visible = true;
        if s.focus {
            s.engaged = true;
        }
        s.watch += 1;
        (frame, s.focus, (!s.focus).then_some(s.watch))
    };
    panel::set_frame(&window, frame);
    panel::present(&window, focus);
    if let Some(n) = watch {
        watch_pointer(app, n);
    }
    tell(app);
}

/// The page changed size while up: the peek follows, keeping its edge by the bar. On the main
/// thread.
pub fn fit(app: &AppHandle, height: f64) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    let Some(frame) = state.inner.lock().ok().filter(|s| s.visible && s.waiting.is_none() && !s.sized).and_then(|s| s.place).map(|p| p.frame(height.ceil())) else { return };
    if panel::frame(&window) != Some(frame) {
        panel::set_frame(&window, frame);
    }
}

/// You're dragging the peek's edge: it takes that size, keeping its left edge and the edge by the
/// bar, and from then on keeps it. On the main thread.
pub fn resize(app: &AppHandle, width: f64, height: f64) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    let frame = {
        let Ok(mut s) = state.inner.lock() else { return };
        if !s.visible {
            return;
        }
        let Some(place) = s.place else { return };
        s.sized = true;
        place.sized(width.ceil(), height.ceil())
    };
    if panel::frame(&window) != Some(frame) {
        panel::set_frame(&window, frame);
    }
}

/// You're dragging the peek by its header, and the pointer has gone `dx`, `dy` since the button
/// went down (as the page counts, y growing down): from here the peek goes where the pointer
/// takes it, at the size it has, and once the button comes up the session is kept on the desktop
/// where the peek was left (kept.rs), as it is when an edge is let go. The pointer is read here
/// until then, as it is for a card dragged to the desktop (desktop.rs), rather than from the
/// page's pointer events, which come from a window that's moving under the pointer. On the main
/// thread.
pub fn drag(app: &AppHandle, dx: f64, dy: f64) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    let (Some(p), Some(frame)) = (panel::pointer(), panel::frame(&window)) else { return };
    let n = {
        let Ok(mut s) = state.inner.lock() else { return };
        if !s.visible || s.waiting.is_some() || s.drag.is_some() {
            return;
        }
        s.sized = true;
        s.drags += 1;
        // AppKit's y grows up.
        s.drag = Some((frame, (p.0 - dx, p.1 + dy)));
        s.drags
    };
    // Where the pointer is already, without waiting for the next look.
    follow_drag(app, n);
    let going = move |app: &AppHandle| app.try_state::<Peek>().is_some_and(|s| s.inner.lock().is_ok_and(|s| s.drags == n && s.drag.is_some()));
    panel::poll(app, panel::DRAG_EVERY, going, move |app| follow_drag(app, n));
}

/// Looks at the pointer for a drag of the peek's header: the peek goes where the pointer has
/// taken it, and with the button up it's kept there. On the main thread.
fn follow_drag(app: &AppHandle, n: u64) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    let Some(p) = panel::pointer() else { return };
    let down = panel::button_down();
    let frame = {
        let Ok(mut s) = state.inner.lock() else { return };
        if s.drags != n || !s.visible {
            return;
        }
        let Some((from, began)) = s.drag else { return };
        if !down {
            s.drag = None;
        }
        dragged(from, began, p)
    };
    if panel::frame(&window) != Some(frame) {
        panel::set_frame(&window, frame);
    }
    if !down {
        if let Err(e) = crate::kept::keep_peeked(app) {
            eprintln!("Couldn't keep the session on the desktop: {e:#}");
        }
    }
}

/// Whether the peek has the keyboard.
pub fn engaged(app: &AppHandle) -> bool {
    app.try_state::<Peek>().is_some_and(|s| s.inner.lock().is_ok_and(|s| s.visible && s.engaged))
}

/// The session the peek shows, and where its card is on the screen: the window less the room it
/// keeps for the card's shadow. For keeping it on the desktop (kept.rs). On the main thread.
pub fn showing(app: &AppHandle) -> Option<(String, Rect)> {
    let (state, window) = (app.try_state::<Peek>()?, app.get_webview_window(LABEL)?);
    let s = state.inner.lock().ok()?;
    let (key, below) = (s.key.clone().filter(|_| s.visible)?, s.place.is_none_or(|p| p.below));
    let f = panel::frame(&window)?;
    // AppKit's y grows up: under the bar the near side is the window's top.
    let (under, over) = if below { (FAR, NEAR) } else { (NEAR, FAR) };
    Some((key, Rect { x: f.x + MARGIN, y: f.y + under, w: f.w - 2.0 * MARGIN, h: f.h - under - over }))
}

/// A kept session's window for a card at `card`: the room around it the peek keeps under the bar,
/// so its page lays out as the peek's does there.
pub fn window_for(card: Rect) -> Rect {
    Rect { x: card.x - MARGIN, y: card.y - FAR, w: card.w + 2.0 * MARGIN, h: card.h + FAR + NEAR }
}

/// Gives the peek the keyboard: you clicked in it. On the main thread.
pub fn take_keys(app: &AppHandle) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    if state.inner.lock().is_ok_and(|s| s.visible) {
        panel::take_keys(&window);
    }
}

/// Hides the peek. On the main thread.
pub fn hide(app: &AppHandle) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    let was = {
        let Ok(mut s) = state.inner.lock() else { return };
        let was = s.visible || s.waiting.is_some();
        s.visible = false;
        s.engaged = false;
        s.waiting = None;
        s.key = None;
        s.away_since = None;
        s.watch += 1;
        s.sized = false;
        s.drag = None;
        was
    };
    panel::hide(&window);
    if was {
        let _ = app.emit_to(LABEL, "peek-hidden", ());
        tell(app);
    }
}

/// Tells the bar and the cards on the desktop (which mark the pill or card) and the page what the
/// peek shows.
fn tell(app: &AppHandle) {
    let Some(state) = app.try_state::<Peek>() else { return };
    let Some(shown) = state.inner.lock().ok().map(|s| Shown { key: s.key.clone().filter(|_| s.visible), engaged: s.engaged }) else { return };
    for label in crate::desktop::labels(app) {
        let _ = app.emit_to(label.as_str(), "peek", shown.clone());
    }
    let _ = app.emit_to(crate::bar::LABEL, "peek", shown.clone());
    let _ = app.emit_to(LABEL, "peek", shown);
}

/// While the peek is up without the keyboard, looks where the pointer is and closes the peek once
/// it has been off both the peek and its pill for a moment. The bar's page only knows its own
/// window, and the peek's page gets no pointer moves off itself.
fn watch_pointer(app: &AppHandle, n: u64) {
    let current = move |app: &AppHandle| app.try_state::<Peek>().and_then(|s| s.inner.lock().ok().map(|s| s.watch == n && s.visible && !s.engaged)) == Some(true);
    panel::poll(app, WATCH_EVERY, current, move |app| look(app, n));
}

fn look(app: &AppHandle, n: u64) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    let (Some(p), Some(frame)) = (panel::pointer(), panel::frame(&window)) else { return };
    let gone = {
        let Ok(mut s) = state.inner.lock() else { return };
        // Dragged by its header, it may trail the pointer, which hasn't left it.
        if s.watch != n || !s.visible || s.engaged || s.drag.is_some() {
            return;
        }
        if over(p, frame, s.pill) {
            s.away_since = None;
            false
        } else {
            let since = *s.away_since.get_or_insert_with(Instant::now);
            since.elapsed() >= LEAVE_AFTER
        }
    };
    if gone {
        hide(app);
    }
}

/// The peek lost the keyboard, and Codealong's window events come after the change is made, so
/// what has it now can be asked. A click somewhere else puts the peek away, as does another of
/// Codealong's windows taking the keyboard. But the keyboard can go without you going anywhere,
/// to an app that brings a window of its own forward, so while the pointer is still on the peek or
/// its pill the peek stays up as a hover has it: a click in it takes the keyboard back, and it
/// closes once the pointer has left. On the main thread.
fn lost_keys(app: &AppHandle) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    let n = {
        let Ok(mut s) = state.inner.lock() else { return };
        // Hiding it takes the keyboard too, and a peek up from a hover never had it.
        if !s.visible || !s.engaged {
            return;
        }
        let on = s.drag.is_some() || panel::pointer().zip(panel::frame(&window)).is_some_and(|(p, f)| over(p, f, s.pill));
        if !on || panel::other_is_key(&window) {
            drop(s);
            hide(app);
            return;
        }
        s.engaged = false;
        s.away_since = None;
        s.watch += 1;
        s.watch
    };
    // Ordered in again as a hover shows it, saying it's key, so the page gets the pointer again.
    panel::present(&window, false);
    watch_pointer(app, n);
    tell(app);
}

/// Window events for the peek: it has the keyboard once it's the key window, lets go of it when
/// another window or app takes it (`lost_keys`), and closing it (⌘W) only hides it. On the main
/// thread.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != LABEL {
        return;
    }
    let app = window.app_handle();
    match event {
        WindowEvent::Focused(true) => {
            let changed = app.try_state::<Peek>().and_then(|s| {
                let mut s = s.inner.lock().ok()?;
                let changed = s.visible && !s.engaged;
                if s.visible {
                    s.engaged = true;
                    s.watch += 1;
                }
                Some(changed)
            });
            if changed == Some(true) {
                tell(app);
            }
        }
        WindowEvent::Focused(false) => lost_keys(app),
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(app);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { x: 0.0, y: 0.0, w: 1500.0, h: 875.0 };

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn hangs_under_the_bar_centered_on_the_pill() {
        let bar = r(500.0, 800.0, 600.0, 56.0);
        let pill = r(600.0, 817.0, 120.0, 22.0);
        let p = place(bar, pill, SCREEN);
        assert!(p.below);
        // The window starts where the bar's card ends, so it never covers a pill.
        assert_eq!(p.edge, 800.0 + BAR_MARGIN);
        assert_eq!(p.x, 660.0 - WIDTH / 2.0);
        assert_eq!(p.room, MAX_HEIGHT);
        // It grows down from its top edge.
        let f = p.frame(300.0);
        assert_eq!((f.top(), f.h, f.w), (p.edge, 300.0, WIDTH));
        // It's never taller than the room it has.
        assert_eq!(p.frame(5000.0).h, MAX_HEIGHT);
    }

    #[test]
    fn stays_on_the_screen_by_a_pill_near_its_edge() {
        let bar = r(0.0, 800.0, 400.0, 56.0);
        assert_eq!(place(bar, r(20.0, 817.0, 80.0, 22.0), SCREEN).x, 0.0);
        let bar = r(1100.0, 800.0, 400.0, 56.0);
        assert_eq!(place(bar, r(1420.0, 817.0, 80.0, 22.0), SCREEN).x, 1500.0 - WIDTH);
    }

    #[test]
    fn opens_over_a_bar_that_sits_low() {
        let bar = r(500.0, 40.0, 600.0, 56.0);
        let p = place(bar, r(600.0, 57.0, 120.0, 22.0), SCREEN);
        assert!(!p.below);
        assert_eq!(p.edge, 96.0 - BAR_MARGIN);
        // It grows up from its bottom edge.
        assert_eq!(p.frame(300.0).y, p.edge);
        // A little room under it is still enough when there's less over it.
        let short = Rect { h: 500.0, ..SCREEN };
        assert!(place(r(500.0, 250.0, 600.0, 56.0), r(600.0, 267.0, 120.0, 22.0), short).below);
        assert!(!place(r(500.0, 200.0, 600.0, 56.0), r(600.0, 217.0, 120.0, 22.0), short).below);
    }

    #[test]
    fn the_shadow_fits_the_window() {
        // A box shadow reaches past the card by its offset, spread and blur; the window keeps that
        // much room on every side, or the shadow ends in a hard edge. Under the bar the far side is
        // the bottom (CSS's y grows down), over it the top.
        let tokens: serde_json::Value = serde_json::from_str(include_str!("../../design/tokens.json")).unwrap();
        let shadows = tokens["shadow"]["tokens"].as_array().unwrap();
        for (name, top, bottom) in [("shadow-peek", NEAR, FAR), ("shadow-peek-over", FAR, NEAR)] {
            let token = shadows.iter().find(|t| t["name"] == name).unwrap();
            for scheme in ["light", "dark"] {
                let value = token["value"][scheme].as_str().unwrap();
                for layer in value.split("),") {
                    let n: Vec<f64> = layer.split("rgba").next().unwrap().split_whitespace().map(|v| v.trim_end_matches("px").parse().unwrap()).collect();
                    let (x, y, blur, spread) = (n[0], n[1], n.get(2).copied().unwrap_or(0.0), n.get(3).copied().unwrap_or(0.0));
                    let reach = |offset: f64| offset + spread + blur;
                    let what = format!("{name} ({scheme}): {layer}");
                    assert!(reach(x.abs()) <= MARGIN, "{what} reaches past the sides");
                    assert!(reach(-y) <= top, "{what} reaches past the top");
                    assert!(reach(y) <= bottom, "{what} reaches past the bottom");
                }
            }
        }
    }

    #[test]
    fn goes_where_its_header_is_dragged() {
        // The pointer went 30 right and 40 up the screen since the button went down.
        let from = r(400.0, 300.0, WIDTH, 500.0);
        assert_eq!(dragged(from, (600.0, 780.0), (630.0, 820.0)), r(430.0, 340.0, WIDTH, 500.0));
        // It keeps its size, and nothing holds it to the bar or the screen while it's dragged: a
        // kept session's window is put whole on a screen as it's made (`kept::on_a_screen`).
        assert_eq!(dragged(from, (600.0, 780.0), (-200.0, 780.0)), r(-400.0, 300.0, WIDTH, 500.0));
        assert_eq!(dragged(from, (600.0, 780.0), (600.0, 780.0)), from);
    }

    #[test]
    fn knows_when_the_pointer_left() {
        let peek = r(400.0, 300.0, WIDTH, 500.0);
        let pill = Some(r(600.0, 817.0, 120.0, 22.0));
        assert!(over((500.0, 500.0), peek, pill));
        assert!(over((650.0, 830.0), peek, pill));
        // A wobble just off the pill still counts.
        assert!(over((597.0, 830.0), peek, pill));
        assert!(!over((580.0, 830.0), peek, pill));
        assert!(!over((200.0, 500.0), peek, None));
    }
}

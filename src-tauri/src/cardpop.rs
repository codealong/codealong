//! A card pinned to the desktop opens the popover its Canvas card opens (card/CardPopover.tsx)
//! when the pointer rests on it, beside the card. The card's window is only as big as the card,
//! so the popover is a panel of its own (DesktopPopover.tsx draws it). The pointer can move onto
//! it, and it closes once the pointer has been off both for a moment, when the card's peek opens,
//! and when the card moves.
//!
//! Like the peek it's a non-activating panel, so it shows over full-screen apps and never takes
//! the keyboard. It's made when the first card is pinned, and never destroyed (bar.rs's
//! `make_panel`). Opening is the peek's handshake: the page gets the card (`card-popover`),
//! answers with its height, and only then does the panel show there.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::bar::{native, Rect};
use crate::desktop;

pub const LABEL: &str = "cardpop";
/// How long the pointer rests on a card before its popover opens (CanvasView.tsx's HOVER_MS).
const HOVER: Duration = Duration::from_millis(380);
/// How long the pointer may be off the card and the popover before the popover closes.
const LEAVE_AFTER: Duration = Duration::from_millis(300);
/// How often the pointer is looked at while the popover is up.
const WATCH_EVERY: Duration = Duration::from_millis(50);
/// How long an open waits for the page before showing anyway.
const READY_WAIT: Duration = Duration::from_millis(400);
/// Room around a card in its window for its shadow (DesktopPin.tsx's MARGIN).
const CARD_MARGIN: f64 = 12.0;
/// Room around the popover in its window for its shadow, and less on the side toward the card,
/// whose clicks a window's clear parts would take (DesktopPopover.tsx pads the page the same).
const MARGIN: f64 = 16.0;
const NEAR: f64 = 6.0;
/// Between the card's window and the popover's, so the card and the popover are 10 points apart
/// (the canvas's popover is 10 from its card).
const GAP: f64 = 4.0;
/// The popover's width (the canvas's) and its window's.
const WIDTH: f64 = 340.0 + MARGIN + NEAR;

/// Which side of its card the popover opens on.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Right,
    Left,
}

/// Where the popover's window goes beside a card's window, at a height, on a screen's free part:
/// to the right when it fits, else to the left, its card's top level with the card's, kept on
/// the screen.
pub fn place(card: Rect, height: f64, screen: Rect) -> (Rect, Side) {
    let right = card.right() - CARD_MARGIN + GAP;
    let left = card.x + CARD_MARGIN - GAP - WIDTH;
    let side = if right + WIDTH <= screen.right() || left < screen.x { Side::Right } else { Side::Left };
    let x = match side {
        Side::Right => right,
        Side::Left => left,
    };
    let h = height.min(screen.h).max(1.0);
    // Its card's top at the card's top: the window's top is the popover's margin above it.
    let top = (card.top() - CARD_MARGIN + MARGIN).min(screen.top());
    let y = (top - h).max(screen.y);
    (Rect { x: x.round(), y: y.round(), w: WIDTH, h }, side)
}

/// Whether the pointer is on the card (its window, less the room for its shadow) or the popover.
pub fn over(p: (f64, f64), card: Rect, popover: Rect) -> bool {
    let card = Rect { x: card.x + CARD_MARGIN, y: card.y + CARD_MARGIN, w: card.w - 2.0 * CARD_MARGIN, h: card.h - 2.0 * CARD_MARGIN };
    card.contains(p) || popover.contains(p)
}

/// What an open tells the page.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Opening {
    id: u64,
    feed: desktop::Feed,
    side: Side,
    /// The tallest the window can be there.
    room: f64,
}

#[derive(Default)]
struct CardPop {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// The window was asked for, and made into a panel it's safe to show.
    asked: bool,
    ready: bool,
    /// Counts hovers, so one that ended, or was followed by another, doesn't open.
    hovers: u64,
    /// Counts opens, so an old one's answer is told apart and its watch stops.
    opens: u64,
    /// The card's window it's for, and that window's frame when it opened.
    card: Option<String>,
    card_frame: Option<Rect>,
    side: Option<Side>,
    screen: Option<Rect>,
    /// The open the page hasn't answered yet.
    waiting: Option<u64>,
    visible: bool,
    /// Since when the pointer has been off the card and the popover.
    away_since: Option<Instant>,
}

pub fn setup(app: &AppHandle) {
    app.manage(CardPop::default());
}

/// Makes the popover's window, hidden, the first time a card is on the desktop, so a hover never
/// waits for a page to load.
pub fn ensure(app: &AppHandle) {
    let Some(state) = app.try_state::<CardPop>() else { return };
    {
        let Ok(mut s) = state.inner.lock() else { return };
        if s.asked {
            return;
        }
        s.asked = true;
    }
    let made = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Codealong")
        .initialization_script("window.__CODEALONG_CARDPOP__ = true;")
        .inner_size(WIDTH, 240.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .accept_first_mouse(true)
        // Up over other apps, where WebKit would slow its timers and drawing.
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
        .visible(false)
        .build();
    if let Err(e) = made {
        eprintln!("Couldn't make the popover for desktop cards: {e:#}");
        return;
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let (Some(state), Some(w)) = (handle.try_state::<CardPop>(), handle.get_webview_window(LABEL)) else { return };
        // A panel before it's first shown, as the cards are, so it goes onto full-screen Spaces
        // with them. It's never destroyed, so the class it had isn't kept.
        let _ = native::make_panel(&w);
        native::prepare(&w);
        if let Ok(mut s) = state.inner.lock() {
            s.ready = true;
        };
    });
}

/// The pointer came onto a card (`on`) or left it. Resting there opens the card's popover; leaving
/// before it opened stops it opening, and once it's up the pointer watch closes it.
pub fn hover(app: &AppHandle, label: &str, on: bool) {
    let Some(state) = app.try_state::<CardPop>() else { return };
    let n = {
        let Ok(mut s) = state.inner.lock() else { return };
        s.hovers += 1;
        // Leaving stops one about to open; one that's up is closed by its watch. Up for this card
        // already, it stays; another card's hover takes it over once the pointer has rested there.
        if !on || (s.visible && s.card.as_deref() == Some(label)) {
            return;
        }
        s.hovers
    };
    let (handle, label) = (app.clone(), label.to_string());
    std::thread::spawn(move || {
        std::thread::sleep(HOVER);
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || open(&app, n, &label));
    });
}

/// Opens the popover for the card in the window `label`, if the hover `n` is still the latest and
/// the pointer is still on the card. On the main thread.
fn open(app: &AppHandle, n: u64, label: &str) {
    let Some(state) = app.try_state::<CardPop>() else { return };
    if state.inner.lock().map_or(true, |s| s.hovers != n || !s.ready) || crate::peek::visible(app) {
        return;
    }
    let Some(card) = app.get_webview_window(label).and_then(|w| native::frame(&w)) else { return };
    if !native::pointer().is_some_and(|p| over(p, card, Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 })) {
        return;
    }
    let Some(feed) = app.try_state::<crate::engine::Handle>().and_then(|e| e.read(|b| desktop::feed_for(app, label, b))) else { return };
    let screens = native::screens();
    let Some(screen) = screens.iter().find(|s| s.frame.contains(card.center())).or(screens.first()).map(|s| s.visible) else { return };
    let (_, side) = place(card, 0.0, screen);
    let id = {
        let Ok(mut s) = state.inner.lock() else { return };
        s.opens += 1;
        s.card = Some(label.to_string());
        s.card_frame = Some(card);
        s.side = Some(side);
        s.screen = Some(screen);
        s.waiting = Some(s.opens);
        s.away_since = None;
        s.opens
    };
    let _ = app.emit_to(LABEL, "card-popover", Opening { id, feed, side, room: screen.h });
    // A page that doesn't answer doesn't keep the popover from showing.
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(READY_WAIT);
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || ready(&app, id, None));
    });
}

/// The page has laid out the open `id` at this height: show the popover there. A late answer
/// resizes a popover already up. On the main thread.
pub fn ready(app: &AppHandle, id: u64, height: Option<f64>) {
    let (Some(state), Some(window)) = (app.try_state::<CardPop>(), app.get_webview_window(LABEL)) else { return };
    let frame = {
        let Ok(mut s) = state.inner.lock() else { return };
        let (Some(card), Some(screen)) = (s.card_frame, s.screen) else { return };
        if s.waiting != Some(id) {
            // The page answered after the popover showed without it: it takes the height now.
            if s.visible && s.opens == id {
                if let Some(h) = height {
                    drop(s);
                    native::set_frame(&window, place(card, h.ceil(), screen).0);
                }
            }
            return;
        }
        let height = height.or_else(|| native::frame(&window).map(|f| f.h)).unwrap_or(240.0);
        let frame = place(card, height.ceil(), screen).0;
        // The pointer moved on while the page laid it out: nothing to show it for, and what
        // was up for another card goes too.
        if !native::pointer().is_some_and(|p| over(p, card, frame)) {
            drop(s);
            return hide(app);
        }
        s.waiting = None;
        s.visible = true;
        frame
    };
    native::set_frame(&window, frame);
    native::show(&window);
    watch(app, id);
}

/// The page changed size while up. On the main thread.
pub fn fit(app: &AppHandle, height: f64) {
    let (Some(state), Some(window)) = (app.try_state::<CardPop>(), app.get_webview_window(LABEL)) else { return };
    let Some((card, screen)) = state.inner.lock().ok().filter(|s| s.visible).and_then(|s| s.card_frame.zip(s.screen)) else { return };
    let frame = place(card, height.ceil(), screen).0;
    if native::frame(&window) != Some(frame) {
        native::set_frame(&window, frame);
    }
}

/// The card the popover is for, while it's up.
pub fn card(app: &AppHandle) -> Option<String> {
    app.try_state::<CardPop>().and_then(|s| s.inner.lock().ok().filter(|s| s.visible).and_then(|s| s.card.clone()))
}

/// Hides the popover. On the main thread.
pub fn hide(app: &AppHandle) {
    let (Some(state), Some(window)) = (app.try_state::<CardPop>(), app.get_webview_window(LABEL)) else { return };
    let was = {
        let Ok(mut s) = state.inner.lock() else { return };
        let was = s.visible || s.waiting.is_some();
        s.hovers += 1;
        s.opens += 1;
        s.visible = false;
        s.waiting = None;
        s.away_since = None;
        was
    };
    if was {
        native::hide(&window);
        let _ = app.emit_to(LABEL, "card-popover-hidden", ());
    }
}

/// While the popover is up, looks where the pointer is and closes the popover once it has been
/// off both the card and the popover for a moment, or at once when the card moved or went away.
/// The card's page only knows its own window, and the popover's gets no pointer moves off itself.
fn watch(app: &AppHandle, n: u64) {
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(WATCH_EVERY);
        let up = handle.try_state::<CardPop>().and_then(|s| s.inner.lock().ok().map(|s| s.opens == n && s.visible));
        if up != Some(true) {
            break;
        }
        let app = handle.clone();
        if handle.run_on_main_thread(move || look(&app, n)).is_err() {
            break;
        }
    });
}

fn look(app: &AppHandle, n: u64) {
    let (Some(state), Some(window)) = (app.try_state::<CardPop>(), app.get_webview_window(LABEL)) else { return };
    let gone = {
        let Ok(mut s) = state.inner.lock() else { return };
        if s.opens != n || !s.visible {
            return;
        }
        let card = s.card.as_deref().and_then(|l| app.get_webview_window(l)).and_then(|w| native::frame(&w));
        match (card, native::frame(&window), native::pointer()) {
            (Some(c), Some(pop), Some(p)) if Some(c) == s.card_frame => {
                if over(p, c, pop) {
                    s.away_since = None;
                    false
                } else {
                    s.away_since.get_or_insert_with(Instant::now).elapsed() >= LEAVE_AFTER
                }
            }
            // The card moved, went away, or there's no pointer to go by.
            _ => true,
        }
    };
    if gone {
        hide(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { x: 0.0, y: 0.0, w: 1500.0, h: 875.0 };

    #[test]
    fn it_opens_right_of_its_card_when_that_fits_else_left() {
        // A card in the middle: right of it, 10 points from the card, their tops level.
        let card = Rect { x: 600.0, y: 500.0, w: 304.0, h: 80.0 };
        let (f, side) = place(card, 200.0, SCREEN);
        assert_eq!(side, Side::Right);
        assert_eq!(f.x + NEAR - (card.right() - CARD_MARGIN), 10.0);
        assert_eq!(f.top() - MARGIN, card.top() - CARD_MARGIN);
        assert_eq!(f.w, WIDTH);
        // In the top right corner, where new cards go: left of it.
        let corner = Rect { x: 1190.0, y: 780.0, w: 304.0, h: 80.0 };
        let (f, side) = place(corner, 200.0, SCREEN);
        assert_eq!(side, Side::Left);
        assert_eq!((corner.x + CARD_MARGIN) - (f.right() - NEAR), 10.0);
        assert!(f.top() <= SCREEN.top());
    }

    #[test]
    fn it_stays_on_the_screen() {
        // Low on the screen, it rises to fit; taller than the screen, it's cut to it.
        let low = Rect { x: 100.0, y: 0.0, w: 304.0, h: 80.0 };
        let (f, _) = place(low, 400.0, SCREEN);
        assert_eq!(f.y, 0.0);
        assert_eq!(f.h, 400.0);
        let (f, _) = place(low, 5000.0, SCREEN);
        assert_eq!(f.h, SCREEN.h);
    }

    #[test]
    fn the_pointer_counts_on_the_card_or_the_popover_but_not_the_cards_shadow() {
        let card = Rect { x: 100.0, y: 100.0, w: 304.0, h: 80.0 };
        let pop = Rect { x: 500.0, y: 0.0, w: WIDTH, h: 300.0 };
        assert!(over((200.0, 140.0), card, pop));
        assert!(over((600.0, 140.0), card, pop));
        assert!(!over((105.0, 140.0), card, pop));
        assert!(!over((450.0, 140.0), card, pop));
    }
}

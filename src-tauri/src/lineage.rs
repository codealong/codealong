//! Lines between cards pinned to the desktop whose sessions came one of the other (a fork, or
//! started by the other's agent: `Session::origin`), drawn while the pointer is on one of them.
//! Each card is a window of its own, so the lines are drawn in one more: a clear panel over the
//! screen, just under the cards, that lets every click through (DesktopLines.tsx draws them).
//! It covers the whole screen so a card dragged about only redraws the lines, never resizes the
//! window, which WebKit shows a frame late. It's made the first time two such cards are on the
//! desktop and never destroyed, as the bar isn't (panel.rs's `make`).

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::config::Theme;
use crate::desktop;
use crate::geometry::{screen_at, PageRect, Rect, Screen, CARD_MARGIN};
use crate::model::Board;
use crate::panel;

pub const LABEL: &str = "lineage";
/// How far a line can bow out past the cards it joins, with its arrowhead (lineage.ts's
/// `connector`): where there's less room than this on the left, it bows out on the right.
const BOW: f64 = 100.0;
/// How often the lines follow cards being dragged, and check the pointer is still on its card.
const EVERY: Duration = Duration::from_millis(40);
/// How long the emptied page gets to draw nothing before its window goes: WebKit doesn't draw a
/// window that's off the screen, so it would show the old lines for a moment when it came back.
const EMPTIED: Duration = Duration::from_millis(150);
/// The window's panel is made as a card's is (desktop.rs), and then put just under the cards.
const PANEL: panel::Config = panel::Config { keys: panel::Keys::WhenAsked, level: panel::Level::Card, spaces: panel::Spaces::Stationary, unconstrained: true, tooltips: true };

/// A line as the page gets it, its cards in the window's points, from its top left, y down.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Line {
    /// The card of the session the other came from.
    pub from: PageRect,
    /// The card of the session that came of it.
    pub to: PageRect,
    /// Where the line bows out when the cards are one over the other.
    pub side: &'static str,
}

/// What the page draws.
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct Feed {
    pub theme: Theme,
    pub lines: Vec<Line>,
}

#[derive(Default)]
struct Lineage {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// Sessions that came of another, from the board: (the one it came from, it).
    pairs: Vec<(String, String)>,
    theme: Theme,
    /// Counts hovers, so one that ended stops following.
    n: u64,
    /// The window was asked for, and made into a panel it's safe to show.
    asked: bool,
    ready: bool,
    /// What the page was last sent, and where the window was put.
    shown: Feed,
    frame: Option<Rect>,
}

pub fn setup(app: &AppHandle) {
    app.manage(Lineage::default());
}

/// Keeps what came of what from each board the engine publishes, and makes the window once two
/// cards on the desktop are tied that way.
pub fn publish(app: &AppHandle, board: &Board) {
    let Some(st) = app.try_state::<Lineage>() else { return };
    let pairs: Vec<(String, String)> = board
        .lanes
        .iter()
        .flat_map(|l| l.sessions.iter())
        .filter(|s| s.visitor.is_none())
        .filter_map(|s| s.origin.as_ref().map(|o| (o.key.clone(), s.key.clone())))
        .collect();
    let cards: HashSet<String> = desktop::session_cards(app).into_iter().map(|(_, k)| k).collect();
    let tied = pairs.iter().any(|(a, b)| cards.contains(a) && cards.contains(b));
    let make = {
        let Ok(mut s) = st.state.lock() else { return };
        s.pairs = pairs;
        s.theme = board.theme;
        let make = tied && !s.asked;
        s.asked |= make;
        make
    };
    if make {
        // Making a window waits on the main thread, which may be waiting on the engine.
        let app = app.clone();
        std::thread::spawn(move || {
            if let Err(e) = make_window(&app) {
                eprintln!("Couldn't make the window for lines between cards: {e:#}");
            }
        });
    }
}

fn make_window(app: &AppHandle) -> Result<()> {
    // Every click goes through it, so there's no first one to count.
    let page = panel::Page { title: "Codealong", script: "window.__CODEALONG_LINEAGE__ = true;", size: (100.0, 100.0), focusable: false, first_mouse: false };
    panel::builder(app, LABEL, page).build()?;
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let (Some(st), Some(w)) = (handle.try_state::<Lineage>(), handle.get_webview_window(LABEL)) else { return };
        // A panel before it's first shown, as the cards are, so it goes onto full-screen Spaces
        // with them. It's never destroyed, so the class it had isn't kept.
        let _ = panel::make(&w, PANEL);
        panel::set_level(&w, panel::Level::Lines);
        panel::set_clickable(&w, false);
        panel::no_animation(&w);
        if let Ok(mut s) = st.state.lock() {
            s.ready = true;
        };
    })?;
    Ok(())
}

/// The pointer came onto a card (`on`), or left it. While it's on a session's card, the lines
/// from it to the cards of the sessions it came from or that came of it follow the cards.
pub fn hover(app: &AppHandle, label: &str, on: bool) {
    let Some(st) = app.try_state::<Lineage>() else { return };
    let cards = desktop::session_cards(app);
    let n = {
        let Ok(mut s) = st.state.lock() else { return };
        s.n += 1;
        // Only a card tied to another on the desktop has lines to follow.
        let key = cards.iter().find(|(l, _)| l == label).map(|(_, k)| k);
        let pinned = |k: &String| cards.iter().any(|(_, x)| x == k);
        if on && !s.pairs.iter().any(|(a, b)| (Some(a) == key && pinned(b)) || (Some(b) == key && pinned(a))) {
            return;
        }
        s.n
    };
    if !on {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || clear(&handle));
        return;
    }
    let (handle, label) = (app.clone(), label.to_string());
    let _ = std::thread::Builder::new().name("codealong-lineage".into()).spawn(move || loop {
        let current = handle.try_state::<Lineage>().and_then(|st| st.state.lock().ok().map(|s| s.n));
        if current != Some(n) {
            return;
        }
        let (h, l) = (handle.clone(), label.clone());
        let _ = handle.run_on_main_thread(move || follow(&h, n, &l));
        std::thread::sleep(EVERY);
    });
}

/// Draws the lines for the card the pointer is on, where the cards are now. On the main thread.
fn follow(app: &AppHandle, n: u64, label: &str) {
    let Some(st) = app.try_state::<Lineage>() else { return };
    let (pairs, theme) = {
        let Ok(s) = st.state.lock() else { return };
        if s.n != n {
            return;
        }
        (s.pairs.clone(), s.theme)
    };
    let cards = desktop::session_cards(app);
    let frame_of = |l: &str| app.get_webview_window(l).and_then(|w| panel::frame(&w));
    let Some((key, on)) = cards.iter().find(|(l, _)| l == label).and_then(|(l, k)| Some((k, frame_of(l)?))) else {
        return stop(app, n);
    };
    // The pointer went off the card without the page saying so (a card that went away under it).
    if !panel::pointer().is_some_and(|p| on.contains(p)) {
        return stop(app, n);
    }
    let card_of = |k: &str| cards.iter().find(|(_, x)| x == k).and_then(|(l, _)| frame_of(l));
    let mut ties = Vec::new();
    for (from, to) in &pairs {
        if to == key {
            ties.extend(card_of(from).map(|f| (f, on)));
        }
        if from == key {
            ties.extend(card_of(to).map(|t| (on, t)));
        }
    }
    if ties.is_empty() {
        return clear(app);
    }
    let Some(screen) = screen_at(on.center(), &panel::screens()) else { return clear(app) };
    let (frame, lines) = layout(&ties, screen);
    if lines.is_empty() {
        return clear(app);
    }
    show(app, frame, Feed { theme, lines });
}

/// Ends a hover that's still the latest, and takes its lines away.
fn stop(app: &AppHandle, n: u64) {
    if let Some(st) = app.try_state::<Lineage>() {
        if let Ok(mut s) = st.state.lock() {
            if s.n == n {
                s.n += 1;
            }
        }
    }
    clear(app);
}

/// Sends the page what it draws and puts its window up. On the main thread.
fn show(app: &AppHandle, frame: Rect, feed: Feed) {
    let (Some(st), Some(w)) = (app.try_state::<Lineage>(), app.get_webview_window(LABEL)) else { return };
    let (send, place) = {
        let Ok(mut s) = st.state.lock() else { return };
        if !s.ready {
            return;
        }
        let send = s.shown != feed;
        let place = s.frame != Some(frame);
        s.shown = feed.clone();
        s.frame = Some(frame);
        (send, place)
    };
    if send {
        let _ = app.emit_to(LABEL, "lineage-feed", &feed);
    }
    if place {
        panel::set_frame(&w, frame);
    }
    if !panel::on_screen(&w) {
        panel::show(&w);
    }
}

/// Takes the lines away. On the main thread.
fn clear(app: &AppHandle) {
    let (Some(st), Some(w)) = (app.try_state::<Lineage>(), app.get_webview_window(LABEL)) else { return };
    let empty = {
        let Ok(mut s) = st.state.lock() else { return };
        if s.shown.lines.is_empty() {
            return;
        }
        s.shown.lines.clear();
        s.shown.clone()
    };
    let _ = app.emit_to(LABEL, "lineage-feed", &empty);
    let handle = app.clone();
    drop(w);
    std::thread::spawn(move || {
        std::thread::sleep(EMPTIED);
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            let (Some(st), Some(w)) = (app.try_state::<Lineage>(), app.get_webview_window(LABEL)) else { return };
            // Unless lines came back meanwhile.
            if st.state.lock().is_ok_and(|s| s.shown.lines.is_empty()) {
                panel::hide(&w);
            }
        });
    });
}

/// What the page shows now, for when it loads after the first lines were sent.
pub fn current(app: &AppHandle) -> Feed {
    app.try_state::<Lineage>().and_then(|st| st.state.lock().ok().map(|s| s.shown.clone())).unwrap_or_default()
}

fn inset(r: Rect, by: f64) -> Rect {
    Rect { x: r.x + by, y: r.y + by, w: (r.w - 2.0 * by).max(0.0), h: (r.h - 2.0 * by).max(0.0) }
}

/// The window for lines between pairs of card windows (the one it came from first), and the
/// lines in it: over the screen the hovered card is on, and only between cards on that screen. A
/// window doesn't reach across displays (each has its own Spaces), so a card on another one gets
/// no line. A line bows out on the right when the screen's edge is too close on the left, so it
/// stays on the screen.
pub fn layout(ties: &[(Rect, Rect)], screen: Screen) -> (Rect, Vec<Line>) {
    let frame = screen.frame;
    let here = |r: &Rect| frame.contains(r.center());
    let page = |r: Rect| PageRect::from_screen(r, frame);
    let lines = ties
        .iter()
        .filter(|(a, b)| here(a) && here(b))
        .map(|(a, b)| (inset(*a, CARD_MARGIN), inset(*b, CARD_MARGIN)))
        .map(|(a, b)| Line { from: page(a), to: page(b), side: if a.x.min(b.x) - BOW < screen.visible.x { "right" } else { "left" } })
        .collect();
    (frame, lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    const SCREEN: Screen = Screen { frame: Rect { x: 0.0, y: 0.0, w: 1500.0, h: 900.0 }, visible: Rect { x: 0.0, y: 0.0, w: 1500.0, h: 875.0 } };

    #[test]
    fn lines_go_between_the_cards_inside_their_windows_from_the_top_left() {
        // Two cards in the top right corner, one under the other.
        let upper = r(1200.0, 780.0, 304.0, 80.0);
        let lower = r(1200.0, 700.0, 304.0, 80.0);
        let (frame, lines) = layout(&[(upper, lower)], SCREEN);
        assert_eq!(frame, SCREEN.frame);
        assert_eq!(lines, vec![Line { from: PageRect { x: 1212.0, y: 900.0 - 848.0, w: 280.0, h: 56.0 }, to: PageRect { x: 1212.0, y: 900.0 - 768.0, w: 280.0, h: 56.0 }, side: "left" }]);
    }

    #[test]
    fn a_line_at_the_left_edge_bows_out_on_the_right() {
        let (_, lines) = layout(&[(r(0.0, 500.0, 304.0, 80.0), r(0.0, 300.0, 304.0, 80.0))], SCREEN);
        assert_eq!(lines[0].side, "right");
    }

    #[test]
    fn a_card_on_another_screen_gets_no_line() {
        let other = r(1600.0, 400.0, 304.0, 80.0);
        let near = r(1100.0, 400.0, 304.0, 80.0);
        let (frame, lines) = layout(&[(near, other), (near, r(700.0, 400.0, 304.0, 80.0))], SCREEN);
        assert_eq!(frame, SCREEN.frame);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].to.x, 712.0);
    }

    #[test]
    fn a_screen_beside_the_main_one_counts_from_its_own_corner() {
        // A display up and to the right of the main one, as AppKit places it.
        let screen = Screen { frame: r(918.0, 1117.0, 1920.0, 1080.0), visible: r(918.0, 1117.0, 1920.0, 1080.0) };
        let (frame, lines) = layout(&[(r(1662.0, 1487.0, 304.0, 81.0), r(1972.0, 1480.0, 304.0, 81.0))], screen);
        assert_eq!(frame, screen.frame);
        assert_eq!(lines[0].from, PageRect { x: 1674.0 - 918.0, y: 2197.0 - 1556.0, w: 280.0, h: 57.0 });
        assert_eq!(lines[0].to.x, 1984.0 - 918.0);
    }
}

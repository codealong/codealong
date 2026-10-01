//! Desktop pins: a session, worktree or todo pinned from its right-click menu to a card of its
//! own that floats over every other window, on every Space and over full-screen apps, as the
//! floating bar does (DesktopPin.tsx draws it). You drag a card where you want it, a click opens
//! what it shows, and its own right-click menu takes it off the desktop.
//!
//! A thing can also be dragged onto the desktop: a session off the bar's pills (floating or in the
//! notch), a row out of a list in the main window, a result out of the quick panel. Its card is
//! pinned as the drag leaves its window and follows the pointer until the button comes up
//! (`drag_out`); let go back over that window, the drag pinned nothing.
//!
//! Each card is its own window, made the way the bar is (bar.rs): a non-activating panel, so a
//! click in one leaves the app you're in active and never takes the keyboard. It gets what it
//! shows cut from the board, and only when that changed, as the bar gets its feed. What's pinned
//! and where each card was left are kept in `desktop.json` next to the config.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent};

use crate::bar::{clamp, native, screen_for, Rect, EDGE, TOP_GAP};
use crate::config::{support_dir, Theme};
use crate::model::{Board, Lane};
use crate::todos::{Label, Todo};

/// Cards' windows are `desktop-1`, `desktop-2`… in the order they were made since launch.
pub const PREFIX: &str = "desktop-";
/// A card's window before its page has measured it: the card and room for its shadow.
const START: (f64, f64) = (304.0, 80.0);
/// How often cards are put back up if macOS left them off the Space in front, and where they
/// were moved to is written down.
const CHECK_EVERY: Duration = Duration::from_secs(1);
/// Where the pointer holds a card it drags off the bar, from the window's top left: on the card's
/// first line, a little in from its left edge.
const GRAB: (f64, f64) = (36.0, 26.0);
/// How often a dragged card follows the pointer.
const DRAG_EVERY: Duration = Duration::from_millis(16);

/// What a card shows.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Session,
    Worktree,
    Todo,
}

/// A thing pinned to the desktop: a session's key, a worktree's lane id, or a todo's id.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Pinned {
    #[serde(rename = "ref")]
    pub kind: Kind,
    pub key: String,
}

/// A card as `desktop.json` keeps it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Pin {
    #[serde(flatten)]
    what: Pinned,
    /// Its title when last seen, for when the board no longer has it.
    #[serde(default)]
    title: Option<String>,
    /// Where it was left.
    #[serde(default)]
    frame: Option<Rect>,
}

#[derive(Default, Serialize, Deserialize, Debug, PartialEq)]
struct Saved {
    #[serde(default)]
    pins: Vec<Pin>,
}

impl Saved {
    fn load() -> Saved {
        std::fs::read_to_string(support_dir().join("desktop.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn save(&self) {
        if std::fs::create_dir_all(support_dir()).is_ok() {
            if let Ok(text) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(support_dir().join("desktop.json"), text);
            }
        }
    }
}

#[derive(Default)]
struct Desktop {
    inner: Mutex<Inner>,
    drag: Mutex<Drag>,
}

/// A thing being dragged out of a window onto the desktop, while the mouse button is down.
#[derive(Default)]
struct Drag {
    /// Counts drags, so an old one's watch stops.
    n: u64,
    what: Option<Pinned>,
    /// The window it's dragged out of: the bar, the quick panel, a main window.
    home: String,
    /// The card's window once it's there, and whether the drag made it.
    card: Option<(String, bool)>,
    /// The drag is making its card: a thing not pinned before gets one when the pointer first
    /// leaves the window.
    making: bool,
    /// Where a card pinned before was, for a drag that ends back over its window.
    before: Option<Rect>,
    /// The card is put away while the pointer is back over the window the drag came from.
    parked: bool,
    /// The pointer is off the window the drag came from, as that window was last told.
    away: bool,
    /// Where the pointer was last.
    last: Option<(f64, f64)>,
    /// The button came up before the card was made: over its window (true) or off it.
    ended: Option<bool>,
}

/// What the window a drag came from hears when the button comes up (`desktop-drag-end`).
#[derive(Serialize, Clone, Copy, Debug)]
struct DragEnd {
    /// The card stayed on the desktop; false for a drag let go back over its window.
    kept: bool,
}

#[derive(Default)]
struct Inner {
    /// Each card's window and what it shows, in the order they were pinned.
    cards: Vec<(String, Pin)>,
    /// The feed last sent to each card's window, as sent, so an unchanged one isn't sent again.
    sent: HashMap<String, String>,
    /// The windows made into panels and put on screen, with the class each had before, which it
    /// gets back before it's destroyed. Until then, nothing may show one: a window once shown as
    /// a window stays off full-screen Spaces.
    placed: HashMap<String, Option<native::Original>>,
    /// Numbers the windows.
    made: u64,
    /// A card moved, or what one shows got a new title, since `desktop.json` was written.
    dirty: bool,
}

impl Inner {
    fn save(&mut self) {
        Saved { pins: self.cards.iter().map(|(_, p)| p.clone()).collect() }.save();
        self.dirty = false;
    }

    fn label_of(&self, what: &Pinned) -> Option<String> {
        self.cards.iter().find(|(_, p)| &p.what == what).map(|(l, _)| l.clone())
    }
}

/// What a card's page gets: the thing it shows and the lanes it's in, as the board has them.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Feed {
    pub pin: Pinned,
    /// Its title now, or when last seen once the board no longer has it.
    pub title: Option<String>,
    /// True until the engine's first full scan has finished: a card not found yet may still be.
    pub loading: bool,
    /// The board's time, for settling changes the windows show ahead of the engine (ahead.ts).
    pub generated_at: i64,
    pub theme: Theme,
    /// The terminal app's name, for menu items such as "Show in Ghostty".
    pub terminal: String,
    /// Your editor and the others installed, as the board has them, for the Open in menus.
    pub editor: String,
    pub editor_id: String,
    pub editors: Vec<crate::config::Profile>,
    /// The worktree's own lane; the session's; the todo's linked worktrees and its sessions'.
    /// A worktree keeps all its sessions, the others only the ones the card shows.
    pub lanes: Vec<Lane>,
    /// The todo, for a todo's card.
    pub todos: Vec<Todo>,
    /// The todo's labels.
    pub labels: Vec<Label>,
}

/// The lane a session is listed under as its own, cut down to that session.
fn session_lane(board: &Board, key: &str) -> Option<Lane> {
    let lane = board.lanes.iter().find(|l| l.sessions.iter().any(|s| s.key == key && s.visitor.is_none()))?;
    let mut lane = lane.clone();
    lane.sessions.retain(|s| s.key == key && s.visitor.is_none());
    Some(lane)
}

/// What a card shows, cut from the board.
pub fn feed(board: &Board, pin: &Pinned, last_title: Option<&str>) -> Feed {
    let mut lanes: Vec<Lane> = Vec::new();
    let mut todos = Vec::new();
    let mut labels = Vec::new();
    let title = match pin.kind {
        Kind::Session => session_lane(board, &pin.key).map(|l| {
            let title = l.sessions[0].title.clone();
            lanes.push(l);
            title
        }),
        Kind::Worktree => board.lanes.iter().find(|l| l.id == pin.key).map(|l| {
            lanes.push(l.clone());
            l.callsign.clone()
        }),
        Kind::Todo => board.todos.iter().find(|t| t.id == pin.key).map(|t| {
            for lane in &board.lanes {
                let linked = t.worktrees.contains(&lane.id);
                if !linked && !lane.sessions.iter().any(|s| s.visitor.is_none() && t.sessions.contains(&s.key)) {
                    continue;
                }
                let mut lane = lane.clone();
                lane.sessions.retain(|s| s.visitor.is_none() && t.sessions.contains(&s.key));
                lanes.push(lane);
            }
            labels = board.labels.iter().filter(|l| t.labels.contains(&l.id)).cloned().collect();
            todos.push(t.clone());
            t.title.clone()
        }),
    };
    Feed {
        pin: pin.clone(),
        title: title.or_else(|| last_title.map(str::to_string)),
        loading: board.loading,
        generated_at: board.generated_at,
        theme: board.theme,
        terminal: board.apps.terminal.clone(),
        editor: board.apps.editor.clone(),
        editor_id: board.apps.editor_id.clone(),
        editors: board.apps.editors.clone(),
        lanes,
        todos,
        labels,
    }
}

/// Whether two rectangles overlap.
fn overlap(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.top() && b.y < a.top()
}

/// Where a new card goes: in the top right corner of the screen, under the cards already there,
/// else at the top of the next column to the left. With no room anywhere, in the corner.
pub fn new_frame(w: f64, h: f64, screen: Rect, others: &[Rect]) -> Rect {
    let mut right = screen.right() - EDGE;
    while right - w >= screen.x + EDGE {
        let mut top = screen.top() - TOP_GAP;
        loop {
            let r = Rect { x: right - w, y: top - h, w, h };
            if r.y < screen.y {
                break;
            }
            match others.iter().filter(|o| overlap(**o, r)).map(|o| o.y).reduce(f64::min) {
                Some(lowest) => top = lowest,
                None => return r,
            }
        }
        right -= w;
    }
    clamp(Rect { x: screen.right() - EDGE - w, y: screen.top() - TOP_GAP - h, w, h }, screen)
}

/// Makes a window for each card pinned before, and starts the loop that keeps them up.
pub fn setup(app: &AppHandle) -> Result<()> {
    app.manage(Desktop::default());
    let saved = Saved::load();
    for pin in saved.pins {
        let label = add(app, pin);
        if let Err(e) = make_window(app, &label) {
            eprintln!("Couldn't put a card back on the desktop: {e:#}");
        }
    }
    let handle = app.clone();
    std::thread::Builder::new().name("codealong-desktop".into()).spawn(move || loop {
        std::thread::sleep(CHECK_EVERY);
        let app = handle.clone();
        if handle.run_on_main_thread(move || keep_up(&app)).is_err() {
            break;
        }
    })?;
    Ok(())
}

/// Takes a card into the list and names its window.
fn add(app: &AppHandle, pin: Pin) -> String {
    let Some(state) = app.try_state::<Desktop>() else { return String::new() };
    let Ok(mut inner) = state.inner.lock() else { return String::new() };
    inner.made += 1;
    let label = format!("{PREFIX}{}", inner.made);
    inner.cards.push((label.clone(), pin));
    label
}

/// Makes a card's window, hidden, then on the main thread turns it into a panel and shows it
/// where it was left, or in a free spot on the screen the pointer is on.
fn make_window(app: &AppHandle, label: &str) -> Result<()> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title("Codealong")
        .initialization_script("window.__CODEALONG_DESKTOP__ = true;")
        .inner_size(START.0, START.1)
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
        .build()?;
    let (handle, label) = (app.clone(), label.to_string());
    app.run_on_main_thread(move || place_and_show(&handle, &label))?;
    Ok(())
}

/// On the main thread.
fn place_and_show(app: &AppHandle, label: &str) {
    let (Some(state), Some(window)) = (app.try_state::<Desktop>(), app.get_webview_window(label)) else { return };
    // A panel before it's first shown: one shown as a window stays off full-screen Spaces.
    let original = native::make_panel(&window);
    native::prepare(&window);
    let screens = native::screens();
    // The page may have sized the window already.
    let (w, h) = native::frame(&window).map_or(START, |f| (f.w, f.h));
    let frame = {
        let Ok(mut inner) = state.inner.lock() else { return };
        let others: Vec<Rect> = inner.cards.iter().filter(|(l, _)| l != label).filter_map(|(_, p)| p.frame).collect();
        let Some((_, pin)) = inner.cards.iter_mut().find(|(l, _)| l == label) else { return };
        // Where it was left, by its top left corner, if that screen is still there.
        let left = pin.frame.and_then(|r| screens.iter().find(|s| s.frame.contains(r.center())).map(|s| clamp(Rect { x: r.x, y: r.top() - h, w, h }, s.visible)));
        let frame = match left {
            Some(r) => r,
            None => {
                let at = native::pointer().and_then(|p| screens.iter().find(|s| s.frame.contains(p))).or(screens.first()).map(|s| s.visible);
                let Some(screen) = at else { return };
                new_frame(w, h, screen, &others)
            }
        };
        pin.frame = Some(frame);
        inner.placed.insert(label.to_string(), original);
        inner.save();
        frame
    };
    native::set_frame(&window, frame);
    native::show(&window);
}

/// Writes down where a card's window is now. On the main thread.
fn remember_frame(app: &AppHandle, label: &str) {
    let (Some(state), Some(window)) = (app.try_state::<Desktop>(), app.get_webview_window(label)) else { return };
    let Some(frame) = native::frame(&window) else { return };
    if let Ok(mut inner) = state.inner.lock() {
        if let Some((_, pin)) = inner.cards.iter_mut().find(|(l, _)| l == label) {
            if pin.frame != Some(frame) {
                pin.frame = Some(frame);
                inner.dirty = true;
            }
        }
    };
}

/// Pins a thing to the desktop, or brings its card up if it's there already. `at` is where a new
/// card goes (a session dragged off the bar); without it, a free spot. Returns the card's window.
pub fn pin(app: &AppHandle, what: Pinned, at: Option<Rect>) -> Result<String> {
    let state = app.try_state::<Desktop>().ok_or_else(|| anyhow!("Desktop pins aren't ready yet."))?;
    let already = state.inner.lock().map_err(|_| anyhow!("Desktop pins are unavailable."))?.label_of(&what);
    if let Some(label) = already {
        let (handle, shown) = (app.clone(), label.clone());
        app.run_on_main_thread(move || {
            let placed = handle.try_state::<Desktop>().is_some_and(|s| s.inner.lock().is_ok_and(|i| i.placed.contains_key(&shown)));
            if let (true, Some(w)) = (placed, handle.get_webview_window(&shown)) {
                native::show(&w);
            }
        })?;
        return Ok(label);
    }
    let label = add(app, Pin { what, title: None, frame: at });
    if let Err(e) = make_window(app, &label) {
        if let Ok(mut inner) = state.inner.lock() {
            inner.cards.retain(|(l, _)| l != &label);
        }
        return Err(e);
    }
    tell(app);
    Ok(label)
}

/// A card's window frame held by the pointer at `p`, at the window's size.
pub fn held_at(p: (f64, f64), (w, h): (f64, f64)) -> Rect {
    Rect { x: p.0 - GRAB.0, y: p.1 + GRAB.1 - h, w, h }
}

/// Starts dragging a thing out of the window `home` onto the desktop: the pointer is watched
/// from here (`follow_drag`) until the button comes up, since a page can't place a window under
/// it, hears little of the pointer once it leaves, and the docked bar lets clicks through. The
/// window's page starts it once a pill or a row has moved with the button down. On the main
/// thread.
pub fn drag_out(app: &AppHandle, what: Pinned, home: &str) {
    let Some(state) = app.try_state::<Desktop>() else { return };
    let Some(p) = native::pointer() else { return };
    let existing = state.inner.lock().ok().and_then(|i| i.cards.iter().find(|(_, pin)| pin.what == what).map(|(l, pin)| (l.clone(), pin.frame)));
    let n = {
        let Ok(mut d) = state.drag.lock() else { return };
        let n = d.n + 1;
        let (card, before) = existing.map_or((None, None), |(l, f)| (Some((l, false)), f));
        *d = Drag { n, what: Some(what), home: home.to_string(), card, making: false, before, parked: false, away: false, last: Some(p), ended: None };
        n
    };
    // Off the bar the card is there at once.
    follow_drag(app, n);
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(DRAG_EVERY);
        let going = handle.try_state::<Desktop>().is_some_and(|s| s.drag.lock().is_ok_and(|d| d.n == n && d.what.is_some() && d.ended.is_none()));
        if !going {
            break;
        }
        let app = handle.clone();
        if handle.run_on_main_thread(move || follow_drag(&app, n)).is_err() {
            break;
        }
    });
}

/// Pins the dragged thing, its card held by the pointer at `p` (off the main thread, which
/// making a window waits on), and hands the card to the drag (`made_for_drag`).
fn make_for_drag(app: &AppHandle, n: u64, what: Pinned, p: (f64, f64)) {
    let handle = app.clone();
    std::thread::spawn(move || {
        let made = pin(&handle, what, Some(held_at(p, START)));
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || made_for_drag(&app, n, made.ok()));
    });
}

/// Whether the pointer at `p` is back over the window a drag came from: on the bar's card or
/// docked shape, else anywhere in the window's frame. On the main thread.
fn at_home(app: &AppHandle, home: &str, p: (f64, f64)) -> bool {
    if home == crate::bar::LABEL {
        return crate::bar::covers(app, p);
    }
    app.get_webview_window(home).is_some_and(|w| w.is_visible().unwrap_or(false) && native::frame(&w).is_some_and(|f| f.contains(p)))
}

/// What a dragged card does while the button is down, by where the pointer is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Held {
    /// It hangs from the pointer.
    Follow,
    /// It's put away: the pointer is back over the window the drag came from, whose page shows
    /// where a drop there would go.
    Park,
}

/// A card dragged off the bar stays under the pointer over the bar too, where the pill it came
/// from is; out of any other window it's put away over that window, which it would cover.
pub fn held(home_is_bar: bool, at_home: bool) -> Held {
    if at_home && !home_is_bar {
        Held::Park
    } else {
        Held::Follow
    }
}

/// The card a drag pinned is there: it follows the pointer from now on, or if the button came up
/// meanwhile, is dropped where the pointer last was. On the main thread.
fn made_for_drag(app: &AppHandle, n: u64, label: Option<String>) {
    let Some(state) = app.try_state::<Desktop>() else { return };
    let (what, last, home) = {
        let Ok(mut d) = state.drag.lock() else { return };
        if d.n != n {
            return;
        }
        let Some(label) = label.clone() else {
            d.what = None;
            return;
        };
        let Some(home) = d.ended else {
            d.card = Some((label, true));
            // Put where the pointer is now, or away, without waiting for the next look.
            d.parked = false;
            drop(d);
            follow_drag(app, n);
            return;
        };
        (d.what.take(), d.last, home)
    };
    let Some(label) = label else { return };
    if let Some(p) = last {
        move_card(app, &label, p);
    }
    drop_card(app, what, &label, true, None, home);
}

/// What a look at the pointer has a drag do.
enum Step {
    Nothing,
    /// The thing isn't pinned yet and the pointer has left its window: pin it.
    Make(Pinned),
    Move(String),
    Park(String, bool, Option<Rect>),
    Drop(String, bool, Option<Pinned>, Option<Rect>),
}

/// Looks at the pointer for a drag: while the button is down the card hangs from it, made when
/// the pointer first leaves the window the drag came from, and put away while it's back over
/// that window; once the button is up the card is left where it is, unless that's over that
/// window. On the main thread.
fn follow_drag(app: &AppHandle, n: u64) {
    let Some(state) = app.try_state::<Desktop>() else { return };
    let Some(p) = native::pointer() else { return };
    let down = native::button_down();
    let Some(home) = state.drag.lock().ok().filter(|d| d.n == n).map(|d| d.home.clone()) else { return };
    let home_now = at_home(app, &home, p);
    let mut left = None;
    let step = {
        let Ok(mut d) = state.drag.lock() else { return };
        if d.n != n || d.what.is_none() || d.ended.is_some() {
            return;
        }
        d.last = Some(p);
        if down && d.away == home_now {
            d.away = !home_now;
            left = Some(d.away);
        }
        let park = held(home == crate::bar::LABEL, home_now) == Held::Park;
        // Let go off its window before a look saw it leave, it's pinned where it was let go.
        let make = if down { !park } else { !home_now };
        if !down {
            d.ended = Some(home_now);
        }
        match (down, d.card.clone()) {
            (true, Some((label, made))) => {
                let turned = park != d.parked;
                d.parked = park;
                match (park, turned) {
                    (false, _) => Step::Move(label),
                    (true, true) => Step::Park(label, made, d.before),
                    (true, false) => Step::Nothing,
                }
            }
            (false, Some((label, made))) => {
                d.card = None;
                Step::Drop(label, made, d.what.take(), d.before.take())
            }
            // A card still being made is placed or dropped by its maker (`made_for_drag`).
            (_, None) if make && !d.making => {
                d.making = true;
                d.what.clone().map_or(Step::Nothing, Step::Make)
            }
            (_, None) => Step::Nothing,
        }
    };
    match step {
        Step::Nothing => {}
        Step::Make(what) => make_for_drag(app, n, what, p),
        Step::Move(label) => {
            show_card(app, &label);
            move_card(app, &label, p);
        }
        Step::Park(label, made, before) => park_card(app, &label, made, before),
        Step::Drop(label, made, what, before) => drop_card(app, what, &label, made, before, home_now),
    }
    // The window's page shows where a drop on it would land only while the pointer is over it,
    // and it may hear nothing of the pointer once it has left.
    if let Some(away) = left {
        let _ = app.emit_to(home.as_str(), "desktop-drag-away", away);
    }
    if !down {
        let _ = app.emit_to(home.as_str(), "desktop-drag-end", DragEnd { kept: !home_now });
    }
}

/// Puts a dragged card away while the pointer is back over the window it came from: a card the
/// drag made can't be seen, and one pinned before goes back where it was. On the main thread.
fn park_card(app: &AppHandle, label: &str, made: bool, before: Option<Rect>) {
    let Some(window) = app.get_webview_window(label) else { return };
    if made {
        native::set_alpha(&window, 0.0);
    } else if let Some(before) = before {
        native::set_frame(&window, before);
        remember_frame(app, label);
    }
}

/// A card put away can be seen again. On the main thread.
fn show_card(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        native::set_alpha(&window, 1.0);
    }
}

/// Moves a card so the pointer at `p` holds it. On the main thread.
fn move_card(app: &AppHandle, label: &str, p: (f64, f64)) {
    let (Some(state), Some(window)) = (app.try_state::<Desktop>(), app.get_webview_window(label)) else { return };
    let size = native::frame(&window).map_or(START, |f| (f.w, f.h));
    let frame = held_at(p, size);
    let placed = {
        let Ok(mut inner) = state.inner.lock() else { return };
        if let Some((_, pin)) = inner.cards.iter_mut().find(|(l, _)| l == label) {
            pin.frame = Some(frame);
        }
        inner.dirty = true;
        inner.placed.contains_key(label)
    };
    // Until it's placed, the frame it's placed at is the one just written down.
    if placed {
        native::set_frame(&window, frame);
    }
}

/// Where a drag leaves its card: back over the window it came from (`home`), a card it made goes,
/// and one pinned before goes back where it was; anywhere else it stays, kept on its screen. On
/// the main thread.
fn drop_card(app: &AppHandle, what: Option<Pinned>, label: &str, made: bool, before: Option<Rect>, home: bool) {
    if home {
        match (made, what, before) {
            (true, Some(what), _) => unpin(app, &what),
            (false, _, Some(before)) => {
                if let Some(w) = app.get_webview_window(label) {
                    native::set_frame(&w, before);
                }
                remember_frame(app, label);
            }
            _ => {}
        }
        return;
    }
    let Some(window) = app.get_webview_window(label) else { return };
    native::set_alpha(&window, 1.0);
    let Some(frame) = native::frame(&window) else { return };
    let Some(screen) = screen_for(frame, &native::screens()) else { return };
    let kept = clamp(frame, screen);
    if kept != frame {
        native::set_frame(&window, kept);
    }
    remember_frame(app, label);
}

/// Takes a card off the desktop: its window goes back to being a window, then away.
pub fn unpin(app: &AppHandle, what: &Pinned) {
    let Some(state) = app.try_state::<Desktop>() else { return };
    let (label, original) = {
        let Ok(mut inner) = state.inner.lock() else { return };
        let Some(label) = inner.label_of(what) else { return };
        inner.cards.retain(|(l, _)| l != &label);
        inner.sent.remove(&label);
        let original = inner.placed.remove(&label).flatten();
        inner.save();
        (label, original)
    };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(w) = handle.get_webview_window(&label) else { return };
        if let Some(original) = original {
            native::unmake_panel(&w, original);
        }
        let _ = w.destroy();
    });
    tell(app);
}

/// What's pinned, in the order it was pinned.
pub fn pinned(app: &AppHandle) -> Vec<Pinned> {
    app.try_state::<Desktop>().and_then(|s| s.inner.lock().ok().map(|i| i.cards.iter().map(|(_, p)| p.what.clone()).collect())).unwrap_or_default()
}

/// The cards' windows.
pub fn labels(app: &AppHandle) -> Vec<String> {
    app.try_state::<Desktop>().and_then(|s| s.inner.lock().ok().map(|i| i.cards.iter().map(|(l, _)| l.clone()).collect())).unwrap_or_default()
}

/// The cards that show a session: each one's window and the session's key.
pub fn session_cards(app: &AppHandle) -> Vec<(String, String)> {
    let Some(state) = app.try_state::<Desktop>() else { return Vec::new() };
    let Ok(inner) = state.inner.lock() else { return Vec::new() };
    inner.cards.iter().filter(|(l, p)| p.what.kind == Kind::Session && inner.placed.contains_key(l)).map(|(l, p)| (l.clone(), p.what.key.clone())).collect()
}

/// Tells every window what's pinned, for their menus.
fn tell(app: &AppHandle) {
    let _ = app.emit("desktop-pins", pinned(app));
}

/// The feed for the card in this window, for its first render; after that it gets
/// `desktop-feed` events.
pub fn feed_for(app: &AppHandle, label: &str, board: &Board) -> Option<Feed> {
    let state = app.try_state::<Desktop>()?;
    let inner = state.inner.lock().ok()?;
    let (_, pin) = inner.cards.iter().find(|(l, _)| l == label)?;
    Some(feed(board, &pin.what, pin.title.as_deref()))
}

/// Sends each card what it shows after the engine publishes, if that changed. The board's time
/// changes with every publish and alone doesn't send a feed.
pub fn publish(app: &AppHandle, board: &Board) {
    let Some(state) = app.try_state::<Desktop>() else { return };
    let mut out = Vec::new();
    {
        let Ok(mut inner) = state.inner.lock() else { return };
        let Inner { cards, sent, dirty, .. } = &mut *inner;
        for (label, pin) in cards.iter_mut() {
            let mut feed = feed(board, &pin.what, pin.title.as_deref());
            if feed.title.is_some() && feed.title != pin.title {
                pin.title = feed.title.clone();
                *dirty = true;
            }
            let at = std::mem::take(&mut feed.generated_at);
            let Ok(text) = serde_json::to_string(&feed) else { continue };
            feed.generated_at = at;
            if sent.get(label.as_str()) == Some(&text) {
                continue;
            }
            sent.insert(label.clone(), text);
            out.push((label.clone(), feed));
        }
    }
    for (label, feed) in out {
        let _ = app.emit_to(label.as_str(), "desktop-feed", feed);
    }
}

/// Sizes a card's window to its page, which measures itself whenever what it shows changes. It
/// keeps its top edge and the side of the screen it's nearest, as the bar does.
pub fn fit(app: &AppHandle, label: &str, width: f64, height: f64) {
    let (handle, label) = (app.clone(), label.to_string());
    let _ = app.run_on_main_thread(move || {
        let Some(window) = handle.get_webview_window(&label) else { return };
        let Some(current) = native::frame(&window) else { return };
        let Some(screen) = screen_for(current, &native::screens()) else { return };
        let next = crate::bar::place(current, width.ceil(), height.ceil(), screen);
        if next != current {
            native::set_frame(&window, next);
            // Keeping its top edge doesn't move the window as Tauri sees it, so no Moved comes;
            // the next card placed needs its real height.
            remember_frame(&handle, &label);
        }
    });
}

/// Puts back up a card macOS left off the Space in front, unless another card covers it (you put
/// one on the other: showing it each time would swap them every second), and writes down where
/// cards were moved to. On the main thread.
fn keep_up(app: &AppHandle) {
    let Some(state) = app.try_state::<Desktop>() else { return };
    let cards: Vec<(String, Option<Rect>)> = {
        let Ok(mut inner) = state.inner.lock() else { return };
        if inner.dirty {
            inner.save();
        }
        inner.cards.iter().filter(|(l, _)| inner.placed.contains_key(l)).map(|(l, p)| (l.clone(), p.frame)).collect()
    };
    for (label, frame) in &cards {
        let Some(window) = app.get_webview_window(label) else { continue };
        let covered = frame.is_some_and(|f| cards.iter().any(|(l, o)| l != label && o.is_some_and(|o| overlap(f, o))));
        if !covered && !native::on_screen(&window) {
            native::show(&window);
        }
    }
}

/// A moved card is remembered. On the main thread.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::Moved(_) = event {
        if window.label().starts_with(PREFIX) {
            remember_frame(window.app_handle(), window.label());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { x: 0.0, y: 0.0, w: 1500.0, h: 900.0 };

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn new_cards_stack_down_the_top_right_corner() {
        let first = new_frame(300.0, 80.0, SCREEN, &[]);
        assert_eq!(first, r(1192.0, 814.0, 300.0, 80.0));
        let second = new_frame(300.0, 100.0, SCREEN, &[first]);
        assert_eq!(second, r(1192.0, 714.0, 300.0, 100.0));
        // A gap a card was dragged out of is filled first.
        let third = new_frame(300.0, 80.0, SCREEN, &[second]);
        assert_eq!(third, r(1192.0, 814.0, 300.0, 80.0));
        // A card moved elsewhere on the screen doesn't count.
        assert_eq!(new_frame(300.0, 80.0, SCREEN, &[r(100.0, 814.0, 300.0, 80.0)]), first);
    }

    #[test]
    fn a_full_column_starts_the_next_one_to_the_left() {
        let tall = r(1192.0, 0.0, 300.0, 894.0);
        assert_eq!(new_frame(300.0, 80.0, SCREEN, &[tall]), r(892.0, 814.0, 300.0, 80.0));
        // No room anywhere: the corner, over the others.
        let wall = r(0.0, 0.0, 1500.0, 900.0);
        assert_eq!(new_frame(300.0, 80.0, SCREEN, &[wall]), r(1192.0, 814.0, 300.0, 80.0));
    }

    #[test]
    fn a_dragged_card_hangs_from_the_pointer_by_its_first_line() {
        // The pointer is 36 in from the window's left and 26 down from its top (y grows up).
        assert_eq!(held_at((500.0, 400.0), (304.0, 80.0)), r(464.0, 346.0, 304.0, 80.0));
        let f = held_at((500.0, 400.0), (304.0, 180.0));
        assert_eq!((f.x, f.top()), (464.0, 426.0));
    }

    #[test]
    fn a_dragged_card_is_put_away_over_the_window_it_came_from_but_not_over_the_bar() {
        // Out of a list or the quick panel: under the pointer outside, away over the window.
        assert_eq!(held(false, false), Held::Follow);
        assert_eq!(held(false, true), Held::Park);
        // Off the bar it stays under the pointer, over the bar too.
        assert_eq!(held(true, false), Held::Follow);
        assert_eq!(held(true, true), Held::Follow);
    }

    #[test]
    fn saved_pins_read_old_and_partial_files() {
        let s: Saved = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Saved::default());
        let s: Saved = serde_json::from_str(r#"{"pins":[{"ref":"session","key":"claude:claude:1"},{"ref":"todo","key":"t1","title":"Fix it","frame":{"x":1,"y":2,"w":3,"h":4}}]}"#).unwrap();
        assert_eq!(s.pins[0].what, Pinned { kind: Kind::Session, key: "claude:claude:1".into() });
        assert_eq!(s.pins[0].frame, None);
        assert_eq!(s.pins[1].title.as_deref(), Some("Fix it"));
        assert_eq!(s.pins[1].frame, Some(r(1.0, 2.0, 3.0, 4.0)));
        let text = serde_json::to_string(&s).unwrap();
        assert!(text.contains(r#""ref":"todo""#), "{text}");
    }

    fn session(key: &str, visitor: bool) -> crate::model::Session {
        serde_json::from_value(serde_json::json!({
            "key": key, "id": key, "shortId": key, "harness": "claude", "account": "claude", "surface": "cli",
            "status": "working", "live": true, "pid": null, "title": format!("Title {key}"), "cwd": "/r", "work": [],
            "name": null, "startedAt": null, "statusSince": 1, "lastActivity": 1, "lastPromptAt": 1, "excerpt": null,
            "resumeCommand": null, "appAccount": false, "openTarget": format!("s|{key}|focus"),
            "visitor": if visitor { serde_json::json!("elsewhere") } else { serde_json::Value::Null },
            "headline": "", "detail": null, "branch": null, "archived": false, "attachCommand": null, "stalledSince": null
        }))
        .expect("a session")
    }

    fn lane(id: &str, sessions: Vec<crate::model::Session>) -> Lane {
        let mut l: Lane = serde_json::from_value(serde_json::json!({
            "id": id, "project": "P", "repo": "r", "repoSlug": null, "path": id, "callsign": format!("cs-{id}"),
            "origin": "main", "branch": null, "detached": null, "bay": "idle", "glyph": "idle", "parked": false,
            "hidden": false, "headline": "", "detail": null, "sessions": [], "pr": null, "git": null,
            "lastActivity": 0, "targets": [], "defaultTarget": null
        }))
        .expect("a lane");
        l.sessions = sessions;
        l
    }

    fn todo(id: &str, worktrees: &[&str], sessions: &[&str], labels: &[&str]) -> Todo {
        serde_json::from_value(serde_json::json!({
            "id": id, "project": "P", "title": format!("Todo {id}"), "description": "", "status": "todo",
            "labels": labels, "worktrees": worktrees, "sessions": sessions, "createdAt": 0, "updatedAt": 0, "doneAt": null
        }))
        .expect("a todo")
    }

    fn board() -> Board {
        let mut b = Board::default();
        b.lanes = vec![
            lane("/a", vec![session("s1", false), session("s2", false), session("s3", true)]),
            lane("/b", vec![session("s3", false)]),
            lane("/c", vec![]),
        ];
        b.todos = vec![todo("t1", &["/c"], &["s2", "s3"], &["l1"])];
        b.labels = vec![
            serde_json::from_value(serde_json::json!({"id": "l1", "name": "Bug", "color": "red"})).unwrap(),
            serde_json::from_value(serde_json::json!({"id": "l2", "name": "Idea", "color": "blue"})).unwrap(),
        ];
        b
    }

    fn keys(l: &Lane) -> Vec<&str> {
        l.sessions.iter().map(|s| s.key.as_str()).collect()
    }

    #[test]
    fn a_session_comes_with_its_own_lane_cut_down_to_it() {
        let f = feed(&board(), &Pinned { kind: Kind::Session, key: "s3".into() }, None);
        assert_eq!(f.title.as_deref(), Some("Title s3"));
        assert_eq!(f.lanes.len(), 1);
        // Listed under /a as a visitor, its own lane is /b.
        assert_eq!(f.lanes[0].id, "/b");
        assert_eq!(keys(&f.lanes[0]), ["s3"]);
    }

    #[test]
    fn a_worktree_comes_with_all_its_sessions() {
        let f = feed(&board(), &Pinned { kind: Kind::Worktree, key: "/a".into() }, None);
        assert_eq!(f.title.as_deref(), Some("cs-/a"));
        assert_eq!(keys(&f.lanes[0]), ["s1", "s2", "s3"]);
    }

    #[test]
    fn a_todo_comes_with_its_worktrees_sessions_and_labels() {
        let f = feed(&board(), &Pinned { kind: Kind::Todo, key: "t1".into() }, None);
        assert_eq!(f.title.as_deref(), Some("Todo t1"));
        let lanes: Vec<(&str, Vec<&str>)> = f.lanes.iter().map(|l| (l.id.as_str(), keys(l))).collect();
        assert_eq!(lanes, [("/a", vec!["s2"]), ("/b", vec!["s3"]), ("/c", vec![])]);
        assert_eq!(f.todos.len(), 1);
        assert_eq!(f.labels.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(), ["l1"]);
    }

    #[test]
    fn a_thing_the_board_lost_keeps_its_last_title() {
        let f = feed(&board(), &Pinned { kind: Kind::Todo, key: "gone".into() }, Some("Old title"));
        assert_eq!(f.title.as_deref(), Some("Old title"));
        assert!(f.lanes.is_empty() && f.todos.is_empty());
    }
}

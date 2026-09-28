//! Desktop pins: a session, worktree or todo pinned from its right-click menu to a card of its
//! own that floats over every other window, on every Space and over full-screen apps, as the
//! floating bar does (DesktopPin.tsx draws it). You drag a card where you want it, a click opens
//! what it shows, and its own right-click menu takes it off the desktop.
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

/// Pins a thing to the desktop, or brings its card up if it's there already.
pub fn pin(app: &AppHandle, what: Pinned) -> Result<()> {
    let state = app.try_state::<Desktop>().ok_or_else(|| anyhow!("Desktop pins aren't ready yet."))?;
    let already = state.inner.lock().map_err(|_| anyhow!("Desktop pins are unavailable."))?.label_of(&what);
    if let Some(label) = already {
        let handle = app.clone();
        app.run_on_main_thread(move || {
            let placed = handle.try_state::<Desktop>().is_some_and(|s| s.inner.lock().is_ok_and(|i| i.placed.contains_key(&label)));
            if let (true, Some(w)) = (placed, handle.get_webview_window(&label)) {
                native::show(&w);
            }
        })?;
        return Ok(());
    }
    let label = add(app, Pin { what, title: None, frame: None });
    if let Err(e) = make_window(app, &label) {
        if let Ok(mut inner) = state.inner.lock() {
            inner.cards.retain(|(l, _)| l != &label);
        }
        return Err(e);
    }
    tell(app);
    Ok(())
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

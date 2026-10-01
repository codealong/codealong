//! Codealong: every agent runs in a worktree, and every worktree is a lane.

mod acp;
mod actions;
mod appserver;
mod asks;
mod attachments;
mod bar;
mod barfeed;
mod board;
mod canvas;
mod chrome;
mod chrometabs;
mod caret;
mod claude;
mod claudeapp;
mod cli;
mod antigravity;
mod codex;
mod opencode;
mod origins;
mod peek;
mod config;
mod detect;
mod desktop;
mod devicekey;
mod editors;
mod engine;
mod flash;
mod git;
mod harness;
mod held;
mod idle;
mod github;
mod install;
mod insights;
// Link codes and iroh, the messages between devices, and the model their feeds carry are in
// codealong-peer (peer/), which the phone app builds on too.
use codealong_peer::net as irohnet;
mod jev;
mod launch;
mod lineage;
mod logo;
mod model;
mod peerfeed;
use codealong_peer::proto as peerproto;
use codealong_peer::api;
mod peers;
mod perf;
mod prasks;
mod previews;
mod prlinks;
mod procs;
mod quick;
mod repos;
mod reusable;
mod send;
mod route;
mod runcode;
mod search;
mod side;
mod server;
mod start;
mod starts;
mod state;
mod tailnet;
mod throttle;
mod todojev;
mod todos;
mod transcript;
mod updates;
mod usage;
mod util;

use tauri::{Emitter, Manager, State};

/// `Kestrel` → `project-kestrel`: window labels allow letters, digits, `-`, `/`, `:` and `_`.
fn project_window_label(project: &str) -> String {
    let slug: String = project.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    format!("project-{}", slug.trim_matches('-'))
}

/// A label for one more window on a project: the project's own if no window has it, else
/// `project-kestrel:2`, `:3`… A slug has no `:`, so these never take another project's label.
fn another_window_label(project: &str, taken: impl Fn(&str) -> bool) -> String {
    let own = project_window_label(project);
    std::iter::once(own.clone()).chain((2..).map(|n| format!("{own}:{n}"))).find(|l| !taken(l)).unwrap_or(own)
}

/// Opens a project in a window of its own, or brings that window forward; with `another`
/// (File › New window), always opens one more. The window reads its project from
/// `window.__CODEALONG_PROJECT__`, set before the page loads.
#[tauri::command(async)]
fn open_project_window(app: tauri::AppHandle, window: tauri::WebviewWindow, project: String, another: Option<bool>) -> Result<(), String> {
    let label = if another.unwrap_or(false) {
        another_window_label(&project, |l| app.get_webview_window(l).is_some())
    } else {
        let label = project_window_label(&project);
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.unminimize();
            let _ = w.show();
            return w.set_focus().map_err(|e| e.to_string());
        }
        label
    };
    let script = format!("window.__CODEALONG_PROJECT__ = {};", serde_json::to_string(&project).map_err(|e| e.to_string())?);
    let mut builder = tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App("index.html".into()))
        .title(format!("Codealong · {project}"))
        .inner_size(1320.0, 860.0)
        .min_inner_size(980.0, 600.0)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(tauri::LogicalPosition::new(18.0, 20.0))
        .initialization_script(&script);
    // Cascade from the window you asked in so the new one opens on the same screen but doesn't
    // sit exactly on top of it. The main window can be closed, which only hides it.
    if let (Ok(pos), Ok(scale)) = (window.outer_position(), window.scale_factor()) {
        let at = pos.to_logical::<f64>(scale);
        let step = 28.0 * app.webview_windows().len() as f64;
        builder = builder.position(at.x + step, at.y + step);
    }
    builder.build().map(|_| ()).map_err(|e| format!("Couldn't open a window for {project}: {e}"))
}

#[tauri::command]
fn get_board(engine: State<'_, engine::Handle>) -> model::Board {
    engine.board()
}

#[tauri::command(async)]
fn open_target(app: tauri::AppHandle, engine: State<'_, engine::Handle>, lane_id: String, target_id: String) -> Result<(), String> {
    if peerfeed::split_remote(&lane_id).is_some() {
        return Err("That's on another device: it opens there, and its sessions show in their panel here.".into());
    }
    let from = flash::pointer();
    engine.open(&lane_id, &target_id).map_err(|e| e.to_string())?;
    flash_focused(&app, &engine, &lane_id, &target_id, from);
    Ok(())
}

/// After a session's terminal came forward, shows where when it's on another screen (flash.rs).
fn flash_focused(app: &tauri::AppHandle, engine: &engine::Handle, lane_id: &str, target_id: &str, from: Option<(f64, f64)>) {
    if !target_id.ends_with("|focus") {
        return;
    }
    if let Some(bundle) = target_app(engine, lane_id, target_id) {
        flash::after_focus(app, from, bundle);
    }
}

/// The app an open target lands in (`bar::target_app`): for a session's terminal, the app its
/// process runs under, whichever terminal config.toml names.
fn target_app(engine: &engine::Handle, lane_id: &str, target_id: &str) -> Option<&'static str> {
    let board = engine.board();
    let host = target_id.strip_prefix("s|").and_then(|t| t.rsplit_once('|')).and_then(|(key, _)| {
        let pid = board.lanes.iter().find(|l| l.id == lane_id)?.sessions.iter().find(|s| s.key == key)?.pid?;
        launch::terminal_kind(pid)?.bundle()
    });
    bar::target_app(target_id, &board.apps.terminal_id, host)
}

#[tauri::command(async)]
fn open_config(engine: State<'_, engine::Handle>) -> Result<(), String> {
    engine.open_config().map_err(|e| e.to_string())
}

/// The installed editors' app icons, for the Open in menus. Off the main thread: the first
/// drawing of each takes a moment, and AppKit draws into a bitmap on any thread.
#[tauri::command(async)]
fn editor_icons() -> Vec<editors::Icon> {
    editors::icons()
}

/// What "Close idle terminals" would close, sessions quiet for `after_ms` or longer (idle.rs).
#[tauri::command]
async fn idle_terminals(engine: State<'_, engine::Handle>, after_ms: i64) -> Result<idle::Plan, String> {
    Ok(idle::plan_now(&engine.board(), after_ms))
}

/// Closes the terminals of these sessions, each checked again first.
#[tauri::command]
async fn close_idle_terminals(engine: State<'_, engine::Handle>, keys: Vec<String>, after_ms: i64) -> Result<Vec<idle::Outcome>, String> {
    Ok(idle::close(&engine.board(), &keys, after_ms))
}

/// Closes one session from its menu: its terminal tab, or a background job stopped (idle.rs).
/// Returns what happened, in a sentence.
#[tauri::command(async)]
fn close_session(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<String, String> {
    actions::run(&engine, &peers, actions::CLOSE, &key, serde_json::json!({})).map(|v| v.as_str().unwrap_or_default().to_string())
}

/// Returns the time of the board that shows the sessions on this device seen (none when they're
/// all on other devices, whose feeds show it as they come in), for the window's own copy.
#[tauri::command(async)]
fn mark_seen(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, keys: Vec<String>) -> Option<i64> {
    let (elsewhere, here): (Vec<String>, Vec<String>) = keys.into_iter().partition(|k| peerfeed::split_remote(k).is_some());
    // Seen goes to the device the session is on: its status changes there, and so on every device.
    let mut by_device: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for k in &elsewhere {
        if let Some((device, key)) = peerfeed::split_remote(k) {
            by_device.entry(device.to_string()).or_default().push(key.to_string());
        }
    }
    for (device, keys) in by_device {
        let peers = peers.inner().clone();
        std::thread::spawn(move || {
            let _ = actions::ask(&peers, &device, api::MARK_SEEN, serde_json::json!({ "keys": keys }));
        });
    }
    if here.is_empty() {
        return None;
    }
    actions::run(&engine, &peers, api::MARK_SEEN, "", serde_json::json!({ "keys": here })).ok().and_then(|v| v.as_i64())
}

/// Snoozes sessions until `until` (ms since the epoch), or wakes them with none. Only this Mac's:
/// the floating bar, which snoozes keep sessions off, shows only this Mac's. Returns the time of
/// the board that shows it, as `mark_seen` does.
#[tauri::command(async)]
fn snooze_sessions(engine: State<'_, engine::Handle>, keys: Vec<String>, until: Option<i64>) -> Option<i64> {
    let here: Vec<String> = keys.into_iter().filter(|k| peerfeed::split_remote(k).is_none()).collect();
    if here.is_empty() {
        None
    } else {
        engine.snooze(here, until)
    }
}

#[tauri::command]
fn refresh(engine: State<'_, engine::Handle>) {
    engine.refresh_github();
}

#[tauri::command]
fn refresh_usage(engine: State<'_, engine::Handle>) {
    engine.refresh_usage();
}

#[tauri::command]
fn set_pinned(engine: State<'_, engine::Handle>, keys: Vec<String>) {
    engine.set_pinned(keys);
}

/// Sets a worktree's preview URL, and the Chrome profile it opens in over its project's, or
/// clears them with no URL. Returns the time of the board that shows it, when the engine says
/// in time.
#[tauri::command(async)]
fn set_preview(engine: State<'_, engine::Handle>, lane_id: String, url: Option<String>, port: Option<u16>, chrome_profile: Option<String>) -> Result<Option<i64>, String> {
    if peerfeed::split_remote(&lane_id).is_some() {
        return Err("That worktree is on another device; set its preview there.".into());
    }
    engine.set_preview(&lane_id, url.as_deref(), port, chrome_profile.as_deref()).map_err(|e| format!("{e:#}"))
}

/// Marks a worktree reusable or not (reusable.rs). Returns the time of the board that shows it,
/// when the engine says in time.
#[tauri::command(async)]
fn set_reusable(engine: State<'_, engine::Handle>, lane_id: String, on: bool) -> Result<Option<i64>, String> {
    if peerfeed::split_remote(&lane_id).is_some() {
        return Err("That worktree is on another device; mark it there.".into());
    }
    engine.set_reusable(&lane_id, on).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn set_pinned_worktrees(engine: State<'_, engine::Handle>, ids: Vec<String>) {
    engine.set_pinned_worktrees(ids);
}

/// A window's own timing line for the perf log (see perf.rs), while `[debug] perf_log` is on.
#[tauri::command(async)]
fn perf_mark(window: tauri::Window, line: String) {
    perf::log(&format!("ui[{}] {line}", window.label()));
}

/// The page saw a key typed into a text field while WebKit had it as unfocused, which hides the
/// text cursor (caret.rs).
#[tauri::command]
fn page_unfocused(window: tauri::WebviewWindow) {
    caret::refocus(&window);
}

/// A right-click menu is on its way up or open.
static MENU_UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Pops up a menu the page made with `Menu.new`, in place of Tauri's `plugin:menu|popup`, which
/// keeps the page's resource table locked until the menu closes. The menu runs its own loop on
/// the main thread, and the page's other menu commands (`Menu.new`, closing one) run there too
/// and take that lock: one that came in while a menu was opening, from a second right-click,
/// hung the app for good. A menu asked for while another is opening or open (a burst of
/// right-clicks) is dropped; once that one has closed, the next is shown.
#[tauri::command(async)]
fn popup_menu(app: tauri::AppHandle, webview: tauri::Webview, window: tauri::Window, rid: tauri::ResourceId, at: Option<tauri::Position>) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    use tauri::menu::ContextMenu;
    // The table is unlocked again at the end of this statement.
    let menu = webview.resources_table().get::<tauri::menu::Menu<tauri::Wry>>(rid).map_err(|e| e.to_string())?;
    if MENU_UP.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    // Lowers the flag once the menu has closed, or when it never gets to open.
    struct Up;
    impl Drop for Up {
        fn drop(&mut self) {
            MENU_UP.store(false, Ordering::Release);
        }
    }
    let up = Up;
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        // On the main thread Tauri shows the menu at once and returns when it has closed, so
        // the flag is down before the click that closed it reaches the page.
        let shown = match at {
            Some(at) => menu.popup_at(window, at),
            None => menu.popup(window),
        };
        drop(up);
        let _ = tx.send(shown);
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}

/// The floating bar's page measured itself.
#[tauri::command]
fn fit_bar(app: tauri::AppHandle, width: f64, height: f64) {
    bar::fit(&app, width, height);
}

/// Brings the main window forward; `reveal` is a session or PR for it to select.
#[tauri::command]
fn show_main(app: tauri::AppHandle, reveal: Option<serde_json::Value>) -> Result<(), String> {
    bar::show_main(&app, reveal).map_err(|e| e.to_string())
}

/// Opens a session from the floating bar, then asks macOS to bring its app forward: a click in the
/// bar doesn't make Codealong the active app.
#[tauri::command(async)]
fn open_from_bar(app: tauri::AppHandle, engine: State<'_, engine::Handle>, lane_id: String, target_id: String) -> Result<(), String> {
    if let Some(key) = target_id.strip_prefix("s|").and_then(|t| t.strip_suffix("|hosted")) {
        // Codealong runs it: its panel in the main window is where it lives.
        engine.mark_seen(vec![key.to_string()]);
        return bar::show_main(&app, Some(serde_json::json!({ "kind": "session", "laneId": lane_id, "key": key }))).map_err(|e| e.to_string());
    }
    let from = flash::pointer();
    engine.open(&lane_id, &target_id).map_err(|e| e.to_string())?;
    if let Some(bundle) = target_app(&engine, &lane_id, &target_id) {
        bar::bring_forward(&app, bundle);
    }
    flash_focused(&app, &engine, &lane_id, &target_id, from);
    Ok(())
}

/// Shows a session in the floating bar's peek: for a hover over its pill or a click on a card on
/// the desktop (where the window's page measured it), with `focus` ready to type into, or with
/// `rename` its name ready to change (peek.rs).
#[tauri::command]
fn peek_open(app: tauri::AppHandle, window: tauri::WebviewWindow, engine: State<'_, engine::Handle>, key: String, pill: Option<bar::Rect>, focus: Option<bool>, rename: Option<bool>) -> Result<(), String> {
    let target = engine.read(|b| peek::target(b, &key)).ok_or_else(|| "That session isn't on the board any more.".to_string())?;
    // The pill is in the window that asked: the bar, or a card on the desktop.
    let pill = pill.and_then(|p| bar::native::frame(&window).map(|host| (host, p)));
    let rename = rename.unwrap_or(false);
    peek::open(&app, target, pill, focus.unwrap_or(false) || rename, rename).map_err(|e| e.to_string())
}

/// A session with its worktree and the apps, as the board has them: for a window that doesn't
/// get the board (the floating bar) to build a session's menu on a right-click (menus.ts).
#[derive(serde::Serialize)]
struct SessionAt {
    lane: model::Lane,
    session: model::Session,
    apps: model::AppNames,
}

#[tauri::command(async)]
fn session_at(engine: State<'_, engine::Handle>, key: String) -> Option<SessionAt> {
    // From the worktree it runs in, not one it only visits.
    engine.read(|b| {
        let (lane, session) = b.session(&key)?;
        Some(SessionAt { lane: lane.clone(), session: session.clone(), apps: b.apps.clone() })
    })
}

/// The peek's page is laid out for the open `id`, at this height.
#[tauri::command]
fn peek_ready(app: tauri::AppHandle, id: u64, height: f64) {
    peek::ready(&app, id, Some(height));
}

/// The peek's page changed size while up.
#[tauri::command]
fn peek_fit(app: tauri::AppHandle, height: f64) {
    peek::fit(&app, height);
}

/// A click in the peek: it takes the keyboard.
#[tauri::command]
fn peek_focus(app: tauri::AppHandle) {
    peek::take_keys(&app);
}

#[tauri::command]
fn peek_hide(app: tauri::AppHandle) {
    peek::hide(&app);
}

/// What the floating bar shows, for its first render; after that it gets `bar-feed` events.
#[tauri::command]
fn bar_feed(engine: State<'_, engine::Handle>) -> barfeed::BarFeed {
    barfeed::feed(&engine.board())
}

/// What's pinned to the desktop, for the windows' menus; after that they get `desktop-pins` events.
#[tauri::command]
fn desktop_pins(app: tauri::AppHandle) -> Vec<desktop::Pinned> {
    desktop::pinned(&app)
}

/// Pins a session, worktree or todo to the desktop, or brings its card up (desktop.rs).
#[tauri::command(async)]
fn pin_to_desktop(app: tauri::AppHandle, pin: desktop::Pinned) -> Result<(), String> {
    desktop::pin(&app, pin, None).map(|_| ()).map_err(|e| format!("Couldn't pin it to the desktop: {e}"))
}

/// A thing is being dragged out of the window that asks (a pill off the bar, a row out of a list,
/// a result out of the quick panel): its card follows the pointer onto the desktop (desktop.rs
/// `drag_out`).
#[tauri::command]
fn drag_to_desktop(app: tauri::AppHandle, window: tauri::Window, pin: desktop::Pinned) {
    desktop::drag_out(&app, pin, window.label());
}

/// Takes a card off the desktop; from the card's own menu, that closes the window it came from.
#[tauri::command(async)]
fn unpin_from_desktop(app: tauri::AppHandle, pin: desktop::Pinned) {
    desktop::unpin(&app, &pin);
}

/// What a desktop card shows, for its first render; after that it gets `desktop-feed` events.
#[tauri::command]
fn desktop_feed(app: tauri::AppHandle, window: tauri::Window, engine: State<'_, engine::Handle>) -> Option<desktop::Feed> {
    engine.read(|b| desktop::feed_for(&app, window.label(), b))
}

/// A desktop card's page measured itself.
#[tauri::command]
fn fit_desktop(app: tauri::AppHandle, window: tauri::Window, width: f64, height: f64) {
    desktop::fit(&app, window.label(), width, height);
}

/// The pointer came onto a desktop card or left it: a session's card shows lines to the cards of
/// the sessions it came from or that came of it (lineage.rs).
#[tauri::command(async)]
fn desktop_hover(app: tauri::AppHandle, window: tauri::Window, on: bool) {
    lineage::hover(&app, window.label(), on);
}

/// What the window of lines between desktop cards shows, for its first render; after that it gets
/// `lineage-feed` events.
#[tauri::command]
fn lineage_feed(app: tauri::AppHandle) -> lineage::Feed {
    lineage::current(&app)
}

/// Opens the quick panel for what a window asks (a worktree, a todo, a prompt), or brings it
/// back after a failed start.
#[tauri::command]
fn show_quick(app: tauri::AppHandle, context: Option<serde_json::Value>) {
    quick::open(&app, context.unwrap_or(serde_json::Value::Null));
}

#[tauri::command]
fn hide_quick(app: tauri::AppHandle) {
    quick::hide(&app);
}

/// The quick panel's page is laid out for the open it was asked for, at this height.
#[tauri::command]
fn quick_ready(app: tauri::AppHandle, height: f64) {
    quick::ready(&app, Some(height));
}

/// The quick panel's page changed size.
#[tauri::command]
fn fit_quick(app: tauri::AppHandle, height: f64) {
    quick::fit(&app, height);
}

/// Starts a session from the quick panel, which has already hidden itself, then asks macOS to
/// bring the app it started in forward: the panel never makes Codealong the active app. A
/// worktree on another device starts there, where whatever it opens comes forward.
#[tauri::command(async)]
fn start_from_quick(app: tauri::AppHandle, engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, spec: start::StartSpec) -> Result<start::Started, String> {
    if let Some((device, lane)) = peerfeed::split_remote(&spec.lane_id) {
        return start_on_device(&peers, device, lane, &spec);
    }
    let started = start_and_link(&engine, &spec)?;
    note_start(&engine, &spec);
    if let Some(bundle) = quick::start_app(spec.place, &engine.board().apps.terminal_id) {
        bar::bring_forward(&app, bundle);
    }
    Ok(started)
}

/// Keeps whether a start went into a new worktree, where one was on offer, for Jev's next
/// suggestion in that repo (starts.rs).
fn note_start(engine: &engine::Handle, spec: &start::StartSpec) {
    let cfg = config::load().ok().flatten().unwrap_or_default();
    if !start::offers_worktree(&cfg, spec) {
        return;
    }
    let Some(repo) = engine.board().lanes.iter().find(|l| l.id == spec.lane_id).map(|l| format!("{}/{}", l.project, l.repo)) else {
        return;
    };
    if let Err(e) = starts::record(&repo, spec.worktree) {
        eprintln!("codealong: couldn't note the start in starts.json: {e:#}");
    }
}

/// Where what's typed in the quick panel should go, from Jev (route.rs).
#[tauri::command(async)]
fn route_quick(engine: State<'_, engine::Handle>, project: String, text: String) -> Result<route::Route, String> {
    route_here(&engine, &project, &text)
}

/// Where typed text goes on this Mac's board, for its quick panel or a linked phone's.
fn route_here(engine: &engine::Handle, project: &str, text: &str) -> Result<route::Route, String> {
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    if !cfg.jev.quick_panel {
        return Err("Suggestions in the quick panel are off.".into());
    }
    route::route(&engine.board(), &cfg, project, text).map_err(|e| format!("{e:#}"))
}

/// Sessions whose prompts and replies have every word (search.rs), for the quick panel.
#[tauri::command(async)]
fn search_text(words: Vec<String>) -> search::Found {
    let words: Vec<String> = words.iter().map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect();
    let t = std::time::Instant::now();
    let found = search::find(&words, 200);
    if perf::enabled() {
        perf::log(&format!("search: {} words, {} sessions in {} ms", words.len(), found.hits.len(), t.elapsed().as_millis()));
    }
    found
}

/// A page of a session's conversation, from its own record (transcript.rs): the newest when
/// `before` is none, else what came before it.
#[tauri::command(async)]
fn session_page(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, before: Option<u64>) -> Result<serde_json::Value, String> {
    actions::run(&engine, &peers, api::SESSION_PAGE, &key, serde_json::json!({ "before": before }))
}

/// What a session waiting on you wants to do, when it's a tool call Codealong can answer for it
/// (`harness::Adapter::pending`).
#[tauri::command(async)]
fn session_pending(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<serde_json::Value, String> {
    actions::run(&engine, &peers, api::SESSION_PENDING, &key, serde_json::json!({}))
}

/// How a message sent to a session would reach it, for its box's placeholder (send.rs `way_in`).
#[tauri::command(async)]
fn session_way(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<serde_json::Value, String> {
    actions::run(&engine, &peers, api::SESSION_WAY, &key, serde_json::json!({}))
}

/// Allows or denies the tool call `id` a session waits on, if it's still the one waiting; or,
/// with `answers`, answers the questions it asks (`Pending::questions`).
#[tauri::command(async)]
fn session_answer(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, id: String, answer: harness::Answer, answers: Option<serde_json::Value>) -> Result<String, String> {
    let said = actions::run(&engine, &peers, api::SESSION_ANSWER, &key, serde_json::json!({ "id": id, "answer": answer, "answers": answers }))?;
    Ok(said.as_str().unwrap_or_default().to_string())
}

/// What a session said and did after `after`, as it goes on.
#[tauri::command(async)]
fn session_newer(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, after: u64) -> Result<serde_json::Value, String> {
    actions::run(&engine, &peers, api::SESSION_NEWER, &key, serde_json::json!({ "after": after }))
}

/// The reply a session Codealong runs is writing, for a panel that opens mid-turn, known by the
/// key the window has for it.
#[tauri::command(async)]
fn session_stream(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Option<acp::Stream> {
    let got = actions::run(&engine, &peers, api::SESSION_STREAM, &key, serde_json::json!({})).ok()?;
    let mut s: acp::Stream = serde_json::from_value(got).ok()?;
    s.key = key;
    Some(s)
}

/// Stops the turn a session is taking, here or on another device, and says what happened.
#[tauri::command(async)]
fn session_cancel(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<String, String> {
    let said = actions::run(&engine, &peers, api::SESSION_CANCEL, &key, serde_json::json!({}))?;
    // Devices before stops reached every harness answered with nothing.
    Ok(said.as_str().unwrap_or("Stopped.").to_string())
}

/// Sends a prompt to a session that's already going, where it runs (send.rs). Only Codealong's
/// windows call this, so what it sends is what you typed.
#[tauri::command(async)]
fn send_to_session(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, text: String) -> Result<serde_json::Value, String> {
    actions::run(&engine, &peers, api::SESSION_SEND, &key, serde_json::json!({ "text": text }))
}

/// Sends to a session on this device; the socket (server.rs) does the same for `codealong session
/// send`.
pub(crate) fn send_here(engine: &engine::Handle, key: &str, text: &str, sender: send::Sender) -> Result<send::Sent, String> {
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    send::send(&engine.board(), &cfg, key, text, sender).map_err(|e| format!("{e:#}"))
}

/// What Jev suggests for a todo being written: its project, labels and the todos it waits on
/// (todojev.rs). Errors when `[jev] todos` is off or there's nothing to ask.
#[tauri::command(async)]
fn suggest_todo(engine: State<'_, engine::Handle>, id: Option<String>, title: String, description: String, project: String) -> Result<todojev::Suggested, String> {
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    if !cfg.jev.todos {
        return Err("Todo suggestions are off.".into());
    }
    let board = engine.board();
    let projects: Vec<String> = board.projects.iter().map(|p| p.name.clone()).filter(|p| p != board::ELSEWHERE).collect();
    let req = todojev::suggest_request(cfg.jev.model.trim(), id.as_deref(), &title, &description, &project, &projects, &board.labels, &board.todos, &board)
        .ok_or_else(|| "Nothing to suggest.".to_string())?;
    let response = jev::ask(&req.body).map_err(|e| format!("{e:#}"))?;
    Ok(todojev::suggest_read(&req, &response))
}

/// "Looks done" was wrong for this todo, until something linked to it moves on.
#[tauri::command]
fn dismiss_todo_hint(engine: State<'_, engine::Handle>, id: String) {
    engine.dismiss_todo_hint(id);
}

/// Whether there's a TypeSafe API key, for Settings. Reads no secret.
#[tauri::command(async)]
fn jev_key() -> bool {
    jev::has_key()
}

/// Saves the TypeSafe API key in the keychain, or removes it when it's empty.
#[tauri::command(async)]
fn set_jev_key(key: String) -> Result<(), String> {
    if key.trim().is_empty() { jev::delete_key() } else { jev::save_key(&key) }.map_err(|e| format!("{e:#}"))
}

/// One small call to TypeSafe, for Settings' Test button.
#[tauri::command(async)]
fn test_jev(model: String) -> Result<String, String> {
    jev::check(model.trim()).map_err(|e| format!("{e:#}"))
}

/// The quick panel's shortcut and why it isn't working, if it isn't.
#[tauri::command(async)]
fn quick_shortcut(app: tauri::AppHandle) -> quick::ShortcutStatus {
    quick::status(&app)
}

/// Turns the quick panel's shortcut off while Settings records a new one, and back on.
#[tauri::command(async)]
fn pause_quick_shortcut(app: tauri::AppHandle, paused: bool) -> quick::ShortcutStatus {
    quick::pause_shortcut(&app, paused)
}

/// Whether the floating bar is on.
#[tauri::command]
fn bar_on(app: tauri::AppHandle) -> bool {
    bar::is_on(&app)
}

/// Turns the floating bar on or off.
#[tauri::command]
fn set_bar_on(app: tauri::AppHandle, on: bool) {
    bar::set_on(&app, on);
}

/// Where the bar shows (floating, in the notch, or off), with the notch it docks into.
#[tauri::command]
fn bar_mode(app: tauri::AppHandle) -> bar::ModeView {
    bar::mode_view(&app)
}

/// The shape the docked bar shows now, in its window's points from the top left: the window takes
/// clicks only on it.
#[tauri::command]
fn notch_shape(app: tauri::AppHandle, x: f64, y: f64, w: f64, h: f64) {
    bar::set_shape(&app, bar::Rect { x, y, w, h });
}

/// Floats the bar, docks it in the notch, or turns it off.
#[tauri::command]
fn set_bar_mode(app: tauri::AppHandle, mode: bar::Mode) {
    bar::set_mode(&app, mode);
}

/// Keeps the bar docked in the notch open, or lets it close when the pointer leaves.
#[tauri::command]
fn set_notch_open(app: tauri::AppHandle, keep: bool) {
    bar::set_keep_open(&app, keep);
}

/// A PR's description, reviews and changed files.
#[tauri::command(async)]
fn pr_detail(repo: String, number: u64) -> Result<github::PrDetail, String> {
    github::detail(&repo, number).map_err(|e| format!("{e:#}"))
}

/// A PR the board doesn't list, for a link to it in a conversation: what GitHub says of it, with
/// the worktrees and sessions here that worked on it.
#[tauri::command(async)]
fn pr_card(engine: State<'_, engine::Handle>, repo: String, number: u64) -> Result<model::PrItem, String> {
    engine.pr_card(&repo, number).map_err(|e| format!("{e:#}"))
}

/// Which of a PR's comments ask for a change, by comment id, from Jev (prasks.rs). Empty while
/// `[jev] pull_requests` is off or there's no key: the panel just shows no markers.
#[tauri::command(async)]
fn pr_comment_asks(title: String, comments: Vec<prasks::CommentIn>) -> Result<std::collections::HashMap<String, f64>, String> {
    let cfg = config::load().ok().flatten().unwrap_or_default();
    if !cfg.jev.pull_requests || !jev::has_key() {
        return Ok(Default::default());
    }
    prasks::asks(cfg.jev.model.trim(), &title, &comments).map_err(|e| format!("{e:#}"))
}

/// Approves a PR as you. It returns once the board no longer asks for your review, and PRs are
/// read again for the rest.
#[tauri::command(async)]
fn approve_pr(engine: State<'_, engine::Handle>, repo: String, number: u64) -> Result<(), String> {
    github::approve(&repo, number).map_err(|e| format!("{e:#}"))?;
    engine.approved(&repo, number);
    Ok(())
}

/// Opens a PR in Codex's review.
#[tauri::command(async)]
fn review_in_codex(url: String) -> Result<(), String> {
    launch::open_codex(&launch::codex_review_url(&url)).map_err(|e| e.to_string())
}

/// Opens a web link (from a PR description) in the browser.
#[tauri::command(async)]
fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Only web links open from here.".into());
    }
    launch::open_url(&url).map_err(|e| e.to_string())
}

/// Runs a command from a session's conversation (a shell block in a reply, or a command the agent
/// ran) in a new terminal in `cwd`, where the window says it ran or belongs (runcode.rs).
#[tauri::command(async)]
fn run_in_terminal(app: tauri::AppHandle, window: tauri::WebviewWindow, engine: State<'_, engine::Handle>, cwd: String, text: String) -> Result<(), String> {
    let dir = std::path::PathBuf::from(&cwd);
    let (apps, near) = engine.read(|b| (b.apps.clone(), board::terminal_neighbours(&b.lanes, &dir)));
    runcode::run(&apps, &near, &dir, &text).map_err(|e| format!("{e:#}"))?;
    // A click in the bar's peek, the quick panel or a card on the desktop leaves Codealong
    // inactive, and macOS may not bring the terminal forward for an app that isn't.
    let label = window.label();
    if [bar::LABEL, quick::LABEL, peek::LABEL].contains(&label) || label.starts_with(desktop::PREFIX) {
        if let Some(bundle) = launch::bundle_id(&apps.terminal_id) {
            bar::bring_forward(&app, bundle);
        }
    }
    Ok(())
}

/// Archives or unarchives a session; returns the time of the board that shows it, as
/// `mark_seen` does.
#[tauri::command(async)]
fn archive_session(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, archived: bool) -> Result<Option<i64>, String> {
    actions::run(&engine, &peers, api::SESSION_ARCHIVE, &key, serde_json::json!({ "archived": archived })).map(|v| v.as_i64())
}

/// Names a session where its harness keeps its name, so the harness shows it too. Returns the
/// time of the board that shows it, as `mark_seen` does.
#[tauri::command(async)]
fn rename_session(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, name: String) -> Result<Option<i64>, String> {
    actions::run(&engine, &peers, api::SESSION_RENAME, &key, serde_json::json!({ "name": name })).map(|v| v.as_i64())
}

#[tauri::command(async)]
fn get_accounts(engine: State<'_, engine::Handle>) -> Result<Vec<model::AccountInfo>, String> {
    engine.accounts().map_err(|e| format!("{e:#}"))
}

#[tauri::command(async)]
fn save_accounts(engine: State<'_, engine::Handle>, accounts: Vec<config::AccountCfg>) -> Result<(), String> {
    engine.save_accounts(accounts).map_err(|e| format!("{e:#}"))
}

/// The star on an account in Settings › Accounts: Auto starts preferred accounts first.
#[tauri::command(async)]
fn set_preferred(engine: State<'_, engine::Handle>, account: String, preferred: bool) -> Result<(), String> {
    engine.set_preferred(&account, preferred).map_err(|e| format!("{e:#}"))
}

/// Accounts on this Mac that config.toml doesn't list, for Settings › Accounts.
#[tauri::command(async)]
fn found_accounts() -> Vec<detect::Found> {
    detect::unlisted()
}

/// The settings as parsed and config.toml as written, for Settings.
#[tauri::command(async)]
fn get_config() -> config::ConfigView {
    config::view()
}

/// Saves every setting from the Settings form, in place in config.toml.
#[tauri::command(async)]
fn save_config(engine: State<'_, engine::Handle>, config: config::Config) -> Result<(), String> {
    engine.save_config(&config).map_err(|e| format!("{e:#}"))
}

/// Where automatic updates stand, for Settings and the sidebar (updates.rs).
#[tauri::command]
fn update_status(app: tauri::AppHandle) -> updates::View {
    updates::view(&app)
}

/// Looks for a newer release now; `update-status` events say how it goes.
#[tauri::command]
fn check_for_updates(app: tauri::AppHandle) {
    updates::check_soon(&app);
}

/// Quits and opens the version an update put in place.
#[tauri::command]
fn restart_to_update(app: tauri::AppHandle) {
    app.restart();
}

/// Saves config.toml as typed in Settings, once it parses.
#[tauri::command(async)]
fn save_config_text(engine: State<'_, engine::Handle>, text: String) -> Result<(), String> {
    engine.save_config_text(&text).map_err(|e| format!("{e:#}"))
}

#[tauri::command(async)]
fn sign_in(engine: State<'_, engine::Handle>, name: String) -> Result<(), String> {
    engine.sign_in(&name).map_err(|e| format!("{e:#}"))
}

/// The `codealong` command link and the skill per account, for Settings › Agents.
#[tauri::command(async)]
fn agents_status() -> install::AgentsStatus {
    install::status()
}

/// Links the command and copies the skill for the named accounts, then reports the new state.
#[tauri::command(async)]
fn install_agents(command: bool, accounts: Vec<String>) -> Result<install::AgentsStatus, String> {
    install::install(command, &accounts).map_err(|e| format!("{e:#}"))
}

/// Where each account can start a session, for the new-session sheet.

/// Your other devices as Settings › Devices and the views show them (peers.rs).
#[tauri::command]
fn devices(peers: State<'_, peers::Peers>) -> peers::DevicesView {
    peers.view()
}

/// Gives a linked device a color, for its badge and its lanes' edge; none goes back to the one
/// its key picks.
#[tauri::command(async)]
fn set_device_color(peers: State<'_, peers::Peers>, key: String, color: Option<String>) -> Result<(), String> {
    peers.set_color(&key, color)
}

/// Asks one of your devices on your tailnet to link; returns the code both show.
#[tauri::command(async)]
fn link_device(peers: State<'_, peers::Peers>, key: String) -> Result<String, String> {
    peers.link(&key)
}

#[tauri::command(async)]
fn answer_link(peers: State<'_, peers::Peers>, key: String, accept: bool) -> Result<(), String> {
    peers.answer_link(&key, accept)
}

#[tauri::command(async)]
fn unlink_device(peers: State<'_, peers::Peers>, key: String) -> Result<(), String> {
    peers.unlink(&key)
}

/// A link code for a device that isn't on your tailnet.
#[tauri::command(async)]
fn device_link_code(peers: State<'_, peers::Peers>) -> Result<String, String> {
    peers.link_code()
}

/// A link code as a QR code (SVG) of the link the phone app opens, so a phone's camera links it.
#[tauri::command]
fn link_code_qr(code: String) -> Result<String, String> {
    link_qr_svg(&code)
}

fn link_qr_svg(code: &str) -> Result<String, String> {
    let qr = qrcode::QrCode::new(irohnet::link_url(code).as_bytes()).map_err(|e| e.to_string())?;
    Ok(qr.render::<qrcode::render::svg::Color>().min_dimensions(176, 176).dark_color(qrcode::render::svg::Color("#000000")).light_color(qrcode::render::svg::Color("#ffffff")).build())
}

/// Links with the device that made this link code; returns its name.
#[tauri::command(async)]
fn link_with_code(peers: State<'_, peers::Peers>, code: String) -> Result<String, String> {
    peers.link_with_code(&code)
}

/// What was said in sessions on your other devices, searched on each of them.
#[tauri::command(async)]
fn search_devices(peers: State<'_, peers::Peers>, words: Vec<String>) -> Vec<peers::DeviceFound> {
    let words: Vec<String> = words.iter().map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        return Vec::new();
    }
    peers.search(&words)
}

#[tauri::command(async)]
fn start_options() -> Result<start::StartOptions, String> {
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    Ok(start::options_for(&cfg))
}

/// Starts a session in a worktree: in the terminal, or in the Claude or Codex app. A start from
/// todos links each of them to the session, or to the next one of that account in the repo when
/// an app picks the session's ID.
#[tauri::command(async)]
fn start_session(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, spec: start::StartSpec) -> Result<start::Started, String> {
    if let Some((device, lane)) = peerfeed::split_remote(&spec.lane_id) {
        return start_on_device(&peers, device, lane, &spec);
    }
    start_and_link(&engine, &spec)
}

/// Forks a session: into a new terminal in its folder (`start::fork`), or, `background`, into a
/// copy that goes on without a window, on `prompt` when there's one (`start::fork_background`).
#[tauri::command(async)]
fn fork_session(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, background: Option<bool>, prompt: Option<String>) -> Result<start::Started, String> {
    let got = if background == Some(true) {
        actions::run(&engine, &peers, api::SESSION_FORK, &key, serde_json::json!({ "prompt": prompt.unwrap_or_default() }))?
    } else {
        actions::run(&engine, &peers, actions::FORK_TERMINAL, &key, serde_json::json!({}))?
    };
    serde_json::from_value(got).map_err(|e| e.to_string())
}

/// Asks something about a session on the side (side.rs): a throwaway copy of its conversation
/// answers, and the session carries on, unaware. Returns the side conversation with the question
/// in it; the answer comes as `side` events.
#[tauri::command(async)]
fn side_ask(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, text: String) -> Result<serde_json::Value, String> {
    actions::run(&engine, &peers, actions::SIDE_ASK, &key, serde_json::json!({ "text": text }))
}

/// A session's side conversation, for a window that starts showing the session.
#[tauri::command(async)]
fn side_get(key: String) -> Option<side::Side> {
    side::get(&key)
}

/// The side conversations kept for a session, the latest first, to read after they're closed.
#[tauri::command(async)]
fn side_past(key: String) -> Vec<side::Kept> {
    side::past(&key)
}

/// Forgets a kept side conversation.
#[tauri::command(async)]
fn side_forget(id: String) {
    side::forget(&id);
}

/// Ends a session's side conversation: its copy stops, and what was said is kept to read.
#[tauri::command(async)]
fn side_close(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<(), String> {
    actions::run(&engine, &peers, actions::SIDE_CLOSE, &key, serde_json::json!({})).map(|_| ())
}

/// Starts a session in a worktree of another device, which runs it with its own accounts.
fn start_on_device(peers: &peers::Peers, device: &str, lane: &str, spec: &start::StartSpec) -> Result<start::Started, String> {
    // Its todos are this device's; the other device has its own list.
    let spec = start::StartSpec { lane_id: lane.to_string(), todos: Vec::new(), from: None, ..spec.clone() };
    let got = actions::ask(peers, device, api::START, serde_json::json!({ "spec": spec }))?;
    let mut started: start::Started = serde_json::from_value(got).map_err(|e| e.to_string())?;
    started.key = started.key.map(|k| peerfeed::remote_key(device, &k));
    Ok(started)
}

/// Where each account on another device can start a session, asked of that device.
#[tauri::command(async)]
fn device_start_options(peers: State<'_, peers::Peers>, device: String) -> Result<start::StartOptions, String> {
    let got = actions::ask(&peers, &device, api::START_OPTIONS, serde_json::json!({}))?;
    serde_json::from_value(got).map_err(|e| e.to_string())
}

/// Starts a session and links it to its todos; the socket (server.rs) does the same for the
/// `codealong start` command.
pub(crate) fn start_and_link(engine: &engine::Handle, spec: &start::StartSpec) -> Result<start::Started, String> {
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    let started = start::start(&engine.board(), &cfg, spec).map_err(|e| format!("{e:#}"))?;
    origins::started(spec.from.as_deref(), started.key.as_deref(), &spec.lane_id, &spec.account);
    if !spec.todos.is_empty() {
        // One start for all of them: todos started together wait for the same session.
        let pending = match (&started.key, spec.place) {
            (Some(_), _) => None,
            (None, place) => Some(todos::PendingStart {
                lane: spec.lane_id.clone(),
                harness: cfg.accounts.iter().find(|a| a.name == spec.account).map(|a| a.harness.clone()).unwrap_or_else(|| if place == start::Place::CodexApp { "codex" } else { "claude" }.into()),
                account: spec.account.clone(),
                at: util::now_ms(),
            }),
        };
        // A session in a new worktree won't be in the one it started from: only it is linked.
        let lane = (!(spec.worktree && matches!(spec.place, start::Place::Terminal | start::Place::Background))).then(|| spec.lane_id.clone());
        engine.todo_started(spec.todos.clone(), lane, started.key.clone(), pending);
    }
    Ok(started)
}

/// A project's canvas as the window last saved it, or null.
#[tauri::command(async)]
fn canvas_load(project: String) -> Result<serde_json::Value, String> {
    canvas::load(&project).map_err(|e| format!("{e:#}"))
}

/// Saves a project's canvas.
#[tauri::command(async)]
fn canvas_save(project: String, doc: serde_json::Value) -> Result<(), String> {
    canvas::save(&project, doc).map_err(|e| format!("{e:#}"))
}

/// A change to a todo or to the labels. Returns the time of the first board that shows it.
#[tauri::command(async)]
fn todo_op(engine: State<'_, engine::Handle>, op: todos::TodoOp) -> Result<i64, String> {
    engine.todo(op)
}

/// Keeps a file pasted or picked into a description or a prompt. The window sends its bytes as
/// they are, not as JSON, and its name in `x-name` (URL-encoded, as headers are ASCII); the reply
/// says how text refers to it.
#[tauri::command(async)]
fn save_attachment(request: tauri::ipc::Request<'_>) -> Result<attachments::Attachment, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("That wasn't a file.".into());
    };
    let name = request.headers().get("x-name").and_then(|v| v.to_str().ok()).and_then(|v| urlencoding::decode(v).ok()).map(|v| v.into_owned()).unwrap_or_default();
    attachments::keep(bytes, &name).map_err(|e| format!("{e:#}"))
}

/// Keeps a copy of a file dropped from the Finder.
#[tauri::command(async)]
fn import_attachment(path: String) -> Result<attachments::Attachment, String> {
    attachments::import(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))
}

/// Text with its attachments as full paths, for copying a todo as a prompt.
#[tauri::command]
fn expand_attachments(text: String) -> String {
    attachments::expand(&text, attachments::Style::Path)
}

/// Opens a kept attachment in the app that opens its kind (Preview for an image, say).
#[tauri::command(async)]
fn open_attachment(path: String) -> Result<(), String> {
    let file = attachments::locate(&path).ok_or("That isn't one of Codealong's attachments.")?;
    if !file.exists() {
        return Err("That attachment is gone: it's cleared 30 days after no todo uses it.".into());
    }
    launch::open_url(&file.to_string_lossy()).map_err(|e| e.to_string())
}

/// Serves kept images to the windows at `codealong-image://localhost/<name>`. Only names
/// Codealong made are read, so nothing outside the images folder can be.
fn serve_image(request: tauri::http::Request<Vec<u8>>, responder: tauri::UriSchemeResponder) {
    let name = request.uri().path().trim_start_matches('/').to_string();
    std::thread::spawn(move || {
        let found = attachments::locate(&format!("{}{name}", attachments::IMAGES)).and_then(|p| std::fs::read(p).ok());
        let response = match found {
            Some(bytes) => tauri::http::Response::builder()
                .header(tauri::http::header::CONTENT_TYPE, attachments::mime(&name))
                // A name is the image's content, so it never changes.
                .header(tauri::http::header::CACHE_CONTROL, "max-age=31536000, immutable")
                .body(bytes),
            None => tauri::http::Response::builder().status(tauri::http::StatusCode::NOT_FOUND).body(Vec::new()),
        };
        responder.respond(response.unwrap_or_else(|_| {
            let mut r = tauri::http::Response::new(Vec::new());
            *r.status_mut() = tauri::http::StatusCode::INTERNAL_SERVER_ERROR;
            r
        }));
    });
}

/// Saves the theme to config.toml. Every window follows, through the board.
#[tauri::command(async)]
fn set_theme(engine: State<'_, engine::Handle>, theme: config::Theme) -> Result<(), String> {
    engine.set_theme(theme).map_err(|e| format!("{e:#}"))
}

/// The app's own appearance for a theme: `None` lets macOS decide. Window chrome, native menus
/// and each page's `prefers-color-scheme` all go by it.
fn native_theme(theme: config::Theme) -> Option<tauri::Theme> {
    match theme {
        config::Theme::System => None,
        config::Theme::Light => Some(tauri::Theme::Light),
        config::Theme::Dark => Some(tauri::Theme::Dark),
    }
}

/// The board window in front, which a menu command is for. The floating bar, the quick panel, the
/// peek and the cards pinned to the desktop never are (the bar, the peek and the cards say they're
/// key while they aren't).
fn front_window(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    app.webview_windows()
        .into_values()
        .find(|w| ![bar::LABEL, quick::LABEL, peek::LABEL].contains(&w.label()) && !w.label().starts_with(desktop::PREFIX) && w.is_focused().unwrap_or(false))
}

/// Puts Settings… (⌘,) in the app menu, under About. Choosing it tells the front window to
/// open Settings; without one, the main window comes back and opens it.
fn add_settings_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuItem, MenuItemKind, PredefinedMenuItem};
    let Some(menu) = app.menu() else { return Ok(()) };
    let Some(MenuItemKind::Submenu(first)) = menu.items()?.into_iter().next() else { return Ok(()) };
    let item = MenuItem::with_id(app, SETTINGS_MENU_ID, "Settings…", true, Some("CmdOrCtrl+,"))?;
    first.insert(&item, 2)?;
    first.insert(&PredefinedMenuItem::separator(app)?, 3)?;
    app.on_menu_event(|app, event| {
        if event.id().as_ref() != SETTINGS_MENU_ID {
            return;
        }
        // `emit_to`, not a window's `emit`: that one reaches every window.
        match front_window(app) {
            Some(w) => {
                let _ = app.emit_to(w.label(), "settings", ());
            }
            None => {
                let _ = bar::show_main(app, None);
                let _ = app.emit_to("main", "settings", ());
            }
        }
    });
    Ok(())
}

const SETTINGS_MENU_ID: &str = "settings";

/// Puts New window (⌘⇧N) at the top of the File menu, and Close idle terminals… under it. Choosing it asks the front window for
/// another window on the project it shows; without one (closing the main window only hides
/// it), the main window comes back. The menu's shortcut works with no window open, where a
/// page's own keys can't.
fn add_new_window_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuItem, MenuItemKind, PredefinedMenuItem};
    let Some(menu) = app.menu() else { return Ok(()) };
    let file = menu.items()?.into_iter().find_map(|item| match item {
        MenuItemKind::Submenu(s) if s.text().is_ok_and(|t| t == "File") => Some(s),
        _ => None,
    });
    let Some(file) = file else { return Ok(()) };
    let item = MenuItem::with_id(app, NEW_WINDOW_MENU_ID, "New window", true, Some("CmdOrCtrl+Shift+N"))?;
    let close_idle = MenuItem::with_id(app, CLOSE_IDLE_MENU_ID, "Close idle terminals…", true, None::<&str>)?;
    file.prepend_items(&[&item, &PredefinedMenuItem::separator(app)?, &close_idle, &PredefinedMenuItem::separator(app)?])?;
    app.on_menu_event(|app, event| {
        let id = event.id().as_ref();
        if id != NEW_WINDOW_MENU_ID && id != CLOSE_IDLE_MENU_ID {
            return;
        }
        match front_window(app) {
            Some(w) => {
                let _ = app.emit_to(w.label(), id, ());
            }
            None => {
                let _ = bar::show_main(app, None);
                // Close idle terminals is a dialog, so the main window opens it.
                if id == CLOSE_IDLE_MENU_ID {
                    let _ = app.emit_to("main", id, ());
                }
            }
        }
    });
    Ok(())
}

const NEW_WINDOW_MENU_ID: &str = "new-window";
const CLOSE_IDLE_MENU_ID: &str = "close-idle";

/// Every second while the perf log is on, posts to the main thread and logs how late it ran:
/// the main thread carries every click and pointer move to the windows, so its lag is theirs.
/// One still waiting after a second is stuck at that moment, so it's sampled then (a lag line
/// comes only once it's over): the sample says what it's stuck on (perf::sample_stall).
fn watch_main_thread(app: tauri::AppHandle) {
    use std::time::{Duration, Instant};
    // A probe still waiting this long means the main thread is stuck right now.
    const STUCK: Duration = Duration::from_secs(1);
    // Stuck this long is a freeze, sampled again whenever the last sample was.
    const HUNG: Duration = Duration::from_secs(10);
    // Sampling costs the Mac a few seconds of work, so a stuck main thread is sampled at most
    // this often.
    const SAMPLE_GAP: Duration = Duration::from_secs(300);
    std::thread::spawn(move || {
        let mut sampled: Option<Instant> = None;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            if !perf::enabled() {
                continue;
            }
            let post = |probe: perf::Probe| app.run_on_main_thread(probe).is_ok();
            let waited = perf::probe(post, STUCK, HUNG, |after| {
                if after >= HUNG || sampled.is_none_or(|s| s.elapsed() >= SAMPLE_GAP) {
                    sampled = Some(Instant::now());
                    perf::sample_stall(format!("main thread stuck for {} s", after.as_secs()));
                }
            });
            if let Some(w) = waited {
                perf::lag("main thread", w);
            }
        }
    });
}

pub fn run() {
    perf::arm();
    let args: Vec<String> = std::env::args().collect();
    if let Err(e) = server::take_over(server::opened_plainly(&args)) {
        eprintln!("{e}");
        std::process::exit(2);
    }
    tauri::Builder::default()
        .setup(|app| {
            // Set before the page loads, so a window never opens in the wrong theme.
            let theme = config::theme();
            app.set_theme(native_theme(theme));
            let shown = std::sync::Mutex::new(theme);
            let shown_logo = std::sync::Mutex::new(model::LogoStatus::Idle);
            let shortcut = std::sync::Mutex::new(config::quick_shortcut());
            // Your other devices (peers.rs). They come before the engine, which publishes to them,
            // and ask it for the board and for answers once it's there.
            let engine_slot: std::sync::Arc<std::sync::OnceLock<engine::Handle>> = Default::default();
            let peers = {
                let (boards, answers, emits) = (engine_slot.clone(), engine_slot.clone(), app.handle().clone());
                peers::Peers::new(
                    config::support_dir(),
                    peers::Hooks {
                        board: Box::new(move || boards.get().map(|e| e.board()).unwrap_or_default()),
                        answer: Box::new(move |method, params| match answers.get() {
                            Some(e) => actions::answer(e, method, params),
                            None => Err("Codealong is still starting on that device.".into()),
                        }),
                        emit: Box::new(move |view| {
                            let _ = emits.emit("devices", view);
                        }),
                    },
                )
            };
            let publishes = peers.clone();
            let emitter = app.handle().clone();
            let engine = engine::start(move |board| {
                if let Ok(mut shown) = shown_logo.lock() {
                    if *shown != board.logo_status {
                        match logo::update(&emitter, board.logo_status) {
                            Ok(()) => *shown = board.logo_status,
                            Err(e) => eprintln!("Couldn't update the Codealong icon: {e}"),
                        }
                    }
                }
                if let Ok(mut shown) = shown.lock() {
                    if *shown != board.theme {
                        *shown = board.theme;
                        emitter.set_theme(native_theme(board.theme));
                    }
                }
                // Registering waits on the main thread, so it gets a thread of its own: this one
                // may hold what a command on the main thread is waiting for.
                if let Ok(mut shortcut) = shortcut.lock() {
                    if *shortcut != board.quick_shortcut {
                        *shortcut = board.quick_shortcut.clone();
                        let (app, wanted) = (emitter.clone(), board.quick_shortcut.clone());
                        std::thread::spawn(move || quick::apply_shortcut(&app, &wanted));
                    }
                }
                let _ = emitter.emit("board", board);
                bar::publish(&emitter, board);
                desktop::publish(&emitter, board);
                lineage::publish(&emitter, board);
                publishes.publish(board);
            });
            let _ = engine_slot.set(engine.clone());
            // Sessions Codealong runs itself: a turn's start and end recompose the board, and
            // the reply as it streams goes to whichever windows show the session.
            {
                let (engine, app, watchers) = (engine.clone(), app.handle().clone(), peers.clone());
                acp::init(move |change| match change {
                    acp::Change::Board => engine.hosted_changed(),
                    acp::Change::Stream(s) => {
                        // Linked devices watching the session get it too (a phone showing it).
                        watchers.stream(s.clone());
                        let _ = app.emit("hosted-stream", s);
                    }
                });
            }
            // A side conversation's answer, as its copy writes it, for the windows showing its session.
            {
                let app = app.handle().clone();
                side::init(move |s| {
                    let _ = app.emit("side", s);
                });
            }
            app.manage(peers);
            server::serve(engine.clone(), app.state::<peers::Peers>().inner().clone(), app.handle().clone());
            // After the first scan and the first git runs, which have the Mac to themselves.
            search::spawn_warmer(std::time::Duration::from_secs(20));
            app.manage(engine);
            std::thread::spawn(|| {
                install::follow_command_link();
                install::refresh_installed();
            });
            if let Err(e) = bar::setup(app.handle()) {
                eprintln!("Couldn't make the floating bar: {e:#}");
            }
            if let Err(e) = quick::setup(app.handle()) {
                eprintln!("Couldn't make the quick panel: {e:#}");
            }
            if let Err(e) = peek::setup(app.handle()) {
                eprintln!("Couldn't make the floating bar's peek: {e:#}");
            }
            lineage::setup(app.handle());
            if let Err(e) = desktop::setup(app.handle()) {
                eprintln!("Couldn't put the cards pinned to the desktop back: {e:#}");
            }
            // Registering waits on the main thread, which this is.
            let handle = app.handle().clone();
            std::thread::spawn(move || quick::apply_shortcut(&handle, &config::quick_shortcut()));
            if let Err(e) = add_settings_menu(app.handle()) {
                eprintln!("Couldn't add Settings to the menu: {e:#}");
            }
            if let Err(e) = add_new_window_menu(app.handle()) {
                eprintln!("Couldn't add New window to the menu: {e:#}");
            }
            updates::setup(app.handle());
            watch_main_thread(app.handle().clone());
            Ok(())
        })
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(|app, _, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                quick::toggle(app);
            }
        }).build())
        .on_window_event(|window, event| {
            bar::on_window_event(window, event);
            quick::on_window_event(window, event);
            peek::on_window_event(window, event);
            desktop::on_window_event(window, event);
        })
        .register_asynchronous_uri_scheme_protocol("codealong-image", |_ctx, request, responder| serve_image(request, responder))
        .invoke_handler(tauri::generate_handler![get_board, session_at, idle_terminals, close_idle_terminals, close_session, open_target, open_config, editor_icons, mark_seen, snooze_sessions, refresh, refresh_usage, set_pinned, set_pinned_worktrees, set_preview, set_reusable, archive_session, rename_session, pr_detail, pr_card, pr_comment_asks, approve_pr, review_in_codex, open_url, run_in_terminal, get_accounts, save_accounts, set_preferred, found_accounts, get_config, save_config, save_config_text, sign_in, agents_status, install_agents, start_options, device_start_options, start_session, fork_session, side_ask, side_get, side_past, side_forget, side_close, todo_op, canvas_load, canvas_save, save_attachment, import_attachment, open_attachment, expand_attachments, set_theme, open_project_window, fit_bar, show_main, peek_open, peek_ready, peek_fit, peek_focus, peek_hide, bar_feed, bar_on, set_bar_on, bar_mode, set_bar_mode, set_notch_open, notch_shape, open_from_bar, desktop_pins, pin_to_desktop, unpin_from_desktop, drag_to_desktop, desktop_feed, fit_desktop, desktop_hover, lineage_feed, show_quick, hide_quick, quick_ready, fit_quick, start_from_quick, route_quick, search_text, search_devices, devices, set_device_color, link_device, answer_link, unlink_device, device_link_code, link_code_qr, link_with_code, session_page, session_newer, session_pending, session_way, session_answer, session_stream, session_cancel, send_to_session, suggest_todo, dismiss_todo_hint, jev_key, set_jev_key, test_jev, quick_shortcut, pause_quick_shortcut, perf_mark, page_unfocused, popup_menu, update_status, check_for_updates, restart_to_update])
        .build(tauri::generate_context!())
        .expect("error while running Codealong")
        .run(|app, event| {
            // The Dock icon brings the main window back after it was closed.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                let _ = bar::show_main(app, None);
            }
        });
}

/// Whether this is Codealong Dev: a build of this repo from relaunch.sh (tauri.dev.conf.json),
/// which goes by an id and icon of its own beside the Codealong in Applications.
pub(crate) fn is_dev_build(app: &tauri::AppHandle) -> bool {
    app.config().identifier.ends_with(".dev")
}

/// `codealong <subcommand>`: the command for agents and people (cli.rs).
pub fn cli_main(args: &[String]) -> i32 {
    cli::main(args)
}

/// Whether a command line is for the `codealong` command rather than the app.
pub fn is_cli(args: &[String]) -> bool {
    cli::is_cli(args)
}

/// `codealong --snapshot [--table] [--no-github] [--usage] [--accounts]`: one pass over everything,
/// printed. Claude usage is read only with `--usage`.
/// `codealong --snapshot --config`: the settings and config.toml, as Settings gets them.
/// `codealong --snapshot --found`: accounts on this Mac that config.toml doesn't list.
/// `codealong --snapshot --bar`: what the floating bar would show.
/// `codealong --snapshot --open <lane id> <target id>`: opens a target the way a click does.
/// `codealong --snapshot --pr <owner/name> <number>`: a PR's detail, as the PR panel gets it.
/// `codealong --snapshot --route <text> [--project <name>]`: where the quick panel would suggest
/// sending it (asks TypeSafe; needs the key).
/// `codealong --snapshot --pending <session key>`: what a session waits on you to allow, as its panel shows it.
/// `codealong --snapshot --answer <session key> <id> <allow|deny>`: answers it, as the panel's buttons do.
/// `codealong --snapshot --send <session key> <text>`: sends a session a message, as its panel does.
/// `codealong --snapshot --rename <session key> <name>`: names a session where its harness keeps
/// names, as Rename… in its menu does.
/// `codealong --snapshot --stop <session key>`: stops the turn it's taking, as its Stop does.
/// `codealong --snapshot --transcript <session key> [--before <cursor>]`: a page of a session's
/// conversation, as its panel gets it.
/// `codealong --snapshot --start-options`: where each account can start a session.
/// `codealong --snapshot --start-plan <folder> <account> <place> [prompt] [--model <model>]
/// [--worktree]`: what starting one would do, without doing it.
pub fn snapshot_cli(args: &[String]) -> i32 {
    let started = std::time::Instant::now();
    if let Some(i) = args.iter().position(|a| a == "--pending" || a == "--answer") {
        let answering = args[i] == "--answer";
        let usage = "usage: codealong --snapshot --pending <session key> | --answer <session key> <id> <allow|deny>";
        let Some(key) = args.get(i + 1) else {
            eprintln!("{usage}");
            return 2;
        };
        let cfg = config::load().ok().flatten().unwrap_or_default();
        let board = engine::snapshot(false, false);
        let (s, account) = match harness::find(&board, &cfg, key) {
            Ok(found) => found,
            Err(e) => {
                eprintln!("{e:#}");
                return 1;
            }
        };
        let ctx = harness::Ctx { account };
        if !answering {
            println!("{}", serde_json::to_string_pretty(&harness::adapter(s.harness).pending(&ctx, s)).unwrap_or_default());
            return 0;
        }
        let (Some(id), Some(Ok(answer))) = (args.get(i + 2), args.get(i + 3).map(|a| serde_json::from_value::<harness::Answer>(serde_json::Value::String(a.clone())))) else {
            eprintln!("{usage}");
            return 2;
        };
        return match harness::adapter(s.harness).answer(&ctx, s, id, answer) {
            Ok(said) => {
                println!("{said}");
                0
            }
            Err(e) => {
                eprintln!("{e:#}");
                1
            }
        };
    }
    if let Some(i) = args.iter().position(|a| a == "--stop") {
        let Some(key) = args.get(i + 1) else {
            eprintln!("usage: codealong --snapshot --stop <session key>");
            return 2;
        };
        let cfg = config::load().ok().flatten().unwrap_or_default();
        let board = engine::snapshot(false, false);
        let stopped = harness::find(&board, &cfg, key).and_then(|(s, account)| {
            if !harness::turn_running(s) {
                anyhow::bail!("Its turn has already ended.");
            }
            harness::adapter(s.harness).interrupt(&harness::Ctx { account }, s)
        });
        return match stopped {
            Ok(said) => {
                println!("{said}");
                0
            }
            Err(e) => {
                eprintln!("{e:#}");
                1
            }
        };
    }
    if let Some(i) = args.iter().position(|a| a == "--send") {
        let (Some(key), Some(text)) = (args.get(i + 1), args.get(i + 2)) else {
            eprintln!("usage: codealong --snapshot --send <session key> <text>");
            return 2;
        };
        let cfg = config::load().ok().flatten().unwrap_or_default();
        let board = engine::snapshot(false, false);
        return match send::send(&board, &cfg, key, text, send::Sender::Elsewhere) {
            Ok(sent) => {
                println!("{}", sent.message);
                0
            }
            Err(e) => {
                eprintln!("{e:#}");
                1
            }
        };
    }
    if let Some(i) = args.iter().position(|a| a == "--fork") {
        let Some(key) = args.get(i + 1) else {
            eprintln!("usage: codealong --snapshot --fork <session key> [prompt]");
            return 2;
        };
        let prompt = args.get(i + 2).map(String::as_str).unwrap_or("");
        let cfg = config::load().ok().flatten().unwrap_or_default();
        let board = engine::snapshot(false, false);
        return match start::fork_background(&board, &cfg, key, prompt) {
            Ok(started) => {
                println!("{}", serde_json::to_string_pretty(&started).unwrap_or_default());
                0
            }
            Err(e) => {
                eprintln!("{e:#}");
                1
            }
        };
    }
    if let Some(i) = args.iter().position(|a| a == "--side") {
        let (Some(key), questions) = (args.get(i + 1), args.get(i + 2..).unwrap_or_default()) else {
            eprintln!("usage: codealong --snapshot --side <session key> <question> [a follow-up…]");
            return 2;
        };
        if questions.is_empty() {
            eprintln!("usage: codealong --snapshot --side <session key> <question> [a follow-up…]");
            return 2;
        }
        let cfg = config::load().ok().flatten().unwrap_or_default();
        let board = engine::snapshot(false, false);
        for q in questions {
            if let Err(e) = side::ask(&board, &cfg, key, q) {
                eprintln!("{e:#}");
                side::close(key);
                return 1;
            }
        }
        // Answers take as long as they take; a copy that stops says so.
        let until = std::time::Instant::now() + std::time::Duration::from_secs(600);
        while side::get(key).is_some_and(|s| s.running) && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        println!("{}", serde_json::to_string_pretty(&side::get(key)).unwrap_or_default());
        side::close(key);
        // The copy's process gets a moment to leave by itself.
        std::thread::sleep(std::time::Duration::from_millis(1700));
        return 0;
    }
    if let Some(i) = args.iter().position(|a| a == "--rename") {
        let (Some(key), Some(name)) = (args.get(i + 1), args.get(i + 2)) else {
            eprintln!("usage: codealong --snapshot --rename <session key> <name>");
            return 2;
        };
        let cfg = config::load().ok().flatten().unwrap_or_default();
        let board = engine::snapshot(false, false);
        let renamed = harness::session_name(name).and_then(|name| {
            let (s, account) = harness::find(&board, &cfg, key)?;
            harness::adapter(s.harness).rename(&harness::Ctx { account }, s, &name).map(|_| name)
        });
        return match renamed {
            Ok(name) => {
                println!("Renamed it {name}.");
                0
            }
            Err(e) => {
                eprintln!("{e:#}");
                1
            }
        };
    }
    if let Some(i) = args.iter().position(|a| a == "--transcript") {
        let Some(key) = args.get(i + 1) else {
            eprintln!("usage: codealong --snapshot --transcript <session key> [--before <cursor>]");
            return 2;
        };
        let before = args.iter().position(|a| a == "--before").and_then(|j| args.get(j + 1)).and_then(|b| b.parse().ok());
        let cfg = config::load().ok().flatten().unwrap_or_default();
        let board = engine::snapshot(false, false);
        return match transcript::page(&board, &cfg, key, before, 60) {
            Ok(page) => {
                println!("{}", serde_json::to_string_pretty(&page).unwrap_or_default());
                0
            }
            Err(e) => {
                eprintln!("{e:#}");
                1
            }
        };
    }
    if args.iter().any(|a| a == "--editor-icons") {
        println!("{}", serde_json::to_string(&editors::icons()).unwrap_or_default());
        return 0;
    }
    if args.iter().any(|a| a == "--start-options") {
        let cfg = config::load().ok().flatten().unwrap_or_default();
        println!("{}", serde_json::to_string_pretty(&start::options_for(&cfg)).unwrap_or_default());
        return 0;
    }
    if let Some(i) = args.iter().position(|a| a == "--start-plan") {
        let help = "usage: codealong --snapshot --start-plan <folder> <account> <terminal|background|claude-app|codex-app> [prompt] [--model <model>] [--effort <level>] [--worktree]";
        let (Some(folder), Some(name), Some(place)) = (args.get(i + 1), args.get(i + 2), args.get(i + 3)) else {
            eprintln!("{help}");
            return 2;
        };
        let Ok(place) = serde_json::from_value::<start::Place>(serde_json::Value::String(place.clone())) else {
            eprintln!("{help}");
            return 2;
        };
        let cfg = config::load().ok().flatten().unwrap_or_default();
        let Some(account) = cfg.accounts.iter().find(|a| &a.name == name) else {
            eprintln!("There's no account named {name}.");
            return 1;
        };
        let model = args.iter().position(|a| a == "--model").and_then(|j| args.get(j + 1)).cloned();
        let effort = args.iter().position(|a| a == "--effort").and_then(|j| args.get(j + 1)).cloned();
        let prompt = args.get(i + 4).filter(|p| !p.starts_with("--")).cloned().unwrap_or_default();
        let worktree = args.iter().any(|a| a == "--worktree");
        let plan = start::plan(account, place, &util::expand(folder), &prompt, &start::new_session_id(), model.as_deref(), effort.as_deref(), worktree);
        println!("{}", serde_json::to_string_pretty(&plan).unwrap_or_default());
        return 0;
    }
    if let Some(i) = args.iter().position(|a| a == "--route") {
        let Some(text) = args.get(i + 1) else {
            eprintln!("usage: codealong --snapshot --route <text> [--project <name>]");
            return 2;
        };
        let project = args.iter().position(|a| a == "--project").and_then(|j| args.get(j + 1)).cloned().unwrap_or_else(|| route::ALL_PROJECTS.to_string());
        let cfg = config::load().ok().flatten().unwrap_or_default();
        let board = engine::snapshot(false, false);
        return match route::route(&board, &cfg, &project, text) {
            Ok(r) => {
                println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
                0
            }
            Err(e) => {
                eprintln!("{e:#}");
                1
            }
        };
    }
    if args.iter().any(|a| a == "--config") {
        println!("{}", serde_json::to_string_pretty(&config::view()).unwrap_or_default());
        return 0;
    }
    if args.iter().any(|a| a == "--found") {
        println!("{}", serde_json::to_string_pretty(&detect::unlisted()).unwrap_or_default());
        return 0;
    }
    if args.iter().any(|a| a == "--accounts") {
        let cfg = config::load().ok().flatten().unwrap_or_default();
        let list: Vec<model::AccountInfo> = cfg.accounts.iter().map(usage::describe).collect();
        println!("{}", serde_json::to_string_pretty(&list).unwrap_or_default());
        return 0;
    }
    if let Some(i) = args.iter().position(|a| a == "--pr-card") {
        let (Some(repo), Some(number)) = (args.get(i + 1), args.get(i + 2).and_then(|n| n.parse::<u64>().ok())) else {
            eprintln!("usage: codealong --snapshot --pr-card <owner/name> <number>");
            return 2;
        };
        return match engine::snapshot_pr_card(repo, number) {
            Ok(item) => {
                println!("{}", serde_json::to_string_pretty(&item).unwrap_or_default());
                0
            }
            Err(e) => {
                eprintln!("{e:#}");
                1
            }
        };
    }
    if let Some(i) = args.iter().position(|a| a == "--pr") {
        let (Some(repo), Some(number)) = (args.get(i + 1), args.get(i + 2).and_then(|n| n.parse::<u64>().ok())) else {
            eprintln!("usage: codealong --snapshot --pr <owner/name> <number>");
            return 2;
        };
        return match github::detail(repo, number) {
            Ok(d) => {
                println!("{}", serde_json::to_string_pretty(&d).unwrap_or_default());
                0
            }
            Err(e) => {
                eprintln!("{e:#}");
                1
            }
        };
    }
    if let Some(i) = args.iter().position(|a| a == "--open") {
        let (Some(lane), Some(target)) = (args.get(i + 1), args.get(i + 2)) else {
            eprintln!("usage: codealong --snapshot --open <lane id> <target id>");
            return 2;
        };
        let board = engine::snapshot(false, false);
        return match engine::open_on(&board, lane, target) {
            Ok(_) => 0,
            Err(e) => {
                eprintln!("{e:#}");
                1
            }
        };
    }
    let board = engine::snapshot(!args.iter().any(|a| a == "--no-github"), args.iter().any(|a| a == "--usage"));
    if args.iter().any(|a| a == "--bar") {
        println!("{}", serde_json::to_string_pretty(&barfeed::feed(&board)).unwrap_or_default());
        return 0;
    }
    if args.iter().any(|a| a == "--table") {
        print_table(&board);
    } else {
        match serde_json::to_string_pretty(&board) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        }
    }
    eprintln!("snapshot: {} lanes in {} ms", board.lanes.len(), started.elapsed().as_millis());
    0
}

fn print_table(board: &model::Board) {
    for w in &board.warnings {
        println!("! {w}");
    }
    for p in &board.projects {
        let repos: Vec<String> = p.repos.iter().map(|r| format!("{} {}", r.name, r.lanes)).collect();
        println!("# {} ({} lanes): {}", p.name, p.lanes, repos.join(", "));
    }
    for lane in board.lanes.iter().filter(|l| !l.hidden) {
        let sessions: Vec<String> = lane.sessions.iter().take(4).map(|s| format!("{}:{:?}", s.account, s.status).to_lowercase()).collect();
        let pr = lane.pr.as_ref().map(|p| format!("#{} {:?}", p.number, p.state).to_lowercase()).unwrap_or_default();
        println!(
            "{:<8} {:<30} {:<14} {:<60} | {:<40} [{}] {}{}",
            format!("{:?}", lane.bay).to_lowercase(),
            util::truncate(&lane.callsign, 30),
            util::truncate(&lane.repo, 14),
            util::truncate(&lane.headline, 60),
            util::truncate(lane.detail.as_deref().unwrap_or(""), 40),
            sessions.join(" "),
            pr,
            if lane.parked { " (parked)" } else { "" }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn another_window_takes_the_first_free_label() {
        assert_eq!(project_window_label("Kestrel"), "project-kestrel");
        assert_eq!(project_window_label("All projects"), "project-all-projects");
        assert_eq!(another_window_label("Kestrel", |_| false), "project-kestrel");
        let open = ["project-kestrel", "project-kestrel:2"];
        assert_eq!(another_window_label("Kestrel", |l| open.contains(&l)), "project-kestrel:3");
        // A project whose own label looks like a second window's still gets its own.
        assert_eq!(project_window_label("Kestrel 2"), "project-kestrel-2");
        assert_ne!(another_window_label("Kestrel", |l| l == "project-kestrel"), project_window_label("Kestrel 2"));
    }

    #[test]
    fn every_window_may_listen_for_events() {
        // A window the capability doesn't list is refused `listen`: its page runs commands but never
        // hears an event, so it shows only what it read as it loaded (the lines between desktop
        // cards drew one stale line that way).
        let capability: serde_json::Value = serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let listed: Vec<&str> = capability["windows"].as_array().unwrap().iter().filter_map(|w| w.as_str()).collect();
        let allowed = |label: &str| listed.iter().any(|w| *w == label || w.strip_suffix('*').is_some_and(|prefix| label.starts_with(prefix)));
        let card = format!("{}1", desktop::PREFIX);
        for label in ["main", project_window_label("Kestrel").as_str(), bar::LABEL, peek::LABEL, quick::LABEL, card.as_str(), lineage::LABEL] {
            assert!(allowed(label), "{label} isn't in capabilities/default.json");
        }
    }

    #[test]
    fn link_codes_draw_as_qr_codes() {
        let code = irohnet::Ticket::new(iroh::SecretKey::generate().public()).encode();
        let svg = link_qr_svg(&code).unwrap();
        assert!(svg.contains("<svg") && svg.contains("#000000") && svg.contains("#ffffff"), "{svg}");
    }
}

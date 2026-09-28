//! Codealong: every agent runs in a worktree, and every worktree is a lane.

mod acp;
mod appserver;
mod attachments;
mod bar;
mod barfeed;
mod board;
mod canvas;
mod claude;
mod claudeapp;
mod cli;
mod antigravity;
mod codex;
mod opencode;
mod peek;
mod config;
mod detect;
mod desktop;
mod devicekey;
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
mod logo;
mod model;
mod peerfeed;
use codealong_peer::proto as peerproto;
use codealong_peer::api;
mod peers;
mod perf;
mod prasks;
mod prlinks;
mod procs;
mod quick;
mod repos;
mod send;
mod route;
mod search;
mod server;
mod start;
mod state;
mod tailnet;
mod throttle;
mod todojev;
mod todos;
mod transcript;
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
    flash_focused(&app, &engine, &target_id, from);
    Ok(())
}

/// After a session's terminal came forward, shows where when it's on another screen (flash.rs).
fn flash_focused(app: &tauri::AppHandle, engine: &engine::Handle, target_id: &str, from: Option<(f64, f64)>) {
    if !target_id.ends_with("|focus") {
        return;
    }
    if let Some(bundle) = bar::target_app(target_id, &engine.board().apps.terminal_id) {
        flash::after_focus(app, from, bundle);
    }
}

#[tauri::command(async)]
fn open_config(engine: State<'_, engine::Handle>) -> Result<(), String> {
    engine.open_config().map_err(|e| e.to_string())
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
            let _ = peers.ask(&device, api::MARK_SEEN, serde_json::json!({ "keys": keys }), ASK_ACT);
        });
    }
    if here.is_empty() {
        None
    } else {
        engine.mark_seen_shown(here)
    }
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

#[tauri::command]
fn set_pinned_worktrees(engine: State<'_, engine::Handle>, ids: Vec<String>) {
    engine.set_pinned_worktrees(ids);
}

/// A window's own timing line for the perf log (see perf.rs), while `[debug] perf_log` is on.
#[tauri::command(async)]
fn perf_mark(window: tauri::Window, line: String) {
    perf::log(&format!("ui[{}] {line}", window.label()));
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
    if let Some(bundle) = bar::target_app(&target_id, &engine.board().apps.terminal_id) {
        bar::bring_forward(&app, bundle);
    }
    flash_focused(&app, &engine, &target_id, from);
    Ok(())
}

/// Shows a session in the floating bar's peek: for a hover over its pill (where the bar's page
/// measured it), or with `focus` ready to type into (peek.rs).
#[tauri::command]
fn peek_open(app: tauri::AppHandle, engine: State<'_, engine::Handle>, key: String, pill: Option<bar::Rect>, focus: Option<bool>) -> Result<(), String> {
    let target = engine.read(|b| peek::target(b, &key)).ok_or_else(|| "That session isn't on the board any more.".to_string())?;
    peek::open(&app, target, pill, focus.unwrap_or(false)).map_err(|e| e.to_string())
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
    desktop::pin(&app, pin).map_err(|e| format!("Couldn't pin it to the desktop: {e}"))
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
    if let Some(bundle) = quick::start_app(spec.place, &engine.board().apps.terminal_id) {
        bar::bring_forward(&app, bundle);
    }
    Ok(started)
}

/// Where what's typed in the quick panel should go, from Jev (route.rs).
#[tauri::command(async)]
fn route_quick(engine: State<'_, engine::Handle>, project: String, text: String) -> Result<route::Route, String> {
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    if !cfg.jev.quick_panel {
        return Err("Suggestions in the quick panel are off.".into());
    }
    route::route(&engine.board(), &cfg, &project, &text).map_err(|e| format!("{e:#}"))
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
    if let Some((device, key)) = peerfeed::split_remote(&key) {
        return peers.ask(device, api::SESSION_PAGE, serde_json::json!({ "key": key, "before": before }), ASK_READ);
    }
    page_here(engine.inner(), &key, before)
}

fn page_here(engine: &engine::Handle, key: &str, before: Option<u64>) -> Result<serde_json::Value, String> {
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    let page = transcript::page(&engine.board(), &cfg, key, before, 60).map_err(|e| format!("{e:#}"))?;
    serde_json::to_value(page).map_err(|e| e.to_string())
}

/// What a session waiting on you wants to do, when it's a tool call Codealong can answer for it
/// (`harness::Adapter::pending`).
#[tauri::command(async)]
fn session_pending(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<serde_json::Value, String> {
    if let Some((device, key)) = peerfeed::split_remote(&key) {
        return peers.ask(device, api::SESSION_PENDING, serde_json::json!({ "key": key }), ASK_READ);
    }
    pending_here(engine.inner(), &key)
}

fn pending_here(engine: &engine::Handle, key: &str) -> Result<serde_json::Value, String> {
    if acp::hosts(key) {
        return serde_json::to_value(acp::pending(key)).map_err(|e| e.to_string());
    }
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    let board = engine.board();
    let (s, account) = harness::find(&board, &cfg, key).map_err(|e| format!("{e:#}"))?;
    serde_json::to_value(harness::adapter(s.harness).pending(&harness::Ctx { account }, s)).map_err(|e| e.to_string())
}

/// Allows or denies the tool call `id` a session waits on, if it's still the one waiting.
#[tauri::command(async)]
fn session_answer(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, id: String, answer: harness::Answer) -> Result<String, String> {
    if let Some((device, key)) = peerfeed::split_remote(&key) {
        let said = peers.ask(device, api::SESSION_ANSWER, serde_json::json!({ "key": key, "id": id, "answer": answer }), ASK_ACT)?;
        return Ok(said.as_str().unwrap_or_default().to_string());
    }
    answer_here(engine.inner(), &key, &id, answer)
}

fn answer_here(engine: &engine::Handle, key: &str, id: &str, answer: harness::Answer) -> Result<String, String> {
    if acp::hosts(key) {
        return acp::answer(key, id, answer).map_err(|e| format!("{e:#}"));
    }
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    let board = engine.board();
    let (s, account) = harness::find(&board, &cfg, key).map_err(|e| format!("{e:#}"))?;
    harness::adapter(s.harness).answer(&harness::Ctx { account }, s, id, answer).map_err(|e| format!("{e:#}"))
}

/// What a session said and did after `after`, as it goes on.
#[tauri::command(async)]
fn session_newer(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, after: u64) -> Result<serde_json::Value, String> {
    if let Some((device, key)) = peerfeed::split_remote(&key) {
        return peers.ask(device, api::SESSION_NEWER, serde_json::json!({ "key": key, "after": after }), ASK_READ);
    }
    newer_here(engine.inner(), &key, after)
}

fn newer_here(engine: &engine::Handle, key: &str, after: u64) -> Result<serde_json::Value, String> {
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    let page = transcript::newer(&engine.board(), &cfg, key, after).map_err(|e| format!("{e:#}"))?;
    serde_json::to_value(page).map_err(|e| e.to_string())
}

/// The reply a session Codealong runs is writing, for a panel that opens mid-turn. One on another
/// device is asked of it.
#[tauri::command(async)]
fn session_stream(peers: State<'_, peers::Peers>, key: String) -> Option<acp::Stream> {
    if let Some((device, own)) = peerfeed::split_remote(&key) {
        let got = peers.ask(device, api::SESSION_STREAM, serde_json::json!({ "key": own }), ASK_READ).ok()?;
        let mut s: acp::Stream = serde_json::from_value(got).ok()?;
        s.key = key.clone();
        return Some(s);
    }
    acp::stream(&key)
}

/// Stops the turn a session Codealong runs is taking, here or on another device.
#[tauri::command(async)]
fn session_cancel(peers: State<'_, peers::Peers>, key: String) -> Result<(), String> {
    if let Some((device, own)) = peerfeed::split_remote(&key) {
        return peers.ask(device, api::SESSION_CANCEL, serde_json::json!({ "key": own }), ASK_ACT).map(|_| ());
    }
    acp::cancel(&key).map_err(|e| format!("{e:#}"))
}

/// Sends a prompt to a session that's already going, where it runs (send.rs). Only Codealong's
/// windows call this, so what it sends is what you typed.
#[tauri::command(async)]
fn send_to_session(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, text: String) -> Result<serde_json::Value, String> {
    if let Some((device, key)) = peerfeed::split_remote(&key) {
        return peers.ask(device, api::SESSION_SEND, serde_json::json!({ "key": key, "text": text }), ASK_ACT);
    }
    serde_json::to_value(send_here(engine.inner(), &key, &text, send::Sender::You)?).map_err(|e| e.to_string())
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

/// A PR's description, reviews and changed files.
#[tauri::command(async)]
fn pr_detail(repo: String, number: u64) -> Result<github::PrDetail, String> {
    github::detail(&repo, number).map_err(|e| format!("{e:#}"))
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

/// Approves a PR as you, then refreshes PRs so the board shows it.
#[tauri::command(async)]
fn approve_pr(engine: State<'_, engine::Handle>, repo: String, number: u64) -> Result<(), String> {
    github::approve(&repo, number).map_err(|e| format!("{e:#}"))?;
    engine.refresh_github();
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

/// Returns the time of the board that shows it, as `mark_seen` does.
#[tauri::command(async)]
fn archive_session(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, archived: bool) -> Result<Option<i64>, String> {
    if let Some((device, key)) = peerfeed::split_remote(&key) {
        return peers.ask(device, api::SESSION_ARCHIVE, serde_json::json!({ "key": key, "archived": archived }), ASK_ACT).map(|_| None);
    }
    engine.archive(&key, archived).map_err(|e| format!("{e:#}"))
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
/// How long a window waits on another device: to read, and to act.
const ASK_READ: std::time::Duration = std::time::Duration::from_secs(15);
const ASK_ACT: std::time::Duration = std::time::Duration::from_secs(40);

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

/// Answers a linked device's request, with the function the matching command runs for a window
/// here. peers.rs has already checked the device is linked, and that remote control is on for the
/// requests that act on a session.
fn peer_answer(engine: &engine::Handle, method: &str, p: serde_json::Value) -> Result<serde_json::Value, String> {
    use serde_json::{to_value, Value};
    let key = || p.get("key").and_then(Value::as_str).map(str::to_string).ok_or_else(|| "The request names no session.".to_string());
    let text = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    match method {
        api::SESSION_PAGE => page_here(engine, &key()?, p.get("before").and_then(Value::as_u64)),
        api::SESSION_NEWER => newer_here(engine, &key()?, p.get("after").and_then(Value::as_u64).unwrap_or(0)),
        api::SESSION_PENDING => pending_here(engine, &key()?),
        api::SEARCH => {
            let words: Vec<String> = p.get("words").cloned().and_then(|w| serde_json::from_value(w).ok()).unwrap_or_default();
            to_value(search_text(words)).map_err(|e| e.to_string())
        }
        api::SESSION_SEND => {
            let sent = send_here(engine, &key()?, &text("text"), send::Sender::Elsewhere)?;
            // Opening the session with the prompt ready to send needs someone at this device.
            if sent.opened {
                return Err(format!("It needs someone at this device: {}", sent.message));
            }
            to_value(sent).map_err(|e| e.to_string())
        }
        api::SESSION_ANSWER => {
            let answer: harness::Answer = p.get("answer").cloned().and_then(|a| serde_json::from_value(a).ok()).ok_or("The request has no answer.")?;
            answer_here(engine, &key()?, &text("id"), answer).map(Value::String)
        }
        api::SESSION_ARCHIVE => engine.archive(&key()?, p.get("archived").and_then(Value::as_bool).unwrap_or(true)).map(|_| Value::Null).map_err(|e| format!("{e:#}")),
        api::MARK_SEEN => {
            let keys: Vec<String> = p.get("keys").cloned().and_then(|k| serde_json::from_value(k).ok()).unwrap_or_default();
            engine.mark_seen(keys);
            Ok(Value::Null)
        }
        api::START_OPTIONS => to_value(start_options()?).map_err(|e| e.to_string()),
        api::START => {
            let spec: start::StartSpec = p.get("spec").cloned().and_then(|s| serde_json::from_value(s).ok()).ok_or("The request has no start.")?;
            to_value(start_and_link(engine, &spec)?).map_err(|e| e.to_string())
        }
        api::SESSION_STREAM => to_value(acp::stream(&key()?)).map_err(|e| e.to_string()),
        api::SESSION_CANCEL => acp::cancel(&key()?).map(|_| Value::Null).map_err(|e| format!("{e:#}")),
        _ => Err(format!("Codealong on this device doesn't know how to {method} yet; update it here.")),
    }
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

/// Forks a session into a new terminal in its folder (`start::fork`).
#[tauri::command(async)]
fn fork_session(engine: State<'_, engine::Handle>, key: String) -> Result<start::Started, String> {
    if peerfeed::split_remote(&key).is_some() {
        return Err("Fork it on the Mac it runs on.".into());
    }
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    start::fork(&engine.board(), &cfg, &key).map_err(|e| format!("{e:#}"))
}

/// Starts a session in a worktree of another device, which runs it with its own accounts.
fn start_on_device(peers: &peers::Peers, device: &str, lane: &str, spec: &start::StartSpec) -> Result<start::Started, String> {
    // Its todos are this device's; the other device has its own list.
    let spec = start::StartSpec { lane_id: lane.to_string(), todos: Vec::new(), ..spec.clone() };
    let got = peers.ask(device, api::START, serde_json::json!({ "spec": spec }), ASK_ACT)?;
    let mut started: start::Started = serde_json::from_value(got).map_err(|e| e.to_string())?;
    started.key = started.key.map(|k| peerfeed::remote_key(device, &k));
    Ok(started)
}

/// Where each account on another device can start a session, asked of that device.
#[tauri::command(async)]
fn device_start_options(peers: State<'_, peers::Peers>, device: String) -> Result<start::StartOptions, String> {
    let got = peers.ask(&device, api::START_OPTIONS, serde_json::json!({}), ASK_READ)?;
    serde_json::from_value(got).map_err(|e| e.to_string())
}

/// Starts a session and links it to its todos; the socket (server.rs) does the same for the
/// `codealong start` command.
pub(crate) fn start_and_link(engine: &engine::Handle, spec: &start::StartSpec) -> Result<start::Started, String> {
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    let started = start::start(&engine.board(), &cfg, spec).map_err(|e| format!("{e:#}"))?;
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
                            Some(e) => peer_answer(e, method, params),
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
            app.manage(peers);
            server::serve(engine.clone(), app.state::<peers::Peers>().inner().clone());
            // After the first scan and the first git runs, which have the Mac to themselves.
            search::spawn_warmer(std::time::Duration::from_secs(20));
            app.manage(engine);
            std::thread::spawn(install::refresh_installed);
            if let Err(e) = bar::setup(app.handle()) {
                eprintln!("Couldn't make the floating bar: {e:#}");
            }
            if let Err(e) = quick::setup(app.handle()) {
                eprintln!("Couldn't make the quick panel: {e:#}");
            }
            if let Err(e) = peek::setup(app.handle()) {
                eprintln!("Couldn't make the floating bar's peek: {e:#}");
            }
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
            watch_main_thread(app.handle().clone());
            Ok(())
        })
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
        .invoke_handler(tauri::generate_handler![get_board, idle_terminals, close_idle_terminals, open_target, open_config, mark_seen, snooze_sessions, refresh, refresh_usage, set_pinned, set_pinned_worktrees, archive_session, pr_detail, pr_comment_asks, approve_pr, review_in_codex, open_url, get_accounts, save_accounts, set_preferred, found_accounts, get_config, save_config, save_config_text, sign_in, agents_status, install_agents, start_options, device_start_options, start_session, fork_session, todo_op, canvas_load, canvas_save, save_attachment, import_attachment, open_attachment, expand_attachments, set_theme, open_project_window, fit_bar, show_main, peek_open, peek_ready, peek_fit, peek_focus, peek_hide, bar_feed, bar_on, set_bar_on, open_from_bar, desktop_pins, pin_to_desktop, unpin_from_desktop, desktop_feed, fit_desktop, show_quick, hide_quick, quick_ready, fit_quick, start_from_quick, route_quick, search_text, search_devices, devices, set_device_color, link_device, answer_link, unlink_device, device_link_code, link_code_qr, link_with_code, session_page, session_newer, session_pending, session_answer, session_stream, session_cancel, send_to_session, suggest_todo, dismiss_todo_hint, jev_key, set_jev_key, test_jev, quick_shortcut, pause_quick_shortcut, perf_mark, popup_menu])
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
    fn link_codes_draw_as_qr_codes() {
        let code = irohnet::Ticket::new(iroh::SecretKey::generate().public()).encode();
        let svg = link_qr_svg(&code).unwrap();
        assert!(svg.contains("<svg") && svg.contains("#000000") && svg.contains("#ffffff"), "{svg}");
    }
}

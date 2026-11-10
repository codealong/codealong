//! Codealong: every agent runs in a worktree, and every worktree is a lane.

mod accountpoll;
mod acp;
mod actions;
mod agentdone;
mod artifacts;
mod attachments;
mod autoreview;
mod bar;
mod barfeed;
mod calls;
mod windowfeed;
mod board;
mod boards;
mod changes;
mod chrome;
mod chrometabs;
mod caret;
mod claude;
mod cleanup;
mod cli;
mod antigravity;
mod cursor;
mod cycle;
mod codex;
mod opencode;
mod origins;
mod outbox;
mod os;
mod panel;
mod peek;
mod config;
mod detect;
mod desktop;
mod devicekey;
mod dictation;
mod ear;
mod editors;
mod engine;
// The tests' sessions, lanes and scratch folders are codealong-peer's, compiled here too: one
// crate's tests can't see another's test-only code.
#[cfg(test)]
#[path = "../peer/src/fixtures.rs"]
mod fixtures;
mod geometry;
mod flash;
mod frames;
mod front;
mod ghpoll;
mod git;
mod harness;
mod held;
mod helpers;
mod hibernate;
mod http;
mod hygiene;
mod idle;
mod improve;
mod github;
mod install;
mod insights;
mod emblems;
// Link codes and iroh, the messages between devices, and the model their feeds carry are in
// codealong-peer (peer/), which the phone app builds on too.
use codealong_peer::net as irohnet;
mod jev;
mod jsonedit;
mod kept;
mod launch;
mod ledger;
mod limits;
mod listen;
mod newfolder;
mod nudges;
mod lineage;
mod logo;
mod menubar;
mod mcp;
mod model;
mod peerfeed;
use codealong_peer::proto as peerproto;
use codealong_peer::api;
mod peers;
mod perf;
mod pointer;
mod prasks;
mod prdiff;
mod previews;
mod providerflags;
mod prices;
mod prlinks;
mod prmerge;
mod procs;
mod quick;
mod reconnect;
mod repos;
mod rerequest;
mod reusable;
mod reviewdone;
mod send;
mod route;
mod routelog;
mod rpc;
mod runcode;
mod schedules;
mod search;
mod setup;
mod shared;
mod shown;
mod side;
mod skills;
mod sounds;
mod serve;
mod server;
mod snapshot;
mod snoozewhen;
mod stale;
mod start;
mod starts;
mod state;
mod stops;
mod store;
mod suggestions;
mod tags;
mod tailnet;
mod teamfile;
mod terminal;
mod terminals;
mod throttle;
mod todojev;
mod todos;
mod training;
mod transcript;
mod updates;
mod usage;
mod util;
mod view;
mod views;
mod waiting;
// Holds src/types.ts to the model: tests only.
#[cfg(test)]
mod wiretypes;
mod zoom;

use tauri::{Emitter, Manager, State};
use util::words;

/// The checkout this engine was built from: `.cargo/config.toml` sets `CODEALONG_CHECKOUT` to
/// the folder it's in. Every checkout builds into one shared directory, and the workspace's crates
/// to the same files from each, which cargo would take for fresh by their times alone. Cargo also
/// rebuilds a crate when an environment variable it read has changed, so reading this one here
/// (and in codealong-peer) rebuilds both whenever another checkout built them last. It's `None`
/// in a build that didn't go through that config. `--snapshot --checkout` prints it, which is how
/// scripts/engine.sh knows whose binary it copied.
pub const CHECKOUT: Option<&str> = option_env!("CODEALONG_CHECKOUT");

/// `Acme` → `project-acme-37036cd8`. Window labels allow letters, digits, `-`, `/`, `:` and `_`,
/// so the name is cut down to those, and the start of a hash of the whole name keeps apart two
/// projects that cut down the same (`Foo Bar` and `Foo-Bar`, or names in a script that leaves
/// nothing). A long name is cut short, so the label stays short too.
fn project_window_label(project: &str) -> String {
    use sha2::{Digest, Sha256};
    let slug: String = project.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let slug: String = slug.trim_matches('-').chars().take(40).collect();
    let hash = Sha256::digest(project.as_bytes());
    let short: String = hash[..4].iter().map(|b| format!("{b:02x}")).collect();
    match slug.trim_end_matches('-') {
        "" => format!("project-{short}"),
        slug => format!("project-{slug}-{short}"),
    }
}

/// A label for one more window on a project: the project's own if no window has it, else
/// `project-acme-37036cd8:2`, `:3`… A slug has no `:`, so these never take another project's label.
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
    let builder = tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App("index.html".into()))
        .title(format!("Codealong · {project}"))
        .inner_size(1320.0, 860.0)
        .min_inner_size(980.0, 600.0)
        .initialization_script(&script);
    let mut builder = os::windows::overlay_title_bar(builder, Some((18.0, 20.0)));
    // Cascade from the window you asked in so the new one opens on the same screen but doesn't
    // sit exactly on top of it. The main window can be closed, which only hides it.
    if let (Ok(pos), Ok(scale)) = (window.outer_position(), window.scale_factor()) {
        let at = pos.to_logical::<f64>(scale);
        let step = 28.0 * app.webview_windows().len() as f64;
        builder = builder.position(at.x + step, at.y + step);
    }
    let window = builder.build().map_err(|e| format!("Couldn't open a window for {project}: {e}"))?;
    zoom::fit(&window);
    Ok(())
}

/// Runs a command's body on the blocking pool, for the commands that can wait on a script,
/// another device, `gh` or curl. Tauri's macro runs a sync command marked `async` inline on one
/// of the async runtime's few workers, so a handful of them waiting at once would hold up every
/// other async command.
async fn blocking<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(body).await.map_err(|e| format!("That stopped partway: {e}"))
}

/// Hands `body` to `run` (`run_on_main_thread`, say) and awaits what it gives back, holding no
/// worker of the async runtime meanwhile, however long that takes: a menu runs on the main
/// thread until it closes.
async fn answer_from<T: Send + 'static>(run: impl FnOnce(Box<dyn FnOnce() + Send>) -> Result<(), String>, body: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    let (tx, mut rx) = tauri::async_runtime::channel(1);
    run(Box::new(move || {
        // The one message on a channel that holds one: it always goes, unless the command has
        // stopped waiting, when there's nobody to tell.
        let _ = tx.try_send(body());
    }))?;
    rx.recv().await.ok_or_else(|| "It was dropped before it could run.".to_string())
}

/// Off the main thread, as the floating bar's feed and the devices view are: the board is big to
/// copy and send. It's the app's copy of the server's (view.rs).
#[tauri::command(async)]
fn get_board(mirror: State<'_, view::Mirror>) -> model::Board {
    mirror.board()
}

/// Opens one of a worktree's or session's targets where it lives, from this app, the one you
/// clicked in (`open_and_bring`).
#[tauri::command]
async fn open_target(app: tauri::AppHandle, mirror: State<'_, view::Mirror>, lane_id: String, target_id: String) -> Result<(), String> {
    let mirror = mirror.inner().clone();
    blocking(move || open_and_bring(&app, &mirror.board(), &lane_id, &target_id)).await?
}

/// Opens a target where it lives (`engine::open_here`) and has the server mark the session it
/// opened seen. It's opened here, by the app you clicked in, which macOS lets bring what it opens
/// forward, and the app it landed in is asked to come forward too: the terminals Codealong scripts
/// bring themselves forward; Rex, driven through its command, can't.
fn open_and_bring(app: &tauri::AppHandle, board: &model::Board, lane_id: &str, target_id: &str) -> Result<(), String> {
    if peerfeed::split_remote(lane_id).is_some() {
        return Err("That's on another device: it opens there, and its sessions show in their panel here.".into());
    }
    let from = flash::pointer();
    if let Some(key) = engine::open_here(board, lane_id, target_id).map_err(words)? {
        if let Err(e) = view::call("mark_seen", serde_json::json!({ "keys": [key] })) {
            eprintln!("Couldn't mark {key} seen: {e}");
        }
    }
    if let Some(bundle) = target_app(board, lane_id, target_id) {
        front::bring_forward(app, bundle);
    }
    flash_focused(app, board, lane_id, target_id, from);
    Ok(())
}

/// After a session's terminal came forward, shows where when it's on another screen (flash.rs).
fn flash_focused(app: &tauri::AppHandle, board: &model::Board, lane_id: &str, target_id: &str, from: Option<(f64, f64)>) {
    if !target_id.ends_with("|focus") {
        return;
    }
    if let Some(bundle) = target_app(board, lane_id, target_id) {
        flash::after_focus(app, from, bundle);
    }
}

/// The app an open target lands in (`bar::target_app`): for a session's terminal, the app its
/// process runs under, whichever terminal config.toml names.
fn target_app(board: &model::Board, lane_id: &str, target_id: &str) -> Option<&'static str> {
    let session = target_id.strip_prefix("s|").and_then(|t| t.rsplit_once('|')).and_then(|(key, action)| Some((board.lanes.iter().find(|l| l.id == lane_id)?.sessions.iter().find(|s| s.key == key)?, action)));
    let host = session.and_then(|(s, _)| launch::session_app(s.pid?));
    let own = session.and_then(|(s, action)| harness::adapter(s.harness).app_bundle(action));
    bar::target_app(target_id, &board.apps.terminal_id, host, own)
}

#[tauri::command]
async fn open_config(mirror: State<'_, view::Mirror>) -> Result<(), String> {
    let mirror = mirror.inner().clone();
    blocking(move || mirror.read(engine::open_config).map_err(words)).await?
}

/// The installed editors' app icons, for the Open in menus. Off the main thread: the first
/// drawing of each takes a moment, and AppKit draws into a bitmap on any thread.
#[tauri::command(async)]
fn editor_icons() -> Vec<editors::Icon> {
    editors::icons()
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

/// Whether the window asking has the keyboard, as the OS has it (os/windows `has_keys`): a page's
/// own `document.hasFocus()` is true in a panel that floats while it isn't. A terminal takes its
/// size and colors from the window you're in (src/terminals.ts `hasKeys`).
#[tauri::command]
fn window_keys(window: tauri::WebviewWindow) -> bool {
    panel::has_keys(&window)
}

/// Where the page has the pointer it rests on and its lists scrolled, to compare with macOS and
/// WebKit's drawing side while `[debug] perf_log` is on (pointer.rs).
#[tauri::command]
fn pointer_probe(window: tauri::WebviewWindow, probe: pointer::PageProbe) {
    pointer::compare(&window, probe);
}

/// A right-click menu is on its way up or open.
static MENU_UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Pops up a menu the page made with `Menu.new`, in place of Tauri's `plugin:menu|popup`, which
/// keeps the page's resource table locked until the menu closes. The menu runs its own loop on
/// the main thread, and the page's other menu commands (`Menu.new`, closing one) run there too
/// and take that lock: one that came in while a menu was opening, from a second right-click,
/// hung the app for good. A menu asked for while another is opening or open (a burst of
/// right-clicks) is dropped; once that one has closed, the next is shown. The menu's closing is
/// awaited rather than waited on: it stays up as long as you leave it, and a thread of the async
/// runtime's few held that long would hold up every other async command.
#[tauri::command]
async fn popup_menu(app: tauri::AppHandle, webview: tauri::Webview, window: tauri::Window, rid: tauri::ResourceId, at: Option<tauri::Position>) -> Result<(), String> {
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
    let shown = answer_from(
        |show| app.run_on_main_thread(show).map_err(|e| e.to_string()),
        move || {
            // On the main thread Tauri shows the menu at once and returns when it has closed, so
            // the flag is down before the click that closed it reaches the page.
            let shown = match at {
                Some(at) => menu.popup_at(window, at),
                None => menu.popup(window),
            };
            drop(up);
            shown
        },
    );
    shown.await?.map_err(|e| e.to_string())
}

/// The floating bar's page measured itself.
#[tauri::command]
fn fit_bar(app: tauri::AppHandle, width: f64, height: f64) {
    bar::fit(&app, width, height);
}

/// Brings the main window forward; `reveal` is a session or PR for it to select.
#[tauri::command]
fn show_main(app: tauri::AppHandle, reveal: Option<serde_json::Value>) -> Result<(), String> {
    front::show_main(&app, reveal).map_err(words)
}

/// The link macOS asked Codealong to open (front.rs `open_links`), for the main window's page to
/// show what it names: given once, then none until another is clicked.
#[tauri::command]
fn take_opened_link() -> Option<String> {
    front::take_opened_link()
}

/// Opens a session from the floating bar, then asks macOS to bring its app forward: a click in the
/// bar doesn't make Codealong the active app.
#[tauri::command]
async fn open_from_bar(app: tauri::AppHandle, mirror: State<'_, view::Mirror>, lane_id: String, target_id: String) -> Result<(), String> {
    let mirror = mirror.inner().clone();
    blocking(move || {
        if let Some(key) = target_id.strip_prefix("s|").and_then(|t| t.strip_suffix("|hosted")) {
            // Codealong runs it: its panel in the main window is where it lives.
            if let Err(e) = view::call("mark_seen", serde_json::json!({ "keys": [key] })) {
                eprintln!("Couldn't mark {key} seen: {e}");
            }
            return front::show_main(&app, Some(serde_json::json!({ "kind": "session", "laneId": lane_id, "key": key }))).map_err(words);
        }
        open_and_bring(&app, &mirror.board(), &lane_id, &target_id)
    })
    .await?
}

/// Shows a session in the floating bar's peek: for a hover over its pill or a click on a card on
/// the desktop (where the window's page measured it), with `focus` ready to type into, or with
/// `rename` its name ready to change (peek.rs).
#[tauri::command]
fn peek_open(app: tauri::AppHandle, window: tauri::WebviewWindow, mirror: State<'_, view::Mirror>, floating: State<'_, Floating>, key: String, pill: Option<geometry::PageRect>, focus: Option<bool>, rename: Option<bool>) -> Result<(), String> {
    let target = mirror.read(|b| floating.read(b, |shown| peek::target(shown, &key))).ok_or_else(|| "That session isn't on the board any more.".to_string())?;
    let rename = rename.unwrap_or(false);
    // Kept on the desktop, it's up already: its own window comes forward, to type into if asked.
    if !rename && kept::raise_session(&app, &key, focus.unwrap_or(false)) {
        return Ok(());
    }
    // The pill is in the window that asked: the bar, or a card on the desktop.
    let pill = pill.and_then(|p| panel::frame(&window).map(|host| (host, p)));
    peek::open(&app, target, pill, focus.unwrap_or(false) || rename, rename).map_err(words)
}

/// A session with its worktree and the apps, as the board has them: for a window that doesn't
/// get the board (the floating bar) to build a session's menu on a right-click (menus.ts).
#[derive(serde::Serialize)]
struct SessionAt {
    lane: model::Lane,
    session: model::Session,
    apps: model::AppNames,
}

/// A PR the board lists, with the worktrees that hold it and the ones its sessions are listed
/// under, and the apps: for a window that doesn't get the board (the notch) to show the PR's
/// card with where it lives on this Mac (PrCard.tsx, `prPlaces`).
#[derive(serde::Serialize)]
struct PrAt {
    pr: model::PrItem,
    lanes: Vec<model::Lane>,
    apps: model::AppNames,
}

#[tauri::command(async)]
fn pr_at(mirror: State<'_, view::Mirror>, key: String) -> Option<PrAt> {
    mirror.read(|b| {
        let pr = b.prs.iter().find(|p| p.key == key)?.clone();
        let lanes = b.lanes.iter().filter(|l| pr.lanes.contains(&l.id) || pr.sessions.iter().any(|s| s.lane == l.id)).cloned().collect();
        Some(PrAt { pr, lanes, apps: b.apps.clone() })
    })
}

#[tauri::command(async)]
fn session_at(mirror: State<'_, view::Mirror>, floating: State<'_, Floating>, key: String) -> Option<SessionAt> {
    // From the worktree it runs in, not one it only visits, on whichever Mac it's on.
    mirror.read(|b| {
        floating.read(b, |shown| {
            let (lane, session) = shown.session(&key)?;
            Some(SessionAt { lane: lane.clone(), session: session.clone(), apps: b.apps.clone() })
        })
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

/// You're dragging the peek's edge: it takes that size (peek.rs).
#[tauri::command]
fn peek_resize(app: tauri::AppHandle, width: f64, height: f64) {
    peek::resize(&app, width, height);
}

/// You're dragging the peek by its header, and the pointer has gone this far since the button went
/// down: from here the peek goes where the pointer takes it, and it's kept on the desktop where
/// the button comes up (peek.rs).
#[tauri::command]
fn peek_drag(app: tauri::AppHandle, dx: f64, dy: f64) {
    peek::drag(&app, dx, dy);
}

/// Keeps the session the peek shows on the desktop, in a window of its own where the peek is
/// (kept.rs): its Keep button, or its edge dragged and let go. Its header dragged and let go
/// keeps it from the engine (`peek::drag`).
#[tauri::command]
fn keep_session(app: tauri::AppHandle) -> Result<(), String> {
    kept::keep_peeked(&app).map_err(words)
}

/// The session a kept window shows, for its page.
#[tauri::command]
fn kept_session(app: tauri::AppHandle, window: tauri::Window) -> Option<String> {
    kept::session(&app, window.label())
}

/// A kept session's page has laid it out: its window shows.
#[tauri::command]
fn kept_ready(app: tauri::AppHandle, window: tauri::Window) {
    kept::ready(&app, window.label());
}

/// A click in a kept session: it takes the keyboard.
#[tauri::command]
fn kept_focus(app: tauri::AppHandle, window: tauri::Window) {
    kept::take_keys(&app, window.label());
}

/// You're dragging a kept session's edge: its window takes that size.
#[tauri::command]
fn kept_resize(app: tauri::AppHandle, window: tauri::Window, width: f64, height: f64) {
    kept::resize(&app, window.label(), width, height);
}

/// Takes a kept session off the desktop; that closes the window it came from.
#[tauri::command(async)]
fn kept_close(app: tauri::AppHandle, window: tauri::Window) {
    kept::close(&app, window.label());
}

/// Pops a session out of the window that asked (its panel in the main window) into a window of
/// its own beside it (kept.rs `pop_session`), or brings the one it has forward, with the keyboard
/// if `keys`.
#[tauri::command]
fn session_window(app: tauri::AppHandle, window: tauri::WebviewWindow, key: String, keys: Option<bool>) -> Result<(), String> {
    kept::pop_session(&app, key, panel::frame(&window), keys.unwrap_or(false)).map_err(words)
}

/// Puts a session kept in a window of its own back: the window closes, and with `reveal` (as
/// `show_main` takes it) the main window comes forward on it. Its own window asks for both
/// together, so the main window hears it's no longer kept before it's asked to show it.
#[tauri::command]
fn session_put_back(app: tauri::AppHandle, key: String, reveal: Option<serde_json::Value>) -> Result<(), String> {
    kept::put_back_session(&app, &key);
    match reveal {
        Some(reveal) => front::show_main(&app, Some(reveal)).map_err(words),
        None => Ok(()),
    }
}

/// The sessions kept in windows of their own; `session-windows` events say when that changes.
#[tauri::command]
fn session_windows(app: tauri::AppHandle) -> Vec<String> {
    kept::sessions(&app)
}

/// Pops a session's side conversation out into a window of its own beside the window that asked
/// (kept.rs `pop_side`), or brings the one it has forward, with the keyboard if `keys`.
#[tauri::command]
fn side_window(app: tauri::AppHandle, window: tauri::WebviewWindow, key: String, keys: Option<bool>) -> Result<(), String> {
    kept::pop_side(&app, key, panel::frame(&window), keys.unwrap_or(false)).map_err(words)
}

/// The sessions whose side conversations have windows of their own; `side-windows` events say
/// when that changes.
#[tauri::command]
fn side_windows(app: tauri::AppHandle) -> Vec<String> {
    kept::sides(&app)
}

/// Pops a session's terminal out of the drawer under its box into a window of its own beside the
/// window that asked (kept.rs `pop_terminal`), or brings the one it has forward, with the keyboard
/// if `keys`.
#[tauri::command]
fn terminal_window(app: tauri::AppHandle, window: tauri::WebviewWindow, key: String, id: String, keys: Option<bool>) -> Result<(), String> {
    kept::pop_terminal(&app, key, id, panel::frame(&window), keys.unwrap_or(false)).map_err(words)
}

/// Puts a popped-out terminal back in its drawer: its window closes.
#[tauri::command(async)]
fn terminal_put_back(app: tauri::AppHandle, id: String) {
    kept::put_back_terminal(&app, &id);
}

/// The terminals that have windows of their own; `terminal-windows` events say when that changes.
#[tauri::command]
fn terminal_windows(app: tauri::AppHandle) -> Vec<String> {
    kept::terminals(&app)
}

/// The terminal a kept window shows, for its page.
#[tauri::command]
fn kept_terminal(app: tauri::AppHandle, window: tauri::Window) -> Option<String> {
    kept::terminal(&app, window.label())
}

/// What the floating bar shows, for its first render; after that it gets `bar-feed` events.
#[tauri::command(async)]
fn bar_feed(mirror: State<'_, view::Mirror>, floating: State<'_, Floating>) -> barfeed::BarFeed {
    mirror.read(|b| floating.read(b, barfeed::feed))
}

/// What the windows that float are cut from beside each board: your other Macs' worktrees
/// (shown.rs `Elsewhere`), and the sound that goes with the bar's feed (sounds.rs). A board and
/// those worktrees each come on their own, so each publishes the bar's feed, the desktop cards and
/// the lines between them with the other's latest (`publish`), under this one lock, so the bar and
/// its sounds go by the feeds in the order they were cut.
#[derive(Clone, Default)]
struct Floating(std::sync::Arc<std::sync::Mutex<(shown::Elsewhere, sounds::Chimer)>>);

impl Floating {
    /// Makes `change` to your other Macs' worktrees, then, with a board, cuts the bar's feed, plays
    /// the sound for what came to wait on you by then (`now`, `Chimer::due`), and sends the bar,
    /// the desktop cards and the lines between them what they show.
    fn publish(&self, app: &tauri::AppHandle, board: Option<&model::Board>, now: i64, change: impl FnOnce(&mut shown::Elsewhere)) {
        let Ok(mut held) = self.0.lock() else { return };
        let (elsewhere, chimer) = &mut *held;
        change(elsewhere);
        let Some(board) = board else { return };
        let shown = elsewhere.beside(board);
        let feed = barfeed::feed(shown);
        // On a thread of its own, since the first one waits on macOS's audio to start.
        if let Ok(cfg) = config::current() {
            if let Some(kind) = chimer.due(board, &feed, &cfg.sounds, now) {
                let (name, volume) = (kind.sound(&cfg.sounds).to_string(), cfg.sounds.volume);
                std::thread::spawn(move || {
                    if let Err(e) = sounds::play(&name, volume) {
                        eprintln!("Couldn't play the sound for {kind:?}: {e}");
                    }
                });
            }
        }
        let hosted = board.lanes.iter().flat_map(|l| &l.sessions).filter(|s| s.hosted && s.live).count();
        menubar::publish(app, &feed, hosted);
        bar::publish(app, feed);
        desktop::publish(app, shown);
        lineage::publish(app, shown);
    }

    /// What the windows show with `board` (shown.rs), for a command that cuts something of it.
    /// The board's lock is taken before this one, as the engine's publish takes them.
    fn read<T>(&self, board: &model::Board, f: impl FnOnce(shown::Shown) -> T) -> T {
        match self.0.lock() {
            Ok(held) => f(held.0.beside(board)),
            Err(_) => f(shown::Shown::here(board)),
        }
    }
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
fn desktop_feed(app: tauri::AppHandle, window: tauri::Window, mirror: State<'_, view::Mirror>, floating: State<'_, Floating>) -> Option<desktop::Feed> {
    mirror.read(|b| floating.read(b, |shown| desktop::feed_for(&app, window.label(), shown)))
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
/// back after a failed start. `improve: true` opens it to improve Codealong itself, with pictures
/// of the windows (improve.rs).
#[tauri::command]
fn show_quick(app: tauri::AppHandle, context: Option<serde_json::Value>) {
    let context = context.unwrap_or(serde_json::Value::Null);
    if context.get("improve").and_then(serde_json::Value::as_bool) == Some(true) {
        improve::open(&app, context);
    } else {
        quick::open(&app, context);
    }
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

/// Brings up the window that works through what waits on you (waiting.rs), on a session or PR
/// waiting on your review when the context names one (`key`).
#[tauri::command]
fn show_waiting(app: tauri::AppHandle, context: Option<serde_json::Value>) {
    waiting::open(&app, context.unwrap_or(serde_json::Value::Null));
}

/// What the latest open of the waiting window asked for, for its page as it comes up.
#[tauri::command]
fn waiting_asked(app: tauri::AppHandle) -> serde_json::Value {
    waiting::asked(&app)
}

/// Starts a session from the quick panel, which has already hidden itself, then asks macOS to
/// bring the app it started in forward: the panel never makes Codealong the active app. A
/// worktree on another device starts there, where whatever it opens comes forward.
#[tauri::command]
async fn start_from_quick(app: tauri::AppHandle, mirror: State<'_, view::Mirror>, spec: start::StartSpec) -> Result<start::Started, String> {
    let mirror = mirror.inner().clone();
    blocking(move || {
        let started: start::Started = view::call_as("start_from_quick", serde_json::json!({ "spec": spec }))?;
        if peerfeed::split_remote(&spec.lane_id).is_none() {
            if let Some(bundle) = quick::start_app(spec.place, &mirror.read(|b| b.apps.terminal_id.clone())) {
                front::bring_forward(&app, bundle);
            }
        }
        Ok(started)
    })
    .await?
}

/// One of the shortcuts that work in every app (the quick panel's when `which` is left out) and
/// why it isn't working, if it isn't.
#[tauri::command(async)]
fn quick_shortcut(app: tauri::AppHandle, which: Option<quick::Keys>) -> quick::ShortcutStatus {
    quick::status(&app, which.unwrap_or(quick::Keys::Quick))
}

/// Turns the shortcuts off while Settings records a new one, and back on.
#[tauri::command(async)]
fn pause_quick_shortcut(app: tauri::AppHandle, paused: bool) {
    quick::pause_shortcuts(&app, paused)
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
    bar::set_shape(&app, geometry::PageRect { x, y, w, h });
}

/// The notch itself is being dragged, `dx` across since the button went down: the engine takes
/// the docked bar where the pointer goes and docks it on the screen it's let go on. Whether it
/// started: only a docked bar is dragged.
#[tauri::command]
fn notch_drag(app: tauri::AppHandle, dx: f64) -> bool {
    bar::drag(&app, dx)
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

/// The bar docked in the notch shows a session under its pills and takes the keyboard for it, or
/// has put it away and hands the keyboard back to the app you're in.
#[tauri::command]
fn notch_keys(app: tauri::AppHandle, on: bool) {
    bar::set_keys(&app, on);
}

/// Keeps the session the docked bar shows on the desktop, in a window of its own where its card
/// is (kept.rs): the card as the bar's page measured it, in its window's points from the top left.
#[tauri::command]
fn keep_notch_session(app: tauri::AppHandle, window: tauri::WebviewWindow, key: String, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    let host = panel::frame(&window).ok_or_else(|| "The bar isn't there.".to_string())?;
    let card = geometry::PageRect { x, y, w, h }.to_screen(host);
    kept::keep(&app, key, card, true).map(|_| ()).map_err(words)
}

/// Opens a PR in Codex's review.
#[tauri::command]
async fn review_in_codex(url: String) -> Result<(), String> {
    blocking(move || launch::open_codex(&launch::codex_review_url(&url)).map_err(words)).await?
}

/// Opens a web link (in a conversation, a PR's description) in the browser: one into a
/// worktree's app in the Chrome profile its preview opens in (`engine::open_link`).
#[tauri::command]
async fn open_url(mirror: State<'_, view::Mirror>, url: String) -> Result<(), String> {
    let mirror = mirror.inner().clone();
    blocking(move || {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err("Only web links open from here.".into());
        }
        mirror.read(|b| engine::open_link(b, &url)).map_err(words)
    })
    .await?
}

/// Opens a schedule a harness keeps (schedules/harnesses.rs `Kept::link`) in its app. It can wait
/// on the app to come forward.
#[tauri::command]
async fn open_harness_link(harness: model::Harness, url: String) -> Result<(), String> {
    blocking(move || harness::adapter(harness).open_link(&url).map_err(words)).await?
}

/// Opens one of a PR's changed files in your editor, as a diff or at a line (prdiff.rs). It can
/// wait on GitHub for the file's two sides.
#[tauri::command]
async fn open_pr_file(mirror: State<'_, view::Mirror>, ask: prdiff::Ask) -> Result<(), String> {
    let mirror = mirror.inner().clone();
    blocking(move || {
        let place = mirror.read(|b| prdiff::place(b, &ask.pr)).map_err(words)?;
        let yours = config::current().map_err(words)?.apps.editor;
        prdiff::open(place, &yours, &ask).map_err(words)
    })
    .await?
}

/// Runs a command you ran for a session (led by `!` in its box, or with `block` Run on a shell
/// block in its conversation) in a terminal under the session, or a new tab of your terminal as
/// `[apps] run_in` says, in `folder`, and has the session told what it printed once it ends
/// (actions.rs `run_command`, run by the server). Says how it went, in a sentence.
#[tauri::command]
async fn run_command(app: tauri::AppHandle, mirror: State<'_, view::Mirror>, key: String, folder: String, text: String, block: bool) -> Result<String, String> {
    let mirror = mirror.inner().clone();
    blocking(move || {
        let said: String = view::call_as("run_command", serde_json::json!({ "key": key, "folder": folder, "text": text, "block": block }))?;
        // In a terminal of Codealong's own it shows under the session (terminal.rs `start`), and
        // your terminal app stays where it is.
        if mirror.read(|b| b.apps.run_in) == config::RunIn::Terminal {
            bring_terminal_forward(&app, &mirror, &key);
        }
        Ok(said)
    })
    .await?
}

/// Brings your terminal forward after the server opened a tab in it for session `key`: a program in
/// the background, the server can't bring an app forward, where the app you clicked in can
/// (front.rs `bring_forward`). Not for a session on another device, which opens there.
fn bring_terminal_forward(app: &tauri::AppHandle, mirror: &view::Mirror, key: &str) {
    if peerfeed::split_remote(key).is_some() {
        return;
    }
    if let Some(bundle) = launch::bundle_id(&mirror.read(|b| b.apps.terminal_id.clone())) {
        front::bring_forward(app, bundle);
    }
}

/// Goes on with a session that isn't running where `resume` says, as the server does it
/// (calls.rs `resume_session`), then brings forward the app it went on in.
#[tauri::command]
async fn resume_session(app: tauri::AppHandle, mirror: State<'_, view::Mirror>, key: String, resume: start::ResumeIn, prompt: Option<String>) -> Result<serde_json::Value, String> {
    let mirror = mirror.inner().clone();
    blocking(move || {
        let place = resume.place;
        let said = view::call("resume_session", serde_json::json!({ "key": key, "resume": resume, "prompt": prompt }))?;
        if peerfeed::split_remote(&key).is_none() {
            if let Some(bundle) = quick::start_app(place, &mirror.read(|b| b.apps.terminal_id.clone())) {
                front::bring_forward(&app, bundle);
            }
        }
        Ok(said)
    })
    .await?
}

/// Forks a session as the server does it (calls.rs `fork_session`), then brings your terminal
/// forward for a copy that went on in one: neither `background` nor from a prompt's cursor, whose
/// copies run out of sight or in Codealong.
#[tauri::command]
async fn fork_session(app: tauri::AppHandle, mirror: State<'_, view::Mirror>, key: String, background: Option<bool>, prompt: Option<String>, from: Option<u64>, side: Option<String>) -> Result<start::Started, String> {
    let mirror = mirror.inner().clone();
    blocking(move || {
        let in_a_terminal = from.is_none() && background != Some(true);
        let started: start::Started = view::call_as("fork_session", serde_json::json!({ "key": key, "background": background, "prompt": prompt, "from": from, "side": side }))?;
        if in_a_terminal {
            bring_terminal_forward(&app, &mirror, &key);
        }
        Ok(started)
    })
    .await?
}

/// Signs an account in, in a new terminal the server opens, then brings that terminal forward.
#[tauri::command]
async fn sign_in(app: tauri::AppHandle, mirror: State<'_, view::Mirror>, name: String) -> Result<(), String> {
    let mirror = mirror.inner().clone();
    blocking(move || {
        view::call("sign_in", serde_json::json!({ "name": name }))?;
        if let Some(bundle) = launch::bundle_id(&mirror.read(|b| b.apps.terminal_id.clone())) {
            front::bring_forward(&app, bundle);
        }
        Ok(())
    })
    .await?
}

/// Plays a sound `[sounds]` can name, at a volume in percent: Settings' play button.
#[tauri::command]
async fn play_sound(name: String, volume: u32) -> Result<(), String> {
    blocking(move || sounds::play(&name, volume).map_err(words)).await?
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

/// Whether the app opens when you log in, as the system has it (os/login): Settings › General.
#[tauri::command]
async fn open_at_login(app: tauri::AppHandle) -> Result<os::login::AtLogin, String> {
    blocking(move || os::login::status(&app)).await
}

/// Has the app open when you log in, or not; how it stands after.
#[tauri::command]
async fn set_open_at_login(app: tauri::AppHandle, on: bool) -> Result<os::login::AtLogin, String> {
    blocking(move || os::login::set(&app, on).map_err(words)).await?
}

/// Whether a terminal's app is kept awake behind other windows (os/naps.rs), as the system has
/// it: Settings › General. None for one macOS doesn't nap: tmux, or any off macOS.
#[tauri::command]
async fn terminal_awake(terminal: String) -> Option<bool> {
    blocking(move || launch::bundle_id(&terminal).and_then(os::naps::awake)).await.ok().flatten()
}

/// Keeps a terminal's app awake behind other windows, or lets macOS nap it again, from the next
/// time it opens; how it stands after.
#[tauri::command]
async fn set_terminal_awake(terminal: String, awake: bool) -> Result<Option<bool>, String> {
    blocking(move || {
        let bundle = launch::bundle_id(&terminal).ok_or_else(|| format!("{} isn't an app macOS naps.", launch::terminal_name(&terminal)))?;
        os::naps::keep_awake(bundle, awake).map_err(words)?;
        Ok(os::naps::awake(bundle))
    })
    .await?
}

/// Opens a repo's `.codealong/config.toml` from its main checkout in the editor.
#[tauri::command]
async fn open_team_file(mirror: State<'_, view::Mirror>, repo: String) -> Result<(), String> {
    let mirror = mirror.inner().clone();
    blocking(move || mirror.read(|b| engine::open_team_file(b, &repo)).map_err(words)).await?
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

/// Starts a session in a worktree, as the server does it (calls.rs `start_session`), then brings
/// the app it started in forward, as an open from the main window does (`open_target`).
#[tauri::command]
async fn start_session(app: tauri::AppHandle, mirror: State<'_, view::Mirror>, spec: start::StartSpec) -> Result<start::Started, String> {
    let mirror = mirror.inner().clone();
    blocking(move || {
        let started: start::Started = view::call_as("start_session", serde_json::json!({ "spec": spec }))?;
        if peerfeed::split_remote(&spec.lane_id).is_none() {
            if let Some(bundle) = quick::start_app(spec.place, &mirror.read(|b| b.apps.terminal_id.clone())) {
                front::bring_forward(&app, bundle);
            }
        }
        Ok(started)
    })
    .await?
}

/// Moves a session to another account of its harness, or with `lane` to that worktree, as the
/// server does it (calls.rs `move_session`). Its card on the desktop and its kept window show the
/// copy once the board has it, so neither says it's gone meanwhile.
#[tauri::command]
async fn move_session(app: tauri::AppHandle, mirror: State<'_, view::Mirror>, key: String, account: Option<String>, lane: Option<String>) -> Result<start::Started, String> {
    let mirror = mirror.inner().clone();
    blocking(move || {
        let started: start::Started = view::call_as("move_session", serde_json::json!({ "key": key, "account": account, "lane": lane }))?;
        // It goes on in a new terminal.
        bring_terminal_forward(&app, &mirror, &key);
        if let Some(copy) = started.key.clone().filter(|c| *c != key) {
            std::thread::spawn(move || {
                // Its terminal takes a moment to start Claude Code, which then says it's there.
                let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
                while !mirror.read(|b| b.session(&copy).is_some()) {
                    if std::time::Instant::now() > until {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
                desktop::follow(&app, &key, &copy);
                kept::follow(&app, &key, &copy);
            });
        }
        Ok(started)
    })
    .await?
}

/// The main window's answer to a `codealong board …` request (boards.rs `ask`).
#[tauri::command]
fn board_answer(id: u64, result: serde_json::Value, error: Option<String>) {
    boards::answer(id, error.map_or(Ok(result), Err));
}

/// A schedule's time as it's being written: in words, and its next few times.
#[tauri::command]
fn schedule_times(rule: String) -> schedules::Times {
    schedules::times(&rule, util::now_ms(), &schedules::rule::Local)
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
    attachments::keep(bytes, &name).map_err(words)
}

/// Keeps a copy of a file dropped from the Finder.
#[tauri::command(async)]
fn import_attachment(path: String) -> Result<attachments::Attachment, String> {
    attachments::import(std::path::Path::new(&path)).map_err(words)
}

/// Text with its attachments as full paths, for copying a todo as a prompt.
#[tauri::command]
fn expand_attachments(text: String) -> String {
    attachments::expand(&text, attachments::Style::Path)
}

/// Opens a kept attachment in the app that opens its kind (Preview for an image, say).
#[tauri::command]
async fn open_attachment(path: String) -> Result<(), String> {
    blocking(move || {
        let file = attachments::locate(&path).ok_or("That isn't one of Codealong's attachments.")?;
        if !file.exists() {
            return Err("That attachment is gone: it's cleared 30 days after no todo uses it.".into());
        }
        launch::open_url(&file.to_string_lossy()).map_err(words)
    })
    .await?
}

/// Opens a file a conversation names on this Mac: one an agent sent you, a file Codex cites, or a
/// path it writes (`~/` is home). Something opening would run is shown in the Finder instead
/// (`launch::open_file`).
#[tauri::command]
async fn open_file(path: String) -> Result<(), String> {
    blocking(move || launch::open_file(&util::expand(&path)).map_err(words)).await?
}

/// Shows a file a conversation names in the Finder, selected.
#[tauri::command]
async fn reveal_file(path: String) -> Result<(), String> {
    blocking(move || launch::reveal_file(&util::expand(&path)).map_err(words)).await?
}

/// Opens a file a conversation names in your editor, at `line` where the path gave one.
#[tauri::command]
async fn edit_file(mirror: State<'_, view::Mirror>, path: String, line: Option<u64>) -> Result<(), String> {
    let editor = mirror.read(engine::your_editor).id;
    blocking(move || launch::edit_file(&editor, &util::expand(&path), line).map_err(words)).await?
}

/// Your editor, which a file's menu names in Open in.
#[tauri::command]
fn your_editor(mirror: State<'_, view::Mirror>) -> config::Profile {
    mirror.read(engine::your_editor)
}

/// What each path a conversation names is on this Mac, a file, a folder or nothing, which the
/// windows ask before making one open on a click. Off the main thread: a path on a network volume
/// can take its time to look up.
#[tauri::command]
async fn path_kinds(paths: Vec<String>) -> Result<Vec<Option<launch::PathKind>>, String> {
    blocking(move || launch::path_kinds(&paths)).await
}

/// Serves kept images to the windows at `codealong-image://localhost/<name>`, and those a session
/// on another device refers to at `…/<name>?from=<its short id>`: kept here once that device has
/// sent them (`actions::image_from`). Only names Codealong made are read, so nothing outside the
/// images folder can be.
fn serve_image( request: tauri::http::Request<Vec<u8>>, responder: tauri::UriSchemeResponder) {
    let name = request.uri().path().trim_start_matches('/').to_string();
    let from = request.uri().query().and_then(|q| q.split('&').find_map(|p| p.strip_prefix("from="))).map(String::from);
    std::thread::spawn(move || {
        let path = format!("{}{name}", attachments::IMAGES);
        let here = attachments::locate(&path).and_then(|p| std::fs::read(p).ok());
        // The server fetches it from that device and keeps it here (calls.rs `image_from`).
        let found = here.or_else(|| {
            let device = from.filter(|_| attachments::valid_image(&name))?;
            view::call("image_from", serde_json::json!({ "device": device, "path": path })).ok()?;
            attachments::locate(&path).and_then(|p| std::fs::read(p).ok())
        });
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

/// The app's own appearance for a theme: `None` lets macOS decide. Window chrome, native menus
/// and each page's `prefers-color-scheme` all go by it.
fn native_theme(theme: config::Theme) -> Option<tauri::Theme> {
    match theme {
        config::Theme::System => None,
        config::Theme::Light => Some(tauri::Theme::Light),
        config::Theme::Dark => Some(tauri::Theme::Dark),
    }
}

/// Whether a window shows the board: the main one, or a project's. Only these take the menu's
/// commands.
fn shows_board(label: &str) -> bool {
    label == "main" || label.starts_with("project-")
}

/// The board window in front, which a menu command is for. It goes by which windows show the
/// board, not by which don't: the bar, the peek, the cards pinned to the desktop, the lines
/// between them and the sessions kept there say they're key while they aren't, and the windows
/// come in no set order, so one of those left in took the command from the window in front about
/// half the time (the lines' window did: Close idle terminals… opened nothing).
fn front_window(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    app.webview_windows().into_values().find(|w| shows_board(w.label()) && w.is_focused().unwrap_or(false))
}

/// Puts Settings… (⌘,) in the app menu, under About. Choosing it tells the front window to
/// open Settings; without one, the main window comes back and opens it.
fn add_settings_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuItem, MenuItemKind, PredefinedMenuItem};
    let Some(menu) = app.menu() else { return Ok(()) };
    let Some(MenuItemKind::Submenu(first)) = menu.items()?.into_iter().next() else { return Ok(()) };
    let item = MenuItem::with_id(app, SETTINGS_MENU_ID, "Settings…", true, Some("CmdOrCtrl+,"))?;
    let setup = MenuItem::with_id(app, SETUP_MENU_ID, "Set up Codealong…", true, None::<&str>)?;
    first.insert(&item, 2)?;
    first.insert(&setup, 3)?;
    first.insert(&PredefinedMenuItem::separator(app)?, 4)?;
    app.on_menu_event(|app, event| {
        // The setup wizard is a dialog over the main window, whichever window is in front.
        if event.id().as_ref() == SETUP_MENU_ID {
            let _ = front::show_main(app, None);
            let _ = app.emit_to("main", SETUP_MENU_ID, ());
            return;
        }
        if event.id().as_ref() != SETTINGS_MENU_ID {
            return;
        }
        // `emit_to`, not a window's `emit`: that one reaches every window.
        match front_window(app) {
            Some(w) => {
                let _ = app.emit_to(w.label(), "settings", ());
            }
            None => {
                let _ = front::show_main(app, None);
                let _ = app.emit_to("main", "settings", ());
            }
        }
    });
    Ok(())
}

const SETTINGS_MENU_ID: &str = "settings";
const SETUP_MENU_ID: &str = "setup";

/// Puts New window (⌘⇧N) at the top of the File menu, and Close idle sessions… and Close stale
/// terminals… under it. Choosing New window asks the front window for another window on the
/// project it shows; without one (closing the main window only hides it), the main window comes
/// back. The menu's shortcut works with no window open, where a page's own keys can't.
fn add_new_window_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuItem, MenuItemKind, PredefinedMenuItem};
    let Some(menu) = app.menu() else { return Ok(()) };
    let file = menu.items()?.into_iter().find_map(|item| match item {
        MenuItemKind::Submenu(s) if s.text().is_ok_and(|t| t == "File") => Some(s),
        _ => None,
    });
    let Some(file) = file else { return Ok(()) };
    let item = MenuItem::with_id(app, NEW_WINDOW_MENU_ID, "New window", true, Some("CmdOrCtrl+Shift+N"))?;
    let close_idle = MenuItem::with_id(app, CLOSE_IDLE_MENU_ID, "Close idle sessions…", true, None::<&str>)?;
    let close_stale = MenuItem::with_id(app, CLOSE_STALE_MENU_ID, "Close stale terminals…", true, None::<&str>)?;
    file.prepend_items(&[&item, &PredefinedMenuItem::separator(app)?, &close_idle, &close_stale, &PredefinedMenuItem::separator(app)?])?;
    app.on_menu_event(|app, event| {
        let id = event.id().as_ref();
        if id != NEW_WINDOW_MENU_ID && id != CLOSE_IDLE_MENU_ID && id != CLOSE_STALE_MENU_ID {
            return;
        }
        match front_window(app) {
            Some(w) => {
                let _ = app.emit_to(w.label(), id, ());
            }
            None => {
                let _ = front::show_main(app, None);
                // Close idle sessions and Close stale terminals are dialogs, so the main window
                // opens them.
                if id != NEW_WINDOW_MENU_ID {
                    let _ = app.emit_to("main", id, ());
                }
            }
        }
    });
    Ok(())
}

/// Puts Codealong Website in the Help menu, which is empty on macOS (Tauri makes no menu on
/// Linux): the front board window opens the website (src/App.tsx), or the main window when none
/// is open, so a browser that won't open is a toast there.
fn add_help_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuItem, MenuItemKind, HELP_SUBMENU_ID};
    let Some(menu) = app.menu() else { return Ok(()) };
    let help = menu.items()?.into_iter().find_map(|item| match item {
        MenuItemKind::Submenu(s) if s.id().as_ref() == HELP_SUBMENU_ID => Some(s),
        _ => None,
    });
    let Some(help) = help else { return Ok(()) };
    help.append(&MenuItem::with_id(app, WEBSITE_MENU_ID, "Codealong Website", true, None::<&str>)?)?;
    app.on_menu_event(|app, event| {
        if event.id().as_ref() != WEBSITE_MENU_ID {
            return;
        }
        match front_window(app) {
            Some(w) => {
                let _ = app.emit_to(w.label(), WEBSITE_MENU_ID, ());
            }
            None => {
                let _ = front::show_main(app, None);
                let _ = app.emit_to("main", WEBSITE_MENU_ID, ());
            }
        }
    });
    Ok(())
}

const WEBSITE_MENU_ID: &str = "website";

const NEW_WINDOW_MENU_ID: &str = "new-window";
const CLOSE_IDLE_MENU_ID: &str = "close-idle";
const CLOSE_STALE_MENU_ID: &str = "close-stale";

/// Puts Usage… (⇧⌘U) at the foot of the View menu: the window that says what sessions used and
/// cost (src/ui/UsageDialog.tsx), a dialog the front board window opens, or the main window when
/// none is open.
fn add_usage_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuItem, MenuItemKind, PredefinedMenuItem};
    let Some(menu) = app.menu() else { return Ok(()) };
    let view = menu.items()?.into_iter().find_map(|item| match item {
        MenuItemKind::Submenu(s) if s.text().is_ok_and(|t| t == "View") => Some(s),
        _ => None,
    });
    let Some(view) = view else { return Ok(()) };
    let item = MenuItem::with_id(app, USAGE_MENU_ID, "Usage…", true, Some("CmdOrCtrl+Shift+U"))?;
    view.append_items(&[&PredefinedMenuItem::separator(app)?, &item])?;
    app.on_menu_event(|app, event| {
        if event.id().as_ref() != USAGE_MENU_ID {
            return;
        }
        match front_window(app) {
            Some(w) => {
                let _ = app.emit_to(w.label(), USAGE_MENU_ID, ());
            }
            None => {
                let _ = front::show_main(app, None);
                let _ = app.emit_to("main", USAGE_MENU_ID, ());
            }
        }
    });
    Ok(())
}

const USAGE_MENU_ID: &str = "usage";

/// Puts Reload (⌘R) at the top of the View menu, over the zoom, as a browser has it: the front
/// board window reads again what it shows (src/reload.ts), else the waiting window when it's in
/// front (an ordinary window, key when it says so, waiting.rs), else the main window, shown. The
/// key reaches the page before the menu (zoom.rs `add_menu`), and every window that shows
/// something to read again takes it there, the ones that float too.
fn add_reload_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuItem, MenuItemKind, PredefinedMenuItem};
    let Some(menu) = app.menu() else { return Ok(()) };
    let view = menu.items()?.into_iter().find_map(|item| match item {
        MenuItemKind::Submenu(s) if s.text().is_ok_and(|t| t == "View") => Some(s),
        _ => None,
    });
    let Some(view) = view else { return Ok(()) };
    let item = MenuItem::with_id(app, RELOAD_MENU_ID, "Reload", true, Some("CmdOrCtrl+R"))?;
    view.prepend_items(&[&item, &PredefinedMenuItem::separator(app)?])?;
    app.on_menu_event(|app, event| {
        if event.id().as_ref() != RELOAD_MENU_ID {
            return;
        }
        let waiting = || app.get_webview_window(waiting::LABEL).filter(|w| w.is_focused().unwrap_or(false));
        match front_window(app).or_else(waiting) {
            Some(w) => {
                let _ = app.emit_to(w.label(), RELOAD_MENU_ID, ());
            }
            None => {
                let _ = front::show_main(app, None);
                let _ = app.emit_to("main", RELOAD_MENU_ID, ());
            }
        }
    });
    Ok(())
}

const RELOAD_MENU_ID: &str = "reload";

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

/// The app's own commands, handed to `$then`: `tauri::generate_handler` for the handler, and
/// `names_of` for their names. A window's command that isn't one of these goes to the server when
/// it's one of its calls (`forward`).
macro_rules! app_commands {
    ($($then:tt)*) => {
        $($then)*![get_board, open_target, open_config, editor_icons, open_from_bar, peek_open, pr_at, session_at, perf_mark, page_unfocused, window_keys, pointer_probe, popup_menu, fit_bar, show_main, take_opened_link, peek_ready, peek_fit, peek_focus, peek_hide, peek_resize, peek_drag, keep_session, kept_session, kept_ready, kept_focus, kept_resize, kept_close, session_window, session_put_back, session_windows, side_window, side_windows, terminal_window, terminal_put_back, terminal_windows, kept_terminal, bar_feed, desktop_pins, pin_to_desktop, drag_to_desktop, unpin_from_desktop, desktop_feed, fit_desktop, desktop_hover, lineage_feed, show_quick, hide_quick, quick_ready, show_waiting, waiting_asked, start_from_quick, quick_shortcut, pause_quick_shortcut, bar_on, set_bar_on, bar_mode, notch_shape, notch_drag, set_bar_mode, set_notch_open, notch_keys, keep_notch_session, review_in_codex, open_url, open_harness_link, open_pr_file, run_command, play_sound, update_status, check_for_updates, restart_to_update, open_at_login, set_open_at_login, terminal_awake, set_terminal_awake, open_team_file, link_code_qr, start_session, move_session, resume_session, fork_session, sign_in, board_answer, schedule_times, save_attachment, import_attachment, expand_attachments, open_attachment, open_file, reveal_file, edit_file, your_editor, path_kinds, open_project_window]
    };
}

macro_rules! names_of {
    ($($name:ident),* $(,)?) => {
        &[$(stringify!($name)),*]
    };
}

/// The app's own commands, by name.
const APP_COMMANDS: &[&str] = app_commands!(names_of);

/// Hands a window's command to the server (calls.rs), with the label of the window it came from,
/// and answers the window with what the server says, as the command itself would: on a thread of
/// its own, since the server may take a while.
fn forward(invoke: tauri::ipc::Invoke) {
    let name = invoke.message.command().to_string();
    let mut args = match invoke.message.payload() {
        tauri::ipc::InvokeBody::Json(v) if v.is_object() => v.clone(),
        _ => serde_json::json!({}),
    };
    if let Some(map) = args.as_object_mut() {
        map.insert("windowLabel".into(), invoke.message.webview_ref().label().into());
    }
    let resolver = invoke.resolver;
    std::thread::spawn(move || match view::call(&name, args) {
        Ok(answer) => resolver.resolve(answer),
        Err(e) => resolver.reject(e),
    });
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let plainly = server::opened_plainly(&args);
    if plainly {
        if let Some(code) = install::open_as_the_dock_does() {
            std::process::exit(code);
        }
    }
    perf::arm();
    // The settings the app runs on, before anything reads them (config.rs `follow_file`).
    config::follow_file();
    // When this app opened: of two, the one opened last stays (views.rs `serve`).
    let opened = util::now_ms();
    if let Err(e) = view::take_over(plainly, opened) {
        eprintln!("{e}");
        std::process::exit(2);
    }
    tauri::Builder::default()
        .setup(move |app| {
            crate::os::system::no_app_nap();
            // Set before the page loads, so a window never opens in the wrong theme.
            let theme = config::theme();
            app.set_theme(native_theme(theme));
            // And at its zoom: the main window is made before this runs.
            zoom::follow(app.handle(), config::zoom());
            // And where it was left, which brings it up.
            frames::setup(app.handle());
            // Codealong and Codealong Dev both say they open Codealong's links (Info.plist), and
            // left to itself macOS picks the one it registered last: this build of the repo says
            // each time it launches that it's the one, so a link clicked where both are installed
            // never opens the other, which would take over from the one running (view.rs
            // `take_over`). Codealong never says so: alone, it's the only one there is.
            if is_dev_build(app.handle()) {
                if let Some(bundle) = serve::this_binary().filter(|exe| install::in_a_bundle(exe)).and_then(|exe| exe.ancestors().nth(3).map(std::path::Path::to_path_buf)) {
                    os::apps::claim_links(&bundle, front::LINK_SCHEME);
                }
            }
            let shown = std::sync::Mutex::new(theme);
            let shown_logo = std::sync::Mutex::new(model::LogoStatus::Idle);
            let floating = Floating::default();
            app.manage(floating.clone());
            let shortcuts = config::shortcuts();
            let shortcut = std::sync::Mutex::new(quick::Keys::ALL.map(|which| which.in_config(&shortcuts).to_string()));
            // Whether voice was on at the last board: on at launch, its model is loaded then.
            let voice = std::sync::atomic::AtomicBool::new(config::experimental().voice);
            // The board the server publishes, and what the app does with each: the Dock icon, the
            // theme, the zoom and the shortcuts, the windows' feed, and the windows that float.
            let mirror = view::Mirror::new();
            app.manage(mirror.clone());
            let emitter = app.handle().clone();
            let windows = std::sync::Mutex::new(windowfeed::Feed::default());
            let publishing = floating.clone();
            let on_board = move |board: &model::Board| {
                // The server turns the perf log on as config.toml says, and the app follows it
                // from the board, so the app's own lines go in too: its windows' (`perf_mark`)
                // and its main thread's lag, which went nowhere once the engine moved into the
                // server, since only the engine turned it on. A server running in the app shares
                // the switch, and its first board says off before it has read config.toml.
                if !view::server_in_app() {
                    perf::set_enabled(board.perf_log);
                }
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
                zoom::follow(&emitter, board.zoom);
                // Registering waits on the main thread, so it gets a thread of its own: this one
                // may hold what a command on the main thread is waiting for.
                // Voice turned on: its speech model is loaded (and downloaded, the first time),
                // so the first words said aren't kept waiting on it.
                if board.experimental.voice && !voice.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    std::thread::spawn(dictation::warm);
                } else if !board.experimental.voice {
                    voice.store(false, std::sync::atomic::Ordering::SeqCst);
                }
                if let Ok(mut shortcut) = shortcut.lock() {
                    for (which, was) in quick::Keys::ALL.into_iter().zip(shortcut.iter_mut()) {
                        let now = which.on_board(board);
                        if was != now {
                            *was = now.to_string();
                            let (app, wanted) = (emitter.clone(), now.to_string());
                            std::thread::spawn(move || quick::apply_shortcut(&app, which, &wanted));
                        }
                    }
                }
                // What changed since the board before, which each window puts back together
                // (windowfeed.rs); a window that misses one reads the board whole.
                if let Ok(mut feed) = windows.lock() {
                    let t = std::time::Instant::now();
                    match feed.push(board) {
                        Ok(push) => {
                            let _ = emitter.emit("board", &*push.json);
                            if perf::enabled() {
                                let what = if push.whole { "whole" } else { "what changed" };
                                perf::log(&format!("windows sent {:.1} KB, {what}: {} sessions in {} lanes, in {} ms", push.json.get().len() as f64 / 1024.0, push.sessions, push.lanes, t.elapsed().as_millis()));
                            }
                        }
                        Err(e) => eprintln!("Couldn't write the board out for the windows: {e}"),
                    }
                }
                // The bar with a sound for what came to wait on you (sounds.rs), the desktop cards
                // and the lines between them.
                publishing.publish(&emitter, Some(board), board.generated_at, |_| {});
            };
            // Your other devices: the windows that float show their worktrees beside the board.
            let (devices_app, devices_floating, devices_mirror) = (app.handle().clone(), floating.clone(), mirror.clone());
            let on_devices = move |lanes: &[model::Lane], view: serde_json::Value| {
                devices_mirror.read(|board| devices_floating.publish(&devices_app, Some(board), util::now_ms(), |e| e.on_devices(lanes)));
                let _ = devices_app.emit("devices", view);
            };
            let (showing, asking, events, connecting) = (app.handle().clone(), app.handle().clone(), app.handle().clone(), app.handle().clone());
            let hooks = view::Hooks {
                board: Box::new(on_board),
                devices: Box::new(on_devices),
                show: Box::new(move |reveal| {
                    if let Err(e) = front::show_main(&showing, Some(reveal)) {
                        eprintln!("Couldn't show it in the main window: {e:#}");
                    }
                }),
                // A board the `codealong` command asks about, which the main window works out.
                ask: std::sync::Arc::new(move |what, payload| match what {
                    "board-ask" => {
                        #[derive(serde::Deserialize)]
                        struct BoardAsk {
                            board: String,
                            project: String,
                            ask: boards::Ask,
                        }
                        let a: BoardAsk = serde_json::from_value(payload).map_err(|e| e.to_string())?;
                        boards::ask(&asking, a.board, a.project, a.ask)
                    }
                    other => Err(format!("This Codealong doesn't know how to answer {other}; update it.")),
                }),
                event: Box::new(move |name, payload| {
                    let _ = events.emit(name, payload);
                }),
                // As a copy taking over ends the process (frames.rs says what that means for the
                // windows' places, which are written as they change).
                quit: Box::new(|| {
                    eprintln!("Another Codealong opened, or its server was stopped: quitting.");
                    std::process::exit(0);
                }),
                connected: Box::new(move |_| menubar::connected(&connecting)),
            };
            // A bare binary isn't what its bundle ID opens: that's whichever bundle has it.
            let bundle = std::env::current_exe().ok().and_then(|exe| updates::bundle_of(&exe)).map(|_| app.config().identifier.clone());
            view::follow(mirror, view::info(bundle, opened), hooks);
            std::thread::spawn(|| {
                install::follow_command_link();
                install::refresh_installed();
            });
            // Before the bar, which may be in the menu bar.
            menubar::setup(app.handle());
            if let Err(e) = bar::setup(app.handle()) {
                eprintln!("Couldn't make the floating bar: {e:#}");
            }
            if let Err(e) = quick::setup(app.handle()) {
                eprintln!("Couldn't make the quick panel: {e:#}");
            }
            if let Err(e) = waiting::setup(app.handle()) {
                eprintln!("Couldn't add Waiting on you to the menu: {e:#}");
            }
            if let Err(e) = peek::setup(app.handle()) {
                eprintln!("Couldn't make the floating bar's peek: {e:#}");
            }
            lineage::setup(app.handle());
            if let Err(e) = desktop::setup(app.handle()) {
                eprintln!("Couldn't put the cards pinned to the desktop back: {e:#}");
            }
            if let Err(e) = kept::setup(app.handle()) {
                eprintln!("Couldn't put the sessions kept on the desktop back: {e:#}");
            }
            // Registering waits on the main thread, which this is.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let shortcuts = config::shortcuts();
                for which in quick::Keys::ALL {
                    quick::apply_shortcut(&handle, which, which.in_config(&shortcuts));
                }
                // The speech model, so the first words said aren't kept waiting on it.
                dictation::warm();
            });
            if let Err(e) = add_settings_menu(app.handle()) {
                eprintln!("Couldn't add Settings to the menu: {e:#}");
            }
            if let Err(e) = add_new_window_menu(app.handle()) {
                eprintln!("Couldn't add New window to the menu: {e:#}");
            }
            if let Err(e) = zoom::add_menu(app.handle()) {
                eprintln!("Couldn't add zooming to the menu: {e:#}");
            }
            if let Err(e) = add_reload_menu(app.handle()) {
                eprintln!("Couldn't add Reload to the menu: {e:#}");
            }
            if let Err(e) = add_usage_menu(app.handle()) {
                eprintln!("Couldn't add Usage to the menu: {e:#}");
            }
            if let Err(e) = add_help_menu(app.handle()) {
                eprintln!("Couldn't add the website to the Help menu: {e:#}");
            }
            updates::setup(app.handle());
            watch_main_thread(app.handle().clone());
            Ok(())
        })
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(|app, shortcut, event| {
            // Only the keys held to talk do anything as they come up (dictation.rs).
            match event.state {
                tauri_plugin_global_shortcut::ShortcutState::Pressed => quick::pressed(app, shortcut),
                tauri_plugin_global_shortcut::ShortcutState::Released => quick::released(app, shortcut),
            }
        }).build())
        .on_window_event(|window, event| {
            front::on_window_event(window, event);
            frames::on_window_event(window, event);
            bar::on_window_event(window, event);
            quick::on_window_event(window, event);
            waiting::on_window_event(window, event);
            peek::on_window_event(window, event);
            desktop::on_window_event(window, event);
            kept::on_window_event(window, event);
        })
        .register_asynchronous_uri_scheme_protocol("codealong-image", |_, request, responder| serve_image(request, responder))
        // A window's command is the app's own when it's one of these, and the server's when it's
        // one of its calls (calls.rs), which `forward` hands it.
        .invoke_handler({
            // Given where its type is known: Tauri's handler is a closure that can't say it itself.
            fn handler(f: impl Fn(tauri::ipc::Invoke) -> bool) -> impl Fn(tauri::ipc::Invoke) -> bool {
                f
            }
            let local = handler(app_commands!(tauri::generate_handler));
            move |invoke| {
                let theirs = {
                    let name = invoke.message.command();
                    !APP_COMMANDS.contains(&name) && calls::NAMES.contains(&name)
                };
                if theirs {
                    forward(invoke);
                    return true;
                }
                local(invoke)
            }
        })
        .build(tauri::generate_context!())
        .expect("error while running Codealong")
        .run(|app, event| {
            // A click, in another app, on a link to a session or a todo (front.rs `open_links`).
            let links = os::apps::opened_links(&event);
            if !links.is_empty() {
                front::open_links(app, &links);
            }
            // The Dock icon brings the main window back after it was closed.
            match event {
                ref reopen if os::windows::reopened(reopen) => {
                    let _ = front::show_main(app, None);
                }
                // Where the windows are, for a move too recent to have been written (frames.rs).
                tauri::RunEvent::Exit => {
                    frames::save(app);
                    waiting::save(app);
                }
                _ => {}
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

/// `codealong serve`: Codealong's server, which runs the engine without a window (serve.rs).
pub fn serve_main() -> i32 {
    serve::run()
}

/// `codealong --snapshot …`: the engine without a window, for checking what it does (snapshot.rs).
pub fn snapshot_cli(args: &[String]) -> i32 {
    snapshot::cli(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_waiting_command_runs_on_the_blocking_pool_and_says_when_it_broke() {
        let worker = std::thread::current().id();
        let (ran_on, n) = tauri::async_runtime::block_on(blocking(|| (std::thread::current().id(), 7))).unwrap();
        assert_eq!(n, 7);
        assert_ne!(ran_on, worker, "the body ran off the caller's thread");
        let err = tauri::async_runtime::block_on(blocking(|| -> u8 { panic!("gone") })).unwrap_err();
        assert!(err.starts_with("That stopped partway"), "{err}");
    }

    /// More menus up at once than the async runtime has workers, each left open a while: other
    /// commands still run meanwhile.
    #[test]
    fn a_menu_left_open_holds_no_worker() {
        use std::time::{Duration, Instant};
        let open = std::thread::available_parallelism().map_or(8, |n| n.get()) * 2;
        let left_open = |show: Box<dyn FnOnce() + Send>| {
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(2));
                show();
            });
            Ok(())
        };
        let menus: Vec<_> = (0..open).map(|n| tauri::async_runtime::spawn(answer_from(left_open, move || n))).collect();
        let started = Instant::now();
        assert_eq!(tauri::async_runtime::block_on(tauri::async_runtime::spawn(async { 7 })).unwrap(), 7);
        assert!(started.elapsed() < Duration::from_secs(1), "waited {:?} for a worker", started.elapsed());
        for (n, menu) in menus.into_iter().enumerate() {
            assert_eq!(tauri::async_runtime::block_on(menu).unwrap(), Ok(n));
        }
    }

    #[test]
    fn another_window_takes_the_first_free_label() {
        let spur = project_window_label("Spur");
        assert_eq!(another_window_label("Spur", |_| false), spur);
        let open = [spur.clone(), format!("{spur}:2")];
        assert_eq!(another_window_label("Spur", |l| open.iter().any(|o| o == l)), format!("{spur}:3"));
        // A project whose own label looks like a second window's still gets its own.
        assert_ne!(another_window_label("Spur", |l| l == spur), project_window_label("Spur 2"));
    }

    #[test]
    fn each_project_has_a_window_label_of_its_own() {
        // Readable where the name allows, and the same for the same name every time.
        assert!(project_window_label("All projects").starts_with("project-all-projects-"));
        assert_eq!(project_window_label("Spur"), project_window_label("Spur"));
        // Names that cut down the same, or to nothing, still differ.
        let names = ["Foo Bar", "Foo-Bar", "foo bar", "日本語", "中文", "Ünï", "Uni"];
        let labels: std::collections::HashSet<String> = names.iter().map(|n| project_window_label(n)).collect();
        assert_eq!(labels.len(), names.len(), "{labels:?}");
        assert_eq!(project_window_label("日本語").len(), "project-".len() + 8);
        // Only what a label may hold, and short whatever the name.
        for name in names.iter().copied().chain([&*"a very long project name ".repeat(20)]) {
            let label = project_window_label(name);
            assert!(label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'), "{label}");
            assert!(label.len() <= 60 && !label.contains("--"), "{label}");
        }
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
        let kept = format!("{}1", kept::PREFIX);
        for label in ["main", project_window_label("Spur").as_str(), project_window_label("日本語").as_str(), bar::LABEL, peek::LABEL, quick::LABEL, waiting::LABEL, card.as_str(), kept.as_str(), lineage::LABEL] {
            assert!(allowed(label), "{label} isn't in capabilities/default.json");
        }
    }

    #[test]
    fn the_notes_every_session_reads_stay_short_and_list_the_rest() {
        // CLAUDE.md is loaded into every session in this repo, and Codex reads only its first
        // 32 KiB (AGENTS.md is a link to it, cut at `project_doc_max_bytes`). What one area needs
        // goes beside its code, or in that area's file in .claude/rules/, which loads when a
        // session reads a file its `paths` name, and which CLAUDE.md lists.
        let notes = include_str!("../../CLAUDE.md");
        assert!(notes.len() <= 32 * 1024, "CLAUDE.md is {} bytes, over the 32 KiB Codex reads: move what only one area needs into its .claude/rules/ file", notes.len());
        let rules = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.claude/rules");
        let mut seen = 0;
        for entry in std::fs::read_dir(&rules).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".md") {
                continue;
            }
            seen += 1;
            assert!(notes.contains(&format!("`{name}`")), "CLAUDE.md's *Where facts live* doesn't list .claude/rules/{name}");
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(text.starts_with("---\npaths:\n"), ".claude/rules/{name} doesn't start with the `paths` it loads for");
        }
        assert!(seen > 0, "no rules files in {}", rules.display());
    }

    #[test]
    fn menu_commands_go_only_to_windows_that_show_the_board() {
        let spur = project_window_label("Spur");
        for label in ["main", spur.as_str(), another_window_label("Spur", |l| l == spur).as_str(), project_window_label("日本語").as_str()] {
            assert!(shows_board(label), "{label}");
        }
        // These say they're key while they aren't, or never show a dialog.
        let card = format!("{}1", desktop::PREFIX);
        let kept = format!("{}1", kept::PREFIX);
        for label in [bar::LABEL, peek::LABEL, quick::LABEL, waiting::LABEL, card.as_str(), kept.as_str(), lineage::LABEL] {
            assert!(!shows_board(label), "{label}");
        }
    }

    /// A window's command is the app's or the server's (calls.rs), and only the few the app does
    /// more with, bringing what they started forward, are both: the app's runs the server's.
    #[test]
    fn the_app_and_the_server_share_only_the_commands_the_app_does_more_with() {
        let mut both: Vec<&str> = APP_COMMANDS.iter().copied().filter(|n| calls::NAMES.contains(n)).collect();
        both.sort();
        assert_eq!(both, ["fork_session", "move_session", "resume_session", "run_command", "sign_in", "start_from_quick", "start_session"]);
    }

    /// The app has no engine: a command of its own reads the board the server publishes
    /// (`view::Mirror`), and one that needs the engine or your other devices is a call of the
    /// server's (calls.rs), which `forward` hands it.
    #[test]
    fn no_command_of_the_apps_reads_an_engine_it_hasnt_got() {
        let lib = include_str!("lib.rs");
        for state in ["engine::Handle>", "peers::Peers>"] {
            assert!(!lib.contains(&format!("State<'_, {state}")), "a command in lib.rs takes {state}: move it into calls.rs");
        }
        // Nor does anything of the app's look one up, which would find none and do nothing.
        fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    rust_files(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        rust_files(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            for held in ["engine::Handle>()", "peers::Peers>()"] {
                assert!(!text.contains(&format!("state::<crate::{held}")) && !text.contains(&format!("state::<{held}")), "{} looks up the app's {held}, which only the server has: ask the server (view::call) or read view::Mirror", file.display());
            }
        }
    }

    #[test]
    fn link_codes_draw_as_qr_codes() {
        let code = irohnet::Ticket::new(iroh::SecretKey::generate().public()).encode();
        let svg = link_qr_svg(&code).unwrap();
        assert!(svg.contains("<svg") && svg.contains("#000000") && svg.contains("#ffffff"), "{svg}");
    }
}

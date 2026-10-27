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
mod os;
mod panel;
mod peek;
mod config;
mod detect;
mod desktop;
mod devicekey;
mod editors;
mod engine;
// The tests' sessions, lanes and scratch folders are codealong-peer's, compiled here too: one
// crate's tests can't see another's test-only code.
#[cfg(test)]
#[path = "../peer/src/fixtures.rs"]
mod fixtures;
mod geometry;
mod flash;
mod front;
mod ghpoll;
mod git;
mod harness;
mod held;
mod helpers;
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
mod newfolder;
mod nudges;
mod lineage;
mod logo;
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
mod throttle;
mod todojev;
mod todos;
mod training;
mod transcript;
mod updates;
mod usage;
mod util;
mod waiting;
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
/// copy and send.
#[tauri::command(async)]
fn get_board(engine: State<'_, engine::Handle>) -> model::Board {
    engine.board()
}

#[tauri::command]
async fn open_target(app: tauri::AppHandle, engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, lane_id: String, target_id: String) -> Result<(), String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || {
        let from = flash::pointer();
        actions::run(&engine, &peers, actions::OPEN, &lane_id, actions::params::Open { target: target_id.clone() })?;
        // The terminals Codealong scripts bring themselves forward; Rex, driven through its
        // command, can't, and macOS lets the app you clicked in bring it forward where a program
        // it runs can't.
        if let Some(bundle) = target_app(&engine, &lane_id, &target_id) {
            front::bring_forward(&app, bundle);
        }
        flash_focused(&app, &engine, &lane_id, &target_id, from);
        Ok(())
    })
    .await?
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
    let session = target_id.strip_prefix("s|").and_then(|t| t.rsplit_once('|')).and_then(|(key, action)| Some((board.lanes.iter().find(|l| l.id == lane_id)?.sessions.iter().find(|s| s.key == key)?, action)));
    let host = session.and_then(|(s, _)| launch::session_app(s.pid?));
    let own = session.and_then(|(s, action)| harness::adapter(s.harness).app_bundle(action));
    bar::target_app(target_id, &board.apps.terminal_id, host, own)
}

#[tauri::command]
async fn open_config(engine: State<'_, engine::Handle>) -> Result<(), String> {
    let engine = engine.inner().clone();
    blocking(move || engine.open_config().map_err(words)).await?
}

/// The installed editors' app icons, for the Open in menus. Off the main thread: the first
/// drawing of each takes a moment, and AppKit draws into a bitmap on any thread.
#[tauri::command(async)]
fn editor_icons() -> Vec<editors::Icon> {
    editors::icons()
}

/// What sessions used and cost in the last `days` days, for the Usage window (ledger.rs), for
/// one account or all. Asking reads what's new in the background too, for the next time.
#[tauri::command]
async fn token_report(days: u32, account: Option<String>) -> Result<ledger::Report, String> {
    ledger::nudge();
    blocking(move || ledger::report(days, account.as_deref()).map_err(words)).await?
}

/// What one session used and cost, by model, for its panel.
#[tauri::command]
async fn session_tokens(key: String) -> Result<ledger::SessionSpent, String> {
    blocking(move || ledger::session(&key).map_err(words)).await?
}

/// What "Close idle sessions" would close, sessions quiet for `after_ms` or longer, and with
/// `archive` what it would only archive (idle.rs).
#[tauri::command]
async fn idle_sessions(engine: State<'_, engine::Handle>, after_ms: i64, archive: bool) -> Result<idle::Plan, String> {
    Ok(idle::plan_now(&engine.board(), after_ms, archive))
}

/// Closes these sessions, each checked again first, and with `archive` archives them. On the
/// blocking pool: each terminal is told by a script and each job stopped by a command, which are
/// waited on.
#[tauri::command]
async fn close_idle_sessions(engine: State<'_, engine::Handle>, keys: Vec<String>, after_ms: i64, archive: bool) -> Result<Vec<idle::Outcome>, String> {
    let engine = engine.inner().clone();
    blocking(move || idle::close_on(&engine, &keys, after_ms, archive)).await?
}

/// What "Close stale terminals" would close: the terminal tabs unused for `after_ms` or longer
/// (stale.rs). On the blocking pool: it runs `ps`, which takes seconds on a busy Mac.
#[tauri::command]
async fn stale_terminals(engine: State<'_, engine::Handle>, after_ms: i64) -> Result<stale::Plan, String> {
    let engine = engine.inner().clone();
    blocking(move || stale::plan_now(&engine.board(), engine.terminal_use(), after_ms)).await
}

/// Closes these tabs, each looked at again first. On the blocking pool: each terminal is told by
/// a script, which is waited on.
#[tauri::command]
async fn close_stale_terminals(engine: State<'_, engine::Handle>, keys: Vec<String>, after_ms: i64) -> Result<Vec<stale::Outcome>, String> {
    let engine = engine.inner().clone();
    blocking(move || stale::close(&engine.board(), engine.terminal_use(), &keys, after_ms)).await
}

/// Closes one session from its menu: its terminal tab, or a background job stopped (idle.rs).
/// Returns what happened, in a sentence.
#[tauri::command]
async fn close_session(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<String, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, actions::CLOSE, &key, serde_json::json!({})).map(|v| v.as_str().unwrap_or_default().to_string())).await?
}

/// Done with a session: it ends as far as Codealong can end it, and is marked seen and
/// archived (`actions::done`). `stop` ends work under way too. Returns whether it was done,
/// what happened in a sentence, and the time of the board that shows it.
#[tauri::command]
async fn session_done(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, stop: bool) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SESSION_DONE, &key, actions::params::Done { stop, ..Default::default() })).await?
}

/// Marks sessions seen, each on the device it's on: its status changes there, and so on every
/// device. Returns the time of the board that shows the sessions on this device seen (none when
/// they're all on other devices, whose feeds show it as they come in), for the window's own copy.
#[tauri::command(async)]
fn mark_seen(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, keys: Vec<String>) -> Option<i64> {
    actions::run_keys(&engine, &peers, api::MARK_SEEN, keys, actions::params::Keys::default()).and_then(|v| v.as_i64())
}

/// Marks sessions unread, each on the device it's on: each is Your turn again until it's seen or
/// prompted. Returns the time of the board that shows it, as `mark_seen` does.
#[tauri::command(async)]
fn mark_unread(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, keys: Vec<String>) -> Option<i64> {
    actions::run_keys(&engine, &peers, api::MARK_UNREAD, keys, actions::params::Keys::default()).and_then(|v| v.as_i64())
}

/// Snoozes sessions until `until` (ms since the epoch), or wakes them with none, each on the
/// device it's on, so it's snoozed there and on every device. Returns the time of the board that
/// shows it, as `mark_seen` does, once every device has answered: one that didn't snooze its
/// sessions (it isn't connected, or is from before it could be asked) is the error.
#[tauri::command]
async fn snooze_sessions(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, keys: Vec<String>, until: Option<i64>) -> Result<Option<i64>, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run_keys_heard(&engine, &peers, api::SESSION_SNOOZE, keys, actions::params::Snooze { keys: Vec::new(), until }).map(|v| v.and_then(|v| v.as_i64()))).await?
}

/// Quiets sessions, or makes them heard again with `quiet` false, each on the device it's on, as
/// `snooze_sessions` snoozes them: a quiet one's turns don't wait on you there or anywhere. Returns
/// the time of the board that shows it once every device has answered, and why one didn't.
#[tauri::command]
async fn quiet_sessions(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, keys: Vec<String>, quiet: bool) -> Result<Option<i64>, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run_keys_heard(&engine, &peers, api::SESSION_QUIET, keys, actions::params::Quiet { keys: Vec::new(), quiet }).map(|v| v.and_then(|v| v.as_i64()))).await?
}

/// Makes helpers sessions of your own, for good, each on the device it's on, as `quiet_sessions`
/// quiets them. Returns the time of the board that shows it once every device has answered, and
/// why one didn't.
#[tauri::command]
async fn own_sessions(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, keys: Vec<String>) -> Result<Option<i64>, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run_keys_heard(&engine, &peers, api::SESSION_OWN, keys, actions::params::Own { keys: Vec::new() }).map(|v| v.and_then(|v| v.as_i64()))).await?
}

/// Snoozes your review of PRs (`owner/name#number`) until `until` (ms since the epoch), or wakes
/// them with none. Returns the time of the board that shows it, as `mark_seen` does.
#[tauri::command(async)]
fn snooze_reviews(engine: State<'_, engine::Handle>, keys: Vec<String>, until: Option<i64>) -> Option<i64> {
    engine.snooze_reviews(keys, until)
}

/// Turns telling a PR's primary session what happens on it on or off for the PR
/// `owner/name#number` (nudges.rs): in pr-links.json, as `codealong pr nudge` does, which the
/// engine then reads at once.
#[tauri::command]
async fn set_pr_nudge(engine: State<'_, engine::Handle>, key: String, on: bool) -> Result<(), String> {
    let engine = engine.inner().clone();
    blocking(move || {
        prlinks::change(|store| store.set_nudge(&key, on)).map_err(words)?;
        engine.pr_links_changed();
        Ok(())
    })
    .await?
}

/// Makes a session the PR's primary one, the one a nudge tells, from the PR's menu (Primary
/// session), as `codealong pr nudge --session` does without turning the nudge on.
#[tauri::command]
async fn set_pr_primary(engine: State<'_, engine::Handle>, key: String, session: String) -> Result<(), String> {
    let engine = engine.inner().clone();
    blocking(move || {
        prlinks::change(|store| store.set_primary(&key, &session, crate::util::now_ms())).map_err(words)?;
        engine.pr_links_changed();
        Ok(())
    })
    .await?
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
#[tauri::command]
async fn set_preview(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, lane_id: String, url: Option<String>, port: Option<u16>, chrome_profile: Option<String>) -> Result<Option<i64>, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || {
        // The sheet gives all of it: no URL takes the preview away, and no profile is its project's.
        let change = match url {
            Some(url) => actions::params::Preview { url: Some(url), port: Some(port), chrome_profile: Some(chrome_profile), ..Default::default() },
            None => actions::params::Preview { clear: true, ..Default::default() },
        };
        actions::run(&engine, &peers, actions::PREVIEW, &lane_id, change).map(|v| v["at"].as_i64())
    })
    .await?
}

/// Marks a worktree reusable or not (reusable.rs). Returns the time of the board that shows it,
/// when the engine says in time.
#[tauri::command]
async fn set_reusable(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, lane_id: String, on: bool) -> Result<Option<i64>, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, actions::REUSABLE, &lane_id, actions::params::Reusable { on }).map(|v| v["at"].as_i64())).await?
}

/// Removes a worktree when nothing in it would be lost, or with `dry_run` says whether it can go
/// and what would run (cleanup.rs), on the device it's on. On the blocking pool: a repo's own
/// teardown can take a minute.
#[tauri::command]
async fn remove_worktree(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, lane_id: String, dry_run: bool) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::WORKTREE_REMOVE, &lane_id, api::params::Remove { dry_run, ..Default::default() })).await?
}

#[tauri::command]
fn set_pinned_worktrees(engine: State<'_, engine::Handle>, ids: Vec<String>) {
    engine.set_pinned_worktrees(ids);
}

#[tauri::command]
fn set_pinned_prs(engine: State<'_, engine::Handle>, keys: Vec<String>) {
    engine.set_pinned_prs(keys);
}

#[tauri::command]
fn set_pinned_repos(engine: State<'_, engine::Handle>, keys: Vec<String>) {
    engine.set_pinned_repos(keys);
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

/// Opens a session from the floating bar, then asks macOS to bring its app forward: a click in the
/// bar doesn't make Codealong the active app.
#[tauri::command]
async fn open_from_bar(app: tauri::AppHandle, engine: State<'_, engine::Handle>, lane_id: String, target_id: String) -> Result<(), String> {
    let engine = engine.inner().clone();
    blocking(move || {
        if let Some(key) = target_id.strip_prefix("s|").and_then(|t| t.strip_suffix("|hosted")) {
            // Codealong runs it: its panel in the main window is where it lives.
            engine.mark_seen(vec![key.to_string()]);
            return front::show_main(&app, Some(serde_json::json!({ "kind": "session", "laneId": lane_id, "key": key }))).map_err(words);
        }
        let from = flash::pointer();
        engine.open(&lane_id, &target_id).map_err(words)?;
        if let Some(bundle) = target_app(&engine, &lane_id, &target_id) {
            front::bring_forward(&app, bundle);
        }
        flash_focused(&app, &engine, &lane_id, &target_id, from);
        Ok(())
    })
    .await?
}

/// Shows a session in the floating bar's peek: for a hover over its pill or a click on a card on
/// the desktop (where the window's page measured it), with `focus` ready to type into, or with
/// `rename` its name ready to change (peek.rs).
#[tauri::command]
fn peek_open(app: tauri::AppHandle, window: tauri::WebviewWindow, engine: State<'_, engine::Handle>, floating: State<'_, Floating>, key: String, pill: Option<geometry::PageRect>, focus: Option<bool>, rename: Option<bool>) -> Result<(), String> {
    let target = engine.read(|b| floating.read(b, |shown| peek::target(shown, &key))).ok_or_else(|| "That session isn't on the board any more.".to_string())?;
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
fn pr_at(engine: State<'_, engine::Handle>, key: String) -> Option<PrAt> {
    engine.read(|b| {
        let pr = b.prs.iter().find(|p| p.key == key)?.clone();
        let lanes = b.lanes.iter().filter(|l| pr.lanes.contains(&l.id) || pr.sessions.iter().any(|s| s.lane == l.id)).cloned().collect();
        Some(PrAt { pr, lanes, apps: b.apps.clone() })
    })
}

#[tauri::command(async)]
fn session_at(engine: State<'_, engine::Handle>, floating: State<'_, Floating>, key: String) -> Option<SessionAt> {
    // From the worktree it runs in, not one it only visits, on whichever Mac it's on.
    engine.read(|b| {
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

/// What the floating bar shows, for its first render; after that it gets `bar-feed` events.
#[tauri::command(async)]
fn bar_feed(engine: State<'_, engine::Handle>, floating: State<'_, Floating>) -> barfeed::BarFeed {
    engine.read(|b| floating.read(b, barfeed::feed))
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
fn desktop_feed(app: tauri::AppHandle, window: tauri::Window, engine: State<'_, engine::Handle>, floating: State<'_, Floating>) -> Option<desktop::Feed> {
    engine.read(|b| floating.read(b, |shown| desktop::feed_for(&app, window.label(), shown)))
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
async fn start_from_quick(app: tauri::AppHandle, engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, spec: start::StartSpec) -> Result<start::Started, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || {
        let started = actions::typed(actions::run(&engine, &peers, api::START, &spec.lane_id, actions::params::Start { spec: spec.clone() })?)?;
        if peerfeed::split_remote(&spec.lane_id).is_none() {
            start::note(&engine, &spec);
            if let Some(bundle) = quick::start_app(spec.place, &engine.board().apps.terminal_id) {
                front::bring_forward(&app, bundle);
            }
        }
        Ok(started)
    })
    .await?
}

/// Where what's typed in the quick panel should go, from Jev (route.rs).
#[tauri::command]
async fn route_quick(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, project: String, text: String) -> Result<route::Route, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::typed(actions::run_on(&engine, &peers, api::ROUTE, None, actions::params::Route { project, text })?)).await?
}

/// Keeps what you did with what you typed in the quick panel, beside what Jev suggested for it
/// (routelog.rs).
#[tauri::command(async)]
fn note_route(note: routelog::Note) -> Result<(), String> {
    routelog::note(note, util::now_ms()).map_err(words)
}

/// Sessions whose prompts and replies have every word (search.rs), for the quick panel.
#[tauri::command(async)]
fn search_text(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, words: Vec<String>) -> Result<search::Found, String> {
    actions::typed(actions::run_on(&engine, &peers, api::SEARCH, None, actions::params::Search { words })?)
}

/// A page of a session's conversation, from its own record (transcript.rs): the newest when
/// `before` is none, else what came before it.
#[tauri::command]
async fn session_page(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, before: Option<u64>) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SESSION_PAGE, &key, actions::params::Page { before, ..Default::default() })).await?
}

/// What a session waiting on you wants to do, when it's a tool call Codealong can answer for it
/// (`harness::Adapter::pending`).
#[tauri::command]
async fn session_pending(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SESSION_PENDING, &key, actions::params::Session::default())).await?
}

/// The skills a session in each of `folders` could be asked to run by name, and each of
/// `accounts`' own (skills.rs), for the quick panel's `/` list. This machine's only: its folders
/// and accounts.
#[tauri::command]
async fn skills(folders: Vec<String>, accounts: Vec<String>) -> Result<skills::Listed, String> {
    blocking(move || {
        let cfg = config::current().map_err(util::words)?;
        Ok(skills::list(&folders, &accounts, &cfg))
    })
    .await?
}

/// The skills a session could be asked to run by name, for its reply box's `/` list: its
/// folder's and its account's, read on the device it runs on (`api::SESSION_SKILLS`).
#[tauri::command]
async fn session_skills(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SESSION_SKILLS, &key, actions::params::Session::default())).await?
}

/// How a message sent to a session would reach it, for its box's placeholder (send.rs `way_in`).
#[tauri::command]
async fn session_way(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SESSION_WAY, &key, actions::params::Session::default())).await?
}

/// Allows or denies the tool call `id` a session waits on, if it's still the one waiting; or,
/// with `answers`, answers the questions it asks (`Pending::questions`). A deny with `message`
/// tells it what to do instead.
#[tauri::command]
async fn session_answer(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, id: String, answer: harness::Answer, answers: Option<serde_json::Value>, message: Option<String>) -> Result<String, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || {
        let said = actions::run(&engine, &peers, api::SESSION_ANSWER, &key, actions::params::Answer { id, answer: Some(answer), answers, message, ..Default::default() })?;
        Ok(said.as_str().unwrap_or_default().to_string())
    })
    .await?
}

/// What a session said and did after `after`, as it goes on.
#[tauri::command]
async fn session_newer(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, after: u64) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SESSION_NEWER, &key, actions::params::Newer { after, ..Default::default() })).await?
}

/// The reply a session is writing that its record doesn't have yet (actions.rs `stream`), for a
/// panel that opens mid-turn, known by the key the window has for it.
#[tauri::command]
async fn session_stream(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<Option<acp::Stream>, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || {
        let got = actions::run(&engine, &peers, api::SESSION_STREAM, &key, actions::params::Session::default()).ok()?;
        let mut s: acp::Stream = serde_json::from_value(got).ok()?;
        s.key = key;
        Some(s)
    })
    .await
}

/// Stops the turn a session is taking, here or on another device, and says what happened.
#[tauri::command]
async fn session_cancel(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<String, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || {
        let said = actions::run(&engine, &peers, api::SESSION_CANCEL, &key, actions::params::Session::default())?;
        // Devices before stops reached every harness answered with nothing.
        Ok(said.as_str().unwrap_or("Stopped.").to_string())
    })
    .await?
}

/// Sends a message a session's harness holds until its turn ends (`id`, one of `Session::queued`)
/// into the turn it's taking now, here or on another device, and says what happened.
#[tauri::command]
async fn session_steer(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, id: String) -> Result<String, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || {
        let said = actions::run(&engine, &peers, api::SESSION_STEER, &key, actions::params::Steer { id, ..Default::default() })?;
        Ok(said.as_str().unwrap_or_default().to_string())
    })
    .await?
}

/// Sends a prompt to a session that's already going, where it runs (send.rs); `now` sends it
/// into the turn it's taking at once (Send now). Only Codealong's windows call this, so what it
/// sends is what you typed. A device from before Send now reads no `now` and sends it as usual.
#[tauri::command]
async fn send_to_session(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, text: String, now: Option<bool>) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SESSION_SEND, &key, actions::params::Send { text, now: now.unwrap_or(false), ..Default::default() })).await?
}

/// Goes on with a session that isn't running where `resume` says (`start::resume`), on `prompt`
/// when there's one, answering how it went as a send does.
#[tauri::command]
async fn resume_session(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, resume: start::ResumeIn, prompt: Option<String>) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SESSION_RESUME, &key, actions::params::Resume { resume, prompt: prompt.unwrap_or_default(), ..Default::default() })).await?
}

/// Switches a running session to another of its account's models, for that session alone
/// (`send::switch_model`). Says what happened.
#[tauri::command]
async fn switch_session_model(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, model: String) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SESSION_MODEL, &key, actions::params::Model { model, ..Default::default() })).await?
}

/// Switches a running session to another of its account's effort levels, for that session alone
/// (`send::switch_effort`). Says what happened.
#[tauri::command]
async fn switch_session_effort(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, effort: String) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SESSION_EFFORT, &key, actions::params::Effort { effort, ..Default::default() })).await?
}

/// Switches a session Codealong runs to another of the modes its agent offers, for that session
/// alone (`send::switch_mode`). Says what happened, and the mode it's in now.
#[tauri::command]
async fn switch_session_mode(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, mode: String) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SESSION_MODE, &key, actions::params::Mode { mode, ..Default::default() })).await?
}

/// What Jev suggests for a todo being written: its project, labels and the todos it waits on
/// (todojev.rs). Errors when `[jev] todos` is off or there's nothing to ask.
#[tauri::command]
async fn suggest_todo(engine: State<'_, engine::Handle>, id: Option<String>, title: String, description: String, project: String) -> Result<todojev::Suggested, String> {
    let engine = engine.inner().clone();
    blocking(move || todojev::suggest(&engine.board(), id.as_deref(), &title, &description, &project).map_err(words)).await?
}

/// "Looks done" was wrong for this todo, until something linked to it moves on.
#[tauri::command]
fn dismiss_todo_hint(engine: State<'_, engine::Handle>, id: String) {
    engine.dismiss_todo_hint(id);
}

/// Whether there's a TypeSafe API key, for Settings. Reads no secret.
#[tauri::command]
async fn jev_key() -> Result<bool, String> {
    blocking(move || jev::has_key()).await
}

/// Saves the TypeSafe API key in the keychain, or removes it when it's empty.
#[tauri::command]
async fn set_jev_key(key: String) -> Result<(), String> {
    blocking(move || if key.trim().is_empty() { jev::delete_key() } else { jev::save_key(&key) }.map_err(words)).await?
}

/// One small call to TypeSafe, for Settings' Test button.
#[tauri::command]
async fn test_jev(model: String) -> Result<String, String> {
    blocking(move || jev::check(model.trim()).map_err(words)).await?
}

/// What Codealong's requests to Jev used each of the last `days` days, by feature, for Settings
/// › Jev (ledger.rs `jev_report`).
#[tauri::command]
async fn jev_usage(days: u32) -> Result<ledger::JevReport, String> {
    blocking(move || ledger::jev_report(days).map_err(words)).await?
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

/// A PR's description, reviews and changed files.
#[tauri::command]
async fn pr_detail(repo: String, number: u64) -> Result<github::PrDetail, String> {
    blocking(move || github::detail(&repo, number).map_err(words)).await?
}

/// What changed on a PR's branch from commit `from` (the one your last review was of) to `to`,
/// for its Changes tab (github.rs `since`).
#[tauri::command]
async fn pr_since(repo: String, from: String, to: String) -> Result<github::Since, String> {
    blocking(move || github::since(&repo, &from, &to).map_err(words)).await?
}

/// A PR the board doesn't list, for a link to it in a conversation: what GitHub says of it, with
/// the worktrees and sessions here that worked on it.
#[tauri::command]
async fn pr_card(engine: State<'_, engine::Handle>, repo: String, number: u64) -> Result<model::PrItem, String> {
    let engine = engine.inner().clone();
    blocking(move || engine.pr_card(&repo, number).map_err(words)).await?
}

/// Which of a PR's comments ask for a change, by comment id, from Jev (prasks.rs). Empty while
/// `[jev] pull_requests` is off or there's no key: the panel just shows no markers.
#[tauri::command]
async fn pr_comment_asks(title: String, comments: Vec<prasks::CommentIn>) -> Result<std::collections::HashMap<String, f64>, String> {
    blocking(move || {
        let cfg = config::current().map_err(words)?;
        if !cfg.jev.pull_requests || !jev::has_key() {
            return Ok(Default::default());
        }
        prasks::asks(cfg.jev.model.trim(), &title, &comments).map_err(words)
    })
    .await?
}

/// Reviews a PR as you: approves it, comments on it or asks for changes, with `body` as the
/// review's text, submitting your draft review of it when you have one (github.rs `review`). Any
/// review settles the request to you, so it returns once the board no longer asks for your
/// review, and PRs are read again for the rest. With `agents`, it's the agent's review that's
/// submitted, as the verdict it suggests, `body` going before it (autoreview.rs `submit_now`).
#[tauri::command]
async fn review_pr(engine: State<'_, engine::Handle>, reviews: State<'_, Reviews>, repo: String, number: u64, verdict: github::Verdict, body: String, agents: Option<bool>) -> Result<github::Reviewed, String> {
    let (engine, reviewer) = (engine.inner().clone(), reviews.0.clone());
    blocking(move || {
        if agents == Some(true) {
            return autoreview::submit_now(&engine, &reviewer, &format!("{repo}#{number}"), verdict, &body);
        }
        let done = github::review(&repo, number, verdict, &body).map_err(words)?;
        engine.approved(&repo, number, verdict, done.commit.clone());
        Ok(done)
    })
    .await?
}

/// Your review of a PR on GitHub, for its Review tab: the draft you haven't submitted, with its
/// comments, and the agent's review you did submit (github.rs `your_review`).
#[tauri::command]
async fn pr_draft(repo: String, number: u64) -> Result<github::YourReview, String> {
    blocking(move || github::your_review(&repo, number).map_err(words)).await?
}

/// Gives your draft review of a PR the text `text`.
#[tauri::command]
async fn edit_draft(repo: String, number: u64, text: String) -> Result<(), String> {
    blocking(move || github::edit_draft(&repo, number, &text).map_err(words)).await?
}

/// Gives a comment of your draft review the text `body`.
#[tauri::command]
async fn edit_draft_comment(id: String, body: String) -> Result<(), String> {
    blocking(move || github::edit_draft_comment(&id, &body).map_err(words)).await?
}

/// Takes a comment out of your draft review.
#[tauri::command]
async fn drop_draft_comment(id: String) -> Result<(), String> {
    blocking(move || github::drop_draft_comment(&id).map_err(words)).await?
}

/// What a PR's merge box offers you, as GitHub's has it (prmerge.rs).
#[tauri::command]
async fn merge_box(repo: String, number: u64) -> Result<prmerge::MergeBox, String> {
    blocking(move || prmerge::merge_box(&repo, number).map_err(words)).await?
}

/// What the merge box's act did, for the window that asked.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeDone {
    /// What it couldn't do after (a branch that stayed).
    note: Option<String>,
    /// The time of the first board that shows the PR merged, for a merge.
    shown_at: Option<i64>,
    /// Merge and done…: what marking its session done said (actions.rs `done`), or why that
    /// wasn't done (`done_error`).
    #[serde(skip_serializing_if = "Option::is_none")]
    done: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    done_error: Option<String>,
}

/// Does what the merge box asked to a PR as you: merges it, turns auto-merge on or off, queues it,
/// updates or deletes its branch, takes it out of draft. Returns once GitHub has done it. A PR
/// merged shows merged on the board at once (`engine::Handle::merged`); for anything else PRs are
/// read again. With `done` (Merge and done…, src/ui/MergeDone.tsx), the session that key names
/// is marked done once GitHub has merged the PR, work under way ended too, here rather than by the
/// window, which may have closed by then (a kept session's ⌘W, the peek going).
#[tauri::command]
async fn merge_pr(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, repo: String, number: u64, id: String, op: prmerge::Op, done: Option<String>) -> Result<MergeDone, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || {
        let acted = prmerge::act(number, &id, &op).map_err(words)?;
        let shown_at = if acted.merged {
            engine.merged(&repo, number)
        } else {
            engine.pr_changed();
            None
        };
        let (done, done_error) = match done {
            Some(key) if acted.merged => match actions::run(&engine, &peers, api::SESSION_DONE, &key, actions::params::Done { stop: true, ..Default::default() }) {
                Ok(answer) => (Some(answer), None),
                Err(e) => (None, Some(e)),
            },
            Some(_) => (None, Some(format!("GitHub didn't say #{number} merged, so its session is left as it was."))),
            None => (None, None),
        };
        Ok(MergeDone { note: acted.note, shown_at, done, done_error })
    })
    .await?
}

/// The reviews Codealong's agent wrote of a PR, the latest first, as reviews.json keeps them
/// (autoreview.rs `written`): what its panel shows of each, comments and all.
#[tauri::command]
async fn pr_agent_reviews(key: String) -> Result<Vec<autoreview::WrittenReview>, String> {
    blocking(move || Ok::<_, String>(autoreview::written(&store::read_or_default(&autoreview::path()), &key))).await?
}

/// The automatic reviewer (autoreview.rs), for what a window asks of it.
struct Reviews(std::sync::Arc<std::sync::Mutex<autoreview::Reviewer>>);

/// Hands a PR of yours' latest review to its primary session now (autoreview.rs
/// `send_now`), as its Review tab's Send asks.
#[tauri::command]
async fn send_agent_review(engine: State<'_, engine::Handle>, reviews: State<'_, Reviews>, key: String) -> Result<(), String> {
    let (engine, reviewer) = (engine.inner().clone(), reviews.0.clone());
    blocking(move || autoreview::send_now(&engine, &reviewer, &key)).await?
}

/// Starts a review of a PR now, as Review with… asks (autoreview.rs `review_now`): a round of the
/// automatic reviews, on `account`, else the reviewer config.toml names.
#[tauri::command]
async fn review_pr_now(engine: State<'_, engine::Handle>, reviews: State<'_, Reviews>, key: String, account: Option<String>) -> Result<(), String> {
    let (engine, reviewer) = (engine.inner().clone(), reviews.0.clone());
    blocking(move || autoreview::review_now(&engine, &reviewer, &key, account.as_deref())).await?
}

/// Tries a PR's review that didn't get done again, as Try again on its card and its notice asks
/// (autoreview.rs `again`): posts a review that wasn't posted, else reviews the PR again. Says
/// what it did.
#[tauri::command]
async fn review_pr_again(engine: State<'_, engine::Handle>, reviews: State<'_, Reviews>, key: String) -> Result<String, String> {
    let (engine, reviewer) = (engine.inner().clone(), reviews.0.clone());
    blocking(move || autoreview::again(&engine, &reviewer, &key)).await?
}

/// Opens a PR in Codex's review.
#[tauri::command]
async fn review_in_codex(url: String) -> Result<(), String> {
    blocking(move || launch::open_codex(&launch::codex_review_url(&url)).map_err(words)).await?
}

/// Opens a web link (in a conversation, a PR's description) in the browser: one into a
/// worktree's app in the Chrome profile its preview opens in (`engine::Handle::open_link`).
#[tauri::command]
async fn open_url(engine: State<'_, engine::Handle>, url: String) -> Result<(), String> {
    let engine = engine.inner().clone();
    blocking(move || {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err("Only web links open from here.".into());
        }
        engine.open_link(&url).map_err(words)
    })
    .await?
}

/// Opens one of a PR's changed files in your editor, as a diff or at a line (prdiff.rs). It can
/// wait on GitHub for the file's two sides.
#[tauri::command]
async fn open_pr_file(engine: State<'_, engine::Handle>, ask: prdiff::Ask) -> Result<(), String> {
    let engine = engine.inner().clone();
    blocking(move || {
        let place = engine.read(|b| prdiff::place(b, &ask.pr)).map_err(words)?;
        let yours = config::current().map_err(words)?.apps.editor;
        prdiff::open(place, &yours, &ask).map_err(words)
    })
    .await?
}

/// What a new terminal's shell takes as a command's first word (runcode.rs `commands`), for
/// Run on a block in a reply that doesn't say it's shell. On the blocking pool: the PATH comes
/// from the login shell the first time, and every folder on it is read.
#[tauri::command]
async fn shell_commands() -> Result<Vec<String>, String> {
    blocking(runcode::commands).await
}

/// Sends a session what a command you ran for it printed, as yours (terminal.rs `report_ended`):
/// not to one that takes a message only pasted into its app, which would come forward from the
/// background with it on the clipboard and nobody there to paste it.
fn tell_run(engine: &engine::Handle, peers: &peers::Peers, run: &terminal::Run, text: String) -> Result<(), String> {
    let cfg = config::current().map_err(words)?;
    let way = send::way_in(&engine.board(), &cfg, &run.session).map_err(words)?;
    if matches!(way, Some(api::Way::ClaudeApp | api::Way::Antigravity | api::Way::Cursor)) {
        return Err("it takes a message only pasted into its app, so it wasn't sent.".into());
    }
    let sent = actions::run(engine, peers, api::SESSION_SEND, &run.session, actions::params::Send { text, ..Default::default() })?;
    match sent.get("opened").and_then(serde_json::Value::as_bool) {
        Some(true) => Err("its app opened with it to paste, which nobody did.".into()),
        _ => Ok(()),
    }
}

/// Runs a command you ran for a session (led by `!` in its box, or with `block` Run on a shell
/// block in its conversation) in a new tab of your terminal in `folder`, and has the session told
/// what it printed once it ends (actions.rs `run_command`). Says how it went, in a sentence.
#[tauri::command]
async fn run_command(app: tauri::AppHandle, window: tauri::WebviewWindow, engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, folder: String, text: String, block: bool) -> Result<String, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || {
        let said = actions::run(&engine, &peers, actions::RUN_COMMAND, &key, actions::params::RunCommand { text, folder, block })?;
        // A click in the bar's peek, the quick panel, a card on the desktop or a session kept there
        // leaves Codealong inactive, and macOS may not bring the terminal forward for an app that isn't.
        let label = window.label();
        if [bar::LABEL, quick::LABEL, peek::LABEL].contains(&label) || label.starts_with(desktop::PREFIX) || label.starts_with(kept::PREFIX) {
            if let Some(bundle) = launch::bundle_id(&engine.read(|b| b.apps.terminal_id.clone())) {
                front::bring_forward(&app, bundle);
            }
        }
        Ok(said.as_str().unwrap_or_default().to_string())
    })
    .await?
}

/// Archives or unarchives a session; returns the time of the board that shows it, as
/// `mark_seen` does.
#[tauri::command]
async fn archive_session(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, archived: bool) -> Result<Option<i64>, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SESSION_ARCHIVE, &key, actions::params::Archive { archived, ..Default::default() }).map(|v| v.as_i64())).await?
}

/// Names a session where its harness keeps its name, so the harness shows it too. Returns the
/// time of the board that shows it, as `mark_seen` does.
#[tauri::command]
async fn rename_session(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, name: String) -> Result<Option<i64>, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SESSION_RENAME, &key, actions::params::Rename { name, ..Default::default() }).map(|v| v.as_i64())).await?
}

/// Takes away the replies suggested for a session, on the device it's on. Returns the time of the
/// board that shows it, as `mark_seen` does.
#[tauri::command]
async fn dismiss_suggestions(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<Option<i64>, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SESSION_SUGGEST, &key, actions::params::Suggest::default()).map(|v| v.as_i64())).await?
}

#[tauri::command]
async fn get_accounts(engine: State<'_, engine::Handle>) -> Result<Vec<model::AccountInfo>, String> {
    let engine = engine.inner().clone();
    blocking(move || engine.accounts().map_err(words)).await?
}

#[tauri::command(async)]
fn save_accounts(engine: State<'_, engine::Handle>, accounts: Vec<config::AccountCfg>) -> Result<(), String> {
    engine.save_accounts(accounts).map_err(words)
}

/// The usage bar's chips dragged into another order: the accounts' order in config.toml.
#[tauri::command(async)]
fn order_accounts(accounts: Vec<String>) -> Result<(), String> {
    config::save_account_order(&accounts).map_err(words)
}

/// Where a start from the quick panel opened becomes `start` for its accounts in config.toml
/// (`start::keep_place`), which `codealong start` goes by too.
#[tauri::command(async)]
fn keep_start_place(accounts: Vec<String>, place: start::Place) -> Result<(), String> {
    start::keep_place(&accounts, place).map_err(words)
}

/// What the setup wizard offers: the accounts on this Mac, listed or not, and the folders to
/// search for repos (setup.rs).
#[tauri::command]
async fn setup_view() -> Result<setup::View, String> {
    blocking(move || setup::view_once_read().map_err(words)).await?
}

/// Whether the main window shows the setup wizard by itself: a first run's, until it's finished
/// or skipped.
#[tauri::command(async)]
fn setup_needed() -> bool {
    setup::needed()
}

/// The setup wizard was finished or skipped: it shows again only from the app menu.
#[tauri::command(async)]
fn finish_setup(engine: State<'_, engine::Handle>) {
    engine.finish_setup();
}

/// Accounts on this Mac that config.toml doesn't list, for Settings › Accounts.
#[tauri::command]
async fn found_accounts() -> Result<Vec<detect::Found>, String> {
    blocking(move || detect::unlisted().map_err(words)).await?
}

/// The settings as parsed and config.toml as written, for Settings.
#[tauri::command(async)]
fn get_config() -> config::ConfigView {
    config::view()
}

/// Plays a sound `[sounds]` can name, at a volume in percent: Settings' play button.
#[tauri::command]
async fn play_sound(name: String, volume: u32) -> Result<(), String> {
    blocking(move || sounds::play(&name, volume).map_err(words)).await?
}

/// Saves every setting from the Settings form, in place in config.toml.
#[tauri::command(async)]
fn save_config(engine: State<'_, engine::Handle>, config: config::Config) -> Result<(), String> {
    engine.save_config(&config).map_err(words)
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

/// Takes what a repo's team suggests in its `.codealong/config.toml` on `branch`, writing it into
/// your settings for that repo in config.toml (Use them, in Settings › Automation and a PR's
/// Review tab).
#[tauri::command(async)]
fn take_team_settings(engine: State<'_, engine::Handle>, repo: String, branch: String) -> Result<(), String> {
    engine.take_team_settings(&repo, &branch).map_err(words)
}

/// What a folder typed in the quick panel's worktree menu is (newfolder.rs).
#[tauri::command]
async fn look_folder(engine: State<'_, engine::Handle>, typed: String) -> Result<newfolder::Look, String> {
    let engine = engine.inner().clone();
    blocking(move || newfolder::look(&engine, &typed)).await
}

/// Makes a folder typed in the worktree menu, puts it in a project (one of its own with none),
/// and answers its worktree once the board has it (newfolder.rs).
#[tauri::command]
async fn add_folder(engine: State<'_, engine::Handle>, typed: String, project: Option<String>) -> Result<String, String> {
    let engine = engine.inner().clone();
    blocking(move || newfolder::add(&engine, &typed, project.as_deref()).map_err(words)).await?
}

/// Opens a repo's `.codealong/config.toml` from its main checkout in the editor.
#[tauri::command]
async fn open_team_file(engine: State<'_, engine::Handle>, repo: String) -> Result<(), String> {
    let engine = engine.inner().clone();
    blocking(move || engine.open_team_file(&repo).map_err(words)).await?
}

/// Saves config.toml as typed in Settings, once it parses.
#[tauri::command(async)]
fn save_config_text(engine: State<'_, engine::Handle>, text: String) -> Result<(), String> {
    engine.save_config_text(&text).map_err(words)
}

#[tauri::command]
async fn sign_in(engine: State<'_, engine::Handle>, name: String) -> Result<(), String> {
    let engine = engine.inner().clone();
    blocking(move || engine.sign_in(&name).map_err(words)).await?
}

/// Turns training off for an account at its provider (training.rs), answering where it stands
/// after.
#[tauri::command]
async fn stop_training(engine: State<'_, engine::Handle>, name: String) -> Result<training::Training, String> {
    let engine = engine.inner().clone();
    blocking(move || engine.stop_training(&name).map_err(words)).await?
}

/// The `codealong` command link and the skill per account, for Settings › Agents.
#[tauri::command]
async fn agents_status() -> Result<install::AgentsStatus, String> {
    blocking(move || install::status()).await
}

/// Links the command and copies the skill for the named accounts, then reports the new state.
#[tauri::command]
async fn install_agents(command: bool, accounts: Vec<String>) -> Result<install::AgentsStatus, String> {
    blocking(move || install::install(command, &accounts).map_err(words)).await?
}

/// Your other devices as Settings › Devices and the views show them (peers.rs).
#[tauri::command(async)]
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
#[tauri::command]
async fn link_device(peers: State<'_, peers::Peers>, key: String) -> Result<String, String> {
    let peers = peers.inner().clone();
    blocking(move || peers.link(&key)).await?
}

#[tauri::command]
async fn answer_link(peers: State<'_, peers::Peers>, key: String, accept: bool) -> Result<(), String> {
    let peers = peers.inner().clone();
    blocking(move || peers.answer_link(&key, accept)).await?
}

#[tauri::command]
async fn unlink_device(peers: State<'_, peers::Peers>, key: String) -> Result<(), String> {
    let peers = peers.inner().clone();
    blocking(move || peers.unlink(&key)).await?
}

/// A link code for a device that isn't on your tailnet.
#[tauri::command]
async fn device_link_code(peers: State<'_, peers::Peers>) -> Result<String, String> {
    let peers = peers.inner().clone();
    blocking(move || peers.link_code()).await?
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
#[tauri::command]
async fn link_with_code(peers: State<'_, peers::Peers>, code: String) -> Result<String, String> {
    let peers = peers.inner().clone();
    blocking(move || peers.link_with_code(&code)).await?
}

/// What was said in sessions on your other devices, searched on each of them.
#[tauri::command]
async fn search_devices(peers: State<'_, peers::Peers>, words: Vec<String>) -> Result<Vec<peers::DeviceFound>, String> {
    let peers = peers.inner().clone();
    blocking(move || {
        let words = search::tidy(&words);
        if words.is_empty() {
            return Vec::new();
        }
        peers.search(&words)
    })
    .await
}

/// Where each account can start a session, for the new-session sheet.
#[tauri::command]
async fn start_options(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>) -> Result<start::StartOptions, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::typed(actions::run_on(&engine, &peers, api::START_OPTIONS, None, serde_json::json!({}))?)).await?
}

/// Starts a session in a worktree: in the terminal, or in the Claude or Codex app. A start from
/// todos links each of them to the session, or to the next one of that account in the repo when
/// an app picks the session's ID.
#[tauri::command]
async fn start_session(app: tauri::AppHandle, engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, spec: start::StartSpec) -> Result<start::Started, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || {
        let started = actions::typed(actions::run(&engine, &peers, api::START, &spec.lane_id, actions::params::Start { spec: spec.clone() })?)?;
        // As an open from the main window does (`open_target`).
        if peerfeed::split_remote(&spec.lane_id).is_none() {
            if let Some(bundle) = quick::start_app(spec.place, &engine.board().apps.terminal_id) {
                front::bring_forward(&app, bundle);
            }
        }
        Ok(started)
    })
    .await?
}

/// Moves a session to another account of its harness (`start::move_to`): a copy goes on in a new
/// terminal under `account`, and the session is closed and archived. Or, with `lane`, to that
/// worktree (`start::move_to_folder`), where it goes on as itself. Its card on the desktop and
/// its kept window show a copy once the board has it, so neither says it's gone meanwhile.
#[tauri::command]
async fn move_session(app: tauri::AppHandle, engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, account: Option<String>, lane: Option<String>) -> Result<start::Started, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || {
        let got = actions::run(&engine, &peers, actions::MOVE, &key, actions::params::Move { account: account.unwrap_or_default(), lane: lane.unwrap_or_default() })?;
        let started: start::Started = serde_json::from_value(got).map_err(|e| e.to_string())?;
        if let Some(copy) = started.key.clone().filter(|c| *c != key) {
            std::thread::spawn(move || {
                // Its terminal takes a moment to start Claude Code, which then says it's there.
                let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
                while !engine.read(|b| b.session(&copy).is_some()) {
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

/// Forks a session: into a new terminal in its folder (`start::fork`), or, `background`, into a
/// copy that goes on without a window, on `prompt` when there's one (`start::fork_background`).
/// With `from`, a prompt's cursor, the copy goes up to just before that prompt and starts on
/// `prompt` in its place (`start::fork_from`).
#[tauri::command]
async fn fork_session(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, background: Option<bool>, prompt: Option<String>, from: Option<u64>) -> Result<start::Started, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || {
        let got = if let Some(cursor) = from {
            actions::run(&engine, &peers, api::SESSION_EDIT, &key, actions::params::Edit { cursor, prompt: prompt.unwrap_or_default(), ..Default::default() })?
        } else if background == Some(true) {
            actions::run(&engine, &peers, api::SESSION_FORK, &key, actions::params::Fork { prompt: prompt.unwrap_or_default(), ..Default::default() })?
        } else {
            actions::run(&engine, &peers, actions::FORK_TERMINAL, &key, serde_json::json!({}))?
        };
        serde_json::from_value(got).map_err(|e| e.to_string())
    })
    .await?
}

/// Asks something about a session on the side (side.rs): a throwaway copy of its conversation
/// answers, and the session carries on, unaware. Asked as a window shows one of its side
/// conversations (`from`), it goes on in that one. Returns the side conversation with the
/// question in it; the answer comes as `side` events, from the session's device for one on
/// another (`Msg::Side`).
#[tauri::command]
async fn side_ask(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, text: String, from: Option<String>) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SIDE_ASK, &key, actions::params::Side { text, from, ..Default::default() })).await?
}

/// A session's side conversation, for a window that starts showing the session: from its
/// device, for one on another.
#[tauri::command]
async fn side_get(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SIDE_GET, &key, actions::params::Session::default())).await?
}

/// The side conversations kept for a session, the latest first, to read after they're closed:
/// its device's, for one on another.
#[tauri::command]
async fn side_past(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<serde_json::Value, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SIDE_PAST, &key, actions::params::Session::default())).await?
}

/// Forgets one of a session's kept side conversations, on its device.
#[tauri::command]
async fn side_forget(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String, id: String) -> Result<(), String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SIDE_FORGET, &key, actions::params::SideForget { id, ..Default::default() }).map(|_| ())).await?
}

/// Closes a session's side conversation: a copy that's answering answers what it was asked and
/// then ends (side.rs `close`), and what was said is kept to read.
#[tauri::command]
async fn side_close(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, key: String) -> Result<(), String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::run(&engine, &peers, api::SIDE_CLOSE, &key, serde_json::json!({})).map(|_| ())).await?
}

/// Asks about a PR (`key`, `owner/name#number`) in a conversation of its own on `account`, on
/// `model` and `effort` where they're picked (side.rs `ask_pr`): an agent that reads the PR and
/// the code around it and changes nothing; asked as a window shows one of its conversations
/// (`from`), it goes on in that one.
/// Returns the conversation with the question in it; the answer comes as `side` events, as a
/// session's side conversation's does. Only here: the PR's card is in this Mac's windows.
#[tauri::command]
async fn pr_side_ask(engine: State<'_, engine::Handle>, key: String, account: String, model: Option<String>, effort: Option<String>, text: String, from: Option<String>) -> Result<side::Side, String> {
    let engine = engine.inner().clone();
    blocking(move || {
        let on = side::RunsOn { model: model.as_deref(), effort: effort.as_deref() };
        side::ask_pr(&engine.board(), &config::current().map_err(words)?, &key, &account, on, &text, from.as_deref()).map_err(words)
    })
    .await?
}

/// Closes a PR's conversation: an agent that's answering answers what it was asked and then
/// ends, and what was said is kept to read.
#[tauri::command(async)]
fn pr_side_close(key: String) {
    side::close(&key);
}

/// Where each account on another device can start a session, asked of that device.
#[tauri::command]
async fn device_start_options(engine: State<'_, engine::Handle>, peers: State<'_, peers::Peers>, device: String) -> Result<start::StartOptions, String> {
    let (engine, peers) = (engine.inner().clone(), peers.inner().clone());
    blocking(move || actions::typed(actions::run_on(&engine, &peers, api::START_OPTIONS, Some(&device), serde_json::json!({}))?)).await?
}

/// A project's boards, its own first, without their canvases (boards.rs).
#[tauri::command(async)]
fn boards_list(project: String) -> Result<Vec<boards::BoardDoc>, String> {
    boards::list(&project).map_err(words)
}

/// A change to a board. Every window reads its project's boards again (src/boards/useBoards.ts).
#[tauri::command(async)]
fn board_op(app: tauri::AppHandle, window: tauri::WebviewWindow, op: boards::BoardOp) -> Result<boards::Applied, String> {
    let applied = boards::apply(op, util::now_ms()).map_err(words)?;
    boards::changed(&app, &applied, Some(window.label()));
    Ok(applied)
}

/// A board's canvas as the window last saved it, or null.
#[tauri::command(async)]
fn canvas_load(board: String) -> Result<serde_json::Value, String> {
    boards::canvas(&board).map_err(words)
}

/// Saves a board's canvas. Another window showing the board reads it again (useCanvas.ts).
#[tauri::command(async)]
fn canvas_save(window: tauri::WebviewWindow, board: String, doc: serde_json::Value) -> Result<(), String> {
    boards::save_canvas(&board, doc).map_err(words)?;
    let _ = window.emit("canvas-saved", serde_json::json!({ "board": board, "from": window.label() }));
    Ok(())
}

/// The main window's answer to a `codealong board …` request (boards.rs `ask`).
#[tauri::command]
fn board_answer(id: u64, result: serde_json::Value, error: Option<String>) {
    boards::answer(id, error.map_or(Ok(result), Err));
}

/// A change to the schedules (schedules.rs `Op`): what it came to. The board shows it once the
/// engine reads the file again.
#[tauri::command(async)]
fn schedule_op(engine: State<'_, engine::Handle>, op: schedules::Op) -> Result<serde_json::Value, String> {
    let cfg = config::current().map_err(util::words)?;
    let board = engine.board();
    schedules::apply_op(op, None, Some(&board), &cfg, util::now_ms(), &schedules::rule::Local).map_err(util::words)
}

/// A schedule's time as it's being written: in words, and its next few times.
#[tauri::command]
fn schedule_times(rule: String) -> schedules::Times {
    schedules::times(&rule, util::now_ms(), &schedules::rule::Local)
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
async fn edit_file(engine: State<'_, engine::Handle>, path: String, line: Option<u64>) -> Result<(), String> {
    let engine = engine.inner().clone();
    blocking(move || launch::edit_file(&engine.your_editor().id, &util::expand(&path), line).map_err(words)).await?
}

/// Your editor, which a file's menu names in Open in.
#[tauri::command]
fn your_editor(engine: State<'_, engine::Handle>) -> config::Profile {
    engine.your_editor()
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
fn serve_image(app: tauri::AppHandle, request: tauri::http::Request<Vec<u8>>, responder: tauri::UriSchemeResponder) {
    let name = request.uri().path().trim_start_matches('/').to_string();
    let from = request.uri().query().and_then(|q| q.split('&').find_map(|p| p.strip_prefix("from="))).map(String::from);
    std::thread::spawn(move || {
        let path = format!("{}{name}", attachments::IMAGES);
        let here = attachments::locate(&path).and_then(|p| std::fs::read(p).ok());
        let found = here.or_else(|| {
            let device = from.filter(|_| attachments::valid_image(&name))?;
            let (engine, peers) = (app.try_state::<engine::Handle>()?, app.try_state::<peers::Peers>()?);
            actions::image_from(&engine, &peers, &device, &path).ok()
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

/// Saves the theme to config.toml. Every window follows, through the board.
#[tauri::command(async)]
fn set_theme(engine: State<'_, engine::Handle>, theme: config::Theme) -> Result<(), String> {
    engine.set_theme(theme).map_err(words)
}

/// Zooms every board window a step (zoom.rs), saved to config.toml; answers the zoom, in percent.
#[tauri::command(async)]
fn zoom(engine: State<'_, engine::Handle>, step: zoom::Step) -> Result<u32, String> {
    engine.zoom(step).map_err(words)
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

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let plainly = server::opened_plainly(&args);
    if plainly {
        if let Some(code) = install::open_as_the_dock_does() {
            std::process::exit(code);
        }
    }
    perf::arm();
    if let Err(e) = server::take_over(plainly) {
        eprintln!("{e}");
        std::process::exit(2);
    }
    tauri::Builder::default()
        .setup(|app| {
            crate::os::system::no_app_nap();
            // Set before the page loads, so a window never opens in the wrong theme.
            let theme = config::theme();
            app.set_theme(native_theme(theme));
            // And at its zoom: the main window is made before this runs.
            zoom::follow(app.handle(), config::zoom());
            let shown = std::sync::Mutex::new(theme);
            let shown_logo = std::sync::Mutex::new(model::LogoStatus::Idle);
            let floating = Floating::default();
            app.manage(floating.clone());
            let shortcuts = config::shortcuts();
            let shortcut = std::sync::Mutex::new(quick::Keys::ALL.map(|which| which.in_config(&shortcuts).to_string()));
            // Your other devices (peers.rs). They come before the engine, which publishes to them,
            // and ask it for the board and for answers once it's there.
            let engine_slot: std::sync::Arc<std::sync::OnceLock<engine::Handle>> = Default::default();
            let peers = {
                let (boards, answers, emits, runs, sides, floating) = (engine_slot.clone(), engine_slot.clone(), app.handle().clone(), engine_slot.clone(), app.handle().clone(), floating.clone());
                peers::Peers::new(
                    config::support_dir(),
                    peers::Hooks {
                        board: Box::new(move || boards.get().map(|e| e.board()).unwrap_or_default()),
                        answer: Box::new(move |method, params, watching| match answers.get() {
                            Some(e) => actions::answer(e, method, params, watching),
                            None => Err("Codealong is still starting on that device.".into()),
                        }),
                        emit: Box::new(move |view| {
                            // The engine goes by what they run too (nudges.rs `running_elsewhere`),
                            // and the windows that float show them beside the board as it is.
                            let there = |e: &mut shown::Elsewhere| e.on_devices(&view.lanes);
                            match runs.get() {
                                Some(engine) => {
                                    engine.peer_lanes(&view.lanes);
                                    engine.peer_machines(view.machines.clone());
                                    engine.read(|board| floating.publish(&emits, Some(board), util::now_ms(), there));
                                }
                                None => floating.publish(&emits, None, 0, there),
                            }
                            let _ = emits.emit("devices", view);
                        }),
                        side: Box::new(move |s| {
                            let _ = sides.emit("side", s);
                        }),
                    },
                )
            };
            let publishes = peers.clone();
            let emitter = app.handle().clone();
            let (mover, moves_with) = (std::sync::Mutex::new(limits::Mover::default()), engine_slot.clone());
            let (nudger, nudges_with) = (std::sync::Arc::new(std::sync::Mutex::new(nudges::Nudger::default())), engine_slot.clone());
            let ends_with = engine_slot.clone();
            let (continuer, continues_with) = (std::sync::Arc::new(std::sync::Mutex::new(reconnect::Continuer::default())), engine_slot.clone());
            let (rerequester, rerequests_with) = (std::sync::Arc::new(std::sync::Mutex::new(rerequest::Rerequester::default())), engine_slot.clone());
            let (reporter, reports_with) = (std::sync::Arc::new(std::sync::Mutex::new(helpers::Reporter::default())), engine_slot.clone());
            let (reviewer, reviews_with) = (std::sync::Arc::new(std::sync::Mutex::new(autoreview::Reviewer::load())), engine_slot.clone());
            app.manage(Reviews(reviewer.clone()));
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
                zoom::follow(&emitter, board.zoom);
                // Registering waits on the main thread, so it gets a thread of its own: this one
                // may hold what a command on the main thread is waiting for.
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
                let _ = emitter.emit("board", board);
                // The bar with a sound for what came to wait on you (sounds.rs), the desktop cards
                // and the lines between them.
                floating.publish(&emitter, Some(board), board.generated_at, |_| {});
                publishes.publish(board);
                // Only once there's a handle to move them with, so a board before that leaves them due.
                if let (Some(engine), Ok(mut mover)) = (moves_with.get(), mover.lock()) {
                    for (key, to) in mover.due(board, board.generated_at) {
                        let engine = engine.clone();
                        std::thread::spawn(move || limits::move_now(&engine, &key, &to));
                    }
                }
                // The same for news on PRs with their nudge on: one thread for the batch, since two
                // sends typing into one terminal at once would interleave, and it says how each
                // went, so what wasn't told is tried again.
                if let Some(engine) = nudges_with.get() {
                    let due = nudger.lock().map(|mut n| n.due(board, board.generated_at)).unwrap_or_default();
                    if !due.is_empty() {
                        let (engine, nudger) = (engine.clone(), nudger.clone());
                        std::thread::spawn(move || nudges::send_now(&engine, &nudger, due));
                    }
                }
                // And sessions a lost connection stopped, once their API can be reached, a usage
                // limit, once it has lifted, or Codealong quitting, at once: one thread for the
                // batch, for the same reason, which tries the API first.
                if let Some(engine) = continues_with.get() {
                    let cut_offs = config::current().is_ok_and(|c| c.sessions.continue_after_restart);
                    let due = continuer.lock().map(|mut c| c.due(board, board.generated_at, cut_offs)).unwrap_or_default();
                    if !due.is_empty() {
                        let (engine, continuer) = (engine.clone(), continuer.clone());
                        std::thread::spawn(move || reconnect::send_now(&engine, &continuer, due));
                    }
                }
                // And reviewers of PRs of yours whose fixes went unsaid, asked to look again where
                // config.toml says to (rerequest.rs): one thread for the batch, as GitHub is asked.
                if let (Some(engine), Ok(cfg)) = (rerequests_with.get(), config::current()) {
                    let due = rerequester.lock().map(|mut r| r.due(board, &cfg.pull_requests, board.generated_at)).unwrap_or_default();
                    if !due.is_empty() {
                        let (engine, rerequester) = (engine.clone(), rerequester.clone());
                        std::thread::spawn(move || rerequest::send_now(&engine, &rerequester, due));
                    }
                }
                // And sessions whose helpers report to them (helpers.rs): a turn that ended, a wait
                // on you that began, or a start stuck before its session. One thread for the batch,
                // for the same reason.
                if let Some(engine) = reports_with.get() {
                    let pending = &origins::current().pending;
                    let due = reporter.lock().map(|mut r| r.due(pending, board, board.generated_at)).unwrap_or_default();
                    if !due.is_empty() {
                        let (engine, reporter) = (engine.clone(), reporter.clone());
                        std::thread::spawn(move || helpers::send_now(&engine, &reporter, due));
                    }
                }
                // PRs that wait on your review, reviewed without being asked when config.toml says
                // to (autoreview.rs): a thread for each job, since reading GitHub, starting a
                // session and posting each take a while; one PR's jobs never overlap.
                if let Some(engine) = reviews_with.get() {
                    let due = reviewer.lock().map(|mut r| r.due(board, &board.reviews, board.generated_at)).unwrap_or_default();
                    for job in due {
                        let (engine, reviewer) = (engine.clone(), reviewer.clone());
                        std::thread::spawn(move || autoreview::run(&engine, &reviewer, job));
                    }
                }
                // Sessions that asked from the command to be ended once their turn ended
                // (agentdone.rs); closing a terminal takes a moment, so off this thread.
                if let Some(engine) = ends_with.get() {
                    for key in agentdone::due(board) {
                        let engine = engine.clone();
                        std::thread::spawn(move || match actions::end_after_turn(&engine, &key) {
                            Ok(said) => eprintln!("Ended {key} once its turn did, as it asked: {said}"),
                            Err(e) => eprintln!("Couldn't end {key} once its turn did: {e}"),
                        });
                    }
                }

            });
            let _ = engine_slot.set(engine.clone());
            // Schedules (schedules/runner.rs): what's due, every few seconds, for the life of the app.
            schedules::runner::spawn(engine.clone());
            // Automatic reviews' sessions once they're done with (reviewdone.rs): their PR merged,
            // closed or gone, or they sat unused. It goes by the clock, so on a thread of its own.
            reviewdone::spawn(engine.clone());
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
            // A side conversation's answer, as its copy writes it, for the windows showing its
            // session and the linked devices that asked about it; the board again when what it
            // lists of them changes.
            {
                let (app, engine, askers) = (app.handle().clone(), engine.clone(), peers.clone());
                side::init(
                    move |s| {
                        askers.side(&s);
                        let _ = app.emit("side", s);
                    },
                    move || engine.sides_changed(),
                );
            }
            app.manage(peers);
            server::serve(engine.clone(), app.state::<peers::Peers>().inner().clone(), app.handle().clone());
            // What a command you ran for a session printed goes to that session once it ends, as
            // your message: the `!` you ran it with is for the session to read it.
            {
                let (engine, peers) = (engine.clone(), app.state::<peers::Peers>().inner().clone());
                let board = engine.clone();
                terminal::report_ended(move || board.read(|b| !b.loading), move |run, text| {
                    let command = util::one_line(&run.run.command, 60);
                    let told = tell_run(&engine, &peers, &run.run, text).map_err(|why| format!("What `{command}` printed didn't reach “{}”: {why} `codealong terminal read {}` has it.", run.run.session_title, run.run.id));
                    engine.run_reported(told);
                });
            }
            // After the first scan and the first git runs, which have the Mac to themselves.
            search::spawn_warmer(std::time::Duration::from_secs(20));
            // The token ledger reads every record once, so it waits for the search index's first read.
            ledger::spawn(std::time::Duration::from_secs(45));
            app.manage(engine);
            std::thread::spawn(|| {
                install::follow_command_link();
                install::refresh_installed();
                acp::refresh_adapters();
            });
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
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                quick::pressed(app, shortcut);
            }
        }).build())
        .on_window_event(|window, event| {
            front::on_window_event(window, event);
            bar::on_window_event(window, event);
            quick::on_window_event(window, event);
            waiting::on_window_event(window, event);
            peek::on_window_event(window, event);
            desktop::on_window_event(window, event);
            kept::on_window_event(window, event);
        })
        .register_asynchronous_uri_scheme_protocol("codealong-image", |ctx, request, responder| serve_image(ctx.app_handle().clone(), request, responder))
        .invoke_handler(tauri::generate_handler![get_board, session_at, pr_at, token_report, session_tokens, idle_sessions, close_idle_sessions, stale_terminals, close_stale_terminals, close_session, session_done, open_target, open_config, editor_icons, mark_seen, mark_unread, set_pr_nudge, set_pr_primary, snooze_sessions, quiet_sessions, own_sessions, snooze_reviews, refresh, refresh_usage, set_pinned, set_pinned_worktrees, set_pinned_prs, set_pinned_repos, set_preview, set_reusable, remove_worktree, archive_session, rename_session, dismiss_suggestions, pr_detail, pr_since, pr_card, pr_comment_asks, pr_agent_reviews, send_agent_review, review_pr_now, review_pr_again, review_pr, pr_draft, edit_draft, edit_draft_comment, drop_draft_comment, merge_box, merge_pr, review_in_codex, open_url, open_pr_file, run_command, shell_commands, get_accounts, save_accounts, order_accounts, keep_start_place, found_accounts, setup_view, setup_needed, finish_setup, get_config, save_config, save_config_text, play_sound, take_team_settings, open_team_file, look_folder, add_folder, sign_in, stop_training, agents_status, install_agents, start_options, device_start_options, start_session, fork_session, move_session, side_ask, side_get, side_past, side_forget, side_close, pr_side_ask, pr_side_close, todo_op, schedule_op, schedule_times, boards_list, board_op, canvas_load, canvas_save, board_answer, save_attachment, import_attachment, open_attachment, open_file, reveal_file, edit_file, your_editor, path_kinds, expand_attachments, set_theme, zoom, open_project_window, fit_bar, show_main, peek_open, peek_ready, peek_fit, peek_focus, peek_hide, peek_resize, peek_drag, keep_session, kept_session, kept_ready, kept_focus, kept_resize, kept_close, side_window, side_windows, bar_feed, bar_on, set_bar_on, bar_mode, set_bar_mode, set_notch_open, notch_keys, keep_notch_session, notch_shape, notch_drag, open_from_bar, desktop_pins, pin_to_desktop, unpin_from_desktop, drag_to_desktop, desktop_feed, fit_desktop, desktop_hover, lineage_feed, show_quick, hide_quick, quick_ready, show_waiting, waiting_asked, start_from_quick, route_quick, note_route, skills, session_skills, search_text, search_devices, devices, set_device_color, link_device, answer_link, unlink_device, device_link_code, link_code_qr, link_with_code, session_page, session_newer, session_pending, session_way, session_answer, session_stream, session_cancel, session_steer, send_to_session, resume_session, switch_session_model, switch_session_effort, switch_session_mode, suggest_todo, dismiss_todo_hint, jev_key, set_jev_key, test_jev, jev_usage, quick_shortcut, pause_quick_shortcut, perf_mark, page_unfocused, pointer_probe, popup_menu, update_status, check_for_updates, restart_to_update, open_at_login, set_open_at_login])
        .build(tauri::generate_context!())
        .expect("error while running Codealong")
        .run(|app, event| {
            // The Dock icon brings the main window back after it was closed.
            match event {
                ref reopen if os::windows::reopened(reopen) => {
                    let _ = front::show_main(app, None);
                }
                // Where the waiting window is, written when it's put away, for one that's up as
                // Codealong quits.
                tauri::RunEvent::Exit => waiting::save(app),
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

    #[test]
    fn link_codes_draw_as_qr_codes() {
        let code = irohnet::Ticket::new(iroh::SecretKey::generate().public()).encode();
        let svg = link_qr_svg(&code).unwrap();
        assert!(svg.contains("<svg") && svg.contains("#000000") && svg.contains("#ffffff"), "{svg}");
    }
}

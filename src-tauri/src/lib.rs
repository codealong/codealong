//! Codealong: every agent runs in a worktree, and every worktree is a lane.

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
mod config;
mod detect;
mod engine;
mod git;
mod github;
mod install;
mod insights;
mod jev;
mod launch;
mod logo;
mod model;
mod perf;
mod prasks;
mod procs;
mod quick;
mod repos;
mod send;
mod route;
mod search;
mod server;
mod start;
mod state;
mod throttle;
mod todojev;
mod todos;
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
fn open_target(engine: State<'_, engine::Handle>, lane_id: String, target_id: String) -> Result<(), String> {
    engine.open(&lane_id, &target_id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn open_config(engine: State<'_, engine::Handle>) -> Result<(), String> {
    engine.open_config().map_err(|e| e.to_string())
}

#[tauri::command]
fn mark_seen(engine: State<'_, engine::Handle>, keys: Vec<String>) {
    engine.mark_seen(keys);
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
    engine.open(&lane_id, &target_id).map_err(|e| e.to_string())?;
    if let Some(bundle) = bar::target_app(&target_id, &engine.board().apps.terminal_id) {
        bar::bring_forward(&app, bundle);
    }
    Ok(())
}

/// What the floating bar shows, for its first render; after that it gets `bar-feed` events.
#[tauri::command]
fn bar_feed(engine: State<'_, engine::Handle>) -> barfeed::BarFeed {
    barfeed::feed(&engine.board())
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
/// bring the app it started in forward: the panel never makes Codealong the active app.
#[tauri::command(async)]
fn start_from_quick(app: tauri::AppHandle, engine: State<'_, engine::Handle>, spec: start::StartSpec) -> Result<start::Started, String> {
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
    search::find(&words, 200)
}

/// A session's last few messages, yours and the agent's, for its panel on the canvas.
#[tauri::command(async)]
fn session_messages(key: String) -> Vec<search::Said> {
    search::recent(&key, 6)
}

/// Sends a prompt to a session that's already going, where it runs (send.rs).
#[tauri::command(async)]
fn send_to_session(engine: State<'_, engine::Handle>, key: String, text: String) -> Result<send::Sent, String> {
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    send::send(&engine.board(), &cfg, &key, &text).map_err(|e| format!("{e:#}"))
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

#[tauri::command(async)]
fn archive_session(engine: State<'_, engine::Handle>, key: String, archived: bool) -> Result<(), String> {
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
#[tauri::command(async)]
fn start_options() -> Result<start::StartOptions, String> {
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    Ok(start::options_for(&cfg))
}

/// Starts a session in a worktree: in the terminal, or in the Claude or Codex app. A start from
/// a todo links the todo to the session, or to the next one of that account in the repo when an
/// app picks the session's ID.
#[tauri::command(async)]
fn start_session(engine: State<'_, engine::Handle>, spec: start::StartSpec) -> Result<start::Started, String> {
    start_and_link(&engine, &spec)
}

/// Starts a session and links it to its todo; the socket (server.rs) does the same for the
/// `codealong start` command.
pub(crate) fn start_and_link(engine: &engine::Handle, spec: &start::StartSpec) -> Result<start::Started, String> {
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    let started = start::start(&engine.board(), &cfg, spec).map_err(|e| format!("{e:#}"))?;
    if let Some(id) = spec.todo.clone() {
        let pending = match (&started.key, spec.place) {
            (Some(_), _) => None,
            (None, place) => Some(todos::PendingStart {
                lane: spec.lane_id.clone(),
                harness: if place == start::Place::CodexApp { "codex" } else { "claude" }.into(),
                account: spec.account.clone(),
                at: util::now_ms(),
            }),
        };
        // A session in a new worktree won't be in the one it started from: only it is linked.
        let lane = (!(spec.worktree && spec.place == start::Place::Terminal)).then(|| spec.lane_id.clone());
        engine.todo_started(id, lane, started.key.clone(), pending);
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

/// The board window in front, which a menu command is for. The floating bar and the quick panel
/// never are.
fn front_window(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    app.webview_windows().into_values().find(|w| w.label() != bar::LABEL && w.label() != quick::LABEL && w.is_focused().unwrap_or(false))
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

/// Puts New window (⌘⇧N) at the top of the File menu. Choosing it asks the front window for
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
    file.prepend_items(&[&item, &PredefinedMenuItem::separator(app)?])?;
    app.on_menu_event(|app, event| {
        if event.id().as_ref() != NEW_WINDOW_MENU_ID {
            return;
        }
        match front_window(app) {
            Some(w) => {
                let _ = app.emit_to(w.label(), "new-window", ());
            }
            None => {
                let _ = bar::show_main(app, None);
            }
        }
    });
    Ok(())
}

const NEW_WINDOW_MENU_ID: &str = "new-window";

/// Every second while the perf log is on, posts to the main thread and logs how late it ran:
/// the main thread carries every click and pointer move to the windows, so its lag is theirs.
fn watch_main_thread(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if !perf::enabled() {
            continue;
        }
        let t = std::time::Instant::now();
        let _ = app.run_on_main_thread(move || perf::lag("main thread", t.elapsed()));
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
            });
            server::serve(engine.clone());
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
        })
        .register_asynchronous_uri_scheme_protocol("codealong-image", |_ctx, request, responder| serve_image(request, responder))
        .invoke_handler(tauri::generate_handler![get_board, open_target, open_config, mark_seen, refresh, refresh_usage, set_pinned, set_pinned_worktrees, archive_session, pr_detail, pr_comment_asks, approve_pr, review_in_codex, open_url, get_accounts, save_accounts, found_accounts, get_config, save_config, save_config_text, sign_in, agents_status, install_agents, start_options, start_session, todo_op, canvas_load, canvas_save, save_attachment, import_attachment, open_attachment, expand_attachments, set_theme, open_project_window, fit_bar, show_main, bar_feed, bar_on, set_bar_on, open_from_bar, show_quick, hide_quick, quick_ready, fit_quick, start_from_quick, route_quick, search_text, session_messages, send_to_session, suggest_todo, dismiss_todo_hint, jev_key, set_jev_key, test_jev, quick_shortcut, pause_quick_shortcut, perf_mark])
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
/// `codealong --snapshot --start-options`: where each account can start a session.
/// `codealong --snapshot --start-plan <folder> <account> <place> [prompt] [--model <model>]
/// [--worktree]`: what starting one would do, without doing it.
pub fn snapshot_cli(args: &[String]) -> i32 {
    let started = std::time::Instant::now();
    if args.iter().any(|a| a == "--start-options") {
        let cfg = config::load().ok().flatten().unwrap_or_default();
        println!("{}", serde_json::to_string_pretty(&start::options_for(&cfg)).unwrap_or_default());
        return 0;
    }
    if let Some(i) = args.iter().position(|a| a == "--start-plan") {
        let help = "usage: codealong --snapshot --start-plan <folder> <account> <terminal|claude-app|codex-app> [prompt] [--model <model>] [--worktree]";
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
        let prompt = args.get(i + 4).filter(|p| !p.starts_with("--")).cloned().unwrap_or_default();
        let worktree = args.iter().any(|a| a == "--worktree");
        let plan = start::plan(account, place, &util::expand(folder), &prompt, &start::new_session_id(), model.as_deref(), worktree);
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
}

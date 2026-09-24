//! Codealong: every agent runs in a worktree, and every worktree is a lane.

mod appserver;
mod bar;
mod board;
mod claude;
mod claudeapp;
mod codex;
mod config;
mod engine;
mod git;
mod github;
mod images;
mod launch;
mod model;
mod perf;
mod procs;
mod repos;
mod start;
mod state;
mod throttle;
mod todos;
mod usage;
mod util;

use tauri::{Emitter, Manager, State};

/// `Kestrel` → `project-kestrel`: window labels allow letters, digits, `-`, `/`, `:` and `_`.
fn project_window_label(project: &str) -> String {
    let slug: String = project.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    format!("project-{}", slug.trim_matches('-'))
}

/// Opens a project in a window of its own, or brings that window forward. The window reads its
/// project from `window.__CODEALONG_PROJECT__`, set before the page loads.
#[tauri::command(async)]
fn open_project_window(app: tauri::AppHandle, project: String) -> Result<(), String> {
    let label = project_window_label(&project);
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.unminimize();
        let _ = w.show();
        return w.set_focus().map_err(|e| e.to_string());
    }
    let script = format!("window.__CODEALONG_PROJECT__ = {};", serde_json::to_string(&project).map_err(|e| e.to_string())?);
    let mut builder = tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App("index.html".into()))
        .title(format!("Codealong · {project}"))
        .inner_size(1320.0, 860.0)
        .min_inner_size(980.0, 600.0)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(tauri::LogicalPosition::new(18.0, 20.0))
        .initialization_script(&script);
    // Cascade from the main window so the new one doesn't sit exactly on top of it.
    if let Some(main) = app.get_webview_window("main") {
        if let (Ok(pos), Ok(scale)) = (main.outer_position(), main.scale_factor()) {
            let at = pos.to_logical::<f64>(scale);
            let step = 28.0 * app.webview_windows().len() as f64;
            builder = builder.position(at.x + step, at.y + step);
        }
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
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    let started = start::start(&engine.board(), &cfg, &spec).map_err(|e| format!("{e:#}"))?;
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
        engine.todo_started(id, spec.lane_id.clone(), started.key.clone(), pending);
    }
    Ok(started)
}

/// A change to a todo or to the labels. Returns the time of the first board that shows it.
#[tauri::command(async)]
fn todo_op(engine: State<'_, engine::Handle>, op: todos::TodoOp) -> Result<i64, String> {
    engine.todo(op)
}

/// Keeps an image pasted into a description or a prompt. The window sends the file's bytes as
/// they are, not as JSON; the reply is the name text refers to it by.
#[tauri::command(async)]
fn save_image(request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("That wasn't an image.".into());
    };
    images::save(bytes).map_err(|e| format!("{e:#}"))
}

/// Keeps a copy of an image file dropped from the Finder.
#[tauri::command(async)]
fn import_image(path: String) -> Result<String, String> {
    images::import(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))
}

/// Text with its images as full paths, for copying a todo as a prompt.
#[tauri::command]
fn expand_images(text: String) -> String {
    images::expand(&text, images::Style::Path)
}

/// Opens a kept image in the app that opens images (Preview, usually).
#[tauri::command(async)]
fn open_image(name: String) -> Result<(), String> {
    let path = images::file(&name).ok_or("That isn't one of Codealong's images.")?;
    launch::open_url(&path.to_string_lossy()).map_err(|e| e.to_string())
}

/// Serves kept images to the windows at `codealong-image://localhost/<name>`. Only names
/// Codealong made are read, so nothing outside the images folder can be.
fn serve_image(request: tauri::http::Request<Vec<u8>>, responder: tauri::UriSchemeResponder) {
    let name = request.uri().path().trim_start_matches('/').to_string();
    std::thread::spawn(move || {
        let found = images::file(&name).and_then(|p| std::fs::read(p).ok());
        let response = match found {
            Some(bytes) => tauri::http::Response::builder()
                .header(tauri::http::header::CONTENT_TYPE, images::mime(&name))
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
        let front = app.webview_windows().into_values().find(|w| w.label() != bar::LABEL && w.is_focused().unwrap_or(false));
        match front {
            Some(w) => {
                let _ = w.emit("settings", ());
            }
            None => {
                let _ = bar::show_main(app, None);
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.emit("settings", ());
                }
            }
        }
    });
    Ok(())
}

const SETTINGS_MENU_ID: &str = "settings";

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
            let emitter = app.handle().clone();
            let engine = engine::start(move |board| {
                if let Ok(mut shown) = shown.lock() {
                    if *shown != board.theme {
                        *shown = board.theme;
                        emitter.set_theme(native_theme(board.theme));
                    }
                }
                let _ = emitter.emit("board", board);
            });
            app.manage(engine);
            if let Err(e) = bar::setup(app.handle()) {
                eprintln!("Couldn't make the floating bar: {e:#}");
            }
            if let Err(e) = add_settings_menu(app.handle()) {
                eprintln!("Couldn't add Settings to the menu: {e:#}");
            }
            watch_main_thread(app.handle().clone());
            Ok(())
        })
        .on_window_event(bar::on_window_event)
        .register_asynchronous_uri_scheme_protocol("codealong-image", |_ctx, request, responder| serve_image(request, responder))
        .invoke_handler(tauri::generate_handler![get_board, open_target, open_config, mark_seen, refresh, refresh_usage, set_pinned, set_pinned_worktrees, archive_session, pr_detail, approve_pr, review_in_codex, open_url, get_accounts, save_accounts, get_config, save_config, save_config_text, sign_in, start_options, start_session, todo_op, save_image, import_image, open_image, expand_images, set_theme, open_project_window, fit_bar, show_main, bar_on, set_bar_on, perf_mark])
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

/// `codealong --snapshot [--table] [--no-github] [--usage] [--accounts]`: one pass over everything,
/// printed. Claude usage is read only with `--usage`.
/// `codealong --snapshot --config`: the settings and config.toml, as Settings gets them.
/// `codealong --snapshot --open <lane id> <target id>`: opens a target the way a click does.
/// `codealong --snapshot --pr <owner/name> <number>`: a PR's detail, as the PR panel gets it.
/// `codealong --snapshot --start-options`: where each account can start a session.
/// `codealong --snapshot --start-plan <folder> <account> <place> [prompt] [--model <model>]`: what
/// starting one would do, without doing it.
pub fn snapshot_cli(args: &[String]) -> i32 {
    let started = std::time::Instant::now();
    if args.iter().any(|a| a == "--start-options") {
        let cfg = config::load().ok().flatten().unwrap_or_default();
        println!("{}", serde_json::to_string_pretty(&start::options_for(&cfg)).unwrap_or_default());
        return 0;
    }
    if let Some(i) = args.iter().position(|a| a == "--start-plan") {
        let help = "usage: codealong --snapshot --start-plan <folder> <account> <terminal|claude-app|codex-app> [prompt] [--model <model>]";
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
        let plan = start::plan(account, place, &util::expand(folder), &prompt, &start::new_session_id(), model.as_deref());
        println!("{}", serde_json::to_string_pretty(&plan).unwrap_or_default());
        return 0;
    }
    if args.iter().any(|a| a == "--config") {
        println!("{}", serde_json::to_string_pretty(&config::view()).unwrap_or_default());
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

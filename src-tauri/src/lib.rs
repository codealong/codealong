//! Codealong: every agent runs in a worktree, and every worktree is a lane.

mod appserver;
mod board;
mod claude;
mod claudeapp;
mod codex;
mod config;
mod engine;
mod git;
mod github;
mod launch;
mod model;
mod procs;
mod repos;
mod start;
mod state;
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

/// Starts a session in a worktree: in the terminal, or in the Claude or Codex app.
#[tauri::command(async)]
fn start_session(engine: State<'_, engine::Handle>, spec: start::StartSpec) -> Result<start::Started, String> {
    let cfg = config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default();
    start::start(&engine.board(), &cfg, &spec).map_err(|e| format!("{e:#}"))
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let emitter = app.handle().clone();
            let engine = engine::start(move |board| {
                let _ = emitter.emit("board", board);
            });
            app.manage(engine);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_board, open_target, open_config, mark_seen, refresh, refresh_usage, set_pinned, set_pinned_worktrees, archive_session, pr_detail, approve_pr, review_in_codex, open_url, get_accounts, save_accounts, sign_in, start_options, start_session, open_project_window])
        .run(tauri::generate_context!())
        .expect("error while running Codealong");
}

/// `codealong --snapshot [--table] [--no-github] [--usage] [--accounts]`: one pass over everything,
/// printed. Claude usage is read only with `--usage`.
/// `codealong --snapshot --open <lane id> <target id>`: opens a target the way a click does.
/// `codealong --snapshot --pr <owner/name> <number>`: a PR's detail, as the PR panel gets it.
/// `codealong --snapshot --start-options`: where each account can start a session.
/// `codealong --snapshot --start-plan <folder> <account> <place> [prompt]`: what starting one
/// would do, without doing it.
pub fn snapshot_cli(args: &[String]) -> i32 {
    let started = std::time::Instant::now();
    if args.iter().any(|a| a == "--start-options") {
        let cfg = config::load().ok().flatten().unwrap_or_default();
        println!("{}", serde_json::to_string_pretty(&start::options_for(&cfg)).unwrap_or_default());
        return 0;
    }
    if let Some(i) = args.iter().position(|a| a == "--start-plan") {
        let help = "usage: codealong --snapshot --start-plan <folder> <account> <terminal|claude-app|codex-app> [prompt]";
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
        let prompt = args.get(i + 4).cloned().unwrap_or_default();
        let plan = start::plan(account, place, &util::expand(folder), &prompt, &start::new_session_id());
        println!("{}", serde_json::to_string_pretty(&plan).unwrap_or_default());
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

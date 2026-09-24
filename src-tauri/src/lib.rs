//! Codealong: every agent runs in a worktree, and every worktree is a lane.

mod board;
mod claude;
mod codex;
mod config;
mod engine;
mod git;
mod github;
mod launch;
mod model;
mod procs;
mod repos;
mod state;
mod usage;
mod util;

use tauri::{Emitter, Manager, State};

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
        .invoke_handler(tauri::generate_handler![get_board, open_target, open_config, mark_seen, refresh, refresh_usage, get_accounts, save_accounts, sign_in])
        .run(tauri::generate_context!())
        .expect("error while running Codealong");
}

/// `codealong --snapshot [--table] [--no-github] [--accounts]`: one pass over everything, printed.
pub fn snapshot_cli(args: &[String]) -> i32 {
    let started = std::time::Instant::now();
    if args.iter().any(|a| a == "--accounts") {
        let cfg = config::load().ok().flatten().unwrap_or_default();
        let list: Vec<model::AccountInfo> = cfg.accounts.iter().map(usage::describe).collect();
        println!("{}", serde_json::to_string_pretty(&list).unwrap_or_default());
        return 0;
    }
    let board = engine::snapshot(!args.iter().any(|a| a == "--no-github"));
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

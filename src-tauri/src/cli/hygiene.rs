//! `codealong hygiene`: the processes old sessions left behind, with this machine's load, memory
//! and swap (hygiene.rs). Nothing is judged on a board read from the files, where every session
//! looks old: it takes the server's, once its first scan is done.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{anyhow, Result};
use serde::Serialize;

use crate::model::{Board, Session};
use crate::procs;
use crate::server::{self, Request};
use crate::util::{self, expand, now_ms, tilde, truncate};

use super::*;

// ---- hygiene ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HygieneOut {
    system: crate::os::system::System,
    /// The app's board answered, so processes were judged.
    judged: bool,
    notes: Vec<String>,
    /// What's left over from old sessions, the most memory first: what --kill ends.
    found: Vec<crate::hygiene::Found>,
    /// Quiet sessions' processes left running, each waiting on a check it scheduled itself.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    kept: Vec<crate::hygiene::Kept>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ended: Option<Vec<crate::hygiene::Ended>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    log: Option<String>,
}

pub(super) fn hygiene(c: &mut Client, a: &HygieneArgs, json: bool) -> Result<()> {
    use crate::hygiene;
    let after_ms = crate::idle::parse_after(&a.after).ok_or_else(|| anyhow!("--after is like 90m, 24h, 3d or 1w, not {}.", a.after))?;
    let mut out = HygieneOut { system: crate::os::system::system(), judged: false, notes: Vec::new(), found: Vec::new(), kept: Vec::new(), ended: None, log: None };
    // Without the app's board every session would look old, so nothing is judged on a light one.
    let app_pid = c.app.as_ref().map(|s| s.pid);
    let ready = match app_pid {
        Some(_) => {
            c.board()?;
            c.from_app && !c.board()?.loading
        }
        None => false,
    };
    let procs = match (ready, app_pid) {
        (true, _) => hygiene::processes().map_err(|e| out.notes.push(format!("`ps` didn't answer ({}), so nothing was judged.", util::words(e)))).ok(),
        (false, None) => {
            out.notes.push("Codealong isn't running, so nothing was judged or ended: without its board every session looks old.".into());
            None
        }
        (false, Some(_)) => {
            out.notes.push("Codealong didn't answer, or is still reading after a start, so nothing was judged or ended.".into());
            None
        }
    };
    if let Some(procs) = procs {
        let mut protected: HashSet<u32> = procs::ancestors(std::process::id()).into_iter().collect();
        protected.extend(app_pid);
        let listening: HashSet<u32> = procs::listeners().into_iter().map(|l| l.pid).collect();
        let cfg = c.cfg()?.clone();
        let cwd = |pid: u32| procs::cwd_of(pid).map(|p| p.to_string_lossy().into_owned());
        let linked = |dir: &str| Path::new(dir).join(".git").is_file();
        let wakes = |s: &Session, since: i64, now: i64| {
            let account = cfg.accounts.iter().find(|a| a.name == s.account)?;
            crate::harness::adapter(s.harness).wakes_at(&crate::harness::Ctx { account }, s, since, now)
        };
        let judged = hygiene::judge(&hygiene::Scene { board: c.board()?, procs: &procs, now: now_ms(), after_ms, protected: &protected, cwd: &cwd, listening: &listening, linked: &linked, wakes: &wakes });
        let mut found = judged.found;
        out.kept = judged.kept;
        for f in &mut found {
            f.memory = std::iter::once(f.pid).chain(f.children.iter().copied()).filter_map(crate::os::system::footprint).sum();
        }
        found.sort_by_key(|f| std::cmp::Reverse(f.memory));
        out.judged = true;
        if a.kill && !found.is_empty() {
            // Judged again on the processes and the board as they are now, read in that order,
            // just before anything is ended: `ps` takes a while on a busy Mac, and a session that
            // took a turn meanwhile stays.
            let table = hygiene::processes();
            c.fresh_board()?;
            match (table, c.from_app && !c.board()?.loading) {
                (Ok(table), true) => {
                    let board = c.board()?;
                    let again = hygiene::judge(&hygiene::Scene { board, procs: &table, now: now_ms(), after_ms, protected: &protected, cwd: &cwd, listening: &listening, linked: &linked, wakes: &wakes });
                    let due: Vec<hygiene::Found> = again.found.into_iter().filter(|g| found.iter().any(|f| g.pid == f.pid && g.kind == f.kind)).map(|g| hygiene::Found { memory: found.iter().find(|f| f.pid == g.pid).map_or(0, |f| f.memory), ..g }).collect();
                    let fresh = || match server::ask(&Request::Board) {
                        Some(Ok(v)) => serde_json::from_value::<Board>(v).ok().filter(|b| !b.loading),
                        _ => None,
                    };
                    let ended = hygiene::end(&due, &table, &protected, board, &cfg, &fresh);
                    let path = a.log.as_deref().map(expand).unwrap_or_else(|| crate::config::logs_dir().join("kills.log"));
                    if let Err(e) = hygiene::log(&path, &ended, &due) {
                        out.notes.push(format!("Couldn't log to {}: {e}", path.display()));
                    }
                    out.log = Some(path.to_string_lossy().into_owned());
                    out.ended = Some(ended);
                }
                (Err(e), _) => out.notes.push(format!("`ps` didn't answer when asked again before ending them ({}), so nothing was ended.", util::words(e))),
                (_, false) => out.notes.push("Codealong didn't answer when asked again before ending them, so nothing was ended.".into()),
            }
        }
        out.found = found;
    }
    if json {
        return print_json(&out);
    }
    print_hygiene(&out);
    Ok(())
}

fn print_hygiene(out: &HygieneOut) {
    let s = &out.system;
    let gb = |b: u64| b as f64 / 1_073_741_824.0;
    let mb = |b: u64| format!("{} MB", b / 1_048_576);
    let free = s.free_percent.map(|p| format!(", {p}% free")).unwrap_or_default();
    let compressed = s.compressed.map(|c| format!(", {:.1} GB compressed", gb(c))).unwrap_or_default();
    let swap = if s.swap_total > 0 { format!("swap {:.1} of {:.1} GB used", gb(s.swap_used), gb(s.swap_total)) } else { "no swap".into() };
    println!("Load {:.0} {:.0} {:.0} on {} CPUs · memory {:.0} GB{free}{compressed} · {swap}", s.load[0], s.load[1], s.load[2], s.cpus, gb(s.memory));
    for n in &out.notes {
        println!("Note: {n}");
    }
    if !out.judged {
        return;
    }
    match &out.ended {
        Some(ended) => {
            let gone: Vec<&crate::hygiene::Ended> = ended.iter().filter(|e| e.result == "ended").collect();
            println!("\nEnded {} of {} ({:.1} GB):", gone.len(), ended.len(), gb(gone.iter().map(|e| e.memory).sum()));
            let rows = ended.iter().map(|e| vec![mb(e.memory), e.pid.to_string(), e.result.clone(), word(&e.kind), truncate(&e.why, 90)]).collect();
            print!("{}", table_text(&["MEMORY", "PID", "RESULT", "KIND", "WHY"], rows));
            if let Some(log) = &out.log {
                println!("Logged in {}", tilde(Path::new(log)));
            }
        }
        None if out.found.is_empty() => println!("\nNothing left over from old sessions."),
        None => {
            println!("\nLeft over from old sessions, {:.1} GB (--kill ends them):", gb(out.found.iter().map(|f| f.memory).sum()));
            let rows = out.found.iter().map(|f| vec![mb(f.memory), f.pid.to_string(), word(&f.kind), truncate(&f.why, 90)]).collect();
            print!("{}", table_text(&["MEMORY", "PID", "KIND", "WHY"], rows));
        }
    }
    if !out.kept.is_empty() {
        println!("\nKept, each waiting on a check it scheduled itself:");
        let rows = out.kept.iter().map(|k| vec![k.pid.to_string(), crate::schedules::clock_text(k.wakes_at, &crate::schedules::rule::Local), truncate(&k.why, 90)]).collect();
        print!("{}", table_text(&["PID", "WAKES", "WHY"], rows));
    }
}

//! Removing a worktree with nothing in it lost: its menu's Clean up worktree, `codealong worktree
//! remove`, and Mark as done on a background job that holds the worktree its harness made for it
//! (actions.rs). Elsewhere Codealong only reads git; this changes a repository, and only where
//! nothing would be lost (`board::cleanup`), checked again here against git as it is now.
//!
//! The repo's own teardown goes first: a repo whose worktrees a hook makes may set up much more
//! than a folder for each (ports, routes, databases), which only its `WorktreeRemove` hook takes
//! down, so that runs as its harness would run it (`Adapter::teardown`). Without one, `git
//! worktree remove`, never forced, then its branch with `git branch -d`, which keeps a branch with
//! work merged nowhere. Either way it went when the folder is gone. Then what a background job
//! that held it keeps of it (`Adapter::forget_worktree`); the job's conversation stays.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::board::{claude_lock, cleanup, Leaving};
use crate::config::Config;
use crate::harness::{self, Ctx};
use crate::model::{Board, Session};
use crate::procs;
use crate::util::{file_name, tilde};

/// How long git gets to remove a worktree: as long as deleting its files takes, and a worktree
/// with its dependencies installed has many.
const GIT_WAIT: Duration = Duration::from_secs(300);

/// How long a harness's command gets (`claude rm <job>`).
const COMMAND_WAIT: Duration = Duration::from_secs(60);

/// How long the process of a session that's ending gets to end before its lock comes off.
const ENDING_WAIT: Duration = Duration::from_secs(15);

/// A command a repo declares to run as one of its worktrees goes (`Adapter::teardown`): run
/// through `/bin/sh -c` in `cwd`, with `env` added and `input` on stdin, for up to `timeout`.
#[derive(Debug, Clone, PartialEq)]
pub struct Hook {
    pub command: String,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub input: String,
    pub timeout: Duration,
}

/// What removing a worktree would do, as it stands now.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub path: PathBuf,
    /// Its name on the board.
    pub name: String,
    /// The repo's main checkout, where git and the hooks run.
    pub main: PathBuf,
    pub branch: Option<String>,
    /// Why it stays, when it can't go.
    pub kept: Option<String>,
    /// The lock a session left on it, which comes off first: one hung up on, or the one being
    /// ended, whose process (`ending`) is given a moment to go.
    pub unlock: bool,
    pub ending: Option<u32>,
    /// The repo's teardown; none removes it with git.
    pub hooks: Vec<Hook>,
    /// Why the repo's teardown won't run, when it declares one (an account that doesn't trust it).
    pub skipped: Option<String>,
    /// Its branch goes after it: no other worktree has it.
    pub branch_goes: bool,
    /// The harness's commands for background jobs that held it.
    pub after: Vec<String>,
}

impl Plan {
    /// What would run, in order, as commands to read.
    pub fn steps(&self) -> Vec<String> {
        let main = tilde(&self.main);
        let mut steps = Vec::new();
        if self.unlock {
            steps.push(format!("git -C {main} worktree unlock {}", tilde(&self.path)));
        }
        if self.hooks.is_empty() {
            steps.push(format!("git -C {main} worktree remove {}", tilde(&self.path)));
            if let (Some(branch), true) = (&self.branch, self.branch_goes) {
                steps.push(format!("git -C {main} branch -d {branch}"));
            }
        } else {
            steps.extend(self.hooks.iter().map(|h| format!("(in {main}) {}", h.command)));
        }
        steps.extend(self.after.iter().cloned());
        steps
    }
}

/// What removing the worktree `lane_id` would do, checked against git now. `ending` is a session
/// in it being marked done (its background job stopped just before), which doesn't hold it and
/// whose lock comes off. Err when it isn't a worktree on this device's board.
pub fn plan(board: &Board, cfg: &Config, lane_id: &str, ending: Option<&Session>) -> Result<Plan, String> {
    let lane = board.lanes.iter().find(|l| l.id == lane_id && l.device.is_none()).ok_or("That worktree isn't on the board here.")?;
    let path = PathBuf::from(&lane.path);
    if !path.exists() {
        return Err(format!("{} is gone already.", tilde(&path)));
    }
    let Some((root, main)) = crate::repos::locate(&path) else { return Err(format!("{} isn't in a git repository.", tilde(&path))) };
    let base = lane.git.as_ref().and_then(|g| g.base.clone());
    let git = crate::git::status(&path, base.as_deref());
    let listed = crate::repos::load_checkout(&main).map(|c| crate::repos::list_worktrees(&c)).unwrap_or_default();
    let holds = listed.iter().find(|w| w.path != path && w.path.starts_with(&path)).map(|w| file_name(&w.path));
    let branch = listed.iter().find(|w| w.path == path).and_then(|w| w.branch.clone()).or_else(|| lane.branch.clone());
    let branch_goes = branch.as_ref().is_some_and(|b| !listed.iter().any(|w| w.path != path && w.branch.as_ref() == Some(b)));
    let running = lane.sessions.iter().filter(|s| s.visitor.is_none() && s.live && ending.map_or(true, |e| e.key != s.key)).count();
    // A lock holds while the process that took it runs, the session being ended aside.
    let ending_pid = ending.and_then(|s| s.pid);
    let lock = git.locked.as_deref().map(|reason| (reason, claude_lock(reason)));
    let lock_holds = |pid: u32, start: Option<&str>| Some(pid) != ending_pid && runs(pid, start);
    let held = lock.filter(|(_, claude)| claude.map_or(true, |(pid, start)| lock_holds(pid, start))).map(|(reason, _)| reason);
    let marked = crate::reusable::load().ok().and_then(|s| s.get(&lane.id));
    let kept = cleanup(&Leaving {
        origin: if root == main { crate::model::Origin::Main } else { lane.origin },
        chats: lane.starts_chats,
        reusable: lane.reuse.map(|_| marked == Some(true)),
        git: Some(&git),
        running,
        holds: holds.as_deref(),
        lock: held,
    })
    .filter(|c| !c.ok)
    .map(|c| c.why);
    let unlock = lock.is_some() && held.is_none();
    let ends_first = lock.and_then(|(_, claude)| claude).map(|(pid, _)| pid).filter(|pid| Some(*pid) == ending_pid);
    let (hooks, skipped) = teardown(cfg, &main, &path);
    let mut after: Vec<String> = Vec::new();
    for account in &cfg.accounts {
        let Some(adapter) = harness::of(account) else { continue };
        for command in adapter.forget_worktree(&Ctx { account }, &path) {
            if !after.contains(&command) {
                after.push(command);
            }
        }
    }
    Ok(Plan { name: lane.callsign.clone(), path, main, branch, kept, unlock, ending: ends_first, hooks, skipped, branch_goes, after })
}

/// Whether the process `pid` runs, and is the one that started at `start` (`ps -o lstart` in
/// UTC, as Claude Code writes it in its lock) when that's given: a number goes to another process
/// once its own has ended.
fn runs(pid: u32, start: Option<&str>) -> bool {
    if !procs::pid_alive(pid) {
        return false;
    }
    let (Some(start), Some(at)) = (start.and_then(lstart_ms), procs::proc_start_ms(pid)) else { return true };
    (at - start).abs() < 2_000
}

/// `ps -o lstart`'s "Mon Oct 12 22:47:57 2026", read as UTC, in ms.
fn lstart_ms(text: &str) -> Option<i64> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    let [_, month, day, time, year] = parts.as_slice() else { return None };
    let month = MONTHS.iter().position(|m| m == month)? as i64 + 1;
    let hms: Vec<i64> = time.split(':').map(|n| n.parse().ok()).collect::<Option<_>>()?;
    let [h, m, s] = hms.as_slice() else { return None };
    let days = crate::util::days_from_civil(year.parse().ok()?, month, day.parse().ok()?);
    Some((((days * 24 + h) * 60 + m) * 60 + s) * 1000)
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// The repo's teardown, from the first account whose harness would run it, and why it won't run
/// when one is declared that none would.
fn teardown(cfg: &Config, main: &Path, path: &Path) -> (Vec<Hook>, Option<String>) {
    let mut skipped = None;
    for account in &cfg.accounts {
        let Some(adapter) = harness::of(account) else { continue };
        match adapter.teardown(&Ctx { account }, main, path) {
            Ok(hooks) if !hooks.is_empty() => return (hooks, None),
            Ok(_) => {}
            Err(why) => skipped = skipped.or(Some(why)),
        }
    }
    (Vec::new(), skipped)
}

/// Removes the worktree as `plan` says, or says why it stays. What was done, in a sentence.
pub fn remove(plan: &Plan) -> Result<String, String> {
    if let Some(why) = &plan.kept {
        return Err(format!("{} stays: {why}.", plan.name));
    }
    let main = plan.main.to_string_lossy().into_owned();
    let path = plan.path.to_string_lossy().into_owned();
    if let Some(pid) = plan.ending {
        let started = Instant::now();
        while procs::pid_alive(pid) && started.elapsed() < ENDING_WAIT {
            std::thread::sleep(Duration::from_millis(200));
        }
        if procs::pid_alive(pid) {
            return Err(format!("{} stays: its session's process hasn't ended.", plan.name));
        }
    }
    if plan.unlock {
        procs::run("git", &["-C", &main, "worktree", "unlock", &path], None, COMMAND_WAIT).map_err(|e| format!("Couldn't take the lock its session left off {}: {}", plan.name, last_line(&e.to_string())))?;
    }
    let mut said = if plan.hooks.is_empty() {
        procs::run("git", &["-C", &main, "worktree", "remove", &path], None, GIT_WAIT).map_err(|e| format!("Git didn't remove {}: {}", plan.name, last_line(&e.to_string())))?;
        let branch = plan.branch.as_deref().filter(|_| plan.branch_goes);
        // `-d` keeps a branch whose work is merged nowhere: then only the folder goes.
        match branch.map(|b| procs::run("git", &["-C", &main, "branch", "-d", b], None, COMMAND_WAIT)) {
            Some(Ok(_)) => format!("Removed the worktree {} and its branch, {}.", plan.name, branch.unwrap_or_default()),
            Some(Err(_)) => format!("Removed the worktree {}; its branch, {}, stays: its work isn't merged.", plan.name, branch.unwrap_or_default()),
            None => format!("Removed the worktree {}.", plan.name),
        }
    } else {
        let mut failed = None;
        for h in &plan.hooks {
            match procs::run_in("/bin/sh", &["-c", &h.command], &h.cwd, &h.env, &h.input, h.timeout) {
                Ok(o) if o.ok => {}
                Ok(o) => failed = failed.or(Some(last_line(&o.err).to_string())),
                Err(e) => failed = failed.or(Some(last_line(&e.to_string()).to_string())),
            }
        }
        if plan.path.exists() {
            let why = failed.map(|w| format!(": {w}")).unwrap_or_default();
            return Err(format!("The repo's WorktreeRemove hook ran, but {} is still there{why}", plan.name));
        }
        format!("Removed the worktree {} through the repo's WorktreeRemove hook.", plan.name)
    };
    if plan.path.exists() {
        return Err(format!("{} is still there.", plan.name));
    }
    let mut kept_jobs = Vec::new();
    for command in &plan.after {
        if let Err(e) = login_shell(command) {
            kept_jobs.push(format!("`{command}` failed: {}", last_line(&e)));
        }
    }
    match (plan.after.len(), kept_jobs.is_empty()) {
        (0, _) => {}
        (1, true) => said.push_str(" Its background job's record went with it; its conversation stays."),
        (_, true) => said.push_str(" The records of the background jobs that held it went with it; their conversations stay."),
        (_, false) => said.push_str(&format!(" {}", kept_jobs.join("; "))),
    }
    Ok(said)
}

/// The worktree its harness made for `s` (`Session::own_worktree`), once Mark as done has stopped
/// it as a background job: removed when nothing in it would be lost, the job's record with it
/// (`claude rm`), else kept. None when it holds none on the board; Ok with what was done, or Err
/// with why it stays.
pub fn after_job(board: &Board, cfg: &Config, s: &Session) -> Option<Result<String, String>> {
    let own = Path::new(s.own_worktree.as_deref()?);
    let real = std::fs::canonicalize(own).ok();
    let lane = board.lanes.iter().find(|l| l.device.is_none() && (Path::new(&l.path) == own || real.as_deref().is_some_and(|r| std::fs::canonicalize(&l.path).is_ok_and(|p| p == r))))?;
    let name = lane.callsign.clone();
    Some(match plan(board, cfg, &lane.id, Some(s)) {
        Ok(p) if p.kept.is_none() => remove(&p),
        Ok(p) => Err(format!("Its worktree, {name}, stays: {}.", p.kept.unwrap_or_default())),
        Err(e) => Err(e),
    })
}

/// Runs a harness's command through a login shell, as a start does: the account's command can be
/// an alias. Its own reason when it fails, after whatever the shell's startup printed.
fn login_shell(command: &str) -> Result<String, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    procs::run(&shell, &["-ilc", command], None, COMMAND_WAIT).map_err(|e| e.to_string())
}

/// What a command said last, which is why it failed.
fn last_line(text: &str) -> &str {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("it failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_with(hooks: Vec<Hook>) -> Plan {
        Plan {
            path: PathBuf::from("/r/app/.claude/worktrees/brave-otter"),
            name: "brave-otter".into(),
            main: PathBuf::from("/r/app"),
            branch: Some("worktree-brave-otter".into()),
            kept: None,
            unlock: false,
            ending: None,
            hooks,
            skipped: None,
            branch_goes: true,
            after: vec!["claude rm 1df9ebbd".into()],
        }
    }

    #[test]
    fn says_what_would_run() {
        let plan = plan_with(vec![]);
        assert_eq!(
            plan.steps(),
            [
                "git -C /r/app worktree remove /r/app/.claude/worktrees/brave-otter",
                "git -C /r/app branch -d worktree-brave-otter",
                "claude rm 1df9ebbd",
            ]
        );
        // A branch another worktree has stays; a lock left behind comes off first.
        let shared = Plan { branch_goes: false, unlock: true, after: vec![], ..plan.clone() };
        assert_eq!(shared.steps(), ["git -C /r/app worktree unlock /r/app/.claude/worktrees/brave-otter", "git -C /r/app worktree remove /r/app/.claude/worktrees/brave-otter"]);
        // The repo's teardown in git's place, the branch its own business.
        let hook = Hook { command: "\"$CLAUDE_PROJECT_DIR\"/scripts/remove.sh".into(), cwd: "/r/app".into(), env: vec![], input: "{}".into(), timeout: Duration::from_secs(60) };
        assert_eq!(plan_with(vec![hook]).steps(), ["(in /r/app) \"$CLAUDE_PROJECT_DIR\"/scripts/remove.sh", "claude rm 1df9ebbd"]);
    }

    #[test]
    fn nothing_runs_for_one_that_stays() {
        let kept = Plan { kept: Some("3 uncommitted files".into()), ..plan_with(vec![]) };
        procs::stand_in::set(Some(Box::new(|program, args, _, _| panic!("ran {program} {args:?}"))));
        assert_eq!(remove(&kept), Err("brave-otter stays: 3 uncommitted files.".into()));
        procs::stand_in::set(None);
    }

    #[test]
    fn reads_the_start_claude_code_writes_in_its_lock() {
        assert_eq!(lstart_ms("Mon Oct 12 22:47:57 2026"), Some(1_791_845_277_000));
        assert_eq!(lstart_ms("Tue Oct  6 01:02:03 2026"), Some(1_791_248_523_000));
        assert_eq!(lstart_ms("soon"), None);
        assert_eq!(lstart_ms("Mon Okt 12 22:47:57 2026"), None);
        // This process, as Claude Code would write it: alive and itself.
        let me = std::process::id();
        let at = procs::proc_start_ms(me).unwrap() / 1000;
        let (y, mo, d) = crate::util::civil_from_days(at.div_euclid(86_400));
        let secs = at.rem_euclid(86_400);
        let written = format!("Mon {} {d:>2} {:02}:{:02}:{:02} {y}", MONTHS[mo as usize - 1], secs / 3600, secs / 60 % 60, secs % 60);
        assert!(runs(me, Some(&written)), "{written}");
        assert!(!runs(me, Some("Mon Oct 12 22:47:57 2001")));
        assert!(runs(me, None));
    }

    /// A real repo in a scratch folder: a worktree with nothing in it goes with its branch, one
    /// with work stays, and a repo's hook runs in git's place. `cargo test cleanup::tests::removes`.
    #[test]
    fn removes_a_real_worktree_only_when_nothing_would_be_lost() {
        let dir = std::env::temp_dir().join(format!("codealong-cleanup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::fs::canonicalize(&dir).unwrap();
        let main = dir.join("app");
        let git = |args: &[&str]| procs::run("git", &[&["-C", main.to_str().unwrap()], args].concat(), None, Duration::from_secs(20)).unwrap();
        std::fs::create_dir_all(&main).unwrap();
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(main.join(".gitignore"), ".claude/worktrees/\nnode_modules/\n").unwrap();
        git(&["add", ".gitignore"]);
        git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "init"]);
        // Its remote has everything main has.
        let origin = dir.join("origin.git");
        procs::run("git", &["init", "-q", "--bare", origin.to_str().unwrap()], None, Duration::from_secs(20)).unwrap();
        git(&["remote", "add", "origin", origin.to_str().unwrap()]);
        git(&["push", "-q", "origin", "main"]);
        let wt = main.join(".claude/worktrees/brave-otter");
        git(&["worktree", "add", "-q", "-b", "worktree-brave-otter", wt.to_str().unwrap(), "main"]);
        std::fs::create_dir_all(wt.join("node_modules")).unwrap();
        std::fs::write(wt.join("node_modules/x.js"), "").unwrap();
        std::fs::write(wt.join("notes.txt"), "").unwrap();
        let lane = |git: crate::model::GitInfo| crate::model::Lane {
            callsign: "brave-otter".into(),
            origin: crate::model::Origin::ClaudeApp,
            branch: Some("worktree-brave-otter".into()),
            git: Some(git),
            ..crate::fixtures::lane(wt.to_str().unwrap(), vec![])
        };
        let mut board = Board::default();
        board.lanes = vec![lane(crate::git::status(&wt, Some("origin/main")))];
        let cfg = Config::default();
        // An untracked file holds it; ignored ones don't.
        let p = plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap();
        assert_eq!(p.kept.as_deref(), Some("1 uncommitted file"), "notes.txt");
        std::fs::remove_file(wt.join("notes.txt")).unwrap();
        let p = plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap();
        assert_eq!(p.kept, None, "{p:?}");
        assert!(p.hooks.is_empty() && p.branch_goes);
        assert_eq!(remove(&p), Ok("Removed the worktree brave-otter and its branch, worktree-brave-otter.".into()));
        assert!(!wt.exists());
        assert!(git(&["branch", "--list", "worktree-brave-otter"]).trim().is_empty());
        // A commit on no remote holds the next one.
        git(&["worktree", "add", "-q", "-b", "worktree-kept", wt.to_str().unwrap(), "main"]);
        procs::run("git", &["-C", wt.to_str().unwrap(), "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "work"], None, Duration::from_secs(20)).unwrap();
        let p = plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap();
        assert_eq!(p.kept.as_deref(), Some("1 commit on no remote"));
        assert!(remove(&p).is_err());
        assert!(wt.exists());
        // Pushed, it goes; its branch stays where its work is merged nowhere but the remote's.
        procs::run("git", &["-C", wt.to_str().unwrap(), "push", "-q", "origin", "worktree-kept"], None, Duration::from_secs(20)).unwrap();
        let p = plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap();
        assert_eq!(p.kept, None);
        assert_eq!(remove(&p), Ok("Removed the worktree brave-otter; its branch, worktree-kept, stays: its work isn't merged.".into()));
        // A hook-made one goes through the repo's hook, which is handed the event on stdin.
        git(&["worktree", "add", "-q", "-b", "worktree-hooked", wt.to_str().unwrap(), "main"]);
        let log = dir.join("hook.log");
        let hook = Hook {
            command: format!("cat > {log}; git -C \"$CLAUDE_PROJECT_DIR\" worktree remove --force {wt}", log = log.display(), wt = wt.display()),
            cwd: main.clone(),
            env: vec![("CLAUDE_PROJECT_DIR".into(), main.to_string_lossy().into_owned())],
            input: serde_json::json!({ "hook_event_name": "WorktreeRemove", "worktree_path": wt }).to_string(),
            timeout: Duration::from_secs(30),
        };
        let p = Plan { hooks: vec![hook], after: vec![], ..plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap() };
        assert_eq!(remove(&p), Ok("Removed the worktree brave-otter through the repo's WorktreeRemove hook.".into()));
        assert!(!wt.exists());
        assert!(std::fs::read_to_string(&log).unwrap().contains("\"worktree_path\""));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

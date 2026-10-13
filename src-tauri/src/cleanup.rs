//! Removing a worktree with nothing in it lost: its menu's Clean up worktree, `codealong worktree
//! remove`, and Mark as done on a background job that holds the worktree its harness made for it
//! (actions.rs). Elsewhere Codealong only reads git; this changes a repository, and only where
//! nothing would be lost (`board::cleanup`), checked again here against git as it is now.
//!
//! The check reads git itself rather than taking the board's word (`facts`): untracked files are
//! counted whatever `status.showUntrackedFiles` says, a git that fails or times out keeps the
//! worktree, and its folders are looked through for another repository inside them, which its
//! own git can't see (`repo_inside`). Just before it goes it's read again, one removal at a time,
//! and anything changed since keeps it (`verify`).
//!
//! The repo's own teardown goes first: a repo whose worktrees a hook makes may set up much more
//! than a folder for each (ports, routes, databases), which only its `WorktreeRemove` hook takes
//! down, so that runs as its harness would run it (`Adapter::teardown`). Without one, `git
//! worktree remove`, never forced, then its branch with `git branch -d`, which keeps a branch with
//! work merged nowhere. Either way it went when the folder is gone. Then what a background job
//! that held it keeps of it (`Adapter::forget_worktree`); the job's conversation stays.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::board::{claude_lock, cleanup, Leaving};
use crate::config::Config;
use crate::harness::{self, Ctx};
use crate::model::{Board, Origin, Session};
use crate::procs;
use crate::util::tilde;

/// How long git gets to remove a worktree: as long as deleting its files takes, and a worktree
/// with its dependencies installed has many.
const GIT_WAIT: Duration = Duration::from_secs(300);

/// How long a harness's command gets (`claude rm <job>`).
const COMMAND_WAIT: Duration = Duration::from_secs(60);

/// How long the process of a session that's ending gets to end before its lock comes off.
const ENDING_WAIT: Duration = Duration::from_secs(15);

/// How long each of the check's git commands gets: one that runs out keeps the worktree.
const CHECK_WAIT: Duration = Duration::from_secs(60);

/// Folders looked into only a little way for another repository (`SHALLOW`): dependencies and
/// builds, which hold many files and, below their first levels, no work of yours.
const SHALLOW_IN: [&str; 9] = ["node_modules", "target", ".next", ".nx", ".turbo", ".venv", "venv", "__pycache__", ".gradle"];

/// How many levels of folders below one of `SHALLOW_IN` are listed; the level under the last is
/// only looked at for a `.git` of its own. With one, a worktree put in `target/feature` and a
/// package's own repository in `node_modules/@scope/pkg` are found.
const SHALLOW: usize = 1;

/// The most files and folders looked at for another repository inside a worktree: one with more
/// stays, as it couldn't be looked through.
const MOST_LOOKED_AT: usize = 200_000;

/// One removal at a time: two of a repo's teardowns at once could trip over each other, and
/// each is checked again just before it goes.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

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
    /// What was checked, which must still hold just before it goes (`verify`): its HEAD, its
    /// lock's reason, and the branch its new work is counted from.
    pub head: String,
    pub lock: Option<String>,
    pub base: Option<String>,
    /// Whether it could be reusable (in one of your projects, a worktree of a repo), and how you
    /// marked it, for checking again that it isn't (`verify`).
    pub can_reuse: bool,
    pub marked: Option<bool>,
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
    let origin = if root == main { Origin::Main } else { lane.origin };
    let checkout = crate::repos::load_checkout(&main);
    // The default branch, which resets onto count as new work: the board's, else the repo's own
    // (the command's board, with the app closed, read no git).
    let base = lane.git.as_ref().and_then(|g| g.base.clone()).or_else(|| checkout.as_ref().and_then(crate::repos::default_branch));
    let checked = facts(&path, base.as_deref());
    let listed = checkout.as_ref().map(crate::repos::list_worktrees).unwrap_or_default();
    // Another worktree inside it, of this repo or any the board has, or a repository nothing
    // lists: its files would go too.
    let listed_inside = listed
        .iter()
        .map(|w| w.path.clone())
        .chain(board.lanes.iter().filter(|l| l.device.is_none()).map(|l| PathBuf::from(&l.path)))
        .find(|p| *p != path && p.starts_with(&path));
    let inside = match listed_inside {
        Some(p) => Ok(Some(p)),
        None if origin == Origin::Main => Ok(None),
        None => repo_inside(&path),
    };
    let holds = inside.as_ref().ok().cloned().flatten().map(|p| p.strip_prefix(&path).map(Path::to_path_buf).unwrap_or(p).display().to_string());
    let branch = listed.iter().find(|w| w.path == path).and_then(|w| w.branch.clone()).or_else(|| lane.branch.clone());
    let branch_goes = branch.as_ref().is_some_and(|b| !listed.iter().any(|w| w.path != path && w.branch.as_ref() == Some(b)));
    // Every running session is on its lane's list (`board::compose` never cuts one).
    let running = lane.sessions.iter().filter(|s| s.visitor.is_none() && s.live && ending.map_or(true, |e| e.key != s.key)).count();
    let git = checked.as_ref().ok().map(Facts::git);
    // A lock holds while the process that took it runs, the session being ended aside.
    let ending_pid = ending.and_then(|s| s.pid);
    let lock = git.as_ref().and_then(|g| g.locked.clone());
    let claude = lock.as_deref().and_then(claude_lock);
    let held = match (&lock, claude) {
        (Some(_), Some((pid, start))) => Some(pid) != ending_pid && runs(pid, start),
        (Some(_), None) => true,
        (None, _) => false,
    };
    // Reusable as the board would have it, from git as it is: the command, with the app closed,
    // has a board that read no git.
    let marked = crate::reusable::load().ok().and_then(|s| s.get(&lane.id));
    let can_reuse = lane.project != crate::board::ELSEWHERE && !lane.starts_chats && origin != Origin::Folder;
    let reusable = can_reuse && marked.unwrap_or(origin == Origin::Main || crate::board::reused(git.as_ref()));
    let kept = match (&checked, &inside) {
        (Err(e), _) => Some(format!("git couldn't check it ({e})")),
        (_, Err(e)) => Some(e.clone()),
        _ => cleanup(&Leaving {
            origin,
            chats: lane.starts_chats,
            reusable: reusable.then_some(marked == Some(true)),
            git: git.as_ref(),
            running,
            holds: holds.as_deref(),
            lock: lock.as_deref().filter(|_| held),
        })
        .filter(|c| !c.ok)
        .map(|c| c.why),
    };
    let unlock = lock.is_some() && !held;
    let ends_first = claude.map(|(pid, _)| pid).filter(|pid| Some(*pid) == ending_pid);
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
    let head = checked.map(|f| f.head).unwrap_or_default();
    Ok(Plan { name: lane.callsign.clone(), path, main, branch, head, lock, base, can_reuse, marked, kept, unlock, ending: ends_first, hooks, skipped, branch_goes, after })
}

/// What git says of a worktree, read for removing it. A command that fails or runs out is an
/// error, never a clean worktree; untracked files count whatever `status.showUntrackedFiles`
/// says, and so do submodules' changes and tracked files git is told not to look at
/// (`hidden_changes`).
#[derive(Debug, Clone, PartialEq)]
struct Facts {
    dirty: u32,
    unpushed: u32,
    head: String,
    locked: Option<String>,
    /// How many times it took new work (`git::new_work`), for whether it's reusable.
    new_work: u32,
}

impl Facts {
    /// As the board's rule reads them.
    fn git(&self) -> crate::model::GitInfo {
        crate::model::GitInfo { dirty: self.dirty, unpushed: Some(self.unpushed), locked: self.locked.clone(), new_work_count: Some(self.new_work), ..Default::default() }
    }
}

fn facts(path: &Path, base: Option<&str>) -> Result<Facts, String> {
    let dir = path.to_string_lossy();
    let run = |args: &[&str]| procs::run("git", &[&["-C", dir.as_ref()], args].concat(), None, CHECK_WAIT).map_err(|e| last_line(&e.to_string()).to_string());
    let status = run(&["status", "--porcelain=v2", "--untracked-files=normal", "--ignore-submodules=none"])?;
    let head = run(&["rev-parse", "--verify", "HEAD"])?.trim().to_string();
    let unpushed = run(&["rev-list", "--count", "HEAD", "--not", "--remotes"])?.trim().parse().map_err(|_| "its count of commits didn't read".to_string())?;
    let dirty = crate::git::parse_status(&status).dirty + hidden_changes(path)?;
    Ok(Facts { dirty, unpushed, head, locked: crate::git::locked(path), new_work: crate::git::new_work(path, base).len() as u32 })
}

/// Tracked files git is told not to look at (`update-index --assume-unchanged`, and
/// `--skip-worktree` ones that are there anyway) whose contents aren't what the index has: status
/// doesn't count them, and git's own removal wouldn't see them either. Read from `ls-files -v`,
/// whose tag is a lower-case letter for the first and `S` for the second, and compared by hash.
fn hidden_changes(path: &Path) -> Result<u32, String> {
    let dir = path.to_string_lossy();
    let git = |args: &[&str]| procs::run("git", &[&["-C", dir.as_ref()], args].concat(), None, CHECK_WAIT).map_err(|e| last_line(&e.to_string()).to_string());
    let listed = git(&["ls-files", "-v", "-z"])?;
    let flagged: Vec<&str> = listed
        .split('\0')
        .filter_map(|entry| entry.split_once(' '))
        .filter(|(tag, _)| tag.chars().next().is_some_and(|t| t.is_ascii_lowercase() || t == 'S'))
        .map(|(_, file)| file)
        .filter(|file| std::fs::symlink_metadata(path.join(file)).is_ok_and(|m| m.is_file()))
        .collect();
    if flagged.is_empty() {
        return Ok(0);
    }
    let indexed = git(&[&["ls-files", "-s", "-z", "--"][..], &flagged].concat())?;
    // One process hashes them all, a line each in order, through the repo's filters as `add` would.
    let names: String = flagged.iter().map(|f| format!("{f}\n")).collect();
    let hashed = procs::run_stdin("git", &["-C", dir.as_ref(), "hash-object", "--stdin-paths"], &names, CHECK_WAIT).map_err(|e| last_line(&e.to_string()).to_string())?;
    let mut now = hashed.lines();
    let mut changed = 0;
    for file in flagged {
        let staged = indexed.split('\0').find_map(|entry| entry.split_once('\t').filter(|(_, f)| *f == file).and_then(|(meta, _)| meta.split_whitespace().nth(1)));
        let hash = now.next().map(str::trim);
        if staged.is_none() || hash != staged {
            changed += 1;
        }
    }
    Ok(changed)
}

/// Another repository or worktree inside `path`, found by its `.git`, which the folder's own git
/// doesn't see into: one in an ignored folder, say. Links aren't followed, and folders of
/// dependencies and builds are looked into only `SHALLOW` levels deep (`SHALLOW_IN`). Err for a
/// folder with too many files to look through.
fn repo_inside(path: &Path) -> Result<Option<PathBuf>, String> {
    // Each folder to list, with how many levels more below it are listed: None for all of them.
    let mut folders: Vec<(PathBuf, Option<usize>)> = vec![(path.to_path_buf(), None)];
    let mut looked = 0;
    while let Some((dir, left)) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            looked += 1;
            if looked > MOST_LOOKED_AT {
                return Err("too many files in it to look through for another repository".into());
            }
            let name = e.file_name();
            if name == ".git" {
                if dir != path {
                    return Ok(Some(dir));
                }
                continue;
            }
            let Ok(kind) = e.file_type() else { continue };
            if !kind.is_dir() {
                continue;
            }
            let below = match left {
                Some(n) => n.checked_sub(1),
                None if SHALLOW_IN.iter().any(|n| name == *n) => Some(SHALLOW),
                None => None,
            };
            match (left, below) {
                // The last level is only looked at for a `.git` of its own, not listed.
                (Some(_), None) => {
                    if std::fs::symlink_metadata(e.path().join(".git")).is_ok() {
                        return Ok(Some(e.path()));
                    }
                }
                _ => folders.push((e.path(), below)),
            }
        }
    }
    Ok(None)
}

/// Reads the worktree again just before it goes: what the plan saw must still hold, the same
/// HEAD and lock, nothing uncommitted or unpushed, no repository inside it and not reusable, or it
/// stays. Waiting for a session's process to end gives time for work to come in. A lock the plan
/// was to take off may have gone by itself (a session's own, as its process ends): whether it's
/// still there to take off.
fn verify(plan: &Plan) -> Result<bool, String> {
    let now = facts(&plan.path, plan.base.as_deref()).map_err(|e| format!("{} stays: git couldn't check it again ({e}).", plan.name))?;
    let stays = |why: &str| Err(format!("{} stays: {why}.", plan.name));
    if now.head != plan.head {
        return stays("its HEAD moved since it was checked");
    }
    let went = plan.unlock && now.locked.is_none();
    if now.locked != plan.lock && !went {
        return stays("its lock changed since it was checked");
    }
    if now.dirty > 0 {
        return stays("it has uncommitted files now");
    }
    if now.unpushed > 0 {
        return stays("it has commits on no remote now");
    }
    match repo_inside(&plan.path) {
        Ok(None) => {}
        Ok(Some(inner)) => return stays(&format!("another repository is inside it now ({})", inner.strip_prefix(&plan.path).unwrap_or(&inner).display())),
        Err(why) => return stays(&why),
    }
    if plan.can_reuse && plan.marked.unwrap_or_else(|| crate::board::reused(Some(&now.git()))) {
        return stays("it's reusable now");
    }
    Ok(plan.unlock && !went)
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

/// A worktree that went.
#[derive(Debug, Clone, PartialEq)]
pub struct Gone {
    /// What was done, in a sentence or two.
    pub said: String,
    /// Something didn't go as asked though the folder went: the rest of the repo's teardown, a
    /// job's record. `said` says what.
    pub partly: bool,
}

/// What a removal came to: the plan it went by, and the worktree gone, or why it stays or what
/// failed.
pub struct Removed {
    pub plan: Plan,
    pub result: Result<Gone, String>,
}

/// Removes the worktree `lane_id` when nothing in it would be lost, one removal at a time: it's
/// planned once the removals ahead of it are done, and checked again just before it goes (`plan`,
/// `carry_out`). `ending` is as `plan` has it. Err when it isn't a worktree here.
pub fn remove(board: &Board, cfg: &Config, lane_id: &str, ending: Option<&Session>) -> Result<Removed, String> {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let plan = plan(board, cfg, lane_id, ending)?;
    let result = carry_out(&plan);
    Ok(Removed { plan, result })
}

/// Removes the worktree as `plan` says, or says why it stays.
fn carry_out(plan: &Plan) -> Result<Gone, String> {
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
    if verify(plan)? {
        procs::run("git", &["-C", &main, "worktree", "unlock", &path], None, COMMAND_WAIT).map_err(|e| format!("Couldn't take the lock its session left off {}: {}", plan.name, last_line(&e.to_string())))?;
    }
    let mut partly = false;
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
        // The folder can go and the rest of the teardown fail (a port, a route, a database).
        partly = failed.is_some();
        match failed {
            Some(why) => format!("Removed the worktree {} through the repo's WorktreeRemove hook, which failed, though: {why}.", plan.name),
            None => format!("Removed the worktree {} through the repo's WorktreeRemove hook.", plan.name),
        }
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
        (_, false) => {
            partly = true;
            said.push_str(&format!(" {}", kept_jobs.join("; ")));
        }
    }
    Ok(Gone { said, partly })
}

/// The worktree its harness made for `s` (`Session::own_worktree`), once Mark as done has stopped
/// it as a background job: removed when nothing in it would be lost, the job's record with it
/// (`claude rm`), else kept. None when it holds none on the board; Ok when it went, or Err with why
/// it stays.
pub fn after_job(board: &Board, cfg: &Config, s: &Session) -> Option<Result<Gone, String>> {
    let own = Path::new(s.own_worktree.as_deref()?);
    let real = std::fs::canonicalize(own).ok();
    let lane = board.lanes.iter().find(|l| l.device.is_none() && (Path::new(&l.path) == own || real.as_deref().is_some_and(|r| std::fs::canonicalize(&l.path).is_ok_and(|p| p == r))))?;
    let name = lane.callsign.clone();
    Some(match remove(board, cfg, &lane.id, Some(s)) {
        Ok(Removed { plan: Plan { kept: Some(why), .. }, .. }) => Err(format!("Its worktree, {name}, stays: {why}.")),
        Ok(Removed { result, .. }) => result,
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
            head: "0ef4ed2".into(),
            lock: None,
            base: Some("origin/main".into()),
            can_reuse: true,
            marked: None,
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
        assert_eq!(carry_out(&kept), Err("brave-otter stays: 3 uncommitted files.".into()));
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
        assert_eq!(carry_out(&p), Ok(Gone { said: "Removed the worktree brave-otter and its branch, worktree-brave-otter.".into(), partly: false }));
        assert!(!wt.exists());
        assert!(git(&["branch", "--list", "worktree-brave-otter"]).trim().is_empty());
        // A commit on no remote holds the next one.
        git(&["worktree", "add", "-q", "-b", "worktree-kept", wt.to_str().unwrap(), "main"]);
        procs::run("git", &["-C", wt.to_str().unwrap(), "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "work"], None, Duration::from_secs(20)).unwrap();
        let p = plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap();
        assert_eq!(p.kept.as_deref(), Some("1 commit on no remote"));
        assert!(carry_out(&p).is_err());
        assert!(wt.exists());
        // Pushed, it goes; its branch stays where its work is merged nowhere but the remote's.
        procs::run("git", &["-C", wt.to_str().unwrap(), "push", "-q", "origin", "worktree-kept"], None, Duration::from_secs(20)).unwrap();
        let p = plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap();
        assert_eq!(p.kept, None);
        assert_eq!(carry_out(&p), Ok(Gone { said: "Removed the worktree brave-otter; its branch, worktree-kept, stays: its work isn't merged.".into(), partly: false }));
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
        assert_eq!(carry_out(&p), Ok(Gone { said: "Removed the worktree brave-otter through the repo's WorktreeRemove hook.".into(), partly: false }));
        assert!(!wt.exists());
        assert!(std::fs::read_to_string(&log).unwrap().contains("\"worktree_path\""));
        // A hook that takes the folder and then fails says so.
        git(&["worktree", "add", "-q", "-b", "worktree-half", wt.to_str().unwrap(), "main"]);
        let half = Hook { command: format!("git -C \"$CLAUDE_PROJECT_DIR\" worktree remove --force {}; echo 'port 3000 still taken' >&2; exit 1", wt.display()), ..p.hooks[0].clone() };
        let p = Plan { hooks: vec![half], after: vec![], ..plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap() };
        assert_eq!(carry_out(&p), Ok(Gone { said: "Removed the worktree brave-otter through the repo's WorktreeRemove hook, which failed, though: port 3000 still taken.".into(), partly: true }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What the board can't see keeps a worktree: untracked files a setting hides, another
    /// repository in an ignored folder, reuse its reflog shows, and work that came in after the
    /// check. `cargo test cleanup::tests::keeps`.
    #[test]
    fn keeps_what_the_board_cant_see_and_what_changed_after_the_check() {
        let dir = std::env::temp_dir().join(format!("codealong-cleanup-keeps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::fs::canonicalize(&dir).unwrap();
        let main = dir.join("app");
        let at = |p: &Path, args: &[&str]| procs::run("git", &[&["-C", p.to_str().unwrap(), "-c", "user.name=t", "-c", "user.email=t@t"], args].concat(), None, Duration::from_secs(20)).unwrap();
        std::fs::create_dir_all(&main).unwrap();
        at(&main, &["init", "-q", "-b", "main"]);
        std::fs::write(main.join(".gitignore"), "vendor/\ntarget/\nnode_modules/\n").unwrap();
        std::fs::write(main.join("local.conf"), "port = 3000\n").unwrap();
        at(&main, &["add", ".gitignore", "local.conf"]);
        at(&main, &["commit", "-q", "-m", "init"]);
        let origin = dir.join("origin.git");
        procs::run("git", &["init", "-q", "--bare", origin.to_str().unwrap()], None, Duration::from_secs(20)).unwrap();
        at(&main, &["remote", "add", "origin", origin.to_str().unwrap()]);
        at(&main, &["push", "-q", "origin", "main"]);
        let wt = dir.join("app-wt");
        at(&main, &["worktree", "add", "-q", "-b", "wt", wt.to_str().unwrap(), "main"]);
        let mut board = Board::default();
        board.lanes = vec![crate::model::Lane { callsign: "wt".into(), origin: Origin::Script, ..crate::fixtures::lane(wt.to_str().unwrap(), vec![]) }];
        let cfg = Config::default();
        let kept = |board: &Board| plan(board, &cfg, wt.to_str().unwrap(), None).unwrap().kept;
        assert_eq!(kept(&board), None);
        // A file git status would hide by the repo's setting.
        at(&main, &["config", "status.showUntrackedFiles", "no"]);
        std::fs::write(wt.join("notes.txt"), "").unwrap();
        assert_eq!(kept(&board).as_deref(), Some("1 uncommitted file"));
        std::fs::remove_file(wt.join("notes.txt")).unwrap();
        // A tracked file changed under a flag that tells git not to look at it.
        at(&wt, &["update-index", "--assume-unchanged", "local.conf"]);
        std::fs::write(wt.join("local.conf"), "port = 4000\n").unwrap();
        assert_eq!(kept(&board).as_deref(), Some("1 uncommitted file"));
        std::fs::write(wt.join("local.conf"), "port = 3000\n").unwrap();
        assert_eq!(kept(&board), None, "the same contents as the index");
        at(&wt, &["update-index", "--no-assume-unchanged", "local.conf"]);
        // Another repository in an ignored folder, which nothing lists, and one a little way into
        // a folder of builds or dependencies.
        std::fs::create_dir_all(wt.join("vendor/lib/.git")).unwrap();
        assert_eq!(kept(&board).as_deref(), Some("another worktree is inside it (vendor/lib)"));
        std::fs::remove_dir_all(wt.join("vendor")).unwrap();
        std::fs::create_dir_all(wt.join("target/feature")).unwrap();
        std::fs::write(wt.join("target/feature/.git"), "gitdir: /elsewhere\n").unwrap();
        assert_eq!(kept(&board).as_deref(), Some("another worktree is inside it (target/feature)"));
        std::fs::remove_dir_all(wt.join("target")).unwrap();
        std::fs::create_dir_all(wt.join("node_modules/@scope/pkg/.git")).unwrap();
        assert_eq!(kept(&board).as_deref(), Some("another worktree is inside it (node_modules/@scope/pkg)"));
        std::fs::remove_dir_all(wt.join("node_modules")).unwrap();
        // Work that comes in between the check and the removal keeps it.
        let p = plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap();
        assert_eq!(p.kept, None);
        at(&wt, &["commit", "-q", "--allow-empty", "-m", "late"]);
        assert_eq!(carry_out(&p), Err("wt stays: its HEAD moved since it was checked.".into()));
        at(&wt, &["reset", "-q", "--hard", "origin/main"]);
        let p = plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap();
        std::fs::write(wt.join("late.txt"), "").unwrap();
        assert_eq!(carry_out(&p), Err("wt stays: it has uncommitted files now.".into()));
        std::fs::remove_file(wt.join("late.txt")).unwrap();
        let p = plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap();
        std::fs::create_dir_all(wt.join("vendor/late/.git")).unwrap();
        assert_eq!(carry_out(&p), Err("wt stays: another repository is inside it now (vendor/late).".into()));
        std::fs::remove_dir_all(wt.join("vendor")).unwrap();
        // One that has taken new work three times is reusable, though the board read no git: the
        // reset above, and two more onto the default branch, as a reused worktree is put back.
        for _ in 0..2 {
            at(&wt, &["reset", "-q", "--hard", "origin/main"]);
        }
        assert_eq!(kept(&board).as_deref(), Some("reusable: it has taken several pieces of work"));
        assert!(wt.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

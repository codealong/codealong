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

/// The most files and folders looked at for another repository in a worktree's ignored folders:
/// one with more stays, as it couldn't be looked through. A worktree of atlas's with its
/// dependencies installed has some 420,000 in all, about 220,000 of them in `node_modules`
/// (2026-10-12), which took 20 s to list on a busy Mac.
const MOST_LOOKED_AT: usize = 2_000_000;

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
    // Another worktree inside it, of this repo or any the board has: its files would go too.
    let inner = |p: PathBuf| p.strip_prefix(&path).map(Path::to_path_buf).unwrap_or(p).display().to_string();
    let listed_inside = listed
        .iter()
        .map(|w| w.path.clone())
        .chain(board.lanes.iter().filter(|l| l.device.is_none()).map(|l| PathBuf::from(&l.path)))
        .find(|p| *p != path && p.starts_with(&path))
        .map(inner);
    let branch = listed.iter().find(|w| w.path == path).and_then(|w| w.branch.clone()).or_else(|| lane.branch.clone());
    let branch_goes = branch.as_ref().is_some_and(|b| !listed.iter().any(|w| w.path != path && w.branch.as_ref() == Some(b)));
    // Every running session is on its lane's list (`board::compose` never cuts one).
    let running = running_in(board, lane_id, ending.map(|s| s.key.as_str()));
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
    let rule = |holds: Option<&str>| {
        let leaving = Leaving { origin, chats: lane.starts_chats, reusable: reusable.then_some(marked == Some(true)), git: git.as_ref(), running, holds, lock: lock.as_deref().filter(|_| held) };
        cleanup(&leaving).filter(|c| !c.ok).map(|c| c.why)
    };
    // A repository nothing lists is looked for last, as it can take a while (`repo_inside`): only
    // in a worktree that would otherwise go.
    let kept = match &checked {
        Err(e) => Some(format!("git couldn't check it ({e})")),
        Ok(_) => rule(listed_inside.as_deref()).or_else(|| match repo_inside(&path) {
            Ok(None) => None,
            Ok(Some(p)) => rule(Some(&inner(p))),
            Err(why) => Some(why),
        }),
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
/// whose tag is a lower-case letter for the first and `S` for the second, and compared by hash: a
/// file through the repo's filters as `add` would, and its mode (executable or not, a file where
/// the index has a link), a link by where it points. One that's neither, or can't be read, counts
/// as changed; one that isn't there (a sparse checkout's) has nothing to lose.
fn hidden_changes(path: &Path) -> Result<u32, String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = path.to_string_lossy();
    let fail = |e: anyhow::Error| last_line(&e.to_string()).to_string();
    let git = |args: &[&str]| procs::run("git", &[&["-C", dir.as_ref()], args].concat(), None, CHECK_WAIT).map_err(fail);
    let listed = git(&["ls-files", "-v", "-z"])?;
    let flagged: Vec<&str> = listed
        .split('\0')
        .filter_map(|entry| entry.split_once(' '))
        .filter(|(tag, _)| tag.chars().next().is_some_and(|t| t.is_ascii_lowercase() || t == 'S'))
        .map(|(_, file)| file)
        .collect();
    if flagged.is_empty() {
        return Ok(0);
    }
    let indexed = git(&[&["ls-files", "-s", "-z", "--"][..], &flagged].concat())?;
    // Each one's mode and hash in the index.
    let staged = |file: &str| indexed.split('\0').find_map(|entry| entry.split_once('\t').filter(|(_, f)| *f == file).map(|(meta, _)| meta.split_whitespace().take(2).collect::<Vec<_>>()));
    let (mut files, mut changed) = (Vec::new(), 0);
    for file in flagged {
        match std::fs::symlink_metadata(path.join(file)) {
            Ok(m) if m.is_file() => {
                let mode = if m.permissions().mode() & 0o111 != 0 { "100755" } else { "100644" };
                match staged(file) {
                    Some(s) if s.first() == Some(&mode) => files.push(file),
                    _ => changed += 1,
                }
            }
            Ok(m) if m.file_type().is_symlink() => {
                let to = std::fs::read_link(path.join(file)).map(|t| t.to_string_lossy().into_owned());
                let hash = to.ok().and_then(|to| procs::run_stdin("git", &["-C", dir.as_ref(), "hash-object", "--stdin"], &to, CHECK_WAIT).ok());
                let same = staged(file).is_some_and(|s| s.first() == Some(&"120000") && hash.as_deref().map(str::trim) == s.get(1).copied());
                changed += u32::from(!same);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            _ => changed += 1,
        }
    }
    if files.is_empty() {
        return Ok(changed);
    }
    // One process hashes the files, a line each in order.
    let names: String = files.iter().map(|f| format!("{f}\n")).collect();
    let hashed = procs::run_stdin("git", &["-C", dir.as_ref(), "hash-object", "--stdin-paths"], &names, CHECK_WAIT).map_err(fail)?;
    let mut now = hashed.lines();
    for file in files {
        let hash = now.next().map(str::trim);
        if staged(file).and_then(|s| s.get(1).copied()).is_none_or(|s| Some(s) != hash) {
            changed += 1;
        }
    }
    Ok(changed)
}

/// Another repository or worktree inside `path`, found by its `.git`, which its own git doesn't
/// see: git passes over every path named `.git`. One can hide in a folder of tracked files (`git
/// init` where they are, its history its own), each of which, and every folder above one, is
/// looked at for a `.git` (`ls-files`); or in an ignored folder, each of which an ignore pattern
/// matches is looked through whole, links not followed: `--ignored=matching` lists one even when
/// all it holds is a worktree whose repository is gone, which the default listing takes for empty
/// and leaves out (git 2.50, 2026-10-12). Anywhere else it's an untracked folder, which status
/// counts. Err when git can't say, a folder can't be read, or there are too many ignored files to
/// look through (`MOST_LOOKED_AT`).
fn repo_inside(path: &Path) -> Result<Option<PathBuf>, String> {
    let dir = path.to_string_lossy();
    let tracked = procs::run("git", &["-C", dir.as_ref(), "ls-files", "-z"], None, CHECK_WAIT).map_err(|e| format!("git couldn't list what it tracks ({})", last_line(&e.to_string())))?;
    let mut folders: std::collections::BTreeSet<&Path> = std::collections::BTreeSet::new();
    for file in tracked.split('\0').filter(|f| !f.is_empty()) {
        folders.extend(Path::new(file).ancestors().skip(1).filter(|a| !a.as_os_str().is_empty()));
    }
    if let Some(folder) = folders.into_iter().map(|f| path.join(f)).find(|f| std::fs::symlink_metadata(f.join(".git")).is_ok()) {
        return Ok(Some(folder));
    }
    let status = procs::run("git", &["-C", dir.as_ref(), "status", "--porcelain=v2", "-z", "--ignored=matching", "--untracked-files=normal"], None, CHECK_WAIT)
        .map_err(|e| format!("git couldn't say what it ignores ({})", last_line(&e.to_string())))?;
    let mut folders: Vec<PathBuf> = status
        .split('\0')
        .filter_map(|entry| entry.strip_prefix("! "))
        .map(|p| path.join(p.trim_end_matches('/')))
        .filter(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir()))
        .collect();
    let mut looked = 0;
    while let Some(folder) = folders.pop() {
        let entries = std::fs::read_dir(&folder).map_err(|e| format!("couldn't look in {}: {e}", folder.strip_prefix(path).unwrap_or(&folder).display()))?;
        for e in entries.flatten() {
            looked += 1;
            if looked > MOST_LOOKED_AT {
                return Err("too many ignored files in it to look through for another repository".into());
            }
            if e.file_name() == ".git" {
                return Ok(Some(folder));
            }
            if e.file_type().is_ok_and(|k| k.is_dir()) {
                folders.push(e.path());
            }
        }
    }
    Ok(None)
}

/// How many sessions run in the worktree `lane_id` on `board`, but `except`.
fn running_in(board: &Board, lane_id: &str, except: Option<&str>) -> usize {
    let lane = board.lanes.iter().find(|l| l.id == lane_id && l.device.is_none());
    lane.map_or(0, |l| l.sessions.iter().filter(|s| s.visitor.is_none() && s.live && except != Some(s.key.as_str())).count())
}

/// Reads the worktree again just before it goes: what the plan saw must still hold, the same
/// HEAD and lock, nothing uncommitted or unpushed, not reusable, and no session running in it
/// (`running`, counted on the board as it is now), or it stays. Waiting for a session's process to
/// end gives time for work to come in, and only then are its ignored folders looked through
/// again: the plan has just done it otherwise. A lock the plan was to take off may have gone by
/// itself (a session's own, as its process ends): whether it's still there to take off.
fn verify(plan: &Plan, running: usize) -> Result<bool, String> {
    let now = facts(&plan.path, plan.base.as_deref()).map_err(|e| format!("{} stays: git couldn't check it again ({e}).", plan.name))?;
    let stays = |why: &str| Err(format!("{} stays: {why}.", plan.name));
    if running > 0 {
        return stays("a session is running in it now");
    }
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
    match plan.ending.map(|_| repo_inside(&plan.path)) {
        None | Some(Ok(None)) => {}
        Some(Ok(Some(inner))) => return stays(&format!("another repository is inside it now ({})", inner.strip_prefix(&plan.path).unwrap_or(&inner).display())),
        Some(Err(why)) => return stays(&why),
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
/// planned once the removals ahead of it are done, on the board as it is then (`board`, read each
/// time it's asked), and checked again just before it goes (`plan`, `carry_out`). `ending` is as
/// `plan` has it. Err when it isn't a worktree here.
pub fn remove(board: &dyn Fn() -> Board, cfg: &Config, lane_id: &str, ending: Option<&Session>) -> Result<Removed, String> {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let plan = plan(&board(), cfg, lane_id, ending)?;
    let except = ending.map(|s| s.key.as_str());
    let result = carry_out(&plan, &|| running_in(&board(), lane_id, except));
    Ok(Removed { plan, result })
}

/// Removes the worktree as `plan` says, or says why it stays. `running` counts the sessions
/// running in it now, for the last check.
fn carry_out(plan: &Plan, running: &dyn Fn() -> usize) -> Result<Gone, String> {
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
    if verify(plan, running())? {
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
/// it stays. `board` reads the board as it is, as `remove` does.
pub fn after_job(board: &dyn Fn() -> Board, cfg: &Config, s: &Session) -> Option<Result<Gone, String>> {
    let own = Path::new(s.own_worktree.as_deref()?);
    let real = std::fs::canonicalize(own).ok();
    let lanes = board().lanes;
    let lane = lanes.iter().find(|l| l.device.is_none() && (Path::new(&l.path) == own || real.as_deref().is_some_and(|r| std::fs::canonicalize(&l.path).is_ok_and(|p| p == r))))?;
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
        assert_eq!(carry_out(&kept, &|| 0), Err("brave-otter stays: 3 uncommitted files.".into()));
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
        assert_eq!(carry_out(&p, &|| 0), Ok(Gone { said: "Removed the worktree brave-otter and its branch, worktree-brave-otter.".into(), partly: false }));
        assert!(!wt.exists());
        assert!(git(&["branch", "--list", "worktree-brave-otter"]).trim().is_empty());
        // A commit on no remote holds the next one.
        git(&["worktree", "add", "-q", "-b", "worktree-kept", wt.to_str().unwrap(), "main"]);
        procs::run("git", &["-C", wt.to_str().unwrap(), "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "work"], None, Duration::from_secs(20)).unwrap();
        let p = plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap();
        assert_eq!(p.kept.as_deref(), Some("1 commit on no remote"));
        assert!(carry_out(&p, &|| 0).is_err());
        assert!(wt.exists());
        // Pushed, it goes; its branch stays where its work is merged nowhere but the remote's.
        procs::run("git", &["-C", wt.to_str().unwrap(), "push", "-q", "origin", "worktree-kept"], None, Duration::from_secs(20)).unwrap();
        let p = plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap();
        assert_eq!(p.kept, None);
        assert_eq!(carry_out(&p, &|| 0), Ok(Gone { said: "Removed the worktree brave-otter; its branch, worktree-kept, stays: its work isn't merged.".into(), partly: false }));
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
        assert_eq!(carry_out(&p, &|| 0), Ok(Gone { said: "Removed the worktree brave-otter through the repo's WorktreeRemove hook.".into(), partly: false }));
        assert!(!wt.exists());
        assert!(std::fs::read_to_string(&log).unwrap().contains("\"worktree_path\""));
        // A hook that takes the folder and then fails says so.
        git(&["worktree", "add", "-q", "-b", "worktree-half", wt.to_str().unwrap(), "main"]);
        let half = Hook { command: format!("git -C \"$CLAUDE_PROJECT_DIR\" worktree remove --force {}; echo 'port 3000 still taken' >&2; exit 1", wt.display()), ..p.hooks[0].clone() };
        let p = Plan { hooks: vec![half], after: vec![], ..plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap() };
        assert_eq!(carry_out(&p, &|| 0), Ok(Gone { said: "Removed the worktree brave-otter through the repo's WorktreeRemove hook, which failed, though: port 3000 still taken.".into(), partly: true }));
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
        std::fs::create_dir_all(main.join("src/component")).unwrap();
        std::fs::write(main.join("src/component/a.txt"), "a\n").unwrap();
        at(&main, &["add", ".gitignore", "local.conf", "src"]);
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
        // A repository of its own in a folder, as a clone or `git init` leaves one.
        let nested = |at: &Path| {
            std::fs::create_dir_all(at).unwrap();
            procs::run("git", &["-C", at.to_str().unwrap(), "init", "-q"], None, Duration::from_secs(20)).unwrap();
        };
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
        // Or made executable, which git takes for a change of its own.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(wt.join("local.conf"), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(kept(&board).as_deref(), Some("1 uncommitted file"));
        std::fs::set_permissions(wt.join("local.conf"), std::fs::Permissions::from_mode(0o644)).unwrap();
        at(&wt, &["update-index", "--no-assume-unchanged", "local.conf"]);
        // A repository made where tracked files are, its history its own: git passes over its
        // `.git`, so status sees nothing.
        procs::run("git", &["-C", wt.join("src/component").to_str().unwrap(), "init", "-q"], None, Duration::from_secs(20)).unwrap();
        assert_eq!(kept(&board).as_deref(), Some("another worktree is inside it (src/component)"));
        std::fs::remove_dir_all(wt.join("src/component/.git")).unwrap();
        // Another repository in an ignored folder, which nothing lists, and ones deep in folders
        // of builds or dependencies.
        nested(&wt.join("vendor/lib"));
        assert_eq!(kept(&board).as_deref(), Some("another worktree is inside it (vendor/lib)"));
        std::fs::remove_dir_all(wt.join("vendor")).unwrap();
        std::fs::create_dir_all(wt.join("target/feature")).unwrap();
        std::fs::write(wt.join("target/feature/.git"), "gitdir: /elsewhere\n").unwrap();
        assert_eq!(kept(&board).as_deref(), Some("another worktree is inside it (target/feature)"));
        std::fs::remove_dir_all(wt.join("target")).unwrap();
        nested(&wt.join("node_modules/@scope/pkg"));
        assert_eq!(kept(&board).as_deref(), Some("another worktree is inside it (node_modules/@scope/pkg)"));
        std::fs::remove_dir_all(wt.join("node_modules")).unwrap();
        // Work that comes in between the check and the removal keeps it.
        let p = plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap();
        assert_eq!(p.kept, None);
        at(&wt, &["commit", "-q", "--allow-empty", "-m", "late"]);
        assert_eq!(carry_out(&p, &|| 0), Err("wt stays: its HEAD moved since it was checked.".into()));
        at(&wt, &["reset", "-q", "--hard", "origin/main"]);
        let p = plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap();
        std::fs::write(wt.join("late.txt"), "").unwrap();
        assert_eq!(carry_out(&p, &|| 0), Err("wt stays: it has uncommitted files now.".into()));
        std::fs::remove_file(wt.join("late.txt")).unwrap();
        // Or while a session it was waiting on ended (a process here that has ended already).
        let mut ended = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let _ = ended.wait();
        let p = Plan { ending: Some(ended.id()), ..plan(&board, &cfg, wt.to_str().unwrap(), None).unwrap() };
        nested(&wt.join("vendor/late"));
        assert_eq!(carry_out(&p, &|| 0), Err("wt stays: another repository is inside it now (vendor/late).".into()));
        std::fs::remove_dir_all(wt.join("vendor")).unwrap();
        // A session that started in it meanwhile keeps it, as the board has it then.
        assert_eq!(carry_out(&p, &|| 1), Err("wt stays: a session is running in it now.".into()));
        // A tracked link git is told not to look at, pointed elsewhere.
        std::os::unix::fs::symlink("local.conf", wt.join("link")).unwrap();
        at(&wt, &["add", "link"]);
        at(&wt, &["commit", "-q", "-m", "link"]);
        at(&wt, &["push", "-q", "origin", "HEAD:refs/heads/wt"]);
        at(&wt, &["update-index", "--assume-unchanged", "link"]);
        assert_eq!(kept(&board), None, "a link as the index has it");
        std::fs::remove_file(wt.join("link")).unwrap();
        std::os::unix::fs::symlink(".gitignore", wt.join("link")).unwrap();
        assert_eq!(kept(&board).as_deref(), Some("1 uncommitted file"));
        std::fs::remove_file(wt.join("link")).unwrap();
        std::os::unix::fs::symlink("local.conf", wt.join("link")).unwrap();
        at(&wt, &["update-index", "--no-assume-unchanged", "link"]);
        at(&wt, &["reset", "-q", "--hard", "origin/main"]);
        // One that has taken new work three times is reusable, though the board read no git: the
        // resets above, onto the default branch, as a reused worktree is put back.
        for _ in 0..1 {
            at(&wt, &["reset", "-q", "--hard", "origin/main"]);
        }
        assert_eq!(kept(&board).as_deref(), Some("reusable: it has taken several pieces of work"));
        assert!(wt.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

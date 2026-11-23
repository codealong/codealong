//! A worktree's own rules. Whether it's reusable, taking one piece of work after another
//! (`reusable`, `looks_reusable`), what a reusable one holds now and which of its sessions are
//! from the work before (`reuse_of`, `earlier`); whether it can be removed with nothing lost
//! (`cleanup`, over a `Leaving`) and whether its lock holds; and whether it looks finished.
//!
//! Pure functions over git state and sessions already read: cleanup.rs reads a worktree again
//! just before it goes and asks `cleanup` the same way the board did. A rule's test goes in
//! `tests` below.

use super::*;

/// Whether a worktree looks finished: not the main checkout or a plain folder (an app's chats
/// among them), which aren't cleaned up, no PR that says where it stands already (a merged or
/// closed one has its own bay), nothing running or waiting, and every session of its own read
/// by Jev as finished.
/// Whether a worktree could take one piece of work after another: a repo's worktree in one of
/// your projects. A plain folder, an app's chats and a repo outside every project have no work
/// of yours to reuse them for.
pub fn can_reuse(seed: &LaneSeed) -> bool {
    !seed.elsewhere && !seed.chats && seed.origin != Origin::Folder
}

/// Whether a worktree takes one piece of work after another: as marked (its menu's Reuse for
/// new work, reusable.rs), else a main checkout does, being where work starts when there's no
/// other; the rest are kept for one piece of work each.
pub fn reusable(seed: &LaneSeed, marks: &HashMap<String, bool>) -> bool {
    can_reuse(seed) && marks.get(seed.path.to_string_lossy().as_ref()).copied().unwrap_or(seed.origin == Origin::Main)
}

/// A worktree Settings suggests making reusable: marked neither way and not reusable already,
/// whose branch has carried two or more merged PRs among the repo's recent ones. A branch that
/// goes on from one merged PR to the next is a worktree taking one piece of work after another
/// (one repo's `app2`, reset onto `origin/main` after each). PRs from anyone's fork count: a
/// teammate's `app2` says the repo's people name reused worktrees so.
pub fn looks_reusable(seed: &LaneSeed, marked: bool, prs: &HashMap<String, Vec<PrRaw>>) -> bool {
    if marked || !can_reuse(seed) || seed.origin == Origin::Main {
        return false;
    }
    let (Some(slug), Some(branch)) = (seed.repo_slug.as_ref(), seed.branch.as_ref()) else { return false };
    if seed.default_branch.as_ref() == Some(branch) {
        return false;
    }
    let merged = |p: &&PrRaw| &p.head_ref == branch && p.state == PrState::Merged;
    prs.get(slug).map_or(0, |list| list.iter().filter(merged).count()) >= 2
}

/// Whether a session in a reusable worktree belongs to the work before its latest: it was last
/// active before the worktree last took new work (`git::reset_at`). One that needs you or is
/// working belongs to now, whenever it began.
pub fn earlier(s: &Session, reset_at: Option<i64>) -> bool {
    reset_at.is_some_and(|at| s.last_activity < at) && !matches!(s.status, Status::Needs | Status::Error | Status::Working)
}

/// What a reusable worktree holds, from its PR, its git state and its current work's sessions:
/// in review while its PR is open; open while an agent of its own is at work there or it has
/// uncommitted changes or commits that haven't landed; landed once its PR merged, until the
/// next work starts; otherwise free. A session idle there doesn't hold it: it can take the
/// next work, as another can.
pub fn reuse_of(pr: Option<&Pr>, git: Option<&GitInfo>, sessions: &[Session]) -> Reuse {
    if pr.is_some_and(|p| p.state.is_open()) {
        return Reuse::Review;
    }
    let at_work = sessions.iter().any(|s| s.visitor.is_none() && !s.archived && !s.earlier && matches!(s.status, Status::Working | Status::Needs | Status::Error));
    if at_work || git.is_some_and(|g| g.dirty > 0) {
        return Reuse::Open;
    }
    // A squash merge leaves the branch's commits off the default branch, so a merged PR on the
    // worktree's HEAD (`find_pr`) says they landed.
    if pr.is_some_and(|p| p.state == PrState::Merged) {
        return Reuse::Landed;
    }
    if git.and_then(|g| g.base_ahead).unwrap_or(0) > 0 {
        return Reuse::Open;
    }
    Reuse::Free
}

/// What decides whether a worktree can go (`cleanup`): as the board has it, or as it's read
/// again just before it goes (cleanup.rs).
#[derive(Debug, Clone, Copy)]
pub struct Leaving<'a> {
    pub origin: Origin,
    /// Codealong's own chats (`LaneSeed::starts_chats`).
    pub chats: bool,
    /// It's marked reusable (`reusable`).
    pub reusable: bool,
    pub git: Option<&'a GitInfo>,
    /// How many sessions run in it: a process has them open.
    pub running: usize,
    /// A worktree inside its folder, by its name: removing this one would take that one's files.
    pub holds: Option<&'a str>,
    /// Its lock still holds (`lock_holds`), with the lock's reason.
    pub lock: Option<&'a str>,
    /// Its HEAD is the last commit of a PR that merged on GitHub (ghpoll.rs `Heads`), which keeps
    /// its commits after deleting the PR's branch.
    pub merged: bool,
}

/// The worktrees GitHub is asked about for whether their HEAD is a merged PR's last commit
/// (ghpoll.rs `Heads`), as (repo slug, commit): those `cleanup` would keep for commits on no
/// remote, in a repo on GitHub, at their HEAD as `heads` has it. A main checkout and a plain
/// folder are kept whatever it says.
pub fn heads_on_no_remote(seeds: &[LaneSeed], git: &HashMap<PathBuf, GitInfo>, heads: &HashMap<PathBuf, String>) -> Vec<(String, String)> {
    seeds
        .iter()
        .filter(|s| !matches!(s.origin, Origin::Main | Origin::Folder) && !s.chats && !s.starts_chats)
        .filter(|s| git.get(&s.path).and_then(|g| g.unpushed).is_some_and(|n| n > 0))
        .filter_map(|s| Some((s.repo_slug.clone()?, heads.get(&s.path)?.clone())))
        .collect()
}

/// Whether a worktree can be removed with nothing in it lost: no file uncommitted or untracked
/// (ignored ones, such as dependencies and env files, don't count), no commit that isn't on some
/// remote unless its HEAD is a merged PR's last commit, no session running in it or lock held on
/// it, and no worktree inside it. A main checkout, a plain folder and a reusable worktree are kept
/// whatever they hold. None while its git hasn't been read, when nothing else has said it stays.
pub fn cleanup(w: &Leaving) -> Option<Cleanup> {
    let kept = |why: String| Some(Cleanup { ok: false, why });
    if w.origin == Origin::Main {
        return kept("the main checkout".into());
    }
    if w.origin == Origin::Folder || w.chats {
        return kept("a folder, not a worktree".into());
    }
    if w.reusable {
        return kept("marked reusable".into());
    }
    let mut why = Vec::new();
    match w.running {
        0 => {}
        1 => why.push("a session is running in it".to_string()),
        n => why.push(format!("{n} sessions are running in it")),
    }
    // A session's own lock goes without saying while it runs.
    if let (Some(reason), 0) = (w.lock, w.running) {
        why.push(match claude_lock(reason) {
            Some(_) => "locked by a running session".to_string(),
            None if reason.is_empty() => "locked".to_string(),
            None => format!("locked: {}", crate::util::truncate(reason, 60)),
        });
    }
    if let Some(inner) = w.holds {
        why.push(format!("another worktree is inside it ({inner})"));
    }
    if let Some(g) = w.git {
        match g.dirty {
            0 => {}
            1 => why.push("1 uncommitted file".into()),
            n => why.push(format!("{n} uncommitted files")),
        }
        match g.unpushed {
            Some(0) => {}
            Some(_) if w.merged => {}
            Some(1) => why.push("1 commit on no remote".into()),
            Some(n) => why.push(format!("{n} commits on no remote")),
            // Git couldn't say: HEAD has no commit yet, or git failed.
            None => why.push("its commits couldn't be checked".into()),
        }
    }
    if !why.is_empty() {
        return kept(why.join(", "));
    }
    w.git.map(|_| Cleanup { ok: true, why: String::new() })
}

/// A lock Claude Code put on a worktree a session of its runs in, `claude session <name> (pid
/// <n> start <time>)`: the process that holds it, and when it started, as `ps -o lstart` writes
/// it in UTC ("Mon Oct 12 22:47:57 2026"), which tells it from another process given the same
/// number since. None for any other lock.
pub fn claude_lock(reason: &str) -> Option<(u32, Option<&str>)> {
    let rest = reason.trim().strip_prefix("claude session ")?;
    let (_, held) = rest.rsplit_once(" (pid ")?;
    let held = held.strip_suffix(')')?;
    let (pid, start) = match held.split_once(" start ") {
        Some((pid, start)) => (pid, Some(start.trim())),
        None => (held, None),
    };
    Some((pid.trim().parse().ok()?, start))
}

/// Whether a worktree's lock holds, as the board sees it: Claude Code's while the session that
/// took it runs (`live_pids`, the processes of the board's running sessions), any other always.
/// A session hung up on leaves its lock behind (`/exit` takes it off), which holds nothing.
pub fn lock_holds(reason: &str, live_pids: &HashSet<u32>) -> bool {
    claude_lock(reason).map_or(true, |(pid, _)| live_pids.contains(&pid))
}

pub fn looks_finished(origin: Origin, pr: Option<&Pr>, sessions: &[Session]) -> bool {
    if matches!(origin, Origin::Main | Origin::Folder) || pr.is_some_and(|p| !p.state.is_open()) {
        return false;
    }
    let own: Vec<&Session> = sessions.iter().filter(|s| s.visitor.is_none() && !s.archived && !s.unattended()).collect();
    !own.is_empty()
        && own.iter().all(|s| !matches!(s.status, Status::Working | Status::Needs) && s.insight.as_ref().and_then(|i| i.finished).is_some_and(|f| f >= FINISHED))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::fixtures::{acct, insight, pr, reg, seed, tr, NOW};

    #[test]
    fn main_checkouts_and_marked_worktrees_are_reusable() {
        let main = LaneSeed { origin: Origin::Main, ..seed("/r/spark", "app") };
        let wt = seed("/r/spark-app3", "app3");
        let none = HashMap::new();
        assert!(reusable(&main, &none));
        assert!(!reusable(&wt, &none));
        let marks = HashMap::from([("/r/spark".to_string(), false), ("/r/spark-app3".to_string(), true)]);
        assert!(!reusable(&main, &marks));
        assert!(reusable(&wt, &marks));
        // A plain folder, an app's chats and a repo outside your projects never are.
        let all = HashMap::from([("/r/notes".to_string(), true)]);
        assert!(!reusable(&LaneSeed { origin: Origin::Folder, ..seed("/r/notes", "x") }, &all));
        assert!(!reusable(&LaneSeed { chats: true, ..seed("/r/notes", "x") }, &all));
        assert!(!reusable(&LaneSeed { elsewhere: true, ..seed("/r/notes", "x") }, &all));
    }

    #[test]
    fn a_branch_that_carried_several_merged_prs_looks_reusable() {
        let wt = seed("/r/spark-app3", "app3");
        let merged = |n: u64, owner: &str| PrRaw { number: n, head_ref: "app3".into(), head_owner: Some(owner.into()), ..pr(PrState::Merged, 0) };
        let prs = |list: Vec<PrRaw>| HashMap::from([("sprucehq/spark".to_string(), list)]);
        let two = prs(vec![merged(1, "gwen"), merged(2, "gwen")]);
        assert!(looks_reusable(&wt, false, &two));
        // A teammate's `app3` from their fork says the same of the name.
        assert!(looks_reusable(&wt, false, &prs(vec![merged(1, "gwen"), merged(2, "someone")])));
        // One merged PR is a branch's usual life, and an open one hasn't landed.
        assert!(!looks_reusable(&wt, false, &prs(vec![merged(1, "gwen"), PrRaw { state: PrState::Open, ..merged(2, "gwen") }])));
        // Marked either way, or a main checkout, it isn't suggested; nor is the default branch.
        assert!(!looks_reusable(&wt, true, &two));
        assert!(!looks_reusable(&LaneSeed { origin: Origin::Main, ..wt.clone() }, false, &two));
        let on_main = |n| PrRaw { head_ref: "main".into(), ..merged(n, "gwen") };
        assert!(!looks_reusable(&LaneSeed { branch: Some("main".into()), ..wt.clone() }, false, &prs(vec![on_main(1), on_main(2)])));
    }

    #[test]
    fn what_a_reusable_worktree_holds() {
        let git = |dirty: u32, ahead: u32| GitInfo { dirty, base_ahead: Some(ahead), ..Default::default() };
        let base = claude_session(&acct("claude"), Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let with = |status: Status| Session { status, live: true, ..base.clone() };
        let pr_in = |state: PrState| Pr { number: 1, title: String::new(), url: String::new(), state, checks: Checks::default(), merge: Merge::Unknown, updated_at: 0, merged_at: None, closed_at: None, stale_approval: None };
        let clean = git(0, 0);
        assert_eq!(reuse_of(None, Some(&clean), &[]), Reuse::Free);
        // An agent sitting idle there doesn't hold it; one at work does.
        assert_eq!(reuse_of(None, Some(&clean), &[with(Status::Idle), with(Status::Unread)]), Reuse::Free);
        assert_eq!(reuse_of(None, Some(&clean), &[with(Status::Working)]), Reuse::Open);
        assert_eq!(reuse_of(None, Some(&clean), &[with(Status::Needs)]), Reuse::Open);
        // Visitors, and earlier work's sessions, belong elsewhere.
        assert_eq!(reuse_of(None, Some(&clean), &[Session { visitor: Some("worked here".into()), ..with(Status::Working) }]), Reuse::Free);
        // Uncommitted changes, or commits that haven't landed, are work under way.
        assert_eq!(reuse_of(None, Some(&git(2, 0)), &[]), Reuse::Open);
        assert_eq!(reuse_of(None, Some(&git(0, 3)), &[]), Reuse::Open);
        assert_eq!(reuse_of(Some(&pr_in(PrState::Open)), Some(&git(0, 3)), &[]), Reuse::Review);
        assert_eq!(reuse_of(Some(&pr_in(PrState::Changes)), Some(&git(1, 3)), &[]), Reuse::Review);
        // A squash merge leaves the commits off the default branch; the merged PR says they landed.
        assert_eq!(reuse_of(Some(&pr_in(PrState::Merged)), Some(&git(0, 3)), &[]), Reuse::Landed);
        assert_eq!(reuse_of(Some(&pr_in(PrState::Merged)), Some(&git(1, 3)), &[]), Reuse::Open);
        // Closed without merging, its commits haven't landed.
        assert_eq!(reuse_of(Some(&pr_in(PrState::Closed)), Some(&git(0, 3)), &[]), Reuse::Open);
    }

    #[test]
    fn a_reused_worktree_sets_aside_the_sessions_from_before_its_latest_work() {
        let path = "/r/spark-app3";
        let seeds = vec![seed(path, "app3")];
        let base = claude_session(&acct("claude"), Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let at = |key: &str, status: Status, ago: i64| Session { key: key.into(), id: key.into(), cwd: path.into(), status, live: false, last_activity: NOW - ago, last_prompt_at: NOW - ago, ..base.clone() };
        // Shipped the last work and was never read; the worktree was reset for new work since.
        let shipped = at("c:shipped", Status::Unread, 5 * 3_600_000);
        let asked = at("c:asked", Status::Needs, 6 * 3_600_000);
        let fresh = at("c:fresh", Status::Idle, 60_000);
        let git = HashMap::from([(PathBuf::from(path), GitInfo { reset_at: Some(NOW - 2 * 3_600_000), base_ahead: Some(0), ..Default::default() })]);
        let (visitors, prs, order) = (HashMap::new(), HashMap::new(), vec!["Spruce".to_string()]);
        let board_with = |sessions: Vec<Session>, marks: &HashMap<String, bool>| {
            compose(Inputs {
                now: NOW,
                apps: &AppNames::default(),
                seeds: &seeds,
                sessions,
                visitors: &visitors,
                git: &git,
                prs: &prs,
                requested: &HashMap::new(),
                made: &HashMap::new(),
                project_order: &order,
                notices: vec![],
                loading: false,
                github_at: None,
                pinned: &[],
                github_login: None,
                pr_links: &PrLinks::default(),
                previews: &Previews::default(),
                reusable: marks,
                review_snoozes: &[],
                session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new(),
            })
        };
        let marked = HashMap::from([(path.to_string(), true)]);
        let b = board_with(vec![shipped.clone(), asked.clone(), fresh.clone()], &marked);
        let lane = &b.lanes[0];
        // The one that needs you belongs to now whenever it began; the shipped one comes after,
        // and out of the worktree's status.
        let keys: Vec<(&str, bool)> = lane.sessions.iter().map(|s| (s.key.as_str(), s.earlier)).collect();
        assert_eq!(keys, vec![("c:asked", false), ("c:fresh", false), ("c:shipped", true)]);
        assert_eq!(lane.bay, Bay::Needs);
        assert_eq!(lane.reuse, Some(Reuse::Open));
        // Without the one that needs you, the unread reply from before doesn't make it Your turn.
        let b = board_with(vec![shipped.clone(), fresh.clone()], &marked);
        assert_eq!(b.lanes[0].bay, Bay::Idle);
        assert_eq!(b.lanes[0].reuse, Some(Reuse::Free));
        // Kept for its own work, nothing is set aside.
        let b = board_with(vec![shipped.clone(), asked.clone(), fresh.clone()], &HashMap::new());
        assert!(b.lanes[0].sessions.iter().all(|s| !s.earlier));
        assert_eq!(b.lanes[0].reuse, None);
    }

    #[test]
    fn a_worktree_looks_finished_when_every_session_in_it_is() {
        let a = acct("claude2");
        let done = |f: f64| {
            let mut s = claude_session(&a, None, Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
            apply_insight(&mut s, &insight(|i| i.finished = Some(f)));
            s
        };
        assert!(looks_finished(Origin::Script, None, &[done(0.9)]));
        assert!(!looks_finished(Origin::Script, None, &[done(0.9), done(0.5)]));
        assert!(!looks_finished(Origin::Main, None, &[done(0.9)]));
        assert!(!looks_finished(Origin::Script, None, &[]));
        let mut visitor = done(0.1);
        visitor.visitor = Some("worked here".into());
        assert!(looks_finished(Origin::Script, None, &[done(0.9), visitor]));
        let mut working = done(0.9);
        working.status = Status::Working;
        assert!(!looks_finished(Origin::Script, None, &[working]));
    }

    #[test]
    fn a_worktree_can_go_only_when_nothing_in_it_would_be_lost() {
        let clean = GitInfo { unpushed: Some(0), ..Default::default() };
        let w = Leaving { origin: Origin::ClaudeApp, chats: false, reusable: false, git: Some(&clean), running: 0, holds: None, lock: None, merged: false };
        let said = |w: Leaving| cleanup(&w).map(|c| (c.ok, c.why));
        assert_eq!(said(w), Some((true, String::new())));
        // Kept whatever they hold.
        assert_eq!(said(Leaving { origin: Origin::Main, ..w }), Some((false, "the main checkout".into())));
        assert_eq!(said(Leaving { origin: Origin::Folder, ..w }), Some((false, "a folder, not a worktree".into())));
        assert_eq!(said(Leaving { chats: true, ..w }), Some((false, "a folder, not a worktree".into())));
        assert_eq!(said(Leaving { reusable: true, git: None, ..w }), Some((false, "marked reusable".into())));
        // Unknown until its git is read.
        assert_eq!(said(Leaving { git: None, ..w }), None);
        // What would be lost, all of it said.
        let work = GitInfo { dirty: 3, unpushed: Some(2), ..Default::default() };
        assert_eq!(said(Leaving { git: Some(&work), ..w }), Some((false, "3 uncommitted files, 2 commits on no remote".into())));
        let one = GitInfo { dirty: 1, unpushed: Some(1), ..Default::default() };
        assert_eq!(said(Leaving { git: Some(&one), ..w }), Some((false, "1 uncommitted file, 1 commit on no remote".into())));
        let unborn = GitInfo { unpushed: None, ..Default::default() };
        assert_eq!(said(Leaving { git: Some(&unborn), ..w }), Some((false, "its commits couldn't be checked".into())));
        // At a merged PR's last commit, its commits on no remote are GitHub's to keep; what's
        // uncommitted still isn't, nor commits git couldn't count.
        let merged = GitInfo { unpushed: Some(4), ..Default::default() };
        assert_eq!(said(Leaving { git: Some(&merged), merged: true, ..w }), Some((true, String::new())));
        assert_eq!(said(Leaving { git: Some(&work), merged: true, ..w }), Some((false, "3 uncommitted files".into())));
        assert_eq!(said(Leaving { git: Some(&unborn), merged: true, ..w }), Some((false, "its commits couldn't be checked".into())));
        assert_eq!(said(Leaving { origin: Origin::Main, git: Some(&merged), merged: true, ..w }), Some((false, "the main checkout".into())));
        assert_eq!(said(Leaving { holds: Some("brave-otter"), ..w }), Some((false, "another worktree is inside it (brave-otter)".into())));
        // A running session; its own lock goes without saying.
        let lock = "claude session brave-otter (pid 46561 start Mon Oct 12 22:47:57 2026)";
        assert_eq!(said(Leaving { running: 1, lock: Some(lock), ..w }), Some((false, "a session is running in it".into())));
        assert_eq!(said(Leaving { running: 2, ..w }), Some((false, "2 sessions are running in it".into())));
        assert_eq!(said(Leaving { lock: Some(lock), ..w }), Some((false, "locked by a running session".into())));
        assert_eq!(said(Leaving { lock: Some("moving disks"), ..w }), Some((false, "locked: moving disks".into())));
        assert_eq!(said(Leaving { lock: Some(""), ..w }), Some((false, "locked".into())));
    }

    #[test]
    fn reads_claude_codes_lock_and_whether_it_holds() {
        assert_eq!(claude_lock("claude session brave-otter (pid 46561 start Mon Oct 12 22:47:57 2026)\n"), Some((46561, Some("Mon Oct 12 22:47:57 2026"))));
        assert_eq!(claude_lock("claude session my (odd) name (pid 7)"), Some((7, None)));
        assert_eq!(claude_lock("moving disks"), None);
        assert_eq!(claude_lock("claude session x (pid many)"), None);
        let live = HashSet::from([46561]);
        assert!(lock_holds("claude session a (pid 46561 start Mon Oct 12 22:47:57 2026)", &live));
        // Left behind by a session that's gone.
        assert!(!lock_holds("claude session a (pid 99 start Mon Oct 12 22:47:57 2026)", &live));
        // Anyone else's holds.
        assert!(lock_holds("moving disks", &live));
    }

    #[test]
    fn the_board_says_which_worktrees_can_go() {
        let apps = AppNames::default();
        let main = LaneSeed { origin: Origin::Main, callsign: "spark".into(), ..seed("/r/spark", "main") };
        let done = LaneSeed { origin: Origin::ClaudeApp, callsign: "done".into(), ..seed("/r/spark/.claude/worktrees/done", "worktree-done") };
        let busy = LaneSeed { origin: Origin::ClaudeApp, callsign: "busy".into(), ..seed("/r/spark/.claude/worktrees/busy", "worktree-busy") };
        let left = LaneSeed { origin: Origin::ClaudeApp, callsign: "left".into(), ..seed("/r/spark/.claude/worktrees/left", "worktree-left") };
        let outer = LaneSeed { callsign: "outer".into(), ..seed("/r/spark-outer", "outer") };
        let inner = LaneSeed { origin: Origin::ClaudeApp, callsign: "inner".into(), ..seed("/r/spark-outer/.claude/worktrees/inner", "worktree-inner") };
        let unread = LaneSeed { callsign: "unread".into(), ..seed("/r/spark-unread", "unread") };
        // Branches GitHub deleted: one's HEAD is its merged PR's last commit, the other's isn't.
        let landed = LaneSeed { origin: Origin::ClaudeApp, callsign: "landed".into(), head: Some("1a2b3c4".into()), ..seed("/r/spark/.claude/worktrees/landed", "worktree-landed") };
        let parked = LaneSeed { origin: Origin::ClaudeApp, callsign: "parked".into(), head: Some("5d6e7f8".into()), ..seed("/r/spark/.claude/worktrees/parked", "worktree-parked") };
        let seeds = vec![main, done, busy, left, outer, inner, unread, landed, parked];
        let clean = GitInfo { unpushed: Some(0), ..Default::default() };
        let lock = |pid: u32| GitInfo { locked: Some(format!("claude session x (pid {pid} start Mon Oct 12 22:47:57 2026)")), ..clean.clone() };
        let git: HashMap<PathBuf, GitInfo> = HashMap::from([
            (PathBuf::from("/r/spark"), clean.clone()),
            (PathBuf::from("/r/spark/.claude/worktrees/done"), clean.clone()),
            (PathBuf::from("/r/spark/.claude/worktrees/busy"), lock(4242)),
            // Its session was hung up on: the lock it left holds nothing.
            (PathBuf::from("/r/spark/.claude/worktrees/left"), lock(99)),
            (PathBuf::from("/r/spark-outer"), clean.clone()),
            (PathBuf::from("/r/spark-outer/.claude/worktrees/inner"), clean.clone()),
            (PathBuf::from("/r/spark/.claude/worktrees/landed"), GitInfo { unpushed: Some(3), ..clean.clone() }),
            (PathBuf::from("/r/spark/.claude/worktrees/parked"), GitInfo { unpushed: Some(2), ..clean.clone() }),
        ]);
        // GitHub is asked only about the HEADs with commits on no remote, as git last read them:
        // parked has a commit the worktree listing hasn't caught up with.
        let mut heads: HashMap<PathBuf, String> = seeds.iter().filter_map(|s| Some((s.path.clone(), s.head.clone()?))).collect();
        heads.insert(PathBuf::from("/r/spark/.claude/worktrees/parked"), "9f8e7d6".into());
        assert_eq!(heads_on_no_remote(&seeds, &git, &heads), [("sprucehq/spark".to_string(), "1a2b3c4".to_string()), ("sprucehq/spark".to_string(), "9f8e7d6".to_string())]);
        let mut running = claude_session(&acct("claude"), Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        (running.cwd, running.pid, running.live) = ("/r/spark/.claude/worktrees/busy".into(), Some(4242), true);
        // Archived, it still runs.
        running.archived = true;
        let (prs, visitors) = (HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![running], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::from([PathBuf::from("/r/spark/.claude/worktrees/landed")]), working_elsewhere: &BTreeSet::new() });
        let said = |p: &str| board.lanes.iter().find(|l| l.path == p).unwrap().cleanup.clone().map(|c| (c.ok, c.why));
        assert_eq!(said("/r/spark"), Some((false, "the main checkout".into())));
        assert_eq!(said("/r/spark/.claude/worktrees/landed"), Some((true, String::new())));
        assert_eq!(said("/r/spark/.claude/worktrees/parked"), Some((false, "2 commits on no remote".into())));
        assert_eq!(said("/r/spark/.claude/worktrees/done"), Some((true, String::new())));
        assert_eq!(said("/r/spark/.claude/worktrees/busy"), Some((false, "a session is running in it".into())));
        assert_eq!(said("/r/spark/.claude/worktrees/left"), Some((true, String::new())));
        assert_eq!(said("/r/spark-outer"), Some((false, "another worktree is inside it (inner)".into())));
        assert_eq!(said("/r/spark-outer/.claude/worktrees/inner"), Some((true, String::new())));
        assert_eq!(said("/r/spark-unread"), None);
    }
}

//! Orchestrators: a schedule that keeps watch over your sessions for you (`Schedule::orchestrates`),
//! one for a project, and one for All projects that takes every project without one of its own and
//! the sessions in none (Elsewhere). It keeps one session going (`To::Thread`), started afresh each
//! day with its notes carried over, quiet unless it asks you something, and it runs only when
//! something it keeps watch over needs a look (`Event::Changes`): a session newly waiting on you, a
//! PR of yours that waits on you with no session told of it, two agents in one worktree. It runs
//! at most once every `GAP_MS`, what comes meanwhile waiting for that run. Nothing runs while
//! nothing changes, so an orchestrator costs no usage in a quiet hour, and one per project costs no
//! more than what happens in them. A finished turn, a PR opened or merged, aren't a run of their
//! own: Codealong shows them already, and the next run reads them (`codealong changes`).
//!
//! Each of a project's sessions is its project's orchestrator's when that one is on, else All
//! projects' (`owner`): two orchestrators never read, tag or suggest for one session. An
//! orchestrator's own sessions are listed first in the Sessions of what it keeps watch over, under
//! no worktree (`Session::orchestrator`, `mark`), and never among what it watches.
//!
//! What it does each run is the codealong skill's references/orchestrate.md: what its session is
//! told is a pointer to that, with what it found and its notes (runner.rs `prompt_of`).

use serde::{Deserialize, Serialize};

use super::*;
use crate::model::{Lane, Session, Status};

/// What an orchestrator keeps watch over.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Orchestrates {
    /// The project, by its name in config.toml; none for All projects: every project without an
    /// orchestrator of its own, and the sessions in none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// It may message sessions (`session send`, a steer, a resume with a prompt): only when you
    /// say so, since what it sends starts a turn you didn't ask for (`refused`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub send: bool,
    /// It may start sessions (`codealong start`, a helper, a fork): only when you say so, since
    /// each spends usage.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub start: bool,
}

impl Orchestrates {
    pub fn of(project: Option<&str>) -> Orchestrates {
        Orchestrates { project: project.map(str::trim).filter(|p| !p.is_empty()).map(str::to_string), ..Orchestrates::default() }
    }

    /// Whether it's the same as another, the project's name whatever its case.
    pub fn same(&self, other: &Orchestrates) -> bool {
        match (&self.project, &other.project) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            (None, None) => true,
            _ => false,
        }
    }

    /// What it keeps watch over, in a word or two: the project's name, or All projects.
    pub fn words(&self) -> String {
        self.project.clone().unwrap_or_else(|| ALL.to_string())
    }
}

/// All projects, as the project switcher says it.
pub const ALL: &str = "All projects";
/// An orchestrator runs at most this often for what it finds: what comes meanwhile waits for its
/// next run, which takes it all at once, so a burst of sessions waiting on you is one run rather
/// than one each (`resting`). Most runs, one for each event, found nothing to tell you.
pub const GAP_MS: i64 = 15 * 60_000;
/// How what `shared` finds is told apart: two agents in one worktree, which don't wait out the gap.
const SHARED: &str = "shared:";
/// How a PR's move is told apart (`found`): one that's yours doesn't wait out the gap either, as
/// its states in between would go unnoted, and the one it ends at be told against the last.
const PR_MOVE: &str = "pr:";
/// Its session is started afresh this often, its notes carrying what the last one knew.
pub const FRESH_AFTER: &str = "1d";

/// Whether a schedule is an orchestrator that runs: one that's paused or finished keeps watch over
/// nothing, and what it kept watch over goes to All projects'.
pub fn on(s: &Schedule) -> bool {
    s.orchestrates.is_some() && !s.paused && s.finished_at.is_none()
}

/// The orchestrator for a scope, on or not.
pub fn find<'a>(file: &'a File, scope: &Orchestrates) -> Option<&'a Schedule> {
    file.schedules.iter().find(|s| s.orchestrates.as_ref().is_some_and(|o| o.same(scope)))
}

/// The orchestrator a project's sessions are for (`Lane::project`, Elsewhere among them): its own,
/// while that's on, else All projects', while that's on; none when neither is.
pub fn owner<'a>(file: &'a File, project: &str) -> Option<&'a Schedule> {
    let own = Orchestrates::of(Some(project));
    let runs = |scope: &Orchestrates| find(file, scope).filter(|s| on(s));
    runs(&own).or_else(|| runs(&Orchestrates::default()))
}

/// The projects whose sessions an orchestrator keeps watch over are told by these: its own, for a
/// project's; every project but these, for All projects'.
pub fn told_elsewhere(file: &File, s: &Schedule) -> Vec<String> {
    match s.orchestrates.as_ref() {
        Some(Orchestrates { project: None, .. }) => file
            .schedules
            .iter()
            .filter(|o| on(o))
            .filter_map(|o| o.orchestrates.as_ref()?.project.clone())
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether a project's sessions are an orchestrator's to keep watch over.
pub fn keeps(file: &File, s: &Schedule, project: &str) -> bool {
    owner(file, project).is_some_and(|o| o.id == s.id)
}

/// What its session is told each run, before what it found and its notes.
pub fn prompt(scope: &Orchestrates) -> String {
    let over = match &scope.project {
        Some(p) => format!("the {p} project's sessions"),
        None => "every project's sessions that hasn't an orchestrator of its own, and the sessions in no project".to_string(),
    };
    format!(
        "You're Codealong's orchestrator for {}: you keep watch over {over} for your user. Run a tick as the codealong skill's references/orchestrate.md says, from *Each tick*.",
        scope.words()
    )
}

/// Its name in the Schedules view and on its session: "Orchestrator" for All projects, "Northwind
/// orchestrator" for a project's.
pub fn name(scope: &Orchestrates) -> String {
    match &scope.project {
        Some(p) => format!("{p} orchestrator"),
        None => "Orchestrator".to_string(),
    }
}

/// When it runs, in words.
pub fn when_words(scope: &Orchestrates) -> String {
    match &scope.project {
        Some(p) => format!("When something in {p} needs a look"),
        None => "When something needs a look".to_string(),
    }
}

/// A new orchestrator, as `orchestrator start` and the Sessions view's Start orchestrator make
/// one: one session kept going out of sight among Codealong's chats, started afresh each day with
/// its notes, quiet, on the account given (empty for Auto, picked at each start), on this machine.
pub fn schedule(scope: Orchestrates, account: &str, model: Option<String>) -> Schedule {
    Schedule {
        name: name(&scope),
        when: When::Event { event: Event::Changes, repo: String::new(), label: String::new(), mine: false },
        run: Run {
            to: To::Thread,
            account: account.trim().to_string(),
            model: model.filter(|m| !m.trim().is_empty()),
            prompt: prompt(&scope),
            fresh_after: FRESH_AFTER.to_string(),
            ..Run::default()
        },
        quiet: true,
        notes: true,
        catch_up: CatchUp::Skip,
        orchestrates: Some(scope),
        ..Schedule::default()
    }
}

/// Why an orchestrator can't be saved as it is.
pub fn check(s: &Schedule, cfg: &Config) -> std::result::Result<(), String> {
    let Some(scope) = &s.orchestrates else { return Ok(()) };
    if let Some(p) = &scope.project {
        if !cfg.projects.iter().any(|c| c.name.eq_ignore_ascii_case(p)) {
            return Err(format!("There's no project named {p} in Settings › Projects."));
        }
    }
    if !matches!(s.when, When::Event { event: Event::Changes, .. }) {
        return Err("An orchestrator runs when something it keeps watch over needs a look.".into());
    }
    if s.run.to != To::Thread {
        return Err("An orchestrator keeps one session going.".into());
    }
    // Your other machines would keep a copy, and one of a build from before orchestrators
    // couldn't read its event, nor the schedules beside it; and what it keeps watch over is read
    // from this machine's board alone.
    if s.runs_on.shared() {
        return Err("An orchestrator runs on this machine alone, for now.".into());
    }
    Ok(())
}

/// Why an orchestrator's session may not ask for a request (actions.rs, as the `codealong`
/// command it runs asks it), past what any agent may not: merging, reviewing or approving a PR,
/// and removing a worktree are never its to do, whoever told it to, since they reach other people
/// or can't be undone; messaging and starting sessions are only once you've let it (`send`,
/// `start`). Reading, tagging, suggesting, moving to the top and marking its own turns seen are
/// what it's for, and always allowed.
pub fn refused(request: &str, may: &Orchestrates) -> Option<String> {
    use codealong_peer::api;
    let words = may.words();
    let let_it = |what: &str, flag: &str| {
        format!(
            "The orchestrator for {words} may not {what}: its user can let it in its schedule (Schedules, {}), or with `codealong orchestrator start{} {flag}`.",
            name(may),
            may.project.as_deref().map(|p| format!(" {p}")).unwrap_or_default()
        )
    };
    match request {
        api::PR_MERGE => Some("An orchestrator never merges a PR: that's its user's to do.".into()),
        api::PR_REVIEW => Some("An orchestrator never reviews or approves a PR: that's its user's to do.".into()),
        api::WORKTREE_REMOVE => Some("An orchestrator never removes a worktree: a schedule of its user's can (`codealong schedule add`).".into()),
        api::SESSION_SEND | api::SESSION_STEER | api::SESSION_RESUME if !may.send => Some(let_it("message sessions", "--may-send")),
        api::START | api::SESSION_FORK | api::SESSION_EDIT if !may.start => Some(let_it("start sessions", "--may-start")),
        _ => None,
    }
}

/// Why the session `caller` may not ask for `request`, when it's an orchestrator's (`refused`), by
/// what its schedule as written lets it: actions.rs asks it of what the `codealong` command it
/// runs asks for, and the command itself of what it asks with no session named (a start, a fork,
/// removing a worktree). A schedule that doesn't read lets it only what it may without leave.
pub fn refused_to(board: &Board, caller: &str, request: &str) -> Option<String> {
    let theirs = board.session(caller).and_then(|(_, s)| s.orchestrator.clone())?;
    let may = load().ok().and_then(|f| f.schedules.into_iter().find(|s| s.id == theirs.schedule)).and_then(|s| s.orchestrates).unwrap_or_else(|| Orchestrates::of(theirs.project.as_deref()));
    refused(request, &may)
}

/// The project as Settings › Projects spells it, for one named in another case.
pub fn spelled(cfg: &Config, project: Option<&str>) -> Option<String> {
    let p = project?.trim();
    Some(cfg.projects.iter().find(|c| c.name.eq_ignore_ascii_case(p)).map_or_else(|| p.to_string(), |c| c.name.clone())).filter(|p| !p.is_empty())
}

/// Why another orchestrator can't be saved beside the ones there are: one per project, and one for
/// All projects.
pub fn clash(file: &File, s: &Schedule) -> Option<String> {
    let scope = s.orchestrates.as_ref()?;
    file.schedules
        .iter()
        .find(|o| o.id != s.id && o.orchestrates.as_ref().is_some_and(|x| x.same(scope)))
        .map(|o| format!("{} keeps watch over {} already.", o.name, scope.words()))
}

/// A session that an orchestrator's notes and runs aren't about: its own, another orchestrator's,
/// one that's archived or only visits, a headless run (what ran it reads it), and a helper that
/// doesn't need you (what it helps reads it).
fn watched(s: &Session) -> bool {
    s.orchestrator.is_none() && !s.archived && s.visitor.is_none() && !s.headless && !s.listening && !s.helping()
}

/// Whether an orchestrator looked too lately to run again now for what it found (`GAP_MS`): what
/// it found stays unseen, and its first run after the gap takes it with whatever else came. Two
/// agents in one linked worktree (`shared`) don't wait: they overwrite each other's work
/// meanwhile. Nor does a PR's move that's yours: noted only by its run, the states it went through
/// while it waited would be lost (approved, sent back and approved again, told as nothing new).
pub fn resting(runs: &[RunRecord], found: &[Found], now: i64) -> bool {
    !found.iter().any(|f| f.id.starts_with(SHARED) || f.id.starts_with(PR_MOVE)) && looked_at(runs.iter()).is_some_and(|at| now - at < GAP_MS)
}

/// What an orchestrator finds on the board to look at now, each by an id of its own that a run
/// notes as seen, so each is told once: a session of its own that waits on you (a call to allow, a
/// question, an error), a PR its sessions worked on, or of yours, that waits on you (approved, sent
/// back, or with failing checks) where no session of its own is told of it (nudges.rs `tells`), and a linked
/// worktree two agents work in at once. A finished turn, and a PR opened, merged or closed, aren't
/// found: Codealong shows them already, its next run reads them (`codealong changes`), and a run
/// for each was most of what an orchestrator ran for, to find nothing to tell you. `seen` is what
/// it was told of already, which a PR's moves count by.
pub fn found(board: &Board, file: &File, s: &Schedule, now: i64, seen: &[String]) -> Vec<Found> {
    let mut out = Vec::new();
    let keeps = |project: &str| keeps(file, s, project);
    // What a boundary group's session says isn't for an orchestrator that may run outside it
    // (boundary.rs): it gets the session's title and what it waits on, not its words.
    let cfg = crate::config::current().ok();
    let qualifies = |name: &str| cfg.as_ref().and_then(|c| c.accounts.iter().find(|a| a.name == name)).is_some_and(|a| crate::signin::current(a, now, crate::signin::Ask::Never).qualifies());
    // The account it would start on, and the ones its sessions run on now, which a move can
    // have taken elsewhere.
    let on: Vec<&str> = board.lanes.iter().flat_map(|l| &l.sessions).filter(|x| x.orchestrator.as_ref().is_some_and(|o| o.schedule == s.id)).map(|x| x.account.as_str()).collect();
    let reads = |account: &str| {
        cfg.as_ref().is_none_or(|c| {
            let reader = |a: &str| crate::boundary::Reader { account: a.to_string(), qualifies: qualifies(a) };
            crate::boundary::start_may_read(c, &s.run.account, account, qualifies) && on.iter().all(|a| crate::boundary::may_read(c, Some(&reader(a)), account))
        })
    };
    for lane in board.lanes.iter().filter(|l| l.device.is_none() && keeps(&l.project)) {
        for x in lane.sessions.iter().filter(|x| watched(x) && !x.held_back()) {
            let what = match x.status {
                Status::Needs => need_words(x),
                Status::Error => "stopped with an error".to_string(),
                _ => continue,
            };
            out.push(Found {
                id: format!("waits:{}:{}", x.key, x.status_since.unwrap_or(x.last_activity)),
                title: format!("“{}” {what}", x.title),
                text: if reads(&x.account) { x.excerpt.clone().unwrap_or_default() } else { String::new() },
                ..Found::default()
            });
        }
        if let Some(found) = shared(lane, now) {
            out.push(found);
        }
    }
    for p in board.prs.iter().filter(|p| keeps(&p.project) && (p.mine || !p.sessions.is_empty())) {
        use crate::model::PrState;
        use crate::nudges::{tells, Why};
        // What of it waits on you, and what its nudge would be telling its session.
        let (state, said, yours) = match p.state {
            PrState::Draft => ("draft", "opened as a draft", None),
            PrState::Open => ("open", "open for review", None),
            PrState::Approved => ("approved", "approved", Some(Why { approved: true, ..Why::default() })),
            PrState::Changes => ("changes", "sent back with changes asked for", Some(Why { changes: true, ..Why::default() })),
            PrState::Merged => ("merged", "merged", None),
            PrState::Closed => ("closed", "closed without merging", None),
        };
        // Each move is noted once, against the state last noted, so one it comes back to
        // (approved again after a push took its approval away, sent back again) is news again,
        // and a push while it's in one isn't. By when it was first seen there, which no id kept
        // from before can be, however many the list keeps. Only a move to what waits on you, of
        // which no session of its own is told (nudges.rs `tells`), is a run; the rest are noted
        // without one (`Found::quiet`).
        let prefix = format!("{PR_MOVE}{}:", p.key);
        let last = seen.iter().filter_map(|x| x.strip_prefix(&prefix)?.split_once(':')).filter_map(|(at, was)| Some((at.parse::<i64>().ok()?, was))).max_by_key(|(at, _)| *at);
        if last.map(|(_, was)| was) != Some(state) {
            let quiet = !yours.is_some_and(|why| !tells(board, p, &why));
            out.push(Found { id: format!("{prefix}{now}:{state}"), title: format!("#{} {} in {}: {said}", p.number, p.title, p.repo_slug), url: p.url.clone(), quiet, ..Found::default() });
        }
        if p.state.is_open() && p.checks.failed > 0 {
            let failing = Why { failed: if p.checks.failing.is_empty() { vec![String::new()] } else { p.checks.failing.clone() }, ..Why::default() };
            out.push(Found {
                id: format!("checks:{}@{}", p.key, p.head_oid.as_deref().unwrap_or(&p.head)),
                title: format!("#{} {} in {}: checks failing", p.number, p.title, p.repo_slug),
                url: p.url.clone(),
                text: if p.checks.failing.is_empty() { String::new() } else { format!("Failing: {}", p.checks.failing.join(", ")) },
                quiet: tells(board, p, &failing),
                ..Found::default()
            });
        }
    }
    out
}

/// What a session that needs you waits on, in words.
fn need_words(s: &Session) -> String {
    use crate::model::Need;
    match s.need() {
        Some(Need::Approval) => "waits on you to allow something",
        Some(Need::Question) => "asks you a question",
        Some(Need::Reply) => "asks you something in its reply",
        Some(Need::Message) => "holds another session's message for you",
        Some(Need::Stuck) => "looks stuck",
        Some(Need::Input) => "waits for you to type in its terminal",
        Some(Need::Start) => "waits on a screen of its own before it starts",
        _ => "needs you",
    }
    .to_string()
}

/// A linked worktree two agents or more work in at once, by them: in a linked one it's nearly
/// always a mistake, as they overwrite each other's files, branch and builds. Not a main checkout,
/// which some repos work in that way, nor an app's chats, listed together for having no folder.
fn shared(lane: &Lane, now: i64) -> Option<Found> {
    if lane.chats || lane.origin == crate::model::Origin::Main {
        return None;
    }
    const ACTIVE_MS: i64 = 60 * 60_000;
    let here: Vec<&Session> = lane
        .sessions
        .iter()
        .filter(|s| watched(s) && !s.helps_an_agent() && (matches!(s.status, Status::Working | Status::Needs | Status::Error) || (s.live && now - s.last_activity < ACTIVE_MS)))
        .collect();
    if here.len() < 2 {
        return None;
    }
    let mut keys: Vec<&str> = here.iter().map(|s| s.key.as_str()).collect();
    keys.sort();
    let titles: Vec<String> = here.iter().map(|s| format!("“{}”", s.title)).collect();
    Some(Found { id: format!("{SHARED}{}:{}", lane.id, keys.join(",")), title: format!("{} work in {} at once", titles.join(" and "), lane.callsign), text: lane.path.clone(), ..Found::default() })
}

/// Each orchestrator's session by its key, as its runs have it (`Track::thread`), with when it last
/// looked: for the engine to mark its session before reading and composing it (engine.rs
/// `with_orchestrators`), so what Jev reads and the board works out from it go by it. `mark` puts
/// it on again once the schedules' items have it as their next runs find it.
pub fn sessions(file: &File, runs: &Runs) -> std::collections::HashMap<String, crate::model::Orchestrator> {
    let mut out = std::collections::HashMap::new();
    for s in &file.schedules {
        let (Some(scope), Some(t)) = (&s.orchestrates, runs.of.get(&s.id)) else { continue };
        let Some(key) = t.thread.clone() else { continue };
        let looked_at = looked_at(t.runs.iter());
        out.insert(key, crate::model::Orchestrator { schedule: s.id.clone(), project: scope.project.clone(), paused: s.paused, looked_at });
    }
    out
}

/// When an orchestrator last looked: its latest run that went to a session.
fn looked_at<'a>(runs: impl Iterator<Item = &'a RunRecord>) -> Option<i64> {
    runs.filter(|r| matches!(r.outcome, Outcome::Started | Outcome::Sent | Outcome::Resumed | Outcome::Queued)).map(|r| r.at).max()
}

/// Marks each orchestrator's session as its own (`Session::orchestrator`): the one it keeps going,
/// as its next run finds it, with when it last looked (its latest run that went to a session), which
/// every window and device then says without the schedules.
pub fn mark(lanes: &mut [Lane], items: &[Item]) {
    for i in items {
        let (Some(scope), Some(key)) = (&i.schedule.orchestrates, &i.thread) else { continue };
        let looked_at = looked_at(i.runs.iter());
        for s in lanes.iter_mut().flat_map(|l| l.sessions.iter_mut()).filter(|s| &s.key == key) {
            s.orchestrator = Some(crate::model::Orchestrator { schedule: i.schedule.id.clone(), project: scope.project.clone(), paused: i.schedule.paused, looked_at });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Harness, PrItem, PrState};
    use crate::schedules::{File, Item, Runs, Track};

    const NOW: i64 = 1_800_000_000_000;

    fn orch(id: &str, project: Option<&str>) -> Schedule {
        Schedule { id: id.into(), ..schedule(Orchestrates::of(project), "", None) }
    }

    fn lane(project: &str, path: &str, sessions: Vec<Session>) -> Lane {
        Lane { path: path.into(), callsign: path.trim_start_matches('/').into(), sessions, ..Lane::new(path, project, "web") }
    }

    fn session(id: &str, status: Status, since: i64) -> Session {
        Session { status, status_since: Some(since), last_activity: since, title: id.into(), ..Session::new(Harness::Claude, "claude", id) }
    }

    fn pr(project: &str, state: PrState) -> PrItem {
        PrItem { project: project.into(), mine: true, head_oid: Some("abc".into()), ..crate::schedules::tests::pr("acme/web#4", state, None) }
    }

    fn ids(found: Vec<Found>) -> Vec<String> {
        found.into_iter().map(|f| f.id).collect()
    }

    #[test]
    fn a_projects_own_orchestrator_keeps_its_sessions_and_all_projects_the_rest() {
        let mut file = File { schedules: vec![orch("all", None), orch("web", Some("Web"))], ..File::default() };
        assert_eq!(owner(&file, "web").map(|s| s.id.as_str()), Some("web"));
        assert_eq!(owner(&file, "Docs").map(|s| s.id.as_str()), Some("all"));
        assert_eq!(owner(&file, crate::board::ELSEWHERE).map(|s| s.id.as_str()), Some("all"));
        assert_eq!(told_elsewhere(&file, &file.schedules[0]), vec!["Web".to_string()]);
        // Paused, its project goes back to All projects'.
        file.schedules[1].paused = true;
        assert_eq!(owner(&file, "Web").map(|s| s.id.as_str()), Some("all"));
        assert!(told_elsewhere(&file, &file.schedules[0]).is_empty());
        file.schedules[0].paused = true;
        assert!(owner(&file, "Web").is_none());
    }

    #[test]
    fn one_orchestrator_for_each_scope() {
        let file = File { schedules: vec![orch("all", None), orch("web", Some("Web"))], ..File::default() };
        assert!(clash(&file, &orch("", Some("web"))).is_some());
        assert!(clash(&file, &orch("", None)).is_some());
        assert!(clash(&file, &orch("", Some("Docs"))).is_none());
        // Saved again as itself, it's no clash.
        assert!(clash(&file, &file.schedules[1]).is_none());
    }

    #[test]
    fn finds_what_waits_on_you_and_not_a_finished_turn() {
        let file = File { schedules: vec![orch("web", Some("Web"))], ..File::default() };
        let s = &file.schedules[0];
        let asks = session("asks", Status::Needs, NOW - 1000);
        let failed = session("failed", Status::Error, NOW - 2000);
        let ended = session("ended", Status::Unread, NOW - 60 * 60_000);
        let working = session("working", Status::Working, NOW - 1000);
        // A main checkout, where two agents at once aren't news.
        let board = Board { lanes: vec![Lane { origin: crate::model::Origin::Main, ..lane("Web", "/w", vec![asks, failed, ended, working]) }], ..Board::default() };
        assert_eq!(ids(found(&board, &file, s, NOW, &[])), vec![format!("waits:claude:claude:asks:{}", NOW - 1000), format!("waits:claude:claude:failed:{}", NOW - 2000)]);
    }

    #[test]
    fn it_rests_between_runs_unless_two_agents_share_a_worktree() {
        let ran = |at: i64, outcome: Outcome| RunRecord { at, outcome, ..RunRecord::default() };
        let waits = vec![Found { id: "waits:k:1".into(), ..Found::default() }];
        // Looked five minutes ago: what it found waits for the gap to pass.
        assert!(resting(&[ran(NOW - 5 * 60_000, Outcome::Sent)], &waits, NOW));
        assert!(resting(&[ran(NOW - 5 * 60_000, Outcome::Queued)], &waits, NOW), "a run waiting for its session's turn to end looked too");
        // Once it has, or when it never looked, or only skipped, it runs.
        assert!(!resting(&[ran(NOW - GAP_MS, Outcome::Sent)], &waits, NOW));
        assert!(!resting(&[], &waits, NOW));
        assert!(!resting(&[ran(NOW - 60_000, Outcome::Skipped)], &waits, NOW));
        // Two agents in one worktree don't wait, nor a PR's move that's yours.
        let shared = vec![Found { id: format!("{SHARED}/w:a,b"), ..Found::default() }];
        assert!(!resting(&[ran(NOW - 60_000, Outcome::Sent)], &shared, NOW));
        let approved = vec![Found { id: format!("{PR_MOVE}acme/web#4:{NOW}:approved"), ..Found::default() }];
        assert!(!resting(&[ran(NOW - 60_000, Outcome::Sent)], &approved, NOW));
    }

    #[test]
    fn leaves_out_what_isnt_its_to_watch() {
        let file = File { schedules: vec![orch("web", Some("Web")), orch("all", None)], ..File::default() };
        let mut own = session("own", Status::Needs, NOW);
        own.orchestrator = Some(crate::model::Orchestrator { schedule: "web".into(), project: Some("Web".into()), ..Default::default() });
        let mut snoozed = session("snoozed", Status::Needs, NOW);
        snoozed.snoozed_until = Some(NOW + 1000);
        let mut headless = session("headless", Status::Unread, 0);
        headless.headless = true;
        let mut finished = session("finished", Status::Unread, 0);
        finished.finished = Some(Default::default());
        let elsewhere = session("other", Status::Needs, NOW);
        let board = Board { lanes: vec![lane("Web", "/w", vec![own, snoozed, headless, finished]), lane("Docs", "/d", vec![elsewhere])], ..Board::default() };
        assert!(found(&board, &file, &file.schedules[0], NOW, &[]).is_empty());
        // Docs has no orchestrator of its own: All projects' keeps it.
        assert_eq!(found(&board, &file, &file.schedules[1], NOW, &[]).len(), 1);
    }

    /// What a run is for, and what's only noted, by their ids.
    fn runs_and_notes(found: Vec<Found>) -> (Vec<String>, Vec<String>) {
        let (quiet, runs): (Vec<Found>, Vec<Found>) = found.into_iter().partition(|f| f.quiet);
        (ids(runs), ids(quiet))
    }

    #[test]
    fn a_prs_moves_are_noted_once_and_only_what_waits_on_you_is_a_run() {
        let file = File { schedules: vec![orch("web", Some("Web"))], ..File::default() };
        let s = &file.schedules[0];
        let at = |state: PrState| Board { prs: vec![pr("Web", state)], ..Board::default() };
        let told = |state: PrState, seen: &[String], now: i64| runs_and_notes(found(&at(state), &file, s, now, seen));
        let none: Vec<String> = Vec::new();
        // Opened, a draft, merged or closed: only noted, as Codealong shows it and the next run
        // reads it.
        assert_eq!(told(PrState::Open, &[], 10), (none.clone(), vec!["pr:acme/web#4:10:open".to_string()]));
        assert_eq!(told(PrState::Merged, &[], 10), (none.clone(), vec!["pr:acme/web#4:10:merged".to_string()]));
        assert_eq!(told(PrState::Approved, &[], 10), (vec!["pr:acme/web#4:10:approved".to_string()], none.clone()));
        // Noted already, it isn't again, a push while it's approved too.
        let mut seen = vec!["pr:acme/web#4:10:approved".to_string()];
        assert_eq!(told(PrState::Approved, &seen, 20), (none.clone(), none.clone()));
        let mut pushed = at(PrState::Approved);
        pushed.prs[0].head_oid = Some("def".into());
        assert!(found(&pushed, &file, s, 20, &seen).is_empty());
        // A push takes the approval away (noted), and an approval comes again: a run.
        assert_eq!(told(PrState::Open, &seen, 20), (none.clone(), vec!["pr:acme/web#4:20:open".to_string()]));
        seen.push("pr:acme/web#4:20:open".into());
        assert_eq!(told(PrState::Approved, &seen, 30).0, vec!["pr:acme/web#4:30:approved"]);
        // Sent back, its review dismissed, and sent back again: each time a run.
        let back = vec!["pr:acme/web#4:10:changes".to_string(), "pr:acme/web#4:20:open".to_string()];
        assert_eq!(told(PrState::Changes, &back, 30).0, vec!["pr:acme/web#4:30:changes"]);
        // With its oldest moves trimmed from the list, a move is still told, by an id none kept has.
        let trimmed = vec!["pr:acme/web#4:20:closed".to_string(), "pr:acme/web#4:30:open".to_string()];
        assert_eq!(told(PrState::Closed, &trimmed, 40).1, vec!["pr:acme/web#4:40:closed"]);
        let mut failing = at(PrState::Open);
        failing.prs[0].checks.failed = 1;
        assert_eq!(runs_and_notes(found(&failing, &file, s, NOW, &["pr:acme/web#4:1:open".to_string()])), (vec!["checks:acme/web#4@abc".to_string()], none.clone()));
        // One whose running primary session its nudge tells of it is that session's to see to,
        // and only noted; what its nudge leaves out (an approval, failing checks, by
        // `nudge_about`) is still a run.
        let primary = Session { live: true, ..session("p", Status::Idle, NOW) };
        let nudged = |state: PrState, about: crate::config::NudgeAbout| {
            let mut b = Board { lanes: vec![lane("Web", "/w", vec![primary.clone()])], nudge_about: about, ..failing.clone() };
            (b.prs[0].state, b.prs[0].nudge, b.prs[0].primary) = (state, true, Some(primary.key.clone()));
            runs_and_notes(found(&b, &file, s, NOW, &[]))
        };
        assert_eq!(nudged(PrState::Approved, crate::config::NudgeAbout::Everything), (none.clone(), vec![format!("pr:acme/web#4:{NOW}:approved"), "checks:acme/web#4@abc".to_string()]));
        assert_eq!(nudged(PrState::Approved, crate::config::NudgeAbout::Feedback), (vec![format!("pr:acme/web#4:{NOW}:approved"), "checks:acme/web#4@abc".to_string()], none.clone()));
        assert_eq!(nudged(PrState::Changes, crate::config::NudgeAbout::Feedback), (vec!["checks:acme/web#4@abc".to_string()], vec![format!("pr:acme/web#4:{NOW}:changes")]));
        // One whose primary has stopped, and isn't resumed for its news, is yours too.
        let mut stopped = Board { nudge_about: crate::config::NudgeAbout::Everything, ..failing.clone() };
        (stopped.prs[0].nudge, stopped.prs[0].primary) = (true, Some("claude:claude:gone".into()));
        assert_eq!(runs_and_notes(found(&stopped, &file, s, NOW, &["pr:acme/web#4:1:open".to_string()])).0, vec!["checks:acme/web#4@abc"]);
        // Another project's PR, or one neither yours nor worked on here, isn't its.
        assert!(found(&Board { prs: vec![pr("Docs", PrState::Approved)], ..Board::default() }, &file, s, NOW, &[]).is_empty());
        let mut theirs = at(PrState::Approved);
        theirs.prs[0].mine = false;
        assert!(found(&theirs, &file, s, NOW, &[]).is_empty());
    }

    #[test]
    fn two_agents_in_a_linked_worktree_are_found_once_for_who_they_are() {
        let file = File { schedules: vec![orch("web", Some("Web"))], ..File::default() };
        let s = &file.schedules[0];
        let two = vec![session("a", Status::Working, NOW), session("b", Status::Working, NOW)];
        let linked = Board { lanes: vec![Lane { origin: crate::model::Origin::Script, ..lane("Web", "/w", two.clone()) }], ..Board::default() };
        assert_eq!(ids(found(&linked, &file, s, NOW, &[])), vec!["shared:/w:claude:claude:a,claude:claude:b"]);
        let main = Board { lanes: vec![Lane { origin: crate::model::Origin::Main, ..lane("Web", "/w", two) }], ..Board::default() };
        assert!(found(&main, &file, s, NOW, &[]).is_empty());
    }

    #[test]
    fn its_session_is_known_from_its_runs_before_the_board_is() {
        let file = File { schedules: vec![orch("web", Some("Web")), orch("plain", None)], ..File::default() };
        let ran = |at: i64, outcome: Outcome| RunRecord { at, outcome, ..RunRecord::default() };
        let runs = Runs { of: [("web".to_string(), Track { thread: Some("claude:claude:t".into()), runs: vec![ran(10, Outcome::Started), ran(20, Outcome::Skipped)], ..Track::default() })].into(), ..Runs::default() };
        let by = sessions(&file, &runs);
        assert_eq!(by.len(), 1, "one with no session yet has none");
        let o = &by["claude:claude:t"];
        assert_eq!((o.schedule.as_str(), o.project.as_deref(), o.looked_at), ("web", Some("Web"), Some(10)));
    }

    #[test]
    fn its_session_is_marked_as_its_own() {
        let s = orch("web", Some("Web"));
        let ran = |at: i64, outcome: Outcome| RunRecord { at, outcome, ..RunRecord::default() };
        let item = Item { schedule: s, thread: Some("claude:claude:t".into()), runs: vec![ran(30, Outcome::Skipped), ran(20, Outcome::Sent), ran(10, Outcome::Started)], ..Item::default() };
        let mut lanes = vec![lane("Web", "/w", vec![session("t", Status::Idle, 0), session("u", Status::Idle, 0)])];
        mark(&mut lanes, &[item]);
        assert_eq!(lanes[0].sessions[0].orchestrator.as_ref().map(|o| o.schedule.as_str()), Some("web"));
        // When it last looked: a run that went to its session, not one skipped since.
        assert_eq!(lanes[0].sessions[0].orchestrator.as_ref().and_then(|o| o.looked_at), Some(20));
        assert!(lanes[0].sessions[1].orchestrator.is_none());
    }

    #[test]
    fn an_orchestrator_never_merges_and_messages_or_starts_only_when_let() {
        use codealong_peer::api;
        let mut may = Orchestrates::of(Some("Web"));
        assert!(refused(api::PR_MERGE, &may).is_some());
        assert!(refused(api::PR_REVIEW, &may).is_some());
        assert!(refused(api::WORKTREE_REMOVE, &may).is_some());
        assert!(refused(api::SESSION_SEND, &may).is_some_and(|w| w.contains("--may-send")));
        assert!(refused(api::START, &may).is_some_and(|w| w.contains("--may-start")));
        assert!(refused(api::SESSION_SUGGEST, &may).is_none());
        assert!(refused(api::SESSION_TAG, &may).is_none());
        (may.send, may.start) = (true, true);
        assert!(refused(api::SESSION_SEND, &may).is_none());
        assert!(refused(api::START, &may).is_none());
        assert!(refused(api::PR_MERGE, &may).is_some(), "letting it send lets it merge nothing");
    }

    #[test]
    fn an_orchestrator_is_checked_for_its_project_and_its_shape() {
        let cfg = Config { projects: vec![crate::config::ProjectCfg { name: "Web".into(), repos: Vec::new(), folders: Vec::new(), chrome_profile: None, default: None }], ..Config::default() };
        assert!(check(&orch("web", Some("web")), &cfg).is_ok());
        assert!(check(&orch("docs", Some("Docs")), &cfg).is_err());
        let mut new_each = orch("all", None);
        new_each.run.to = To::New;
        assert!(check(&new_each, &cfg).is_err());
        let mut shared = orch("all", None);
        shared.runs_on = RunsOn::One { prefer: String::new() };
        assert!(check(&shared, &cfg).is_err());
    }
}

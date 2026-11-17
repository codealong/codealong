//! Orchestrators: a schedule that keeps watch over your sessions for you (`Schedule::orchestrates`),
//! one for a project, and one for All projects that takes every project without one of its own and
//! the sessions in none (Elsewhere). It keeps one session going (`To::Thread`), started afresh each
//! day with its notes carried over, quiet unless it asks you something, and it runs only when
//! something it keeps watch over needs a look (`Event::Changes`): a session newly waiting on you, a
//! PR's state moving, two agents in one worktree. Nothing runs while nothing changes, so an
//! orchestrator costs no usage in a quiet hour, and one per project costs no more than what happens
//! in them.
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
/// A finished turn waits this long before its orchestrator is told: it's often followed at once by
/// another (a background command's report, your reply), and one run for both is enough. What
/// waits on you to allow or answer is told at once.
pub const SETTLE_MS: i64 = 2 * 60_000;
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

/// What an orchestrator finds on the board to look at now, each by an id of its own that a run
/// notes as seen, so each is told once: a session of its own that waits on you (a call to allow, a
/// question, an error, at once; a finished turn once it has settled, unless Codealong knows its
/// work is done, which is only yours to read), a PR its sessions worked on, or of yours, opened,
/// out of draft, approved, sent back, merged, closed or with failing checks, and a linked worktree
/// two agents work in at once. `seen` is what it was told of already, which a PR's moves count by.
pub fn found(board: &Board, file: &File, s: &Schedule, now: i64, seen: &[String]) -> Vec<Found> {
    let mut out = Vec::new();
    let keeps = |project: &str| keeps(file, s, project);
    for lane in board.lanes.iter().filter(|l| l.device.is_none() && keeps(&l.project)) {
        for x in lane.sessions.iter().filter(|x| watched(x) && !x.held_back()) {
            let since = x.status_since.unwrap_or(x.last_activity);
            let (what, settled) = match x.status {
                Status::Needs => (need_words(x), true),
                Status::Error => ("stopped with an error".to_string(), true),
                Status::Unread if x.finished.is_none() => ("ended its turn".to_string(), now - since >= SETTLE_MS),
                _ => continue,
            };
            if settled {
                out.push(Found {
                    id: format!("waits:{}:{since}", x.key),
                    title: format!("“{}” {what}", x.title),
                    url: String::new(),
                    text: x.excerpt.clone().unwrap_or_default(),
                    todo: None,
                });
            }
        }
        if let Some(found) = shared(lane, now) {
            out.push(found);
        }
    }
    for p in board.prs.iter().filter(|p| keeps(&p.project) && (p.mine || !p.sessions.is_empty())) {
        let state = match p.state {
            crate::model::PrState::Draft => "draft",
            crate::model::PrState::Open => "open",
            crate::model::PrState::Approved => "approved",
            crate::model::PrState::Changes => "changes",
            crate::model::PrState::Merged => "merged",
            crate::model::PrState::Closed => "closed",
        };
        let said = match p.state {
            crate::model::PrState::Draft => "opened as a draft",
            crate::model::PrState::Open => "open for review",
            crate::model::PrState::Approved => "approved",
            crate::model::PrState::Changes => "sent back with changes asked for",
            crate::model::PrState::Merged => "merged",
            crate::model::PrState::Closed => "closed without merging",
        };
        // Told once each time it moves to another state: by how many of its moves were told before,
        // so one it comes back to (reopened, approved again after changes were asked) is news
        // again, and the same state is told once however often it's looked at.
        let prefix = format!("pr:{}:", p.key);
        let told: Vec<&String> = seen.iter().filter(|x| x.starts_with(&prefix)).collect();
        if told.last().and_then(|x| x.rsplit(':').next()) != Some(state) {
            out.push(Found { id: format!("{prefix}{}:{state}", told.len()), title: format!("#{} {} in {}: {said}", p.number, p.title, p.repo_slug), url: p.url.clone(), text: String::new(), todo: None });
        }
        if p.state.is_open() && p.checks.failed > 0 {
            out.push(Found {
                id: format!("checks:{}@{}", p.key, p.head_oid.as_deref().unwrap_or(&p.head)),
                title: format!("#{} {} in {}: checks failing", p.number, p.title, p.repo_slug),
                url: p.url.clone(),
                text: if p.checks.failing.is_empty() { String::new() } else { format!("Failing: {}", p.checks.failing.join(", ")) },
                todo: None,
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
    Some(Found { id: format!("shared:{}:{}", lane.id, keys.join(",")), title: format!("{} work in {} at once", titles.join(" and "), lane.callsign), url: String::new(), text: lane.path.clone(), todo: None })
}

/// Marks each orchestrator's session as its own (`Session::orchestrator`): the one it keeps going,
/// as its next run finds it.
pub fn mark(lanes: &mut [Lane], items: &[Item]) {
    for i in items {
        let (Some(scope), Some(key)) = (&i.schedule.orchestrates, &i.thread) else { continue };
        for s in lanes.iter_mut().flat_map(|l| l.sessions.iter_mut()).filter(|s| &s.key == key) {
            s.orchestrator = Some(crate::model::Orchestrator { schedule: i.schedule.id.clone(), project: scope.project.clone(), paused: i.schedule.paused });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Harness, PrItem, PrState};
    use crate::schedules::{File, Item};

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
        PrItem { project: project.into(), mine: true, ..crate::schedules::tests::pr("acme/web#4", state, None) }
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
    fn finds_what_waits_on_you_at_once_and_a_finished_turn_once_it_settles() {
        let file = File { schedules: vec![orch("web", Some("Web"))], ..File::default() };
        let s = &file.schedules[0];
        let asks = session("asks", Status::Needs, NOW - 1000);
        let ended = session("ended", Status::Unread, NOW - 1000);
        let settled = session("settled", Status::Unread, NOW - SETTLE_MS - 1);
        let working = session("working", Status::Working, NOW - 1000);
        // A main checkout, where two agents at once aren't news.
        let board = Board { lanes: vec![Lane { origin: crate::model::Origin::Main, ..lane("Web", "/w", vec![asks, ended, settled, working]) }], ..Board::default() };
        assert_eq!(ids(found(&board, &file, s, NOW, &[])), vec![format!("waits:claude:claude:asks:{}", NOW - 1000), format!("waits:claude:claude:settled:{}", NOW - SETTLE_MS - 1)]);
    }

    #[test]
    fn leaves_out_what_isnt_its_to_watch() {
        let file = File { schedules: vec![orch("web", Some("Web")), orch("all", None)], ..File::default() };
        let mut own = session("own", Status::Needs, NOW);
        own.orchestrator = Some(crate::model::Orchestrator { schedule: "web".into(), project: Some("Web".into()), paused: false });
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

    #[test]
    fn a_prs_each_state_is_told_once_and_failing_checks_at_each_commit() {
        let file = File { schedules: vec![orch("web", Some("Web"))], ..File::default() };
        let s = &file.schedules[0];
        let at = |state: PrState| Board { prs: vec![pr("Web", state)], ..Board::default() };
        let told = |state: PrState, seen: &[String]| ids(found(&at(state), &file, s, NOW, seen));
        assert_eq!(told(PrState::Open, &[]), vec!["pr:acme/web#4:0:open"]);
        // Told already, it isn't again; closed, then reopened, each is.
        let mut seen = vec!["pr:acme/web#4:0:open".to_string()];
        assert!(told(PrState::Open, &seen).is_empty());
        assert_eq!(told(PrState::Closed, &seen), vec!["pr:acme/web#4:1:closed"]);
        seen.push("pr:acme/web#4:1:closed".into());
        assert_eq!(told(PrState::Open, &seen), vec!["pr:acme/web#4:2:open"]);
        // Approved, sent back and approved again on one commit: three moves, each told.
        let mut seen = vec!["pr:acme/web#4:0:approved".to_string(), "pr:acme/web#4:1:changes".to_string()];
        assert_eq!(told(PrState::Approved, &seen), vec!["pr:acme/web#4:2:approved"]);
        seen.push("pr:acme/web#4:2:approved".into());
        assert!(told(PrState::Approved, &seen).is_empty());
        let mut failing = at(PrState::Open);
        failing.prs[0].checks.failed = 1;
        failing.prs[0].head_oid = Some("abc".into());
        assert_eq!(ids(found(&failing, &file, s, NOW, &[])), vec!["pr:acme/web#4:0:open", "checks:acme/web#4@abc"]);
        // Another project's PR, or one neither yours nor worked on here, isn't its.
        assert!(found(&Board { prs: vec![pr("Docs", PrState::Open)], ..Board::default() }, &file, s, NOW, &[]).is_empty());
        let mut theirs = at(PrState::Open);
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
    fn its_session_is_marked_as_its_own() {
        let s = orch("web", Some("Web"));
        let item = Item { schedule: s, thread: Some("claude:claude:t".into()), ..Item::default() };
        let mut lanes = vec![lane("Web", "/w", vec![session("t", Status::Idle, 0), session("u", Status::Idle, 0)])];
        mark(&mut lanes, &[item]);
        assert_eq!(lanes[0].sessions[0].orchestrator.as_ref().map(|o| o.schedule.as_str()), Some("web"));
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
        let cfg = Config { projects: vec![crate::config::ProjectCfg { name: "Web".into(), repos: Vec::new(), folders: Vec::new(), chrome_profile: None }], ..Config::default() };
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

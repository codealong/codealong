//! Where a session's work stands: finished, or the first thing that says it isn't
//! (`Standing`, `standing_of`), from the read of its last reply weighed against what's known of
//! its work besides (`Work`: its PRs, its todos, what its worktree holds). `settle_finished`
//! says it of every session once the board has all three, and lists the finished after the
//! rest.
//!
//! Pure functions over a board already composed. Every `Standing` has a word and a sentence
//! (`Standing::word`, `said`), and `STANDINGS` lists the words for the tests that hold each to
//! one: a new standing goes in all three. A rule's test goes in `tests` below.

use super::*;

/* ---- Finished ---- */

/// A read that a reply may ask you something, from this sure, keeps its session open and
/// unfinished: well under `ASKS_YOU`, which makes it Needs you.
pub const MAY_ASK: f64 = 0.3;
/// A read of whether what you asked for is done says it isn't under this, whatever else is known.
pub const NOT_FINISHED: f64 = 0.5;

/// Whether a session's last reply may ask you something: by Jev's read of it (or an agent's tag)
/// when there's one, else by its having a question in it, Markdown's or one with more after it.
pub fn reply_asks(s: &Session) -> bool {
    match s.insight.as_ref().and_then(|i| i.asks_you) {
        Some(p) => p >= MAY_ASK,
        None => s.reply_tail.as_deref().is_some_and(has_question),
    }
}

/// What's known of a session's work besides the read of its last reply (`settle_finished`
/// gathers it): what `finished_of` weighs that read against.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Work {
    /// The PRs it worked on, as the PRs view lists it under them (`pr_items`): each one's
    /// number and state.
    pub prs: Vec<(u64, PrState)>,
    /// The todos linked to it that aren't archived: whether each is done.
    pub todos: Vec<bool>,
    /// Its worktree holds what would go with it (uncommitted files, commits on no remote), where
    /// the worktree is its alone: no main checkout or plain folder, which hold others' work too,
    /// and no other session of the work the worktree holds now.
    pub unsaved: bool,
    /// How far its change got by the first line of its last reply, where that says
    /// (`insights::landing_said`): the session's own word, read whether or not Jev reads
    /// sessions, which is the only time `SessionInsight::landing` has it.
    pub said: Option<&'static str>,
}

/// Where a session's work stands: finished, or the first thing that says it isn't
/// (`standing_of`). `finished_of` is its `Finished`, and a session marked done is noted with the
/// rest (dismissed.rs), which is how you're told which ones you left unfinished.
#[derive(Debug, Clone, PartialEq)]
pub enum Standing {
    /// Not yours to weigh: a helper's work or a headless run's is its starter's, and an archived
    /// session is behind you already.
    NotYours,
    /// Its turn is running.
    Working,
    /// It waits on you to allow a call or to answer a question it put to you.
    Needs,
    /// It stopped on an error.
    Error,
    /// Its last reply asks you something.
    Asks,
    /// Its harness cut its turn short: a usage limit, a lost connection, a hangup.
    CutShort,
    /// Something of its is still to come: a command or agents left running, a message queued, a
    /// snooze.
    ToCome,
    /// Its reply hands you a next step or waits on what it started.
    LeavesStep,
    /// Its change hasn't landed: blocked, uncommitted, or committed and not pushed.
    NotLanded,
    /// The read of its reply says what you asked for isn't done.
    NotDone,
    /// A PR it worked on is still open.
    OpenPr,
    /// A todo linked to it isn't done.
    OpenTodo,
    /// Its own worktree holds uncommitted files or commits on no remote.
    Unsaved,
    /// Nothing says either way: no more finished than not.
    Unknown,
    /// Finished, for sure or by the read of its reply (`Finished::sure`).
    Finished(Finished),
}

/// Every standing's word, for the tests that hold each to a sentence.
#[cfg(test)]
pub const STANDINGS: [&str; 16] = ["not-yours", "working", "needs", "error", "asks", "cut-short", "to-come", "step", "not-landed", "not-done", "open-pr", "open-todo", "unsaved", "unknown", "looks-finished", "finished"];

impl Standing {
    /// Its word, as the ledger keeps it; a finished one's is `finished` when something besides
    /// its reply says so, else `looks-finished`.
    pub fn word(&self) -> &'static str {
        match self {
            Standing::NotYours => "not-yours",
            Standing::Working => "working",
            Standing::Needs => "needs",
            Standing::Error => "error",
            Standing::Asks => "asks",
            Standing::CutShort => "cut-short",
            Standing::ToCome => "to-come",
            Standing::LeavesStep => "step",
            Standing::NotLanded => "not-landed",
            Standing::NotDone => "not-done",
            Standing::OpenPr => "open-pr",
            Standing::OpenTodo => "open-todo",
            Standing::Unsaved => "unsaved",
            Standing::Unknown => "unknown",
            Standing::Finished(f) if f.sure => "finished",
            Standing::Finished(_) => "looks-finished",
        }
    }

    /// How a session marked done stood, as a sentence says it: "It was still working".
    pub fn said(word: &str) -> &'static str {
        match word {
            "working" => "It was still working",
            "needs" => "It was waiting on you to allow a call or answer it",
            "error" => "It had stopped on an error",
            "asks" => "Its last reply asked you something",
            "cut-short" => "Its turn had been cut short",
            "to-come" => "It had work still to come",
            "step" => "Its reply left you a next step",
            "not-landed" => "Its change hadn't landed",
            "not-done" => "Its reply read as not done",
            "open-pr" => "A PR it worked on was still open",
            "open-todo" => "A todo linked to it wasn't done",
            "unsaved" => "Its worktree held work that wasn't saved",
            "finished" => "Finished",
            "looks-finished" => "It looked finished",
            "not-yours" => "It worked for another session",
            _ => "Nothing said whether it was finished",
        }
    }

    /// Whether a standing's word is one of work left undone: not finished, and not merely
    /// unknown.
    pub fn unfinished(word: &str) -> bool {
        !matches!(word, "finished" | "looks-finished" | "unknown" | "not-yours")
    }
}

/// Where a session's work stands (`Standing`), for a session of yours.
///
/// It isn't finished while its turn runs or waits on you, when its reply may ask you something
/// (`reply_asks`), hands you a next step or waits on what it started (`SessionInsight::ending`:
/// only a report or an offer of more leaves nothing), says the work goes on at a later time
/// (`SessionInsight::waits_until`), says its change didn't land or that it couldn't finish (the
/// read's `landing`, else its own status line, `Work::said`), or reads as not done
/// (`NOT_FINISHED`); nor while a PR it worked on is open, a todo linked to it isn't done, or its
/// own worktree holds what would be lost.
///
/// Then it's finished for sure when something besides the reply says so: a PR it worked on
/// merged, a todo linked to it done, or its status line saying the change landed. With none of
/// those it looks finished when the read of its reply says what you asked for is done
/// (`FINISHED`), and otherwise nothing is known: a session nothing is known of is no more
/// finished than not.
pub fn standing_of(s: &Session, work: &Work) -> Standing {
    if s.archived || s.helps_an_agent() || s.headless {
        return Standing::NotYours;
    }
    if s.asks {
        return Standing::Asks;
    }
    match s.status {
        Status::Working => return Standing::Working,
        Status::Needs => return Standing::Needs,
        Status::Error => return Standing::Error,
        Status::Unread | Status::Idle | Status::Stopped => {}
    }
    if s.cut_short() {
        return Standing::CutShort;
    }
    if s.background.is_some() || s.running.is_some() || s.stalled_since.is_some() || s.queued_at.is_some() || !s.queued.is_empty() || s.snoozed_until.is_some() {
        return Standing::ToCome;
    }
    if reply_asks(s) {
        return Standing::Asks;
    }
    let read = s.insight.as_ref();
    if read.and_then(|i| i.ending.as_deref()).is_some_and(|e| !matches!(e, "report" | "offer")) {
        return Standing::LeavesStep;
    }
    // The read's landing has the status line under an agent's tag of it; with no read of it, the
    // line itself.
    let landing = read.and_then(|i| i.landing.as_deref()).or(work.said);
    // Work put off to a later time isn't done, however the reply reads: not till then, and not
    // once the time has come, when it's due.
    if read.is_some_and(|i| i.waits_until.is_some()) {
        return Standing::ToCome;
    }
    if landing.is_some_and(|l| matches!(l, "blocked" | "uncommitted" | "committed")) {
        return Standing::NotLanded;
    }
    let done = read.and_then(|i| i.finished);
    if done.is_some_and(|p| p < NOT_FINISHED) {
        return Standing::NotDone;
    }
    if work.prs.iter().any(|(_, state)| state.is_open()) {
        return Standing::OpenPr;
    }
    if work.todos.contains(&false) {
        return Standing::OpenTodo;
    }
    if work.unsaved {
        return Standing::Unsaved;
    }
    let mut why = Vec::new();
    let merged: Vec<u64> = work.prs.iter().filter(|(_, state)| *state == PrState::Merged).map(|(n, _)| *n).collect();
    match merged.as_slice() {
        [] => {}
        [n] => why.push(format!("#{n} merged")),
        [a, b] => why.push(format!("#{a} and #{b} merged")),
        many => why.push(format!("{} PRs merged", many.len())),
    }
    match work.todos.len() {
        0 => {}
        1 => why.push("its todo is done".to_string()),
        n => why.push(format!("its {n} todos are done")),
    }
    // A status line an agent's tag reads otherwise isn't its last word on the change.
    if work.said == Some("landed") && landing == Some("landed") {
        why.push("it says its change landed".to_string());
    }
    let sure = !why.is_empty();
    if done.is_some_and(|p| p >= FINISHED) {
        why.push(match read.and_then(|i| i.by.as_ref().map(|_| i.by_title.as_deref().unwrap_or("an agent"))) {
            Some(by) => format!("{by} read it as done"),
            None => "its reply reads as done".to_string(),
        });
    }
    if why.is_empty() {
        Standing::Unknown
    } else {
        Standing::Finished(Finished { sure, why })
    }
}

/// Whether a session's work is done with nothing left for you, and why: `standing_of`'s
/// `Finished`, for a session of yours whose turn ended as turns do, with nothing left running,
/// queued or put off.
pub fn finished_of(s: &Session, work: &Work) -> Option<Finished> {
    match standing_of(s, work) {
        Standing::Finished(f) => Some(f),
        _ => None,
    }
}

/// What's known of each session's work besides the read of its last reply, from the PRs that list
/// it, the todos linked to it and its worktree: by its key, for the sessions a worktree lists as
/// its own.
pub fn works(lanes: &[Lane], prs: &[PrItem], todos: &[crate::todos::Todo]) -> HashMap<String, Work> {
    let mut worked: HashMap<&str, Vec<(u64, PrState)>> = HashMap::new();
    for p in prs {
        // Not a PR its primary took over from a helper it started: that work was the helper's.
        let taker = |k: &str| p.taken_over && p.primary.as_deref() == Some(k);
        for s in p.sessions.iter().filter(|s| !taker(&s.key)) {
            worked.entry(s.key.as_str()).or_default().push((p.number, p.state));
        }
    }
    let mut linked: HashMap<&str, Vec<bool>> = HashMap::new();
    for t in todos.iter().filter(|t| t.archived_at.is_none()) {
        for key in &t.sessions {
            linked.entry(key.as_str()).or_default().push(t.status == crate::todos::TodoStatus::Done);
        }
    }
    let mut found = HashMap::new();
    for lane in lanes.iter() {
        let own = |s: &&Session| s.visitor.is_none() && !s.archived && !s.earlier && !s.unattended();
        let alone = !matches!(lane.origin, Origin::Main | Origin::Folder) && !lane.chats && lane.sessions.iter().filter(own).count() == 1;
        let holds = alone && lane.git.as_ref().is_some_and(|g| g.dirty > 0 || g.unpushed.unwrap_or(0) > 0);
        for s in lane.sessions.iter().filter(|s| s.visitor.is_none()) {
            found.insert(
                s.key.clone(),
                Work {
                    prs: worked.get(s.key.as_str()).cloned().unwrap_or_default(),
                    todos: linked.get(s.key.as_str()).cloned().unwrap_or_default(),
                    unsaved: holds && own(&s),
                    said: crate::insights::landing_said(s),
                },
            );
        }
    }
    found
}

/// Where the session `key` stands on a board (`standing_of`), as it's listed in its own worktree;
/// None for a session the board doesn't list.
pub fn standing_on(lanes: &[Lane], prs: &[PrItem], todos: &[crate::todos::Todo], key: &str) -> Option<Standing> {
    let s = lanes.iter().flat_map(|l| &l.sessions).find(|s| s.key == key && s.visitor.is_none())?;
    let works = works(lanes, prs, todos);
    Some(standing_of(s, works.get(key).unwrap_or(&Work::default())))
}

/// Says of each session on the board whether it's finished (`finished_of`), from the PRs that
/// list it, the todos linked to it and its worktree, once the board has all three; a session
/// that only visits a worktree is as finished there as where it's listed. Then puts each
/// worktree's own sessions back in their order, which has the finished turns after the others
/// (`sort_sessions`), and says the worktree's sentence again where a turn of its waits on you:
/// it's the first one's.
pub fn settle_finished(lanes: &mut [Lane], prs: &[PrItem], todos: &[crate::todos::Todo]) {
    let works = works(lanes, prs, todos);
    let mut found: HashMap<String, Finished> = HashMap::new();
    for s in lanes.iter().flat_map(|l| &l.sessions).filter(|s| s.visitor.is_none()) {
        if let Some(f) = works.get(&s.key).and_then(|work| finished_of(s, work)) {
            found.insert(s.key.clone(), f);
        }
    }
    for lane in lanes.iter_mut() {
        let mut moved = false;
        for s in &mut lane.sessions {
            let now = found.get(&s.key).cloned();
            moved |= now.is_some() != s.finished.is_some();
            s.finished = now;
        }
        if !moved {
            continue;
        }
        let current = lane.sessions.iter().take_while(|s| s.visitor.is_none() && !s.archived && !s.earlier && !s.unattended()).count();
        sort_sessions(&mut lane.sessions[..current]);
        if lane.bay == Bay::Unread {
            (lane.headline, lane.detail) = headline(lane.bay, &lane.sessions[..current], lane.pr.as_ref(), lane.git.as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::fixtures::{insight, pr, NOW};

    /// A session whose turn ended and that you haven't seen, with this read of its reply.
    fn ended(read: impl FnOnce(&mut SessionInsight)) -> Session {
        Session { status: Status::Unread, insight: Some(insight(read)), ..crate::fixtures::session("claude:claude:a") }
    }

    #[test]
    fn a_session_is_finished_by_what_is_known_of_its_work() {
        let merged = Work { prs: vec![(12, PrState::Merged)], ..Default::default() };
        let said = |s: &Session, work: &Work| finished_of(s, work).map(|f| (f.sure, f.why.join(", ")));
        // A merged PR says so by itself, with no read of the reply at all, and with one that
        // agrees: what's known comes first.
        let unread = Session { insight: None, ..ended(|_| {}) };
        assert_eq!(said(&unread, &merged), Some((true, "#12 merged".into())));
        assert_eq!(said(&ended(|i| i.finished = Some(0.9)), &merged), Some((true, "#12 merged, its reply reads as done".into())));
        // So does a todo linked to it that's done, and its own status line.
        let all = Work { prs: vec![(12, PrState::Merged), (13, PrState::Merged), (9, PrState::Closed)], todos: vec![true], said: Some("landed"), ..Default::default() };
        assert_eq!(said(&unread, &all), Some((true, "#12 and #13 merged, its todo is done, it says its change landed".into())));
        let many = Work { prs: (1..=3).map(|n| (n, PrState::Merged)).collect(), todos: vec![true, true], ..Default::default() };
        assert_eq!(said(&unread, &many), Some((true, "3 PRs merged, its 2 todos are done".into())));
        // A status line an agent's tag reads otherwise isn't the last word on the change.
        let line = Work { said: Some("landed"), ..Default::default() };
        assert_eq!(said(&ended(|i| i.landing = Some("landed".into())), &line), Some((true, "it says its change landed".into())));
        assert_eq!(said(&ended(|i| i.landing = Some("pushed".into())), &line), None);
        // The line is the session's own word with nothing reading sessions (`[jev] sessions` off,
        // no tag): one that says the change didn't land holds it, whatever merged; one that only
        // says it's pushed doesn't, its PR having merged since.
        assert_eq!(said(&unread, &line), Some((true, "it says its change landed".into())));
        for held in ["uncommitted", "committed"] {
            assert_eq!(said(&unread, &Work { said: Some(held), ..merged.clone() }), None, "{held}");
        }
        assert_eq!(said(&unread, &Work { said: Some("pushed"), ..merged.clone() }), Some((true, "#12 merged".into())));
        // Seen or stopped, it's as finished as unread: the lists of what to clear go by it too.
        for status in [Status::Idle, Status::Stopped] {
            assert!(finished_of(&Session { status, ..unread.clone() }, &merged).is_some(), "{status:?}");
        }
    }

    #[test]
    fn a_session_only_looks_finished_by_the_read_of_its_reply() {
        let nothing = Work::default();
        let said = |s: &Session| finished_of(s, &nothing).map(|f| (f.sure, f.why.join(", ")));
        assert_eq!(said(&ended(|i| i.finished = Some(FINISHED))), Some((false, "its reply reads as done".into())));
        let tagged = ended(|i| (i.finished, i.by, i.by_title) = (Some(1.0), Some("claude:claude:boss".into()), Some("Orchestrator".into())));
        assert_eq!(said(&tagged), Some((false, "Orchestrator read it as done".into())));
        // Nothing known of it either way is no reason to call it finished.
        assert_eq!(said(&ended(|i| i.finished = Some(0.7))), None);
        assert_eq!(said(&ended(|_| {})), None);
        assert_eq!(said(&Session { insight: None, ..ended(|_| {}) }), None);
    }

    #[test]
    fn a_session_with_anything_left_is_not_finished() {
        const MIN: i64 = 60_000;
        let merged = Work { prs: vec![(12, PrState::Merged)], todos: vec![true], said: Some("landed"), ..Default::default() };
        let done = || ended(|i| i.finished = Some(1.0));
        assert!(finished_of(&done(), &merged).is_some());
        let left = |what: &str, s: Session| assert_eq!(finished_of(&s, &merged), None, "{what}");
        // What its reply says: it may ask you something, hands you a step, waits on what it
        // started, didn't land its change, or isn't done.
        left("may ask", ended(|i| (i.finished, i.asks_you) = (Some(1.0), Some(MAY_ASK))));
        left("a question in a reply nothing read", Session { insight: None, reply_tail: Some("Merged. Shall I deploy it?".into()), ..done() });
        for ending in ["handoff", "waiting", "question", "blocked"] {
            left(ending, ended(|i| (i.finished, i.ending) = (Some(1.0), Some(ending.into()))));
        }
        for ending in ["report", "offer"] {
            assert!(finished_of(&ended(|i| (i.finished, i.ending) = (Some(1.0), Some(ending.into()))), &merged).is_some(), "{ending}");
        }
        for landing in ["blocked", "uncommitted", "committed"] {
            left(landing, ended(|i| (i.finished, i.landing) = (Some(1.0), Some(landing.into()))));
        }
        left("reads as not done", ended(|i| i.finished = Some(0.4)));
        left("its work goes on at a later time", ended(|i| (i.finished, i.waits_until) = (Some(1.0), Some(NOW + DAY_MS))));
        left("work it put off is due", ended(|i| (i.finished, i.waits_until) = (Some(1.0), Some(NOW - DAY_MS))));
        // What it is: working, waiting on you, archived, an agent's helper, a script's run.
        left("working", Session { status: Status::Working, ..done() });
        left("needs you", Session { status: Status::Needs, ..done() });
        left("asks in its reply", Session { status: Status::Needs, asks: true, ..done() });
        left("failed", Session { status: Status::Error, ..done() });
        left("archived", Session { archived: true, ..done() });
        left("a helper", Session { helper: Some(Helper::default()), ..done() });
        left("headless", Session { headless: true, ..done() });
        assert!(finished_of(&Session { helper: Some(Helper { yours: true, ..Default::default() }), ..done() }, &merged).is_some(), "a helper you talk to is yours");
        // What's still to come, and a turn its harness cut short.
        left("a command left running", Session { background: Some("npm run dev".into()), ..done() });
        left("a message waiting to go in", Session { queued_at: Some(NOW), ..done() });
        left("snoozed", Session { snoozed_until: Some(NOW + MIN), ..done() });
        left("stuck", Session { stalled_since: Some(NOW), ..done() });
        left("hit its limit", Session { limit: Some(SessionLimit::default()), ..done() });
        left("lost its connection", Session { connection_lost: Some(ConnectionLost::default()), ..done() });
        left("cut off", Session { cut_off: Some(NOW), ..done() });
        // What's left open of its work.
        let open = |work: Work| assert_eq!(finished_of(&done(), &work), None, "{work:?}");
        open(Work { prs: vec![(12, PrState::Merged), (13, PrState::Approved)], ..Default::default() });
        open(Work { prs: vec![(13, PrState::Draft)], ..Default::default() });
        open(Work { todos: vec![true, false], ..merged.clone() });
        open(Work { unsaved: true, ..merged.clone() });
    }

    #[test]
    fn the_board_says_which_sessions_are_finished_and_lists_them_after_the_rest() {
        const MIN: i64 = 60_000;
        let at = |id: &str, prompted: i64| Session { status: Status::Unread, live: false, last_prompt_at: NOW - prompted * MIN, last_activity: NOW - prompted * MIN, ..crate::fixtures::session(&Session::key_for(Harness::Claude, "claude", id)) };
        let read = |s: Session, f: f64| Session { insight: Some(insight(|i| i.finished = Some(f))), ..s };
        let (done, open, tied, kept) = (read(at("done", 1), 0.9), at("open", 5), read(at("tied", 2), 0.9), read(at("kept", 3), 0.9));
        let pr = |n: u64, state: PrState, of: &Session| PrItem {
            sessions: vec![PrSession { key: of.key.clone(), lane: "/r/w".into(), pushes: 0, primary: None }],
            ..pr_item(&[], &HashMap::new(), "o/r", ("P", "r", None), &PrRaw { number: n, ..pr(state, 0) }, None, None, &PrLinks::default())
        };
        let todo = |status: crate::todos::TodoStatus, archived: bool, of: &Session| -> crate::todos::Todo {
            serde_json::from_value(serde_json::json!({ "id": "t", "status": status, "sessions": [of.key], "archivedAt": archived.then_some(NOW) })).expect("a todo")
        };
        let own = |sessions: Vec<Session>, git: GitInfo| Lane { origin: Origin::Script, bay: Bay::Unread, git: Some(git), sessions, ..crate::fixtures::lane("/r/w", vec![]) };
        let mut lanes = vec![own(vec![done.clone(), tied.clone(), kept.clone(), open.clone()], GitInfo::default())];
        let prs = [pr(7, PrState::Open, &tied)];
        let todos = [todo(crate::todos::TodoStatus::InProgress, false, &kept), todo(crate::todos::TodoStatus::InProgress, true, &done)];
        settle_finished(&mut lanes, &prs, &todos);
        // By the name each was made with: a fixture's id is its whole key.
        let name = |s: &Session| s.key.rsplit(':').next().unwrap_or_default().to_string();
        let said: Vec<(String, Option<bool>)> = lanes[0].sessions.iter().map(|s| (name(s), s.finished.as_ref().map(|f| f.sure))).collect();
        let said: Vec<(&str, Option<bool>)> = said.iter().map(|(n, f)| (n.as_str(), *f)).collect();
        // The turns that still want something keep their order, the latest prompted first; the
        // finished one, prompted last of all, comes after them. An archived todo holds nothing.
        assert_eq!(said, [("tied", None), ("kept", None), ("open", None), ("done", Some(false))]);
        assert!(lanes[0].headline.contains("Title claude:claude:tied"), "the worktree's sentence is its first session's: {}", lanes[0].headline);
        // A worktree that's one session's alone holds its work: what's uncommitted there, or on no
        // remote, would go with it. Beside another session, or in a main checkout, whose it is
        // isn't known.
        let dirty = GitInfo { dirty: 2, ..Default::default() };
        let unpushed = GitInfo { unpushed: Some(1), ..Default::default() };
        for git in [dirty.clone(), unpushed] {
            let mut alone = vec![own(vec![done.clone()], git)];
            settle_finished(&mut alone, &[], &[]);
            assert_eq!(alone[0].sessions[0].finished, None);
        }
        let mut shared = vec![own(vec![done.clone(), open.clone()], dirty.clone())];
        settle_finished(&mut shared, &[], &[]);
        assert!(shared[0].sessions.iter().any(|s| name(s) == "done" && s.finished.is_some()));
        let mut main = vec![Lane { origin: Origin::Main, ..own(vec![done.clone()], dirty) }];
        settle_finished(&mut main, &[], &[]);
        assert!(main[0].sessions[0].finished.is_some());
        // Where it only visits, it's as finished as where it's listed; and what it was said to be
        // goes once it no longer is.
        let visitor = Session { visitor: Some("worked here".into()), finished: None, ..done.clone() };
        let stale = Session { finished: Some(Finished::default()), ..open.clone() };
        let mut both = vec![own(vec![done.clone(), stale], GitInfo::default()), Lane { id: "/r/v".into(), bay: Bay::Idle, ..own(vec![visitor], GitInfo::default()) }];
        settle_finished(&mut both, &[pr(8, PrState::Merged, &done)], &[]);
        assert_eq!(both[1].sessions[0].finished.as_ref().map(|f| f.why.join(", ")).as_deref(), Some("#8 merged, its reply reads as done"));
        assert_eq!(both[0].sessions.iter().map(|s| (name(s), s.finished.is_some())).collect::<Vec<_>>(), [("open".to_string(), false), ("done".to_string(), true)]);
        // Its status line is read with nothing reading sessions (`[jev] sessions` off, no tag):
        // one that says the change isn't committed holds a session whose PR merged, in a main
        // checkout too, where git says nothing of whose the changes are; one that says it landed
        // is why it's finished.
        let says = |line: &str| Session { insight: None, reply_head: Some(line.into()), ..done.clone() };
        let mut held = vec![Lane { origin: Origin::Main, ..own(vec![says("Not committed: edits in /r/w; the tests fail.")], GitInfo::default()) }];
        settle_finished(&mut held, &[pr(9, PrState::Merged, &done)], &[]);
        assert_eq!(held[0].sessions[0].finished, None);
        let mut landed = vec![own(vec![says("Landed: 1a2b3c4 is in origin/main.")], GitInfo::default())];
        settle_finished(&mut landed, &[], &[]);
        assert_eq!(landed[0].sessions[0].finished.as_ref().map(|f| (f.sure, f.why.join(", "))), Some((true, "it says its change landed".to_string())));
    }
}

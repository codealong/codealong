//! What Codealong lays over a session's status once its harness's builder has made it
//! (sessions.rs), before bays and headlines are worked out. Jev's read of its last reply
//! (`apply_insight`, with how sure Jev must be and where a reply's question is), and what you
//! and agents do to it from Codealong: quiet, the labels you gave it, a mark of seen from its
//! own agent, a mark of unread, a snooze, a move to the top, a side conversation's answer and
//! being set aside for one, the replies suggested for it. With them, the sessions nobody is at:
//! a helper and what it helps, a headless run and who started it, a listening session.
//!
//! Each is a pure function over a session already built. The order they're laid in is
//! `Engine::laid_over`'s (engine.rs), a stage at a time, where a later one has the last word:
//! Jev's read (`with_insights`), then `quiet_orchestrator` on its own, the suggested replies
//! (`with_suggestions`), and the rest as `with_snoozes` lists them. An overlay's doc says what
//! it comes after and before: a new one says so too, and goes into that order. A rule's test
//! goes in `tests` below.

use super::*;

/* ---- Jev's read (insights.rs) ---- */

/// How sure Jev has to be that a finished turn asks you something before it's Needs you.
pub const ASKS_YOU: f64 = 0.7;
/// A quiet command is stuck only when Jev gives it at least this chance of waiting for typing.
pub const WAITS_FOR_INPUT: f64 = 0.3;
/// A background command is stuck when Jev is this sure.
pub const BACKGROUND_STALLED: f64 = 0.7;
/// A background command is a server or watcher left running when Jev is this sure.
pub const BACKGROUND_LEFT_RUNNING: f64 = 0.7;
/// A session counts as finished for its worktree from this sure.
pub const FINISHED: f64 = 0.8;

/// The last sentence of a reply, for a row: where its question is.
pub fn last_sentence(text: &str) -> String {
    truncate(sentences(text).last().copied().unwrap_or(""), 140)
}

/// Where a reply's question is, for a row that says it asks you: its last sentence that ends in
/// a question mark, else its last sentence. A reply often goes on after its question, with what
/// it tested or the sources it read, and the last of that isn't what it asks.
pub fn question_in(text: &str) -> String {
    let asked = sentences(text).into_iter().filter(|s| is_question(s)).last();
    asked.map_or_else(|| last_sentence(text), |q| truncate(q, 140))
}

/// Whether a reply has a question in it, wherever it is and however it's set (`**Keep it?**`),
/// for where there's no read of what it asks to go by.
pub fn has_question(text: &str) -> bool {
    sentences(text).into_iter().any(is_question)
}

fn is_question(sentence: &str) -> bool {
    sentence.trim_end_matches(CLOSERS).ends_with('?')
}

/// What can close a sentence after its stop: Markdown's emphasis, quotes, a bracket.
const CLOSERS: [char; 7] = ['*', '_', '"', '\'', '”', '’', ')'];

/// A reply's sentences, in order.
fn sentences(text: &str) -> Vec<&str> {
    let t = text.trim().trim_start_matches('…').trim();
    // A sentence ends at a stop, and what closes it, followed by a space, so `App.tsx`, `e.g.`
    // and a link's `?q=` don't split one, and `?**` does.
    let chars: Vec<(usize, char)> = t.char_indices().collect();
    let mut starts = vec![0];
    for (i, &(_, c)) in chars.iter().enumerate() {
        if matches!(c, '.' | '!' | '?') {
            let after = chars[i + 1..].iter().find(|(_, c)| !CLOSERS.contains(c));
            if let Some(&(at, _)) = after.filter(|(_, c)| c.is_whitespace()) {
                starts.push(at);
            }
        }
    }
    let ends = starts.iter().skip(1).copied().chain([t.len()]);
    starts.iter().zip(ends).map(|(&a, b)| t[a..b].trim()).collect()
}

/// Applies quiet from Codealong to a session, after its status and Jev's read of it and before a
/// mark of unread or a snooze, before bays and headlines are worked out. A quiet session's turn
/// that ended is idle rather than Your turn, so it's in no queue, on no pill and off the mark:
/// one that loops, or wakes on a schedule, doesn't wait on you each time it stops. What needs you
/// still does: a reply Jev reads as asking you something (Needs you by then), a call to allow, a
/// question, an error. Your own Mark as unread, and a snooze's end, still bring it back, as they
/// come after.
pub fn apply_quiet(s: &mut Session, quiet: bool) {
    if !quiet {
        return;
    }
    s.quiet = true;
    if s.status == Status::Unread {
        s.status = Status::Idle;
    }
}

/// What an orchestrator's row says when its last run found nothing to tell you.
pub const NOTHING_NEW: &str = "Nothing new";

/// An orchestrator's session (`Session::orchestrator`, set before its read is applied) waits on
/// you only for what its harness holds it on: a call to allow, a question it asked with its
/// harness's question tool, an error. Its replies are its digests of what other sessions want of
/// you, which wait on you on their own, so no read of a reply makes it wait on you too
/// (`needs_your_answer`, from an agent's tag or one Jev made before it was one: insights.rs leaves
/// its turns unread). What a read says of its reply goes; what Jev says of a call it waits to have
/// allowed (its risk), or of what it left running, stays. A run that found nothing to tell you
/// ends with a dash (listen.rs `says_nothing`, as skills/codealong/references/orchestrate.md asks),
/// and says so rather than heading its row with the dash.
pub fn quiet_orchestrator(s: &mut Session) {
    if s.orchestrator.is_none() {
        return;
    }
    if s.asks {
        s.asks = false;
        if s.status == Status::Needs {
            s.status = Status::Idle;
        }
        s.excerpt = s.reply_head.clone().or_else(|| s.reply_tail.as_deref().map(last_sentence));
    }
    if matches!(s.status, Status::Idle | Status::Unread) && s.reply_tail.as_deref().is_some_and(crate::listen::says_nothing) {
        s.excerpt = Some(NOTHING_NEW.to_string());
    }
    if let Some(i) = s.insight.as_mut() {
        (i.asks_you, i.ending, i.landing, i.finished, i.waits_until) = (None, None, None, None, None);
    }
    s.settle_need();
}

/// Puts on a session the labels you gave it (state.rs `labels`), by their ids among the todos'
/// labels, in the order you gave them. One since deleted from the labels shows on none.
pub fn apply_labels(s: &mut Session, ids: &[String], labels: &[Label]) {
    s.labels = ids.iter().filter_map(|id| labels.iter().find(|l| &l.id == id)).cloned().collect();
}

/// Applies a mark of seen from a session's own agent (`codealong session seen me`), after quiet
/// and before what you do to it yourself: a turn whose agent said it needs nothing from you is
/// idle rather than Your turn, and says so (`Session::agent_seen`). Only a turn that ended as
/// Your turn: a reply Jev reads as asking you something is Needs you by then, and a call to
/// allow, a question or an error still wait on you, as does a turn its harness ended short
/// (`Session::cut_short`), which isn't the reply its agent spoke for. The agent marks the turn
/// it's taking, and what came after the mark is past what it spoke for
/// (`agent_seen_spent`). Your Mark as unread, a side conversation's answer and a snooze that ends
/// still bring it back, as they come after (`settle_agent_seen`).
pub fn apply_agent_seen(s: &mut Session, mark: Option<&AgentSeen>) {
    let Some(m) = mark else { return };
    if s.status != Status::Unread || s.cut_short() || agent_seen_spent(s, m.at) {
        return;
    }
    s.status = Status::Idle;
    s.agent_seen = Some(m.clone());
}

/// Whether a session has moved on from the turn its agent marked seen at `at` (`moved_on`):
/// something came into it since, a prompt or, where its harness says (`Session::input_at`),
/// another session's message, a background task's report or a scheduled wakeup, which the record
/// keeps after the turn they brought has ended, though Codealong wasn't running to see it; or a
/// turn of its started since, seen as it runs, where its harness says no more. Spent, the mark
/// is void, and the engine forgets it (`Engine::forget_spent_agent_seen`).
pub fn agent_seen_spent(s: &Session, at: i64) -> bool {
    moved_on(s, at, at).is_some()
}

/// Once the rest is applied: a turn its agent marked seen that something after brought back (a
/// mark of unread, a side conversation's answer, a snooze that ended, a move to the top) waits on
/// you as any does, and no longer says its agent saw it.
pub fn settle_agent_seen(s: &mut Session) {
    if s.status != Status::Idle {
        s.agent_seen = None;
    }
}

/// What a session helps (`Session::helper`): the session whose agent started it as a helper
/// (origins.json, with whether that session is told what it does), while the board has that
/// session, or the PR whose automatic review it is (reviews.json). A helper whose session is gone
/// is one of your own again, so nothing it does is left where nobody looks.
pub fn helper_of(parent: Option<(&str, bool)>, on_board: impl Fn(&str) -> bool, reviews: Option<&str>) -> Option<Helper> {
    let (parent, report) = match parent.filter(|(p, _)| on_board(p)) {
        Some((p, report)) => (Some(p.to_string()), report),
        None => (None, false),
    };
    let pr = reviews.map(String::from);
    (parent.is_some() || pr.is_some()).then_some(Helper { parent, pr, report, yours: false })
}

/// Sessions started in another session's own scratch folder (`Adapter::scratch_owner`: Claude
/// Code's scratchpad, a background job's folder) are that session's helpers, where nothing else
/// says whose they are: its agent ran them there (test chats in tmux, say) and reads them itself,
/// so their ended turns don't wait on you, as a helper's don't. None is told of, since none asked
/// to be (`Helper::report`). Only a session nothing says the origin of: a fork of one, or one
/// started without `--helper`, was made for you. Headless runs have their own rule, and a
/// session you made your own (`own`) stays yours. `owner_of` names a folder's session by its
/// harness and ID.
pub fn scratch_helpers(sessions: &mut [Session], owner_of: impl Fn(&Path) -> Option<(Harness, String)>, own: impl Fn(&str) -> bool) {
    let keys: HashMap<(Harness, String), String> = sessions.iter().map(|s| ((s.harness, s.id.clone()), s.key.clone())).collect();
    for s in sessions.iter_mut().filter(|s| s.helper.is_none() && s.origin.is_none() && !s.headless && !own(&s.key)) {
        let Some(parent) = owner_of(Path::new(&s.cwd)).and_then(|o| keys.get(&o)).filter(|k| **k != s.key) else { continue };
        s.helper = Some(Helper { parent: Some(parent.clone()), pr: None, report: false, yours: false });
        s.origin = Some(SessionOrigin { key: parent.clone(), how: OriginHow::Started });
    }
}

/// A session's helpers on the board (`Session::helper`): the sessions its agent started as
/// helpers, each once, from the lane it runs in, the archived ones left out.
pub fn helpers_of<'a>(lanes: &'a [Lane], key: &str) -> Vec<&'a Session> {
    lanes.iter().flat_map(|l| &l.sessions).filter(|s| s.visitor.is_none() && !s.archived && s.helper.as_ref().and_then(|h| h.parent.as_deref()) == Some(key)).collect()
}

/// Applies a helper's rule to it, where quiet is applied (`apply_quiet`): a turn it ends is idle
/// rather than Your turn, as its agent reads what it wrote, not you. One that needs you (a call
/// to allow, a question, an error) still does, and your own Mark as unread still brings it back.
/// One you're talking to (`Helper::yours`) wrote it for you, so its turn is yours.
pub fn apply_helper(s: &mut Session) {
    if s.helps_an_agent() && s.status == Status::Unread {
        s.status = Status::Idle;
    }
}

/// Applies an automatic review's rule while its next round is to come in it
/// (`autoreview::to_come`), with a helper's: a turn cut off partway, whose round made way for
/// commits that came in meanwhile, is idle rather than an error, as one you've seen is, so it
/// doesn't wait on you. The next round picks it up once the branch holds still, handed the
/// commits since, and Carry on would only finish a review of a commit the PR has moved on from.
/// It still says it was cut off (`Session::cut_off`); one you're talking to, a call to allow and
/// any other error still wait on you.
pub fn apply_round_to_come(s: &mut Session, to_come: bool) {
    if to_come && s.helps_an_agent() && s.status == Status::Error && s.cut_off.is_some() && !s.live {
        s.status = Status::Idle;
    }
}

/// Applies a listening session's rule to it (listen.rs), with a helper's: while it hears, a turn
/// it ends is idle rather than Your turn. It ends one every few seconds, each an answer to what
/// was just heard, which the windows show as it comes; none waits on you. Nor does one whose
/// answer reads as a question for you (`apply_insight`): what a coach says is often a question,
/// to put to the meeting, and it's shown where its answers are, not as something to answer. One
/// that needs you at a prompt (a call to allow) or stopped on an error still does, and your own
/// Mark as unread still brings it back. Once hearing has ended its turns are yours again, the one
/// that sums the meeting up first. That it's hearing is said on it (`Session::listening`), for
/// whatever shows it.
pub fn apply_listening(s: &mut Session, hearing: bool) {
    s.listening = hearing;
    if hearing && (s.status == Status::Unread || (s.status == Status::Needs && s.asks)) {
        s.status = Status::Idle;
        s.asks = false;
    }
}

/// Applies a headless run's rule to it (`Session::headless`), with a helper's: a turn it ends is
/// idle, an error too, as what ran it reads how it ended and nobody is at it to go on from
/// there. One waiting to have a call allowed still needs you, and your own Mark as unread still
/// brings it back.
pub fn apply_headless(s: &mut Session) {
    if s.headless && matches!(s.status, Status::Unread | Status::Error) {
        s.status = Status::Idle;
    }
}

/// The session whose agent started a headless run, by the run's process: the nearest of its
/// parents that's a session's own (`by_pid`), as a command an agent runs goes on under it (an
/// eval's `claude -p` under python, under the shell Claude Code's Bash tool opened, under that
/// session's claude). `parent` gives a process's parent, None once it's gone or isn't yours. A
/// run a script in a terminal of yours started climbs to launchd without meeting one.
pub fn started_by<'a>(pid: u32, parent: impl Fn(u32) -> Option<u32>, by_pid: &HashMap<u32, &'a str>) -> Option<&'a str> {
    let mut at = pid;
    // However deep a script nests, a climb this long is a loop in what `parent` said.
    for _ in 0..64 {
        at = parent(at).filter(|p| *p > 1)?;
        if let Some(key) = by_pid.get(&at) {
            return Some(key);
        }
    }
    None
}

/// The session that started a headless run: the nearest running session above its process
/// (`started_by`), else the first named in the environment the run started with (`under`, each a
/// harness and a session ID: `harness::started_under`) of the sessions on the board (`by_id`). A
/// script an agent starts on its own (`start_new_session`, `nohup … &`) is launchd's once its
/// launcher returns, so no session is above it, but it keeps what the agent's tool set: an
/// eval's driver did, and each `claude -p` its judge ran was listed as a session of its own. A
/// script you run in a terminal of yours carries no session's.
pub fn run_started_by<'a>(pid: u32, parent: impl Fn(u32) -> Option<u32>, under: impl Fn(u32) -> Vec<(Harness, String)>, by_pid: &HashMap<u32, &'a str>, by_id: &HashMap<(Harness, &str), &'a str>) -> Option<&'a str> {
    started_by(pid, parent, by_pid).or_else(|| under(pid).iter().find_map(|(harness, id)| by_id.get(&(*harness, id.as_str())).copied()))
}

/// What moved a session on from where it stood when an agent gave its word on it (a tag, replies
/// suggested), so that word no longer answers where it stands. The engine keeps it as it takes
/// the word away (`tags::Store::gone`, `suggestions::Store::gone`), for `codealong changes` to
/// say why a session that had one has none now.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase", tag = "by")]
pub enum MovedOn {
    /// It was sent a prompt after the agent read it (its `last_prompt_at`), or its user answered
    /// it from Codealong.
    Prompt { at: i64 },
    /// A turn of its started after the word was given, at this time. A message from another
    /// session or from Codealong, a background command's report or a scheduled wakeup can start
    /// one without moving its `last_prompt_at`.
    Turn { at: i64 },
}

impl MovedOn {
    pub fn at(&self) -> i64 {
        match self {
            MovedOn::Prompt { at } | MovedOn::Turn { at } => *at,
        }
    }
}

/// What moved the session on from an agent's word on it given at `at`, about the reply to the
/// prompt it sent at `prompt_at`: a prompt sent since, or a turn started since the word was
/// given. A turn that started after it has moved on from what it answered, even once it ends, so
/// the engine takes the word away once it sees one (`Engine::forget_stale_tags`,
/// `Engine::forget_stale_suggestions`). Busy with nothing said since a turn that ended before
/// the word isn't one (`Session::turn_ended`): Claude Code compacting a conversation left idle
/// took away the tags and replies of sessions still waiting on what they were for. Something
/// that came into it since (`Session::input_at`, where its harness says: a scheduled wakeup,
/// another session's message, a background task's report) started a turn too, one that may have
/// come and gone while Codealong wasn't running to see it. None while the word holds; a turn the
/// agent read it in started before it.
pub fn moved_on(s: &Session, prompt_at: i64, at: i64) -> Option<MovedOn> {
    if s.last_prompt_at > prompt_at {
        return Some(MovedOn::Prompt { at: s.last_prompt_at });
    }
    let said_nothing = s.turn_ended.is_some_and(|ended| ended <= at);
    s.status_since.filter(|since| s.status == Status::Working && *since > at && !said_nothing).or(s.input_at.filter(|came| *came > at)).map(|since| MovedOn::Turn { at: since })
}

/// Whether replies suggested for a session still answer where it stands (`moved_on`): it's been
/// sent nothing since the prompt whose reply the suggester read (`prompt_at`; a prompt of yours,
/// from Codealong or where it runs), and no turn of its has started since they were suggested.
/// Ones from Macs that didn't say what was read go by when they were suggested.
pub fn suggestions_hold(s: &Session, g: &Suggestions) -> bool {
    moved_on(s, g.prompt_at.unwrap_or(g.at), g.at).is_none()
}

/// Why replies for a session are turned down, given its last prompt as the suggester read it, or
/// none when they answer where it stands. Read before you replied, they'd answer the turn before
/// yours; suggested while it takes a turn, the reply it's writing isn't what was read.
pub fn suggestion_refused(s: &Session, read_prompt_at: i64) -> Option<String> {
    read_refused(s, read_prompt_at, "suggest")
}

/// Why an agent's word on a session is turned down, given the session's last prompt as the agent
/// read it: `verb` is what the agent does (suggest, tag), for the sentence. None when its word is
/// for where the session stands.
pub fn read_refused(s: &Session, read_prompt_at: i64, verb: &str) -> Option<String> {
    if s.last_prompt_at > read_prompt_at {
        return Some(format!("It's been sent a prompt since you read it (lastPromptAt {} now, {read_prompt_at} as you read it). Read it again (`codealong session transcript`) and {verb} for where it stands.", s.last_prompt_at));
    }
    if s.status == Status::Working && !s.waits_in_background() {
        let verb = format!("{}{}", verb[..1].to_uppercase(), &verb[1..]);
        return Some(format!("It's taking a turn now, so what you read isn't where it stands. {verb} once it waits on its user again."));
    }
    None
}

/// Puts the replies suggested for a session, or the choices picked for questions it asks, on it
/// while they hold (`suggestions_hold`): after its status and Jev's read of it, before a snooze or
/// a mark of unread moves when its status began.
pub fn apply_suggestions(s: &mut Session, suggested: Option<&Suggestions>) {
    s.suggestions = suggested.filter(|g| (!g.replies.is_empty() || !g.picks.is_empty()) && suggestions_hold(s, g)).cloned();
}

/// Applies a snooze from Codealong to a session, after its status and Jev's read of it, before
/// bays and headlines are worked out. Until its time the session carries `snoozed_until`, and the
/// floating bar and the mark leave it out of Needs you and Your turn. When the time comes it's
/// back as it is then, and one with nothing to show for itself (its turn seen, or its process
/// gone) is Your turn from then until you've seen it: a snooze always brings its session back. A
/// prompt you sent it after snoozing it ends the snooze without that: you took it up. So does a
/// turn it took by itself since that left it waiting on you (`snooze_called_back`), which the
/// engine then wakes for good.
pub fn apply_snooze(s: &mut Session, snooze: Option<&Snooze>, seen_at: i64, now: i64) {
    let Some(z) = snooze else { return };
    if s.last_prompt_at > z.at || s.archived || snooze_called_back(s, z.at) {
        return;
    }
    if now < z.until {
        s.snoozed_until = Some(z.until);
    } else if seen_at < z.until && matches!(s.status, Status::Idle | Status::Stopped) {
        s.status = Status::Unread;
        s.status_since = Some(z.until);
    }
}

/// Whether a turn a session took without you since it was snoozed at `at` (a scheduled wakeup or
/// check-in, another session's message, a background task's report: `Session::input_at`, where
/// its harness says) has left it waiting on you: its reply asks you something or it's blocked on
/// you, a call it wants to make or a question it put waits for your answer. What you put off has
/// come to you, so the snooze ends, as it does for a prompt of yours. A turn that only reports
/// keeps it snoozed until its time (asked 2026-11-18), as does a turn of its own begun before
/// the snooze, and a wait that began before what came in (a call it waits on, from before the
/// snooze or from the turn it was snoozed in, with a report come in since). It began waiting
/// when a command it runs stalled (`stalled_since`, its status having begun with the turn), else
/// when its status did, or where only its record is read, when its turn ended.
pub fn snooze_called_back(s: &Session, at: i64) -> bool {
    let waits = s.stalled_since.or(s.status_since).or(s.turn_ended);
    s.status == Status::Needs && s.input_at.is_some_and(|came| came > at && waits.is_some_and(|w| w >= came))
}

/// Applies a move to the top from Codealong to a session (`Session::raised`), after its snooze,
/// before bays and headlines are worked out: every queue goes to it first (`raised_order`). It
/// holds until you take it up: a prompt sent to it after it was moved puts it back in its place,
/// and so does its finished turn marked seen, which takes the move away for good
/// (`seen_takes_up`). One moved while its turn was over and seen (idle or stopped) waits on you
/// as Your turn from then, as a mark of unread does, since it was moved to the top of what waits;
/// one that's running keeps it, and is first once its turn ends. One moved back to its place
/// (`Raise::lowered`) and an archived one show nothing.
pub fn apply_raise(s: &mut Session, raise: Option<&Raise>) {
    let Some(r) = raise.filter(|r| r.lowered.is_none()) else { return };
    if s.archived || s.last_prompt_at > r.raised.at {
        return;
    }
    if matches!(s.status, Status::Idle | Status::Stopped) {
        s.status = Status::Unread;
        s.status_since = Some(r.raised.at);
    }
    s.raised = Some(r.raised.clone());
}

/// Whether marking a session seen takes it up, ending a move to the top: its turn is over (Your
/// turn, a reply that asks you something, a turn that was cut off, or one already seen), as the
/// windows' Mark as seen has it (lanes.ts `seenable`). Opening one that's running marks it seen
/// too, which mustn't take away a move meant for when its turn ends; nor does looking at a call to
/// allow, which waits on an answer.
pub fn seen_takes_up(s: &Session) -> bool {
    s.asks || (s.status == Status::Error && s.cut_off.is_some()) || matches!(s.status, Status::Unread | Status::Idle | Status::Stopped)
}

/// The order of sessions moved to the top (`Session::raised`), ahead of the rest: the ones you
/// moved, then the ones agents did, so an agent's never goes over yours, each the latest first.
/// Two that weren't moved are equal, and go by their status and wait.
pub fn raised_order(a: Option<&Raised>, b: Option<&Raised>) -> std::cmp::Ordering {
    let rank = |r: &Raised| (r.agent, Reverse(r.at));
    match (a, b) {
        (Some(a), Some(b)) => rank(a).cmp(&rank(b)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Why an agent may not move a session to the top, or back to its place (`raised` false), or none
/// when it may: you moved it to the top yourself, which is yours to undo and goes ahead of any
/// agent's; you snoozed it; or you moved it back to its place from where an agent had put it and
/// it hasn't moved on since (no prompt sent to it, and its status hasn't changed).
pub fn raise_refused(s: &Session, raise: Option<&Raise>, raised: bool) -> Option<String> {
    if s.raised.as_ref().is_some_and(|r| !r.agent) {
        return Some("Your user moved it to the top themselves, which is theirs to undo.".to_string());
    }
    if !raised {
        return None;
    }
    if s.snoozed_until.is_some() {
        return Some("Your user snoozed it. Leave it until it's back.".to_string());
    }
    if s.aside.is_some() {
        return Some("Your user set it aside until its side conversation answers. Leave it until then.".to_string());
    }
    let lowered = raise.and_then(|r| r.lowered)?;
    let moved_on = s.last_prompt_at > lowered || s.status_since.is_some_and(|t| t > lowered);
    (!moved_on).then(|| "Your user moved it back to its place after it was moved to the top. Leave it there until it moves on.".to_string())
}

/// Applies a mark of unread from Codealong to a session: one whose turn is over (idle or
/// stopped) is Your turn again, from when it was marked, until you see it or send it a prompt
/// after that, as a turn that just ended would be. One that's running or waits on you shows that
/// instead, and an archived one stays as it is.
pub fn apply_unread(s: &mut Session, mark: Option<&UnreadMark>, seen_at: i64) {
    let Some(u) = mark else { return };
    if s.archived || seen_at >= u.at || s.last_prompt_at > u.at {
        return;
    }
    if matches!(s.status, Status::Idle | Status::Stopped) {
        s.status = Status::Unread;
        s.status_since = Some(u.at);
    }
}

/// Applies what a session's side conversations answered to it, before a mark of unread and a
/// snooze: finishing what it was asked is a turn ending, so the latest one to answer since you saw
/// the session is `Session::side_answer`, and one whose turn is over (idle or stopped) is Your turn
/// from then, saying so, until you see it or send it a prompt after that. One that's running or
/// waits on you shows that instead and keeps the answer for when you come to it, and an archived
/// one stays as it is.
pub fn apply_side_answer(s: &mut Session, sides: &[crate::side::Glance], seen_at: i64) {
    let unseen = sides.iter().filter_map(|g| g.answered_at.filter(|at| *at > seen_at && *at >= s.last_prompt_at).map(|at| (at, g))).max_by_key(|(at, _)| *at);
    let Some((at, g)) = unseen.filter(|_| !s.archived) else { return };
    if matches!(s.status, Status::Idle | Status::Stopped) {
        s.status = Status::Unread;
        s.status_since = Some(at);
        s.excerpt = Some(format!("Answered on the side · {}", g.question));
    }
    s.side_answer = Some(SideAnswer { id: g.id.clone(), open: g.open, at });
}

/// Applies a side conversation you set a session aside for (state.rs `aside`), after what its
/// side conversations answered and before a mark of unread and a snooze. A session that needs you
/// (a call to allow, a question, a held message) set aside while its side conversation answers
/// waits on that first: it carries `Session::aside` and says what it asked on the side, and the
/// windows list it with what's working, out of every queue, off the pills and the mark. Once the
/// side conversation is done (answered, closed or gone) it needs you again as it did, where it
/// waited, with the answer kept for when you come to it (`apply_side_answer`). Only the need it
/// was set aside from, and only until something's sent it (`aside_holds`).
pub fn apply_aside(s: &mut Session, at: Option<i64>, sides: &[crate::side::Glance]) {
    let Some(at) = at.filter(|at| aside_holds(s, *at, sides)) else { return };
    s.aside = Some(at);
    if let Some(g) = sides.iter().find(|g| g.running && g.open) {
        s.excerpt = Some(format!("{ASKING_ON_THE_SIDE}{}", g.question));
    }
}

/// What a session set aside says, before what it asked on the side (`apply_aside`), which its
/// headline leaves below (`session_headline`).
pub(super) const ASKING_ON_THE_SIDE: &str = "Asking on the side · ";

/// Whether a session set aside at `at` still waits on its side conversation: it needs you as it
/// did then (none of its own since: no new need began, nothing was sent it), and its open side
/// conversation is still answering, none having finished since. One you closed lets it go,
/// though its copy answers on: you've done with it. Once it doesn't hold, the
/// engine forgets it (`Engine::forget_spent_aside`), so a side conversation asked later doesn't
/// set it aside again.
pub fn aside_holds(s: &Session, at: i64, sides: &[crate::side::Glance]) -> bool {
    s.status == Status::Needs
        && !s.archived
        && s.status_since.unwrap_or(s.last_activity) <= at
        && s.last_prompt_at <= at
        && sides.iter().any(|g| g.running && g.open)
        && !sides.iter().any(|g| g.answered_at.is_some_and(|t| t > at))
}

/// Applies Jev's read of a session to what the board shows, before bays and headlines are
/// worked out, so they follow it:
/// - a finished turn whose reply asks you something is Needs you, with the question;
/// - a busy session gone quiet on a command that isn't waiting for typing is working on it,
///   not stuck;
/// - a command left running in the background says whether it polls or runs a job, is left
///   running when it's a server or watcher and nothing else it started there runs beside it
///   (the turn is over; the engine makes that *Your turn* when you haven't seen it;
///   `waits_beside`), and is stuck when it looks stuck.
pub fn apply_insight(s: &mut Session, i: &SessionInsight) {
    s.insight = Some(i.clone());
    if s.status == Status::Unread && i.asks_you.unwrap_or(0.0) >= ASKS_YOU {
        needs_your_answer(s);
    }
    if let (Some(since), Some(p)) = (s.stalled_since, i.waits_for_input) {
        if p < WAITS_FOR_INPUT {
            s.status = Status::Working;
            s.stalled_since = None;
            s.status_since = Some(s.status_since.unwrap_or(since).min(since));
            s.excerpt = s.excerpt.as_deref().map(|e| e.replacen("Stuck on", "Running", 1));
        }
    }
    if let (Some(cmd), Some(kind)) = (s.background.clone(), i.background.as_deref()) {
        let shown = code(&truncate(&one_line(&cmd, 200), 80));
        match kind {
            // Once its processes have sat still for a while, what the command looks like is moot.
            "polling" | "job" if s.status != Status::Working => {}
            "polling" => {
                let waits = s.reply_tail.as_deref().map(last_sentence).filter(|q| !q.is_empty());
                s.excerpt = Some(format!("Polling in the background · {}", waits.unwrap_or(shown)));
            }
            "job" => s.excerpt = Some(format!("Running in the background · {shown}")),
            "server" if i.background_p.unwrap_or(0.0) >= BACKGROUND_LEFT_RUNNING => match waits_beside(s, &cmd) {
                // The server isn't all its turn left: it still waits on what else runs.
                Some(task) if s.status == Status::Working => {
                    let name = [&task.command, &task.about].into_iter().map(|t| t.trim()).find(|t| !t.is_empty());
                    s.excerpt = Some(match name {
                        Some(name) => format!("Waiting on {}", code(&truncate(&one_line(name, 200), 80))),
                        None => "Waiting on a command in the background".to_string(),
                    });
                }
                _ => {
                    if s.status == Status::Working {
                        s.status = Status::Idle;
                    }
                    s.excerpt = Some(format!("Left running · {shown}"));
                }
            },
            "stalled" if i.background_p.unwrap_or(0.0) >= BACKGROUND_STALLED => {
                s.status = Status::Needs;
                s.stalled_since = Some(s.status_since.unwrap_or(s.last_activity));
                s.excerpt = Some(format!("Stuck on {shown}"));
            }
            _ => {}
        }
    }
}

/// What a session whose turn left `judged` running in the background (`Session::background`, the
/// one command Jev reads) still waits on beside it, by its harness's list (`Session::tasks`): the
/// newest other command it started there, or a Monitor, whose every line wakes it whatever its
/// command looks like. So Jev's word that the command is a server says the turn is over only when
/// nothing else runs: a Monitor's `tail -f`, read as one, left a session that waited on the
/// relaunch it had started in Your turn (2026-11-14). The process table names a command as the
/// list does (`shells::eval_command` in the Claude adapter), so one the list names otherwise is
/// another, even where the list has lost the judged one (it keeps only so many, and reads only so
/// far back): that it still runs is surer than which of them Jev read.
fn waits_beside<'a>(s: &'a Session, judged: &str) -> Option<&'a BackgroundTask> {
    s.tasks.iter().rev().filter(|t| t.ended_at.is_none()).find(|t| t.monitor || t.command.trim() != judged.trim())
}

/// Makes a session whose turn ended on a reply that asks you something Needs you, with the
/// question.
fn needs_your_answer(s: &mut Session) {
    s.status = Status::Needs;
    s.asks = true;
    let question = s.reply_tail.as_deref().map(question_in).filter(|q| !q.is_empty());
    s.excerpt = Some(match question {
        Some(q) => format!("Asks you · {q}"),
        None => "Asks you something".to_string(),
    });
}

/// A session waiting on the agents its turn left running (`Session::agents`) whose reply asks you
/// something you haven't seen (its turn ended after `seen_at`) Needs you, as an unread turn's
/// does (`apply_insight`): it sits at its prompt, and its agents carry on whatever you answer.
/// After `apply_insight`, which doesn't know when you looked.
pub fn asks_while_agents_run(s: &mut Session, i: &SessionInsight, seen_at: i64) {
    if s.status == Status::Working && !s.agents.is_empty() && s.status_since.unwrap_or(0) > seen_at && i.asks_you.unwrap_or(0.0) >= ASKS_YOU {
        needs_your_answer(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::fixtures::{acct, insight, reg, seed, tr, NOW};

    #[test]
    fn an_orchestrators_reply_never_makes_it_wait_on_you_but_its_harness_still_does() {
        let o = Some(crate::model::Orchestrator { schedule: "orch".into(), ..Default::default() });
        // Its digest read as asking you: it's quiet again, with its reply's first line, and the
        // read of its reply goes.
        let read_as_asking = SessionInsight { asks_you: Some(0.9), ending: Some("question".into()), ..SessionInsight::default() };
        let mut read = Session { status: Status::Needs, asks: true, reply_head: Some("Two sessions wait on you.".into()), excerpt: Some("Asks you something".into()), insight: Some(read_as_asking), orchestrator: o.clone(), ..Session::new(Harness::Claude, "claude", "o") };
        quiet_orchestrator(&mut read);
        assert_eq!((read.status, read.asks, read.need), (Status::Idle, false, None));
        assert_eq!(read.excerpt.as_deref(), Some("Two sessions wait on you."));
        assert!(read.insight.as_ref().is_some_and(|i| i.asks_you.is_none() && i.ending.is_none()));
        // A call to allow, held by its harness, still waits on you, with Jev's read of its risk.
        let risky = SessionInsight { risk: Some(0.8), ..SessionInsight::default() };
        let mut held = Session { status: Status::Needs, excerpt: Some("Bash: rm -rf build".into()), insight: Some(risky), orchestrator: o.clone(), ..Session::new(Harness::Claude, "claude", "o") };
        quiet_orchestrator(&mut held);
        assert_eq!(held.status, Status::Needs);
        assert_eq!(held.need, Some(crate::model::Need::Approval));
        assert_eq!(held.insight.and_then(|i| i.risk), Some(0.8));
        // A run that found nothing new, its reply a dash, says so.
        let mut nothing = Session { status: Status::Idle, reply_tail: Some("—".into()), excerpt: Some("—".into()), orchestrator: o, ..Session::new(Harness::Claude, "claude", "o") };
        quiet_orchestrator(&mut nothing);
        assert_eq!(nothing.excerpt.as_deref(), Some(NOTHING_NEW));
        // Which its row says, as another idle session's says what it last said.
        assert_eq!(session_headline(&nothing).1.as_deref(), Some(NOTHING_NEW));
        // Any other session is left as it is.
        let mut other = Session { status: Status::Needs, asks: true, ..Session::new(Harness::Claude, "claude", "x") };
        quiet_orchestrator(&mut other);
        assert!(other.asks && other.status == Status::Needs);
    }

    #[test]
    fn a_listening_sessions_finished_turns_arent_yours_while_it_hears() {
        let of = |status: Status, hearing: bool| {
            let mut s = Session { status, ..crate::fixtures::session("k") };
            apply_listening(&mut s, hearing);
            s.status
        };
        assert_eq!(of(Status::Unread, true), Status::Idle);
        // An answer that reads as a question for you is a cue, shown as its answers are.
        let mut asks = Session { status: Status::Needs, asks: true, ..crate::fixtures::session("k") };
        apply_listening(&mut asks, true);
        assert_eq!((asks.status, asks.asks), (Status::Idle, false));
        // What needs you at a prompt still does, and a turn under way still shows as one.
        for status in [Status::Needs, Status::Error, Status::Working, Status::Idle] {
            assert_eq!(of(status, true), status);
        }
        // Once hearing has ended, its turns are yours again.
        assert_eq!(of(Status::Unread, false), Status::Unread);
        // That it's hearing is said on it, and no longer once it isn't; sent only when it is.
        let mut s = crate::fixtures::session("k");
        assert!(!serde_json::to_value(&s).unwrap().as_object().unwrap().contains_key("listening"));
        apply_listening(&mut s, true);
        assert_eq!(serde_json::to_value(&s).unwrap()["listening"], true);
        apply_listening(&mut s, false);
        assert!(!s.listening);
    }

    #[test]
    fn finds_the_last_sentence() {
        assert_eq!(last_sentence("…tests pass in App.tsx. Should I also migrate the old table?"), "Should I also migrate the old table?");
        assert_eq!(last_sentence("One line only"), "One line only");
        assert_eq!(last_sentence("Done! Next up: the docs."), "Next up: the docs.");
        assert_eq!(last_sentence(""), "");
    }

    /// A reply that asks often goes on after its question; the row shows the question.
    #[test]
    fn finds_the_question_a_reply_asks() {
        assert_eq!(question_in("…would go against that choice. Do you want me to make that change? To test, I trusted a folder. I removed it after."), "Do you want me to make that change?");
        assert_eq!(question_in("**Do you want these as main addresses, or is forwarding enough?** Sources: - [Google](https://support.google.com/mail/answer/22370?hl=en)"), "**Do you want these as main addresses, or is forwarding enough?**");
        assert_eq!(question_in("Should I merge it? Or should I wait for Wes?"), "Or should I wait for Wes?", "the last of several");
        assert_eq!(question_in("I'm still waiting on your `aws sso login` to rerun the plan."), "I'm still waiting on your `aws sso login` to rerun the plan.", "no question mark: the last sentence");
        assert_eq!(question_in("See claude.ai/settings/usage?from=cli for more. Then tell me."), "Then tell me.", "a question mark inside a link isn't a question");
    }

    #[test]
    fn tells_a_reply_with_a_question_in_it() {
        assert!(has_question("Deleted the branch. **Should I keep the worktree?**"), "set in Markdown");
        assert!(has_question("Do you want me to make that change? To test, I trusted a folder."), "with more after it");
        assert!(has_question("Pushed it. Merge it now?\n"));
        assert!(!has_question("Deleted the branch; this session closes now."));
        assert!(!has_question("See claude.ai/settings/usage?from=cli for more. Then tell me."), "a question mark inside a link isn't a question");
    }

    #[test]
    fn a_finished_turn_that_asks_you_something_needs_you() {
        let a = acct("claude2");
        let mut s = claude_session(&a, Some(&reg("idle", NOW - 10)), Some(&tr()), NOW - 100, NOW, &Quiet::default()).unwrap();
        s.reply_tail = Some("…all green. Want me to open the PR, or keep it local?".into());
        assert_eq!(s.status, Status::Unread);
        let mut unsure = s.clone();
        apply_insight(&mut unsure, &insight(|i| i.asks_you = Some(0.5)));
        assert_eq!(unsure.status, Status::Unread);
        assert!(unsure.insight.is_some() && !unsure.asks);
        apply_insight(&mut s, &insight(|i| i.asks_you = Some(0.9)));
        assert_eq!(s.status, Status::Needs);
        // It sits at its prompt, so a reply is its answer.
        assert!(s.asks);
        assert_eq!(s.excerpt.as_deref(), Some("Asks you · Want me to open the PR, or keep it local?"));
        // Seen already, it's yours to answer when you like: no change.
        let mut idle = claude_session(&a, Some(&reg("idle", NOW - 100)), Some(&tr()), NOW - 10, NOW, &Quiet::default()).unwrap();
        apply_insight(&mut idle, &insight(|i| i.asks_you = Some(0.9)));
        assert_eq!(idle.status, Status::Idle);
        assert!(!idle.asks);
        // Waiting on a permission prompt is never a question to type an answer to.
        let mut prompt = claude_session(&a, Some(&reg("waiting", NOW - 10)), Some(&tr()), NOW - 100, NOW, &Quiet::default()).unwrap();
        apply_insight(&mut prompt, &insight(|i| i.asks_you = Some(0.9)));
        assert_eq!((prompt.status, prompt.asks), (Status::Needs, false));
    }

    #[test]
    fn a_snooze_keeps_a_session_off_until_its_time_then_brings_it_back() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let z = Snooze { key: "k".into(), at: NOW - 10 * MIN, until: NOW + 50 * MIN };
        let unread = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        assert_eq!(unread.status, Status::Unread);
        assert!(unread.last_prompt_at < z.at);
        let snoozed = |s: &Session, seen: i64, now: i64| {
            let mut s = s.clone();
            apply_snooze(&mut s, Some(&z), seen, now);
            (s.status, s.snoozed_until, s.status_since)
        };
        // Until its time it keeps its status and says when it's back.
        assert_eq!(snoozed(&unread, NOW - 30 * MIN, NOW), (Status::Unread, Some(z.until), unread.status_since));
        // Then it's back as it is.
        assert_eq!(snoozed(&unread, NOW - 30 * MIN, z.until), (Status::Unread, None, unread.status_since));
        // One with nothing to show for itself is Your turn from its time until you've seen it.
        let idle = Session { status: Status::Idle, ..unread.clone() };
        assert_eq!(snoozed(&idle, NOW - 5 * MIN, z.until + MIN), (Status::Unread, None, Some(z.until)));
        let stopped = Session { status: Status::Stopped, live: false, ..unread.clone() };
        assert_eq!(snoozed(&stopped, NOW - 5 * MIN, z.until + MIN).0, Status::Unread);
        assert_eq!(snoozed(&idle, z.until + MIN, z.until + 2 * MIN).0, Status::Idle, "seen since");
        let working = Session { status: Status::Working, ..unread.clone() };
        assert_eq!(snoozed(&working, NOW - 5 * MIN, z.until + MIN).0, Status::Working);
        // A prompt sent since snoozing ends it.
        let prompted = Session { status: Status::Idle, last_prompt_at: NOW - 5 * MIN, ..unread.clone() };
        assert_eq!(snoozed(&prompted, NOW - 30 * MIN, NOW), (Status::Idle, None, prompted.status_since));
        assert_eq!(snoozed(&prompted, NOW - 30 * MIN, z.until + MIN).0, Status::Idle);
        let archived = Session { archived: true, ..idle.clone() };
        assert_eq!(snoozed(&archived, NOW - 30 * MIN, z.until + MIN).0, Status::Idle);
        // A turn it took by itself since (a scheduled check-in) that left it waiting on you ends
        // it; one that only reports, or one begun before the snooze, doesn't.
        let woke = |status: Status, input: i64| Session { status, status_since: Some(NOW - MIN), input_at: Some(input), ..unread.clone() };
        assert_eq!(snoozed(&woke(Status::Needs, NOW - 5 * MIN), NOW - 30 * MIN, NOW), (Status::Needs, None, Some(NOW - MIN)));
        assert!(snooze_called_back(&woke(Status::Needs, NOW - 5 * MIN), z.at));
        assert_eq!(snoozed(&woke(Status::Unread, NOW - 5 * MIN), NOW - 30 * MIN, NOW).1, Some(z.until), "a report");
        assert_eq!(snoozed(&woke(Status::Needs, NOW - 15 * MIN), NOW - 30 * MIN, NOW).1, Some(z.until), "a turn from before the snooze");
        assert_eq!(snoozed(&Session { input_at: None, ..woke(Status::Needs, 0) }, NOW - 30 * MIN, NOW).1, Some(z.until), "a harness that doesn't say");
        let waited = Session { status_since: Some(NOW - 20 * MIN), ..woke(Status::Needs, NOW - 5 * MIN) };
        assert_eq!(snoozed(&waited, NOW - 30 * MIN, NOW).1, Some(z.until), "a call it was snoozed waiting on, a report come in since");
        let asked_mid_turn = Session { status_since: Some(NOW - 4 * MIN), ..woke(Status::Needs, NOW - 2 * MIN) };
        assert!(!snooze_called_back(&asked_mid_turn, z.at), "a call the turn it was snoozed in asked for, a report come in since");
        // Read from its record alone (its process gone), it began waiting as its turn ended.
        let recorded = Session { status_since: None, turn_ended: Some(NOW - MIN), ..woke(Status::Needs, NOW - 5 * MIN) };
        assert!(snooze_called_back(&recorded, z.at));
        // A command that stalled on you in the turn a wakeup started, its status begun with the turn.
        let stalled = Session { status_since: Some(NOW - 6 * MIN), stalled_since: Some(NOW - 2 * MIN), ..woke(Status::Needs, NOW - 5 * MIN) };
        assert!(snooze_called_back(&stalled, z.at));
        let mut untouched = idle.clone();
        apply_snooze(&mut untouched, None, 0, NOW);
        assert_eq!((untouched.status, untouched.snoozed_until), (Status::Idle, None));
    }

    #[test]
    fn a_move_to_the_top_holds_until_its_session_is_taken_up() {
        const MIN: i64 = 60_000;
        let at = NOW - 10 * MIN;
        let raise = |agent: bool| Raise { key: "k".into(), raised: Raised { at, agent, ..Default::default() }, lowered: None };
        let unread = Session { status: Status::Unread, status_since: Some(NOW - 30 * MIN), last_prompt_at: NOW - 40 * MIN, ..crate::fixtures::session("k") };
        let raised = |s: &Session, r: Option<&Raise>| {
            let mut s = s.clone();
            apply_raise(&mut s, r);
            (s.status, s.status_since, s.raised.map(|r| r.at))
        };
        // Waiting on you, it keeps its status and its wait, and says it's first.
        assert_eq!(raised(&unread, Some(&raise(false))), (Status::Unread, unread.status_since, Some(at)));
        let needs = Session { status: Status::Needs, ..unread.clone() };
        assert_eq!(raised(&needs, Some(&raise(true))).2, Some(at));
        // One that's running keeps it, for when its turn ends.
        let working = Session { status: Status::Working, ..unread.clone() };
        assert_eq!(raised(&working, Some(&raise(false))), (Status::Working, unread.status_since, Some(at)));
        // One whose turn is over and seen waits on you again, from when it was moved.
        let idle = Session { status: Status::Idle, ..unread.clone() };
        assert_eq!(raised(&idle, Some(&raise(false))), (Status::Unread, Some(at), Some(at)));
        let stopped = Session { status: Status::Stopped, live: false, ..unread.clone() };
        assert_eq!(raised(&stopped, Some(&raise(false))).0, Status::Unread);
        // A prompt sent to it since takes it up.
        let prompted = Session { status: Status::Idle, last_prompt_at: at + MIN, ..unread.clone() };
        assert_eq!(raised(&prompted, Some(&raise(false))), (Status::Idle, unread.status_since, None));
        // Moved back to its place, archived, or never moved, it shows nothing.
        let lowered = Raise { lowered: Some(NOW), ..raise(true) };
        assert_eq!(raised(&unread, Some(&lowered)).2, None);
        assert_eq!(raised(&Session { archived: true, ..idle.clone() }, Some(&raise(false))), (Status::Idle, unread.status_since, None));
        assert_eq!(raised(&idle, None), (Status::Idle, unread.status_since, None));
        // Seeing it takes it up once its turn is over, not while it runs or waits on a call.
        assert!(seen_takes_up(&unread) && seen_takes_up(&idle) && seen_takes_up(&Session { asks: true, ..needs.clone() }));
        assert!(seen_takes_up(&Session { status: Status::Error, cut_off: Some(NOW), ..unread.clone() }), "a turn cut off");
        assert!(!seen_takes_up(&working) && !seen_takes_up(&needs) && !seen_takes_up(&Session { status: Status::Error, ..unread.clone() }));
    }

    #[test]
    fn moved_to_the_top_goes_first_yours_before_agents_the_latest_first() {
        use std::cmp::Ordering::{Greater, Less};
        let r = |at: i64, agent: bool| Raised { at, agent, ..Default::default() };
        assert_eq!(raised_order(Some(&r(1, true)), None), Less);
        assert_eq!(raised_order(None, Some(&r(1, false))), Greater);
        assert_eq!(raised_order(Some(&r(2, false)), Some(&r(1, false))), Less, "the latest first");
        assert_eq!(raised_order(Some(&r(1, false)), Some(&r(9, true))), Less, "yours before an agent's");
        assert!(raised_order(None, None).is_eq());
        // In a lane, first among its kind: never ahead of a more urgent status.
        let s = |key: &str, status: Status, raised: Option<Raised>| Session { status, status_since: Some(NOW), raised, ..crate::fixtures::session(key) };
        let mut sessions = vec![s("a", Status::Unread, None), s("b", Status::Unread, Some(r(NOW, true))), s("c", Status::Needs, None)];
        sort_sessions(&mut sessions);
        assert_eq!(sessions.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["c", "b", "a"]);
    }

    #[test]
    fn an_agent_may_not_move_up_what_you_snoozed_or_moved_back() {
        let s = Session { status: Status::Unread, status_since: Some(NOW - 1_000), last_prompt_at: NOW - 2_000, ..crate::fixtures::session("k") };
        let lowered = |at: i64| Raise { key: "k".into(), raised: Raised { at: NOW - 5_000, agent: true, ..Default::default() }, lowered: Some(at) };
        assert_eq!(raise_refused(&s, None, true), None);
        assert!(raise_refused(&Session { snoozed_until: Some(NOW + 1), ..s.clone() }, None, true).is_some_and(|w| w.contains("snoozed")));
        assert!(raise_refused(&s, Some(&lowered(NOW)), true).is_some_and(|w| w.contains("back to its place")));
        // Once it's moved on since, it may.
        assert_eq!(raise_refused(&s, Some(&lowered(NOW - 1_500)), true), None, "its status changed since");
        assert_eq!(raise_refused(&Session { last_prompt_at: NOW + 1, ..s.clone() }, Some(&lowered(NOW)), true), None, "sent a prompt since");
        // What you moved up is yours: an agent neither moves it again nor puts it back, though it
        // may put back its own.
        let yours = Session { raised: Some(Raised { at: NOW, ..Default::default() }), ..s.clone() };
        assert!(raise_refused(&yours, None, true).is_some_and(|w| w.contains("theirs to undo")));
        assert!(raise_refused(&yours, None, false).is_some_and(|w| w.contains("theirs to undo")));
        let its = Session { raised: Some(Raised { at: NOW, agent: true, ..Default::default() }), ..s.clone() };
        assert_eq!(raise_refused(&its, None, false), None);
    }

    #[test]
    fn a_session_wears_the_labels_it_was_given_that_still_are() {
        let label = |id: &str, name: &str| Label { id: id.into(), name: name.into(), color: "blue".into() };
        let labels = vec![label("l1", "Release"), label("l2", "Bug")];
        let mut s = crate::fixtures::session("claude:claude:a");
        apply_labels(&mut s, &["l2".into(), "l-gone".into(), "l1".into()], &labels);
        assert_eq!(s.labels, vec![label("l2", "Bug"), label("l1", "Release")], "in the order given, a deleted one left out");
        apply_labels(&mut s, &[], &labels);
        assert!(s.labels.is_empty());
    }

    #[test]
    fn a_quiet_session_waits_on_you_only_when_it_needs_you() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let ended = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        assert_eq!(ended.status, Status::Unread);
        let quiet = |s: &Session| {
            let mut s = s.clone();
            apply_quiet(&mut s, true);
            s
        };
        // A turn that ended is idle, so it's not Your turn: not its lane's bay, and not the mark.
        let q = quiet(&ended);
        assert_eq!((q.status, q.quiet, q.status_since), (Status::Idle, true, ended.status_since));
        assert_eq!(bay_of(std::slice::from_ref(&q), None).0, Bay::Idle);
        // A reply that asks you something is Needs you by then, and stays so.
        let mut asks = ended.clone();
        apply_insight(&mut asks, &insight(|i| i.asks_you = Some(0.9)));
        assert_eq!(quiet(&asks).status, Status::Needs);
        // A call to allow or a question, an error, and work under way show as they are.
        let waiting = claude_session(&a, Some(&reg("waiting", NOW - MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        assert_eq!(quiet(&waiting).status, Status::Needs);
        for st in [Status::Error, Status::Working, Status::Stopped] {
            assert_eq!(quiet(&Session { status: st, ..ended.clone() }).status, st);
        }
        // Not quiet, it's as it was.
        let mut heard = ended.clone();
        apply_quiet(&mut heard, false);
        assert_eq!((heard.status, heard.quiet), (Status::Unread, false));
        // Your own mark of unread, and a snooze's end, come after and bring it back.
        let mut marked = quiet(&ended);
        apply_unread(&mut marked, Some(&UnreadMark { key: ended.key.clone(), at: NOW - 5 * MIN }), NOW - 30 * MIN);
        assert_eq!(marked.status, Status::Unread);
        let mut woke = quiet(&ended);
        apply_snooze(&mut woke, Some(&Snooze { key: ended.key.clone(), at: NOW - 15 * MIN, until: NOW - MIN }), NOW - 30 * MIN, NOW);
        assert_eq!(woke.status, Status::Unread);
    }

    #[test]
    fn suggested_replies_hold_until_the_session_moves_on() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        // Its turn ended 20 minutes ago, on a prompt sent 30 minutes ago; replies were suggested 10
        // minutes ago.
        let ended = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        let ended = Session { last_prompt_at: NOW - 30 * MIN, ..ended };
        // It was read with its prompt of 30 minutes ago the last.
        let suggested = Suggestions { replies: vec!["Yes, push it".into(), "Run the tests first".into()], picks: Default::default(), why: Default::default(), note: Some("It asks whether to push".into()), by: Some("claude:claude:sup".into()), by_title: Some("Supervisor".into()), at: NOW - 10 * MIN, prompt_at: Some(NOW - 30 * MIN) };
        let with = |s: &Session, g: Option<&Suggestions>| {
            let mut s = s.clone();
            apply_suggestions(&mut s, g);
            s.suggestions
        };
        assert_eq!(with(&ended, Some(&suggested)), Some(suggested.clone()));
        // Waiting on you, or still at the turn they were suggested in, they hold.
        assert!(with(&Session { status: Status::Needs, ..ended.clone() }, Some(&suggested)).is_some());
        assert!(with(&Session { status: Status::Working, status_since: Some(NOW - 20 * MIN), ..ended.clone() }, Some(&suggested)).is_some());
        // A prompt sent since, from anywhere, answered them.
        assert_eq!(with(&Session { last_prompt_at: NOW - 5 * MIN, ..ended.clone() }, Some(&suggested)), None);
        // A prompt sent after it was read and before they were suggested answered the turn they
        // were written for: they're for the turn before.
        assert_eq!(with(&Session { last_prompt_at: NOW - 15 * MIN, ..ended.clone() }, Some(&suggested)), None);
        // Ones from a Mac that didn't say what was read go by when they were suggested.
        let undated = Suggestions { prompt_at: None, ..suggested.clone() };
        assert!(with(&Session { last_prompt_at: NOW - 15 * MIN, ..ended.clone() }, Some(&undated)).is_some());
        // A turn started since (a background task woke it) has moved on from them.
        assert!(!suggestions_hold(&Session { status: Status::Working, status_since: Some(NOW - 5 * MIN), ..ended.clone() }, &suggested));
        // None, or none left, puts none on it, and takes away what it had.
        assert_eq!(with(&Session { suggestions: Some(suggested.clone()), ..ended.clone() }, None), None);
        assert_eq!(with(&ended, Some(&Suggestions { replies: vec![], ..suggested.clone() })), None);
        // Choices picked for the questions it asks hold as replies do.
        let picked = Suggestions { replies: vec![], picks: [("Push it?".to_string(), vec!["Yes".to_string()])].into(), ..suggested.clone() };
        assert_eq!(with(&Session { status: Status::Needs, ..ended.clone() }, Some(&picked)), Some(picked.clone()));
        assert_eq!(with(&Session { status: Status::Working, status_since: Some(NOW - 5 * MIN), ..ended.clone() }, Some(&picked)), None);
    }

    #[test]
    fn what_moved_a_session_on_says_when() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let ended = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        let ended = Session { last_prompt_at: NOW - 30 * MIN, ..ended };
        // Read with its prompt of 30 minutes ago the last, tagged 10 minutes ago.
        let (read, at) = (NOW - 30 * MIN, NOW - 10 * MIN);
        assert_eq!(moved_on(&ended, read, at), None);
        assert_eq!(moved_on(&Session { last_prompt_at: NOW - 5 * MIN, ..ended.clone() }, read, at), Some(MovedOn::Prompt { at: NOW - 5 * MIN }));
        // A turn its user's prompt didn't start (a scheduled wakeup, another session's message).
        let woke = Session { status: Status::Working, status_since: Some(NOW - 2 * MIN), ..ended.clone() };
        assert_eq!(moved_on(&woke, read, at), Some(MovedOn::Turn { at: NOW - 2 * MIN }));
        // The turn it was read in is where it stands.
        assert_eq!(moved_on(&Session { status_since: Some(NOW - 20 * MIN), ..woke.clone() }, read, at), None);
        // A prompt says so over the turn it started.
        assert_eq!(moved_on(&Session { last_prompt_at: NOW - 2 * MIN, ..woke }, read, at), Some(MovedOn::Prompt { at: NOW - 2 * MIN }));
    }

    #[test]
    fn a_compaction_is_no_turn_for_tags_and_suggested_replies() {
        const MIN: i64 = 60_000;
        let a = acct("claude");
        // Its turn ended an hour ago, on a prompt sent before it; an agent read it then and both
        // tagged it and suggested replies 50 minutes ago.
        let (read, at) = (NOW - 70 * MIN, NOW - 50 * MIN);
        let ended = TranscriptInfo { last_prompt_at: Some(read), ended_at: Some(NOW - 60 * MIN), last_ts: NOW - 60 * MIN, ..tr() };
        let tag = crate::tags::Tag { insight: SessionInsight { asks_you: Some(1.0), ..Default::default() }, at, prompt_at: read };
        let suggested = Suggestions { replies: vec!["Yes, push it".into()], picks: Default::default(), why: Default::default(), note: None, by: None, by_title: None, at, prompt_at: Some(read) };
        let holds = |s: &Session| (crate::tags::moved_on(s, &tag), suggestions_hold(s, &suggested));
        // Claude Code compacts it, going busy for a minute and writing nothing it says.
        let compacting = claude_session(&a, Some(&reg("busy", NOW - MIN)), Some(&ended), NOW - 55 * MIN, NOW, &Quiet::default()).unwrap();
        assert_eq!((compacting.status, compacting.status_since), (Status::Working, Some(NOW - MIN)));
        assert_eq!(holds(&compacting), (None, true));
        // Once it's done, it's where it was.
        let compacted = TranscriptInfo { compacted: Some(NOW - 60 * MIN), last_ts: NOW - 10_000, ..ended.clone() };
        assert_eq!(holds(&claude_session(&a, Some(&reg("idle", NOW - 9_000)), Some(&compacted), NOW - 55 * MIN, NOW, &Quiet::default()).unwrap()), (None, true));
        // A turn another session's message started has said it, and takes both away.
        let messaged = TranscriptInfo { ended_at: None, last_ts: NOW - MIN, ..ended.clone() };
        let woke = claude_session(&a, Some(&reg("busy", NOW - MIN)), Some(&messaged), NOW - 55 * MIN, NOW, &Quiet::default()).unwrap();
        assert_eq!(holds(&woke), (Some(MovedOn::Turn { at: NOW - MIN }), false));
        // So does a turn that ended since they were given, though nothing's been said after it.
        let again = TranscriptInfo { ended_at: Some(NOW - 5 * MIN), ..ended.clone() };
        assert_eq!(holds(&claude_session(&a, Some(&reg("busy", NOW - MIN)), Some(&again), NOW - 55 * MIN, NOW, &Quiet::default()).unwrap()), (Some(MovedOn::Turn { at: NOW - MIN }), false));
        // A turn they were given in, gone on after you allowed its call, has moved on too.
        let allowed = TranscriptInfo { ended_at: None, unfinished: Some(NOW - 52 * MIN), ..ended };
        assert_eq!(holds(&claude_session(&a, Some(&reg("busy", NOW - MIN)), Some(&allowed), NOW - 55 * MIN, NOW, &Quiet::default()).unwrap()).1, false);
    }

    #[test]
    fn replies_are_turned_down_for_a_session_that_moved_on_since_it_was_read() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let ended = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        let ended = Session { last_prompt_at: NOW - 30 * MIN, ..ended };
        // As it was read, they're taken.
        assert_eq!(suggestion_refused(&ended, NOW - 30 * MIN), None);
        assert_eq!(suggestion_refused(&Session { status: Status::Needs, ..ended.clone() }, NOW - 30 * MIN), None);
        // You replied after it was read: the turn they'd answer is gone.
        let replied = Session { last_prompt_at: NOW - MIN, ..ended.clone() };
        assert!(suggestion_refused(&replied, NOW - 30 * MIN).unwrap().contains("Read it again"));
        // Taking a turn, a prompt or not, it's moving on from what was read; a turn that left a
        // command waiting in the background is over.
        let working = Session { status: Status::Working, status_since: Some(NOW - MIN), ..ended.clone() };
        assert!(suggestion_refused(&working, NOW - 30 * MIN).unwrap().contains("taking a turn"));
        assert_eq!(suggestion_refused(&Session { background: Some("npm run dev".into()), ..working }, NOW - 30 * MIN), None);
    }

    #[test]
    fn a_turn_its_agent_marked_seen_is_idle_until_it_needs_you() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let session = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        let ended = Session { last_prompt_at: NOW - 25 * MIN, ..session };
        assert_eq!(ended.status, Status::Unread);
        let mark = AgentSeen { at: NOW - 21 * MIN, note: Some("Nothing new on the deploy".into()) };
        let marked = |s: &Session| {
            let mut s = s.clone();
            apply_agent_seen(&mut s, Some(&mark));
            s
        };
        // The turn it was marked in ended: idle, not its lane's bay, saying its agent saw it.
        let m = marked(&ended);
        assert_eq!((m.status, m.agent_seen.as_ref(), m.status_since), (Status::Idle, Some(&mark), ended.status_since));
        assert_eq!(bay_of(std::slice::from_ref(&m), None).0, Bay::Idle);
        // A prompt sent after the mark: the turn answering it is Your turn.
        let prompted = Session { last_prompt_at: NOW - 20 * MIN, ..ended.clone() };
        assert_eq!((marked(&prompted).status, marked(&prompted).agent_seen), (Status::Unread, None));
        // A reply that asks you something is Needs you by then, and stays so.
        let mut asks = ended.clone();
        apply_insight(&mut asks, &insight(|i| i.asks_you = Some(0.9)));
        assert_eq!(marked(&asks).status, Status::Needs);
        // A call to allow, an error, work under way and a turn you'd seen show as they are.
        for st in [Status::Needs, Status::Error, Status::Working, Status::Stopped, Status::Idle] {
            let s = marked(&Session { status: st, ..ended.clone() });
            assert_eq!((s.status, s.agent_seen), (st, None));
        }
        let mut untouched = ended.clone();
        apply_agent_seen(&mut untouched, None);
        assert_eq!(untouched.status, Status::Unread);
        // A turn its harness ended isn't the reply its agent spoke for.
        let lost = Session { connection_lost: Some(ConnectionLost { at: NOW - 19 * MIN, ..Default::default() }), ..ended.clone() };
        assert_eq!(marked(&lost).status, Status::Unread);
        let cut = Session { cut_off: Some(NOW - 19 * MIN), ..ended.clone() };
        assert_eq!(marked(&cut).status, Status::Unread);
        // Another session's message or a background task's report taken in after the mark, or a
        // later turn a scheduled wakeup started and ended while Codealong was closed: its record
        // says something came in after it, though no prompt did.
        let came = Session { input_at: Some(NOW - 20 * MIN), ..ended.clone() };
        assert!(agent_seen_spent(&came, mark.at));
        assert_eq!((marked(&came).status, marked(&came).agent_seen), (Status::Unread, None));
        let before = Session { input_at: Some(NOW - 25 * MIN), ..ended.clone() };
        assert_eq!(marked(&before).status, Status::Idle, "what started the turn it was marked in");
        // Where its harness says no more than its prompts, a turn started after the mark is seen
        // as it runs, and the engine forgets the mark then.
        let woke = Session { status: Status::Working, status_since: Some(NOW - 10 * MIN), ..ended.clone() };
        assert!(agent_seen_spent(&woke, mark.at));
        let taking = Session { status: Status::Working, status_since: Some(NOW - 24 * MIN), ..ended.clone() };
        assert!(!agent_seen_spent(&taking, mark.at), "the turn it was marked in");
        // Quiet already has it idle: nothing more to say.
        let mut quiet = ended.clone();
        apply_quiet(&mut quiet, true);
        apply_agent_seen(&mut quiet, Some(&mark));
        assert_eq!((quiet.status, quiet.agent_seen), (Status::Idle, None));
        // Your own mark of unread comes after and brings it back, no longer saying its agent saw it.
        let mut brought = marked(&ended);
        apply_unread(&mut brought, Some(&UnreadMark { key: ended.key.clone(), at: NOW - 5 * MIN }), NOW - 30 * MIN);
        settle_agent_seen(&mut brought);
        assert_eq!((brought.status, brought.agent_seen), (Status::Unread, None));
        let mut kept = marked(&ended);
        settle_agent_seen(&mut kept);
        assert_eq!(kept.agent_seen, Some(mark.clone()));
    }

    #[test]
    fn a_session_marked_unread_is_your_turn_until_seen_or_prompted() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let seen = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        let idle = Session { status: Status::Idle, ..seen };
        let mark = UnreadMark { key: idle.key.clone(), at: NOW - 5 * MIN };
        let marked = |s: &Session, seen_at: i64| {
            let mut s = s.clone();
            apply_unread(&mut s, Some(&mark), seen_at);
            (s.status, s.status_since)
        };
        // Its turn over and seen before the mark: Your turn again, from the mark.
        assert_eq!(marked(&idle, NOW - 10 * MIN), (Status::Unread, Some(mark.at)));
        let stopped = Session { status: Status::Stopped, live: false, ..idle.clone() };
        assert_eq!(marked(&stopped, NOW - 10 * MIN).0, Status::Unread);
        // Seen since, or prompted since, it's as it was.
        assert_eq!(marked(&idle, NOW - MIN).0, Status::Idle);
        let prompted = Session { last_prompt_at: NOW - MIN, ..idle.clone() };
        assert_eq!(marked(&prompted, NOW - 10 * MIN).0, Status::Idle);
        // Running or waiting on you, it shows that; archived, it stays as it is.
        let working = Session { status: Status::Working, ..idle.clone() };
        assert_eq!(marked(&working, NOW - 10 * MIN).0, Status::Working);
        let needs = Session { status: Status::Needs, ..idle.clone() };
        assert_eq!(marked(&needs, NOW - 10 * MIN).0, Status::Needs);
        let archived = Session { archived: true, ..idle.clone() };
        assert_eq!(marked(&archived, NOW - 10 * MIN).0, Status::Idle);
        let mut untouched = idle.clone();
        apply_unread(&mut untouched, None, 0);
        assert_eq!(untouched.status, Status::Idle);
    }

    #[test]
    fn a_side_conversations_answer_is_your_turn_until_seen_or_prompted() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let seen = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        let idle = Session { status: Status::Idle, last_prompt_at: NOW - 40 * MIN, ..seen };
        let side = |id: &str, open: bool, answered_at: Option<i64>| crate::side::Glance { id: id.into(), question: "Why a queue?".into(), open, answered_at, ..Default::default() };
        let answered = NOW - 5 * MIN;
        let applied = |s: &Session, sides: &[crate::side::Glance], seen_at: i64| {
            let mut s = s.clone();
            apply_side_answer(&mut s, sides, seen_at);
            s
        };
        let one = [side("a", true, Some(answered))];

        // Its turn over and seen before the answer: Your turn, from the answer, saying what for.
        let s = applied(&idle, &one, NOW - 10 * MIN);
        assert_eq!((s.status, s.status_since, s.excerpt.as_deref()), (Status::Unread, Some(answered), Some("Answered on the side · Why a queue?")));
        assert_eq!(s.side_answer, Some(SideAnswer { id: "a".into(), open: true, at: answered }));
        let stopped = Session { status: Status::Stopped, live: false, ..idle.clone() };
        assert_eq!(applied(&stopped, &one, NOW - 10 * MIN).status, Status::Unread);

        // Seen since, or prompted since, it's as it was, and nothing's left to show.
        let s = applied(&idle, &one, NOW - MIN);
        assert_eq!((s.status, s.side_answer), (Status::Idle, None));
        let prompted = Session { last_prompt_at: NOW - MIN, ..idle.clone() };
        assert_eq!(applied(&prompted, &one, NOW - 10 * MIN).status, Status::Idle);

        // Running or waiting on you, it shows that, and has the answer for when you come to it.
        for status in [Status::Working, Status::Needs, Status::Error] {
            let s = applied(&Session { status, excerpt: Some("Reading".into()), ..idle.clone() }, &one, NOW - 10 * MIN);
            assert_eq!((s.status, s.excerpt.as_deref(), s.side_answer.is_some()), (status, Some("Reading"), true));
        }
        // A turn of its own that ended stays what it waits for, since when it did.
        let turn = Session { status: Status::Unread, status_since: Some(NOW - 20 * MIN), excerpt: Some("Done.".into()), ..idle.clone() };
        let s = applied(&turn, &one, NOW - 30 * MIN);
        assert_eq!((s.status_since, s.excerpt.as_deref(), s.side_answer.is_some()), (Some(NOW - 20 * MIN), Some("Done."), true));

        // Archived, it stays as it is.
        let s = applied(&Session { archived: true, ..idle.clone() }, &one, NOW - 10 * MIN);
        assert_eq!((s.status, s.side_answer), (Status::Idle, None));
        // What's kept from before answers have times, or one still answering, waits on nothing.
        let s = applied(&idle, &[side("old", false, None)], NOW - 10 * MIN);
        assert_eq!((s.status, s.side_answer), (Status::Idle, None));
        // Of several, the latest shows.
        let s = applied(&idle, &[side("first", false, Some(answered - MIN)), side("last", false, Some(answered)), side("seen", false, Some(NOW - 20 * MIN))], NOW - 10 * MIN);
        assert_eq!(s.side_answer, Some(SideAnswer { id: "last".into(), open: false, at: answered }));
    }

    #[test]
    fn a_session_set_aside_waits_on_its_side_conversation_then_needs_you_again() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let base = claude_session(&a, Some(&reg("waiting", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        let asked = NOW - 20 * MIN;
        let needs = Session { status: Status::Needs, need: Some(Need::Question), status_since: Some(asked), last_prompt_at: NOW - 40 * MIN, excerpt: Some("Restore test?".into()), ..base };
        let side = |running: bool, answered_at: Option<i64>| crate::side::Glance { id: "a".into(), question: "Are we restoring for real?".into(), running, open: true, answered_at, ..Default::default() };
        let set = NOW - 5 * MIN;
        let applied = |s: &Session, at: Option<i64>, sides: &[crate::side::Glance]| {
            let mut s = s.clone();
            apply_aside(&mut s, at, sides);
            s
        };

        // Set aside while its side conversation answers: still needing you underneath, but set
        // aside since then, saying what it asked on the side, and out of the lane's wait.
        let s = applied(&needs, Some(set), &[side(true, None)]);
        assert_eq!((s.status, s.aside, s.excerpt.as_deref()), (Status::Needs, Some(set), Some("Asking on the side · Are we restoring for real?")));
        assert!(s.held_back());
        assert_eq!(lane_waiting_since(std::slice::from_ref(&s)), None);
        assert!(raise_refused(&s, None, true).is_some());

        // Once it has answered, or nothing answers any more, or you closed it as it answered, it
        // needs you again as it did.
        let closed = crate::side::Glance { open: false, ..side(true, None) };
        for sides in [vec![side(false, Some(NOW - MIN))], vec![side(false, None)], vec![], vec![side(true, Some(NOW - MIN))], vec![closed]] {
            let s = applied(&needs, Some(set), &sides);
            assert_eq!((s.status, s.aside, s.excerpt.as_deref()), (Status::Needs, None, Some("Restore test?")), "{sides:?}");
        }
        // An answer from before it was set aside doesn't end it.
        assert_eq!(applied(&needs, Some(set), &[side(true, Some(set - MIN))]).aside, Some(set));

        // Only the need it was set aside from, and only until something's sent it.
        let later = Session { status_since: Some(set + MIN), ..needs.clone() };
        assert_eq!(applied(&later, Some(set), &[side(true, None)]).aside, None);
        let prompted = Session { last_prompt_at: set + MIN, ..needs.clone() };
        assert_eq!(applied(&prompted, Some(set), &[side(true, None)]).aside, None);
        // Not needing you, or archived, there's nothing to set aside; nor without a mark.
        for status in [Status::Working, Status::Unread, Status::Idle, Status::Error] {
            assert_eq!(applied(&Session { status, ..needs.clone() }, Some(set), &[side(true, None)]).aside, None, "{status:?}");
        }
        assert_eq!(applied(&Session { archived: true, ..needs.clone() }, Some(set), &[side(true, None)]).aside, None);
        assert_eq!(applied(&needs, None, &[side(true, None)]).aside, None);
    }

    #[test]
    fn a_quiet_command_that_isnt_waiting_for_typing_is_working() {
        let a = acct("claude2");
        let mut t = tr();
        t.last_ts = NOW - 3 * DAY_MS;
        let quiet = Quiet { subagent_at: None, running: Some("cargo build --release"), ..Default::default() };
        let mut s = claude_session(&a, Some(&reg("busy", NOW - 3 * DAY_MS)), Some(&t), 0, NOW, &quiet).unwrap();
        let mut login = s.clone();
        apply_insight(&mut login, &insight(|i| i.waits_for_input = Some(0.8)));
        assert_eq!(login.status, Status::Needs);
        apply_insight(&mut s, &insight(|i| i.waits_for_input = Some(0.1)));
        assert_eq!((s.status, s.stalled_since), (Status::Working, None));
        assert!(s.excerpt.as_deref().unwrap().starts_with("Running `cargo build --release` for {for:"));
    }

    #[test]
    fn a_background_command_polls_runs_or_is_stuck() {
        let a = acct("claude2");
        let quiet = Quiet { subagent_at: None, running: Some("while true; do sleep 60; gh pr view 1054; done"), ..Default::default() };
        let mut s = claude_session(&a, Some(&reg("shell", NOW - DAY_MS)), Some(&tr()), 0, NOW, &quiet).unwrap();
        s.reply_tail = Some("I'll keep checking. Waiting for the reviewer to approve #1054.".into());
        let mut poll = s.clone();
        apply_insight(&mut poll, &insight(|i| (i.background, i.background_p) = (Some("polling".into()), Some(0.9))));
        assert_eq!((poll.status, poll.excerpt.as_deref()), (Status::Working, Some("Polling in the background · Waiting for the reviewer to approve #1054.")));
        let mut job = s.clone();
        apply_insight(&mut job, &insight(|i| (i.background, i.background_p) = (Some("job".into()), Some(0.9))));
        assert!(job.excerpt.as_deref().unwrap().starts_with("Running in the background · `while true"));
        let mut maybe = s.clone();
        apply_insight(&mut maybe, &insight(|i| (i.background, i.background_p) = (Some("stalled".into()), Some(0.5))));
        assert_eq!(maybe.status, Status::Working);
        apply_insight(&mut s, &insight(|i| (i.background, i.background_p) = (Some("stalled".into()), Some(0.8))));
        assert_eq!(s.status, Status::Needs);
        assert!(s.stalled_since.is_some());
        assert_eq!(session_headline(&s).0, "`claude2` is stuck · Portal daily log + photo sync");
        // It needs you, but its turn is over: what you send it is a prompt, not an answer.
        assert!(!crate::send::waits_on_you(&s));
    }

    #[test]
    fn jev_takes_a_server_out_of_working() {
        let a = acct("claude2");
        let quiet = Quiet { subagent_at: None, running: Some("npm run dev"), ..Default::default() };
        let s = claude_session(&a, Some(&reg("shell", NOW - 60_000)), Some(&tr()), 0, NOW, &quiet).unwrap();
        assert_eq!(s.status, Status::Working);
        let mut server = s.clone();
        apply_insight(&mut server, &insight(|i| (i.background, i.background_p) = (Some("server".into()), Some(0.9))));
        assert_eq!((server.status, server.excerpt.as_deref()), (Status::Idle, Some("Left running · `npm run dev`")));
        let mut unsure = s.clone();
        apply_insight(&mut unsure, &insight(|i| (i.background, i.background_p) = (Some("server".into()), Some(0.5))));
        assert_eq!(unsure.status, Status::Working);
    }

    #[test]
    fn a_server_read_leaves_working_a_session_that_waits_on_more() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let watch = "tail -n +1 -f ~/vm/test.log | grep --line-buffered -E \"^error|test result\"";
        let task = |id: &str, command: &str, monitor: bool, started_at: i64| BackgroundTask { id: id.into(), command: command.into(), monitor, started_at, ..Default::default() };
        let read = |s: &Session| {
            let mut s = s.clone();
            apply_insight(&mut s, &insight(|i| (i.background, i.background_p) = (Some("server".into()), Some(0.97))));
            (s.status, s.excerpt)
        };
        let quiet = Quiet { running: Some(watch), ..Default::default() };
        let s = claude_session(&a, Some(&reg("shell", NOW - 60_000)), Some(&tr()), 0, NOW, &quiet).unwrap();
        // The process table's oldest shell is a Monitor's `tail -f`, which Jev reads as a server,
        // while the relaunch it started after runs on: it waits on the relaunch (2026-11-14).
        let relaunch = "scripts/relaunch.sh 2>&1 | tail -5";
        let both = Session { tasks: vec![task("b1", watch, true, NOW - 25 * MIN), task("b2", relaunch, false, NOW - MIN)], ..s.clone() };
        assert_eq!(read(&both), (Status::Working, Some("Waiting on `scripts/relaunch.sh 2>&1 | tail -5`".into())));
        // A Monitor alone wakes it with each line, so it waits on that.
        let alone = Session { tasks: vec![task("b1", watch, true, NOW - 25 * MIN)], ..s.clone() };
        assert_eq!(read(&alone).0, Status::Working);
        // A dev server beside a build: the build. One that ended doesn't count, nor does an
        // empty list, as before.
        let server = Session { background: Some("npm run dev".into()), ..s.clone() };
        let build = task("b4", "", false, NOW - MIN);
        let beside = Session { tasks: vec![task("b3", "npm run dev", false, NOW - 9 * MIN), BackgroundTask { about: "Build the app".into(), ..build.clone() }], ..server.clone() };
        assert_eq!(read(&beside), (Status::Working, Some("Waiting on `Build the app`".into())));
        let unnamed = Session { tasks: vec![task("b3", "npm run dev", false, NOW - 9 * MIN), build.clone()], ..server.clone() };
        assert_eq!(read(&unnamed).1.as_deref(), Some("Waiting on a command in the background"));
        let ended = Session { tasks: vec![task("b3", "npm run dev", false, NOW - 9 * MIN), BackgroundTask { ended_at: Some(NOW - MIN), status: Some("completed".into()), ..build }], ..server.clone() };
        for left in [ended, server] {
            assert_eq!(read(&left), (Status::Idle, Some("Left running · `npm run dev`".into())));
        }
        // Where the list has lost the server, what it still names runs beside it.
        let lost = Session { tasks: vec![task("b5", "cargo build", false, NOW - MIN)], background: Some("npm run dev".into()), ..s.clone() };
        assert_eq!(read(&lost), (Status::Working, Some("Waiting on `cargo build`".into())));
        // The same command twice is the one Jev read.
        let twice = Session { tasks: vec![task("b3", "npm run dev", false, NOW - 9 * MIN), task("b6", " npm run dev", false, NOW - MIN)], background: Some("npm run dev".into()), ..s.clone() };
        assert_eq!(read(&twice).0, Status::Idle);
        // Its processes already sat still long enough to be left running: so it stays.
        let still = Session { status: Status::Idle, excerpt: Some("Left running · `npm run dev`".into()), ..beside };
        assert_eq!(read(&still), (Status::Idle, Some("Left running · `npm run dev`".into())));
    }

    #[test]
    fn a_helper_is_out_of_its_worktrees_status_until_it_needs_you() {
        const MIN: i64 = 60_000;
        let apps = AppNames::default();
        let seeds = vec![seed("/r/codealong", "main")];
        let (git, prs, visitors) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let a = acct("claude");
        let session = |id: &str, status: &str, seen_at: i64| {
            let mut s = claude_session(&a, Some(&reg(status, NOW - 20 * MIN)), Some(&tr()), seen_at, NOW, &Quiet::default()).unwrap();
            (s.id, s.key, s.cwd) = (id.into(), Session::key_for(Harness::Claude, "claude", id), "/r/codealong".into());
            s
        };
        // You've seen what the session that started it last said.
        let boss = session("boss", "idle", NOW);
        assert_eq!(boss.status, Status::Idle);
        let helper = |status: &str| {
            let mut s = session("rev", status, 0);
            s.helper = helper_of(Some((&boss.key, false)), |k| k == boss.key, None);
            apply_helper(&mut s);
            s
        };
        let board = |h: Session| compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![h, boss.clone()], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new() });
        // Working, it's its agent's work: the worktree stands as its own sessions do, and the mark
        // doesn't spin for it. It's listed after them.
        let working = board(helper("busy"));
        let lane = &working.lanes[0];
        assert_eq!((lane.bay, lane.sessions.iter().map(|s| s.id.as_str()).collect::<Vec<_>>()), (Bay::Idle, vec!["boss", "rev"]));
        assert_eq!(working.logo_status, LogoStatus::Idle);
        // A turn it ended is its agent's to read, not Your turn.
        let ended = helper("idle");
        assert_eq!(ended.status, Status::Idle);
        assert_eq!(board(ended).lanes[0].bay, Bay::Idle);
        // Waiting on you to allow something, it's in Needs you like any session, saying so.
        let waiting = board(helper("waiting"));
        assert_eq!((waiting.lanes[0].bay, waiting.lanes[0].sessions[0].id.as_str()), (Bay::Needs, "rev"));
        assert_eq!(waiting.logo_status, LogoStatus::Needs);
    }

    #[test]
    fn a_helper_helps_a_session_still_on_the_board_or_a_pr() {
        let here = |k: &str| k == "claude:claude:boss";
        assert_eq!(helper_of(Some(("claude:claude:boss", true)), here, None), Some(Helper { parent: Some("claude:claude:boss".into()), pr: None, report: true, yours: false }));
        // The session that started it is gone: it's yours again.
        assert_eq!(helper_of(Some(("claude:claude:gone", true)), here, None), None);
        assert_eq!(helper_of(None, here, Some("o/r#7")), Some(Helper { parent: None, pr: Some("o/r#7".into()), report: false, yours: false }));
        assert_eq!(helper_of(None, here, None), None);
        // A helper's ended turn is idle; anyone else's, and its other statuses, are as they were.
        let ended = Session { status: Status::Unread, ..Session::new(Harness::Claude, "claude", "rev") };
        let mut theirs = ended.clone();
        apply_helper(&mut theirs);
        assert_eq!(theirs.status, Status::Unread);
        for (st, want) in [(Status::Unread, Status::Idle), (Status::Needs, Status::Needs), (Status::Error, Status::Error), (Status::Working, Status::Working)] {
            let mut h = Session { status: st, helper: helper_of(None, here, Some("o/r#7")), ..ended.clone() };
            apply_helper(&mut h);
            assert_eq!((h.status, h.helping()), (want, !matches!(want, Status::Needs | Status::Error)));
        }
        // One you're talking to wrote its turn for you: it waits on you as yours do, still the
        // PR's review.
        let mut talked = Session { helper: Some(Helper { pr: Some("o/r#7".into()), yours: true, ..Default::default() }), ..ended.clone() };
        apply_helper(&mut talked);
        assert_eq!(talked.status, Status::Unread);
        assert!(!talked.helping() && !talked.unattended() && !talked.helps_an_agent() && talked.helper.is_some());
    }

    /// A review cut off partway whose round made way for new commits sat in Needs you as Cut off
    /// until its next round came, with nothing for you to do there (2026-11-19): while that round
    /// is to come in it, it's idle, still saying it was cut off. Any other error, one you're
    /// talking to and one whose round isn't to come still wait on you.
    #[test]
    fn a_review_cut_off_waits_on_its_next_round_not_on_you() {
        let review = Session { status: Status::Error, live: false, cut_off: Some(NOW), helper: helper_of(None, |_| false, Some("o/r#7")), ..Session::new(Harness::Codex, "codex", "rev") };
        let applied = |s: &Session, to_come: bool| {
            let mut s = s.clone();
            apply_round_to_come(&mut s, to_come);
            s
        };
        let waits = applied(&review, true);
        assert_eq!((waits.status, waits.cut_off), (Status::Idle, Some(NOW)));
        assert!(waits.helping() && waits.unattended());
        assert_eq!(applied(&review, false).status, Status::Error, "no round to come");
        assert_eq!(applied(&Session { cut_off: None, ..review.clone() }, true).status, Status::Error, "another error");
        let talked = Session { helper: Some(Helper { pr: Some("o/r#7".into()), yours: true, ..Default::default() }), ..review.clone() };
        assert_eq!(applied(&talked, true).status, Status::Error, "yours");
        assert_eq!(applied(&Session { status: Status::Needs, ..review.clone() }, true).status, Status::Needs, "a call to allow");
    }

    /// A background job's agent tried effort levels on test chats in tmux, in its own folder, and
    /// each one's ended turn waited on you under the job's name (2026-10-14).
    #[test]
    fn a_session_in_another_sessions_scratch_folder_is_its_helper() {
        const JOB: &str = "/Users/me/.claude/jobs/54a9a7a0";
        let at = |id: &str, cwd: &str| Session { cwd: cwd.into(), status: Status::Unread, ..Session::new(Harness::Claude, "claude", id) };
        let owner_of = |p: &Path| p.starts_with(JOB).then(|| (Harness::Claude, "54a9a7a0-b633".to_string()));
        let mut sessions = vec![
            at("54a9a7a0-b633", "/r/codealong"),
            at("test-chat", &format!("{JOB}/tmp/efftest")),
            // Started there with `codealong start --helper`, it keeps what that said.
            Session { helper: Some(Helper { parent: Some("claude:claude:other".into()), pr: None, report: true, yours: false }), ..at("asked", JOB) },
            Session { headless: true, ..at("run", JOB) },
            at("mine", JOB),
            at("elsewhere", "/r/codealong"),
            // A fork you made of one there, and one its agent started for you without --helper.
            Session { origin: Some(SessionOrigin { key: "claude:claude:mine".into(), how: OriginHow::Fork }), ..at("fork", JOB) },
            Session { origin: Some(SessionOrigin { key: "claude:claude:54a9a7a0-b633".into(), how: OriginHow::Started }), ..at("yours", JOB) },
        ];
        scratch_helpers(&mut sessions, owner_of, |k| k == "claude:claude:mine");
        let boss = "claude:claude:54a9a7a0-b633";
        let helps = |id: &str| sessions.iter().find(|s| s.id == id).and_then(|s| s.helper.as_ref()).map(|h| (h.parent.clone().unwrap_or_default(), h.report));
        assert_eq!(helps("test-chat"), Some((boss.to_string(), false)));
        assert_eq!(sessions[1].origin, Some(SessionOrigin { key: boss.into(), how: OriginHow::Started }));
        assert_eq!(helps("asked"), Some(("claude:claude:other".to_string(), true)));
        assert_eq!((helps("run"), helps("mine"), helps("elsewhere"), helps("54a9a7a0-b633")), (None, None, None, None));
        assert_eq!((helps("fork"), helps("yours")), (None, None));
        // Its ended turn is its agent's to read.
        apply_helper(&mut sessions[1]);
        assert_eq!(sessions[1].status, Status::Idle);
        // A session whose own folder it isn't on the board makes no helper.
        let mut alone = vec![at("test-chat", &format!("{JOB}/tmp/efftest"))];
        scratch_helpers(&mut alone, owner_of, |_| false);
        assert_eq!(alone[0].helper, None);
    }

    #[test]
    fn a_print_run_is_headless_and_never_waits_on_you_for_its_end() {
        let a = acct("claude2");
        let at = |entrypoint: &str| TranscriptInfo { entrypoint: Some(entrypoint.into()), ..tr() };
        let headless = |r: Option<&RegistryEntry>, t: &TranscriptInfo| claude_session(&a, r, Some(t), 0, NOW, &Quiet::default()).unwrap().headless;
        // `claude -p`'s, running or done; not a terminal's, the app's or the Agent SDK's.
        assert!(headless(None, &at("sdk-cli")));
        assert!(headless(Some(&RegistryEntry { entrypoint: Some("sdk-cli".into()), ..reg("busy", NOW) }), &tr()));
        for e in ["cli", "claude-desktop", "sdk-ts"] {
            assert!(!headless(None, &at(e)), "{e}");
        }
        // Resumed in a terminal, it's that while it runs there.
        assert!(!headless(Some(&RegistryEntry { entrypoint: Some("cli".into()), ..reg("idle", NOW) }), &at("sdk-cli")));
        // Its ended turn is idle, an error too; one waiting on a call to allow still needs you.
        for (st, want) in [(Status::Unread, Status::Idle), (Status::Error, Status::Idle), (Status::Needs, Status::Needs), (Status::Working, Status::Working)] {
            let mut s = Session { status: st, headless: true, ..Session::new(Harness::Claude, "claude", "p") };
            apply_headless(&mut s);
            assert_eq!(s.status, want);
        }
        let mut yours = Session { status: Status::Error, ..Session::new(Harness::Claude, "claude", "y") };
        apply_headless(&mut yours);
        assert_eq!(yours.status, Status::Error);
        // Out of what waits on you while it doesn't need you, as a helper is.
        let run = |status| Session { status, headless: true, ..Session::new(Harness::Claude, "claude", "p") };
        assert!(run(Status::Working).unattended() && run(Status::Idle).unattended());
        assert!(!run(Status::Needs).unattended());
        assert!(!Session { status: Status::Working, ..Session::new(Harness::Claude, "claude", "y") }.unattended());
    }

    #[test]
    fn a_headless_run_was_started_by_the_nearest_session_above_its_process() {
        // An eval's judge: claude -p (40) under python (30), under the shell the agent's Bash
        // tool opened (20), under the agent's claude (10), under a background job's host (5).
        let parents = HashMap::from([(40, 30), (30, 20), (20, 10), (10, 5), (5, 1), (77, 1), (90, 91), (91, 90)]);
        let parent = |p: u32| parents.get(&p).copied();
        let by_pid = HashMap::from([(10, "claude:claude2:agent"), (5, "claude:claude2:host")]);
        assert_eq!(started_by(40, parent, &by_pid), Some("claude:claude2:agent"));
        // Run from a script in a terminal of yours, it meets no session's process on the way up.
        assert_eq!(started_by(77, parent, &by_pid), None);
        // Nor once a process on the way is gone, nor round a loop.
        assert_eq!(started_by(41, parent, &by_pid), None);
        assert_eq!(started_by(90, parent, &by_pid), None);
    }

    #[test]
    fn a_headless_run_no_session_is_above_is_the_one_its_environment_names() {
        // The judge's claude -p (40) under a driver the agent started on its own (30), left to
        // launchd; a run beside the agent (50); one from a terminal of yours (77); one whose
        // session isn't on the board (88).
        let parents = HashMap::from([(40, 30), (30, 1), (50, 10), (10, 1), (77, 1), (88, 1)]);
        let under = |p: u32| match p {
            40 | 50 => vec![(Harness::Claude, "agent".to_string())],
            88 => vec![(Harness::Claude, "gone".to_string())],
            _ => vec![],
        };
        let by_pid = HashMap::from([(10, "claude:claude2:beside")]);
        let by_id = HashMap::from([((Harness::Claude, "agent"), "claude:claude2:agent")]);
        let found = |pid| run_started_by(pid, |p: u32| parents.get(&p).copied(), under, &by_pid, &by_id);
        assert_eq!(found(40), Some("claude:claude2:agent"));
        // The session above it is the one that ran it, whatever its environment says.
        assert_eq!(found(50), Some("claude:claude2:beside"));
        assert_eq!(found(77), None);
        assert_eq!(found(88), None);
        // An ID is another harness's session's only under that harness.
        let codex = HashMap::from([((Harness::Codex, "agent"), "codex:codex:agent")]);
        assert_eq!(run_started_by(40, |p: u32| parents.get(&p).copied(), under, &by_pid, &codex), None);
        // A run from a Codex thread's command names the thread and the Claude Code session that
        // started Codex: the one on the board is the run's.
        let both = |_: u32| vec![(Harness::Claude, "gone".to_string()), (Harness::Codex, "agent".to_string())];
        assert_eq!(run_started_by(40, |p: u32| parents.get(&p).copied(), both, &by_pid, &codex), Some("codex:codex:agent"));
    }

    #[test]
    #[cfg_attr(windows, ignore = "Git Bash starts a background job by fork and exec, whose Windows parent is gone by then, so nothing climbs to its shell")]
    fn a_real_process_climbs_to_the_one_that_started_its_shell() {
        // This test as the session, a shell its command opened, and a run the shell started.
        let mut shell = std::process::Command::new(crate::procs::posix_tool("sh")).args(["-c", &format!("sleep 5 & echo {}; wait", crate::procs::LAST_PID)]).stdout(std::process::Stdio::piped()).spawn().unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(shell.stdout.take().unwrap()), &mut line).unwrap();
        let run: u32 = line.trim().parse().unwrap();
        let by_pid = HashMap::from([(std::process::id(), "claude:claude:me")]);
        // Windows' list of processes is a snapshot up to a quarter of a second old.
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut found = started_by(run, crate::procs::parent_pid, &by_pid);
        while found.is_none() && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(20));
            found = started_by(run, crate::procs::parent_pid, &by_pid);
        }
        let _ = crate::os::procs::signal(run, crate::os::procs::Signal::Terminate);
        let _ = shell.wait();
        assert_eq!(found, Some("claude:claude:me"));
    }

    #[test]
    fn a_sessions_helpers_are_the_ones_it_started_as_helpers_each_once() {
        let of = |id: &str, parent: Option<&str>| Session { helper: parent.map(|p| Helper { parent: Some(p.into()), pr: None, report: false, yours: false }), ..Session::new(Harness::Claude, "claude", id) };
        let visiting = Session { visitor: Some("worked here".into()), ..of("a", Some("claude:claude:boss")) };
        let archived = Session { archived: true, ..of("old", Some("claude:claude:boss")) };
        let lanes = [
            Lane { sessions: vec![of("boss", None), of("a", Some("claude:claude:boss")), of("b", Some("claude:claude:other")), archived], ..Lane::new("/r/app", "P", "app") },
            Lane { sessions: vec![visiting, of("c", Some("claude:claude:boss"))], ..Lane::new("/r/app-two", "P", "app") },
        ];
        let ids: Vec<&str> = helpers_of(&lanes, "claude:claude:boss").iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["a", "c"]);
    }
}

//! Jev's read of each session (`[jev] sessions`): one request per session per turn, asking only
//! what that moment can answer, all at once (docs.typesafe.ai, speculative fan-out).
//!
//! - A turn that ended: does the reply end by asking you something, how far did the work get
//!   (landed, pushed, committed…), and is what you asked for done. The work's landing isn't asked
//!   when the reply's first line says it in the words CLAUDE.md's *Landing work* asks agents for
//!   (`landing_said`): that's exact, and free.
//! - A busy Claude session gone quiet on a command: is the command waiting for typing, or is it
//!   long work.
//! - A Claude session whose turn left a command running in the background: is it polling for
//!   something outside, one long job, or stuck.
//! - A session waiting on your approval: how risky the command looks, from the whole call when
//!   its harness shows it (`Pending::input`), else from the line the excerpt has.
//!
//! What's asked is decided by `ask`, from the session as the board built it; the answer is kept
//! under that moment's signature, so a session is asked once per turn, also across restarts
//! (insights.json). An agent that read the session can say the same things itself
//! (`codealong session tag`, tags.rs), and its word stands over Jev's for the moment it read
//! (`merged`). `board::apply_insight` turns the result into what the board shows.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::jev;
use crate::model::{Session, SessionInsight, Status};
use crate::util::DAY_MS;

/// Turns that ended longer ago than this aren't asked about: a first run shouldn't ask about
/// every session of the past two weeks.
const RECENT_MS: i64 = 2 * DAY_MS;
/// Answers for sessions not seen for this long are dropped from insights.json.
const KEEP_MS: i64 = 14 * DAY_MS;
/// How much of the call a session waits on you to allow goes in.
const WAITING_MOST: usize = 1500;

/// What a session's moment lets Jev be asked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Its turn ended.
    Turn,
    /// Busy, but quiet for a long while on a known command.
    Quiet,
    /// Its turn left a command running in the background.
    Background,
    /// Waiting on your approval for a command.
    Approval,
}

impl Kind {
    fn tag(self) -> &'static str {
        match self {
            Kind::Turn => "turn",
            Kind::Quiet => "quiet",
            Kind::Background => "bg",
            Kind::Approval => "approval",
        }
    }
    fn from_tag(tag: &str) -> Option<Kind> {
        [Kind::Turn, Kind::Quiet, Kind::Background, Kind::Approval].into_iter().find(|k| k.tag() == tag)
    }
}

fn hash(parts: &[&str]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// The command in an excerpt such as "Bash · `npm install`" or "Stuck on `x` for {for:…}".
fn command_in(excerpt: &str) -> Option<&str> {
    let start = excerpt.find('`')? + 1;
    let end = start + excerpt[start..].find('`')?;
    Some(&excerpt[start..end]).filter(|c| !c.trim().is_empty())
}

/// Whether its turn is over: its last reply is what it has to say. One that Needs you only
/// because that reply asks you something (`asks`, from `board::apply_insight`) is at its prompt
/// too: asked about again from the published board, it's the same moment, not an approval.
fn turn_ended(s: &Session) -> bool {
    matches!(s.status, Status::Unread | Status::Idle | Status::Stopped) || s.asks
}

/// What to ask about a session now, and the signature that moment is kept under: `<kind>-<hash>`
/// of what the answer depends on. None when there's nothing to ask.
pub fn ask(s: &Session, now: i64) -> Option<(Kind, String)> {
    if s.archived || s.visitor.is_some() {
        return None;
    }
    // When it began waiting: each call a session waits on you for has a time of its own
    // (`status_since` while it Needs you), so two calls that show the same line (the same tool,
    // other arguments) are two moments, and the risk read for one, from the whole call, isn't
    // kept for the other. A call replaced while its question waits has a new time too, so an
    // answer read for the old one never applies.
    let since = s.status_since.filter(|_| s.status == Status::Needs).map(|t| t.to_string()).unwrap_or_default();
    let (kind, basis): (Kind, Vec<&str>) = if let Some(cmd) = s.background.as_deref() {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return None;
        }
        (Kind::Background, vec![cmd, s.reply_tail.as_deref().unwrap_or("")])
    } else if s.stalled_since.is_some() {
        (Kind::Quiet, vec![command_in(s.excerpt.as_deref()?)?])
    } else if s.status == Status::Needs && !s.asks {
        let excerpt = s.excerpt.as_deref()?;
        (Kind::Approval, vec![command_in(excerpt).unwrap_or(excerpt), since.as_str()])
    } else if turn_ended(s) {
        // A helper's last reply is for its agent to read: what it asks isn't asked of you. A turn
        // its harness ended (the connection lost, a usage limit, cut off) has an error for a
        // reply, which Jev would take for the agent being blocked.
        if now - s.last_activity > RECENT_MS || s.helper.is_some() || s.connection_lost.is_some() || s.cut_off.is_some() || s.limit.is_some() {
            return None;
        }
        // The same reply to another prompt is another moment: finished is asked about the prompt.
        (Kind::Turn, vec![s.reply_tail.as_deref().filter(|t| !t.trim().is_empty())?, s.last_prompt.as_deref().unwrap_or("")])
    } else {
        return None;
    };
    let mut parts = vec![kind.tag()];
    parts.extend(basis);
    Some((kind, format!("{}-{}", kind.tag(), hash(&parts))))
}

/// The kind a signature was asked as.
pub fn kind_of(signature: &str) -> Option<Kind> {
    Kind::from_tag(signature.split('-').next()?)
}

/// How far a code change can have got, by name, with what each means.
pub const LANDING: [(&str, &str); 6] = [
    ("landed", "The change is merged, or pushed straight to the main branch."),
    ("pushed", "The change is pushed to a branch or a pull request that isn't merged yet."),
    ("committed", "The change is committed but not pushed."),
    ("uncommitted", "Files were changed but nothing is committed."),
    ("blocked", "The agent couldn't finish: something failed, or it's waiting on something else."),
    ("none", "No code change: an answer, research, a review, or a question."),
];

/// What a command left running in the background can be doing, by name, with what each means.
pub const BACKGROUND: [(&str, &str); 4] = [
    ("polling", "Checks again and again for something outside to happen: a review or approval, CI, a deploy, a file or URL appearing."),
    ("job", "Runs one long piece of work that ends by itself: a build, tests, a download, a migration."),
    ("server", "Keeps running until someone stops it, and nothing waits for it to end: a dev server, a file watcher, `tail -f`, a tunnel."),
    ("stalled", "Looks stuck, or waits on something that won't happen without the user: a sign-in in a browser, a password, a question on the terminal."),
];

const RISK: [&str; 4] = [
    "Only reads or inspects; changes nothing",
    "Changes files or state on this computer only",
    "Affects shared or remote things: pushes, deploys, publishes, sends messages, changes cloud resources",
    "Destructive or hard to undo: deletes data, force-pushes, drops tables, rewrites history",
];

/// How far the work got when the first line of the session's last reply says so, as CLAUDE.md's
/// *Landing work* asks agents to open their last message: its first word `Landed:`, `Pushed:`,
/// `Committed:` or `Not committed:`, after any Markdown marks. Only once its turn is over: while
/// it works, the line is the turn before's.
pub fn landing_said(s: &Session) -> Option<&'static str> {
    if !turn_ended(s) {
        return None;
    }
    let head = s.reply_head.as_deref()?.trim_start_matches(|c: char| !c.is_alphanumeric());
    let lower: String = head.chars().take(20).collect::<String>().to_lowercase();
    [("landed:", "landed"), ("pushed:", "pushed"), ("committed:", "committed"), ("not committed:", "uncommitted")].into_iter().find(|(w, _)| lower.starts_with(w)).map(|(_, l)| l)
}

/// The call a session waits on you to allow, for Jev to judge: its name and the line the windows
/// show (`label`, "Bash · `rm -rf build`"), then all of it (`input`), so a tool's arguments count
/// and two tools given the same arguments still differ. None when the harness shows no more than
/// the label, which the excerpt has already.
pub fn waiting_words(p: &crate::harness::Pending) -> Option<String> {
    let input = p.input.as_deref().map(str::trim).filter(|i| !i.is_empty())?;
    Some(format!("{}\n{input}", p.label.trim()))
}

/// The request for a session's moment. `waiting` is the whole call a session waits on you to
/// allow, when its harness shows it (`waiting_words`), for an approval's risk.
pub fn request(s: &Session, kind: Kind, model: &str, waiting: Option<&str>) -> Value {
    let mut state = Map::new();
    state.insert("session title".into(), json!(jev::clip(&s.title, 200)));
    if let Some(p) = s.first_prompt.as_deref().filter(|p| !p.trim().is_empty()) {
        state.insert("first prompt".into(), json!(jev::clip(p, 400)));
    }
    // The latest prompt is what the last reply answers; the first says what the session is for.
    let latest = s.last_prompt.as_deref().filter(|p| !p.trim().is_empty() && Some(*p) != s.first_prompt.as_deref());
    if let Some(p) = latest {
        state.insert("latest prompt".into(), json!(jev::clip(p, 400)));
    }
    if let Some(t) = s.reply_tail.as_deref().filter(|t| !t.trim().is_empty()) {
        state.insert("end of the agent's last reply".into(), json!(t));
    }
    let questions = match kind {
        Kind::Turn => {
            let asked_for = if latest.is_some() { "`latest prompt` asked for" } else { "the user asked for" };
            let mut q = json!({
                "asks_you": {
                    "type": "noul",
                    "instructions": "Does the end of the agent's last reply ask the user a question, or for a decision, that the user must answer before the agent can go on?",
                    "criteria": {
                        "true": "It asks the user something and waits: a question, a choice between options, or a request for approval or information.",
                        "false": "It reports what was done or found. Offers such as 'let me know if you want more' don't count.",
                    },
                },
                "finished": {
                    "type": "noul",
                    "instructions": format!("Is the task {asked_for} complete, with nothing left for the agent to do?"),
                },
            });
            if landing_said(s).is_none() {
                q["landing"] = json!({
                    "type": "choice",
                    "instructions": "How far did the agent's code change get, going by the end of its last reply?",
                    "criteria": Value::Object(LANDING.iter().map(|(k, v)| (k.to_string(), json!(v))).collect()),
                });
            }
            q
        }
        Kind::Quiet => {
            state.insert("command".into(), json!(command_in(s.excerpt.as_deref().unwrap_or("")).unwrap_or_default()));
            json!({
                "waits_for_input": {
                    "type": "noul",
                    "instructions": "Is `command` waiting for someone to type something (a password, a login, a yes or no, an editor, a pager) rather than doing long-running work?",
                },
            })
        }
        Kind::Background => {
            state.insert("background command".into(), json!(jev::clip(s.background.as_deref().unwrap_or(""), 600)));
            json!({
                "background": {
                    "type": "choice",
                    "instructions": "What is `background command` doing?",
                    "criteria": Value::Object(BACKGROUND.iter().map(|(k, v)| (k.to_string(), json!(v))).collect()),
                },
            })
        }
        Kind::Approval => {
            let excerpt = s.excerpt.as_deref().unwrap_or("");
            let call = waiting.map(str::trim).filter(|w| !w.is_empty()).unwrap_or_else(|| command_in(excerpt).unwrap_or(excerpt));
            state.insert("command waiting for approval".into(), json!(jev::clip(call, WAITING_MOST)));
            json!({
                "risk": {
                    "type": "score",
                    "instructions": "How risky is it to approve `command waiting for approval`?",
                    "criteria": RISK,
                },
            })
        }
    };
    jev::body(model, Value::Object(state), questions)
}

/// Jev's answer as the board keeps it, with how sure it was of its least sure answer.
pub fn read(kind: Kind, response: &Value) -> SessionInsight {
    let mut out = SessionInsight::default();
    let asked: &[&str] = match kind {
        Kind::Turn => {
            out.asks_you = jev::noul(response, "asks_you");
            out.landing = jev::choice(response, "landing").first().map(|(k, _)| k.clone());
            out.finished = jev::noul(response, "finished");
            &["asks_you", "landing", "finished"]
        }
        Kind::Quiet => {
            out.waits_for_input = jev::noul(response, "waits_for_input");
            &["waits_for_input"]
        }
        Kind::Background => {
            let picks = jev::choice(response, "background");
            if let Some((k, p)) = picks.first() {
                out.background = Some(k.clone());
                out.background_p = Some(*p);
            }
            &["background"]
        }
        Kind::Approval => {
            out.risk = jev::score(response, "risk");
            &["risk"]
        }
    };
    out.confidence = asked.iter().filter_map(|id| jev::confidence(response, id)).reduce(f64::min);
    out
}

/// What a session shows of its moment: Jev's read, with the landing its reply's first line says
/// in place of Jev's, and under an agent's tag of it (tags.rs) where the agent said, named by
/// `by`. None with none of the three.
pub fn merged(jev: Option<&SessionInsight>, said: Option<&str>, tag: Option<&SessionInsight>) -> Option<SessionInsight> {
    let mut out = jev.cloned();
    if let Some(l) = said {
        out.get_or_insert_with(Default::default).landing = Some(l.to_string());
    }
    if let Some(t) = tag {
        let o = out.get_or_insert_with(Default::default);
        if t.asks_you.is_some() {
            o.asks_you = t.asks_you;
        }
        if t.landing.is_some() {
            o.landing = t.landing.clone();
        }
        if t.finished.is_some() {
            o.finished = t.finished;
        }
        if t.risk.is_some() {
            o.risk = t.risk;
        }
        if t.waits_for_input.is_some() {
            o.waits_for_input = t.waits_for_input;
        }
        if t.background.is_some() {
            o.background = t.background.clone();
            o.background_p = t.background_p.or(Some(1.0));
        }
        // An agent's word is a judgment, not a probability: Jev's confidence no longer says how
        // sure what's shown is.
        o.confidence = None;
        o.by = t.by.clone();
        o.by_title = t.by_title.clone();
        o.note = t.note.clone();
    }
    out
}

/* ---- Kept answers ---- */

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Kept {
    pub signature: String,
    pub insight: SessionInsight,
    /// When the session was last seen with this answer, to drop old ones.
    pub seen_at: i64,
}

/// Answers by session key, as insights.json keeps them.
#[derive(Default)]
pub struct Store {
    pub kept: HashMap<String, Kept>,
    /// Signatures whose question failed, and when: tried again only after a while.
    pub failed: HashMap<String, i64>,
    dirty: bool,
}

/// How long a failed question waits before it's asked again.
const RETRY_MS: i64 = 5 * 60_000;

fn path() -> PathBuf {
    crate::config::support_dir().join("insights.json")
}

impl Store {
    pub fn load() -> Store {
        // Jev's answers can be asked for again: a file that doesn't read starts over.
        Store { kept: crate::store::read_or_default(&path()), ..Default::default() }
    }

    /// The answer for a session, when it's for the moment the session is in.
    pub fn for_session(&self, key: &str, signature: &str) -> Option<&SessionInsight> {
        self.kept.get(key).filter(|k| k.signature == signature).map(|k| &k.insight)
    }

    /// Whether to ask now: not answered for this moment, and not failed a moment ago.
    pub fn wants(&self, key: &str, signature: &str, now: i64) -> bool {
        self.for_session(key, signature).is_none() && self.failed.get(signature).is_none_or(|at| now - at > RETRY_MS)
    }

    pub fn answered(&mut self, key: String, signature: String, insight: SessionInsight, now: i64) {
        self.failed.remove(&signature);
        self.kept.insert(key, Kept { signature, insight, seen_at: now });
        self.dirty = true;
    }

    pub fn failed(&mut self, signature: String, now: i64) {
        self.failed.insert(signature, now);
    }

    /// Notes that a session with an answer is still around.
    pub fn seen(&mut self, key: &str, now: i64) {
        if let Some(k) = self.kept.get_mut(key) {
            // Written at most once a day per session: this runs on every publish.
            if now - k.seen_at > DAY_MS {
                k.seen_at = now;
                self.dirty = true;
            }
        }
    }

    /// Writes insights.json when something changed, dropping answers for sessions not seen for
    /// two weeks.
    pub fn save(&mut self, now: i64) {
        if !self.dirty {
            return;
        }
        self.kept.retain(|_, k| now - k.seen_at < KEEP_MS);
        self.dirty = false;
        if let Ok(text) = serde_json::to_string(&self.kept) {
            let _ = crate::store::write(&path(), text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Harness;

    const NOW: i64 = 30 * DAY_MS;

    fn session(status: Status) -> Session {
        Session {
            status,
            live: true,
            pid: Some(1),
            title: "Fix the login".into(),
            cwd: "/r".into(),
            reply_tail: Some("…tests pass. Should I also migrate the old table?".into()),
            reply_head: Some("The login works again with the new token check.".into()),
            first_prompt: Some("Fix the login bug".into()),
            last_activity: NOW - 60_000,
            last_prompt_at: NOW - 600_000,
            excerpt: Some("Bash · `rm -rf build`".into()),
            ..Session::new(Harness::Claude, "claude", "abc")
        }
    }

    #[test]
    fn asks_what_the_moment_can_answer() {
        assert_eq!(ask(&session(Status::Unread), NOW).map(|a| a.0), Some(Kind::Turn));
        assert_eq!(ask(&session(Status::Stopped), NOW).map(|a| a.0), Some(Kind::Turn));
        assert_eq!(ask(&session(Status::Needs), NOW).map(|a| a.0), Some(Kind::Approval));
        assert_eq!(ask(&session(Status::Working), NOW), None);
        let mut quiet = session(Status::Needs);
        quiet.stalled_since = Some(NOW - DAY_MS);
        quiet.excerpt = Some("Stuck on `gcloud auth login` for {for:1}".into());
        assert_eq!(ask(&quiet, NOW).map(|a| a.0), Some(Kind::Quiet));
        let mut bg = session(Status::Working);
        bg.background = Some("while true; do sleep 60; gh pr view 1; done".into());
        assert_eq!(ask(&bg, NOW).map(|a| a.0), Some(Kind::Background));
        bg.background = Some(String::new());
        assert_eq!(ask(&bg, NOW), None);
    }

    #[test]
    fn leaves_old_archived_visiting_and_cut_short_sessions_alone() {
        let mut old = session(Status::Idle);
        old.last_activity = NOW - 3 * DAY_MS;
        assert_eq!(ask(&old, NOW), None);
        let mut archived = session(Status::Unread);
        archived.archived = true;
        assert_eq!(ask(&archived, NOW), None);
        let mut no_reply = session(Status::Unread);
        no_reply.reply_tail = None;
        assert_eq!(ask(&no_reply, NOW), None);
        // A helper's turn is its agent's to read; what it waits on you to allow is still asked.
        let helper = |status| Session { helper: Some(crate::model::Helper { parent: Some("claude:claude:boss".into()), pr: None, report: false }), ..session(status) };
        assert_eq!(ask(&helper(Status::Unread), NOW), None);
        assert_eq!(ask(&helper(Status::Needs), NOW).map(|a| a.0), Some(Kind::Approval));
        // A turn its harness ended has an error for a reply, which would read as blocked.
        let mut lost = session(Status::Unread);
        lost.connection_lost = Some(crate::model::ConnectionLost { at: NOW - 1000, continues: false });
        assert_eq!(ask(&lost, NOW), None);
        let mut cut = session(Status::Stopped);
        cut.cut_off = Some(NOW - 1000);
        assert_eq!(ask(&cut, NOW), None);
    }

    #[test]
    fn a_new_reply_is_a_new_moment_and_seeing_it_isnt() {
        let a = ask(&session(Status::Unread), NOW).unwrap().1;
        assert_eq!(ask(&session(Status::Idle), NOW).unwrap().1, a);
        // Read as asking you, the session Needs you with the question for an excerpt; asked
        // about again from the published board, it's still that turn, not an approval.
        let mut flipped = session(Status::Unread);
        flipped.reply_head = Some("Pushed: 1a2b3c4 is in origin/main.".into());
        crate::board::apply_insight(&mut flipped, &SessionInsight { asks_you: Some(0.9), ..Default::default() });
        assert_eq!((flipped.status, flipped.asks), (Status::Needs, true));
        assert_eq!(ask(&flipped, NOW).unwrap().1, a);
        assert_eq!(landing_said(&flipped), Some("pushed"));
        let mut next = session(Status::Unread);
        next.reply_tail = Some("Done.".into());
        assert_ne!(ask(&next, NOW).unwrap().1, a);
        // The same reply to another prompt is another moment: whether it's finished depends on
        // what was asked.
        let mut again = session(Status::Unread);
        again.last_prompt = Some("Now the docs too".into());
        assert_ne!(ask(&again, NOW).unwrap().1, a);
        // Each call a session waits on is a moment of its own, by when it began waiting: the
        // same line shown for another call (the same tool, other arguments) isn't the same
        // moment, so the risk read for one, from the whole call, isn't kept for the other.
        let mut first = session(Status::Needs);
        first.status_since = Some(NOW - 50_000);
        let mut second = first.clone();
        second.status_since = Some(NOW - 10_000);
        let (p, q) = (ask(&first, NOW).unwrap().1, ask(&second, NOW).unwrap().1);
        assert_ne!(p, q);
        assert_eq!(ask(&first.clone(), NOW).unwrap().1, p, "the same call, seen again");
        assert_eq!(kind_of(&p), Some(Kind::Approval));
        assert_eq!(kind_of(&a), Some(Kind::Turn));
    }

    #[test]
    fn builds_one_request_per_moment() {
        let turn = request(&session(Status::Unread), Kind::Turn, "jev-latest", None);
        assert_eq!(turn["model"], "jev-latest");
        assert_eq!(turn["state"]["end of the agent's last reply"], "…tests pass. Should I also migrate the old table?");
        assert_eq!(turn["state"]["first prompt"], "Fix the login bug");
        assert!(turn["state"].get("latest prompt").is_none(), "none without one, and not the first again");
        assert_eq!(turn["questions"]["asks_you"]["type"], "noul");
        assert_eq!(turn["questions"]["landing"]["criteria"].as_object().unwrap().len(), 6);
        assert_eq!(turn["questions"]["finished"]["instructions"], "Is the task the user asked for complete, with nothing left for the agent to do?");
        let approval = request(&session(Status::Needs), Kind::Approval, "m", None);
        assert_eq!(approval["state"]["command waiting for approval"], "rm -rf build");
        assert_eq!(approval["questions"]["risk"]["criteria"].as_array().unwrap().len(), 4);
    }

    /// The last reply answers the latest prompt, so finished is asked about that one; the first
    /// prompt stays for what the session is about.
    #[test]
    fn asks_whether_the_latest_prompt_is_done() {
        let mut s = session(Status::Unread);
        s.last_prompt = Some("Now also migrate the old table".into());
        let turn = request(&s, Kind::Turn, "m", None);
        assert_eq!(turn["state"]["latest prompt"], "Now also migrate the old table");
        assert_eq!(turn["state"]["first prompt"], "Fix the login bug");
        assert_eq!(turn["questions"]["finished"]["instructions"], "Is the task `latest prompt` asked for complete, with nothing left for the agent to do?");
        s.last_prompt = s.first_prompt.clone();
        assert!(request(&s, Kind::Turn, "m", None)["state"].get("latest prompt").is_none());
    }

    /// A reply that opens with the status line CLAUDE.md asks for says its landing itself: Jev
    /// isn't asked it, and the line's word is what shows.
    #[test]
    fn a_status_line_says_the_landing() {
        let said = |head: &str| {
            let mut s = session(Status::Unread);
            s.reply_head = Some(head.into());
            landing_said(&s)
        };
        assert_eq!(said("Landed: 1a2b3c4 is in origin/main and the app is relaunched."), Some("landed"));
        assert_eq!(said("**Pushed:** 1a2b3c4 is in origin/main; the relaunch waits."), Some("pushed"));
        assert_eq!(said("Committed: fix-x in ../codealong-fix-x, not pushed; review pending."), Some("committed"));
        assert_eq!(said("Not committed: edits in ../codealong-x; tests fail."), Some("uncommitted"));
        assert_eq!(said("Committed changes are listed below."), None, "the word, then the colon");
        assert_eq!(said("The login works again."), None);
        let mut s = session(Status::Unread);
        s.reply_head = Some("Landed: 1a2b3c4 is in origin/main.".into());
        assert!(request(&s, Kind::Turn, "m", None)["questions"].get("landing").is_none());
        assert!(request(&s, Kind::Turn, "m", None)["questions"].get("asks_you").is_some());
        s.status = Status::Working;
        assert_eq!(landing_said(&s), None, "while it works, the line is the turn before's");
    }

    /// The whole call goes in when the harness shows it, with its name, so the arguments count,
    /// not just the line the excerpt has.
    #[test]
    fn an_approval_is_judged_by_the_whole_call() {
        let s = session(Status::Needs);
        let pending = |label: &str, input: Option<&str>| crate::harness::Pending { id: "toolu_1".into(), label: label.into(), input: input.map(String::from), cannot: None, always: false, no_once: false, plan: false, message: false, always_rule: None, questions: Vec::new(), unwritten: false };
        let waiting = waiting_words(&pending("Bash · `rm -rf build`", Some("rm -rf build\ngit push --force origin main"))).unwrap();
        assert_eq!(waiting, "Bash · `rm -rf build`\nrm -rf build\ngit push --force origin main");
        assert_eq!(waiting_words(&pending("mcp__db__drop · `users`", Some("{\"table\": \"users\"}"))).unwrap(), "mcp__db__drop · `users`\n{\"table\": \"users\"}", "the tool's name stays with its arguments");
        assert_eq!(waiting_words(&pending("Bash · `ls`", None)), None, "no more than the excerpt has");
        assert_eq!(waiting_words(&pending("Bash · `ls`", Some("  "))), None);
        let approval = request(&s, Kind::Approval, "m", Some(&waiting));
        assert_eq!(approval["state"]["command waiting for approval"], "Bash · `rm -rf build` rm -rf build git push --force origin main", "on one line, as Jev reads text");
        assert_eq!(request(&s, Kind::Approval, "m", None)["state"]["command waiting for approval"], "rm -rf build");
    }

    #[test]
    fn reads_answers_back() {
        let v = json!({ "answers": {
            "asks_you": { "type": "noul", "noul": 0.92, "confidence": 0.9 },
            "landing": { "type": "choice", "probabilities": { "pushed": 0.7, "committed": 0.2, "none": 0.1 }, "confidence": 0.6 },
            "finished": { "type": "noul", "noul": 0.3 },
        }});
        let i = read(Kind::Turn, &v);
        assert_eq!((i.asks_you, i.landing.as_deref(), i.finished), (Some(0.92), Some("pushed"), Some(0.3)));
        assert_eq!(i.confidence, Some(0.6), "the least sure answer");
        let bg = read(Kind::Background, &json!({ "answers": { "background": { "probabilities": { "polling": 0.8, "job": 0.15, "stalled": 0.05 } } } }));
        assert_eq!((bg.background.as_deref(), bg.background_p, bg.confidence), (Some("polling"), Some(0.8), None));
        assert_eq!(read(Kind::Approval, &json!({ "answers": { "risk": { "score": 2.6 } } })).risk, Some(2.6));
    }

    /// An agent's tag stands over Jev's read where the agent said, and the status line over
    /// Jev's landing; what nobody said stays Jev's.
    #[test]
    fn an_agents_tag_stands_over_jevs_read() {
        let jev = SessionInsight { asks_you: Some(0.2), landing: Some("committed".into()), finished: Some(0.5), confidence: Some(0.7), ..Default::default() };
        let tag = SessionInsight { finished: Some(1.0), landing: Some("pushed".into()), by: Some("claude:claude:boss".into()), by_title: Some("Orchestrator".into()), note: Some("PR #12 is up".into()), ..Default::default() };
        let m = merged(Some(&jev), Some("landed"), Some(&tag)).unwrap();
        assert_eq!((m.asks_you, m.landing.as_deref(), m.finished), (Some(0.2), Some("pushed"), Some(1.0)));
        assert_eq!((m.by.as_deref(), m.by_title.as_deref(), m.note.as_deref(), m.confidence), (Some("claude:claude:boss"), Some("Orchestrator"), Some("PR #12 is up"), None));
        assert_eq!(merged(Some(&jev), Some("landed"), None).unwrap().landing.as_deref(), Some("landed"));
        assert_eq!(merged(None, None, Some(&tag)).unwrap().finished, Some(1.0));
        let bg = SessionInsight { background: Some("server".into()), ..Default::default() };
        assert_eq!(merged(None, None, Some(&bg)).unwrap().background_p, Some(1.0), "an agent's word is sure");
        assert_eq!(merged(None, None, None), None);
    }

    #[test]
    fn keeps_one_answer_per_session_and_waits_after_a_failure() {
        let mut store = Store::default();
        assert!(store.wants("k", "turn-1", NOW));
        store.failed("turn-1".into(), NOW);
        assert!(!store.wants("k", "turn-1", NOW + 60_000));
        assert!(store.wants("k", "turn-1", NOW + RETRY_MS + 1));
        store.answered("k".into(), "turn-1".into(), SessionInsight { finished: Some(1.0), ..Default::default() }, NOW);
        assert!(!store.wants("k", "turn-1", NOW));
        assert!(store.for_session("k", "turn-2").is_none());
        assert!(store.wants("k", "turn-2", NOW));
    }
}

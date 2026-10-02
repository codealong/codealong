//! Jev's read of each session (`[jev] sessions`): one request per session per turn, asking only
//! what that moment can answer, all at once (docs.typesafe.ai, speculative fan-out).
//!
//! - A turn that ended: does the reply end by asking you something, how far did the work get
//!   (landed, pushed, committed…), and is what you asked for done.
//! - A busy Claude session gone quiet on a command: is the command waiting for typing, or is it
//!   long work.
//! - A Claude session whose turn left a command running in the background: is it polling for
//!   something outside, one long job, or stuck.
//! - A session waiting on your approval: how risky the command looks.
//!
//! What's asked is decided by `ask`, from the session as the board built it; the answer is kept
//! under that moment's signature, so a session is asked once per turn, also across restarts
//! (insights.json). `board::apply_insight` turns an answer into what the board shows.

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

/// What to ask about a session now, and the signature that moment is kept under: `<kind>-<hash>`
/// of what the answer depends on. None when there's nothing to ask.
pub fn ask(s: &Session, now: i64) -> Option<(Kind, String)> {
    if s.archived || s.visitor.is_some() {
        return None;
    }
    let (kind, basis): (Kind, Vec<&str>) = if let Some(cmd) = s.background.as_deref() {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return None;
        }
        (Kind::Background, vec![cmd, s.reply_tail.as_deref().unwrap_or("")])
    } else if s.stalled_since.is_some() {
        (Kind::Quiet, vec![command_in(s.excerpt.as_deref()?)?])
    } else if s.status == Status::Needs {
        let excerpt = s.excerpt.as_deref()?;
        (Kind::Approval, vec![command_in(excerpt).unwrap_or(excerpt)])
    } else if matches!(s.status, Status::Unread | Status::Idle | Status::Stopped) {
        if now - s.last_activity > RECENT_MS {
            return None;
        }
        (Kind::Turn, vec![s.reply_tail.as_deref().filter(|t| !t.trim().is_empty())?])
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

const LANDING: [(&str, &str); 6] = [
    ("landed", "The change is merged, or pushed straight to the main branch."),
    ("pushed", "The change is pushed to a branch or a pull request that isn't merged yet."),
    ("committed", "The change is committed but not pushed."),
    ("uncommitted", "Files were changed but nothing is committed."),
    ("blocked", "The agent couldn't finish: something failed, or it's waiting on something else."),
    ("none", "No code change: an answer, research, a review, or a question."),
];

const RISK: [&str; 4] = [
    "Only reads or inspects; changes nothing",
    "Changes files or state on this computer only",
    "Affects shared or remote things: pushes, deploys, publishes, sends messages, changes cloud resources",
    "Destructive or hard to undo: deletes data, force-pushes, drops tables, rewrites history",
];

/// The request for a session's moment.
pub fn request(s: &Session, kind: Kind, model: &str) -> Value {
    let mut state = Map::new();
    state.insert("session title".into(), json!(jev::clip(&s.title, 200)));
    if let Some(p) = s.first_prompt.as_deref().filter(|p| !p.trim().is_empty()) {
        state.insert("first prompt".into(), json!(jev::clip(p, 400)));
    }
    if let Some(t) = s.reply_tail.as_deref().filter(|t| !t.trim().is_empty()) {
        state.insert("end of the agent's last reply".into(), json!(t));
    }
    let questions = match kind {
        Kind::Turn => json!({
            "asks_you": {
                "type": "noul",
                "instructions": "Does the end of the agent's last reply ask the user a question, or for a decision, that the user must answer before the agent can go on?",
                "criteria": {
                    "true": "It asks the user something and waits: a question, a choice between options, or a request for approval or information.",
                    "false": "It reports what was done or found. Offers such as 'let me know if you want more' don't count.",
                },
            },
            "landing": {
                "type": "choice",
                "instructions": "How far did the agent's code change get, going by the end of its last reply?",
                "criteria": Value::Object(LANDING.iter().map(|(k, v)| (k.to_string(), json!(v))).collect()),
            },
            "finished": {
                "type": "noul",
                "instructions": "Is the task the user asked for complete, with nothing left for the agent to do?",
            },
        }),
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
                    "criteria": {
                        "polling": "Checks again and again for something outside to happen: a review or approval, CI, a deploy, a file or URL appearing.",
                        "job": "Runs one long piece of work that ends by itself: a build, tests, a download, a migration.",
                        "server": "Keeps running until someone stops it, and nothing waits for it to end: a dev server, a file watcher, `tail -f`, a tunnel.",
                        "stalled": "Looks stuck, or waits on something that won't happen without the user: a sign-in in a browser, a password, a question on the terminal.",
                    },
                },
            })
        }
        Kind::Approval => {
            let excerpt = s.excerpt.as_deref().unwrap_or("");
            state.insert("command waiting for approval".into(), json!(jev::clip(command_in(excerpt).unwrap_or(excerpt), 600)));
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

/// Jev's answer as the board keeps it.
pub fn read(kind: Kind, response: &Value) -> SessionInsight {
    let mut out = SessionInsight::default();
    match kind {
        Kind::Turn => {
            out.asks_you = jev::noul(response, "asks_you");
            out.landing = jev::choice(response, "landing").first().map(|(k, _)| k.clone());
            out.finished = jev::noul(response, "finished");
        }
        Kind::Quiet => out.waits_for_input = jev::noul(response, "waits_for_input"),
        Kind::Background => {
            let picks = jev::choice(response, "background");
            if let Some((k, p)) = picks.first() {
                out.background = Some(k.clone());
                out.background_p = Some(*p);
            }
        }
        Kind::Approval => out.risk = jev::score(response, "risk"),
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
    fn leaves_old_archived_and_visiting_sessions_alone() {
        let mut old = session(Status::Idle);
        old.last_activity = NOW - 3 * DAY_MS;
        assert_eq!(ask(&old, NOW), None);
        let mut archived = session(Status::Unread);
        archived.archived = true;
        assert_eq!(ask(&archived, NOW), None);
        let mut no_reply = session(Status::Unread);
        no_reply.reply_tail = None;
        assert_eq!(ask(&no_reply, NOW), None);
    }

    #[test]
    fn a_new_reply_is_a_new_moment_and_seeing_it_isnt() {
        let a = ask(&session(Status::Unread), NOW).unwrap().1;
        assert_eq!(ask(&session(Status::Idle), NOW).unwrap().1, a);
        let mut next = session(Status::Unread);
        next.reply_tail = Some("Done.".into());
        assert_ne!(ask(&next, NOW).unwrap().1, a);
        assert_eq!(kind_of(&a), Some(Kind::Turn));
    }

    #[test]
    fn builds_one_request_per_moment() {
        let turn = request(&session(Status::Unread), Kind::Turn, "jev-latest");
        assert_eq!(turn["model"], "jev-latest");
        assert_eq!(turn["state"]["end of the agent's last reply"], "…tests pass. Should I also migrate the old table?");
        assert_eq!(turn["state"]["first prompt"], "Fix the login bug");
        assert_eq!(turn["questions"]["asks_you"]["type"], "noul");
        assert_eq!(turn["questions"]["landing"]["criteria"].as_object().unwrap().len(), 6);
        assert_eq!(turn["questions"]["finished"]["type"], "noul");
        let approval = request(&session(Status::Needs), Kind::Approval, "m");
        assert_eq!(approval["state"]["command waiting for approval"], "rm -rf build");
        assert_eq!(approval["questions"]["risk"]["criteria"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn reads_answers_back() {
        let v = json!({ "answers": {
            "asks_you": { "type": "noul", "noul": 0.92 },
            "landing": { "type": "choice", "probabilities": { "pushed": 0.7, "committed": 0.2, "none": 0.1 } },
            "finished": { "type": "noul", "noul": 0.3 },
        }});
        let i = read(Kind::Turn, &v);
        assert_eq!((i.asks_you, i.landing.as_deref(), i.finished), (Some(0.92), Some("pushed"), Some(0.3)));
        let bg = read(Kind::Background, &json!({ "answers": { "background": { "probabilities": { "polling": 0.8, "job": 0.15, "stalled": 0.05 } } } }));
        assert_eq!((bg.background.as_deref(), bg.background_p), (Some("polling"), Some(0.8)));
        assert_eq!(read(Kind::Approval, &json!({ "answers": { "risk": { "score": 2.6 } } })).risk, Some(2.6));
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

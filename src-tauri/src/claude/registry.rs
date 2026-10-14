//! Claude Code's live sessions: the registry (`<home>/sessions/<pid>.json`), and what the
//! account's background service says of its jobs (`<home>/daemon/roster.json`,
//! `<home>/jobs/<job>/state.json`).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::model::Session;
use crate::procs;
use crate::util::mtime_ms;

/// What a session that says it's `waiting` waits on (`waitingFor`): its permission prompt.
pub const PERMISSION_PROMPT: &str = "permission prompt";
/// Or its questions (`AskUserQuestion`).
pub const INPUT_NEEDED: &str = "input needed";

/// A process's entry in the registry, `<home>/sessions/<pid>.json`: what a running Claude Code
/// says of itself. Everything that reads one reads it as this (`registry_of`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegistryEntry {
    pub pid: u32,
    pub session_id: String,
    pub cwd: String,
    /// `busy`, `idle`, `shell`, `waiting`, `blocked`…
    pub status: String,
    /// What a `waiting` session waits on (`PERMISSION_PROMPT`, `INPUT_NEEDED`).
    pub waiting_for: Option<String>,
    pub kind: Option<String>,
    /// Set on a background job (`kind: "bg"`): what `claude attach` takes.
    pub job_id: Option<String>,
    /// Set on a window whose conversation moved to that background job: until you go back to
    /// it, the window shows Claude Code's list of agents and runs no session.
    pub parked_job: Option<String>,
    /// A background session the account's service started ahead of time, so the next
    /// background start takes it over at once. It has no conversation until then, and loses
    /// the mark when its first turn starts.
    pub spare: bool,
    pub entrypoint: Option<String>,
    pub name: Option<String>,
    pub name_source: Option<String>,
    pub started_at: Option<i64>,
    pub status_updated_at: Option<i64>,
    /// For a background job that goes on from another session's conversation (a fork, or a
    /// window's `/background`), that session's id, from the service's roster.
    pub resumes: Option<String>,
    /// The socket it listens on for other sessions' messages, and the protocol it speaks there.
    pub messaging_socket: Option<String>,
    pub peer_protocol: Option<u64>,
    /// The Claude app's own id for a session it runs (`local_…`), which names the app's record
    /// of it.
    pub host_session: Option<String>,
}

impl RegistryEntry {
    /// Whether the session has a prompt of this kind up (`requires_action`): it says it waits,
    /// and on that.
    pub fn waits_on(&self, what: &str) -> bool {
        self.status == "waiting" && self.waiting_for.as_deref() == Some(what)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRegistry {
    pid: u32,
    session_id: Option<String>,
    cwd: Option<String>,
    status: Option<String>,
    kind: Option<String>,
    job_id: Option<String>,
    parked_job_id: Option<String>,
    #[serde(default)]
    spare: bool,
    entrypoint: Option<String>,
    name: Option<String>,
    name_source: Option<String>,
    started_at: Option<i64>,
    status_updated_at: Option<i64>,
    // Read as they come: one of these in a shape Codealong doesn't know mustn't cost the
    // board the session.
    waiting_for: Option<Value>,
    messaging_socket_path: Option<Value>,
    peer_protocol: Option<Value>,
    host_session_id: Option<Value>,
}

pub fn parse_registry(text: &str) -> Option<RegistryEntry> {
    let raw: RawRegistry = serde_json::from_str(text).ok()?;
    let text_of = |v: Option<Value>| v.as_ref().and_then(Value::as_str).map(String::from);
    Some(RegistryEntry {
        pid: raw.pid,
        session_id: raw.session_id?,
        cwd: raw.cwd?,
        status: raw.status.unwrap_or_else(|| "idle".into()),
        waiting_for: text_of(raw.waiting_for),
        kind: raw.kind,
        job_id: raw.job_id,
        parked_job: raw.parked_job_id,
        spare: raw.spare,
        entrypoint: raw.entrypoint,
        name: raw.name,
        name_source: raw.name_source,
        started_at: raw.started_at,
        status_updated_at: raw.status_updated_at,
        resumes: None,
        messaging_socket: text_of(raw.messaging_socket_path),
        peer_protocol: raw.peer_protocol.as_ref().and_then(Value::as_u64),
        host_session: text_of(raw.host_session_id),
    })
}

/// The entry of the process `pid`, whatever it says of itself: a window whose conversation
/// moved away, a spare and a side conversation's copy have one too, and only `read_registry`
/// leaves them out.
pub fn registry_of(home: &Path, pid: u32) -> Option<RegistryEntry> {
    parse_registry(&std::fs::read_to_string(home.join("sessions").join(format!("{pid}.json"))).ok()?)
}

/// A live session's entry, while its process is still that session's.
pub fn session_entry(home: &Path, session: &Session) -> Option<RegistryEntry> {
    registry_of(home, session.pid?).filter(|r| r.session_id == session.id)
}

/// The conversation a background job goes on from, from the service's roster
/// (`<home>/daemon/roster.json`): `workers.<job>.dispatch.launch` says `mode: "resume"` and
/// names the session, by its id (`claude --bg --resume <id>`, with `--fork-session` for a copy)
/// or by its transcript's path (`/background`). None for a job that started afresh, and for
/// one that goes on as the session itself.
pub fn resumed_from(roster: &Value, job: &str) -> Option<String> {
    let worker = roster.get("workers")?.get(job)?;
    let launch = worker.pointer("/dispatch/launch")?;
    if launch.get("mode")?.as_str()? != "resume" {
        return None;
    }
    let from = Path::new(launch.get("sessionId")?.as_str()?).file_stem()?.to_str()?.to_string();
    let own = worker.get("sessionId").and_then(Value::as_str);
    (!from.is_empty() && own != Some(from.as_str())).then_some(from)
}

/// A record newer than this may not name what it forked from yet, so it's read again.
const JOB_SETTLES_MS: i64 = 60_000;

/// What background jobs were forked from, from their records (`<home>/jobs/<job>/state.json`):
/// `forkParentSessionId` names the session whose conversation a job copied, for `/fork`,
/// `/background` and `claude --bg --resume <id> --fork-session`. A record names it as the job
/// starts and never changes it, and the records stay after a job ends, so each is read once.
#[derive(Default, Debug)]
pub struct Forks {
    /// Job folders read, so they aren't read again.
    read: HashSet<String>,
    /// By the job's session id, the one it was forked from.
    parents: HashMap<String, String>,
}

impl Forks {
    /// Reads the records of jobs not read yet. Returns whether it learned of a fork.
    pub fn refresh(&mut self, home: &Path, now: i64) -> bool {
        let Ok(entries) = std::fs::read_dir(home.join("jobs")) else { return false };
        let mut changed = false;
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if self.read.contains(&name) {
                continue;
            }
            let path = e.path().join("state.json");
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let Some((id, parent)) = parse_job_state(&bytes) else { continue };
            match parent {
                Some(parent) => {
                    self.parents.insert(id, parent);
                    changed = true;
                }
                None if now - mtime_ms(&path).unwrap_or(0) < JOB_SETTLES_MS => continue,
                None => {}
            }
            self.read.insert(name);
        }
        changed
    }

    /// The session a job's session was forked from.
    pub fn parent_of(&self, id: &str) -> Option<&str> {
        self.parents.get(id).map(String::as_str)
    }
}

/// A job's session and the session it was forked from, from its record.
pub fn parse_job_state(bytes: &[u8]) -> Option<(String, Option<String>)> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Raw {
        session_id: Option<String>,
        fork_parent_session_id: Option<String>,
    }
    let raw: Raw = serde_json::from_slice(bytes).ok()?;
    let id = raw.session_id.filter(|id| !id.is_empty())?;
    let parent = raw.fork_parent_session_id.filter(|p| !p.is_empty() && p != &id);
    Some((id, parent))
}

/// Live sessions for one account. Entries whose process is gone, or whose pid now belongs
/// to a newer process, are dropped, and so are windows whose conversation moved to the
/// background: the job it moved to runs it now (`board::claude_moves`). So are spares, which
/// Claude Code's own lookups skip too: nothing has happened in one yet. And so are the copies
/// side conversations run (side.rs): a headless fork that keeps no record still lists itself
/// here while it runs (`kind` interactive, `entrypoint` sdk-cli), and it's no session; nor is
/// the run that lists an account's built-in skills (claude/builtin.rs), known by its folder.
pub fn read_registry(home: &Path) -> Vec<RegistryEntry> {
    let Ok(entries) = std::fs::read_dir(home.join("sessions")) else { return vec![] };
    let mut out = Vec::new();
    // Read once, and only when there's a job to look up.
    let mut roster: Option<Value> = None;
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Some(mut entry) = parse_registry(&text) else { continue };
        if entry.parked_job.is_some() || entry.spare || !procs::pid_alive(entry.pid) || crate::side::runs(entry.pid) || super::builtin::runs_in(&entry.cwd) {
            continue;
        }
        if let (Some(proc_start), Some(started)) = (procs::proc_start_ms(entry.pid), entry.started_at) {
            if proc_start > started + 60_000 {
                continue;
            }
        }
        if let Some(job) = entry.job_id.as_deref() {
            let roster = roster.get_or_insert_with(|| std::fs::read(home.join("daemon/roster.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null));
            entry.resumes = resumed_from(roster, job);
        }
        out.push(entry);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_registry_entries() {
        let e = parse_registry(r#"{"pid":22303,"sessionId":"9f62","cwd":"/x","status":"busy","kind":"interactive","entrypoint":"cli","name":"spark-app4-4b","nameSource":"derived","startedAt":1,"statusUpdatedAt":2}"#).unwrap();
        assert_eq!(e.status, "busy");
        assert_eq!(e.name.as_deref(), Some("spark-app4-4b"));
        assert_eq!(e.parked_job, None);
        assert!(!e.spare);
        assert!(parse_registry(r#"{"pid":1}"#).is_none());
    }

    #[test]
    fn reads_the_conversation_a_background_job_goes_on_from() {
        let roster = serde_json::json!({"workers": {
            "37620e2b": {"sessionId": "37620e2b-35d0-47b2-8e04-9a55468b75fe", "dispatch": {"launch": {"mode": "resume", "sessionId": "3628c9e6-0309-4a30-aed9-3e68d0e66b7e", "fork": true}}},
            "5a1b2c3d": {"sessionId": "5a1b2c3d-0000-4000-8000-000000000000", "dispatch": {"source": "slash", "launch": {"mode": "resume", "fork": true, "sessionId": "/Users/me/.claude/projects/-r-x/9f62aa00-0000-4000-8000-000000000000.jsonl"}}},
            "77777777": {"sessionId": "77777777-0000-4000-8000-000000000000", "dispatch": {"launch": {"mode": "resume", "sessionId": "77777777-0000-4000-8000-000000000000"}}},
            "d3d3cd21": {"sessionId": "d3d3cd21-0478-4ac5-a902-a215a327e140", "dispatch": {"launch": {"mode": "prompt"}}}
        }});
        // A fork names the session it copies; a window's /background, its transcript.
        assert_eq!(resumed_from(&roster, "37620e2b").as_deref(), Some("3628c9e6-0309-4a30-aed9-3e68d0e66b7e"));
        assert_eq!(resumed_from(&roster, "5a1b2c3d").as_deref(), Some("9f62aa00-0000-4000-8000-000000000000"));
        // Going on as the session itself, started afresh, or not in the roster.
        assert_eq!(resumed_from(&roster, "77777777"), None);
        assert_eq!(resumed_from(&roster, "d3d3cd21"), None);
        assert_eq!(resumed_from(&roster, "00000000"), None);
    }

    #[test]
    fn reads_what_each_job_was_forked_from_once() {
        let home = crate::fixtures::scratch("job-forks");
        let job = |name: &str, state: &str| {
            std::fs::create_dir_all(home.join("jobs").join(name)).unwrap();
            std::fs::write(home.join("jobs").join(name).join("state.json"), state).unwrap();
        };
        job("fdfea444", r#"{"state":"done","sessionId":"fdfea444-5c75","forkSessionId":"fdfea444-5c75","forkParentSessionId":"b887d77f-c07e","intent":"x"}"#);
        job("f905c97d", r#"{"state":"stopped","sessionId":"f905c97d-d6fc","intent":""}"#);
        job("starting", r#"{"state":"starting"}"#);
        std::fs::write(home.join("jobs").join("pins.json"), "{}").unwrap();
        let later = crate::util::now_ms() + JOB_SETTLES_MS + 1;
        let mut forks = Forks::default();
        assert!(forks.refresh(&home, later));
        assert_eq!(forks.parent_of("fdfea444-5c75"), Some("b887d77f-c07e"));
        assert_eq!(forks.parent_of("f905c97d-d6fc"), None);
        assert_eq!(forks.parent_of("b887d77f-c07e"), None);
        // Read once: a record that changes afterwards isn't read again.
        job("fdfea444", r#"{"sessionId":"fdfea444-5c75","forkParentSessionId":"someone-else"}"#);
        assert!(!forks.refresh(&home, later));
        assert_eq!(forks.parent_of("fdfea444-5c75"), Some("b887d77f-c07e"));
        // One without a session yet, or just written without a parent, is read again later.
        job("starting", r#"{"sessionId":"aaaa","forkParentSessionId":"bbbb"}"#);
        job("fresh", r#"{"sessionId":"cccc"}"#);
        assert!(forks.refresh(&home, crate::util::now_ms()));
        assert_eq!(forks.parent_of("aaaa"), Some("bbbb"));
        job("fresh", r#"{"sessionId":"cccc","forkParentSessionId":"dddd"}"#);
        assert!(forks.refresh(&home, crate::util::now_ms()));
        assert_eq!(forks.parent_of("cccc"), Some("dddd"));
    }

    #[test]
    fn parked_windows_and_spares_run_no_session() {
        let home = crate::fixtures::scratch("parked");
        std::fs::create_dir_all(home.join("sessions")).unwrap();
        let (pid, now) = (std::process::id(), crate::util::now_ms());
        let entry = |id: &str, extra: &str| format!(r#"{{"pid":{pid},"sessionId":"{id}","cwd":"/x","status":"idle","kind":"interactive","startedAt":{now}{extra}}}"#);
        std::fs::write(home.join("sessions/1.json"), entry("b664dd38", r#","parkedJobId":"398f3533""#)).unwrap();
        std::fs::write(home.join("sessions/2.json"), entry("c1456f74", "")).unwrap();
        // A spare, as 2.1.286 registers one: named its job, idle, no transcript.
        let spare = r#","jobId":"acdfc0fb","name":"acdfc0fb","spare":true"#;
        std::fs::write(home.join("sessions/3.json"), entry("acdfc0fb-8aa7-4e09-b822-056b058f5ca5", spare)).unwrap();
        assert_eq!(parse_registry(&entry("b664dd38", r#","parkedJobId":"398f3533""#)).unwrap().parked_job.as_deref(), Some("398f3533"));
        assert!(parse_registry(&entry("acdfc0fb-8aa7-4e09-b822-056b058f5ca5", spare)).unwrap().spare);
        let live: Vec<String> = read_registry(&home).into_iter().map(|e| e.session_id).collect();
        assert_eq!(live, vec!["c1456f74"]);
    }
}

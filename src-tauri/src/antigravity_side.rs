//! Antigravity side conversations (`Adapter::side`): a fresh headless `agy -p` stream-json run
//! seeded with the session's transcript, since `agy` (1.5.13) has no native ephemeral fork flag
//! or `session/fork` in ACP. Each throwaway run still writes a row to `conversation_summaries.db`,
//! so its `conversation_id` (from the `init` event) is recorded here and persisted to
//! `antigravity-sides.json` to keep it off the board, out of search and out of workspace CWDs,
//! while `side::runs(pid)` hides its process while it runs.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::config::AccountCfg;
use crate::model::{ModeKind, Session, SessionMode};
use crate::side::{Dialect, Event, Recipe};
use crate::util::{code, file_name, one_line};

/// How much of the end of a session's transcript to read when seeding a side copy.
const TRANSCRIPT_READ_BYTES: u64 = 512 * 1024;

/// Maximum bytes of inline transcript context sent with the side copy's first question.
/// Longer conversations keep their newest turns inline and point the copy at the full file.
const CONTEXT_BUDGET: usize = 48 * 1024;

/// Maximum throwaway conversation IDs kept in `antigravity-sides.json`.
const MAX_HIDDEN: usize = 500;

/// What `antigravity-sides.json` holds, for store errors.
const HIDDEN_WHAT: &str = "a list of hidden Antigravity side conversations";

#[derive(Default)]
struct HiddenStore {
    loaded: bool,
    ids: Vec<String>,
    set: HashSet<String>,
}

fn hidden_state() -> std::sync::MutexGuard<'static, HiddenStore> {
    static HIDDEN: OnceLock<Mutex<HiddenStore>> = OnceLock::new();
    HIDDEN.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

fn hidden_path() -> PathBuf {
    crate::config::support_dir().join("antigravity-sides.json")
}

/// Whether `id` is a throwaway Antigravity conversation created for a Codealong side copy.
pub fn is_hidden(id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    let mut st = hidden_state();
    if !st.loaded && crate::side::keeps() {
        let ids: Vec<String> = crate::store::read_or_default(&hidden_path());
        st.set = ids.iter().cloned().collect();
        st.ids = ids;
        st.loaded = true;
    }
    st.set.contains(id)
}

/// Marks `id` as a throwaway side conversation immediately in memory and persists it off-thread
/// when the server is keeping side conversations (`side::keeps()`).
pub fn hide(id: &str) {
    let id = id.trim();
    if id.is_empty() {
        return;
    }
    let is_new = {
        let mut st = hidden_state();
        if !st.loaded && crate::side::keeps() {
            let ids: Vec<String> = crate::store::read_or_default(&hidden_path());
            st.set = ids.iter().cloned().collect();
            st.ids = ids;
            st.loaded = true;
        }
        if st.set.insert(id.to_string()) {
            st.ids.push(id.to_string());
            if st.ids.len() > MAX_HIDDEN {
                let extra = st.ids.len() - MAX_HIDDEN;
                let removed: Vec<String> = st.ids.drain(..extra).collect();
                for old in removed {
                    st.set.remove(&old);
                }
            }
            true
        } else {
            false
        }
    };
    if is_new && crate::side::keeps() {
        let id = id.to_string();
        std::thread::spawn(move || {
            let path = hidden_path();
            let _ = crate::store::noted(&path, hide_in(&path, &id));
        });
    }
}

fn hide_in(path: &Path, id: &str) -> anyhow::Result<()> {
    crate::store::change(path, HIDDEN_WHAT, |all: &mut Vec<String>| {
        if all.iter().any(|existing| existing == id) {
            return false;
        }
        all.push(id.to_string());
        let extra = all.len().saturating_sub(MAX_HIDDEN);
        all.drain(..extra);
        true
    })?;
    Ok(())
}

/// Where `agy -p` side copies write their log so they don't fill `~/.gemini/antigravity-cli/log/`.
fn side_log() -> PathBuf {
    crate::config::logs_dir().join("agy-side.log")
}

/// Permission flags for a side copy, matching what the session itself may do without asking.
pub(crate) fn mode_args(mode: Option<&SessionMode>) -> Vec<String> {
    match mode.map(|m| &m.kind) {
        Some(ModeKind::Skips) => vec!["--dangerously-skip-permissions".into()],
        Some(ModeKind::Auto) => vec!["--mode".into(), "auto".into()],
        Some(ModeKind::Edits) => vec!["--mode".into(), "accept-edits".into()],
        _ => Vec::new(),
    }
}

/// Resolves the `agy` binary to spawn for `account`: its configured command, or `~/.local/bin/agy`
/// when `agy` isn't on the user's PATH.
fn cli_program(account: &AccountCfg) -> PathBuf {
    let cmd = crate::antigravity::cli_command(account);
    if cmd == crate::antigravity::CLI && !crate::detect::on_user_path(cmd) {
        let installed = crate::util::home().join(".local/bin").join(cmd);
        if crate::detect::is_executable(&installed) {
            return installed;
        }
    }
    PathBuf::from(cmd)
}

/// Builds the `Recipe` for an Antigravity side copy of session `s` in `dir`.
pub(crate) fn side_recipe(account: &AccountCfg, s: &Session, dir: &Path, model: Option<String>, mode: Option<SessionMode>) -> anyhow::Result<Recipe> {
    if let Some(missing) = crate::antigravity::missing_cli(account, &crate::util::home()) {
        anyhow::bail!("Antigravity's command line, {missing}, isn't installed.");
    }
    let log = side_log();
    if let Some(parent) = log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut args = vec![
        "-p".into(),
        String::new(),
        "--input-format".into(),
        "stream-json".into(),
        "--output-format".into(),
        "stream-json".into(),
        "--disable-slash-commands".into(),
        "--log-file".into(),
        log.to_string_lossy().into_owned(),
    ];
    let brain = dir.join("brain").join(&s.id);
    if brain.is_dir() {
        args.push("--add-dir".into());
        args.push(brain.to_string_lossy().into_owned());
    }
    if let Some(m) = model.filter(|m| crate::start::valid_model(m)) {
        args.push("--model".into());
        args.push(m);
    }
    args.extend(mode_args(mode.as_ref()));

    let transcript = crate::antigravity::transcript_file(dir, &s.id);
    let context = transcript.as_deref().and_then(transcript_context);
    let rules = crate::side::rules(&s.key);
    let dialect = SideCopy::new(rules, context);
    Ok(Recipe {
        program: cli_program(account),
        args,
        env: Vec::new(),
        unset: vec!["CODEALONG_SESSION"],
        dialect: Box::new(dialect),
    })
}

/// Reads the end of `path` and formats recent turns for the side copy's initial context.
fn transcript_context(path: &Path) -> Option<String> {
    let end = crate::util::read_last(path, TRANSCRIPT_READ_BYTES).ok()?;
    format_transcript(&end.text(), path, CONTEXT_BUDGET)
}

/// Formats an Antigravity JSONL transcript into a compact chronological excerpt up to `budget`
/// bytes, keeping the most recent turns and pointing to `path` for anything earlier.
pub(crate) fn format_transcript(jsonl: &str, path: &Path, budget: usize) -> Option<String> {
    let mut steps: Vec<(i64, Vec<String>)> = Vec::new();
    for line in jsonl.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let Some(index) = v.get("step_index").and_then(Value::as_i64) else { continue };
        let mut lines = Vec::new();
        for item in crate::antigravity::conversation_items(&v, 0) {
            match item.kind {
                "you" if !item.text.trim().is_empty() => lines.push(format!("User: {}", item.text.trim())),
                "agent" if !item.text.trim().is_empty() => lines.push(format!("Assistant: {}", item.text.trim())),
                "tool" if !item.text.trim().is_empty() => lines.push(format!("[Tool] {}", item.text.trim())),
                _ => {}
            }
        }
        if !lines.is_empty() {
            steps.push((index, lines));
        }
    }
    if steps.is_empty() {
        return None;
    }
    steps.sort_by_key(|(idx, _)| *idx);
    let entries: Vec<String> = steps.into_iter().flat_map(|(_, lines)| lines).collect();
    let total = entries.len();
    let mut kept = Vec::new();
    let mut used = 0usize;
    for entry in entries.into_iter().rev() {
        let add = entry.len() + 2;
        if !kept.is_empty() && used + add > budget {
            break;
        }
        used += add;
        kept.push(entry);
    }
    kept.reverse();
    let omitted = total.saturating_sub(kept.len());
    let mut header = format!("Transcript of the main conversation (full file: {}):", path.display());
    if omitted > 0 {
        header.push_str(&format!(" [{omitted} earlier entries omitted; read the full file if needed]"));
    }
    Some(format!("{header}\n\n{}", kept.join("\n\n")))
}

/// Formats a `step_update` tool event (`tool_info` / `tool_name`) as a single line.
fn tool_line(update: &Value) -> String {
    let info = update.get("tool_info").unwrap_or(&Value::Null);
    let name = info.get("name").or_else(|| update.get("tool_name")).and_then(Value::as_str).unwrap_or("tool");
    let params = info.get("parameters").unwrap_or(&Value::Null);
    let said = name.replace('_', " ");
    let mut chars = said.chars();
    let said = chars.next().map(|f| f.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default();
    let str_param = |k: &str| params.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    if name == "run_command" {
        if let Some(cmd) = str_param("CommandLine") {
            return format!("{said} · {}", code(&one_line(cmd, 80)));
        }
    }
    if let Some(summary) = str_param("toolSummary") {
        return format!("{said} · {}", one_line(summary, 80));
    }
    let file = ["TargetFile", "AbsolutePath", "Path", "DirectoryPath", "SearchPath", "SearchDirectory"].iter().find_map(|k| str_param(k));
    match file {
        Some(f) => format!("{said} · {}", code(&file_name(Path::new(f)))),
        None => said,
    }
}

/// How Codealong talks to a headless `agy -p --input-format stream-json --output-format stream-json`
/// side copy.
pub struct SideCopy {
    rules: String,
    context: Option<String>,
    told: bool,
    said: bool,
    last_step: Option<i64>,
    seen_id: Option<String>,
    on_id: fn(&str),
}

impl SideCopy {
    pub fn new(rules: String, context: Option<String>) -> SideCopy {
        SideCopy { rules, context, told: false, said: false, last_step: None, seen_id: None, on_id: hide }
    }

    #[cfg(test)]
    fn with_on_id(mut self, on_id: fn(&str)) -> SideCopy {
        self.on_id = on_id;
        self
    }

    fn note_id(&mut self, line: &Value) {
        let id = line
            .get("conversation_id")
            .or_else(|| line.pointer("/step_update/conversation_id"))
            .or_else(|| line.pointer("/result/conversation_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(id) = id {
            if self.seen_id.as_deref() != Some(id) {
                self.seen_id = Some(id.to_string());
                (self.on_id)(id);
            }
        }
    }
}

impl Dialect for SideCopy {
    fn open(&mut self) -> Vec<Value> {
        Vec::new()
    }

    fn ask(&mut self, text: &str) -> Vec<Value> {
        let prompt = if self.told {
            text.to_string()
        } else {
            match &self.context {
                Some(ctx) => format!("<system-reminder>\n{}\n\n{ctx}\n</system-reminder>\n\n{text}", self.rules),
                None => format!("<system-reminder>\n{}\n</system-reminder>\n\n{text}", self.rules),
            }
        };
        self.told = true;
        self.said = false;
        self.last_step = None;
        vec![serde_json::json!({ "event": "user", "message": { "content": prompt } })]
    }

    fn hear(&mut self, line: &Value) -> (Vec<Event>, Vec<Value>) {
        self.note_id(line);
        let event = line.get("event").and_then(Value::as_str).unwrap_or("");
        let events = match event {
            "step_update" => {
                let u = line.get("step_update").unwrap_or(&Value::Null);
                let step_type = u.get("step_type").and_then(Value::as_str).unwrap_or("");
                let state = u.get("state").and_then(Value::as_str).unwrap_or("");
                let step_index = u.get("step_index").and_then(Value::as_i64);
                match step_type {
                    "agent_response" => {
                        let delta = u.get("text_delta").and_then(Value::as_str).filter(|s| !s.is_empty());
                        match delta {
                            Some(t) => {
                                let mut out = Vec::new();
                                if self.said && self.last_step.is_some() && self.last_step != step_index {
                                    out.push(Event::Said("\n\n".into()));
                                }
                                self.said = true;
                                self.last_step = step_index;
                                out.push(Event::Said(t.to_string()));
                                out
                            }
                            None => Vec::new(),
                        }
                    }
                    "tool" if state == "ACTIVE" => vec![Event::Tool(tool_line(u))],
                    _ => Vec::new(),
                }
            }
            "result" => {
                let r = line.get("result").unwrap_or(line);
                let status = r.get("status").and_then(Value::as_str).unwrap_or("");
                let finished = r.get("finished_reason").and_then(Value::as_str);
                let failed = !status.eq_ignore_ascii_case("SUCCESS") || matches!(finished, Some("error" | "interrupted" | "print_timeout"));
                let mut out = Vec::new();
                if !failed && !self.said {
                    if let Some(resp) = r.get("response").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
                        out.push(Event::Said(resp.to_string()));
                    }
                }
                let err = failed.then(|| {
                    r.get("error")
                        .or_else(|| r.get("response"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .unwrap_or_else(|| "It ended without an answer.".into())
                });
                out.push(Event::Done(err));
                out
            }
            _ => Vec::new(),
        };
        (events, Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    fn recorded() -> &'static Mutex<Vec<String>> {
        static REC: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
        REC.get_or_init(Default::default)
    }

    fn record_id(id: &str) {
        recorded().lock().unwrap().push(id.to_string());
    }

    #[test]
    fn side_copy_seeds_once_hides_id_and_streams_events() {
        recorded().lock().unwrap().clear();
        let mut copy = SideCopy::new("RULES".into(), Some("Transcript:\n\nUser: hi".into())).with_on_id(record_id);

        let first = copy.ask("what is a skiff?");
        assert_eq!(first.len(), 1);
        let first_text = first[0].pointer("/message/content").and_then(Value::as_str).unwrap();
        assert!(first_text.contains("RULES"));
        assert!(first_text.contains("User: hi"));
        assert!(first_text.ends_with("what is a skiff?"));

        let (ev, _) = copy.hear(&json!({"event": "init", "conversation_id": "conv-123"}));
        assert!(ev.is_empty());
        assert_eq!(*recorded().lock().unwrap(), vec!["conv-123"]);

        // Streaming text, a tool call, and a second text step insert a paragraph break.
        let (ev, _) = copy.hear(&json!({"event": "step_update", "step_update": {"conversation_id": "conv-123", "step_index": 1, "state": "ACTIVE", "step_type": "agent_response", "text_delta": "Checking"}}));
        assert_eq!(ev, vec![Event::Said("Checking".into())]);
        let (ev, _) = copy.hear(&json!({"event": "step_update", "step_update": {"conversation_id": "conv-123", "step_index": 2, "state": "ACTIVE", "step_type": "tool", "tool_name": "run_command", "tool_info": {"name": "run_command", "parameters": {"CommandLine": "ls -la"}}}}));
        assert_eq!(ev, vec![Event::Tool("Run command · `ls -la`".into())]);
        let (ev, _) = copy.hear(&json!({"event": "step_update", "step_update": {"conversation_id": "conv-123", "step_index": 3, "state": "DONE", "step_type": "agent_response", "text_delta": "It is a SCIF."}}));
        assert_eq!(ev, vec![Event::Said("\n\n".into()), Event::Said("It is a SCIF.".into())]);
        let (ev, _) = copy.hear(&json!({"event": "result", "result": {"conversation_id": "conv-123", "status": "SUCCESS", "response": "It is a SCIF."}}));
        assert_eq!(ev, vec![Event::Done(None)]);
        // conversation_id is only reported once.
        assert_eq!(*recorded().lock().unwrap(), vec!["conv-123"]);

        // Second question omits rules and transcript context.
        let second = copy.ask("follow up");
        assert_eq!(second[0].pointer("/message/content").and_then(Value::as_str), Some("follow up"));
    }

    #[test]
    fn side_copy_falls_back_to_result_response_and_reports_errors() {
        let mut copy = SideCopy::new("RULES".into(), None).with_on_id(|_| {});
        copy.ask("hello");
        let (ev, _) = copy.hear(&json!({"event": "result", "result": {"status": "SUCCESS", "response": "Final answer"}}));
        assert_eq!(ev, vec![Event::Said("Final answer".into()), Event::Done(None)]);

        copy.ask("boom");
        let (ev, _) = copy.hear(&json!({"event": "result", "result": {"status": "ERROR", "finished_reason": "error", "error": "quota exceeded"}}));
        assert_eq!(ev, vec![Event::Done(Some("quota exceeded".into()))]);
    }

    #[test]
    fn format_transcript_orders_steps_and_respects_budget() {
        let jsonl = [
            json!({"step_index": 2, "source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "created_at": "2026-11-20T10:00:02Z", "content": "Second answer"}).to_string(),
            json!({"step_index": 0, "source": "USER_EXPLICIT", "type": "USER_INPUT", "status": "DONE", "created_at": "2026-11-20T10:00:00Z", "content": "<USER_REQUEST>\nFirst question\n</USER_REQUEST>"}).to_string(),
            json!({"step_index": 1, "source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "created_at": "2026-11-20T10:00:01Z", "content": "First answer"}).to_string(),
        ]
        .join("\n");
        let path = Path::new("/tmp/transcript_full.jsonl");
        let full = format_transcript(&jsonl, path, 4096).unwrap();
        let u0 = full.find("User: First question").unwrap();
        let a1 = full.find("Assistant: First answer").unwrap();
        let a2 = full.find("Assistant: Second answer").unwrap();
        assert!(u0 < a1 && a1 < a2);

        let tight = format_transcript(&jsonl, path, 30).unwrap();
        assert!(tight.contains("2 earlier entries omitted"));
        assert!(tight.contains("Assistant: Second answer"));
        assert!(!tight.contains("First question"));
    }

    #[test]
    fn hide_in_persists_unique_ids_and_caps_list() {
        let dir = std::env::temp_dir().join(format!("ca-agy-side-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("antigravity-sides.json");

        hide_in(&path, "c1").unwrap();
        hide_in(&path, "c1").unwrap();
        hide_in(&path, "c2").unwrap();
        let saved: Vec<String> = crate::store::read_or_default(&path);
        assert_eq!(saved, vec!["c1", "c2"]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mode_args_maps_session_modes() {
        assert_eq!(mode_args(Some(&SessionMode::new(ModeKind::Skips, "Always proceed", ""))), vec!["--dangerously-skip-permissions"]);
        assert_eq!(mode_args(Some(&SessionMode::new(ModeKind::Auto, "Auto", ""))), vec!["--mode", "auto"]);
        assert_eq!(mode_args(Some(&SessionMode::new(ModeKind::Edits, "Accept edits", ""))), vec!["--mode", "accept-edits"]);
        assert!(mode_args(Some(&SessionMode::new(ModeKind::Plan, "Plan", ""))).is_empty());
        assert!(mode_args(Some(&SessionMode::new(ModeKind::Asks, "Request review", ""))).is_empty());
        assert!(mode_args(None).is_empty());
    }
}

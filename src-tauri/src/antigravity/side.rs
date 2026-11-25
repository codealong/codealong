//! Antigravity side conversations (`Adapter::side`): a fresh headless `agy -p` stream-json run
//! seeded with the session's transcript, since `agy` (1.5.13) has no native ephemeral fork flag
//! or `session/fork` in ACP. Each throwaway run still writes a row to `conversation_summaries.db`,
//! so its `conversation_id` (from the `init` event) is recorded here and persisted to
//! `antigravity-sides.json` to keep it off the board, out of search, out of workspace CWDs and
//! out of the token ledger, while `side::runs(pid)` hides its process while it runs.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

use crate::config::AccountCfg;
use crate::model::{ModeKind, Session, SessionMode};
use crate::side::{Dialect, Event, Recipe};
use crate::util::truncate;

/// How much of the end of a session's transcript to read when seeding a side copy.
const TRANSCRIPT_READ_BYTES: u64 = 512 * 1024;

/// Maximum bytes of inline transcript context sent with the side copy's first question.
/// Longer conversations keep their newest turns inline and point the copy at the full file.
const CONTEXT_BUDGET: usize = 48 * 1024;

/// Maximum throwaway conversation IDs kept in `antigravity-sides.json`.
const MAX_HIDDEN: usize = 2000;

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

/// Loads `antigravity-sides.json` once in non-test builds so `--snapshot`, `CODEALONG_NO_SERVER=1`,
/// `scripts/preview.sh` and early server scans all see hidden IDs without depending on `side::init`.
fn ensure_loaded(st: &mut HiddenStore) {
    if st.loaded {
        return;
    }
    #[cfg(not(test))]
    {
        let ids: Vec<String> = crate::store::read_or_default(&hidden_path());
        st.set = ids.iter().cloned().collect();
        st.ids = ids;
    }
    st.loaded = true;
}

/// Whether `id` is a throwaway Antigravity conversation created for a Codealong side copy.
pub(crate) fn is_hidden(id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    let mut st = hidden_state();
    ensure_loaded(&mut st);
    st.set.contains(id)
}

/// Marks `id` as a throwaway side conversation immediately in memory and persists it off-thread
/// when the server is keeping side conversations (`side::keeps()`).
pub(crate) fn hide(id: &str) {
    let id = id.trim();
    if id.is_empty() {
        return;
    }
    let is_new = {
        let mut st = hidden_state();
        ensure_loaded(&mut st);
        insert_capped(&mut st, id, MAX_HIDDEN)
    };
    if is_new && crate::side::keeps() {
        let id = id.to_string();
        std::thread::spawn(move || {
            let path = hidden_path();
            let _ = crate::store::noted(&path, hide_in_cap(&path, &id, MAX_HIDDEN));
        });
    }
}

fn insert_capped(st: &mut HiddenStore, id: &str, cap: usize) -> bool {
    if !st.set.insert(id.to_string()) {
        return false;
    }
    st.ids.push(id.to_string());
    if st.ids.len() > cap {
        let extra = st.ids.len() - cap;
        let removed: Vec<String> = st.ids.drain(..extra).collect();
        for old in removed {
            st.set.remove(&old);
        }
    }
    true
}

fn hide_in_cap(path: &Path, id: &str, cap: usize) -> anyhow::Result<()> {
    crate::store::change(path, HIDDEN_WHAT, |all: &mut Vec<String>| {
        if all.iter().any(|existing| existing == id) {
            return false;
        }
        all.push(id.to_string());
        let extra = all.len().saturating_sub(cap);
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
pub(super) fn mode_args(mode: Option<&SessionMode>) -> Vec<String> {
    match mode.map(|m| &m.kind) {
        Some(ModeKind::Skips) => vec!["--dangerously-skip-permissions".into()],
        Some(ModeKind::Auto) => vec!["--mode".into(), "auto".into()],
        Some(ModeKind::Edits) => vec!["--mode".into(), "accept-edits".into()],
        _ => Vec::new(),
    }
}

/// Builds `(program, args)` for `cli`: direct invocation for a plain binary name/path (resolved
/// by `side::spawn` via `procs::user_path()`, which already includes `~/.local/bin`), or via
/// `sh -c 'exec <cmd> "$@"' _` when the account's `command` sets env vars or shell words.
fn cli_launch(cli: &str, args: Vec<String>) -> anyhow::Result<(PathBuf, Vec<String>)> {
    if cli.chars().any(|c| c.is_whitespace() || matches!(c, '=' | ';' | '|' | '&' | '\'' | '"')) {
        let sh = crate::procs::sh()?;
        let mut sh_args = vec!["-c".into(), format!("exec {cli} \"$@\""), "_".into()];
        sh_args.extend(args);
        Ok((sh.into(), sh_args))
    } else {
        Ok((PathBuf::from(cli), args))
    }
}

/// Rules for an Antigravity side copy: `side::rules` without the `/fork` suggestion, since
/// Antigravity has no `/fork` command in Codealong.
fn side_rules(key: &str) -> String {
    const FORK_NOTE: &str = "say in a line that /fork, then what to do, typed in this box, has another copy of the conversation do it in the background, told what was said here, or that you can tell the main conversation to.";
    const NO_FORK: &str = "say in a line that you can tell the main conversation to do it, or that the user can ask it there.";
    crate::side::rules(key).replace(FORK_NOTE, NO_FORK)
}

/// Builds the `Recipe` for an Antigravity side copy of session `s` in `dir`.
pub(super) fn recipe(account: &AccountCfg, s: &Session, dir: &Path, model: Option<String>, mode: Option<SessionMode>) -> anyhow::Result<Recipe> {
    if let Some(missing) = super::missing_cli(account, &crate::util::home()) {
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

    let (program, args) = cli_launch(super::cli_command(account), args)?;
    let transcript = super::transcript_file(dir, &s.id);
    let context = transcript.as_deref().and_then(transcript_context);
    let dialect = SideCopy::new(side_rules(&s.key), context);
    // Started as soon as this returns.
    super::kept_mode::keep(&account.home_path());
    Ok(Recipe {
        program,
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
pub(super) fn format_transcript(jsonl: &str, path: &Path, budget: usize) -> Option<String> {
    let mut steps: Vec<(i64, Vec<String>)> = Vec::new();
    for line in jsonl.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let Some(index) = v.get("step_index").and_then(Value::as_i64) else { continue };
        let mut lines = Vec::new();
        for item in super::conversation_items(&v, 0) {
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
        if kept.is_empty() && entry.len() > budget {
            kept.push(truncate(&entry, budget));
            break;
        }
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

/// Formats a `step_update` tool event (`tool_info` / `tool_name`) using `antigravity::call`.
fn tool_line(update: &Value) -> String {
    let info = update.get("tool_info").unwrap_or(&Value::Null);
    let name = info.get("name").or_else(|| update.get("tool_name")).and_then(Value::as_str).unwrap_or("tool");
    let params = info.get("parameters").cloned().unwrap_or(Value::Null);
    super::call(&json!({ "name": name, "args": params }), 0).label
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
        vec![json!({ "event": "user", "message": { "content": prompt } })]
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

        let (ev, _) = copy.hear(&json!({"event": "step_update", "step_update": {"conversation_id": "conv-123", "step_index": 1, "state": "ACTIVE", "step_type": "agent_response", "text_delta": "Checking"}}));
        assert_eq!(ev, vec![Event::Said("Checking".into())]);
        let (ev, _) = copy.hear(&json!({"event": "step_update", "step_update": {"conversation_id": "conv-123", "step_index": 2, "state": "ACTIVE", "step_type": "tool", "tool_name": "run_command", "tool_info": {"name": "run_command", "parameters": {"CommandLine": "ls -la"}}}}));
        assert_eq!(ev, vec![Event::Tool("Run command · `ls -la`".into())]);
        let (ev, _) = copy.hear(&json!({"event": "step_update", "step_update": {"conversation_id": "conv-123", "step_index": 3, "state": "DONE", "step_type": "agent_response", "text_delta": "It is a SCIF."}}));
        assert_eq!(ev, vec![Event::Said("\n\n".into()), Event::Said("It is a SCIF.".into())]);
        let (ev, _) = copy.hear(&json!({"event": "result", "result": {"conversation_id": "conv-123", "status": "SUCCESS", "response": "It is a SCIF."}}));
        assert_eq!(ev, vec![Event::Done(None)]);
        assert_eq!(*recorded().lock().unwrap(), vec!["conv-123"]);

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

        // A single entry longer than the budget is truncated rather than blowing past it.
        let huge = json!({"step_index": 0, "source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "created_at": "2026-11-20T10:00:00Z", "content": "x".repeat(200)}).to_string();
        let clipped = format_transcript(&huge, path, 40).unwrap();
        let body = clipped.split_once("\n\n").unwrap().1;
        assert!(body.chars().count() <= 40);
    }

    #[test]
    fn hide_store_and_file_dedupe_and_evict_past_cap() {
        let mut st = HiddenStore::default();
        assert!(insert_capped(&mut st, "c1", 2));
        assert!(!insert_capped(&mut st, "c1", 2));
        assert!(insert_capped(&mut st, "c2", 2));
        assert!(insert_capped(&mut st, "c3", 2));
        assert_eq!(st.ids, vec!["c2", "c3"]);
        assert!(!st.set.contains("c1"));
        assert!(st.set.contains("c2") && st.set.contains("c3"));

        let dir = crate::fixtures::scratch("agy-side-hide");
        let path = dir.join("antigravity-sides.json");
        hide_in_cap(&path, "c1", 2).unwrap();
        hide_in_cap(&path, "c1", 2).unwrap();
        hide_in_cap(&path, "c2", 2).unwrap();
        hide_in_cap(&path, "c3", 2).unwrap();
        let saved: Vec<String> = crate::store::read_or_default(&path);
        assert_eq!(saved, vec!["c2", "c3"]);
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

    #[test]
    fn cli_launch_uses_shell_for_env_prefixed_command() {
        let (prog, args) = cli_launch("agy", vec!["-p".into(), String::new()]).unwrap();
        assert_eq!(prog, PathBuf::from("agy"));
        assert_eq!(args, vec!["-p", ""]);

        let (prog, args) = cli_launch("FOO=1 agy", vec!["-p".into(), String::new()]).unwrap();
        assert!(prog.to_string_lossy().ends_with("sh"));
        assert_eq!(&args[..3], &["-c", "exec FOO=1 agy \"$@\"", "_"]);
        assert_eq!(&args[3..], &["-p", ""]);
    }

    #[test]
    fn side_rules_omits_fork_hint_for_antigravity() {
        let r = side_rules("antigravity:a:1");
        assert!(!r.contains("/fork"));
        assert!(r.contains("antigravity:a:1"));
    }
}

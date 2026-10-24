//! Actions Codex owns, done the way the Codex app does them: a short-lived `codex app-server`
//! over stdio. Archiving a thread moves its rollout into `archived_sessions` and marks it in
//! Codex's shared state database, so the Codex app sees the change the next time it reads its
//! threads. See .claude/rules/codex.md.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use crate::procs;
use crate::rpc::{self, Failure};
use crate::send::MayHaveArrived;
use crate::util::truncate;

const TIMEOUT: Duration = Duration::from_secs(20);

/// The handshake an app-server wants before anything else: an `initialize` request that says
/// who's asking and, once that's answered, an `initialized` notification. The client below
/// sends it, and so does a side conversation's copy of a thread (`codex::SideThread`), which
/// writes its own lines.
pub const INITIALIZE: &str = "initialize";
pub const INITIALIZED: &str = "initialized";

/// What `initialize` says.
pub fn initialize_params() -> Value {
    json!({ "clientInfo": rpc::client_info() })
}

/// The Codex CLI the installed app ships, so the protocol matches the app's own; else `codex`
/// on the PATH.
pub fn codex_binary() -> PathBuf {
    crate::detect::codex_app_cli().unwrap_or_else(|| PathBuf::from("codex"))
}

/// Sends one request to a fresh app-server for the account at `codex_home` and returns its result.
pub fn call(codex_home: &Path, method: &str, params: Value) -> Result<Value> {
    with_server(codex_home, |s| s.request(method, params))
}

/// A fresh app-server, initialized, for as long as `f` runs.
struct Server {
    client: rpc::Client,
    /// When everything asked of it must be answered by.
    deadline: Instant,
}

impl Server {
    fn request(&self, method: &str, params: Value) -> Result<Value> {
        self.answer(method, params).map_err(|e| match e {
            Failure::Timeout => anyhow!("Codex didn't answer in time."),
            Failure::Closed(said) if said.is_empty() => anyhow!("Codex stopped before it answered."),
            Failure::Closed(said) => anyhow!("Codex stopped before it answered: {}", truncate(&said.join(" "), 300)),
            Failure::Refused(e) => anyhow!("Codex: {}", e.get("message").and_then(Value::as_str).unwrap_or("unknown error")),
            Failure::Write(e) => e.into(),
        })
    }

    /// The answer as it came, for a caller that tells a refusal from silence.
    fn answer(&self, method: &str, params: Value) -> std::result::Result<Value, Failure> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        self.client.request(method, params, Some(left))
    }
}

fn with_server<T>(codex_home: &Path, f: impl FnOnce(&Server) -> Result<T>) -> Result<T> {
    serve(app_server(codex_home)?, f)
}

/// What Codex's app-server not starting is called.
const NOT_STARTED: &str = "Couldn't start Codex. Is the Codex app installed?";

fn app_server(codex_home: &Path) -> Result<Command> {
    let mut cmd = procs::command(codex_binary(), procs::user_path(), None).context(NOT_STARTED)?;
    cmd.args(["app-server", "--listen", "stdio://"]).env("CODEX_HOME", codex_home);
    Ok(cmd)
}

/// Runs `f` against the app-server `cmd` starts, once the handshake is done. The server ends
/// with it: the client kills its process as it's dropped. What the server tells or asks
/// meanwhile is left unread.
fn serve<T>(cmd: Command, f: impl FnOnce(&Server) -> Result<T>) -> Result<T> {
    serve_as(cmd, initialize_params(), f)
}

/// `serve`, introduced with `hello` as the `initialize` params.
fn serve_as<T>(cmd: Command, hello: Value, f: impl FnOnce(&Server) -> Result<T>) -> Result<T> {
    f(&Server::start(cmd, hello)?)
}

impl Server {
    /// The app-server `cmd` starts, once the handshake is done, introduced with `hello` as the
    /// `initialize` params.
    fn start(cmd: Command, hello: Value) -> Result<Server> {
        let client = rpc::Client::spawn(cmd, rpc::Stderr::Tail, rpc::Wire::Bare, |_, _| {}).context(NOT_STARTED)?;
        let server = Server { client, deadline: Instant::now() + TIMEOUT };
        server.request(INITIALIZE, hello)?;
        server.client.notify(INITIALIZED, Value::Null)?;
        Ok(server)
    }
}

/// Has Codex trust the hook in `file` that runs `command`, as its terminal UI's own review of a
/// new hook does it (0.162.1's `tui/src/hooks_rpc.rs`): the hook's key and hash from `hooks/list`,
/// the hash written under config.toml's `hooks.state` (`config/batchWrite`), and the list asked
/// again to see it taken. Whether it had to be trusted. Checked 2026-10-24 on a scratch
/// project's hook, which 0.162.1 and the app's 0.163.0-alpha then both listed as trusted.
pub fn trust_hook(codex_home: &Path, file: &Path, command: &str) -> Result<bool> {
    // Codex names it by its path with links resolved (`/private/var` for `/var`).
    let file = &std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    with_server(codex_home, |s| {
        let listed = || -> Result<Value> {
            let all = s.request("hooks/list", json!({ "cwds": [codex_home] }))?;
            all["data"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|cwd| cwd["hooks"].as_array().into_iter().flatten())
                .find(|h| h["command"].as_str() == Some(command) && h["sourcePath"].as_str().map(Path::new) == Some(file))
                .cloned()
                .ok_or_else(|| anyhow!("Codex doesn't list Codealong's hook in {}.", file.display()))
        };
        let hook = listed()?;
        if matches!(hook["trustStatus"].as_str(), Some("trusted" | "managed")) {
            return Ok(false);
        }
        let (Some(key), Some(hash)) = (hook["key"].as_str(), hook["currentHash"].as_str()) else {
            bail!("Codex listed Codealong's hook without its key and hash.");
        };
        s.request("config/batchWrite", json!({ "edits": [{ "keyPath": "hooks.state", "value": { key: { "trusted_hash": hash } }, "mergeStrategy": "upsert" }], "reloadUserConfig": true }))?;
        match listed()?["trustStatus"].as_str() {
            Some("trusted") => Ok(true),
            other => bail!("Codex still lists Codealong's hook as {}.", other.unwrap_or("untrusted")),
        }
    })
}

/// Whether Codex would run a `PermissionRequest` hook in `cwd` beside Codealong's (one whose
/// command has `ours`): the user's, a project's, a plugin's or a managed one (`hooks/list`, which
/// lists them all with their event). Codex runs a request's hooks together and shows its prompt
/// once they've all finished (0.162.1's `hooks/src/engine/dispatcher.rs` `execute_handlers`), so
/// with another, the prompt may not be up when Codealong's hook has noted it.
pub fn other_prompt_hooks(codex_home: &Path, cwd: &Path, ours: &str) -> Result<bool> {
    Ok(others_in(&call(codex_home, "hooks/list", json!({ "cwds": [cwd] }))?, ours))
}

fn others_in(listed: &Value, ours: &str) -> bool {
    let hooks = listed["data"].as_array().into_iter().flatten().flat_map(|cwd| cwd["hooks"].as_array().into_iter().flatten());
    hooks.filter(|h| h["eventName"].as_str() == Some("permissionRequest")).any(|h| !h["command"].as_str().is_some_and(|c| c.contains(ours)))
}

/// Whether Codex has managed requirements for the account (`configRequirements/read`: a
/// workspace's from its backend, an MDM profile's, `/etc/codex`'s), which can take entries out of
/// what `/permissions` lists. It answered `requirements: null` with none (2026-10-24).
pub fn has_requirements(codex_home: &Path) -> Result<bool> {
    let read = call(codex_home, "configRequirements/read", Value::Null)?;
    Ok(read.get("requirements").is_some_and(|r| !r.is_null()))
}

/// Queues `text` in a thread, for the Codex app to start once the turn it's taking ends, as a
/// follow-up queued in the app is (`thread/queue/add`, 26.1007, which takes the experimental
/// API). The queue is `queue_1.sqlite` in the account's folder: whichever app-server has the
/// thread loaded starts what's in it as each turn ends, and notices an addition from another
/// process within seconds while the thread is idle. This one doesn't load the thread, so nothing
/// runs here (two app-servers on one CODEX_HOME, 2026-10-12). Once Codex has the request, no
/// answer may mean it was queued (`MayHaveArrived`). `id` is its `clientUserMessageId`, which the
/// queue lists it by (queue.rs).
pub fn queue(codex_home: &Path, thread_id: &str, text: &str, id: &str) -> Result<()> {
    serve_as(app_server(codex_home)?, queue_hello(), |s| add_to_queue(s, thread_id, text, id))
}

fn queue_hello() -> Value {
    json!({ "clientInfo": rpc::client_info(), "capabilities": { "experimentalApi": true } })
}

/// The prompt's text and the `input` items Codex's app and app-server take for `text`: kept
/// images go as `{ "type": "localImage", "path": "<path>" }` items, taken out of the text, and
/// other attachments stay in the text as paths.
pub fn input_items_in(text: &str, images: &Path, files: &Path) -> (String, Value) {
    let (prose, taken) = crate::attachments::take_images_in(text, images, files, crate::attachments::Style::Path);
    let mut items = Vec::new();
    if !prose.is_empty() || taken.is_empty() {
        items.push(json!({ "type": "text", "text": prose, "text_elements": [] }));
    }
    for path in taken {
        items.push(json!({ "type": "localImage", "path": path }));
    }
    (prose, Value::Array(items))
}

pub fn input_items(text: &str) -> (String, Value) {
    input_items_in(text, &crate::attachments::images_dir(), &crate::attachments::files_dir())
}

fn add_to_queue(s: &Server, thread_id: &str, text: &str, message_id: &str) -> Result<()> {
    let (_, input) = input_items(text);
    let params = json!({ "threadId": thread_id, "input": input, "clientUserMessageId": message_id });
    match s.answer("thread/queue/add", params) {
        Ok(_) => Ok(()),
        Err(Failure::Refused(e)) => bail!("Codex didn't queue it: {}", e.get("message").and_then(Value::as_str).unwrap_or("unknown error")),
        Err(Failure::Write(e)) => Err(e.into()),
        Err(_) => Err(MayHaveArrived("Codealong asked Codex to queue this and didn't hear how it went: look there before sending it again.".into()).into()),
    }
}

/// A message the Codex app holds for a thread, as its queue lists it (`thread/queue/list`).
#[derive(Debug, Clone, PartialEq)]
pub struct Queued {
    /// Codex's own id for it, which `thread/queue/delete` takes.
    pub id: String,
    /// The `clientUserMessageId` it was added with.
    pub message_id: String,
    /// Its parts, as `turn/start` and `turn/steer` take them: an image the app-server was handed
    /// as a path comes back as `{"type": "image", "url": "data:…"}`.
    pub input: Value,
}

impl Queued {
    /// Its words, without its images, as the app's box would hold them.
    pub fn prose(&self) -> String {
        let parts = self.input.as_array().into_iter().flatten();
        parts.filter(|p| p["type"] == "text").filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n\n")
    }
}

/// The Codex app's follow-up queue (26.1007), through an app-server of Codealong's own for as
/// long as it's kept, which lists and changes it as the app's windows do: a queued message's
/// Steer in the app takes it from the list, steers it in and deletes it.
pub struct Queue(Server);

impl Queue {
    pub fn open(codex_home: &Path) -> Result<Queue> {
        Ok(Queue(Server::start(app_server(codex_home)?, queue_hello())?))
    }

    /// The queue through the app-server `cmd` runs (`stand_in`).
    #[cfg(test)]
    pub(super) fn through(cmd: Command) -> Result<Queue> {
        Ok(Queue(Server::start(cmd, queue_hello())?))
    }

    /// The message `thread` holds as `id`, the `clientUserMessageId` it went under or Codex's own
    /// id for it, as queue.rs lists it; None once it's gone (sent, or taken out).
    pub fn find(&self, thread: &str, id: &str) -> Result<Option<Queued>> {
        let mut cursor = Value::Null;
        loop {
            let page = self.0.request("thread/queue/list", json!({ "threadId": thread, "cursor": cursor }))?;
            let listed = page["data"].as_array().into_iter().flatten();
            let found = listed.filter(|q| q["id"] == id || q["clientUserMessageId"] == id).find_map(|q| {
                Some(Queued { id: q["id"].as_str()?.to_string(), message_id: q["clientUserMessageId"].as_str().unwrap_or_default().to_string(), input: q["input"].clone() })
            });
            match (found, &page["nextCursor"]) {
                (Some(q), _) => return Ok(Some(q)),
                (None, Value::String(next)) if *next != cursor => cursor = json!(next),
                _ => return Ok(None),
            }
        }
    }

    /// Takes the message Codex knows as `id` out of `thread`'s queue.
    pub fn delete(&self, thread: &str, id: &str) -> Result<()> {
        self.0.request("thread/queue/delete", json!({ "threadId": thread, "queuedSubmissionId": id })).map(|_| ())
    }
}

/// A thread made for the Codex app to run: its id, and the model and effort the account's
/// settings chose for it.
pub struct NewThread {
    pub id: String,
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// Makes a thread in `cwd` that the Codex app can open, without running a turn in it. A new
/// thread's rollout isn't written until something is in its history, and the app can't open a
/// thread without one, so a short note goes in first. The first turn then goes to the app
/// (`ipc::codex_start_turn`), which runs it as if you'd typed it there.
pub fn new_thread(codex_home: &Path, cwd: &Path, model: Option<&str>) -> Result<NewThread> {
    with_server(codex_home, |s| make_thread(s, cwd, model))
}

fn make_thread(s: &Server, cwd: &Path, model: Option<&str>) -> Result<NewThread> {
    let mut params = json!({ "cwd": cwd });
    if let Some(m) = model {
        params["model"] = json!(m);
    }
    let r = s.request("thread/start", params)?;
    let id = r.pointer("/thread/id").and_then(Value::as_str).ok_or_else(|| anyhow!("Codex didn't make a thread."))?.to_string();
    let note = json!({ "type": "message", "role": "developer", "content": [{ "type": "input_text", "text": "This thread was started from Codealong." }] });
    s.request("thread/inject_items", json!({ "threadId": id, "items": [note] }))?;
    let text = |k: &str| r.get(k).and_then(Value::as_str).filter(|v| !v.is_empty()).map(String::from);
    Ok(NewThread { model: text("model"), effort: text("reasoningEffort"), id })
}

/// Archives or unarchives a thread in Codex.
pub fn set_archived(codex_home: &Path, thread_id: &str, archived: bool) -> Result<()> {
    let method = if archived { "thread/archive" } else { "thread/unarchive" };
    call(codex_home, method, json!({ "threadId": thread_id })).map(|_| ())
}

/// Names a thread in Codex: a line in its thread index (`session_index.jsonl`) and the name in
/// its state database, where the Codex app reads it. It refuses an archived thread ("no rollout
/// found"; 0.158.0-alpha.2, checked on a scratch `CODEX_HOME`).
pub fn set_name(codex_home: &Path, thread_id: &str, name: &str) -> Result<()> {
    call(codex_home, "thread/name/set", json!({ "threadId": thread_id, "name": name })).map(|_| ())
}

/// A stand-in for the app-server: keeps each line it's written in the file named by its first
/// argument, and answers every request as `answer` has it (`$id` is the request's, `$line` the
/// request).
#[cfg(test)]
pub(super) fn stand_in(heard: &Path, answer: &str) -> Command {
    let script = format!(r#"while IFS= read -r line; do printf '%s\n' "$line" >> "$1"; case "$line" in *'"id":'*) id=${{line#*\"id\":}}; id=${{id%%,*}}; {answer};; esac; done"#);
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", &script, "sh"]).arg(heard);
    cmd
}

#[cfg(test)]
mod tests {
    /// Another `PermissionRequest` hook keeps Codex's prompt back while it runs; other events'
    /// hooks don't. As 0.162.1's `hooks/list` answered for a project's hook (2026-10-24).
    #[test]
    fn another_prompt_hook_is_told_apart_from_codealongs() {
        let entry = |event: &str, command: &str| serde_json::json!({ "key": "/r/.codex/hooks.json:permission_request:0:0", "eventName": event, "handlerType": "command", "command": command, "trustStatus": "trusted" });
        let ours = r#""/Users/me/Library/Application Support/Codealong/plugin-marketplace/plugin/bin/codealong" session hook codex-permission-request >/dev/null 2>&1 || true"#;
        let listed = |hooks: Vec<serde_json::Value>| serde_json::json!({ "data": [{ "cwd": "/r", "hooks": hooks, "warnings": [], "errors": [] }] });
        let mark = crate::codex::hook::MARK;
        assert!(!super::others_in(&listed(vec![entry("permissionRequest", ours)]), mark));
        assert!(!super::others_in(&listed(vec![entry("permissionRequest", ours), entry("preToolUse", "./check.sh")]), mark));
        assert!(super::others_in(&listed(vec![entry("permissionRequest", ours), entry("permissionRequest", "./slow-audit.sh")]), mark));
        assert!(super::others_in(&listed(vec![serde_json::json!({ "eventName": "permissionRequest", "handlerType": "mcpTool" })]), mark), "a tool's hook has no command");
    }

    use super::*;

    /// What Codex's app-server is written, line for line, for each thing asked of it.
    #[test]
    fn the_app_server_is_written_the_handshake_and_then_what_is_asked() {
        let dir = crate::store::test_dir("appserver-lines");
        let heard = dir.join("heard");
        let ok = r#"printf '{"id":%s,"result":{"thread":{"id":"t1"},"model":"gpt-x","reasoningEffort":""}}\n' "$id""#;
        let lines = |f: &dyn Fn(&Server) -> Result<()>| {
            let _ = std::fs::remove_file(&heard);
            serve(stand_in(&heard, ok), |s| f(s)).unwrap();
            std::fs::read_to_string(&heard).unwrap().lines().map(String::from).collect::<Vec<_>>()
        };
        let hello = format!(r#"{{"id":1,"method":"initialize","params":{{"clientInfo":{{"name":"codealong","title":"Codealong","version":"{}"}}}}}}"#, env!("CARGO_PKG_VERSION"));
        let ready = r#"{"method":"initialized"}"#;
        // Archiving, as `set_archived` asks through `call`.
        let wrote = lines(&|s| s.request("thread/archive", json!({ "threadId": "t9" })).map(|_| ()));
        assert_eq!(wrote, [hello.as_str(), ready, r#"{"id":2,"method":"thread/archive","params":{"threadId":"t9"}}"#]);
        // Naming.
        let wrote = lines(&|s| s.request("thread/name/set", json!({ "threadId": "t9", "name": "A name" })).map(|_| ()));
        assert_eq!(wrote, [hello.as_str(), ready, r#"{"id":2,"method":"thread/name/set","params":{"name":"A name","threadId":"t9"}}"#]);
        // A thread for the app to run: started, then given a note so its rollout is written.
        let wrote = lines(&|s| {
            let t = make_thread(s, Path::new("/w"), Some("gpt-y"))?;
            assert_eq!((t.id.as_str(), t.model.as_deref(), t.effort.as_deref()), ("t1", Some("gpt-x"), None));
            Ok(())
        });
        assert_eq!(
            wrote,
            [
                hello.as_str(),
                ready,
                r#"{"id":2,"method":"thread/start","params":{"cwd":"/w","model":"gpt-y"}}"#,
                r#"{"id":3,"method":"thread/inject_items","params":{"items":[{"content":[{"text":"This thread was started from Codealong.","type":"input_text"}],"role":"developer","type":"message"}],"threadId":"t1"}}"#,
            ]
        );
        // A follow-up queued in a thread, as `queue` asks, introduced as taking the experimental API.
        let _ = std::fs::remove_file(&heard);
        serve_as(stand_in(&heard, ok), queue_hello(), |s| add_to_queue(s, "t9", "and the docs", "u1")).unwrap();
        let wrote = std::fs::read_to_string(&heard).unwrap();
        let hello = format!(r#"{{"id":1,"method":"initialize","params":{{"capabilities":{{"experimentalApi":true}},"clientInfo":{{"name":"codealong","title":"Codealong","version":"{}"}}}}}}"#, env!("CARGO_PKG_VERSION"));
        assert_eq!(
            wrote.lines().collect::<Vec<_>>(),
            [hello.as_str(), ready, r#"{"id":2,"method":"thread/queue/add","params":{"clientUserMessageId":"u1","input":[{"text":"and the docs","text_elements":[],"type":"text"}],"threadId":"t9"}}"#]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Turned down, nothing was queued; once Codex had it and said nothing, it may have been.
    #[test]
    fn a_follow_up_codex_went_quiet_on_may_be_queued() {
        let dir = crate::store::test_dir("appserver-queue");
        let heard = dir.join("heard");
        let ask = |answer: &str| serve_as(stand_in(&heard, answer), queue_hello(), |s| add_to_queue(s, "t9", "hi", "u1")).unwrap_err();
        let refused = ask(r#"[ "$id" = 1 ] && echo '{"id":1,"result":{}}' || echo '{"id":2,"error":{"code":-32600,"message":"ephemeral thread does not support queued submissions"}}'"#);
        assert!(!refused.is::<MayHaveArrived>());
        assert_eq!(refused.to_string(), "Codex didn't queue it: ephemeral thread does not support queued submissions");
        assert!(ask(r#"[ "$id" = 1 ] && echo '{"id":1,"result":{}}' || exit 1"#).is::<MayHaveArrived>());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failure_says_what_codex_said() {
        let dir = crate::store::test_dir("appserver-fails");
        let heard = dir.join("heard");
        let ask = |answer: &str| serve(stand_in(&heard, answer), |s| s.request("thread/archive", json!({ "threadId": "t9" }))).unwrap_err().to_string();
        // Its own error, as before.
        assert_eq!(ask(r#"[ "$id" = 1 ] && echo '{"id":1,"result":{}}' || echo '{"id":2,"error":{"code":-32600,"message":"no rollout found for thread id t9"}}'"#), "Codex: no rollout found for thread id t9");
        // What it wrote to stderr as it stopped, which used to be lost.
        assert_eq!(ask("echo 'Error: the state database is locked' >&2; exit 1"), "Codex stopped before it answered: Error: the state database is locked");
        assert_eq!(ask("exit 1"), "Codex stopped before it answered.");
        assert_eq!(serve(Command::new("/nowhere/codex"), |_| Ok(())).unwrap_err().to_string(), "Couldn't start Codex. Is the Codex app installed?");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Against the real Codex app: `CODEALONG_TEST_CODEX_START=<folder> cargo test codex_app_runs
    /// -- --ignored`. Makes a thread in the folder and has the app run a first turn in it, as a
    /// start from Codealong does; the thread stays in Codex, to archive when done.
    #[test]
    #[ignore]
    fn codex_app_runs_a_new_thread() {
        let Ok(folder) = std::env::var("CODEALONG_TEST_CODEX_START") else { return };
        let home = crate::util::home().join(".codex");
        let t = new_thread(&home, Path::new(&folder), None).unwrap();
        let model = t.model.as_deref().map(|m| (m, t.effort.as_deref()));
        crate::codex::ipc::codex_start_turn(&home, &t.id, "Reply with just the word pong.", model).unwrap();
        println!("thread {}", t.id);
    }

    /// Against the Codex app's CLI, in a CODEX_HOME of its own (no sign-in, no turn):
    /// `cargo test queues_a_follow_up_through_codex -- --ignored`. A thread made there takes a
    /// queued follow-up from a server that never loaded it, which another server finds by the id
    /// it went under and takes out, as Send now on it does.
    #[test]
    #[ignore]
    fn queues_a_follow_up_through_codex() {
        let home = crate::store::test_dir("appserver-real-queue");
        let t = new_thread(&home, &home, None).unwrap();
        queue(&home, &t.id, "and the docs", "u1").unwrap();
        let listed = serve_as(app_server(&home).unwrap(), queue_hello(), |s| s.request("thread/queue/list", json!({ "threadId": t.id }))).unwrap();
        assert_eq!(listed["data"][0]["input"], json!([{ "type": "text", "text": "and the docs", "text_elements": [] }]), "{listed}");
        let q = Queue::open(&home).unwrap();
        let found = q.find(&t.id, "u1").unwrap().expect("queued as u1");
        assert_eq!((found.message_id.as_str(), found.prose().as_str()), ("u1", "and the docs"));
        assert_eq!(q.find(&t.id, &found.id).unwrap().as_ref(), Some(&found));
        q.delete(&t.id, &found.id).unwrap();
        assert_eq!(q.find(&t.id, "u1").unwrap(), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Against the real Codex: `CODEALONG_TEST_THREAD=<an archived thread> cargo test -- --ignored`.
    /// Unarchives the thread and archives it again, leaving it as it was.
    #[test]
    #[ignore]
    fn archive_round_trip_through_codex() {
        let Ok(thread) = std::env::var("CODEALONG_TEST_THREAD") else { return };
        let home = crate::util::home().join(".codex");
        let archived = || std::fs::read_dir(home.join("archived_sessions")).unwrap().flatten().any(|e| e.file_name().to_string_lossy().contains(&thread));
        assert!(archived(), "pick a thread that's archived");
        set_archived(&home, &thread, false).unwrap();
        assert!(!archived());
        set_archived(&home, &thread, true).unwrap();
        assert!(archived());
    }
}

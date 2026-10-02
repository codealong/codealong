//! Actions Codex owns, done the way the Codex app does them: a short-lived `codex app-server`
//! over stdio. Archiving a thread moves its rollout into `archived_sessions` and marks it in
//! Codex's shared state database, so the Codex app sees the change the next time it reads its
//! threads. See *Harness adapters*, Archiving.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::procs;
use crate::rpc::{self, Failure};
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
        let left = self.deadline.saturating_duration_since(Instant::now());
        self.client.request(method, params, Some(left)).map_err(|e| match e {
            Failure::Timeout => anyhow!("Codex didn't answer in time."),
            Failure::Closed(said) if said.is_empty() => anyhow!("Codex stopped before it answered."),
            Failure::Closed(said) => anyhow!("Codex stopped before it answered: {}", truncate(&said.join(" "), 300)),
            Failure::Refused(e) => anyhow!("Codex: {}", e.get("message").and_then(Value::as_str).unwrap_or("unknown error")),
            Failure::Write(e) => e.into(),
        })
    }
}

fn with_server<T>(codex_home: &Path, f: impl FnOnce(&Server) -> Result<T>) -> Result<T> {
    let mut cmd = Command::new(codex_binary());
    cmd.args(["app-server", "--listen", "stdio://"]).env("CODEX_HOME", codex_home).env("PATH", procs::user_path());
    serve(cmd, f)
}

/// Runs `f` against the app-server `cmd` starts, once the handshake is done. The server ends
/// with it: the client kills its process as it's dropped. What the server tells or asks
/// meanwhile is left unread.
fn serve<T>(cmd: Command, f: impl FnOnce(&Server) -> Result<T>) -> Result<T> {
    let client = rpc::Client::spawn(cmd, rpc::Stderr::Tail, rpc::Wire::Bare, |_, _| {}).context("Couldn't start Codex. Is the Codex app installed?")?;
    let server = Server { client, deadline: Instant::now() + TIMEOUT };
    server.request(INITIALIZE, initialize_params())?;
    server.client.notify(INITIALIZED, Value::Null)?;
    f(&server)
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
/// (`send::codex_start_turn`), which runs it as if you'd typed it there.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for the app-server: keeps each line it's written in the file named by its
    /// first argument, and answers every request as `answer` has it (`$id` is the request's).
    fn stand_in(heard: &Path, answer: &str) -> Command {
        let script = format!(r#"while IFS= read -r line; do printf '%s\n' "$line" >> "$1"; case "$line" in *'"id":'*) id=${{line#*\"id\":}}; id=${{id%%,*}}; {answer};; esac; done"#);
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", &script, "sh"]).arg(heard);
        cmd
    }

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
        crate::send::codex_start_turn(&home, &t.id, "Reply with just the word pong.", model).unwrap();
        println!("thread {}", t.id);
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

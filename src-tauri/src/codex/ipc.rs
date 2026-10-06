//! The socket the Codex app's windows use to reach each other (`<CODEX_HOME>/ipc/ipc.sock`):
//! starting a turn in a thread, which is a way in (`InApp`), sending a prompt into the turn it's
//! taking, and stopping one, through the window that owns the thread.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use crate::attachments::{self, Style};
use crate::launch;
use crate::send::{opened, sent, Delivery, MayHaveArrived, Own, Sent, Way};

/// A frame on the Codex app's IPC socket: a 4-byte little-endian length, then JSON.
pub fn ipc_frame(message: &Value) -> Vec<u8> {
    let body = message.to_string().into_bytes();
    let mut out = (body.len() as u32).to_le_bytes().to_vec();
    out.extend(body);
    out
}

/// Takes one whole frame off the front of `buf`, if one is there.
pub fn ipc_take(buf: &mut Vec<u8>) -> Option<Value> {
    if buf.len() < 4 {
        return None;
    }
    let n = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if buf.len() < 4 + n {
        return None;
    }
    let frame: Vec<u8> = buf.drain(..4 + n).skip(4).collect();
    Some(serde_json::from_slice(&frame).unwrap_or(Value::Null))
}

/// The request that starts a turn in a thread the app has open, sent to the window that owns it.
/// The app's router drops a request whose `version` isn't the one it expects for the method
/// (2 for this one in 0.155), so a changed app fails here rather than doing something else.
/// A thread with no turns yet needs its model named: the window otherwise sends an empty one,
/// which the API refuses.
pub fn codex_start_request(request_id: &str, client_id: &str, owner: &str, thread: &str, text: &str, message_id: &str, model: Option<(&str, Option<&str>)>) -> Value {
    let mut request = json!({
        "threadId": thread,
        "input": [{ "type": "text", "text": text, "text_elements": [] }],
        "clientUserMessageId": message_id,
    });
    if let Some((m, effort)) = model {
        request["model"] = json!(m);
        if let Some(e) = effort {
            request["effort"] = json!(e);
        }
    }
    json!({
        "type": "request",
        "requestId": request_id,
        "sourceClientId": client_id,
        "version": 2,
        "method": "thread-follower-start-turn",
        "targetClientId": owner,
        "timeoutMs": 10_000,
        "params": {
            "conversationId": thread,
            "turnStart": { "request": request, "context": {} },
        },
    })
}

struct Ipc {
    stream: UnixStream,
    buf: Vec<u8>,
    client_id: String,
}

impl Ipc {
    fn send(&mut self, message: &Value) -> Result<()> {
        self.stream.write_all(&ipc_frame(message))?;
        Ok(())
    }

    /// Waits for the response to `request_id`. The router also asks every client whether it can
    /// handle other clients' requests; Codealong can't, and says so at once, or they'd wait on it.
    fn response(&mut self, request_id: &str, deadline: Instant) -> Result<Value> {
        loop {
            while let Some(m) = ipc_take(&mut self.buf) {
                match m.get("type").and_then(Value::as_str) {
                    Some("response") if m.get("requestId").and_then(Value::as_str) == Some(request_id) => return Ok(m),
                    Some("client-discovery-request") => {
                        let id = m.get("requestId").cloned().unwrap_or(Value::Null);
                        self.send(&json!({ "type": "client-discovery-response", "requestId": id, "response": { "canHandle": false } }))?;
                    }
                    Some("request") => {
                        let id = m.get("requestId").cloned().unwrap_or(Value::Null);
                        self.send(&json!({ "type": "response", "requestId": id, "resultType": "error", "error": "no-handler-for-request" }))?;
                    }
                    _ => {}
                }
            }
            let left = deadline.checked_duration_since(Instant::now()).ok_or_else(|| anyhow!("Codex didn't answer in time."))?;
            self.stream.set_read_timeout(Some(left.max(Duration::from_millis(10))))?;
            let mut chunk = [0u8; 65536];
            let n = self.stream.read(&mut chunk)?;
            if n == 0 {
                bail!("Codex closed the connection.");
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }
}

impl Ipc {
    /// Connects to the socket the Codex app's windows use to reach each other
    /// (`<CODEX_HOME>/ipc/ipc.sock`) and joins it as a client.
    fn connect(codex_home: &Path, deadline: Instant) -> Result<Ipc> {
        let stream = UnixStream::connect(codex_home.join("ipc/ipc.sock")).map_err(|_| anyhow!("The Codex app isn't running."))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let mut ipc = Ipc { stream, buf: Vec::new(), client_id: String::new() };
        let init = crate::start::new_session_id();
        ipc.send(&json!({ "type": "request", "requestId": init, "method": "initialize", "params": { "clientType": "codealong" } }))?;
        let r = ipc.response(&init, deadline)?;
        ipc.client_id = r.pointer("/result/clientId").and_then(Value::as_str).ok_or_else(|| anyhow!("Codex didn't take Codealong as a client."))?.to_string();
        Ok(ipc)
    }

    /// The window that has `thread` loaded, if one answers by `wait`. A window owns only the
    /// threads it has loaded, and asking about any other goes unanswered until the router gives
    /// up ten seconds later.
    fn owner(&mut self, thread: &str, wait: Instant) -> Result<Option<String>> {
        let find = crate::start::new_session_id();
        self.send(&json!({
            "type": "request", "requestId": find, "sourceClientId": self.client_id, "version": 1,
            "method": "thread-owner-discovery", "params": { "hostId": "local", "conversationId": thread },
        }))?;
        Ok(self.response(&find, wait).ok().filter(|r| r.get("resultType").and_then(Value::as_str) == Some("success")).and_then(|r| r.get("handledByClientId").and_then(Value::as_str).map(String::from)))
    }
}

/// Starts a turn with `text` in a Codex thread, through the socket the app's windows use to
/// reach each other. The window that owns the thread runs the turn in the app's own app-server,
/// so the app shows it as if you'd sent it there. A thread no window has loaded is opened in the
/// app after a short wait, which loads it and takes it within a couple of seconds, and asked
/// about again.
pub fn codex_start_turn(codex_home: &Path, thread: &str, text: &str, model: Option<(&str, Option<&str>)>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(18);
    let mut ipc = Ipc::connect(codex_home, deadline)?;
    let client = ipc.client_id.clone();

    let mut opened = false;
    let owner = loop {
        let wait = (Instant::now() + Duration::from_millis(1500)).min(deadline);
        if let Some(owner) = ipc.owner(thread, wait)? {
            if opened {
                // A window takes a thread as it starts loading it, and a turn that arrives
                // before its settings are in goes out with an empty model (refused) and the
                // app's default reviewer. They're in within about a second.
                std::thread::sleep(Duration::from_secs(2));
            }
            break owner;
        }
        if Instant::now() >= deadline {
            bail!("No Codex window took this thread.");
        }
        if !opened {
            launch::open_codex(&launch::codex_thread_url(thread))?;
            opened = true;
        }
        std::thread::sleep(Duration::from_millis(300));
    };

    let turn = crate::start::new_session_id();
    ipc.send(&codex_start_request(&turn, &client, &owner, thread, text, &crate::start::new_session_id(), model))?;
    let r = ipc.response(&turn, deadline)?;
    if r.get("resultType").and_then(Value::as_str) != Some("success") {
        bail!("Codex didn't start the turn: {}", r.get("error").and_then(Value::as_str).unwrap_or("no reason given"));
    }
    Ok(())
}

/// The request that sends a prompt into the turn a thread is taking, sent to the window that
/// owns it: what the app sends when you steer from a window that doesn't (26.924's code; version
/// 1). `restoreMessage` is the composer's message as the app makes one for a plain prompt,
/// which the window reads the thread's folders from and puts back in the box if the steer is
/// turned down.
pub fn codex_steer_request(request_id: &str, client_id: &str, owner: &str, thread: &str, text: &str, message_id: &str, cwd: &str, at: i64) -> Value {
    json!({
        "type": "request",
        "requestId": request_id,
        "sourceClientId": client_id,
        "version": 1,
        "method": "thread-follower-steer-turn",
        "targetClientId": owner,
        "timeoutMs": 10_000,
        "params": {
            "conversationId": thread,
            "clientUserMessageId": message_id,
            "input": [{ "type": "text", "text": text, "text_elements": [] }],
            "attachments": [],
            "restoreMessage": {
                "id": message_id,
                "text": text,
                "context": { "prompt": text, "addedFiles": [], "fileAttachments": [], "ideContext": null, "imageAttachments": [], "workspaceRoots": [cwd] },
                "cwd": cwd,
                "createdAt": at,
            },
        },
    })
}

/// Sends `text` into the turn a Codex thread is taking in the app, through the window that runs
/// it, as the app's own steer does: the turn takes it in at its next step. A thread no window
/// has loaded isn't running in the app, and one whose turn has ended is turned down; either way
/// nothing went. No answer once it's asked may mean it went (`MayHaveArrived`).
pub fn codex_steer_turn(codex_home: &Path, thread: &str, text: &str, cwd: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut ipc = Ipc::connect(codex_home, deadline)?;
    let Some(owner) = ipc.owner(thread, (Instant::now() + Duration::from_secs(3)).min(deadline))? else {
        bail!("No Codex window has this thread open, so it isn't running in the app.");
    };
    let id = crate::start::new_session_id();
    ipc.send(&codex_steer_request(&id, &ipc.client_id.clone(), &owner, thread, text, &crate::start::new_session_id(), cwd, crate::util::now_ms()))?;
    let unknown = |why: &str| MayHaveArrived(format!("Codealong asked Codex to send this into its running turn and didn't hear how it went ({why}): look there before sending it again."));
    let r = ipc.response(&id, deadline).map_err(|e| unknown(&e.to_string()))?;
    if r.get("resultType").and_then(Value::as_str) != Some("success") {
        let why = r.get("error").and_then(Value::as_str).unwrap_or("no reason given");
        // The router says these once it has handed the request to the window, which may go on
        // with it; its other refusals, and the window's own, come before anything is sent.
        if matches!(why, "request-timeout" | "client-disconnected" | "server-closed") {
            return Err(unknown(why).into());
        }
        bail!("Codex didn't take it into the turn: {why}");
    }
    Ok(())
}

/// The request that stops the turn a thread is taking, sent to the window that owns it: what
/// the app's own Stop sends from a window that doesn't (`user-stop`, which also pauses the
/// thread's goal and stops its subagents). Without the turn's id the router expects version 3;
/// with one, 4 (26.924's code).
pub fn codex_interrupt_request(request_id: &str, client_id: &str, owner: &str, thread: &str) -> Value {
    json!({
        "type": "request",
        "requestId": request_id,
        "sourceClientId": client_id,
        "version": 3,
        "method": "thread-follower-interrupt-turn",
        "targetClientId": owner,
        "timeoutMs": 10_000,
        "params": { "conversationId": thread, "mode": "user-stop" },
    })
}

/// Stops the turn a Codex thread is taking in the app, through the window that runs it. A thread
/// that's running there is loaded in some window, so one that no window claims isn't running in
/// the app.
pub fn codex_interrupt_turn(codex_home: &Path, thread: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut ipc = Ipc::connect(codex_home, deadline)?;
    let Some(owner) = ipc.owner(thread, (Instant::now() + Duration::from_secs(3)).min(deadline))? else {
        bail!("No Codex window has this thread open, so it isn't running in the app.");
    };
    let id = crate::start::new_session_id();
    ipc.send(&codex_interrupt_request(&id, &ipc.client_id.clone(), &owner, thread))?;
    let r = ipc.response(&id, deadline)?;
    if r.get("resultType").and_then(Value::as_str) != Some("success") {
        bail!("Codex didn't stop the turn: {}", r.get("error").and_then(Value::as_str).unwrap_or("no reason given"));
    }
    Ok(())
}

/// The way in for a thread the app has: a turn started through the app's own windows, as if
/// you'd sent it there, else the app opens the thread with the prompt filled in to send.
#[derive(Debug, Clone, PartialEq)]
pub struct InApp;

impl Own for InApp {
    fn way(&self) -> Way {
        Way::CodexApp
    }

    fn takes_agents(&self) -> bool {
        true
    }

    fn deliver(&self, d: &Delivery) -> Result<Sent> {
        let title = d.title();
        let prompt = attachments::expand(d.text, Style::Path);
        // Going on there with nothing to say yet (`Adapter::app_way`): the thread opens.
        if prompt.trim().is_empty() {
            launch::open_codex(&launch::codex_thread_url(&d.session.id))?;
            return Ok(opened(format!("Codex has {title} open.")));
        }
        if d.now {
            match codex_steer_turn(&d.account.home_path(), &d.session.id, &prompt, &d.session.cwd) {
                Ok(()) => return Ok(sent(format!("Sent now to {title} in Codex: it takes it into the turn it's taking."))),
                Err(e) if e.is::<MayHaveArrived>() => return Err(e),
                // Its turn ended meanwhile, or no window runs it: a turn starts on it, as any
                // send does.
                Err(_) => {}
            }
        }
        match codex_start_turn(&d.account.home_path(), &d.session.id, &prompt, None) {
            Ok(()) => Ok(sent(if d.busy() { format!("Sent to {title} in Codex; it takes it after its current turn.") } else { format!("Sent to {title} in Codex.") })),
            // A prompt filled in for someone to send isn't a message an agent can leave.
            Err(e) if d.from_agent() => bail!("Codex didn't take it for {title} ({e}), so nothing was sent."),
            Err(_) => {
                // The app isn't running, didn't take the thread, or has changed how its
                // windows talk: its own link fills the reply in instead.
                launch::open_codex(&format!("{}?prompt={}", launch::codex_thread_url(&d.session.id), urlencoding::encode(&prompt)))?;
                Ok(opened(format!("Codex has {title} open with the prompt filled in; send it there.")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AccountCfg;
    use crate::model::{Harness, Session, Status, Surface};
    use std::path::PathBuf;

    /// A folder of its own under the system's temporary one, short enough for a socket path.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ca-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn stops_a_codex_turn_through_the_window_that_owns_the_thread() {
        let home = scratch("codex");
        std::fs::create_dir_all(home.join("ipc")).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(home.join("ipc/ipc.sock")).unwrap();
        let router = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                while let Some(m) = ipc_take(&mut buf) {
                    let id = m["requestId"].clone();
                    let answer = match m["method"].as_str() {
                        Some("initialize") => json!({ "type": "response", "requestId": id, "resultType": "success", "result": { "clientId": "me" } }),
                        Some("thread-owner-discovery") => json!({ "type": "response", "requestId": id, "resultType": "success", "handledByClientId": "window" }),
                        _ => {
                            conn.write_all(&ipc_frame(&json!({ "type": "response", "requestId": id, "resultType": "success", "result": { "ok": true } }))).unwrap();
                            return m;
                        }
                    };
                    conn.write_all(&ipc_frame(&answer)).unwrap();
                }
                let n = conn.read(&mut chunk).unwrap();
                assert!(n > 0, "Codealong hung up before asking to stop");
                buf.extend_from_slice(&chunk[..n]);
            }
        });
        codex_interrupt_turn(&home, "t1").unwrap();
        let asked = router.join().unwrap();
        assert_eq!(asked["method"], "thread-follower-interrupt-turn");
        assert_eq!((&asked["version"], &asked["sourceClientId"], &asked["targetClientId"]), (&json!(3), &json!("me"), &json!("window")));
        assert_eq!(asked["params"], json!({ "conversationId": "t1", "mode": "user-stop" }));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn frames_codex_ipc_messages() {
        let m = json!({ "type": "request", "requestId": "1" });
        let mut buf = ipc_frame(&m);
        assert_eq!(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize, buf.len() - 4);
        buf.extend(ipc_frame(&json!({ "n": 2 })));
        let half = buf.split_off(buf.len() - 3);
        assert_eq!(ipc_take(&mut buf), Some(m));
        assert_eq!(ipc_take(&mut buf), None);
        buf.extend(half);
        assert_eq!(ipc_take(&mut buf), Some(json!({ "n": 2 })));
        assert!(buf.is_empty());
        let r = codex_start_request("r", "me", "owner", "t1", "hello", "u1", None);
        assert_eq!(r["version"], 2);
        assert_eq!(r["targetClientId"], "owner");
        assert_eq!(r["params"]["turnStart"]["request"]["input"][0], json!({ "type": "text", "text": "hello", "text_elements": [] }));
        assert!(r["params"]["turnStart"]["request"].get("model").is_none());
        let r = codex_start_request("r", "me", "owner", "t1", "hello", "u1", Some(("gpt-x", Some("high"))));
        assert_eq!((&r["params"]["turnStart"]["request"]["model"], &r["params"]["turnStart"]["request"]["effort"]), (&json!("gpt-x"), &json!("high")));
    }

    /// What the Codex app's windows are written, byte for byte.
    #[test]
    fn the_codex_apps_windows_are_written_these_frames() {
        assert_eq!(ipc_frame(&json!({ "a": 1 })), b"\x07\x00\x00\x00{\"a\":1}");
        assert_eq!(
            codex_start_request("r1", "me", "window", "t1", "hello", "u1", Some(("gpt-x", Some("high")))).to_string(),
            concat!(
                r#"{"method":"thread-follower-start-turn","params":{"conversationId":"t1","turnStart":{"context":{},"request":{"clientUserMessageId":"u1","effort":"high","#,
                r#""input":[{"text":"hello","text_elements":[],"type":"text"}],"model":"gpt-x","threadId":"t1"}}},"#,
                r#""requestId":"r1","sourceClientId":"me","targetClientId":"window","timeoutMs":10000,"type":"request","version":2}"#
            )
        );
        assert_eq!(
            codex_interrupt_request("r2", "me", "window", "t1").to_string(),
            r#"{"method":"thread-follower-interrupt-turn","params":{"conversationId":"t1","mode":"user-stop"},"requestId":"r2","sourceClientId":"me","targetClientId":"window","timeoutMs":10000,"type":"request","version":3}"#
        );
    }

    /// Everything a turn's start writes on the windows' socket, in order, the ids Codealong makes
    /// up aside: it joins, finds the window that has the thread (telling the router meanwhile
    /// that it handles nothing itself), and starts the turn there.
    #[test]
    fn starts_a_codex_turn_through_the_window_that_owns_the_thread() {
        let home = scratch("codex-start");
        std::fs::create_dir_all(home.join("ipc")).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(home.join("ipc/ipc.sock")).unwrap();
        let router = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let mut heard = Vec::new();
            loop {
                while let Some(m) = ipc_take(&mut buf) {
                    let id = m["requestId"].clone();
                    let method = m["method"].as_str().map(String::from);
                    heard.push(m);
                    let mut say = |v: Value| conn.write_all(&ipc_frame(&v)).unwrap();
                    match method.as_deref() {
                        Some("initialize") => say(json!({ "type": "response", "requestId": id, "resultType": "success", "result": { "clientId": "me" } })),
                        Some("thread-owner-discovery") => {
                            say(json!({ "type": "client-discovery-request", "requestId": "d1" }));
                            say(json!({ "type": "request", "requestId": "q1", "method": "something-else" }));
                            say(json!({ "type": "response", "requestId": id, "resultType": "success", "handledByClientId": "window" }));
                        }
                        Some("thread-follower-start-turn") => {
                            say(json!({ "type": "response", "requestId": id, "resultType": "success", "result": {} }));
                            return heard;
                        }
                        _ => {}
                    }
                }
                let n = conn.read(&mut chunk).unwrap();
                assert!(n > 0, "Codealong hung up before starting the turn");
                buf.extend_from_slice(&chunk[..n]);
            }
        });
        codex_start_turn(&home, "t1", "hello", None).unwrap();
        let mut heard = router.join().unwrap();
        // The ids are made up afresh each time; the answers to the router's own keep its ids.
        for m in heard.iter_mut().filter(|m| m.get("method").is_some()) {
            m["requestId"] = json!("id");
        }
        heard[4]["params"]["turnStart"]["request"]["clientUserMessageId"] = json!("id");
        let wrote: Vec<String> = heard.iter().map(Value::to_string).collect();
        assert_eq!(
            wrote,
            [
                r#"{"method":"initialize","params":{"clientType":"codealong"},"requestId":"id","type":"request"}"#,
                r#"{"method":"thread-owner-discovery","params":{"conversationId":"t1","hostId":"local"},"requestId":"id","sourceClientId":"me","type":"request","version":1}"#,
                r#"{"requestId":"d1","response":{"canHandle":false},"type":"client-discovery-response"}"#,
                r#"{"error":"no-handler-for-request","requestId":"q1","resultType":"error","type":"response"}"#,
                r#"{"method":"thread-follower-start-turn","params":{"conversationId":"t1","turnStart":{"context":{},"request":{"clientUserMessageId":"id","input":[{"text":"hello","text_elements":[],"type":"text"}],"threadId":"t1"}}},"requestId":"id","sourceClientId":"me","targetClientId":"window","timeoutMs":10000,"type":"request","version":2}"#,
            ]
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn asks_the_codex_window_to_steer_as_the_app_does() {
        let r = codex_steer_request("r", "me", "owner", "t1", "and the docs", "u1", "/r/app", 1_790_000_000_000);
        assert_eq!((&r["method"], &r["version"], &r["targetClientId"], &r["sourceClientId"]), (&json!("thread-follower-steer-turn"), &json!(1), &json!("owner"), &json!("me")));
        let p = &r["params"];
        assert_eq!(p["conversationId"], "t1");
        assert_eq!(p["input"], json!([{ "type": "text", "text": "and the docs", "text_elements": [] }]));
        assert_eq!(p["attachments"], json!([]));
        // The window reads the thread's folders from the message it would put back in the box,
        // and its id is the message's.
        let m = &p["restoreMessage"];
        assert_eq!((&m["id"], &p["clientUserMessageId"]), (&json!("u1"), &json!("u1")));
        assert_eq!((&m["text"], &m["cwd"], &m["createdAt"]), (&json!("and the docs"), &json!("/r/app"), &json!(1_790_000_000_000i64)));
        assert_eq!(m["context"], json!({ "prompt": "and the docs", "addedFiles": [], "fileAttachments": [], "ideContext": null, "imageAttachments": [], "workspaceRoots": ["/r/app"] }));
    }

    #[test]
    fn an_agent_leaves_no_codex_prompt_for_someone_to_send() {
        // No Codex app answers here, so the turn can't start; what you send would open the
        // thread with the prompt filled in, an agent's goes nowhere.
        let account: AccountCfg = serde_json::from_value(json!({ "name": "codex", "harness": "codex", "home": "/nonexistent/codex" })).unwrap();
        let s = Session { surface: Surface::App, status: Status::Idle, live: true, title: "Fix the build".into(), cwd: "/tmp".into(), ..Session::new(Harness::Codex, "codex", "abc") };
        let err = InApp.deliver(&Delivery { session: &s, account: &account, text: "hi", agent: Some("codealong"), bypasses: false, now: false, untyped: None }).map(|_| ()).unwrap_err();
        assert!(err.to_string().contains("nothing was sent"), "{err}");
    }
}

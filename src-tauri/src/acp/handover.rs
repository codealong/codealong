//! Handing live ACP adapter processes and their sessions across a server `exec` (serve.rs
//! `hand_over` / `take_handover`), so an engine relaunch doesn't cut off running hosted agents.
//!
//! Each account runs one adapter process (`Agent`) over stdin/stdout pipes (`rpc::Client`). On
//! `hand_over`, we briefly wait for any short control RPCs (`session/new`, `session/load`,
//! `session/set_config_option`, `session/delete`, permission answers) to finish, then freeze each
//! clean adapter's pipes (`rpc::Client::hand_over`) and serialize its sessions (including in-flight
//! `session/prompt` request IDs, pending permission requests, queued prompts, and queued
//! model/effort switches). Any account that can't be handed over cleanly (still mid-control-RPC,
//! upgraded npm adapter packages on the receiving side, or an adapter told in `initialize` what
//! another build of Codealong takes, which it goes by for good) is skipped and falls back to the
//! existing `hosted.json` `cut_off` + `Carry on` path.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{client_capabilities, flush_queued, hear, notify, package_stamp, resume_turn, state, stopped, Agent, Change, Claim, Hosted, Permission, State, HANDING_OVER};
use crate::model::Harness;
use crate::os::procs::Fd;
use crate::rpc;

/// How long `hand_over` waits for short control RPCs (`session/load`, `session/set_config_option`,
/// permission answers, etc.) to finish before freezing each adapter.
const CONTROL_WAIT: Duration = Duration::from_secs(2);

/// How long `hand_over` waits for a `run_turn` thread whose `session/prompt` answer arrived right
/// as the reader was stopping to finish recording its turn result in memory.
const TURN_SETTLE_WAIT: Duration = Duration::from_millis(500);

/// One session's in-memory state carried across a server `exec`.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct HandedSession {
    key: String,
    id: String,
    cwd: String,
    loaded: bool,
    held: bool,
    running: bool,
    prompt_id: Option<u64>,
    since: i64,
    permissions: Vec<Permission>,
    queue: Vec<String>,
    pending_model: Option<String>,
    pending_effort: Option<String>,
    text: String,
    tool: Option<String>,
    error: Option<String>,
    options: Value,
    remembered: bool,
    cut_off: Option<i64>,
}

/// One account's running adapter process and its sessions as handed across a server `exec`.
/// `stdin_fd` and `stdout_fd` travel via `HandOverSocket` (`serve.rs`) rather than JSON.
#[derive(Serialize, Deserialize, Debug)]
pub struct Handed {
    pub account: String,
    who: Option<(Harness, PathBuf)>,
    packages: Option<String>,
    /// What Codealong told it it takes (`client_capabilities`), which it goes by while it runs
    /// (claude-agent-acp leaves `AskUserQuestion` out of every session for a client that takes
    /// no forms). None from a server before it was written down.
    #[serde(default)]
    told: Option<Value>,
    pub pid: u32,
    caps: Value,
    next_id: u64,
    pending_ids: Vec<u64>,
    unread: Vec<u8>,
    sessions: Vec<HandedSession>,
    #[serde(skip)]
    pub stdin_fd: Fd,
    #[serde(skip)]
    pub stdout_fd: Fd,
}

impl Handed {
    fn close_fds(&mut self) {
        for fd in [&mut self.stdin_fd, &mut self.stdout_fd] {
            if *fd >= 0 {
                // SAFETY: each non-negative descriptor is uniquely owned by this `Handed`.
                unsafe { crate::os::pty::close(*fd) };
                *fd = -1;
            }
        }
    }
}

/// Whether any ACP adapter process is currently running and alive, which a server replaced by a
/// new build hands over (`server.rs` `Request::HandOver`).
pub fn any_running() -> bool {
    state().agents.values().any(|a| !a.client.is_closed())
}

/// Whether `account` has any short control RPC or transition in flight that prevents a clean
/// hand-over right now. Long-running `session/prompt` RPCs with a recorded `prompt_id` are fine.
fn has_control_in_flight(st: &State, account: &str, ag: &Agent) -> bool {
    let mut prompt_ids: HashSet<u64> = HashSet::new();
    for h in st.sessions.values().filter(|h| h.account == account) {
        if h.deleting || h.replaying || h.permissions.iter().any(|p| p.answering) {
            return true;
        }
        if h.running {
            match h.prompt_id {
                Some(id) => {
                    prompt_ids.insert(id);
                }
                // `begin` ran, `session/load` or `session/prompt` write hasn't completed yet.
                None => return true,
            }
        }
    }
    ag.client.pending_ids().into_iter().any(|id| !prompt_ids.contains(&id))
}

/// Hands every live ACP adapter process and its loaded/held sessions to the next server build
/// (`serve.rs::hand_over`). Sets `HANDING_OVER` so no new turns or control RPCs begin meanwhile.
pub fn hand_over() -> Vec<Handed> {
    HANDING_OVER.store(true, Ordering::SeqCst);
    hand_over_matching(|_| true)
}

fn hand_over_matching(pick: impl Fn(&str) -> bool) -> Vec<Handed> {
    let candidates: Vec<(String, Arc<Agent>)> = {
        let st = state();
        st.agents.iter().filter(|(name, ag)| pick(name) && !ag.client.is_closed()).map(|(name, ag)| (name.clone(), ag.clone())).collect()
    };
    if candidates.is_empty() {
        return Vec::new();
    }
    let until = Instant::now() + CONTROL_WAIT;
    while Instant::now() < until {
        let busy = {
            let st = state();
            candidates.iter().any(|(name, ag)| !ag.client.is_closed() && has_control_in_flight(&st, name, ag))
        };
        if !busy {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let mut out = Vec::new();
    for (account, ag) in candidates {
        let ready = {
            let st = state();
            !ag.client.is_closed() && !has_control_in_flight(&st, &account, &ag)
        };
        if !ready {
            eprintln!("ACP adapter for {account} had a control RPC in flight; skipping hand-over.");
            continue;
        }
        let Some(client) = ag.client.hand_over() else { continue };
        let pending_set: HashSet<u64> = client.pending_ids.iter().copied().collect();
        // If a `session/prompt` response arrived right as the stdout reader stopped, wait briefly
        // for `run_turn_loop` to record the turn end and clear `running`/`prompt_id`.
        let settle_until = Instant::now() + TURN_SETTLE_WAIT;
        let settled = loop {
            let settling = {
                let st = state();
                st.sessions.values().any(|h| h.account == account && h.running && h.prompt_id.is_none_or(|id| !pending_set.contains(&id)))
            };
            if !settling {
                break true;
            }
            if Instant::now() >= settle_until {
                break false;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut handed = Handed {
            account: account.clone(),
            who: ag.who.clone(),
            packages: ag.packages.clone(),
            // Every adapter this server runs was told what it says now: it started one, or took
            // one over only when it had been told the same (`take_over`).
            told: Some(client_capabilities()),
            pid: client.pid,
            caps: ag.caps.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            next_id: client.next_id,
            pending_ids: client.pending_ids,
            unread: client.unread,
            sessions: Vec::new(),
            stdin_fd: client.stdin_fd,
            stdout_fd: client.stdout_fd,
        };
        if !settled {
            eprintln!("ACP adapter for {account} didn't settle in time; skipping hand-over.");
            handed.close_fds();
            continue;
        }
        let (sessions, valid) = {
            let mut st = state();
            let mut prompt_ids: HashSet<u64> = HashSet::new();
            let mut ok = true;
            let mut list = Vec::new();
            for (key, h) in st.sessions.iter().filter(|(_, h)| h.account == account && (h.loaded || h.held)) {
                if h.deleting || h.replaying || h.permissions.iter().any(|p| p.answering) {
                    ok = false;
                    break;
                }
                if h.running {
                    match h.prompt_id {
                        Some(id) if pending_set.contains(&id) => {
                            prompt_ids.insert(id);
                        }
                        _ => {
                            ok = false;
                            break;
                        }
                    }
                }
                list.push(HandedSession {
                    key: key.clone(),
                    id: h.id.clone(),
                    cwd: h.cwd.clone(),
                    loaded: h.loaded,
                    held: h.held,
                    running: h.running,
                    prompt_id: h.prompt_id,
                    since: h.since,
                    permissions: h.permissions.clone(),
                    queue: h.queue.clone(),
                    pending_model: h.pending_model.clone(),
                    pending_effort: h.pending_effort.clone(),
                    text: h.text.clone(),
                    tool: h.tool.clone(),
                    error: h.error.clone(),
                    options: h.options.clone(),
                    remembered: h.remembered,
                    cut_off: h.cut_off,
                });
            }
            if ok && handed.pending_ids.iter().any(|id| !prompt_ids.contains(id)) {
                ok = false;
            }
            if ok {
                st.agents.remove(&account);
            }
            (list, ok)
        };
        if !valid {
            eprintln!("ACP adapter for {account} changed state during hand-over; skipping.");
            handed.close_fds();
            continue;
        }
        handed.sessions = sessions;
        out.push(handed);
    }
    out
}

/// Takes over ACP adapter processes and their sessions handed across an `exec` (`serve.rs`).
/// Must run before the engine's first board scan so handed-over active turns clear the `cut_off`
/// mark `load_remembered` reads from `hosted.json`. Returns how many adapters were taken over.
pub fn take_over(handed: Vec<Handed>) -> usize {
    let expected_packages = package_stamp();
    let told = client_capabilities();
    let mut taken = 0;
    for mut h in handed {
        if h.stdin_fd < 0 || h.stdout_fd < 0 {
            h.close_fds();
            let pid = h.pid;
            std::thread::spawn(move || crate::os::pty::reap(pid));
            continue;
        }
        if h.told.as_ref() != Some(&told) {
            eprintln!("ACP adapter for {} was told another build of Codealong's capabilities; letting it restart.", h.account);
            h.close_fds();
            let pid = h.pid;
            std::thread::spawn(move || crate::os::pty::reap(pid));
            continue;
        }
        if h.packages.as_deref().is_some_and(|was| was != expected_packages) {
            eprintln!("ACP adapter for {} was built for different package versions; letting it restart.", h.account);
            h.close_fds();
            let pid = h.pid;
            std::thread::spawn(move || crate::os::pty::reap(pid));
            continue;
        }
        let of = h.account.clone();
        // SAFETY: `serve.rs::take_handover` received both fds from `HandOverSocket` and passed
        // exclusive ownership here.
        let restored = unsafe {
            rpc::Client::from_handed(h.pid, h.stdin_fd, h.stdout_fd, h.next_id, &h.pending_ids, h.unread, rpc::Wire::JsonRpc2, move |link, heard| match heard {
                rpc::Heard::Line(msg) => hear(&of, link, msg),
                rpc::Heard::Closed(_) => stopped(&of, link),
            })
        };
        h.stdin_fd = -1;
        h.stdout_fd = -1;
        let (client, mut waiters) = match restored {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("Couldn't take ACP adapter for {} over: {e}", h.account);
                let pid = h.pid;
                std::thread::spawn(move || crate::os::pty::reap(pid));
                continue;
            }
        };
        let ag = Arc::new(Agent { account: h.account.clone(), who: h.who, packages: h.packages, client, caps: Mutex::new(h.caps) });
        let mut to_resume = Vec::new();
        let mut to_flush = Vec::new();
        {
            let mut st = state();
            st.agents.insert(h.account.clone(), ag.clone());
            for s in h.sessions {
                if s.running {
                    if let Some((req_id, rx)) = s.prompt_id.and_then(|id| waiters.remove(&id).map(|rx| (id, rx))) {
                        let turn = Claim::already_sent();
                        st.sessions.insert(
                            s.key.clone(),
                            Hosted {
                                account: h.account.clone(),
                                id: s.id,
                                cwd: s.cwd,
                                loaded: s.loaded,
                                held: s.held,
                                deleting: false,
                                running: true,
                                turn: Some(turn.clone()),
                                prompt_id: Some(req_id),
                                since: s.since,
                                permissions: s.permissions,
                                queue: s.queue,
                                pending_model: s.pending_model,
                                pending_effort: s.pending_effort,
                                replaying: false,
                                text: s.text,
                                tool: s.tool,
                                error: s.error,
                                options: s.options,
                                remembered: s.remembered,
                                cut_off: None,
                            },
                        );
                        to_resume.push((s.key, turn, req_id, rx, s.since));
                    } else {
                        st.sessions.insert(
                            s.key,
                            Hosted {
                                account: h.account.clone(),
                                id: s.id,
                                cwd: s.cwd,
                                loaded: s.loaded,
                                held: s.held,
                                deleting: false,
                                running: false,
                                turn: None,
                                prompt_id: None,
                                since: s.since,
                                permissions: Vec::new(),
                                queue: Vec::new(),
                                pending_model: None,
                                pending_effort: None,
                                replaying: false,
                                text: String::new(),
                                tool: None,
                                error: s.error,
                                options: s.options,
                                remembered: s.remembered,
                                cut_off: Some(s.since),
                            },
                        );
                    }
                } else {
                    let has_queued = s.pending_model.is_some() || s.pending_effort.is_some() || !s.queue.is_empty();
                    let key = s.key.clone();
                    st.sessions.insert(
                        s.key,
                        Hosted {
                            account: h.account.clone(),
                            id: s.id,
                            cwd: s.cwd,
                            loaded: s.loaded,
                            held: s.held,
                            deleting: false,
                            running: false,
                            turn: None,
                            prompt_id: None,
                            since: s.since,
                            permissions: s.permissions,
                            queue: s.queue,
                            pending_model: s.pending_model,
                            pending_effort: s.pending_effort,
                            replaying: false,
                            text: s.text,
                            tool: s.tool,
                            error: s.error,
                            options: s.options,
                            remembered: s.remembered,
                            cut_off: s.cut_off,
                        },
                    );
                    if has_queued {
                        to_flush.push(key);
                    }
                }
            }
        }
        for (key, turn, req_id, rx, began) in to_resume {
            resume_turn(ag.clone(), key, turn, req_id, rx, began);
        }
        for key in to_flush {
            flush_queued(ag.clone(), key);
        }
        taken += 1;
    }
    if taken > 0 {
        notify(Change::Board);
    }
    taken
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::process::Command;

    fn fake_adapter(account: &str, script: &str) -> Arc<Agent> {
        let mut cmd = Command::new(crate::procs::posix_tool("sh"));
        cmd.args(["-c", script]);
        let log = std::fs::OpenOptions::new().write(true).open("/dev/null").unwrap();
        let ag = super::super::connect(account, cmd, log).unwrap();
        *ag.caps.lock().unwrap() = json!({ "sessionCapabilities": { "resume": {} } });
        state().agents.insert(account.to_string(), ag.clone());
        ag
    }

    fn wait_until(timeout: Duration, cond: impl Fn() -> bool) -> bool {
        let until = Instant::now() + timeout;
        while Instant::now() < until {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// Handing over an ACP adapter with an active turn, a pending permission request, and a queued
    /// follow-up prompt keeps the adapter alive, answers the permission request on the resumed
    /// agent, finishes the active turn without marking `cut_off`, and runs the queued follow-up.
    #[cfg(unix)]
    #[test]
    fn handing_over_an_active_acp_turn_preserves_permissions_streaming_and_queue() {
        let account = "test-acp-handover";
        let key = format!("antigravity:{account}:s-handover");
        // Fake ACP adapter:
        // 1. Reads first `session/prompt`, asks a permission request (`id: 99`).
        // 2. Waits for the permission response (which arrives AFTER hand-over!), then streams a
        //    chunk and finishes prompt 1.
        // 3. Reads the queued second `session/prompt`, streams "queued ok", and answers it.
        let script = r#"
read p1
id1=${p1#*\"id\":}; id1=${id1%%,*}
printf '{"jsonrpc":"2.0","id":99,"method":"session/request_permission","params":{"sessionId":"s-handover","toolCall":{"toolCallId":"call-1","title":"Run test"},"options":[{"optionId":"allow","kind":"allow_once","name":"Allow"}]}}\n'
read perm
printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s-handover","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"after handover"}}}}\n'
printf '{"jsonrpc":"2.0","id":%s,"result":{"stopReason":"end_turn"}}\n' "$id1"
read p2
id2=${p2#*\"id\":}; id2=${id2%%,*}
printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s-handover","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"queued ok"}}}}\n'
printf '{"jsonrpc":"2.0","id":%s,"result":{"stopReason":"end_turn"}}\n' "$id2"
cat >/dev/null
"#;
        let ag = fake_adapter(account, script);
        let pid = ag.client.pid();
        let turn = {
            let mut st = state();
            let mut h = Hosted { account: account.into(), id: "s-handover".into(), cwd: "/w".into(), loaded: true, held: true, ..Default::default() };
            let turn = super::super::begin(&mut h);
            h.queue.push("follow up".into());
            st.sessions.insert(key.clone(), h);
            turn
        };
        super::super::run_turn(ag.clone(), key.clone(), "first prompt".into(), turn);

        // Wait until the prompt id is recorded and the permission request has arrived.
        assert!(
            wait_until(Duration::from_secs(5), || {
                let st = state();
                st.sessions.get(&key).is_some_and(|h| h.prompt_id.is_some() && !h.permissions.is_empty())
            }),
            "prompt and permission request did not arrive"
        );

        // Hand over this test account and round-trip its JSON manifest as `serve.rs` does.
        let handed = hand_over_matching(|a| a == account);
        assert_eq!(handed.len(), 1);
        let (stdin_fd, stdout_fd) = (handed[0].stdin_fd, handed[0].stdout_fd);
        let json = serde_json::to_vec(&handed).unwrap();
        let mut restored: Vec<Handed> = serde_json::from_slice(&json).unwrap();
        restored[0].stdin_fd = stdin_fd;
        restored[0].stdout_fd = stdout_fd;
        // Clear in-memory session to simulate a fresh server process (with a stale `cut_off` from
        // hosted.json that `take_over` must clear).
        state().sessions.insert(key.clone(), Hosted { account: account.into(), id: "s-handover".into(), cwd: "/w".into(), remembered: true, cut_off: Some(1234), ..Default::default() });
        drop(ag);
        assert!(crate::procs::pid_alive(pid), "adapter must stay alive after hand_over");

        assert_eq!(take_over(restored), 1);
        {
            let st = state();
            let h = &st.sessions[&key];
            assert!(h.running, "turn must still be running after take_over");
            assert_eq!(h.cut_off, None, "take_over must clear hosted.json cut_off");
            assert_eq!(h.permissions.len(), 1, "pending permission must survive take_over");
            assert_eq!(h.queue, vec!["follow up"]);
        }

        // Answer the permission request on the resumed server; both turns should complete cleanly.
        super::super::answer(&key, "call-1", crate::harness::Answer::Allow).unwrap();
        assert!(
            wait_until(Duration::from_secs(5), || {
                let st = state();
                st.sessions.get(&key).is_some_and(|h| !h.running && h.queue.is_empty())
            }),
            "resumed turn and queued follow-up did not finish"
        );
        {
            let st = state();
            let h = &st.sessions[&key];
            assert_eq!(h.cut_off, None);
            assert_eq!(h.error, None);
        }

        // Clean up the test adapter and session.
        state().sessions.remove(&key);
        let ag = state().agents.remove(account);
        drop(ag);
        assert!(wait_until(Duration::from_secs(5), || !crate::procs::pid_alive(pid)), "adapter should exit when dropped");
    }

    /// An adapter handed over with a stale npm package stamp is rejected on `take_over` so the new
    /// server restarts it with the upgraded packages instead of keeping the old version.
    #[cfg(unix)]
    #[test]
    fn take_over_skips_adapter_when_package_stamp_changed() {
        let account = "test-acp-pkg-mismatch";
        let ag = fake_adapter(account, "cat >/dev/null");
        let pid = ag.client.pid();
        let mut handed = hand_over_matching(|a| a == account);
        assert_eq!(handed.len(), 1);
        handed[0].packages = Some("old-package@0.0.1".into());
        drop(ag);

        assert_eq!(take_over(handed), 0);
        assert!(!state().agents.contains_key(account));
        // Closing its stdin causes `cat` to exit and `take_over`'s reaper reaps it.
        assert!(wait_until(Duration::from_secs(5), || !crate::procs::pid_alive(pid)));
    }

    /// An adapter told in `initialize` what another build of Codealong takes, or handed over by a
    /// server from before that was written down, goes by that while it runs (claude-agent-acp
    /// leaves `AskUserQuestion` out for a client that takes no forms), so it's let go and restarts.
    #[cfg(unix)]
    #[test]
    fn take_over_skips_adapter_told_other_capabilities() {
        let account = "test-acp-told-mismatch";
        let ag = fake_adapter(account, "cat >/dev/null");
        let pid = ag.client.pid();
        let mut handed = hand_over_matching(|a| a == account);
        assert_eq!(handed.len(), 1);
        assert_eq!(handed[0].told, Some(client_capabilities()));
        handed[0].told = None;
        drop(ag);

        assert_eq!(take_over(handed), 0);
        assert!(!state().agents.contains_key(account));
        assert!(wait_until(Duration::from_secs(5), || !crate::procs::pid_alive(pid)));
    }

    /// An account with an in-flight non-prompt control RPC (such as `session/load` or
    /// `session/set_config_option`) is detected by `has_control_in_flight` and not handed over
    /// mid-handshake.
    #[cfg(unix)]
    #[test]
    fn control_rpc_in_flight_is_detected() {
        let account = "test-acp-control-inflight";
        let ag = fake_adapter(account, "cat >/dev/null");
        let pid = ag.client.pid();
        {
            let st = state();
            assert!(!has_control_in_flight(&st, account, &ag));
        }
        let (_id, _rx) = ag.client.start_request_if("session/set_config_option", json!({}), None, || true).unwrap().unwrap();
        {
            let st = state();
            assert!(has_control_in_flight(&st, account, &ag));
        }
        let removed = state().agents.remove(account);
        drop((ag, removed));
        assert!(wait_until(Duration::from_secs(5), || !crate::procs::pid_alive(pid)));
    }
}

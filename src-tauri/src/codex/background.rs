//! The commands a Codex thread left running in background terminals, as its terminal UI's `/ps`
//! lists them (0.162.1): a command still running when its `exec_command` call returned, which
//! the agent can poll or type into (`write_stdin`) by the process id its result gave.
//!
//! What the rollout says of one (0.162.1 on this Mac, 2026-11-12):
//! - The call: a `function_call` `exec_command` with JSON arguments (`{"cmd": …}`), or in code
//!   mode an `exec` `custom_tool_call` whose `input` is JavaScript calling
//!   `tools.exec_command({cmd:"…"})`, often several at once in a `Promise.allSettled([…])`.
//!   Every rollout here was code mode's.
//! - Its result: a direct call's output says "Process running with session ID 19957" (or
//!   "Process exited with code 0" once it's done). Code mode's is what the script printed, as
//!   `input_text` parts: scripts here print each call's result as JSON, `{"i":0,"status":
//!   "fulfilled","value":{"chunk_id","wall_time_seconds","session_id":19957,"output"}}`, `i` the
//!   call's place in the script, with `exit_code` in place of `session_id` once it's done. What a
//!   script prints is up to the agent, so a result it didn't print says nothing.
//! - Its end: an `event_msg` `item_completed` whose item is a `CommandExecution` with
//!   `process_id` "19957", `status` (`completed`, `failed`) and `exit_code`, written as the process
//!   exits whether or not the agent was polling it (the item's `completed_at_ms` is the line's
//!   time). Foreground commands get one too. A script's output is written once all of it is done,
//!   so a command that ran past its yield and ended before the script did has its item written
//!   before the output that says it still ran (`Terminals::done`).

use std::path::Path;

use serde_json::Value;

use crate::util::{parse_rfc3339_ms, scan_lines_back};

/// Most commands kept running per thread: each ends with an `item_completed`, so any past a
/// few are ones whose end was never read.
const RUNNING_MOST: usize = 16;

/// Most commands kept once they ended, for the windows' list of what ended a little while ago.
const ENDED_MOST: usize = 8;

/// Most calls kept waiting for their results.
const CALLS_MOST: usize = 32;

/// Most ended processes remembered (`Terminals::done`): a script's calls end within it.
const DONE_MOST: usize = 64;

/// The background terminals a thread runs and ran (`step`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Terminals {
    /// Running, oldest first.
    pub running: Vec<LeftRunning>,
    /// Ended, the latest last.
    pub ended: Vec<LeftRunning>,
    /// Its latest command calls with no result yet, which name what a result says still runs.
    pub calls: Vec<Call>,
    /// The processes whose items said they ended, the latest last: a script's output can say one
    /// still ran after its item said it ended (module header).
    pub done: Vec<String>,
}

/// A command left running in a background terminal.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LeftRunning {
    /// Codex's process id for it, which `write_stdin` and the `CommandExecution` name.
    pub id: String,
    /// When its call's result said it still ran.
    pub at: i64,
    /// Its call's `cmd`, or "" where the call wasn't read or its command wasn't a plain string.
    pub command: String,
    /// When it ended, how (the item's `status`) and its exit code.
    pub ended: Option<(i64, String, Option<i64>)>,
}

/// A command call until its result: the commands it starts, in order (`None` where one isn't a
/// plain string), and the processes it polls.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    id: String,
    commands: Vec<Option<String>>,
    polls: Vec<String>,
}

/// What a result says of a process: its place among the call's commands where the script said,
/// its process id while it runs, its exit code once it's done.
#[derive(Debug, Default, PartialEq)]
struct Said {
    index: Option<usize>,
    process: Option<String>,
    exit_code: Option<i64>,
    /// What it printed since the agent last read it.
    output: Option<String>,
}

/// Whether a rollout line can start, name or end a background terminal, before it's parsed.
pub fn could_step(line: &str) -> bool {
    ["\"name\":\"exec\"", "\"name\":\"exec_command\"", "\"name\":\"write_stdin\"", "session_id", "Process running with session ID", "Process exited with code"].iter().any(|m| line.contains(m))
        || (line.contains("\"CommandExecution\"") && line.contains("\"process_id\""))
}

/// Follows a thread's background terminals through a rollout line (module header).
pub fn step(t: &mut Terminals, v: &Value, at: Option<i64>) {
    let Some(p) = v.get("payload") else { return };
    let kind = p.get("type").and_then(Value::as_str).unwrap_or_default();
    let call_id = p.get("call_id").and_then(Value::as_str).unwrap_or_default();
    match (v.get("type").and_then(Value::as_str), kind) {
        (Some("response_item"), "function_call" | "custom_tool_call") => {
            let input = p.get("input").or_else(|| p.get("arguments")).and_then(Value::as_str).unwrap_or_default();
            let args = || serde_json::from_str::<Value>(input).ok();
            let (commands, polls) = match p.get("name").and_then(Value::as_str) {
                Some("exec") => (js_commands(input), js_polls(input)),
                Some("exec_command") => (vec![args().and_then(|a| Some(a.get("cmd")?.as_str()?.to_string()))], Vec::new()),
                Some("write_stdin") => (Vec::new(), args().and_then(|a| a.get("session_id").map(process_id)).into_iter().flatten().collect()),
                _ => return,
            };
            t.calls.retain(|c| c.id != call_id);
            t.calls.push(Call { id: call_id.to_string(), commands, polls });
            if t.calls.len() > CALLS_MOST {
                t.calls.remove(0);
            }
        }
        (Some("response_item"), "function_call_output" | "custom_tool_call_output") => {
            let Some(i) = t.calls.iter().position(|c| c.id == call_id) else { return };
            let call = t.calls.remove(i);
            let Some(at) = at else { return };
            for said in results(p.get("output").unwrap_or(&Value::Null)) {
                let polled = || (call.polls.len() == 1).then(|| call.polls[0].clone());
                match (said.process.or_else(polled), said.exit_code) {
                    (Some(id), Some(code)) => end(t, &id, at, if code == 0 { "completed" } else { "failed" }, Some(code)),
                    (Some(id), None) if !t.running.iter().any(|r| r.id == id) && !t.done.contains(&id) => {
                        let named = match said.index {
                            Some(i) => call.commands.get(i).cloned().flatten(),
                            None if call.commands.len() == 1 => call.commands[0].clone(),
                            None => None,
                        };
                        t.running.push(LeftRunning { id, at, command: named.unwrap_or_default(), ended: None });
                        if t.running.len() > RUNNING_MOST {
                            t.running.remove(0);
                        }
                    }
                    _ => {}
                }
            }
        }
        (Some("event_msg"), "item_completed") => {
            let Some(item) = p.get("item").filter(|i| i.get("type").and_then(Value::as_str) == Some("CommandExecution")) else { return };
            let Some(id) = item.get("process_id").and_then(process_id) else { return };
            let at = p.get("completed_at_ms").and_then(Value::as_i64).or(at).unwrap_or_default();
            let status = item.get("status").and_then(Value::as_str).unwrap_or("completed");
            end(t, &id, at, status, item.get("exit_code").and_then(Value::as_i64));
            t.done.retain(|d| d != &id);
            t.done.push(id);
            if t.done.len() > DONE_MOST {
                t.done.remove(0);
            }
        }
        _ => {}
    }
}

/// Ends a running terminal, keeping a few that ended.
fn end(t: &mut Terminals, id: &str, at: i64, status: &str, exit_code: Option<i64>) {
    let Some(i) = t.running.iter().position(|r| r.id == id) else { return };
    let mut ended = t.running.remove(i);
    ended.ended = Some((at, status.to_string(), exit_code));
    t.ended.push(ended);
    if t.ended.len() > ENDED_MOST {
        t.ended.remove(0);
    }
}

/// `step` over lines gathered last first.
pub fn replay(t: &mut Terminals, last_first: &[Value]) {
    for v in last_first.iter().rev() {
        step(t, v, v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms));
    }
}

/// A process id as the rollout writes it: a number in a result, a string in an item.
fn process_id(v: &Value) -> Option<String> {
    match v {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// What a command call's output says of the processes it ran (module header): a direct call's
/// words, or the JSON a code-mode script printed, a line at a time.
fn results(output: &Value) -> Vec<Said> {
    let texts: Vec<&str> = match output {
        Value::String(s) => vec![s.as_str()],
        Value::Array(parts) => parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect(),
        _ => Vec::new(),
    };
    let mut found = Vec::new();
    for text in texts {
        let number = |after: &str| text.split(after).nth(1).and_then(|r| r.split(|c: char| c != '-' && !c.is_ascii_digit()).next()).filter(|n| !n.is_empty()).map(String::from);
        let output = text.split_once("\nOutput:\n").map(|(_, o)| o.to_string());
        if let Some(id) = number("Process running with session ID ") {
            found.push(Said { process: Some(id), output, ..Said::default() });
            continue;
        }
        if let Some(code) = number("Process exited with code ").and_then(|c| c.parse().ok()) {
            found.push(Said { exit_code: Some(code), output, ..Said::default() });
            continue;
        }
        for line in text.lines().map(str::trim).filter(|l| l.starts_with('{') || l.starts_with('[')) {
            match serde_json::from_str::<Value>(line) {
                Ok(Value::Array(all)) => found.extend(all.iter().enumerate().filter_map(|(i, v)| said(v.get("value").unwrap_or(v), Some(i)))),
                Ok(v) => {
                    let index = v.get("i").and_then(Value::as_u64).map(|i| i as usize);
                    found.extend(said(v.get("value").unwrap_or(&v), index));
                }
                Err(_) => {}
            }
        }
    }
    found
}

/// One `exec_command` or `write_stdin` result, as a script printed it.
fn said(v: &Value, index: Option<usize>) -> Option<Said> {
    let process = v.get("session_id").and_then(process_id);
    let exit_code = v.get("exit_code").and_then(Value::as_i64);
    let output = v.get("output").and_then(Value::as_str).map(String::from);
    (process.is_some() || exit_code.is_some()).then_some(Said { index, process, exit_code, output })
}

/// How far back a rollout is read for what a background terminal printed (`output_of`).
const OUTPUT_BACK: u64 = 8 * 1024 * 1024;

/// What the command in background terminal `id` printed, as the rollout has it: all of it once
/// it ended (its `CommandExecution`'s `aggregated_output`), else what the results that named it
/// gave the agent, each what came since the agent last read it, put together. "" where the
/// rollout keeps none of it.
pub fn output_of(rollout: &Path, id: &str) -> String {
    let len = std::fs::metadata(rollout).map(|m| m.len()).unwrap_or(0);
    let (mut whole, mut parts) = (None, Vec::new());
    scan_lines_back(rollout, len.saturating_sub(OUTPUT_BACK), len, |line| {
        if !line.contains(id) {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
        let Some(p) = v.get("payload") else { return false };
        if let Some(item) = p.get("item").filter(|i| i.get("type").and_then(Value::as_str) == Some("CommandExecution") && i.get("process_id").and_then(process_id).as_deref() == Some(id)) {
            whole = item.get("aggregated_output").and_then(Value::as_str).map(String::from);
            return whole.is_some();
        }
        if matches!(p.get("type").and_then(Value::as_str), Some("function_call_output" | "custom_tool_call_output")) {
            let said: Vec<String> = results(p.get("output").unwrap_or(&Value::Null)).into_iter().filter(|s| s.process.as_deref() == Some(id)).filter_map(|s| s.output).collect();
            parts.extend(said.into_iter().rev());
        }
        false
    });
    whole.unwrap_or_else(|| parts.into_iter().rev().collect())
}

/// The command a shell Codex runs one in is running: `/bin/zsh -c <command>`, or `-lc` for a
/// login shell, as `ps` gives the arguments joined (0.162.1 on this Mac). None for any other
/// process.
pub fn shell_script(cmdline: &str) -> Option<String> {
    let (program, rest) = cmdline.split_once(' ')?;
    let shell = program.rsplit('/').next().is_some_and(|p| matches!(p, "zsh" | "bash" | "sh" | "fish"));
    let script = rest.strip_prefix("-c ").or_else(|| rest.strip_prefix("-lc "))?;
    shell.then(|| script.to_string())
}

/// The commands a code-mode script starts, in the order it calls `exec_command`: each call's
/// `cmd` where it's a plain string literal, `None` where it's worked out (`[…].join(" ")`, a
/// variable, a template with `${}`).
fn js_commands(js: &str) -> Vec<Option<String>> {
    js.split("exec_command(")
        .skip(1)
        .map(|call| {
            let call = call.trim_start().strip_prefix('{')?;
            let key = call.find("cmd")?;
            let rest = call[key + 3..].trim_start_matches(['"', '\'']).trim_start().strip_prefix(':')?.trim_start();
            js_string(rest)
        })
        .collect()
}

/// The processes a code-mode script polls or types into: each `write_stdin({session_id: N})`.
fn js_polls(js: &str) -> Vec<String> {
    js.split("write_stdin(")
        .skip(1)
        .filter_map(|call| {
            let rest = &call[call.find("session_id")? + "session_id".len()..];
            let digits: String = rest.trim_start_matches(['"', '\'']).trim_start().strip_prefix(':')?.trim_start().chars().take_while(char::is_ascii_digit).collect();
            Some(digits).filter(|d| !d.is_empty())
        })
        .collect()
}

/// The JavaScript string literal `s` starts with, unescaped; `None` for anything else, or a
/// template literal that works part of itself out.
fn js_string(s: &str) -> Option<String> {
    let mut chars = s.chars();
    let quote = chars.next().filter(|q| matches!(q, '"' | '\'' | '`'))?;
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '0' => out.push('\0'),
                '\n' => {}
                'u' => {
                    let rest = chars.as_str();
                    let (hex, skip) = match rest.strip_prefix('{') {
                        Some(braced) => (braced.split('}').next()?, braced.find('}')? + 2),
                        None => (rest.get(..4)?, 4),
                    };
                    out.push(char::from_u32(u32::from_str_radix(hex, 16).ok()?)?);
                    chars = rest[skip..].chars();
                }
                'x' => {
                    let rest = chars.as_str();
                    out.push(char::from_u32(u32::from_str_radix(rest.get(..2)?, 16).ok()?)?);
                    chars = rest[2..].chars();
                }
                other => out.push(other),
            },
            '$' if quote == '`' && chars.as_str().starts_with('{') => return None,
            c if c == quote => return Some(out),
            c => out.push(c),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(ts: &str, kind: &str, payload: Value) -> Value {
        json!({"timestamp": ts, "type": kind, "payload": payload})
    }

    #[test]
    fn reads_the_commands_a_script_starts() {
        let js = r#"const r=await Promise.allSettled([
tools.exec_command({cmd:"npm run dev -- --port 4321",login:false,yield_time_ms:10000}),
tools.exec_command({ "cmd": 'echo \'hi\' && printf "a\\tb"', max_output_tokens: 4000 }),
tools.exec_command({cmd:["git","status"].join(" ")}),
tools.exec_command({cmd:`ls ${dir}`}),
tools.exec_command({cmd:`tail -f log \u00e9`}),
]);
text(await tools.write_stdin({session_id:19957,chars:"",yield_time_ms:1000}));"#;
        assert_eq!(js_commands(js), [Some("npm run dev -- --port 4321".to_string()), Some("echo 'hi' && printf \"a\\tb\"".to_string()), None, None, Some("tail -f log é".to_string())]);
        assert_eq!(js_polls(js), ["19957"]);
        assert_eq!(js_string(r#""unterminated"#), None);
    }

    #[test]
    fn follows_a_command_left_running_until_it_exits() {
        let mut t = Terminals::default();
        let js = r#"const r=await Promise.allSettled([tools.exec_command({cmd:"npm run dev"}), tools.exec_command({cmd:"git status"})]); r.forEach((x,i)=>text(JSON.stringify({i,...x})));"#;
        let printed = |items: &[&str]| {
            let mut parts = vec![json!({"type": "input_text", "text": "Script completed\nWall time 10.2 seconds\nOutput:\n"})];
            parts.extend(items.iter().map(|t| json!({"type": "input_text", "text": t})));
            Value::Array(parts)
        };
        let lines = [
            line("2026-11-12T18:54:27.000Z", "response_item", json!({"type": "custom_tool_call", "call_id": "c1", "name": "exec", "input": js})),
            line("2026-11-12T18:54:37.000Z", "response_item", json!({"type": "custom_tool_call_output", "call_id": "c1", "output": printed(&[
                r#"{"i":0,"status":"fulfilled","value":{"chunk_id":"99c57a","wall_time_seconds":10.0,"session_id":19957,"output":"ready on http://localhost:4321\n"}}"#,
                r#"{"i":1,"status":"fulfilled","value":{"chunk_id":"fb77ba","wall_time_seconds":0.2,"exit_code":0,"output":"clean\n"}}"#,
            ])})),
        ];
        for v in &lines {
            step(&mut t, v, v["timestamp"].as_str().and_then(parse_rfc3339_ms));
        }
        let at = parse_rfc3339_ms("2026-11-12T18:54:37.000Z").unwrap();
        assert_eq!(t.running, [LeftRunning { id: "19957".into(), at, command: "npm run dev".into(), ended: None }]);
        assert!(t.calls.is_empty());
        // Polled, it still runs; a foreground command's end isn't one's.
        let poll = line("2026-11-12T18:54:48.000Z", "response_item", json!({"type": "custom_tool_call", "call_id": "c2", "name": "exec", "input": "text(await tools.write_stdin({session_id:19957,chars:\"\"}));"}));
        let still = line("2026-11-12T18:54:53.000Z", "response_item", json!({"type": "custom_tool_call_output", "call_id": "c2", "output": printed(&[r#"{"chunk_id":"debc77","wall_time_seconds":5.0,"session_id":19957,"original_token_count":0,"output":""}"#])}));
        let other = line("2026-11-12T18:54:54.000Z", "event_msg", json!({"type": "item_completed", "completed_at_ms": 1, "item": {"type": "CommandExecution", "process_id": "72499", "status": "completed", "exit_code": 0}}));
        replay(&mut t, &[other, still, poll]);
        assert_eq!(t.running.len(), 1);
        // Its item says it ended, when and how, whether or not the agent was polling.
        let done = line("2026-11-12T18:54:57.789Z", "event_msg", json!({"type": "item_completed", "started_at_ms": 1794423267744i64, "completed_at_ms": 1794423297789i64, "item": {"type": "CommandExecution", "process_id": "19957", "status": "failed", "exit_code": 2}}));
        step(&mut t, &done, None);
        assert!(t.running.is_empty());
        assert_eq!(t.ended[0].ended, Some((1794423297789, "failed".to_string(), Some(2))));
        // A script whose command ran past its yield and ended before the script did: its item
        // comes first, and the output that says it still ran doesn't start it (seen 2026-10-19).
        let mut t = Terminals::default();
        let first = line("2026-10-19T22:00:36.188Z", "event_msg", json!({"type": "item_completed", "completed_at_ms": 1792447236187i64, "item": {"type": "CommandExecution", "process_id": "44020", "status": "completed", "exit_code": 0}}));
        let then = line("2026-10-19T22:00:37.215Z", "response_item", json!({"type": "custom_tool_call_output", "call_id": "c1", "output": printed(&[r#"{"i":0,"status":"fulfilled","value":{"chunk_id":"a","wall_time_seconds":1.0,"session_id":44020,"output":""}}"#])}));
        replay(&mut t, &[then, first, lines[0].clone()]);
        assert!(t.running.is_empty() && t.ended.is_empty());
    }

    #[test]
    fn reads_what_a_background_terminal_printed() {
        let dir = crate::fixtures::scratch("codex-terminal-output");
        let path = dir.join("rollout.jsonl");
        let printed = |text: &str| json!([{"type": "input_text", "text": "Script completed\nOutput:\n"}, {"type": "input_text", "text": text}]);
        let start = json!({"i": 0, "status": "fulfilled", "value": {"chunk_id": "a", "session_id": 19957, "output": "compiling\n"}}).to_string();
        let poll = json!({"chunk_id": "b", "session_id": 19957, "output": "ready on http://localhost:4321\n"}).to_string();
        let other = json!({"chunk_id": "c", "session_id": 55001, "output": "not this one\n"}).to_string();
        let lines = [
            line("2026-11-12T18:54:37.000Z", "response_item", json!({"type": "custom_tool_call_output", "call_id": "c1", "output": printed(&start)})),
            line("2026-11-12T18:54:40.000Z", "response_item", json!({"type": "custom_tool_call_output", "call_id": "c2", "output": printed(&other)})),
            line("2026-11-12T18:54:53.000Z", "response_item", json!({"type": "custom_tool_call_output", "call_id": "c3", "output": printed(&poll)})),
        ];
        let write = |lines: &[Value]| std::fs::write(&path, lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
        write(&lines);
        assert_eq!(output_of(&path, "19957"), "compiling\nready on http://localhost:4321\n");
        // Once it ended, its item has all of it.
        let done = line("2026-11-12T18:55:00.000Z", "event_msg", json!({"type": "item_completed", "item": {"type": "CommandExecution", "process_id": "19957", "aggregated_output": "compiling\nready\nbye\n", "exit_code": 0}}));
        write(&[lines[0].clone(), lines[1].clone(), lines[2].clone(), done]);
        assert_eq!(output_of(&path, "19957"), "compiling\nready\nbye\n");
        assert_eq!(output_of(&path, "1"), "");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_the_command_a_shell_runs() {
        assert_eq!(shell_script("/bin/zsh -c codealong session transcript claude:x --full").as_deref(), Some("codealong session transcript claude:x --full"));
        assert_eq!(shell_script("/bin/zsh -lc npm run dev").as_deref(), Some("npm run dev"));
        assert_eq!((shell_script("node server.mjs -c x"), shell_script("/bin/zsh")), (None, None));
    }

    #[test]
    fn reads_a_direct_calls_words() {
        let mut t = Terminals::default();
        let lines = [
            line("2026-11-12T18:00:00.000Z", "response_item", json!({"type": "function_call", "call_id": "d1", "name": "exec_command", "arguments": "{\"cmd\":\"cargo watch -x test\",\"yield_time_ms\":10000}"})),
            line("2026-11-12T18:00:10.000Z", "response_item", json!({"type": "function_call_output", "call_id": "d1", "output": "Chunk ID: 1a2b3c\nWall time: 10.0012 seconds\nProcess running with session ID 4242\nOriginal token count: 12\nOutput:\nwatching\n"})),
        ];
        replay(&mut t, &[lines[1].clone(), lines[0].clone()]);
        assert_eq!((t.running[0].id.as_str(), t.running[0].command.as_str()), ("4242", "cargo watch -x test"));
        // A poll's words end it, as the item does.
        let poll = line("2026-11-12T18:01:00.000Z", "response_item", json!({"type": "function_call", "call_id": "d2", "name": "write_stdin", "arguments": "{\"session_id\":4242,\"chars\":\"\\u0003\"}"}));
        let exited = line("2026-11-12T18:01:01.000Z", "response_item", json!({"type": "function_call_output", "call_id": "d2", "output": "Chunk ID: 4d5e6f\nWall time: 0.5 seconds\nProcess exited with code 130\nOutput:\n"}));
        replay(&mut t, &[exited, poll]);
        assert!(t.running.is_empty());
        assert_eq!(t.ended[0].ended.as_ref().map(|(_, s, c)| (s.as_str(), *c)), Some(("failed", Some(130))));
        // A result for a call never read, or a script that printed nothing of it, starts none.
        let lone = line("2026-11-12T18:02:00.000Z", "response_item", json!({"type": "function_call_output", "call_id": "zz", "output": "Process running with session ID 1"}));
        step(&mut t, &lone, Some(1));
        assert!(t.running.is_empty());
    }
}

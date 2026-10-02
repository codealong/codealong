//! A transcript as the session panel's conversation: prompts, replies, tool calls and their
//! results, other sessions' messages, and the prompts Claude Code took back.

use std::collections::HashSet;
use std::path::Path;

use serde_json::Value;

use super::ClaudeCode;
use super::peer::{FROM_CODEALONG, FROM_YOU};
use super::registry::session_entry;
use super::transcript::{read_range, tool_request, typed_text, you_typed};
use crate::transcript::{clip_input, clip_output, item, kept_whole, text_of, Image, Item};
use crate::util::{one_line, parse_rfc3339_ms};

/// One transcript line as conversation items (transcript.rs): a prompt you typed, the agent's
/// text and tool calls, a tool's result by the call's id, and notes.
pub fn conversation_items(v: &Value, at: i64) -> Vec<Item> {
    let flag = |k: &str| v.get(k).and_then(Value::as_bool) == Some(true);
    if flag("isSidechain") {
        return Vec::new();
    }
    match v.get("type").and_then(Value::as_str) {
        Some("user") => {
            if flag("isCompactSummary") {
                return vec![item(at, "note", "The conversation was compacted here")];
            }
            if let Some(peer) = line_peer(v) {
                return vec![peer_item(at, &peer)];
            }
            let images = images_of(v.pointer("/message/content"), v.get("imagePasteIds"));
            if let Some(t) = typed_text(v) {
                return vec![Item { images, ..item(at, "you", t) }];
            }
            // A prompt that was only images, as the Claude app sends one, says nothing else.
            if !images.is_empty() && you_typed(v) && text_of(v.pointer("/message/content").unwrap_or(&Value::Null)).trim().is_empty() {
                return vec![Item { images, ..item(at, "you", "") }];
            }
            if flag("isMeta") {
                return Vec::new();
            }
            let Some(parts) = v.pointer("/message/content").and_then(Value::as_array) else { return Vec::new() };
            let results: Vec<&Value> = parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("tool_result")).collect();
            // The line's `toolUseResult` is its one result's: for questions, what you answered,
            // and for an artifact, where it is.
            let answers = if results.len() == 1 { answers_of(v.pointer("/toolUseResult/answers")) } else { Default::default() };
            let link = if results.len() == 1 { artifact_of(v.get("toolUseResult")) } else { None };
            results
                .into_iter()
                .map(|p| Item {
                    id: p.get("tool_use_id").and_then(Value::as_str).map(String::from),
                    output: Some(clip_output(&text_of(p.get("content").unwrap_or(&Value::Null)))),
                    failed: p.get("is_error").and_then(Value::as_bool) == Some(true),
                    answers: answers.clone(),
                    link: link.clone(),
                    ..item(at, "result", "")
                })
                .collect()
        }
        // A message you sent while it worked: Claude Code writes it when the turn takes it in, as
        // a `queued_command` attachment rather than a prompt. Background tasks' notifications
        // and other sessions' messages come the same way, wrapped in a tag.
        Some("attachment") => {
            if v.pointer("/attachment/type").and_then(Value::as_str) != Some("queued_command") {
                return Vec::new();
            }
            if let Some(peer) = line_peer(v) {
                return vec![peer_item(at, &peer)];
            }
            let t = text_of(v.pointer("/attachment/prompt").unwrap_or(&Value::Null));
            let t = t.trim();
            if t.is_empty() || t.starts_with('<') {
                return Vec::new();
            }
            vec![Item { images: images_of(v.pointer("/attachment/prompt"), v.pointer("/attachment/imagePasteIds")), ..item(at, "you", t) }]
        }
        Some("assistant") => {
            let Some(parts) = v.pointer("/message/content").and_then(Value::as_array) else { return Vec::new() };
            let mut out = Vec::new();
            for p in parts {
                match p.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        let t = p.get("text").and_then(Value::as_str).unwrap_or("").trim();
                        if !t.is_empty() {
                            out.push(item(at, "agent", t));
                        }
                    }
                    Some("tool_use") => {
                        let name = p.get("name").and_then(Value::as_str).unwrap_or("Tool");
                        let input = p.get("input").cloned().unwrap_or(Value::Null);
                        let whole = claude_input(name, &input);
                        let shell = name == "Bash" && !whole.trim().is_empty() && kept_whole(&whole);
                        out.push(Item {
                            id: p.get("id").and_then(Value::as_str).map(String::from),
                            input: Some(clip_input(&whole)),
                            questions: if name == super::asks::QUESTIONS { questions_of(&input) } else { Vec::new() },
                            shell,
                            // The line's folder is the shell's, where the command ran.
                            cwd: shell.then(|| v.get("cwd").and_then(Value::as_str).map(String::from)).flatten(),
                            ..item(at, "tool", tool_request(name, &input))
                        });
                    }
                    _ => {}
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

/// The images a prompt came with, which Claude Code keeps in the prompt's own line: a screenshot
/// pasted into the terminal, which the text names `[Image #N]` by the line's `imagePasteIds`
/// (numbered through the session, in the images' order), or one attached in the Claude app,
/// which the text doesn't name.
fn images_of(content: Option<&Value>, paste_ids: Option<&Value>) -> Vec<Image> {
    let parts = content.and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    let data: Vec<&str> = parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("image")).filter_map(|p| p.pointer("/source/data").and_then(Value::as_str)).collect();
    let numbers: Vec<u32> = paste_ids.and_then(Value::as_array).map(|ids| ids.iter().filter_map(|n| n.as_u64().map(|n| n as u32)).collect()).unwrap_or_default();
    // The numbers are the images' only when there's one for each.
    let numbered = numbers.len() == data.len();
    data.iter().enumerate().map(|(i, d)| Image { number: numbered.then(|| numbers[i]), base64: d.to_string() }).collect()
}

/// A message from another session, as Claude Code records it: `origin: {kind: "peer", name,
/// body}` on the `user` line that started a turn with it, or on the `queued_command` attachment
/// by which a busy turn took it in. `body` is the text inside the message's envelope; a Claude
/// Code that doesn't keep it has only the envelope, in the line's own text.
pub(super) struct Peer {
    name: String,
    body: String,
}

pub(super) fn line_peer(v: &Value) -> Option<Peer> {
    let (origin, text) = match v.get("type").and_then(Value::as_str) {
        Some("user") => (v.get("origin")?, v.pointer("/message/content")),
        Some("attachment") => (v.pointer("/attachment/origin")?, v.pointer("/attachment/prompt")),
        _ => return None,
    };
    // A subagent's report back comes the same way; like a background task's notification, it
    // isn't a message anyone sent.
    if origin.get("kind").and_then(Value::as_str) != Some("peer") || origin.get("handback").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let body = match origin.get("body").and_then(Value::as_str) {
        Some(b) => b.to_string(),
        None => envelope_body(&text_of(text.unwrap_or(&Value::Null)))?,
    };
    // Older lines name the sender in `from`; newer ones say `unknown` there and use `name`.
    let name = [origin.get("name"), origin.get("from")].into_iter().flatten().filter_map(Value::as_str).find(|n| !n.is_empty() && *n != "unknown").unwrap_or("");
    Some(Peer { name: name.to_string(), body: body.replace("<\\/cross-session-message", "</cross-session-message") })
}

/// The text inside a `<cross-session-message …>` envelope.
fn envelope_body(text: &str) -> Option<String> {
    let start = text.find("<cross-session-message")?;
    let inside = start + text[start..].find('>')? + 1;
    let end = inside + text[inside..].find("</cross-session-message>")?;
    Some(text[inside..end].trim().to_string())
}

/// What you sent from Codealong, as the text you wrote. A Claude app session takes it as another
/// session's message (peer.rs, `peer_content`), from Codealong, with a first line saying it's
/// from you, which an agent's message never starts with. It went with its attachments' full
/// paths; they read as references again, as they did in the box you sent it from.
pub(super) fn sent_from_codealong(peer: &Peer) -> Option<String> {
    // The first line as it is now, and as Codealong worded it until 2026-09-28.
    const YOURS: [&str; 2] = [FROM_YOU, "Your user typed this in Codealong's quick panel and sent it to this session:"];
    if peer.name != FROM_CODEALONG {
        return None;
    }
    let body = peer.body.trim_start();
    let text = YOURS.iter().find_map(|first| body.strip_prefix(first))?.trim();
    (!text.is_empty()).then(|| crate::attachments::shorten(text))
}

/// Another session's message in the conversation: yours when Codealong sent it for you, else a
/// note of who it's from and what it says.
fn peer_item(at: i64, peer: &Peer) -> Item {
    if let Some(text) = sent_from_codealong(peer) {
        return item(at, "you", text);
    }
    let from = if peer.name.is_empty() { String::new() } else { format!(", “{}”", peer.name) };
    item(at, "note", format!("From another session{from}: {}", one_line(crate::send::without_byline(&peer.body), 200)))
}

/* ---- Prompts Claude Code took back ---- */

// Escape pressed before the agent has written anything back, with the input box empty, doesn't
// interrupt the turn as it does later: Claude Code (2.1.285's `auto_restore_cancel`) winds the
// conversation back to before the prompt and puts the prompt's text back in its input box, so
// the agent never sees it. Nothing in the transcript says so. The next line written goes on
// from the line before the prompt (its `parentUuid` is outside the prompt's own lines), and
// until one is, the session's registry says idle since after the prompt, with nothing back.

/// How far past a prompt the lines that settle it are read: its attachments (a session's first
/// prompt carries the whole system prompt), then a reply or whatever went on without it.
const FATE_SCAN: u64 = 2 * 1024 * 1024;

/// What became of a prompt, by the lines written after it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Fate {
    /// The agent had it: it answered, or the turn was interrupted with the prompt kept.
    Kept,
    /// The conversation went on without it.
    TakenBack,
    /// Nothing after it settles it yet.
    Open,
}

/// What became of the prompt on the first of `lines`, and when it was sent; none when that line
/// isn't a prompt of yours.
fn fate_of<'a>(mut lines: impl Iterator<Item = &'a str>) -> Option<(Fate, i64)> {
    let first: Value = serde_json::from_str(lines.next()?).ok()?;
    if first.get("type").and_then(Value::as_str) != Some("user") {
        return None;
    }
    typed_text(&first)?;
    let at = first.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
    let mut ours: HashSet<String> = HashSet::from([first.get("uuid")?.as_str()?.to_string()]);
    for line in lines {
        // Titles, modes and file snapshots aren't in the conversation's chain.
        if !line.contains("\"parentUuid\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(id) = v.get("uuid").and_then(Value::as_str) else { continue };
        match v.get("parentUuid").and_then(Value::as_str) {
            // A chain that starts again, as compaction's does, isn't a rewind.
            None => return Some((Fate::Kept, at)),
            Some(parent) if !ours.contains(parent) => return Some((Fate::TakenBack, at)),
            Some(_) => {}
        }
        match v.get("type").and_then(Value::as_str) {
            Some("assistant") => return Some((Fate::Kept, at)),
            // Escape with text in the box interrupts instead, keeping the prompt.
            Some("user") if text_of(v.pointer("/message/content").unwrap_or(&Value::Null)).trim_start().starts_with("[Request interrupted") => return Some((Fate::Kept, at)),
            // A session's first prompt taken back leaves nothing before it to go on from, so the
            // next prompt follows on from its lines, often with its text again.
            Some("user") if typed_text(&v).is_some() => return Some((Fate::TakenBack, at)),
            _ => {}
        }
        ours.insert(id.to_string());
    }
    Some((Fate::Open, at))
}

/// Whether Claude Code took back the prompt whose line starts at `cursor`. `idle_since` is when
/// the session's registry says it last went idle, for a session whose Escape can take one back.
fn taken_back(path: &Path, cursor: u64, idle_since: Option<i64>) -> bool {
    let Ok(len) = std::fs::metadata(path).map(|m| m.len()) else { return false };
    let end = len.min(cursor + FATE_SCAN);
    let Some(text) = read_range(path, cursor, end) else { return false };
    // Whole lines only: the last may still be being written, or cut off by the scan's end.
    let Some(whole) = text.rfind('\n').map(|i| &text[..i]) else { return false };
    match fate_of(whole.lines()) {
        Some((Fate::TakenBack, _)) => true,
        Some((Fate::Open, at)) => end == len && idle_since.is_some_and(|t| t > at),
        _ => false,
    }
}

/// When a live session last went idle, by its registry, when it's idle now. Only Claude Code's
/// own screen takes a prompt back, so the Claude app's sessions aren't asked about.
pub(super) fn idle_since(home: &Path, s: &crate::model::Session) -> Option<i64> {
    if s.surface == crate::model::Surface::App {
        return None;
    }
    let reg = session_entry(home, s)?;
    if reg.status != "idle" {
        return None;
    }
    reg.status_updated_at
}

/// Marks the prompts among `items` that Claude Code took back. Only a prompt with nothing from
/// the agent after it can have been, so the others aren't read again.
pub(super) fn mark_taken_back(path: &Path, items: &mut [Item], idle_since: Option<i64>) {
    for i in 0..items.len() {
        if items[i].kind != "you" {
            continue;
        }
        let answered = items[i + 1..].iter().take_while(|it| it.kind != "you").any(|it| matches!(it.kind, "agent" | "tool"));
        if !answered && taken_back(path, items[i].cursor, idle_since) {
            items[i].withdrawn = true;
        }
    }
}

/// Where the last prompt before `after` starts, when nothing has come back for it before
/// `after`: it may have been taken back since, so a read of what's newer starts there again.
pub(super) fn open_prompt_before(path: &Path, after: u64) -> Option<u64> {
    let page = crate::transcript::file_page(path, conversation_items, Some(after), 1).ok()?;
    page.items.last().filter(|it| it.kind == "you").map(|it| it.cursor)
}

/// Whether a live terminal session's latest prompt was taken back and nothing was sent since,
/// with the session idle: Claude Code may have put the prompt's text back in its input box,
/// where something typed would be added to it (send.rs clears the box first).
pub fn last_prompt_taken_back(account: &crate::config::AccountCfg, s: &crate::model::Session) -> bool {
    use crate::harness::Adapter;
    let Some(idle) = idle_since(&account.home_path(), s) else { return false };
    let Some(path) = ClaudeCode.records(account, s).into_iter().next() else { return false };
    let Ok(mut page) = crate::transcript::file_page(&path, conversation_items, None, 20) else { return false };
    let Some(last) = page.items.iter().rposition(|it| it.kind == "you") else { return false };
    mark_taken_back(&path, &mut page.items[last..], Some(idle));
    page.items[last].withdrawn
}

/// A Claude Code tool's input as you'd want to read it: the command, the file and the change,
/// else its arguments.
pub(super) fn claude_input(name: &str, input: &Value) -> String {
    let s = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match name {
        "Bash" => s("command"),
        "Read" | "Write" => s("file_path"),
        "Edit" => format!("{}\n- {}\n+ {}", s("file_path"), s("old_string").replace('\n', "\n- "), s("new_string").replace('\n', "\n+ ")),
        "Grep" | "Glob" => [s("pattern"), s("path")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" in "),
        // Plan mode's approval: the plan itself, in Markdown.
        "ExitPlanMode" => s("plan"),
        _ => serde_json::to_string_pretty(input).unwrap_or_default(),
    }
}

/// The questions in an `AskUserQuestion` call's input.
pub(super) fn questions_of(input: &Value) -> Vec<crate::harness::Question> {
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(String::from);
    input
        .get("questions")
        .and_then(Value::as_array)
        .map(|qs| {
            qs.iter()
                .filter_map(|q| {
                    let options = q.get("options").and_then(Value::as_array).map(|o| o.iter().filter_map(|c| Some(codealong_peer::api::Choice { label: text(c, "label")?, description: text(c, "description") })).collect()).unwrap_or_default();
                    Some(crate::harness::Question { question: text(q, "question")?, header: text(q, "header"), options, multi_select: q.get("multiSelect").and_then(Value::as_bool) == Some(true) })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Your answers as Claude Code records them on the result (`toolUseResult.answers`): each
/// question's text to a label or words of your own, or a list where it took several. Older
/// versions joined several into one string with commas, which is kept as it is.
fn answers_of(v: Option<&Value>) -> std::collections::BTreeMap<String, Vec<String>> {
    let Some(map) = v.and_then(Value::as_object) else { return Default::default() };
    map.iter()
        .filter_map(|(q, a)| {
            let list: Vec<String> = match a {
                Value::String(t) => vec![t.clone()],
                Value::Array(l) => l.iter().filter_map(Value::as_str).map(String::from).collect(),
                _ => Vec::new(),
            };
            let list: Vec<String> = list.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
            (!list.is_empty()).then(|| (q.clone(), list))
        })
        .collect()
}

/// The artifact an `Artifact` call published, made from a type or opened, as Claude Code records
/// it on the call's result (`toolUseResult`): `url`, `title` and `artifact_id`, with `seq` (its
/// version) and `updated` on a publish. A fetched page's result has a `url` too, so only one that
/// names an artifact counts; a read or a listing changes nothing you'd go and look at.
fn artifact_of(result: Option<&Value>) -> Option<crate::harness::Link> {
    let r = result?;
    let made = r.get("created_from_type").and_then(Value::as_bool) == Some(true);
    if r.get("artifact_id").is_none() && !made {
        return None;
    }
    let text = |k: &str| r.get(k).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty());
    let url = text("url").filter(|u| u.starts_with("https://"))?;
    let did = if made {
        "Created"
    } else if r.get("opened").and_then(Value::as_bool) == Some(true) {
        "Opened"
    } else if r.get("updated").and_then(Value::as_bool) == Some(true) {
        "Updated"
    } else {
        "Published"
    };
    let note = match r.get("seq").and_then(Value::as_u64) {
        Some(n) => format!("{did} · version {n}"),
        None => did.to_string(),
    };
    Some(crate::harness::Link { url: url.to_string(), title: one_line(text("title").unwrap_or("Artifact"), 120), note: Some(note) })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A transcript line in the conversation's chain, as Claude Code 2.1.285 writes them.
    fn chained(kind: &str, uuid: &str, parent: Option<&str>, at: &str, rest: Value) -> String {
        let mut v = serde_json::json!({ "type": kind, "uuid": uuid, "parentUuid": parent, "timestamp": at, "sessionId": "s1" });
        v.as_object_mut().unwrap().extend(rest.as_object().unwrap().clone());
        v.to_string()
    }

    fn prompt(uuid: &str, parent: Option<&str>, text: &str) -> String {
        chained("user", uuid, parent, "2026-09-30T00:15:50.220Z", serde_json::json!({ "origin": { "kind": "human" }, "message": { "role": "user", "content": text } }))
    }

    fn attachment(uuid: &str, parent: &str) -> String {
        chained("attachment", uuid, Some(parent), "2026-09-30T00:15:50.220Z", serde_json::json!({ "attachment": { "type": "total_tokens_reminder", "text": "<total_tokens>1</total_tokens>" } }))
    }

    fn reply(uuid: &str, parent: &str, text: &str) -> String {
        chained("assistant", uuid, Some(parent), "2026-09-30T00:16:10.000Z", serde_json::json!({ "message": { "role": "assistant", "content": [{ "type": "text", "text": text }] } }))
    }

    fn fate(lines: &[String]) -> Option<Fate> {
        fate_of(lines.iter().map(String::as_str)).map(|(f, _)| f)
    }

    #[test]
    fn a_prompt_the_conversation_went_on_without_was_taken_back() {
        // Stopped before a reply: the recap written minutes later goes on from the line before the
        // prompt, as the one on 2026-09-29 did.
        let recap = chained("system", "r", Some("a1"), "2026-09-30T00:19:10.590Z", serde_json::json!({ "subtype": "away_summary", "content": "You're building…" }));
        let title = r#"{"type":"ai-title","aiTitle":"Security dashboard","sessionId":"s1"}"#.to_string();
        assert_eq!(fate(&[prompt("p", Some("a1"), "Who is the sponsor?"), attachment("p1", "p"), title, recap]), Some(Fate::TakenBack));
        // The next prompt goes on from there too.
        assert_eq!(fate(&[prompt("p", Some("a1"), "Who is the sponsor?"), attachment("p1", "p"), prompt("q", Some("a1"), "Just say OK.")]), Some(Fate::TakenBack));
        // A session's first prompt has nothing before it to go on from: the next prompt follows
        // its lines, with its text put back in front.
        assert_eq!(fate(&[prompt("p", None, "Write an essay."), attachment("p1", "p"), prompt("q", Some("p1"), "Write an essay.Just say OK.")]), Some(Fate::TakenBack));
    }

    #[test]
    fn a_prompt_answered_or_interrupted_was_kept() {
        let p = || [prompt("p", Some("a1"), "Run the tests"), attachment("p1", "p")];
        assert_eq!(fate(&[p().as_slice(), &[reply("x", "p1", "Running them.")]].concat()), Some(Fate::Kept));
        // Escape with text in the box interrupts the turn and keeps the prompt.
        let interrupted = chained("user", "i", Some("p1"), "2026-09-30T00:16:00.000Z", serde_json::json!({ "message": { "role": "user", "content": [{ "type": "text", "text": "[Request interrupted by user]" }] } }));
        assert_eq!(fate(&[p().as_slice(), &[interrupted]].concat()), Some(Fate::Kept));
        // Compaction starts the chain again.
        let boundary = chained("system", "c", None, "2026-09-30T00:16:00.000Z", serde_json::json!({ "subtype": "compact_boundary" }));
        assert_eq!(fate(&[p().as_slice(), &[boundary]].concat()), Some(Fate::Kept));
        // Nothing yet either way.
        assert_eq!(fate(&p()), Some(Fate::Open));
        // Only prompts of yours are asked about.
        assert_eq!(fate(&[attachment("p1", "p")]), None);
    }

    #[test]
    fn marks_a_prompt_taken_back_in_the_conversation() {
        let dir = crate::fixtures::scratch("taken-back");
        let path = dir.join("t.jsonl");
        let write = |lines: &[String]| std::fs::write(&path, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        let answered = [prompt("o", Some("z"), "Earlier question"), reply("a0", "o", "Earlier answer.")];
        let mut lines = [answered.as_slice(), &[prompt("p", Some("a0"), "Who is the sponsor?"), attachment("p1", "p")]].concat();
        write(&lines);
        let page = |idle: Option<i64>| {
            let mut page = crate::transcript::file_page(&path, conversation_items, None, 100).unwrap();
            mark_taken_back(&path, &mut page.items, idle);
            page.items.iter().map(|i| (i.kind, i.withdrawn)).collect::<Vec<_>>()
        };
        // Until something is written after it, the registry says: idle since after the prompt.
        let sent = parse_rfc3339_ms("2026-09-30T00:15:50.220Z").unwrap();
        assert_eq!(page(None), [("you", false), ("agent", false), ("you", false)]);
        assert_eq!(page(Some(sent - 5_000)), [("you", false), ("agent", false), ("you", false)], "idle since before it: it hasn't started yet");
        assert_eq!(page(Some(sent + 900)), [("you", false), ("agent", false), ("you", true)]);
        // A read of what's newer starts at it again while nothing has come back for it.
        let cursor = std::fs::metadata(&path).unwrap().len() - (lines[2].len() + lines[3].len() + 2) as u64;
        let end = std::fs::metadata(&path).unwrap().len();
        assert_eq!(open_prompt_before(&path, end), Some(cursor));
        // Once a line goes on without it, it's taken back whatever the registry says.
        lines.push(prompt("q", Some("a0"), "Just say OK."));
        write(&lines);
        assert_eq!(page(None), [("you", false), ("agent", false), ("you", true), ("you", false)]);
        // Answered, it's there for good.
        lines.truncate(4);
        lines.push(reply("x", "p1", "The sponsor is…"));
        write(&lines);
        assert_eq!(page(Some(sent + 900)), [("you", false), ("agent", false), ("you", false), ("agent", false)]);
        assert_eq!(open_prompt_before(&path, std::fs::metadata(&path).unwrap().len()), None);
    }

    /// Lines as 2.1.286 wrote them, cut down: a publish, an update, a canvas made from a type, an
    /// artifact opened for you, and two results that name a page without making one.
    #[test]
    fn a_published_artifact_is_a_link_on_its_calls_result() {
        let call = serde_json::json!({ "type": "assistant", "message": { "content": [
            { "type": "tool_use", "id": "t1", "name": "Artifact", "input": { "file_path": "/private/tmp/claude-501/-x/scratchpad/security-dashboard.html", "label": "Osprey current 2 Oct" } },
            { "type": "tool_use", "id": "t2", "name": "Artifact", "input": { "action": "read", "url": "https://claude.ai/artifact/8ZBq" } },
            { "type": "tool_use", "id": "t3", "name": "Artifact", "input": { "type_url": "https://claude.ai/code/artifact/2333", "title": "Kestrel Design System" } },
        ] } });
        let labels: Vec<String> = conversation_items(&call, 0).into_iter().map(|i| i.text).collect();
        assert_eq!(labels, ["Artifact · `security-dashboard.html`", "Artifact · read", "Artifact · Kestrel Design System"]);

        let result = |r: Value| {
            let line = serde_json::json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "t1", "content": "Published … at https://claude.ai/artifact/Vuaw (Version 9)" }] }, "toolUseResult": r });
            conversation_items(&line, 0).pop().unwrap().link.map(|l| (l.url, l.title, l.note.unwrap_or_default()))
        };
        let link = |url: &str, title: &str, note: &str| Some((url.to_string(), title.to_string(), note.to_string()));
        let published = serde_json::json!({ "url": "https://claude.ai/artifact/Vuaw", "path": "/private/tmp/x.html", "artifact_id": "226b", "title": "Security dashboard", "updated": false, "icon": "checklist", "seq": 1, "version": "1790892882-e855" });
        assert_eq!(result(published), link("https://claude.ai/artifact/Vuaw", "Security dashboard", "Published · version 1"));
        let updated = serde_json::json!({ "url": "https://claude.ai/artifact/Vuaw", "artifact_id": "226b", "title": "Security dashboard", "updated": true, "seq": 9 });
        assert_eq!(result(updated), link("https://claude.ai/artifact/Vuaw", "Security dashboard", "Updated · version 9"));
        // An older Claude Code kept no version number, and a page can have no title.
        let older = serde_json::json!({ "url": "https://claude.ai/artifact/Vuaw", "artifact_id": "226b", "title": " ", "updated": false, "version": "1790892882-e855" });
        assert_eq!(result(older), link("https://claude.ai/artifact/Vuaw", "Artifact", "Published"));
        let made = serde_json::json!({ "created_from_type": true, "url": "https://claude.ai/artifact/BKx5", "version": "1790033813-d831", "title": "Kestrel Design System", "type": { "url": "https://claude.ai/artifact/5M7U" } });
        assert_eq!(result(made), link("https://claude.ai/artifact/BKx5", "Kestrel Design System", "Created"));
        let opened = serde_json::json!({ "artifact_id": "53a0", "opened": true, "title": "Kestrel Design System", "url": "https://claude.ai/artifact/BKx5" });
        assert_eq!(result(opened), link("https://claude.ai/artifact/BKx5", "Kestrel Design System", "Opened"));
        // A read, and a page WebFetch fetched: nothing was made.
        assert_eq!(result(serde_json::json!({ "read": { "url": "https://claude.ai/artifact/8ZBq", "title": "Class C" }, "artifactRead": { "slug": "3d2e" } })), None);
        assert_eq!(result(serde_json::json!({ "bytes": 10, "code": 200, "url": "https://example.com", "result": "" })), None);
        assert_eq!(result(Value::String("Error: artifact not found".into())), None);
    }
}

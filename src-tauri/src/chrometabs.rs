//! The Chrome tabs a Claude Code session drives through Claude in Chrome, read from its
//! transcript, and bringing them back: its tab in whichever window of whichever profile holds
//! it, or its last page again in that profile once its tabs are gone.
//!
//! Each result of the extension's tools ends with a "Tab Context" block: the tab the call acted
//! on (`- Executed on tabId: N`) and the tabs of the session's tab group, one a line (`• tabId N:
//! "title" ("url")`), none once they're closed. `tabs_context_mcp` also answers JSON with the
//! group's id (`{"availableTabs":[…],"selectedTabId":N,"tabGroupId":G}`). Chrome gives a tab one
//! id for extensions and for AppleScript (both `SessionTabHelper`'s), unique across all its
//! profiles' windows while it runs, so AppleScript finds the tab by it.
//!
//! Which profile the tabs are in comes from the extension's storage in each profile (LevelDB
//! under `Local Extension Settings/<extension id>`): its `tabGroups` record has each group it
//! made, `{"<main tab id>":{"chromeGroupId":G,…}}`, and `bridgeDeviceId` is the id
//! `list_connected_browsers` and `select_browser` name that profile's browser by. The files are
//! read as bytes: the log is plain, and the tables' compression leaves most numbers whole. The
//! extension forgets a group once it's gone, and a few days on its storage no longer has it;
//! then each profile's History (SQLite, which Chrome holds locked, so it's read as immutable)
//! says which profile visited the session's last page, and when.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::chrome;
use crate::model::ChromeTabs;
use crate::util::home;

/// Text a transcript line has when it's worth decoding here: a call to one of Claude in Chrome's
/// tools, or what its results say. Other tools' output can say the same (a search of these very
/// transcripts), so a result counts only when it answers one of its calls.
pub const NEEDLES: [&str; 7] = ["claude-in-chrome__", "Tab Context:", "availableTabs", "deviceId", "tab group no longer exists", "Selected browser", "Connected to browser"];

/// Calls waiting for their results that a session keeps track of: a result follows its call
/// within the turn, so more are ones that never got one.
const CALLS_KEPT: usize = 64;

/// Claude in Chrome's extension, from the Chrome Web Store, and the two other builds its native
/// host lets in.
const EXTENSIONS: [&str; 3] = ["fcoeoabgfenejglbffodgkkbkcdhcgfn", "dihbgbndebgnbjfmelmegjepbnkhlgni", "dngcpimnedloihjnnfngkgjoidhnaolf"];

#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub id: i64,
    pub title: String,
    pub url: String,
}

/// What a session's lines say about its Chrome tabs, as of the last of them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Use {
    /// Its group's tabs, the one it last acted on first; empty once they're closed.
    pub tabs: Vec<Tab>,
    /// The page it last acted on, kept after its tabs close.
    pub page: Option<Tab>,
    pub group: Option<i64>,
    /// The browser it picked, by the extension's device id.
    pub device: Option<String>,
    /// The browsers the last `list_connected_browsers` named, as (device id, name):
    /// `switch_browser` answers with a name.
    browsers: Vec<(String, String)>,
    /// Claude in Chrome's calls whose results haven't come, by tool-use id.
    calls: Vec<String>,
    /// When it last used Chrome.
    pub at: i64,
    /// The profile's folder, once the extension's storage said (`Storage::profile_of`).
    pub profile: Option<String>,
}

impl Use {
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty() && self.page.is_none()
    }

    /// What the board shows, or none when there's nothing to bring back: no tab open, and no
    /// web page to open again. `profiles` names the profile, when Chrome has more than one.
    pub fn for_board(&self, profiles: &[chrome::Profile]) -> Option<ChromeTabs> {
        let page = self.page.as_ref().or(self.tabs.first());
        let url = page.map(|p| p.url.clone()).unwrap_or_default();
        if self.tabs.is_empty() && !is_web(&url) {
            return None;
        }
        let profile = self.profile.as_ref().and_then(|dir| profiles.iter().find(|p| &p.dir == dir));
        Some(ChromeTabs {
            tabs: self.tabs.iter().map(|t| t.id).collect(),
            url,
            title: page.map(|p| p.title.clone()).unwrap_or_default(),
            profile: profile.filter(|_| profiles.len() > 1).map(|p| p.name.clone()),
            profile_dir: self.profile.clone(),
            at: self.at,
        })
    }
}

fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Folds lines a session just wrote into `u`; says whether they changed anything. Most
/// sessions never call Claude in Chrome, and their lines are passed over whole.
pub fn fold_text(u: &mut Use, text: &str) -> bool {
    if u.calls.is_empty() && !text.contains(NEEDLES[0]) {
        return false;
    }
    let mut changed = false;
    for line in text.lines() {
        changed |= fold(u, line);
    }
    changed
}

/// Folds one transcript line into `u`; says whether it changed anything. A line is decoded when
/// it calls one of Claude in Chrome's tools, or may be the result of a call waiting for one.
pub fn fold(u: &mut Use, line: &str) -> bool {
    let call = line.contains(NEEDLES[0]);
    if !call && (u.calls.is_empty() || !NEEDLES[1..].iter().any(|n| line.contains(n))) {
        return false;
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
    let Some(parts) = v.pointer("/message/content").and_then(Value::as_array) else { return false };
    let before = u.clone();
    let at = v.get("timestamp").and_then(Value::as_str).and_then(crate::util::parse_rfc3339_ms).unwrap_or(0);
    for p in parts {
        match p.get("type").and_then(Value::as_str) {
            Some("tool_use") => {
                let Some(name) = p.get("name").and_then(Value::as_str).filter(|n| n.contains("claude-in-chrome__")) else { continue };
                if let Some(id) = p.get("id").and_then(Value::as_str) {
                    if u.calls.len() >= CALLS_KEPT {
                        u.calls.remove(0);
                    }
                    u.calls.push(id.to_string());
                }
                if name.ends_with("__select_browser") {
                    if let Some(d) = p.pointer("/input/deviceId").and_then(Value::as_str) {
                        u.device = Some(d.to_string());
                    }
                }
            }
            Some("tool_result") => {
                let Some(i) = p.get("tool_use_id").and_then(Value::as_str).and_then(|id| u.calls.iter().position(|c| c == id)) else { continue };
                u.calls.remove(i);
                let text = match p.get("content") {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Array(a)) => a.iter().filter_map(|x| x.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
                    _ => continue,
                };
                if result(u, &text) {
                    u.at = u.at.max(at);
                }
            }
            _ => {}
        }
    }
    *u != before
}

/// Takes in one tool result's text; says whether it was one of Claude in Chrome's.
fn result(u: &mut Use, text: &str) -> bool {
    let mut seen = false;
    if let Some(i) = text.find("{\"availableTabs\"") {
        if let Some(Ok(v)) = serde_json::Deserializer::from_str(&text[i..]).into_iter::<Value>().next() {
            if let Some(g) = v.get("tabGroupId").and_then(Value::as_i64) {
                u.group = Some(g);
            }
            let active = v.get("selectedTabId").and_then(Value::as_i64);
            let tabs: Vec<Tab> = v
                .get("availableTabs")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|t| Some(Tab { id: t.get("tabId")?.as_i64()?, title: str_of(t, "title"), url: str_of(t, "url") })).collect())
                .unwrap_or_default();
            set_tabs(u, tabs, active);
            seen = true;
        }
    }
    if let Some(i) = text.rfind("Tab Context:") {
        let (tabs, active) = tab_context(&text[i..]);
        set_tabs(u, tabs, active);
        seen = true;
    }
    if text.contains("tab group no longer exists") {
        u.tabs.clear();
        seen = true;
    }
    if text.trim_start().starts_with('[') && text.contains("\"deviceId\"") {
        if let Ok(list) = serde_json::from_str::<Vec<Value>>(text.trim()) {
            u.browsers = list.iter().filter_map(|b| Some((str_of(b, "deviceId"), b.get("name")?.as_str()?.to_string()))).collect();
            if let Some(b) = list.iter().find(|b| b.get("inUse").and_then(Value::as_bool) == Some(true)) {
                u.device = Some(str_of(b, "deviceId"));
            }
            seen = true;
        }
    }
    // `switch_browser` and `select_browser` answer with the browser's name.
    for lead in ["Selected browser \"", "Connected to browser \""] {
        if let Some(name) = text.strip_prefix(lead).and_then(|r| r.split('"').next()) {
            if let Some((d, _)) = u.browsers.iter().find(|(_, n)| n == name) {
                u.device = Some(d.clone());
            }
            seen = true;
        }
    }
    seen
}

fn str_of(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// The tabs listed after "Tab Context:" and the one the call acted on.
fn tab_context(block: &str) -> (Vec<Tab>, Option<i64>) {
    let mut tabs = Vec::new();
    let mut active = None;
    for line in block.lines().skip(1) {
        let l = line.trim();
        if l.is_empty() || l == "- Available tabs:" {
            continue;
        }
        if let Some(n) = l.strip_prefix("- Executed on tabId:") {
            active = n.trim().parse().ok();
        } else if let Some(rest) = l.strip_prefix("• tabId ") {
            // `N: "title" ("url")`. A URL has no quote in it; a title may.
            let Some((id, rest)) = rest.split_once(": ") else { break };
            let Some(body) = rest.strip_prefix('"').and_then(|r| r.strip_suffix("\")")) else { break };
            let Some(cut) = body.rfind("\" (\"") else { break };
            let Ok(id) = id.parse() else { break };
            tabs.push(Tab { id, title: body[..cut].to_string(), url: body[cut + 4..].to_string() });
        } else {
            break;
        }
    }
    (tabs, active)
}

/// The group's tabs now, the one it acted on first, and that tab as its last page.
fn set_tabs(u: &mut Use, mut tabs: Vec<Tab>, active: Option<i64>) {
    if let Some(i) = active.and_then(|a| tabs.iter().position(|t| t.id == a)) {
        let t = tabs.remove(i);
        tabs.insert(0, t);
    }
    if let Some(first) = tabs.first() {
        u.page = Some(first.clone());
    }
    u.tabs = tabs;
}

/// What names the Chrome profile a session's tabs are in. The extension's storage in each
/// profile: every number of six digits or more in it (group and tab ids) and each
/// `bridgeDeviceId`, by the profile folder whose files have it, the newest file winning. Then
/// each profile's History, opened when first needed.
#[derive(Default)]
pub struct Clues {
    numbers: HashMap<i64, usize>,
    devices: HashMap<String, usize>,
    root: std::path::PathBuf,
    dirs: Vec<String>,
    history: Option<Vec<Option<Connection>>>,
}

/// How near a visit to a session's last page has to be to the time it used it, for History
/// to say which profile when more than one visited that page.
const VISIT_NEAR_MS: i64 = 30 * 60_000;

impl Clues {
    pub fn read() -> Clues {
        Self::read_in(&home().join("Library/Application Support/Google/Chrome"), &chrome::profiles().into_iter().map(|p| p.dir).collect::<Vec<_>>())
    }

    fn read_in(root: &Path, dirs: &[String]) -> Clues {
        let mut s = Clues { root: root.to_path_buf(), dirs: dirs.to_vec(), ..Default::default() };
        let mut files: Vec<(std::time::SystemTime, usize, std::path::PathBuf)> = Vec::new();
        for (i, dir) in dirs.iter().enumerate() {
            for ext in EXTENSIONS {
                let Ok(entries) = std::fs::read_dir(root.join(dir).join("Local Extension Settings").join(ext)) else { continue };
                for e in entries.flatten() {
                    let p = e.path();
                    if matches!(p.extension().and_then(|x| x.to_str()), Some("log" | "ldb")) {
                        let at = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
                        files.push((at, i, p));
                    }
                }
            }
        }
        files.sort();
        for (_, i, p) in files {
            if let Ok(bytes) = std::fs::read(&p) {
                s.take(&bytes, i);
            }
        }
        s
    }

    fn take(&mut self, b: &[u8], dir: usize) {
        let mut i = 0;
        while i < b.len() {
            if b[i].is_ascii_digit() {
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if (6..=18).contains(&(i - start)) {
                    if let Some(n) = std::str::from_utf8(&b[start..i]).ok().and_then(|t| t.parse().ok()) {
                        self.numbers.insert(n, dir);
                    }
                }
            } else {
                i += 1;
            }
        }
        for at in memchr::memmem::find_iter(b, b"bridgeDeviceId") {
            let after = &b[at + 14..b.len().min(at + 14 + 48)];
            if let Some(id) = uuid_in(after) {
                self.devices.insert(id, dir);
            }
        }
    }

    /// The profile folder a session's tabs are in: by its group, its tabs or the browser it
    /// picked in the extension's storage, else by who visited its last page in History.
    pub fn profile_of(&mut self, u: &Use) -> Option<String> {
        let number = |n: i64| self.numbers.get(&n.abs()).filter(|_| n.abs() >= 100_000);
        let stored = u
            .group
            .and_then(number)
            .or_else(|| u.tabs.iter().find_map(|t| number(t.id)))
            .or_else(|| u.device.as_ref().and_then(|d| self.devices.get(d)))
            .or_else(|| u.page.as_ref().and_then(|p| number(p.id)))
            .copied();
        let i = match stored {
            Some(i) => i,
            None => self.by_history(u.page.as_ref()?.url.as_str(), u.at)?,
        };
        Some(self.dirs[i].clone())
    }

    /// The profile whose History has `url`: the only one that visited it, or the one whose visit
    /// came clearly nearest `at` (ms).
    fn by_history(&mut self, url: &str, at: i64) -> Option<usize> {
        if !is_web(url) {
            return None;
        }
        let (root, dirs) = (&self.root, &self.dirs);
        let dbs = self.history.get_or_insert_with(|| dirs.iter().map(|d| open_history(&root.join(d).join("History"))).collect());
        // Chrome's times are microseconds since 1601.
        let at_us = (at + 11_644_473_600_000) * 1000;
        let mut near: Vec<(i64, usize)> = dbs
            .iter()
            .enumerate()
            .filter_map(|(i, db)| {
                let q = "SELECT MIN(ABS(v.visit_time - ?1)) FROM visits v JOIN urls u ON v.url = u.id WHERE u.url = ?2";
                let us: Option<i64> = db.as_ref()?.query_row(q, rusqlite::params![at_us, url], |r| r.get(0)).ok()?;
                us.map(|us| (us / 1000, i))
            })
            .collect();
        near.sort();
        match near.as_slice() {
            [(_, i)] => Some(*i),
            [(a, i), (b, _), ..] if *a <= VISIT_NEAR_MS && *b >= 3 * *a => Some(*i),
            _ => None,
        }
    }
}

/// A profile's History, read in place: Chrome holds it locked while it runs, and `immutable`
/// reads without taking locks.
fn open_history(path: &Path) -> Option<Connection> {
    if !path.is_file() {
        return None;
    }
    let mut uri = String::from("file:");
    for b in path.to_string_lossy().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => uri.push(b as char),
            _ => uri.push_str(&format!("%{b:02X}")),
        }
    }
    uri.push_str("?immutable=1");
    Connection::open_with_flags(uri, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()
}

/// The first UUID in `b`, lowercase as the extension writes them.
fn uuid_in(b: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(b);
    let hex = |c: char| c.is_ascii_digit() || ('a'..='f').contains(&c);
    let chars: Vec<char> = text.chars().collect();
    (0..chars.len().saturating_sub(35)).find_map(|s| {
        let w = &chars[s..s + 36];
        let ok = w.iter().enumerate().all(|(i, &c)| if [8, 13, 18, 23].contains(&i) { c == '-' } else { hex(c) });
        ok.then(|| w.iter().collect())
    })
}

/// Chrome's script to bring a tab forward: the first of the ids given that's open in any window,
/// selected there and its window raised. Says the id it found, or nothing. Windows are held by
/// id (`every window` answers `window id N`), and every window's tab ids are asked for before
/// anything moves.
const SHOW_TAB: &str = r#"on run argv
	if application id "com.google.Chrome" is not running then return ""
	tell application id "com.google.Chrome"
		set wins to every window
		set tabIds to {}
		repeat with w in wins
			set end of tabIds to (id of every tab of w)
		end repeat
		repeat with want in argv
			repeat with k from 1 to count of wins
				set ids to item k of tabIds
				repeat with i from 1 to count of ids
					if ((item i of ids) as text) is (want as text) then
						set w to item k of wins
						try
							if minimized of w then set minimized of w to false
						end try
						set active tab index of w to i
						set index of w to 1
						activate
						return want as text
					end if
				end repeat
			end repeat
		end repeat
	end tell
	return ""
end run"#;

/// Brings a session's Chrome tab forward, in its window and tab group. When none of its tabs is
/// open any more (closed, or Chrome restarted and gave them new ids), its last page opens again
/// in its profile.
pub fn show(c: &ChromeTabs) -> Result<()> {
    if !c.tabs.is_empty() {
        let ids: Vec<String> = c.tabs.iter().map(|t| t.to_string()).collect();
        let args: Vec<&str> = ids.iter().map(String::as_str).collect();
        if !crate::launch::osascript_in("Chrome", SHOW_TAB, &args)?.trim().is_empty() {
            return Ok(());
        }
    }
    if !is_web(&c.url) {
        return Err(anyhow!("Its Chrome tabs are closed, and it left no web page to open again."));
    }
    let profile = c.profile_dir.as_ref().and_then(|dir| chrome::profiles().into_iter().find(|p| &p.dir == dir));
    match profile {
        Some(p) => chrome::open(&c.url, &p),
        None => crate::procs::run("/usr/bin/open", &["-b", chrome::BUNDLE_ID, &c.url], None, Duration::from_secs(10)).map(|_| ()).map_err(|e| anyhow!("Couldn't open {} in Chrome: {e}", c.url)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(v: serde_json::Value) -> String {
        v.to_string()
    }

    fn call_line(id: &str, tool: &str) -> String {
        line(serde_json::json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":id,"name":format!("mcp__claude-in-chrome__{tool}"),"input":{}}]}}))
    }

    fn result_line(at: &str, id: &str, text: &str) -> String {
        line(serde_json::json!({"type":"user","timestamp":at,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":id,"content":[{"type":"text","text":text}]}]}}))
    }

    /// A call to `tool` and its result.
    fn call(u: &mut Use, tool: &str, at: &str, text: &str) -> bool {
        let id = format!("toolu_{}", u.calls.len() + 1);
        fold(u, &call_line(&id, tool));
        fold(u, &result_line(at, &id, text))
    }

    #[test]
    fn follows_a_sessions_tabs() {
        let mut u = Use::default();
        let ctx = r#"{"availableTabs":[{"tabId":11,"title":"New Tab","url":"chrome://newtab/"}],"tabGroupId":1858426758}"#;
        assert!(call(&mut u, "tabs_context_mcp", "2026-09-30T17:00:53.116Z", ctx));
        assert_eq!(u.group, Some(1858426758));
        assert_eq!(u.tabs.iter().map(|t| t.id).collect::<Vec<_>>(), [11]);
        // Nothing to bring back yet: a new tab isn't a page to open again, but it's open.
        assert!(u.for_board(&[]).is_some());

        let nav = "Navigated to https://admin.google.com/ac/users\n\n\nTab Context:\n- Executed on tabId: 12\n- Available tabs:\n  • tabId 11: \"Say \"hi\" (twice)\" (\"https://example.com/a?b=(c)\")\n  • tabId 12: \"Users\" (\"https://admin.google.com/ac/users\")\n<system-reminder>batch</system-reminder>";
        assert!(call(&mut u, "navigate", "2026-09-30T17:01:00.000Z", nav));
        assert_eq!(u.tabs, vec![Tab { id: 12, title: "Users".into(), url: "https://admin.google.com/ac/users".into() }, Tab { id: 11, title: "Say \"hi\" (twice)".into(), url: "https://example.com/a?b=(c)".into() }]);
        assert_eq!(u.at, crate::util::parse_rfc3339_ms("2026-09-30T17:01:00.000Z").unwrap());

        // Closing the last tab empties the group; its page stays, to open again.
        let closed = "Closed tab 12. Group is now empty (auto-removed).\n\n\nTab Context:\n- Available tabs:\n";
        assert!(call(&mut u, "tabs_close_mcp", "2026-09-30T17:02:00.000Z", closed));
        assert!(u.tabs.is_empty());
        let board = u.for_board(&[]).unwrap();
        assert_eq!((board.tabs.len(), board.url.as_str(), board.title.as_str()), (0, "https://admin.google.com/ac/users", "Users"));

        // Other tools' output that looks like Chrome's leaves it alone: a search of transcripts.
        let bash = line(serde_json::json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"b1","name":"Bash","input":{"command":"grep -r mcp__claude-in-chrome__navigate ~/.claude"}}]}}));
        fold(&mut u, &bash);
        assert!(!fold(&mut u, &result_line("2026-09-30T17:03:00.000Z", "b1", nav)));
        assert!(!fold_text(&mut u, &[bash.as_str(), &result_line("2026-09-30T17:03:00.000Z", "b1", nav)].join("\n")));
        assert!(u.tabs.is_empty());
        assert!(!fold(&mut u, r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Tab Context: none"}]}}"#));
    }

    #[test]
    fn a_closed_group_with_no_page_shows_nothing() {
        let mut u = Use::default();
        call(&mut u, "tabs_context_mcp", "2026-09-30T17:00:00.000Z", r#"{"availableTabs":[{"tabId":11,"title":"New Tab","url":"chrome://newtab/"}],"tabGroupId":5}"#);
        call(&mut u, "tabs_create_mcp", "2026-09-30T17:01:00.000Z", "This session's tab group no longer exists (tabs were closed). Call tabs_context_mcp with createIfEmpty: true to create a new one.");
        assert!(u.tabs.is_empty());
        assert!(u.for_board(&[]).is_none());
    }

    #[test]
    fn follows_the_browser_it_picks() {
        let mut u = Use::default();
        let list = r#"[{"deviceId":"aaaaaaaa-0000-4000-8000-000000000001","name":"Browser 1","inUse":true},{"deviceId":"bbbbbbbb-0000-4000-8000-000000000002","name":"Main work","inUse":false}]"#;
        call(&mut u, "list_connected_browsers", "2026-09-30T17:00:00.000Z", list);
        assert_eq!(u.device.as_deref(), Some("aaaaaaaa-0000-4000-8000-000000000001"));
        call(&mut u, "switch_browser", "2026-09-30T17:01:00.000Z", "Selected browser \"Main work\".");
        assert_eq!(u.device.as_deref(), Some("bbbbbbbb-0000-4000-8000-000000000002"));
        let pick = line(serde_json::json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"t","name":"mcp__claude-in-chrome__select_browser","input":{"deviceId":"cccccccc-0000-4000-8000-000000000003"}}]}}));
        assert!(fold(&mut u, &pick));
        assert_eq!(u.device.as_deref(), Some("cccccccc-0000-4000-8000-000000000003"));
    }

    #[test]
    fn finds_the_profile_the_tabs_are_in() {
        let root = crate::fixtures::scratch("chrometabs");
        let store = |dir: &str, file: &str, bytes: &[u8]| {
            let d = root.join(dir).join("Local Extension Settings").join(EXTENSIONS[0]);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(file), bytes).unwrap();
        };
        store("Profile 3", "000061.ldb", b"\x00&bridgeDeviceId\x8c\x01\x40\x01\"ab08c4a2-ed95-4e5c-a5c5-3c249283f197\"\x05");
        store("Profile 12", "000017.log", br#"tabGroups..{"199479133":{"chromeGroupId":1858426758,"createdAt":1.790787652991e+12,"mainTabId":199479133}}"#);
        let mut s = Clues::read_in(&root, &["Profile 3".to_string(), "Profile 12".to_string(), "Default".to_string()]);
        let mut u = Use { group: Some(1858426758), ..Default::default() };
        assert_eq!(s.profile_of(&u).as_deref(), Some("Profile 12"));
        u.group = Some(-1858426758);
        assert_eq!(s.profile_of(&u).as_deref(), Some("Profile 12"));
        u.group = Some(42);
        assert_eq!(s.profile_of(&u), None);
        u.tabs = vec![Tab { id: 199479133, title: String::new(), url: String::new() }];
        assert_eq!(s.profile_of(&u).as_deref(), Some("Profile 12"));
        let picked = Use { device: Some("ab08c4a2-ed95-4e5c-a5c5-3c249283f197".into()), ..Default::default() };
        assert_eq!(s.profile_of(&picked).as_deref(), Some("Profile 3"));

        // A group the extension has forgotten: History says who visited its last page, and when.
        let history = |dir: &str, visits: &[(&str, i64)]| {
            let db = Connection::open(root.join(dir).join("History")).unwrap();
            db.execute_batch("CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT); CREATE TABLE visits (id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER);").unwrap();
            for (i, (url, at)) in visits.iter().enumerate() {
                db.execute("INSERT INTO urls (id, url) VALUES (?1, ?2)", rusqlite::params![i as i64 + 1, url]).unwrap();
                db.execute("INSERT INTO visits (url, visit_time) VALUES (?1, ?2)", rusqlite::params![i as i64 + 1, (at + 11_644_473_600_000) * 1000]).unwrap();
            }
        };
        let at = 1_790_000_000_000;
        history("Profile 3", &[("https://app.example.com/a", at - 60_000), ("https://both.example.com/", at - 120_000)]);
        history("Profile 12", &[("https://both.example.com/", at - 5 * 3_600_000), ("https://near.example.com/", at - 20 * 60_000)]);
        std::fs::create_dir_all(root.join("Default")).unwrap();
        history("Default", &[("https://near.example.com/", at - 40 * 60_000)]);
        let mut s = Clues::read_in(&root, &["Profile 3".to_string(), "Profile 12".to_string(), "Default".to_string()]);
        let gone = |url: &str| Use { group: Some(42), page: Some(Tab { id: 7, title: String::new(), url: url.into() }), at, ..Default::default() };
        assert_eq!(s.profile_of(&gone("https://app.example.com/a")).as_deref(), Some("Profile 3"), "the only one that visited it");
        assert_eq!(s.profile_of(&gone("https://both.example.com/")).as_deref(), Some("Profile 3"), "its visit came clearly nearest");
        assert_eq!(s.profile_of(&gone("https://near.example.com/")), None, "two visits too near each other to say");
        assert_eq!(s.profile_of(&gone("https://nobody.example.com/")), None);

        let profiles = vec![
            chrome::Profile { id: "Dev".into(), name: "Dev".into(), dir: "Profile 3".into(), email: None },
            chrome::Profile { id: "spruce.example".into(), name: "spruce.example".into(), dir: "Profile 12".into(), email: None },
        ];
        let u = Use { page: Some(Tab { id: 1, title: "Users".into(), url: "https://example.com/".into() }), profile: Some("Profile 12".into()), ..Default::default() };
        assert_eq!(u.for_board(&profiles).unwrap().profile.as_deref(), Some("spruce.example"));
        // With one profile there's no need to say which.
        assert_eq!(u.for_board(&profiles[1..]).unwrap().profile, None);
    }
}

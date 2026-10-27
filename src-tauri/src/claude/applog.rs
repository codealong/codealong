//! The Claude app's own log, `~/Library/Logs/Claude/main.log`, for the approvals the app asks
//! for itself. Its built-in browser (`mcp__Claude_Browser__*`) asks before it opens a site and
//! before each read of one on a private network, and computer use before it controls an app.
//! Claude Code is still making the call all the while, so its registry says `busy` and its
//! transcript has nothing new, where a prompt of its own says `waiting` (registry.rs
//! `PERMISSION_PROMPT`): without this, such a session read as stuck. The app logs each request
//! as it shows it, and again as it's answered or withdrawn (3.6556, 2026-10-27):
//!
//! ```text
//! 2026-10-27 07:20:04 [info] [Preview] cat3 per-action read approval requested {
//!   origin: 'https://app.northwind.dev',
//!   toolName: 'read_console_messages',
//!   offerAlwaysGrant: false
//! }
//! 2026-10-27 07:20:04 [info] Emitted tool permission request <id> for browser:read_console_messages in session local_<id>
//! 2026-10-27 07:21:10 [info] Received permission response for <id>: once (tool: browser:read_console_messages)
//! 2026-10-05 20:29:39 [info] Permission request <id> for Bash aborted
//! ```
//!
//! Claude Code's own prompts in the app are logged the same way (Bash, AskUserQuestion). Times
//! are the Mac's clock, to the second. At about 10 MB the log moves to `main1.log` and a new one
//! starts; a request left open in the old one when Codealong starts isn't seen.
//!
//! A request whose end the log doesn't say (the app quit with it up, say) goes with the turn it
//! was asked in. The registry says when a process started, and Codealong sees its session at
//! rest as the turn ends; the time it last said busy won't do, since a prompt of Claude Code's own
//! answered meanwhile moves it (`waiting`, then `busy` again) while the app's is still up.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::registry::RegistryEntry;
use crate::util::{code, home, local_ms};

/// Where the app logs.
pub fn path() -> PathBuf {
    home().join("Library/Logs/Claude/main.log")
}

/// An approval the app has up for one of its sessions.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    /// The app's id for the request.
    pub id: String,
    /// The app's id for the session (`local_…`), which its registry entry names (`host_session`).
    pub session: String,
    /// The tool as the app names it: `browser:read_console_messages`, `computer:request_access`.
    pub tool: String,
    /// The site a browser's request is for, when the lines ahead of it name one.
    pub site: Option<String>,
    /// When it was asked, to the second.
    pub at: i64,
}

impl Ask {
    /// What it asks, as a pending call's line says it (transcript.rs `tool_request`):
    /// "browser · read_console_messages · `app.northwind.dev`".
    pub fn label(&self) -> String {
        let tool = self.tool.replace(':', " · ");
        match &self.site {
            Some(site) => format!("{tool} · {}", code(host(site))),
            None => tool,
        }
    }
}

/// A site's host, as the app's prompt names it: "Allow Claude to access app.northwind.dev?".
fn host(origin: &str) -> &str {
    origin.split_once("://").map_or(origin, |(_, rest)| rest).trim_end_matches('/')
}

/// The log as far as it's been read, and the requests it has up.
#[derive(Default)]
struct Log {
    /// The log as opened, and how far into it it's been read. It's held open, so one the app
    /// moves aside is read to its end before the new one is read from its start.
    file: Option<(File, u64)>,
    /// Requests asked and not yet answered or withdrawn, by id.
    open: HashMap<String, Ask>,
    /// The site the lines just read named, for a browser's request that follows them.
    site: Option<String>,
    /// By the app's id for a session, when it last came to rest, as Codealong saw it (`saw`).
    rested: HashMap<String, i64>,
}

impl Log {
    /// Reads what the log at `path` has added since the last read. True when the requests up
    /// changed.
    fn refresh(&mut self, path: &Path) -> bool {
        use std::os::unix::fs::MetadataExt;
        // Another file there is the log started again: the requests up stay up. Looked for
        // before the one held is read on, so all the app wrote there before moving it aside is
        // read; one moved aside after the look is read to its end at the next.
        let there = std::fs::metadata(path).ok().map(|m| m.ino());
        let held = self.file.as_ref().and_then(|(f, _)| f.metadata().ok()).map(|m| m.ino());
        let mut changed = self.read_on();
        if there.is_some() && there != held {
            let Ok(f) = File::open(path) else { return changed };
            self.file = Some((f, 0));
            changed |= self.read_on();
        }
        changed
    }

    /// Reads on in the file held open. True when the requests up changed.
    fn read_on(&mut self) -> bool {
        let Some((file, at)) = &mut self.file else { return false };
        let Some(text) = whole_lines(file, at) else { return false };
        text.lines().fold(false, |changed, line| self.take(line) | changed)
    }

    /// Takes in one line of the log. True when it asked or ended a request.
    fn take(&mut self, line: &str) -> bool {
        let Some((at, said)) = stamped(line) else {
            // A line of the block the line before opened.
            if let Some(origin) = line.trim().strip_prefix("origin: '") {
                self.site = origin.split('\'').next().map(String::from);
            }
            return false;
        };
        let site = self.site.take();
        if let Some(rest) = said.strip_prefix("Emitted tool permission request ") {
            let words: Vec<&str> = rest.split(' ').collect();
            let [id, "for", tool, "in", "session", session] = words[..] else { return false };
            let site = site.filter(|_| tool.starts_with("browser:"));
            self.open.insert(id.to_string(), Ask { id: id.to_string(), session: session.to_string(), tool: tool.to_string(), site, at });
            return true;
        }
        let answered = said.strip_prefix("Received permission response for ").and_then(|r| r.split(':').next());
        let withdrawn = || said.strip_prefix("Permission request ").filter(|r| r.ends_with(" aborted")).and_then(|r| r.split(' ').next());
        answered.or_else(withdrawn).is_some_and(|id| self.open.remove(id).is_some())
    }

    /// Takes in a registry entry as read: one of the app's sessions at rest (idle, or its turn
    /// over with a command left in the background) has ended the turn of every request up for it.
    fn saw(&mut self, reg: &RegistryEntry) {
        let Some(session) = reg.host_session.as_deref().filter(|_| matches!(reg.status.as_str(), "idle" | "shell")) else { return };
        let since = reg.status_updated_at.unwrap_or(0);
        let rested = self.rested.entry(session.to_string()).or_insert(since);
        *rested = (*rested).max(since);
    }

    /// The latest request up for the session `reg` is the registry entry of: one of the app's
    /// sessions that says it's busy, the request asked since its process started and since it
    /// was last seen at rest.
    fn up_for(&self, reg: &RegistryEntry) -> Option<&Ask> {
        let session = reg.host_session.as_deref().filter(|_| reg.status == "busy")?;
        let since = reg.started_at.unwrap_or(0).max(self.rested.get(session).copied().unwrap_or(0));
        // The log's times are to the second, the registry's to the millisecond.
        self.open.values().filter(|a| a.session == session && a.at + 1000 > since).max_by_key(|a| a.at)
    }
}

/// What `file` has from `at` on, whole lines only, with `at` moved past them. A file shorter
/// than `at` was cut short where it is, and is read from its start.
fn whole_lines(file: &mut File, at: &mut u64) -> Option<String> {
    let len = file.metadata().ok()?.len();
    if len < *at {
        *at = 0;
    }
    if len == *at {
        return None;
    }
    file.seek(SeekFrom::Start(*at)).ok()?;
    let mut bytes = Vec::new();
    file.by_ref().take(len - *at).read_to_end(&mut bytes).ok()?;
    bytes.truncate(bytes.iter().rposition(|&b| b == b'\n')? + 1);
    *at += bytes.len() as u64;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// A line's time and what it says after its level: `2026-10-27 07:20:04 [info] …`.
fn stamped(line: &str) -> Option<(i64, &str)> {
    let b = line.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b' ' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let n = |from: usize, to: usize| line.get(from..to)?.parse::<i64>().ok();
    let at = local_ms(n(0, 4)?, n(5, 7)?, n(8, 10)?, n(11, 13)?, n(14, 16)?, n(17, 19)?, 0)?;
    let (_, said) = line.get(19..)?.trim_start().split_once("] ")?;
    Some((at, said))
}

static LOG: Mutex<Option<Log>> = Mutex::new(None);

fn log() -> std::sync::MutexGuard<'static, Option<Log>> {
    LOG.lock().unwrap_or_else(|e| e.into_inner())
}

/// Reads what the app's log has added. True when the requests it has up changed.
pub fn refresh() -> bool {
    log().get_or_insert_with(Log::default).refresh(&path())
}

/// Takes in an account's registry as read, for the app's sessions that have come to rest.
pub fn saw(entries: &[RegistryEntry]) {
    let mut log = log();
    let log = log.get_or_insert_with(Log::default);
    for r in entries.iter().filter(|r| r.host_session.is_some()) {
        log.saw(r);
    }
}

/// The approval the Claude app has up for a live session of its own, by its registry entry.
pub fn up_for(reg: &RegistryEntry) -> Option<Ask> {
    if reg.host_session.is_none() || reg.status != "busy" {
        return None;
    }
    log().as_ref()?.up_for(reg).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const ASKED: &str = "2026-10-27 07:20:04 [info] [Preview] cat3 per-action read approval requested {\n  origin: 'https://app.northwind.dev',\n  toolName: 'read_console_messages',\n  offerAlwaysGrant: false\n}\n2026-10-27 07:20:04 [info] Emitted tool permission request a1b2-01 for browser:read_console_messages in session local_c3d4-77\n";

    /// One of the app's sessions, its process started at `started`, saying `status` since a
    /// minute later.
    fn reg(status: &str, started: i64) -> RegistryEntry {
        RegistryEntry { status: status.into(), started_at: Some(started), status_updated_at: Some(started + 60_000), host_session: Some("local_c3d4-77".into()), ..Default::default() }
    }

    fn at(h: i64, m: i64, s: i64) -> i64 {
        local_ms(2026, 10, 27, h, m, s, 0).unwrap()
    }

    fn append(path: &Path) -> File {
        std::fs::OpenOptions::new().append(true).open(path).unwrap()
    }

    #[test]
    fn keeps_a_request_up_until_its_answer_or_withdrawal() {
        let dir = crate::fixtures::scratch("claude-applog");
        let path = dir.join("main.log");
        std::fs::write(&path, format!("2026-10-27 07:19:58 [info] [git-worker] fetchWorkingTreeDiff timing: total=205ms\n{ASKED}")).unwrap();
        let mut log = Log::default();
        assert!(log.refresh(&path));
        let ask = log.up_for(&reg("busy", at(7, 0, 0))).cloned().unwrap();
        assert_eq!(ask, Ask { id: "a1b2-01".into(), session: "local_c3d4-77".into(), tool: "browser:read_console_messages".into(), site: Some("https://app.northwind.dev".into()), at: at(7, 20, 4) });
        assert_eq!(ask.label(), "browser · read_console_messages · `app.northwind.dev`");
        // Nothing new: nothing read again.
        assert!(!log.refresh(&path));
        // Only while it says busy: Claude Code's own prompts say waiting.
        assert!(log.up_for(&reg("waiting", at(7, 0, 0))).is_none());
        assert!(log.up_for(&RegistryEntry { host_session: Some("local_other".into()), ..reg("busy", 0) }).is_none());

        // The rest of a line comes with the next write.
        let mut f = append(&path);
        write!(f, "2026-10-27 07:21:10 [info] Received permission response for a1b2-01: once (tool: browser:read").unwrap();
        assert!(!log.refresh(&path));
        assert!(log.up_for(&reg("busy", 0)).is_some());
        writeln!(f, "_console_messages)").unwrap();
        assert!(log.refresh(&path));
        assert!(log.up_for(&reg("busy", 0)).is_none());

        // Withdrawn, as the app does when the turn is stopped.
        writeln!(f, "2026-10-27 07:22:00 [info] Emitted tool permission request e5f6-02 for computer:request_access in session local_c3d4-77").unwrap();
        assert!(log.refresh(&path));
        let ask = log.up_for(&reg("busy", 0)).cloned().unwrap();
        assert_eq!((ask.label(), ask.site), ("computer · request_access".to_string(), None), "a site named well before isn't this one's");
        writeln!(f, "2026-10-27 07:22:14 [info] Permission request e5f6-02 for computer:request_access aborted").unwrap();
        assert!(log.refresh(&path));
        assert!(log.up_for(&reg("busy", 0)).is_none());
    }

    #[test]
    fn a_request_whose_end_went_unsaid_goes_with_its_turn() {
        let dir = crate::fixtures::scratch("claude-applog-turns");
        let path = dir.join("main.log");
        std::fs::write(&path, ASKED).unwrap();
        let mut log = Log::default();
        log.refresh(&path);
        // A prompt of Claude Code's own answered while the app's is up moves the time it last
        // said busy, but not its turn.
        let answered_since = RegistryEntry { status_updated_at: Some(at(7, 25, 0)), ..reg("busy", at(7, 0, 0)) };
        assert!(log.up_for(&answered_since).is_some());
        // Asked in the second its process started, a few hundred milliseconds before the
        // registry's time.
        assert!(log.up_for(&reg("busy", at(7, 20, 4) + 841)).is_some());
        // A process started since: the app quit with it up.
        assert!(log.up_for(&reg("busy", at(7, 20, 5))).is_none());
        // Seen at rest since: its turn is over.
        log.saw(&RegistryEntry { status_updated_at: Some(at(7, 21, 0)), ..reg("idle", at(7, 0, 0)) });
        assert!(log.up_for(&reg("busy", at(7, 0, 0))).is_none());
        // Its next turn's requests are up.
        writeln!(append(&path), "2026-10-27 07:22:30 [info] Emitted tool permission request e5f6-02 for browser:screenshot in session local_c3d4-77").unwrap();
        log.refresh(&path);
        assert_eq!(log.up_for(&reg("busy", at(7, 0, 0))).map(|a| a.id.as_str()), Some("e5f6-02"));
        // Waiting on a prompt isn't at rest.
        log.saw(&RegistryEntry { status_updated_at: Some(at(7, 23, 0)), ..reg("waiting", at(7, 0, 0)) });
        assert!(log.up_for(&reg("busy", at(7, 0, 0))).is_some());
    }

    #[test]
    fn reads_a_log_moved_aside_to_its_end_then_the_new_one() {
        let dir = crate::fixtures::scratch("claude-applog-cut");
        let path = dir.join("main.log");
        let filler = "2026-10-27 07:00:00 [info] [process-memory] trigger=interval tree_rss_sum=2131MB\n".repeat(40);
        std::fs::write(&path, format!("{filler}{ASKED}")).unwrap();
        let mut log = Log::default();
        assert!(log.refresh(&path));
        // The answer is written, then the app moves the log aside and starts a new one, all
        // before the next read.
        writeln!(append(&path), "2026-10-27 07:30:00 [info] Received permission response for a1b2-01: always (tool: browser:read_console_messages)").unwrap();
        std::fs::rename(&path, dir.join("main1.log")).unwrap();
        std::fs::write(&path, "2026-10-27 07:30:01 [info] Emitted tool permission request e5f6-02 for browser:screenshot in session local_c3d4-77\n").unwrap();
        assert!(log.refresh(&path));
        assert_eq!(log.open.keys().collect::<Vec<_>>(), ["e5f6-02"], "the old log's answer and the new one's request");
        // Cut short where it is, it's read from its start.
        std::fs::write(&path, "2026-10-27 07:31:00 [info] Permission request e5f6-02 for browser:screenshot aborted\n").unwrap();
        assert!(log.refresh(&path));
        assert!(log.open.is_empty());
    }

    #[test]
    fn reads_only_stamped_lines() {
        assert_eq!(stamped("2026-10-27 07:20:04 [info] Emitted x"), Some((at(7, 20, 4), "Emitted x")));
        assert_eq!(stamped("  origin: 'https://app.northwind.dev',"), None);
        assert_eq!(stamped("2026-10-27 07:20 [info] short"), None);
        assert_eq!(stamped("2026-10-27 07:2é:04 [info] x"), None);
        assert_eq!(host("http://localhost:5173/"), "localhost:5173");
    }
}

//! A worktree's app, to open in the browser: the URL set for it, and the server serving it.
//!
//! URLs are set by hand, with `codealong preview set` or the worktree's menu, both through
//! `set`, and kept in `previews.json` next to the config; the command asks the app when it's up
//! and writes the file under a lock when it isn't, and the engine watches it. A URL set by hand is what opens: a project's dev proxy may serve the app at a
//! name of its own (`https://feature.app.test`), where `localhost:<port>` would fail its
//! cookies and sign-in. For the same reason a worktree can name the Chrome profile it opens
//! in, over its project's (chrome.rs).
//!
//! Servers are found, not polled: a thread looks at the ports your processes listen on
//! (`procs::listeners`, a few milliseconds for every worktree at once), keeps the ones whose
//! process runs in a worktree's folder, and asks each new one for its front page once, to tell
//! the app from its API, debugger and metrics ports. Then it waits on the kernel to say when a
//! server's process exits (a kqueue), so a stopped server shows at once, and looks again every
//! 10 seconds for servers that started.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::config::support_dir;
use crate::procs;
use crate::store;

/// How often to look for servers that started. Stopped ones show at once.
const LOOK_EVERY: Duration = Duration::from_secs(10);
/// A server that hasn't answered its first request is asked again on the next looks, this many
/// times in all: a dev server may build its page on the first request, which takes a while.
const TRIES: u8 = 3;

pub fn path() -> PathBuf {
    support_dir().join("previews.json")
}

/// What the file holds, for the error when it doesn't read as that.
const WHAT: &str = "a list of preview URLs";

/// A worktree's URL, set by hand.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The worktree's path, as the board names it.
    pub worktree: String,
    pub url: String,
    /// Which of the worktree's servers is the app, when more than one serves a page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// The Chrome profile it opens in, over its project's `chrome_profile`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chrome_profile: Option<String>,
    pub at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Store {
    #[serde(default)]
    pub previews: Vec<Entry>,
}

impl Store {
    pub fn get(&self, worktree: &str) -> Option<&Entry> {
        self.previews.iter().find(|e| e.worktree == worktree)
    }

    /// Sets a worktree's URL, port and Chrome profile, replacing what it had. False when it
    /// already had these.
    pub fn set(&mut self, worktree: &str, url: &str, port: Option<u16>, chrome_profile: Option<&str>, at: i64) -> bool {
        if self.get(worktree).is_some_and(|e| e.url == url && e.port == port && e.chrome_profile.as_deref() == chrome_profile) {
            return false;
        }
        self.previews.retain(|e| e.worktree != worktree);
        self.previews.push(Entry { worktree: worktree.to_string(), url: url.to_string(), port, chrome_profile: chrome_profile.map(str::to_string), at });
        true
    }

    /// Takes a worktree's URL away. False when it had none.
    pub fn clear(&mut self, worktree: &str) -> bool {
        let before = self.previews.len();
        self.previews.retain(|e| e.worktree != worktree);
        self.previews.len() != before
    }

    /// Makes a change to a worktree's preview (`Change`, its URL made whole and its profile
    /// Chrome's own already): what it gives changes, what it leaves out stays. False when
    /// nothing changed.
    fn apply(&mut self, worktree: &str, change: &Change, at: i64) -> Result<bool> {
        if change.clear {
            return Ok(self.clear(worktree));
        }
        let had = self.get(worktree).cloned();
        let Some(url) = change.url.clone().or_else(|| had.as_ref().map(|e| e.url.clone())) else {
            bail!("{} has no preview URL yet; give one: `codealong preview set <url>`.", crate::util::tilde(Path::new(worktree)));
        };
        // A new URL's port is its own: the one set was for the URL it replaces.
        let port = match change.port {
            Some(port) => port,
            None if change.url.is_some() => None,
            None => had.as_ref().and_then(|e| e.port),
        };
        let profile = change.chrome_profile.clone().unwrap_or_else(|| had.and_then(|e| e.chrome_profile));
        Ok(self.set(worktree, &url, port, profile.as_deref(), at))
    }
}

/// A change to a worktree's preview, as its sheet in the app and `codealong preview set|clear`
/// ask for it: what it gives changes, and what it leaves out stays. The sheet gives everything.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Change {
    /// Takes the URL away, with its port and profile; the rest is then ignored.
    pub clear: bool,
    /// The URL, as typed (`normalize_url`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Which of the worktree's servers is the app, or null for none. Left out, the one set
    /// stays, unless a URL is given.
    #[serde(deserialize_with = "given", skip_serializing_if = "Option::is_none")]
    pub port: Option<Option<u16>>,
    /// The Chrome profile it opens in, as Chrome or you name it, or null (or empty) for its
    /// project's.
    #[serde(deserialize_with = "given", skip_serializing_if = "Option::is_none")]
    pub chrome_profile: Option<Option<String>>,
}

/// A field that's there, null or not: null takes the value away, where a field left out (the
/// default) keeps it.
fn given<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(d: D) -> std::result::Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

/// What a change did: whether anything changed, and the preview as it's set now.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Set {
    pub changed: bool,
    pub preview: Option<Entry>,
}

/// Changes a worktree's preview, for the app and the command alike: its URL is made whole and its
/// profile named as Chrome names it before the file is locked, since Chrome's profiles are a file
/// of its own.
pub fn set(worktree: &str, change: &Change) -> Result<Set> {
    let url = change.url.as_deref().map(normalize_url).transpose()?;
    let chrome_profile = match &change.chrome_profile {
        Some(Some(p)) if !p.trim().is_empty() => Some(Some(crate::chrome::resolve(p.trim())?.id)),
        Some(_) => Some(None),
        None => None,
    };
    let change = Change { url, chrome_profile, ..change.clone() };
    let mut result = Ok(false);
    let store = self::change(|store| {
        result = store.apply(worktree, &change, crate::util::now_ms());
        matches!(result, Ok(true))
    })?;
    Ok(Set { changed: result?, preview: store.get(worktree).cloned() })
}

/// The store, or an empty one when there's no file. A file that doesn't parse is an error, so
/// it's never overwritten.
pub fn load() -> Result<Store> {
    Ok(store::read(&path(), WHAT)?.unwrap_or_default())
}

/// Reads the store under a lock on `previews.json.lock`, lets `f` change it, and saves it
/// through a temporary file when `f` says it changed. URLs of worktrees that are gone go with
/// it.
pub fn change(f: impl FnOnce(&mut Store) -> bool) -> Result<Store> {
    store::change(&path(), WHAT, |s: &mut Store| {
        let changed = f(s);
        if changed {
            s.previews.retain(|e| Path::new(&e.worktree).exists());
        }
        changed
    })
}

/// A URL as typed, made whole: a port alone (`5173`, `:5173`) is on localhost, a local name
/// without a scheme takes `http://`, and any other `https://`. Only http and https open.
pub fn normalize_url(input: &str) -> Result<String> {
    let t = input.trim();
    if t.is_empty() {
        bail!("Give the URL the worktree's app opens at.");
    }
    if t.chars().any(char::is_whitespace) {
        bail!("A URL has no spaces: {t:?}.");
    }
    if let Ok(port) = t.strip_prefix(':').unwrap_or(t).parse::<u16>() {
        return Ok(format!("http://localhost:{port}"));
    }
    // `mailto:x`, `javascript:…`: a scheme without `//`, where a port would have digits.
    if !t.contains("://") && !t.starts_with('[') {
        if let Some((_, after)) = t.split_once(':') {
            if !after.starts_with(|c: char| c.is_ascii_digit()) {
                bail!("Only http and https URLs open as a preview: {t:?}.");
            }
        }
    }
    let url = match t.split_once("://") {
        Some(_) => t.to_string(),
        None if is_local(host_port(t).0) => format!("http://{t}"),
        None => format!("https://{t}"),
    };
    let Some((scheme, rest)) = url.split_once("://") else { unreachable!() };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        bail!("Only http and https URLs open as a preview, not {scheme}.");
    }
    if host_port(rest).0.is_empty() {
        bail!("{url} has no host.");
    }
    Ok(url)
}

/// The host and port of a URL without its scheme: `[::1]:5173/x` → (`::1`, 5173).
fn host_port(rest: &str) -> (&str, Option<u16>) {
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
    if let Some(v6) = authority.strip_prefix('[') {
        let (host, after) = v6.split_once(']').unwrap_or((v6, ""));
        return (host, after.strip_prefix(':').and_then(|p| p.parse().ok()));
    }
    match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()),
        None => (authority, None),
    }
}

/// Names that reach this Mac without a lookup: browsers send `*.localhost` here too.
fn is_local(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    h == "localhost" || h.ends_with(".localhost") || h == "::1" || h == "0.0.0.0" || h.starts_with("127.")
}

/// The port a URL reaches on this Mac directly, when it names this Mac: which server it is.
/// None for a name a proxy serves.
pub fn local_port(url: &str) -> Option<u16> {
    let (scheme, rest) = url.split_once("://")?;
    let (host, port) = host_port(rest);
    if !is_local(host) {
        return None;
    }
    port.or(if scheme.eq_ignore_ascii_case("https") { Some(443) } else { Some(80) })
}

/// A URL as the board shows it: `feature.app.test`, `localhost:5173/admin`.
pub fn short_url(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.strip_suffix('/').unwrap_or(rest)
}

/// A process listening on a TCP port whose folder is in a worktree.
#[derive(Clone, Debug, PartialEq)]
pub struct Server {
    /// The worktree's path, as the board names it.
    pub lane: PathBuf,
    pub pid: u32,
    pub port: u16,
    pub name: String,
    pub cwd: PathBuf,
    pub started_ms: i64,
    /// It answered its first request with a page (HTML, or a redirect to one): an app, not an
    /// API, a debugger or a metrics port.
    pub page: bool,
    /// The page's `<title>`.
    pub title: Option<String>,
    /// Its parent processes, nearest first, to find the session that started it.
    pub ancestors: Vec<u32>,
}

/// How a server answered its first request.
#[derive(Clone, Debug, PartialEq)]
enum Answer {
    Page(Option<String>),
    Other,
    /// Not yet, after this many tries.
    Unanswered(u8),
}

/// What an HTTP response said, as far as a preview cares.
#[derive(Debug, PartialEq)]
struct Response {
    status: u16,
    content_type: Option<String>,
    title: Option<String>,
}

/// Whether a response is a page someone would open: HTML, a redirect (to a sign-in page, say),
/// or a sign-in wall. An API's 404 at `/`, and anything not HTML, isn't.
fn is_page(status: u16, content_type: Option<&str>) -> bool {
    let html = content_type.is_some_and(|c| c.trim().to_ascii_lowercase().starts_with("text/html"));
    match status {
        300..=399 => true,
        200..=299 | 401 | 403 => html,
        _ => false,
    }
}

/// The status line, content type and title of an HTTP/1 response, from its first bytes. None
/// when it isn't HTTP.
fn parse_response(bytes: &[u8]) -> Option<Response> {
    let text = String::from_utf8_lossy(bytes);
    let first = text.lines().next()?;
    let mut parts = first.split_whitespace();
    if !parts.next()?.starts_with("HTTP/") {
        return None;
    }
    let status: u16 = parts.next()?.parse().ok()?;
    let (head, body) = text.split_once("\r\n\r\n").or_else(|| text.split_once("\n\n")).unwrap_or((&text, ""));
    let content_type = head.lines().skip(1).find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim().eq_ignore_ascii_case("content-type").then(|| v.trim().to_string())
    });
    Some(Response { status, content_type, title: title_of(body) })
}

/// A page's `<title>`, its entities read and its spaces collapsed.
fn title_of(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;
    let raw = html[start..end].split_whitespace().collect::<Vec<_>>().join(" ");
    let title = raw.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&");
    let title: String = title.chars().take(120).collect();
    (!title.is_empty()).then_some(title)
}

/// Asks a server for its front page, as a browser on this Mac would. None when it didn't
/// answer in time; an answer that isn't HTTP reads as one that isn't a page.
fn probe(addr: IpAddr, port: u16) -> Option<Response> {
    const LIMIT: usize = 64 * 1024;
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut s = TcpStream::connect_timeout(&SocketAddr::new(addr, port), Duration::from_secs(1)).ok()?;
    let _ = s.set_write_timeout(Some(Duration::from_secs(1)));
    // The name a browser here sends: dev servers check it (Vite's allowed hosts).
    let host = if addr.is_loopback() { format!("localhost:{port}") } else { SocketAddr::new(addr, port).to_string() };
    // HTTP/1.0, so the answer is neither chunked nor kept open.
    let request = format!("GET / HTTP/1.0\r\nHost: {host}\r\nAccept: text/html\r\nUser-Agent: Codealong\r\nConnection: close\r\n\r\n");
    s.write_all(request.as_bytes()).ok()?;
    let mut got: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let _ = s.set_read_timeout(Some(left));
        match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                got.extend_from_slice(&chunk[..n]);
                // Enough once the title has come, or the head says it isn't a page.
                let lower = String::from_utf8_lossy(&got).to_ascii_lowercase();
                if got.len() >= LIMIT || lower.contains("</title") || (lower.contains("\r\n\r\n") && !lower.contains("text/html")) {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    if got.is_empty() {
        // Connected, then nothing: still starting, or not a server that speaks first.
        return None;
    }
    Some(parse_response(&got).unwrap_or(Response { status: 0, content_type: None, title: None }))
}

/// The worktree a folder is in: the deepest whose path holds it, so a worktree made inside
/// another checkout (`.claude/worktrees/<name>`) is told from it. `canon` has each worktree's
/// real path, since the kernel reports the folder with links followed (`/private/tmp`).
fn lane_of(cwd: &Path, roots: &[PathBuf], canon: &HashMap<PathBuf, PathBuf>) -> Option<PathBuf> {
    roots
        .iter()
        .filter_map(|r| {
            let real = canon.get(r).unwrap_or(r);
            if cwd.starts_with(real) {
                Some((real.components().count(), r))
            } else if cwd.starts_with(r) {
                Some((r.components().count(), r))
            } else {
                None
            }
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, r)| r.clone())
}

/// Finds the servers in worktrees, remembering how each answered so it's asked only once.
#[derive(Default)]
pub struct Scanner {
    answers: HashMap<(u32, u16, i64), Answer>,
    canon: HashMap<PathBuf, PathBuf>,
}

impl Scanner {
    /// The servers listening in `roots` (worktree paths), in a stable order.
    pub fn scan(&mut self, roots: &[PathBuf]) -> Vec<Server> {
        if roots.is_empty() {
            self.answers.clear();
            return Vec::new();
        }
        self.canon.retain(|k, _| roots.contains(k));
        for r in roots {
            if !self.canon.contains_key(r) {
                self.canon.insert(r.clone(), std::fs::canonicalize(r).unwrap_or_else(|_| r.clone()));
            }
        }
        let own = std::process::id();
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for l in procs::listeners() {
            if l.pid == own {
                continue;
            }
            let Some(lane) = lane_of(&l.cwd, roots, &self.canon) else { continue };
            let key = (l.pid, l.port, l.started_ms);
            seen.insert(key);
            let answer = self.answers.entry(key).or_insert(Answer::Unanswered(0));
            if let Answer::Unanswered(tries) = *answer {
                if tries < TRIES {
                    *answer = match probe(l.addr, l.port) {
                        Some(r) if is_page(r.status, r.content_type.as_deref()) => Answer::Page(r.title),
                        Some(_) => Answer::Other,
                        None => Answer::Unanswered(tries + 1),
                    };
                }
            }
            let (page, title) = match answer {
                Answer::Page(t) => (true, t.clone()),
                _ => (false, None),
            };
            out.push(Server { lane, pid: l.pid, port: l.port, name: l.name, cwd: l.cwd, started_ms: l.started_ms, page, title, ancestors: procs::ancestors(l.pid) });
        }
        self.answers.retain(|k, _| seen.contains(k));
        out.sort_by(|a, b| (&a.lane, a.port, a.pid).cmp(&(&b.lane, b.port, b.pid)));
        out
    }
}

/// The servers in `roots` now, for a snapshot.
pub fn scan_once(roots: &[PathBuf]) -> Vec<Server> {
    Scanner::default().scan(roots)
}

/// The thread that finds servers: tell it which worktrees there are, and it says what serves
/// in them whenever that changes.
pub struct Watcher {
    roots: Arc<Mutex<Vec<PathBuf>>>,
    kq: i32,
}

/// The identifier of the event that wakes the thread when the worktrees change.
const WAKE: usize = 1;

impl Watcher {
    /// The worktrees to look in. A change is looked at now rather than on the next look.
    pub fn set_roots(&self, roots: Vec<PathBuf>) {
        let Ok(mut have) = self.roots.lock() else { return };
        if *have == roots {
            return;
        }
        *have = roots;
        drop(have);
        let wake = kevent(WAKE, libc::EVFILT_USER, 0, libc::NOTE_TRIGGER);
        // SAFETY: kevent reads one change from a struct we own and returns no events.
        unsafe { libc::kevent(self.kq, &wake, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
    }
}

fn kevent(ident: usize, filter: i16, flags: u16, fflags: u32) -> libc::kevent {
    libc::kevent { ident, filter, flags, fflags, data: 0, udata: std::ptr::null_mut() }
}

/// Starts the thread. `send` gets the servers each time they change, and stops the thread by
/// returning false. None when the kernel won't make a kqueue.
pub fn watch(send: impl Fn(Vec<Server>) -> bool + Send + 'static) -> Option<Watcher> {
    // SAFETY: kqueue takes no arguments; a negative result is an error.
    let kq = unsafe { libc::kqueue() };
    if kq < 0 {
        return None;
    }
    let wake = kevent(WAKE, libc::EVFILT_USER, libc::EV_ADD | libc::EV_CLEAR, 0);
    // SAFETY: kevent reads one change from a struct we own and returns no events.
    unsafe { libc::kevent(kq, &wake, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
    let roots = Arc::new(Mutex::new(Vec::new()));
    let shared = roots.clone();
    std::thread::Builder::new()
        .name("codealong-previews".into())
        .spawn(move || {
            procs::utility_thread();
            let mut scanner = Scanner::default();
            let mut last: Option<Vec<Server>> = None;
            let mut watched: HashSet<u32> = HashSet::new();
            loop {
                let roots = shared.lock().map(|r| r.clone()).unwrap_or_default();
                let servers = scanner.scan(&roots);
                // The kernel says when each server's process exits, which is looked at at once.
                for pid in servers.iter().map(|s| s.pid) {
                    if watched.insert(pid) {
                        let exit = kevent(pid as usize, libc::EVFILT_PROC, libc::EV_ADD | libc::EV_ONESHOT, libc::NOTE_EXIT);
                        // SAFETY: as above. A process that's already gone is refused, and the
                        // next look finds it gone.
                        unsafe { libc::kevent(kq, &exit, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
                    }
                }
                if last.as_ref() != Some(&servers) {
                    if !send(servers.clone()) {
                        break;
                    }
                    last = Some(servers);
                }
                let mut events: [libc::kevent; 16] = [kevent(0, 0, 0, 0); 16];
                let timeout = libc::timespec { tv_sec: LOOK_EVERY.as_secs() as libc::time_t, tv_nsec: 0 };
                // SAFETY: kevent writes at most `events.len()` events into the array.
                let n = unsafe { libc::kevent(kq, std::ptr::null(), 0, events.as_mut_ptr(), events.len() as libc::c_int, &timeout) };
                for e in events.iter().take(n.max(0) as usize) {
                    if e.filter == libc::EVFILT_PROC {
                        watched.remove(&(e.ident as u32));
                    }
                }
            }
            // SAFETY: the descriptor is ours and nothing uses it after this.
            unsafe { libc::close(kq) };
        })
        .ok()?;
    Some(Watcher { roots, kq })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_keeps_what_it_leaves_out() {
        let mut s = Store::default();
        let change = |url: Option<&str>, port: Option<Option<u16>>, profile: Option<Option<&str>>| Change { url: url.map(String::from), port, chrome_profile: profile.map(|p| p.map(String::from)), ..Default::default() };
        // Nothing to keep yet: a change needs a URL.
        let err = s.apply("/r/app", &change(None, None, Some(Some("Work"))), 1).unwrap_err();
        assert!(err.to_string().contains("no preview URL yet"), "{err}");
        // The sheet gives all of it, which replaces what was there.
        assert!(s.apply("/r/app", &change(Some("https://app.test"), Some(Some(5173)), Some(Some("Profile 2"))), 1).unwrap());
        assert!(!s.apply("/r/app", &change(Some("https://app.test"), Some(Some(5173)), Some(Some("Profile 2"))), 2).unwrap(), "nothing changed");
        // The command's `--chrome-profile` alone keeps the URL and its port.
        assert!(s.apply("/r/app", &change(None, None, Some(Some("Default"))), 3).unwrap());
        let e = s.get("/r/app").unwrap();
        assert_eq!((e.url.as_str(), e.port, e.chrome_profile.as_deref()), ("https://app.test", Some(5173), Some("Default")));
        // A new URL drops the old one's port, and keeps the profile.
        assert!(s.apply("/r/app", &change(Some("https://other.test"), None, None), 4).unwrap());
        let e = s.get("/r/app").unwrap();
        assert_eq!((e.port, e.chrome_profile.as_deref()), (None, Some("Default")));
        // A port alone keeps the URL; no profile takes the worktree's own away.
        assert!(s.apply("/r/app", &change(None, Some(Some(3000)), Some(None)), 5).unwrap());
        let e = s.get("/r/app").unwrap();
        assert_eq!((e.url.as_str(), e.port, e.chrome_profile.as_deref()), ("https://other.test", Some(3000), None));
        assert!(s.apply("/r/app", &Change { clear: true, url: Some("ignored".into()), ..Default::default() }, 6).unwrap());
        assert!(s.get("/r/app").is_none());
        // Left out and null read apart: null takes it away.
        let read: Change = serde_json::from_value(serde_json::json!({ "url": "5173", "chromeProfile": null })).unwrap();
        assert_eq!((read.port, read.chrome_profile), (None, Some(None)));
    }

    #[test]
    fn urls_are_made_whole() {
        assert_eq!(normalize_url("5173").unwrap(), "http://localhost:5173");
        assert_eq!(normalize_url(" :5173 ").unwrap(), "http://localhost:5173");
        assert_eq!(normalize_url("localhost:3000/admin").unwrap(), "http://localhost:3000/admin");
        assert_eq!(normalize_url("app.localhost:1355").unwrap(), "http://app.localhost:1355");
        assert_eq!(normalize_url("127.0.0.1:8080").unwrap(), "http://127.0.0.1:8080");
        assert_eq!(normalize_url("feature.app.test").unwrap(), "https://feature.app.test");
        assert_eq!(normalize_url("http://feature.app.test/").unwrap(), "http://feature.app.test/");
        assert!(normalize_url("").is_err());
        assert!(normalize_url("file:///etc/passwd").is_err());
        assert!(normalize_url("javascript:alert(1)").is_err());
        assert!(normalize_url("mailto:me@example.com").is_err());
        assert_eq!(normalize_url("[::1]:8080").unwrap(), "http://[::1]:8080");
        assert!(normalize_url("https://a b").is_err());
        assert!(normalize_url("https://").is_err());
    }

    #[test]
    fn a_local_url_names_its_server_by_port() {
        assert_eq!(local_port("http://localhost:5173/x"), Some(5173));
        assert_eq!(local_port("http://[::1]:8080"), Some(8080));
        assert_eq!(local_port("http://127.0.0.1"), Some(80));
        assert_eq!(local_port("https://app.localhost"), Some(443));
        assert_eq!(local_port("https://feature.app.test:8443"), None);
        assert_eq!(short_url("https://feature.app.test/"), "feature.app.test");
        assert_eq!(short_url("http://localhost:5173/admin"), "localhost:5173/admin");
    }

    #[test]
    fn a_worktree_keeps_one_url() {
        let mut s = Store::default();
        assert!(s.set("/w/a", "https://a.test", None, None, 1));
        assert!(!s.set("/w/a", "https://a.test", None, None, 2));
        assert!(s.set("/w/a", "https://a.test", Some(5180), None, 3));
        assert!(s.set("/w/a", "https://a.test", Some(5180), Some("Work"), 4));
        assert!(!s.set("/w/a", "https://a.test", Some(5180), Some("Work"), 5));
        assert_eq!(s.previews.len(), 1);
        assert_eq!(s.get("/w/a").unwrap().port, Some(5180));
        assert_eq!(s.get("/w/a").unwrap().chrome_profile.as_deref(), Some("Work"));
        assert!(s.clear("/w/a"));
        assert!(!s.clear("/w/a"));
    }

    #[test]
    fn pages_are_told_from_apis() {
        assert!(is_page(200, Some("text/html; charset=utf-8")));
        assert!(is_page(302, None));
        assert!(is_page(401, Some("text/html")));
        assert!(!is_page(404, Some("application/json; charset=utf-8")));
        assert!(!is_page(404, Some("text/html")));
        assert!(!is_page(200, Some("application/json")));
        assert!(!is_page(200, None));
        assert!(!is_page(0, None));
    }

    #[test]
    fn reads_a_response() {
        let vite = b"HTTP/1.1 200 OK\r\nVary: Origin\r\nContent-Type: text/html\r\nCache-Control: no-cache\r\n\r\n<!doctype html>\n<html><head><TITLE>\n  Specs &amp; more\n</TITLE></head>";
        let r = parse_response(vite).unwrap();
        assert_eq!((r.status, r.content_type.as_deref(), r.title.as_deref()), (200, Some("text/html"), Some("Specs & more")));
        let api = b"HTTP/1.1 404 Not Found\r\ncontent-type: application/json; charset=utf-8\r\n\r\n{\"message\":\"Cannot GET /\"}";
        assert_eq!(parse_response(api).unwrap().status, 404);
        assert!(parse_response(b"\x15\x03\x01\x00\x02\x02\x46").is_none());
        assert!(parse_response(b"-ERR unknown command").is_none());
    }

    #[test]
    fn a_server_belongs_to_the_deepest_worktree() {
        let roots = vec![PathBuf::from("/p/app"), PathBuf::from("/p/app/.claude/worktrees/x"), PathBuf::from("/p/app-two")];
        let canon = HashMap::from([(PathBuf::from("/p/app-two"), PathBuf::from("/private/p/app-two"))]);
        assert_eq!(lane_of(Path::new("/p/app/web"), &roots, &canon), Some(PathBuf::from("/p/app")));
        assert_eq!(lane_of(Path::new("/p/app/.claude/worktrees/x/web"), &roots, &canon), Some(PathBuf::from("/p/app/.claude/worktrees/x")));
        assert_eq!(lane_of(Path::new("/private/p/app-two"), &roots, &canon), Some(PathBuf::from("/p/app-two")));
        assert_eq!(lane_of(Path::new("/p/apple"), &roots, &canon), None);
        assert_eq!(lane_of(Path::new("/"), &roots, &canon), None);
    }

    /// A page served in a worktree is found and read; a port that isn't HTTP is found and isn't
    /// a page; and the kernel's word that the server's process ended reaches the watcher.
    #[test]
    fn finds_servers_in_worktrees() {
        let dir = std::env::temp_dir().join(format!("codealong-previews-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = [
            "import http.server, socketserver",
            "class H(http.server.BaseHTTPRequestHandler):",
            "    def do_GET(self):",
            "        self.send_response(200); self.send_header('Content-Type', 'text/html'); self.end_headers()",
            "        self.wfile.write(b'<html><title>Test app</title></html>')",
            "    def log_message(self, *a): pass",
            "s = socketserver.TCPServer(('127.0.0.1', 0), H)",
            "print(s.server_address[1], flush=True)",
            "s.serve_forever()",
        ]
        .join("\n");
        let mut child = std::process::Command::new("/usr/bin/python3").arg("-c").arg(&script).current_dir(&dir).stdout(std::process::Stdio::piped()).spawn().unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(child.stdout.take().unwrap()), &mut line).unwrap();
        let port: u16 = line.trim().parse().unwrap();
        let roots = vec![dir.clone()];
        let servers = scan_once(&roots);
        let s = servers.iter().find(|s| s.port == port).expect("the test server");
        assert_eq!((s.pid, s.page, s.title.as_deref(), &s.lane), (child.id(), true, Some("Test app"), &dir));
        assert!(s.ancestors.contains(&std::process::id()));
        let seen = Arc::new(Mutex::new(Vec::<Vec<Server>>::new()));
        let log = seen.clone();
        let w = watch(move |s| {
            log.lock().unwrap().push(s);
            true
        })
        .unwrap();
        w.set_roots(roots);
        let waited = Instant::now();
        while !seen.lock().unwrap().iter().any(|s| s.iter().any(|s| s.port == port)) {
            assert!(waited.elapsed() < Duration::from_secs(10), "the watcher didn't find the server");
            std::thread::sleep(Duration::from_millis(50));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        // Well within the 10 seconds between looks: the exit was heard.
        let stopped = Instant::now();
        while seen.lock().unwrap().last().is_some_and(|s| s.iter().any(|s| s.port == port)) {
            assert!(stopped.elapsed() < Duration::from_secs(3), "the watcher didn't hear the server stop");
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

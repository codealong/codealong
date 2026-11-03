//! Improve Codealong: a session that improves Codealong itself, from the button beside New
//! session, its shortcut (`[shortcuts] improve`, quick.rs) or the phone. The quick panel opens on
//! Codealong's own project, in its source's main checkout, with a picture of every Codealong
//! window as it is, and asks what to change; the session starts with the pictures in its prompt.
//!
//! Codealong's source is any checkout a scan finds that holds the app's `src-tauri/tauri.conf.json`
//! with Codealong's identifier (`is_source`), so it's found by content, not by the folder's name or
//! the remote's owner, and a fork or a clone under another name counts. The engine marks the lanes
//! that are its worktrees (`Lane::codealong`, in codealong-peer's model, so the phone finds them the
//! same way) and says on the board where a session goes (`Board::improve`): a project and a lane,
//! or the source found in a folder Codealong searches but in none of your projects, so the panel
//! can offer to make one; or nothing, when the source isn't on this Mac. In the installed app (a
//! release), whose code no change to the source reaches, the board says so (`Improve::installed`)
//! and the panel first suggests switching to Codealong Dev, offering a session that sets it up.
//!
//! The pictures are each window's own drawing, asked of WKWebView (`takeSnapshot`), which draws
//! the page as it is without macOS's screen-recording permission, and are kept as attachments
//! (attachments.rs), so the prompt refers to each as `![Main window](images/…)` and the agent
//! reads them as it reads any image. A session shown with its conversation (a panel or a card,
//! which its page marks) that runs in a terminal tab gets a picture of that tab too, after its
//! window's: the server finds the tab and the title of the window it's drawn in
//! (`launch::tab_window`, which Ghostty answers), and the app finds that window among its
//! terminal's and pictures it (`os::outline::window_picture`), which macOS lets it do only with
//! the Screen Recording permission, asked for the first time one's wanted.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use codealong_peer::model::{Lane, Origin};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::attachments::Attachment;
use crate::model::Board;
use crate::{bar, desktop, kept, panel, quick};

/// The app's identifier in `src-tauri/tauri.conf.json`. The dev build's (`….dev`) is in
/// `tauri.dev.conf.json`, laid over this file from the same checkout, so this one is in every
/// checkout of the source.
pub const IDENTIFIER: &str = "app.codealong.desktop";

/// Whether `root` is a checkout of Codealong's source.
pub fn is_source(root: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(root.join("src-tauri").join("tauri.conf.json")) else { return false };
    is_source_conf(&text)
}

fn is_source_conf(text: &str) -> bool {
    serde_json::from_str::<Value>(text).ok().and_then(|v| v.get("identifier")?.as_str().map(|id| id == IDENTIFIER)).unwrap_or(false)
}

/// Where a session that improves Codealong goes, on the board.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Improve {
    /// The project Codealong's source is in, when it's in one of yours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The worktree the session starts in: the source's main checkout, or another of its
    /// worktrees when the main checkout isn't on the board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane_id: Option<String>,
    /// Codealong's source, found in a folder Codealong searches but in none of your projects: a
    /// project can be made for it. None when the source is in a project, or isn't on this Mac.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// This app is the Codealong people install (a release, `updates::enabled`), not a build of
    /// the source: a change made to the source never shows in it, so the panel suggests setting
    /// up Codealong Dev first, which an agent can do (agentTasks.ts `devSetupTask`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub installed: bool,
}

/// A checkout of Codealong's source in none of your projects.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// Its main checkout's folder.
    pub root: String,
    /// `github.com/owner/name`, as `[[projects]] repos` name repos, when it has a remote; a
    /// project for a checkout with none lists its folder instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
}

/// Marks the lanes that are worktrees of Codealong's source: `checkout_of` is each lane's main
/// checkout (the engine's `lane_checkout`), `sources` the checkouts that are the source. A lane
/// that is a source's main checkout itself is one too: a project's `folders` entry makes a lane
/// of a checkout with no remote, which `checkout_of` doesn't list.
pub fn mark(lanes: &mut [Lane], checkout_of: &HashMap<PathBuf, PathBuf>, sources: &HashSet<PathBuf>) {
    for lane in lanes {
        let path = Path::new(&lane.id);
        lane.codealong = sources.contains(path) || checkout_of.get(path).is_some_and(|root| sources.contains(root));
    }
}

/// Where a session that improves Codealong goes: the source's main checkout when it's on the
/// board, else another of its worktrees; with none, the first source found (`root`, its
/// identity) for a project to be made for. The board has only this device's lanes; the quick
/// panel looks through your other Macs' worktrees itself (improve.ts).
pub fn target(lanes: &[Lane], sources: &[(PathBuf, Option<String>)]) -> Improve {
    // An Elsewhere lane (a session ran in a source checkout that's in none of your projects) is
    // no place to start: the source is offered for a project instead.
    let here: Vec<&Lane> = lanes.iter().filter(|l| l.codealong && !l.starts_chats && l.device.is_none() && l.project != crate::board::ELSEWHERE).collect();
    let pick = here.iter().find(|l| l.origin == Origin::Main && !l.hidden).or_else(|| here.iter().find(|l| !l.hidden)).or_else(|| here.first());
    if let Some(lane) = pick {
        return Improve { project: Some(lane.project.clone()), lane_id: Some(lane.id.clone()), ..Improve::default() };
    }
    let source = sources.first().map(|(root, repo)| Source { root: root.to_string_lossy().into_owned(), repo: repo.clone() });
    Improve { source, ..Improve::default() }
}

/// What a window is called in the prompt, by its label (`![Main window](…)`); None for the quick
/// panel itself, which is about to open on the pictures. `notch` says where the bar is docked.
pub fn window_name(label: &str, notch: bool) -> Option<String> {
    let name = match label {
        quick::LABEL => return None,
        "main" => "Main window",
        l if l.starts_with("project-") => "Project window",
        bar::LABEL if notch => "Bar in the notch",
        bar::LABEL => "Floating bar",
        crate::waiting::LABEL => "Waiting on you window",
        crate::peek::LABEL => "Peek",
        l if l.starts_with(kept::PREFIX) => "Kept session window",
        l if l.starts_with(desktop::PREFIX) => "Card pinned to the desktop",
        crate::lineage::LABEL => "Lineage window",
        other => return Some(format!("{other} window")),
    };
    Some(name.to_string())
}

/// Names for a set of windows, in the order given: two of a kind are numbered ("Kept session
/// window 1", "Kept session window 2"), one of a kind isn't.
pub fn window_names<'a>(labels: impl IntoIterator<Item = &'a str>, notch: bool) -> Vec<(&'a str, String)> {
    let named: Vec<(&str, String)> = labels.into_iter().filter_map(|l| window_name(l, notch).map(|n| (l, n))).collect();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for (_, n) in &named {
        *counts.entry(n.as_str()).or_default() += 1;
    }
    let mut seen: HashMap<String, usize> = HashMap::new();
    named
        .iter()
        .map(|(l, n)| {
            if counts[n.as_str()] > 1 {
                let i = seen.entry(n.clone()).or_default();
                *i += 1;
                (*l, format!("{n} {i}"))
            } else {
                (*l, n.clone())
            }
        })
        .collect()
}

/// How long the pictures are waited for before the panel opens with the ones there are: a page
/// that doesn't draw (one on a Space macOS keeps asleep) doesn't hold the panel.
const CAPTURE_WAIT: Duration = Duration::from_millis(1500);

/// How long, from the click, the pictures of the terminal tabs of the sessions the windows show
/// are waited for. Finding a tab is a question for its terminal (Ghostty's marks its TTY first,
/// launch/ghostty.rs `terminal_of`), which answers within a second as a rule; macOS naps Ghostty
/// behind other windows, and a napped one can take far longer, so the panel opens without it.
const TAB_WAIT: Duration = Duration::from_millis(3000);

/// How long the server looks for a session's tab (calls.rs `improve_tab`): what's left of
/// `TAB_WAIT` once the windows have said what they show, less the picture.
pub const TAB_FIND_WAIT: Duration = Duration::from_millis(2500);

/// Asks a page for the sessions it shows with their conversation, which it marks
/// (SessionPanel.tsx `data-shows-session`), as a JSON list of their keys.
const SHOWN: &str = "JSON.stringify(Array.from(document.querySelectorAll('[data-shows-session]'), (e) => e.dataset.showsSession))";

/// A session a window shows, in a terminal tab whose window can be pictured: what the server
/// finds (calls.rs `improve_tab`), and the app pictures, since macOS's Screen Recording
/// permission is the app's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tab {
    /// The session's title, which names the picture.
    pub session: String,
    #[serde(flatten)]
    pub window: crate::launch::TabWindow,
}

/// The terminal tab the session `key` runs in, when it's live in a terminal app whose tabs'
/// windows Codealong can find (`launch::tab_window`). On the server.
pub fn tab_of(board: &Board, key: &str, within: Duration) -> anyhow::Result<Option<Tab>> {
    let Some((_, s)) = board.session(key) else { return Ok(None) };
    let (true, Some(pid), Some(_)) = (s.live, s.pid, s.terminal.as_ref()) else { return Ok(None) };
    Ok(crate::launch::tab_window(pid, Path::new(&s.cwd), within)?.map(|window| Tab { session: s.title.clone(), window }))
}

/// A picture of a session's terminal tab, as PNG. None where macOS doesn't let Codealong picture
/// another app's window, or the tab's window can't be told from the app's others
/// (`os::outline::window_picture`).
pub fn tab_png(tab: &Tab) -> Option<Vec<u8>> {
    crate::os::outline::window_picture(&tab.window.bundle, |title| same_title(title, &tab.window.title))
}

/// The picture of a session's terminal tab, kept and named for it ("Ghostty tab of …").
fn tab_picture(tab: &Tab) -> Option<Attachment> {
    let png = tab_png(tab)?;
    let name = format!("{} tab of {}", tab.window.terminal, tab.session);
    crate::attachments::keep(&png, &format!("{name}.png")).ok().map(|a| Attachment { name, ..a })
}

/// Whether a window's title is the one its terminal gave the tab, read a moment before: a
/// program that names its tab may lead the name with a sign of what it's doing, which changes
/// meanwhile (Claude Code's ✳ and its spinner), so what comes before the first letter or digit
/// is left out of both.
fn same_title(window: &str, tab: &str) -> bool {
    let bare = |t: &str| t.trim_start_matches(|c: char| !c.is_alphanumeric()).trim_end().to_string();
    let tab = bare(tab);
    !tab.is_empty() && bare(window) == tab
}

/// Where a picture goes in the prompt: its window's place in the order asked, then 0 for the
/// window itself and 1, 2… for the tabs of the sessions it shows, after it.
type Place = (usize, usize);

struct Capture {
    /// Pictures not in yet, and windows that haven't said what sessions they show.
    left: usize,
    /// Of those, the terminal tabs' pictures, which are waited for longer.
    tabs: usize,
    /// The windows' wait is over (`CAPTURE_WAIT`): only the tabs' pictures hold the panel now.
    windows_over: bool,
    /// The sessions whose tab is asked for: one shown in two windows is pictured once.
    asked: HashSet<String>,
    /// Each picture, with its place.
    shots: Vec<(Place, Attachment)>,
    /// What runs once with the pictures; None once it has.
    done: Option<Box<dyn FnOnce(Vec<Attachment>) + Send>>,
}

impl Capture {
    fn new(windows: usize, done: impl FnOnce(Vec<Attachment>) + Send + 'static) -> Capture {
        // Each window's picture, and its say on the sessions it shows.
        Capture { left: 2 * windows, tabs: 0, windows_over: false, asked: HashSet::new(), shots: Vec::new(), done: Some(Box::new(done)) }
    }

    /// Whether what's not in yet holds the panel.
    fn held(&self) -> bool {
        if self.windows_over {
            self.tabs > 0
        } else {
            self.left > 0
        }
    }

    /// The sessions a window shows that no window's shown before, each now waited for.
    fn shown(&mut self, keys: Vec<String>) -> Vec<String> {
        let fresh: Vec<String> = keys.into_iter().filter(|k| self.asked.insert(k.clone())).collect();
        self.left += fresh.len();
        self.tabs += fresh.len();
        fresh
    }

    /// A picture came, or didn't, or a window said what it shows: one less to wait for.
    fn came(&mut self, shot: Option<(Place, Attachment)>, tab: bool) {
        self.left = self.left.saturating_sub(1);
        if tab {
            self.tabs = self.tabs.saturating_sub(1);
        }
        self.shots.extend(shot);
    }
}

/// Runs `done` once: when nothing holds the panel, or when `force` says the wait is over.
fn settle(capture: &Arc<Mutex<Capture>>, force: bool) {
    let Ok(mut c) = capture.lock() else { return };
    if c.held() && !force {
        return;
    }
    let Some(done) = c.done.take() else { return };
    let mut shots = std::mem::take(&mut c.shots);
    drop(c);
    shots.sort_by_key(|(place, _)| *place);
    done(shots.into_iter().map(|(_, a)| a).collect());
}

/// Keeps a picture of every Codealong window that's up, the quick panel aside, and of the
/// terminal tab of each session they show, and calls `done` with them, main window first, the
/// tabs of a window's sessions after it, each named for what it is. On the main thread; `done`
/// runs on it too, after this returns.
pub fn capture(app: &AppHandle, done: impl FnOnce(Vec<Attachment>) + Send + 'static) {
    let notch = bar::mode_of(app) == bar::Mode::Notch;
    let windows: HashMap<String, WebviewWindow> = app.webview_windows();
    let mut labels: Vec<&str> = windows.keys().map(String::as_str).filter(|l| windows.get(*l).is_some_and(panel::is_visible)).collect();
    // The main window first, then the rest in a steady order.
    labels.sort_by_key(|l| (*l != "main", l.to_string()));
    let named = window_names(labels, notch);
    let capture = Arc::new(Mutex::new(Capture::new(named.len(), done)));
    if named.is_empty() {
        settle(&capture, true);
        return;
    }
    for (i, (label, name)) in named.into_iter().enumerate() {
        let Some(window) = windows.get(label) else { continue };
        if !ask_picture(window, i, name, capture.clone()) {
            one_in(&capture, None, false);
        }
        if !ask_shown(app, window, i, capture.clone()) {
            one_in(&capture, None, false);
        }
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(CAPTURE_WAIT);
        let over = capture.clone();
        let _ = handle.run_on_main_thread(move || {
            if let Ok(mut c) = over.lock() {
                c.windows_over = true;
            }
            settle(&over, false);
        });
        std::thread::sleep(TAB_WAIT.saturating_sub(CAPTURE_WAIT));
        let _ = handle.run_on_main_thread(move || settle(&capture, true));
    });
}

/// A picture came, or didn't, or a window said what it shows.
fn one_in(capture: &Arc<Mutex<Capture>>, shot: Option<(Place, Attachment)>, tab: bool) {
    if let Ok(mut c) = capture.lock() {
        c.came(shot, tab);
    }
    settle(capture, false);
}

/// Asks a window's page for its picture (os::windows), kept as `<name>.png` and reported; false
/// when it couldn't be asked, as on Linux.
fn ask_picture(window: &WebviewWindow, i: usize, name: String, capture: Arc<Mutex<Capture>>) -> bool {
    crate::os::windows::picture(window, move |png| {
        let shot = png.and_then(|png| crate::attachments::keep(&png, &format!("{name}.png")).ok()).map(|a| ((i, 0), Attachment { name: name.clone(), ..a }));
        one_in(&capture, shot, false);
    })
}

/// Asks a window's page what sessions it shows, then the server for the terminal tab of each
/// not asked for yet, and pictures it, off the main thread; false when the page couldn't be
/// asked, as on Linux.
fn ask_shown(app: &AppHandle, window: &WebviewWindow, i: usize, capture: Arc<Mutex<Capture>>) -> bool {
    let app = app.clone();
    crate::os::windows::ask_page(window, SHOWN, move |answer| {
        let keys: Vec<String> = answer.and_then(|a| serde_json::from_str(&a).ok()).unwrap_or_default();
        let fresh = capture.lock().map(|mut c| c.shown(keys)).unwrap_or_default();
        for (j, key) in fresh.into_iter().enumerate() {
            let (app, capture) = (app.clone(), capture.clone());
            std::thread::spawn(move || {
                let found = crate::view::call("improve_tab", json!({ "key": key }));
                let tab: Option<Tab> = found.ok().and_then(|v| serde_json::from_value(v).ok()).flatten();
                let shot = tab.as_ref().and_then(tab_picture).map(|a| ((i, j + 1), a));
                let _ = app.run_on_main_thread(move || one_in(&capture, shot, true));
            });
        }
        one_in(&capture, None, false);
    })
}

/// Opens the quick panel for improving Codealong, as the button beside New session and the
/// shortcut do: with a picture of every window up now in `context` (`improve.shots`), which the
/// panel puts into the prompt, and the board's say on where it goes (`Board::improve`). On the
/// main thread.
pub fn open(app: &AppHandle, context: Value) {
    let handle = app.clone();
    capture(app, move |shots| {
        let mut fields = match context {
            Value::Object(fields) => fields,
            _ => Map::new(),
        };
        fields.insert("improve".into(), json!({ "shots": shots }));
        quick::open(&handle, Value::Object(fields));
    });
}

/// The shortcut: opens the panel for improving Codealong, or closes the panel when it's up.
pub fn toggle(app: &AppHandle) {
    if quick::is_up(app) {
        quick::hide(app);
    } else {
        open(app, Value::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::lane;
    use codealong_peer::model::DeviceTag;

    #[test]
    fn source_is_known_by_its_identifier() {
        assert!(is_source_conf(r#"{ "productName": "Codealong", "identifier": "app.codealong.desktop", "build": {} }"#));
        assert!(!is_source_conf(r#"{ "productName": "Other", "identifier": "com.example.other" }"#));
        assert!(!is_source_conf("not json"));
        let scratch = crate::fixtures::scratch("improve-source");
        let root: &Path = scratch.as_ref();
        assert!(!is_source(root));
        std::fs::create_dir_all(root.join("src-tauri")).unwrap();
        std::fs::write(root.join("src-tauri/tauri.conf.json"), r#"{"identifier":"app.codealong.desktop"}"#).unwrap();
        assert!(is_source(root));
    }

    #[test]
    fn marks_the_sources_worktrees() {
        // A clone with no remote is a lane through a project's `folders`, with no checkout entry.
        let mut lanes = vec![lane("/p/codealong", vec![]), lane("/p/codealong/.claude/worktrees/x", vec![]), lane("/p/other", vec![]), lane("/p/clone", vec![])];
        let checkout_of: HashMap<PathBuf, PathBuf> = [("/p/codealong", "/p/codealong"), ("/p/codealong/.claude/worktrees/x", "/p/codealong"), ("/p/other", "/p/other")]
            .into_iter()
            .map(|(a, b)| (PathBuf::from(a), PathBuf::from(b)))
            .collect();
        mark(&mut lanes, &checkout_of, &HashSet::from([PathBuf::from("/p/codealong"), PathBuf::from("/p/clone")]));
        assert_eq!(lanes.iter().map(|l| l.codealong).collect::<Vec<_>>(), vec![true, true, false, true]);
    }

    #[test]
    fn target_prefers_the_main_checkout_here() {
        let mut wt = lane("/p/codealong/.claude/worktrees/x", vec![]);
        wt.codealong = true;
        wt.origin = Origin::ClaudeApp;
        wt.project = "Codealong".into();
        let mut main = lane("/p/codealong", vec![]);
        main.codealong = true;
        main.project = "Codealong".into();
        // Another device's lane on a board is never this device's target: the panel looks there itself.
        let mut remote = lane("/elsewhere/codealong", vec![]);
        remote.codealong = true;
        remote.device = Some(DeviceTag { name: "Mini".into(), short: "abc".into(), color: "blue".into(), online: true, last_seen: None });
        let other = lane("/p/other", vec![]);
        // The main checkout wins over a worktree, whichever is listed first.
        let t = target(&[other.clone(), remote.clone(), wt.clone(), main.clone()], &[]);
        assert_eq!(t, Improve { project: Some("Codealong".into()), lane_id: Some("/p/codealong".into()), ..Improve::default() });
        // Without the main checkout, a worktree; without one here, nothing.
        assert_eq!(target(&[wt.clone(), remote.clone()], &[]).lane_id.as_deref(), Some("/p/codealong/.claude/worktrees/x"));
        assert_eq!(target(&[other.clone(), remote.clone()], &[]), Improve::default());
        // A hidden main checkout still wins over nothing, but a shown worktree over it.
        main.hidden = true;
        assert_eq!(target(&[main.clone(), wt.clone()], &[]).lane_id.as_deref(), Some("/p/codealong/.claude/worktrees/x"));
        assert_eq!(target(&[main.clone()], &[]).lane_id.as_deref(), Some("/p/codealong"));
    }

    #[test]
    fn target_offers_a_source_in_no_project() {
        let sources = vec![(PathBuf::from("/p/codealong"), Some("github.com/me/codealong".to_string()))];
        let offer = Improve { source: Some(Source { root: "/p/codealong".into(), repo: Some("github.com/me/codealong".into()) }), ..Improve::default() };
        assert_eq!(target(&[lane("/p/other", vec![])], &sources), offer);
        // A session ran there: its Elsewhere lane is marked, but the source is still offered.
        let mut elsewhere = lane("/p/codealong", vec![]);
        elsewhere.codealong = true;
        elsewhere.project = crate::board::ELSEWHERE.into();
        assert_eq!(target(&[elsewhere], &sources), offer);
        assert_eq!(target(&[], &[]), Improve::default());
    }

    #[test]
    fn the_installed_app_says_so_and_a_build_of_the_source_says_nothing() {
        // The panel suggests Codealong Dev only when the board says `installed` (improve.ts).
        assert_eq!(serde_json::to_value(Improve::default()).unwrap(), json!({}));
        assert_eq!(serde_json::to_value(Improve { installed: true, ..Improve::default() }).unwrap(), json!({ "installed": true }));
    }

    #[test]
    fn a_tabs_window_is_known_by_its_title_whatever_sign_leads_it() {
        // Claude Code's spinner moves between Ghostty's answer and the window server's.
        assert!(same_title("⠐ Billing export", "✳ Billing export"));
        assert!(same_title("northwind · Billing export", "northwind · Billing export"));
        assert!(same_title("…/projects/northwind", "…/projects/northwind"));
        assert!(!same_title("northwind · Billing export", "northwind · Key rotation"));
        assert!(!same_title("atlas · Billing export", "Billing export"));
        // A tab with no name in letters is never one window's.
        assert!(!same_title("✳", "✳"));
        assert!(!same_title("", ""));
    }

    fn shot(name: &str) -> Attachment {
        Attachment { kind: crate::attachments::Kind::Image, path: format!("images/{name}.png"), name: name.into() }
    }

    /// A capture of two windows, and what its `done` was handed.
    fn two_windows() -> (Arc<Mutex<Capture>>, Arc<Mutex<Option<Vec<String>>>>) {
        let got = Arc::new(Mutex::new(None));
        let into = got.clone();
        let capture = Arc::new(Mutex::new(Capture::new(2, move |shots| *into.lock().unwrap() = Some(shots.into_iter().map(|a| a.name).collect()))));
        (capture, got)
    }

    fn says(capture: &Arc<Mutex<Capture>>, keys: &[&str]) -> Vec<String> {
        let fresh = capture.lock().unwrap().shown(keys.iter().map(|k| k.to_string()).collect());
        one_in(capture, None, false);
        fresh
    }

    #[test]
    fn the_tabs_of_the_sessions_shown_come_after_their_window_each_once() {
        let (capture, got) = two_windows();
        assert_eq!(says(&capture, &["s1"]), vec!["s1"]);
        one_in(&capture, Some(((1, 0), shot("Peek"))), false);
        // A session the main window shows already is asked for once.
        assert_eq!(says(&capture, &["s1", "s2"]), vec!["s2"]);
        one_in(&capture, Some(((0, 0), shot("Main window"))), false);
        // Every window is in, but the tabs hold the panel, past the windows' wait too.
        one_in(&capture, Some(((1, 1), shot("Ghostty tab of s2"))), true);
        capture.lock().unwrap().windows_over = true;
        settle(&capture, false);
        assert_eq!(*got.lock().unwrap(), None);
        one_in(&capture, Some(((0, 1), shot("Ghostty tab of s1"))), true);
        assert_eq!(*got.lock().unwrap(), Some(vec!["Main window".to_string(), "Ghostty tab of s1".into(), "Peek".into(), "Ghostty tab of s2".into()]));
    }

    #[test]
    fn the_panel_opens_without_what_didnt_come_in_time() {
        // A window that never drew, and nothing in a terminal: the windows' wait opens it.
        let (capture, got) = two_windows();
        assert!(says(&capture, &[]).is_empty());
        assert!(says(&capture, &[]).is_empty());
        one_in(&capture, Some(((0, 0), shot("Main window"))), false);
        settle(&capture, false);
        assert_eq!(*got.lock().unwrap(), None);
        capture.lock().unwrap().windows_over = true;
        settle(&capture, false);
        assert_eq!(*got.lock().unwrap(), Some(vec!["Main window".to_string()]));
        // A tab its terminal never finds: the tabs' wait opens it.
        let (capture, got) = two_windows();
        says(&capture, &["s1"]);
        says(&capture, &[]);
        one_in(&capture, Some(((0, 0), shot("Main window"))), false);
        one_in(&capture, Some(((1, 0), shot("Peek"))), false);
        capture.lock().unwrap().windows_over = true;
        settle(&capture, false);
        assert_eq!(*got.lock().unwrap(), None);
        settle(&capture, true);
        assert_eq!(*got.lock().unwrap(), Some(vec!["Main window".to_string(), "Peek".into()]));
        // It opens once: what comes after is let go.
        one_in(&capture, Some(((0, 1), shot("Ghostty tab of s1"))), true);
        assert_eq!(got.lock().unwrap().as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn windows_are_named_for_the_prompt() {
        assert_eq!(window_name("main", false).as_deref(), Some("Main window"));
        assert_eq!(window_name("project-acme-37036cd8", false).as_deref(), Some("Project window"));
        assert_eq!(window_name("bar", false).as_deref(), Some("Floating bar"));
        assert_eq!(window_name("bar", true).as_deref(), Some("Bar in the notch"));
        assert_eq!(window_name("quick", false), None);
        assert_eq!(window_name("kept-3", false).as_deref(), Some("Kept session window"));
        assert_eq!(window_name("desktop-2", false).as_deref(), Some("Card pinned to the desktop"));
        assert_eq!(window_name("something", false).as_deref(), Some("something window"));
        let names = window_names(["main", "kept-1", "quick", "kept-2", "bar"], false);
        assert_eq!(
            names,
            vec![("main", "Main window".to_string()), ("kept-1", "Kept session window 1".to_string()), ("kept-2", "Kept session window 2".to_string()), ("bar", "Floating bar".to_string())]
        );
    }
}

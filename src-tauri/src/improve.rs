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
//! reads them as it reads any image.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use codealong_peer::model::{Lane, Origin};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::attachments::Attachment;
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

struct Capture {
    /// Windows whose picture hasn't come.
    left: usize,
    /// Each picture, with its window's place in the order asked, which is how they're listed.
    shots: Vec<(usize, Attachment)>,
    /// What runs once with the pictures; None once it has.
    done: Option<Box<dyn FnOnce(Vec<Attachment>) + Send>>,
}

/// Runs `done` once: when every window has answered, or when `force` says the wait is over.
fn settle(capture: &Arc<Mutex<Capture>>, force: bool) {
    let Ok(mut c) = capture.lock() else { return };
    if c.left > 0 && !force {
        return;
    }
    let Some(done) = c.done.take() else { return };
    let mut shots = std::mem::take(&mut c.shots);
    drop(c);
    shots.sort_by_key(|(i, _)| *i);
    done(shots.into_iter().map(|(_, a)| a).collect());
}

/// Keeps a picture of every Codealong window that's up, the quick panel aside, and calls `done`
/// with them, main window first, each named for its window. On the main thread; `done` runs on
/// it too, after this returns.
pub fn capture(app: &AppHandle, done: impl FnOnce(Vec<Attachment>) + Send + 'static) {
    let notch = bar::mode_of(app) == bar::Mode::Notch;
    let windows: HashMap<String, WebviewWindow> = app.webview_windows();
    let mut labels: Vec<&str> = windows.keys().map(String::as_str).filter(|l| windows.get(*l).is_some_and(panel::is_visible)).collect();
    // The main window first, then the rest in a steady order.
    labels.sort_by_key(|l| (*l != "main", l.to_string()));
    let named = window_names(labels, notch);
    let capture = Arc::new(Mutex::new(Capture { left: named.len(), shots: Vec::new(), done: Some(Box::new(done)) }));
    if named.is_empty() {
        settle(&capture, true);
        return;
    }
    for (i, (label, name)) in named.into_iter().enumerate() {
        let Some(window) = windows.get(label) else { continue };
        if !ask_picture(window, i, name, capture.clone()) {
            one_in(&capture, None);
        }
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(CAPTURE_WAIT);
        let _ = handle.run_on_main_thread(move || settle(&capture, true));
    });
}

/// One window's picture came, or didn't.
fn one_in(capture: &Arc<Mutex<Capture>>, shot: Option<(usize, Attachment)>) {
    if let Ok(mut c) = capture.lock() {
        c.left = c.left.saturating_sub(1);
        if let Some(s) = shot {
            c.shots.push(s);
        }
    }
    settle(capture, false);
}

/// Asks a window's page for its picture (os::windows), kept as `<name>.png` and reported; false
/// when it couldn't be asked, as on Linux.
fn ask_picture(window: &WebviewWindow, i: usize, name: String, capture: Arc<Mutex<Capture>>) -> bool {
    crate::os::windows::picture(window, move |png| {
        let shot = png.and_then(|png| crate::attachments::keep(&png, &format!("{name}.png")).ok()).map(|a| (i, Attachment { name: name.clone(), ..a }));
        one_in(&capture, shot);
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

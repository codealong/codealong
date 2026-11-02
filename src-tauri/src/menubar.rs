//! The bar in the menu bar (bar.rs `Mode::MenuBar`): an item there in place of the bar's window,
//! picked as the bar's third place beside floating and the notch. Its mark is dimmed while the
//! app isn't connected to Codealong's server (view.rs), with the number of sessions that need you
//! beside it. Its menu lists what the bar shows: the sessions that need you, the PRs waiting on
//! your review, the sessions whose turn ended and the ones working, each shown in the waiting
//! window (waiting.rs) to read and answer. Below them it says how the server is, starts it again
//! (serve.rs `restart`), and quits: the app alone, which leaves the server running with the
//! sessions it runs, or the app and its server, as `codealong server stop` does.
//!
//! The item is made the first time the bar goes to the menu bar, and is there only while the bar
//! is. Tauri's tray calls wait for the main thread when they're made from another, so what it
//! shows is cut where the bar's feed is (`publish`) and put on the item on the main thread
//! (`apply`), its menu made again only when what it lists changes.
//!
//! On a Mac its mark is a template, which macOS draws in the menu bar's own colors. On Linux it's
//! a StatusNotifierItem (libayatana-appindicator) with the app's icon, which opens its menu
//! however it's clicked. A desktop that shows no tray items (GNOME without its AppIndicator
//! extension) would show neither it nor the bar, so the bar floats there for now instead
//! (`os::windows::tray_shown`, bar.rs `to_menu_bar`), as it does where the tray's library is
//! missing.
//!
//! A server the app runs in itself, its starts as a job of its own having failed, goes when the
//! app does, so the menu says so and offers no Restart server there.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::barfeed::{BarFeed, Group};
use crate::model::Status;

const ID: &str = "menubar";
/// A session's or PR's item: its key follows.
const SHOW: &str = "menubar:show:";
const MORE: &str = "menubar:more";
const MAIN: &str = "menubar:main";
const WAITING: &str = "menubar:waiting";
const RESTART: &str = "menubar:restart";
const SETTINGS: &str = "menubar:settings";
const QUIT: &str = "menubar:quit";
const QUIT_SERVER: &str = "menubar:quit-server";

/// How many of each group the menu lists; the rest are in the main window.
const EACH: usize = 5;
/// How much of a title the menu shows.
const TITLE_CHARS: usize = 60;
/// How much of the mark shows while the app isn't connected to its server.
const DIMMED: f32 = 0.35;
/// How long Quit and stop server waits for the server to quit the app before it quits by itself.
const QUIT_WAIT: Duration = Duration::from_secs(15);

/// What the menu lists of the bar's feed: each group in the bar's order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Listed {
    needs: Vec<Row>,
    reviews: Vec<Row>,
    unread: Vec<Row>,
    working: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq)]
struct Row {
    key: String,
    label: String,
}

/// One line of the item's menu.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// What it says, and what picking it does (its id), when it can be picked.
    Item { id: Option<String>, label: String },
    Separator,
}

/// How the app's server is, as the menu says it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Server {
    /// The app isn't connected to one: it's starting, or being started again.
    Starting,
    /// A job of its own, which Restart server starts again.
    Running,
    /// In the app, its starts as a job of its own having failed (view.rs `server_in_app`): it
    /// goes when the app does, and isn't restarted without it.
    InApp,
}

/// The item itself: the number beside its mark, its tooltip, and whether the mark is dimmed.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub title: Option<String>,
    pub tooltip: String,
    pub dimmed: bool,
}

/// The sessions and reviews the menu lists, from the bar's feed. Those in worktrees the board
/// keeps out of sight are left out, as the bar leaves them out until Elsewhere is picked.
pub fn listed(feed: &BarFeed) -> Listed {
    let rows = |group: Group| -> Vec<Row> {
        feed.items
            .iter()
            .filter(|i| i.group == group && !i.hidden)
            .map(|i| Row { key: i.key.clone(), label: label(&i.title, &i.project, i.device.as_ref().map(|d| d.name.as_str()), i.status == Status::Error) })
            .collect()
    };
    Listed {
        needs: rows(Group::Needs),
        reviews: feed.reviews.iter().map(|r| Row { key: r.key.clone(), label: label(&format!("#{} {}", r.number, r.title), &r.repo, None, false) }).collect(),
        unread: rows(Group::Unread),
        working: rows(Group::Working),
    }
}

/// A session's or PR's line: its title, then where it is.
fn label(title: &str, project: &str, device: Option<&str>, failed: bool) -> String {
    let title = match crate::util::truncate(title.trim(), TITLE_CHARS) {
        t if t.is_empty() => "Untitled".to_string(),
        t => t,
    };
    let wheres: Vec<&str> = [Some(project).filter(|p| !p.is_empty()), device].into_iter().flatten().collect();
    let said = if wheres.is_empty() { title } else { format!("{title} · {}", wheres.join(", ")) };
    if failed {
        format!("{said} (failed)")
    } else {
        said
    }
}

fn item(id: &str, label: impl Into<String>) -> Line {
    Line::Item { id: Some(id.to_string()), label: label.into() }
}

fn note(label: impl Into<String>) -> Line {
    Line::Item { id: None, label: label.into() }
}

/// The item's menu: what the bar lists, each group under its heading, then the windows, the
/// server and quitting. `hosted` is how many sessions the server runs.
pub fn lines(listed: &Listed, hosted: usize, server: Server) -> Vec<Line> {
    let mut out = Vec::new();
    let groups = [("Needs you", &listed.needs), ("Review requested", &listed.reviews), ("Your turn", &listed.unread), ("Working", &listed.working)];
    for (heading, rows) in groups.into_iter().filter(|(_, rows)| !rows.is_empty()) {
        if !out.is_empty() {
            out.push(Line::Separator);
        }
        out.push(note(heading));
        out.extend(rows.iter().take(EACH).map(|r| item(&format!("{SHOW}{}", r.key), &r.label)));
        if rows.len() > EACH {
            out.push(item(MORE, format!("{} more in Codealong…", rows.len() - EACH)));
        }
    }
    if out.is_empty() {
        out.push(note("Nothing waits on you or is working"));
    }
    let waits = !(listed.needs.is_empty() && listed.reviews.is_empty() && listed.unread.is_empty());
    out.push(Line::Separator);
    out.push(item(MAIN, "Open Codealong"));
    out.push(Line::Item { id: waits.then(|| WAITING.to_string()), label: "Waiting on you…".into() });
    out.push(Line::Separator);
    out.push(note(match (server, hosted) {
        (Server::Starting, _) => "Server starting…".to_string(),
        (Server::InApp, _) => "Server running in the app".to_string(),
        (Server::Running, 0) => "Server running".to_string(),
        (Server::Running, 1) => "Server running 1 session".to_string(),
        (Server::Running, n) => format!("Server running {n} sessions"),
    }));
    out.push(Line::Item { id: (server == Server::Running).then(|| RESTART.to_string()), label: "Restart server".into() });
    out.push(Line::Separator);
    out.push(item(SETTINGS, "Settings…"));
    out.push(item(QUIT, "Quit Codealong"));
    out.push(item(QUIT_SERVER, "Quit Codealong and stop its server"));
    out
}

/// The item's number (the sessions that need you), its tooltip and whether its mark is dimmed.
/// `name` is the app's, which tells Codealong Dev apart.
pub fn face(listed: &Listed, connected: bool, name: &str) -> Face {
    let needs = listed.needs.len();
    let counts = [
        (needs, "need you"),
        (listed.reviews.len(), if listed.reviews.len() == 1 { "PR to review" } else { "PRs to review" }),
        (listed.unread.len(), "done"),
        (listed.working.len(), "working"),
    ];
    let said: Vec<String> = counts.iter().filter(|(n, _)| *n > 0).map(|(n, what)| format!("{n} {what}")).collect();
    let tooltip = match (connected, said.is_empty()) {
        (false, _) => format!("{name}: starting its server"),
        (true, true) => format!("{name}: nothing waits on you"),
        (true, false) => format!("{name}: {}", said.join(", ")),
    };
    Face { title: (needs > 0).then(|| needs.to_string()), tooltip, dimmed: !connected }
}

struct MenuBar {
    /// Made the first time the bar goes to the menu bar.
    tray: Mutex<Option<TrayIcon<Wry>>>,
    /// Whether it's in the menu bar: only while the bar is.
    up: AtomicBool,
    /// What it lists, as last cut from the bar's feed, and how many sessions the server runs.
    latest: Mutex<(Listed, usize)>,
    /// What's on the item now, which is put on it again only when it changes.
    put: Mutex<(Vec<Line>, Option<Face>)>,
}

/// Gets ready to show the bar in the menu bar, and hears what's picked in its menu.
pub fn setup(app: &AppHandle) {
    app.manage(MenuBar { tray: Mutex::new(None), up: AtomicBool::new(false), latest: Default::default(), put: Default::default() });
    app.on_menu_event(|app, event| picked(app, event.id().as_ref()));
}

/// Puts the item in the menu bar or takes it out, as the bar goes there or leaves. Whether it's
/// there. Whether the desktop shows tray items at all is asked before, off the main thread
/// (bar.rs `to_menu_bar`). On the main thread.
pub fn show(app: &AppHandle, on: bool) -> bool {
    let Some(state) = app.try_state::<MenuBar>() else { return false };
    let Ok(mut tray) = state.tray.lock() else { return false };
    if !on {
        state.up.store(false, Ordering::Relaxed);
        if let Some(t) = tray.as_ref() {
            let _ = t.set_visible(false);
        }
        return false;
    }
    if tray.is_none() {
        match make(app) {
            Ok(t) => *tray = Some(t),
            Err(e) => {
                eprintln!("Couldn't put Codealong in the menu bar: {e:#}");
                return false;
            }
        }
    }
    if let Some(t) = tray.as_ref() {
        let _ = t.set_visible(true);
    }
    state.up.store(true, Ordering::Relaxed);
    drop(tray);
    apply(app);
    true
}

fn make(app: &AppHandle) -> Result<TrayIcon<Wry>> {
    let build = || TrayIconBuilder::with_id(ID).icon(mark(app, false)?).icon_as_template(crate::os::is_macos()).show_menu_on_left_click(true).tooltip(app.package_info().name.as_str()).build(app).map_err(anyhow::Error::from);
    // Linux's tray library is loaded as the first item is made, and a desktop without it panics.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(build)).unwrap_or_else(|_| Err(anyhow!("this desktop has no libayatana-appindicator3 for a tray")))
}

/// The mark the item shows: the template on a Mac, the app's icon on Linux, whose trays draw
/// what they're given; dimmed, mostly clear.
fn mark(app: &AppHandle, dimmed: bool) -> Result<Image<'static>> {
    let png: &[u8] = match (crate::os::is_macos(), crate::is_dev_build(app)) {
        (true, _) => include_bytes!("../icons/menubar.png"),
        (false, false) => include_bytes!("../icons/32x32.png"),
        (false, true) => include_bytes!("../icons/dev/32x32.png"),
    };
    let (w, h, mut rgba) = crate::emblems::decode(png)?;
    if dimmed {
        for p in rgba.chunks_exact_mut(4) {
            p[3] = (p[3] as f32 * DIMMED).round() as u8;
        }
    }
    Ok(Image::new_owned(rgba, w as u32, h as u32))
}

/// The bar's feed changed (lib.rs `Floating`): the item shows it, with `hosted`, the sessions the
/// server runs.
pub fn publish(app: &AppHandle, feed: &BarFeed, hosted: usize) {
    let Some(state) = app.try_state::<MenuBar>() else { return };
    let now = (listed(feed), hosted);
    {
        let Ok(mut latest) = state.latest.lock() else { return };
        if *latest == now {
            return;
        }
        *latest = now;
    }
    refresh(app);
}

/// The app connected to its server or lost it (view.rs `Hooks::connected`).
pub fn connected(app: &AppHandle) {
    refresh(app);
}

/// Puts what the item shows on it, on the main thread, while it's up.
fn refresh(app: &AppHandle) {
    if app.try_state::<MenuBar>().is_some_and(|s| s.up.load(Ordering::Relaxed)) {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || apply(&handle));
    }
}

/// Puts the menu and face on the item where they changed. On the main thread.
fn apply(app: &AppHandle) {
    let Some(state) = app.try_state::<MenuBar>() else { return };
    if !state.up.load(Ordering::Relaxed) {
        return;
    }
    let Some(tray) = state.tray.lock().ok().and_then(|t| t.clone()) else { return };
    let Some((listed, hosted)) = state.latest.lock().ok().map(|l| l.clone()) else { return };
    let connected = crate::view::connected();
    let server = match (connected, crate::view::server_in_app()) {
        (false, _) => Server::Starting,
        (true, true) => Server::InApp,
        (true, false) => Server::Running,
    };
    let (lines, face) = (lines(&listed, hosted, server), face(&listed, connected, &app.package_info().name));
    let Ok(mut put) = state.put.lock() else { return };
    if put.0 != lines {
        match menu(app, &lines) {
            Ok(m) => {
                let _ = tray.set_menu(Some(m));
                put.0 = lines;
            }
            Err(e) => eprintln!("Couldn't make the menu bar item's menu: {e:#}"),
        }
    }
    if put.1.as_ref() != Some(&face) {
        let _ = tray.set_title(face.title.as_deref());
        let _ = tray.set_tooltip(Some(&face.tooltip));
        if put.1.as_ref().map(|f| f.dimmed) != Some(face.dimmed) {
            match mark(app, face.dimmed) {
                Ok(m) => {
                    let _ = tray.set_icon_with_as_template(Some(m), crate::os::is_macos());
                }
                Err(e) => eprintln!("Couldn't draw the menu bar item's mark: {e:#}"),
            }
        }
        put.1 = Some(face);
    }
}

fn menu(app: &AppHandle, lines: &[Line]) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    for line in lines {
        match line {
            Line::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
            Line::Item { id: Some(id), label } => menu.append(&MenuItem::with_id(app, id.as_str(), label, true, None::<&str>)?)?,
            Line::Item { id: None, label } => menu.append(&MenuItem::new(app, label, false, None::<&str>)?)?,
        }
    }
    Ok(menu)
}

/// Something was picked in the item's menu (or any other: those aren't its). On the main thread.
fn picked(app: &AppHandle, id: &str) {
    if let Some(key) = id.strip_prefix(SHOW) {
        crate::waiting::open(app, json!({ "key": key }));
        return;
    }
    match id {
        MAIN | MORE => {
            if let Err(e) = crate::front::show_main(app, None) {
                eprintln!("Couldn't show the main window: {e:#}");
            }
        }
        WAITING => crate::waiting::open(app, Value::Null),
        SETTINGS => {
            if crate::front::show_main(app, None).is_ok() {
                let _ = app.emit_to("main", "settings", ());
            }
        }
        RESTART => {
            std::thread::spawn(|| {
                if !crate::serve::restart() {
                    eprintln!("Codealong's server wasn't running to restart; the app starts it.");
                }
            });
        }
        QUIT => app.exit(0),
        QUIT_SERVER => quit_with_server(),
        _ => {}
    }
}

/// Quits the app and stops its server, as `codealong server stop` does, which it runs: the server
/// tells the app to quit as it goes, and the app starts no other once it's gone (`view::leave`).
/// One that doesn't hear from it quits by itself.
fn quit_with_server() {
    crate::view::leave();
    let stopping = crate::serve::this_binary()
        .ok_or_else(|| std::io::Error::other("Codealong couldn't find its own program"))
        .and_then(|exe| Command::new(exe).args(["server", "stop"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn());
    if let Err(e) = stopping {
        eprintln!("Couldn't run codealong server stop ({e}); stopping the server from the app.");
        std::thread::spawn(|| {
            crate::serve::stop();
            std::process::exit(0);
        });
    }
    std::thread::spawn(|| {
        std::thread::sleep(QUIT_WAIT);
        eprintln!("Codealong's server didn't quit the app as it stopped: quitting.");
        std::process::exit(0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: &str, label: &str) -> Row {
        Row { key: key.into(), label: label.into() }
    }

    fn ids(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| match l {
                Line::Separator => "--".to_string(),
                Line::Item { id: Some(id), label } => format!("{label} [{}]", id.trim_start_matches("menubar:")),
                Line::Item { id: None, label } => label.clone(),
            })
            .collect()
    }

    #[test]
    fn a_line_says_the_title_then_where_it_is() {
        assert_eq!(label("  Fix the login redirect ", "northwind", None, false), "Fix the login redirect · northwind");
        assert_eq!(label("Fix it", "northwind", Some("Studio Mac"), true), "Fix it · northwind, Studio Mac (failed)");
        assert_eq!(label("", "", None, false), "Untitled");
        let long = "x".repeat(80);
        assert_eq!(label(&long, "", None, false).chars().count(), TITLE_CHARS);
    }

    #[test]
    fn the_menu_lists_each_group_under_its_heading_then_the_app_and_its_server() {
        let listed = Listed {
            needs: vec![row("claude:a", "Answer me · northwind")],
            reviews: vec![row("pr:atlas/web#12", "#12 Tidy the header · web")],
            unread: vec![],
            working: (0..7).map(|n| row(&format!("codex:{n}"), &format!("Working {n} · kestrel"))).collect(),
        };
        let lines = lines(&listed, 2, Server::Running);
        assert_eq!(
            ids(&lines),
            [
                "Needs you",
                "Answer me · northwind [show:claude:a]",
                "--",
                "Review requested",
                "#12 Tidy the header · web [show:pr:atlas/web#12]",
                "--",
                "Working",
                "Working 0 · kestrel [show:codex:0]",
                "Working 1 · kestrel [show:codex:1]",
                "Working 2 · kestrel [show:codex:2]",
                "Working 3 · kestrel [show:codex:3]",
                "Working 4 · kestrel [show:codex:4]",
                "2 more in Codealong… [more]",
                "--",
                "Open Codealong [main]",
                "Waiting on you… [waiting]",
                "--",
                "Server running 2 sessions",
                "Restart server [restart]",
                "--",
                "Settings… [settings]",
                "Quit Codealong [quit]",
                "Quit Codealong and stop its server [quit-server]",
            ]
            .map(String::from)
        );
    }

    #[test]
    fn with_nothing_listed_or_no_server_it_says_so() {
        let lines = lines(&Listed::default(), 0, Server::Starting);
        let said = ids(&lines);
        assert_eq!(said[0], "Nothing waits on you or is working");
        // Nothing waits, so there's nothing to go through; with no server yet, nothing to restart.
        assert!(said.contains(&"Waiting on you…".to_string()));
        assert!(said.contains(&"Server starting…".to_string()));
        assert!(said.contains(&"Restart server".to_string()));
        assert_eq!(ids(&super::lines(&Listed::default(), 1, Server::Running))[5], "Server running 1 session");
        // A server the app runs in itself goes with it: there's no restarting it from here.
        let in_app = ids(&super::lines(&Listed::default(), 1, Server::InApp));
        assert_eq!(in_app[5..7], ["Server running in the app", "Restart server"]);
    }

    #[test]
    fn the_item_counts_what_needs_you_and_dims_without_its_server() {
        let listed = Listed {
            needs: vec![row("a", "A"), row("b", "B")],
            reviews: vec![row("pr", "#1 P")],
            unread: vec![row("c", "C")],
            working: vec![],
        };
        assert_eq!(face(&listed, true, "Codealong"), Face { title: Some("2".into()), tooltip: "Codealong: 2 need you, 1 PR to review, 1 done".into(), dimmed: false });
        assert_eq!(face(&Listed::default(), true, "Codealong Dev"), Face { title: None, tooltip: "Codealong Dev: nothing waits on you".into(), dimmed: false });
        assert_eq!(face(&listed, false, "Codealong"), Face { title: Some("2".into()), tooltip: "Codealong: starting its server".into(), dimmed: true });
    }

    #[test]
    fn its_marks_read() {
        for png in [&include_bytes!("../icons/menubar.png")[..], include_bytes!("../icons/32x32.png"), include_bytes!("../icons/dev/32x32.png")] {
            let (w, h, rgba) = crate::emblems::decode(png).unwrap();
            assert!(w > 0 && h > 0 && rgba.len() == w * h * 4);
        }
    }
}

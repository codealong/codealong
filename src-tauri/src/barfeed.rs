//! What the floating bar shows, cut from the board: the sessions that need you, whose turn ended,
//! or that are running, in bar order, the rest of the sessions started in the last day, the ones
//! you snoozed, the PRs waiting on your review, and the todos ready to start. Each names its project, and the bar narrows them to the one picked in its project menu.
//! The bar gets this instead of the board, and only when it changes: the board is about half a
//! megabyte and is published many times a minute, while this is a few kilobytes and changes far
//! less often.
//!
//! Your other devices' sessions are among them, each with its device, while `[devices] in_bar`
//! is on (it is unless you turn it off): what waits on you there waits on you wherever you sit.
//! Only the devices connected now: one that's away shows in the main window as it was when last
//! seen, but nothing of it can be answered from here, and what it shows isn't news. Their
//! worktrees are beside the board (shown.rs), and the feed is cut again whenever either changes
//! (lib.rs `Floating`).

use serde::Serialize;

use crate::config::Theme;
use crate::model::{Board, DeviceTag, LogoStatus, PrItem, Status};
use crate::shown::Shown;
use crate::todos;

/// The bar's groups, most urgent first.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    Needs,
    Unread,
    Working,
    /// Started in the last day and in none of the others: listed apart, in `recent`.
    Recent,
    /// Snoozed and not running: listed apart, in `snoozed`, until the snooze's time.
    Snoozed,
}

/// How far back the bar's recent sessions go, and how many it lists at most.
const RECENT_MS: i64 = 24 * 60 * 60 * 1000;
const RECENT_MAX: usize = 20;

pub fn group_of(status: Status) -> Option<Group> {
    match status {
        Status::Needs | Status::Error => Some(Group::Needs),
        Status::Unread => Some(Group::Unread),
        Status::Working => Some(Group::Working),
        _ => None,
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BarFeed {
    /// True until the engine's first full scan has finished.
    pub loading: bool,
    /// The board's time: a change the bar shows ahead of the engine goes once a feed this new
    /// arrives (src/pending.ts).
    pub generated_at: i64,
    pub theme: Theme,
    /// The zoom, in percent: the session or PR the notch shows draws at it (zoom.rs).
    pub zoom: u32,
    /// What the mark's colors say, as on the board, with your other devices' sessions it lists.
    pub logo_status: LogoStatus,
    pub perf_log: bool,
    /// The terminal app's name, for menu items such as "Show in Ghostty".
    pub terminal: String,
    /// The projects, in your order, for the bar's project menu.
    pub projects: Vec<String>,
    /// In bar order: by group, then each the longest first. The notch, the quick panel and the
    /// waiting window work through what waits on you in this order (src/queue.ts `byPlace`).
    pub items: Vec<BarItem>,
    /// The other sessions started in the last day (by their last activity when the start isn't
    /// known), the newest status first, as the quick panel lists them.
    pub recent: Vec<BarItem>,
    /// Sessions you snoozed, out of Needs you and Your turn until their time comes (running ones
    /// stay in Working), the soonest back first.
    pub snoozed: Vec<BarItem>,
    /// Open PRs that ask for your review, the longest wait first, as Needs you is: the notch
    /// and the quick panel work through them in this order.
    pub reviews: Vec<BarReview>,
    /// Todos ready to start (not started, and waiting on nothing), in your order.
    pub todos: Vec<BarTodo>,
    /// The other devices whose sessions it lists, by short id: those connected now. One that
    /// wasn't on the feed before has just come back, and what it brings isn't news to sound or
    /// knock for (sounds.rs, src/mark.ts).
    pub devices: Vec<String>,
}

/// One session on the bar, with what its pill, tooltip and menu need.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BarItem {
    pub key: String,
    pub group: Group,
    pub status: Status,
    /// Working only in that it waits on a command left running in the background
    /// (`Session::waits_in_background`): its pill draws a clock, not a spinner.
    pub waiting: bool,
    /// Needs you only because its last reply asks you something: marking it seen settles it.
    pub asks: bool,
    /// Its last turn was cut off (`Session::cut_off`): marking it seen settles it too.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cut_off: Option<i64>,
    pub title: String,
    pub account: String,
    pub lane_id: String,
    pub callsign: String,
    pub project: String,
    /// In a worktree the board keeps out of sight (Elsewhere's, while it holds no session you
    /// haven't archived): the bar shows it only with Elsewhere picked, as the quick panel does.
    pub hidden: bool,
    pub detail: Option<String>,
    /// When its status began (its last activity when that isn't known), for its age.
    pub since: i64,
    pub open_target: String,
    /// The lane's own entry for `open_target`, when it lists one: lanes list their first few
    /// sessions' targets only.
    pub target: Option<BarTarget>,
    /// The terminal app it runs in (`Session::terminal`), for its words where `target` is None.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<String>,
    /// When it's back, while it's snoozed.
    pub snoozed_until: Option<i64>,
    /// Quieted: its turns don't wait on you (`board::apply_quiet`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub quiet: bool,
    /// Moved to the top of what waits on you (`Session::raised`): first in its group, and in the
    /// notch's queue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raised: Option<crate::model::Raised>,
    /// Its turn is running and Codealong can stop it: its menu offers Stop.
    pub stoppable: bool,
    /// Its emblem (`Session::emblem`), drawn on its pill beside its title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emblem: Option<crate::model::Emblem>,
    /// The device it's on, when that's another of yours: its key starts `@<short>:`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceTag>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BarTarget {
    pub label: String,
    pub detail: Option<String>,
    pub disabled: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BarReview {
    pub key: String,
    pub number: u64,
    pub title: String,
    pub repo: String,
    /// `owner/name`, for asking GitHub about it.
    pub repo_slug: String,
    pub url: String,
    pub project: String,
    /// Who asked for your review, when GitHub says.
    pub requested_by: Option<String>,
    /// Since when it waits on you: the latest request to you, else the PR's last update. The
    /// queue in the notch and the quick panel keeps one you dealt with out until this moves on.
    pub since: i64,
    /// The PR's last update, which a card showing it refetches on.
    pub updated_at: i64,
    /// Codealong's agent approves the commit it's at (autoreview.rs `approved`): what's asked of
    /// you is a sign-off.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub approves: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BarTodo {
    pub id: String,
    pub title: String,
    /// Empty until you pick one or link a worktree.
    pub project: String,
    /// The worktree a session for it starts in, as the Todo view picks it: the first one linked
    /// to it, else its latest session's.
    pub callsign: Option<String>,
    /// The day it's due, as `2026-10-03`.
    pub due: Option<String>,
}

/// The bar's feed, from the board and the worktrees of your other devices connected now. Status
/// times don't move while a status lasts, so ordering by them keeps pills in place while agents
/// work. Visitors, archived sessions and helpers that don't need you are left out, as on the board.
pub fn feed(shown: Shown) -> BarFeed {
    let board = shown.board;
    let there: Vec<_> = if board.devices.enabled && board.devices.in_bar { shown.connected().collect() } else { Vec::new() };
    let mut items: Vec<(i64, BarItem)> = Vec::new();
    let mut recent: Vec<(i64, BarItem)> = Vec::new();
    let mut snoozed: Vec<(i64, BarItem)> = Vec::new();
    for lane in board.lanes.iter().chain(there.iter().copied()) {
        for s in &lane.sessions {
            // A helper or a headless run is on the bar only while it needs you
            // (`Session::unattended`): its agent or script waits on it, not you.
            if s.visitor.is_some() || s.archived || s.unattended() {
                continue;
            }
            let group = match group_of(s.status) {
                Some(Group::Working) => Group::Working,
                _ if s.snoozed_until.is_some() => Group::Snoozed,
                Some(g) => g,
                None if board.generated_at - s.started_at.unwrap_or(s.last_activity) <= RECENT_MS => Group::Recent,
                None => continue,
            };
            let target = lane.targets.iter().find(|t| t.id == s.open_target).map(|t| BarTarget {
                label: t.label.clone(),
                detail: t.detail.clone(),
                disabled: t.disabled,
            });
            let at = s.status_since.unwrap_or(s.last_activity);
            let item = BarItem {
                key: s.key.clone(),
                group,
                status: s.status,
                waiting: s.waits_in_background(),
                asks: s.asks,
                cut_off: s.cut_off,
                title: s.title.clone(),
                account: s.account.clone(),
                lane_id: lane.id.clone(),
                callsign: lane.callsign.clone(),
                project: lane.project.clone(),
                hidden: lane.hidden,
                detail: s.detail.clone(),
                since: at,
                open_target: s.open_target.clone(),
                target,
                terminal: s.terminal.clone(),
                snoozed_until: s.snoozed_until,
                quiet: s.quiet,
                raised: s.raised.clone(),
                stoppable: s.stoppable,
                emblem: s.emblem.clone(),
                device: lane.device.clone(),
            };
            match group {
                Group::Recent => recent.push((at, item)),
                Group::Snoozed => snoozed.push((s.snoozed_until.unwrap_or(at), item)),
                _ => items.push((at, item)),
            }
        }
    }
    // Each group is a queue, the longest first: a new one joins the end, where the group folds,
    // and can't push an old one down or out of sight. A turn that ended again after you answered
    // it goes behind the ones that waited longer, so working through them is first come, first
    // served. Only one moved to the top goes ahead (`board::raised_order`).
    items.sort_by(|(a_at, a), (b_at, b)| a.group.cmp(&b.group).then(crate::board::raised_order(a.raised.as_ref(), b.raised.as_ref())).then(a_at.cmp(b_at)).then_with(|| a.key.cmp(&b.key)));
    recent.sort_by(|(a_at, a), (b_at, b)| b_at.cmp(a_at).then_with(|| a.key.cmp(&b.key)));
    recent.truncate(RECENT_MAX);
    snoozed.sort_by(|(a_at, a), (b_at, b)| a_at.cmp(b_at).then_with(|| a.key.cmp(&b.key)));

    let mut prs: Vec<_> = board.prs.iter().filter(|p| p.waits_on_review()).collect();
    let since = |p: &PrItem| p.requested_at.unwrap_or(p.updated_at);
    prs.sort_by(|a, b| since(a).cmp(&since(b)).then_with(|| a.key.cmp(&b.key)));
    let reviews = prs
        .into_iter()
        .map(|p| BarReview {
            key: p.key.clone(),
            number: p.number,
            title: p.title.clone(),
            repo: p.repo.clone(),
            repo_slug: p.repo_slug.clone(),
            url: p.url.clone(),
            project: p.project.clone(),
            requested_by: p.requested_by.clone(),
            since: p.requested_at.unwrap_or(p.updated_at),
            updated_at: p.updated_at,
            approves: p.auto_review.as_ref().and_then(|a| a.approves.as_deref()).is_some_and(|a| p.head_oid.as_deref() == Some(a)),
        })
        .collect();

    let ready = board.todos.iter().filter(|t| todos::is_ready(t, &board.todos)).map(|t| bar_todo(board, t)).collect();
    let mut devices: Vec<String> = there.iter().filter_map(|l| l.device.as_ref().map(|d| d.short.clone())).collect();
    devices.sort();
    devices.dedup();

    BarFeed {
        loading: board.loading,
        generated_at: board.generated_at,
        theme: board.theme,
        zoom: board.zoom,
        logo_status: board.logo_status.min(crate::board::logo_status(there.iter().copied(), &[])),
        perf_log: board.perf_log,
        terminal: board.apps.terminal.clone(),
        projects: board.projects.iter().map(|p| p.name.clone()).collect(),
        items: items.into_iter().map(|(_, item)| item).collect(),
        recent: recent.into_iter().map(|(_, item)| item).collect(),
        snoozed: snoozed.into_iter().map(|(_, item)| item).collect(),
        reviews,
        todos: ready,
        devices,
    }
}

/// A ready todo on the bar. Where it starts is `todoItem`'s rule in src/todos.ts: the first
/// linked worktree that's on the board, else the worktree of its latest session.
fn bar_todo(board: &Board, t: &todos::Todo) -> BarTodo {
    let linked = t.worktrees.iter().find_map(|w| board.lanes.iter().find(|l| &l.id == w));
    let latest = || {
        board
            .lanes
            .iter()
            .flat_map(|l| l.sessions.iter().filter(|s| s.visitor.is_none() && t.sessions.contains(&s.key)).map(move |s| (s.last_activity, l)))
            .min_by_key(|(at, _)| std::cmp::Reverse(*at))
            .map(|(_, l)| l)
    };
    BarTodo { id: t.id.clone(), title: t.title.clone(), project: t.project.clone(), callsign: linked.or_else(latest).map(|l| l.callsign.clone()), due: t.due.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Lane, PrState, Session};

    const NOW: i64 = 1_790_200_000_000;

    fn session(key: &str, status: Status, since: i64) -> Session {
        Session { status, status_since: Some(since), last_activity: since, last_prompt_at: since, ..crate::fixtures::session(key) }
    }

    fn lane(id: &str, sessions: Vec<Session>, hidden: bool) -> Lane {
        Lane { hidden, ..crate::fixtures::lane(id, sessions) }
    }

    fn board(lanes: Vec<Lane>, prs: Vec<PrItem>) -> Board {
        Board { lanes, prs, ..Board::default() }
    }

    #[test]
    fn lists_what_needs_you_then_finished_turns_then_running_agents_each_the_longest_first() {
        let b = board(
            vec![
                lane("/a", vec![session("w1", Status::Working, NOW - 50), session("i1", Status::Idle, NOW), session("u1", Status::Unread, NOW - 10)], false),
                lane("/b", vec![session("n1", Status::Needs, NOW - 90), session("e1", Status::Error, NOW - 5), session("w2", Status::Working, NOW - 20), session("s1", Status::Stopped, NOW)], false),
                lane("/c", vec![session("u2", Status::Unread, NOW - 40), session("u3", Status::Unread, NOW)], false),
            ],
            vec![],
        );
        let got: Vec<(String, Group)> = feed(Shown::here(&b)).items.into_iter().map(|i| (i.key, i.group)).collect();
        let want = [("n1", Group::Needs), ("e1", Group::Needs), ("u2", Group::Unread), ("u1", Group::Unread), ("u3", Group::Unread), ("w1", Group::Working), ("w2", Group::Working)];
        assert_eq!(got, want.map(|(k, g)| (k.to_string(), g)));
    }

    #[test]
    fn leaves_out_visitors_and_archived_sessions_and_marks_lanes_kept_out_of_sight() {
        let mut visitor = session("v", Status::Working, NOW);
        visitor.visitor = Some("worked here".into());
        let mut archived = session("x", Status::Unread, NOW);
        archived.archived = true;
        let mut elsewhere = lane("/elsewhere", vec![session("h", Status::Working, NOW - 1)], true);
        elsewhere.project = "Elsewhere".into();
        let b = board(vec![lane("/a", vec![visitor, archived], false), elsewhere, lane("/b", vec![session("ok", Status::Working, NOW)], false)], vec![]);
        let got: Vec<(String, String, bool)> = feed(Shown::here(&b)).items.into_iter().map(|i| (i.key, i.project, i.hidden)).collect();
        assert_eq!(got, vec![("h".to_string(), "Elsewhere".to_string(), true), ("ok".to_string(), "P".to_string(), false)]);
    }

    #[test]
    fn lists_a_helper_only_while_it_needs_you() {
        let helper = |key: &str, status: Status| Session { helper: Some(crate::model::Helper { parent: Some("parent".into()), pr: None, report: false, yours: false }), ..session(key, status, NOW - 10) };
        let b = board(vec![lane("/a", vec![session("parent", Status::Working, NOW - 20), helper("busy", Status::Working), helper("done", Status::Idle), helper("asks", Status::Needs), helper("failed", Status::Error)], false)], vec![]);
        let f = feed(Shown::here(&b));
        let got: Vec<(String, Group)> = f.items.into_iter().map(|i| (i.key, i.group)).collect();
        assert_eq!(got, vec![("asks".to_string(), Group::Needs), ("failed".to_string(), Group::Needs), ("parent".to_string(), Group::Working)]);
        // Nor among the last day's sessions.
        assert!(f.recent.iter().all(|i| i.key != "done"));
    }

    #[test]
    fn one_moved_to_the_top_goes_first_in_its_group() {
        let raised = Session { raised: Some(crate::model::Raised { at: NOW, ..Default::default() }), ..session("late", Status::Unread, NOW - 10) };
        let b = board(vec![lane("/a", vec![session("early", Status::Unread, NOW - 50), raised, session("needs", Status::Needs, NOW - 5)], false)], vec![]);
        let got: Vec<(String, Group)> = feed(Shown::here(&b)).items.into_iter().map(|i| (i.key, i.group)).collect();
        assert_eq!(got, vec![("needs".to_string(), Group::Needs), ("late".to_string(), Group::Unread), ("early".to_string(), Group::Unread)]);
    }

    #[test]
    fn lists_the_rest_of_the_last_days_sessions_apart_newest_status_first() {
        const HOUR: i64 = 60 * 60 * 1000;
        let started = |key: &str, status: Status, since: i64, at: Option<i64>| {
            let mut s = session(key, status, since);
            s.started_at = at;
            s
        };
        let mut b = board(
            vec![lane(
                "/a",
                vec![
                    started("old", Status::Idle, NOW - HOUR, Some(NOW - 30 * HOUR)),
                    started("i1", Status::Idle, NOW - 3 * HOUR, Some(NOW - 5 * HOUR)),
                    started("s1", Status::Stopped, NOW - HOUR, Some(NOW - 2 * HOUR)),
                    started("w", Status::Working, NOW, Some(NOW - HOUR)),
                    started("guess", Status::Idle, NOW - 2 * HOUR, None),
                    started("stale", Status::Idle, NOW - 25 * HOUR, None),
                ],
                false,
            )],
            vec![],
        );
        b.generated_at = NOW;
        let f = feed(Shown::here(&b));
        assert_eq!(f.items.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), vec!["w"]);
        let got: Vec<(&str, Group)> = f.recent.iter().map(|i| (i.key.as_str(), i.group)).collect();
        assert_eq!(got, vec![("s1", Group::Recent), ("guess", Group::Recent), ("i1", Group::Recent)]);
    }

    #[test]
    fn lists_snoozed_sessions_apart_soonest_back_first_unless_they_run() {
        let snoozed = |key: &str, status: Status, until: i64| {
            let mut s = session(key, status, NOW - 10);
            s.snoozed_until = Some(until);
            s
        };
        let mut b = board(
            vec![lane(
                "/a",
                vec![
                    snoozed("later", Status::Needs, NOW + 3_000),
                    snoozed("soon", Status::Unread, NOW + 1_000),
                    snoozed("idle", Status::Idle, NOW + 2_000),
                    snoozed("busy", Status::Working, NOW + 1_000),
                    session("n", Status::Needs, NOW),
                ],
                false,
            )],
            vec![],
        );
        b.generated_at = NOW;
        let f = feed(Shown::here(&b));
        let got: Vec<(&str, Group, Option<i64>)> = f.items.iter().map(|i| (i.key.as_str(), i.group, i.snoozed_until)).collect();
        assert_eq!(got, vec![("n", Group::Needs, None), ("busy", Group::Working, Some(NOW + 1_000))]);
        let got: Vec<(&str, Group)> = f.snoozed.iter().map(|i| (i.key.as_str(), i.group)).collect();
        assert_eq!(got, vec![("soon", Group::Snoozed), ("idle", Group::Snoozed), ("later", Group::Snoozed)]);
        assert!(f.recent.is_empty());
    }

    /// A worktree of another device's, as peers.rs hands it to the windows.
    fn there(short: &str, online: bool, sessions: Vec<Session>) -> Lane {
        let tag = DeviceTag { name: format!("Mac {short}"), short: short.into(), color: "teal".into(), online, last_seen: None };
        Lane { device: Some(tag), ..lane(&format!("@{short}:/a"), sessions, false) }
    }

    #[test]
    fn lists_the_sessions_of_your_other_devices_connected_now_unless_turned_off() {
        let mut b = board(vec![lane("/a", vec![session("w", Status::Working, NOW - 10)], false)], vec![]);
        b.devices.enabled = true;
        b.logo_status = LogoStatus::Working;
        let mut elsewhere = crate::shown::Elsewhere::default();
        elsewhere.on_devices(&[there("abc", true, vec![session("@abc:n", Status::Needs, NOW - 20), session("@abc:i", Status::Idle, NOW)]), there("def", false, vec![session("@def:u", Status::Unread, NOW - 30)])]);
        let got = feed(elsewhere.beside(&b));
        let items: Vec<(&str, Option<&str>)> = got.items.iter().map(|i| (i.key.as_str(), i.device.as_ref().map(|d| d.name.as_str()))).collect();
        assert_eq!(items, vec![("@abc:n", Some("Mac abc")), ("w", None)], "an away device's aren't listed");
        assert_eq!(got.devices, vec!["abc"]);
        // The mark says what waits on you there too.
        assert_eq!(got.logo_status, LogoStatus::Needs);

        // Turned off, or with devices off, only this device's.
        b.devices.in_bar = false;
        let off = feed(elsewhere.beside(&b));
        assert_eq!(off.items.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), vec!["w"]);
        assert!(off.devices.is_empty());
        assert_eq!(off.logo_status, LogoStatus::Working);
        b.devices = crate::config::DevicesCfg { enabled: false, ..Default::default() };
        assert_eq!(feed(elsewhere.beside(&b)).items.len(), 1);
    }

    #[test]
    fn names_the_projects_in_your_order_for_the_bars_menu() {
        let b = Board {
            projects: serde_json::from_value(serde_json::json!([
                { "name": "Spruce", "lanes": 3, "repos": [] },
                { "name": "Spur", "lanes": 1, "repos": [] },
                { "name": "Elsewhere", "lanes": 2, "repos": [] }
            ]))
            .expect("projects"),
            ..Board::default()
        };
        assert_eq!(feed(Shown::here(&b)).projects, vec!["Spruce", "Spur", "Elsewhere"]);
    }

    #[test]
    fn lists_todos_ready_to_start_in_your_order_with_where_they_start() {
        let todo = |id: &str, extra: serde_json::Value| -> todos::Todo {
            let mut v = serde_json::json!({ "id": id, "project": "P", "title": format!("Todo {id}") });
            v.as_object_mut().expect("an object").extend(extra.as_object().expect("an object").clone());
            serde_json::from_value(v).expect("a todo")
        };
        let mut b = board(
            vec![lane("/a", vec![session("s1", Status::Idle, NOW - 10), session("s2", Status::Idle, NOW)], false), lane("/b", vec![session("s3", Status::Idle, NOW - 5)], false)],
            vec![],
        );
        b.todos = vec![
            todo("linked", serde_json::json!({ "worktrees": ["/gone", "/b"], "sessions": ["s2"] })),
            todo("started", serde_json::json!({ "status": "in-progress", "sessions": ["s1"] })),
            todo("waiting", serde_json::json!({ "waiting": { "note": "design", "since": NOW } })),
            todo("blocked", serde_json::json!({ "dependsOn": ["open"] })),
            todo("open", serde_json::json!({ "project": "" })),
            todo("finished", serde_json::json!({ "status": "done" })),
            todo("unblocked", serde_json::json!({ "dependsOn": ["finished"], "sessions": ["s3", "s2"] })),
        ];
        let got: Vec<(String, String, Option<String>)> = feed(Shown::here(&b)).todos.into_iter().map(|t| (t.id, t.project, t.callsign)).collect();
        assert_eq!(
            got,
            vec![
                // The first linked worktree that's on the board wins over its sessions'.
                ("linked".to_string(), "P".to_string(), Some("b".to_string())),
                ("open".to_string(), String::new(), None),
                // Without one, the worktree of its latest session.
                ("unblocked".to_string(), "P".to_string(), Some("a".to_string())),
            ]
        );
    }

    #[test]
    fn keeps_pills_in_place_while_a_status_lasts() {
        let mut a = session("a", Status::Working, NOW - 100);
        a.last_activity = NOW;
        let c = session("c", Status::Working, NOW - 50);
        let got: Vec<String> = feed(Shown::here(&board(vec![lane("/a", vec![a, c], false)], vec![]))).items.into_iter().map(|i| i.key).collect();
        assert_eq!(got, vec!["a".to_string(), "c".to_string()]);
    }

    #[test]
    fn carries_the_lanes_entry_for_how_a_session_opens() {
        let mut l = lane("/a", vec![session("s", Status::Needs, NOW)], false);
        l.targets = serde_json::from_value(serde_json::json!([
            { "id": "s|s|focus", "icon": "terminal", "label": "Respond in Ghostty", "detail": null, "kbd": null, "disabled": false, "group": "session" },
            { "id": "l|editor", "icon": "code", "label": "Open in Cursor", "detail": null, "kbd": null, "disabled": false, "group": "lane" }
        ]))
        .expect("targets");
        let item = &feed(Shown::here(&board(vec![l], vec![]))).items[0];
        assert_eq!((item.lane_id.as_str(), item.callsign.as_str(), item.open_target.as_str()), ("/a", "a", "s|s|focus"));
        assert_eq!(item.target.as_ref().map(|t| t.label.as_str()), Some("Respond in Ghostty"));
    }

    #[test]
    fn keeps_open_prs_that_ask_for_your_review_the_longest_wait_first_unless_snoozed() {
        let pr = |n: u64, requested: bool, state: PrState, updated: i64| -> PrItem {
            serde_json::from_value(serde_json::json!({
                "key": format!("o/r#{n}"), "repoSlug": "o/r", "repo": "r", "project": "P", "number": n, "title": format!("PR {n}"),
                "url": "", "state": state, "author": null, "mine": false, "reviewRequested": requested, "requestedBy": if n == 1 { Some("ana") } else { None }, "head": "h", "base": null,
                "checks": { "passed": 0, "failed": 0, "pending": 0, "skipped": 0, "failing": [] }, "merge": "clean",
                "additions": 0, "deletions": 0, "changedFiles": 0, "createdAt": 0, "updatedAt": updated,
                "mergedAt": null, "closedAt": null, "lanes": [], "sessions": []
            }))
            .expect("a PR")
        };
        // One you snoozed is off the bar until its time.
        let snoozed = PrItem { snoozed_until: Some(NOW + 1_000), ..pr(5, true, PrState::Open, 5) };
        let b = board(vec![], vec![pr(1, true, PrState::Open, 1), pr(2, true, PrState::Merged, 3), pr(3, false, PrState::Open, 4), pr(4, true, PrState::Draft, 2), snoozed]);
        let got: Vec<(u64, String, Option<String>)> = feed(Shown::here(&b)).reviews.into_iter().map(|r| (r.number, r.project, r.requested_by)).collect();
        assert_eq!(got, vec![(1, "P".to_string(), Some("ana".to_string())), (4, "P".to_string(), None)]);
    }

    #[test]
    fn a_review_waits_since_its_request_else_the_prs_last_update() {
        let pr = |n: u64, requested_at: Option<i64>| -> PrItem {
            serde_json::from_value(serde_json::json!({
                "key": format!("o/r#{n}"), "repoSlug": "o/r", "repo": "r", "project": "P", "number": n, "title": format!("PR {n}"),
                "url": format!("https://github.com/o/r/pull/{n}"), "state": "open", "author": "ana", "mine": false, "reviewRequested": true, "requestedAt": requested_at, "head": "h", "base": null,
                "checks": { "passed": 0, "failed": 0, "pending": 0, "skipped": 0, "failing": [] }, "merge": "clean",
                "additions": 0, "deletions": 0, "changedFiles": 0, "createdAt": 0, "updatedAt": 50,
                "mergedAt": null, "closedAt": null, "lanes": [], "sessions": []
            }))
            .expect("a PR")
        };
        // The one asked for first waits first, whichever was updated last.
        let b = board(vec![], vec![pr(2, None), pr(1, Some(20))]);
        let got: Vec<(u64, i64, i64, String, String)> = feed(Shown::here(&b)).reviews.into_iter().map(|r| (r.number, r.since, r.updated_at, r.repo_slug, r.url)).collect();
        assert_eq!(got, vec![(1, 20, 50, "o/r".into(), "https://github.com/o/r/pull/1".into()), (2, 50, 50, "o/r".into(), "https://github.com/o/r/pull/2".into())]);
    }
}

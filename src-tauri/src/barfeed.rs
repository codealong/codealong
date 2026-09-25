//! What the floating bar shows, cut from the board: the sessions that need you, whose turn ended,
//! or that are running, in bar order, and the PRs waiting on your review. The bar gets this
//! instead of the board, and only when it changes: the board is about half a megabyte and is
//! published many times a minute, while this is a few kilobytes and changes far less often.

use serde::Serialize;

use crate::config::Theme;
use crate::model::{Board, LogoStatus, Status};

/// The bar's groups, most urgent first.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    Needs,
    Unread,
    Working,
}

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
    pub theme: Theme,
    /// What the mark's colors say, as on the board.
    pub logo_status: LogoStatus,
    pub perf_log: bool,
    /// The terminal app's name, for menu items such as "Show in Ghostty".
    pub terminal: String,
    /// In bar order: by group, then the newest status first.
    pub items: Vec<BarItem>,
    /// Open PRs that ask for your review, newest first.
    pub reviews: Vec<BarReview>,
}

/// One session on the bar, with what its pill, tooltip and menu need.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BarItem {
    pub key: String,
    pub group: Group,
    pub status: Status,
    pub title: String,
    pub account: String,
    pub lane_id: String,
    pub callsign: String,
    pub detail: Option<String>,
    pub open_target: String,
    /// The lane's own entry for `open_target`, when it lists one: lanes list their first few
    /// sessions' targets only.
    pub target: Option<BarTarget>,
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
}

/// The bar's feed. Status times don't move while a status lasts, so ordering by them keeps pills
/// in place while agents work. Visitors, archived sessions and the lanes the board keeps out of
/// sight (Elsewhere) are left out, as on the board.
pub fn feed(board: &Board) -> BarFeed {
    let mut items: Vec<(i64, BarItem)> = Vec::new();
    for lane in board.lanes.iter().filter(|l| !l.hidden) {
        for s in &lane.sessions {
            if s.visitor.is_some() || s.archived {
                continue;
            }
            let Some(group) = group_of(s.status) else { continue };
            let target = lane.targets.iter().find(|t| t.id == s.open_target).map(|t| BarTarget {
                label: t.label.clone(),
                detail: t.detail.clone(),
                disabled: t.disabled,
            });
            let item = BarItem {
                key: s.key.clone(),
                group,
                status: s.status,
                title: s.title.clone(),
                account: s.account.clone(),
                lane_id: lane.id.clone(),
                callsign: lane.callsign.clone(),
                detail: s.detail.clone(),
                open_target: s.open_target.clone(),
                target,
            };
            items.push((s.status_since.unwrap_or(s.last_activity), item));
        }
    }
    items.sort_by(|(a_at, a), (b_at, b)| a.group.cmp(&b.group).then(b_at.cmp(a_at)).then_with(|| a.key.cmp(&b.key)));

    let mut prs: Vec<_> = board.prs.iter().filter(|p| p.review_requested && p.state.is_open()).collect();
    prs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let reviews = prs.into_iter().map(|p| BarReview { key: p.key.clone(), number: p.number, title: p.title.clone(), repo: p.repo.clone() }).collect();

    BarFeed {
        loading: board.loading,
        theme: board.theme,
        logo_status: board.logo_status,
        perf_log: board.perf_log,
        terminal: board.apps.terminal.clone(),
        items: items.into_iter().map(|(_, item)| item).collect(),
        reviews,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Lane, PrItem, PrState, Session};

    const NOW: i64 = 1_790_200_000_000;

    fn session(key: &str, status: Status, since: i64) -> Session {
        serde_json::from_value(serde_json::json!({
            "key": key, "id": key, "shortId": key, "harness": "claude", "account": "claude", "surface": "cli",
            "status": status, "live": true, "pid": null, "title": format!("Title {key}"), "cwd": "/r", "work": [],
            "name": null, "startedAt": null, "statusSince": since, "lastActivity": since, "lastPromptAt": since, "excerpt": null,
            "resumeCommand": null, "appAccount": false, "openTarget": format!("s|{key}|focus"), "visitor": null,
            "headline": "", "detail": null, "branch": null, "archived": false, "attachCommand": null, "stalledSince": null
        }))
        .expect("a session")
    }

    fn lane(id: &str, sessions: Vec<Session>, hidden: bool) -> Lane {
        let mut l: Lane = serde_json::from_value(serde_json::json!({
            "id": id, "project": "P", "repo": "r", "repoSlug": null, "path": id, "callsign": id.trim_start_matches('/'),
            "origin": "main", "branch": null, "detached": null, "bay": "idle", "glyph": "idle", "parked": false,
            "hidden": hidden, "headline": "", "detail": null, "sessions": [], "pr": null, "git": null,
            "lastActivity": 0, "targets": [], "defaultTarget": null
        }))
        .expect("a lane");
        l.sessions = sessions;
        l
    }

    fn board(lanes: Vec<Lane>, prs: Vec<PrItem>) -> Board {
        Board { lanes, prs, ..Board::default() }
    }

    #[test]
    fn lists_what_needs_you_then_finished_turns_then_running_agents_newest_first() {
        let b = board(
            vec![
                lane("/a", vec![session("w1", Status::Working, NOW - 50), session("i1", Status::Idle, NOW), session("u1", Status::Unread, NOW - 10)], false),
                lane("/b", vec![session("n1", Status::Needs, NOW - 90), session("e1", Status::Error, NOW - 5), session("w2", Status::Working, NOW - 20), session("s1", Status::Stopped, NOW)], false),
            ],
            vec![],
        );
        let got: Vec<(String, Group)> = feed(&b).items.into_iter().map(|i| (i.key, i.group)).collect();
        let want = [("e1", Group::Needs), ("n1", Group::Needs), ("u1", Group::Unread), ("w2", Group::Working), ("w1", Group::Working)];
        assert_eq!(got, want.map(|(k, g)| (k.to_string(), g)));
    }

    #[test]
    fn leaves_out_visitors_archived_sessions_and_lanes_kept_out_of_sight() {
        let mut visitor = session("v", Status::Working, NOW);
        visitor.visitor = Some("worked here".into());
        let mut archived = session("x", Status::Unread, NOW);
        archived.archived = true;
        let b = board(
            vec![
                lane("/a", vec![visitor, archived], false),
                lane("/elsewhere", vec![session("h", Status::Working, NOW)], true),
                lane("/b", vec![session("ok", Status::Working, NOW)], false),
            ],
            vec![],
        );
        let got: Vec<String> = feed(&b).items.into_iter().map(|i| i.key).collect();
        assert_eq!(got, vec!["ok".to_string()]);
    }

    #[test]
    fn keeps_pills_in_place_while_a_status_lasts() {
        let mut a = session("a", Status::Working, NOW - 100);
        a.last_activity = NOW;
        let c = session("c", Status::Working, NOW - 50);
        let got: Vec<String> = feed(&board(vec![lane("/a", vec![a, c], false)], vec![])).items.into_iter().map(|i| i.key).collect();
        assert_eq!(got, vec!["c".to_string(), "a".to_string()]);
    }

    #[test]
    fn carries_the_lanes_entry_for_how_a_session_opens() {
        let mut l = lane("/a", vec![session("s", Status::Needs, NOW)], false);
        l.targets = serde_json::from_value(serde_json::json!([
            { "id": "s|s|focus", "icon": "terminal", "label": "Respond in Ghostty", "detail": null, "kbd": null, "disabled": false, "group": "session" },
            { "id": "l|editor", "icon": "code", "label": "Open in Cursor", "detail": null, "kbd": null, "disabled": false, "group": "lane" }
        ]))
        .expect("targets");
        let item = &feed(&board(vec![l], vec![])).items[0];
        assert_eq!((item.lane_id.as_str(), item.callsign.as_str(), item.open_target.as_str()), ("/a", "a", "s|s|focus"));
        assert_eq!(item.target.as_ref().map(|t| t.label.as_str()), Some("Respond in Ghostty"));
    }

    #[test]
    fn keeps_open_prs_that_ask_for_your_review_newest_first() {
        let pr = |n: u64, requested: bool, state: PrState, updated: i64| -> PrItem {
            serde_json::from_value(serde_json::json!({
                "key": format!("o/r#{n}"), "repoSlug": "o/r", "repo": "r", "project": "P", "number": n, "title": format!("PR {n}"),
                "url": "", "state": state, "author": null, "mine": false, "reviewRequested": requested, "head": "h", "base": null,
                "checks": { "passed": 0, "failed": 0, "pending": 0, "skipped": 0, "failing": [] }, "merge": "clean",
                "additions": 0, "deletions": 0, "changedFiles": 0, "createdAt": 0, "updatedAt": updated,
                "mergedAt": null, "closedAt": null, "lanes": [], "sessions": []
            }))
            .expect("a PR")
        };
        let b = board(vec![], vec![pr(1, true, PrState::Open, 1), pr(2, true, PrState::Merged, 3), pr(3, false, PrState::Open, 4), pr(4, true, PrState::Draft, 2)]);
        let got: Vec<u64> = feed(&b).reviews.into_iter().map(|r| r.number).collect();
        assert_eq!(got, vec![4, 1]);
    }
}

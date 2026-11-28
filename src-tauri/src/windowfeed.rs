//! What the windows are sent of each board the engine publishes (`Feed::push`): the first board
//! whole, then what changed since the one before, which `src/boardPush.ts` puts back together.
//! The board was some 2 MB, two thirds of it sessions and most of those archived, and a publish
//! usually changes one session; sent whole, every window that follows the board parsed all of it
//! for that, some nine thousand times in 17 hours (perf.log, 2026-10-28). A session is the
//! smallest part sent: a busy worktree holds two hundred, and the whole worktree for each change in
//! one of them would still be 300 KB.
//!
//! A push has the board's `rev`, and one with only what changed has the `base` it changes: a
//! window whose board isn't that one reads the board whole (`get_board`), which is this push's or
//! a later one. The board is written out once, as it was when it went whole, and its parts are cut
//! from that text (`RawValue`), hashed, and pasted into the push as they were written.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};

use serde_json::value::RawValue;

use crate::model::Board;

/// A list of the board's sent an item at a time, as the sessions are: its field's name as
/// written, the name its order goes by in a push, and its items' keys on the board, in order.
/// Any other field goes whole when it changes. The schedules (some 270 KB, one of them changed by
/// a check every few minutes) and the todos (some 900 KB) did: the schedules were 1.5 GB of the
/// 2.1 GB the windows were sent in a week (perf.log, 2026-11-21 to 11-28).
struct List {
    name: &'static str,
    order: &'static str,
    keys: for<'a> fn(&'a Board) -> Vec<&'a str>,
}

const LISTS: [List; 3] = [
    List { name: "prs", order: "prOrder", keys: |b| b.prs.iter().map(|p| p.key.as_str()).collect() },
    List { name: "schedules", order: "scheduleOrder", keys: |b| b.schedules.iter().map(|s| s.schedule.id.as_str()).collect() },
    List { name: "todos", order: "todoOrder", keys: |b| b.todos.iter().map(|t| t.id.as_str()).collect() },
];

/// What the windows were last sent, part by part.
#[derive(Default)]
pub struct Feed {
    /// The rev of the board last sent; none before the first push.
    rev: Option<u64>,
    /// Each field of the board but its lanes and `LISTS`, by its name as written.
    fields: HashMap<String, u64>,
    lanes: Vec<String>,
    lane: HashMap<String, LaneSent>,
    /// Each of `LISTS`, in its order there.
    lists: Vec<ListSent>,
}

/// One of `LISTS` as last sent: its items' keys in order, and each item's hash by its key.
#[derive(Default)]
struct ListSent {
    keys: Vec<String>,
    items: HashMap<String, u64>,
}

/// A lane as last sent: the lane without its sessions, and its sessions by key, in order.
#[derive(Default, Clone, PartialEq)]
struct LaneSent {
    head: u64,
    keys: Vec<String>,
    sessions: HashMap<String, u64>,
}

/// A push, as the JSON it's sent as, and what it held, for the perf log.
pub struct Push {
    pub json: Box<RawValue>,
    /// It's the whole board, not what changed.
    pub whole: bool,
    pub lanes: usize,
    pub sessions: usize,
}

/// A board's text cut into its parts, each as it was written.
struct Cut<'a> {
    /// Every field but the lanes, `LISTS` and the rev, which each push carries apart.
    fields: Vec<(&'a str, &'a RawValue)>,
    lanes: Vec<LaneCut<'a>>,
    /// Each of `LISTS`' items by its key, in its order there.
    lists: Vec<Vec<(&'a str, &'a RawValue)>>,
}

struct LaneCut<'a> {
    id: &'a str,
    /// The lane without its sessions.
    head: String,
    sessions: Vec<(&'a str, &'a RawValue)>,
}

impl Feed {
    /// What to send the windows of `board`. A board that can't be written out is an error, and
    /// the feed starts over, so the next push is whole.
    pub fn push(&mut self, board: &Board) -> serde_json::Result<Push> {
        let text = serde_json::to_string(board)?;
        let cut = match cut(&text, board) {
            Ok(cut) => cut,
            Err(e) => {
                *self = Feed::default();
                return Err(e);
            }
        };
        let next = sent(&cut, board.rev);
        // Two lanes with one id, or two sessions in a lane or two items of a list with one key,
        // can't be told apart in what changed.
        let told_apart = next.lane.len() == cut.lanes.len() && next.lists.iter().zip(&cut.lists).all(|(sent, items)| sent.items.len() == items.len()) && cut.lanes.iter().all(|l| next.lane[l.id].sessions.len() == l.sessions.len());
        let delta = match self.rev {
            Some(base) if told_apart => Some(self.delta(&cut, &next, base, board.rev)),
            _ => None,
        };
        // Whole when most of it changed anyway, as at launch or once the repos are read again.
        let push = match delta.filter(|(json, _, _)| json.len() < text.len() / 2) {
            Some((json, lanes, sessions)) => Push { json: RawValue::from_string(json)?, whole: false, lanes, sessions },
            None => Push { json: RawValue::from_string(format!(r#"{{"rev":{},"full":{text}}}"#, board.rev))?, whole: true, lanes: cut.lanes.len(), sessions: cut.lanes.iter().map(|l| l.sessions.len()).sum() },
        };
        *self = if told_apart { next } else { Feed::default() };
        Ok(push)
    }

    /// What changed from what was last sent to `next`, as the push's JSON, with how many lanes and
    /// sessions it carries.
    fn delta(&self, cut: &Cut, next: &Feed, base: u64, rev: u64) -> (String, usize, usize) {
        let mut out = format!(r#"{{"rev":{rev},"base":{base}"#);
        let changed: Vec<String> = cut.fields.iter().filter(|(k, _)| self.fields.get(*k) != next.fields.get(*k)).map(|(k, v)| format!("{}:{}", quoted(k), v.get())).collect();
        if !changed.is_empty() {
            let _ = write!(out, r#","fields":{{{}}}"#, changed.join(","));
        }
        let mut gone: Vec<&String> = self.fields.keys().filter(|k| !next.fields.contains_key(*k)).collect();
        gone.sort();
        if !gone.is_empty() {
            let _ = write!(out, r#","gone":{}"#, list(gone.iter().map(|k| k.as_str())));
        }
        if next.lanes != self.lanes {
            let _ = write!(out, r#","laneOrder":{}"#, list(next.lanes.iter().map(String::as_str)));
        }
        let (mut heads, mut orders, mut groups, mut lanes, mut sessions) = (Vec::new(), Vec::new(), Vec::new(), 0, 0);
        for l in &cut.lanes {
            let was = self.lane.get(l.id);
            let now = &next.lane[l.id];
            if was.map(|w| w.head) != Some(now.head) {
                heads.push(l.head.as_str());
            }
            if was.map(|w| &w.keys) != Some(&now.keys) {
                orders.push(format!("{}:{}", quoted(l.id), list(now.keys.iter().map(String::as_str))));
            }
            let changed: Vec<&str> = l.sessions.iter().filter(|(k, _)| was.and_then(|w| w.sessions.get(*k)) != now.sessions.get(*k)).map(|(_, v)| v.get()).collect();
            if !changed.is_empty() {
                sessions += changed.len();
                groups.push(format!("{}:[{}]", quoted(l.id), changed.join(",")));
            }
            if was != Some(now) {
                lanes += 1;
            }
        }
        if !heads.is_empty() {
            let _ = write!(out, r#","lanes":[{}]"#, heads.join(","));
        }
        if !orders.is_empty() {
            let _ = write!(out, r#","sessionsOf":{{{}}}"#, orders.join(","));
        }
        if !groups.is_empty() {
            let _ = write!(out, r#","sessions":{{{}}}"#, groups.join(","));
        }
        let none = ListSent::default();
        for (i, kind) in LISTS.iter().enumerate() {
            let (was, now) = (self.lists.get(i).unwrap_or(&none), &next.lists[i]);
            if now.keys != was.keys {
                let _ = write!(out, r#","{}":{}"#, kind.order, list(now.keys.iter().map(String::as_str)));
            }
            let changed: Vec<&str> = cut.lists[i].iter().filter(|(k, _)| was.items.get(*k) != now.items.get(*k)).map(|(_, v)| v.get()).collect();
            if !changed.is_empty() {
                let _ = write!(out, r#","{}":[{}]"#, kind.name, changed.join(","));
            }
        }
        out.push('}');
        (out, lanes, sessions)
    }
}

/// `text`, the board written out, cut into its parts; the lanes' ids and the keys of sessions and
/// of `LISTS`' items are the board's own, in the order they were written.
fn cut<'a>(text: &'a str, board: &'a Board) -> serde_json::Result<Cut<'a>> {
    let top: BTreeMap<&str, &RawValue> = serde_json::from_str(text)?;
    let parts = |name: &str| top.get(name).map(|raw| raw.get()).unwrap_or("[]");
    let lanes: Vec<BTreeMap<&str, &RawValue>> = serde_json::from_str(parts("lanes"))?;
    let mismatch = || serde::de::Error::custom("the board's text doesn't match the board");
    if lanes.len() != board.lanes.len() {
        return Err(mismatch());
    }
    let mut lists = Vec::with_capacity(LISTS.len());
    for kind in &LISTS {
        let items: Vec<&RawValue> = serde_json::from_str(parts(kind.name))?;
        let keys = (kind.keys)(board);
        if items.len() != keys.len() {
            return Err(mismatch());
        }
        lists.push(keys.into_iter().zip(items).collect());
    }
    let mut cut_lanes = Vec::with_capacity(lanes.len());
    for (raw, lane) in lanes.iter().zip(&board.lanes) {
        let sessions: Vec<&RawValue> = serde_json::from_str(raw.get("sessions").map(|s| s.get()).unwrap_or("[]"))?;
        if sessions.len() != lane.sessions.len() {
            return Err(mismatch());
        }
        let head: Vec<String> = raw.iter().filter(|(k, _)| **k != "sessions").map(|(k, v)| format!("{}:{}", quoted(k), v.get())).collect();
        cut_lanes.push(LaneCut { id: &lane.id, head: format!("{{{}}}", head.join(",")), sessions: lane.sessions.iter().map(|s| s.key.as_str()).zip(sessions).collect() });
    }
    Ok(Cut {
        fields: top.into_iter().filter(|(k, _)| !matches!(*k, "lanes" | "rev") && !LISTS.iter().any(|l| l.name == *k)).collect(),
        lanes: cut_lanes,
        lists,
    })
}

/// What the windows have once `cut` is sent.
fn sent(cut: &Cut, rev: u64) -> Feed {
    let lane = cut
        .lanes
        .iter()
        .map(|l| (l.id.to_string(), LaneSent { head: hash(&l.head), keys: l.sessions.iter().map(|(k, _)| k.to_string()).collect(), sessions: l.sessions.iter().map(|(k, v)| (k.to_string(), hash(v.get()))).collect() }))
        .collect();
    Feed {
        rev: Some(rev),
        fields: cut.fields.iter().map(|(k, v)| (k.to_string(), hash(v.get()))).collect(),
        lanes: cut.lanes.iter().map(|l| l.id.to_string()).collect::<Vec<_>>(),
        lane,
        lists: cut.lists.iter().map(|items| ListSent { keys: items.iter().map(|(k, _)| k.to_string()).collect(), items: items.iter().map(|(k, v)| (k.to_string(), hash(v.get()))).collect() }).collect(),
    }
}

fn hash(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

fn quoted(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

fn list<'a>(items: impl Iterator<Item = &'a str>) -> String {
    format!("[{}]", items.map(quoted).collect::<Vec<_>>().join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{lane, session};
    use crate::model::{PrItem, Status};
    use serde_json::{json, Value};

    fn pr(n: u64, title: &str) -> PrItem {
        serde_json::from_value(json!({
            "key": format!("o/r#{n}"), "repoSlug": "o/r", "repo": "r", "project": "P", "number": n, "title": title,
            "url": format!("https://github.com/o/r/pull/{n}"), "state": "open", "author": "ana", "mine": false,
            "reviewRequested": false, "head": "b", "checks": crate::model::Checks::default(), "merge": crate::model::Merge::default(), "additions": 1, "deletions": 0,
            "changedFiles": 1, "createdAt": 1, "updatedAt": 2, "lanes": [], "sessions": [], "labels": []
        }))
        .expect("a PR")
    }

    fn with(key: &str, status: Status) -> crate::model::Session {
        crate::model::Session { status, ..session(key) }
    }

    fn schedule(id: &str, next_at: i64) -> crate::schedules::Item {
        let schedule = crate::schedules::Schedule { id: id.into(), name: format!("Schedule {id}"), ..Default::default() };
        crate::schedules::Item { schedule, next_at: Some(next_at), ..Default::default() }
    }

    fn todo(id: &str, title: &str) -> crate::todos::Todo {
        serde_json::from_value(json!({ "id": id, "title": title })).expect("a todo")
    }

    /// The boards a run of pushes goes through, each changing the one before as publishes do.
    fn boards() -> Vec<Board> {
        let first = Board {
            generated_at: 1_000,
            rev: 1,
            lanes: vec![lane("/work/app", vec![session("claude:a:1"), session("claude:a:2"), session("claude:a:3")]), lane("/work/api", vec![session("codex:c:4")])],
            prs: vec![pr(1, "Add teams"), pr(2, "Fix login")],
            github_current: [("o/r".to_string(), Some(900))].into(),
            schedules: vec![schedule("s-1", 5_000), schedule("s-2", 9_000)],
            todos: vec![todo("t-1", "Write the guide"), todo("t-2", "Fix the bar")],
            ..Board::default()
        };
        let mut boards = vec![first];
        let mut next = |change: &dyn Fn(&mut Board)| {
            let mut b = boards.last().unwrap().clone();
            b.rev += 1;
            b.generated_at += 1_000;
            change(&mut b);
            boards.push(b);
        };
        // A session's status, the most common change.
        next(&|b| b.lanes[0].sessions[1] = with("claude:a:2", Status::Working));
        // Sessions reordered in a lane, and a lane's own field.
        next(&|b| {
            b.lanes[0].sessions.swap(0, 1);
            b.lanes[1].headline = "Fixing the login".into();
        });
        // A lane comes, one goes, and a session moves to the new one.
        next(&|b| {
            let moved = b.lanes[0].sessions.remove(2);
            b.lanes.remove(1);
            b.lanes.insert(0, lane("/work/app-fix", vec![moved, session("claude:a:5")]));
        });
        // A PR changes, one comes, one goes; a field changes and one goes from the board.
        next(&|b| {
            b.prs[0].title = "Add teams and roles".into();
            b.prs.remove(1);
            b.prs.push(pr(3, "Bump deps"));
            b.loading = true;
            b.github_current.clear();
        });
        // A schedule's check moves its next run; a todo changes, and one comes ahead of the rest.
        next(&|b| {
            b.schedules[1].next_at = Some(12_000);
            b.todos[0].title = "Write the guide's first part".into();
            b.todos.insert(0, todo("t-3", "Bump deps"));
        });
        // Only the time.
        next(&|_| {});
        boards
    }

    fn pushes(boards: &[Board]) -> Vec<Push> {
        let mut feed = Feed::default();
        boards.iter().map(|b| feed.push(b).expect("pushed")).collect()
    }

    fn value(push: &Push) -> Value {
        serde_json::from_str(push.json.get()).expect("JSON")
    }

    #[test]
    fn the_first_push_is_whole_and_the_rest_only_what_changed() {
        let boards = boards();
        let pushes = pushes(&boards);
        assert!(pushes[0].whole);
        assert_eq!(value(&pushes[0])["full"], serde_json::to_value(&boards[0]).unwrap());
        assert!(pushes[1..].iter().all(|p| !p.whole));

        // One session, and the time.
        let one = value(&pushes[1]);
        assert_eq!((one["rev"].as_u64(), one["base"].as_u64()), (Some(2), Some(1)));
        assert_eq!(one["sessions"]["/work/app"].as_array().map(|s| s.len()), Some(1));
        assert_eq!(one["sessions"]["/work/app"][0]["key"], "claude:a:2");
        assert_eq!(one["fields"].as_object().map(|f| f.keys().cloned().collect::<Vec<_>>()), Some(vec!["generatedAt".to_string()]));
        for absent in ["lanes", "laneOrder", "sessionsOf", "prs", "prOrder", "schedules", "scheduleOrder", "todos", "todoOrder", "gone"] {
            assert!(one.get(absent).is_none(), "{absent}");
        }

        // A new order says the keys, and a lane's field sends the lane without its sessions.
        let order = value(&pushes[2]);
        assert_eq!(order["sessionsOf"]["/work/app"], json!(["claude:a:2", "claude:a:1", "claude:a:3"]));
        assert!(order.get("sessions").is_none(), "no session changed");
        assert_eq!(order["lanes"][0]["headline"], "Fixing the login");
        assert!(order["lanes"][0].get("sessions").is_none());

        // A moved session is sent with its new lane, whole; the lane that went is only missing.
        let moved = value(&pushes[3]);
        assert_eq!(moved["laneOrder"], json!(["/work/app-fix", "/work/app"]));
        assert_eq!(moved["sessions"]["/work/app-fix"].as_array().map(|s| s.len()), Some(2));
        assert_eq!(moved["sessionsOf"]["/work/app"], json!(["claude:a:2", "claude:a:1"]));

        // PRs by key; a field the board no longer writes is gone.
        let prs = value(&pushes[4]);
        assert_eq!(prs["prOrder"], json!(["o/r#1", "o/r#3"]));
        assert_eq!(prs["prs"].as_array().map(|p| p.iter().map(|p| p["key"].clone()).collect::<Vec<_>>()), Some(vec![json!("o/r#1"), json!("o/r#3")]));
        assert_eq!(prs["gone"], json!(["githubCurrent"]));
        assert_eq!(prs["fields"]["loading"], true);

        // Schedules and todos by id, as PRs go: only the ones that changed, and the order when it
        // did, never the whole list as a field.
        let lists = value(&pushes[5]);
        let ids = |v: &Value| v.as_array().map(|items| items.iter().map(|i| i["id"].clone()).collect::<Vec<_>>());
        assert_eq!(ids(&lists["schedules"]), Some(vec![json!("s-2")]));
        assert!(lists.get("scheduleOrder").is_none(), "the schedules' order didn't change");
        assert_eq!(ids(&lists["todos"]), Some(vec![json!("t-3"), json!("t-1")]));
        assert_eq!(lists["todoOrder"], json!(["t-3", "t-1", "t-2"]));
        assert_eq!(lists["fields"].as_object().map(|f| f.keys().cloned().collect::<Vec<_>>()), Some(vec!["generatedAt".to_string()]));

        // Nothing but the time: a few bytes.
        assert!(pushes[6].json.get().len() < 80, "{}", pushes[6].json.get());
    }

    #[test]
    fn sessions_that_cant_be_told_apart_go_whole() {
        let boards = boards();
        let mut feed = Feed::default();
        feed.push(&boards[0]).unwrap();
        let mut twice = boards[1].clone();
        twice.lanes[0].sessions.push(session("claude:a:1"));
        assert!(feed.push(&twice).unwrap().whole);
        assert!(feed.push(&boards[2]).unwrap().whole, "and the next, since what was sent couldn't be kept");
        assert!(!feed.push(&boards[3]).unwrap().whole);
        // So do two schedules with one id.
        let mut again = boards[4].clone();
        again.schedules.push(schedule("s-1", 7_000));
        assert!(feed.push(&again).unwrap().whole);
    }

    /// The pushes `boards` go through, with each board as a window should have it once the push
    /// is applied: src/boardPush.test.ts applies them, so the two sides can't drift apart. Written
    /// with `CODEALONG_WRITE_FIXTURES=1 cargo test pushes_fixture`.
    #[test]
    fn pushes_fixture_is_current() {
        let boards = boards();
        let made = Value::Array(pushes(&boards).iter().zip(&boards).map(|(p, b)| json!({ "push": value(p), "board": serde_json::to_value(b).unwrap() })).collect());
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/testing/boardPushes.json");
        if std::env::var_os("CODEALONG_WRITE_FIXTURES").is_some() {
            std::fs::write(&path, serde_json::to_string_pretty(&made).unwrap() + "\n").unwrap();
        }
        let kept: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("the fixture")).expect("JSON");
        assert!(kept == made, "src/testing/boardPushes.json is stale: write it again with `CODEALONG_WRITE_FIXTURES=1 cargo test pushes_fixture`");
    }
}

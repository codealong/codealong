//! Schedules your machines share (`RunsOn::One`, `RunsOn::Every`). Each machine keeps a copy of
//! every one of them in its own schedules.json, so a machine that's asleep or off when another runs
//! one misses nothing, and one that's up when the others aren't can run it:
//!
//! - Each machine tells the others of its copies in its feed (`outgoing`, codealong-peer's
//!   `schedules`): the schedule, whether it can run it there (`able`: its folder, account and the
//!   session it sends to are there), the latest of its times it took and the ones it's done with,
//!   its latest runs, what it ran for, what its watch saw, and its notes.
//! - Each takes in the others' copies (`merged`): one it hasn't got, and one changed since its own
//!   was, the newest change winning wherever it was made. One removed, or made one machine's
//!   again, is listed as gone with when, and every copy from before then goes.
//! - For one that runs on one of them, each works out by itself which one runs it (`line`): of the
//!   machines that are around (shared.rs) and can run it, the one it prefers, then first choices
//!   before backups (`[devices] automations`), then by key. The first in line runs its times, looks
//!   with its watch and runs for what happens on its board; a time another machine took isn't run
//!   again, and one the first in line doesn't take within `TAKE_OVER_MS` goes to the next. A machine
//!   that was asleep waits `SETTLE_MS` for the others' feeds before it catches up on a time, so it
//!   doesn't run one another ran meanwhile.
//! - The one that runs it next goes on from the last: what its watch saw and what it ran for are
//!   every machine's together, and its notes are the latest any machine wrote.
//!
//! One that runs on each of them runs on each by itself, as one of this machine's does; only its
//! copy and its runs go around. Times are read on each machine's own clock, so machines in other
//! time zones run a time as their clocks have it.

use std::collections::HashSet;

use codealong_peer::schedules::{Day, Notes, Shared, SharedSchedule, Watch};

use super::*;
use crate::shared::Machines;

/// A time the first in line hasn't taken this long after a machine noticed it goes to the next.
pub const TAKE_OVER_MS: i64 = 10 * 60_000;
/// How long a machine that was asleep, or just opened, waits for the others' feeds before it runs
/// a time of a schedule they share.
pub const SETTLE_MS: i64 = 90_000;
/// How long a schedule that left the shared list is remembered as gone.
const KEEP_GONE_MS: i64 = 30 * crate::util::DAY_MS;
/// How many of its latest runs a machine tells the others of.
const SHARE_RUNS: usize = 10;
/// How many of the latest ids of what it ran for a machine tells the others of.
const SHARE_SEEN: usize = 300;
/// The most of a schedule's notes that go around.
pub const MOST_NOTES: usize = 64 * 1024;

/// Why this machine can't run a schedule now, past what `check` says of it: its folder or the
/// session it sends to isn't here, or its account is turned off here.
pub fn able(s: &Schedule, board: &Board, cfg: &Config, zone: &dyn Zone) -> std::result::Result<(), String> {
    check(s, cfg, s.updated_at.max(s.created_at), zone)?;
    if let Some(why) = board_problem(s, board) {
        return Err(why);
    }
    if s.run.to != To::Session {
        let account = s.run.account.trim();
        let on = |name: &str| cfg.accounts.iter().any(|a| a.name == name && !a.disabled);
        if let Some(a) = cfg.accounts.iter().find(|a| a.name == account && a.disabled) {
            return Err(format!("{} is turned off here.", a.name));
        }
        // The routing group it starts from: its own, or Auto's when `[routing] auto` names one, as
        // a start picks (start.rs `resolve_group`); with members, one of them on here, and else
        // (Auto, a group of every account) any account.
        let auto = account.is_empty() || account == "auto";
        let named = if auto { cfg.routing.auto.trim() } else { account };
        let group = (!named.is_empty()).then(|| cfg.routing.groups.iter().find(|g| g.name.trim() == named)).flatten();
        let none_on = match cfg.accounts.is_empty() {
            true => Some("No account is set up here."),
            false => cfg.accounts.iter().all(|a| a.disabled).then_some("Every account is turned off here."),
        };
        match group {
            None if auto && !named.is_empty() => return Err(format!("Auto's routing group, {named}, isn't one here.")),
            Some(g) if !g.members.is_empty() => {
                if !g.members.iter().any(|m| on(&m.account)) {
                    return Err(format!("Every account in {} is turned off here.", g.name.trim()));
                }
            }
            Some(_) | None if auto || group.is_some() => {
                if let Some(why) = none_on {
                    return Err(why.into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// A schedule as another machine is told of it: its folder under home written `~/…`, which is the
/// same folder on a machine whose home is elsewhere, and the repo it's a checkout of, so a machine
/// whose checkout of it is elsewhere runs it there (`place`). One whose folder isn't a worktree
/// here passes on the repo it came with.
fn portable(s: &Schedule, board: &Board) -> Schedule {
    let mut s = s.clone();
    let folder = s.run.folder.trim().to_string();
    if !folder.is_empty() {
        if let Some(c) = worktree(board, &folder).and_then(Checkout::of) {
            s.checkout = Some(c);
        }
        s.run.folder = crate::util::tilde(&crate::util::expand(&folder));
    }
    s
}

/// What this machine tells your others of the schedules they share.
pub fn outgoing(file: &File, runs: &Runs, board: &Board, cfg: &Config, zone: &dyn Zone) -> Shared {
    let list = file
        .schedules
        .iter()
        .filter(|s| s.runs_on.shared())
        .map(|s| {
            let t = runs.of.get(&s.id);
            let able = able(s, board, cfg, zone);
            // What the next machine goes on from: only for one that runs on one of them.
            let one = matches!(s.runs_on, RunsOn::One { .. });
            SharedSchedule {
                id: s.id.clone(),
                schedule: serde_json::to_value(portable(s, board)).unwrap_or_default(),
                able: able.is_ok(),
                why_not: able.err(),
                took: t.and_then(|t| t.took),
                last_due: t.and_then(|t| t.last_due),
                runs: t.map(|t| t.runs.iter().rev().take(SHARE_RUNS).filter_map(|r| serde_json::to_value(r).ok()).collect()).unwrap_or_default(),
                today: t.and_then(|t| t.day).map(|d| Day { day: d.day, runs: d.runs }),
                seen: t.filter(|_| one).map(|t| t.seen[t.seen.len().saturating_sub(SHARE_SEEN)..].to_vec()).unwrap_or_default(),
                watch: t.filter(|t| one && t.seeded && matches!(s.when, When::Watch { .. })).map(|t| Watch { looked: t.looked.clone(), value: t.value.clone(), seeded: true, changed_at: t.changed_at.unwrap_or_default(), since: t.armed_at }),
                notes: t.filter(|_| one && s.notes).and_then(|t| t.notes.as_ref()).map(|n| Notes { text: n.text.clone(), at: n.at }),
            }
        })
        .collect();
    Shared { list, gone: file.gone.clone() }
}

/// Your other machines' copies taken in: what's changed in the schedules, and of them the ones
/// removed. None when nothing changed. A copy resumed there carries when (`Schedule::resumed_at`),
/// which the scheduler goes from (runner.rs `Note::Resumed`).
#[derive(Debug, Clone, PartialEq)]
pub struct Merged {
    pub file: File,
    pub removed: Vec<String>,
}

pub fn merged(file: &File, machines: &Machines, now: i64) -> Option<Merged> {
    let mut out = file.clone();
    let mut removed = Vec::new();
    // What went first, so a copy from before it went isn't taken in from a machine that hasn't
    // heard yet.
    for g in machines.others.iter().flat_map(|m| &m.schedules.gone) {
        if let Some(i) = out.schedules.iter().position(|s| s.id == g.id && s.runs_on.shared() && s.updated_at < g.at) {
            removed.push(out.schedules.remove(i).id);
        }
        // Kept here too, so it goes on to a machine that only hears from this one.
        let later = out.schedules.iter().any(|s| s.id == g.id && s.updated_at >= g.at);
        match out.gone.iter_mut().find(|x| x.id == g.id) {
            Some(x) if x.at < g.at && !later => x.at = g.at,
            None if !later => out.gone.push(g.clone()),
            _ => {}
        }
    }
    for x in machines.others.iter().flat_map(|m| &m.schedules.list) {
        let Ok(theirs) = serde_json::from_value::<Schedule>(x.schedule.clone()) else { continue };
        if theirs.id != x.id || !theirs.runs_on.shared() || out.gone.iter().any(|g| g.id == theirs.id && g.at >= theirs.updated_at) {
            continue;
        }
        match out.schedules.iter().position(|s| s.id == theirs.id) {
            None => out.schedules.push(theirs),
            Some(i) if theirs.updated_at > out.schedules[i].updated_at => out.schedules[i] = theirs,
            Some(_) => {}
        }
    }
    // A schedule shared again since it went is back; and what went long ago is forgotten, once
    // every other machine has been heard from since without a copy from before it went (one that's
    // off keeps its copy, and it would come back from that).
    let back = |g: &codealong_peer::schedules::Gone| out.schedules.iter().any(|s| s.id == g.id && s.runs_on.shared() && s.updated_at > g.at);
    let copy_before = |m: &crate::shared::Machine, g: &codealong_peer::schedules::Gone| m.schedules.list.iter().any(|x| x.id == g.id && x.schedule.get("updatedAt").and_then(serde_json::Value::as_i64).is_none_or(|u| u <= g.at));
    let heard = |g: &codealong_peer::schedules::Gone| machines.others.iter().all(|m| m.fed_at.is_some_and(|t| t > g.at) && !copy_before(m, g));
    let gone: Vec<_> = out.gone.iter().filter(|g| !back(g) && (now - g.at < KEEP_GONE_MS || !heard(g))).cloned().collect();
    out.gone = gone;
    (out != *file).then_some(Merged { file: out, removed })
}

/// A machine's place in a schedule's line.
#[derive(Debug, Clone, PartialEq)]
pub struct Seat {
    /// Its device key, as hex; empty for this machine when it shares nothing.
    pub key: String,
    pub name: String,
    pub me: bool,
    /// The latest of the schedule's times it's done with, for another machine.
    pub last_due: Option<i64>,
}

/// The machines that would run a schedule now, in the order they'd take it: of those that are
/// around and can run it (this one when `able_here`), the one it prefers, then first choices before
/// backups, then by key, the same on every machine.
pub fn line(s: &Schedule, machines: &Machines, able_here: bool, now: i64) -> Vec<Seat> {
    let prefer = match &s.runs_on {
        RunsOn::One { prefer } => prefer.as_str(),
        _ => "",
    };
    let Some(me) = &machines.me else {
        // This machine shares nothing: it's the only one there is.
        return able_here.then(|| Seat { key: String::new(), name: "this machine".into(), me: true, last_due: None }).into_iter().collect();
    };
    let mut seats: Vec<(codealong_peer::feed::Rank, Seat)> = Vec::new();
    if able_here {
        seats.push((me.rank, Seat { key: me.key.clone(), name: "this machine".into(), me: true, last_due: None }));
    }
    for m in &machines.others {
        let Some(x) = m.schedules.get(&s.id).filter(|x| x.able && m.around(now)) else { continue };
        seats.push((m.rank, Seat { key: m.key.clone(), name: m.name.clone(), me: false, last_due: x.last_due }));
    }
    seats.sort_by(|(ra, a), (rb, b)| (a.key != prefer || prefer.is_empty(), crate::shared::place(*ra, &a.key)).cmp(&(b.key != prefer || prefer.is_empty(), crate::shared::place(*rb, &b.key))));
    seats.into_iter().map(|(_, seat)| seat).collect()
}

/// Whose turn it is for a time noticed at `noticed_at`: the first in line, then the next once each
/// has had `TAKE_OVER_MS` without taking it.
pub fn turn(line: &[Seat], noticed_at: i64, now: i64) -> Option<&Seat> {
    let k = ((now - noticed_at).max(0) / TAKE_OVER_MS) as usize;
    line.get(k.min(line.len().saturating_sub(1)))
}

/// The other machine that took a run of a schedule's time, or of one after it.
pub fn taken<'a>(machines: &'a Machines, id: &str, due: i64) -> Option<&'a str> {
    machines.others.iter().find(|m| m.schedules.get(id).and_then(|x| x.took).is_some_and(|t| t >= due)).map(|m| m.name.as_str())
}

/// Whether a machine that has a copy of a schedule hasn't been heard from since `since` (this one
/// woke), so what it did meanwhile isn't known here: a connection that outlived the sleep, or
/// one just made, brings its feed a moment later.
pub fn unheard(machines: &Machines, id: &str, since: i64) -> bool {
    machines.others.iter().any(|m| m.schedules.get(id).is_some() && m.fed_at.is_none_or(|t| t < since))
}

/// What the other machines ran a schedule for.
pub fn their_seen<'a>(machines: &'a Machines, id: &str) -> HashSet<&'a str> {
    machines.others.iter().filter_map(|m| m.schedules.get(id)).flat_map(|x| x.seen.iter().map(String::as_str)).collect()
}

/// What the other machines' watches saw of a schedule's.
pub fn their_watches<'a>(machines: &'a Machines, id: &str) -> Vec<&'a Watch> {
    machines.others.iter().filter_map(|m| m.schedules.get(id)?.watch.as_ref()).collect()
}

/// The latest notes another machine has of a schedule.
pub fn their_notes<'a>(machines: &'a Machines, id: &str) -> Option<&'a Notes> {
    machines.others.iter().filter_map(|m| m.schedules.get(id)?.notes.as_ref()).max_by_key(|n| n.at)
}

/// How many runs the other machines made of a schedule on `day`.
pub fn their_runs_on(machines: &Machines, id: &str, day: i64) -> usize {
    machines.others.iter().filter_map(|m| m.schedules.get(id)?.today).filter(|d| d.day == day).map(|d| d.runs as usize).sum()
}

/// The other machines' runs of a schedule, each saying where it ran, with the session it started
/// or sent to as a key from there.
pub fn their_runs(id: &str, machines: &Machines) -> Vec<RunRecord> {
    let mut out = Vec::new();
    for m in &machines.others {
        let Some(x) = m.schedules.get(id) else { continue };
        for v in &x.runs {
            let Ok(mut r) = serde_json::from_value::<RunRecord>(v.clone()) else { continue };
            r.machine = Some(m.name.clone());
            r.session = r.session.map(|k| codealong_peer::feed::remote_key(m.short(), &k));
            r.pending = None;
            out.push(r);
        }
    }
    out
}

/// Where a schedule runs next, in a few words, from its line.
pub fn words(s: &Schedule, line: &[Seat], machines: &Machines) -> String {
    let prefer = match &s.runs_on {
        RunsOn::One { prefer } => prefer.as_str(),
        RunsOn::Every => return "On every machine".into(),
        RunsOn::This => return "On this machine".into(),
    };
    let Some(first) = line.first() else { return "None of your machines can run it now".into() };
    let mine = machines.me.as_ref().is_some_and(|m| m.key == prefer);
    let preferred = (!prefer.is_empty()).then(|| match machines.others.iter().find(|m| m.key == prefer) {
        _ if mine => "this machine".to_string(),
        Some(m) => m.name.clone(),
        None => "the machine it prefers".to_string(),
    });
    match preferred {
        Some(p) if first.key != prefer => format!("Next on {}, in {p}'s place", first.name),
        _ => format!("Next on {}", first.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::{Machine, Me};
    use codealong_peer::feed::Rank;
    use codealong_peer::schedules::Gone;

    const ME: &str = "aa";
    const MINI: &str = "bb";
    const STUDIO: &str = "cc";
    const NOW: i64 = 1_800_000_000_000;

    fn schedule(id: &str, runs_on: RunsOn, updated_at: i64) -> Schedule {
        Schedule { id: id.into(), name: format!("Schedule {id}"), when: When::Time { rule: "weekdays 9:00".into() }, run: Run { prompt: "Look".into(), ..Run::default() }, runs_on, created_at: 1, updated_at, ..Schedule::default() }
    }

    fn one(prefer: &str) -> RunsOn {
        RunsOn::One { prefer: prefer.into() }
    }

    fn copy(s: &Schedule, able: bool) -> SharedSchedule {
        SharedSchedule { id: s.id.clone(), schedule: serde_json::to_value(s).unwrap(), able, ..SharedSchedule::default() }
    }

    fn machine(key: &str, name: &str, rank: Rank, online: bool, list: Vec<SharedSchedule>) -> Machine {
        Machine { key: key.into(), name: name.into(), rank, online, last_seen: online.then_some(NOW), fed_at: online.then_some(NOW), prs: Vec::new(), schedules: Shared { list, gone: Vec::new() } }
    }

    fn machines(rank: Rank, others: Vec<Machine>) -> Machines {
        Machines { me: Some(Me { key: ME.into(), rank }), others }
    }

    #[test]
    fn the_machine_it_prefers_runs_it_while_its_around_and_can() {
        let s = schedule("s-1", one(MINI), 5);
        let mini = |online| machine(MINI, "Mini", Rank::Backup, online, vec![copy(&s, true)]);
        let studio = machine(STUDIO, "Studio", Rank::First, true, vec![copy(&s, true)]);
        let names = |l: Vec<Seat>| l.into_iter().map(|x| x.name).collect::<Vec<_>>();
        // Preferred, Mini goes first though it's a backup; then first choices, then keys.
        assert_eq!(names(line(&s, &machines(Rank::First, vec![mini(true), studio.clone()]), true, NOW)), ["Mini", "this machine", "Studio"]);
        // Away for longer than a moment, it's passed over.
        let away = Machine { last_seen: Some(NOW - 2 * crate::shared::AWAY_AFTER_MS), ..mini(false) };
        assert_eq!(names(line(&s, &machines(Rank::First, vec![away.clone(), studio.clone()]), true, NOW)), ["this machine", "Studio"]);
        // Up, but unable to run it there, it's passed over too; as is this machine when it can't.
        let unable = machine(MINI, "Mini", Rank::First, true, vec![copy(&s, false)]);
        assert_eq!(names(line(&s, &machines(Rank::Backup, vec![unable, studio.clone()]), false, NOW)), ["Studio"]);
        // Without a preference, first choices before backups, then keys.
        let free = schedule("s-1", one(""), 5);
        assert_eq!(names(line(&free, &machines(Rank::Backup, vec![mini(true), studio]), true, NOW)), ["Studio", "this machine", "Mini"]);
    }

    #[test]
    fn a_routing_group_or_auto_with_nothing_on_here_cant_run_it() {
        let z = rule::tests::Fake::utc();
        let account = |name: &str, disabled| crate::config::AccountCfg { name: name.into(), harness: "claude".into(), home: "~/.claude".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled };
        let member = |a: &str| crate::config::GroupMemberCfg { account: a.into(), ..crate::config::GroupMemberCfg::default() };
        let mut cfg = Config::default();
        cfg.accounts = vec![account("claude", true), account("codex", false)];
        cfg.routing.groups = vec![crate::config::RoutingGroupCfg { name: "reviewers".into(), members: vec![member("claude")], ..crate::config::RoutingGroupCfg::default() }];
        let mut s = schedule("s-1", one(""), 5);
        s.run.account = "reviewers".into();
        assert_eq!(able(&s, &Board::default(), &cfg, &z), Err("Every account in reviewers is turned off here.".into()));
        s.run.account = "claude".into();
        assert_eq!(able(&s, &Board::default(), &cfg, &z), Err("claude is turned off here.".into()));
        s.run.account = String::new();
        assert_eq!(able(&s, &Board::default(), &cfg, &z), Ok(()), "Auto has codex");
        // Auto that starts from a routing group goes by its members.
        cfg.routing.auto = "reviewers".into();
        assert_eq!(able(&s, &Board::default(), &cfg, &z), Err("Every account in reviewers is turned off here.".into()));
        cfg.routing.auto = "nobody".into();
        assert_eq!(able(&s, &Board::default(), &cfg, &z), Err("Auto's routing group, nobody, isn't one here.".into()));
        cfg.routing.auto = String::new();
        cfg.accounts[1].disabled = true;
        assert_eq!(able(&s, &Board::default(), &cfg, &z), Err("Every account is turned off here.".into()));
        cfg.accounts.clear();
        assert_eq!(able(&s, &Board::default(), &cfg, &z), Err("No account is set up here.".into()));
    }

    #[test]
    fn says_where_it_runs_next() {
        let s = schedule("s-1", one(MINI), 5);
        let mini = machine(MINI, "Mini", Rank::First, true, vec![copy(&s, true)]);
        let up = machines(Rank::First, vec![mini.clone()]);
        assert_eq!(words(&s, &line(&s, &up, true, NOW), &up), "Next on Mini");
        let down = machines(Rank::First, vec![Machine { online: false, last_seen: None, ..mini }]);
        assert_eq!(words(&s, &line(&s, &down, true, NOW), &down), "Next on this machine, in Mini's place");
        assert_eq!(words(&s, &line(&s, &down, false, NOW), &down), "None of your machines can run it now");
        let mine = schedule("s-1", one(ME), 5);
        assert_eq!(words(&mine, &line(&mine, &up, true, NOW), &up), "Next on this machine");
    }

    #[test]
    fn the_next_in_line_takes_a_time_the_first_hasnt() {
        let seats: Vec<Seat> = ["Mini", "this machine"].iter().map(|n| Seat { key: n.to_string(), name: n.to_string(), me: *n == "this machine", last_due: None }).collect();
        assert_eq!(turn(&seats, NOW, NOW + TAKE_OVER_MS - 1).map(|s| s.name.as_str()), Some("Mini"));
        assert_eq!(turn(&seats, NOW, NOW + TAKE_OVER_MS).map(|s| s.name.as_str()), Some("this machine"));
        // The last in line keeps it.
        assert_eq!(turn(&seats, NOW, NOW + 9 * TAKE_OVER_MS).map(|s| s.name.as_str()), Some("this machine"));
        assert_eq!(turn(&[], NOW, NOW), None);
    }

    #[test]
    fn takes_in_the_newest_copy_from_wherever_it_changed() {
        let mine = schedule("s-1", one(""), 10);
        let file = File { schedules: vec![mine.clone(), schedule("s-local", RunsOn::This, 1)], gone: Vec::new() };
        // An older copy changes nothing; a newer one replaces this one's; a new one is added.
        let older = Schedule { name: "Old".into(), ..schedule("s-1", one(""), 5) };
        assert_eq!(merged(&file, &machines(Rank::First, vec![machine(MINI, "Mini", Rank::First, true, vec![copy(&older, true)])]), NOW), None);
        let newer = Schedule { paused: false, name: "New".into(), ..schedule("s-1", one(MINI), 20) };
        let added = schedule("s-2", RunsOn::Every, 3);
        let m = merged(&file, &machines(Rank::First, vec![machine(MINI, "Mini", Rank::First, true, vec![copy(&newer, true), copy(&added, true)])]), NOW).unwrap();
        assert_eq!(m.file.schedules.iter().map(|s| (s.id.as_str(), s.name.as_str())).collect::<Vec<_>>(), [("s-1", "New"), ("s-local", "Schedule s-local"), ("s-2", "Schedule s-2")]);
        // Resumed there, then renamed: the copy says when it was resumed, which the scheduler
        // goes from.
        let resumed_there = Schedule { resumed_at: Some(15), ..newer.clone() };
        let paused = File { schedules: vec![Schedule { paused: true, ..mine }], gone: Vec::new() };
        let m = merged(&paused, &machines(Rank::First, vec![machine(MINI, "Mini", Rank::First, true, vec![copy(&resumed_there, true)])]), NOW).unwrap();
        assert_eq!(m.file.schedules[0].resumed_at, Some(15));
        // A copy made one machine's there isn't a shared one to take in.
        let theirs = schedule("s-3", RunsOn::This, 30);
        assert_eq!(merged(&file, &machines(Rank::First, vec![machine(MINI, "Mini", Rank::First, true, vec![copy(&theirs, true)])]), NOW), None);
    }

    #[test]
    fn a_schedule_removed_anywhere_goes_everywhere_but_comes_back_when_shared_again() {
        let at = |x: i64| NOW - 1000 + x;
        let file = File { schedules: vec![schedule("s-1", one(""), at(10)), schedule("s-2", one(""), at(10))], gone: Vec::new() };
        let mut mini = machine(MINI, "Mini", Rank::First, false, Vec::new());
        mini.schedules.gone = vec![Gone { id: "s-1".into(), at: at(20) }];
        let m = merged(&file, &machines(Rank::First, vec![mini.clone()]), NOW).unwrap();
        assert_eq!(m.removed, ["s-1"]);
        assert_eq!(m.file.gone, [Gone { id: "s-1".into(), at: at(20) }], "kept, to go on to the others");
        // A copy from before it went, from a machine that hasn't heard, isn't taken in again.
        let studio = machine(STUDIO, "Studio", Rank::First, true, vec![copy(&schedule("s-1", one(""), at(15)), true)]);
        assert_eq!(merged(&m.file, &machines(Rank::First, vec![mini.clone(), studio]), NOW), None);
        // Shared again after, it's back, and what said it went is forgotten.
        let again = machine(STUDIO, "Studio", Rank::First, true, vec![copy(&schedule("s-1", one(""), at(30)), true)]);
        let back = merged(&m.file, &machines(Rank::First, vec![mini, again]), NOW).unwrap();
        assert!(back.file.schedules.iter().any(|s| s.id == "s-1") && back.file.gone.is_empty());
        // Made this machine's own here after it went there, it stays.
        let made_mine = File { schedules: vec![schedule("s-1", RunsOn::This, at(25))], gone: Vec::new() };
        let mut went = machine(MINI, "Mini", Rank::First, true, Vec::new());
        went.schedules.gone = vec![Gone { id: "s-1".into(), at: at(20) }];
        assert_eq!(merged(&made_mine, &machines(Rank::First, vec![went.clone()]), NOW), None);
        // What went a month ago is forgotten, but not while a machine still has a copy from before.
        let old = File { schedules: Vec::new(), gone: vec![Gone { id: "s-7".into(), at: NOW - KEEP_GONE_MS }] };
        assert!(merged(&old, &machines(Rank::First, vec![went]), NOW).unwrap().file.gone.iter().all(|g| g.id != "s-7"));
        let off = machine(STUDIO, "Studio", Rank::First, false, vec![copy(&schedule("s-7", one(""), NOW - 2 * KEEP_GONE_MS), true)]);
        assert_eq!(merged(&old, &machines(Rank::First, vec![off]), NOW), None, "kept, and the copy not taken in");
        // Nor while a machine hasn't been heard from since it went, whose copy may not be in its
        // feed yet.
        let quiet = Machine { last_seen: Some(NOW), fed_at: Some(NOW - 2 * KEEP_GONE_MS), ..machine(STUDIO, "Studio", Rank::First, false, Vec::new()) };
        assert_eq!(merged(&old, &machines(Rank::First, vec![quiet]), NOW), None);
    }

    #[test]
    fn tells_the_others_what_the_next_machine_goes_on_from() {
        let mut s = schedule("s-1", one(""), 10);
        s.run.folder = crate::util::home().join("projects/northwind").to_string_lossy().into_owned();
        s.notes = true;
        let track = Track { took: Some(900), last_due: Some(900), seen: (0..400).map(|i| i.to_string()).collect(), notes: Some(KeptNotes { text: "Known noise: none".into(), at: 50, mtime: 50 }), ..Track::default() };
        let runs = Runs { of: [("s-1".to_string(), track)].into(), ..Runs::default() };
        let file = File { schedules: vec![s, schedule("s-local", RunsOn::This, 1)], gone: vec![Gone { id: "s-9".into(), at: 7 }] };
        let out = outgoing(&file, &runs, &Board::default(), &Config::default(), &rule::tests::Fake::utc());
        assert_eq!(out.list.len(), 1, "only the shared ones");
        let x = &out.list[0];
        assert_eq!(x.schedule["run"]["folder"], "~/projects/northwind");
        // Its worktree here says which repo it's a checkout of, for a machine laid out otherwise.
        let here = crate::util::home().join("projects/northwind").to_string_lossy().into_owned();
        let lane = crate::model::Lane { id: here.clone(), path: here, repo_slug: Some("acme/northwind".into()), ..crate::fixtures::lane("/w", Vec::new()) };
        let with = outgoing(&file, &runs, &Board { lanes: vec![lane], ..Board::default() }, &Config::default(), &rule::tests::Fake::utc());
        assert_eq!(with.list[0].schedule["checkout"], serde_json::json!({ "repo": "acme/northwind" }));
        assert_eq!((x.took, x.seen.len(), x.seen.last().map(String::as_str)), (Some(900), SHARE_SEEN, Some("399")));
        assert_eq!(x.notes.as_ref().map(|n| n.text.as_str()), Some("Known noise: none"));
        // Its folder isn't a worktree this board knows: it can't run here.
        assert!(!x.able && x.why_not.as_deref().is_some_and(|w| w.contains("isn't a worktree")));
        assert_eq!(out.gone, file.gone);
    }

    #[test]
    fn the_view_lists_the_others_runs_among_its_own() {
        let s = schedule("s-1", one(""), 10);
        let run = RunRecord { at: 5, why: "9:00".into(), session: Some("claude:claude:9f".into()), ..RunRecord::default() };
        let mut x = copy(&s, true);
        x.runs = vec![serde_json::to_value(&run).unwrap()];
        let board = Board { machines: std::sync::Arc::new(machines(Rank::First, vec![machine("bbbbbbbbbbbb", "Mini", Rank::First, true, vec![x])])), ..Board::default() };
        let mine = Track { runs: vec![RunRecord { at: 9, why: "Run now".into(), ..RunRecord::default() }], ..Track::default() };
        let runs = Runs { of: [("s-1".to_string(), mine)].into(), ..Runs::default() };
        let items = items(&File { schedules: vec![s], gone: Vec::new() }, &runs, &board, &Config::default(), NOW, &rule::tests::Fake::utc());
        let listed: Vec<(i64, Option<&str>, Option<&str>)> = items[0].runs.iter().map(|r| (r.at, r.machine.as_deref(), r.session.as_deref())).collect();
        assert_eq!(listed, [(9, None, None), (5, Some("Mini"), Some("@bbbbbbbbbb:claude:claude:9f"))]);
    }

    #[test]
    fn shows_the_others_runs_with_where_they_ran() {
        let s = schedule("s-1", one(""), 10);
        let run = RunRecord { at: 5, why: "9:00".into(), session: Some("claude:claude:9f".into()), ..RunRecord::default() };
        let mut x = copy(&s, true);
        x.runs = vec![serde_json::to_value(&run).unwrap()];
        let ms = machines(Rank::First, vec![machine("bbbbbbbbbbbb", "Mini", Rank::First, true, vec![x])]);
        let theirs = their_runs("s-1", &ms);
        assert_eq!((theirs[0].machine.as_deref(), theirs[0].session.as_deref()), (Some("Mini"), Some("@bbbbbbbbbb:claude:claude:9f")));
    }
}

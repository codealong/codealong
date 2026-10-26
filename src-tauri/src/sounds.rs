//! A short sound when something starts to wait on you, to get your attention while you're looking
//! somewhere else: a session that needs you (it asks you something, waits on a call to allow, or
//! stopped with an error), a session whose turn ended, or a PR that asks for your review. Each has
//! its sound in config.toml (`[sounds]`, "" for none): one of the OS's, or one you put in your own
//! sounds folder, at the volume there (os::sound). Notification Center banners are left out on purpose:
//! they need a permission, pile up, and say again what the bar's mark and the notch show already
//! (src/mark.ts). The sound is for when you aren't looking at those.
//!
//! What came is read from two bar feeds in a row, as the mark's knock and ripple are (mark.ts
//! `happened`), so a sound goes with what the bar shows: this Mac's sessions and, with `[devices]
//! in_bar` on, those of your other devices that are connected (each of them sounds too, so you
//! hear it wherever you are), and of those not a snoozed one, the end of a quieted one's turn, a
//! helper that doesn't need you, or one in a worktree kept out of sight. And to stay out of the
//! way:
//!
//! - nothing sounds for what's there already at launch, nor for the reviews GitHub's first poll
//!   brings, nor for what another device brings as it connects again;
//! - a session that comes to wait on you within `WATCHED_MS` of going back to work makes none:
//!   you just sent it something or answered it, so you're likely still looking at it;
//! - one sound at most every `REST_MS`: a burst, as when the Mac wakes, is one sound, the most
//!   urgent's.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::barfeed::{BarFeed, Group};
use crate::config::SoundsCfg;
use crate::model::Board;
use crate::os;

/// A session that comes to wait on you this soon after it went back to work is one you're likely
/// still watching: the turn you just started, or the call you just allowed.
const WATCHED_MS: i64 = 15_000;
/// The least time between two sounds.
const REST_MS: i64 = 10_000;

/// What starts to wait on you, the most urgent first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    NeedsYou,
    ReviewRequested,
    YourTurn,
}

impl Kind {
    /// The sound config.toml gives it; empty for none.
    pub fn sound(self, cfg: &SoundsCfg) -> &str {
        match self {
            Kind::NeedsYou => cfg.needs_you.trim(),
            Kind::ReviewRequested => cfg.review_requested.trim(),
            Kind::YourTurn => cfg.your_turn.trim(),
        }
    }
}

/// What waited on you on a feed, by kind, and what was running.
#[derive(Debug, Default)]
struct Waiting {
    needs: HashSet<String>,
    turn: HashSet<String>,
    working: HashSet<String>,
    /// None until GitHub's first poll has ended (`Board::github_polled`): until then the board
    /// has only the reviews its answers so far brought, and the rest coming in isn't news.
    reviews: Option<HashSet<String>>,
    /// The other devices whose sessions the feed listed (`BarFeed::devices`).
    devices: HashSet<String>,
}

fn waiting(feed: &BarFeed, reviews_read: bool) -> Waiting {
    let mut w = Waiting::default();
    for i in &feed.items {
        let shown = !i.hidden;
        match i.group {
            Group::Needs if shown => w.needs.insert(i.key.clone()),
            Group::Unread if shown => w.turn.insert(i.key.clone()),
            Group::Working => w.working.insert(i.key.clone()),
            _ => false,
        };
    }
    w.reviews = reviews_read.then(|| feed.reviews.iter().map(|r| r.key.clone()).collect());
    w.devices = feed.devices.iter().cloned().collect();
    w
}

/// Which sound is due on each board the engine publishes (lib.rs plays it).
#[derive(Default)]
pub struct Chimer {
    /// What waited on you on the last feed; none before the first one that isn't loading.
    was: Option<Waiting>,
    /// When each session went back to work, as far as the feeds show: the later of when its turn
    /// says it began and the first feed that had it running again. Codex keeps a turn's start
    /// through a call you allow in it, which would have it working since long before your answer.
    working_since: HashMap<String, i64>,
    /// When the last sound played.
    last_at: Option<i64>,
}

impl Chimer {
    /// The sound due on a board and its bar feed at `now` (ms), if any: the most urgent kind that
    /// came and has a sound in `cfg`. A kind without one leaves the way clear for the rest. `now`
    /// is the board's time for a board the engine published, and the clock's for your other
    /// devices' worktrees, which come while the board stays as it was.
    pub fn due(&mut self, board: &Board, feed: &BarFeed, cfg: &SoundsCfg, now: i64) -> Option<Kind> {
        if feed.loading {
            return None;
        }
        let now_waiting = waiting(feed, board.github_polled);
        let came = self.was.as_ref().map(|was| self.came(was, &now_waiting, feed)).unwrap_or_default();
        self.remember_work(feed, now);
        self.was = Some(now_waiting);
        let kind = came.into_iter().filter(|k| !k.sound(cfg).is_empty()).min()?;
        if cfg.volume == 0 || self.last_at.is_some_and(|at| now - at < REST_MS) {
            return None;
        }
        self.last_at = Some(now);
        Some(kind)
    }

    /// What came to wait on you between two feeds, less the sessions you're likely watching and
    /// those of a device that wasn't on the feed before.
    fn came(&self, was: &Waiting, now: &Waiting, feed: &BarFeed) -> Vec<Kind> {
        let watched = |key: &str, since: i64| self.working_since.get(key).is_some_and(|&w| since >= w && since - w < WATCHED_MS);
        let back = |key: &str| crate::peerfeed::split_remote(key).is_some_and(|(short, _)| !was.devices.contains(short));
        let mut out = Vec::new();
        for i in feed.items.iter().filter(|i| !i.hidden && !back(&i.key)) {
            let (kind, before) = match i.group {
                Group::Needs => (Kind::NeedsYou, &was.needs),
                Group::Unread => (Kind::YourTurn, &was.turn),
                _ => continue,
            };
            if !before.contains(&i.key) && !watched(&i.key, i.since) {
                out.push(kind);
            }
        }
        if let (Some(before), Some(reviews)) = (&was.reviews, &now.reviews) {
            if reviews.iter().any(|k| !before.contains(k)) {
                out.push(Kind::ReviewRequested);
            }
        }
        out
    }

    /// Notes when each running session went back to work, at `now` (ms), before `was` moves on,
    /// and forgets the ones the bar no longer has.
    fn remember_work(&mut self, feed: &BarFeed, now: i64) {
        let was = self.was.as_ref();
        for i in feed.items.iter().filter(|i| i.group == Group::Working) {
            let start = match (was, self.working_since.get(&i.key)) {
                // Still running: since it went back, or a later turn's start a feed didn't catch.
                (Some(w), Some(&at)) if w.working.contains(&i.key) => at.max(i.since),
                // Back at work since the last feed.
                (Some(_), _) => i.since.max(now),
                // Running at launch, as its turn says.
                (None, _) => i.since,
            };
            self.working_since.insert(i.key.clone(), start);
        }
        let known: HashSet<&str> = feed.items.iter().chain(&feed.recent).chain(&feed.snoozed).map(|i| i.key.as_str()).collect();
        self.working_since.retain(|k, _| known.contains(k.as_str()));
    }
}

/// The sounds that can be named in `[sounds]`, by name, sorted, each once.
pub fn names() -> Vec<String> {
    names_in(&os::sound::folders())
}

fn names_in(folders: &[PathBuf]) -> Vec<String> {
    let mut out: Vec<String> = folders
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| os::sound::is_sound(p))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(String::from))
        .collect();
    out.sort_by_key(|n| n.to_lowercase());
    out.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    out
}

/// Why `[sounds] <key> = name` can't be played, if it can't.
pub fn check(key: &str, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || names().iter().any(|n| n.eq_ignore_ascii_case(name)) {
        return Ok(());
    }
    // A default the OS may not have (a Linux without the sound theme) plays nothing rather than
    // refusing every setting saved.
    let (needs_you, your_turn) = os::sound::defaults();
    if os::sound::default_may_be_missing() && [needs_you, your_turn].contains(&name) {
        return Ok(());
    }
    Err(format!("[sounds] {key} is {name:?}, which isn't in {}; use the name of a sound there, or \"\" for none.", os::sound::folders().iter().map(|p| crate::util::tilde(p)).collect::<Vec<_>>().join(", ")))
}

pub use crate::os::sound::play;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::barfeed::feed;
    use crate::model::{Lane, PrItem, Session, Status};

    const NOW: i64 = 1_790_200_000_000;

    fn cfg() -> SoundsCfg {
        SoundsCfg { needs_you: "Glass".into(), your_turn: "Pop".into(), review_requested: "Tink".into(), volume: 60 }
    }

    fn session(key: &str, status: Status, since: i64) -> Session {
        Session { status, status_since: Some(since), last_activity: since, last_prompt_at: since, ..crate::fixtures::session(key) }
    }

    /// The board at `at`, once GitHub's first poll has ended.
    fn board_at(at: i64, sessions: Vec<Session>) -> Board {
        Board { lanes: vec![crate::fixtures::lane("/a", sessions)], generated_at: at, github_polled: true, ..Board::default() }
    }

    fn board(sessions: Vec<Session>) -> Board {
        board_at(NOW, sessions)
    }

    fn review(n: u64) -> PrItem {
        serde_json::from_value(serde_json::json!({
            "key": format!("o/r#{n}"), "repoSlug": "o/r", "repo": "r", "project": "P", "number": n, "title": format!("PR {n}"),
            "url": "", "state": "open", "author": null, "mine": false, "reviewRequested": true, "requestedBy": null, "head": "h", "base": null,
            "checks": { "passed": 0, "failed": 0, "pending": 0, "skipped": 0, "failing": [] }, "merge": "clean",
            "additions": 0, "deletions": 0, "changedFiles": 0, "createdAt": 0, "updatedAt": NOW,
            "mergedAt": null, "closedAt": null, "lanes": [], "sessions": []
        }))
        .expect("a PR")
    }

    fn due(c: &mut Chimer, b: &Board, cfg: &SoundsCfg) -> Option<Kind> {
        c.due(b, &feed(b, &[]), cfg, b.generated_at)
    }

    /// The chimer after a first board of `sessions` an hour before `NOW`, which never sounds.
    fn after(sessions: Vec<Session>) -> Chimer {
        let mut c = Chimer::default();
        assert_eq!(due(&mut c, &board_at(NOW - 3_600_000, sessions), &cfg()), None);
        c
    }

    #[test]
    fn nothing_sounds_for_what_waits_at_launch_or_while_loading() {
        let mut c = Chimer::default();
        let mut loading = board(vec![session("a", Status::Needs, NOW - 60_000)]);
        loading.loading = true;
        assert_eq!(due(&mut c, &loading, &cfg()), None);
        assert_eq!(due(&mut c, &board(vec![session("a", Status::Needs, NOW - 60_000)]), &cfg()), None);
        // Still waiting on the next board is no news either.
        assert_eq!(due(&mut c, &board_at(NOW + 1_000, vec![session("a", Status::Needs, NOW - 60_000)]), &cfg()), None);
    }

    #[test]
    fn a_session_that_comes_to_need_you_or_ends_its_turn_sounds() {
        let mut c = after(vec![session("a", Status::Working, NOW - 3_600_000)]);
        assert_eq!(due(&mut c, &board(vec![session("a", Status::Needs, NOW)]), &cfg()), Some(Kind::NeedsYou));
        let mut c = after(vec![session("a", Status::Working, NOW - 3_600_000)]);
        assert_eq!(due(&mut c, &board(vec![session("a", Status::Unread, NOW)]), &cfg()), Some(Kind::YourTurn));
        let mut c = after(vec![session("a", Status::Working, NOW - 3_600_000)]);
        assert_eq!(due(&mut c, &board(vec![session("a", Status::Error, NOW)]), &cfg()), Some(Kind::NeedsYou));
        // One the bar hadn't seen before, as a helper that comes to need you.
        let mut c = after(vec![]);
        assert_eq!(due(&mut c, &board(vec![session("new", Status::Needs, NOW)]), &cfg()), Some(Kind::NeedsYou));
    }

    #[test]
    fn a_session_you_just_answered_makes_no_sound() {
        // It went back to work 5 seconds before it asked again: you're still looking at it.
        let mut c = after(vec![session("a", Status::Needs, NOW - 3_600_000)]);
        assert_eq!(due(&mut c, &board_at(NOW - 5_000, vec![session("a", Status::Working, NOW - 5_000)]), &cfg()), None);
        assert_eq!(due(&mut c, &board(vec![session("a", Status::Needs, NOW)]), &cfg()), None);
        let mut c = after(vec![session("a", Status::Unread, NOW - 3_600_000)]);
        assert_eq!(due(&mut c, &board_at(NOW - 5_000, vec![session("a", Status::Working, NOW - 5_000)]), &cfg()), None);
        assert_eq!(due(&mut c, &board(vec![session("a", Status::Unread, NOW)]), &cfg()), None);
        // A turn as long as that sounds.
        let mut c = after(vec![session("a", Status::Unread, NOW - 3_600_000)]);
        assert_eq!(due(&mut c, &board_at(NOW - WATCHED_MS, vec![session("a", Status::Working, NOW - WATCHED_MS)]), &cfg()), None);
        assert_eq!(due(&mut c, &board(vec![session("a", Status::Unread, NOW)]), &cfg()), Some(Kind::YourTurn));
    }

    #[test]
    fn going_back_to_work_counts_from_when_a_board_saw_it_not_the_turns_start() {
        // A Codex turn keeps its start through a call you allow: back at work 5 seconds before it
        // ended, it began an hour ago.
        let turn = NOW - 3_600_000;
        let mut c = after(vec![session("a", Status::Working, turn)]);
        assert_eq!(due(&mut c, &board_at(NOW - 60_000, vec![session("a", Status::Needs, NOW - 60_000)]), &cfg()), Some(Kind::NeedsYou));
        assert_eq!(due(&mut c, &board_at(NOW - 5_000, vec![session("a", Status::Working, turn)]), &cfg()), None);
        assert_eq!(due(&mut c, &board(vec![session("a", Status::Unread, NOW)]), &cfg()), None);
        // Running on, it still counts from when it went back.
        let mut c = after(vec![session("a", Status::Needs, turn)]);
        assert_eq!(due(&mut c, &board_at(NOW - 60_000, vec![session("a", Status::Working, turn)]), &cfg()), None);
        assert_eq!(due(&mut c, &board_at(NOW - 30_000, vec![session("a", Status::Working, turn)]), &cfg()), None);
        assert_eq!(due(&mut c, &board(vec![session("a", Status::Unread, NOW)]), &cfg()), Some(Kind::YourTurn));
    }

    #[test]
    fn a_burst_is_one_sound_the_most_urgent() {
        let mut c = after(vec![session("a", Status::Working, NOW - 3_600_000), session("b", Status::Working, NOW - 3_600_000)]);
        let both = board(vec![session("a", Status::Unread, NOW), session("b", Status::Needs, NOW)]);
        assert_eq!(due(&mut c, &both, &cfg()), Some(Kind::NeedsYou));
        // Another that comes within the rest is quiet; after it, it sounds.
        let mut more = Board { generated_at: NOW + 2_000, ..both.clone() };
        more.lanes[0].sessions.push(session("c", Status::Unread, NOW + 2_000));
        assert_eq!(due(&mut c, &more, &cfg()), None);
        more.generated_at = NOW + REST_MS;
        more.lanes[0].sessions.push(session("d", Status::Unread, NOW + REST_MS));
        assert_eq!(due(&mut c, &more, &cfg()), Some(Kind::YourTurn));
    }

    #[test]
    fn a_kind_without_a_sound_leaves_the_way_clear_and_volume_zero_is_silent() {
        let quiet_turns = SoundsCfg { your_turn: String::new(), ..cfg() };
        let mut c = after(vec![session("a", Status::Working, NOW - 3_600_000), session("b", Status::Working, NOW - 3_600_000)]);
        assert_eq!(due(&mut c, &board(vec![session("a", Status::Unread, NOW), session("b", Status::Working, NOW - 3_600_000)]), &quiet_turns), None);
        // The turn that made no sound doesn't hold back the one that comes to need you.
        assert_eq!(due(&mut c, &board_at(NOW + 1_000, vec![session("a", Status::Unread, NOW), session("b", Status::Needs, NOW + 1_000)]), &quiet_turns), Some(Kind::NeedsYou));
        let mut c = after(vec![session("a", Status::Working, NOW - 3_600_000)]);
        assert_eq!(due(&mut c, &board(vec![session("a", Status::Needs, NOW)]), &SoundsCfg { volume: 0, ..cfg() }), None);
    }

    #[test]
    fn a_pr_that_asks_for_your_review_sounds_once_github_has_said_which_already_do() {
        // Before GitHub's first poll ends, the reviews its answers bring, in whatever order, are
        // no news (ghpoll.rs `it_has_polled_only_once_a_poll_ends_whatever_answered_first`).
        let unread = |prs: Vec<PrItem>| Board { prs, github_polled: false, ..board(vec![]) };
        let mut c = after(vec![]);
        assert_eq!(due(&mut c, &unread(vec![review(1)]), &cfg()), None);
        assert_eq!(due(&mut c, &Board { prs: vec![review(1), review(2)], ..board(vec![]) }, &cfg()), None);
        // After it, one more does.
        let three = Board { prs: vec![review(1), review(2), review(3)], ..board_at(NOW + 1_000, vec![]) };
        assert_eq!(due(&mut c, &three, &cfg()), Some(Kind::ReviewRequested));
        assert_eq!(due(&mut c, &Board { generated_at: NOW + REST_MS * 2, ..three }, &cfg()), None);
    }

    #[test]
    fn what_the_bar_leaves_out_or_keeps_out_of_sight_makes_no_sound() {
        let mut c = after(vec![session("snoozed", Status::Working, NOW - 3_600_000), session("hidden", Status::Working, NOW - 3_600_000)]);
        let snoozed = Session { snoozed_until: Some(NOW + 60_000), ..session("snoozed", Status::Needs, NOW) };
        let mut b = board(vec![snoozed]);
        b.lanes.push(Lane { hidden: true, ..crate::fixtures::lane("/elsewhere", vec![session("hidden", Status::Needs, NOW)]) });
        assert_eq!(due(&mut c, &b, &cfg()), None);
        // A quiet session's turn that ended is idle on the board: nothing comes.
        let mut quiet = session("q", Status::Working, NOW - 3_600_000);
        let mut c = after(vec![quiet.clone()]);
        quiet.status = Status::Unread;
        crate::board::apply_quiet(&mut quiet, true);
        assert_eq!(due(&mut c, &board(vec![quiet]), &cfg()), None);
    }

    #[test]
    fn another_devices_session_sounds_but_not_what_it_brings_as_it_connects_again() {
        let there = |online: bool, sessions: Vec<Session>| {
            let tag = crate::model::DeviceTag { name: "Mac mini".into(), short: "abc".into(), color: "teal".into(), online, last_seen: None };
            vec![Lane { device: Some(tag), ..crate::fixtures::lane("@abc:/a", sessions) }]
        };
        let on = |at: i64| Board { devices: crate::config::DevicesCfg { enabled: true, ..Default::default() }, ..board_at(at, vec![]) };
        let (mut c, mut f) = (Chimer::default(), crate::barfeed::Feeder::default());
        let mut due = |b: &Board, lanes: &[Lane]| {
            f.on_there(lanes);
            c.due(b, &f.on_board(b), &cfg(), b.generated_at)
        };
        let (s1, s2) = ("@abc:claude:claude:s1", "@abc:claude:claude:s2");
        assert_eq!(due(&on(NOW - 3_600_000), &there(true, vec![session(s1, Status::Working, NOW - 3_600_000)])), None);
        assert_eq!(due(&on(NOW), &there(true, vec![session(s1, Status::Needs, NOW)])), Some(Kind::NeedsYou));
        // Away, it's off the bar; back, what it brings isn't news.
        assert_eq!(due(&on(NOW + REST_MS), &there(false, vec![session(s1, Status::Needs, NOW)])), None);
        assert_eq!(due(&on(NOW + 2 * REST_MS), &there(true, vec![session(s1, Status::Unread, NOW + REST_MS), session(s2, Status::Working, NOW - 3_600_000)])), None);
        // What comes after that is.
        let next = there(true, vec![session(s1, Status::Unread, NOW + REST_MS), session(s2, Status::Unread, NOW + 4 * REST_MS)]);
        assert_eq!(due(&on(NOW + 4 * REST_MS), &next), Some(Kind::YourTurn));
    }

    #[test]
    fn lists_sound_files_by_name_once_each() {
        let root = std::env::temp_dir().join(format!("codealong-sounds-{}", std::process::id()));
        let (mine, macos) = (root.join("mine"), root.join("macos"));
        for d in [&mine, &macos] {
            std::fs::create_dir_all(d).unwrap();
        }
        for f in [mine.join("Chirp.wav"), mine.join("glass.aiff"), mine.join("chime.oga"), mine.join("notes.txt"), macos.join("Glass.aiff"), macos.join("Pop.AIFF")] {
            std::fs::write(&f, b"").unwrap();
        }
        let folders = [mine.clone(), macos.clone(), root.join("missing")];
        // macOS plays AIFF, Linux Ogg; both WAV.
        assert_eq!(os::as_if(os::Os::MacOs, || names_in(&folders)), vec!["Chirp", "glass", "Pop"]);
        assert_eq!(os::as_if(os::Os::Linux, || names_in(&folders)), vec!["chime", "Chirp"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_sounds_are_found_and_others_are_refused() {
        assert!(check("needs_you", "Glass").is_ok());
        assert!(check("needs_you", "").is_ok());
        let e = check("your_turn", "Kazoo").unwrap_err();
        assert!(e.starts_with("[sounds] your_turn is \"Kazoo\", which isn't in ~/Library/Sounds"), "{e}");
    }
}

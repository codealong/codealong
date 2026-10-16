//! The engine's side of GitHub: what it has read of each repo's PRs, when it asks again and
//! for what, and what GitHub calls each repo now. github.rs talks to `gh`; this decides when
//! it's asked, keeps the answers and says what went wrong. It holds no part of the engine:
//! the worktrees and the time come in with each call, and what it asks in the background comes
//! back as an `Event` for the engine to hand to `on_event`. So its rules are tested with
//! made-up clocks and without `gh`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::board::{self, LaneSeed};
use crate::github::{self, Failure, Glance, Points, PrRaw, RepoPrs, Requested, Stamp};
use crate::model::{Notice, PrState};
use crate::throttle;
use crate::util::now_ms;

/// GitHub is polled this often.
const EVERY: Duration = Duration::from_secs(60);
/// A poll a window asks for isn't sent this soon after the last one.
const ASKED_LATELY: Duration = Duration::from_secs(20);
/// How long GitHub is left alone once it says the account's rate limit is used up, when no
/// glance before said when the hour ends (`Schedule::refused`). A refusal doesn't say, and
/// asking again costs nothing.
const HOLD: Duration = Duration::from_secs(10 * 60);
/// How long before a repo GitHub didn't give the name of is asked about again.
const NAME_RETRY: Duration = Duration::from_secs(60 * 60);
/// What a glance's failure is kept under, beside each repo's slug and the search's
/// `github::REQUESTED`.
const GLANCE: &str = "glance";

/// The time, both ways it's told here: for waits, and as GitHub's answers tell it, in
/// milliseconds. Every rule takes it rather than reading a clock, so tests make it up.
#[derive(Debug, Clone, Copy)]
pub struct Now {
    pub at: Instant,
    pub ms: i64,
}

impl Now {
    pub fn real() -> Now {
        Now { at: Instant::now(), ms: now_ms() }
    }
}

/// What comes back from the background, for `GhPoll::on_event`.
pub enum Event {
    /// What a poll saw at a glance, before its answers.
    Glance(Result<Glance, Failure>),
    /// A repo's PRs and what they were read at.
    Repo(String, Result<RepoPrs, Failure>, Option<Stamp>),
    /// The PRs that wait on your review anywhere, from the same poll.
    Requested(Result<Requested, Failure>, Option<Stamp>),
    /// The poll is over, however it ended: everything it had to say has been sent.
    PollEnded,
    /// What GitHub calls each repo now, by the slug its remote has (lowercase); None where
    /// GitHub didn't answer. The asking is over: it comes empty when it fell over.
    RepoNames(Vec<(String, Option<String>)>),
}

impl From<github::Answer> for Event {
    fn from(answer: github::Answer) -> Event {
        match answer {
            github::Answer::Glance(seen) => Event::Glance(seen),
            github::Answer::Repo(slug, result, stamp) => Event::Repo(slug, result, stamp),
            github::Answer::Requested(result, stamp) => Event::Requested(result, stamp),
        }
    }
}

/// How events get back to whoever owns the `GhPoll`, from any thread.
pub type Back = Arc<dyn Fn(Event) + Send + Sync>;

/// When GitHub is polled, and how thriftily: what's on its way, what each answer was read at,
/// and what GitHub has said of the account's points.
#[derive(Default)]
struct Schedule {
    /// A poll is on its way.
    inflight: bool,
    /// Poll again once the one on its way returns (`refetch`).
    again: bool,
    /// When the latest poll was sent: the one on its way is the latest.
    last: Option<Instant>,
    /// What each repo's PRs were read at, by slug, and the search's under `github::REQUESTED`:
    /// a poll reads again only those whose glance has changed since (github.rs).
    stamps: HashMap<String, Stamp>,
    /// The next poll reads everything, whatever its glance says (`refetch`).
    all: bool,
    /// The GraphQL points GitHub said were left, at its glances, for each hour of the account's
    /// that hasn't ended (`github::Points`): the fewest are what's left.
    points: Vec<Points>,
    /// GitHub said the account's rate limit is used up: it's left alone until then.
    hold: Option<Instant>,
    /// When the hour of that rate limit ends, where GitHub or a glance before said, for the
    /// notice.
    resets: Option<i64>,
}

impl Schedule {
    /// Whether the regular poll is due: a minute after the last was sent, or none has been.
    fn due(&self, now: Instant) -> bool {
        self.last.is_none_or(|t| now.duration_since(t) >= EVERY)
    }

    /// Whether a poll may be sent now: none is on its way, GitHub isn't being left alone, and
    /// unless it's the engine's own (`force`), the last wasn't sent a moment ago.
    fn may_start(&self, force: bool, now: Instant) -> bool {
        if self.inflight {
            return false;
        }
        if !force && self.last.is_some_and(|t| now.duration_since(t) < ASKED_LATELY) {
            return false;
        }
        !self.hold.is_some_and(|until| now < until)
    }

    /// A poll is sent: what it goes by, for these repos, `busy` being those whose checks run.
    fn start(&mut self, slugs: Vec<String>, busy: BTreeSet<String>, now: Now) -> github::Ask {
        self.inflight = true;
        self.last = Some(now.at);
        github::Ask { slugs, have: self.stamps.clone(), busy, all: std::mem::take(&mut self.all), thrifty: self.thrifty(now.ms) }
    }

    /// Few points are left in an hour that hasn't ended: only a changed glance gets a repo read.
    fn thrifty(&self, now_ms: i64) -> bool {
        self.points.iter().any(|p| p.resets > now_ms && p.left < github::RESERVE)
    }

    /// The poll on its way is over. True when another was asked for meanwhile, which goes now.
    fn ended(&mut self) -> bool {
        self.inflight = false;
        std::mem::take(&mut self.again)
    }

    /// Something you just did on GitHub is read in full: its glance may not have moved yet.
    /// True when a poll can go now; else it goes once the one on its way returns, which may
    /// have been answered before the change.
    fn refetch(&mut self) -> bool {
        self.all = true;
        if self.inflight {
            self.again = true;
        }
        !self.inflight
    }

    /// A glance was answered: the points it tells of are noted, and GitHub isn't left alone
    /// any longer.
    fn answered(&mut self, points: Option<Points>, now_ms: i64) {
        if let Some(points) = points {
            self.points.retain(|p| p.resets > now_ms && p.resets != points.resets);
            self.points.push(points);
        }
        self.hold = None;
        self.resets = None;
    }

    /// GitHub refused for want of points: it's left alone until the hour ends, as the refusal
    /// said (`resets`), else as the glances said of the counter that was nearly spent, or for
    /// `HOLD` when nothing said.
    fn refused(&mut self, resets: Option<i64>, now: Now) {
        let spent = || self.points.iter().filter(|p| p.left < github::RESERVE).map(|p| p.resets).min();
        self.resets = resets.or_else(spent).filter(|at| *at > now.ms).or(self.resets.filter(|at| *at > now.ms));
        let wait = self.resets.map_or(HOLD, |at| Duration::from_millis((at - now.ms) as u64) + Duration::from_secs(5));
        self.hold = Some(now.at + wait);
    }
}

/// What GitHub calls each repo now: asked once a launch for each repo, and again after an hour
/// for one it didn't answer for.
#[derive(Default)]
struct Names {
    /// Slug a remote has (lowercase) → what GitHub calls the repo now (`github::current_slug`).
    now: HashMap<String, String>,
    /// When each slug was last asked about, so one GitHub didn't answer is asked again later.
    asked: HashMap<String, Instant>,
    inflight: bool,
}

impl Names {
    /// The repos to ask GitHub about now, of those with these slugs, marked as asked. None
    /// while an asking is on its way, or when there's nothing to ask.
    fn to_ask<'a>(&mut self, slugs: impl Iterator<Item = &'a String>, now: Instant) -> Option<BTreeSet<String>> {
        if self.inflight {
            return None;
        }
        let slugs: BTreeSet<String> = slugs
            .map(|slug| slug.to_lowercase())
            .filter(|slug| !self.now.contains_key(slug))
            .filter(|slug| self.asked.get(slug).is_none_or(|t| now.duration_since(*t) >= NAME_RETRY))
            .collect();
        if slugs.is_empty() {
            return None;
        }
        for slug in &slugs {
            self.asked.insert(slug.clone(), now);
        }
        self.inflight = true;
        Some(slugs)
    }

    /// The asking is over, with what it found. True when a repo isn't called what its remote
    /// says, which the board shows.
    fn answered(&mut self, names: Vec<(String, Option<String>)>) -> bool {
        self.inflight = false;
        let mut renamed = false;
        for (slug, name) in names {
            let Some(name) = name else { continue };
            renamed |= !name.eq_ignore_ascii_case(&slug);
            self.now.insert(slug, name);
        }
        renamed
    }
}

/// GitHub as the engine has it. The engine calls `poll`, `tick` and the `on_*` methods, each
/// with the worktrees and the time, and publishes when one returns true; `compose` reads the
/// rest.
pub struct GhPoll {
    back: Back,
    prs: HashMap<String, Vec<PrRaw>>,
    /// The open PRs GitHub's search says wait on your review, by the slug a project's repo has
    /// here: the board lists them for the repos `prs` doesn't read (`board::pr_items`).
    requested: HashMap<String, Vec<PrRaw>>,
    /// What went wrong, by what was asked (`GLANCE`, a repo's slug, `github::REQUESTED`): the
    /// repos and the search are read at once, so each failure stays until what failed answers
    /// again, whatever else answers meanwhile.
    failures: BTreeMap<String, Failure>,
    /// What the poll on its way has answered for, and whether its glance was answered: at its
    /// end, what a poll answered at a glance didn't read stands as it was, with nothing wrong.
    heard: (bool, HashSet<String>),
    /// PRs you approved here (slug, number) and when, until GitHub is asked after it.
    approved_here: HashMap<(String, u64), Instant>,
    /// PRs you merged here (slug, number), when, and when by the clock, until GitHub is asked
    /// after it.
    merged_here: HashMap<(String, u64), (Instant, i64)>,
    /// When GitHub last answered.
    at: Option<i64>,
    /// When the poll on its way was sent, by the clock, and the repos it asks about.
    asked_ms: Option<i64>,
    asked_slugs: Vec<String>,
    /// When each repo's PRs here were last as GitHub had them, by slug: the time a poll was sent
    /// that read them, or whose glance found them unchanged. A repo whose read fails, or is still
    /// on its way, keeps its earlier time.
    current: HashMap<String, i64>,
    /// Its search for the PRs that wait on your review has answered, once at least.
    searched: bool,
    /// A poll has ended since the search first answered, so every answer that poll brought is
    /// in: the search and each repo's read, which come at once and in any order.
    polled: bool,
    login: Option<String>,
    schedule: Schedule,
    names: Names,
}

/// A PR as the board shows it once you've merged it here: merged then, and waiting on no review.
fn shown_merged(pr: &mut PrRaw, ms: i64) {
    if pr.state != PrState::Merged {
        pr.state = PrState::Merged;
        pr.merged_at = Some(ms);
        pr.closed_at = Some(ms);
        pr.updated_at = pr.updated_at.max(ms);
    }
    pr.review_requested = false;
}

impl GhPoll {
    pub fn new(back: Back) -> GhPoll {
        GhPoll {
            back,
            prs: HashMap::new(),
            requested: HashMap::new(),
            failures: BTreeMap::new(),
            heard: (false, HashSet::new()),
            approved_here: HashMap::new(),
            merged_here: HashMap::new(),
            at: None,
            asked_ms: None,
            asked_slugs: Vec::new(),
            current: HashMap::new(),
            searched: false,
            polled: false,
            login: None,
            schedule: Schedule::default(),
            names: Names::default(),
        }
    }

    /// Each polled repo's PRs, by slug.
    pub fn prs(&self) -> &HashMap<String, Vec<PrRaw>> {
        &self.prs
    }

    /// The PRs that wait on your review, by the slug their repo has here.
    pub fn requested(&self) -> &HashMap<String, Vec<PrRaw>> {
        &self.requested
    }

    /// When GitHub last answered.
    pub fn at(&self) -> Option<i64> {
        self.at
    }

    /// Whether a poll has ended since GitHub's search for the PRs that wait on your review first
    /// answered. Before that, the PRs here are what's come so far: a review requested in a repo
    /// whose read hasn't come isn't here yet, though the search's answer has come
    /// (`board::pr_items` lists a read repo's from its read), and a poll that failed brought
    /// none. A repo whose read fails then is left to bring its reviews when it answers.
    pub fn polled(&self) -> bool {
        self.polled
    }

    /// When each repo's PRs here were last as GitHub had them, by slug (`current`), with the
    /// repos polls ask about that it hasn't been yet.
    pub fn current(&self) -> BTreeMap<String, Option<i64>> {
        let asked = self.asked_slugs.iter().map(|s| (s.clone(), None));
        asked.chain(self.current.iter().map(|(k, v)| (k.clone(), Some(*v)))).collect()
    }

    /// Who `gh` is signed in as.
    pub fn login(&self) -> Option<&str> {
        self.login.as_deref()
    }

    /// Slug a remote has (lowercase) → what GitHub calls the repo now.
    pub fn repo_names(&self) -> &HashMap<String, String> {
        &self.names.now
    }

    /// What GitHub calls a repo now, when that isn't the name its remote has.
    pub fn renamed(&self, slug: &str) -> Option<&String> {
        self.names.now.get(&slug.to_lowercase()).filter(|now| !now.eq_ignore_ascii_case(slug))
    }

    /// What isn't working, as the windows say it: every failure still outstanding, titled by
    /// the one that says most (`gh` missing or signed out explains the rest). A rate limit says
    /// when its hour ends.
    pub fn notice(&self) -> Option<Notice> {
        let rank = |f: &Failure| match f {
            Failure::NotInstalled => 0,
            Failure::SignedOut => 1,
            Failure::RateLimited { .. } => 2,
            Failure::Other(_) => 3,
        };
        let mut failures: Vec<&Failure> = self.failures.values().collect();
        failures.sort_by_key(|f| rank(f));
        let first = *failures.first()?;
        let until = if first.rate_limited() { self.schedule.resets } else { None };
        let mut said: Vec<String> = Vec::new();
        for text in failures.iter().map(|f| f.to_string()) {
            if !said.contains(&text) {
                said.push(text);
            }
        }
        Some(Notice { key: "github".into(), title: first.title().into(), text: said.join(" "), until, ..Default::default() })
    }

    /// What `key` asked brought: a failure is kept until it answers again.
    fn answered(&mut self, key: &str, failure: Option<Failure>) {
        self.heard.1.insert(key.to_string());
        match failure {
            Some(e) => self.failures.insert(key.to_string(), e),
            None => self.failures.remove(key),
        };
    }

    /// Polls GitHub in the background when it may be (`ask`). Each answer comes back as an
    /// event as it lands, and `Event::PollEnded` last.
    pub fn poll(&mut self, seeds: &[LaneSeed], force: bool, now: Now) {
        let Some(ask) = self.ask(seeds, force, now) else { return };
        // A glance at every repo, then one request for each repo it says changed and one
        // for your review requests, all at once; each result is applied as it lands.
        self.run_poll(move |back| github::poll(&ask, &|answer| back(answer.into())));
    }

    /// What a poll sent now goes by, marking it on its way; None when none may be sent, or no
    /// worktree's repo is on GitHub.
    fn ask(&mut self, seeds: &[LaneSeed], force: bool, now: Now) -> Option<github::Ask> {
        if !self.schedule.may_start(force, now.at) || !seeds.iter().any(|s| s.repo_slug.is_some()) {
            return None;
        }
        let busy = self.prs.iter().filter(|(_, prs)| github::busy(prs, now.ms)).map(|(slug, _)| slug.clone()).collect();
        self.heard = (false, HashSet::new());
        self.asked_ms = Some(now.ms);
        let slugs: Vec<String> = board::polled_slugs(seeds).into_iter().collect();
        self.asked_slugs = slugs.clone();
        Some(self.schedule.start(slugs, busy, now))
    }

    /// Runs a poll's work on a thread of its own. `Event::PollEnded` follows it however it
    /// ends, so a poll that falls over doesn't stay on its way for good.
    fn run_poll(&self, work: impl FnOnce(&Back) + Send + 'static) {
        let (back, ended) = (self.back.clone(), self.back.clone());
        throttle::background("codealong-github", move || work(&back), move |_| ended(Event::PollEnded));
    }

    /// The engine's ten-second tick: the regular poll when it's due, and the names of repos
    /// not asked about yet.
    pub fn tick(&mut self, seeds: &[LaneSeed], now: Now) {
        if self.schedule.due(now.at) {
            self.poll(seeds, true, now);
            self.ask_names(seeds, now.at);
        }
    }

    /// Asks GitHub what each repo is called now: once a launch for each repo, and again after
    /// an hour for one it didn't answer for. A worktree carries the answer when it isn't the name
    /// its remote has, so your other devices know a renamed repo for the same one (peers.rs).
    pub fn ask_names(&mut self, seeds: &[LaneSeed], now: Instant) {
        let Some(slugs) = self.names.to_ask(seeds.iter().filter_map(|s| s.repo_slug.as_ref()), now) else { return };
        let back = self.back.clone();
        let ask = move || {
            crate::os::system::utility_thread();
            slugs
                .into_iter()
                .map(|slug| {
                    let name = github::current_slug(&slug);
                    (slug, name)
                })
                .collect::<Vec<_>>()
        };
        // However the asking ends, it's over: one that fell over found nothing.
        throttle::background("codealong-repo-names", ask, move |names| back(Event::RepoNames(names.unwrap_or_default())));
    }

    /// Takes in what came back from the background. True when the board has changed.
    pub fn on_event(&mut self, event: Event, seeds: &[LaneSeed], now: Now) -> bool {
        match event {
            Event::Glance(seen) => self.on_glance(seen, now),
            Event::Repo(slug, result, stamp) => {
                // Current as of when its answer was read from GitHub: a kept one, given while
                // GitHub refuses, is as old as its stamp.
                let asked = self.asked_ms.unwrap_or(now.ms);
                let read_at = result.is_ok().then(|| stamp.as_ref().map_or(asked, |s| s.at.min(asked)));
                self.on_read(&slug, result.as_ref().err(), stamp, now);
                if let Some(at) = read_at {
                    self.current.insert(slug.clone(), at);
                }
                self.on_repo(slug, result, now)
            }
            Event::Requested(result, stamp) => {
                self.on_read(github::REQUESTED, result.as_ref().err(), stamp, now);
                self.on_requested(result, seeds, now)
            }
            Event::PollEnded => {
                // The board says so at once (sounds.rs counts reviews from then).
                let ready = !self.polled && self.search_answered();
                self.polled |= ready;
                // A glance was answered, and what it didn't have read stands: nothing is wrong
                // with that now, and a repo that's no longer polled isn't either.
                let (glanced, heard) = std::mem::take(&mut self.heard);
                let cleared = glanced && {
                    let before = self.failures.len();
                    self.failures.retain(|key, _| heard.contains(key));
                    self.failures.len() != before
                };
                if self.schedule.ended() {
                    self.poll(seeds, true, now);
                }
                cleared || ready
            }
            Event::RepoNames(names) => self.names.answered(names),
        }
    }

    /// The end of the one pass `take` was handed (`--snapshot`), as `Event::PollEnded` ends a
    /// poll, without asking again: the board then says what GitHub had is all in.
    pub fn took_all(&mut self) {
        self.polled |= self.search_answered();
    }

    /// The search for the PRs that wait on your review has answered, and its last answer
    /// didn't fail.
    fn search_answered(&self) -> bool {
        self.searched && !self.failures.contains_key(github::REQUESTED)
    }

    /// One answer of `github::read_once`, for an engine that makes one pass and starts with
    /// nothing (`--snapshot`).
    pub fn take(&mut self, answer: github::Answer, seeds: &[LaneSeed], now: Now) {
        match answer {
            github::Answer::Glance(seen) => {
                self.on_glance(seen, now);
            }
            github::Answer::Repo(slug, Ok(repo), stamp) => {
                self.answered(&slug, None);
                self.current.insert(slug.clone(), stamp.map_or(now.ms, |s| s.at));
                self.prs.insert(slug, repo.prs);
                if repo.login.is_some() {
                    self.login = repo.login;
                }
                self.at = Some(now.ms);
            }
            github::Answer::Repo(slug, Err(e), _) => self.answered(&slug, Some(e)),
            github::Answer::Requested(result, _) => {
                self.on_requested(result, seeds, now);
            }
        }
    }

    /// What a poll saw at a glance: GitHub answered, so the board is as GitHub has it now
    /// whether or not anything is read again.
    fn on_glance(&mut self, seen: Result<Glance, Failure>, now: Now) -> bool {
        match seen {
            Ok(seen) => {
                if seen.login.is_some() {
                    self.login = seen.login;
                }
                self.schedule.answered(seen.points, now.ms);
                // A repo whose mark is the one it was read at hasn't changed since.
                let asked = self.asked_ms.unwrap_or(now.ms);
                for (slug, mark) in &seen.repos {
                    if mark.as_ref().is_some_and(|m| self.schedule.stamps.get(slug).is_some_and(|s| &s.glance == m)) && self.prs.contains_key(slug) {
                        self.current.insert(slug.clone(), asked);
                    }
                }
                // GitHub answered, so a refusal for want of points is over, whatever refused. A
                // repo GitHub doesn't show you says so next, at every poll.
                self.failures.remove(GLANCE);
                self.failures.retain(|_, f| !f.rate_limited());
                self.heard.0 = true;
                self.at = Some(now.ms);
            }
            Err(e) => {
                if let Failure::RateLimited { resets } = e {
                    self.schedule.refused(resets, now);
                }
                self.failures.insert(GLANCE.to_string(), e);
            }
        }
        true
    }

    /// A poll's answer for a repo, or for the search (`github::REQUESTED`): what it was read at
    /// is noted, so the next polls leave it be until its glance changes. A refusal for want of
    /// points holds GitHub off, as one of a glance does.
    fn on_read(&mut self, key: &str, failure: Option<&Failure>, stamp: Option<Stamp>, now: Now) {
        match (failure, stamp) {
            (None, Some(stamp)) => {
                self.schedule.stamps.insert(key.to_string(), stamp);
            }
            (Some(Failure::RateLimited { resets }), _) => self.schedule.refused(*resets, now),
            _ => {}
        }
    }

    /// `gh` has approved a PR: the board stops asking you to review it, and GitHub is asked for
    /// the rest (it may be approved now, and ready to merge). True when the board has changed.
    pub fn on_approved(&mut self, slug: String, number: u64, seeds: &[LaneSeed], now: Now) -> bool {
        let mut dirty = false;
        if let Some(pr) = self.prs.get_mut(&slug).and_then(|prs| prs.iter_mut().find(|p| p.number == number)) {
            pr.review_requested = false;
            dirty = true;
        }
        if let Some(prs) = self.requested.get_mut(&slug) {
            prs.retain(|p| p.number != number);
            dirty = true;
        }
        self.approved_here.insert((slug, number), now.at);
        self.on_changed(seeds, now);
        dirty
    }

    /// `gh` has merged a PR: the board shows it merged now, not at the next poll, and waiting on
    /// no review, and GitHub is asked for the rest. GitHub's search for the PRs that wait on your
    /// review lists only open ones, so one it found is kept, merged, until GitHub is asked again.
    /// True when the board has changed.
    pub fn on_merged(&mut self, slug: String, number: u64, seeds: &[LaneSeed], now: Now) -> bool {
        let mut dirty = false;
        let lists = self.prs.get_mut(&slug).into_iter().chain(self.requested.get_mut(&slug));
        for pr in lists.flat_map(|prs| prs.iter_mut()).filter(|p| p.number == number) {
            shown_merged(pr, now.ms);
            dirty = true;
        }
        self.merged_here.insert((slug, number), (now.at, now.ms));
        self.on_changed(seeds, now);
        dirty
    }

    /// Something you did changed a PR on GitHub (merged it, queued it, updated its branch):
    /// PRs are read again in full, now or as soon as the poll on its way returns.
    pub fn on_changed(&mut self, seeds: &[LaneSeed], now: Now) {
        if self.schedule.refetch() {
            self.poll(seeds, true, now);
        }
    }

    fn on_repo(&mut self, slug: String, result: Result<RepoPrs, Failure>, now: Now) -> bool {
        match result {
            Ok(mut repo) => {
                // An answer to a request sent before you approved a PR here still asks for your
                // review; one sent after has GitHub's word on it.
                let asked = self.schedule.last;
                self.approved_here.retain(|(s, n), at| {
                    if *s != slug {
                        return true;
                    }
                    if asked.is_some_and(|t| t >= *at) {
                        return false;
                    }
                    if let Some(pr) = repo.prs.iter_mut().find(|p| p.number == *n) {
                        pr.review_requested = false;
                    }
                    true
                });
                // And one sent before you merged a PR here still has it open.
                self.merged_here.retain(|(s, n), (at, ms)| {
                    if *s != slug {
                        return true;
                    }
                    if asked.is_some_and(|t| t >= *at) {
                        return false;
                    }
                    if let Some(pr) = repo.prs.iter_mut().find(|p| p.number == *n) {
                        shown_merged(pr, *ms);
                    }
                    true
                });
                self.answered(&slug, None);
                self.prs.insert(slug, repo.prs);
                if repo.login.is_some() {
                    self.login = repo.login;
                }
                self.at = Some(now.ms);
            }
            Err(e) => self.answered(&slug, Some(e)),
        }
        true
    }

    /// GitHub's search for the PRs that wait on your review, each filed under the repo it's in
    /// here, by the name GitHub has for it now; a repo no project covers is left out, as its
    /// PRs are.
    fn on_requested(&mut self, result: Result<Requested, Failure>, seeds: &[LaneSeed], now: Now) -> bool {
        let found = match result {
            Ok(found) => found,
            Err(e) => {
                self.answered(github::REQUESTED, Some(e));
                return true;
            }
        };
        let mut here: HashMap<String, String> = HashMap::new();
        for slug in seeds.iter().filter_map(|s| s.repo_slug.as_ref()) {
            let now = self.names.now.get(&slug.to_lowercase()).unwrap_or(slug);
            here.entry(now.to_lowercase()).or_insert_with(|| slug.clone());
        }
        let mut requested: HashMap<String, Vec<PrRaw>> = HashMap::new();
        for (slug, pr) in found.prs {
            if let Some(local) = here.get(&slug.to_lowercase()) {
                requested.entry(local.clone()).or_default().push(pr);
            }
        }
        // As with a repo's own PRs (`on_repo`): an answer asked for before you approved a PR
        // here still lists it.
        let asked = self.schedule.last;
        self.approved_here.retain(|(s, n), at| {
            if asked.is_some_and(|t| t >= *at) {
                return false;
            }
            if let Some(prs) = requested.get_mut(s) {
                prs.retain(|p| p.number != *n);
            }
            true
        });
        // One merged here stays, merged: one asked for after has GitHub's word, which no longer
        // lists it.
        let previous = std::mem::take(&mut self.requested);
        self.merged_here.retain(|(s, n), (at, ms)| {
            if asked.is_some_and(|t| t >= *at) {
                return false;
            }
            let list = requested.entry(s.clone()).or_default();
            match list.iter_mut().find(|p| p.number == *n) {
                Some(pr) => shown_merged(pr, *ms),
                None => {
                    if let Some(was) = previous.get(s).and_then(|prs| prs.iter().find(|p| p.number == *n)) {
                        list.push(was.clone());
                    }
                }
            }
            true
        });
        requested.retain(|_, prs| !prs.is_empty());
        self.requested = requested;
        if found.login.is_some() {
            self.login = found.login;
        }
        self.answered(github::REQUESTED, None);
        self.at = Some(now.ms);
        self.searched = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Checks, Merge, Origin, PrState};
    use std::path::PathBuf;
    use std::sync::mpsc::channel;

    /// A made-up time, and one so many seconds on.
    fn clock() -> Now {
        Now { at: Instant::now(), ms: 1_800_000_000_000 }
    }

    fn after(now: Now, secs: u64) -> Now {
        Now { at: now.at + Duration::from_secs(secs), ms: now.ms + secs as i64 * 1000 }
    }

    /// GitHub with nobody listening for what it asks in the background.
    fn quiet() -> GhPoll {
        GhPoll::new(Arc::new(|_| {}))
    }

    /// A worktree of `slug`'s repo on `branch`, whose default branch is main.
    fn seed(path: &str, slug: &str, branch: &str) -> LaneSeed {
        LaneSeed {
            path: PathBuf::from(path),
            project: "P".into(),
            repo: "repo".into(),
            repo_slug: Some(slug.into()),
            head_owners: Vec::new(),
            callsign: "repo".into(),
            origin: Origin::Main,
            branch: Some(branch.into()),
            head: None,
            default_branch: Some("main".into()),
            elsewhere: false,
            chats: false,
            starts_chats: false,
            exact: false,
        }
    }

    fn pr(number: u64) -> PrRaw {
        PrRaw {
            number,
            title: "Tune issue verification".into(),
            url: "u".into(),
            state: PrState::Open,
            head_ref: "app4".into(),
            head_owner: None,
            head_oid: None,
            updated_at: 0,
            merged_at: None,
            closed_at: None,
            checks: Checks::default(),
            merge: Merge::Clean,
            author: Some("whather".into()),
            base_ref: Some("main".into()),
            created_at: 0,
            additions: 0,
            deletions: 0,
            changed_files: 0,
            review_requested: true,
            requested_by: None,
            requested_at: None,
            reviewers_asked: Vec::new(),
            authored_by: None,
        }
    }

    #[test]
    fn a_poll_goes_once_a_minute_and_a_refresh_not_right_after_one() {
        let mut s = Schedule::default();
        let t = clock();
        assert!(s.due(t.at) && s.may_start(false, t.at), "nothing sent yet");
        s.start(vec![], BTreeSet::new(), t);
        assert!(!s.may_start(true, t.at), "one is on its way");
        assert!(!s.ended(), "nothing was asked for meanwhile");

        // The engine's own poll goes when the minute is up; a window's refresh needs only
        // twenty seconds to have passed.
        assert!(!s.due(t.at + Duration::from_secs(59)) && s.due(t.at + EVERY));
        assert!(!s.may_start(false, t.at + Duration::from_secs(19)));
        assert!(s.may_start(true, t.at + Duration::from_secs(19)));
        assert!(s.may_start(false, t.at + ASKED_LATELY));

        // While GitHub is left alone nothing goes, forced or not.
        s.refused(None, t);
        assert!(!s.may_start(true, t.at + HOLD - Duration::from_secs(1)));
        assert!(s.may_start(true, t.at + HOLD));
    }

    #[test]
    fn a_poll_asks_by_what_was_read_and_what_points_are_left() {
        let mut gh = quiet();
        let t = clock();
        let stamp = Stamp { glance: "a".into(), at: t.ms };

        // No worktree's repo is on GitHub: nothing is asked, and nothing is on its way.
        let plain = LaneSeed { repo_slug: None, ..seed("/x/notes", "", "main") };
        assert!(gh.ask(&[plain.clone()], true, t).is_none());
        assert!(!gh.schedule.inflight && gh.schedule.last.is_none());

        // Only a repo with a worktree off its default branch has its PRs read; the search goes
        // for any.
        let seeds = [seed("/x/spark", "sprucehq/spark", "main"), seed("/x/spark-app4", "sprucehq/spark", "app4"), seed("/x/spur", "spurhq/spur", "main"), plain];
        gh.schedule.stamps.insert("sprucehq/spark".into(), stamp.clone());
        gh.prs.insert("sprucehq/spark".into(), vec![PrRaw { checks: Checks { pending: 1, ..Checks::default() }, updated_at: t.ms - 60_000, ..pr(1) }]);
        gh.prs.insert("spurhq/spur".into(), vec![pr(2)]);
        let ask = gh.ask(&seeds, true, t).unwrap();
        assert_eq!(ask.slugs, ["sprucehq/spark"]);
        assert_eq!(ask.have.get("sprucehq/spark"), Some(&stamp));
        assert_eq!(ask.busy.iter().collect::<Vec<_>>(), ["sprucehq/spark"], "its checks are running");
        assert!(!ask.all && !ask.thrifty);
        assert_eq!((gh.schedule.inflight, gh.schedule.last), (true, Some(t.at)));
        assert!(gh.ask(&seeds, true, after(t, 60)).is_none(), "one is on its way");

        // Something you did on GitHub from here: everything is read, once.
        gh.schedule.ended();
        gh.schedule.refetch();
        assert!(gh.ask(&seeds, true, after(t, 60)).unwrap().all);
        gh.schedule.ended();
        assert!(!gh.ask(&seeds, true, after(t, 120)).unwrap().all);
        gh.schedule.ended();

        // Few points left in an hour that hasn't ended: the poll is told to be thrifty.
        gh.schedule.answered(Some(Points { left: github::RESERVE - 1, resets: t.ms + 600_000 }), t.ms);
        assert!(gh.ask(&seeds, true, after(t, 180)).unwrap().thrifty);
        gh.schedule.ended();
        assert!(!gh.ask(&seeds, true, after(t, 601)).unwrap().thrifty, "that hour has ended");
    }

    #[test]
    fn github_is_left_alone_once_its_points_are_gone() {
        let mut gh = quiet();
        let now = clock();
        let stamp = Stamp { glance: "a".into(), at: 1 };
        let unloaded = || Failure::Other("GitHub: couldn't load PRs for a/b".into());
        let glance = |gh: &mut GhPoll, seen: Result<Glance, Failure>, now: Now| assert!(gh.on_event(Event::Glance(seen), &[], now), "a glance changes the board");

        // A glance: GitHub answered, whatever is read after it. A repo's failure is the repo's
        // to clear.
        gh.failures.insert(GLANCE.into(), Failure::Other("GitHub didn't answer".into()));
        gh.failures.insert("a/b".into(), unloaded());
        let points = |left: u64, resets: i64| Some(Points { left, resets: now.ms + resets });
        glance(&mut gh, Ok(Glance { login: Some("gwen".into()), points: points(120, 60_000), ..Glance::default() }), now);
        assert_eq!((gh.failures.keys().map(String::as_str).collect::<Vec<_>>(), gh.login()), (vec!["a/b"], Some("gwen")));
        assert_eq!(gh.at(), Some(now.ms));
        // GitHub's other counter answers, with plenty left; then the first again, with fewer.
        // What's left is the fewest, until that hour ends.
        glance(&mut gh, Ok(Glance { points: points(4800, 1_800_000), ..Glance::default() }), now);
        glance(&mut gh, Ok(Glance { points: points(110, 60_000), ..Glance::default() }), now);
        gh.schedule.points.sort_by_key(|p| p.left);
        assert_eq!(gh.schedule.points.iter().map(|p| p.left).collect::<Vec<_>>(), vec![110, 4800]);
        assert!(gh.schedule.thrifty(now.ms));
        // A counter whose hour has ended is forgotten at the next glance.
        glance(&mut gh, Ok(Glance { points: points(4790, 1_800_000), ..Glance::default() }), after(now, 61));
        assert_eq!(gh.schedule.points.iter().map(|p| p.left).collect::<Vec<_>>(), vec![4790]);
        // A repo GitHub doesn't show you: its failure, which comes next, isn't cleared first.
        glance(&mut gh, Ok(Glance { repos: HashMap::from([("a/b".to_string(), None)]), ..Glance::default() }), now);
        assert!(gh.failures.contains_key("a/b"));
        // Until it answers.
        gh.on_event(Event::Repo("a/b".into(), Ok(RepoPrs { login: None, prs: vec![] }), None), &[], now);
        assert!(gh.failures.is_empty());

        // What an answer was read at is noted; a failure keeps the one before.
        gh.on_read("a/b", None, Some(stamp.clone()), now);
        gh.on_read("a/b", Some(&unloaded()), None, now);
        assert_eq!(gh.schedule.stamps.get("a/b"), Some(&stamp));
        assert!(gh.schedule.hold.is_none());

        // Out of points, said by a glance or by a read: GitHub is left alone for a while.
        let refused = Failure::RateLimited { resets: None };
        gh.on_read("a/b", Some(&refused), None, now);
        assert_eq!(gh.schedule.hold, Some(now.at + HOLD));
        gh.schedule.hold = None;
        let refusal = |resets: Option<i64>| Err(Failure::RateLimited { resets });
        glance(&mut gh, refusal(None), now);
        assert_eq!(gh.schedule.hold, Some(now.at + HOLD), "nothing said when the hour ends");
        assert_eq!(gh.failures.get(GLANCE), Some(&refused));
        let said = gh.notice().unwrap();
        assert_eq!((said.key.as_str(), said.title.as_str(), said.until), ("github", "GitHub rate limit reached", None));
        assert_eq!(said.text, "GitHub: your account's rate limit is used up, so PRs wait until it resets. `gh` and your agents draw on the same limit.");
        // A glance had said the hour was nearly spent, and when it ends: asked again then, and
        // the notice says when.
        let ends = now.ms + 120_000;
        gh.schedule.points = vec![Points { left: 4000, resets: now.ms + 60_000 }, Points { left: 12, resets: ends }];
        glance(&mut gh, refusal(None), now);
        assert_eq!(gh.schedule.hold, Some(now.at + Duration::from_secs(125)), "until the hour ends, and a moment more");
        assert_eq!(gh.notice().unwrap().until, Some(ends));
        // The refusal itself says when: that's GitHub's word.
        glance(&mut gh, refusal(Some(ends + 60_000)), now);
        assert_eq!(gh.notice().unwrap().until, Some(ends + 60_000));
        assert_eq!(gh.schedule.hold, Some(now.at + Duration::from_secs(185)));
        // No poll goes meanwhile, and one does once the hour has ended.
        let seeds = [seed("/x/spark-app4", "sprucehq/spark", "app4")];
        assert!(gh.ask(&seeds, true, after(now, 184)).is_none());
        assert!(!gh.schedule.inflight);
        // Another failure of GitHub's has no hour to wait out.
        let refusal_was = gh.failures.remove(GLANCE).unwrap();
        gh.failures.insert("a/b".into(), unloaded());
        let said = gh.notice().unwrap();
        assert_eq!((said.title.as_str(), said.text.as_str(), said.until), ("GitHub didn't answer", "GitHub: couldn't load PRs for a/b", None));
        gh.failures.clear();
        gh.failures.insert(GLANCE.into(), refusal_was);
        // A read refused the same way: the next glance that's answered ends both.
        gh.on_event(Event::Repo("a/b".into(), Err(refused.clone()), None), &[], now);
        glance(&mut gh, Ok(Glance::default()), now);
        assert!(gh.schedule.hold.is_none() && gh.failures.is_empty() && gh.notice().is_none());
        assert!(gh.ask(&seeds, true, now).is_some());
    }

    /// The repos and the search are read at once, each answer applied as it lands: which lands
    /// last mustn't decide whether a failure is said.
    #[test]
    fn a_failure_stays_until_what_failed_answers_again() {
        let mut gh = quiet();
        let t = clock();
        let seeds = [seed("/x/a-dev", "o/a", "dev"), seed("/x/b-dev", "o/b", "dev")];
        let ok = |slug: &str| Event::Repo(slug.into(), Ok(RepoPrs { login: None, prs: vec![] }), None);
        let failed = |slug: &str| Event::Repo(slug.into(), Err(Failure::Other(format!("GitHub: couldn't load PRs for {slug}"))), None);
        let text = |gh: &GhPoll| gh.notice().map(|n| n.text);
        gh.ask(&seeds, true, t).unwrap();
        gh.on_event(Event::Glance(Ok(Glance::default())), &seeds, t);
        gh.on_event(failed("o/a"), &seeds, t);
        gh.on_event(ok("o/b"), &seeds, t);
        gh.on_event(Event::Requested(Ok(Requested::default()), None), &seeds, t);
        assert_eq!(text(&gh).as_deref(), Some("GitHub: couldn't load PRs for o/a"), "o/b and the search answering don't clear o/a's");
        // The search fails too: both are said, and o/a answering leaves the search's.
        let search = Failure::Other("GitHub: couldn't load your review requests".into());
        gh.on_event(Event::Requested(Err(search.clone()), None), &seeds, t);
        assert!(!gh.on_event(Event::PollEnded, &seeds, t), "what failed in this poll still stands");
        assert_eq!(text(&gh).as_deref(), Some("GitHub: couldn't load PRs for o/a GitHub: couldn't load your review requests"));
        gh.ask(&seeds, true, after(t, 60)).unwrap();
        gh.on_event(Event::Glance(Ok(Glance::default())), &seeds, after(t, 60));
        gh.on_event(ok("o/a"), &seeds, after(t, 60));
        assert_eq!(text(&gh), Some(search.to_string()));
        // A poll whose glance was answered and which had no need to read the search again: its
        // answer stands as it is, so nothing is wrong with it now.
        assert!(gh.on_event(Event::PollEnded, &seeds, after(t, 60)), "the notice goes");
        assert_eq!(gh.notice(), None);
        // Without a glance answered nothing is known to stand: a failure not read again stays.
        gh.on_event(failed("o/b"), &seeds, after(t, 120));
        gh.ask(&seeds, true, after(t, 120)).unwrap();
        gh.on_event(Event::Glance(Err(Failure::Other("GitHub didn't answer".into()))), &seeds, after(t, 120));
        gh.on_event(Event::PollEnded, &seeds, after(t, 120));
        assert_eq!(gh.failures.keys().collect::<Vec<_>>(), [GLANCE, "o/b"]);
        // `gh` signed out explains the rest, so it titles the notice.
        gh.on_event(Event::Requested(Err(Failure::SignedOut), None), &seeds, after(t, 120));
        assert_eq!(gh.notice().unwrap().title, "Signed out of GitHub");
    }

    #[test]
    fn a_repo_is_current_as_of_the_poll_that_read_it_or_found_it_unchanged() {
        let mut gh = quiet();
        let t = clock();
        let seeds = [seed("/x/a-dev", "o/a", "dev"), seed("/x/b-dev", "o/b", "dev")];
        let read = |slug: &str, mark: &str, at: i64| Event::Repo(slug.into(), Ok(RepoPrs { login: None, prs: vec![] }), Some(Stamp { glance: mark.into(), at }));
        let glance = |a: &str, b: &str| Event::Glance(Ok(Glance { repos: HashMap::from([("o/a".to_string(), Some(a.to_string())), ("o/b".to_string(), Some(b.to_string()))]), ..Glance::default() }));
        let current = |a: Option<i64>, b: Option<i64>| BTreeMap::from([("o/a".to_string(), a), ("o/b".to_string(), b)]);
        // A poll on its way that hasn't read them: both are asked about, neither current yet.
        gh.ask(&seeds, true, t).unwrap();
        assert_eq!(gh.current(), current(None, None));
        // It reads both: each is current as of when the poll was sent.
        gh.on_event(glance("1", "1"), &seeds, after(t, 1));
        gh.on_event(read("o/a", "1", t.ms), &seeds, after(t, 2));
        gh.on_event(read("o/b", "1", t.ms), &seeds, after(t, 3));
        gh.on_event(Event::PollEnded, &seeds, after(t, 3));
        assert_eq!(gh.current(), current(Some(t.ms), Some(t.ms)));
        // The next glance finds o/a as it was read and o/b changed: o/a is current as of this poll,
        // and o/b only once its read comes, which here fails.
        let next = after(t, 60);
        gh.ask(&seeds, true, next).unwrap();
        gh.on_event(glance("1", "2"), &seeds, after(next, 1));
        assert_eq!(gh.current(), current(Some(next.ms), Some(t.ms)));
        gh.on_event(Event::Repo("o/b".into(), Err(Failure::Other("GitHub: couldn't load PRs for o/b".into())), None), &seeds, after(next, 2));
        assert_eq!(gh.current(), current(Some(next.ms), Some(t.ms)));
    }

    #[test]
    fn a_kept_answer_given_while_github_refuses_is_only_as_current_as_its_stamp() {
        let mut gh = quiet();
        let t = clock();
        let seeds = [seed("/x/a-dev", "o/a", "dev")];
        // After a restart with no points left, the poll hands over what was kept from an hour ago.
        gh.ask(&seeds, true, t).unwrap();
        let hour_ago = t.ms - 3_600_000;
        gh.on_event(Event::Repo("o/a".into(), Ok(RepoPrs { login: None, prs: vec![pr(1)] }), Some(Stamp { glance: "1".into(), at: hour_ago })), &seeds, t);
        gh.on_event(Event::Glance(Err(Failure::RateLimited { resets: Some(t.ms + 600_000) })), &seeds, t);
        assert_eq!(gh.current(), BTreeMap::from([("o/a".to_string(), Some(hour_ago))]));
    }

    #[test]
    fn each_failure_has_its_own_few_words() {
        let mut gh = quiet();
        assert_eq!(gh.notice(), None);
        for (failure, title) in [(Failure::NotInstalled, "GitHub CLI isn't installed"), (Failure::SignedOut, "Signed out of GitHub")] {
            gh.on_event(Event::Requested(Err(failure.clone()), None), &[], clock());
            let said = gh.notice().unwrap();
            assert_eq!((said.key.as_str(), said.title.as_str(), said.until), ("github", title, None));
            assert_eq!(said.text, failure.to_string());
        }
    }

    #[test]
    fn github_is_asked_once_what_each_repo_is_called_now() {
        let mut gh = quiet();
        let t = Instant::now();
        let seeds = [seed("/x/spark", "SparkHQ/spark", "main"), seed("/x/spur", "spurhq/spur", "main")];
        let slugs = || seeds.iter().filter_map(|s| s.repo_slug.as_ref());
        let asked = gh.names.to_ask(slugs(), t).unwrap();
        assert_eq!(asked.iter().collect::<Vec<_>>(), ["sparkhq/spark", "spurhq/spur"], "by the slug a remote has, lowercase");
        assert!(gh.names.to_ask(slugs(), t).is_none(), "an asking is on its way");

        // GitHub answered for one repo, which has another name now.
        assert!(gh.on_event(Event::RepoNames(vec![("sparkhq/spark".into(), Some("sprucehq/spark".into())), ("spurhq/spur".into(), None)]), &seeds, clock()), "a rename changes the board");
        assert_eq!(gh.renamed("SparkHQ/spark").map(String::as_str), Some("sprucehq/spark"));
        assert_eq!(gh.renamed("spurhq/spur"), None);
        // The one it didn't answer for is asked about again after an hour, and only that one.
        assert!(gh.names.to_ask(slugs(), t + NAME_RETRY - Duration::from_secs(1)).is_none());
        assert!(!gh.names.inflight);
        assert_eq!(gh.names.to_ask(slugs(), t + NAME_RETRY).unwrap().iter().collect::<Vec<_>>(), ["spurhq/spur"]);
        assert!(!gh.on_event(Event::RepoNames(vec![("spurhq/spur".into(), Some("spurhq/spur".into()))]), &seeds, clock()), "a repo that kept its name changes nothing");
        assert_eq!(gh.renamed("spurhq/spur"), None, "a repo that kept its name says nothing more");
        // Every repo has its answer, so there's nothing to ask GitHub.
        gh.ask_names(&seeds, t + NAME_RETRY * 3);
        assert!(!gh.names.inflight);

        // An asking that fell over found nothing, and is over: its repos are asked about later.
        let mut gh = quiet();
        gh.names.to_ask(slugs(), t).unwrap();
        assert!(!gh.on_event(Event::RepoNames(Vec::new()), &seeds, clock()));
        assert!(!gh.names.inflight);
        assert_eq!(gh.names.to_ask(slugs(), t + NAME_RETRY).map(|s| s.len()), Some(2));
    }

    #[test]
    fn it_has_polled_only_once_a_poll_ends_whatever_answered_first() {
        let mut gh = quiet();
        let seeds = [seed("/r/spark", "sprucehq/spark", "feature")];
        let asked = PrRaw { review_requested: true, ..pr(7) };
        let reviews = |gh: &GhPoll| board::pr_items(&seeds, &[], gh.prs(), gh.requested(), Some("gwen"), &Default::default()).into_iter().filter(|p| p.waits_on_review()).count();
        // The search answers first: the board lists a read repo's reviews from its read, which
        // is still to come.
        let found = Requested { login: Some("gwen".into()), prs: vec![("sprucehq/spark".into(), asked.clone())] };
        gh.on_event(Event::Requested(Ok(found), None), &seeds, clock());
        assert_eq!(reviews(&gh), 0);
        assert!(!gh.polled());
        gh.on_event(Event::Repo("sprucehq/spark".into(), Ok(RepoPrs { login: Some("gwen".into()), prs: vec![asked] }), None), &seeds, clock());
        assert_eq!(reviews(&gh), 1, "the review comes with the repo's read");
        assert!(!gh.polled());
        assert!(gh.on_event(Event::PollEnded, &seeds, clock()), "the board says so at once");
        assert!(gh.polled(), "and the poll that brought both has ended");
        assert!(!gh.on_event(Event::PollEnded, &seeds, after(clock(), 60)), "and only once");
    }

    #[test]
    fn a_poll_whose_search_failed_hasnt_polled_until_one_answers() {
        let mut gh = quiet();
        let seeds = [seed("/r/spark", "sprucehq/spark", "feature")];
        // Offline at launch: nothing came, so what waits on you isn't known.
        gh.on_event(Event::Requested(Err(Failure::Other("GitHub didn't answer".into())), None), &seeds, clock());
        gh.on_event(Event::PollEnded, &seeds, clock());
        assert!(!gh.polled());
        // Back online, the next poll brings it.
        gh.on_event(Event::Requested(Ok(Requested::default()), None), &seeds, after(clock(), 60));
        assert!(!gh.polled(), "not before its poll ends");
        gh.on_event(Event::PollEnded, &seeds, after(clock(), 60));
        assert!(gh.polled());
    }

    #[test]
    fn a_snapshots_one_pass_has_polled_once_its_search_answered() {
        let seeds = [seed("/r/spark", "sprucehq/spark", "feature")];
        let pass = |search: Result<Requested, Failure>| {
            let mut gh = quiet();
            gh.take(github::Answer::Requested(search, None), &seeds, clock());
            gh.take(github::Answer::Repo("sprucehq/spark".into(), Ok(RepoPrs::default()), None), &seeds, clock());
            gh.took_all();
            gh.polled()
        };
        assert!(pass(Ok(Requested::default())), "everything GitHub had came in the one pass");
        assert!(!pass(Err(Failure::Other("GitHub didn't answer".into()))), "what waits on you isn't known");
    }

    #[test]
    fn a_pr_approved_here_stops_asking_at_once() {
        let mut gh = quiet();
        let slug = "sprucehq/spark".to_string();
        let asked = |gh: &GhPoll| gh.prs()[&slug][0].review_requested;
        let answer = |requested: bool| Event::Repo(slug.clone(), Ok(RepoPrs { login: Some("gwen".into()), prs: vec![PrRaw { review_requested: requested, ..pr(1089) }] }), None);
        gh.prs.insert(slug.clone(), vec![pr(1089)]);
        let sent = clock();

        // A poll is on its way when you approve.
        gh.schedule.inflight = true;
        gh.schedule.last = Some(sent.at);
        assert!(gh.on_approved(slug.clone(), 1089, &[], after(sent, 1)));
        assert!(!asked(&gh), "the board stops asking at once");
        assert!(gh.schedule.again, "GitHub is asked again once the poll returns");
        // That poll was answered before GitHub had your review.
        assert!(gh.on_event(answer(true), &[], after(sent, 2)));
        assert!(!asked(&gh), "an answer asked for before the approval doesn't bring it back");
        assert!(!gh.on_event(Event::PollEnded, &[], after(sent, 2)), "the end of a poll changes nothing on the board");
        assert!(!gh.schedule.inflight && !gh.schedule.again);
        assert!(gh.schedule.all, "and everything is read then, whatever its glance says");

        // An answer asked for after it is GitHub's word, a new request included.
        gh.schedule.last = Some(after(sent, 3).at);
        gh.on_event(answer(true), &[], after(sent, 4));
        assert!(asked(&gh));
        assert!(gh.approved_here.is_empty());
    }

    #[test]
    fn a_pr_approved_here_leaves_the_ones_that_wait_on_you() {
        let mut gh = quiet();
        let seeds = [seed("/x/spark", "SparkHQ/spark", "main")];
        gh.names.now.insert("sparkhq/spark".into(), "sprucehq/spark".into());
        let found = || Event::Requested(Ok(Requested { login: Some("gwen".into()), prs: vec![("sprucehq/spark".into(), pr(7)), ("else/where".into(), pr(8))] }), None);
        let sent = clock();
        gh.schedule.last = Some(sent.at);

        // Filed under the slug the repo has here, by the name GitHub has for it now; a repo no
        // project covers is left out.
        assert!(gh.on_event(found(), &seeds, sent));
        assert_eq!(gh.requested().keys().collect::<Vec<_>>(), ["SparkHQ/spark"]);
        assert_eq!((gh.login(), gh.at()), (Some("gwen"), Some(sent.ms)));

        // Approved here: gone at once, and an answer asked for before doesn't bring it back.
        gh.schedule.inflight = true;
        assert!(gh.on_approved("SparkHQ/spark".into(), 7, &seeds, after(sent, 1)));
        assert!(gh.requested()["SparkHQ/spark"].is_empty());
        gh.on_event(found(), &seeds, after(sent, 2));
        assert!(gh.requested().get("SparkHQ/spark").is_none_or(|prs| prs.is_empty()));
        // One asked for after is GitHub's word.
        gh.schedule.last = Some(after(sent, 3).at);
        gh.on_event(found(), &seeds, after(sent, 4));
        assert_eq!(gh.requested()["SparkHQ/spark"].len(), 1);
    }

    #[test]
    fn a_pr_merged_here_shows_merged_at_once() {
        let mut gh = quiet();
        let slug = "sprucehq/spark".to_string();
        let state = |gh: &GhPoll| (gh.prs()[&slug][0].state, gh.prs()[&slug][0].merged_at.is_some(), gh.prs()[&slug][0].review_requested);
        let answer = || Event::Repo(slug.clone(), Ok(RepoPrs { login: Some("gwen".into()), prs: vec![PrRaw { review_requested: true, ..pr(1089) }] }), None);
        gh.prs.insert(slug.clone(), vec![PrRaw { review_requested: true, ..pr(1089) }]);
        let sent = clock();

        // A poll is on its way when you merge.
        gh.schedule.inflight = true;
        gh.schedule.last = Some(sent.at);
        assert!(gh.on_merged(slug.clone(), 1089, &[], after(sent, 1)));
        assert_eq!(state(&gh), (PrState::Merged, true, false), "merged at once, and waiting on no review");
        assert_eq!(gh.prs()[&slug][0].merged_at, Some(after(sent, 1).ms));
        assert!(gh.schedule.again, "GitHub is asked again once the poll returns");
        // That poll was answered before GitHub had merged it.
        gh.on_event(answer(), &[], after(sent, 2));
        assert_eq!(state(&gh), (PrState::Merged, true, false), "an answer asked for before the merge doesn't open it again");

        // An answer asked for after it is GitHub's word.
        gh.schedule.last = Some(after(sent, 3).at);
        gh.on_event(answer(), &[], after(sent, 4));
        assert_eq!(state(&gh).0, PrState::Open);
        assert!(gh.merged_here.is_empty());
    }

    #[test]
    fn a_pr_merged_here_stays_merged_among_the_ones_that_waited_on_you() {
        let mut gh = quiet();
        let seeds = [seed("/x/spark", "SparkHQ/spark", "main")];
        gh.names.now.insert("sparkhq/spark".into(), "sprucehq/spark".into());
        let found = |prs: Vec<PrRaw>| Event::Requested(Ok(Requested { login: Some("gwen".into()), prs: prs.into_iter().map(|p| ("sprucehq/spark".to_string(), p)).collect() }), None);
        let asked = || PrRaw { review_requested: true, ..pr(7) };
        let sent = clock();
        gh.schedule.last = Some(sent.at);
        gh.on_event(found(vec![asked()]), &seeds, sent);

        // Merged here: it stays listed, merged, as the search lists only open PRs.
        gh.schedule.inflight = true;
        assert!(gh.on_merged("SparkHQ/spark".into(), 7, &seeds, after(sent, 1)));
        let seven = |gh: &GhPoll| gh.requested().get("SparkHQ/spark").and_then(|prs| prs.iter().find(|p| p.number == 7)).map(|p| (p.state, p.review_requested));
        assert_eq!(seven(&gh), Some((PrState::Merged, false)));
        // A search asked for before the merge still lists it open, or already doesn't list it.
        gh.on_event(found(vec![asked()]), &seeds, after(sent, 2));
        assert_eq!(seven(&gh), Some((PrState::Merged, false)));
        gh.on_event(found(vec![]), &seeds, after(sent, 2));
        assert_eq!(seven(&gh), Some((PrState::Merged, false)));
        // One asked for after is GitHub's word, which no longer lists it.
        gh.schedule.last = Some(after(sent, 3).at);
        gh.on_event(found(vec![]), &seeds, after(sent, 4));
        assert_eq!(seven(&gh), None);
        assert!(gh.merged_here.is_empty());
    }

    /// A poll runs on a thread nobody watches. If it fell over without a word, the poll would
    /// stay on its way and GitHub would never be asked again.
    #[test]
    fn a_poll_that_falls_over_is_over() {
        let (tx, rx) = channel();
        let tx = std::sync::Mutex::new(tx);
        let mut gh = GhPoll::new(Arc::new(move |event| {
            let _ = tx.lock().unwrap().send(event);
        }));
        let t = clock();
        gh.schedule.start(vec![], BTreeSet::new(), t);
        gh.run_poll(|back| {
            back(Event::Glance(Ok(Glance::default())));
            // Without the hook's report on stderr, as a panic would unwind.
            std::panic::resume_unwind(Box::new("gh fell over"));
        });
        let wait = Duration::from_secs(30);
        assert!(matches!(rx.recv_timeout(wait), Ok(Event::Glance(_))));
        let Ok(ended) = rx.recv_timeout(wait) else { panic!("the poll never said it was over") };
        assert!(matches!(ended, Event::PollEnded));
        gh.on_event(ended, &[], t);
        assert!(!gh.schedule.inflight && gh.schedule.may_start(true, t.at));
    }
}

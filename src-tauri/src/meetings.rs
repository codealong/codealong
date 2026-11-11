//! Meetings: as one in your calendar is about to start, Codealong offers to take its notes, and
//! taking it starts a session that listens (listen.rs) with nothing more to set up. Part of
//! voice (`[experimental] voice`), and off until `[voice] meetings` is on.
//!
//! The calendar is macOS's own, read by the helper (ear.rs `calendar`, EventKit): whichever
//! accounts you added to this Mac, with no sign-in of Codealong's. macOS asks you once whether
//! Codealong may read it, and only when you turn this on or ask for it (`ask`): a look at the
//! calendar asks nobody anything. It's read again every minute for the day ahead.
//!
//! A meeting is an event with a time of day that someone else is in, or that's held on a call
//! (`is_meeting`), and that you didn't say no to. One you're invited to at several of your
//! addresses is in each account's calendar and counts once, as one you said yes to when you
//! did at any of them (`counted`). It's on offer from a
//! minute before it starts until ten minutes in, or its end (`on_offer`); one you took or passed
//! on isn't offered again, which `meetings.json` beside config.toml keeps across a server started
//! again, nor is one a session is hearing.
//! What's on offer is on the board (`Board::meeting`), which every window draws from.
//!
//! Taking one (`take`) starts a session for it as a chat of its own, on the account and model
//! `[voice]` names (or the ones Auto picks), and has it listen to your microphone and what the
//! Mac plays, as its note-taker. Its transcript and notes go in the meeting's own folder among
//! your meetings (`folder_for`): `[voice] meetings_folder`, or `meetings` beside config.toml. A
//! folder a meeting, named for when it was and what it was called, with `transcript.md`,
//! `notes.md` and `meeting.json` (what the calendar said of it, and the session that took it).
//! Plain files, so the folder is the record: nothing else lists your meetings, and a session
//! started there can read them all. Its listening ends by itself once the meeting's time is up
//! and the talk has stopped (listen.rs `AFTER_MEETING_MS`).
//!
//! Only this Mac's calendar and sessions: nothing of it goes to your other devices.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::ear::{self, Calendar, Meeting};
use crate::engine::Handle;
use crate::listen::{self, Listening, Role};
use crate::{config, store, util};

/// How far into a meeting its offer stays; how long before it starts is `[voice] meeting_lead`.
const LATE_MS: i64 = 10 * 60 * 1000;

/// An event longer than this isn't a meeting to take notes of: a day blocked out with others
/// in it, an offsite, a conference.
const LONGEST_MS: i64 = 4 * 60 * 60 * 1000;

/// A meeting that's going on, or starts within this long, can be taken from the list of what's
/// ahead, before or after its offer.
const SOON_MS: i64 = 10 * 60 * 1000;

/// What `[voice]` says of which meetings are offered: not those of the calendars left out, and
/// from how long before one starts.
#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    pub off: Vec<String>,
    pub lead_ms: i64,
}

impl Rules {
    pub fn of(cfg: &Config) -> Rules {
        Rules { off: cfg.voice.meeting_calendars_off.clone(), lead_ms: i64::from(cfg.voice.meeting_lead.min(config::MEETING_LEAD_MAX)) * 60 * 1000 }
    }

    /// Whether an event is one to offer at all: a meeting (`is_meeting`), in a calendar that
    /// counts.
    pub fn counts(&self, m: &Meeting) -> bool {
        is_meeting(m) && (m.calendar_id.is_empty() || !self.off.contains(&m.calendar_id))
    }
}

/// How far ahead the calendar is read, and how often.
const AHEAD_MS: i64 = 24 * 60 * 60 * 1000;
const READ_EVERY: Duration = Duration::from_secs(60);

/// How often what's on offer is looked at again: a meeting comes on offer on the minute.
const TICK: Duration = Duration::from_secs(5);

/// How long a new session has to show on the board before its listening starts.
const SHOWS_WITHIN: Duration = Duration::from_secs(45);

/// How long an answer to an offer is remembered: past any meeting's being on offer.
const ANSWERS_KEPT_MS: i64 = 2 * AHEAD_MS;

/// The meeting on offer, as the windows show it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    #[serde(flatten)]
    pub meeting: Meeting,
    /// You took it, and its session is being started: nothing's hearing yet.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub taking: bool,
}

/// Where the calendar stands, for Settings and `codealong listen status`: whether it's read at
/// all, whether Codealong may, the accounts it has calendars from, and how many meetings are
/// still to come in the day ahead with when the next starts. No meeting's name: the command
/// prints this, and an agent may run it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Status {
    /// `[voice] meetings`.
    pub on: bool,
    pub access: String,
    pub sources: Vec<ear::CalendarSource>,
    pub meetings: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_at: Option<i64>,
    /// Why it couldn't be read, when it couldn't.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// Where your meetings are kept (`root`), as a path from your home.
    pub folder: String,
    /// Every calendar, by account then name, for Settings to offer: which count is `[voice]
    /// meeting_calendars_off`.
    pub calendars: Vec<ear::CalendarInfo>,
}

/// A meeting in the day ahead, for the window that lists them: whether it can be taken now
/// (it's going on, or about to), whether you answered its offer, and whether it's being heard.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Ahead {
    #[serde(flatten)]
    pub meeting: Meeting,
    pub takeable: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub answered: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub heard: bool,
}

/// The meetings ahead that count, the first to start first, with one that's going on.
fn ahead_of(events: &[Meeting], rules: &Rules, answered: &HashMap<String, i64>, heard: &[String], now: i64) -> Vec<Ahead> {
    let mut ahead: Vec<Ahead> = counted(events, rules)
        .into_iter()
        .filter(|m| m.end > now)
        .map(|m| Ahead { takeable: now >= m.start - SOON_MS && !heard.contains(&m.id), answered: answered.contains_key(&m.id), heard: heard.contains(&m.id), meeting: m.clone() })
        .collect();
    ahead.sort_by(|a, b| a.meeting.start.cmp(&b.meeting.start).then_with(|| a.meeting.id.cmp(&b.meeting.id)));
    ahead
}

/// The meetings in the day ahead, as last read (`Ahead`): none while meetings aren't offered.
pub fn ahead(cfg: &Config) -> Vec<Ahead> {
    if !turned_on(cfg) {
        return Vec::new();
    }
    let heard = listen::meetings_heard();
    let state = state();
    ahead_of(&state.calendar.events, &Rules::of(cfg), &state.answered, &heard, util::now_ms())
}

/// What a meeting's folder says of it (`meeting.json`): what the calendar had, and what took it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Record {
    #[serde(flatten)]
    pub meeting: Meeting,
    /// The listening that heard it, and the key of the session that took its notes.
    pub listening: String,
    pub session: String,
    pub role: Role,
    pub taken_at: i64,
}

/// A meeting kept among your meetings: what its folder says of it (`Record`), and what's in it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Kept {
    pub folder: String,
    /// The folder's name: when it was and what it was called.
    pub name: String,
    #[serde(flatten)]
    pub record: Record,
    /// Whether notes were kept of it, and how many lines were heard.
    pub notes: bool,
    pub lines: usize,
}

/// A kept meeting's notes and transcript, as its files have them: None for one that's empty or
/// isn't there.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Files {
    pub notes: Option<String>,
    pub transcript: Option<String>,
}

/// Your meetings, the latest first: every folder in `root` that says which meeting it is
/// (`meeting.json`). The folder is the record, so one moved in, renamed or taken away is as it's
/// found.
pub fn kept(root: &Path) -> Vec<Kept> {
    let Ok(folders) = std::fs::read_dir(root) else { return Vec::new() };
    let mut all: Vec<Kept> = folders
        .flatten()
        .filter_map(|entry| {
            let folder = entry.path();
            let record: Record = serde_json::from_str(&std::fs::read_to_string(folder.join("meeting.json")).ok()?).ok()?;
            let lines = std::fs::read_to_string(folder.join("transcript.md")).map(|t| t.lines().filter(|l| l.starts_with('[')).count()).unwrap_or(0);
            let notes = std::fs::metadata(folder.join("notes.md")).is_ok_and(|m| m.len() > 0);
            Some(Kept { name: util::file_name(&folder), folder: folder.to_string_lossy().into_owned(), record, notes, lines })
        })
        .collect();
    all.sort_by(|a, b| b.record.meeting.start.cmp(&a.record.meeting.start).then_with(|| b.name.cmp(&a.name)));
    all
}

/// A kept meeting's notes and transcript, for the window that shows it. Only of a folder among
/// your meetings that says which meeting it is: nothing else is read by asking for it here.
pub fn files(root: &Path, folder: &str) -> Result<Files> {
    let folder = Path::new(folder);
    if folder.parent() != Some(root) || !folder.join("meeting.json").is_file() {
        bail!("That isn't one of your meetings.");
    }
    let read = |name: &str| std::fs::read_to_string(folder.join(name)).ok().filter(|text| !text.trim().is_empty());
    Ok(Files { notes: read("notes.md"), transcript: read("transcript.md") })
}

/// The meetings you took or passed on, by id, and when, as `meetings.json` keeps them: a server
/// started again in the middle of a meeting's offer doesn't make it twice.
#[derive(Serialize, Deserialize, Default)]
struct Answers {
    #[serde(default)]
    answered: std::collections::BTreeMap<String, i64>,
}

fn answers_path() -> PathBuf {
    config::support_dir().join("meetings.json")
}

/// Notes an answer in the file, and lets go of the ones too old to matter.
fn note_answer(path: &Path, id: &str, now: i64) {
    let noted = store::change(path, "the meetings you answered", |file: &mut Answers| {
        file.answered.retain(|_, at| now - *at < ANSWERS_KEPT_MS);
        file.answered.insert(id.to_string(), now);
        true
    });
    // One that isn't noted is still remembered until the server goes; the file's trouble is
    // said as a notice (store.rs `problems`).
    let _ = store::noted(path, noted);
}

fn kept_answers(path: &Path) -> HashMap<String, i64> {
    store::read_or_default::<Answers>(path).answered.into_iter().collect()
}

#[derive(Default)]
struct State {
    calendar: Calendar,
    read_at: Option<Instant>,
    problem: Option<String>,
    /// The meetings you took or passed on, by id, and when: the file's (`Answers`), read at the
    /// first look, and what's been answered since.
    answered: HashMap<String, i64>,
    answers_read: bool,
    /// The one whose session is being started.
    taking: Option<String>,
    offer: Option<Offer>,
}

fn state() -> MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(Mutex::default).lock().unwrap_or_else(|e| e.into_inner())
}

// The rules.

/// Whether an event is a meeting to take notes of: someone else is in it or it's held on a call,
/// and it isn't hours long. A block of time you set aside for yourself isn't one, nor a day away
/// with others in it. Events that take the whole day, and ones called off, the helper leaves out
/// as it reads (`calendar` in main.swift); one you said no to, `counted` does.
pub fn is_meeting(m: &Meeting) -> bool {
    (!m.attendees.is_empty() || m.link.is_some()) && m.end - m.start <= LONGEST_MS
}

/// The events that count under the rules, one of each meeting, without the ones you said no to.
/// A meeting you're invited to at several of your addresses is an event in each account's
/// calendar, and the helper gives the copies one id (main.swift `name(of:)`): it counts once, as
/// its first copy in a calendar that counts, so leaving one of those calendars out leaves the
/// meeting in, and what a copy there says counts for nothing. You answer such a meeting in one
/// calendar and leave the rest: it's one you said no to when you did in some copy and yes in
/// none, so a second invitation turned down doesn't lose the meeting you took at the first.
fn counted<'a>(events: &'a [Meeting], rules: &Rules) -> Vec<&'a Meeting> {
    let copies: Vec<&Meeting> = events.iter().filter(|m| rules.counts(m)).collect();
    let ids = |said: fn(&Meeting) -> bool| copies.iter().filter(|m| said(m)).map(|m| m.id.as_str()).collect::<HashSet<_>>();
    let (yes, no) = (ids(|m| m.accepted), ids(|m| m.declined));
    let mut seen = HashSet::new();
    copies.into_iter().filter(|m| (yes.contains(m.id.as_str()) || !no.contains(m.id.as_str())) && seen.insert(m.id.as_str())).collect()
}

/// A meeting by its id: the copy that counts (`counted`), else whichever has it, as one being
/// taken stays in sight whatever the settings say since.
fn found<'a>(events: &'a [Meeting], rules: &Rules, id: &str) -> Option<&'a Meeting> {
    counted(events, rules).into_iter().find(|m| m.id == id).or_else(|| events.iter().find(|m| m.id == id))
}

/// The meeting to offer at `now`: one about to start or not long begun, still going, that you
/// haven't answered and that no session is hearing (`heard`). The first to start, of several.
pub fn on_offer<'a>(events: &'a [Meeting], rules: &Rules, answered: &HashMap<String, i64>, heard: &[String], now: i64) -> Option<&'a Meeting> {
    counted(events, rules)
        .into_iter()
        .filter(|m| !answered.contains_key(&m.id) && !heard.contains(&m.id))
        .filter(|m| now >= m.start - rules.lead_ms && now < m.end.min(m.start + LATE_MS))
        .min_by(|a, b| a.start.cmp(&b.start).then_with(|| a.id.cmp(&b.id)))
}

/// A local time's parts: year, month, day, hour, minute.
fn local(ms: i64) -> (i32, u32, u32, u32, u32) {
    let secs = ms.div_euclid(1000) as libc::time_t;
    // SAFETY: localtime_r fills the struct it's given; a zeroed tm is a valid one.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    (tm.tm_year + 1900, (tm.tm_mon + 1) as u32, tm.tm_mday as u32, tm.tm_hour as u32, tm.tm_min as u32)
}

/// A time of day as a clock has it, on the Mac's own clock.
pub fn hour(ms: i64) -> String {
    let (_, _, _, h, m) = local(ms);
    format!("{h:02}:{m:02}")
}

/// A meeting's name as a folder's: nothing a path can't hold, one line, not too long.
fn safe(title: &str) -> String {
    let plain: String = title.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '\0') || c.is_control() { ' ' } else { c }).collect();
    let words = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    match util::truncate(words.trim_matches('.'), 80) {
        name if name.trim().is_empty() => "Meeting".into(),
        name => name,
    }
}

/// The folder a meeting's transcript and notes go in, among your meetings in `root`: named for
/// the day and time it starts and what it's called, so they list in order.
pub fn folder_for(root: &Path, m: &Meeting) -> PathBuf {
    let (y, mo, d, h, mi) = local(m.start);
    root.join(format!("{y:04}-{mo:02}-{d:02} {h:02}{mi:02} {}", safe(&m.title)))
}

/// Where your meetings are kept: `[voice] meetings_folder`, or `meetings` among Codealong's
/// files.
pub fn root(cfg: &Config) -> PathBuf {
    match cfg.voice.meetings_folder.trim() {
        "" => config::support_dir().join("meetings"),
        folder => util::expand(folder),
    }
}

/// Makes the folder a meeting is taken into. One that already has a transcript (the meeting was
/// taken before, and stopped) is left as it is, and this one goes beside it.
fn new_folder(root: &Path, m: &Meeting) -> Result<PathBuf> {
    let first = folder_for(root, m);
    let folder = (1..100)
        .map(|n| if n == 1 { first.clone() } else { PathBuf::from(format!("{} ({n})", first.display())) })
        .find(|f| !f.join("transcript.md").exists())
        .ok_or_else(|| anyhow!("{} has been taken too many times.", util::tilde(&first)))?;
    std::fs::create_dir_all(&folder).map_err(|e| anyhow!("Couldn't make {}: {e}", util::tilde(&folder)))?;
    Ok(folder)
}

/// The others in a meeting, in words: "with Dana and Lee", or nothing when it's only you.
pub fn with_whom(m: &Meeting) -> String {
    match m.attendees.as_slice() {
        [] => String::new(),
        [a, b, c, d, rest @ ..] if !rest.is_empty() => format!(" with {a}, {b}, {c}, {d} and {} more", rest.len()),
        names => format!(" with {}", util::listed(names, "and")),
    }
}

/// What a session started for a meeting is asked first, before it's told how it listens
/// (listen.rs `brief`): only to be there.
fn opening(m: &Meeting) -> String {
    format!(
        "Your user has a meeting, “{}”, from {} to {}{}, and asked Codealong to have you take its notes. Codealong is about to tell you how: it transcribes the meeting on their Mac and tells you what's said as it's heard. There's nothing to do until it does: reply with only “Ready”.",
        m.title.trim(),
        hour(m.start),
        hour(m.end),
        with_whom(m)
    )
}

// What's running.

/// Starts the meetings' thread in the server.
pub fn spawn(engine: Handle) {
    let started = std::thread::Builder::new().name("codealong-meetings".into()).spawn(move || loop {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| look(&engine))).is_err() {
            eprintln!("meetings: a look at the calendar panicked");
        }
        std::thread::sleep(TICK);
    });
    if let Err(e) = started {
        eprintln!("meetings: the calendar's thread didn't start: {e}");
    }
}

fn turned_on(cfg: &Config) -> bool {
    cfg.experimental.voice && cfg.voice.meetings
}

/// One look: the calendar read again when it's time, and what's on offer worked out anew.
fn look(engine: &Handle) {
    let Ok(cfg) = config::current() else { return };
    if !turned_on(&cfg) {
        let had = {
            let mut state = state();
            let had = state.offer.take().is_some();
            *state = State { answered: std::mem::take(&mut state.answered), answers_read: state.answers_read, ..State::default() };
            had
        };
        if had {
            engine.listenings_changed();
        }
        return;
    }
    if !state().answers_read {
        let kept = kept_answers(&answers_path());
        let mut state = state();
        for (id, at) in kept {
            state.answered.entry(id).or_insert(at);
        }
        state.answers_read = true;
    }
    if state().read_at.is_none_or(|at| at.elapsed() >= READ_EVERY) {
        read(false);
    }
    settle(engine, util::now_ms());
}

/// Reads the calendar for the day ahead, keeping what it says; with `ask`, macOS asks the person
/// first whether Codealong may. Outside the lock: the helper takes a moment, and their answer
/// as long as they take.
fn read(ask: bool) {
    let now = util::now_ms();
    let got = ear::calendar(ask, now - LATE_MS, now + AHEAD_MS);
    let mut state = state();
    state.read_at = Some(Instant::now());
    match got {
        Ok(calendar) => {
            state.calendar = calendar;
            state.problem = None;
        }
        Err(refused) => state.problem = Some(refused.message),
    }
    state.answered.retain(|_, at| now - *at < ANSWERS_KEPT_MS);
}

/// What's on offer: the meeting being taken, in sight until it's hearing whatever the time, else
/// the one `on_offer` gives.
fn offer_of(events: &[Meeting], rules: &Rules, answered: &HashMap<String, i64>, heard: &[String], taking: Option<&str>, now: i64) -> Option<Offer> {
    match taking {
        Some(id) => found(events, rules, id).map(|m| Offer { meeting: m.clone(), taking: true }),
        None => on_offer(events, rules, answered, heard, now).map(|m| Offer { meeting: m.clone(), taking: false }),
    }
}

/// Works out what's on offer, and has the board say so when it changed.
fn settle(engine: &Handle, now: i64) {
    // A meeting a session is hearing was taken, whatever the answers say: by a server before
    // this one that went between starting it and noting it, say. Only while it's heard, and
    // never remembered: one whose take is under way is listed as hearing before it is, and a
    // take that then fails leaves the meeting to be taken again.
    let heard = listen::meetings_heard();
    // As the settings are now: a calendar left out in Settings is out at the next look.
    let Ok(cfg) = config::current() else { return };
    let rules = Rules::of(&cfg);
    let changed = {
        let mut state = state();
        let offer = offer_of(&state.calendar.events, &rules, &state.answered, &heard, state.taking.as_deref(), now);
        let changed = offer != state.offer;
        state.offer = offer;
        changed
    };
    if changed {
        engine.listenings_changed();
    }
}

/// The meeting on offer now, for the board.
pub fn offered() -> Option<Offer> {
    state().offer.clone()
}

/// Where the calendar stands (`Status`), as last read.
pub fn status(cfg: &Config) -> Status {
    let now = util::now_ms();
    let state = state();
    let rules = Rules::of(cfg);
    let ahead: Vec<&Meeting> = counted(&state.calendar.events, &rules).into_iter().filter(|m| m.start > now).collect();
    Status {
        on: turned_on(cfg),
        access: state.calendar.access.clone(),
        sources: state.calendar.sources.clone(),
        meetings: ahead.len(),
        next_at: ahead.iter().map(|m| m.start).min(),
        problem: state.problem.clone(),
        folder: util::tilde(&root(cfg)),
        calendars: state.calendar.calendars.clone(),
    }
}

/// Has macOS ask the person whether Codealong may read their calendar, when it hasn't yet, and
/// reads it: where it stands after. What Settings asks as this is turned on.
pub fn ask(engine: &Handle) -> Result<Status> {
    let cfg = config::current()?;
    listen::turned_on(&cfg)?;
    read(true);
    settle(engine, util::now_ms());
    Ok(status(&cfg))
}

/// Passes on a meeting: it isn't offered again.
pub fn pass(engine: &Handle, id: &str) {
    let now = util::now_ms();
    state().answered.insert(id.to_string(), now);
    note_answer(&answers_path(), id, now);
    settle(engine, now);
}

/// Takes a meeting: a session is started for it and listens, as `role`. It returns once the
/// session is hearing; until then the offer says it's being taken.
pub fn take(engine: &Handle, id: &str, role: Role) -> Result<Listening> {
    let cfg = config::current()?;
    listen::turned_on(&cfg)?;
    // Asked for from a list read a while ago, say: taken since, in another window.
    if listen::meetings_heard().iter().any(|heard| heard == id) {
        bail!("That meeting's notes are being taken already.");
    }
    let meeting = {
        let mut state = state();
        let Some(meeting) = found(&state.calendar.events, &Rules::of(&cfg), id).cloned() else { bail!("That meeting isn't in your calendar any more.") };
        if state.taking.is_some() {
            bail!("A meeting's notes are being started already.");
        }
        state.taking = Some(id.to_string());
        meeting
    };
    settle(engine, util::now_ms());
    let taken = begin(engine, &cfg, &meeting, role);
    let now = util::now_ms();
    {
        let mut state = state();
        state.taking = None;
        if taken.is_ok() {
            state.answered.insert(id.to_string(), now);
        }
    }
    if taken.is_ok() {
        note_answer(&answers_path(), id, now);
    }
    settle(engine, now);
    taken
}

/// Starts a meeting's session and its listening.
fn begin(engine: &Handle, cfg: &Config, meeting: &Meeting, role: Role) -> Result<Listening> {
    use codealong_peer::api;
    let now = util::now_ms();
    let board = engine.board();
    // A chat of its own, as a schedule with no folder starts one (schedules/runner.rs).
    let lane = config::chats_dir().to_string_lossy().into_owned();
    let account = Some(cfg.voice.meeting_account.trim()).filter(|a| !a.is_empty()).unwrap_or("auto");
    let model = Some(cfg.voice.meeting_model.trim()).filter(|m| !m.is_empty());
    let pick = crate::start::pick(cfg, Some(&board), account, None, model, None, true, now, &util::home())?;
    let folder = new_folder(&root(cfg), meeting)?;
    let spec = api::StartSpec { lane_id: lane.clone(), account: pick.account, place: pick.place, prompt: opening(meeting), model: pick.model, effort: pick.effort, worktree: false, todos: Vec::new(), from: None, helper: false, report: false };
    let on = spec.account.clone();
    let started: api::Started = crate::actions::local(engine, &crate::actions::From::Here, api::START, &lane, api::params::Start { spec }).and_then(crate::actions::typed).map_err(|e| anyhow!(e))?;
    let Some(key) = started.key else {
        bail!("A session was started on {on} for it, but its harness names its own sessions, so Codealong can't tell which to have listen. Have it listen from its menu once it shows, or name another account in Settings › Voice.");
    };
    shows(engine, &key)?;
    // Named for its meeting, where its harness would name it for what it was first asked.
    let _ = crate::actions::local(engine, &crate::actions::From::Here, api::SESSION_RENAME, &key, api::params::Rename { key: String::new(), name: util::truncate(meeting.title.trim(), 80) });
    let ask = listen::Ask { mic: true, system: true, speakers: cfg.voice.on_speakers, role, ..listen::Ask::default() };
    let listening = listen::start_of(engine, &key, &ask, Some(listen::Of { meeting: meeting.clone(), folder: folder.clone() }))?;
    // The last meeting's is over, if it's still hearing: its time is up, and this one has come.
    // Only now that this one is hearing: a take that failed leaves the last one as it was.
    listen::end_meetings_but(&listening.id, "the next meeting was taken");
    write_record(&folder.join("meeting.json"), &Record { meeting: meeting.clone(), listening: listening.id.clone(), session: listening.session.clone(), role, taken_at: now });
    Ok(listening)
}

fn write_record(path: &Path, record: &Record) {
    if let Err(e) = serde_json::to_string_pretty(record).map_err(anyhow::Error::from).and_then(|text| util::write_whole(path, &text)) {
        eprintln!("meetings: {} wasn't written: {e}", path.display());
    }
}

/// Keeps a meeting's record with the session its listening goes on under (listen.rs
/// `goes_on_as`: one that had stopped was resumed as a copy, with another key), so *Its session*
/// in the window that lists your meetings goes to the one that has the conversation.
pub fn follow(l: &Listening) {
    if l.folder.is_none() {
        return;
    }
    let path = l.dir().join("meeting.json");
    let Some(mut record) = std::fs::read_to_string(&path).ok().and_then(|text| serde_json::from_str::<Record>(&text).ok()) else { return };
    if record.listening == l.id && record.session != l.session {
        record.session = l.session.clone();
        write_record(&path, &record);
    }
}

/// Waits for a session just started to show on the board, which its listening is started from.
fn shows(engine: &Handle, key: &str) -> Result<()> {
    let since = Instant::now();
    while since.elapsed() < SHOWS_WITHIN {
        if engine.board().session(key).is_some() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    bail!("Its session was started but hasn't shown yet. Have it listen from its menu once it does.")
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_794_330_000_000;
    const MIN: i64 = 60_000;
    /// As a new config.toml has it: every calendar, a minute before.
    const PLAIN: Rules = Rules { off: Vec::new(), lead_ms: MIN };

    fn meeting(id: &str, start: i64, minutes: i64) -> Meeting {
        Meeting { id: id.into(), title: "Renewal call".into(), start, end: start + minutes * MIN, calendar: "Work".into(), attendees: vec!["Dana Kestrel".into()], ..Meeting::default() }
    }

    #[test]
    fn a_meeting_is_an_event_with_someone_else_or_a_call_that_you_didnt_decline() {
        let m = meeting("a", NOW, 30);
        assert!(is_meeting(&m));
        // Time you set aside for yourself isn't one; held on a call, it is.
        let alone = Meeting { attendees: Vec::new(), ..m.clone() };
        assert!(!is_meeting(&alone));
        assert!(is_meeting(&Meeting { link: Some("https://meet.example/abc".into()), ..alone }));
        // One you said no to is neither offered nor listed.
        let none = HashMap::new();
        let declined = [Meeting { declined: true, ..m.clone() }];
        assert_eq!(on_offer(&declined, &PLAIN, &none, &[], NOW), None);
        assert!(ahead_of(&declined, &PLAIN, &none, &[], NOW).is_empty());
        assert_eq!(on_offer(&[m.clone()], &PLAIN, &none, &[], NOW).map(|m| m.id.as_str()), Some("a"));
        // Hours long, with others in it: a day away, not a meeting to take notes of.
        assert!(is_meeting(&Meeting { end: m.start + LONGEST_MS, ..m.clone() }));
        assert!(!is_meeting(&Meeting { end: m.start + LONGEST_MS + MIN, ..m }));
    }

    #[test]
    fn the_settings_say_which_calendars_count_and_how_long_before() {
        let work = Meeting { calendar_id: "work".into(), ..meeting("w", NOW, 30) };
        let home = Meeting { calendar_id: "home".into(), ..meeting("h", NOW, 30) };
        let events = [home.clone(), work.clone()];
        let none = HashMap::new();
        // Both count until one's calendar is left out; an event that names no calendar counts.
        assert_eq!(on_offer(&events, &PLAIN, &none, &[], NOW).map(|m| m.id.as_str()), Some("h"));
        let rules = Rules { off: vec!["home".into()], lead_ms: 5 * MIN };
        assert!(rules.counts(&work) && !rules.counts(&home) && rules.counts(&meeting("x", NOW, 30)));
        assert_eq!(on_offer(&events, &rules, &none, &[], NOW).map(|m| m.id.as_str()), Some("w"));
        // Offered from as long before as asked, and at its start with none.
        assert_eq!(on_offer(&events, &rules, &none, &[], NOW - 5 * MIN).map(|m| m.id.as_str()), Some("w"));
        assert_eq!(on_offer(&events, &rules, &none, &[], NOW - 6 * MIN), None);
        let at_start = Rules { off: Vec::new(), lead_ms: 0 };
        assert_eq!(on_offer(&events, &at_start, &none, &[], NOW - 1), None);
        assert!(on_offer(&events, &at_start, &none, &[], NOW).is_some());
        // As config.toml has them, the lead in minutes and never more than the most.
        let mut cfg: Config = toml::from_str("").unwrap();
        cfg.voice.meeting_lead = 2;
        cfg.voice.meeting_calendars_off = vec!["home".into()];
        assert_eq!(Rules::of(&cfg), Rules { off: vec!["home".into()], lead_ms: 2 * MIN });
        cfg.voice.meeting_lead = 500;
        assert_eq!(Rules::of(&cfg).lead_ms, i64::from(config::MEETING_LEAD_MAX) * MIN);
    }

    #[test]
    fn a_meeting_in_several_of_your_calendars_counts_once() {
        // Invited at two addresses: the same event in each account's calendar, with one id.
        let copy = |calendar: &str| Meeting { calendar_id: calendar.into(), calendar: calendar.into(), ..meeting("m", NOW, 30) };
        let events = [copy("home"), copy("work"), Meeting { calendar_id: "work".into(), ..meeting("other", NOW + 2 * MIN, 30) }];
        let none = HashMap::new();
        let ids = |rules: &Rules| ahead_of(&events, rules, &none, &[], NOW).into_iter().map(|a| (a.meeting.id, a.meeting.calendar_id)).collect::<Vec<_>>();
        assert_eq!(ids(&PLAIN), vec![("m".to_string(), "home".to_string()), ("other".to_string(), "work".to_string())]);
        // With one of its calendars left out it's the other's, and still a meeting; with both, not.
        let without = |off: &[&str]| Rules { off: off.iter().map(|c| c.to_string()).collect(), lead_ms: MIN };
        assert_eq!(ids(&without(&["home"])), vec![("m".to_string(), "work".to_string()), ("other".to_string(), "work".to_string())]);
        assert_eq!(found(&events, &without(&["home"]), "m").map(|m| m.calendar_id.as_str()), Some("work"));
        assert_eq!(ids(&without(&["home", "work"])), vec![]);
        // Taken, it's found whatever the settings say since.
        assert_eq!(found(&events, &without(&["home", "work"]), "m").map(|m| m.calendar_id.as_str()), Some("home"));
        assert_eq!(found(&events, &PLAIN, "gone"), None);
        // Answered once, neither copy is offered; the next meeting is.
        assert_eq!(on_offer(&events, &PLAIN, &none, &[], NOW).map(|m| m.calendar_id.as_str()), Some("home"));
        let answered = HashMap::from([("m".to_string(), NOW)]);
        assert_eq!(on_offer(&events, &PLAIN, &answered, &[], NOW), None);
        assert_eq!(on_offer(&events, &PLAIN, &answered, &[], NOW + MIN).map(|m| m.id.as_str()), Some("other"));
        // Said no to at one address and left at the other, it's one you said no to; with that
        // calendar left out, what its copy says counts for nothing.
        let events = [copy("home"), Meeting { declined: true, ..copy("work") }];
        assert_eq!(on_offer(&events, &PLAIN, &none, &[], NOW), None);
        assert!(ahead_of(&events, &PLAIN, &none, &[], NOW).is_empty());
        assert_eq!(on_offer(&events, &without(&["work"]), &none, &[], NOW).map(|m| m.calendar_id.as_str()), Some("home"));
        // Said yes to at one and no at the other, it's one you took, in either order.
        let events = [Meeting { declined: true, ..copy("home") }, Meeting { accepted: true, ..copy("work") }];
        assert_eq!(on_offer(&events, &PLAIN, &none, &[], NOW).map(|m| m.id.as_str()), Some("m"));
        assert_eq!(ahead_of(&events, &PLAIN, &none, &[], NOW).len(), 1);
        let events = [Meeting { accepted: true, ..copy("home") }, Meeting { declined: true, ..copy("work") }];
        assert_eq!(on_offer(&events, &PLAIN, &none, &[], NOW).map(|m| m.id.as_str()), Some("m"));
        // The yes in a calendar left out doesn't count either: what's left is the no.
        assert_eq!(on_offer(&events, &without(&["home"]), &none, &[], NOW), None);
    }

    #[test]
    fn the_meetings_ahead_are_listed_with_what_can_be_done_with_each() {
        let events = [
            meeting("later", NOW + 3 * 60 * MIN, 30),
            meeting("over", NOW - 60 * MIN, 30),
            meeting("going", NOW - 5 * MIN, 30),
            Meeting { attendees: Vec::new(), ..meeting("alone", NOW + MIN, 30) },
            meeting("soon", NOW + 9 * MIN, 30),
            Meeting { calendar_id: "home".into(), ..meeting("home", NOW + 2 * MIN, 30) },
        ];
        let rules = Rules { off: vec!["home".into()], lead_ms: MIN };
        let answered = HashMap::from([("soon".to_string(), NOW)]);
        let got: Vec<(String, bool, bool, bool)> = ahead_of(&events, &rules, &answered, &["going".to_string()], NOW).into_iter().map(|a| (a.meeting.id, a.takeable, a.answered, a.heard)).collect();
        // In order; one that's over, one that's no meeting and one in a calendar left out aren't
        // listed. One being heard can't be taken again; one you passed on still can.
        assert_eq!(got, vec![("going".to_string(), false, false, true), ("soon".to_string(), true, true, false), ("later".to_string(), false, false, false)]);
    }

    #[test]
    fn a_meeting_is_on_offer_from_a_minute_before_until_ten_minutes_in_or_its_end() {
        let events = [meeting("a", NOW, 30)];
        let none = HashMap::new();
        let offered = |now: i64| on_offer(&events, &PLAIN, &none, &[], now).map(|m| m.id.as_str());
        assert_eq!(offered(NOW - 2 * MIN), None);
        assert_eq!(offered(NOW - MIN), Some("a"));
        assert_eq!(offered(NOW), Some("a"));
        assert_eq!(offered(NOW + LATE_MS - 1), Some("a"));
        assert_eq!(offered(NOW + LATE_MS), None);
        // A short one is on offer only while it goes on.
        let short = [meeting("s", NOW, 5)];
        assert_eq!(on_offer(&short, &PLAIN, &none, &[], NOW + 4 * MIN).map(|m| m.id.as_str()), Some("s"));
        assert_eq!(on_offer(&short, &PLAIN, &none, &[], NOW + 5 * MIN), None);
    }

    #[test]
    fn one_you_answered_isnt_offered_again_and_the_next_at_the_same_time_is() {
        let events = [meeting("b", NOW, 30), meeting("a", NOW, 30), meeting("later", NOW + 5 * MIN, 30), Meeting { attendees: Vec::new(), ..meeting("alone", NOW - MIN, 60) }];
        let mut answered = HashMap::new();
        // The first to start, and of two at once the same one every time.
        assert_eq!(on_offer(&events, &PLAIN, &answered, &[], NOW).map(|m| m.id.as_str()), Some("a"));
        answered.insert("a".to_string(), NOW);
        assert_eq!(on_offer(&events, &PLAIN, &answered, &[], NOW).map(|m| m.id.as_str()), Some("b"));
        answered.insert("b".to_string(), NOW);
        assert_eq!(on_offer(&events, &PLAIN, &answered, &[], NOW), None);
        assert_eq!(on_offer(&events, &PLAIN, &answered, &[], NOW + 4 * MIN).map(|m| m.id.as_str()), Some("later"));
    }

    #[test]
    fn a_meeting_being_heard_isnt_on_offer_and_one_whose_take_failed_is_again() {
        let events = [meeting("a", NOW, 30), meeting("b", NOW + 2 * MIN, 30)];
        let answered = HashMap::new();
        let offered = |heard: &[&str], taking: Option<&str>, now: i64| {
            let heard: Vec<String> = heard.iter().map(|id| id.to_string()).collect();
            offer_of(&events, &PLAIN, &answered, &heard, taking, now).map(|o| (o.meeting.id, o.taking))
        };
        assert_eq!(offered(&[], None, NOW), Some(("a".to_string(), false)));
        // Taken: in sight as starting, while its listening is filed before anything's heard.
        assert_eq!(offered(&[], Some("a"), NOW), Some(("a".to_string(), true)));
        assert_eq!(offered(&["a"], Some("a"), NOW + 5000), Some(("a".to_string(), true)));
        // It's hearing: no longer on offer, by that alone, and the next one is in its turn.
        assert_eq!(offered(&["a"], None, NOW + 10_000), None);
        assert_eq!(offered(&["a"], None, NOW + MIN).map(|(id, _)| id), Some("b".to_string()));
        // The take failed and its listening is gone: nothing was remembered, so it's on offer
        // again, to try once more.
        assert_eq!(offered(&[], None, NOW + 10_000), Some(("a".to_string(), false)));
    }

    #[test]
    fn the_answers_to_offers_are_kept_in_a_file_and_the_old_ones_let_go() {
        let path = store::test_dir("meetings-answers").join("meetings.json");
        assert!(kept_answers(&path).is_empty());
        note_answer(&path, "a", NOW);
        note_answer(&path, "b", NOW + MIN);
        // As a server started again reads them: neither is on offer.
        let kept = kept_answers(&path);
        assert_eq!((kept.get("a"), kept.get("b")), (Some(&NOW), Some(&(NOW + MIN))));
        assert_eq!(on_offer(&[meeting("a", NOW, 30), meeting("b", NOW, 30)], &PLAIN, &kept, &[], NOW + MIN), None);
        // One from long ago goes as the next is noted.
        note_answer(&path, "c", NOW + ANSWERS_KEPT_MS + MIN);
        let kept = kept_answers(&path);
        assert_eq!(kept.keys().map(String::as_str).collect::<Vec<_>>(), vec!["c"]);
    }

    #[test]
    fn a_meetings_record_follows_its_listening_to_the_session_it_goes_on_under() {
        let folder = store::test_dir("meetings-follow").join("2026-11-10 1400 Renewal call");
        std::fs::create_dir_all(&folder).unwrap();
        let record = Record { meeting: meeting("a", NOW, 30), listening: "l1".into(), session: "claude:work:old".into(), role: Role::Notes, taken_at: NOW };
        write_record(&folder.join("meeting.json"), &record);
        let read = || serde_json::from_str::<Record>(&std::fs::read_to_string(folder.join("meeting.json")).unwrap()).unwrap();
        let listening = |id: &str, session: &str| Listening { id: id.into(), session: session.into(), folder: Some(folder.to_string_lossy().into_owned()), meeting: Some(meeting("a", NOW, 30)), ..Listening::default() };
        // Another listening's move leaves it (the meeting taken again, into a folder beside it).
        follow(&listening("l2", "claude:work:other"));
        assert_eq!(read(), record);
        follow(&listening("l1", "claude:work:new"));
        assert_eq!(read(), Record { session: "claude:work:new".into(), ..record });
        // One that's no meeting's has no record to keep.
        follow(&Listening { id: "l1".into(), session: "claude:work:x".into(), ..Listening::default() });
        assert_eq!(read().session, "claude:work:new");
    }

    #[test]
    fn a_meetings_folder_is_named_for_when_it_was_and_what_it_was_called() {
        let root = Path::new("/Users/me/Meetings");
        let m = Meeting { title: "  Q3/Q4 planning: atlas\n& kestrel  ".into(), ..meeting("a", NOW, 30) };
        let name = folder_for(root, &m).file_name().unwrap().to_string_lossy().into_owned();
        let (y, mo, d, h, mi) = local(NOW);
        assert_eq!(name, format!("{y:04}-{mo:02}-{d:02} {h:02}{mi:02} Q3 Q4 planning atlas & kestrel"));
        assert!(folder_for(root, &m).starts_with(root));
        // One with no name, or a name that's all dots, still has a folder.
        for title in ["", " .. ", "\t"] {
            let name = folder_for(root, &Meeting { title: title.into(), ..m.clone() }).file_name().unwrap().to_string_lossy().into_owned();
            assert!(name.ends_with(" Meeting"), "{name}");
        }
        assert!(safe(&"long ".repeat(40)).chars().count() <= 80);
    }

    #[test]
    fn a_meeting_taken_again_goes_beside_the_first() {
        let root = std::env::temp_dir().join(format!("codealong-meetings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let m = meeting("a", NOW, 30);
        let first = new_folder(&root, &m).unwrap();
        assert_eq!(first, folder_for(&root, &m));
        // Nothing heard into it yet: the same folder is taken again.
        assert_eq!(new_folder(&root, &m).unwrap(), first);
        std::fs::write(first.join("transcript.md"), "[10:00:00] You: Hello.\n").unwrap();
        let second = new_folder(&root, &m).unwrap();
        assert_eq!(second.file_name().unwrap().to_string_lossy(), format!("{} (2)", first.file_name().unwrap().to_string_lossy()));
        assert!(second.is_dir());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn your_meetings_are_the_folders_that_say_which_meeting_they_are_the_latest_first() {
        let root = std::env::temp_dir().join(format!("codealong-meetings-kept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(kept(&root), Vec::new());
        let keep = |m: &Meeting, transcript: &str, notes: &str| {
            let folder = new_folder(&root, m).unwrap();
            let record = Record { meeting: m.clone(), listening: "l".into(), session: format!("claude:work:{}", m.id), role: Role::Notes, taken_at: m.start };
            std::fs::write(folder.join("meeting.json"), serde_json::to_string_pretty(&record).unwrap()).unwrap();
            std::fs::write(folder.join("transcript.md"), transcript).unwrap();
            std::fs::write(folder.join("notes.md"), notes).unwrap();
            folder
        };
        let first = keep(&meeting("a", NOW, 30), "[10:00:00] You: Hello.\n[10:00:04] Them: Hi.\n(Codealong's hearing stopped and started again here.)\n", "# Renewal call\n\n- Signed by June.\n");
        let later = keep(&Meeting { title: "Standup".into(), ..meeting("b", NOW + 60 * MIN, 15) }, "", "");
        // A folder that says nothing of a meeting isn't one, whatever's in it.
        std::fs::create_dir_all(root.join("scratch")).unwrap();
        std::fs::write(root.join("scratch").join("notes.md"), "mine").unwrap();
        let all = kept(&root);
        assert_eq!(all.iter().map(|k| (k.record.meeting.title.as_str(), k.lines, k.notes)).collect::<Vec<_>>(), vec![("Standup", 0, false), ("Renewal call", 2, true)]);
        assert_eq!(all[1].folder, first.to_string_lossy());
        assert!(all[1].name.ends_with(" Renewal call"), "{}", all[1].name);
        // Its files are read as they are; an empty one is none; and only a meeting's are read.
        let got = files(&root, &all[1].folder).unwrap();
        assert_eq!(got.notes.as_deref(), Some("# Renewal call\n\n- Signed by June.\n"));
        assert!(got.transcript.unwrap().starts_with("[10:00:00] You: Hello."));
        assert_eq!(files(&root, &later.to_string_lossy()).unwrap(), Files::default());
        for other in [root.join("scratch"), root.clone(), std::env::temp_dir(), first.join("..").join("..")] {
            assert_eq!(files(&root, &other.to_string_lossy()).unwrap_err().to_string(), "That isn't one of your meetings.", "{}", other.display());
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn says_who_else_is_in_it_and_what_its_session_is_first_asked() {
        let m = meeting("a", NOW, 30);
        assert_eq!(with_whom(&m), " with Dana Kestrel");
        assert_eq!(with_whom(&Meeting { attendees: Vec::new(), ..m.clone() }), "");
        let many = Meeting { attendees: ["A", "B", "C", "D", "E", "F"].map(String::from).to_vec(), ..m.clone() };
        assert_eq!(with_whom(&many), " with A, B, C, D and 2 more");
        let five = Meeting { attendees: ["A", "B", "C", "D", "E"].map(String::from).to_vec(), ..m.clone() };
        assert_eq!(with_whom(&five), " with A, B, C, D and 1 more");
        let said = opening(&m);
        assert!(said.starts_with("Your user has a meeting, “Renewal call”, from "), "{said}");
        assert!(said.contains(" with Dana Kestrel, and asked Codealong to have you take its notes."), "{said}");
        assert!(said.ends_with("reply with only “Ready”."), "{said}");
    }

    #[test]
    fn what_a_meetings_folder_says_of_it_reads_back() {
        let record = Record { meeting: Meeting { link: Some("https://meet.example/abc".into()), series: Some("E1".into()), ..meeting("E1@1", NOW, 30) }, listening: "l1".into(), session: "claude:work:abc".into(), role: Role::Notes, taken_at: NOW };
        let text = serde_json::to_string_pretty(&record).unwrap();
        assert!(text.contains("\"title\": \"Renewal call\"") && text.contains("\"session\": \"claude:work:abc\"") && text.contains("\"takenAt\""), "{text}");
        assert_eq!(serde_json::from_str::<Record>(&text).unwrap(), record);
    }
}

//! Starting a session from Codealong. Codealong only launches: the harness runs the session,
//! makes a worktree when you ask it to, and the session reaches the board through its adapter
//! like any other. See *Surfaces*, New session.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;

use crate::attachments::{self, Style};
use crate::config::{self, AccountCfg, Config};
use crate::engine::Handle;
use crate::harness::{Adapter, StartAsk};
use crate::launch;
use crate::model::{Board, Harness, Lane, Meter, Origin, Session, Severity, Usage};
use crate::todos::PendingStart;
use crate::util::{home, now_ms, tilde};

/// The environment variable a terminal start hands the prompt over in. The command Codealong
/// types stays short and needs no quoting, whatever the prompt holds.
pub const PROMPT_VAR: &str = "CODEALONG_PROMPT";
/// The session's key (`claude:<account>:<id>`), set in a terminal start's environment so the
/// `codealong` command can tell which session runs it (`whoami`).
pub const SESSION_VAR: &str = "CODEALONG_SESSION";
/// The images folder, for `--add-dir`, when the prompt has images. Its path has a space in it.
pub const IMAGES_VAR: &str = "CODEALONG_IMAGES";
/// The attached files' folder, for `--add-dir`, when the prompt has other files.
pub const FILES_VAR: &str = "CODEALONG_FILES";

/// What a start asks for and gets back goes between devices too (a phone starts sessions on your
/// Macs), so those types are in codealong-peer.
pub use codealong_peer::api::{GroupMember, ModelOption, Place, PlaceOption, ResumeIn, RoutingGroup, StartAccount, StartOptions, StartSpec, Started};

/// What a start does. `--snapshot --start-plan` prints it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Plan {
    /// A new terminal window at `cwd`, with `env` set in its shell, typing `input`; `behind` your
    /// windows for a Background start of a harness with no background service of its own.
    Terminal { cwd: PathBuf, input: String, env: Vec<(String, String)>, behind: bool },
    /// `command` run by the login shell at `cwd`: a Claude Code background session. Its
    /// arguments are quoted in the command itself: the service the session runs under may start
    /// from this shell and keep its environment, so nothing is handed over in that.
    Background { cwd: PathBuf, command: String },
    /// A link the Claude app opens.
    ClaudeApp { url: String },
    /// A link the Codex app opens.
    CodexApp { url: String },
    /// A session Codealong runs over the account's ACP adapter, on `prompt`.
    Hosted { account: String, cwd: PathBuf, prompt: String, model: Option<String>, effort: Option<String> },
}

fn harness_of(a: &AccountCfg) -> Harness {
    Harness::from_config(&a.harness)
}

fn adapter_of(a: &AccountCfg) -> &'static dyn Adapter {
    crate::harness::adapter(harness_of(a))
}

/// Where an account can start a session, the usual place first, or why it can't
/// (`Adapter::places`). The Claude app and the Codex app each run one account, the one in the
/// default folder (`~/.claude`, `~/.codex`); the terminal runs any account that has a command
/// line, through its command.
pub fn places_of(a: &AccountCfg, home: &Path) -> std::result::Result<Vec<Place>, String> {
    adapter_of(a).places(a, home)
}

/// Why an account's `start` can't be where its sessions start, in words.
pub fn check_start(a: &AccountCfg, place: &str, home: &Path) -> std::result::Result<(), String> {
    let offered = places_of(a, home).unwrap_or_default();
    let words = |ps: &[Place]| ps.iter().map(|p| format!("\"{}\"", p.word())).collect::<Vec<_>>().join(", ");
    match Place::from_word(place) {
        _ if offered.is_empty() => Err(format!("{} can't start sessions from Codealong, so it takes no start.", a.name)),
        Some(p) if offered.contains(&p) => Ok(()),
        _ => Err(format!("{} can start sessions in {}; \"{}\" isn't one of them.", a.name, words(&offered), place.trim())),
    }
}

/// The places an account offers, with the one its `start` names first.
pub fn ordered_places(a: &AccountCfg, home: &Path) -> std::result::Result<Vec<Place>, String> {
    let mut places = places_of(a, home)?;
    if let Some(i) = a.start.as_deref().and_then(Place::from_word).and_then(|p| places.iter().position(|&q| q == p)) {
        let p = places.remove(i);
        places.insert(0, p);
    }
    Ok(places)
}

/// The places a session starts in out of sight: under its harness's background service, or in
/// Codealong.
pub const OUT_OF_SIGHT: [Place; 2] = [Place::Background, Place::Codealong];

/// Where one of `a`'s sessions starts out of sight: under its harness's background service, else
/// in Codealong, else in a terminal behind your windows (Background, for a harness with no
/// service of its own). Helpers (`codealong start --helper`) start there unless told where;
/// automatic reviews go where `[reviews] start` says (autoreview.rs `review_places`).
pub fn out_of_sight(a: &AccountCfg, home: &Path) -> Option<Place> {
    out_of_sight_of(a, &ordered_places(a, home).unwrap_or_default())
}

/// Where one of `a`'s sessions that isn't running goes on out of sight, of the places it can go
/// on in (`resume_places`), in `out_of_sight`'s order: for a resume Codealong makes by itself on a
/// message from a session it started (send.rs `wakes`).
pub fn resume_out_of_sight(a: &AccountCfg, home: &Path) -> Option<Place> {
    out_of_sight_of(a, &resume_places(a, home))
}

fn out_of_sight_of(a: &AccountCfg, offered: &[Place]) -> Option<Place> {
    let service = crate::harness::of(a).is_some_and(|h| h.runs_in_background());
    let order = if service { OUT_OF_SIGHT } else { [Place::Codealong, Place::Background] };
    order.into_iter().find(|p| offered.contains(p))
}

/// Where one of an account's sessions that isn't running can go on (`resume`): the places it
/// starts sessions in that its harness can take a session up again in (`Adapter::resumes_in`),
/// in the same order.
pub fn resume_places(a: &AccountCfg, home: &Path) -> Vec<Place> {
    let adapter = adapter_of(a);
    ordered_places(a, home).unwrap_or_default().into_iter().filter(|&p| adapter.resumes_in(p)).collect()
}

/// Where a start from the quick panel opened becomes `start` for `names`: the account it ran on,
/// or for Auto the accounts Auto picks among. So the panel opens there next time, and
/// `codealong start` without `--in` starts there too. An account that can't start there, or
/// already does, is left as it is, and config.toml isn't written when none changes.
pub fn keep_place(names: &[String], place: Place) -> Result<()> {
    let changed = place_changes(&config::current()?.accounts, names, place, &home());
    if changed.is_empty() {
        return Ok(());
    }
    config::save_start(&changed, place.word())
}

/// The accounts among `names` whose `start` would become `place` (`keep_place`).
pub fn place_changes(accounts: &[AccountCfg], names: &[String], place: Place, home: &Path) -> Vec<String> {
    accounts
        .iter()
        .filter(|a| names.contains(&a.name) && a.start.as_deref().and_then(Place::from_word) != Some(place) && check_start(a, place.word(), home).is_ok())
        .map(|a| a.name.clone())
        .collect()
}

/// Where each account can start a session, as the sheet offers it. `default_model` reads an
/// account's default model. The places keep their order whatever the account's `start` says
/// (`chosen` says that), so the quick panel's places hold still as a start there changes it.
pub fn options(accounts_in: &[AccountCfg], terminal: &str, codex_projects: Vec<String>, home: &Path, default_model: impl Fn(&AccountCfg) -> Option<String>) -> StartOptions {
    let accounts = accounts_in
        .iter()
        .map(|a| {
            let harness = harness_of(a);
            let (places, unavailable) = match places_of(a, home) {
                Ok(ps) => (ps, None),
                Err(why) => (Vec::new(), Some(why)),
            };
            let adapter = adapter_of(a);
            let resumes = places.iter().copied().filter(|&p| adapter.resumes_in(p)).collect();
            let places = places
                .into_iter()
                .map(|place| {
                    let label = match place {
                        Place::Terminal => terminal.to_string(),
                        Place::Background => "Background".into(),
                        Place::ClaudeApp => "Claude app".into(),
                        Place::CodexApp => "Codex app".into(),
                        Place::Codealong => "Codealong".into(),
                    };
                    PlaceOption { place, label, behind: place == Place::Background && !adapter.runs_in_background() }
                })
                .collect();
            let chosen = a.start.as_deref().and_then(Place::from_word);
            // What its command line takes; an account with nowhere to start is asked nothing.
            let tuned = adapter.takes_model() && unavailable.is_none();
            StartAccount {
                name: a.name.clone(),
                harness,
                places,
                chosen,
                unavailable,
                default_model: if tuned { default_model(a) } else { None },
                preferred: false,
                usage_cap: a.usage_cap,
                models: if tuned { adapter.models(a) } else { Vec::new() },
                efforts: if tuned { listed(adapter.efforts()) } else { Vec::new() },
                worktree: adapter.makes_worktrees(),
                resumes,
                switches_model: tuned && adapter.model_command().is_some(),
                switches_effort: tuned && !adapter.efforts().is_empty() && adapter.effort_command().is_some(),
            }
        })
        .collect();
    // Devices that read the lists from here, before each account had its own, start Claude Code.
    let claude = crate::harness::adapter(Harness::Claude);
    let claude_models = accounts_in.iter().find(|a| harness_of(a) == Harness::Claude).map(|a| claude.models(a)).unwrap_or_default();
    StartOptions { accounts, codex_projects, claude_models, claude_efforts: listed(claude.efforts()), groups: Vec::new(), auto_group: String::new() }
}

fn listed(list: &[(&str, &str)]) -> Vec<ModelOption> {
    list.iter().map(|(id, label)| ModelOption { id: id.to_string(), label: label.to_string() }).collect()
}

/// Why Auto starts the account it does: its weekly limit resets soonest, so its room is the
/// first to be lost (`reset`); it has the most room (`room`); or there are no numbers to go by
/// (`first`).
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AutoReason {
    Reset,
    Room,
    First,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutoPick {
    pub account: String,
    pub reason: AutoReason,
    /// The weekly reset, for `reset`.
    pub resets_at: Option<i64>,
}

/// An account Auto could start: whether it's preferred (see `preferred_names`), the model it
/// would start on (None when that could be any), and its usage cap (`AccountCfg::usage_cap`).
pub struct Candidate<'a> {
    pub name: &'a str,
    pub preferred: bool,
    pub model: Option<&'a str>,
    pub usage_cap: Option<u8>,
}

/// An account this close to any limit could stall a new session.
const NEAR_LIMIT: f64 = 90.0;

/// The room below an account's cap that Auto leaves on the account whose week resets soonest
/// rather than spend it first: the sessions already running there go on using it, and with every
/// start sent their way too they'd run into the limit mid-turn (asked 2026-10-12).
const KEPT_FOR_RUNNING: f64 = 5.0;

fn cap_pct(cap: Option<u8>) -> f64 {
    cap.map(f64::from).unwrap_or(NEAR_LIMIT)
}

/// A limit with no room to start or move a session onto.
fn near(m: &Meter, cap: Option<u8>) -> bool {
    match cap {
        Some(c) => m.percent >= f64::from(c),
        None => m.severity == Severity::Critical || m.percent >= NEAR_LIMIT,
    }
}

/// Whether an account's meters say it's near a limit a session on `model` spends, against `cap`
/// (`NEAR_LIMIT` when None).
pub fn near_limit(u: &Usage, model: Option<&str>, cap: Option<u8>) -> bool {
    u.meters.iter().any(|m| spends(model, m.model.as_deref(), &u.meters) && near(m, cap))
}

/// How long a refusal at a limit counts as word that its account is out: a month covers a
/// monthly spend limit, which says nothing of when it lifts.
const REFUSED_FOR: i64 = 31 * 24 * 3_600_000;

/// The accounts a usage limit has stopped, by what their sessions' last turns say: a session of
/// it was refused (`Session::limit`), the limit hasn't lifted by its harness's word, and no
/// session of it has ended a turn without one since. The meters can't say this alone: a spend
/// limit has none, and they're read every few minutes.
pub fn refused_accounts(sessions: &[Session], now: i64) -> std::collections::HashSet<String> {
    let mut refused: std::collections::HashMap<&str, i64> = std::collections::HashMap::new();
    for s in sessions {
        let Some(l) = s.limit.as_ref().filter(|l| !l.resets_at.is_some_and(|r| r <= now) && now - l.at < REFUSED_FOR) else { continue };
        let at = refused.entry(s.account.as_str()).or_insert(l.at);
        *at = (*at).max(l.at);
    }
    // A working session hasn't said yet whether it got through.
    let through = |acct: &str, at: i64| sessions.iter().any(|s| s.account == acct && s.limit.is_none() && s.status != crate::model::Status::Working && s.last_activity > at);
    refused.into_iter().filter(|&(acct, at)| !through(acct, at)).map(|(acct, _)| acct.to_string()).collect()
}

/// For each session a usage limit stopped, the account it could move to and go on
/// (`SessionLimit::move_to`): another of its harness's accounts, chosen as Auto chooses
/// (`auto_pick`) among those with room for its model by their meters and that no limit has
/// stopped by what their sessions say (`refused_accounts`). None when no account has room,
/// its harness can't move sessions (`moves`), or the limit has lifted, when it can go on where it is.
pub fn limit_moves(sessions: &mut [Session], accounts: &[AccountCfg], usages: &[Usage], moves: impl Fn(Harness) -> bool, now: i64) {
    let refused = refused_accounts(sessions, now);
    for s in sessions.iter_mut() {
        let Some(limit) = s.limit.as_mut() else { continue };
        limit.move_to = None;
        if !moves(s.harness) || limit.resets_at.is_some_and(|r| r <= now) {
            continue;
        }
        let model = limit.model.as_deref();
        let candidates: Vec<Candidate> = accounts
            .iter()
            .filter(|a| a.name != s.account && !a.disabled && Harness::from_config(&a.harness) == s.harness && !refused.contains(&a.name))
            .filter(|a| !usages.iter().any(|u| u.account == a.name && near_limit(u, model, a.usage_cap)))
            .map(|a| Candidate { name: &a.name, preferred: true, model, usage_cap: a.usage_cap })
            .collect();
        limit.move_to = auto_pick(&candidates, usages, now).map(|p| p.account);
    }
}

/// The accounts Auto prefers among those that can start sessions (`startable`, in config
/// order): those of the first account's harness, so Auto keeps to one agent until its accounts
/// are out of room. `src/start.ts`'s `preferredNames` has the same rule.
pub fn preferred_names<'a>(startable: &[&'a AccountCfg]) -> Vec<&'a str> {
    let first = startable.first().map(|a| a.harness.as_str());
    startable.iter().filter(|a| Some(a.harness.as_str()) == first).map(|a| a.name.as_str()).collect()
}

/// The families of models with weekly limits of their own on Claude plans.
const FAMILIES: [&str; 4] = ["fable", "opus", "sonnet", "haiku"];

/// The families a limit for `limit_model` is for: one, or several joined by ` & ` as
/// Antigravity's `Claude & GPT` is, lowercased.
fn families_of(limit_model: &str) -> impl Iterator<Item = String> + '_ {
    limit_model.split(" & ").map(|f| f.trim().to_lowercase()).filter(|f| !f.is_empty())
}

/// Whether a session on `model` spends a limit for `limit_model` (None: every model), one of an
/// account's `meters`: the model's name has one of the limit's families. A model whose name has
/// none of Claude's families nor any the account's limits are for could be any of them, so it
/// spends every limit. `src/start.ts`'s `metersFor` has the same rule.
pub fn spends(model: Option<&str>, limit_model: Option<&str>, meters: &[Meter]) -> bool {
    let m = model.unwrap_or("").to_lowercase();
    let Some(limit) = limit_model else { return true };
    if families_of(limit).any(|f| m.contains(&f)) {
        return true;
    }
    let named = FAMILIES.iter().any(|f| m.contains(f)) || meters.iter().filter_map(|x| x.model.as_deref()).flat_map(families_of).any(|f| m.contains(&f));
    !named
}

/// The account Auto starts, from `candidates` in config order. Only the limits each one's model
/// spends count, and an account at its usage cap (`usage_cap`, default 90%) of one of them has no
/// room. Preferred accounts with room go first, then a preferred one without numbers yet, then the
/// others the same way, and only when all are near a limit the least full of them. Among accounts
/// with room, the one whose weekly limit resets soonest wins, so no week's room goes unused, while
/// its fullest limit is under `cap - KEPT_FOR_RUNNING`; without such a reset, the one whose
/// fullest limit is least full. Signed-out accounts are passed over. `src/start.ts`'s `autoPick`
/// has the same rule.
pub fn auto_pick(candidates: &[Candidate], usages: &[Usage], now: i64) -> Option<AutoPick> {
    auto_pick_indexed(candidates, usages, now).map(|(_, p)| p)
}

fn auto_pick_indexed(candidates: &[Candidate], usages: &[Usage], now: i64) -> Option<(usize, AutoPick)> {
    struct Measured<'a> {
        idx: usize,
        name: &'a str,
        preferred: bool,
        full: f64,
        cap: f64,
        near: bool,
        reset: Option<i64>,
    }
    let mut measured: Vec<Measured> = Vec::new();
    let mut unknown: Vec<(usize, &Candidate)> = Vec::new();
    for (idx, c) in candidates.iter().enumerate() {
        let u = usages.iter().find(|u| u.account == c.name);
        if u.is_some_and(|u| u.needs_sign_in) {
            continue;
        }
        let meters: Vec<&Meter> = u.map(|u| u.meters.iter().filter(|m| spends(c.model, m.model.as_deref(), &u.meters)).collect()).unwrap_or_default();
        if meters.is_empty() {
            unknown.push((idx, c));
            continue;
        }
        measured.push(Measured {
            idx,
            name: c.name,
            preferred: c.preferred,
            full: meters.iter().map(|m| m.percent).fold(0.0, f64::max),
            cap: cap_pct(c.usage_cap),
            near: meters.iter().any(|m| near(m, c.usage_cap)),
            reset: meters.iter().filter(|m| m.key == "week" || m.key.starts_with("model:")).filter_map(|m| m.resets_at).filter(|&at| at > now).min(),
        });
    }
    let least_full = |pool: &[&Measured]| pool.iter().min_by(|a, b| a.full.total_cmp(&b.full)).map(|m| (m.idx, AutoPick { account: m.name.to_string(), reason: AutoReason::Room, resets_at: None }));
    let best = |pool: Vec<&Measured>| {
        let soonest = pool.iter().filter(|m| m.reset.is_some() && m.full < (m.cap - KEPT_FOR_RUNNING).max(0.0)).min_by(|a, b| a.reset.cmp(&b.reset).then(a.full.total_cmp(&b.full)));
        match soonest {
            Some(m) => Some((m.idx, AutoPick { account: m.name.to_string(), reason: AutoReason::Reset, resets_at: m.reset })),
            None => least_full(&pool),
        }
    };
    let first = |(idx, c): (usize, &Candidate)| (idx, AutoPick { account: c.name.to_string(), reason: AutoReason::Reset.into_first(), resets_at: None });
    for preferred in [true, false] {
        let roomy: Vec<&Measured> = measured.iter().filter(|m| m.preferred == preferred && !m.near).collect();
        if !roomy.is_empty() {
            return best(roomy);
        }
        if let Some(&u) = unknown.iter().find(|(_, c)| c.preferred == preferred) {
            return Some(first(u));
        }
    }
    let all: Vec<&Measured> = measured.iter().collect();
    least_full(&all).or_else(|| candidates.iter().enumerate().find(|(_, c)| c.preferred).or_else(|| candidates.first().map(|c| (0, c))).map(first))
}

impl AutoReason {
    fn into_first(self) -> Self {
        AutoReason::First
    }
}

/// An account and optional model chosen from a routing group.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupPick {
    pub account: String,
    pub model: Option<String>,
}

/// The sessions on this Mac's `lanes`, flattened.
pub fn all_sessions(lanes: &[Lane]) -> Vec<Session> {
    lanes.iter().filter(|l| l.device.is_none()).flat_map(|l| l.sessions.iter().cloned()).collect()
}

/// The accounts, harnesses and models of sessions on `lanes`, with when each started: review
/// sessions (`Helper::pr`) when `reviews_only` is true, else your own sessions (`helper` none).
pub fn recent_sessions(lanes: &[Lane], reviews_only: bool) -> Vec<(String, Harness, Option<String>, i64)> {
    lanes
        .iter()
        .filter(|l| l.device.is_none())
        .flat_map(|l| &l.sessions)
        .filter(|s| s.visitor.is_none() && if reviews_only { s.helper.as_ref().is_some_and(|h| h.pr.is_some()) } else { s.helper.is_none() })
        .map(|s| (s.account.clone(), s.harness, s.model.clone(), s.started_at.unwrap_or(s.last_activity)))
        .collect()
}

fn model_matches(session_model: &str, member_model: &str) -> bool {
    let sm = session_model.to_ascii_lowercase();
    let mm = member_model.to_ascii_lowercase();
    let sf = FAMILIES.iter().copied().find(|f| sm.contains(f));
    let mf = FAMILIES.iter().copied().find(|f| mm.contains(f));
    sm == mm
        || sm.contains(&mm)
        || mm.contains(&sm)
        || (sf.is_some() && sf == mf && sm.ends_with("[1m]") == mm.ends_with("[1m]"))
}

/// Picks an account and model from `group` according to its strategy (`usage`,
/// `alternate-harness`, or `round-robin`), passing over accounts that are turned off, refused at a
/// limit (`refused_accounts`), or at their `usage_cap`, and keeping off `avoid` whenever another
/// harness in the group can take the start.
#[allow(clippy::too_many_arguments)]
pub fn pick_from_group(
    group: &config::RoutingGroupCfg,
    cfg: &Config,
    usages: &[Usage],
    sessions: &[Session],
    recent: &[(String, Harness, Option<String>, i64)],
    places: &[Place],
    model: Option<&str>,
    avoid: Option<Harness>,
    now: i64,
    keep: impl Fn(&AccountCfg) -> bool,
) -> Option<GroupPick> {
    pick_from_group_in(group, cfg, usages, sessions, recent, places, model, avoid, now, &home(), keep)
}

#[allow(clippy::too_many_arguments)]
pub fn pick_from_group_in(
    group: &config::RoutingGroupCfg,
    cfg: &Config,
    usages: &[Usage],
    sessions: &[Session],
    recent: &[(String, Harness, Option<String>, i64)],
    places: &[Place],
    model: Option<&str>,
    avoid: Option<Harness>,
    now: i64,
    home: &Path,
    keep: impl Fn(&AccountCfg) -> bool,
) -> Option<GroupPick> {
    struct Member<'a> {
        account: &'a AccountCfg,
        harness: Harness,
        preferred: bool,
        explicit_model: Option<String>,
        check_model: Option<String>,
        roomy: bool,
        usable: bool,
    }
    let refused = refused_accounts(sessions, now);
    let startable: Vec<(&AccountCfg, Vec<Place>)> = cfg
        .accounts
        .iter()
        .filter(|a| !a.disabled && keep(a))
        .filter_map(|a| ordered_places(a, home).ok().filter(|p| !p.is_empty()).map(|p| (a, p)))
        .collect();
    let preferred = preferred_names(&startable.iter().map(|(a, _)| *a).collect::<Vec<_>>());
    let raw: Vec<(&AccountCfg, &[Place], Option<&str>, bool)> = if group.members.is_empty() {
        startable.iter().map(|(a, ps)| (*a, ps.as_slice(), None, preferred.contains(&a.name.as_str()))).collect()
    } else {
        group
            .members
            .iter()
            .filter_map(|m| {
                let (a, ps) = startable.iter().find(|(a, _)| a.name == m.account)?;
                let member_model = Some(m.model.trim()).filter(|s| !s.is_empty());
                Some((*a, ps.as_slice(), member_model, true))
            })
            .collect()
    };
    let asked_model = model.map(str::trim).filter(|s| !s.is_empty());
    let members: Vec<Member> = raw
        .into_iter()
        .filter(|(a, offered, _, _)| {
            (places.is_empty() || places.iter().any(|p| offered.contains(p)))
                && asked_model.is_none_or(|m| crate::harness::of(a).is_some_and(|h| h.offers_model(a, m)))
        })
        .map(|(a, _, member_model, pref)| {
            let explicit_model = asked_model.or(member_model).map(str::to_string);
            let check_model = explicit_model.clone().or_else(|| crate::harness::of(a).and_then(|h| h.default_model(a)));
            let u = usages.iter().find(|u| u.account == a.name);
            let usable = !u.is_some_and(|u| u.needs_sign_in) && !refused.contains(&a.name);
            let roomy = usable && !u.is_some_and(|u| near_limit(u, check_model.as_deref(), a.usage_cap));
            Member {
                account: a,
                harness: harness_of(a),
                preferred: pref,
                explicit_model,
                check_model,
                roomy,
                usable,
            }
        })
        .collect();
    if members.is_empty() {
        return None;
    }
    let roomy: Vec<&Member> = members.iter().filter(|m| m.roomy).collect();
    let mut pool = if !roomy.is_empty() {
        roomy
    } else {
        let usable: Vec<&Member> = members.iter().filter(|m| m.usable).collect();
        if !usable.is_empty() { usable } else { members.iter().collect() }
    };
    if let Some(h) = avoid {
        if pool.iter().any(|m| m.harness != h) {
            pool.retain(|m| m.harness != h);
        }
    }
    let by_usage = |slice: &[&Member]| -> Option<GroupPick> {
        let cs: Vec<Candidate> = slice
            .iter()
            .map(|m| Candidate { name: &m.account.name, preferred: m.preferred, model: m.check_model.as_deref(), usage_cap: m.account.usage_cap })
            .collect();
        let (idx, _) = auto_pick_indexed(&cs, usages, now)?;
        let m = slice.get(idx)?;
        Some(GroupPick { account: m.account.name.clone(), model: m.explicit_model.clone() })
    };
    match group.strategy {
        config::RoutingStrategy::Usage => by_usage(&pool),
        config::RoutingStrategy::AlternateHarness => {
            let mut harnesses: Vec<Harness> = Vec::new();
            for m in &pool {
                if !harnesses.contains(&m.harness) {
                    harnesses.push(m.harness);
                }
            }
            let last_harness = |h: Harness| recent.iter().filter(|(_, rh, _, _)| *rh == h).map(|(_, _, _, at)| *at).max().unwrap_or(0);
            let chosen = *harnesses.iter().min_by_key(|&&h| last_harness(h))?;
            let in_harness: Vec<&Member> = pool.into_iter().filter(|m| m.harness == chosen).collect();
            by_usage(&in_harness)
        }
        config::RoutingStrategy::RoundRobin => {
            let last_member = |m: &Member| {
                let shared = pool.iter().filter(|x| x.account.name == m.account.name).count() > 1;
                recent
                    .iter()
                    .filter(|(ra, _, rm, _)| {
                        if ra != &m.account.name {
                            return false;
                        }
                        if !shared {
                            return true;
                        }
                        if let Some(em) = m.explicit_model.as_deref().or(m.check_model.as_deref()) {
                            rm.as_deref().is_some_and(|sm| model_matches(sm, em))
                        } else {
                            rm.as_deref().is_none_or(|sm| {
                                !pool
                                    .iter()
                                    .filter(|x| x.account.name == m.account.name)
                                    .filter_map(|x| x.explicit_model.as_deref())
                                    .any(|em| model_matches(sm, em))
                            })
                        }
                    })
                    .map(|(_, _, _, at)| *at)
                    .max()
                    .unwrap_or(0)
            };
            let m = pool.iter().min_by_key(|m| last_member(m))?;
            Some(GroupPick { account: m.account.name.clone(), model: m.explicit_model.clone() })
        }
    }
}

/// Resolves `"auto"` (or empty) or a routing group's name in `cfg.routing.groups` to the account
/// and model it picks; None when `name` is a single account rather than Auto or a group.
#[allow(clippy::too_many_arguments)]
pub fn resolve_group(
    cfg: &Config,
    board: Option<&Board>,
    name: &str,
    places: &[Place],
    model: Option<&str>,
    avoid: Option<Harness>,
    reviews_only: bool,
    now: i64,
    keep: impl Fn(&AccountCfg) -> bool,
) -> Option<std::result::Result<GroupPick, String>> {
    let trimmed = name.trim();
    let default_group;
    let group = if trimmed.is_empty() || trimmed == "auto" {
        let auto_name = cfg.routing.auto.trim();
        if !auto_name.is_empty() {
            cfg.routing.groups.iter().find(|g| g.name == auto_name)?
        } else {
            default_group = config::RoutingGroupCfg { name: "auto".into(), strategy: config::RoutingStrategy::Usage, members: Vec::new() };
            &default_group
        }
    } else {
        cfg.routing.groups.iter().find(|g| g.name == trimmed)?
    };
    let usages = board.map(|b| b.usage.as_slice()).unwrap_or(&[]);
    let sessions = board.map(|b| all_sessions(&b.lanes)).unwrap_or_default();
    let recent = board.map(|b| recent_sessions(&b.lanes, reviews_only)).unwrap_or_default();
    Some(pick_from_group(group, cfg, usages, &sessions, &recent, places, model, avoid, now, keep).ok_or_else(|| format!("no account in {} can start a session there", group.name)))
}

/// Auto's account, by the quick panel's rule (`auto_pick` or `[routing] auto`): among the accounts
/// that can start a session (in one of `places`, when any are given; on `model`, when given, for
/// the accounts whose command line takes it), those of the first account's harness with room first,
/// and among those the one whose week resets soonest.
#[cfg(test)]
pub fn pick_account(cfg: &Config, usages: &[Usage], places: &[Place], model: Option<&str>, now: i64) -> Option<String> {
    pick_account_where(cfg, usages, places, model, now, |_| true)
}

/// `pick_account` among the accounts `keep` takes: the reviewer of a PR of yours, among those of
/// another harness than the session that opened it (autoreview.rs).
pub fn pick_account_where(cfg: &Config, usages: &[Usage], places: &[Place], model: Option<&str>, now: i64, keep: impl Fn(&AccountCfg) -> bool) -> Option<String> {
    let default_group = config::RoutingGroupCfg { name: "auto".into(), strategy: config::RoutingStrategy::Usage, members: Vec::new() };
    let group = Some(cfg.routing.auto.trim())
        .filter(|s| !s.is_empty())
        .and_then(|n| cfg.routing.groups.iter().find(|g| g.name == n))
        .unwrap_or(&default_group);
    pick_from_group(group, cfg, usages, &[], &[], places, model, None, now, keep).map(|p| p.account)
}

/// A model name safe to type into the shell inside single quotes: letters, digits and `-._:[]`.
/// The quotes matter: zsh would read `fable[1m]` as a file pattern.
pub fn valid_model(m: &str) -> bool {
    !m.is_empty() && m.len() <= 80 && m.chars().all(|c| c.is_ascii_alphanumeric() || "-._:[]".contains(c))
}

/// The effort levels the harnesses' command lines take between them, lowest first.
pub fn effort_levels() -> Vec<&'static str> {
    let mut out = Vec::new();
    for adapter in crate::harness::all() {
        for (id, _) in adapter.efforts() {
            if !out.contains(id) {
                out.push(*id);
            }
        }
    }
    out
}

/// The sheet's options for the accounts in `cfg` that aren't turned off.
pub fn options_for(cfg: &Config) -> StartOptions {
    let on: Vec<AccountCfg> = cfg.accounts.iter().filter(|a| !a.disabled).cloned().collect();
    let projects = on.iter().flat_map(|a| crate::harness::of(a).map(|h| h.app_projects(a)).unwrap_or_default()).collect();
    let mut out = options(&on, &launch::terminal_name(&cfg.apps.terminal), projects, &home(), |a| crate::harness::of(a).and_then(|h| h.default_model(a)));
    out.auto_group = cfg.routing.auto.trim().to_string();
    out.groups = cfg
        .routing
        .groups
        .iter()
        .map(|g| RoutingGroup {
            name: g.name.clone(),
            strategy: g.strategy.word().to_string(),
            members: g
                .members
                .iter()
                .map(|m| GroupMember { account: m.account.clone(), model: m.model.trim().to_string() })
                .collect(),
        })
        .collect();
    out
}

/// The sheet's options for the accounts config.toml has now: what a window's new-session sheet
/// and another device's get.
pub fn options_here() -> Result<StartOptions> {
    Ok(options_for(&config::current()?))
}

/// The launch for one start, as its harness takes it (`Adapter::start_in_terminal`,
/// `background_command`, `app_link`): a command line gets the prompt, with its attachments
/// written out the way that harness reads files, and the model, effort and new worktree it
/// offers; an app gets a link that opens its new-session page with the folder and prompt filled
/// in, and you send it there. `session_id` is for a harness that takes an id Codealong picks,
/// so the board knows the session the moment it appears.
#[allow(clippy::too_many_arguments)]
pub fn plan(account: &AccountCfg, place: Place, folder: &Path, prompt: &str, session_id: &str, model: Option<&str>, effort: Option<&str>, worktree: bool) -> Plan {
    plan_in(account, place, folder, prompt, session_id, model, effort, worktree, (&attachments::images_dir(), &attachments::files_dir()))
}

/// What a start asks of an account's harness: only what the harness offers gets through.
#[allow(clippy::too_many_arguments)]
fn asked<'a>(adapter: &dyn Adapter, prompt: &'a str, session_id: &'a str, model: Option<&'a str>, effort: Option<&'a str>, worktree: bool, (images_dir, files_dir): (&'a Path, &'a Path)) -> StartAsk<'a> {
    StartAsk {
        prompt,
        session_id,
        model: model.filter(|m| valid_model(m) && adapter.takes_model()),
        effort: effort.filter(|e| adapter.efforts().iter().any(|(id, _)| id == e)),
        worktree: worktree && adapter.makes_worktrees(),
        images_dir,
        files_dir,
    }
}

#[allow(clippy::too_many_arguments)]
fn plan_in(account: &AccountCfg, place: Place, folder: &Path, prompt: &str, session_id: &str, model: Option<&str>, effort: Option<&str>, worktree: bool, dirs: (&Path, &Path)) -> Plan {
    let adapter = adapter_of(account);
    let ask = asked(adapter, prompt, session_id, model, effort, worktree, dirs);
    let cwd = folder.to_path_buf();
    match place {
        Place::Background if adapter.runs_in_background() => Plan::Background { cwd, command: adapter.background_command(account, &ask) },
        Place::Terminal | Place::Background => {
            let typed = adapter.start_in_terminal(account, &ask);
            Plan::Terminal { cwd, input: typed.input, env: typed.env, behind: place == Place::Background }
        }
        Place::ClaudeApp => Plan::ClaudeApp { url: adapter.app_link(folder, &ask).unwrap_or_default() },
        Place::CodexApp => Plan::CodexApp { url: adapter.app_link(folder, &ask).unwrap_or_default() },
        // The agents take the prompt as text, so attachments go as paths they can read.
        Place::Codealong => Plan::Hosted { account: account.name.clone(), cwd, prompt: hosted_prompt(&ask), model: ask.model.map(String::from), effort: ask.effort.map(String::from) },
    }
}

fn hosted_prompt(ask: &StartAsk) -> String {
    attachments::expand_in(ask.prompt, ask.images_dir, ask.files_dir, Style::Path).trim().to_string()
}

/// `url` with one more query parameter, when it has a value.
pub fn with_param(url: String, key: &str, value: &str) -> String {
    if value.is_empty() {
        url
    } else {
        format!("{url}&{key}={}", urlencoding::encode(value))
    }
}

/// Claude Code reads an argument that starts with `-` as an option, and one that is only a
/// command's name (`update`, `doctor`) as that command. A space in front of the one and after
/// the other keeps each a prompt.
pub fn as_argument(prompt: &str) -> String {
    let mut p = prompt.to_string();
    if p.starts_with('-') {
        p.insert(0, ' ');
    }
    if !p.trim().contains(char::is_whitespace) {
        p.push(' ');
    }
    p
}

/// A random version 4 UUID, the form `--session-id` takes.
pub fn new_session_id() -> String {
    let mut b = [0u8; 16];
    // SAFETY: arc4random_buf writes exactly the length it's given into the buffer.
    unsafe { libc::arc4random_buf(b.as_mut_ptr().cast(), b.len()) };
    uuid_v4(b)
}

fn uuid_v4(mut b: [u8; 16]) -> String {
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// Whether a start could go into a new worktree: on the command line of a harness that makes
/// one (`Adapter::makes_worktrees`), in a terminal or in the background, in a repo's worktree.
pub fn offers_worktree(board: &Board, cfg: &Config, spec: &StartSpec) -> bool {
    let chats = board.lanes.iter().any(|l| l.id == spec.lane_id && l.starts_chats);
    !chats && matches!(spec.place, Place::Terminal | Place::Background) && cfg.accounts.iter().any(|a| a.name == spec.account && adapter_of(a).makes_worktrees())
}

/// A new folder among Codealong's chats (`chats`, `config::chats_dir`) for a session that
/// belongs to no project: named for the day and a few random letters, as the Claude app names
/// its scratch workspaces, so they list in the order they were made. `make_chat_folder` makes it.
fn chat_folder_in(chats: &Path) -> PathBuf {
    let (y, m, d) = crate::util::civil_from_days(crate::util::today_local());
    chats.join(format!("{y:04}-{m:02}-{d:02}-{}", &new_session_id()[..6]))
}

fn make_chat_folder(folder: &Path) -> Result<()> {
    std::fs::create_dir_all(folder).map_err(|e| anyhow!("Couldn't make a folder for it in {}: {e}", tilde(folder.parent().unwrap_or(folder))))
}

/// Starts a session: checks the choice against the board and the config, then hands it to the
/// terminal or the app, the way the account's harness starts one.
pub fn start(board: &Board, cfg: &Config, spec: &StartSpec) -> Result<Started> {
    let lane = board.lanes.iter().find(|l| l.id == spec.lane_id).ok_or_else(|| anyhow!("That worktree just changed. Try again."))?;
    let account = cfg.accounts.iter().find(|a| a.name == spec.account).ok_or_else(|| anyhow!("There's no account named {}.", spec.account))?;
    account.ensure_on()?;
    let offered = ordered_places(account, &home());
    if !offered.as_ref().is_ok_and(|ps| ps.contains(&spec.place)) {
        bail!(offered.err().unwrap_or_else(|| format!("{} can't start a session there.", account.name)));
    }
    // A session in no project gets a folder of its own among Codealong's chats, made once the
    // start is known to be good.
    let chat = lane.starts_chats.then(|| chat_folder_in(Path::new(&lane.path)));
    let folder = chat.as_deref().unwrap_or(Path::new(&lane.path));
    if chat.is_none() && !folder.is_dir() {
        bail!("{} isn't there any more.", tilde(folder));
    }
    let adapter = adapter_of(account);
    let word = adapter.harness().word();
    let (model, effort) = tuning(adapter, spec.place, spec.model.as_deref(), spec.effort.as_deref())?;
    // A new worktree goes where the harness's command line takes it.
    let on_command_line = matches!(spec.place, Place::Terminal | Place::Background);
    let worktree = spec.worktree && on_command_line && chat.is_none();
    if chat.is_some() {
        make_chat_folder(folder)?;
    }
    if on_command_line {
        adapter.before_start(account, folder);
    }
    let id = new_session_id();
    let dirs = (attachments::images_dir(), attachments::files_dir());
    let ask = asked(adapter, &spec.prompt, &id, model, effort, worktree, (&dirs.0, &dirs.1));
    match spec.place {
        Place::Background if adapter.runs_in_background() => adapter.start_in_background(account, folder, &adapter.background_command(account, &ask)),
        // A harness with no background service of its own starts in its terminal, behind your
        // windows, so you stay where you are.
        Place::Terminal | Place::Background => {
            let behind = spec.place == Place::Background;
            let typed = adapter.start_in_terminal(account, &ask);
            let near = crate::board::terminal_neighbours(&board.lanes, folder);
            // It has no title yet, so its tab goes by what it was asked.
            let label = crate::board::tab_label(&board.lanes, folder, &spec.prompt);
            launch::terminal_run(&cfg.apps.terminal, launch::Placement { new_terminal: cfg.apps.new_terminal, near: &near, behind, label: label.as_deref() }, folder, &typed.input, &typed.env)?;
            // A harness that names its own sessions shows one on the board once it's written.
            let message = typed.key.is_none().then(|| {
                if behind {
                    format!("{word} is starting in {}, in {} behind your windows.", lane.callsign, launch::terminal_name(&cfg.apps.terminal))
                } else {
                    format!("{word} is starting in {}.", lane.callsign)
                }
            });
            Ok(Started { key: typed.key, message })
        }
        Place::ClaudeApp | Place::CodexApp => {
            let link = adapter.app_link(folder, &ask).ok_or_else(|| anyhow!("{} can't start a session there.", account.name))?;
            adapter.start_in_app(account, &lane.callsign, folder, &spec.prompt, &link)
        }
        Place::Codealong => {
            let key = crate::acp::start(account, folder, &hosted_prompt(&ask), ask.model, ask.effort)?;
            Ok(Started { key: Some(key), message: None })
        }
    }
}

/// The model and effort asked for, where `place` takes them: a model, an effort level and a new
/// worktree go where the harness's command line takes them, in a terminal or the background,
/// or through its adapter under Codealong, which takes the same names. The apps pick their own,
/// so there they're left out. One the harness doesn't take is an error.
fn tuning<'a>(adapter: &dyn Adapter, place: Place, model: Option<&'a str>, effort: Option<&'a str>) -> Result<(Option<&'a str>, Option<&'a str>)> {
    let word = adapter.harness().word();
    let tuned = matches!(place, Place::Terminal | Place::Background | Place::Codealong) && adapter.takes_model();
    let model = model.map(str::trim).filter(|m| !m.is_empty() && tuned);
    let effort = effort.map(str::trim).filter(|e| !e.is_empty() && tuned && !adapter.efforts().is_empty());
    if let Some(m) = model.filter(|m| !valid_model(m)) {
        bail!("{m:?} isn't a model name Codealong can pass to {word}.");
    }
    if let Some(e) = effort.filter(|e| !adapter.efforts().iter().any(|(id, _)| id == e)) {
        bail!("{e:?} isn't an effort level; {word} takes {}.", adapter.efforts().iter().map(|(id, _)| *id).collect::<Vec<_>>().join(", "));
    }
    Ok((model, effort))
}

/// What a session whose last turn was cut off (`Session::cut_off`) is told as it's resumed with
/// nothing else to say. A command its last step ran may have been stopped partway.
pub const CARRY_ON: &str = "Your last turn was cut off partway: what was running you ended (its terminal closed, Codealong or the Mac restarted, or it was killed). Check what your last step did, since a command it ran may have been stopped, then carry on where you left off.";

/// How a session that wasn't running went on: the key it goes on under (a copy's, where its
/// harness copied it; none where the harness hasn't said yet), and what to say of it, as a send
/// says how it went.
#[derive(Debug, Clone)]
pub struct Resumed {
    pub key: Option<String>,
    pub sent: crate::send::Sent,
}

/// Goes on with a session that isn't running where `how` says, of the places its account starts
/// sessions in (`resume_places`): a new terminal; Background, under the harness's background
/// service where it has one, else in a terminal behind your windows; its app; or Codealong,
/// which runs it from then on. On the model and effort where the place takes them, as a start
/// is, and on `prompt` when there's one, else waiting for one; one whose last turn was cut off
/// is told to carry on instead (`CARRY_ON`). What a box's send to a session
/// that isn't running comes to (send.rs `Route::Resume`), and Resume in, in its menu. Only as
/// `sender` may: never for an agent (`send::ensure_may_resume`).
pub fn resume(board: &Board, cfg: &Config, key: &str, how: &ResumeIn, prompt: &str, sender: &crate::send::Sender) -> Result<Resumed> {
    // Before the folder is readied for the harness (`to_fork`), let alone anything started.
    crate::send::ensure_not_moved(&crate::origins::current(), board, key)?;
    if let Some((_, s)) = board.session(key).filter(|(_, s)| !s.live) {
        crate::send::ensure_may_resume(sender, &s.title)?;
    }
    let (s, account, _, cwd) = to_fork(board, cfg, key)?;
    let title = format!("“{}”", s.title);
    if s.live {
        bail!("{title} is running already; send it a message instead.");
    }
    let adapter = crate::harness::adapter(s.harness);
    if !resume_places(account, &home()).contains(&how.place) {
        bail!("{title} can't go on {} from here.", place_words(how.place, cfg));
    }
    let (model, effort) = tuning(adapter, how.place, how.model.as_deref(), how.effort.as_deref())?;
    let (prompt, carries_on) = resume_prompt(prompt, s);
    let dirs = (attachments::images_dir(), attachments::files_dir());
    let ask = asked(adapter, prompt, &s.id, model, effort, false, (&dirs.0, &dirs.1));
    let ctx = crate::harness::Ctx { account };
    let prompted = !prompt.trim().is_empty();
    let on = model.map(|m| format!(" on {}", model_label(adapter, account, m))).unwrap_or_default();
    let (goes_on, message) = match how.place {
        Place::Background if adapter.runs_in_background() => {
            let job = adapter.resume_in_background(&ctx, s, &cwd, &ask)?;
            let copied = job.as_deref().is_some_and(|k| k != key);
            if let Some(copy) = job.as_deref().filter(|_| copied) {
                crate::origins::record(copy, key, crate::model::OriginHow::Fork);
            }
            let copy_note = if copied { " Claude Code copies a session a background job had before into a new job, so the copy takes its place." } else { "" };
            let carry = if carries_on { ", carrying on the turn that was cut off" } else { "" };
            (job, format!("{title} goes on in the background{on}{carry}. Attach a terminal any time.{copy_note}"))
        }
        Place::Terminal | Place::Background => {
            let behind = how.place == Place::Background;
            let (input, env) = adapter.resume(account, s, &ask)?;
            let near = crate::board::terminal_neighbours(&board.lanes, &cwd);
            let label = crate::board::tab_label(&board.lanes, &cwd, &s.title);
            launch::terminal_run(&cfg.apps.terminal, launch::Placement { new_terminal: cfg.apps.new_terminal, near: &near, behind, label: label.as_deref() }, &cwd, &input, &env)?;
            let terminal = launch::terminal_name(&cfg.apps.terminal);
            let behind = if behind { " behind your windows" } else { "" };
            let with = match (carries_on, prompted) {
                (true, _) => ", told to carry on the turn that was cut off",
                (false, true) => " with this prompt",
                (false, false) => "",
            };
            (Some(key.to_string()), format!("Resumed {title} in {terminal}{behind}{on}{with}."))
        }
        Place::ClaudeApp | Place::CodexApp => {
            let Some(crate::send::Route::Own(app)) = adapter.app_way() else { bail!("{title} can't go on in {} from here.", adapter.app()) };
            let sent = crate::send::deliver_yours(app.as_ref(), s, account, prompt)?;
            return Ok(Resumed { key: Some(key.to_string()), sent });
        }
        Place::Codealong => {
            let hosted = crate::acp::adopt(account, &s.id, &cwd, &hosted_prompt(&ask), model, effort)?;
            let then = if prompted { "it's on it" } else { "its panel here takes the next prompt" };
            (Some(hosted), format!("Codealong runs {title} now{on}: {then}."))
        }
    };
    Ok(Resumed { key: goes_on, sent: crate::send::sent(message) })
}

/// What a resume goes on with: the prompt given, else `CARRY_ON` for a session whose last turn
/// was cut off; and whether it's that.
fn resume_prompt<'a>(prompt: &'a str, s: &crate::model::Session) -> (&'a str, bool) {
    if prompt.trim().is_empty() && s.cut_off.is_some() {
        (CARRY_ON, true)
    } else {
        (prompt, false)
    }
}

/// Where a place is, as "can't go on …" says it.
pub(crate) fn place_words(place: Place, cfg: &Config) -> String {
    match place {
        Place::Terminal => format!("in {}", launch::terminal_name(&cfg.apps.terminal)),
        Place::Background => "in the background".into(),
        Place::ClaudeApp => "in the Claude app".into(),
        Place::CodexApp => "in the Codex app".into(),
        Place::Codealong => "in Codealong".into(),
    }
}

/// A model the way the account's list names it (`Opus Plan Mode` for `opusplan`), else as asked for.
pub(crate) fn model_label(adapter: &dyn Adapter, account: &AccountCfg, model: &str) -> String {
    let listed = adapter.models(account).into_iter().find(|m| m.id == model);
    listed.and_then(|m| m.label.split(':').next().map(str::to_string)).unwrap_or_else(|| model.to_string())
}

/// What a start from todos leaves to link them by (`TodoStore::started`): the worktree it's
/// linked to, and the session, when Codealong chose its ID, or the start to wait for.
#[derive(Debug, Clone, PartialEq)]
pub struct Linked {
    pub ids: Vec<String>,
    /// None for a session in a new worktree.
    pub lane: Option<String>,
    pub session: Option<String>,
    pub pending: Option<PendingStart>,
}

/// How a start's todos are linked, or none for a start from no todo. `harness` is the
/// account's, as config.toml names it.
fn linked(spec: &StartSpec, started: &Started, harness: &str, now: i64) -> Option<Linked> {
    if spec.todos.is_empty() {
        return None;
    }
    // One start for all of them: todos started together wait for the same session.
    let pending = started.key.is_none().then(|| PendingStart { lane: spec.lane_id.clone(), harness: harness.to_string(), account: spec.account.clone(), at: now });
    // A session in a new worktree won't be in the one it started from, and Codealong's chats are
    // one lane for every chat, whose later ones would each be told of it: only it is linked.
    let new_tree = spec.worktree && matches!(spec.place, Place::Terminal | Place::Background);
    let lane = (!new_tree && Path::new(&spec.lane_id) != config::chats_dir()).then(|| spec.lane_id.clone());
    Some(Linked { ids: spec.todos.clone(), lane, session: started.key.clone(), pending })
}

/// Starts a session and links it to its todos, each to the session, or to the next one of that
/// account in the repo when an app picks the session's ID. `record` writes the link down: the
/// engine does for the app (`start_here`), and the `codealong` command writes todos.json itself
/// when the app is closed.
pub fn start_and_link(board: &Board, cfg: &Config, spec: &StartSpec, record: impl FnOnce(Linked) -> Result<()>) -> Result<Started> {
    let started = start(board, cfg, spec)?;
    crate::origins::started(spec, started.key.as_deref());
    // `start` found the account, or it would have failed.
    let harness = cfg.accounts.iter().find(|a| a.name == spec.account).map(|a| a.harness.as_str());
    if let Some(link) = harness.and_then(|h| linked(spec, &started, h, now_ms())) {
        record(link)?;
    }
    Ok(started)
}

/// Starts a session on this device and links it to its todos through the engine: a window's
/// start, another device's, and `codealong start` with the app up (server.rs).
pub fn start_here(engine: &Handle, spec: &StartSpec) -> Result<Started> {
    let cfg = config::current()?;
    start_and_link(&engine.board(), &cfg, spec, |link| {
        engine.todo_started(link.ids, link.lane, link.session, link.pending);
        Ok(())
    })
}

/// Keeps whether a start went into a new worktree, where one was on offer, for Jev's next
/// suggestion in that repo (starts.rs).
pub fn note(engine: &Handle, spec: &StartSpec) {
    // Only a note for later: settings that don't read leave nothing to note.
    let Ok(cfg) = config::current() else { return };
    let offered = |b: &Board| offers_worktree(b, &cfg, spec).then(|| b.lanes.iter().find(|l| l.id == spec.lane_id).map(|l| format!("{}/{}", l.project, l.repo))).flatten();
    let Some(repo) = engine.read(offered) else {
        return;
    };
    if let Err(e) = crate::starts::record(&repo, spec.worktree) {
        eprintln!("codealong: couldn't note the start in starts.json: {e:#}");
    }
}

/// A session to fork, move or resume, with its account, the worktree it's in and the folder it
/// goes on in: the session's own, or its worktree's when that's gone.
fn to_fork<'a>(board: &'a Board, cfg: &'a Config, key: &str) -> Result<(&'a crate::model::Session, &'a AccountCfg, &'a crate::model::Lane, PathBuf)> {
    let (s, account) = crate::harness::find(board, cfg, key)?;
    account.ensure_on()?;
    // The id is typed into a shell, and the harnesses' ids are letters, digits, - and _.
    if s.id.is_empty() || !s.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("Codealong won't type {}'s id into a shell: {:?}.", s.title, s.id);
    }
    // Its own worktree, not one it only visits.
    let own = |l: &&crate::model::Lane| l.sessions.iter().any(|x| x.key == key && x.visitor.is_none());
    let holds = |l: &&crate::model::Lane| l.sessions.iter().any(|x| x.key == key);
    let lane = board.lanes.iter().find(own).or_else(|| board.lanes.iter().find(holds)).ok_or_else(|| anyhow!("That session isn't on the board any more."))?;
    let cwd = if Path::new(&s.cwd).is_dir() { PathBuf::from(&s.cwd) } else { PathBuf::from(&lane.path) };
    // An app's session can run where its harness's command line would stop to ask about the folder.
    crate::harness::adapter(s.harness).before_start(account, &cwd);
    Ok((s, account, lane, cwd))
}

/// Forks a session into a new terminal in its folder (`Adapter::fork`): the copy goes on from
/// what the session had written, and the session itself carries on as it was.
pub fn fork(board: &Board, cfg: &Config, key: &str) -> Result<Started> {
    let (s, account, lane, cwd) = to_fork(board, cfg, key)?;
    let fork = crate::harness::adapter(s.harness).fork(&crate::harness::Ctx { account }, s)?;
    let near = crate::board::terminal_neighbours(&board.lanes, &cwd);
    let label = crate::board::tab_label(&board.lanes, &cwd, &s.title);
    launch::terminal_run(&cfg.apps.terminal, launch::Placement { new_terminal: cfg.apps.new_terminal, near: &near, behind: false, label: label.as_deref() }, &cwd, &fork.input, &fork.env)?;
    // One whose harness names the copy shows on the board once it's written.
    let message = fork.key.is_none().then(|| format!("{} is opening a fork of {} in {}.", s.harness.word(), s.title, lane.callsign));
    // Claude Code's transcript doesn't say what a fork into a terminal copied.
    if let Some(copy) = &fork.key {
        crate::origins::record(copy, key, crate::model::OriginHow::Fork);
    }
    Ok(Started { key: fork.key, message })
}

/// Forks a session into a copy that goes on where no window comes forward
/// (`Adapter::fork_background`): on `prompt` when there's one, else waiting for one. You stay
/// where you are; the copy shows on the board beside the session.
pub fn fork_background(board: &Board, cfg: &Config, key: &str, prompt: &str) -> Result<Started> {
    let (s, account, _, cwd) = to_fork(board, cfg, key)?;
    let copy = crate::harness::adapter(s.harness).fork_background(&crate::harness::Ctx { account }, s, &cwd, prompt)?;
    let message = if prompt.trim().is_empty() {
        format!("Forked in the background: the copy of “{}” waits for its first message.", s.title)
    } else {
        format!("Forked in the background: the copy of “{}” is on it.", s.title)
    };
    if let Some(copy) = &copy {
        crate::origins::record(copy, key, crate::model::OriginHow::Fork);
    }
    Ok(Started { key: copy, message: Some(message) })
}

/// Moves a session to `to`, another account of its harness (`Adapter::move_to`): a copy goes on
/// from what it had written, in a new terminal in its folder under `to` (a background job's copy
/// as a job there, `Adapter::move_to_background`), on `prompt` when there's one, and the
/// session itself is closed where Codealong can close it. Not mid-turn: the copy would see the
/// turn cut off, so the caller stops a turn first (actions.rs), and archives the session once
/// this returns.
pub fn move_to(board: &Board, cfg: &Config, key: &str, to: &str, prompt: &str) -> Result<Started> {
    let (s, account, lane, cwd) = to_fork(board, cfg, key)?;
    let target = move_target(cfg, s, account, to)?;
    if crate::harness::turn_running(s) {
        bail!("“{}” is in the middle of a turn. Stop it or let it finish, then move it.", s.title);
    }
    let adapter = crate::harness::adapter(s.harness);
    let ctx = crate::harness::Ctx { account };
    let in_background = s.attach_command.is_some();
    // The move is noted as a fork from another account, which is how the windows tell a move
    // (lineage.ts), so the shared model needs no word for it that older devices wouldn't read.
    // Until the copy's first message the note is all there is of it, and it keeps the copy on the
    // board to go on with however its terminal ends (`claude::Reader::closed_copies`), so the
    // session is closed only once it's written.
    let note = |copy: &str| crate::origins::write(copy, key, crate::model::OriginHow::Fork);
    let copy = if in_background {
        adapter.before_start(target, &cwd);
        let copy = adapter.move_to_background(&ctx, s, target, &cwd, prompt)?;
        // The service picks a job's ID, so it's noted once the job has one.
        if let Some(copy) = &copy {
            note(copy).map_err(|e| anyhow!("Its copy started as a job under {to} ({copy}), but the move couldn't be noted, so “{}” stays where it is: {e:#}", s.title))?;
        }
        copy
    } else {
        let copy = adapter.move_to(&ctx, s, target, prompt)?;
        if let Some(copy) = &copy.key {
            note(copy).map_err(|e| anyhow!("Couldn't note the move, so “{}” stays where it is: {e:#}", s.title))?;
        }
        adapter.before_start(target, &cwd);
        let near = crate::board::terminal_neighbours(&board.lanes, &cwd);
        let label = crate::board::tab_label(&board.lanes, &cwd, &s.title);
        if let Err(e) = launch::terminal_run(&cfg.apps.terminal, launch::Placement { new_terminal: cfg.apps.new_terminal, near: &near, behind: false, label: label.as_deref() }, &cwd, &copy.input, &copy.env) {
            // No copy started, so there's none to keep.
            if let Some(Err(forgot)) = copy.key.as_deref().map(crate::origins::forget) {
                eprintln!("The move's terminal didn't open, and its note couldn't be taken back, so its copy shows stopped: {forgot:#}");
            }
            return Err(e);
        }
        copy.key
    };
    let mut left = if !s.live {
        String::new()
    } else {
        match crate::idle::close_one(board, cfg, key) {
            Ok(_) if in_background => " The old job is stopped.".to_string(),
            Ok(_) => " Its old terminal is closed.".to_string(),
            Err(e) => format!(" The old one is still running: {e:#}"),
        }
    };
    // A command its last turn left running belonged to the old process, and the copy has none.
    if s.background.is_some() {
        left.push_str(" It had a command running in the background: ask the copy to start it again if it's still needed.");
    }
    let place = if in_background { "as a background job".to_string() } else { format!("in a new terminal in {}", lane.callsign) };
    let goes = if prompt.trim().is_empty() { format!("it goes on {place}") } else { format!("it carries on {place}") };
    let message = format!("Moved “{}” to {to}: {goes}.{left}", s.title);
    Ok(Started { key: copy, message: Some(message) })
}

/// Moves a session to another folder, `lane_id`'s, or a new chat when that's Codealong's chats:
/// it goes on there as itself (`Adapter::move_folder`), in a new terminal, or as a background
/// job when it is one, told where it is now. Its own process is closed first, since one
/// conversation runs in one place, and a chat's folder stays readable to it. Not mid-turn: the
/// caller stops a turn first (actions.rs). Where the harness copies a background job rather
/// than move it, the copy's key comes back, and the caller hands the session's place over to it.
pub fn move_to_folder(board: &Board, cfg: &Config, key: &str, lane_id: &str) -> Result<Started> {
    let (s, account) = crate::harness::find(board, cfg, key)?;
    account.ensure_on()?;
    // The id is typed into a shell, and the harnesses' ids are letters, digits, - and _.
    if s.id.is_empty() || !s.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("Codealong won't type {}'s id into a shell: {:?}.", s.title, s.id);
    }
    let to = board.lanes.iter().find(|l| l.id == lane_id).ok_or_else(|| anyhow!("That worktree just changed. Try again."))?;
    if to.device.is_some() {
        bail!("{} is on another device, and a session moves only between folders on its own Mac.", to.callsign);
    }
    let home_lane = board.lanes.iter().find(|l| l.sessions.iter().any(|x| x.key == key && x.visitor.is_none()));
    if home_lane.is_some_and(|l| l.id == to.id) {
        bail!("“{}” is in {} already.", s.title, to.callsign);
    }
    if crate::harness::turn_running(s) {
        bail!("“{}” is in the middle of a turn. Stop it or let it finish, then move it.", s.title);
    }
    let from = if s.cwd.is_empty() { home_lane.map(|l| PathBuf::from(&l.path)).unwrap_or_default() } else { PathBuf::from(&s.cwd) };
    let folder = if to.starts_chats { chat_folder_in(Path::new(&to.path)) } else { PathBuf::from(&to.path) };
    if !to.starts_chats && !folder.is_dir() {
        bail!("{} isn't there any more.", tilde(&folder));
    }
    // What a chat made in its folder may be what it's moved for.
    let keep = (from.starts_with(config::chats_dir()) && from.is_dir()).then_some(from.as_path());
    let prompt = moved_prompt(to, &folder, &from, keep.is_some());
    let adapter = crate::harness::adapter(s.harness);
    let ctx = crate::harness::Ctx { account };
    // Asked before anything is closed, as it says whether the harness can move a session at all.
    let typed = adapter.move_folder(&ctx, s, keep, &prompt)?;
    if to.starts_chats {
        make_chat_folder(&folder)?;
    }
    adapter.before_start(account, &folder);
    let in_background = s.live && s.attach_command.is_some();
    if s.live {
        crate::idle::close_one(board, cfg, key).map_err(|e| anyhow!("Couldn't close “{}” where it runs, so it stays where it is: {e:#}", s.title))?;
        if let Some(pid) = s.pid {
            let since = std::time::Instant::now();
            while crate::procs::pid_alive(pid) && since.elapsed() < std::time::Duration::from_secs(10) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            if crate::procs::pid_alive(pid) {
                bail!("“{}” hasn't closed yet. Move it once it has.", s.title);
            }
        }
    }
    let there = if to.starts_chats { "a chat of its own".to_string() } else { to.callsign.clone() };
    let (goes_on, how) = if in_background {
        (adapter.move_folder_background(&ctx, s, &folder, keep, &prompt)?, "in the background")
    } else {
        let near = crate::board::terminal_neighbours(&board.lanes, &folder);
        let label = crate::board::tab_label(&board.lanes, &folder, &s.title);
        launch::terminal_run(&cfg.apps.terminal, launch::Placement { new_terminal: cfg.apps.new_terminal, near: &near, behind: false, label: label.as_deref() }, &folder, &typed.input, &typed.env)
            .map_err(|e| anyhow!("“{}” is closed, but a terminal didn't open in {there}, so it hasn't moved; resume it from its menu: {e:#}", s.title))?;
        (typed.key, "in a new terminal")
    };
    let copied = goes_on.as_deref().is_some_and(|k| k != key);
    if let Some(copy) = goes_on.as_deref().filter(|_| copied) {
        crate::origins::record(copy, key, crate::model::OriginHow::Fork);
    }
    let copy_note = if copied { " Claude Code copies a background job's conversation into a new job rather than move it, so the copy takes its place." } else { "" };
    let message = format!("Moved “{}” to {there}: it goes on there {how}.{copy_note}", s.title);
    Ok(Started { key: goes_on, message: Some(message) })
}

/// What a session moved to another folder is told as it goes on there: where it is now, what
/// that is, and where it was.
pub fn moved_prompt(to: &Lane, folder: &Path, from: &Path, readable: bool) -> String {
    let what = if to.starts_chats {
        "a folder of its own, outside every project".to_string()
    } else if to.origin == Origin::Main {
        format!("the main checkout of {}", to.repo)
    } else if let Some(branch) = &to.branch {
        format!("a worktree of {} on the branch {branch}", to.repo)
    } else {
        format!("{} in {}", to.callsign, to.project)
    };
    let before = if readable { format!("What you made before is still in {}, where you can read it.", from.display()) } else { format!("You were in {} before.", from.display()) };
    let rules = if to.starts_chats { "" } else { " If the folder keeps instructions for agents, such as a CLAUDE.md or AGENTS.md, read them before you change anything." };
    format!("This conversation has moved to {}, {what}. Work there from now on. {before}{rules}", folder.display())
}

/// What a session stopped at its usage limit is told as it goes on under another account.
pub const GO_ON: &str = "Your last turn was stopped by a usage limit, and this conversation has moved to another account that has room. Carry on where you left off.";

/// The account a session can move to by that name: another account of the session's harness.
fn move_target<'a>(cfg: &'a Config, s: &crate::model::Session, account: &AccountCfg, to: &str) -> Result<&'a AccountCfg> {
    let target = cfg.accounts.iter().find(|a| a.name == to).ok_or_else(|| anyhow!("There's no account named {to} in config.toml."))?;
    if target.name == account.name {
        bail!("“{}” is on {to} already.", s.title);
    }
    if crate::harness::of(target).map(|a| a.harness()) != Some(s.harness) {
        bail!("{to} isn't a {} account; a session moves only between accounts of its own harness.", s.harness.word());
    }
    target.ensure_on()?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_a_moved_session_where_it_is_now() {
        let mut to = Lane::new("/r/app", "Acme", "app");
        to.origin = Origin::Main;
        to.callsign = "app".into();
        let (folder, from) = (Path::new("/r/app"), Path::new("/c/chats/2026-10-05-a1b2c3"));
        assert_eq!(
            moved_prompt(&to, folder, from, true),
            "This conversation has moved to /r/app, the main checkout of app. Work there from now on. What you made before is still in /c/chats/2026-10-05-a1b2c3, where you can read it. If the folder keeps instructions for agents, such as a CLAUDE.md or AGENTS.md, read them before you change anything."
        );
        to.origin = Origin::Script;
        to.branch = Some("login".into());
        assert!(moved_prompt(&to, folder, Path::new("/r/app-2"), false).starts_with("This conversation has moved to /r/app, a worktree of app on the branch login. Work there from now on. You were in /r/app-2 before. If"));
        // Into a chat of its own: nothing to read first.
        let mut chats = Lane::new("/c/chats", "Elsewhere", "Chats");
        chats.starts_chats = true;
        assert_eq!(moved_prompt(&chats, Path::new("/c/chats/2026-10-06-ffffff"), Path::new("/r/app"), false), "This conversation has moved to /c/chats/2026-10-06-ffffff, a folder of its own, outside every project. Work there from now on. You were in /r/app before.");
    }

    #[test]
    fn a_chat_gets_a_folder_of_its_own_named_for_the_day() {
        let root = crate::fixtures::scratch("chats");
        let (a, b) = (chat_folder_in(&root), chat_folder_in(&root));
        make_chat_folder(&a).unwrap();
        make_chat_folder(&b).unwrap();
        assert!(a.is_dir() && b.is_dir() && a != b);
        let name = crate::util::file_name(&a);
        let (y, m, d) = crate::util::civil_from_days(crate::util::today_local());
        assert!(name.starts_with(&format!("{y:04}-{m:02}-{d:02}-")), "{name}");
        assert_eq!(name.len(), "2026-10-05-a1b2c3".len());
    }

    fn account(name: &str, harness: &str, home: &str, command: Option<&str>) -> AccountCfg {
        AccountCfg { name: name.into(), harness: harness.into(), home: home.into(), command: command.map(String::from), start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false }
    }

    fn meter(key: &str, percent: f64, resets_at: Option<i64>) -> Meter {
        let severity = if percent >= 90.0 { Severity::Critical } else { Severity::Normal };
        Meter { key: key.into(), label: key.into(), window: key.into(), percent, resets_at, severity, model: None }
    }

    const NOW: i64 = 1_790_200_000_000;

    #[test]
    fn a_start_from_todos_links_them_to_its_session_or_waits_for_it() {
        let spec = |place: Place, worktree: bool, todos: &[&str]| StartSpec { lane_id: "/r/app".into(), account: "codex".into(), place, prompt: String::new(), model: None, effort: None, worktree, todos: todos.iter().map(|t| t.to_string()).collect(), from: None, helper: false, report: false };
        let known = Started { key: Some("claude:claude:abc".into()), message: None };
        let unknown = Started { key: None, message: None };
        // No todo, nothing to link.
        assert_eq!(linked(&spec(Place::Terminal, false, &[]), &known, "claude", NOW), None);
        // Codealong chose the session's ID: every todo is linked to it and to its worktree at once.
        let link = linked(&spec(Place::Terminal, false, &["t-1", "t-2"]), &known, "claude", NOW).unwrap();
        assert_eq!(link, Linked { ids: vec!["t-1".into(), "t-2".into()], lane: Some("/r/app".into()), session: known.key.clone(), pending: None });
        // An app picks the ID: the todos wait for the account's next session there, by the
        // account's own harness.
        let link = linked(&spec(Place::CodexApp, false, &["t-1"]), &unknown, "codex", NOW).unwrap();
        assert_eq!(link.pending, Some(PendingStart { lane: "/r/app".into(), harness: "codex".into(), account: "codex".into(), at: NOW }));
        assert_eq!((link.lane.as_deref(), link.session), (Some("/r/app"), None));
        // A session in a new worktree isn't in the one it started from, on a command line; the
        // apps have their own worktree choice, which the spec's doesn't reach.
        for place in [Place::Terminal, Place::Background] {
            assert_eq!(linked(&spec(place, true, &["t-1"]), &known, "claude", NOW).unwrap().lane, None);
        }
        assert_eq!(linked(&spec(Place::ClaudeApp, true, &["t-1"]), &unknown, "claude", NOW).unwrap().lane.as_deref(), Some("/r/app"));
        // Codealong's chats are one lane for every chat: a todo started in one is the session's.
        let chat = StartSpec { lane_id: config::chats_dir().to_string_lossy().into_owned(), ..spec(Place::Terminal, false, &["t-1"]) };
        assert_eq!(linked(&chat, &known, "claude", NOW).unwrap().lane, None);
    }
    const HOUR: i64 = 3_600_000;
    const MIN: i64 = 60_000;

    /// An account's 5-hour and weekly use, and in how many hours its week resets.
    fn usage(account: &str, session: f64, week: f64, week_resets_in: Option<i64>) -> Usage {
        Usage {
            account: account.into(),
            harness: Harness::Claude,
            plan: None,
            meters: vec![meter("session", session, Some(NOW + 2 * HOUR)), meter("week", week, week_resets_in.map(|h| NOW + h * HOUR))],
            as_of: Some(NOW),
            error: None,
            needs_sign_in: false,
            loading: false,
            no_limits: false,
            preferred: false,
            rate_limited: false,
        }
    }

    fn limited(key: &str, at: i64, resets_at: Option<i64>) -> Session {
        let limit = crate::model::SessionLimit { at, resets_at, model: Some("claude-opus-5-5".into()), ..Default::default() };
        Session { status: crate::model::Status::Error, last_activity: at, limit: Some(limit), ..on(key, crate::model::Status::Error, at) }
    }

    /// A session of the account its key names (`claude:claude2:b` is claude2's).
    fn on(key: &str, status: crate::model::Status, last_activity: i64) -> Session {
        let account = key.split(':').nth(1).unwrap_or("claude").to_string();
        Session { status, last_activity, account, ..crate::fixtures::session(key) }
    }

    fn claude_accounts() -> Vec<AccountCfg> {
        ["claude", "claude2", "claude3"].iter().map(|n| account(n, "claude", &format!("~/.{n}"), Some(n))).chain([account("codex", "codex", "~/.codex", None)]).collect()
    }

    fn moves_of(sessions: &mut [Session], usages: &[Usage]) -> Vec<Option<String>> {
        limit_moves(sessions, &claude_accounts(), usages, |h| h == Harness::Claude, NOW);
        sessions.iter().map(|s| s.limit.as_ref().and_then(|l| l.move_to.clone())).collect()
    }

    #[test]
    fn a_limited_session_moves_to_another_account_of_its_harness_with_room() {
        let mut s = [limited("claude:claude:a", NOW - MIN, Some(NOW + HOUR))];
        // claude3's week resets soonest, as Auto would start it; codex isn't Claude's.
        let usages = [usage("claude", 100.0, 60.0, Some(30)), usage("claude2", 20.0, 40.0, Some(100)), usage("claude3", 10.0, 50.0, Some(20))];
        assert_eq!(moves_of(&mut s, &usages), [Some("claude3".to_string())]);
        // One near a limit by its meters has no room: claude3's session is at 95%.
        let usages = [usage("claude2", 20.0, 40.0, Some(100)), usage("claude3", 95.0, 50.0, Some(20))];
        assert_eq!(moves_of(&mut s, &usages), [Some("claude2".to_string())]);
        // None has room: nothing's offered.
        let usages = [usage("claude2", 92.0, 40.0, Some(100)), usage("claude3", 95.0, 50.0, Some(20))];
        assert_eq!(moves_of(&mut s, &usages), [None]);
        // A limit that has lifted goes on where it is.
        let mut lifted = [limited("claude:claude:a", NOW - 3 * HOUR, Some(NOW - HOUR))];
        assert_eq!(moves_of(&mut lifted, &[usage("claude2", 20.0, 40.0, None)]), [None]);
        // A harness that can't move sessions isn't offered one.
        let mut codex = [Session { harness: Harness::Codex, account: "codex".into(), ..limited("codex:codex:t", NOW - MIN, None) }];
        assert_eq!(moves_of(&mut codex, &[]), [None]);
    }

    #[test]
    fn an_account_its_own_sessions_say_is_out_isnt_offered() {
        // A spend limit shows on no meter: claude2's meters have room, but a session of its own
        // was refused after claude's was, and nothing of claude2's has got through since.
        let usages = [usage("claude2", 20.0, 40.0, Some(100)), usage("claude3", 30.0, 50.0, Some(120))];
        let mut s = [limited("claude:claude:a", NOW - 10 * MIN, None), limited("claude:claude2:b", NOW - 5 * MIN, None)];
        assert_eq!(moves_of(&mut s, &usages), [Some("claude3".to_string()), Some("claude3".to_string())]);
        // A turn of claude2's that ended since without one says it has room again; one still
        // working says nothing yet.
        let mut s = [limited("claude:claude:a", NOW - 10 * MIN, None), limited("claude:claude2:b", NOW - 5 * MIN, None), on("claude:claude2:c", crate::model::Status::Working, NOW - MIN)];
        assert_eq!(moves_of(&mut s, &usages)[0].as_deref(), Some("claude3"));
        let mut s = [limited("claude:claude:a", NOW - 10 * MIN, None), limited("claude:claude2:b", NOW - 5 * MIN, None), on("claude:claude2:c", crate::model::Status::Unread, NOW - MIN)];
        assert_eq!(moves_of(&mut s, &usages)[0].as_deref(), Some("claude2"));
        // Word of a refusal lasts until its limit lifts, or a month when it didn't say.
        let lifted = [limited("claude:claude2:b", NOW - 2 * HOUR, Some(NOW - HOUR)), limited("claude:claude2:c", NOW - 32 * 24 * HOUR, None)];
        assert!(refused_accounts(&lifted, NOW).is_empty());
    }

    fn cand(name: &str, preferred: bool) -> Candidate<'_> {
        Candidate { name, preferred, model: None, usage_cap: None }
    }

    fn pick(cs: &[Candidate], us: &[Usage]) -> (String, AutoReason) {
        let p = auto_pick(cs, us, NOW).unwrap();
        (p.account, p.reason)
    }

    #[test]
    fn auto_spends_the_week_that_resets_soonest() {
        let cs = [cand("claude", true), cand("claude2", true)];
        // claude2 has less room, but its week resets first: its room is the first to be lost.
        assert_eq!(pick(&cs, &[usage("claude", 10.0, 20.0, Some(100)), usage("claude2", 30.0, 70.0, Some(90))]), ("claude2".into(), AutoReason::Reset));
        // A week that hasn't started has nothing to lose.
        assert_eq!(pick(&cs, &[usage("claude", 10.0, 20.0, Some(100)), usage("claude2", 0.0, 0.0, None)]).0, "claude");
        // Without a reset under way, the most room.
        assert_eq!(pick(&cs, &[usage("claude", 38.0, 20.0, None), usage("claude2", 12.0, 45.0, None)]), ("claude".into(), AutoReason::Room));
        // Near a limit is no room, while another has some.
        assert_eq!(pick(&cs, &[usage("claude", 10.0, 20.0, Some(100)), usage("claude2", 95.0, 70.0, Some(6))]).0, "claude");
        // All near: the least full.
        assert_eq!(pick(&cs, &[usage("claude", 92.0, 20.0, Some(100)), usage("claude2", 95.0, 70.0, Some(6))]), ("claude".into(), AutoReason::Room));
        // An account's own usage_cap passes it over before 90%, or allows above 90% when raised.
        let capped = [cand("claude", true), Candidate { usage_cap: Some(70), ..cand("claude2", true) }];
        assert_eq!(pick(&capped, &[usage("claude", 10.0, 20.0, Some(100)), usage("claude2", 30.0, 70.0, Some(6))]).0, "claude");
        let raised = [cand("claude", true), Candidate { usage_cap: Some(98), ..cand("claude2", true) }];
        assert_eq!(pick(&raised, &[usage("claude", 10.0, 20.0, Some(100)), usage("claude2", 30.0, 91.0, Some(6))]).0, "claude2");
    }

    #[test]
    fn auto_leaves_the_last_room_before_a_reset_to_the_sessions_running_there() {
        let cs = [cand("claude", true), cand("claude2", true)];
        // claude2's week resets first, but at 85% of a limit, its 5-hour one too, what's left is
        // for the sessions already on it: claude's week is the next to reset, else the most room.
        for (session, week) in [(30.0, 85.0), (87.0, 40.0)] {
            assert_eq!(pick(&cs, &[usage("claude", 10.0, 20.0, Some(100)), usage("claude2", session, week, Some(6))]), ("claude".into(), AutoReason::Reset));
            assert_eq!(pick(&cs, &[usage("claude", 10.0, 20.0, None), usage("claude2", session, week, Some(6))]), ("claude".into(), AutoReason::Room));
        }
        assert_eq!(pick(&cs, &[usage("claude", 10.0, 20.0, Some(100)), usage("claude2", 30.0, 84.0, Some(6))]), ("claude2".into(), AutoReason::Reset));
        // The next reset to come still goes first.
        let three = [cand("claude", true), cand("claude2", true), cand("claude3", true)];
        let us = [usage("claude", 10.0, 20.0, None), usage("claude2", 30.0, 86.0, Some(6)), usage("claude3", 10.0, 50.0, Some(40))];
        assert_eq!(pick(&three, &us), ("claude3".into(), AutoReason::Reset));
        // Still room, so it's taken when no other account has more.
        assert_eq!(pick(&cs, &[usage("claude", 10.0, 89.0, Some(100)), usage("claude2", 30.0, 86.0, Some(6))]), ("claude2".into(), AutoReason::Room));
    }

    #[test]
    fn auto_keeps_to_preferred_accounts_while_they_have_room() {
        let cs = [cand("claude", true), cand("codex", false)];
        let codex_soon = usage("codex", 0.0, 10.0, Some(3));
        assert_eq!(pick(&cs, &[usage("claude", 50.0, 80.0, Some(100)), codex_soon.clone()]).0, "claude");
        // A preferred account without numbers yet still goes first.
        assert_eq!(pick(&cs, &[codex_soon.clone()]), ("claude".into(), AutoReason::First));
        // Out of room: the others.
        assert_eq!(pick(&cs, &[usage("claude", 50.0, 95.0, Some(100)), codex_soon.clone()]), ("codex".into(), AutoReason::Reset));
        // Signed out counts as out.
        let signed_out = Usage { needs_sign_in: true, ..usage("claude", 0.0, 0.0, None) };
        assert_eq!(pick(&cs, &[signed_out, codex_soon]).0, "codex");
        assert_eq!(pick(&[cand("codex", false), cand("claude", true)], &[]).0, "claude");
    }

    #[test]
    fn auto_counts_only_the_models_limits() {
        assert!(spends(Some("opus"), None, &[]));
        assert!(!spends(Some("opus[1m]"), Some("Fable"), &[]));
        assert!(spends(Some("claude-opus-5-5"), Some("Opus"), &[]));
        assert!(spends(Some("fable"), Some("Fable"), &[]));
        assert!(spends(Some("best"), Some("Fable"), &[]));
        assert!(spends(None, Some("Fable"), &[]));
        // Antigravity's limits are for groups of models: a model spends its own group's only.
        let groups = [Meter { model: Some("Gemini".into()), ..meter("model:Gemini", 95.0, None) }, Meter { model: Some("Claude & GPT".into()), ..meter("model:Claude & GPT", 10.0, None) }];
        let spent = |model: Option<&str>| groups.iter().filter(|m| spends(model, m.model.as_deref(), &groups)).map(|m| m.label.as_str()).collect::<Vec<_>>();
        assert_eq!(spent(Some("gemini-3.8-flash-low")), ["model:Gemini"]);
        assert_eq!(spent(Some("claude-sonnet-4-6")), ["model:Claude & GPT"]);
        assert_eq!(spent(Some("gpt-oss-120b")), ["model:Claude & GPT"]);
        assert_eq!(spent(None), ["model:Gemini", "model:Claude & GPT"]);
        // Codex's limit for one model leaves another model of its own alone no more than before.
        let codex = [meter("week", 20.0, None), Meter { model: Some("GPT-5.3-Codex-Spark".into()), ..meter("model:GPT-5.3-Codex-Spark", 50.0, None) }];
        assert!(spends(Some("gpt-5.5-codex"), Some("GPT-5.3-Codex-Spark"), &codex));
        assert!(spends(Some("gpt-5.3-codex-spark"), Some("GPT-5.3-Codex-Spark"), &codex));
        let ag = Usage { meters: groups.to_vec(), ..usage("antigravity", 0.0, 0.0, None) };
        assert!(near_limit(&ag, Some("gemini-3.8-flash-low"), None) && !near_limit(&ag, Some("claude-opus-4-6"), None));
        assert!(near_limit(&ag, Some("claude-opus-4-6"), Some(10)));
        let mut u = usage("claude", 10.0, 20.0, Some(20));
        u.meters.push(Meter { model: Some("Fable".into()), ..meter("model:Fable", 100.0, Some(NOW + 20 * HOUR)) });
        let us = [u, usage("claude2", 20.0, 50.0, None)];
        let on = |m| [Candidate { model: Some(m), ..cand("claude", true) }, Candidate { model: Some(m), ..cand("claude2", true) }];
        assert_eq!(pick(&on("opus"), &us).0, "claude");
        assert_eq!(pick(&on("fable"), &us).0, "claude2");
    }

    #[test]
    fn routing_groups_alternate_harnesses_round_robin_and_respect_usage_caps() {
        use config::{GroupMemberCfg, RoutingGroupCfg, RoutingStrategy};
        let home = Path::new("/Users/me");
        let cfg = Config {
            accounts: vec![
                AccountCfg { usage_cap: Some(80), ..account("claude", "claude", "/Users/me/.claude", Some("claude")) },
                account("claude2", "claude", "/Users/me/.claude2", Some("claude2")),
                account("codex", "codex", "/Users/me/.codex", Some("codex")),
            ],
            ..Config::default()
        };
        let members = vec![
            GroupMemberCfg { account: "claude".into(), model: "opus".into() },
            GroupMemberCfg { account: "claude2".into(), model: "sonnet".into() },
            GroupMemberCfg { account: "codex".into(), model: String::new() },
        ];
        let alt = RoutingGroupCfg { name: "reviewers".into(), strategy: RoutingStrategy::AlternateHarness, members: members.clone() };
        let usages = [usage("claude", 20.0, 30.0, Some(40)), usage("claude2", 10.0, 20.0, Some(80)), usage("codex", 15.0, 25.0, Some(20))];
        // First pick takes the first harness (Claude) and the account in it whose week resets soonest (claude).
        assert_eq!(
            pick_from_group_in(&alt, &cfg, &usages, &[], &[], &[], None, None, NOW, home, |_| true),
            Some(GroupPick { account: "claude".into(), model: Some("opus".into()) })
        );
        // After a Claude review, it switches to Codex.
        let recent = vec![("claude".to_string(), Harness::Claude, Some("opus".to_string()), NOW - 10)];
        assert_eq!(
            pick_from_group_in(&alt, &cfg, &usages, &[], &recent, &[], None, None, NOW, home, |_| true),
            Some(GroupPick { account: "codex".into(), model: None })
        );
        // After Codex ran more recently than Claude, it switches back to Claude, passing over claude when it hits its 80% usage_cap.
        let recent2 = vec![("claude".to_string(), Harness::Claude, Some("opus".to_string()), NOW - 20), ("codex".to_string(), Harness::Codex, None, NOW - 5)];
        let capped = [usage("claude", 20.0, 82.0, Some(40)), usage("claude2", 10.0, 20.0, Some(80)), usage("codex", 15.0, 25.0, Some(20))];
        assert_eq!(
            pick_from_group_in(&alt, &cfg, &capped, &[], &recent2, &[], None, None, NOW, home, |_| true),
            Some(GroupPick { account: "claude2".into(), model: Some("sonnet".into()) })
        );
        // Avoiding Claude (for a PR Claude wrote) picks Codex even when Claude was used less recently.
        assert_eq!(
            pick_from_group_in(&alt, &cfg, &usages, &[], &recent2, &[], None, Some(Harness::Claude), NOW, home, |_| true),
            Some(GroupPick { account: "codex".into(), model: None })
        );
        // Round-robin rotates across every roomy member in order.
        let rr = RoutingGroupCfg { name: "rr".into(), strategy: RoutingStrategy::RoundRobin, members };
        let recent_rr = vec![("claude".to_string(), Harness::Claude, Some("opus".to_string()), NOW - 30), ("claude2".to_string(), Harness::Claude, Some("sonnet".to_string()), NOW - 10)];
        assert_eq!(
            pick_from_group_in(&rr, &cfg, &usages, &[], &recent_rr, &[], None, None, NOW, home, |_| true),
            Some(GroupPick { account: "codex".into(), model: None })
        );
        // Multiple members on the same account keep their own model when the second has more room,
        // and round-robin rotates across models on the same account.
        let same_acct = RoutingGroupCfg {
            name: "models".into(),
            strategy: RoutingStrategy::Usage,
            members: vec![
                GroupMemberCfg { account: "claude2".into(), model: "opus".into() },
                GroupMemberCfg { account: "claude2".into(), model: "sonnet".into() },
            ],
        };
        let mut per_model = usage("claude2", 10.0, 20.0, None);
        per_model.meters.push(Meter { model: Some("Opus".into()), ..meter("model:Opus", 80.0, None) });
        assert_eq!(
            pick_from_group_in(&same_acct, &cfg, &[per_model], &[], &[], &[], None, None, NOW, home, |_| true),
            Some(GroupPick { account: "claude2".into(), model: Some("sonnet".into()) })
        );
        let rr_models = RoutingGroupCfg { strategy: RoutingStrategy::RoundRobin, ..same_acct };
        let after_opus = vec![("claude2".to_string(), Harness::Claude, Some("claude-opus-4-6".to_string()), NOW - 5)];
        assert_eq!(
            pick_from_group_in(&rr_models, &cfg, &usages, &[], &after_opus, &[], None, None, NOW, home, |_| true),
            Some(GroupPick { account: "claude2".into(), model: Some("sonnet".into()) })
        );
        let after_sonnet = vec![
            ("claude2".to_string(), Harness::Claude, Some("claude-opus-4-6".to_string()), NOW - 10),
            ("claude2".to_string(), Harness::Claude, Some("claude-sonnet-4-6".to_string()), NOW - 2),
        ];
        assert_eq!(
            pick_from_group_in(&rr_models, &cfg, &usages, &[], &after_sonnet, &[], None, None, NOW, home, |_| true),
            Some(GroupPick { account: "claude2".into(), model: Some("opus".into()) })
        );
    }

    #[test]
    fn prefers_the_first_accounts_agent() {
        let a = |name: &str, harness: &str| account(name, harness, &format!("~/.{name}"), None);
        let (claude, claude2, codex) = (a("claude", "claude"), a("claude2", "claude"), a("codex", "codex"));
        assert_eq!(preferred_names(&[&codex, &claude, &claude2]), vec!["codex"]);
        assert_eq!(preferred_names(&[&claude, &codex, &claude2]), vec!["claude", "claude2"]);
    }

    #[test]
    fn resumes_only_a_session_that_isnt_running_where_its_account_can() {
        let home = crate::fixtures::scratch("resume-home");
        let running = crate::fixtures::session("live");
        let stopped = crate::model::Session { live: false, ..crate::fixtures::session("stopped") };
        let board = Board { lanes: vec![crate::fixtures::lane("/r", vec![running, stopped])], ..Board::default() };
        let cfg = Config { accounts: vec![account("claude", "claude", &home.to_string_lossy(), None)], ..Config::default() };
        let to = |place| ResumeIn { place, model: None, effort: None };
        let you = crate::send::Sender::You;
        let refused = |key: &str, how: ResumeIn| resume(&board, &cfg, key, &how, "", &you).unwrap_err().to_string();
        assert_eq!(refused("live", to(Place::Terminal)), "“Title live” is running already; send it a message instead.");
        // An agent's words don't start it again, nor does an agent with none: that's its user's to do.
        for prompt in ["Go on", ""] {
            let e = resume(&board, &cfg, "stopped", &to(Place::Terminal), prompt, &crate::send::Sender::Agent(Some("claude:claude:me".into()))).unwrap_err().to_string();
            assert_eq!(e, "“Title stopped” isn't running, and a message from another session doesn't start it again, so nothing was sent. Its user can resume it, or `codealong start` starts a new session.");
        }
        // The Claude app runs the account in ~/.claude only.
        assert_eq!(refused("stopped", to(Place::ClaudeApp)), "“Title stopped” can't go on in the Claude app from here.");
        // What's asked of the command line is checked as a start's is.
        assert!(refused("stopped", ResumeIn { model: Some("opus; rm -rf ~".into()), ..to(Place::Terminal) }).contains("isn't a model name"));
        assert!(refused("stopped", ResumeIn { effort: Some("ludicrous".into()), ..to(Place::Background) }).contains("isn't an effort level"));
    }

    /// Nothing starts on an account turned off: Auto passes it by, the start options leave it
    /// out, a session stopped at a limit doesn't move to it, and its own sessions don't go on.
    #[test]
    fn starts_nothing_on_an_account_turned_off() {
        let dir = |name: &str| crate::fixtures::scratch(name).to_string_lossy().into_owned();
        let off = AccountCfg { disabled: true, ..account("claude", "claude", &dir("off-claude"), None) };
        let cfg = Config { accounts: vec![off, account("claude2", "claude", &dir("off-claude2"), None)], ..Config::default() };
        assert_eq!(pick_account(&cfg, &[], &[], None, NOW).as_deref(), Some("claude2"));
        assert_eq!(options_for(&cfg).accounts.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["claude2"]);

        let mut accounts = claude_accounts();
        accounts[2].disabled = true;
        let mut s = [limited("claude:claude:a", NOW - MIN, Some(NOW + HOUR))];
        let usages = [usage("claude", 100.0, 60.0, Some(30)), usage("claude2", 20.0, 40.0, Some(100)), usage("claude3", 10.0, 50.0, Some(20))];
        limit_moves(&mut s, &accounts, &usages, |h| h == Harness::Claude, NOW);
        assert_eq!(s[0].limit.as_ref().and_then(|l| l.move_to.as_deref()), Some("claude2"));

        let stopped = crate::model::Session { live: false, ..crate::fixtures::session("stopped") };
        let board = Board { lanes: vec![crate::fixtures::lane("/r", vec![stopped])], ..Board::default() };
        let how = ResumeIn { place: Place::Terminal, model: None, effort: None };
        let e = resume(&board, &cfg, "stopped", &how, "", &crate::send::Sender::You).unwrap_err().to_string();
        assert_eq!(e, "claude is turned off in Settings › Accounts.");
    }

    #[test]
    fn a_session_cut_off_is_told_to_carry_on_unless_youve_more_to_say() {
        let stopped = crate::model::Session { live: false, ..crate::fixtures::session("stopped") };
        let cut = crate::model::Session { cut_off: Some(1), ..stopped.clone() };
        assert_eq!(resume_prompt("", &cut), (CARRY_ON, true));
        assert_eq!(resume_prompt("  ", &cut), (CARRY_ON, true));
        assert_eq!(resume_prompt("Try the other fix", &cut), ("Try the other fix", false));
        assert_eq!(resume_prompt("", &stopped), ("", false));
    }

    #[test]
    fn a_model_goes_only_where_the_command_line_or_codealong_takes_it() {
        let claude = crate::harness::adapter(Harness::Claude);
        assert_eq!(tuning(claude, Place::Terminal, Some(" opus "), Some("high")).unwrap(), (Some("opus"), Some("high")));
        assert_eq!(tuning(claude, Place::Codealong, Some("sonnet"), None).unwrap(), (Some("sonnet"), None));
        assert_eq!(tuning(claude, Place::ClaudeApp, Some("opus"), Some("high")).unwrap(), (None, None));
        assert_eq!(tuning(crate::harness::adapter(Harness::Codex), Place::Terminal, Some("gpt-5"), Some("high")).unwrap(), (None, None));
    }

    fn places(o: &StartOptions, name: &str) -> Vec<Place> {
        o.accounts.iter().find(|a| a.name == name).unwrap().places.iter().map(|p| p.place).collect()
    }

    #[test]
    fn offers_each_app_only_its_own_account() {
        let accounts = vec![
            account("claude", "claude", "/Users/me/.claude", Some("claude")),
            account("claude2", "claude", "/Users/me/.claude2", Some("claude2")),
            account("codex", "codex", "/Users/me/.codex", None),
            account("codex2", "codex", "/Users/me/.codex2", None),
            account("opencode", "opencode", "/Users/me/.local/share/opencode", Some("opencode")),
            account("antigravity", "antigravity", "/Users/me/.gemini/antigravity", None),
        ];
        let o = options(&accounts, "Ghostty", vec!["/r/spur".into()], Path::new("/Users/me"), |a| (a.name == "claude2").then(|| "claude-fable-5-1".to_string()));
        assert_eq!(places(&o, "claude"), vec![Place::Terminal, Place::Background, Place::ClaudeApp, Place::Codealong]);
        assert_eq!(places(&o, "claude2"), vec![Place::Terminal, Place::Background, Place::Codealong]);
        assert_eq!(places(&o, "codex"), vec![Place::CodexApp, Place::Terminal, Place::Background, Place::Codealong]);
        assert_eq!(places(&o, "codex2"), vec![Place::Terminal, Place::Background, Place::Codealong]);
        assert_eq!(places(&o, "opencode"), vec![Place::Terminal, Place::Background, Place::Codealong]);
        assert!(places(&o, "antigravity").is_empty());
        // Only Claude Code has a background service of its own; the others' Background is their
        // terminal, behind your windows.
        let behind = |name: &str| o.accounts.iter().find(|a| a.name == name).unwrap().places.iter().find(|p| p.place == Place::Background).map(|p| p.behind);
        assert_eq!(behind("claude2"), Some(false));
        assert_eq!(behind("codex2"), Some(true));
        assert_eq!(behind("opencode"), Some(true));
        assert!(o.accounts[5].unavailable.as_deref().unwrap_or("").contains("its own window"));
        assert_eq!(o.accounts[4].default_model, None);
        assert_eq!(o.accounts[0].places[0].label, "Ghostty");
        assert_eq!(o.codex_projects, vec!["/r/spur".to_string()]);
        // A session that isn't running goes on in the places its account starts sessions in
        // where its harness takes one up again: OpenCode's run in its service, never stopped.
        let resumes = |name: &str| o.accounts.iter().find(|a| a.name == name).unwrap().resumes.clone();
        assert_eq!(resumes("claude"), places(&o, "claude"));
        assert_eq!(resumes("claude2"), vec![Place::Terminal, Place::Background, Place::Codealong]);
        assert_eq!(resumes("codex"), places(&o, "codex"));
        assert!(resumes("opencode").is_empty() && resumes("antigravity").is_empty());
        assert_eq!(resume_places(&accounts[1], Path::new("/Users/me")), resumes("claude2"));
        // Claude Code's running sessions switch models by a command typed into them; Codex's take none.
        let switches = |name: &str| o.accounts.iter().find(|a| a.name == name).unwrap().switches_model;
        assert!(switches("claude") && switches("claude2") && !switches("codex") && !switches("opencode"));
        let switches_effort = |name: &str| o.accounts.iter().find(|a| a.name == name).unwrap().switches_effort;
        assert!(switches_effort("claude") && !switches_effort("codex") && !switches_effort("opencode"));
        assert_eq!(o.accounts[1].default_model.as_deref(), Some("claude-fable-5-1"));
        assert_eq!(o.accounts[0].default_model, None);
        assert!(o.claude_models.iter().any(|m| m.id == "opus[1m]"));
    }

    #[test]
    fn an_accounts_start_setting_goes_first() {
        let home = Path::new("/Users/me");
        let with = |name: &str, h: &str, dir: &str, start: &str| AccountCfg { start: Some(start.into()), ..account(name, h, dir, None) };
        let accounts = vec![with("claude", "claude", "/Users/me/.claude", "claude-app"), with("claude2", "claude", "/Users/me/.claude2", "background"), account("claude3", "claude", "/Users/me/.claude3", None)];
        // The command and a resume go by the order; the quick panel lists the places as they
        // come and goes by `chosen`, so they hold still as a start there changes `start`.
        assert_eq!(ordered_places(&accounts[0], home).unwrap(), vec![Place::ClaudeApp, Place::Terminal, Place::Background, Place::Codealong]);
        assert_eq!(ordered_places(&accounts[1], home).unwrap(), vec![Place::Background, Place::Terminal, Place::Codealong]);
        let o = options(&accounts, "Ghostty", vec![], home, |_| None);
        assert_eq!(places(&o, "claude"), vec![Place::Terminal, Place::Background, Place::ClaudeApp, Place::Codealong]);
        assert_eq!(o.accounts[0].chosen, Some(Place::ClaudeApp));
        assert_eq!(places(&o, "claude2"), vec![Place::Terminal, Place::Background, Place::Codealong]);
        assert_eq!(o.accounts[1].chosen, Some(Place::Background));
        assert_eq!(places(&o, "claude3"), vec![Place::Terminal, Place::Background, Place::Codealong]);
        assert_eq!(o.accounts[2].chosen, None);

        // A start from the quick panel keeps its place for the accounts named that can start
        // there and don't already.
        let names = |ns: &[&str]| ns.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(place_changes(&accounts, &names(&["claude", "claude2", "claude3"]), Place::Background, home), names(&["claude", "claude3"]));
        assert_eq!(place_changes(&accounts, &names(&["claude2"]), Place::ClaudeApp, home), Vec::<String>::new());
        assert_eq!(place_changes(&accounts, &names(&["claude3"]), Place::Terminal, home), names(&["claude3"]));

        assert!(check_start(&accounts[0], "claude-app", home).is_ok());
        let err = check_start(&accounts[1], "claude-app", home).unwrap_err();
        assert!(err.contains("\"terminal\", \"background\", \"codealong\""), "{err}");
        assert!(check_start(&account("codex", "codex", "/Users/me/.codex", None), "terminal", home).is_ok());
        assert!(check_start(&account("codex2", "codex", "/Users/me/.codex2", None), "background", home).is_ok());
        assert!(check_start(&account("codex2", "codex", "/Users/me/.codex2", None), "codex-app", home).is_err());
        assert!(check_start(&account("ag", "antigravity", "/Users/me/.gemini/antigravity", None), "terminal", home).unwrap_err().contains("takes no start"));
        assert!(check_start(&accounts[1], "somewhere", home).is_err());
    }

    #[test]
    fn background_is_the_terminal_behind_your_windows_without_a_service_of_its_own() {
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let cx = account("codex2", "codex", "~/.codex2", Some("codex2"));
        let Plan::Terminal { cwd, input, env, behind } = plan(&cx, Place::Background, Path::new("/r/x"), "Fix it", id, None, None, false) else { panic!() };
        assert!(behind);
        assert_eq!(plan(&cx, Place::Terminal, Path::new("/r/x"), "Fix it", id, None, None, false), Plan::Terminal { cwd, input, env, behind: false });
        let claude = account("claude2", "claude", "~/.claude2", Some("claude2"));
        assert!(matches!(plan(&claude, Place::Background, Path::new("/r/x"), "Fix it", id, None, None, false), Plan::Background { .. }));
    }

    #[test]
    fn reads_the_todos_a_start_is_for_old_and_new() {
        let read = |json: &str| serde_json::from_str::<StartSpec>(json).unwrap().todos;
        let base = r#""laneId":"/r/x","account":"claude","place":"terminal""#;
        assert_eq!(read(&format!(r#"{{{base},"todos":["t-1","t-2"]}}"#)), vec!["t-1", "t-2"]);
        // As windows and the command sent it before a start could be for several.
        assert_eq!(read(&format!(r#"{{{base},"todo":"t-1"}}"#)), vec!["t-1"]);
        assert!(read(&format!(r#"{{{base},"todo":null}}"#)).is_empty());
        assert!(read(&format!("{{{base}}}")).is_empty());
    }

    #[test]
    fn asks_claude_code_for_a_model_in_quotes() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let Plan::Terminal { input, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "Fix it", id, Some("fable[1m]"), None, false) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id} --model 'fable[1m]' \"$CODEALONG_PROMPT\"\n"));
        // Nothing that could leave the quotes is ever typed.
        let Plan::Terminal { input, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "", id, Some("opus'; rm -rf ~ '"), None, false) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id}\n"));
        assert!(valid_model("claude-opus-5-5") && valid_model("sonnet[1m]") && valid_model("opusplan"));
        assert!(!valid_model("") && !valid_model("opus fable") && !valid_model("$(x)") && !valid_model("a'b"));
        // The apps take no model.
        let claude = account("claude", "claude", "~/.claude", Some("claude"));
        assert_eq!(plan(&claude, Place::ClaudeApp, Path::new("/r/x"), "", id, Some("opus"), None, false), Plan::ClaudeApp { url: "claude://code/new?folder=%2Fr%2Fx".into() });
    }

    #[test]
    fn asks_claude_code_for_an_effort_level_it_knows() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let Plan::Terminal { input, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "Fix it", id, Some("opus"), Some("xhigh"), false) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id} --model 'opus' --effort xhigh \"$CODEALONG_PROMPT\"\n"));
        let Plan::Background { command, .. } = plan(&a, Place::Background, Path::new("/r/x"), "", id, None, Some("low"), false) else { panic!() };
        assert_eq!(command, "claude2 --bg --effort low");
        let Plan::Hosted { effort, .. } = plan(&a, Place::Codealong, Path::new("/r/x"), "Fix it", id, None, Some("max"), false) else { panic!() };
        assert_eq!(effort.as_deref(), Some("max"));
        // Only the levels Claude Code takes are typed.
        let Plan::Terminal { input, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "", id, None, Some("high; rm -rf ~"), false) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id}\n"));
        assert!(effort_levels().contains(&"medium") && !effort_levels().contains(&"ultracode") && !effort_levels().contains(&""));
        // The other agents choose their own.
        let o = account("opencode", "opencode", "~/.local/share/opencode", Some("opencode"));
        let Plan::Terminal { input, .. } = plan(&o, Place::Terminal, Path::new("/r/x"), "", id, None, Some("high"), false) else { panic!() };
        assert_eq!(input, "opencode\n");
        let cx = account("codex", "codex", "~/.codex", Some("codex"));
        let Plan::Terminal { input, .. } = plan(&cx, Place::Terminal, Path::new("/r/x"), "", id, None, Some("high"), false) else { panic!() };
        assert_eq!(input, "codex -c check_for_update_on_startup=false\n");
        let o = options(&[a], "Ghostty", Vec::new(), Path::new("/Users/me"), |_| None);
        assert_eq!(o.claude_efforts.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["low", "medium", "high", "xhigh", "max"]);
    }

    #[test]
    fn hands_the_terminal_the_prompt_in_the_environment() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let prompt = "Fix the \"flaky\" test\nthen run `npm test` and $(say done)";
        let Plan::Terminal { cwd, input, env, behind } = plan(&a, Place::Terminal, Path::new("/r/spark-app4"), &format!("  {prompt}\n"), id, None, None, false) else { panic!() };
        assert_eq!(cwd, PathBuf::from("/r/spark-app4"));
        assert!(!behind);
        assert_eq!(input, format!("claude2 --session-id {id} \"$CODEALONG_PROMPT\"\n"));
        let session = (SESSION_VAR.to_string(), format!("claude:claude2:{id}"));
        assert_eq!(env, vec![session.clone(), (PROMPT_VAR.to_string(), prompt.to_string())]);
        let Plan::Terminal { input, env, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "  ", id, None, None, false) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id}\n"));
        assert_eq!(env, vec![session]);
        let custom = account("claude3", "claude", "~/.claude3", Some("CLAUDE_CONFIG_DIR=~/.claude3 claude"));
        let Plan::Terminal { input, .. } = plan(&custom, Place::Terminal, Path::new("/r/x"), "", id, None, None, false) else { panic!() };
        assert_eq!(input, format!("CLAUDE_CONFIG_DIR=~/.claude3 claude --session-id {id}\n"));
    }

    #[test]
    fn starts_in_the_background_with_everything_quoted_in_the_command() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let (images, files) = (Path::new("/Users/me/Library/Application Support/Codealong/images"), Path::new("/f"));
        let prompt = "Don't break $HOME or `this`\n![image](images/0123456789abcdef.png)";
        let Plan::Background { cwd, command } = plan_in(&a, Place::Background, Path::new("/r/x"), prompt, id, Some("fable[1m]"), None, true, (images, files)) else { panic!() };
        assert_eq!(cwd, PathBuf::from("/r/x"));
        // The prompt after `--`, which neither --worktree (it takes an optional name) nor
        // --add-dir (it takes several folders) reads as theirs once --bg takes itself out, and no
        // --session-id, which --bg ignores.
        assert_eq!(
            command,
            format!("claude2 --worktree --add-dir '{}' --bg --model 'fable[1m]' -- 'Don'\\''t break $HOME or `this`\n@\"{}/0123456789abcdef.png\"'", images.display(), images.display())
        );
        // As the jobs that went wrong had it: nothing between --bg and the prompt.
        let Plan::Background { command, .. } = plan_in(&a, Place::Background, Path::new("/r/x"), "Fix it\n![image](images/0123456789abcdef.png)", id, None, None, true, (images, files)) else { panic!() };
        assert_eq!(command, format!("claude2 --worktree --add-dir '{}' --bg -- 'Fix it\n@\"{}/0123456789abcdef.png\"'", images.display(), images.display()));
        let Plan::Background { command, .. } = plan(&a, Place::Background, Path::new("/r/x"), "  ", id, None, None, false) else { panic!() };
        assert_eq!(command, "claude2 --bg");
        // A one-word prompt keeps its padding, so it isn't read as a command.
        let Plan::Background { command, .. } = plan(&a, Place::Background, Path::new("/r/x"), "update", id, None, None, false) else { panic!() };
        assert_eq!(command, "claude2 --bg -- 'update '");
    }

    #[test]
    fn starts_opencode_on_its_own_terms() {
        let a = account("opencode", "opencode", "~/.local/share/opencode", Some("opencode"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let Plan::Terminal { input, env, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "Fix it", id, Some("opus"), None, true) else { panic!() };
        assert_eq!(input, "opencode --prompt \"$CODEALONG_PROMPT\"\n");
        assert_eq!(env, vec![(PROMPT_VAR.to_string(), "Fix it".to_string())]);
        let Plan::Terminal { input, env, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "", id, None, None, false) else { panic!() };
        assert_eq!(input, "opencode\n");
        assert!(env.is_empty());
    }

    #[test]
    fn hands_images_over_as_paths_claude_code_may_read() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let dir = Path::new("/Users/me/Library/Application Support/Codealong/images");
        let prompt = "Match this:\n![image](images/0123456789abcdef.png)";
        let Plan::Terminal { input, env, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), prompt, id, Some("opus"), None, false, (dir, Path::new("/f"))) else { panic!() };
        assert_eq!(input, format!("claude2 --add-dir \"$CODEALONG_IMAGES\" --session-id {id} --model 'opus' \"$CODEALONG_PROMPT\"\n"));
        assert_eq!(
            env,
            vec![
                (IMAGES_VAR.to_string(), dir.to_string_lossy().to_string()),
                (SESSION_VAR.to_string(), format!("claude:claude2:{id}")),
                (PROMPT_VAR.to_string(), format!("Match this:\n@\"{}/0123456789abcdef.png\"", dir.display())),
            ]
        );
        // Without images, nothing changes.
        let Plan::Terminal { input, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), "Fix it", id, None, None, false, (dir, Path::new("/f"))) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id} \"$CODEALONG_PROMPT\"\n"));
        // The Codex CLI gets the image with -i and taken out of the prompt, `--` before the prompt.
        let cx2 = account("codex2", "codex", "~/.codex2", Some("CODEX_HOME=~/.codex2 codex"));
        let Plan::Terminal { input, env, .. } = plan_in(&cx2, Place::Terminal, Path::new("/r/x"), prompt, id, Some("opus"), None, true, (dir, Path::new("/f"))) else { panic!() };
        assert_eq!(input, format!("CODEX_HOME=~/.codex2 codex -c check_for_update_on_startup=false -i '{}/0123456789abcdef.png' -- \"$CODEALONG_PROMPT\"\n", dir.display()));
        assert_eq!(env, vec![(PROMPT_VAR.to_string(), "Match this:".to_string())]);
        let Plan::Terminal { input, env, .. } = plan_in(&cx2, Place::Terminal, Path::new("/r/x"), "", id, None, None, false, (dir, Path::new("/f"))) else { panic!() };
        assert_eq!((input.as_str(), env.len()), ("CODEX_HOME=~/.codex2 codex -c check_for_update_on_startup=false\n", 0));
        // Codex opens the path; the Claude app gets the same mention the terminal does.
        let codex = account("codex", "codex", "~/.codex", None);
        let Plan::CodexApp { url } = plan_in(&codex, Place::CodexApp, Path::new("/r/x"), prompt, id, None, None, false, (dir, Path::new("/f"))) else { panic!() };
        assert!(url.ends_with(&urlencoding::encode(&format!("Match this:\n![image]({}/0123456789abcdef.png)", dir.display())).to_string()));
        let claude = account("claude", "claude", "~/.claude", Some("claude"));
        let Plan::ClaudeApp { url } = plan_in(&claude, Place::ClaudeApp, Path::new("/r/x"), prompt, id, None, None, false, (dir, Path::new("/f"))) else { panic!() };
        assert!(url.contains(&urlencoding::encode("@\"/Users/me/Library").to_string()));
        // In a new worktree, the images folder comes after --worktree, which takes an optional name.
        let Plan::Terminal { input, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), prompt, id, None, None, true, (dir, Path::new("/f"))) else { panic!() };
        assert_eq!(input, format!("claude2 --worktree --add-dir \"$CODEALONG_IMAGES\" --session-id {id} \"$CODEALONG_PROMPT\"\n"));
    }

    #[test]
    fn hands_other_files_over_from_their_own_folder() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let (images, files) = (Path::new("/i"), Path::new("/Users/me/Library/Application Support/Codealong/files"));
        let prompt = "Why does this fail?\n[crash.log](files/0123456789abcdef/crash.log)";
        let Plan::Terminal { input, env, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), prompt, id, None, None, false, (images, files)) else { panic!() };
        assert_eq!(input, format!("claude2 --add-dir \"$CODEALONG_FILES\" --session-id {id} \"$CODEALONG_PROMPT\"\n"));
        assert_eq!(env[0], (FILES_VAR.to_string(), files.to_string_lossy().to_string()));
        assert_eq!(env[2].1, format!("Why does this fail?\n@\"{}/0123456789abcdef/crash.log\"", files.display()));
        // With an image too, both folders, each its own --add-dir.
        let both = format!("{prompt}\n![image](images/fedcba9876543210.png)");
        let Plan::Terminal { input, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), &both, id, None, None, false, (images, files)) else { panic!() };
        assert_eq!(input, format!("claude2 --add-dir \"$CODEALONG_IMAGES\" --add-dir \"$CODEALONG_FILES\" --session-id {id} \"$CODEALONG_PROMPT\"\n"));
        // OpenCode gets the path, and no folders it has no option for.
        let o = account("opencode", "opencode", "~/.local/share/opencode", Some("opencode"));
        let Plan::Terminal { input, env, .. } = plan_in(&o, Place::Terminal, Path::new("/r/x"), prompt, id, None, None, false, (images, files)) else { panic!() };
        assert_eq!(input, "opencode --prompt \"$CODEALONG_PROMPT\"\n");
        assert_eq!(env[0].1, format!("Why does this fail?\n[crash.log]({}/0123456789abcdef/crash.log)", files.display()));
    }

    #[test]
    fn asks_claude_code_for_a_new_worktree_before_anything_it_could_take_as_its_name() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let Plan::Terminal { cwd, input, .. } = plan(&a, Place::Terminal, Path::new("/r/spark"), "Fix it", id, Some("opus"), None, true) else { panic!() };
        assert_eq!(cwd, PathBuf::from("/r/spark"));
        assert_eq!(input, format!("claude2 --worktree --session-id {id} --model 'opus' \"$CODEALONG_PROMPT\"\n"));
        // The apps choose their own worktrees.
        let claude = account("claude", "claude", "~/.claude", Some("claude"));
        assert_eq!(plan(&claude, Place::ClaudeApp, Path::new("/r/x"), "", id, None, None, true), Plan::ClaudeApp { url: "claude://code/new?folder=%2Fr%2Fx".into() });
    }

    #[test]
    fn keeps_a_prompt_from_reading_as_an_option_or_a_command() {
        assert_eq!(as_argument("Fix the build"), "Fix the build");
        assert_eq!(as_argument("update"), "update ");
        assert_eq!(as_argument("-v"), " -v ");
        assert_eq!(as_argument("--help with the README"), " --help with the README");
    }

    #[test]
    fn builds_the_app_links() {
        let claude = account("claude", "claude", "~/.claude", Some("claude"));
        let codex = account("codex", "codex", "~/.codex", None);
        let folder = Path::new("/Users/me/My Repo");
        assert_eq!(
            plan(&claude, Place::ClaudeApp, folder, "Fix it & ship\nnow", "id", None, None, false),
            Plan::ClaudeApp { url: "claude://code/new?folder=%2FUsers%2Fme%2FMy%20Repo&q=Fix%20it%20%26%20ship%0Anow".into() }
        );
        assert_eq!(plan(&claude, Place::ClaudeApp, folder, "", "id", None, None, false), Plan::ClaudeApp { url: "claude://code/new?folder=%2FUsers%2Fme%2FMy%20Repo".into() });
        assert_eq!(
            plan(&codex, Place::CodexApp, folder, "Review #12", "id", None, None, false),
            Plan::CodexApp { url: "codex://threads/new?path=%2FUsers%2Fme%2FMy%20Repo&prompt=Review%20%2312".into() }
        );
    }

    #[test]
    fn makes_version_4_session_ids() {
        assert_eq!(uuid_v4([0xff; 16]), "ffffffff-ffff-4fff-bfff-ffffffffffff");
        assert_eq!(uuid_v4([0; 16]), "00000000-0000-4000-8000-000000000000");
        let a = new_session_id();
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
        assert_ne!(a, new_session_id());
    }

    #[test]
    fn a_session_moves_only_to_another_account_of_its_harness() {
        let cfg = Config { accounts: vec![account("claude", "claude", "~/.claude", None), account("claude2", "claude", "~/.claude2", None), account("codex", "codex", "~/.codex", None)], ..Config::default() };
        let s = crate::model::Session { title: "Photo tools".into(), ..crate::fixtures::session("claude:claude:abc") };
        let from = &cfg.accounts[0];
        assert_eq!(move_target(&cfg, &s, from, "claude2").unwrap().name, "claude2");
        let err = |to: &str| move_target(&cfg, &s, from, to).unwrap_err().to_string();
        assert_eq!(err("claude"), "“Photo tools” is on claude already.");
        assert_eq!(err("codex"), "codex isn't a Claude Code account; a session moves only between accounts of its own harness.");
        assert_eq!(err("claude3"), "There's no account named claude3 in config.toml.");
    }
}

//! Cross-provider session handoff: carrying any session's conversation, workspace state and next
//! step onto any other account or harness (Claude Code, Codex, OpenCode, Antigravity, Cursor).
//!
//! Same-harness native moves (`Adapter::moves_accounts`, e.g. Claude Code → Claude Code) still
//! use the lossless native `--resume <path> --fork-session` path when moving with default launch
//! settings (`move_or_handoff`). Every other transition — cross-provider, same-harness on a
//! harness without native moves, kept copies (`keep: true`), or moves with an explicit target
//! model, effort, place or directive — runs through the generic handoff engine here:
//!
//! 1. Read the canonical conversation via `transcript::page` (every harness implements it),
//!    pairing tool results onto their calls, dropping withdrawn prompts, and unwrapping any prior
//!    handoff header so chained handoffs (`Claude → Codex → Antigravity`) stay flat.
//! 2. Write the complete Markdown transcript to `<files_dir>/handoffs/<date>-<hash>-<slug>.md`
//!    (never touching the repo's working tree) so the target agent can read earlier turns on
//!    demand, and prune handoff files older than 30 days.
//! 3. Build a tiered, byte-budgeted inline handoff prompt:
//!    - Fixed header (source identity, workspace/branch/repo, PR, linked todos, prior handoff
//!      chain, reason, full transcript path, workspace rules).
//!    - Session footprint (deduplicated files touched + notable failed tool calls/errors).
//!    - Anchor (original user goal + latest compaction note if the session was compacted).
//!    - Compacted middle timeline (one-liners for older turns between anchor and tail).
//!    - High-fidelity recent turns (bulk of the budget, verbatim).
//!    - Immediate directive (user's next-step instruction, limit/cut-off carry-on, or safe
//!      inspect-and-confirm default) — never truncated.
//! 4. Start the target session in the same worktree via `start::launch` (reusing every harness's
//!    native launch, model/effort tuning, `--add-dir` for `files_dir`, and locked-screen rules).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;

use crate::attachments;
use crate::config::{AccountCfg, Config};
use crate::harness::{adapter, find, of, turn_running};
use crate::model::{Board, Lane, Session};
use crate::start::{self, Place, StartSpec, Started};
use crate::transcript::{self, Item};
use crate::util::{now_ms, tilde, DAY_MS};

/// Prefix of every handoff prompt's opening line, used both to match async target sessions
/// (`origins::Pending::opens`) and to unwrap chained handoffs (`A → B → C`).
pub const HEADER_PREFIX: &str = "# Handoff from \"";
const NEXT_STEP_HEADING: &str = "## Next step";

/// Maximum conversation items and pages read from the source session so giant transcripts don't
/// hang or exhaust memory.
const MAX_ITEMS: usize = 4000;
const PAGE_LIMIT: usize = 200;
const MAX_PAGES: usize = 40;

/// Inline prompt byte budgets: generous for CLI / Background / ACP / CodexApp (which creates its
/// thread over app-server), compact for URL-launched apps (`claude://code/new?q=...`).
pub const CLI_BUDGET: usize = 24 * 1024;
pub const URL_BUDGET: usize = 3_500;

/// Maximum age of saved full-transcript handoff files before lazy cleanup removes them.
const KEEP_FILES_MS: i64 = 30 * DAY_MS;

/// Maximum files and failures listed in the compact footprint index.
const MAX_FILES: usize = 25;
const MAX_FAILURES: usize = 8;
/// Maximum chars per compacted middle-turn line (T3 Code `compactText` style).
const COMPACT_MAX_CHARS: usize = 220;
/// Maximum chars for a single recent turn before head+tail clipping protects sibling recent turns.
const SINGLE_TURN_MAX_CHARS: usize = 6_000;
/// Maximum chars of error output kept inline for a failed tool call in recent turns.
const FAILED_OUTPUT_MAX_CHARS: usize = 600;

/// Parameters for a move or handoff request.
#[derive(Debug, Clone, Default)]
pub struct Ask {
    pub account: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub place: Option<Place>,
    /// Optional user instruction for what the target session should do next.
    pub prompt: String,
    /// When true, keep the original session open and unarchived (non-destructive handoff copy).
    pub keep: bool,
}

/// Why a session is being handed off, inferred from its board state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    Manual,
    Limit(String),
    CutOff,
    ApiError(String),
}

impl Reason {
    pub fn of(s: &Session) -> Self {
        if let Some(limit) = &s.limit {
            let meter = limit.meter.as_deref().unwrap_or("usage");
            return Reason::Limit(format!("Stopped at {}'s {meter} limit", s.account));
        }
        if s.cut_off.is_some() {
            return Reason::CutOff;
        }
        if let Some(err) = &s.api_error {
            let said = err.said.trim();
            return Reason::ApiError(if said.is_empty() { "Last turn ended on an API error".into() } else { said.to_string() });
        }
        if s.connection_lost.is_some() {
            return Reason::ApiError("Last turn ended because the API couldn't be reached".into());
        }
        Reason::Manual
    }

    fn line(&self) -> String {
        match self {
            Reason::Manual => "Handed off by the user to continue here.".into(),
            Reason::Limit(why) => format!("{why}."),
            Reason::CutOff => "The previous session's last turn was cut off partway.".into(),
            Reason::ApiError(why) => format!("Previous turn ended on an API error: {why}"),
        }
    }

    fn default_directive(&self) -> &'static str {
        match self {
            Reason::Limit(_) => "The previous session stopped at its usage limit and was handed off here to carry on. Inspect the current workspace state and continue where it left off.",
            Reason::CutOff => "The previous session's last turn was cut off partway (a command or edit may have been interrupted). Check what its last step did on disk, then carry on where it left off.",
            Reason::ApiError(_) => "The previous session's last turn ended on an API error. Inspect where things stand in the workspace and carry on where it left off.",
            Reason::Manual => "Review the handoff context and current workspace state, briefly confirm where things stand, and wait for the user's next instruction unless the context above leaves an obvious unfinished request from them.",
        }
    }
}

/// PR metadata carried in the handoff header when the source worktree has an open/linked PR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrSummary {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: String,
}

/// One prior handoff unwrapped from the start of a chained session (`Claude → Codex → Antigravity`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriorHandoff {
    pub title: String,
    pub source_harness: String,
    pub source_account: String,
    pub source_key: Option<String>,
    pub transcript_file: Option<String>,
}

/// All inputs to the pure packet builder (`build_packet`), kept free of disk or board lookups so
/// every tier, budget, chaining and edge case is unit-tested directly.
pub struct PacketInput<'a> {
    pub source_key: &'a str,
    pub source_title: &'a str,
    pub source_harness: &'a str,
    pub source_account: &'a str,
    pub cwd: &'a str,
    pub branch: Option<&'a str>,
    pub repo: Option<&'a str>,
    pub pr: Option<PrSummary>,
    pub todos: Vec<String>,
    pub reason: Reason,
    pub directive: &'a str,
    pub transcript_file: Option<&'a Path>,
    pub budget: usize,
    pub read_capped: bool,
}

/// The built handoff packet: inline prompt + complete Markdown transcript + stats.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Packet {
    pub prompt: String,
    pub transcript_markdown: String,
    pub total_items: usize,
    pub omitted_items: usize,
    pub files: Vec<String>,
    pub failures: Vec<String>,
    pub prior_chain: Vec<PriorHandoff>,
    pub read_capped: bool,
}

/// Dry-run preview of a move or handoff (`--snapshot --handoff-plan`, `codealong session handoff --dry-run`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanPreview {
    pub source_key: String,
    pub source_title: String,
    pub source_harness: String,
    pub source_account: String,
    pub target_account: String,
    pub target_harness: String,
    /// `"native"` for lossless same-harness moves, `"handoff"` for generic packet handoffs.
    pub mode: &'static str,
    pub keep_source: bool,
    pub place: Place,
    pub budget_bytes: usize,
    pub prompt_bytes: usize,
    pub transcript_bytes: usize,
    pub total_items: usize,
    pub omitted_items: usize,
    pub files_count: usize,
    pub failures_count: usize,
    pub prior_handoffs: usize,
    pub prompt: String,
    pub launch: serde_json::Value,
}

/// Result of launching a move or handoff, for `actions.rs` to wire origins and ownership handover.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub started: Started,
    pub in_terminal: bool,
    pub opening: Option<String>,
    pub lane_id: String,
    pub target_account: String,
    pub native: bool,
}

/// Directory for full Markdown handoff transcripts, inside Codealong's `files` folder so harnesses
/// that take `--add-dir` (`attachments::uses_files_dir`) can read them on demand without asking.
pub fn handoffs_dir() -> PathBuf {
    attachments::files_dir().join("handoffs")
}

/// Inline byte budget for `place`: compact for URL-scheme app links, generous for CLI/ACP/CodexApp.
pub fn budget_for(place: Place) -> usize {
    match place {
        Place::ClaudeApp => URL_BUDGET,
        Place::Terminal | Place::Background | Place::Codealong | Place::CodexApp => CLI_BUDGET,
    }
}

/// Whether a move to `target` with `ask` uses the lossless native same-harness fast path
/// (`Adapter::move_to`, `move_to_background`, or `move_to_codealong`) rather than a generic
/// handoff packet. All Claude Code → Claude Code moves preserve the full transcript natively
/// (including model/effort switches, follow-up prompts, Codealong-hosted sessions, and
/// `--keep`), except URL-only `Place::ClaudeApp` which cannot load a local transcript file.
pub fn uses_native_move(s: &Session, target: &AccountCfg, ask: &Ask) -> bool {
    let same_harness = of(target).is_some_and(|a| a.harness() == s.harness);
    if !same_harness || !adapter(s.harness).moves_accounts() {
        return false;
    }
    !matches!(ask.place, Some(Place::ClaudeApp))
}

/// Picks the target `Place` for a lossless native same-harness move: what `asked` requests, else
/// Codealong when the source session is hosted there and `target` supports it, Background for a
/// background job, or Terminal otherwise.
pub fn native_place(s: &Session, target: &AccountCfg, asked: Option<Place>, home: &Path) -> Result<Place> {
    let offered = start::ordered_places(target, home).map_err(|e| anyhow!(e))?;
    if let Some(p) = asked {
        if !offered.contains(&p) {
            bail!("{} can't start a session there.", target.name);
        }
        return Ok(p);
    }
    if s.hosted && offered.contains(&Place::Codealong) {
        return Ok(Place::Codealong);
    }
    if s.attach_command.is_some() {
        return Ok(Place::Background);
    }
    Ok(Place::Terminal)
}

/// Resolves the target account (any enabled account other than `account`, same or different harness).
pub fn resolve_target<'a>(cfg: &'a Config, s: &Session, account: &AccountCfg, to: &str) -> Result<&'a AccountCfg> {
    let to = to.trim();
    if to.is_empty() {
        bail!("Name an account to move or hand off “{}” to.", s.title);
    }
    let target = cfg.accounts.iter().find(|a| a.name == to).ok_or_else(|| anyhow!("There's no account named {to} in config.toml."))?;
    if target.name == account.name {
        bail!("“{}” is on {to} already.", s.title);
    }
    target.ensure_on()?;
    Ok(target)
}

/// Picks the target `Place`: what `asked` requests (checked against `offered`), else the target
/// account's preferred start place (preferring non-URL places when a default isn't explicitly set
/// to an app).
pub fn resolve_place(target: &AccountCfg, asked: Option<Place>, home: &Path) -> Result<Place> {
    let offered = start::ordered_places(target, home).map_err(|e| anyhow!(e))?;
    if offered.is_empty() {
        bail!("{} can't start sessions from Codealong right now.", target.name);
    }
    if let Some(p) = asked {
        if !offered.contains(&p) {
            bail!("{} can't start a session there.", target.name);
        }
        return Ok(p);
    }
    Ok(offered[0])
}

/// Preview a move or handoff without launching or modifying anything (`--snapshot --handoff-plan`,
/// `codealong session handoff --dry-run`).
pub fn preview(board: &Board, cfg: &Config, key: &str, ask: &Ask) -> Result<PlanPreview> {
    let (s, account, lane, cwd) = session_home(board, cfg, key)?;
    let target = resolve_target(cfg, s, account, &ask.account)?;
    let target_harness = of(target).map(|a| a.harness().word()).unwrap_or(target.harness.as_str());
    let home = crate::util::home();

    if uses_native_move(s, target, ask) {
        let place = native_place(s, target, ask.place, &home)?;
        let kind = match place {
            Place::Codealong => "native-move-codealong",
            Place::Background => "native-move-background",
            _ => "native-move-terminal",
        };
        let launch = serde_json::json!({
            "kind": kind,
            "cwd": cwd,
            "sourceAccount": account.name,
            "targetAccount": target.name,
            "model": ask.model,
            "effort": ask.effort,
        });
        return Ok(PlanPreview {
            source_key: s.key.clone(),
            source_title: s.title.clone(),
            source_harness: s.harness.word().into(),
            source_account: account.name.clone(),
            target_account: target.name.clone(),
            target_harness: target_harness.into(),
            mode: "native",
            keep_source: ask.keep,
            place,
            budget_bytes: 0,
            prompt_bytes: 0,
            transcript_bytes: 0,
            total_items: 0,
            omitted_items: 0,
            files_count: 0,
            failures_count: 0,
            prior_handoffs: 0,
            prompt: ask.prompt.clone(),
            launch,
        });
    }

    let place = resolve_place(target, ask.place, &home)?;
    let (items, read_capped) = collect_items(board, cfg, s)?;
    let preview_path = handoff_file_path(&handoffs_dir(), &s.title, &s.id);
    let budget = budget_for(place);
    let input = packet_input(board, s, lane, &cwd, &ask.prompt, Some(&preview_path), budget, read_capped);
    let packet = build_packet(&input, &items);
    let launch = start::plan_now(cfg, target, place, &cwd, &packet.prompt, ask.model.as_deref(), ask.effort.as_deref(), false)?;

    Ok(PlanPreview {
        source_key: s.key.clone(),
        source_title: s.title.clone(),
        source_harness: s.harness.word().into(),
        source_account: account.name.clone(),
        target_account: target.name.clone(),
        target_harness: target_harness.into(),
        mode: "handoff",
        keep_source: ask.keep,
        place,
        budget_bytes: budget,
        prompt_bytes: packet.prompt.len(),
        transcript_bytes: packet.transcript_markdown.len(),
        total_items: packet.total_items,
        omitted_items: packet.omitted_items,
        files_count: packet.files.len(),
        failures_count: packet.failures.len(),
        prior_handoffs: packet.prior_chain.len(),
        prompt: packet.prompt,
        launch,
    })
}

/// Executes a move or handoff: native same-harness fast path when eligible, else generic handoff
/// packet + full transcript file + target launch. Closes the source session when `!ask.keep`.
pub fn execute(board: &Board, cfg: &Config, key: &str, ask: &Ask) -> Result<Outcome> {
    let (s, account, lane, cwd) = session_home(board, cfg, key)?;
    let target = resolve_target(cfg, s, account, &ask.account)?;

    if uses_native_move(s, target, ask) {
        let home = crate::util::home();
        let place = native_place(s, target, ask.place, &home)?;
        let go_on = crate::send::by_codealong(start::GO_ON);
        let prompt = if !ask.prompt.trim().is_empty() {
            ask.prompt.trim()
        } else if s.limit.is_some() && !ask.keep {
            go_on.as_str()
        } else {
            ""
        };
        let started = start::move_to_with(board, cfg, key, &target.name, place, prompt, ask.model.as_deref(), ask.effort.as_deref(), ask.keep)?;
        return Ok(Outcome {
            started,
            in_terminal: place == Place::Terminal,
            opening: None,
            lane_id: lane.id.clone(),
            target_account: target.name.clone(),
            native: true,
        });
    }

    if !ask.keep && turn_running(s) {
        bail!("“{}” is in the middle of a turn. Stop it or let it finish, then move it.", s.title);
    }

    let home = crate::util::home();
    let place = resolve_place(target, ask.place, &home)?;
    let (items, read_capped) = collect_items(board, cfg, s)?;
    if items.is_empty() && ask.prompt.trim().is_empty() {
        bail!("Nothing's been said in “{}” yet, so there's nothing to hand off.", s.title);
    }

    let budget = budget_for(place);
    // Write the full transcript file before building the inline packet so the exact path goes into
    // the handoff header (and is skipped cleanly if there are no items to write).
    let file_path = if items.is_empty() {
        None
    } else {
        let dir = handoffs_dir();
        let path = handoff_file_path(&dir, &s.title, &s.id);
        let input = packet_input(board, s, lane, &cwd, &ask.prompt, Some(&path), budget, read_capped);
        let md = render_full_markdown(&input, &normalize_items(&items).0);
        write_transcript_file(&path, &md).ok().map(|()| path)
    };

    let input = packet_input(board, s, lane, &cwd, &ask.prompt, file_path.as_deref(), budget, read_capped);
    let packet = build_packet(&input, &items);
    let opening = crate::origins::opening(&packet.prompt);

    let source_claim = (!ask.keep).then(|| crate::send::note_waking(key)).flatten();
    let spec = StartSpec {
        lane_id: lane.id.clone(),
        account: target.name.clone(),
        place,
        prompt: packet.prompt.clone(),
        model: ask.model.clone(),
        effort: ask.effort.clone(),
        worktree: false,
        todos: Vec::new(),
        from: None,
        helper: false,
        report: false,
    };

    let (started, in_terminal, actual_place) = start::launch(board, cfg, &spec)?;
    if let Some(copy) = &started.key {
        let copy_claim = crate::send::note_waking(copy);
        if let Err(e) = crate::origins::write_handoff(copy, key, !ask.keep, Some(s.last_activity)) {
            eprintln!("Couldn't note handoff from {key} to {copy}: {e:#}");
        }
        if let Some(c) = copy_claim {
            c.keep();
        }
    }
    if let Some(c) = source_claim {
        c.keep();
    }

    let mut left = String::new();
    if !ask.keep {
        if s.live && !s.hosted {
            match crate::idle::close_one(board, cfg, key) {
                Ok(_) if s.attach_command.is_some() => left.push_str(" The old job is stopped."),
                Ok(_) => left.push_str(" Its old terminal is closed."),
                Err(e) => left.push_str(&format!(" The old one is still running: {e:#}")),
            }
        }
        if s.background.is_some() {
            left.push_str(" It had a command running in the background: ask the new session to start it again if it's still needed.");
        }
        if !s.agents.is_empty() {
            left.push_str(" It had agents running in the background: ask the new session to start them again if they're still needed.");
        }
    }

    let target_word = of(target).map(|a| a.harness().word()).unwrap_or(target.harness.as_str());
    let where_str = start::place_words(actual_place, cfg);
    let verb = if ask.keep { "Handed off" } else { "Moved" };
    let note = started.message.as_deref().map(|m| format!(" {m}")).unwrap_or_default();
    let message = format!("{verb} “{}” to {} ({target_word}): it goes on {where_str}.{note}{left}", s.title, target.name);

    Ok(Outcome {
        started: Started { key: started.key, message: Some(message) },
        in_terminal,
        opening,
        lane_id: lane.id.clone(),
        target_account: target.name.clone(),
        native: false,
    })
}

/// Action entry for `actions::MOVE`: moves to another worktree when `to.lane` is set, otherwise
/// moves or hands off to another account/provider.
pub(crate) fn run_move(engine: &crate::engine::Handle, cfg: &Config, key: &str, to: crate::actions::params::Move, from: &crate::actions::From) -> std::result::Result<Started, String> {
    let words = |e: anyhow::Error| format!("{e:#}");
    let before = engine.board();
    find(&before, cfg, key).map_err(words)?.1.ensure_on().map_err(words)?;
    if let Some(target) = cfg.accounts.iter().find(|a| a.name == to.account) {
        target.ensure_on().map_err(words)?;
    }
    if !to.lane.is_empty() {
        let stopped = stop_to_move(engine, cfg, key, &crate::stops::who(&before, from))?;
        let board = engine.board();
        let mut started = start::move_to_folder(&board, cfg, key, &to.lane).map_err(words)?;
        if stopped {
            started.message = started.message.map(|m| format!("{m} Its turn was stopped first, so it goes on from where it was interrupted."));
        }
        if let Some(copy) = started.key.as_deref().filter(|k| *k != key) {
            crate::actions::hand_over(engine, key, copy);
        }
        return Ok(started);
    }
    let ask = Ask { account: to.account, model: to.model, effort: to.effort, place: to.place, prompt: to.prompt, keep: to.keep };
    run_move_or_handoff(engine, cfg, key, ask, from)
}

/// Action entry for `actions::HANDOFF` (`codealong session handoff`): checks agent `--keep`
/// requirement, flagged-session fork blocks, boundary groups, and `--dry-run` before running.
pub(crate) fn run_handoff(engine: &crate::engine::Handle, load_cfg: fn() -> std::result::Result<Config, String>, key: &str, p: crate::actions::params::Handoff, from: &crate::actions::From) -> std::result::Result<serde_json::Value, String> {
    let words = |e: anyhow::Error| format!("{e:#}");
    let board = engine.board();
    crate::actions::ensure_fork_not_blocked(&board, key, from)?;
    if !p.dry_run && !p.keep {
        if let crate::actions::From::Command(Some(caller)) = from {
            if caller != key {
                return Err("Pass --keep to hand off another session without closing or archiving it, or move it from Codealong's windows.".into());
            }
        }
    }
    let cfg = load_cfg()?;
    let (s, source_acct) = find(&board, &cfg, key).map_err(words)?;
    source_acct.ensure_on().map_err(words)?;
    let target = resolve_target(&cfg, s, source_acct, &p.account).map_err(words)?;
    if matches!(from, crate::actions::From::Command(_) | crate::actions::From::Device) {
        let qualifies = crate::signin::current(target, now_ms(), crate::signin::Ask::IfStale).qualifies();
        if !crate::boundary::may_send(&cfg, &source_acct.name, &target.name, qualifies) {
            return Err(format!("“{}” is in a boundary group that {} isn't in (or whose sign-in doesn't qualify), so its conversation can't be handed off there.", s.title, target.name));
        }
    }
    let ask = Ask { account: p.account, model: p.model, effort: p.effort, place: p.place, prompt: p.prompt, keep: p.keep };
    if p.dry_run {
        return serde_json::to_value(preview(&board, &cfg, key, &ask).map_err(words)?).map_err(|e| format!("{e:#}"));
    }
    let started = run_move_or_handoff(engine, &cfg, key, ask, from)?;
    serde_json::to_value(started).map_err(|e| format!("{e:#}"))
}

fn run_move_or_handoff(engine: &crate::engine::Handle, cfg: &Config, key: &str, ask: Ask, from: &crate::actions::From) -> std::result::Result<Started, String> {
    let words = |e: anyhow::Error| format!("{e:#}");
    let before = engine.board();
    let stopped = if ask.keep { false } else { stop_to_move(engine, cfg, key, &crate::stops::who(&before, from))? };
    let board = engine.board();
    let was_pinned = board.pinned.iter().any(|k| k == key);
    let seen_through = board.session(key).map(|(_, s)| s.last_activity);
    let mut outcome = execute(&board, cfg, key, &ask).map_err(words)?;
    if stopped {
        outcome.started.message = outcome.started.message.map(|m| format!("{m} Its turn was stopped first, so it goes on from where it was interrupted."));
    }
    if ask.keep {
        match &outcome.started.key {
            Some(copy) => crate::actions::labels_follow(engine, &board, key, copy),
            None => crate::origins::pending_handoff(key, &outcome.lane_id, &outcome.target_account, outcome.opening, outcome.in_terminal, true, false, seen_through),
        }
    } else {
        match &outcome.started.key {
            Some(copy) => crate::actions::hand_over(engine, key, copy),
            None => {
                if !outcome.native {
                    // Atomic begin/settle: keep the source unarchived until the async target
                    // session appears on the board and `finish_pending_handover` settles the move.
                    crate::origins::pending_handoff(key, &outcome.lane_id, &outcome.target_account, outcome.opening, outcome.in_terminal, false, was_pinned, seen_through);
                } else {
                    crate::actions::archive_moved(engine, key);
                }
            }
        }
    }
    Ok(outcome.started)
}

/// Stops the turn a session is taking so it can move, as Stop does (`cancel`), and waits for the
/// board to show the turn ended: a copy made mid-turn would see it cut off, where one made after
/// sees it interrupted. Says whether there was a turn to stop. `who` asked for the move, for
/// stops.log.
fn stop_to_move(engine: &crate::engine::Handle, cfg: &Config, key: &str, who: &str) -> std::result::Result<bool, String> {
    let words = |e: anyhow::Error| format!("{e:#}");
    let running = || engine.read(|b| b.session(key).is_some_and(|(_, s)| crate::harness::turn_running(s)));
    if !running() {
        return Ok(false);
    }
    let board = engine.board();
    let (s, account) = find(&board, cfg, key).map_err(words)?;
    if !crate::harness::stoppable(s, Some(account)) {
        return Err(format!("“{}” is in the middle of a turn Codealong can't stop. Stop it where it runs, then move it.", s.title));
    }
    crate::stops::noted(crate::stops::Stop::Move, key, &s.title, who, || if crate::acp::hosts(key) { crate::acp::cancel(key) } else { adapter(s.harness).interrupt(&crate::harness::Ctx { account }, s).map(|_| ()) }).map_err(words)?;
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while running() {
        if std::time::Instant::now() > until {
            return Err(format!("Stopped “{}”, but its turn still hadn't ended 30 seconds later. Move it once it has.", s.title));
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    Ok(true)
}

/// Finds a session, its account, its own lane, and working directory without requiring its ID to
/// be shell-safe (unlike native `--resume <id>`, generic handoffs never pass the source ID to a shell).
fn session_home<'a>(board: &'a Board, cfg: &'a Config, key: &str) -> Result<(&'a Session, &'a AccountCfg, &'a Lane, PathBuf)> {
    let (s, account) = find(board, cfg, key)?;
    account.ensure_on()?;
    let own = |l: &&Lane| l.sessions.iter().any(|x| x.key == key && x.visitor.is_none());
    let holds = |l: &&Lane| l.sessions.iter().any(|x| x.key == key);
    let lane = board.lanes.iter().find(own).or_else(|| board.lanes.iter().find(holds)).ok_or_else(|| anyhow!("That session isn't on the board any more."))?;
    let cwd = if Path::new(&s.cwd).is_dir() { PathBuf::from(&s.cwd) } else { PathBuf::from(&lane.path) };
    if !cwd.is_dir() {
        bail!("{} isn't there any more.", tilde(&cwd));
    }
    Ok((s, account, lane, cwd))
}

fn packet_input<'a>(board: &'a Board, s: &'a Session, lane: &'a Lane, cwd: &'a Path, directive: &'a str, transcript_file: Option<&'a Path>, budget: usize, read_capped: bool) -> PacketInput<'a> {
    let pr = lane.pr.as_ref().map(|p| PrSummary {
        number: p.number,
        title: p.title.clone(),
        url: p.url.clone(),
        state: format!("{:?}", p.state).to_lowercase(),
    });
    let todos: Vec<String> = board.todos.iter().filter(|t| t.sessions.iter().any(|k| k == &s.key)).map(|t| t.title.clone()).collect();
    PacketInput {
        source_key: &s.key,
        source_title: &s.title,
        source_harness: s.harness.word(),
        source_account: &s.account,
        cwd: cwd.to_str().unwrap_or(&s.cwd),
        branch: s.branch.as_deref().or(lane.branch.as_deref()),
        repo: (!lane.repo.is_empty()).then_some(lane.repo.as_str()),
        pr,
        todos,
        reason: Reason::of(s),
        directive,
        transcript_file,
        budget,
        read_capped,
    }
}

/// Reads the source session's canonical conversation via `transcript::page` (every harness
/// implements it), walking backwards from the newest page up to `MAX_ITEMS` / `MAX_PAGES`.
/// Falls back to the board's cached prompt/reply snippets if the harness hasn't flushed a record yet.
fn collect_items(board: &Board, cfg: &Config, s: &Session) -> Result<(Vec<Item>, bool)> {
    let mut pages: Vec<Vec<Item>> = Vec::new();
    let mut before: Option<u64> = None;
    let mut total = 0usize;
    let mut read_capped = false;

    for page_idx in 0..MAX_PAGES {
        match transcript::page(board, cfg, &s.key, before, PAGE_LIMIT) {
            Ok(page) => {
                total += page.items.len();
                let next = page.before;
                pages.push(page.items);
                match next {
                    Some(cursor) if total < MAX_ITEMS && page_idx + 1 < MAX_PAGES => before = Some(cursor),
                    Some(_) => {
                        read_capped = true;
                        break;
                    }
                    None => break,
                }
            }
            Err(_) if page_idx == 0 => {
                // Brand-new or unreadable record: synthesize minimal items from board metadata if any.
                return Ok((fallback_items(s), false));
            }
            Err(_) => {
                read_capped = true;
                break;
            }
        }
    }

    let items: Vec<Item> = pages.into_iter().rev().flatten().collect();
    if items.is_empty() {
        return Ok((fallback_items(s), false));
    }
    Ok((items, read_capped))
}

fn fallback_items(s: &Session) -> Vec<Item> {
    let mut out = Vec::new();
    if let Some(first) = s.first_prompt.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        out.push(transcript::item(s.started_at.unwrap_or(s.last_prompt_at), "you", first));
    }
    if let Some(last) = s.last_prompt.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        if out.first().map(|i| i.text.as_str()) != Some(last) {
            out.push(transcript::item(s.last_prompt_at, "you", last));
        }
    }
    let reply = s.reply_tail.as_deref().or(s.reply_head.as_deref()).or(s.excerpt.as_deref()).map(str::trim).filter(|t| !t.is_empty());
    if let Some(r) = reply {
        out.push(transcript::item(s.last_activity, "agent", r));
    }
    out
}

/// Normalizes raw `transcript::Item`s into a clean chronological stream:
/// - drops withdrawn prompts;
/// - pairs `result` items onto their matching `tool` call by `id`;
/// - unwraps any leading prior handoff prompt so chained handoffs (`A → B → C`) stay flat.
pub fn normalize_items(raw: &[Item]) -> (Vec<Item>, Vec<PriorHandoff>) {
    let mut out: Vec<Item> = Vec::with_capacity(raw.len());
    let mut tool_idx: HashMap<String, usize> = HashMap::new();

    for it in raw {
        if it.withdrawn {
            continue;
        }
        if it.kind == "result" {
            if let Some(id) = it.id.as_deref() {
                if let Some(&idx) = tool_idx.get(id) {
                    let call = &mut out[idx];
                    if call.output.is_none() {
                        call.output = it.output.clone().or_else(|| (!it.text.trim().is_empty()).then(|| it.text.clone()));
                    }
                    call.failed = call.failed || it.failed;
                    if call.answers.is_empty() && !it.answers.is_empty() {
                        call.answers = it.answers.clone();
                    }
                    if call.link.is_none() {
                        call.link = it.link.clone();
                    }
                    continue;
                }
            }
            // Keep unpaired failed results or Q&A answers; skip routine orphan results.
            if !it.failed && it.answers.is_empty() {
                continue;
            }
        }
        if it.kind == "tool" {
            if let Some(id) = it.id.as_deref() {
                tool_idx.insert(id.to_string(), out.len());
            }
        }
        out.push(it.clone());
    }

    let mut prior_chain = Vec::new();
    if let Some(first_you) = out.iter().position(|it| it.kind == "you") {
        if let Some((priors, residual_directive)) = unwrap_prior_handoff(&out[first_you].text) {
            prior_chain = priors;
            match residual_directive {
                Some(dir) => out[first_you].text = dir,
                None => {
                    out.remove(first_you);
                }
            }
        }
    }

    (out, prior_chain)
}

/// Detects a prompt generated by `build_packet`, extracts its `PriorHandoff` chain, and returns
/// any explicit `## Next step` directive the user gave it so that intent stays in the conversation.
pub fn unwrap_prior_handoff(text: &str) -> Option<(Vec<PriorHandoff>, Option<String>)> {
    let trimmed = text.trim_start();
    let rest = trimmed.strip_prefix(HEADER_PREFIX)?;
    let first_line = rest.lines().next()?;
    let (title, meta) = first_line.rsplit_once("\" (")?;
    let meta = meta.strip_suffix(')')?;
    let (source_harness, source_account) = meta.split_once(" · ")?;

    let mut source_key = None;
    let mut transcript_file = None;
    let mut earlier = Vec::new();

    for line in trimmed.lines().skip(1) {
        if line.starts_with("## ") {
            break;
        }
        if let Some(v) = line.strip_prefix("- Source session: ") {
            source_key = Some(v.trim().trim_matches('`').to_string());
        } else if let Some(v) = line.strip_prefix("- Full transcript: ") {
            let path = v.trim().split(' ').next().unwrap_or("").trim_matches('`');
            if !path.is_empty() {
                transcript_file = Some(path.to_string());
            }
        } else if let Some(v) = line.strip_prefix("- Prior handoff: ") {
            if let Some(parsed) = parse_prior_line(v) {
                earlier.push(parsed);
            }
        }
    }

    earlier.push(PriorHandoff {
        title: title.to_string(),
        source_harness: source_harness.trim().to_string(),
        source_account: source_account.trim().to_string(),
        source_key,
        transcript_file,
    });

    // Preserve a user-written `## Next step` if it wasn't one of our boilerplate default directives.
    let residual = trimmed.rsplit_once(NEXT_STEP_HEADING).and_then(|(_, after)| {
        let body = after.trim();
        let is_default = body.is_empty()
            || body == Reason::Manual.default_directive()
            || body == Reason::CutOff.default_directive()
            || body == Reason::Limit(String::new()).default_directive()
            || body == Reason::ApiError(String::new()).default_directive();
        (!is_default).then(|| body.to_string())
    });

    Some((earlier, residual))
}

fn parse_prior_line(line: &str) -> Option<PriorHandoff> {
    // Format: `"<title>" (<harness> · <account>) [— transcript: `<path>`]`
    let rest = line.trim().strip_prefix('"')?;
    let (title, after) = rest.split_once("\" (")?;
    let (who, tail) = after.split_once(')')?;
    let (source_harness, source_account) = who.split_once(" · ")?;
    let transcript_file = tail
        .split_once("transcript: ")
        .map(|(_, p)| p.trim().trim_matches('`').to_string())
        .filter(|p| !p.is_empty());
    Some(PriorHandoff {
        title: title.to_string(),
        source_harness: source_harness.trim().to_string(),
        source_account: source_account.trim().to_string(),
        source_key: None,
        transcript_file,
    })
}

/// Builds the full tiered handoff packet from canonical `transcript::Item`s.
pub fn build_packet(input: &PacketInput<'_>, raw_items: &[Item]) -> Packet {
    let (items, prior_chain) = normalize_items(raw_items);
    let transcript_markdown = render_full_markdown(input, &items);
    let files = extract_files(&items, input.cwd);
    let failures = extract_failures(&items);

    let header = render_header(input, &prior_chain);
    let directive_block = render_directive(&input.reason, input.directive);

    let fixed_bytes = header.len() + directive_block.len() + 64;
    let remaining = input.budget.saturating_sub(fixed_bytes);

    let mut sections: Vec<String> = vec![header];
    let mut used = 0usize;

    // 1. Footprint index (files touched + notable failures), up to ~12% of remaining (max 2,400 B).
    let footprint_budget = (remaining / 8).min(2_400);
    if let Some(fp) = render_footprint(&files, &failures, footprint_budget) {
        used += fp.len() + 2;
        sections.push(fp);
    }

    // Format every item for the high-fidelity tail once so we know if the entire session fits verbatim.
    let rendered_entries: Vec<(usize, String)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| format_recent_item(it, input.cwd).map(|s| (i, s)))
        .collect();

    let total_entries = rendered_entries.len();
    let avail_for_history = remaining.saturating_sub(used);

    // Try fitting the entire conversation verbatim first: short sessions need zero compaction!
    let full_len: usize = rendered_entries.iter().map(|(_, s)| s.len() + 2).sum();
    let mut omitted_items = 0usize;

    if !rendered_entries.is_empty() && full_len <= avail_for_history {
        let body = rendered_entries.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join("\n\n");
        sections.push(format!("## Conversation\n\n{body}"));
    } else if !rendered_entries.is_empty() {
        // Tiered packing: Anchor (goal + latest compaction) + Compacted middle + High-fidelity tail.
        let anchor_budget = (avail_for_history / 6).min(3_200);
        let (anchor_block, anchor_indices) = render_anchor(&items, anchor_budget);
        let after_anchor = avail_for_history.saturating_sub(anchor_block.as_ref().map_or(0, |a| a.len() + 2));

        // Bulk (~75%) to recent turns verbatim; rest (~25%) to compacted middle timeline.
        let recent_budget = (after_anchor * 3) / 4;
        let mut recent: Vec<(usize, String)> = Vec::new();
        let mut recent_used = 0usize;
        for (idx, text) in rendered_entries.iter().rev() {
            let clipped = if text.chars().count() > SINGLE_TURN_MAX_CHARS {
                clip_head_tail(text, SINGLE_TURN_MAX_CHARS)
            } else {
                text.clone()
            };
            let add = clipped.len() + 2;
            if !recent.is_empty() && recent_used + add > recent_budget {
                break;
            }
            if recent.is_empty() && add > recent_budget && recent_budget > 200 {
                recent.push((*idx, clip_head_tail(&clipped, recent_budget.saturating_sub(40))));
                recent_used = recent_budget;
                break;
            }
            recent_used += add;
            recent.push((*idx, clipped));
        }
        recent.reverse();

        let recent_start = recent.first().map(|(i, _)| *i).unwrap_or(items.len());
        let middle_budget = after_anchor.saturating_sub(recent_used);

        // Middle turns: strictly before `recent_start`, excluding items already shown in `anchor`.
        let middle_candidates: Vec<String> = items
            .iter()
            .enumerate()
            .take(recent_start)
            .filter(|(i, _)| !anchor_indices.contains(i))
            .filter_map(|(_, it)| format_compact_item(it, input.cwd))
            .collect();

        let mut kept_middle: Vec<String> = Vec::new();
        let mut mid_used = 0usize;
        for line in middle_candidates.iter().rev() {
            let add = line.len() + 1;
            if mid_used + add > middle_budget {
                break;
            }
            mid_used += add;
            kept_middle.push(line.clone());
        }
        kept_middle.reverse();
        omitted_items = middle_candidates.len().saturating_sub(kept_middle.len());

        if let Some(a) = anchor_block {
            // Only emit the anchor if the first item wasn't already included in `recent`.
            if !anchor_indices.iter().all(|idx| *idx >= recent_start) {
                sections.push(a);
            }
        }
        if !kept_middle.is_empty() || omitted_items > 0 {
            let mut block = String::from("## Earlier turns (compacted)\n");
            if omitted_items > 0 {
                block.push_str(&format!("\n*[{omitted_items} earlier entries omitted — read the full transcript file if needed]*\n"));
            }
            if !kept_middle.is_empty() {
                block.push('\n');
                block.push_str(&kept_middle.join("\n"));
            }
            sections.push(block);
        }
        if !recent.is_empty() {
            let body = recent.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join("\n\n");
            sections.push(format!("## Recent turns\n\n{body}"));
        }
    }

    sections.push(directive_block);
    let prompt = sections.join("\n\n");

    Packet {
        prompt,
        transcript_markdown,
        total_items: total_entries,
        omitted_items,
        files,
        failures,
        prior_chain,
        read_capped: input.read_capped,
    }
}

fn render_header(input: &PacketInput<'_>, prior_chain: &[PriorHandoff]) -> String {
    let title = one_line_clean(input.source_title);
    let mut lines = vec![
        format!("{HEADER_PREFIX}{title}\" ({} · {})", input.source_harness, input.source_account),
        String::new(),
        format!("You are taking over this coding session from {} ({}).", input.source_harness, input.source_account),
        format!("- Source session: `{}`", input.source_key),
    ];

    let mut ws = format!("- Workspace: `{}`", input.cwd);
    let mut details = Vec::new();
    if let Some(repo) = input.repo.filter(|r| !r.is_empty()) {
        details.push(format!("repo {repo}"));
    }
    if let Some(branch) = input.branch.filter(|b| !b.is_empty()) {
        details.push(format!("branch `{branch}`"));
    }
    if !details.is_empty() {
        ws.push_str(&format!(" ({})", details.join(", ")));
    }
    lines.push(ws);

    if let Some(pr) = &input.pr {
        lines.push(format!("- Pull request: #{} \"{}\" ({}, {})", pr.number, one_line_clean(&pr.title), pr.state, pr.url));
    }
    if !input.todos.is_empty() {
        let joined: Vec<String> = input.todos.iter().take(5).map(|t| one_line_clean(t)).collect();
        lines.push(format!("- Linked todos: {}", joined.join("; ")));
    }
    for p in prior_chain {
        let mut l = format!("- Prior handoff: \"{}\" ({} · {})", one_line_clean(&p.title), p.source_harness, p.source_account);
        if let Some(path) = &p.transcript_file {
            l.push_str(&format!(" — transcript: `{path}`"));
        }
        lines.push(l);
    }
    lines.push(format!("- Reason: {}", input.reason.line()));
    if let Some(path) = input.transcript_file {
        let note = if input.read_capped {
            "(most recent turns captured; read on demand if you need earlier details)"
        } else {
            "(read on demand if you need earlier details)"
        };
        lines.push(format!("- Full transcript: `{}` {note}", path.display()));
    }
    lines.push(String::new());
    lines.push("The workspace on disk already reflects all changes the previous agent made. Inspect current files or `git status` / `git diff` rather than assuming, do not repeat failed attempts, and follow any project instructions (`CLAUDE.md`, `AGENTS.md`, etc.) in the workspace.".into());
    lines.join("\n")
}

fn render_directive(reason: &Reason, directive: &str) -> String {
    let dir = directive.trim();
    let body = if dir.is_empty() { reason.default_directive() } else { dir };
    format!("{NEXT_STEP_HEADING}\n\n{body}")
}

fn render_footprint(files: &[String], failures: &[String], budget: usize) -> Option<String> {
    if (files.is_empty() && failures.is_empty()) || budget < 120 {
        return None;
    }
    let mut parts = vec!["## Session footprint".to_string()];
    if !files.is_empty() {
        let list = files.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", ");
        parts.push(format!("- Files referenced: {list}"));
    }
    if !failures.is_empty() {
        parts.push("- Notable failures:".into());
        for f in failures {
            parts.push(format!("  - {f}"));
        }
    }
    let text = parts.join("\n");
    if text.len() <= budget {
        Some(text)
    } else {
        Some(clip_chars(&text, budget.saturating_sub(12)))
    }
}

/// Extracts the anchor: the session's opening user prompt and the latest compaction summary note
/// (when the source harness compacted earlier context).
fn render_anchor(items: &[Item], budget: usize) -> (Option<String>, HashSet<usize>) {
    let mut indices = HashSet::new();
    if budget < 200 {
        return (None, indices);
    }
    let mut parts = Vec::new();
    if let Some((i, first)) = items.iter().enumerate().find(|(_, it)| it.kind == "you" && !it.text.trim().is_empty()) {
        indices.insert(i);
        let goal_budget = (budget * 2 / 3).min(1_600);
        parts.push(format!("**Original request:**\n{}", clip_chars(first.text.trim(), goal_budget)));
    }
    if let Some((i, note)) = items.iter().enumerate().rev().find(|(_, it)| it.kind == "note" && !it.stop && is_compaction_note(&it.text)) {
        indices.insert(i);
        let note_budget = (budget / 2).min(1_400);
        parts.push(format!("**Latest compaction summary:**\n{}", clip_chars(note.text.trim(), note_budget)));
    }
    if parts.is_empty() {
        return (None, indices);
    }
    let block = format!("## Goal & checkpoint\n\n{}", parts.join("\n\n"));
    (Some(block), indices)
}

fn is_compaction_note(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    text.len() > 80 && (lower.contains("compact") || lower.contains("summar") || lower.contains("context"))
}

fn extract_files(items: &[Item], cwd: &str) -> Vec<String> {
    // Keep the most-recently-touched deduplicated files in reverse-recency order, then reverse.
    let mut seen = HashSet::new();
    let mut recent = Vec::new();
    for it in items.iter().rev() {
        let Some(f) = it.file.as_deref().map(str::trim).filter(|f| !f.is_empty()) else { continue };
        let rel = relativize(f, cwd);
        if seen.insert(rel.clone()) {
            recent.push(rel);
            if recent.len() >= MAX_FILES {
                break;
            }
        }
    }
    recent.reverse();
    recent
}

fn extract_failures(items: &[Item]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut recent = Vec::new();
    for it in items.iter().rev() {
        if !it.failed {
            continue;
        }
        let label = one_line_clean(&it.text);
        let err_line = it
            .output
            .as_deref()
            .and_then(|o| o.lines().map(str::trim).find(|l| !l.is_empty()))
            .map(|l| clip_chars(l, 120));
        let entry = match err_line {
            Some(e) if !label.is_empty() => format!("{label} — {e}"),
            Some(e) => e,
            None if !label.is_empty() => label,
            None => continue,
        };
        let entry = clip_chars(&entry, 180);
        if seen.insert(entry.clone()) {
            recent.push(entry);
            if recent.len() >= MAX_FAILURES {
                break;
            }
        }
    }
    recent.reverse();
    recent
}

fn format_recent_item(it: &Item, cwd: &str) -> Option<String> {
    match it.kind {
        "you" => {
            let t = it.text.trim();
            (!t.is_empty()).then(|| format!("**User:**\n{t}"))
        }
        "agent" => {
            let t = it.text.trim();
            (!t.is_empty()).then(|| format!("**Assistant:**\n{t}"))
        }
        "peer" => {
            let t = it.text.trim();
            if t.is_empty() {
                return None;
            }
            let who = it.from.as_deref().unwrap_or("another agent");
            Some(format!("**Message from {who}:**\n{t}"))
        }
        "codealong" => {
            let t = it.text.trim();
            (!t.is_empty()).then(|| format!("**Codealong:**\n{t}"))
        }
        "note" => {
            let t = it.text.trim();
            (!t.is_empty()).then(|| format!("*[Note: {}]*", one_line_clean(t)))
        }
        "tool" => format_recent_tool(it, cwd),
        "result" => {
            if !it.answers.is_empty() {
                return Some(format_qa(&it.questions, &it.answers));
            }
            if it.failed {
                let out = it.output.as_deref().unwrap_or(&it.text).trim();
                if !out.is_empty() {
                    return Some(format!("- Tool error: {}", clip_tail(out, FAILED_OUTPUT_MAX_CHARS)));
                }
            }
            None
        }
        _ => None,
    }
}

fn format_recent_tool(it: &Item, cwd: &str) -> Option<String> {
    if !it.questions.is_empty() || !it.answers.is_empty() {
        return Some(format_qa(&it.questions, &it.answers));
    }
    let label = one_line_clean(&it.text);
    if label.is_empty() {
        return None;
    }
    let file_note = it.file.as_deref().filter(|f| !label.contains(*f)).map(|f| format!(" (`{}`)", relativize(f, cwd))).unwrap_or_default();
    if it.failed {
        let mut line = format!("- Tool failed: {label}{file_note}");
        if let Some(out) = it.output.as_deref().map(str::trim).filter(|o| !o.is_empty()) {
            line.push_str(&format!("\n  ```\n  {}\n  ```", clip_tail(out, FAILED_OUTPUT_MAX_CHARS)));
        }
        Some(line)
    } else {
        Some(format!("- Tool: {label}{file_note}"))
    }
}

fn format_compact_item(it: &Item, cwd: &str) -> Option<String> {
    match it.kind {
        "you" => {
            let t = one_line_clean(&it.text);
            (!t.is_empty()).then(|| format!("- User: {}", clip_chars(&t, COMPACT_MAX_CHARS)))
        }
        "agent" => {
            let t = one_line_clean(&it.text);
            (!t.is_empty()).then(|| format!("- Assistant: {}", clip_chars(&t, COMPACT_MAX_CHARS)))
        }
        "peer" => {
            let t = one_line_clean(&it.text);
            let who = it.from.as_deref().unwrap_or("agent");
            (!t.is_empty()).then(|| format!("- {who}: {}", clip_chars(&t, COMPACT_MAX_CHARS)))
        }
        "codealong" => {
            let t = one_line_clean(&it.text);
            (!t.is_empty()).then(|| format!("- Codealong: {}", clip_chars(&t, COMPACT_MAX_CHARS)))
        }
        "note" => {
            let t = one_line_clean(&it.text);
            (!t.is_empty()).then(|| format!("- Note: {}", clip_chars(&t, COMPACT_MAX_CHARS)))
        }
        "tool" => {
            if !it.answers.is_empty() {
                let qa = one_line_clean(&format_qa(&it.questions, &it.answers));
                return Some(format!("- {}", clip_chars(&qa, COMPACT_MAX_CHARS)));
            }
            if it.failed {
                let label = one_line_clean(&it.text);
                let file = it.file.as_deref().map(|f| format!(" ({})", relativize(f, cwd))).unwrap_or_default();
                return (!label.is_empty()).then(|| format!("- Failed tool: {}{file}", clip_chars(&label, COMPACT_MAX_CHARS)));
            }
            None
        }
        _ => None,
    }
}

fn format_qa(questions: &[crate::harness::Question], answers: &std::collections::BTreeMap<String, Vec<String>>) -> String {
    let mut lines = vec!["**Questions & answers:**".to_string()];
    if !answers.is_empty() {
        for (q, ans) in answers {
            lines.push(format!("- Q: {} → A: {}", one_line_clean(q), ans.join(", ")));
        }
    } else {
        for q in questions {
            lines.push(format!("- Q: {}", one_line_clean(&q.question)));
        }
    }
    lines.join("\n")
}

/// Renders the complete Markdown transcript saved to disk for on-demand reading by the target agent.
pub fn render_full_markdown(input: &PacketInput<'_>, items: &[Item]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Full transcript: {}\n\n", one_line_clean(input.source_title)));
    out.push_str(&format!("- Source: `{}` ({} · {})\n", input.source_key, input.source_harness, input.source_account));
    out.push_str(&format!("- Workspace: `{}`\n", input.cwd));
    if let Some(branch) = input.branch.filter(|b| !b.is_empty()) {
        out.push_str(&format!("- Branch: `{branch}`\n"));
    }
    if let Some(pr) = &input.pr {
        out.push_str(&format!("- PR: #{} \"{}\" ({})\n", pr.number, one_line_clean(&pr.title), pr.url));
    }
    out.push('\n');

    for it in items {
        match it.kind {
            "you" if !it.text.trim().is_empty() => {
                out.push_str(&format!("## User\n\n{}\n\n", it.text.trim()));
            }
            "agent" if !it.text.trim().is_empty() => {
                out.push_str(&format!("## Assistant\n\n{}\n\n", it.text.trim()));
            }
            "peer" if !it.text.trim().is_empty() => {
                let who = it.from.as_deref().unwrap_or("another agent");
                out.push_str(&format!("## Message from {who}\n\n{}\n\n", it.text.trim()));
            }
            "codealong" if !it.text.trim().is_empty() => {
                out.push_str(&format!("## Codealong\n\n{}\n\n", it.text.trim()));
            }
            "note" if !it.text.trim().is_empty() => {
                out.push_str(&format!("> Note: {}\n\n", it.text.trim()));
            }
            "tool" => {
                if !it.questions.is_empty() || !it.answers.is_empty() {
                    out.push_str(&format!("{}\n\n", format_qa(&it.questions, &it.answers)));
                    continue;
                }
                let label = one_line_clean(&it.text);
                if label.is_empty() {
                    continue;
                }
                let status = if it.failed { " [FAILED]" } else { "" };
                out.push_str(&format!("### Tool{status}: {label}\n"));
                if let Some(f) = it.file.as_deref() {
                    out.push_str(&format!("File: `{}`\n", relativize(f, input.cwd)));
                }
                if let Some(inp) = it.input.as_deref().map(str::trim).filter(|s| !s.is_empty() && !label.contains(*s)) {
                    out.push_str(&format!("\n```\n{}\n```\n", clip_chars(inp, 2_000)));
                }
                if let Some(res) = it.output.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    let cap = if it.failed { 3_000 } else { 1_000 };
                    out.push_str(&format!("\nOutput:\n```\n{}\n```\n", clip_tail(res, cap)));
                }
                out.push('\n');
            }
            _ => {}
        }
    }
    out
}

/// Deterministic, path-safe transcript file path under `dir`.
pub fn handoff_file_path(dir: &Path, title: &str, id: &str) -> PathBuf {
    let (y, m, d) = crate::util::civil_from_days(crate::util::today_local());
    let short: String = id.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect();
    let short = if short.is_empty() { "session".to_string() } else { short };
    let slug: String = title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .take(5)
        .collect::<Vec<_>>()
        .join("-");
    let name = if slug.is_empty() {
        format!("{y:04}-{m:02}-{d:02}-{short}.md")
    } else {
        format!("{y:04}-{m:02}-{d:02}-{short}-{}.md", clip_ascii(&slug, 40))
    };
    dir.join(name)
}

fn write_transcript_file(path: &Path, markdown: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        prune_old_files(parent, now_ms());
    }
    std::fs::write(path, markdown)?;
    Ok(())
}

fn prune_old_files(dir: &Path, now: i64) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if crate::util::mtime_ms(&p).is_some_and(|mt| now - mt > KEEP_FILES_MS) {
            let _ = std::fs::remove_file(&p);
        }
    }
}

fn relativize(path: &str, cwd: &str) -> String {
    if !cwd.is_empty() {
        let prefix = cwd.trim_end_matches('/');
        if let Some(rest) = path.strip_prefix(prefix).and_then(|r| r.strip_prefix('/')) {
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }
    path.to_string()
}

fn one_line_clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clip_ascii(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

/// Truncates `s` to at most `max_chars` characters with a clean ellipsis.
fn clip_chars(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut iter = s.char_indices();
    match iter.nth(max_chars) {
        None => s.to_string(),
        Some((byte_idx, _)) => {
            let keep = s[..byte_idx].trim_end();
            format!("{keep}…")
        }
    }
}

/// Keeps the tail of long error output (where exit codes and assertions live).
fn clip_tail(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let skip = count - max_chars;
    let start = s.char_indices().nth(skip).map(|(i, _)| i).unwrap_or(0);
    let tail = s[start..].trim_start();
    format!("…\n{tail}")
}

/// Clips an oversized single turn by keeping both its opening (~60%) and closing (~40%).
fn clip_head_tail(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars || max_chars < 40 {
        return clip_chars(s, max_chars);
    }
    let head_n = (max_chars * 3) / 5;
    let tail_n = max_chars.saturating_sub(head_n + 24);
    let head_end = s.char_indices().nth(head_n).map(|(i, _)| i).unwrap_or(s.len());
    let tail_start = s.char_indices().nth(count.saturating_sub(tail_n)).map(|(i, _)| i).unwrap_or(0);
    format!("{}\n\n…[clipped]…\n\n{}", s[..head_end].trim_end(), s[tail_start..].trim_start())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Harness;
    use std::collections::BTreeMap;

    fn sample_input<'a>(transcript_file: Option<&'a Path>, budget: usize, directive: &'a str) -> PacketInput<'a> {
        PacketInput {
            source_key: "claude:claude:abc12345",
            source_title: "Fix webhook retries",
            source_harness: "Claude Code",
            source_account: "claude",
            cwd: "/Users/me/work/api",
            branch: Some("fix/retries"),
            repo: Some("api"),
            pr: Some(PrSummary { number: 42, title: "Fix webhook retries".into(), url: "https://github.com/acme/api/pull/42".into(), state: "open".into() }),
            todos: vec!["Fix flaky retry backoff".into()],
            reason: Reason::Manual,
            directive,
            transcript_file,
            budget,
            read_capped: false,
        }
    }

    fn tool(id: &str, text: &str, file: Option<&str>) -> Item {
        Item { id: Some(id.into()), file: file.map(String::from), ..transcript::item(10, "tool", text) }
    }

    fn result(id: &str, output: &str, failed: bool) -> Item {
        Item { id: Some(id.into()), output: Some(output.into()), failed, ..transcript::item(11, "result", "") }
    }

    #[test]
    fn short_conversation_renders_verbatim_with_workspace_metadata_and_footprint() {
        let path = PathBuf::from("/tmp/handoffs/2026-11-24-abc-fix.md");
        let items = vec![
            transcript::item(1, "you", "Fix the exponential backoff bug in webhooks."),
            Item { withdrawn: true, ..transcript::item(2, "you", "Never mind this typo") },
            tool("t1", "Read · src/retry.rs", Some("/Users/me/work/api/src/retry.rs")),
            result("t1", "fn backoff() {}", false),
            tool("t2", "Bash · cargo test", None),
            result("t2", "thread 'backoff' panicked at overflow", true),
            transcript::item(12, "agent", "Found an overflow in `retry.rs:42` when attempt > 30."),
        ];
        let input = sample_input(Some(&path), CLI_BUDGET, "Add a saturating cap and regression test.");
        let packet = build_packet(&input, &items);

        assert!(packet.prompt.starts_with("# Handoff from \"Fix webhook retries\" (Claude Code · claude)"));
        assert!(packet.prompt.contains("branch `fix/retries`"));
        assert!(packet.prompt.contains("Pull request: #42"));
        assert!(packet.prompt.contains("Linked todos: Fix flaky retry backoff"));
        assert!(packet.prompt.contains("Full transcript: `/tmp/handoffs/2026-11-24-abc-fix.md`"));
        assert!(packet.prompt.contains("`src/retry.rs`"), "relativized to cwd");
        assert!(packet.prompt.contains("panicked at overflow"), "failure captured");
        assert!(!packet.prompt.contains("Never mind this typo"), "withdrawn prompt dropped");
        assert!(packet.prompt.contains("## Conversation"), "short session keeps full verbatim conversation");
        assert!(packet.prompt.contains("## Next step\n\nAdd a saturating cap and regression test."));
        assert_eq!(packet.omitted_items, 0);
        assert_eq!(packet.files, vec!["src/retry.rs"]);
        assert_eq!(packet.failures.len(), 1);
    }

    #[test]
    fn long_conversation_tiers_goal_compaction_middle_and_recent_within_budget() {
        let mut items = vec![
            transcript::item(1, "you", "Original goal: migrate billing ledger to double-entry accounting."),
        ];
        for i in 0..80 {
            items.push(transcript::item(10 + i * 2, "agent", format!("Step {i}: updated module {i} with intermediate notes and verification output. {}", "detail ".repeat(40))));
            items.push(transcript::item(11 + i * 2, "you", format!("Follow up {i}: check edge case {i} in ledger reconciliation.")));
        }
        items.push(transcript::item(500, "note", "Conversation compacted: completed schema migration and phase 1 backfill; remaining work is invoice reconciliation."));
        items.push(transcript::item(501, "you", "Now wire up invoice reconciliation tests."));
        items.push(transcript::item(502, "agent", "I wrote the test scaffold in tests/reconcile.rs; two assertions are failing on rounding."));

        let path = PathBuf::from("/tmp/handoffs/long.md");
        let input = sample_input(Some(&path), 4_500, "Fix the rounding assertions.");
        let packet = build_packet(&input, &items);

        assert!(packet.prompt.len() <= 5_200, "stays close to budget: {}", packet.prompt.len());
        assert!(packet.prompt.contains("Original goal: migrate billing ledger"), "preserves original goal");
        assert!(packet.prompt.contains("Latest compaction summary:"), "preserves compaction summary");
        assert!(packet.prompt.contains("## Earlier turns (compacted)"), "compacts middle");
        assert!(packet.prompt.contains("## Recent turns"), "keeps recent turns verbatim");
        assert!(packet.prompt.contains("two assertions are failing on rounding"), "latest turn verbatim");
        assert!(packet.prompt.ends_with("## Next step\n\nFix the rounding assertions."), "directive never truncated");
        assert!(packet.omitted_items > 0);
        // Full markdown transcript retains every turn, including step 0 and step 79.
        assert!(packet.transcript_markdown.contains("Step 0:"));
        assert!(packet.transcript_markdown.contains("Step 79:"));
    }

    #[test]
    fn chained_handoff_unwraps_prior_header_without_nesting() {
        // First handoff: Claude → Codex.
        let p1_file = PathBuf::from("/tmp/handoffs/first.md");
        let first_items = vec![
            transcript::item(1, "you", "Build the export CLI."),
            transcript::item(2, "agent", "Scaffolded CLI flags."),
        ];
        let p1 = build_packet(&sample_input(Some(&p1_file), CLI_BUDGET, "Add CSV streaming."), &first_items);

        // Second handoff: Codex → Antigravity, where Codex's first user message was `p1.prompt`.
        let p2_file = PathBuf::from("/tmp/handoffs/second.md");
        let second_items = vec![
            transcript::item(10, "you", p1.prompt.clone()),
            transcript::item(11, "agent", "Implemented CSV streaming in src/csv.rs."),
        ];
        let input2 = PacketInput {
            source_key: "codex:codex:def67890",
            source_title: "Export CLI",
            source_harness: "Codex",
            source_account: "codex",
            ..sample_input(Some(&p2_file), CLI_BUDGET, "Review performance.")
        };
        let p2 = build_packet(&input2, &second_items);

        assert_eq!(p2.prior_chain.len(), 1);
        assert_eq!(p2.prior_chain[0].source_harness, "Claude Code");
        assert_eq!(p2.prior_chain[0].transcript_file.as_deref(), Some("/tmp/handoffs/first.md"));
        assert!(p2.prompt.contains("- Prior handoff: \"Fix webhook retries\" (Claude Code · claude) — transcript: `/tmp/handoffs/first.md`"));
        assert!(p2.prompt.contains("Add CSV streaming."), "kept user's previous directive");
        assert!(p2.prompt.contains("Implemented CSV streaming"), "kept second session's work");
        assert_eq!(p2.prompt.matches("# Handoff from \"").count(), 1, "no nested handoff headers");
    }

    #[test]
    fn questions_and_answers_and_multibyte_unicode_clip_safely() {
        let mut answers = BTreeMap::new();
        answers.insert("Which strategy?".into(), vec!["Tiered deterministic".into()]);
        let items = vec![
            Item { id: Some("q1".into()), ..transcript::item(1, "tool", "AskUserQuestion") },
            Item { id: Some("q1".into()), answers, ..transcript::item(2, "result", "") },
            transcript::item(3, "agent", "🔥".repeat(2000)),
        ];
        let input = sample_input(None, URL_BUDGET, "");
        let packet = build_packet(&input, &items);
        assert!(packet.prompt.contains("Which strategy? → A: Tiered deterministic"));
        assert!(packet.prompt.contains("## Next step"));
    }

    #[test]
    fn uses_native_move_for_all_same_harness_claude_moves_except_claude_app() {
        let acct = |name: &str, harness: &str| AccountCfg {
            name: name.into(),
            harness: harness.into(),
            home: format!("~/.{name}"),
            command: None,
            start: None,
            acp: None,
            monthly: None,
            per_token: false,
            usage_cap: None,
            disabled: false,
            chrome_profile: None,
        };
        let claude_s = Session::new(Harness::Claude, "claude", "s1");
        let codex_s = Session::new(Harness::Codex, "codex", "s2");
        let claude2 = acct("claude2", "claude");
        let codex = acct("codex", "codex");
        let codex2 = acct("codex2", "codex");

        assert!(uses_native_move(&claude_s, &claude2, &Ask { account: "claude2".into(), ..Default::default() }));
        assert!(uses_native_move(&claude_s, &claude2, &Ask { account: "claude2".into(), keep: true, ..Default::default() }));
        assert!(uses_native_move(&claude_s, &claude2, &Ask { account: "claude2".into(), prompt: "Do X".into(), ..Default::default() }));
        assert!(uses_native_move(&claude_s, &claude2, &Ask { account: "claude2".into(), model: Some("opus".into()), ..Default::default() }));
        assert!(uses_native_move(&claude_s, &claude2, &Ask { account: "claude2".into(), place: Some(Place::Codealong), ..Default::default() }));
        assert!(!uses_native_move(&claude_s, &claude2, &Ask { account: "claude2".into(), place: Some(Place::ClaudeApp), ..Default::default() }));
        assert!(!uses_native_move(&claude_s, &codex, &Ask { account: "codex".into(), ..Default::default() }));
        assert!(!uses_native_move(&codex_s, &codex2, &Ask { account: "codex2".into(), ..Default::default() }));

        let hosted = Session { hosted: true, ..claude_s.clone() };
        let bg = Session { attach_command: Some("claude attach abc".into()), ..claude_s.clone() };
        let home = Path::new("/tmp");
        assert_eq!(native_place(&hosted, &claude2, None, home).unwrap(), Place::Codealong);
        assert_eq!(native_place(&bg, &claude2, None, home).unwrap(), Place::Background);
        assert_eq!(native_place(&claude_s, &claude2, None, home).unwrap(), Place::Terminal);
        assert_eq!(native_place(&claude_s, &claude2, Some(Place::Codealong), home).unwrap(), Place::Codealong);
    }

    #[test]
    fn transcript_file_is_written_and_stale_files_are_pruned() {
        let dir = crate::fixtures::scratch("handoff-files");
        let stale = dir.join("2020-01-01-old.md");
        std::fs::write(&stale, "old").unwrap();
        let old_time = std::time::SystemTime::now() - std::time::Duration::from_secs(60 * 60 * 24 * 45);
        std::fs::File::options().write(true).open(&stale).unwrap().set_modified(old_time).unwrap();

        let fresh = handoff_file_path(&dir, "Fix login & tests!", "abc12345-ffff");
        write_transcript_file(&fresh, "# Full transcript\n").unwrap();
        assert!(fresh.is_file());
        assert!(!stale.exists(), "files older than 30 days are pruned");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

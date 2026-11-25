//! Usage-limit gating for agent starts and sends.
//!
//! An account is "exhausted" for a model when a local session on it was refused at a usage limit
//! (`Session::limit`) whose reset time is still in the future (`resets_at > now`), the model
//! spends that limit (`start::spends`), and no turn spending that limit has succeeded on the
//! account since.
//!
//! Refusals are used directly rather than `Usage` meters or `needs_sign_in`:
//! - `needs_sign_in` trips on idle Claude accounts whose OAuth access token expired, which the
//!   next Claude start refreshes; gating on it would block the fix.
//! - Meters at 100% don't know whether Claude extra usage is funded; `SessionLimit` fires only
//!   when the harness actually rejected a turn (including overage rejection).
//! - Refusals with no `resets_at` (monthly spend limits) aren't hard-gated because they stay on
//!   the board until a turn succeeds, so blocking agents would keep the account gated until a
//!   human starts a session.

use crate::config::{self, AccountCfg, Config};
use crate::model::{Board, Meter, Session, Status};
use crate::start::{self, StartSpec};

/// How old a refusal may be and still count, matching `start::refused_accounts`.
const REFUSED_FOR: i64 = 31 * 24 * 3_600_000;

/// An active usage-limit refusal that blocks new agent turns on an account for a model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Out {
    /// The limit's meter key (`session`, `week`, `model:Opus`), when the harness named it.
    pub meter: Option<String>,
    /// When the limit lifts (epoch ms). Always `> now`.
    pub resets_at: i64,
    /// The account's extra usage was refused too.
    pub spend_capped: bool,
}

/// The model family a limit key (`SessionLimit::meter`) applies to: `Some("Opus")` for
/// `"model:Opus"`, `None` for `"session"`, `"week"`, or unnamed limits (which apply to every
/// model on the account).
pub fn limit_model(meter: Option<&str>) -> Option<&str> {
    meter.and_then(|m| m.strip_prefix("model:")).filter(|m| !m.is_empty())
}

/// Human-readable noun for a limit's meter key: `"5-hour limit"`, `"weekly limit"`,
/// `"Opus weekly limit"`, or `"usage limit"`.
pub fn limit_noun(meter: Option<&str>) -> String {
    match meter {
        Some("session") => "5-hour limit".to_string(),
        Some("week") => "weekly limit".to_string(),
        Some(m) => match m.strip_prefix("model:").filter(|x| !x.is_empty()) {
            Some(name) => format!("{name} weekly limit"),
            None => "usage limit".to_string(),
        },
        None => "usage limit".to_string(),
    }
}

/// Relative time until a reset (`ms > 0`): `"under a minute"`, `"45m"`, `"1h 15m"`, `"2h"`, `"3d"`.
pub fn in_words(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    match s {
        0..=59 => "under a minute".into(),
        60..=3599 => format!("{}m", s / 60),
        3600..=86_399 => {
            let h = s / 3600;
            let m = (s % 3600) / 60;
            if m > 0 { format!("{h}h {m}m") } else { format!("{h}h") }
        }
        _ => format!("{}d", s / 86_400),
    }
}

/// Whether `account` is currently exhausted for `model` (its own default model when `None`),
/// based on active local `SessionLimit` refusals with a future `resets_at`.
pub fn check(board: &Board, cfg: &Config, account: &str, model: Option<&str>, now: i64) -> Option<Out> {
    let default = cfg.accounts.iter().find(|a| a.name == account).and_then(|a| crate::harness::of(a)?.default_model(a));
    let effective = model.map(str::trim).filter(|m| !m.is_empty()).or(default.as_deref());
    check_with_model(board, account, effective, now)
}

/// `check` given the already-resolved `model` (no config lookup needed, for pure tests and
/// checking candidate models).
pub fn check_with_model(board: &Board, account: &str, model: Option<&str>, now: i64) -> Option<Out> {
    let meters: &[Meter] = board.usage.iter().find(|u| u.account == account).map(|u| u.meters.as_slice()).unwrap_or(&[]);
    let sessions: Vec<&Session> = board.lanes.iter().filter(|l| l.device.is_none()).flat_map(|l| &l.sessions).filter(|s| s.account == account).collect();
    let mut best: Option<(i64, Out)> = None;
    for s in &sessions {
        let Some(lim) = &s.limit else { continue };
        let Some(resets_at) = lim.resets_at.filter(|&r| r > now) else { continue };
        if now - lim.at >= REFUSED_FOR {
            continue;
        }
        let lm = limit_model(lim.meter.as_deref());
        if !start::spends(model, lm, meters) {
            continue;
        }
        // A later non-working turn without a limit that spends the same limit proves the refusal
        // is over. For a per-model limit, a session whose model isn't known doesn't clear it.
        let cleared = sessions.iter().any(|other| {
            other.limit.is_none()
                && other.status != Status::Working
                && other.last_activity > lim.at
                && (lm.is_none() || (other.model.is_some() && start::spends(other.model.as_deref(), lm, meters)))
        });
        if cleared {
            continue;
        }
        let out = Out { meter: lim.meter.clone(), resets_at, spend_capped: lim.spend_capped };
        let replace = match &best {
            None => true,
            Some((prev_at, prev)) => resets_at > prev.resets_at || (resets_at == prev.resets_at && lim.at > *prev_at),
        };
        if replace {
            best = Some((lim.at, out));
        }
    }
    best.map(|(_, out)| out)
}

/// Error message when an agent `codealong start` is refused because `spec.account` is exhausted.
/// Names the limit and reset time, offers a model on the same account when only a per-model limit
/// is hit, and offers other accounts with room without ever crossing a boundary group.
pub fn start_error(board: &Board, cfg: &Config, spec: &StartSpec, out: &Out, now: i64) -> String {
    let account = &spec.account;
    let noun = limit_noun(out.meter.as_deref());
    let when = in_words(out.resets_at - now);
    let mut parts = vec![format!("{account} is at its {noun} (resets in {when}).")];
    if let Some(alt_model) = alternate_model(board, cfg, account, out, now) {
        parts.push(format!("`--model {alt_model}` has room on {account}."));
    }
    let alts = alternate_accounts(board, cfg, spec, now);
    if !alts.is_empty() {
        let word = if alts.len() == 1 { "has" } else { "have" };
        parts.push(format!("{} {word} room.", alts.join(", ")));
    }
    parts.push(format!("Otherwise tell your user when it resets; don't start or wake {account} sessions before then."));
    parts.join(" ")
}

/// Error message when an agent `codealong session send` is refused for a stopped or hosted session
/// whose account is exhausted. Deliberately does not suggest `codealong start`.
pub fn send_error(session: &Session, out: &Out, now: i64) -> String {
    let noun = limit_noun(out.meter.as_deref());
    let when = in_words(out.resets_at - now);
    format!(
        "“{}” is on {}, which is at its {noun} (resets in {when}), so nothing was sent. Tell your user when it resets rather than starting or waking {} sessions before then.",
        session.title.trim(),
        session.account,
        session.account
    )
}

/// Short note appended to a delivered agent send to a live session whose account is exhausted.
pub fn live_send_note(account: &str, out: &Out, now: i64) -> String {
    format!("Note: {account} is at its {} (resets in {}).", limit_noun(out.meter.as_deref()), in_words(out.resets_at - now))
}

/// Another model on `account` that does not spend the exhausted per-model limit and has room.
/// Prefers general fallback models (`opus`, `sonnet`, matching `start::auto_model`) over preview
/// models when the account offers them.
fn alternate_model(board: &Board, cfg: &Config, account: &str, out: &Out, now: i64) -> Option<String> {
    let lm = limit_model(out.meter.as_deref())?;
    let acct = cfg.accounts.iter().find(|a| a.name == account)?;
    let adapter = crate::harness::of(acct)?;
    if !adapter.takes_model() {
        return None;
    }
    let u = board.usage.iter().find(|u| u.account == account);
    let meters = u.map(|u| u.meters.as_slice()).unwrap_or(&[]);
    let ok = |id: &str| {
        !start::spends(Some(id), Some(lm), meters)
            && check_with_model(board, account, Some(id), now).is_none()
            && !u.is_some_and(|u| start::near_limit(u, Some(id), acct.usage_cap))
    };
    let offered = adapter.models(acct);
    for preferred in ["opus", "sonnet"] {
        if offered.iter().any(|m| m.id == preferred) && ok(preferred) {
            return Some(preferred.to_string());
        }
    }
    offered.into_iter().find(|m| ok(&m.id)).map(|m| m.id)
}

/// Other accounts with room that are safe to suggest for `spec` without crossing a boundary group.
fn alternate_accounts(board: &Board, cfg: &Config, spec: &StartSpec, now: i64) -> Vec<String> {
    let qualifies = |a: &AccountCfg| match board.sign_ins.iter().find(|s| s.account == a.name) {
        Some(s) => s.qualifies(),
        None => crate::signin::current(a, now, crate::signin::Ask::Never).qualifies(),
    };
    let caller_acct = spec.from.as_deref().and_then(|k| board.session(k).map(|(_, s)| s.account.as_str()).or_else(|| crate::boundary::account_of_key(k)));
    let caller_boundary = caller_acct.and_then(|a| crate::boundary::group_of(cfg, a));
    let target_boundary = crate::boundary::group_of(cfg, &spec.account);
    let project = start::project_at(board, &spec.lane_id);
    let auto_name = config::auto_group(cfg, project);
    let default_group = (!auto_name.is_empty()).then(|| cfg.routing.groups.iter().find(|g| g.name == auto_name)).flatten();

    // Respect the tightest boundary: caller's boundary, target account's boundary, or project's
    // boundary default. Never suggest an account outside it.
    let boundary = caller_boundary.or(target_boundary).or_else(|| default_group.filter(|g| g.boundary));

    let has_room = |a: &AccountCfg| -> bool {
        if a.name == spec.account || a.disabled {
            return false;
        }
        let model = spec.model.as_deref().map(str::trim).filter(|m| !m.is_empty()).map(String::from).or_else(|| crate::harness::of(a).and_then(|h| h.default_model(a)));
        if let Some(m) = spec.model.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
            if !crate::harness::of(a).is_some_and(|h| h.offers_model(a, m)) {
                return false;
            }
        }
        if check_with_model(board, &a.name, model.as_deref(), now).is_some() {
            return false;
        }
        !board.usage.iter().any(|u| u.account == a.name && (u.needs_sign_in || start::near_limit(u, model.as_deref(), a.usage_cap)))
    };

    if let Some(bg) = boundary {
        return bg
            .members
            .iter()
            .filter_map(|m| cfg.accounts.iter().find(|a| a.name == m.account.trim()))
            .filter(|a| qualifies(a) && has_room(a))
            .map(|a| a.name.clone())
            .collect();
    }

    // Outside a boundary, prefer other members of the project's default group when it names any.
    if let Some(dg) = default_group.filter(|g| !g.members.is_empty()) {
        let in_default: Vec<String> = dg
            .members
            .iter()
            .filter_map(|m| cfg.accounts.iter().find(|a| a.name == m.account.trim()))
            .filter(|a| crate::boundary::group_of(cfg, &a.name).is_none() && has_room(a))
            .map(|a| a.name.clone())
            .collect();
        if !in_default.is_empty() {
            return in_default;
        }
    }

    cfg.accounts.iter().filter(|a| crate::boundary::group_of(cfg, &a.name).is_none() && has_room(a)).map(|a| a.name.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GroupMemberCfg, ProjectCfg, RoutingGroupCfg, RoutingStrategy};
    use crate::model::{SessionLimit, Severity, Usage};
    use crate::start::Place;

    const NOW: i64 = 1_800_000_000_000;
    const MIN: i64 = 60_000;
    const HOUR: i64 = 3_600_000;

    fn acct(name: &str) -> AccountCfg {
        AccountCfg { name: name.into(), harness: "claude".into(), home: format!("/tmp/.{name}"), command: Some("claude".into()), start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None }
    }

    fn limited(key: &str, meter: Option<&str>, at: i64, resets_at: Option<i64>, model: Option<&str>) -> Session {
        let account = key.split(':').nth(1).unwrap_or("claude").to_string();
        let limit = SessionLimit { meter: meter.map(String::from), at, resets_at, model: model.map(String::from), ..Default::default() };
        Session { account, status: Status::Error, last_activity: at, model: model.map(String::from), limit: Some(limit), ..crate::fixtures::session(key) }
    }

    fn ok_session(key: &str, status: Status, at: i64, model: Option<&str>) -> Session {
        let account = key.split(':').nth(1).unwrap_or("claude").to_string();
        Session { account, status, last_activity: at, model: model.map(String::from), ..crate::fixtures::session(key) }
    }

    fn board_with(sessions: Vec<Session>, usage: Vec<Usage>) -> Board {
        Board { lanes: vec![crate::fixtures::lane("/work", sessions)], usage, ..Board::default() }
    }

    #[test]
    fn active_session_limit_blocks_until_reset_and_clears_after_success_or_expiry() {
        let b = board_with(vec![limited("claude:claude-work:s1", Some("session"), NOW - 5 * MIN, Some(NOW + 75 * MIN), Some("claude-opus-4-6"))], vec![]);
        let out = check_with_model(&b, "claude-work", Some("sonnet"), NOW).unwrap();
        assert_eq!(out, Out { meter: Some("session".into()), resets_at: NOW + 75 * MIN, spend_capped: false });
        assert_eq!(in_words(out.resets_at - NOW), "1h 15m");
        assert_eq!(limit_noun(out.meter.as_deref()), "5-hour limit");

        // Once resets_at passes, it's no longer exhausted.
        assert_eq!(check_with_model(&b, "claude-work", Some("sonnet"), NOW + 75 * MIN), None);

        // A working session doesn't clear it (hasn't succeeded yet).
        let working = board_with(
            vec![
                limited("claude:claude-work:s1", Some("session"), NOW - 5 * MIN, Some(NOW + HOUR), Some("opus")),
                ok_session("claude:claude-work:s2", Status::Working, NOW - MIN, Some("opus")),
            ],
            vec![],
        );
        assert!(check_with_model(&working, "claude-work", Some("opus"), NOW).is_some());

        // A later completed turn clears it.
        let cleared = board_with(
            vec![
                limited("claude:claude-work:s1", Some("session"), NOW - 5 * MIN, Some(NOW + HOUR), Some("opus")),
                ok_session("claude:claude-work:s2", Status::Idle, NOW - MIN, Some("sonnet")),
            ],
            vec![],
        );
        assert_eq!(check_with_model(&cleared, "claude-work", Some("opus"), NOW), None);
    }

    #[test]
    fn refusal_without_reset_time_does_not_hard_gate() {
        let b = board_with(vec![limited("claude:claude-work:s1", Some("session"), NOW - 5 * MIN, None, Some("opus"))], vec![]);
        assert_eq!(check_with_model(&b, "claude-work", Some("opus"), NOW), None);
    }

    #[test]
    fn per_model_refusal_only_blocks_that_family_and_requires_matching_turn_to_clear() {
        let b = board_with(
            vec![
                limited("claude:claude-work:s1", Some("model:Opus"), NOW - 10 * MIN, Some(NOW + 2 * HOUR), Some("claude-opus-4-6")),
                // A Sonnet turn succeeding after an Opus refusal does NOT clear the Opus limit.
                ok_session("claude:claude-work:s2", Status::Idle, NOW - 2 * MIN, Some("claude-sonnet-4-6")),
                // An unknown-model turn does NOT clear a per-model limit either.
                ok_session("claude:claude-work:s3", Status::Idle, NOW - MIN, None),
            ],
            vec![],
        );
        assert!(check_with_model(&b, "claude-work", Some("opus"), NOW).is_some());
        assert!(check_with_model(&b, "claude-work", None, NOW).is_some(), "unknown model could spend Opus");
        assert_eq!(check_with_model(&b, "claude-work", Some("sonnet"), NOW), None, "Sonnet doesn't spend Opus limit");

        // An Opus turn succeeding DOES clear it.
        let mut cleared = b;
        cleared.lanes[0].sessions.push(ok_session("claude:claude-work:s4", Status::Idle, NOW - 10_000, Some("claude-opus-4-6")));
        assert_eq!(check_with_model(&cleared, "claude-work", Some("opus"), NOW), None);
    }

    #[test]
    fn start_error_respects_boundaries_and_suggests_alternate_model_or_roomy_accounts() {
        let mut cfg = Config::default();
        cfg.accounts = vec![acct("claude"), acct("claude2"), acct("claude-work")];
        cfg.routing.groups = vec![RoutingGroupCfg {
            name: "authorized".into(),
            strategy: RoutingStrategy::Usage,
            members: vec![GroupMemberCfg { account: "claude-work".into(), model: String::new(), effort: String::new() }],
            boundary: true,
            decisions: None,
        }];
        cfg.projects = vec![ProjectCfg { name: "Acme".into(), repos: vec![], folders: vec![], default: Some("authorized".into()), chrome_profile: None, decisions: None, background_prompt: None }];

        // 1-member boundary group on 5h limit: must NOT suggest personal accounts `claude` or `claude2`.
        let mut lane = crate::fixtures::lane("/work", vec![limited("claude:claude-work:s1", Some("session"), NOW - 5 * MIN, Some(NOW + HOUR), Some("opus"))]);
        lane.project = "Acme".into();
        let b = Board { lanes: vec![lane], ..Board::default() };
        let spec = StartSpec { lane_id: "/work".into(), account: "claude-work".into(), place: Place::Background, prompt: "do work".into(), model: None, effort: None, worktree: false, todos: vec![], from: Some("claude:claude2:coord".into()), helper: true, report: true };
        let out = check_with_model(&b, "claude-work", None, NOW).unwrap();
        let err = start_error(&b, &cfg, &spec, &out, NOW);
        assert!(err.contains("claude-work is at its 5-hour limit (resets in 1h)"), "{err}");
        assert!(!err.contains("claude2") && !err.contains("claude has") && !err.contains("claude,"), "must not suggest accounts outside boundary: {err}");
        assert!(err.contains("don't start or wake claude-work sessions before then"), "{err}");

        // Per-model Opus limit inside the same 1-member boundary group: DOES suggest `--model sonnet` on claude-work.
        let mut opus_lane = crate::fixtures::lane("/work", vec![limited("claude:claude-work:s1", Some("model:Opus"), NOW - 5 * MIN, Some(NOW + 2 * HOUR), Some("opus"))]);
        opus_lane.project = "Acme".into();
        let opus_board = Board { lanes: vec![opus_lane], ..Board::default() };
        let out = check_with_model(&opus_board, "claude-work", Some("opus"), NOW).unwrap();
        let err = start_error(&opus_board, &cfg, &spec, &out, NOW);
        assert!(err.contains("`--model sonnet` has room on claude-work"), "{err}");
        assert!(!err.contains("claude2"), "{err}");

        // Non-boundary account exhausted: DOES suggest roomy non-boundary account, and skips boundary & near-limit accounts.
        let usage_of = |account: &str, meters: Vec<Meter>| Usage { account: account.into(), harness: crate::model::Harness::Claude, plan: None, meters, as_of: Some(NOW), error: None, needs_sign_in: false, loading: false, no_limits: false, preferred: false, rate_limited: false };
        let usages = vec![
            usage_of("claude", vec![Meter { key: "session".into(), label: "5h".into(), window: "5h".into(), percent: 100.0, resets_at: Some(NOW + HOUR), severity: Severity::Critical, model: None }]),
            usage_of("claude2", vec![Meter { key: "week".into(), label: "Week".into(), window: "Week".into(), percent: 10.0, resets_at: Some(NOW + 24 * HOUR), severity: Severity::Normal, model: None }]),
        ];
        let b2 = board_with(vec![limited("claude:claude:s1", Some("session"), NOW - 5 * MIN, Some(NOW + 30 * MIN), Some("opus"))], usages);
        let spec2 = StartSpec { account: "claude".into(), ..spec };
        let out2 = check_with_model(&b2, "claude", Some("opus"), NOW).unwrap();
        let err2 = start_error(&b2, &cfg, &spec2, &out2, NOW);
        assert!(err2.contains("claude2 has room"), "{err2}");
        assert!(!err2.contains("claude-work"), "must not suggest boundary account to non-boundary start: {err2}");
    }

    #[test]
    fn send_error_does_not_suggest_starting_a_new_session() {
        let s = limited("claude:claude-work:s1", Some("session"), NOW - 5 * MIN, Some(NOW + 45 * MIN), Some("opus"));
        let out = Out { meter: Some("session".into()), resets_at: NOW + 45 * MIN, spend_capped: false };
        let err = send_error(&s, &out, NOW);
        assert!(err.contains("claude-work") && err.contains("5-hour limit") && err.contains("resets in 45m"), "{err}");
        assert!(!err.contains("codealong start"), "{err}");
    }
}

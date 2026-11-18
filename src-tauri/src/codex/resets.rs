//! A Codex account's limit resets ("reset credits"), read and used the way Codex's terminal UI
//! does it, through an app-server (0.169.0's schema, `codex app-server generate-json-schema`):
//!
//! - `account/rateLimits/read` answers `rateLimitResetCredits: {availableCount, credits}`, each
//!   credit `{id, title, description, grantedAt, expiresAt, resetType, status}` (times in
//!   seconds; `status` `available`, `redeeming`, `redeemed`), `credits` null when only the count
//!   is known and possibly shorter than it. The same answer has the account's limits now,
//!   `rateLimitsByLimitId`, as the rollouts' `token_count` reports them (`limitId` `codex`,
//!   `primary` and `secondary` windows).
//! - `account/rateLimitResetCredit/consume {idempotencyKey, creditId?}` uses one (the backend
//!   picks when no id is given) and answers `outcome`: `reset`, `nothingToReset` (no window is
//!   far enough along, and nothing is spent), `noCredit` or `alreadyRedeemed` (the same key went
//!   through before). The key names one attempt, so a retry sends the same one.
//!
//! An account here held three "Full reset" credits ("Thanks for using Codex! You've been granted
//! one free rate limit reset."), each good for 30 days (2026-11-18). The CLI inside the ChatGPT
//! app (0.169.0-alpha.11.1) has neither call, where the Homebrew cask's 0.169.0 has both, so these
//! go to the first CLI that answers with the credits: the `codex` on your PATH, then the app's.
//! None was used to check this: using one spends it.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use super::appserver;
use crate::rpc::Failure;
use super::rollout::{RateLimit, RateWindow};
use crate::config::AccountCfg;
use crate::resets::{Outcome, ResetCredit, Resets};
use crate::util::now_ms;

const READ: &str = "account/rateLimits/read";
const CONSUME: &str = "account/rateLimitResetCredit/consume";
const TOO_OLD: &str = "Your Codex doesn't offer limit resets: Codex 0.169 or later does.";

/// The account's limits as its latest read here answered them, by its folder, timed when the read
/// was asked: a report as fresh as the read, which codex/usage.rs weighs with the rollouts' own,
/// so the bar shows a reset as soon as it's used rather than after the account's next turn. A read
/// asked earlier that answers later (the hourly one, out as a reset is used) doesn't replace it.
static REPORTS: Mutex<BTreeMap<PathBuf, BTreeMap<String, RateLimit>>> = Mutex::new(BTreeMap::new());

/// A use that went unanswered, by the account's folder: its key and the credit it named. The next
/// use of the same credit sends that key again, so if the first went through the backend answers
/// `alreadyRedeemed` rather than spending another (which, with no credit named, it could).
static UNANSWERED: Mutex<BTreeMap<PathBuf, (String, Option<String>)>> = Mutex::new(BTreeMap::new());

/// The limits the account's last read here answered, for its usage.
pub fn report(account: &AccountCfg) -> Option<BTreeMap<String, RateLimit>> {
    REPORTS.lock().ok()?.get(&account.home_path()).cloned()
}

/// The CLIs to ask, in order: the one on your PATH, then the app's.
fn clis() -> Vec<PathBuf> {
    let path = crate::detect::on_user_path("codex").then(|| PathBuf::from("codex"));
    path.into_iter().chain(crate::detect::codex_app_cli()).collect()
}

/// Reads what the account holds. A CLI that answers without the credits doesn't know them, and
/// the next is asked.
pub fn read(account: &AccountCfg) -> Resets {
    let home = account.home_path();
    let mut why = None;
    for cli in clis() {
        let asked = now_ms();
        match appserver::with_cli(&cli, &home, |s| s.request(READ, json!({}))) {
            Ok(answer) => {
                note_report(&home, limits_of(&answer, asked));
                if let Some((credits, available)) = credits_of(&answer) {
                    return Resets::held(account, credits, available, now_ms());
                }
            }
            Err(e) => why = Some(e.to_string()),
        }
    }
    match why {
        Some(why) => Resets::failed(account, format!("Couldn't read Codex's limit resets: {why}")),
        None => Resets::none(account, TOO_OLD),
    }
}

/// Uses one of the account's resets through the first CLI that knows them, asking it for the
/// credits first. Every attempt sends the same key, and so does the next use of the same credit
/// after one that wasn't answered (`UNANSWERED`), so one that went through isn't spent twice.
pub fn use_one(account: &AccountCfg, credit: Option<&str>) -> Result<Outcome> {
    let home = account.home_path();
    let key = key_for(&home, credit);
    let used = use_with(&home, credit, &key);
    settle(&home, key, credit, &used);
    match used {
        Used::Answered(outcome) => Ok(outcome),
        Used::Unanswered(e) => Err(anyhow!("Codex didn't say whether the reset went through: {e}")),
        Used::Failed(e) => Err(e),
    }
}

/// The key a use sends: the unanswered one's for the same credit, else a new one.
fn key_for(home: &Path, credit: Option<&str>) -> String {
    let kept = UNANSWERED.lock().ok().and_then(|u| u.get(home).filter(|(_, c)| c.as_deref() == credit).map(|(k, _)| k.clone()));
    kept.unwrap_or_else(crate::start::new_session_id)
}

/// Notes how a use went for the next one's key: one unanswered keeps its key, one answered lets
/// it go, and one that never reached Codex's backend leaves what was kept as it was.
fn settle(home: &Path, key: String, credit: Option<&str>, used: &Used) {
    if let Ok(mut unanswered) = UNANSWERED.lock() {
        match used {
            Used::Unanswered(_) => unanswered.insert(home.to_path_buf(), (key, credit.map(str::to_string))),
            Used::Answered(_) => unanswered.remove(home),
            Used::Failed(_) => None,
        };
    }
}

/// How a use went.
enum Used {
    /// Codex answered it.
    Answered(Outcome),
    /// It was sent and no answer came (or one Codealong can't read): it may have gone through.
    Unanswered(anyhow::Error),
    /// Nothing was spent: it never got as far as the use, or Codex turned it down.
    Failed(anyhow::Error),
}

fn use_with(home: &Path, credit: Option<&str>, key: &str) -> Used {
    let (mut unanswered, mut failed) = (None, None);
    for cli in clis() {
        let sent = Cell::new(false);
        let tried = appserver::with_cli(&cli, home, |s| {
            if credits_of(&s.request(READ, json!({}))?).is_none() {
                return Ok(None);
            }
            sent.set(true);
            Ok(Some(match s.answer(CONSUME, json!({ "idempotencyKey": key, "creditId": credit })) {
                Ok(answer) => outcome_of(&answer).map_err(|e| (true, e)),
                // A refusal spent nothing; silence may have.
                Err(e) => Err((!matches!(e, Failure::Refused(_)), appserver::failure(e))),
            }))
        });
        match tried {
            Ok(Some(Ok(outcome))) => return Used::Answered(outcome),
            Ok(Some(Err((false, refused)))) => return Used::Failed(refused),
            // The next CLI is asked with the same key, which the backend takes as the same use.
            Ok(Some(Err((true, e)))) => unanswered = Some(e),
            Ok(None) => {}
            Err(e) if sent.get() => unanswered = Some(e),
            Err(e) => failed = Some(e),
        }
    }
    match (unanswered, failed) {
        (Some(e), _) => Used::Unanswered(e),
        (None, Some(e)) => Used::Failed(e),
        (None, None) => Used::Failed(anyhow!(TOO_OLD)),
    }
}

/// The credits an answer lists that can still be used, and how many it says there are; none when
/// it says nothing of them, as a Codex from before them answers.
fn credits_of(answer: &Value) -> Option<(Vec<ResetCredit>, u32)> {
    let summary = answer.get("rateLimitResetCredits")?;
    let available = summary.get("availableCount").and_then(Value::as_u64).unwrap_or(0) as u32;
    let rows = summary.get("credits").and_then(Value::as_array).into_iter().flatten();
    let seconds = |c: &Value, k: &str| c.get(k).and_then(Value::as_i64).map(|s| s * 1000);
    let text = |c: &Value, k: &str| c.get(k).and_then(Value::as_str).filter(|t| !t.is_empty()).map(str::to_string);
    let credits = rows
        .filter(|c| c.get("status").and_then(Value::as_str).is_none_or(|s| s == "available"))
        .filter_map(|c| {
            Some(ResetCredit {
                id: text(c, "id")?,
                title: text(c, "title"),
                description: text(c, "description"),
                granted_at: seconds(c, "grantedAt"),
                expires_at: seconds(c, "expiresAt"),
            })
        })
        .collect();
    Some((credits, available))
}

fn outcome_of(answer: &Value) -> Result<Outcome> {
    Ok(match answer.get("outcome").and_then(Value::as_str) {
        Some("reset") => Outcome::Reset,
        Some("nothingToReset") => Outcome::NothingToReset,
        Some("noCredit") => Outcome::NoCredit,
        Some("alreadyRedeemed") => Outcome::AlreadyRedeemed,
        other => bail!("Codex answered a reset with {}.", other.unwrap_or("nothing it said")),
    })
}

fn note_report(home: &Path, limits: BTreeMap<String, RateLimit>) {
    if let Ok(mut reports) = REPORTS.lock() {
        newer_report(&mut reports, home, limits);
    }
}

/// Keeps `limits` for the folder unless what's kept was asked for later.
fn newer_report(reports: &mut BTreeMap<PathBuf, BTreeMap<String, RateLimit>>, home: &Path, limits: BTreeMap<String, RateLimit>) {
    let asked = |l: &BTreeMap<String, RateLimit>| l.values().map(|r| r.at).max();
    if limits.is_empty() || reports.get(home).and_then(asked) > asked(&limits) {
        return;
    }
    reports.insert(home.to_path_buf(), limits);
}

/// The answer's limits as the rollouts report them (rollout.rs `parse_rate_limit`), by limit id.
fn limits_of(answer: &Value, at: i64) -> BTreeMap<String, RateLimit> {
    let by_id = answer.get("rateLimitsByLimitId").and_then(Value::as_object);
    let all: Vec<&Value> = match by_id {
        Some(map) => map.values().collect(),
        None => answer.get("rateLimits").into_iter().collect(),
    };
    all.into_iter()
        .filter_map(|v| {
            let windows: Vec<RateWindow> = ["primary", "secondary"]
                .iter()
                .filter_map(|k| v.get(*k).filter(|w| w.is_object()))
                .map(|w| RateWindow {
                    used_percent: w.get("usedPercent").and_then(Value::as_f64).unwrap_or(0.0),
                    minutes: w.get("windowDurationMins").and_then(Value::as_i64).unwrap_or(0),
                    resets_at: w.get("resetsAt").and_then(Value::as_i64).map(|s| s * 1000),
                })
                .collect();
            if windows.is_empty() {
                return None;
            }
            let id = v.get("limitId").and_then(Value::as_str).unwrap_or("codex").to_string();
            let name = v.get("limitName").and_then(Value::as_str).map(str::to_string);
            let plan = v.get("planType").and_then(Value::as_str).map(str::to_string);
            Some((id, RateLimit { name, plan, windows, at }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As the app-server of 0.169.0 answered an account here (2026-11-18), one credit shown.
    const READ_ANSWER: &str = r#"{"accountId":null,"ordinaryUsageAllowed":true,"rateLimitUpsell":null,
      "rateLimits":{"limitId":"codex","limitName":null,"primary":{"usedPercent":6,"windowDurationMins":10080,"resetsAt":1795620622},"secondary":null,"planType":"pro"},
      "rateLimitsByLimitId":{"codex":{"limitId":"codex","limitName":null,"normalModelSlug":null,"primary":{"usedPercent":6,"windowDurationMins":10080,"resetsAt":1795620622},"secondary":null,
        "credits":{"hasCredits":true,"unlimited":false,"balance":"62500.0000000000"},"individualLimit":null,"spendControlReached":false,"planType":"pro","rateLimitReachedType":null}},
      "rateLimitResetCredits":{"availableCount":3,"credits":[
        {"id":"RateLimitResetCredit_aa","resetType":"codexRateLimits","status":"available","grantedAt":1793477434,"expiresAt":1796069434,"title":"Full reset","description":"Thanks for using Codex! You've been granted one free rate limit reset."},
        {"id":"RateLimitResetCredit_bb","resetType":"codexRateLimits","status":"redeemed","grantedAt":1793477434,"expiresAt":1796069434,"title":"Full reset","description":null}]}}"#;

    #[test]
    fn reads_the_credits_that_can_still_be_used() {
        let answer: Value = serde_json::from_str(READ_ANSWER).unwrap();
        let (credits, available) = credits_of(&answer).unwrap();
        assert_eq!(available, 3, "more than the rows it listed");
        assert_eq!(credits.len(), 1, "a redeemed one isn't listed");
        assert_eq!(credits[0].id, "RateLimitResetCredit_aa");
        assert_eq!(credits[0].title.as_deref(), Some("Full reset"));
        assert_eq!(credits[0].expires_at, Some(1_796_069_434_000));
    }

    #[test]
    fn a_codex_without_them_says_nothing_and_none_held_is_zero() {
        let old: Value = serde_json::from_str(r#"{"rateLimits":{"limitId":"codex","primary":null}}"#).unwrap();
        assert_eq!(credits_of(&old), None);
        let none: Value = serde_json::from_str(r#"{"rateLimitResetCredits":{"availableCount":0,"credits":null}}"#).unwrap();
        assert_eq!(credits_of(&none), Some((Vec::new(), 0)));
    }

    #[test]
    fn reads_every_outcome_and_refuses_others() {
        let o = |s: &str| outcome_of(&json!({ "outcome": s }));
        assert_eq!(o("reset").unwrap(), Outcome::Reset);
        assert_eq!(o("nothingToReset").unwrap(), Outcome::NothingToReset);
        assert_eq!(o("noCredit").unwrap(), Outcome::NoCredit);
        assert_eq!(o("alreadyRedeemed").unwrap(), Outcome::AlreadyRedeemed);
        assert!(o("later").is_err());
    }

    /// A use that went unanswered may have spent one: the next use of the same credit sends its
    /// key again, so the backend answers `alreadyRedeemed` rather than spending another, until a
    /// use is answered. One that never reached the backend changes nothing.
    #[test]
    fn a_use_left_unanswered_is_retried_under_its_key() {
        let home = PathBuf::from("/Users/me/.codex-unanswered-test");
        let first = key_for(&home, None);
        settle(&home, first.clone(), None, &Used::Unanswered(anyhow!("Codex didn't answer in time.")));
        assert_eq!(key_for(&home, None), first, "the same use again");
        assert_ne!(key_for(&home, Some("RateLimitResetCredit_b")), first, "another credit is another use");
        settle(&home, "never-sent".into(), None, &Used::Failed(anyhow!("Couldn't start Codex.")));
        assert_eq!(key_for(&home, None), first, "nothing reached the backend");
        settle(&home, first.clone(), None, &Used::Answered(Outcome::AlreadyRedeemed));
        assert_ne!(key_for(&home, None), first, "answered: the next use is a new one");
    }

    /// The hourly read, asked before a reset was used and answered after, doesn't put back the
    /// limits from before it.
    #[test]
    fn a_read_asked_earlier_doesnt_replace_a_later_ones_limits() {
        let window = |used_percent| RateWindow { used_percent, minutes: 10080, resets_at: None };
        let report = |at, percent| BTreeMap::from([("codex".to_string(), RateLimit { name: None, plan: None, windows: vec![window(percent)], at })]);
        let home = Path::new("/Users/me/.codex");
        let mut reports = BTreeMap::new();
        newer_report(&mut reports, home, report(200, 0.0));
        newer_report(&mut reports, home, report(100, 95.0));
        assert_eq!(reports[home]["codex"].windows[0].used_percent, 0.0);
        newer_report(&mut reports, home, report(300, 4.0));
        assert_eq!(reports[home]["codex"].windows[0].used_percent, 4.0);
        newer_report(&mut reports, home, BTreeMap::new());
        assert_eq!(reports[home]["codex"].at, 300, "a read with no limits keeps the last");
    }

    /// The read's limits read as the rollouts' report of them does.
    #[test]
    fn the_reads_limits_are_a_report_like_a_turns() {
        let answer: Value = serde_json::from_str(READ_ANSWER).unwrap();
        let limits = limits_of(&answer, 42);
        let codex = &limits["codex"];
        assert_eq!((codex.name.as_deref(), codex.plan.as_deref(), codex.at), (None, Some("pro"), 42));
        assert_eq!(codex.windows, [RateWindow { used_percent: 6.0, minutes: 10080, resets_at: Some(1_795_620_622_000) }]);
        let only_one: Value = serde_json::from_str(r#"{"rateLimits":{"limitId":"codex","primary":{"usedPercent":90,"windowDurationMins":300,"resetsAt":null}}}"#).unwrap();
        assert_eq!(limits_of(&only_one, 1)["codex"].windows[0].minutes, 300);
    }
}

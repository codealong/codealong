//! A Codex account's plan usage: no network and no credentials. Every turn records the
//! account's rate limits in its rollout, and the newest report wins.

use std::collections::BTreeMap;

use super::rollout::RateLimit;
use crate::config::AccountCfg;
use crate::model::{Meter, Usage};
use crate::usage::{loading, severity, title};
use crate::util::DAY_MS;

/// Codex usage from the newest rate-limit reports across the account's threads. A window whose
/// reset time has passed since the report reads as empty. Per-model limits show only when
/// they've been used this week.
pub fn codex<'a>(account: &AccountCfg, reports: impl Iterator<Item = &'a BTreeMap<String, RateLimit>>, now: i64) -> Usage {
    let mut latest: BTreeMap<&str, &RateLimit> = BTreeMap::new();
    for map in reports {
        for (id, r) in map {
            if latest.get(id.as_str()).map(|old| old.at < r.at).unwrap_or(true) {
                latest.insert(id, r);
            }
        }
    }
    let mut u = Usage { loading: false, ..loading(account) };
    let Some(main) = latest.get("codex").copied().or_else(|| latest.values().find(|r| r.name.is_none()).copied()) else {
        u.error = Some("Usage shows after your next Codex turn.".to_string());
        return u;
    };
    u.plan = main.plan.as_deref().map(title);
    u.as_of = Some(main.at);
    let mut windows: Vec<_> = main.windows.iter().collect();
    windows.sort_by_key(|w| w.minutes);
    for w in windows {
        let (percent, resets_at) = current(w.used_percent, w.resets_at, now);
        let (key, label, window) = match w.minutes {
            300 => ("session".to_string(), "5h".to_string(), "5-hour window".to_string()),
            10080 => ("week".to_string(), "Week".to_string(), "Weekly".to_string()),
            m => (format!("window:{m}"), span(m), format!("{} window", span(m))),
        };
        u.meters.push(Meter { key, label, window, percent, resets_at, severity: severity(None, percent), model: None });
    }
    for r in latest.values().filter(|r| r.name.is_some() && now - r.at < 7 * DAY_MS) {
        let name = r.name.clone().unwrap_or_default();
        let fullest = r
            .windows
            .iter()
            .map(|w| (w, current(w.used_percent, w.resets_at, now)))
            .max_by(|a, b| a.1 .0.total_cmp(&b.1 .0));
        let Some((w, (percent, resets_at))) = fullest.filter(|(_, (p, _))| *p > 0.0) else { continue };
        u.meters.push(Meter {
            key: format!("model:{name}"),
            label: name.clone(),
            window: format!("{}, {name} only", if w.minutes == 10080 { "Weekly".to_string() } else { format!("{} window", span(w.minutes)) }),
            percent,
            resets_at,
            severity: severity(None, percent),
            model: Some(name),
        });
    }
    u
}

fn current(percent: f64, resets_at: Option<i64>, now: i64) -> (f64, Option<i64>) {
    match resets_at {
        Some(t) if t <= now => (0.0, None),
        other => (percent, other),
    }
}

fn span(minutes: i64) -> String {
    if minutes % 1440 == 0 {
        format!("{}d", minutes / 1440)
    } else if minutes % 60 == 0 {
        format!("{}h", minutes / 60)
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::rollout::RateWindow;

    fn limit(name: Option<&str>, windows: &[(f64, i64, Option<i64>)], at: i64) -> RateLimit {
        RateLimit {
            name: name.map(str::to_string),
            plan: Some("pro".into()),
            windows: windows.iter().map(|&(used_percent, minutes, resets_at)| RateWindow { used_percent, minutes, resets_at }).collect(),
            at,
        }
    }

    #[test]
    fn codex_takes_the_newest_report_and_applies_resets() {
        let now = 1_000 * DAY_MS;
        let acct = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false };
        let a: BTreeMap<String, RateLimit> = [("codex".to_string(), limit(None, &[(40.0, 10080, Some(now + DAY_MS))], now - 5_000))].into();
        let b: BTreeMap<String, RateLimit> = [
            ("codex".to_string(), limit(None, &[(48.0, 10080, Some(now + DAY_MS))], now - 1_000)),
            ("codex_spark".to_string(), limit(Some("GPT-5.3-Codex-Spark"), &[(30.0, 300, Some(now - 10)), (12.0, 10080, Some(now + DAY_MS))], now - 1_000)),
        ]
        .into();
        let u = codex(&acct, [&a, &b].into_iter(), now);
        assert_eq!(u.plan.as_deref(), Some("Pro"));
        assert_eq!(u.meters.len(), 2);
        assert_eq!((u.meters[0].label.as_str(), u.meters[0].percent), ("Week", 48.0));
        // The Spark 5-hour window reset, so its week is the fullest.
        assert_eq!((u.meters[1].label.as_str(), u.meters[1].percent), ("GPT-5.3-Codex-Spark", 12.0));
    }

    #[test]
    fn codex_without_reports_says_so() {
        let acct = AccountCfg { name: "codex2".into(), harness: "codex".into(), home: "~/.codex2".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false };
        let u = codex(&acct, std::iter::empty(), 0);
        assert!(u.error.is_some() && u.meters.is_empty());
    }
}

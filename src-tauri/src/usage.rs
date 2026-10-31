//! Plan usage per account: what every harness's numbers are put into, and the asking. Where
//! the numbers come from is each harness's own (`Adapter::read_usage` for somewhere slow to
//! ask, `Reader::usage` for what comes with its sessions): Claude Code's sign-in and usage
//! endpoint are claude/usage.rs, Codex's rate-limit reports codex/usage.rs, Antigravity's
//! quotas its command line's `/usage` (antigravity.rs `read_usage`), and Cursor's its servers,
//! asked with the app's sign-in (cursor.rs `read_usage`). OpenCode has nothing to read
//! (`Adapter::no_limits`), as it bills whichever provider you bring; nor has Antigravity without
//! its command line, nor Cursor without a sign-in Codealong reads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::accountpoll::identity;
use crate::config::AccountCfg;
use crate::model::{AccountInfo, Harness, Severity, Usage};
use crate::util::now_ms;

/// How old numbers can be for a launch to show them: older, and the app was closed rather than
/// relaunched, and they could say anything by now.
const KEPT_FOR_MS: i64 = 60 * 60 * 1000;

/// The last numbers read, beside the config, for the launch after this one: a relaunch would
/// otherwise have none to show until its first read, and when the usage endpoint turns that read
/// away (it did both accounts here at once, 2026-10-31) none for ten minutes more.
fn kept_file() -> PathBuf {
    crate::config::support_dir().join("usage.json")
}

/// Kept by what each account is, not by its name: a name config.toml gave to another folder
/// while the app was closed is another account, whose limits these aren't.
pub fn keep(accounts: &[AccountCfg], usage: &HashMap<String, Usage>) {
    keep_in(&kept_file(), accounts, usage);
}

fn keep_in(path: &Path, accounts: &[AccountCfg], usage: &HashMap<String, Usage>) {
    let numbers: HashMap<String, &Usage> = accounts.iter().filter_map(|a| Some((identity(a), usage.get(&a.name).filter(|u| !u.meters.is_empty())?))).collect();
    // All a failure costs is a launch that starts without numbers, as one always did.
    let _ = crate::store::save(path, &numbers);
}

/// The numbers `keep` left for these accounts that are recent enough to show, by account name.
pub fn kept(accounts: &[AccountCfg], now: i64) -> HashMap<String, Usage> {
    kept_in(&kept_file(), accounts, now)
}

fn kept_in(path: &Path, accounts: &[AccountCfg], now: i64) -> HashMap<String, Usage> {
    let mut numbers: HashMap<String, Usage> = crate::store::read_or_default(path);
    let recent = |u: &Usage| u.as_of.is_some_and(|at| now - at < KEPT_FOR_MS);
    accounts.iter().filter_map(|a| Some((a.name.clone(), Usage { account: a.name.clone(), ..numbers.remove(&identity(a)).filter(recent)? }))).collect()
}

pub fn loading(account: &AccountCfg) -> Usage {
    Usage {
        account: account.name.clone(),
        harness: Harness::from_config(&account.harness),
        plan: None,
        meters: Vec::new(),
        as_of: None,
        error: None,
        needs_sign_in: false,
        loading: true,
        no_limits: false,
        preferred: false,
        rate_limited: false,
    }
}

/// An account whose harness reports no limits, with why in words.
pub fn none(account: &AccountCfg, note: &str) -> Usage {
    Usage { loading: false, error: Some(note.to_string()), no_limits: true, ..loading(account) }
}

pub(crate) fn severity(word: Option<&str>, percent: f64) -> Severity {
    match word {
        Some("critical") => Severity::Critical,
        Some("warning") => Severity::Warning,
        Some(_) => Severity::Normal,
        None if percent >= 90.0 => Severity::Critical,
        None if percent >= 75.0 => Severity::Warning,
        None => Severity::Normal,
    }
}

pub(crate) fn title(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Where the account stands, for the Accounts dialog. Reads no secret: the keychain is asked
/// whether it holds a sign-in, and the email comes from the account's own settings file.
pub fn describe(a: &AccountCfg) -> AccountInfo {
    let dir = a.home_path();
    let harness = crate::harness::of(a);
    let (signed_in, email) = harness.map_or((false, None), |h| h.signed_in(&dir));
    let sign_in_note = harness.and_then(|h| h.sign_in_note(&dir));
    let missing_cli = harness.and_then(|h| h.missing_cli(a, &crate::util::home())).map(str::to_string);
    AccountInfo {
        name: a.name.clone(),
        harness: a.harness.clone(),
        home: a.home.clone(),
        command: a.command.clone(),
        start: a.start.clone(),
        acp: a.acp.clone(),
        monthly: a.monthly,
        per_token: a.per_token,
        usage_cap: a.usage_cap,
        disabled: a.disabled,
        folder_exists: dir.is_dir(),
        signed_in,
        email,
        sign_in_note,
        missing_cli,
    }
}

/// The command that signs an account in, typed into a new terminal window.
pub fn sign_in_command(a: &AccountCfg) -> String {
    crate::harness::of(a).map_or_else(|| a.command().to_string(), |h| h.sign_in_command(a))
}

/// Reads the usage of every account whose harness keeps it somewhere slow to ask
/// (`Adapter::polls_usage`), one after another, but those turned off. For the snapshot and the
/// engine's background fetch.
pub fn read_all(accounts: &[AccountCfg]) -> Vec<Usage> {
    let now = now_ms();
    accounts.iter().filter(|a| !a.disabled).filter_map(|a| crate::harness::of(a).filter(|h| h.polls_usage(a)).map(|h| h.read_usage(a, now))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Meter;

    const NOW: i64 = 1_800_000_000_000;

    fn read(account: &str, as_of: i64, percent: f64) -> Usage {
        let meter = Meter { key: "week".into(), label: "Week".into(), window: "Weekly, all models".into(), percent, resets_at: None, severity: Severity::Normal, model: None };
        Usage { account: account.into(), harness: Harness::Claude, plan: None, meters: vec![meter], as_of: Some(as_of), error: None, needs_sign_in: false, loading: false, no_limits: false, preferred: false, rate_limited: false }
    }

    fn account(name: &str) -> AccountCfg {
        AccountCfg { name: name.into(), harness: "claude".into(), home: format!("~/.{name}"), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false }
    }

    #[test]
    fn a_relaunch_gets_the_numbers_of_the_last_hour_and_no_older() {
        let file = crate::store::test_dir("usage-kept").join("usage.json");
        let accounts = ["recent", "old", "unread", "signed-out"].map(account);
        assert!(kept_in(&file, &accounts, NOW).is_empty(), "a first launch has none");

        let mut usage = HashMap::new();
        usage.insert("recent".to_string(), read("recent", NOW - 5 * 60_000, 16.0));
        usage.insert("old".to_string(), read("old", NOW - KEPT_FOR_MS, 3.0));
        usage.insert("unread".to_string(), Usage { as_of: None, ..read("unread", 0, 1.0) });
        // One with no numbers has nothing to keep: signed out, say.
        usage.insert("signed-out".to_string(), Usage { meters: Vec::new(), needs_sign_in: true, ..read("signed-out", NOW, 0.0) });
        keep_in(&file, &accounts, &usage);

        let back = kept_in(&file, &accounts, NOW);
        assert_eq!(back.keys().collect::<Vec<_>>(), ["recent"]);
        assert_eq!(back["recent"], usage["recent"]);

        std::fs::write(&file, "not what this build wrote").unwrap();
        assert!(kept_in(&file, &accounts, NOW).is_empty(), "only a cache: it starts over");
    }

    #[test]
    fn numbers_stay_with_the_account_they_were_read_for() {
        let file = crate::store::test_dir("usage-kept-identity").join("usage.json");
        let usage = HashMap::from([("claude".to_string(), read("claude", NOW, 16.0)), ("gone".to_string(), read("gone", NOW, 3.0))]);
        keep_in(&file, &[account("claude")], &usage);

        let renamed = AccountCfg { name: "work".into(), ..account("claude") };
        let back = kept_in(&file, &[renamed], NOW);
        assert_eq!(back["work"].account, "work", "the same folder under another name is the same account");
        assert_eq!(back["work"].meters, usage["claude"].meters);

        let moved = AccountCfg { home: "~/.elsewhere".into(), ..account("claude") };
        assert!(kept_in(&file, &[moved], NOW).is_empty(), "the same name on another folder isn't");
        assert!(kept_in(&file, &[account("gone")], NOW).is_empty(), "one config.toml no longer lists isn't kept");
    }
}

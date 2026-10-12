//! Plan usage per account: what every harness's numbers are put into, and the asking. Where
//! the numbers come from is each harness's own (`Adapter::read_usage` for somewhere slow to
//! ask, `Reader::usage` for what comes with its sessions): Claude Code's sign-in and usage
//! endpoint are claude/usage.rs, Codex's rate-limit reports codex/usage.rs. OpenCode,
//! Antigravity and Cursor have nothing to read: OpenCode bills whichever provider you bring,
//! Antigravity keeps its quotas in the app, and Cursor asks its servers.

use crate::config::AccountCfg;
use crate::model::{AccountInfo, Harness, Severity, Usage};
use crate::util::now_ms;

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
        preferred: account.preferred,
        rate_limited: false,
    }
}

/// An account whose harness reports no limits, with why in words.
pub fn none(account: &AccountCfg, note: &str) -> Usage {
    Usage { loading: false, error: Some(note.to_string()), ..loading(account) }
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
    let (signed_in, email) = crate::harness::of(a).map_or((false, None), |h| h.signed_in(&dir));
    AccountInfo {
        name: a.name.clone(),
        harness: a.harness.clone(),
        home: a.home.clone(),
        command: a.command.clone(),
        start: a.start.clone(),
        preferred: a.preferred,
        acp: a.acp.clone(),
        monthly: a.monthly,
        per_token: a.per_token,
        folder_exists: dir.is_dir(),
        signed_in,
        email,
    }
}

/// The command that signs an account in, typed into a new terminal window.
pub fn sign_in_command(a: &AccountCfg) -> String {
    crate::harness::of(a).map_or_else(|| a.command().to_string(), |h| h.sign_in_command(a))
}

/// Reads the usage of every account whose harness keeps it somewhere slow to ask
/// (`Adapter::polls_usage`), one after another. For the snapshot and the engine's background
/// fetch.
pub fn read_all(accounts: &[AccountCfg]) -> Vec<Usage> {
    let now = now_ms();
    accounts.iter().filter_map(|a| crate::harness::of(a).filter(|h| h.polls_usage()).map(|h| h.read_usage(a, now))).collect()
}

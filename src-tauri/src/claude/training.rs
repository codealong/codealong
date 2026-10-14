//! Whether Anthropic may train its models on a Claude account's chats and coding sessions: "Help
//! improve Claude" in claude.ai's privacy settings, which is `grove_enabled` in the account's
//! settings. Codealong reads and changes it with the account's own sign-in (usage.rs), as
//! Claude Code 2.1.296's `/privacy-settings` does: `GET /api/oauth/account/settings`, and
//! `PATCH` of `{grove_enabled}` to the same place. It's `null` until the account is asked, which
//! Claude Code then does at its next start. One setting covers the account everywhere: claude.ai,
//! the Claude app and Claude Code. Claude Code offers it only to Pro and Max sign-ins, the plans
//! under Anthropic's consumer terms; Team and Enterprise are under its commercial terms, which
//! don't train on what's sent.

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use super::usage::{read_sign_in, request, Fetch};
use crate::config::AccountCfg;
use crate::training::{State, Training};
use crate::util::{code, now_ms};

const SETTINGS_URL: &str = "https://api.anthropic.com/api/oauth/account/settings";
/// Where a person changes it by hand.
pub const PAGE: &str = "https://claude.ai/settings/data-privacy-controls";

/// Plans under the commercial terms, as the sign-in's `subscriptionType` names them.
fn commercial(subscription: &str) -> bool {
    matches!(subscription, "team" | "enterprise")
}

pub fn read(account: &AccountCfg) -> Training {
    let fix = format!("run {} once to sign in again", code(account.command()));
    let Some(sign_in) = read_sign_in(&account.home_path()) else {
        return Training::unknown(account, format!("Not signed in: {fix}.")).at(PAGE);
    };
    if let Some(plan) = sign_in.subscription.as_deref().filter(|s| commercial(s)) {
        return Training::new(account, State::Off, format!("{} plans are under Anthropic's commercial terms, which don't train on what you send.", crate::usage::title(plan)));
    }
    if sign_in.expires_at.is_some_and(|t| t <= now_ms() + 60_000) {
        return Training::unknown(account, format!("Sign-in expired: {fix}.")).at(PAGE);
    }
    match request(&sign_in.token, "GET", SETTINGS_URL, None) {
        Ok(v) => from_settings(account, &v),
        Err(Fetch::SignedOut) => Training::unknown(account, format!("Sign-in expired: {fix}.")).at(PAGE),
        Err(Fetch::RateLimited) => Training::unknown(account, "Couldn't read its privacy settings: rate limited.").at(PAGE),
        Err(Fetch::Failed(why)) => Training::unknown(account, format!("Couldn't read its privacy settings: {why}.")).at(PAGE),
    }
}

/// Where the account stands from its settings.
pub fn from_settings(account: &AccountCfg, settings: &Value) -> Training {
    match settings.get("grove_enabled") {
        Some(Value::Bool(false)) => Training::new(account, State::Off, "Help improve Claude is off in its privacy settings.").at(PAGE),
        Some(Value::Bool(true)) => {
            Training::new(account, State::On, "Help improve Claude is on: Anthropic may train its models on this account's chats and coding sessions, and keeps them for five years rather than 30 days.").at(PAGE).stoppable()
        }
        _ => Training::new(account, State::Unset, "Help improve Claude isn't chosen yet: Claude asks the next time it starts. Turning it off now settles it.").at(PAGE).stoppable(),
    }
}

/// Turns Help improve Claude off.
pub fn stop(account: &AccountCfg) -> Result<()> {
    let sign_in = read_sign_in(&account.home_path()).ok_or_else(|| anyhow!("{} isn't signed in: run {} once to sign in.", account.name, code(account.command())))?;
    match request(&sign_in.token, "PATCH", SETTINGS_URL, Some(&json!({ "grove_enabled": false }))) {
        Ok(_) => Ok(()),
        Err(Fetch::SignedOut) => bail!("{}'s sign-in has expired: run {} once to sign in again.", account.name, code(account.command())),
        Err(Fetch::RateLimited) => bail!("Anthropic asked Codealong to slow down; try again in a minute."),
        Err(Fetch::Failed(why)) => bail!("Couldn't change {}'s privacy settings: {why}.", account.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> AccountCfg {
        AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false, disabled: false }
    }

    /// As the endpoint answered on 2026-10-13, with the rest of the settings left out.
    #[test]
    fn reads_help_improve_claude_from_the_account_settings() {
        let read = |v: Value| from_settings(&account(), &v);
        let on = read(json!({"grove_enabled": true, "grove_updated_at": "2025-10-08T18:11:20.506150Z", "grove_notice_viewed_at": "2025-10-08T18:11:19.017332Z"}));
        assert_eq!((on.state, on.can_stop, on.url.as_deref()), (State::On, true, Some(PAGE)));
        let off = read(json!({"grove_enabled": false}));
        assert_eq!((off.state, off.can_stop), (State::Off, false));
        let unset = read(json!({"grove_enabled": null, "grove_notice_viewed_at": null}));
        assert_eq!((unset.state, unset.can_stop), (State::Unset, true));
        assert_eq!(read(json!({})).state, State::Unset);
    }

    #[test]
    fn team_and_enterprise_are_commercial() {
        assert!(commercial("team") && commercial("enterprise"));
        assert!(!commercial("max") && !commercial("pro"));
    }
}

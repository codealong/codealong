//! Who a Claude account is signed in as, and on what plan (signin.rs). Its files: the state
//! file's `oauthAccount` (trust.rs `state_file`), which Claude Code writes as it signs in and keeps
//! up to date from the profile below: `accountUuid`, `emailAddress`, `organizationUuid`,
//! `organizationName`, `organizationType` (`claude_team` here for a Team seat, 2026-11-17), with
//! `seatTier`, `billingType`, `profileFetchedAt` and more beside them. Its provider:
//! `GET https://api.anthropic.com/api/oauth/profile` with the sign-in's token, as Claude Code
//! 2.1.327 asks it, whose answer has `account {uuid, email, display_name…}` and `organization
//! {uuid, name, organization_type, rate_limit_tier, seat_tier, billing_type…}`; Claude Code maps
//! `organization_type` `claude_max`, `claude_pro`, `claude_team` and `claude_enterprise` to the
//! plans max, pro, team and enterprise, and so does this. Team and Enterprise are under
//! Anthropic's commercial terms (training.rs `commercial`).

use serde_json::Value;

use super::usage::{read_sign_in, request, Fetch};
use crate::config::AccountCfg;
use crate::signin::{Local, NotSaid, Said};

const PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";

/// The plan an organization's type stands for, as Claude Code names it; another type as it is.
fn plan_of(organization_type: &str) -> String {
    organization_type.strip_prefix("claude_").unwrap_or(organization_type).to_string()
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

/// What the state file's `oauthAccount` says.
pub fn local(account: &AccountCfg) -> Option<Local> {
    let state: Value = serde_json::from_str(&std::fs::read_to_string(super::trust::state_file(&account.home_path())).ok()?).ok()?;
    local_from(&state)
}

pub fn local_from(state: &Value) -> Option<Local> {
    let o = state.get("oauthAccount").filter(|o| o.is_object())?;
    let kind = text(o, "organizationType");
    let plan = kind.as_deref().map(plan_of);
    let said = Said {
        email: text(o, "emailAddress"),
        organization: text(o, "organizationUuid"),
        organization_name: text(o, "organizationName"),
        commercial: plan.as_deref().is_some_and(super::training::commercial),
        plan,
    };
    let identity = ["accountUuid", "organizationUuid", "organizationType", "emailAddress"].map(|k| text(o, k).unwrap_or_default()).join("\n");
    Some(Local { said, identity })
}

/// What Anthropic says, asked with the account's own sign-in.
pub fn verify(account: &AccountCfg) -> Result<Said, NotSaid> {
    let sign_in = read_sign_in(&account.home_path()).ok_or_else(|| NotSaid::Refused("not signed in".into()))?;
    match request(&sign_in.token, "GET", PROFILE_URL, None) {
        Ok(v) => from_profile(&v).ok_or_else(|| NotSaid::Refused("its profile came back without an organization".into())),
        Err(Fetch::SignedOut) => Err(NotSaid::Refused("its sign-in has expired".into())),
        Err(Fetch::RateLimited) => Err(NotSaid::Unreached("rate limited".into())),
        Err(Fetch::Failed(why)) => Err(NotSaid::Unreached(why)),
    }
}

pub fn from_profile(v: &Value) -> Option<Said> {
    let org = v.get("organization").filter(|o| o.is_object())?;
    let plan = text(org, "organization_type").map(|t| plan_of(&t));
    Some(Said {
        email: v.get("account").and_then(|a| text(a, "email")),
        organization: Some(text(org, "uuid")?),
        organization_name: text(org, "name"),
        commercial: plan.as_deref().is_some_and(super::training::commercial),
        plan,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_state_files_sign_in() {
        let state = json!({"oauthAccount": {"accountUuid": "a1", "emailAddress": "me@northwind.test", "organizationUuid": "o1", "organizationName": "Northwind", "organizationType": "claude_team", "seatTier": "team_tier_1"}});
        let l = local_from(&state).unwrap();
        assert_eq!((l.said.plan.as_deref(), l.said.commercial, l.said.organization_name.as_deref()), (Some("team"), true, Some("Northwind")));
        let max = json!({"oauthAccount": {"accountUuid": "a1", "emailAddress": "me@northwind.test", "organizationUuid": "o2", "organizationType": "claude_max"}});
        let m = local_from(&max).unwrap();
        assert!(!m.said.commercial && m.identity != l.identity, "another organization is another sign-in");
        assert!(local_from(&json!({"projects": {}})).is_none());
    }

    #[test]
    fn reads_the_profile_as_claude_code_does() {
        let p = json!({"account": {"uuid": "a1", "email": "me@northwind.test"}, "organization": {"uuid": "o1", "name": "Northwind", "organization_type": "claude_enterprise"}});
        let s = from_profile(&p).unwrap();
        assert_eq!((s.plan.as_deref(), s.commercial, s.organization.as_deref()), (Some("enterprise"), true, Some("o1")));
        let pro = from_profile(&json!({"account": {"email": "x"}, "organization": {"uuid": "o3", "organization_type": "claude_pro"}})).unwrap();
        assert!(!pro.commercial);
        // A type nobody named is never commercial.
        let odd = from_profile(&json!({"organization": {"uuid": "o4", "organization_type": "claude_something"}})).unwrap();
        assert_eq!((odd.plan.as_deref(), odd.commercial), (Some("something"), false));
        assert!(from_profile(&json!({"account": {}})).is_none());
    }
}

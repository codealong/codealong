//! Who a Codex account is signed in as, and on what plan (signin.rs). Its files: `auth.json`'s
//! access token, whose claims name the ChatGPT account (`https://api.openai.com/auth`
//! `chatgpt_account_id`, a personal account's or a workspace's), the plan (`chatgpt_plan_type`:
//! plus, pro, team, business, enterprise, edu) and the person (`https://api.openai.com/profile`
//! `email`). Its provider: `GET chatgpt.com/backend-api/accounts/check/v4-2023-04-27` with that
//! token, as the ChatGPT app asks it, whose `accounts` are keyed by account id, each with
//! `account {account_id, plan_type, plan_display_name, structure, name…}`; `structure` is
//! `personal` or `workspace` (a personal Pro account here, 2026-11-17). Workspace plans are under
//! OpenAI's business terms (training.rs `workspace`). Signed in with an API key, there's no
//! account to read: an API key's terms are its organization's, which only the API's dashboard
//! says.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::config::AccountCfg;
use crate::signin::{Local, NotSaid, Said};

const ACCOUNTS: &str = "https://chatgpt.com/backend-api/accounts/check/v4-2023-04-27";

struct Token {
    token: String,
    claims: Value,
}

fn token(home: &Path) -> Option<Token> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(home.join("auth.json")).ok()?).ok()?;
    let token = v.pointer("/tokens/access_token").and_then(Value::as_str).filter(|t| !t.is_empty())?.to_string();
    let payload = token.split('.').nth(1)?;
    let bytes = data_encoding::BASE64URL_NOPAD.decode(payload.trim_end_matches('=').as_bytes()).ok()?;
    let claims = serde_json::from_slice(&bytes).ok()?;
    Some(Token { token, claims })
}

fn claim(claims: &Value, path: &str) -> Option<String> {
    claims.pointer(path).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

/// What the token's claims say.
pub fn local(account: &AccountCfg) -> Option<Local> {
    local_from(&token(&account.home_path())?.claims)
}

fn local_from(claims: &Value) -> Option<Local> {
    let organization = claim(claims, "/https:~1~1api.openai.com~1auth/chatgpt_account_id")?;
    let plan = claim(claims, "/https:~1~1api.openai.com~1auth/chatgpt_plan_type");
    let email = claim(claims, "/https:~1~1api.openai.com~1profile/email");
    let identity = [Some(organization.clone()), plan.clone(), email.clone()].map(Option::unwrap_or_default).join("\n");
    let said = Said { email, organization: Some(organization), organization_name: None, commercial: plan.as_deref().is_some_and(super::training::workspace), plan };
    Some(Local { said, identity })
}

/// What OpenAI says of the account the token is for.
pub fn verify(account: &AccountCfg) -> Result<Said, NotSaid> {
    let t = token(&account.home_path()).ok_or_else(|| NotSaid::Refused("not signed in to ChatGPT".into()))?;
    let id = claim(&t.claims, "/https:~1~1api.openai.com~1auth/chatgpt_account_id").ok_or_else(|| NotSaid::Refused("its sign-in names no ChatGPT account".into()))?;
    let headers = vec![format!("Authorization: Bearer {}", t.token), format!("ChatGPT-Account-Id: {id}")];
    let reply = crate::http::call("GET", ACCOUNTS, &headers, None, Duration::from_secs(10)).map_err(NotSaid::Unreached)?;
    let v = match reply.status {
        200..=299 => reply.json().ok_or_else(|| NotSaid::Unreached("unexpected response".into()))?,
        401 | 403 => return Err(NotSaid::Refused("its sign-in has expired".into())),
        429 => return Err(NotSaid::Unreached("rate limited".into())),
        other => return Err(NotSaid::Unreached(format!("HTTP {other}"))),
    };
    from_accounts(&v, &id, claim(&t.claims, "/https:~1~1api.openai.com~1profile/email")).ok_or_else(|| NotSaid::Refused("OpenAI didn't list the account its sign-in names".into()))
}

/// The account `id` from `accounts/check`'s answer. Only a workspace's plan counts as commercial:
/// a personal account's `plan_type` reads its own plan.
fn from_accounts(v: &Value, id: &str, email: Option<String>) -> Option<Said> {
    let a = v.pointer("/accounts")?.get(id)?.get("account")?;
    let plan = claim(a, "/plan_type");
    let workspace = claim(a, "/structure").as_deref() == Some("workspace");
    Some(Said { email, organization: Some(claim(a, "/account_id").unwrap_or_else(|| id.to_string())), organization_name: claim(a, "/name"), commercial: workspace && plan.as_deref().is_some_and(super::training::workspace), plan })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_tokens_claims() {
        let claims = json!({"https://api.openai.com/auth": {"chatgpt_account_id": "acct-1", "chatgpt_plan_type": "business"}, "https://api.openai.com/profile": {"email": "me@atlas.test"}});
        let l = local_from(&claims).unwrap();
        assert_eq!((l.said.plan.as_deref(), l.said.commercial, l.said.organization.as_deref()), (Some("business"), true, Some("acct-1")));
        let pro = json!({"https://api.openai.com/auth": {"chatgpt_account_id": "acct-2", "chatgpt_plan_type": "pro"}});
        assert!(!local_from(&pro).unwrap().said.commercial);
        assert!(local_from(&json!({})).is_none());
    }

    /// As `accounts/check` answered for a personal Pro account on 2026-11-17, cut to what's read.
    #[test]
    fn reads_the_account_openai_lists() {
        let v = json!({"accounts": {"acct-1": {"account": {"account_id": "acct-1", "plan_type": "pro", "structure": "personal", "name": null}}, "default": {}}});
        let s = from_accounts(&v, "acct-1", Some("me@atlas.test".into())).unwrap();
        assert_eq!((s.plan.as_deref(), s.commercial), (Some("pro"), false));
        let w = json!({"accounts": {"ws-1": {"account": {"account_id": "ws-1", "plan_type": "enterprise", "structure": "workspace", "name": "Atlas"}}}});
        let s = from_accounts(&w, "ws-1", None).unwrap();
        assert_eq!((s.commercial, s.organization_name.as_deref()), (true, Some("Atlas")));
        // A personal account that reads a workspace's plan isn't one.
        let odd = json!({"accounts": {"acct-3": {"account": {"plan_type": "business", "structure": "personal"}}}});
        assert!(!from_accounts(&odd, "acct-3", None).unwrap().commercial);
        assert!(from_accounts(&v, "acct-9", None).is_none());
    }
}

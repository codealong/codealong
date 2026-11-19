//! `codealong accounts`, `codealong mcp` and `codealong account check`: the configured accounts
//! with their sign-ins and usage, the services their agents reach, and which account the session
//! running the command is on.
//!
//! A guard hook runs `account check` before every command and gives it 3 seconds in all
//! (`CHECK_WAIT`, `CHECK_BUDGET`), so it asks the server one question, which `account_check_on`
//! answers, and `main` makes no `Client` for it; only with no server, or one from before the
//! question, does it look for itself as `whoami` does (`account_check_here`). Its exit code is
//! the answer (`AccountCmd::Check`), and a repository's guard hook reads what it prints as JSON
//! (`AccountCheck`) by the names its fields have: they don't change.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::json;

use crate::config::{self, AccountCfg, Config};
use crate::engine;
use crate::model::Board;
use crate::server::{self, Request};
use crate::start;
use crate::usage;
use crate::util::now_ms;

use super::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountRow {
    name: String,
    harness: String,
    command: Option<String>,
    signed_in: bool,
    email: Option<String>,
    /// What it's signed in to, for a harness that brings other providers' sign-ins (OpenCode).
    #[serde(skip_serializing_if = "Option::is_none")]
    sign_in_note: Option<String>,
    /// Turned off in config.toml: Codealong starts nothing on it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    disabled: bool,
    /// Its command line, when it isn't on this Mac: it starts nothing that needs it.
    #[serde(skip_serializing_if = "Option::is_none")]
    missing_cli: Option<String>,
    home: String,
    /// The plan limits, when read: label, percent used, when it resets.
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<Vec<MeterRow>>,
    /// How much of the tightest limit is left, when read.
    #[serde(skip_serializing_if = "Option::is_none")]
    room: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeterRow {
    label: String,
    window: String,
    percent: f64,
    resets_at: Option<i64>,
}

/// Each account as `usage::describe` has it: the server's (`Request::Accounts`), which keeps what
/// your shell said of their command lines, else this command's own, which starts a login shell
/// for each command line the PATH misses (`detect::shell_finds`): half a second each time.
fn described(c: &Client, cfg: &Config) -> Result<Vec<crate::model::AccountInfo>> {
    if c.app.is_some() {
        match server::ask(&Request::Accounts) {
            Some(Ok(v)) => return serde_json::from_value(v).context("reading the server's accounts"),
            // A server older than the request, or one gone since it answered: this command looks itself.
            Some(Err(e)) if !e.starts_with(server::UNKNOWN_REQUEST) => bail!("{e}"),
            _ => {}
        }
    }
    Ok(cfg.accounts.iter().map(usage::describe).collect())
}

pub(super) fn accounts(c: &mut Client, with_usage: bool, json: bool) -> Result<()> {
    let cfg = c.cfg()?.clone();
    let mut rows: Vec<AccountRow> = described(c, &cfg)?
        .into_iter()
        .map(|info| AccountRow { name: info.name, harness: info.harness, command: info.command, signed_in: info.signed_in, email: info.email, sign_in_note: info.sign_in_note, disabled: info.disabled, missing_cli: info.missing_cli, home: info.home, usage: None, room: None, usage_error: None })
        .collect();
    if with_usage {
        let usages = match c.app.is_some() {
            true => c.board()?.usage.clone(),
            false => usage::read_all(&cfg.accounts),
        };
        for u in usages {
            let Some(row) = rows.iter_mut().find(|r| r.name == u.account) else { continue };
            if u.error.is_some() && u.meters.is_empty() {
                row.usage_error = u.error.clone();
                continue;
            }
            row.room = u.meters.iter().map(|m| 100.0 - m.percent).fold(None, |acc: Option<f64>, x| Some(acc.map_or(x, |a| a.min(x))));
            row.usage = Some(u.meters.iter().map(|m| MeterRow { label: m.label.clone(), window: m.window.clone(), percent: m.percent, resets_at: m.resets_at }).collect());
        }
    }
    if json {
        return print_json(&rows);
    }
    let now = now_ms();
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let usage = match (&r.usage, &r.usage_error) {
                (Some(ms), _) => ms.iter().map(|m| format!("{} {:.0}%{}", m.label, m.percent, m.resets_at.map(|t| format!(" (resets in {})", ago(t - now))).unwrap_or_default())).collect::<Vec<_>>().join(", "),
                (None, Some(e)) => e.clone(),
                _ => String::new(),
            };
            let sign_in = match (r.disabled, r.signed_in) {
                (true, _) => "turned off",
                (false, true) => "signed in",
                (false, false) => "not signed in",
            };
            let sign_in = r.missing_cli.as_ref().map_or_else(|| sign_in.to_string(), |cli| format!("{sign_in}, no {cli}"));
            vec![r.name.clone(), r.harness.clone(), sign_in, r.sign_in_note.clone().or_else(|| r.email.clone()).unwrap_or_default(), usage]
        })
        .collect();
    print!("{}", table_text(&["ACCOUNT", "HARNESS", "SIGN-IN", "EMAIL", "USAGE"], table));
    Ok(())
}

// ---- mcp ----

/// What `codealong mcp` prints: kept small, since it's mostly read by an agent looking for a
/// service its own account doesn't reach.
#[derive(Serialize, Debug, PartialEq)]
struct McpOut {
    services: Vec<ServiceRow>,
    /// The accounts whose servers couldn't all be read, and why: a service missing from the list
    /// may be on one of them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    notes: Vec<McpNote>,
}

#[derive(Serialize, Debug, PartialEq)]
struct ServiceRow {
    name: String,
    /// The accounts a session can start on that reach it, the one Auto would start first.
    accounts: Vec<String>,
    /// The model Auto would start the first of them on, when it isn't that account's default
    /// (its default's limit nearly used, say): a start with `--account` takes it only as
    /// `--model`.
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    /// Only when a service was named or `--tools` given: they're most of what there is to say.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<String>,
}

#[derive(Serialize, Debug, PartialEq)]
struct McpNote {
    account: String,
    note: String,
}

/// The app's reading of each account's servers (mcp.rs, `McpPoll`), with the accounts it hasn't
/// read yet (it reads them at launch, and a light board has none) read here.
pub(super) fn mcp(c: &mut Client, service: Option<&str>, tools: bool, json: bool) -> Result<()> {
    let cfg = c.cfg()?.clone();
    let board = c.board()?;
    let mut readings = board.mcp.clone();
    let unread: Vec<AccountCfg> = cfg.accounts.iter().filter(|a| !readings.iter().any(|m| m.account == a.name)).cloned().collect();
    readings.extend(crate::mcp::read_all(&unread));
    let now = now_ms();
    let out = mcp_out(crate::route::reach(board, &readings, &cfg, now), mcp_notes(&cfg.accounts, &readings, now), service, tools)?;
    if json {
        return print_json(&out);
    }
    let with_tools = tools || service.is_some();
    let accounts = |s: &ServiceRow| {
        let mut names = s.accounts.clone();
        if let (Some(first), Some(model)) = (names.first_mut(), &s.model) {
            *first = format!("{first} ({model})");
        }
        names.join(", ")
    };
    let rows: Vec<Vec<String>> = out.services.iter().map(|s| vec![s.name.clone(), accounts(s), s.tools.join(", ")]).collect();
    let head: &[&str] = if with_tools { &["SERVICE", "ACCOUNTS", "TOOLS"] } else { &["SERVICE", "ACCOUNTS"] };
    print!("{}", table_text(head, rows));
    for n in &out.notes {
        println!("{}: {}", n.account, n.note);
    }
    Ok(())
}

/// The services to print (`services`, as `route::reach` has them): the ones with `query` in their
/// names (`mcp::key`, so "google calendar" finds Google Calendar), with their tools, else all of
/// them, with their tools only when asked. One named that no account reaches is an error saying
/// which they do.
fn mcp_out(services: Vec<crate::route::Service>, notes: Vec<McpNote>, query: Option<&str>, tools: bool) -> Result<McpOut> {
    let want = query.map(crate::mcp::key);
    if want.as_deref() == Some("") {
        bail!("Give a service's name, or part of it: `codealong mcp slack`.");
    }
    let named = |s: &crate::route::Service| want.as_ref().is_none_or(|w| crate::mcp::key(&s.name).contains(w.as_str()));
    if let (Some(q), false) = (query, services.iter().any(named)) {
        let mut why = match services.is_empty() {
            true => format!("No account reaches a service whose name has \"{q}\" in it: they reach none through MCP servers or connectors."),
            false => format!("No account reaches a service whose name has \"{q}\" in it. They reach {}.", services.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")),
        };
        for n in &notes {
            why.push_str(&format!(" {}'s couldn't all be read: {}", n.account, n.note));
        }
        bail!(why);
    }
    let services = services
        .into_iter()
        .filter(|s| named(s))
        .map(|s| ServiceRow { name: s.name, accounts: s.accounts, model: s.model, tools: if tools || query.is_some() { s.tools } else { Vec::new() } })
        .collect();
    Ok(McpOut { services, notes })
}

/// What couldn't be read of each account that's on, in config order: one not read at all, part
/// of one (the connectors, when its sign-in expired), or one whose last read failed, which keeps
/// the reading before it (`stale`).
fn mcp_notes(accounts: &[AccountCfg], readings: &[crate::mcp::Mcp], now: i64) -> Vec<McpNote> {
    accounts
        .iter()
        .filter(|a| !a.disabled)
        .filter_map(|a| {
            let note = match readings.iter().find(|m| m.account == a.name) {
                None => "Not read yet.".to_string(),
                Some(m) => match (&m.note, &m.stale, m.as_of) {
                    (Some(note), _, _) => note.clone(),
                    (None, Some(why), Some(at)) => {
                        let when = match ago(now - at).as_str() {
                            "now" => "just now".to_string(),
                            a => format!("{a} ago"),
                        };
                        format!("As read {when}; reading it since failed: {why}")
                    }
                    (None, Some(why), None) => why.clone(),
                    (None, None, _) => return None,
                },
            };
            Some(McpNote { account: a.name.clone(), note })
        })
        .collect()
}

// ---- account check ----

/// What `account check` answers, as `account check --json` prints it.
#[derive(Serialize, serde::Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountCheck {
    /// A session runs the command.
    pub(super) found: bool,
    /// How it was found, as `whoami` says: `process`, `record` or `environment`.
    pub(super) how: Option<String>,
    session: Option<String>,
    pub(super) account: Option<String>,
    harness: Option<String>,
    pub(super) sign_in: Option<crate::signin::SignIn>,
    /// The routing groups the account is in.
    groups: Vec<String>,
    /// With a group asked about: whether the account is in it, which for a boundary takes a
    /// sign-in that qualifies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    in_group: Option<bool>,
}

/// How long `account check` waits on the server: a guard hook gives it 3 seconds in all.
pub(super) const CHECK_WAIT: std::time::Duration = std::time::Duration::from_millis(2500);

/// The server's question for the command running now.
pub(super) fn check_request(group: Option<&str>) -> Request {
    Request::AccountCheck { pid: std::process::id(), args: std::env::args().skip(1).collect(), session_var: std::env::var(start::SESSION_VAR).ok(), group: group.map(String::from) }
}

/// `account check`: the server's answer, or with none, this command's own from the files, the
/// provider asked here with nothing kept. The exit code says it (`AccountCmd::Check`).
pub(super) fn account_check(group: Option<&str>, json: bool) -> i32 {
    let asked = match crate::os::sock::UnixStream::connect(server::path()) {
        Ok(stream) => match server::ask_on(stream, &check_request(group), CHECK_WAIT) {
            Some(Ok(v)) => serde_json::from_value::<AccountCheck>(v).map_err(|e| format!("The server's answer didn't read: {e}")),
            // A server older than the request: this command looks for itself.
            Some(Err(e)) if e.starts_with(server::UNKNOWN_REQUEST) => account_check_here(group),
            Some(Err(e)) => Err(e),
            None => Err("Codealong's server didn't answer in time.".into()),
        },
        // No server: this command looks for itself.
        Err(_) => account_check_here(group),
    };
    let check = match asked {
        Ok(c) => c,
        Err(e) => {
            if json {
                eprintln!("{}", json!({ "error": e }));
            } else {
                eprintln!("error: {e}");
            }
            return 2;
        }
    };
    if json {
        if print_json(&check).is_err() {
            return 2;
        }
    } else {
        print!("{}", check_text(&check, group));
    }
    match (check.found, check.in_group) {
        (false, _) => 3,
        (true, Some(false)) => 1,
        _ => 0,
    }
}

fn account_check_here(group: Option<&str>) -> std::result::Result<AccountCheck, String> {
    let mut c = Client::new(false, false, false);
    let me = find_me(&mut c).map_err(|e| format!("{e:#}"))?;
    let cfg = c.cfg().map_err(|e| format!("{e:#}"))?.clone();
    let board = c.raw_board().map_err(|e| format!("{e:#}"))?;
    let found = me.0.as_deref().and_then(|k| account_of_session(board, k));
    account_check_for(&cfg, me, found, group, |a| crate::signin::current_within(a, now_ms(), PROVIDER_WAIT))
}

/// How long `account check` has, from when it's asked, before it answers with what it has: a
/// guard hook gives it 3 seconds in all.
const CHECK_BUDGET: std::time::Duration = std::time::Duration::from_millis(2200);
/// How long a provider is asked when nothing else took the time.
const PROVIDER_WAIT: std::time::Duration = std::time::Duration::from_millis(1800);

/// The server's answer to `account check` for the command `pid`: the session found as `whoami`
/// finds it, by the board as it is, and the account's sign-in as it's kept (signin.rs).
pub(crate) fn account_check_on(engine: &engine::Handle, pid: u32, args: &[String], var: Option<&str>, group: Option<&str>) -> std::result::Result<AccountCheck, String> {
    let asked = std::time::Instant::now();
    let cfg = config::current().map_err(|e| format!("{e:#}"))?;
    // By its processes under a read of the board; the records only when they don't say.
    let chain = chain_of(pid);
    let me = match engine.read(|b| by_process(b, &chain)) {
        Some(key) => (Some(key), Some("process")),
        None => session_running(&engine.board(), &cfg, pid, args, var, now_ms()),
    };
    // The board's lock is let go before a provider is asked.
    let found = me.0.as_deref().and_then(|k| engine.read(|b| account_of_session(b, k)));
    // What finding the session took comes off the provider's time.
    let left = CHECK_BUDGET.saturating_sub(asked.elapsed()).min(PROVIDER_WAIT);
    account_check_for(&cfg, me, found, group, |a| crate::signin::current_within(a, now_ms(), left))
}

/// `account check`'s answer from a board, the provider asked as `ask` says.
pub(crate) fn account_check_of(board: &Board, cfg: &Config, me: (Option<String>, Option<&'static str>), group: Option<&str>, ask: crate::signin::Ask, now: i64) -> std::result::Result<AccountCheck, String> {
    let found = me.0.as_deref().and_then(|k| account_of_session(board, k));
    account_check_for(cfg, me, found, group, |a| crate::signin::current(a, now, ask))
}

/// The account and harness of the session `key` on the board.
fn account_of_session(board: &Board, key: &str) -> Option<(String, String)> {
    all_sessions(board).into_iter().find(|(s, _)| s.key == key).map(|(s, _)| (s.account.clone(), lower_debug(&s.harness)))
}

/// `account check`'s answer, given the session found (`me`) and how, its account and harness
/// where the board has it (`found`), and how to read a sign-in. A session found by the variable
/// alone isn't on the board yet: its key names its account.
fn account_check_for(cfg: &Config, me: (Option<String>, Option<&'static str>), found: Option<(String, String)>, group: Option<&str>, sign_in_of: impl Fn(&crate::config::AccountCfg) -> crate::signin::SignIn) -> std::result::Result<AccountCheck, String> {
    let asked = match group {
        Some(name) => Some(cfg.routing.groups.iter().find(|g| g.name.trim() == name.trim()).ok_or_else(|| format!("There's no routing group named {name:?} in config.toml."))?),
        None => None,
    };
    let (Some(key), how) = me else {
        return Ok(AccountCheck { in_group: asked.map(|_| false), ..AccountCheck::default() });
    };
    let (account, harness) = found.unwrap_or_else(|| {
        let mut parts = key.splitn(3, ':');
        let harness = parts.next().unwrap_or_default().to_string();
        (parts.next().unwrap_or_default().to_string(), harness)
    });
    let configured = cfg.accounts.iter().find(|a| a.name == account);
    let sign_in = configured.map(sign_in_of);
    let qualifies = sign_in.as_ref().is_some_and(|s| s.qualifies());
    Ok(AccountCheck {
        found: true,
        how: how.map(str::to_string),
        session: Some(key),
        groups: if configured.is_some() { crate::boundary::groups_of(cfg, &account, qualifies) } else { Vec::new() },
        // Found by the variable alone, which an agent can set, it's in no group: a hook that goes
        // by the exit code never lets it in.
        in_group: asked.map(|g| configured.is_some() && how != Some("environment") && crate::boundary::in_group(cfg, g, &account, qualifies)),
        account: Some(account),
        harness: Some(harness),
        sign_in,
    })
}

/// `account check` in words.
fn check_text(c: &AccountCheck, group: Option<&str>) -> String {
    let (Some(session), Some(account)) = (&c.session, &c.account) else {
        return "No session runs this command: it's a person's own terminal, as far as Codealong can tell.\n".into();
    };
    let by = match c.how.as_deref() {
        Some("process") => "its process",
        Some("record") => "its own record of this command",
        _ => "CODEALONG_SESSION, which an agent can set itself",
    };
    let mut out = format!("Session {session} ({} on {account}), found by {by}.\n", c.harness.as_deref().unwrap_or("?"));
    match &c.sign_in {
        Some(s) => {
            let who = [s.email.as_deref(), s.organization_name.as_deref().or(s.organization.as_deref())].into_iter().flatten().collect::<Vec<_>>().join(" in ");
            let plan = s.plan.as_deref().unwrap_or("an unknown plan");
            let terms = if s.commercial { "commercial terms" } else { "not commercial terms" };
            let checked = if s.verified { "as its provider says" } else { "as its files say; its provider hasn't confirmed it" };
            out.push_str(&format!("Signed in{} on {plan}: {terms}, {checked}.\n", if who.is_empty() { String::new() } else { format!(" as {who}") }));
            if let Some(note) = &s.note {
                out.push_str(&format!("{note}\n"));
            }
        }
        None => out.push_str(&format!("{account} isn't in config.toml's [[accounts]], so its sign-in isn't known.\n")),
    }
    if !c.groups.is_empty() {
        out.push_str(&format!("In routing groups: {}.\n", c.groups.join(", ")));
    }
    if let (Some(g), Some(inside)) = (group, c.in_group) {
        out.push_str(&format!("{} group {g}.\n", if inside { "In" } else { "Not in" }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::tests::boundary_cfg;

    fn reached(name: &str, accounts: &[&str], tools: &[&str]) -> crate::route::Service {
        crate::route::Service { name: name.into(), tools: tools.iter().map(|t| t.to_string()).collect(), accounts: accounts.iter().map(|a| a.to_string()).collect(), model: None }
    }

    #[test]
    fn mcp_lists_services_without_their_tools_unless_one_is_named() {
        let services = || vec![reached("Gmail", &["claude", "codex"], &["search_threads"]), reached("Google Calendar", &["claude"], &["list_events"]), reached("Slack", &["atlas"], &["slack_read_channel"])];
        let names = |out: &McpOut| out.services.iter().map(|s| (s.name.clone(), s.accounts.join(" "), s.tools.len())).collect::<Vec<_>>();
        let all = mcp_out(services(), Vec::new(), None, false).unwrap();
        assert_eq!(names(&all), [("Gmail".into(), "claude codex".into(), 0), ("Google Calendar".into(), "claude".into(), 0), ("Slack".into(), "atlas".into(), 0)]);
        assert_eq!(serde_json::to_value(&all).unwrap()["services"][0], json!({ "name": "Gmail", "accounts": ["claude", "codex"] }));
        assert_eq!(mcp_out(services(), Vec::new(), None, true).unwrap().services[2].tools, ["slack_read_channel"]);
        // The model Auto would start the first account on goes with it, for `--model`.
        let fallback = crate::route::Service { model: Some("opus".into()), ..reached("Slack", &["atlas", "claude"], &[]) };
        assert_eq!(serde_json::to_value(mcp_out(vec![fallback], Vec::new(), None, false).unwrap()).unwrap()["services"][0], json!({ "name": "Slack", "accounts": ["atlas", "claude"], "model": "opus" }));
        // Part of a name, written any way, finds it, with its tools.
        let calendar = mcp_out(services(), Vec::new(), Some("google-calendar"), false).unwrap();
        assert_eq!(names(&calendar), [("Google Calendar".into(), "claude".into(), 1)]);
        assert_eq!(names(&mcp_out(services(), Vec::new(), Some("google"), false).unwrap()).len(), 1);
        assert_eq!(names(&mcp_out(services(), Vec::new(), Some("MAIL"), false).unwrap())[0].0, "Gmail");
        assert!(mcp_out(services(), Vec::new(), Some(" - "), false).is_err());
    }

    #[test]
    fn mcp_says_what_the_accounts_reach_when_none_has_the_service_named() {
        let services = vec![reached("Gmail", &["claude"], &[]), reached("Slack", &["atlas"], &[])];
        let notes = vec![McpNote { account: "kestrel".into(), note: "Not read yet.".into() }];
        let err = mcp_out(services, notes, Some("figma"), false).unwrap_err().to_string();
        assert_eq!(err, "No account reaches a service whose name has \"figma\" in it. They reach Gmail, Slack. kestrel's couldn't all be read: Not read yet.");
        let err = mcp_out(Vec::new(), Vec::new(), Some("figma"), false).unwrap_err().to_string();
        assert!(err.ends_with("they reach none through MCP servers or connectors."), "{err}");
    }

    #[test]
    fn mcp_notes_what_it_couldnt_read() {
        let account = |name: &str, disabled: bool| AccountCfg { name: name.into(), harness: "claude".into(), home: format!("~/.{name}"), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled, chrome_profile: None };
        let reading = |name: &str, note: Option<&str>, stale: Option<&str>| crate::mcp::Mcp { account: name.into(), servers: Vec::new(), note: note.map(String::from), as_of: Some(1_000), stale: stale.map(String::from) };
        let accounts = [account("claude", false), account("atlas", false), account("kestrel", false), account("northwind", false), account("off", true)];
        let readings = [reading("claude", None, None), reading("atlas", Some("The connectors couldn't be read: sign in again."), None), reading("kestrel", None, Some("mcp.json doesn't parse."))];
        let notes = mcp_notes(&accounts, &readings, 1_000 + 2 * 3_600_000);
        assert_eq!(notes.iter().map(|n| (n.account.as_str(), n.note.as_str())).collect::<Vec<_>>(), [
            ("atlas", "The connectors couldn't be read: sign in again."),
            ("kestrel", "As read 2h ago; reading it since failed: mcp.json doesn't parse."),
            ("northwind", "Not read yet."),
        ]);
        assert_eq!(mcp_notes(&accounts[2..3], &readings, 1_000)[0].note, "As read just now; reading it since failed: mcp.json doesn't parse.");
    }

    fn signed(qualifies: bool) -> impl Fn(&crate::config::AccountCfg) -> crate::signin::SignIn {
        move |a| crate::signin::SignIn { account: a.name.clone(), plan: Some(if qualifies { "team" } else { "max" }.into()), commercial: qualifies, verified: qualifies, as_of: Some(1), ..Default::default() }
    }

    #[test]
    fn account_check_says_the_sessions_account_its_sign_in_and_whether_its_in_the_group() {
        let cfg = boundary_cfg();
        let me = || (Some("claude:claude-work:k1".to_string()), Some("process"));
        let found = || Some(("claude-work".to_string(), "claude".to_string()));
        let c = account_check_for(&cfg, me(), found(), Some("authorized"), signed(true)).unwrap();
        assert!(c.found && c.in_group == Some(true) && c.groups == ["authorized"] && c.how.as_deref() == Some("process"));
        let c = account_check_for(&cfg, me(), found(), Some("authorized"), signed(false)).unwrap();
        assert_eq!(c.in_group, Some(false), "a member whose sign-in doesn't qualify is outside a boundary");
        assert_eq!(account_check_for(&cfg, me(), found(), None, signed(false)).unwrap().in_group, None);
        // By the variable alone the board doesn't have it yet: the key names the account.
        let c = account_check_for(&cfg, (Some("claude:claude:k2".into()), Some("environment")), None, Some("authorized"), signed(true)).unwrap();
        assert_eq!((c.account.as_deref(), c.harness.as_deref(), c.in_group), (Some("claude"), Some("claude"), Some(false)));
        let claimed = account_check_for(&cfg, (Some("claude:claude-work:k3".into()), Some("environment")), None, Some("authorized"), signed(true)).unwrap();
        assert_eq!((claimed.account.as_deref(), claimed.in_group), (Some("claude-work"), Some(false)), "the variable alone, which an agent can set, is in no group");
        let none = account_check_for(&cfg, (None, None), None, Some("authorized"), signed(true)).unwrap();
        assert!(!none.found && none.session.is_none());
        assert!(account_check_for(&cfg, me(), found(), Some("reviewers"), signed(true)).is_err(), "a group config.toml doesn't have");
    }

    /// The names a repository's guard hook reads, as the contract it's built against has them.
    #[test]
    fn account_check_writes_the_contracts_names() {
        let cfg = boundary_cfg();
        let c = account_check_for(&cfg, (Some("claude:claude-work:k1".into()), Some("process")), Some(("claude-work".into(), "claude".into())), Some("authorized"), signed(true)).unwrap();
        let v = serde_json::to_value(&c).unwrap();
        for key in ["found", "how", "session", "account", "harness", "signIn", "groups", "inGroup"] {
            assert!(v.get(key).is_some(), "{key}");
        }
        for key in ["email", "organization", "organizationName", "plan", "commercial", "verified", "asOf"] {
            assert!(v["signIn"].get(key).is_some(), "signIn.{key}");
        }
        assert_eq!(check_text(&c, Some("authorized")).lines().last(), Some("In group authorized."));
    }
}

//! Boundary groups: a routing group (`[[routing.groups]]` with `boundary = true`) whose members'
//! sessions keep what they say among themselves. It's for a team that lets agents on accounts
//! under a commercial agreement (a Claude Team or Enterprise seat, a ChatGPT Business workspace)
//! read what personal subscriptions may not, such as production data: the repository's own guard
//! hook asks `codealong account check --group <name>` whether the session running a command is in
//! the group, and Codealong keeps what those sessions say from reaching sessions outside it.
//!
//! - Who's in it: the accounts it lists, each only while its provider says its sign-in is on a
//!   commercial plan (signin.rs `SignIn::qualifies`). A member whose sign-in doesn't qualify is
//!   outside: it may not read the group's sessions, and the hook check says no. Its own sessions
//!   are still the group's to keep, since it may have read what they hold while it qualified.
//! - What's kept in: a session's text (its prompts, replies, tool calls, excerpts, what a search
//!   found in it, Jev's notes on it, suggested replies), from a session on an account outside the
//!   group and from an orchestrator that runs outside it. They get its metadata: title, status,
//!   worktree, branch, PR and todos (`veil`). A member's message to a session outside isn't sent
//!   (send.rs `leaves_boundary`): the agent is told to ask its user, who can send it from the
//!   windows, which show everything: they're the person's.
//! - Which decision model may read its sessions (`decisions`): Jev unless the group says `none`.
//!   An absent `decisions` stays Jev even once other decision models come, so a change of the
//!   default never sends a boundary's sessions to another provider.
//!
//! It holds Codealong's own ways between sessions, not the operating system's: an agent that can
//! read another account's folder on disk reads its transcripts there. A guard hook that stops an
//! outside agent from reading the members' folders closes that.

use crate::config::{Config, RoutingGroupCfg};
use crate::model::Session;

/// What a session outside a boundary group is told when it asks for what one of its sessions said.
pub const REFUSED: &str = "That session is in a boundary group the session asking isn't in (or whose sign-in doesn't qualify), so what it said stays there: its title, status, worktree, PR and todos are there to see (`codealong sessions`).";

/// The account a session's key names (`<harness>:<account>:<id>`), for a session the board
/// doesn't list yet.
pub fn account_of_key(key: &str) -> Option<&str> {
    let mut parts = key.splitn(3, ':');
    let (_, account, _) = (parts.next()?, parts.next()?, parts.next()?);
    Some(account).filter(|a| !a.is_empty())
}

/// What a boundary group's `decisions` takes, until other decision models come.
pub const DECISION_MODELS: [&str; 2] = ["jev", "none"];

/// The boundary group an account is listed in (config.rs lets it be in one).
pub fn group_of<'a>(cfg: &'a Config, account: &str) -> Option<&'a RoutingGroupCfg> {
    cfg.routing.groups.iter().find(|g| g.boundary && lists(g, account))
}

fn lists(g: &RoutingGroupCfg, account: &str) -> bool {
    g.members.iter().any(|m| m.account.trim() == account)
}

/// Whether an account is in a group: one of its members, or for a group with none listed, any
/// enabled account. For a boundary, only while its sign-in `qualifies`.
pub fn in_group(cfg: &Config, g: &RoutingGroupCfg, account: &str, qualifies: bool) -> bool {
    // A boundary that lists nobody lets nobody in, though config check turns one down.
    let member = if g.members.is_empty() { !g.boundary && cfg.accounts.iter().any(|a| a.name == account && !a.disabled) } else { lists(g, account) };
    member && (!g.boundary || qualifies)
}

/// The names of the groups an account is in, in config order: a boundary only while its sign-in
/// `qualifies`.
pub fn groups_of(cfg: &Config, account: &str, qualifies: bool) -> Vec<String> {
    cfg.routing.groups.iter().filter(|g| in_group(cfg, g, account, qualifies)).map(|g| g.name.trim().to_string()).collect()
}


/// Who asks for a session's text: the account of the session that asked, and whether its
/// sign-in qualifies for a boundary. None is a person (a terminal no session runs, the windows).
#[derive(Clone, Debug, PartialEq)]
pub struct Reader {
    pub account: String,
    pub qualifies: bool,
}

/// Whether `reader` may have what a session on `account` says.
pub fn may_read(cfg: &Config, reader: Option<&Reader>, account: &str) -> bool {
    let Some(g) = group_of(cfg, account) else { return true };
    let Some(r) = reader else { return true };
    r.qualifies && lists(g, &r.account)
}

/// Whether a session on `from` may send to one on `to` without the person: text from inside a
/// boundary may go only to a session inside it.
pub fn may_send(cfg: &Config, from: &str, to: &str, to_qualifies: bool) -> bool {
    group_of(cfg, from).is_none_or(|g| lists(g, to) && to_qualifies)
}

/// What `config check` says of the boundary groups' members, given how each one's sign-in
/// stands: a member whose sign-in doesn't qualify is outside its group until it does.
pub fn warnings(cfg: &Config, sign_in: impl Fn(&crate::config::AccountCfg) -> crate::signin::SignIn) -> Vec<String> {
    let mut out = Vec::new();
    for g in cfg.routing.groups.iter().filter(|g| g.boundary) {
        for m in &g.members {
            let Some(a) = cfg.accounts.iter().find(|a| a.name == m.account.trim()) else { continue };
            let s = sign_in(a);
            let (name, group) = (&a.name, g.name.trim());
            let plan = s.plan.as_deref().map_or("a plan Codealong can't read".to_string(), |p| format!("a {p} plan"));
            if a.disabled {
                out.push(format!("{name} is in boundary group {group} but turned off."));
            } else if !s.commercial {
                out.push(format!("{name} is in boundary group {group} but signed in on {plan}, which isn't under its provider's commercial terms: it's outside the group until it is."));
            } else if !s.verified {
                out.push(format!("{name} is in boundary group {group}, and its files say {plan}, but its provider didn't confirm it ({}): it's outside the group until it does.", s.note.as_deref().unwrap_or("not asked").trim_end_matches('.')));
            }
        }
    }
    out
}

/// Whether a group that names `account` (or is one) keeps every session it may start inside
/// the boundary of `inside`: a start on it can't land on an account outside.
pub fn stays_inside(cfg: &Config, inside: &RoutingGroupCfg, account_or_group: &str) -> bool {
    match cfg.routing.groups.iter().find(|g| g.name.trim() == account_or_group) {
        Some(g) => !g.members.is_empty() && g.members.iter().all(|m| lists(inside, m.account.trim())),
        None => lists(inside, account_or_group),
    }
}

/// Whether a session started on `account_or_group` (an orchestrator's, a schedule's) may read
/// what a session on `account` says, wherever among the group's members it lands: each must be in
/// the boundary and qualify. Auto (empty) and a group of every account may land outside.
pub fn start_may_read(cfg: &Config, account_or_group: &str, account: &str, qualifies: impl Fn(&str) -> bool) -> bool {
    let Some(g) = group_of(cfg, account) else { return true };
    let on = account_or_group.trim();
    let members: Vec<&str> = match cfg.routing.groups.iter().find(|x| x.name.trim() == on) {
        Some(x) if x.members.is_empty() => return false,
        Some(x) => x.members.iter().map(|m| m.account.trim()).collect(),
        None if on.is_empty() => return false,
        None => vec![on],
    };
    members.iter().all(|m| lists(g, m) && qualifies(m))
}

/// The decision model that may read a session on `account`'s text: `jev`, or `none` for none.
pub fn decision_model<'a>(cfg: &'a Config, account: &str) -> &'a str {
    group_of(cfg, account).and_then(|g| g.decisions.as_deref()).unwrap_or("jev")
}

/// Whether Jev may read what a session on `account` says.
pub fn jev_reads(cfg: &Config, account: &str) -> bool {
    decision_model(cfg, account) == "jev"
}

/// A session as a reader outside its boundary has it: its metadata, none of what was said. The
/// headline is its title, as a headline's words come from what it said.
pub fn veil(s: &mut Session) {
    s.reply_tail = None;
    s.first_prompt = None;
    s.last_prompt = None;
    s.reply_head = None;
    s.excerpt = None;
    s.detail = None;
    s.headline = s.title.clone();
    s.background = s.background.as_ref().map(|_| String::new());
    s.agents = s.agents.iter().map(|_| String::new()).collect();
    s.tasks.clear();
    s.queued.clear();
    s.suggestions = None;
    s.side_answer = None;
    if let Some(i) = s.insight.as_mut() {
        i.note = None;
        i.background = None;
    }
    if let Some(r) = s.raised.as_mut() {
        r.note = None;
    }
    if let Some(a) = s.agent_seen.as_mut() {
        a.note = None;
    }
}

/// Veils, on a board a reader reads, every session it may not read, and says which.
pub fn veil_lanes(cfg: &Config, reader: Option<&Reader>, lanes: &mut [crate::model::Lane]) -> Vec<String> {
    let mut keys = Vec::new();
    if !cfg.routing.groups.iter().any(|g| g.boundary) {
        return keys;
    }
    for s in lanes.iter_mut().flat_map(|l| l.sessions.iter_mut()) {
        if !may_read(cfg, reader, &s.account) {
            veil(s);
            keys.push(s.key.clone());
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GroupMemberCfg;

    fn cfg() -> Config {
        let mut cfg = Config::default();
        let member = |a: &str| GroupMemberCfg { account: a.into(), model: String::new(), effort: String::new() };
        cfg.routing.groups = vec![
            RoutingGroupCfg { name: "authorized".into(), members: vec![member("claude-work"), member("codex-work")], boundary: true, decisions: Some("none".into()), ..RoutingGroupCfg::default() },
            RoutingGroupCfg { name: "reviewers".into(), members: vec![member("claude"), member("claude-work")], ..RoutingGroupCfg::default() },
            RoutingGroupCfg { name: "work-only".into(), members: vec![member("codex-work")], ..RoutingGroupCfg::default() },
        ];
        cfg
    }

    fn reader(account: &str, qualifies: bool) -> Reader {
        Reader { account: account.into(), qualifies }
    }

    #[test]
    fn a_members_text_stays_with_qualifying_members() {
        let cfg = cfg();
        assert!(may_read(&cfg, Some(&reader("codex-work", true)), "claude-work"));
        assert!(!may_read(&cfg, Some(&reader("claude", true)), "claude-work"), "a personal account outside");
        assert!(!may_read(&cfg, Some(&reader("codex-work", false)), "claude-work"), "a member whose sign-in doesn't qualify");
        assert!(may_read(&cfg, None, "claude-work"), "the person");
        assert!(may_read(&cfg, Some(&reader("claude", false)), "claude"), "outside any boundary, anyone reads");
    }

    #[test]
    fn a_boundary_member_is_in_only_while_it_qualifies() {
        let cfg = cfg();
        let g = &cfg.routing.groups[0];
        assert!(in_group(&cfg, g, "claude-work", true));
        assert!(!in_group(&cfg, g, "claude-work", false));
        assert!(in_group(&cfg, &cfg.routing.groups[1], "claude-work", false), "not a boundary: membership alone");
        assert_eq!(groups_of(&cfg, "claude-work", true), ["authorized", "reviewers"]);
        assert_eq!(groups_of(&cfg, "claude-work", false), ["reviewers"], "a boundary only while it qualifies");
        let mut empty = cfg.clone();
        empty.routing.groups[0].members.clear();
        assert!(!in_group(&empty, &empty.routing.groups[0], "claude-work", true), "a boundary that lists nobody lets nobody in");
    }

    #[test]
    fn sending_out_of_a_boundary_is_held_and_in_isnt() {
        let cfg = cfg();
        assert!(!may_send(&cfg, "claude-work", "claude", true));
        assert!(may_send(&cfg, "claude", "claude-work", true), "into a boundary");
        assert!(may_send(&cfg, "claude-work", "codex-work", true));
        assert!(!may_send(&cfg, "claude-work", "codex-work", false), "to a member that doesn't qualify");
    }

    #[test]
    fn a_start_stays_inside_only_on_members_or_a_group_of_them() {
        let cfg = cfg();
        let g = &cfg.routing.groups[0];
        assert!(stays_inside(&cfg, g, "authorized") && stays_inside(&cfg, g, "work-only") && stays_inside(&cfg, g, "codex-work"));
        assert!(!stays_inside(&cfg, g, "reviewers") && !stays_inside(&cfg, g, "claude"));
    }

    #[test]
    fn an_orchestrator_reads_a_boundarys_sessions_only_where_it_cant_land_outside() {
        let cfg = cfg();
        let yes = |_: &str| true;
        assert!(start_may_read(&cfg, "authorized", "claude-work", yes));
        assert!(start_may_read(&cfg, "codex-work", "claude-work", yes));
        assert!(!start_may_read(&cfg, "reviewers", "claude-work", yes), "it may land on claude");
        assert!(!start_may_read(&cfg, "", "claude-work", yes), "Auto may land anywhere");
        assert!(!start_may_read(&cfg, "authorized", "claude-work", |a| a != "codex-work"), "a member that doesn't qualify");
        assert!(start_may_read(&cfg, "claude", "claude", |_| false), "outside any boundary");
    }

    #[test]
    fn a_key_names_its_account() {
        assert_eq!(account_of_key("claude:claude-work:k1"), Some("claude-work"));
        assert_eq!(account_of_key("claude-work"), None);
    }

    #[test]
    fn the_decision_model_is_jev_unless_the_boundary_says_otherwise() {
        let mut cfg = cfg();
        assert_eq!((decision_model(&cfg, "claude-work"), decision_model(&cfg, "claude")), ("none", "jev"));
        cfg.routing.groups[0].decisions = None;
        assert!(jev_reads(&cfg, "claude-work"));
    }

    #[test]
    fn config_check_names_members_whose_sign_in_doesnt_qualify() {
        let mut cfg = cfg();
        let account = |name: &str| crate::config::AccountCfg { name: name.into(), harness: "claude".into(), home: format!("~/.{name}"), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        cfg.accounts = vec![account("claude-work"), account("codex-work")];
        let said = |a: &crate::config::AccountCfg| match a.name.as_str() {
            "claude-work" => crate::signin::SignIn { account: a.name.clone(), plan: Some("team".into()), commercial: true, verified: true, ..Default::default() },
            _ => crate::signin::SignIn { account: a.name.clone(), plan: Some("pro".into()), commercial: false, verified: true, ..Default::default() },
        };
        let w = warnings(&cfg, said);
        assert_eq!(w.len(), 1);
        assert!(w[0].starts_with("codex-work is in boundary group authorized but signed in on a pro plan"));
    }

    #[test]
    fn a_veiled_session_keeps_its_metadata_and_says_nothing() {
        let mut s = Session::new(crate::model::Harness::Claude, "claude-work", "k1");
        s.title = "Atlas billing export".into();
        s.excerpt = Some("SELECT * FROM customers".into());
        s.reply_tail = Some("the rows say".into());
        s.headline = "Asks: which customers?".into();
        s.status = crate::model::Status::Needs;
        veil(&mut s);
        assert_eq!((s.excerpt, s.reply_tail, s.headline.as_str(), s.status), (None, None, "Atlas billing export", crate::model::Status::Needs));
    }
}

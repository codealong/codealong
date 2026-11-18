//! Which subscription an account is signed in to: its provider's organization or workspace, the
//! plan, and whether that plan is under the provider's commercial terms (a Claude Team or
//! Enterprise seat, a ChatGPT Business or Enterprise workspace), as against a personal one. A
//! repository's guard hook asks it through `codealong account check` before every command an agent
//! runs, and boundary groups (boundary.rs) let only a commercial sign-in in.
//!
//! Each harness says it two ways (`Adapter::local_sign_in`, `Adapter::verify_sign_in`): from the
//! account's own files, which anything on the Mac can write and which can be out of date, and
//! from its provider, asked with the account's own sign-in, which is the answer that counts
//! (`verified`). Asking the provider takes a second or so, and a hook asks before every command,
//! so a provider's answer is kept for `FRESH_MS` while the files say the same sign-in
//! (`Local::identity`): a sign-in to another organization in the same folder is asked about again
//! at once. One that couldn't be asked again keeps an answer for the same sign-in up to `KEEP_MS`.
//! Unknown is never commercial: a harness whose plan Codealong can't read, a folder signed out,
//! a plan by a name Codealong doesn't know.
//!
//! The cache is the process's: the server's, which every command asks while it runs, and which
//! the engine keeps warm for the accounts in boundary groups (`Engine::read_sign_ins`), so a hook
//! rarely waits on the provider.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::AccountCfg;

/// How long a provider's answer stands while the account's files say the same sign-in.
pub const FRESH_MS: i64 = 10 * 60 * 1000;
/// How long one stands when the provider can't be asked again (offline, rate limited).
pub const KEEP_MS: i64 = 60 * 60 * 1000;

/// What an account's sign-in is, as its files or its provider say it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Said {
    pub email: Option<String>,
    /// The provider's organization or workspace id.
    pub organization: Option<String>,
    pub organization_name: Option<String>,
    /// The provider's own name for the plan: team, enterprise, max, pro, business…
    pub plan: Option<String>,
    /// The plan is under the provider's commercial terms.
    pub commercial: bool,
}

/// What an account's files say, with what tells one sign-in from another: a sign-in to another
/// organization or as another person in the same folder has another `identity`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Local {
    pub said: Said,
    pub identity: String,
}

/// Why a provider didn't say what an account's sign-in is.
#[derive(Clone, Debug, PartialEq)]
pub enum NotSaid {
    /// It turned the sign-in down (expired, signed out, the account not among its own): an
    /// earlier answer for it no longer stands.
    Refused(String),
    /// It couldn't be asked (offline, rate limited, a server error): an earlier answer for the
    /// same sign-in stands a while (`KEEP_MS`).
    Unreached(String),
}

impl NotSaid {
    fn why(&self) -> &str {
        match self {
            NotSaid::Refused(w) | NotSaid::Unreached(w) => w,
        }
    }
}

/// Where an account's sign-in stands, for `account check`, boundary groups and Settings.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SignIn {
    pub account: String,
    pub email: Option<String>,
    pub organization: Option<String>,
    pub organization_name: Option<String>,
    pub plan: Option<String>,
    pub commercial: bool,
    /// Read from the provider with the account's own sign-in; false when only its files answered.
    pub verified: bool,
    /// When the provider said it, or when the files were read.
    pub as_of: Option<i64>,
    /// Why it isn't verified, or why an older answer is kept, in words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl SignIn {
    fn of(account: &AccountCfg, said: &Said, verified: bool, as_of: i64, note: Option<String>) -> SignIn {
        SignIn {
            account: account.name.clone(),
            email: said.email.clone(),
            organization: said.organization.clone(),
            organization_name: said.organization_name.clone(),
            plan: said.plan.clone(),
            commercial: said.commercial,
            verified,
            as_of: Some(as_of),
            note,
        }
    }

    fn unknown(account: &AccountCfg, note: String) -> SignIn {
        SignIn { account: account.name.clone(), note: Some(note), ..SignIn::default() }
    }

    /// The sign-in may be let into a boundary group: its provider says it's commercial.
    pub fn qualifies(&self) -> bool {
        self.commercial && self.verified
    }
}

/// Whether `current` may ask the provider.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ask {
    /// When no answer for this sign-in is fresh: a check that must be right now.
    IfStale,
    /// Never: the files and what's kept. For a thread that mustn't wait on the network.
    Never,
    /// When the answer kept is half as old as `FRESH_MS`: the engine's poll, so a check rarely
    /// finds it stale.
    Warm,
}

/// A provider's answer, kept for the sign-in it was for.
#[derive(Clone, Debug)]
struct Kept {
    identity: String,
    said: Said,
    at: i64,
}

/// Kept answers by account (`accountpoll::identity`: its harness and folder).
static KEPT: Mutex<Option<HashMap<String, Kept>>> = Mutex::new(None);

fn kept(account: &AccountCfg) -> Option<Kept> {
    KEPT.lock().ok()?.as_ref()?.get(&crate::accountpoll::identity(account)).cloned()
}

fn keep(account: &AccountCfg, k: Kept) {
    if let Ok(mut map) = KEPT.lock() {
        map.get_or_insert_with(HashMap::new).insert(crate::accountpoll::identity(account), k);
    }
}

/// Where an account's sign-in stands now: the provider's answer for the sign-in its files show,
/// asked again when it's older than `FRESH_MS` and `ask` lets it, else the files alone.
pub fn current(account: &AccountCfg, now: i64, ask: Ask) -> SignIn {
    let Some(adapter) = crate::harness::of(account) else {
        return SignIn::unknown(account, "Codealong doesn't know this account's harness.".into());
    };
    let Some(local) = adapter.local_sign_in(account) else {
        return SignIn::unknown(account, format!("Codealong can't read who {} is signed in as.", account.name));
    };
    let same = kept(account).filter(|k| k.identity == local.identity);
    current_from(account, &local, same, now, ask, || adapter.verify_sign_in(account))
}

/// `current`, asking the provider for no longer than `wait`: past it, what's kept or the files
/// answer, verified only if what's kept still stands, and the ask goes on by itself, so the next
/// check finds it. For a guard hook's check, which has seconds in all.
pub fn current_within(account: &AccountCfg, now: i64, wait: std::time::Duration) -> SignIn {
    // One ask of a provider at a time for an account: a check that comes while one is out
    // answers with what's kept, rather than start another.
    let id = crate::accountpoll::identity(account);
    let first = !wait.is_zero() && ASKING.lock().is_ok_and(|mut a| a.get_or_insert_with(std::collections::HashSet::new).insert(id.clone()));
    if !first {
        return current(account, now, Ask::Never);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let asked = account.clone();
    let spawned = std::thread::Builder::new().name("codealong-sign-in".into()).spawn(move || {
        let _ = tx.send(current(&asked, now, Ask::IfStale));
        if let Ok(mut a) = ASKING.lock() {
            a.get_or_insert_with(std::collections::HashSet::new).remove(&crate::accountpoll::identity(&asked));
        }
    });
    match spawned {
        Ok(_) => rx.recv_timeout(wait).unwrap_or_else(|_| current(account, now, Ask::Never)),
        Err(_) => {
            if let Ok(mut a) = ASKING.lock() {
                a.get_or_insert_with(std::collections::HashSet::new).remove(&id);
            }
            current(account, now, Ask::Never)
        }
    }
}

/// The accounts whose provider `current_within` is asking now.
static ASKING: Mutex<Option<std::collections::HashSet<String>>> = Mutex::new(None);

/// `current`'s rules, given the files' answer, what's kept for the same sign-in, and how to ask
/// the provider.
fn current_from(account: &AccountCfg, local: &Local, same: Option<Kept>, now: i64, ask: Ask, verify: impl FnOnce() -> Result<Said, NotSaid>) -> SignIn {
    let fresh = if ask == Ask::Warm { FRESH_MS / 2 } else { FRESH_MS };
    if let Some(k) = same.as_ref().filter(|k| now - k.at < fresh) {
        return SignIn::of(account, &k.said, true, k.at, None);
    }
    let why = match ask {
        Ask::Never => "not asked of its provider yet".to_string(),
        Ask::IfStale | Ask::Warm => match verify() {
            Ok(said) => {
                keep(account, Kept { identity: local.identity.clone(), said: said.clone(), at: now });
                return SignIn::of(account, &said, true, now, None);
            }
            Err(NotSaid::Refused(why)) => {
                forget_kept(account);
                return SignIn::of(account, &local.said, false, now, Some(format!("Its provider turned the sign-in down: {why}.")));
            }
            Err(e) => e.why().to_string(),
        },
    };
    match same.filter(|k| now - k.at < KEEP_MS) {
        Some(k) => SignIn::of(account, &k.said, true, k.at, Some(format!("Its provider's answer from before: asking again failed ({why})."))),
        None => SignIn::of(account, &local.said, false, now, Some(format!("From its files only: {why}."))),
    }
}

/// The sign-ins of the accounts in boundary groups, which the engine keeps fresh off its thread
/// every `POLL_EVERY` (`Ask::Warm`), for the board (Settings says a member whose sign-in no longer
/// qualifies) and so a guard hook's check rarely waits on a provider.
#[derive(Default)]
pub struct SignInPoll {
    list: Vec<SignIn>,
    running: bool,
    last: Option<std::time::Instant>,
}

/// How often the engine reads the boundary members' sign-ins.
pub const POLL_EVERY: std::time::Duration = std::time::Duration::from_secs(120);

impl SignInPoll {
    /// The accounts to read now: every boundary group's members that are on, once `POLL_EVERY`
    /// has passed since the last read began, and none while one is out. Marks a read out.
    pub fn due(&mut self, cfg: &crate::config::Config, now: std::time::Instant) -> Vec<AccountCfg> {
        if self.running || self.last.is_some_and(|t| now.duration_since(t) < POLL_EVERY) {
            return Vec::new();
        }
        let asks: Vec<AccountCfg> = cfg.accounts.iter().filter(|a| !a.disabled && crate::boundary::group_of(cfg, &a.name).is_some()).cloned().collect();
        if !asks.is_empty() {
            self.running = true;
            self.last = Some(now);
        } else {
            self.list.clear();
        }
        asks
    }

    /// The read is over, however it ended. Says whether the list changed.
    pub fn on_read(&mut self, list: Option<Vec<SignIn>>) -> bool {
        self.running = false;
        let Some(list) = list else { return false };
        // Times move on every read: only what's said counts as a change.
        let said = |l: &[SignIn]| l.iter().map(|s| (s.account.clone(), s.organization.clone(), s.plan.clone(), s.commercial, s.verified)).collect::<Vec<_>>();
        let changed = said(&list) != said(&self.list);
        self.list = list;
        changed
    }

    /// The members' sign-ins, for the board, in config order; none for an account no boundary
    /// lists any more.
    pub fn list(&self, cfg: &crate::config::Config) -> Vec<SignIn> {
        self.list.iter().filter(|s| crate::boundary::group_of(cfg, &s.account).is_some()).cloned().collect()
    }
}

/// Forgets what's kept for an account: its provider turned the sign-in down.
fn forget_kept(account: &AccountCfg) {
    if let Ok(mut map) = KEPT.lock() {
        if let Some(map) = map.as_mut() {
            map.remove(&crate::accountpoll::identity(account));
        }
    }
}

/// Forgets what's kept for an account, for a test.
#[cfg(test)]
fn forget(account: &AccountCfg) {
    if let Ok(mut map) = KEPT.lock() {
        if let Some(map) = map.as_mut() {
            map.remove(&crate::accountpoll::identity(account));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(name: &str) -> AccountCfg {
        AccountCfg { name: name.into(), harness: "claude".into(), home: format!("~/.signin-test-{name}"), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None }
    }

    fn said(org: &str, plan: &str, commercial: bool) -> Said {
        Said { email: Some("me@northwind.test".into()), organization: Some(org.into()), organization_name: Some("Northwind".into()), plan: Some(plan.into()), commercial }
    }

    fn local(org: &str, plan: &str, commercial: bool) -> Local {
        Local { said: said(org, plan, commercial), identity: format!("{org}/{plan}") }
    }

    #[test]
    fn the_providers_answer_is_kept_for_the_same_sign_in() {
        let a = account("kept");
        forget(&a);
        let files = local("org-a", "team", true);
        let first = current_from(&a, &files, kept(&a), 1_000, Ask::IfStale, || Ok(said("org-a", "team", true)));
        assert!(first.verified && first.commercial && first.qualifies());
        // Asked again within FRESH_MS: the provider isn't.
        let again = current_from(&a, &files, kept(&a).filter(|k| k.identity == files.identity), 2_000, Ask::IfStale, || panic!("asked again"));
        assert_eq!((again.verified, again.as_of), (true, Some(1_000)));
        forget(&a);
    }

    #[test]
    fn a_sign_in_to_another_organization_is_asked_about_at_once() {
        let a = account("moved");
        forget(&a);
        let team = local("org-a", "team", true);
        current_from(&a, &team, None, 1_000, Ask::IfStale, || Ok(said("org-a", "team", true)));
        // The files now say another organization: what's kept was for another sign-in.
        let personal = local("org-b", "max", false);
        let same = kept(&a).filter(|k| k.identity == personal.identity);
        assert!(same.is_none());
        let now = current_from(&a, &personal, same, 2_000, Ask::IfStale, || Ok(said("org-b", "max", false)));
        assert!(!now.commercial && now.verified && !now.qualifies());
        forget(&a);
    }

    #[test]
    fn files_alone_are_never_verified_and_a_failed_ask_keeps_an_answer_an_hour() {
        let a = account("offline");
        forget(&a);
        let files = local("org-a", "team", true);
        let unasked = current_from(&a, &files, None, 1_000, Ask::Never, || panic!("asked"));
        assert!(unasked.commercial && !unasked.verified && !unasked.qualifies(), "files say team, but nobody checked");
        let failed = current_from(&a, &files, None, 1_000, Ask::IfStale, || Err(NotSaid::Unreached("offline".into())));
        assert!(!failed.verified && failed.note.as_deref().unwrap().contains("offline"));
        current_from(&a, &files, None, 1_000, Ask::IfStale, || Ok(said("org-a", "team", true)));
        let later = 1_000 + FRESH_MS + 1;
        let stale = current_from(&a, &files, kept(&a), later, Ask::IfStale, || Err(NotSaid::Unreached("offline".into())));
        assert!(stale.verified && stale.as_of == Some(1_000) && stale.note.is_some());
        let gone = current_from(&a, &files, kept(&a), 1_000 + KEEP_MS, Ask::IfStale, || Err(NotSaid::Unreached("offline".into())));
        assert!(!gone.verified);
        forget(&a);
    }

    #[test]
    fn a_sign_in_its_provider_turns_down_is_unverified_at_once() {
        let a = account("refused");
        forget(&a);
        let files = local("org-a", "team", true);
        current_from(&a, &files, None, 1_000, Ask::IfStale, || Ok(said("org-a", "team", true)));
        let later = 1_000 + FRESH_MS + 1;
        let refused = current_from(&a, &files, kept(&a), later, Ask::IfStale, || Err(NotSaid::Refused("its sign-in has expired".into())));
        assert!(!refused.verified && !refused.qualifies(), "an answer from before doesn't stand for a sign-in turned down");
        assert!(kept(&a).is_none());
        forget(&a);
    }

    #[test]
    fn the_engine_warms_an_answer_before_a_check_finds_it_stale() {
        let a = account("warm");
        forget(&a);
        let files = local("org-a", "team", true);
        current_from(&a, &files, None, 1_000, Ask::IfStale, || Ok(said("org-a", "team", true)));
        let half = 1_000 + FRESH_MS / 2;
        let check = current_from(&a, &files, kept(&a), half, Ask::IfStale, || panic!("a check asks only once it's stale"));
        assert_eq!(check.as_of, Some(1_000));
        let warmed = current_from(&a, &files, kept(&a), half, Ask::Warm, || Ok(said("org-a", "team", true)));
        assert_eq!(warmed.as_of, Some(half));
        forget(&a);
    }

    #[test]
    fn only_boundary_members_are_polled_every_two_minutes() {
        use crate::config::{Config, GroupMemberCfg, RoutingGroupCfg};
        let mut cfg = Config { accounts: vec![account("inside"), account("outside")], ..Config::default() };
        cfg.routing.groups = vec![RoutingGroupCfg { name: "authorized".into(), boundary: true, members: vec![GroupMemberCfg { account: "inside".into(), model: String::new(), effort: String::new() }], ..RoutingGroupCfg::default() }];
        let mut poll = SignInPoll::default();
        let t0 = std::time::Instant::now();
        let names = |v: Vec<AccountCfg>| v.into_iter().map(|a| a.name).collect::<Vec<_>>();
        assert_eq!(names(poll.due(&cfg, t0)), ["inside"]);
        assert!(poll.due(&cfg, t0).is_empty(), "nothing while a read is out");
        let read = SignIn { account: "inside".into(), commercial: true, verified: true, ..SignIn::default() };
        assert!(poll.on_read(Some(vec![read.clone()])));
        assert!(!poll.on_read(Some(vec![SignIn { as_of: Some(5), ..read }])), "a later time alone is no change");
        assert!(poll.due(&cfg, t0 + POLL_EVERY / 2).is_empty());
        assert_eq!(poll.due(&cfg, t0 + POLL_EVERY).len(), 1);
        cfg.routing.groups.clear();
        assert!(poll.list(&cfg).is_empty(), "no boundary lists it any more");
    }

    #[test]
    fn a_provider_that_says_otherwise_than_the_files_wins() {
        let a = account("forged");
        forget(&a);
        let files = local("org-a", "enterprise", true);
        let asked = current_from(&a, &files, None, 1_000, Ask::IfStale, || Ok(said("org-a", "pro", false)));
        assert!(!asked.commercial && asked.verified);
        forget(&a);
    }
}

//! What a lane and a session say and where a lane opens. A lane's bay, by the first rule that
//! matches (`bay_of`), and its one sentence, from the rule that picked the bay (`headline`); a
//! session's own sentence (`session_headline`); and everywhere a lane can open, the default
//! first (`targets`), with its app's URL and server (`lane_preview`) and a session's Chrome
//! item.
//!
//! Pure functions over sessions and a PR already read. src/lanes.ts works the bay and the
//! headlines out again for the windows, each rule there a copy of one here kept in step by
//! hand, and both are held to the cases `rule_cases` composes (board.rs's tests), checked in
//! as src/testing/ruleCases.json. So after a change to `bay_of`, `headline` or
//! `session_headline`, write the cases again first, from src-tauri/
//! (`CODEALONG_WRITE_FIXTURES=1 cargo test rule_cases`); then `npm test rules` says which rule
//! of lanes.ts no longer agrees. `npm test rules` alone reads the cases as they were, and
//! passes on the old behaviour. A rule's test goes in `tests` below.

use super::*;

/// The first rule that matches. See *The model*, Lane status. Needs you, Working and Your turn
/// are about the lane's agents; a PR that needs work (failing checks, changes requested) keeps
/// it in In review, where the Inbox's PR needs work finds it.
pub fn bay_of(sessions: &[Session], pr: Option<&Pr>) -> (Bay, Status) {
    let has = |st: Status| sessions.iter().any(|s| s.status == st);
    if has(Status::Needs) {
        return (Bay::Needs, Status::Needs);
    }
    if has(Status::Error) {
        return (Bay::Needs, Status::Error);
    }
    if has(Status::Working) {
        return (Bay::Working, Status::Working);
    }
    if has(Status::Unread) {
        return (Bay::Unread, Status::Unread);
    }
    match pr {
        Some(pr) if pr.state.is_open() => (Bay::Review, Status::Idle),
        Some(_) => (Bay::Merged, Status::Idle),
        None => (Bay::Idle, Status::Idle),
    }
}

pub fn checks_summary(c: &Checks) -> String {
    let mut parts = Vec::new();
    if c.failed > 0 {
        parts.push(format!("{} failing", c.failed));
    }
    if c.pending > 0 {
        parts.push(format!("{} running", c.pending));
    }
    if c.passed > 0 {
        parts.push(format!("{} passed", c.passed));
    }
    if parts.is_empty() {
        return "No checks".to_string();
    }
    if parts.len() == 1 && c.passed > 0 {
        return format!("{} checks passed", c.passed);
    }
    parts.join(" · ")
}

/// A session's own sentence. See *The model*, Headlines.
pub fn session_headline(s: &Session) -> (String, Option<String>) {
    let acct = code(&s.account);
    if s.need() == Some(Need::Stuck) {
        return (format!("{acct} is stuck · {}", s.title), s.excerpt.clone());
    }
    let title = &s.title;
    match s.status {
        // Every headline names the session: its title after what it's doing. A question, whether
        // its reply ends in one or it's held through the plugin (`claude::asked_label`), says so
        // up front and leaves the question itself below.
        // Set aside, it waits on its side conversation before it waits on you (`apply_aside`).
        Status::Needs if s.aside.is_some() => (format!("{acct} waits on its side conversation · {title}"), s.excerpt.as_deref().and_then(|e| e.strip_prefix(ASKING_ON_THE_SIDE)).map(str::to_string)),
        Status::Needs => match (s.need(), s.harness) {
            (Some(Need::Reply | Need::Question), _) => (format!("{acct} asks you · {title}"), s.excerpt.as_deref().and_then(|e| e.strip_prefix("Asks you · ")).map(str::to_string)),
            (Some(Need::Message), _) => (format!("{acct} holds a message for you · {title}"), s.excerpt.as_deref().and_then(|e| e.strip_prefix("Held message ")).map(str::to_string)),
            // A command it runs stopped at a question in its terminal: what it asks below.
            (Some(Need::Input), _) => (format!("{acct}'s command asks you · {title}"), s.excerpt.as_deref().and_then(|e| e.strip_prefix(TERMINAL_ASKS)).map(str::to_string)),
            // A helper it started waits in its terminal before it starts: what that shows below.
            (Some(Need::Start), _) => (format!("{acct}'s helper waits on you · {title}"), s.excerpt.as_deref().and_then(|e| e.strip_prefix(START_WAITS)).map(str::to_string)),
            (Some(Need::Approval), Harness::Claude) => (format!("{acct} is waiting for approval · {title}"), s.excerpt.clone()),
            (Some(Need::Approval), Harness::Codex | Harness::Opencode | Harness::Antigravity | Harness::Cursor) => (format!("{acct} asks for approval · {title}"), s.excerpt.clone()),
            (Some(Need::Stuck | Need::Other) | None, _) => (format!("{acct} needs you · {title}"), None),
        },
        Status::Error if s.limit.is_some() => (format!("{acct} hit its usage limit · {title}"), s.excerpt.clone()),
        Status::Error if s.cut_off.is_some() => (format!("{acct} was cut off · {title}"), s.excerpt.clone()),
        Status::Error => (format!("{acct} stopped with an error · {title}"), s.excerpt.clone()),
        Status::Working => {
            // How long it has worked on what you last asked. Claude Code doesn't restart a busy
            // session's clock for a message sent while it works, but that message is a new ask.
            let since = s.status_since.map(|t| t.max(s.last_prompt_at));
            let detail = match (since, &s.excerpt) {
                (Some(t), Some(ex)) => Some(format!("{{for:{t}}} · {ex}")),
                (Some(t), None) => Some(format!("{{for:{t}}}")),
                (None, ex) => ex.clone(),
            };
            (format!("{acct} · {}", s.title), detail)
        }
        // Its glyph and its bay (Your turn) already say it finished, and the word on every row
        // there was noise: the title stands alone, as a working session's does.
        Status::Unread => (format!("{acct} · {}", s.title), s.excerpt.clone()),
        // What it last said, as a finished turn's row says (an orchestrator's, what its last run
        // came to: Nothing new, `quiet_orchestrator`): when it was is the row's own time, and
        // "last turn 20m ago" beside "20m" only said it twice.
        Status::Idle | Status::Stopped => (format!("{acct} · {}", s.title), s.excerpt.clone()),
    }
}

/// The lane's one sentence, from the rule that picked its bay. See *The model*, Headlines.
pub fn headline(bay: Bay, sessions: &[Session], pr: Option<&Pr>, git: Option<&GitInfo>) -> (String, Option<String>) {
    let acct = |s: &Session| code(&s.account);
    let first = |st: Status| sessions.iter().find(|s| s.status == st);
    match bay {
        Bay::Needs => session_headline(first(Status::Needs).or_else(|| first(Status::Error)).expect("needs bay has an agent that needs you")),
        Bay::Working => session_headline(first(Status::Working).expect("working bay has a working session")),
        Bay::Unread => session_headline(first(Status::Unread).expect("unread bay has an unread session")),
        Bay::Review => {
            let pr = pr.expect("review bay has a PR");
            let c = &pr.checks;
            let n = pr.number;
            // Failing checks are what the PR needs from you first; the detail names them.
            if c.failed > 0 && !c.failing.is_empty() {
                let mut names: Vec<String> = c.failing.iter().take(3).map(|n| code(n)).collect();
                if c.failing.len() > 3 {
                    names.push(format!("+{} more", c.failing.len() - 3));
                }
                return (format!("#{n} · checks failing"), Some(names.join(" · ")));
            }
            // Ready to merge is GitHub's word for it: the reviews the repo requires, passing
            // checks and no conflicts. Until GitHub has worked that out, an approval is only that.
            let head = match pr.state {
                PrState::Draft => format!("#{n} is a draft"),
                _ if pr.merge == Merge::Conflicts => format!("#{n} has conflicts"),
                _ if pr.merge == Merge::Behind => format!("#{n} · branch out of date"),
                PrState::Changes => format!("#{n} · changes requested"),
                _ if c.failed > 0 => format!("#{n} · checks failing"),
                _ if c.pending > 0 && pr.state == PrState::Approved => format!("#{n} approved · checks running"),
                _ if c.pending > 0 => format!("#{n} · checks running"),
                PrState::Approved if pr.merge == Merge::Clean && pr.stale_approval.is_none() => format!("#{n} approved · ready to merge"),
                PrState::Approved => format!("#{n} approved"),
                _ if pr.merge == Merge::Clean => format!("#{n} ready to merge"),
                _ => format!("#{n} waiting for review"),
            };
            (head, Some(checks_summary(c)))
        }
        Bay::Merged => {
            let pr = pr.expect("merged bay has a PR");
            let (verb, when) = match pr.merged_at {
                Some(t) => ("merged", t),
                None => ("closed", pr.closed_at.unwrap_or(pr.updated_at)),
            };
            let detail = match sessions.iter().find(|s| s.live) {
                Some(s) => format!("{} is still open", acct(s)),
                None => "No agents running".to_string(),
            };
            (format!("#{} {verb} {{ago:{when}}}", pr.number), Some(detail))
        }
        Bay::Idle => {
            if let Some(s) = sessions.iter().max_by_key(|s| s.last_activity) {
                let (h, _) = session_headline(s);
                return (h, Some(format!("last turn {{ago:{}}}", s.last_activity)));
            }
            match git {
                Some(g) if g.dirty > 0 => ("Uncommitted changes".to_string(), Some(format!("{} files", g.dirty))),
                Some(g) => (
                    "No agents".to_string(),
                    g.last_commit.as_ref().map(|c| match g.last_commit_at {
                        Some(t) => format!("Last commit {{ago:{t}}} · {}", plain(&truncate(c, 70))),
                        None => plain(&truncate(c, 70)),
                    }),
                ),
                None => ("No agents".to_string(), None),
            }
        }
    }
}

pub fn target(id: String, icon: &str, label: String, detail: Option<String>, kbd: Option<&[&str]>, disabled: bool, group: &str) -> Target {
    Target {
        id,
        icon: icon.to_string(),
        label,
        detail,
        kbd: kbd.map(|k| k.iter().map(|s| s.to_string()).collect()),
        disabled,
        group: group.to_string(),
    }
}

/// The session a lane opens to: its most recent own session, else the most recent one that
/// visited it. A tie goes to the one listed first, the more urgent.
fn latest_session(sessions: &[Session]) -> Option<&Session> {
    let newest = |visitors: bool| sessions.iter().filter(|s| s.visitor.is_some() == visitors).min_by_key(|s| Reverse(s.last_activity));
    newest(false).or_else(|| newest(true))
}

/// Everywhere a lane can open, the default first. See *Surfaces*, Open where it lives.
/// A worktree's app: the URL set for it and the server that serves it. The server is the one on
/// the port given with the URL, or the port a local URL names; with neither, the one serving a
/// page (the lowest port, when several do). With no URL set, a page served in the worktree is
/// its app, at `localhost`. The session is the one whose command started the server: one of its
/// parent processes is the session's. It opens in the Chrome profile set for the worktree, or
/// else in its project's.
pub fn lane_preview(set: Option<&crate::previews::Entry>, servers: &[&crate::previews::Server], sessions: &[Session], project_profile: Option<&str>) -> Option<Preview> {
    let port = set.and_then(|e| e.port.or_else(|| crate::previews::local_port(&e.url)));
    let server = match port {
        Some(p) => servers.iter().find(|s| s.port == p),
        None => servers.iter().filter(|s| s.page).min_by_key(|s| s.port),
    };
    let server = server.map(|s| PreviewServer {
        pid: s.pid,
        port: s.port,
        name: s.name.clone(),
        title: s.title.clone(),
        started_at: s.started_ms,
        session: sessions.iter().find(|x| x.pid.is_some_and(|p| s.ancestors.contains(&p))).map(|x| x.key.clone()),
    });
    let own = set.and_then(|e| e.chrome_profile.clone());
    let chrome_profile_set = own.is_some();
    let chrome_profile = own.or_else(|| project_profile.map(str::to_string));
    match set {
        Some(e) => Some(Preview { url: e.url.clone(), set: true, port: e.port, server, chrome_profile, chrome_profile_set }),
        None => server.map(|s| Preview { url: format!("http://localhost:{}", s.port), set: false, port: None, server: Some(s), chrome_profile, chrome_profile_set }),
    }
}

/// The worktree on this Mac whose app a web link is to, when its preview opens in a Chrome
/// profile: the link opens there too, where the app has its sign-in, and any other link opens
/// in the default browser. Another device's previews open on that device, in its own profiles.
pub fn app_of_link<'a>(lanes: &'a [Lane], url: &str) -> Option<&'a Lane> {
    lanes.iter().filter(|l| l.device.is_none()).find(|l| {
        l.preview.as_ref().is_some_and(|p| p.chrome_profile.is_some() && crate::previews::is_app_link(url, &p.url, p.server.as_ref().map(|s| s.port).or(p.port)))
    })
}

/// The sessions on this Mac that made the page a web link is to (`Session.artifacts`), the one
/// that last did something with it first: its account's Chrome profile is where you're signed in
/// to see it (`engine::link_opens`). Its harness says which links are to it (`Adapter::same_page`:
/// a Claude artifact has two addresses). Another device's sessions are its accounts', not this
/// Mac's.
pub fn sessions_with_page<'a>(lanes: &'a [Lane], url: &str) -> Vec<&'a Session> {
    let mut found: Vec<(&Session, i64)> = lanes
        .iter()
        .filter(|l| l.device.is_none())
        .flat_map(|l| &l.sessions)
        .filter_map(|s| {
            let harness = crate::harness::adapter(s.harness);
            s.artifacts.iter().find(|a| harness.same_page(&a.url, url)).map(|a| (s, a.at))
        })
        .collect();
    found.sort_by_key(|(_, at)| Reverse(*at));
    found.into_iter().map(|(s, _)| s).collect()
}

/// What a session's Chrome item says: Show in Chrome while one of its tabs is open, else Reopen
/// with the site of its last page, and the profile when Chrome has more than one. `chromeLabel`
/// in `src/open.ts` says the same.
pub fn chrome_label(c: &ChromeTabs) -> String {
    let what = if c.tabs.is_empty() { format!("Reopen {} in Chrome", site(&c.url)) } else { "Show in Chrome".to_string() };
    match &c.profile {
        Some(p) => format!("{what} ({p})"),
        None => what,
    }
}

/// A web address's host, without `www.`.
fn site(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    host.strip_prefix("www.").unwrap_or(host)
}

pub fn targets(seed: &LaneSeed, sessions: &[Session], pr: Option<&Pr>, preview: Option<&Preview>, apps: &AppNames) -> (Vec<Target>, Option<String>) {
    let term = &apps.terminal;
    let editor = &apps.editor;
    let mut t = Vec::new();
    // The most recent session leads, since opening the lane opens it; the rest keep the lane's
    // order, most urgent first.
    let latest = latest_session(sessions);
    let rest = sessions.iter().filter(|s| latest.map_or(true, |l| l.key != s.key));
    let shown: Vec<&Session> = latest.into_iter().chain(rest).take(4).collect();
    for (i, s) in shown.iter().enumerate() {
        let first = i == 0;
        let enter: Option<&[&str]> = if first { Some(&["⏎"]) } else { None };
        let who = format!("{} · {}", s.account, truncate(&s.title, 48));
        let respond = s.status == Status::Needs;
        if s.hosted {
            // Codealong runs it: its panel is where it's read, sent to and answered.
            let verb = if respond { "Respond in" } else { "Show in" };
            t.push(target(format!("s|{}|hosted", s.key), "app", format!("{verb} main window"), Some(who), enter, false, "session"));
            continue;
        }
        // Each harness says where its sessions open (`Adapter::targets`).
        let opens = crate::harness::Opens { term, who, first, respond };
        t.extend(crate::harness::adapter(s.harness).targets(s, &opens));
    }
    // Their tabs in Chrome, apart from the ways to open them: ⌘⏎ and the lane's default are those.
    for s in &shown {
        if let Some(c) = &s.chrome {
            let page = if c.title.is_empty() { crate::previews::short_url(&c.url) } else { c.title.as_str() };
            let detail = format!("{} · {} · {}", truncate(page, 48), s.account, truncate(&s.title, 48));
            t.push(target(format!("s|{}|chrome", s.key), "world", chrome_label(c), Some(detail), None, false, "browser"));
        }
    }
    let here = tilde(&seed.path);
    t.push(target("l|terminal".into(), "terminal", format!("New {term} terminal here"), Some(here), Some(&["⌘", "1"]), false, "lane"));
    t.push(target("l|editor".into(), "code", format!("Open in {editor}"), None, Some(&["⌘", "2"]), false, "lane"));
    if let Some(p) = preview {
        let at = crate::previews::short_url(&p.url);
        let mut detail = match &p.server {
            Some(s) => format!("{at} · {} on port {}", s.name, s.port),
            None => format!("{at} · not running"),
        };
        if let Some(profile) = &p.chrome_profile {
            detail.push_str(&format!(" · Chrome's {profile} profile"));
        }
        t.push(target("l|preview".into(), "world", "Open preview".into(), Some(detail), Some(&["⌘", "5"]), false, "lane"));
    }
    t.push(target("l|finder".into(), "folder", "Reveal in Finder".into(), None, None, false, "lane"));
    if let Some(pr) = pr {
        t.push(target("p|github".into(), "external", format!("Open #{} on GitHub", pr.number), None, Some(&["⌘", "O"]), false, "pr"));
    }
    // The main action pulls up the most recent session where it lives; with no sessions, a
    // terminal at the lane. The editor is always secondary.
    let default = t
        .iter()
        .find(|x| x.group == "session" && !x.disabled)
        .map(|x| x.id.clone())
        .or_else(|| Some("l|terminal".to_string()));
    (t, default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::fixtures::{acct, pr, reg, seed, tr, NOW};

    #[test]
    fn ranks_bays() {
        let a = acct("claude2");
        let needs = claude_session(&a, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let working = claude_session(&a, Some(&reg("busy", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let unread = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let open_pr = find_pr(&seed("/r", "portal"), &HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Open, 0)])]), NOW);
        let failing = find_pr(&seed("/r", "portal"), &HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Open, 2)])]), NOW);
        let merged = find_pr(&seed("/r", "portal"), &HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Merged, 0)])]), NOW);
        assert_eq!(bay_of(&[working.clone(), needs.clone()], None), (Bay::Needs, Status::Needs));
        // A PR that needs work doesn't make the worktree need you: its agents do.
        assert_eq!(bay_of(&[unread.clone()], failing.as_ref()), (Bay::Unread, Status::Unread));
        assert_eq!(bay_of(&[], failing.as_ref()), (Bay::Review, Status::Idle));
        assert_eq!(bay_of(&[working.clone()], failing.as_ref()), (Bay::Working, Status::Working));
        assert_eq!(bay_of(&[unread.clone()], open_pr.as_ref()), (Bay::Unread, Status::Unread));
        assert_eq!(bay_of(&[], open_pr.as_ref()), (Bay::Review, Status::Idle));
        assert_eq!(bay_of(&[], merged.as_ref()), (Bay::Merged, Status::Idle));
        assert_eq!(bay_of(&[], None), (Bay::Idle, Status::Idle));
    }

    #[test]
    fn writes_headlines() {
        let a = acct("claude2");
        let needs = claude_session(&a, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        // One whose reply asks you something asks you; it isn't waiting for approval.
        let asking = Session { asks: true, excerpt: Some("Asks you · Want the PR?".into()), ..needs.clone() };
        assert_eq!(session_headline(&asking), ("`claude2` asks you · Portal daily log + photo sync".into(), Some("Want the PR?".into())));
        // So does one whose question is held through the plugin, though what you type doesn't
        // answer it.
        let held = Session { asks: false, excerpt: Some("Asks you · Which color?".into()), ..needs.clone() };
        assert_eq!(session_headline(&held), ("`claude2` asks you · Portal daily log + photo sync".into(), Some("Which color?".into())));
        // Needs you with nothing to say: the title is in the headline, nothing under it.
        assert_eq!(session_headline(&Session { excerpt: None, ..needs.clone() }), ("`claude2` needs you · Portal daily log + photo sync".to_string(), None));
        // A finished turn is its title alone: its glyph and bay say it finished.
        let done = Session { status: Status::Unread, excerpt: Some("Pushed the fix".into()), ..needs.clone() };
        assert_eq!(session_headline(&done), ("`claude2` · Portal daily log + photo sync".to_string(), Some("Pushed the fix".to_string())));
        assert_eq!(
            headline(Bay::Needs, &[needs], None, None),
            ("`claude2` is waiting for approval · Portal daily log + photo sync".to_string(), Some("Bash · `yarn mikro-orm migration:up`".to_string()))
        );
        let working = claude_session(&a, Some(&reg("busy", NOW - 5_000)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let (h, d) = headline(Bay::Working, &[working], None, None);
        assert_eq!(h, "`claude2` · Portal daily log + photo sync");
        assert_eq!(d, Some(format!("{{for:{}}} · running `yarn test`", NOW - 5_000)));
        // A message sent while it works restarts the clock.
        let nudged = TranscriptInfo { last_prompt_at: Some(NOW - 2_000), ..tr() };
        let working = claude_session(&a, Some(&reg("busy", NOW - 5_000)), Some(&nudged), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!(headline(Bay::Working, &[working], None, None).1, Some(format!("{{for:{}}} · running `yarn test`", NOW - 2_000)));
        let failing = find_pr(&seed("/r", "portal"), &HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Open, 2)])]), NOW);
        assert_eq!(
            headline(Bay::Review, &[], failing.as_ref(), None),
            ("#1037 · checks failing".to_string(), Some("`check-0` · `check-1`".to_string()))
        );
        let review = |state: PrState, merge: Merge| {
            let pr = find_pr(&seed("/r", "portal"), &HashMap::from([("sprucehq/spark".to_string(), vec![PrRaw { merge, ..pr(state, 0) }])]), NOW);
            headline(Bay::Review, &[], pr.as_ref(), None)
        };
        assert_eq!(review(PrState::Approved, Merge::Clean), ("#1037 approved · ready to merge".to_string(), Some("11 checks passed".to_string())));
        let stale = find_pr(
            &seed("/r", "portal"),
            &HashMap::from([("sprucehq/spark".to_string(), vec![PrRaw { merge: Merge::Clean, stale_approval: Some(StaleApproval { by: vec!["ana".into()], commit: "a11ce55".into() }), ..pr(PrState::Approved, 0) }])]),
            NOW,
        );
        assert_eq!(headline(Bay::Review, &[], stale.as_ref(), None).0, "#1037 approved");
        // Only GitHub can say it's ready: approved and passing can still conflict with the base.
        assert_eq!(review(PrState::Approved, Merge::Unknown).0, "#1037 approved");
        assert_eq!(review(PrState::Approved, Merge::Conflicts).0, "#1037 has conflicts");
        assert_eq!(review(PrState::Open, Merge::Blocked).0, "#1037 waiting for review");
        // A repo that requires no review.
        assert_eq!(review(PrState::Open, Merge::Clean).0, "#1037 ready to merge");
    }

    #[test]
    fn a_worktrees_url_opens_on_the_server_it_names() {
        let server = |port: u16, page: bool, ancestors: Vec<u32>| crate::previews::Server {
            lane: PathBuf::from("/r/spark-portal"),
            pid: port as u32,
            port,
            name: "node".into(),
            cwd: PathBuf::from("/r/spark-portal/apps/app"),
            started_ms: NOW - 60_000,
            page,
            title: page.then(|| "Spruce".to_string()),
            ancestors,
        };
        let (api, app, storybook) = (server(3040, false, vec![3040, 900]), server(5210, true, vec![5210, 900]), server(6006, true, vec![6006, 1]));
        let servers = [&api, &app, &storybook];
        let entry = |url: &str, port: Option<u16>| crate::previews::Entry { worktree: "/r/spark-portal".into(), url: url.into(), port, chrome_profile: None, at: NOW };
        let port_of = |p: &Option<Preview>| p.as_ref().and_then(|p| p.server.as_ref()).map(|s| s.port);

        // A URL a proxy serves opens on the worktree's page, the lowest port when two serve one.
        let proxied = lane_preview(Some(&entry("https://portal.app.test", None)), &servers, &[], None);
        assert_eq!(port_of(&proxied), Some(5210));
        assert!(proxied.as_ref().is_some_and(|p| p.set && p.url == "https://portal.app.test"));
        // The port given with it, or the one a local URL names, picks the server, page or not.
        assert_eq!(port_of(&lane_preview(Some(&entry("https://portal.app.test", Some(3040))), &servers, &[], None)), Some(3040));
        assert_eq!(port_of(&lane_preview(Some(&entry("http://localhost:6006", None)), &servers, &[], None)), Some(6006));
        // Nothing listening: set, not running.
        let down = lane_preview(Some(&entry("https://portal.app.test", None)), &[], &[], None).unwrap();
        assert!(down.set && down.server.is_none());
        assert!(lane_preview(Some(&entry("https://portal.app.test", Some(5299))), &servers, &[], None).unwrap().server.is_none());
        // With no URL set, a page served there opens at localhost; an API alone isn't an app.
        let found = lane_preview(None, &servers, &[], None).unwrap();
        assert_eq!((found.url.as_str(), found.set), ("http://localhost:5210", false));
        assert_eq!(lane_preview(None, &[&api], &[], None), None);
        assert_eq!(lane_preview(None, &[], &[], None), None);
        // The session whose command started it: its process is one of the server's parents.
        let a = acct("claude2");
        let mut started = claude_session(&a, Some(&reg("shell", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        started.pid = Some(900);
        let found = lane_preview(None, &servers, &[started.clone()], None).unwrap();
        assert_eq!(found.server.unwrap().session.as_deref(), Some(started.key.as_str()));
        // It's opened from the worktree's menus, and says where and whether it runs.
        let apps = AppNames { editor: "Cursor".into(), ..Default::default() };
        let (t, default) = targets(&seed("/r/spark-portal", "portal"), &[], None, proxied.as_ref(), &apps);
        let open = t.iter().find(|x| x.id == "l|preview").unwrap();
        assert_eq!((open.label.as_str(), open.detail.as_deref()), ("Open preview", Some("portal.app.test · node on port 5210")));
        assert_eq!(default.as_deref(), Some("l|terminal"));
        let (t, _) = targets(&seed("/r/spark-portal", "portal"), &[], None, Some(&down), &apps);
        assert_eq!(t.iter().find(|x| x.id == "l|preview").unwrap().detail.as_deref(), Some("portal.app.test · not running"));

        // It opens in the project's Chrome profile unless the worktree names its own.
        let mut own = entry("https://portal.app.test", None);
        let project = lane_preview(Some(&own), &servers, &[], Some("Work")).unwrap();
        assert_eq!((project.chrome_profile.as_deref(), project.chrome_profile_set), (Some("Work"), false));
        own.chrome_profile = Some("Dev".into());
        let mine = lane_preview(Some(&own), &servers, &[], Some("Work")).unwrap();
        assert_eq!((mine.chrome_profile.as_deref(), mine.chrome_profile_set), (Some("Dev"), true));
        assert_eq!(lane_preview(None, &servers, &[], Some("Work")).unwrap().chrome_profile.as_deref(), Some("Work"));
        assert_eq!(lane_preview(None, &servers, &[], None).unwrap().chrome_profile, None);
        let (t, _) = targets(&seed("/r/spark-portal", "portal"), &[], None, Some(&mine), &apps);
        assert_eq!(t.iter().find(|x| x.id == "l|preview").unwrap().detail.as_deref(), Some("portal.app.test · node on port 5210 · Chrome's Dev profile"));

        // A link into its app opens in its profile, from any worktree's conversation; a link
        // elsewhere, or into an app that opens in the default browser, doesn't.
        let at = |id: &str, preview: Option<Preview>| Lane { preview, ..crate::fixtures::lane(id, vec![]) };
        let lanes = vec![at("/r/spark", None), at("/r/spark-plain", lane_preview(None, &[&api, &storybook], &[], None)), at("/r/spark-portal", Some(mine.clone()))];
        let app = |url: &str| app_of_link(&lanes, url).map(|l| l.id.as_str());
        assert_eq!(app("https://portal.app.test/orgs/a/projects/b"), Some("/r/spark-portal"));
        assert_eq!(app("http://127.0.0.1:5210/"), Some("/r/spark-portal"));
        assert_eq!(app("http://localhost:6006"), None);
        assert_eq!(app("https://github.com/o/r/pull/1"), None);
        let elsewhere = DeviceTag { name: "Mini".into(), short: "abc".into(), color: "blue".into(), online: true, last_seen: None };
        let theirs = vec![Lane { device: Some(elsewhere), ..lanes[2].clone() }];
        assert!(app_of_link(&theirs, "https://portal.app.test/x").is_none());
    }

    #[test]
    fn a_page_is_the_sessions_that_made_it_latest_first() {
        let made = |url: &str, at: i64| Artifact { url: url.into(), title: "Roles".into(), note: None, at };
        let session = |key: &str, account: &str, artifacts: Vec<Artifact>| Session { account: account.into(), artifacts, ..crate::fixtures::session(key) };
        // One artifact as Claude Code links what it published, which the Claude app links by its
        // uuid; and another.
        let roles = "https://claude.ai/artifact/Lra3DKxkDb6AU1Apmc4F19";
        let viewed = "https://claude.ai/code/artifact/a0c97093-8bbc-43cc-852c-63bff702d568";
        let other = "https://claude.ai/artifact/6zxdiitFmMYwA7d92F7zSo";
        let mine = Lane {
            sessions: vec![session("claude:claude:a", "claude", vec![made(roles, 10)]), session("claude:claude2:b", "claude2", vec![made(other, 5), made(roles, 20)])],
            ..crate::fixtures::lane("/r/atlas", vec![])
        };
        let keys = |lanes: &[Lane], url: &str| sessions_with_page(lanes, url).iter().map(|s| s.key.clone()).collect::<Vec<_>>();
        let lanes = vec![mine.clone()];
        assert_eq!(keys(&lanes, roles), ["claude:claude2:b", "claude:claude:a"]);
        assert_eq!(keys(&lanes, viewed), ["claude:claude2:b", "claude:claude:a"]);
        assert_eq!(keys(&lanes, &format!("{other}/?v=2#top")), ["claude:claude2:b"]);
        assert!(keys(&lanes, "https://claude.ai/artifact/Other").is_empty());
        // Another machine's sessions are signed in to its accounts, not this Mac's.
        let elsewhere = DeviceTag { name: "Mini".into(), short: "abc".into(), color: "blue".into(), online: true, last_seen: None };
        assert!(keys(&[Lane { device: Some(elsewhere), ..mine }], roles).is_empty());
    }

    #[test]
    fn builds_targets_with_app_names() {
        let apps = AppNames { terminal: "Ghostty".into(), terminal_id: "ghostty".into(), editor: "Cursor".into(), editor_id: "cursor".into(), ..Default::default() };
        let a = acct("claude2");
        let live = claude_session(&a, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let (t, default) = targets(&seed("/r/spark-portal", "portal"), &[live.clone()], None, None, &apps);
        assert_eq!(t[0].label, "Respond in Ghostty");
        assert_eq!(default.as_deref(), Some("s|claude:claude2:abc1234567|focus"));
        // A session running in another terminal than the one config.toml names is shown there.
        let (t, _) = targets(&seed("/r/spark-portal", "portal"), &[Session { terminal: Some("Rex".into()), ..live }], None, None, &apps);
        assert_eq!(t[0].label, "Respond in Rex");
        assert!(t.iter().all(|x| !x.id.ends_with("|claude-app")), "claude2 isn't the Claude app's account");
        assert!(t.iter().any(|x| x.label == "Open in Cursor"));
        let main = AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let mine = claude_session(&main, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let (t, _) = targets(&seed("/r/spark-portal", "portal"), &[mine], None, None, &apps);
        let app = |t: &[Target]| t.iter().find(|x| x.id.ends_with("|claude-app")).map(|x| (x.label.clone(), x.detail.clone().unwrap_or_default(), x.disabled));
        // Live in a terminal, the app takes it only through its Remote Control.
        assert_eq!(app(&t), Some(("Open in Claude app".to_string(), "Needs Remote Control (/remote-control)".to_string(), true)));
        let controlled = RegistryEntry { remote_control: Some("session_01abc".into()), ..reg("waiting", NOW) };
        let mine = claude_session(&main, Some(&controlled), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!(mine.remote_control.as_deref(), Some("session_01abc"));
        let (t, _) = targets(&seed("/r/spark-portal", "portal"), &[mine], None, None, &apps);
        assert_eq!(t[0].label, "Respond in Ghostty", "the terminal still leads");
        assert!(app(&t).is_some_and(|(_, why, off)| !off && why.starts_with("claude · ")));
        // A background job too, beside attaching it.
        let job = RegistryEntry { kind: Some("bg".into()), job_id: Some("b887d77f".into()), ..controlled };
        let (t, _) = targets(&seed("/r/spark-portal", "portal"), &[claude_session(&main, Some(&job), Some(&tr()), 0, NOW, &Quiet::default()).unwrap()], None, None, &apps);
        assert_eq!(t[0].label, "Attach in Ghostty");
        assert!(app(&t).is_some_and(|(_, _, off)| !off));
        let (none, default) = targets(&seed("/r/x", "x"), &[], None, None, &apps);
        assert_eq!(default.as_deref(), Some("l|terminal"));
        assert_eq!(none.len(), 3);
    }

    #[test]
    fn a_session_offers_its_chrome_tabs() {
        let apps = AppNames { terminal: "Ghostty".into(), ..Default::default() };
        let a = acct("claude2");
        let mut s = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        s.chrome = Some(ChromeTabs { tabs: vec![12, 11], url: "https://www.admin.google.com/ac/users?x=1".into(), title: "Users".into(), profile: Some("Work".into()), profile_dir: Some("Profile 12".into()), at: NOW });
        let (t, default) = targets(&seed("/r/x", "x"), &[s.clone()], None, None, &apps);
        let c = t.iter().find(|x| x.id == "s|claude:claude2:abc1234567|chrome").expect("a Chrome target");
        assert_eq!((c.label.as_str(), c.group.as_str()), ("Show in Chrome (Work)", "browser"));
        assert!(c.detail.as_deref().unwrap().starts_with("Users · claude2 · "));
        // It isn't how the session or the lane opens.
        assert_eq!(default.as_deref(), Some(s.open_target.as_str()));
        let tabs = s.chrome.as_mut().unwrap();
        tabs.tabs.clear();
        tabs.profile = None;
        assert_eq!(chrome_label(tabs), "Reopen admin.google.com in Chrome");
    }

    #[test]
    fn a_lane_opens_its_most_recent_session() {
        let apps = AppNames::default();
        let lane = seed("/r/x", "x");
        let a = acct("claude2");
        let mut needs = claude_session(&a, Some(&reg("waiting", NOW - 3_600_000)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        needs.last_activity = NOW - 3_600_000;
        let mut newer = claude_session(&a, None, Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        newer.key = "claude:claude2:newer".into();
        newer.open_target = format!("s|{}|resume", newer.key);
        newer.last_activity = NOW - 60_000;
        // The lane lists the one that needs you first, but opening the lane resumes the newer one.
        let (t, default) = targets(&lane, &[needs.clone(), newer.clone()], None, None, &apps);
        assert_eq!(default.as_deref(), Some(newer.open_target.as_str()));
        assert_eq!(t[0].id, newer.open_target);
        assert_eq!(t[0].kbd, Some(vec!["⏎".to_string()]));
        let other = t.iter().find(|x| x.id == needs.open_target).expect("the other session is still offered");
        assert_eq!(other.kbd, None);
        // A tie goes to the more urgent one, listed first.
        newer.last_activity = needs.last_activity;
        let (_, default) = targets(&lane, &[needs.clone(), newer.clone()], None, None, &apps);
        assert_eq!(default.as_deref(), Some(needs.open_target.as_str()));
        // A session from another lane that worked here more recently doesn't beat the lane's own,
        // but opens a lane that has none.
        let mut guest = newer.clone();
        guest.key = "claude:claude2:guest".into();
        guest.open_target = format!("s|{}|resume", guest.key);
        guest.visitor = Some("worked here".into());
        guest.last_activity = NOW;
        let (_, default) = targets(&lane, &[needs.clone(), guest.clone()], None, None, &apps);
        assert_eq!(default.as_deref(), Some(needs.open_target.as_str()));
        let (_, default) = targets(&lane, &[guest.clone()], None, None, &apps);
        assert_eq!(default.as_deref(), Some(guest.open_target.as_str()));
    }

    #[test]
    fn hosted_session_targets_main_window() {
        let apps = AppNames::default();
        let lane = seed("/r/x", "x");
        let a = acct("claude2");
        let mut s = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        s.hosted = true;
        let (t, _) = targets(&lane, &[s.clone()], None, None, &apps);
        assert_eq!(t[0].label, "Show in main window");
        s.status = Status::Needs;
        let (t, _) = targets(&lane, &[s], None, None, &apps);
        assert_eq!(t[0].label, "Respond in main window");
    }
}

//! `codealong pr`: a PR with the sessions that worked on it (`show`), a session linked to one by
//! hand, what its primary session is told of (`nudge`, `watch`), an agent's review of it, and
//! your review of it snoozed.
//!
//! The server reads PRs, and a board from the files has none: `show`, `review` and `snooze` say
//! so when none runs. `link`, `unlink`, `nudge` and `watch` change pr-links.json themselves
//! (prlinks.rs), which the app watches, so they work without one; with one they also say when
//! Codealong doesn't read the PR. A PR is named by its URL, `owner/name#number`, or a number in
//! the current folder's repo (`resolve_pr`).

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;

use crate::actions::params;
use crate::api;
use crate::model::{Board, PrState};
use crate::server::{self, Request};
use crate::start::lane_for_path;
use crate::util::{now_ms, tilde};

use super::*;

/// `pr snooze`: your review of a PR that asks for it, as its menu offers Snooze review only then.
/// Waking one is fine whatever it asks now.
pub(super) fn pr_snooze(c: &mut Client, pr: &str, when: &SnoozeWhen, json: bool) -> Result<()> {
    let until = when.until()?;
    let key = resolve_pr(c, pr)?;
    if c.app.is_none() {
        bail!("Snoozing needs Codealong's server, which couldn't be started here (`codealong server start` says why): it keeps the snooze, and PRs come through it.");
    }
    let board = c.board()?;
    let p = find_pr(board, &key).ok_or_else(|| anyhow!("Codealong doesn't read {key}: its repo isn't in a project and no session here worked on it, or it's closed and older than GitHub's recent list."))?;
    let key = p.key.clone();
    if until.is_some() && (!(p.review_requested || p.waits_to_merge) || matches!(p.state, PrState::Merged | PrState::Closed)) {
        bail!("{key} doesn't ask for your review, nor wait on you to merge it, so there's nothing of it to snooze.");
    }
    c.act(api::PR_SNOOZE, &key, params::Snooze { keys: vec![key.clone()], until }, None)?;
    SnoozeOut { key, until, said: snooze_said(until) }.print(json)
}

// ---- PRs ----

/// `owner/name#number` for a PR given as its URL, that, or a number in the repo of the current
/// folder's worktree.
pub(super) fn resolve_pr(c: &mut Client, pr: &str) -> Result<String> {
    let pr = pr.trim();
    if let Some(key) = crate::prlinks::from_url(pr) {
        return Ok(key);
    }
    if let Some((slug, n)) = pr.split_once('#') {
        if slug.contains('/') && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
            return Ok(pr.to_string());
        }
    }
    let n = pr.trim_start_matches('#');
    if n.is_empty() || !n.chars().all(|c| c.is_ascii_digit()) {
        bail!("{pr} isn't a PR. Give its URL, owner/name#number, or a number in this folder's repo.");
    }
    let cwd = std::env::current_dir()?;
    let board = c.board()?;
    let slug = lane_for_path(board, &cwd).and_then(|l| l.repo_slug.clone());
    slug.map(|s| format!("{s}#{n}")).ok_or_else(|| anyhow!("This folder isn't in a GitHub repo the board knows, so #{n} names no PR. Give owner/name#{n}."))
}

pub(super) fn find_pr<'a>(board: &'a Board, key: &str) -> Option<&'a crate::model::PrItem> {
    board.prs.iter().find(|p| p.key.eq_ignore_ascii_case(key))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrDetail<'a> {
    #[serde(flatten)]
    pr: &'a crate::model::PrItem,
    /// The sessions in `sessions`, in full.
    session_rows: Vec<SessionRow>,
    /// What its nudge tells of while it's on (nudges.rs `about_of`): everything where it's
    /// watched, else what `[pull_requests] nudge_about` keeps it to for its repo.
    #[serde(skip_serializing_if = "Option::is_none")]
    nudge_about: Option<crate::config::NudgeAbout>,
}

/// What a PR's nudge that tells of `about` leaves out that a session may be waiting on, with
/// how to hear of it (`nudges::left_out`), for `pr show` and `pr nudge`.
fn left_out_line(key: &str, about: crate::config::NudgeAbout) -> Option<String> {
    let what = crate::nudges::left_out(about)?;
    Some(format!("Its nudge leaves out {what} (nudge_about = \"{}\"): for a session waiting on one, `codealong pr watch {key}` tells of them too.", about.as_str()))
}

/// What `pr nudge` notes its PR's nudge leaves out (`left_out_line`), by its repo's settings on
/// `board`, once it's on and not watched: `listed` is the PR as the board had it, and a PR it
/// doesn't list yet (one just opened, when agents turn nudges on) goes by its repo's settings for
/// its default branch. Nothing for a PR that has ended.
fn nudge_note(board: &Board, key: &str, listed: Option<&crate::model::PrItem>) -> Option<String> {
    let about = match listed {
        Some(p) if !p.state.is_open() => return None,
        Some(p) => board.repo_settings.nudge_about(board.nudge_about, p),
        None => board.repo_settings.nudge_about_in(board.nudge_about, key.split_once('#')?.0, None),
    };
    left_out_line(key, about)
}

/// Whether a PR is watched once `pr nudge` or `pr watch` has written its watch as `stored`
/// (prlinks.rs `is_watched`). A watch holds only while the session it picked is the PR's primary
/// (board.rs `pr_item`): one this command picks (`picks`), else the pick the board had
/// (`listed`), which a watch whose session was archived since no longer is, though the file still
/// says it.
fn watch_holds(stored: bool, picks: bool, listed: Option<&crate::model::PrItem>) -> bool {
    stored && (picks || listed.is_some_and(|p| p.watched))
}

pub(super) fn pr_show(c: &mut Client, pr: &str, json: bool) -> Result<()> {
    let key = resolve_pr(c, pr)?;
    if c.app.is_none() {
        bail!("Codealong isn't running, and PRs come through it.");
    }
    let board = c.board()?;
    let p = find_pr(board, &key).ok_or_else(|| anyhow!("Codealong doesn't read {key}: its repo isn't in a project and no session here worked on it, or it's closed and older than GitHub's recent list."))?;
    let all = all_sessions(board);
    let rows: Vec<SessionRow> = p.sessions.iter().filter_map(|ps| all.iter().find(|(s, _)| s.key == ps.key)).map(|(s, l)| session_row(s, l, &board.todos)).collect();
    let nudge_about = p.nudge.then(|| crate::nudges::about_of(board, p));
    if json {
        return print_json(&PrDetail { pr: p, session_rows: rows, nudge_about });
    }
    println!("{} {} ({})", p.key, p.title, lower_debug(&p.state));
    println!("{} → {}  {}", p.head, p.base.as_deref().unwrap_or("?"), p.url);
    if !p.labels.is_empty() {
        println!("Labels: {}", p.labels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>().join(", "));
    }
    for l in &p.lanes {
        println!("Worktree: {}", tilde(Path::new(l)));
    }
    for r in &rows {
        let pushes = p.sessions.iter().find(|s| s.key == r.key).map_or(0, |s| s.pushes);
        let pushed = if pushes > 0 { format!(", pushed {pushes}×") } else { String::new() };
        let primary = match p.primary_by {
            _ if p.primary.as_deref() != Some(r.key.as_str()) => String::new(),
            Some(by) => format!(", primary: {}", primary_why(by)),
            // An app from before says no why.
            None => ", primary".to_string(),
        };
        println!("Session: {} {} ({}, {}{pushed}{primary})", r.key, r.title, r.status, tilde(Path::new(&r.worktree)));
    }
    if rows.is_empty() {
        println!("No session is linked. `codealong pr link {} --session <key>` links one.", p.key);
    }
    if p.nudge && p.watched {
        println!("Its primary session is told of everything that happens on it while it runs (`codealong pr watch {} --off` goes back to what its settings tell of).", p.key);
    } else if p.nudge {
        let why = if p.nudge_by_default { "[pull_requests] nudge_by_default; " } else { "" };
        println!("Its primary session is told of what happens on it while it runs ({why}`codealong pr nudge {} --off` stops that).", p.key);
    }
    if let Some(line) = nudge_about.and_then(|about| left_out_line(&p.key, about)) {
        println!("{line}");
    }
    if let Some(r) = &p.auto_review {
        println!("Review: {}", review_line(r, p.head_oid.as_deref()));
    }
    Ok(())
}

/// Where a PR's agent review stands, in the words its card has (review.ts `autoReviewWords`):
/// what was submitted for you first, since a submitted draft keeps its stage; then its reviewers'
/// sessions and why it went as it did; then that it waits on your sign-off, when the agent approves
/// the commit the PR is at (`head`), or the commit it approved before.
fn review_line(r: &crate::model::AutoReview, head: Option<&str>) -> String {
    use crate::github::Verdict;
    use crate::model::ReviewStage;
    let at = r.head.get(..7).unwrap_or(&r.head);
    // Another machine of yours reviews it (autoreview.rs `reviewer_of`): said as review.ts says it.
    let on = r.machine.as_deref().map(|m| format!(" on {m}")).unwrap_or_default();
    let agent = r.machine.as_deref().map_or_else(|| "Agent".to_string(), |m| format!("Agent on {m}"));
    let says = match (r.submitted, r.stage) {
        (Some(Verdict::Approve), _) => format!("{agent} approved {at} for you"),
        (Some(Verdict::RequestChanges), _) => format!("{agent} requested changes on {at} for you"),
        (Some(Verdict::Comment), _) => format!("{agent} commented on {at} for you"),
        (None, ReviewStage::Queued) => format!("Agent review of {at} to come{on}"),
        (None, ReviewStage::Reviewing) => format!("Agent reviewing {at}{on}"),
        (None, ReviewStage::Posted) => format!("Agent reviewed {at}{on} and commented"),
        (None, ReviewStage::Drafted) => format!("Agent drafted a review of {at}{on}"),
        (None, ReviewStage::Kept) => format!("Agent reviewed {at}{on}"),
        (None, ReviewStage::Skipped) => format!("{agent} passed on {at}"),
        (None, ReviewStage::Failed) if r.parts.iter().any(|p| p.session.is_some()) => format!("Agent's review of {at}{on} didn't finish"),
        (None, ReviewStage::Failed) if r.machine.is_some() => format!("Agent's review of {at}{on} didn't get done"),
        (None, ReviewStage::Failed) => format!("Agent's review of {at} didn't start"),
    };
    let who: String = r.parts.iter().map(|x| x.session.as_ref().map_or_else(|| format!(", {}", x.who), |s| format!(", {} in {s}", x.who))).collect();
    let note = r.note.as_deref().map(|n| format!(": {n}")).unwrap_or_default();
    let signoff = match r.approves.as_deref() {
        Some(a) if head != Some(a) => format!(" (it approved {} before)", a.get(..7).unwrap_or(a)),
        Some(_) if !r.holds => " (it waits on your sign-off)".to_string(),
        _ => String::new(),
    };
    format!("{says}{who}{note}{signoff}")
}

/// Why a PR's primary session is the one it is, as `pr show` says it.
fn primary_why(by: crate::model::PrimaryBy) -> &'static str {
    use crate::model::PrimaryBy::*;
    match by {
        Picked => "picked",
        Pushed => "pushed to its branch most",
        Opened => "opened it",
        Started => "started first",
        Helper => "started the helper that made it, which is done",
        // Only another machine's PR card says one this build doesn't know, and `pr show` lists
        // this machine's sessions.
        Other => "for a reason this build doesn't know",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrLinkOut {
    pr: String,
    session: String,
    linked: bool,
    /// False when it was already so.
    changed: bool,
}

pub(super) fn pr_link(c: &mut Client, a: &PrLinkArgs, linked: bool, json: bool) -> Result<()> {
    let key = resolve_pr(c, &a.pr)?;
    let session = resolve_session(c, &a.session)?;
    // Linked to a session on the board, a PR in a repo whose PRs aren't read is read by its number
    // (ghpoll.rs `made_to_read`); one older than its repo's recent PRs still isn't.
    if linked && c.app.is_some() && find_pr(c.board()?, &key).is_none() {
        eprintln!("Note: Codealong doesn't list {key} yet. It reads it within a minute if its repo's PRs aren't read here; if it's older than its repo's recent PRs, the link won't show.");
    }
    let mut changed = false;
    crate::prlinks::change(|store| {
        changed = store.set(&key, &session, linked, now_ms());
        changed
    })?;
    let out = PrLinkOut { pr: crate::prlinks::pr_key(&key), session, linked, changed };
    if json {
        return print_json(&out);
    }
    let what = if linked { "Linked" } else { "Unlinked" };
    println!("{what} {} {} {}{}", out.session, if linked { "to" } else { "from" }, key, if changed { "" } else { " (it already was)" });
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrReviewOut {
    pr: String,
    /// Its latest round, the one started: its commit and its reviewers, each with its session.
    #[serde(flatten)]
    round: Option<crate::autoreview::Begun>,
}

pub(super) fn pr_review(c: &mut Client, a: &PrReviewArgs, json: bool) -> Result<()> {
    let key = resolve_pr(c, &a.pr)?;
    c.note();
    if c.app.is_none() {
        bail!("Codealong isn't running, and reviews run in it.");
    }
    // Before its session starts, a review reads the PR from GitHub, and what changed since the
    // last review of it, each of which may take a minute or more before it gives up.
    let round: Option<crate::autoreview::Begun> = match server::ask_within(&Request::PrReview { pr: key.clone(), account: a.account.clone() }, std::time::Duration::from_secs(600)) {
        Some(Ok(v)) => serde_json::from_value(v).context("reading the app's reply")?,
        Some(Err(e)) if e.starts_with(server::UNKNOWN_REQUEST) => bail!("The Codealong that's running is older than this command and can't start a review for it. Update it, or quit it and open it again."),
        Some(Err(e)) => bail!("{e}"),
        None => bail!("The app stopped answering."),
    };
    let out = PrReviewOut { pr: key, round };
    if json {
        return print_json(&out);
    }
    let at = out.round.as_ref().map(|r| format!(" at {}", r.head.get(..7).unwrap_or(&r.head))).unwrap_or_default();
    println!("Started a review of {}{at}.", out.pr);
    for p in out.round.iter().flat_map(|r| &r.parts) {
        match &p.session {
            Some(s) => println!("{} reviews in {s}: `codealong session wait {s}` waits for its turn.", p.who),
            None => println!("{}'s session hasn't shown yet; `codealong pr show {}` names it once it does.", p.who, out.pr),
        }
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrNudgeOut {
    pr: String,
    /// Whether its primary session is told of what happens on it, now.
    nudge: bool,
    /// Whether that's everything, whatever its settings keep nudges to (`pr watch`).
    watched: bool,
    /// The session told: the PR's primary, when that's known.
    #[serde(skip_serializing_if = "Option::is_none")]
    session: Option<String>,
    /// False when it was already so.
    changed: bool,
}

/// Turns telling a PR's primary session what happens on it on or off (nudges.rs), in
/// pr-links.json, which the app watches. Turning it on from a session, or for one named, makes
/// that session the PR's primary, unless another running session is (`--take`). A PR the board
/// doesn't list yet has none: one just opened, whose opener is the one asking. With `watch`
/// (`pr watch`) it's a watch that's started or stopped (prlinks.rs `set_watch`), the rest alike.
pub(super) fn pr_nudge(c: &mut Client, a: &PrNudgeArgs, watch: bool, json: bool) -> Result<()> {
    let key = resolve_pr(c, &a.pr)?;
    let on = !a.off;
    let listed = if (on || watch) && c.app.is_some() { find_pr(c.board()?, &key).cloned() } else { None };
    if on && c.app.is_some() {
        match &listed {
            None => eprintln!("Note: Codealong doesn't read {key} yet (it's new or old, or in a repo no project covers that no session here worked on), so nobody is told until it does."),
            Some(p) if !p.state.is_open() => eprintln!("Note: {key} is {}, so there's nothing more to tell of.", lower_debug(&p.state)),
            Some(_) => {}
        }
    }
    let named = a.session.as_deref().map(|k| resolve_session(c, k)).transpose()?;
    let wanted = match named {
        Some(k) => Some(k),
        None if on => find_me(c)?.0,
        None => None,
    };
    if a.take && wanted.is_none() {
        bail!("Codealong couldn't tell which session runs this command, so there's none to make {key}'s primary session. Name one with --session.");
    }
    let current = listed.as_ref().and_then(|p| p.primary.clone());
    let runs = |c: &mut Client, k: &str| c.board().ok().and_then(|b| b.session(k)).is_some_and(|(_, s)| s.live && !s.archived);
    let mut held = None;
    let pick = match wanted {
        Some(w) if current.as_deref() == Some(w.as_str()) => None,
        Some(w) => match current.clone().filter(|k| runs(c, k)) {
            Some(k) if !a.take => {
                held = Some(k);
                None
            }
            _ => Some(w),
        },
        None => None,
    };
    // A watch is the session's picked as the PR's primary, so that it ends with that session
    // (`PrItem::watched`): the one told picked, though it's its primary already.
    let picks = pick.clone().or_else(|| current.clone().filter(|_| watch && on));
    if watch && on && picks.is_none() {
        bail!("{key} has no session here to tell, and this command doesn't run in one, so nothing would watch it. Name one with --session.");
    }
    let mut changed = false;
    let store = crate::prlinks::change(|store| {
        let told = if watch { store.set_watch(&key, on) } else { store.set_nudge(&key, on) };
        let primary = picks.as_ref().is_some_and(|w| store.set_primary(&key, w, now_ms()));
        changed = told || primary;
        changed
    })?;
    let session = pick.clone().or(current).filter(|_| on);
    let pr = crate::prlinks::pr_key(&key);
    // Stopping a watch leaves the nudge as it was: turned on for the PR, or as the board has it.
    let nudge = if watch && !on { store.nudges.contains(&pr) || listed.as_ref().is_some_and(|p| p.nudge) } else { on };
    let watched = watch_holds(store.is_watched(&pr), picks.is_some(), listed.as_ref());
    let out = PrNudgeOut { watched, pr, nudge, session: session.clone(), changed };
    if let Some(k) = &held {
        let who = session_words(c, k);
        let verb = if watch { "watch" } else { "nudge" };
        eprintln!("Note: {key}'s primary session is {who}, which is running, so it's the one told. `codealong pr {verb} {key} --take` has this session told instead: for when you've taken the PR over.");
    }
    // A nudge that isn't a watch tells of what its settings say, which may leave out the approval
    // a session waits on. The settings come through the app, without which nothing is told.
    if out.nudge && !out.watched && c.app.is_some() {
        if let Some(line) = nudge_note(c.board()?, &key, listed.as_ref()) {
            eprintln!("Note: {line}");
        }
    }
    if json {
        return print_json(&out);
    }
    let already = if changed { "" } else { " (it already was)" };
    let what = if out.watched { "everything that happens" } else { "what happens" };
    match (on, &session) {
        (false, _) if watch => println!("{key} isn't watched, so its nudge tells only what its settings say{already}."),
        (false, _) => println!("Nobody is told of what happens on {key}{already}."),
        (true, Some(k)) => println!("{} is told of {what} on {key} while it runs{already}.", session_words(c, k)),
        (true, None) => println!("{key}'s primary session is told of {what} on it while it runs{already}."),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pr_item(state: crate::model::PrState, watched: bool) -> crate::model::PrItem {
        serde_json::from_value(json!({
            "key": "o/r#7", "repoSlug": "o/r", "repo": "r", "project": "P", "number": 7, "title": "Add photos",
            "url": "", "state": state, "author": "me", "mine": true, "reviewRequested": false, "head": "photos", "base": "main",
            "checks": { "passed": 0, "failed": 0, "pending": 0, "skipped": 0, "failing": [] }, "merge": "clean",
            "additions": 0, "deletions": 0, "changedFiles": 0, "createdAt": 0, "updatedAt": 0,
            "mergedAt": null, "closedAt": null, "lanes": [], "sessions": [], "nudge": true, "watched": watched
        }))
        .expect("a PR")
    }

    /// `pr nudge` notes what a nudge leaves out by the PR's repo's settings, for a PR the board
    /// doesn't list yet too (one just opened), and says nothing for one that tells of everything
    /// or has ended.
    #[test]
    fn pr_nudge_notes_what_its_nudge_leaves_out() {
        use crate::config::{NudgeAbout, PullRequestsPatch, RepoCfg};
        use crate::model::PrState;
        let mut board = Board { nudge_about: NudgeAbout::FeedbackAndChecks, ..Board::default() };
        let open = pr_item(PrState::Open, false);
        let note = nudge_note(&board, "o/r#7", Some(&open)).unwrap();
        assert_eq!(note, "Its nudge leaves out approvals and merges (nudge_about = \"feedback-and-checks\"): for a session waiting on one, `codealong pr watch o/r#7` tells of them too.");
        assert_eq!(nudge_note(&board, "o/r#8", None).unwrap(), note.replace("o/r#7", "o/r#8"), "one not listed yet");
        assert_eq!(nudge_note(&board, "o/r#7", Some(&pr_item(PrState::Merged, false))), None);
        // Your table for the repo goes, listed or not.
        let table = |about| RepoCfg { pull_requests: PullRequestsPatch { nudge_about: Some(about), ..Default::default() }, ..Default::default() };
        board.repo_settings.mine.insert(crate::config::repo_key("o/r"), table(NudgeAbout::FeedbackChecksAndMerges));
        assert!(nudge_note(&board, "o/r#8", None).unwrap().starts_with("Its nudge leaves out approvals (nudge_about = \"feedback-checks-and-merges\")"));
        board.repo_settings.mine.insert(crate::config::repo_key("o/r"), table(NudgeAbout::Everything));
        assert_eq!(nudge_note(&board, "o/r#8", None), None);
        assert_eq!(nudge_note(&board, "o/r#7", Some(&open)), None);
    }

    /// A watch the file still has holds only while the board has it, or where this command picks
    /// the session told: the session it picked may have been archived since.
    #[test]
    fn a_watch_holds_only_while_its_pick_does() {
        use crate::model::PrState;
        let (watched, unwatched) = (pr_item(PrState::Open, true), pr_item(PrState::Open, false));
        assert!(watch_holds(true, false, Some(&watched)));
        assert!(!watch_holds(true, false, Some(&unwatched)), "its session was archived, and another is the PR's primary");
        assert!(watch_holds(true, true, Some(&unwatched)), "picked now");
        assert!(watch_holds(true, true, None), "a PR not listed yet, picked now");
        assert!(!watch_holds(true, false, None));
        assert!(!watch_holds(false, true, Some(&watched)), "stopped");
    }

    #[test]
    fn says_where_a_prs_agent_review_stands_as_its_card_does() {
        use crate::model::{AutoReview, PartState, ReviewPart, ReviewStage};
        let part = ReviewPart { who: "reviewers".into(), session: Some("cursor:cursor:c1".into()), state: PartState::Done, note: None };
        let r = AutoReview { stage: ReviewStage::Drafted, head: "1c1706be2657c978".into(), parts: vec![part], at: 0, comment: None, note: None, rounds: 1, behind: false, holds: false, own: false, sent: None, sent_at: None, sendable: None, suggests: None, approves: None, submitted: None, submitted_at: None, machine: None, replaced: Vec::new() };
        let at = Some("1c1706be2657c978");
        assert_eq!(review_line(&r, at), "Agent drafted a review of 1c1706b, reviewers in cursor:cursor:c1");
        // A draft submitted for you keeps its stage, and says what it went as.
        let submitted = AutoReview { submitted: Some(crate::github::Verdict::Approve), ..r.clone() };
        assert_eq!(review_line(&submitted, at), "Agent approved 1c1706b for you, reviewers in cursor:cursor:c1");
        // An approval of the PR's commit is yours to sign off.
        let approving = AutoReview { stage: ReviewStage::Posted, suggests: Some(crate::github::Verdict::Approve), approves: Some("1c1706be2657c978".into()), ..r.clone() };
        assert_eq!(review_line(&approving, at), "Agent reviewed 1c1706b and commented, reviewers in cursor:cursor:c1 (it waits on your sign-off)");
        // One of an earlier commit isn't: the PR moved on, whether or not a review of it is under way.
        assert_eq!(review_line(&approving, Some("2d2806cf")), "Agent reviewed 1c1706b and commented, reviewers in cursor:cursor:c1 (it approved 1c1706b before)");
        let reviewing = AutoReview { stage: ReviewStage::Reviewing, head: "2d2806cf".into(), approves: Some("1c1706be2657c978".into()), ..r.clone() };
        assert_eq!(review_line(&reviewing, Some("2d2806cf")), "Agent reviewing 2d2806c, reviewers in cursor:cursor:c1 (it approved 1c1706b before)");
        // One whose session never began didn't start, and says why.
        let failed = AutoReview { stage: ReviewStage::Failed, parts: vec![ReviewPart { session: None, state: PartState::Failed, ..r.parts[0].clone() }], note: Some("Its session never showed up.".into()), ..r.clone() };
        assert_eq!(review_line(&failed, at), "Agent's review of 1c1706b didn't start, reviewers: Its session never showed up.");
        // Another machine's review says which machine's it is.
        let theirs = AutoReview { stage: ReviewStage::Reviewing, parts: Vec::new(), machine: Some("Studio".into()), ..r.clone() };
        assert_eq!(review_line(&theirs, at), "Agent reviewing 1c1706b on Studio");
        assert_eq!(review_line(&AutoReview { stage: ReviewStage::Skipped, ..theirs.clone() }, at), "Agent on Studio passed on 1c1706b");
        assert_eq!(review_line(&AutoReview { stage: ReviewStage::Failed, ..theirs }, at), "Agent's review of 1c1706b on Studio didn't get done");
    }
}

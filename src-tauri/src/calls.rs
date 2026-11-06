//! What the windows ask of the engine, answered by the server (serve.rs): every command of a
//! window's that reads or changes what the engine, your other devices or Codealong's files hold.
//! A window invokes one by its name as it always has, the app hands it to the server over its
//! socket (`server::Request::Call`, lib.rs `forward`), and the server runs it here. What a window
//! does on its own screen (its windows, opening a file or a link, the floating bar's feed) is the
//! app's own, in lib.rs.
//!
//! Each call takes its arguments as a Tauri command does, by their names in camelCase, missing
//! ones left out as `None`, and its answer goes back as the command's would: its `Ok` as JSON,
//! its `Err` as the words the window shows. `calls!` makes the table from the functions.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::util::{self, words};
use crate::{acp, actions, api, autoreview, boards, config, detect, engine, github, harness, idle, improve, install, jev, ledger, model, newfolder, peers, prasks, prlinks, prmerge, route, routelog, runcode, schedules, search, send, setup, side, skills, stale, start, store, terminals, todojev, todos, training, views, zoom};

/// What a call runs with: the server's engine, your other devices, and the automatic reviewer.
#[derive(Clone)]
pub struct Ctx {
    pub engine: engine::Handle,
    pub peers: peers::Peers,
    pub reviews: Arc<Mutex<autoreview::Reviewer>>,
}

/// Makes each function, `NAMES`, and `call`, which reads a call's arguments into the function's
/// parameters and writes its answer out.
macro_rules! calls {
    ($( $(#[$meta:meta])* fn $name:ident($c:ident $(, $arg:ident: $ty:ty)* $(,)?) -> Result<$ok:ty, String> $body:block )*) => {
        // Not every call needs what it runs with.
        $( $(#[$meta])* #[allow(unused_variables)] fn $name($c: &Ctx $(, $arg: $ty)*) -> Result<$ok, String> $body )*

        /// Every call, by the name a window invokes it by.
        pub const NAMES: &[&str] = &[$(stringify!($name)),*];

        /// Runs call `name` with the arguments a window sent; None for one there's no call for.
        pub fn call(c: &Ctx, name: &str, args: Value) -> Option<Result<Value, String>> {
            match name {
                $( stringify!($name) => Some((move || {
                    #[derive(serde::Deserialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Args { $( $arg: $ty, )* }
                    let Args { $($arg),* } = serde_json::from_value(args).map_err(|e| format!("{}'s arguments didn't read: {e}", stringify!($name)))?;
                    let answer = $name(c $(, $arg)*)?;
                    serde_json::to_value(answer).map_err(|e| e.to_string())
                })()), )*
                _ => None,
            }
        }
    };
}

/// The time of the board that shows a change, out of what an action answered.
fn shown_at(v: Value) -> Option<i64> {
    v.as_i64()
}

/// What the merge box's act did, for the window that asked.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeDone {
    /// What it couldn't do after (a branch that stayed).
    note: Option<String>,
    /// The time of the first board that shows the PR merged, for a merge.
    shown_at: Option<i64>,
    /// Merge and done…: what marking its session done said (actions.rs `done`), or why that
    /// wasn't done (`done_error`).
    #[serde(skip_serializing_if = "Option::is_none")]
    done: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    done_error: Option<String>,
}

calls! {
    /// What sessions used and cost in the last `days` days, for the Usage window (ledger.rs), for
    /// one account or all. Asking reads what's new in the background too, for the next time.
    fn token_report(c, days: u32, account: Option<String>) -> Result<ledger::Report, String> {
        ledger::nudge();
        ledger::report(days, account.as_deref()).map_err(words)
    }

    /// What one session used and cost, by model, for its panel.
    fn session_tokens(c, key: String) -> Result<ledger::SessionSpent, String> {
        ledger::session(&key).map_err(words)
    }

    /// What "Close idle sessions" would close, sessions quiet for `after_ms` or longer, and with
    /// `archive` what it would only archive (idle.rs).
    fn idle_sessions(c, after_ms: i64, archive: bool) -> Result<idle::Plan, String> {
        Ok(idle::plan_now(&c.engine.board(), after_ms, archive))
    }

    /// Closes these sessions, each checked again first, and with `archive` archives them.
    fn close_idle_sessions(c, keys: Vec<String>, after_ms: i64, archive: bool) -> Result<Vec<idle::Outcome>, String> {
        idle::close_on(&c.engine, &keys, after_ms, archive)
    }

    /// What "Close stale terminals" would close: the terminal tabs unused for `after_ms` or longer
    /// (stale.rs).
    fn stale_terminals(c, after_ms: i64) -> Result<stale::Plan, String> {
        Ok(stale::plan_now(&c.engine.board(), c.engine.terminal_use(), after_ms))
    }

    /// Closes these tabs, each looked at again first.
    fn close_stale_terminals(c, keys: Vec<String>, after_ms: i64) -> Result<Vec<stale::Outcome>, String> {
        Ok(stale::close(&c.engine.board(), c.engine.terminal_use(), &keys, after_ms))
    }

    /// Closes one session from its menu: its terminal tab, or a background job stopped (idle.rs).
    /// Returns what happened, in a sentence.
    fn close_session(c, key: String) -> Result<String, String> {
        actions::run(&c.engine, &c.peers, actions::CLOSE, &key, serde_json::json!({})).map(|v| v.as_str().unwrap_or_default().to_string())
    }

    /// Done with a session: it ends as far as Codealong can end it, and is marked seen and
    /// archived (`actions::done`). `stop` ends work under way too. Returns whether it was done,
    /// what happened in a sentence, and the time of the board that shows it.
    fn session_done(c, key: String, stop: bool) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_DONE, &key, actions::params::Done { stop, ..Default::default() })
    }

    /// Marks sessions seen, each on the device it's on: its status changes there, and so on every
    /// device. Returns the time of the board that shows the sessions on this device seen (none
    /// when they're all on other devices, whose feeds show it as they come in).
    fn mark_seen(c, keys: Vec<String>) -> Result<Option<i64>, String> {
        Ok(actions::run_keys(&c.engine, &c.peers, api::MARK_SEEN, keys, actions::params::Keys::default()).and_then(shown_at))
    }

    /// Marks sessions unread, each on the device it's on: each is Your turn again until it's seen
    /// or prompted. Returns the time of the board that shows it, as `mark_seen` does.
    fn mark_unread(c, keys: Vec<String>) -> Result<Option<i64>, String> {
        Ok(actions::run_keys(&c.engine, &c.peers, api::MARK_UNREAD, keys, actions::params::Keys::default()).and_then(shown_at))
    }

    /// Snoozes sessions until `until` (ms since the epoch), or wakes them with none, each on the
    /// device it's on. Returns the time of the board that shows it once every device has
    /// answered: one that didn't snooze its sessions is the error.
    fn snooze_sessions(c, keys: Vec<String>, until: Option<i64>) -> Result<Option<i64>, String> {
        actions::run_keys_heard(&c.engine, &c.peers, api::SESSION_SNOOZE, keys, actions::params::Snooze { keys: Vec::new(), until }).map(|v| v.and_then(shown_at))
    }

    /// Quiets sessions, or makes them heard again with `quiet` false, each on the device it's on.
    fn quiet_sessions(c, keys: Vec<String>, quiet: bool) -> Result<Option<i64>, String> {
        actions::run_keys_heard(&c.engine, &c.peers, api::SESSION_QUIET, keys, actions::params::Quiet { keys: Vec::new(), quiet }).map(|v| v.and_then(shown_at))
    }

    /// Moves sessions to the top of what waits on you, or back to their places with `raised`
    /// false, each on the device it's on.
    fn raise_sessions(c, keys: Vec<String>, raised: bool) -> Result<Option<i64>, String> {
        actions::run_keys_heard(&c.engine, &c.peers, api::SESSION_RAISE, keys, actions::params::Raise { raised, ..Default::default() }).map(|v| v.and_then(shown_at))
    }

    /// Makes helpers sessions of your own, for good, each on the device it's on.
    fn own_sessions(c, keys: Vec<String>) -> Result<Option<i64>, String> {
        actions::run_keys_heard(&c.engine, &c.peers, api::SESSION_OWN, keys, actions::params::Own { keys: Vec::new() }).map(|v| v.and_then(shown_at))
    }

    /// Snoozes your review of PRs (`owner/name#number`) until `until`, or wakes them with none.
    fn snooze_reviews(c, keys: Vec<String>, until: Option<i64>) -> Result<Option<i64>, String> {
        Ok(c.engine.snooze_reviews(keys, until))
    }

    /// Turns telling a PR's primary session what happens on it on or off for the PR
    /// `owner/name#number` (nudges.rs), in pr-links.json, which the engine then reads at once.
    fn set_pr_nudge(c, key: String, on: bool) -> Result<(), String> {
        prlinks::change(|store| store.set_nudge(&key, on)).map_err(words)?;
        c.engine.pr_links_changed();
        Ok(())
    }

    /// Makes a session the PR's primary one, the one a nudge tells, from the PR's menu.
    fn set_pr_primary(c, key: String, session: String) -> Result<(), String> {
        prlinks::change(|store| store.set_primary(&key, &session, util::now_ms())).map_err(words)?;
        c.engine.pr_links_changed();
        Ok(())
    }

    fn refresh(c) -> Result<(), String> {
        c.engine.refresh_github();
        Ok(())
    }

    fn refresh_usage(c) -> Result<(), String> {
        c.engine.refresh_usage();
        Ok(())
    }

    fn set_pinned(c, keys: Vec<String>) -> Result<(), String> {
        c.engine.set_pinned(keys);
        Ok(())
    }

    /// Sets a worktree's preview URL, and the Chrome profile it opens in over its project's, or
    /// clears them with no URL. Returns the time of the board that shows it.
    fn set_preview(c, lane_id: String, url: Option<String>, port: Option<u16>, chrome_profile: Option<String>) -> Result<Option<i64>, String> {
        // The sheet gives all of it: no URL takes the preview away, and no profile is its project's.
        let change = match url {
            Some(url) => actions::params::Preview { url: Some(url), port: Some(port), chrome_profile: Some(chrome_profile), ..Default::default() },
            None => actions::params::Preview { clear: true, ..Default::default() },
        };
        actions::run(&c.engine, &c.peers, actions::PREVIEW, &lane_id, change).map(|v| v["at"].as_i64())
    }

    /// Marks a worktree reusable or not (reusable.rs). Returns the time of the board that shows it.
    fn set_reusable(c, lane_id: String, on: bool) -> Result<Option<i64>, String> {
        actions::run(&c.engine, &c.peers, actions::REUSABLE, &lane_id, actions::params::Reusable { on }).map(|v| v["at"].as_i64())
    }

    /// Removes a worktree when nothing in it would be lost, or with `dry_run` says whether it can
    /// go and what would run (cleanup.rs), on the device it's on.
    fn remove_worktree(c, lane_id: String, dry_run: bool) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::WORKTREE_REMOVE, &lane_id, api::params::Remove { dry_run, ..Default::default() })
    }

    fn set_pinned_worktrees(c, ids: Vec<String>) -> Result<(), String> {
        c.engine.set_pinned_worktrees(ids);
        Ok(())
    }

    fn set_pinned_prs(c, keys: Vec<String>) -> Result<(), String> {
        c.engine.set_pinned_prs(keys);
        Ok(())
    }

    fn set_pinned_repos(c, keys: Vec<String>) -> Result<(), String> {
        c.engine.set_pinned_repos(keys);
        Ok(())
    }

    /// Where what's typed in the quick panel should go, from Jev (route.rs).
    fn route_quick(c, project: String, text: String) -> Result<route::Route, String> {
        actions::typed(actions::run_on(&c.engine, &c.peers, api::ROUTE, None, actions::params::Route { project, text })?)
    }

    /// Keeps what you did with what you typed in the quick panel, beside what Jev suggested for it
    /// (routelog.rs).
    fn note_route(c, note: routelog::Note) -> Result<(), String> {
        routelog::note(note, util::now_ms()).map_err(words)
    }

    /// Sessions whose prompts and replies have every word (search.rs), for the quick panel.
    fn search_text(c, words: Vec<String>) -> Result<search::Found, String> {
        actions::typed(actions::run_on(&c.engine, &c.peers, api::SEARCH, None, actions::params::Search { words })?)
    }

    /// A page of a session's conversation, from its own record (transcript.rs): the newest when
    /// `before` is none, else what came before it.
    fn session_page(c, key: String, before: Option<u64>) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_PAGE, &key, actions::params::Page { before, ..Default::default() })
    }

    /// The terminal tab a session shown in a window runs in, for Improve Codealong to picture
    /// (improve.rs `capture`): None when it isn't live in one whose tabs' windows can be found.
    /// The app's own question, about this Mac's screen, so no other device asks it.
    fn improve_tab(c, key: String) -> Result<Option<improve::Tab>, String> {
        improve::tab_of(&c.engine.board(), &key, improve::TAB_FIND_WAIT).map_err(words)
    }

    /// What a session waiting on you wants to do, when it's a tool call Codealong can answer for
    /// it (`harness::Adapter::pending`).
    fn session_pending(c, key: String) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_PENDING, &key, actions::params::Session::default())
    }

    /// The skills a session in each of `folders` could be asked to run by name, and each of
    /// `accounts`' own (skills.rs), for the quick panel's `/` list. This machine's only.
    fn skills(c, folders: Vec<String>, accounts: Vec<String>) -> Result<skills::Listed, String> {
        let cfg = config::current().map_err(words)?;
        Ok(skills::list(&folders, &accounts, &cfg))
    }

    /// The skills a session could be asked to run by name, for its reply box's `/` list, read on
    /// the device it runs on (`api::SESSION_SKILLS`).
    fn session_skills(c, key: String) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_SKILLS, &key, actions::params::Session::default())
    }

    /// How a message sent to a session would reach it, for its box's placeholder (send.rs `way_in`).
    fn session_way(c, key: String) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_WAY, &key, actions::params::Session::default())
    }

    /// How a command of yours led by `!` would reach a session to run in its own shell, for what
    /// ⌥⏎ does in its box (send.rs `shell_way_in`); null where it can't. Only this Mac's
    /// sessions are asked: the box sends `!` to another device's as any message.
    fn session_shell_way(c, key: String) -> Result<Option<api::Way>, String> {
        let cfg = config::current().map_err(words)?;
        send::shell_way_in(&c.engine.board(), &cfg, &key).map_err(words)
    }

    /// Allows or denies the tool call `id` a session waits on, if it's still the one waiting; or,
    /// with `answers`, answers the questions it asks. A deny with `message` tells it what to do
    /// instead.
    fn session_answer(c, key: String, id: String, answer: harness::Answer, answers: Option<Value>, message: Option<String>) -> Result<String, String> {
        let said = actions::run(&c.engine, &c.peers, api::SESSION_ANSWER, &key, actions::params::Answer { id, answer: Some(answer), answers, message, ..Default::default() })?;
        Ok(said.as_str().unwrap_or_default().to_string())
    }

    /// What a session said and did after `after`, as it goes on.
    fn session_newer(c, key: String, after: u64) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_NEWER, &key, actions::params::Newer { after, ..Default::default() })
    }

    /// The reply a session is writing that its record doesn't have yet (actions.rs `stream`), for
    /// a panel that opens mid-turn, known by the key the window has for it.
    fn session_stream(c, key: String) -> Result<Option<acp::Stream>, String> {
        let Ok(got) = actions::run(&c.engine, &c.peers, api::SESSION_STREAM, &key, actions::params::Session::default()) else { return Ok(None) };
        Ok(serde_json::from_value::<acp::Stream>(got).ok().map(|s| acp::Stream { key, ..s }))
    }

    /// Stops the turn a session is taking, here or on another device, and says what happened.
    fn session_cancel(c, key: String) -> Result<String, String> {
        let said = actions::run(&c.engine, &c.peers, api::SESSION_CANCEL, &key, actions::params::Session::default())?;
        // Devices before stops reached every harness answered with nothing.
        Ok(said.as_str().unwrap_or("Stopped.").to_string())
    }

    /// Sends a message a session's harness holds until its turn ends (`id`, one of
    /// `Session::queued`) into the turn it's taking now, and says what happened.
    fn session_steer(c, key: String, id: String) -> Result<String, String> {
        let said = actions::run(&c.engine, &c.peers, api::SESSION_STEER, &key, actions::params::Steer { id, ..Default::default() })?;
        Ok(said.as_str().unwrap_or_default().to_string())
    }

    /// Takes a message a session's harness holds until its turn ends out of what it holds.
    fn session_unqueue(c, key: String, id: String) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_UNQUEUE, &key, actions::params::Steer { id, ..Default::default() })
    }

    /// Sends a prompt to a session that's already going, where it runs (send.rs); `now` sends it
    /// into the turn it's taking at once (Send now). Only Codealong's windows call this, so what
    /// it sends is what you typed.
    fn send_to_session(c, key: String, text: String, now: Option<bool>) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_SEND, &key, actions::params::Send { text, now: now.unwrap_or(false), ..Default::default() })
    }

    /// Goes on with a session that isn't running where `resume` says (`start::resume`), on
    /// `prompt` when there's one, answering how it went as a send does.
    fn resume_session(c, key: String, resume: start::ResumeIn, prompt: Option<String>) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_RESUME, &key, actions::params::Resume { resume, prompt: prompt.unwrap_or_default(), ..Default::default() })
    }

    /// Switches a running session to another of its account's models, for that session alone.
    fn switch_session_model(c, key: String, model: String) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_MODEL, &key, actions::params::Model { model, ..Default::default() })
    }

    /// Switches a running session to another of its account's effort levels.
    fn switch_session_effort(c, key: String, effort: String) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_EFFORT, &key, actions::params::Effort { effort, ..Default::default() })
    }

    /// Switches a session Codealong runs to another of the modes its agent offers.
    fn switch_session_mode(c, key: String, mode: String) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_MODE, &key, actions::params::Mode { mode, ..Default::default() })
    }

    /// What Jev suggests for a todo being written: its project, labels and the todos it waits on
    /// (todojev.rs).
    fn suggest_todo(c, id: Option<String>, title: String, description: String, project: String) -> Result<todojev::Suggested, String> {
        todojev::suggest(&c.engine.board(), id.as_deref(), &title, &description, &project).map_err(words)
    }

    /// "Looks done" was wrong for this todo, until something linked to it moves on.
    fn dismiss_todo_hint(c, id: String) -> Result<(), String> {
        c.engine.dismiss_todo_hint(id);
        Ok(())
    }

    /// Whether there's a TypeSafe API key, for Settings. Reads no secret.
    fn jev_key(c) -> Result<bool, String> {
        Ok(jev::has_key())
    }

    /// Saves the TypeSafe API key in the keychain, or removes it when it's empty.
    fn set_jev_key(c, key: String) -> Result<(), String> {
        if key.trim().is_empty() { jev::delete_key() } else { jev::save_key(&key) }.map_err(words)
    }

    /// One small call to TypeSafe, for Settings' Test button.
    fn test_jev(c, model: String) -> Result<String, String> {
        jev::check(model.trim()).map_err(words)
    }

    /// What Codealong's requests to Jev used each of the last `days` days, by feature.
    fn jev_usage(c, days: u32) -> Result<ledger::JevReport, String> {
        ledger::jev_report(days).map_err(words)
    }

    /// A PR's description, reviews and changed files.
    fn pr_detail(c, repo: String, number: u64) -> Result<github::PrDetail, String> {
        github::detail(&repo, number).map_err(words)
    }

    /// What changed on a PR's branch from commit `from` to `to`, for its Changes tab.
    fn pr_since(c, repo: String, from: String, to: String) -> Result<github::Since, String> {
        github::since(&repo, &from, &to).map_err(words)
    }

    /// A PR the board doesn't list, for a link to it in a conversation: what GitHub says of it,
    /// with the worktrees and sessions here that worked on it.
    fn pr_card(c, repo: String, number: u64) -> Result<model::PrItem, String> {
        c.engine.pr_card(&repo, number).map_err(words)
    }

    /// Which of a PR's comments ask for a change, by comment id, from Jev (prasks.rs). Empty while
    /// `[jev] pull_requests` is off or there's no key.
    fn pr_comment_asks(c, title: String, comments: Vec<prasks::CommentIn>) -> Result<HashMap<String, f64>, String> {
        let cfg = config::current().map_err(words)?;
        if !cfg.jev.pull_requests || !jev::has_key() {
            return Ok(Default::default());
        }
        prasks::asks(cfg.jev.model.trim(), &title, &comments).map_err(words)
    }

    /// Reviews a PR as you: approves it, comments on it or asks for changes, with `body` as the
    /// review's text, submitting your draft review of it when you have one (github.rs `review`).
    /// With `agents`, it's the agent's review that's submitted (autoreview.rs `submit_now`).
    fn review_pr(c, repo: String, number: u64, verdict: github::Verdict, body: String, agents: Option<bool>) -> Result<github::Reviewed, String> {
        if agents == Some(true) {
            return autoreview::submit_now(&c.engine, &c.reviews, &format!("{repo}#{number}"), verdict, &body);
        }
        let done = github::review(&repo, number, verdict, &body).map_err(words)?;
        c.engine.approved(&repo, number, verdict, done.commit.clone());
        Ok(done)
    }

    /// Your review of a PR on GitHub, for its Review tab (github.rs `your_review`).
    fn pr_draft(c, repo: String, number: u64) -> Result<github::YourReview, String> {
        github::your_review(&repo, number).map_err(words)
    }

    /// Gives your draft review of a PR the text `text`.
    fn edit_draft(c, repo: String, number: u64, text: String) -> Result<(), String> {
        github::edit_draft(&repo, number, &text).map_err(words)
    }

    /// Gives a comment of your draft review the text `body`.
    fn edit_draft_comment(c, id: String, body: String) -> Result<(), String> {
        github::edit_draft_comment(&id, &body).map_err(words)
    }

    /// Takes a comment out of your draft review.
    fn drop_draft_comment(c, id: String) -> Result<(), String> {
        github::drop_draft_comment(&id).map_err(words)
    }

    /// What a PR's merge box offers you, as GitHub's has it (prmerge.rs).
    fn merge_box(c, repo: String, number: u64) -> Result<prmerge::MergeBox, String> {
        prmerge::merge_box(&repo, number).map_err(words)
    }

    /// Does what the merge box asked to a PR as you, and returns once GitHub has done it. With
    /// `done` (Merge and done…), the session that key names is marked done once GitHub has merged
    /// the PR, here rather than by the window, which may have closed by then.
    fn merge_pr(c, repo: String, number: u64, id: String, op: prmerge::Op, done: Option<String>) -> Result<MergeDone, String> {
        let acted = prmerge::act(number, &id, &op).map_err(words)?;
        let shown_at = if acted.merged {
            c.engine.merged(&repo, number)
        } else {
            c.engine.pr_changed();
            None
        };
        let (done, done_error) = match done {
            Some(key) if acted.merged => match actions::run(&c.engine, &c.peers, api::SESSION_DONE, &key, actions::params::Done { stop: true, ..Default::default() }) {
                Ok(answer) => (Some(answer), None),
                Err(e) => (None, Some(e)),
            },
            Some(_) => (None, Some(format!("GitHub didn't say #{number} merged, so its session is left as it was."))),
            None => (None, None),
        };
        Ok(MergeDone { note: acted.note, shown_at, done, done_error })
    }

    /// The reviews Codealong's agent wrote of a PR, the latest first (autoreview.rs `written`).
    fn pr_agent_reviews(c, key: String) -> Result<Vec<autoreview::WrittenReview>, String> {
        Ok(autoreview::written(&store::read_or_default(&autoreview::path()), &key))
    }

    /// Hands a PR of yours' latest review to its primary session now (autoreview.rs `send_now`).
    fn send_agent_review(c, key: String) -> Result<(), String> {
        autoreview::send_now(&c.engine, &c.reviews, &key)
    }

    /// Starts a review of a PR now, as Review with… asks (autoreview.rs `review_now`).
    fn review_pr_now(c, key: String, account: Option<String>) -> Result<(), String> {
        autoreview::review_now(&c.engine, &c.reviews, &key, account.as_deref())
    }

    /// Tries a PR's review that didn't get done again (autoreview.rs `again`). Says what it did.
    fn review_pr_again(c, key: String) -> Result<String, String> {
        autoreview::again(&c.engine, &c.reviews, &key)
    }

    /// What a new terminal's shell takes as a command's first word (runcode.rs `commands`).
    fn shell_commands(c) -> Result<Vec<String>, String> {
        Ok(runcode::commands())
    }

    /// Archives or unarchives a session; returns the time of the board that shows it.
    fn archive_session(c, key: String, archived: bool) -> Result<Option<i64>, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_ARCHIVE, &key, actions::params::Archive { archived, ..Default::default() }).map(shown_at)
    }

    /// Names a session where its harness keeps its name, so the harness shows it too.
    fn rename_session(c, key: String, name: String) -> Result<Option<i64>, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_RENAME, &key, actions::params::Rename { name, ..Default::default() }).map(shown_at)
    }

    /// Takes away the replies suggested for a session, on the device it's on.
    fn dismiss_suggestions(c, key: String) -> Result<Option<i64>, String> {
        actions::run(&c.engine, &c.peers, api::SESSION_SUGGEST, &key, actions::params::Suggest::default()).map(shown_at)
    }

    fn get_accounts(c) -> Result<Vec<model::AccountInfo>, String> {
        c.engine.accounts().map_err(words)
    }

    fn save_accounts(c, accounts: Vec<config::AccountCfg>) -> Result<(), String> {
        c.engine.save_accounts(accounts).map_err(words)
    }

    /// The usage bar's chips dragged into another order: the accounts' order in config.toml.
    fn order_accounts(c, accounts: Vec<String>) -> Result<(), String> {
        config::save_account_order(&accounts).map_err(words)
    }

    /// Where a start from the quick panel opened becomes `start` for its accounts in config.toml.
    fn keep_start_place(c, accounts: Vec<String>, place: start::Place) -> Result<(), String> {
        start::keep_place(&accounts, place).map_err(words)
    }

    /// What the setup wizard offers (setup.rs).
    fn setup_view(c) -> Result<setup::View, String> {
        setup::view_once_read().map_err(words)
    }

    /// Whether the main window shows the setup wizard by itself: a first run's, until it's
    /// finished or skipped.
    fn setup_needed(c) -> Result<bool, String> {
        Ok(setup::needed())
    }

    /// The setup wizard was finished or skipped: it shows again only from the app menu.
    fn finish_setup(c) -> Result<(), String> {
        c.engine.finish_setup();
        Ok(())
    }

    /// Accounts on this Mac that config.toml doesn't list, for Settings › Accounts.
    fn found_accounts(c) -> Result<Vec<detect::Found>, String> {
        detect::unlisted().map_err(words)
    }

    /// The settings as parsed and config.toml as written, for Settings.
    fn get_config(c) -> Result<config::ConfigView, String> {
        Ok(config::view())
    }

    /// Saves every setting from the Settings form, in place in config.toml.
    fn save_config(c, config: config::Config) -> Result<(), String> {
        c.engine.save_config(&config).map_err(words)
    }

    /// Takes what a repo's team suggests in its `.codealong/config.toml` on `branch`, writing it
    /// into your settings for that repo.
    fn take_team_settings(c, repo: String, branch: String) -> Result<(), String> {
        c.engine.take_team_settings(&repo, &branch).map_err(words)
    }

    /// What a folder typed in the quick panel's worktree menu is (newfolder.rs).
    fn look_folder(c, typed: String) -> Result<newfolder::Look, String> {
        Ok(newfolder::look(&c.engine, &typed))
    }

    /// Makes a folder typed in the worktree menu, puts it in a project, and answers its worktree
    /// once the board has it (newfolder.rs).
    fn add_folder(c, typed: String, project: Option<String>) -> Result<String, String> {
        newfolder::add(&c.engine, &typed, project.as_deref()).map_err(words)
    }

    /// Saves config.toml as typed in Settings, once it parses.
    fn save_config_text(c, text: String) -> Result<(), String> {
        c.engine.save_config_text(&text).map_err(words)
    }

    fn sign_in(c, name: String) -> Result<(), String> {
        c.engine.sign_in(&name).map_err(words)
    }

    /// Turns training off for an account at its provider (training.rs).
    fn stop_training(c, name: String) -> Result<training::Training, String> {
        c.engine.stop_training(&name).map_err(words)
    }

    /// The `codealong` command link and the skill per account, for Settings › Agents.
    fn agents_status(c) -> Result<install::AgentsStatus, String> {
        Ok(install::status())
    }

    /// Links the command and copies the skill for the named accounts, then reports the new state.
    fn install_agents(c, command: bool, accounts: Vec<String>) -> Result<install::AgentsStatus, String> {
        install::install(command, &accounts).map_err(words)
    }

    /// Your other devices as Settings › Devices and the views show them (peers.rs).
    fn devices(c) -> Result<peers::DevicesView, String> {
        Ok(c.peers.view())
    }

    /// Gives a linked device a color; none goes back to the one its key picks.
    fn set_device_color(c, key: String, color: Option<String>) -> Result<(), String> {
        c.peers.set_color(&key, color)
    }

    /// Asks one of your devices on your tailnet to link; returns the code both show.
    fn link_device(c, key: String) -> Result<String, String> {
        c.peers.link(&key)
    }

    fn answer_link(c, key: String, accept: bool) -> Result<(), String> {
        c.peers.answer_link(&key, accept)
    }

    fn unlink_device(c, key: String) -> Result<(), String> {
        c.peers.unlink(&key)
    }

    /// A link code for a device that isn't on your tailnet.
    fn device_link_code(c) -> Result<String, String> {
        c.peers.link_code()
    }

    /// Links with the device that made this link code; returns its name.
    fn link_with_code(c, code: String) -> Result<String, String> {
        c.peers.link_with_code(&code)
    }

    /// What was said in sessions on your other devices, searched on each of them.
    fn search_devices(c, words: Vec<String>) -> Result<Vec<peers::DeviceFound>, String> {
        let words = search::tidy(&words);
        Ok(if words.is_empty() { Vec::new() } else { c.peers.search(&words) })
    }

    /// Where each account can start a session, for the new-session sheet.
    fn start_options(c) -> Result<start::StartOptions, String> {
        actions::typed(actions::run_on(&c.engine, &c.peers, api::START_OPTIONS, None, serde_json::json!({}))?)
    }

    /// Where each account on another device can start a session, asked of that device.
    fn device_start_options(c, device: String) -> Result<start::StartOptions, String> {
        actions::typed(actions::run_on(&c.engine, &c.peers, api::START_OPTIONS, Some(&device), serde_json::json!({}))?)
    }

    /// Starts a session in a worktree: in the terminal, in the Claude or Codex app, in the
    /// background or in Codealong. A start from todos links each of them to the session. The app
    /// brings what it started in forward (lib.rs `start_session`).
    fn start_session(c, spec: start::StartSpec) -> Result<start::Started, String> {
        let lane = spec.lane_id.clone();
        actions::typed(actions::run(&c.engine, &c.peers, api::START, &lane, actions::params::Start { spec })?)
    }

    /// A start from the quick panel, kept for how your past starts weigh a new one (starts.rs).
    fn start_from_quick(c, spec: start::StartSpec) -> Result<start::Started, String> {
        let started = actions::typed(actions::run(&c.engine, &c.peers, api::START, &spec.lane_id, actions::params::Start { spec: spec.clone() })?)?;
        if crate::peerfeed::split_remote(&spec.lane_id).is_none() {
            start::note(&c.engine, &spec);
        }
        Ok(started)
    }

    /// Moves a session to another account of its harness (`start::move_to`), or, with `lane`, to
    /// that worktree (`start::move_to_folder`).
    fn move_session(c, key: String, account: Option<String>, lane: Option<String>) -> Result<start::Started, String> {
        let got = actions::run(&c.engine, &c.peers, actions::MOVE, &key, actions::params::Move { account: account.unwrap_or_default(), lane: lane.unwrap_or_default() })?;
        serde_json::from_value(got).map_err(|e| e.to_string())
    }

    /// Runs a command you ran for a session in a new tab of your terminal in `folder`, and has the
    /// session told what it printed once it ends (actions.rs `run_command`).
    fn run_command(c, key: String, folder: String, text: String, block: bool) -> Result<String, String> {
        let said = actions::run(&c.engine, &c.peers, actions::RUN_COMMAND, &key, actions::params::RunCommand { text, folder, block })?;
        Ok(said.as_str().unwrap_or_default().to_string())
    }

    /// Forks a session: into a new terminal in its folder, or, `background`, into a copy that
    /// goes on without a window, on `prompt` when there's one. With `from`, a prompt's cursor, the
    /// copy goes up to just before that prompt and starts on `prompt` in its place. With `side`,
    /// the id of one of its side conversations, the copy is told what was said there first.
    fn fork_session(c, key: String, background: Option<bool>, prompt: Option<String>, from: Option<u64>, side: Option<String>) -> Result<start::Started, String> {
        let got = if let Some(cursor) = from {
            actions::run(&c.engine, &c.peers, api::SESSION_EDIT, &key, actions::params::Edit { cursor, prompt: prompt.unwrap_or_default(), ..Default::default() })?
        } else if background == Some(true) {
            actions::run(&c.engine, &c.peers, api::SESSION_FORK, &key, actions::params::Fork { prompt: prompt.unwrap_or_default(), side, ..Default::default() })?
        } else {
            actions::run(&c.engine, &c.peers, actions::FORK_TERMINAL, &key, serde_json::json!({}))?
        };
        serde_json::from_value(got).map_err(|e| e.to_string())
    }

    /// Asks something about a session on the side (side.rs). Returns the side conversation with
    /// the question in it; the answer comes as `side` events.
    fn side_ask(c, key: String, text: String, from: Option<String>) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SIDE_ASK, &key, actions::params::Side { text, from, ..Default::default() })
    }

    /// A session's side conversation, for a window that starts showing the session.
    fn side_get(c, key: String) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SIDE_GET, &key, actions::params::Session::default())
    }

    /// The side conversations kept for a session, the latest first.
    fn side_past(c, key: String) -> Result<Value, String> {
        actions::run(&c.engine, &c.peers, api::SIDE_PAST, &key, actions::params::Session::default())
    }

    /// Forgets one of a session's kept side conversations, on its device.
    fn side_forget(c, key: String, id: String) -> Result<(), String> {
        actions::run(&c.engine, &c.peers, api::SIDE_FORGET, &key, actions::params::SideForget { id, ..Default::default() }).map(|_| ())
    }

    /// Opens a terminal of Codealong's own for a session (terminals.rs), in `folder`: a shell, or
    /// `command` run in one. `size` and `theme` are the window's, which it measured.
    fn terminal_open(c, key: String, folder: String, command: Option<String>, size: Option<terminals::Size>, theme: Option<terminals::Theme>) -> Result<terminals::Glance, String> {
        if c.engine.read(|b| b.session(&key).is_none()) {
            return Err("Codealong doesn't list that session.".into());
        }
        // A shell closes when it exits, as a tab does; a command stays to say how it ended.
        let close_on_exit = command.is_none();
        terminals::open(terminals::Open { session: key, folder: folder.into(), command, size, theme, close_on_exit, ..Default::default() })
    }

    /// A window shows a terminal (`on`), and is sent its frames while it says so again every few
    /// seconds; or stops showing it.
    fn terminal_watch(c, id: String, on: bool, window_label: Option<String>) -> Result<(), String> {
        terminals::watch(&id, window_label.as_deref().unwrap_or("main"), on)
    }

    /// What a window typed or did with the mouse in a terminal, in order.
    fn terminal_input(c, id: String, inputs: Vec<terminals::Input>) -> Result<(), String> {
        terminals::input(&id, inputs)
    }

    /// Pastes into a terminal. False when the text could run commands as it's pasted (a line
    /// break where the program doesn't bracket pastes) and `force` wasn't given.
    fn terminal_paste(c, id: String, text: String, force: bool) -> Result<bool, String> {
        terminals::paste(&id, text, force)
    }

    fn terminal_resize(c, id: String, size: terminals::Size) -> Result<(), String> {
        terminals::resize(&id, size)
    }

    /// A window's colors, for a terminal it shows.
    fn terminal_theme(c, id: String, theme: terminals::Theme) -> Result<(), String> {
        terminals::theme(&id, theme)
    }

    /// The text selected in a terminal.
    fn terminal_copy(c, id: String) -> Result<Option<String>, String> {
        terminals::copy(&id)
    }

    /// Everything a terminal holds as text, scrollback included.
    fn terminal_text(c, id: String) -> Result<String, String> {
        terminals::text(&id)
    }

    /// Closes a terminal, hanging up on what runs in it.
    fn terminal_close(c, id: String) -> Result<(), String> {
        terminals::close(&id)
    }

    /// Closes a session's side conversation (side.rs `close`); what was said is kept to read.
    fn side_close(c, key: String) -> Result<(), String> {
        actions::run(&c.engine, &c.peers, api::SIDE_CLOSE, &key, serde_json::json!({})).map(|_| ())
    }

    /// Asks about a PR (`key`) in a conversation of its own on `account` (side.rs `ask_pr`).
    fn pr_side_ask(c, key: String, account: String, model: Option<String>, effort: Option<String>, text: String, from: Option<String>) -> Result<side::Side, String> {
        let on = side::RunsOn { model: model.as_deref(), effort: effort.as_deref() };
        side::ask_pr(&c.engine.board(), &config::current().map_err(words)?, &key, &account, on, &text, from.as_deref()).map_err(words)
    }

    /// Closes a PR's conversation; what was said is kept to read.
    fn pr_side_close(c, key: String) -> Result<(), String> {
        side::close(&key);
        Ok(())
    }

    /// A project's boards, its own first, without their canvases (boards.rs).
    fn boards_list(c, project: String) -> Result<Vec<boards::BoardDoc>, String> {
        boards::list(&project).map_err(words)
    }

    /// A change to a board. Every window reads its project's boards again (useBoards.ts), but the
    /// one that made it (`window_label`), which has it.
    fn board_op(c, op: boards::BoardOp, window_label: Option<String>) -> Result<boards::Applied, String> {
        let applied = boards::apply(op, util::now_ms()).map_err(words)?;
        boards::changed(&applied, window_label.as_deref());
        Ok(applied)
    }

    /// A board's canvas as a window last saved it, or null.
    fn canvas_load(c, board: String) -> Result<Value, String> {
        boards::canvas(&board).map_err(words)
    }

    /// Saves a board's canvas. Another window showing the board reads it again (useCanvas.ts).
    fn canvas_save(c, board: String, doc: Value, window_label: Option<String>) -> Result<(), String> {
        boards::save_canvas(&board, doc).map_err(words)?;
        views::emit("canvas-saved", &serde_json::json!({ "board": board, "from": window_label }));
        Ok(())
    }

    /// The schedules the harnesses keep themselves (schedules/harnesses.rs), for the Schedules view.
    fn harness_schedules(c) -> Result<Vec<schedules::harnesses::Kept>, String> {
        let cfg = config::current().map_err(words)?;
        Ok(schedules::harnesses::list(&cfg, &c.engine.board(), util::now_ms()))
    }

    /// A change to the schedules (schedules.rs `Op`): what it came to.
    fn schedule_op(c, op: schedules::Op) -> Result<Value, String> {
        let cfg = config::current().map_err(words)?;
        let board = c.engine.board();
        schedules::apply_op(op, None, Some(&board), &cfg, util::now_ms(), &schedules::rule::Local).map_err(words)
    }

    /// A change to a todo or to the labels. Returns the time of the first board that shows it.
    fn todo_op(c, op: todos::TodoOp) -> Result<i64, String> {
        c.engine.todo(op)
    }

    /// Saves the theme to config.toml. Every window follows, through the board.
    fn set_theme(c, theme: config::Theme) -> Result<(), String> {
        c.engine.set_theme(theme).map_err(words)
    }

    /// Zooms every board window a step (zoom.rs), saved to config.toml; answers the zoom.
    fn zoom(c, step: zoom::Step) -> Result<u32, String> {
        c.engine.zoom(step).map_err(words)
    }

    /// Fetches an image a session on another device refers to, and keeps it here for the windows
    /// to show (actions.rs `image_from`, lib.rs `serve_image`).
    fn image_from(c, device: String, path: String) -> Result<(), String> {
        actions::image_from(&c.engine, &c.peers, &device, &path).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (Ctx, crate::fixtures::Scratch) {
        let dir = crate::fixtures::scratch("calls");
        let hooks = peers::Hooks { board: Box::new(model::Board::default), answer: Box::new(|_, _, _| Err("no".into())), emit: Box::new(|_| {}), side: Box::new(|_| {}) };
        (Ctx { engine: engine::Handle::over(model::Board::default()), peers: peers::Peers::new(dir.to_path_buf(), hooks), reviews: Default::default() }, dir)
    }

    #[test]
    fn a_call_reads_its_arguments_as_a_window_sends_them() {
        let (c, _dir) = ctx();
        // camelCase, as Tauri reads a command's; a missing Option is None.
        let plan = call(&c, "idle_sessions", serde_json::json!({ "afterMs": 60_000, "archive": false })).unwrap().unwrap();
        assert!(plan.is_object(), "{plan}");
        let err = call(&c, "idle_sessions", serde_json::json!({ "archive": false })).unwrap().unwrap_err();
        assert!(err.contains("idle_sessions") && err.contains("afterMs"), "{err}");
        // Arguments it doesn't take, such as the window it came from, are left alone.
        assert_eq!(call(&c, "jev_key", serde_json::json!({ "windowLabel": "main" })).map(|r| r.is_ok()), Some(true));
        assert!(call(&c, "no_such_call", serde_json::json!({})).is_none());
    }

    #[test]
    fn every_name_is_one_call() {
        let mut names = NAMES.to_vec();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), NAMES.len());
    }
}

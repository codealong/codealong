//! `codealong --snapshot …`: the engine without a window, for checking what it does. With no
//! mode it reads everything once and prints the board. A mode (`MODES`) prints one thing the app
//! shows, or does one thing it does, over a board read once. A session's modes are the app's own
//! actions (actions.rs), asked as another device asks them, so each does what a window's does,
//! for a session Codealong runs too, behind the same settings; and none of them claims to be you
//! at a window.

use serde::Serialize;
use serde_json::Value;

use crate::actions::{self, params, From};
use crate::{api, autoreview, barfeed, chrome, config, detect, editors, engine, github, harness, model, route, send, side, start, usage, util};

/// The command line, read one way for every mode.
struct Args<'a>(&'a [String]);

impl<'a> Args<'a> {
    fn has(&self, flag: &str) -> bool {
        self.0.iter().any(|a| a == flag)
    }

    /// What follows `flag` on the line, in order: a mode's own arguments. Nothing when the flag
    /// isn't there.
    fn after(&self, flag: &str) -> &'a [String] {
        self.0.iter().position(|a| a == flag).map(|i| &self.0[i + 1..]).unwrap_or_default()
    }

    /// An option's value: the argument after it, wherever it is on the line.
    fn option(&self, flag: &str) -> Option<&'a str> {
        self.after(flag).first().map(String::as_str)
    }
}

/// How a mode ends when it has nothing to print.
enum Stop {
    /// The line doesn't read as the mode's: its usage is printed, and the exit code is 2.
    Usage,
    /// It went wrong, in these words: exit code 1.
    Failed(String),
}

/// What a mode prints; nothing for an empty text.
type Done = Result<String, Stop>;

fn failed(e: anyhow::Error) -> Stop {
    Stop::Failed(util::words(e))
}

fn pretty(v: &impl Serialize) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

struct Mode {
    flag: &'static str,
    /// Its arguments, after `codealong --snapshot <flag>`.
    takes: &'static str,
    /// Gets the whole line, and what follows its flag.
    run: fn(&Args, &[String]) -> Done,
}

/// Every mode, in the order the line is looked through for them: the first one there runs.
const MODES: &[Mode] = &[
    Mode { flag: "--pending", takes: "<session key>", run: pending },
    Mode { flag: "--answer", takes: "<session key> <id> <allow|deny|always> [answers to its questions, as JSON]", run: answer },
    Mode { flag: "--stop", takes: "<session key>", run: stop },
    Mode { flag: "--steer", takes: "<session key> <queued message id>", run: steer },
    Mode { flag: "--done", takes: "<session key> [--stop]", run: done },
    Mode { flag: "--remove-worktree", takes: "<worktree folder> [--dry-run]", run: remove_worktree },
    Mode { flag: "--send", takes: "<session key> <text> [--now]", run: send_text },
    Mode { flag: "--tell", takes: "<session key> <text>", run: tell },
    Mode { flag: "--fork", takes: "<session key> [prompt] [--from <cursor>]", run: fork },
    Mode { flag: "--switch-model", takes: "<session key> <model>", run: switch_model },
    Mode { flag: "--switch-effort", takes: "<session key> <level>", run: switch_effort },
    Mode { flag: "--switch-mode", takes: "<session key> <mode id>", run: switch_mode },
    Mode { flag: "--resume", takes: "<session key> <terminal|background|claude-app|codex-app|codealong> [prompt] [--model <model>] [--effort <level>]", run: resume },
    Mode { flag: "--move", takes: "<session key> <account | worktree folder>", run: move_session },
    Mode { flag: "--side", takes: "<session key> <question> [a follow-up…]", run: side_ask },
    Mode { flag: "--rename", takes: "<session key> <name>", run: rename },
    Mode { flag: "--transcript", takes: "<session key> [--before <cursor>]", run: transcript },
    Mode { flag: "--editor-icons", takes: "", run: editor_icons },
    Mode { flag: "--shell-commands", takes: "", run: |_, _| Ok(serde_json::to_string(&crate::runcode::commands()).unwrap_or_default()) },
    Mode { flag: "--start-options", takes: "", run: start_options },
    Mode { flag: "--skills", takes: "[folder…]", run: skills },
    Mode { flag: "--start-plan", takes: "<folder> <account> <terminal|background|claude-app|codex-app> [prompt] [--model <model>] [--effort <level>] [--worktree]", run: start_plan },
    Mode { flag: "--route", takes: "<text> [--project <name>]", run: route_text },
    Mode { flag: "--routes", takes: "[days]", run: routes },
    Mode { flag: "--config", takes: "", run: |_, _| Ok(pretty(&config::view())) },
    Mode { flag: "--setup", takes: "", run: |_, _| Ok(pretty(&crate::setup::view().map_err(|e| Stop::Failed(format!("{e:#}")))?)) },
    Mode { flag: "--found", takes: "", run: |_, _| Ok(pretty(&detect::unlisted().map_err(|e| Stop::Failed(format!("{e:#}")))?)) },
    Mode { flag: "--accounts", takes: "", run: accounts },
    Mode { flag: "--training-off", takes: "<account>", run: training_off },
    Mode { flag: "--training", takes: "", run: |_, _| Ok(pretty(&crate::training::read_all(&config::current().map_err(failed)?.accounts))) },
    Mode { flag: "--mcp", takes: "", run: |_, _| Ok(pretty(&crate::mcp::read_all(&config::current().map_err(failed)?.accounts))) },
    Mode { flag: "--pr-side", takes: "<owner/name> <number> <account> <question> [a follow-up…] [--model <model>] [--effort <level>]", run: pr_side },
    Mode { flag: "--pr-card", takes: "<owner/name> <number>", run: pr_card },
    Mode { flag: "--pr-draft", takes: "<owner/name> <number>", run: |_, rest| { let (repo, number) = repo_and_number(rest)?; Ok(pretty(&crate::github::your_review(repo, number).map_err(failed)?)) } },
    Mode { flag: "--pr-since", takes: "<owner/name> <commit> <commit>", run: |_, rest| match rest { [repo, from, to] => Ok(pretty(&github::since(repo, from, to).map_err(failed)?)), _ => Err(Stop::Usage) } },
    Mode { flag: "--pr", takes: "<owner/name> <number>", run: pr },
    Mode { flag: "--merge-box", takes: "<owner/name> <number>", run: merge_box },
    Mode { flag: "--review-plan", takes: "<owner/name> <number>", run: review_plan },
    Mode { flag: "--open", takes: "<lane id> <target id>", run: open },
    Mode { flag: "--link", takes: "<url>", run: link },
    Mode { flag: "--checkout", takes: "", run: |_, _| Ok(crate::CHECKOUT.unwrap_or_default().to_string()) },
    Mode { flag: "--improve", takes: "", run: |_, _| Ok(pretty(&board_now().read(|b| b.improve.clone()))) },
    Mode { flag: "--tokens", takes: "[days] [--account <name>] [--session <key>] [--into <ledger file>]", run: tokens },
    Mode { flag: "--jev-usage", takes: "[days]", run: jev_usage },
    Mode { flag: "--stale-terminals", takes: "[how long unused: 90m, 24h, 3d, 1w]", run: stale_terminals },
    Mode { flag: "--schedules", takes: "", run: schedules },
    Mode { flag: "--schedule-plan", takes: "<schedule>", run: schedule_plan },
    Mode { flag: "--schedule-check", takes: "<schedule>", run: schedule_check },
    Mode { flag: "--schedule-run", takes: "<schedule>", run: schedule_run },
    Mode { flag: "--schedule-times", takes: "<time, as a schedule writes it>", run: |_, rest| Ok(pretty(&crate::schedules::times(&rest.join(" "), util::now_ms(), &crate::schedules::rule::Local))) },
];

/// `codealong --snapshot --stale-terminals [wait]`: the terminal tabs File › Close stale
/// terminals… would list, unused for the wait (a day when left out), closing nothing.
fn stale_terminals(_: &Args, rest: &[String]) -> Done {
    let after_ms = match rest.first() {
        Some(wait) => crate::idle::parse_after(wait).ok_or(Stop::Usage)?,
        None => util::DAY_MS,
    };
    Ok(pretty(&crate::stale::plan_now(&engine::snapshot(false, false), after_ms)))
}

/// `codealong --snapshot --schedules`: the schedules as the Schedules view lists them, with when
/// each runs next and its latest runs.
fn schedules(_: &Args, _: &[String]) -> Done {
    let board = engine::snapshot(false, false);
    let cfg = config::current().map_err(failed)?;
    let (file, runs) = (crate::schedules::load().map_err(failed)?, crate::schedules::load_runs().map_err(failed)?);
    Ok(pretty(&crate::schedules::items(&file, &runs, &board, &cfg, util::now_ms(), &crate::schedules::rule::Local)))
}

/// A schedule by its name or id, with what's noted of it.
fn schedule_named(rest: &[String]) -> Result<(crate::schedules::Schedule, crate::schedules::Track), Stop> {
    let [name, ..] = rest else { return Err(Stop::Usage) };
    let file = crate::schedules::load().map_err(failed)?;
    let s = file.find(name).cloned().ok_or_else(|| Stop::Failed(format!("There's no schedule named {name}.")))?;
    let track = crate::schedules::load_runs().map_err(failed)?.of.get(&s.id).cloned().unwrap_or_default();
    Ok((s, track))
}

/// `codealong --snapshot --schedule-plan <schedule>`: what a run of it would do now, as Run now
/// would, without starting or sending anything: its prompt, and the session it would start (on
/// the account picked now) or send to, and how.
fn schedule_plan(_: &Args, rest: &[String]) -> Done {
    let (s, track) = schedule_named(rest)?;
    let board = engine::snapshot(true, false);
    let cfg = config::current().map_err(failed)?;
    Ok(pretty(&crate::schedules::runner::plan_now(&board, &cfg, &s, &track, util::now_ms())))
}

/// `codealong --snapshot --schedule-run <schedule>`: runs it now, as Run now does, over a board
/// read now, and notes the run in schedule-runs.json: a session starts, or one is sent to, for
/// real. Asked as another device asks, so what it sends isn't yours. Prints what it came to.
fn schedule_run(_: &Args, rest: &[String]) -> Done {
    let (s, _) = schedule_named(rest)?;
    let engine = board_now();
    let record = crate::schedules::runner::run_once(&engine, &s.id).ok_or_else(|| Stop::Failed("It didn't run: its files couldn't be read.".into()))?;
    Ok(pretty(&record))
}

/// `codealong --snapshot --schedule-check <schedule>`: runs its watch's command (or its check
/// before each run) once and prints what it printed and which of it would be new. Nothing is
/// noted or run.
fn schedule_check(_: &Args, rest: &[String]) -> Done {
    let (s, track) = schedule_named(rest)?;
    let board = board_now().board();
    Ok(pretty(&crate::schedules::runner::check_now(&s, &track, &board)))
}

/// `codealong --snapshot --jev-usage [days]`: what Settings › Jev says Codealong's requests to Jev
/// used each of the last `days` days (30 unless given), read from the app's ledger.
fn jev_usage(_: &Args, rest: &[String]) -> Done {
    let days: u32 = match rest.first() {
        Some(d) => d.parse().map_err(|_| Stop::Usage)?,
        None => 30,
    };
    Ok(pretty(&crate::ledger::jev_report(days).map_err(failed)?))
}

/// `codealong --snapshot --routes [days]`: how often what you did with what you typed in the quick
/// panel was Jev's first suggestion, over the last `days` (7 by default), with the times it
/// wasn't (routelog.rs).
fn routes(_: &Args, rest: &[String]) -> Done {
    let days: u32 = match rest.first() {
        Some(d) => d.parse().map_err(|_| Stop::Usage)?,
        None => 7,
    };
    Ok(pretty(&crate::routelog::report(crate::routelog::since_days(days, crate::util::now_ms())).map_err(failed)?))
}

/// `codealong --snapshot --training-off <account>`: turns training off for the account at its
/// provider, as the usage bar's flag does, and prints where it stands after. Real.
fn training_off(_: &Args, rest: &[String]) -> Done {
    let [name] = rest else { return Err(Stop::Usage) };
    let cfg = config::current().map_err(failed)?;
    let a = cfg.accounts.iter().find(|a| &a.name == name).ok_or_else(|| Stop::Failed(format!("There's no account named {name}.")))?;
    Ok(pretty(&crate::training::stop(a).map_err(failed)?))
}

/// `codealong --snapshot --tokens [days]`: what the Usage window shows for the last `days` days
/// (30 unless given), from this Mac's records read afresh into a ledger of the check's own (a
/// temporary file, or `--into` one to keep and read on from), never the app's. Only files written
/// in those days are read. `--session <key>` prints that session's instead.
fn tokens(args: &Args, rest: &[String]) -> Done {
    let days: u32 = match rest.first().filter(|d| !d.starts_with("--")) {
        Some(d) => d.parse().map_err(|_| Stop::Usage)?,
        None => 30,
    };
    let cfg = config::current().map_err(failed)?;
    let keep = args.option("--into").map(std::path::PathBuf::from);
    let file = keep.clone().unwrap_or_else(|| std::env::temp_dir().join(format!("codealong-ledger-check-{}.sqlite", std::process::id())));
    let db = crate::ledger::open(&file).map_err(failed)?;
    let since = util::now_ms() - (days as i64 + 1) * util::DAY_MS;
    let caught = crate::ledger::catch_up_since(&db, &cfg.accounts, if args.option("--session").is_some() { 0 } else { since });
    let out = match args.option("--session") {
        Some(key) => pretty(&crate::ledger::session_in(&db, &cfg, key).map_err(failed)?),
        None => {
            let mut r = crate::ledger::report_in(&db, &cfg, days, args.option("--account"), util::now_ms()).map_err(failed)?;
            r.problem = caught.problem();
            pretty(&r)
        }
    };
    drop(db);
    if keep.is_none() {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", file.display()));
        }
    }
    Ok(out)
}

/// `codealong --snapshot [--table] [--no-github] [--usage]`: one pass over everything, printed as
/// JSON, or as a table. GitHub is asked unless `--no-github` says not to; Claude usage is read
/// only with `--usage`.
/// `codealong --snapshot --bar [--no-github]`: what the floating bar would show.
/// `codealong --snapshot --accounts`: the configured accounts, each with whether it's signed in.
/// `codealong --snapshot --found`: accounts on this Mac that config.toml doesn't list.
/// `codealong --snapshot --config`: the settings and config.toml, as Settings gets them.
/// `codealong --snapshot --editor-icons`: the installed editors' app icons, as the Open in
/// menus get them.
/// `codealong --snapshot --shell-commands`: what a new terminal's shell takes as a command's
/// first word, as a conversation's code blocks get it for Run.
/// `codealong --snapshot --skills [folder…]`: the skills a session in each folder could be asked
/// to run by name, and each account's own, as the `/` lists get them.
/// `codealong --snapshot --open <lane id> <target id>`: opens a target the way a click does.
/// `codealong --snapshot --link <url>`: where a click on a web link in a conversation opens it,
/// without opening it.
/// `codealong --snapshot --pr <owner/name> <number>`: a PR's detail, as the PR panel gets it.
/// `codealong --snapshot --pr-card <owner/name> <number>`: a PR the board doesn't list, as a link
/// to it in a conversation gets it.
/// `codealong --snapshot --merge-box <owner/name> <number>`: what a PR's merge box offers you, as
/// its panel gets it. It only reads: what the box does is GitHub's, on the PR.
/// `codealong --snapshot --pr-draft <owner/name> <number>`: your draft review of a PR with its
/// comments, and an agent's review of yours you submitted, as its Review tab gets them.
/// `codealong --snapshot --route <text> [--project <name>]`: where the quick panel would suggest
/// sending it (asks TypeSafe; needs the key, and `[jev] quick_panel` on).
/// `codealong --snapshot --jev-usage [days]`: what Codealong's requests to Jev used, by day and
/// feature, as Settings › Jev gets it.
/// `codealong --snapshot --transcript <session key> [--before <cursor>]`: a page of a session's
/// conversation, as its panel gets it.
/// `codealong --snapshot --pending <session key>`: what a session waits on you to allow, as its
/// panel shows it.
/// `codealong --snapshot --answer <session key> <id> <allow|deny|always> [answers]`: answers it,
/// as the panel's buttons do; `answers` are its questions' answers, as JSON.
/// `codealong --snapshot --send <session key> <text> [--now]`: sends a session a message, as
/// another device does; `--now` sends it into the turn it's taking, as Send now does.
/// `codealong --snapshot --tell <session key> <text>`: sends it what Codealong sends by itself, as
/// a PR's news or a review handed back goes (send.rs `tell`).
/// `codealong --snapshot --rename <session key> <name>`: names a session where its harness keeps
/// names, as Rename… in its menu does.
/// `codealong --snapshot --stop <session key>`: stops the turn it's taking, as its Stop does.
/// `codealong --snapshot --done <session key> [--stop]`: ends it as Mark as done does, `--stop`
/// ending work under way too, as Mark as done pressed again does. Marking it seen and archiving
/// it are the app engine's, so nothing here keeps them.
/// `codealong --snapshot --remove-worktree <worktree folder> [--dry-run]`: removes a worktree when
/// nothing in it would be lost, as its menu's Clean up worktree does; `--dry-run` says whether it
/// can go and what would run.
/// `codealong --snapshot --fork <session key> [prompt]`: forks it into the background, as `/fork`
/// in its box does.
/// `codealong --snapshot --resume <session key> <place> [prompt] [--model <model>] [--effort
/// <level>]`: goes on with a session that isn't running there, as its box and Resume in do.
/// `codealong --snapshot --switch-model <session key> <model>`: switches a running session's
/// model, or a Codex app thread's for its next turn, as its box does.
/// `codealong --snapshot --switch-mode <session key> <mode id>`: switches a session to one of the
/// modes it offers (`Session::modes`), as its mode chip does: one in a terminal by Shift+Tab
/// pressed there (cycle.rs). What Codealong expects of a Claude Code session's mode after it
/// lasts only as long as the check, which ends there.
/// `codealong --snapshot --move <session key> <account | worktree folder>`: moves it to another
/// account of its harness, or to a worktree (a lane's folder: Codealong's chats for a chat of its
/// own), as Move to in its menu does.
/// `codealong --snapshot --side <session key> <question> [a follow-up…]`: asks a throwaway copy
/// of it, as `/side` in its box does, and prints the questions with their answers.
/// `codealong --snapshot --pr-side <owner/name> <number> <account> <question> [a follow-up…]`:
/// asks about a PR the board lists on an account, as Ask in its card's box does, and prints the
/// questions with their answers.
/// `codealong --snapshot --start-options`: where each account can start a session.
/// `codealong --snapshot --start-plan <folder> <account> <place> [prompt] [--model <model>]
/// [--effort <level>] [--worktree]`: what starting one would do, without doing it: where it would
/// go now, and for a terminal whether it would run in tmux (`start::plan_now`).
pub fn cli(args: &[String]) -> i32 {
    let args = Args(args);
    let Some(mode) = MODES.iter().find(|m| args.has(m.flag)) else {
        return print_board(&args);
    };
    match (mode.run)(&args, args.after(mode.flag)) {
        Ok(text) => {
            if !text.is_empty() {
                println!("{text}");
            }
            0
        }
        Err(Stop::Usage) => {
            eprintln!("usage: codealong --snapshot {} {}", mode.flag, mode.takes);
            2
        }
        Err(Stop::Failed(why)) => {
            eprintln!("{why}");
            1
        }
    }
}

/// The board, read once with git and, unless the line says otherwise, GitHub: as JSON, as a
/// table, or as the floating bar's feed.
fn print_board(args: &Args) -> i32 {
    let started = std::time::Instant::now();
    let board = engine::snapshot(!args.has("--no-github"), args.has("--usage"));
    if args.has("--bar") {
        println!("{}", pretty(&barfeed::feed(&board)));
        return 0;
    }
    if args.has("--table") {
        print_table(&board);
    } else {
        match serde_json::to_string_pretty(&board) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        }
    }
    eprintln!("snapshot: {} lanes in {} ms", board.lanes.len(), started.elapsed().as_millis());
    0
}

fn print_table(board: &model::Board) {
    for n in &board.notices {
        println!("! {}", n.text);
    }
    for p in &board.projects {
        let repos: Vec<String> = p.repos.iter().map(|r| format!("{} {}", r.name, r.lanes)).collect();
        println!("# {} ({} lanes): {}", p.name, p.lanes, repos.join(", "));
    }
    for lane in board.lanes.iter().filter(|l| !l.hidden) {
        let sessions: Vec<String> = lane.sessions.iter().take(4).map(|s| format!("{}:{:?}", s.account, s.status).to_lowercase()).collect();
        let pr = lane.pr.as_ref().map(|p| format!("#{} {:?}", p.number, p.state).to_lowercase()).unwrap_or_default();
        println!(
            "{:<8} {:<30} {:<14} {:<60} | {:<40} [{}] {}{}",
            format!("{:?}", lane.bay).to_lowercase(),
            util::truncate(&lane.callsign, 30),
            util::truncate(&lane.repo, 14),
            util::truncate(&lane.headline, 60),
            util::truncate(lane.detail.as_deref().unwrap_or(""), 40),
            sessions.join(" "),
            pr,
            if lane.parked { " (parked)" } else { "" }
        );
    }
}

/// The board the modes act on: read now, with git but without GitHub or usage, and no engine
/// behind it.
fn board_now() -> engine::Handle {
    engine::Handle::over(engine::snapshot(false, false))
}

/// One of the app's actions on a session of this device, over a board read now. It's asked as
/// another device asks it: a send isn't yours, and one that would only open the session for
/// someone to send is an error.
fn act(name: &str, key: &str, params: impl serde::Serialize) -> Result<Value, Stop> {
    actions::local(&board_now(), &From::Device, name, key, params).map_err(Stop::Failed)
}

/// What an action said happened, in its sentence.
fn said(v: Value) -> Done {
    Ok(v.as_str().unwrap_or_default().to_string())
}

fn pending(_: &Args, rest: &[String]) -> Done {
    let [key, ..] = rest else { return Err(Stop::Usage) };
    Ok(pretty(&act(api::SESSION_PENDING, key, params::Session::default())?))
}

fn answer(_: &Args, rest: &[String]) -> Done {
    let [key, id, answer, more @ ..] = rest else { return Err(Stop::Usage) };
    let answer: harness::Answer = serde_json::from_value(Value::String(answer.clone())).map_err(|_| Stop::Usage)?;
    let answers: Option<Value> = more.first().map(|a| serde_json::from_str(a)).transpose().map_err(|_| Stop::Usage)?;
    said(act(api::SESSION_ANSWER, key, params::Answer { id: id.clone(), answer: Some(answer), answers, ..Default::default() })?)
}

fn stop(_: &Args, rest: &[String]) -> Done {
    let [key, ..] = rest else { return Err(Stop::Usage) };
    said(act(api::SESSION_CANCEL, key, params::Session::default())?)
}

fn steer(_: &Args, rest: &[String]) -> Done {
    let [key, id, ..] = rest else { return Err(Stop::Usage) };
    said(act(api::SESSION_STEER, key, params::Steer { id: id.clone(), ..Default::default() })?)
}

fn done(args: &Args, rest: &[String]) -> Done {
    let [key, ..] = rest else { return Err(Stop::Usage) };
    let v = act(api::SESSION_DONE, key, params::Done { stop: args.has("--stop"), ..Default::default() })?;
    Ok(v.get("message").and_then(Value::as_str).unwrap_or_default().to_string())
}

fn remove_worktree(args: &Args, rest: &[String]) -> Done {
    let [folder, ..] = rest else { return Err(Stop::Usage) };
    let v = act(api::WORKTREE_REMOVE, folder, params::Remove { dry_run: args.has("--dry-run"), ..Default::default() })?;
    Ok(pretty(&v))
}

fn send_text(args: &Args, rest: &[String]) -> Done {
    let [key, text, ..] = rest else { return Err(Stop::Usage) };
    let sent: send::Sent = serde_json::from_value(act(api::SESSION_SEND, key, params::Send { text: text.clone(), now: args.has("--now"), ..Default::default() })?).map_err(|e| Stop::Failed(e.to_string()))?;
    Ok(sent.message)
}

fn tell(_: &Args, rest: &[String]) -> Done {
    let [key, text, ..] = rest else { return Err(Stop::Usage) };
    let cfg = config::current().map_err(failed)?;
    let board = engine::snapshot(false, false);
    Ok(send::send(&board, &cfg, key, text, send::Sender::Codealong, false).map_err(failed)?.message)
}

fn fork(args: &Args, rest: &[String]) -> Done {
    let [key, more @ ..] = rest else { return Err(Stop::Usage) };
    let prompt = more.first().filter(|p| !p.starts_with("--")).cloned().unwrap_or_default();
    match args.option("--from") {
        Some(cursor) => {
            let cursor = cursor.parse().map_err(|_| Stop::Usage)?;
            Ok(pretty(&act(api::SESSION_EDIT, key, params::Edit { cursor, prompt, ..Default::default() })?))
        }
        None => Ok(pretty(&act(api::SESSION_FORK, key, params::Fork { prompt, ..Default::default() })?)),
    }
}

fn switch_model(_: &Args, rest: &[String]) -> Done {
    let [key, model, ..] = rest else { return Err(Stop::Usage) };
    said(act(api::SESSION_MODEL, key, params::Model { model: model.clone(), ..Default::default() })?)
}

fn switch_effort(_: &Args, rest: &[String]) -> Done {
    let [key, effort, ..] = rest else { return Err(Stop::Usage) };
    said(act(api::SESSION_EFFORT, key, params::Effort { effort: effort.clone(), ..Default::default() })?)
}

fn switch_mode(_: &Args, rest: &[String]) -> Done {
    let [key, mode, ..] = rest else { return Err(Stop::Usage) };
    let switched: send::ModeSwitched = serde_json::from_value(act(api::SESSION_MODE, key, params::Mode { mode: mode.clone(), ..Default::default() })?).map_err(|e| Stop::Failed(e.to_string()))?;
    Ok(format!("{} (now {})", switched.message, switched.mode))
}

fn resume(args: &Args, rest: &[String]) -> Done {
    let [key, place, more @ ..] = rest else { return Err(Stop::Usage) };
    let place: start::Place = serde_json::from_value(Value::String(place.clone())).map_err(|_| Stop::Usage)?;
    let prompt = more.first().filter(|p| !p.starts_with("--")).cloned().unwrap_or_default();
    let resume = start::ResumeIn { place, model: args.option("--model").map(String::from), effort: args.option("--effort").map(String::from) };
    let sent: send::Sent = serde_json::from_value(act(api::SESSION_RESUME, key, params::Resume { resume, prompt, ..Default::default() })?).map_err(|e| Stop::Failed(e.to_string()))?;
    Ok(sent.message)
}

fn move_session(_: &Args, rest: &[String]) -> Done {
    let [key, to, ..] = rest else { return Err(Stop::Usage) };
    // A worktree is named by its folder, an account by its name, which has no slash.
    let to = if to.contains('/') { params::Move { lane: to.clone(), ..Default::default() } } else { params::Move { account: to.clone(), ..Default::default() } };
    Ok(pretty(&act(actions::MOVE, key, to)?))
}

fn side_ask(_: &Args, rest: &[String]) -> Done {
    let [key, questions @ ..] = rest else { return Err(Stop::Usage) };
    if questions.is_empty() {
        return Err(Stop::Usage);
    }
    let engine = board_now();
    for q in questions {
        if let Err(e) = actions::local(&engine, &From::Device, api::SIDE_ASK, key, params::Side { text: q.clone(), ..Default::default() }) {
            side::close_now(key);
            return Err(Stop::Failed(e));
        }
    }
    answered(key)
}

/// Waits for `key`'s side conversation to answer what it was asked, prints it and closes it.
fn answered(key: &str) -> Done {
    // Answers take as long as they take; a copy that stops says so.
    let until = std::time::Instant::now() + std::time::Duration::from_secs(600);
    while side::get(key).is_some_and(|s| s.running) && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let asked = pretty(&side::get(key));
    side::close_now(key);
    // The copy's process gets a moment to leave by itself.
    std::thread::sleep(std::time::Duration::from_millis(1700));
    Ok(asked)
}

fn pr_side(args: &Args, rest: &[String]) -> Done {
    let [repo, number, account, more @ ..] = rest else { return Err(Stop::Usage) };
    let questions: Vec<&String> = more.iter().take_while(|q| !q.starts_with("--")).collect();
    if questions.is_empty() || number.parse::<u64>().is_err() {
        return Err(Stop::Usage);
    }
    let key = format!("{repo}#{number}");
    let board = engine::snapshot(true, false);
    let cfg = config::current().map_err(|e| Stop::Failed(format!("{e:#}")))?;
    let on = side::RunsOn { model: args.option("--model"), effort: args.option("--effort") };
    for q in questions {
        if let Err(e) = side::ask_pr(&board, &cfg, &key, account, on, q) {
            side::close_now(&key);
            return Err(Stop::Failed(format!("{e:#}")));
        }
    }
    answered(&key)
}

fn rename(_: &Args, rest: &[String]) -> Done {
    let [key, name, ..] = rest else { return Err(Stop::Usage) };
    // The name as the action keeps it, for saying what it's called now.
    let name = harness::session_name(name).map_err(failed)?;
    act(api::SESSION_RENAME, key, params::Rename { name: name.clone(), ..Default::default() })?;
    Ok(format!("Renamed it {name}."))
}

fn transcript(args: &Args, rest: &[String]) -> Done {
    let [key, ..] = rest else { return Err(Stop::Usage) };
    let before: Option<u64> = args.option("--before").and_then(|b| b.parse().ok());
    Ok(pretty(&act(api::SESSION_PAGE, key, params::Page { before, ..Default::default() })?))
}

fn editor_icons(_: &Args, _: &[String]) -> Done {
    Ok(serde_json::to_string(&editors::icons()).unwrap_or_default())
}

fn start_options(_: &Args, _: &[String]) -> Done {
    let cfg = config::current().map_err(|e| Stop::Failed(format!("{e:#}")))?;
    // A harness whose list of models is asked for in the background (Antigravity's) has
    // none on the first ask: the app asks as it starts, and a one-off waits a moment here.
    let mut options = start::options_for(&cfg);
    let waits = |o: &start::StartOptions| o.accounts.iter().any(|a| a.models.is_empty() && !a.places.is_empty() && harness::adapter(a.harness).takes_model());
    for _ in 0..20 {
        if !waits(&options) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
        options = start::options_for(&cfg);
    }
    Ok(pretty(&options))
}

/// The skills sessions in the folders given could be asked to run by name, and every account's
/// own, as the `/` lists get them.
fn skills(_: &Args, rest: &[String]) -> Done {
    let cfg = config::current().map_err(|e| Stop::Failed(format!("{e:#}")))?;
    let folders: Vec<String> = rest.iter().filter(|a| !a.starts_with("--")).cloned().collect();
    let accounts: Vec<String> = cfg.accounts.iter().map(|a| a.name.clone()).collect();
    Ok(pretty(&crate::skills::list(&folders, &accounts, &cfg)))
}

fn start_plan(args: &Args, rest: &[String]) -> Done {
    let [folder, name, place, more @ ..] = rest else { return Err(Stop::Usage) };
    let place: start::Place = serde_json::from_value(Value::String(place.clone())).map_err(|_| Stop::Usage)?;
    let cfg = config::current().map_err(|e| Stop::Failed(format!("{e:#}")))?;
    let Some(account) = cfg.accounts.iter().find(|a| &a.name == name) else {
        return Err(Stop::Failed(format!("There's no account named {name}.")));
    };
    let prompt = more.first().filter(|p| !p.starts_with("--")).cloned().unwrap_or_default();
    let plan = start::plan_now(&cfg, account, place, &util::expand(folder), &prompt, args.option("--model"), args.option("--effort"), args.has("--worktree")).map_err(|e| Stop::Failed(format!("{e:#}")))?;
    Ok(pretty(&plan))
}

fn route_text(args: &Args, rest: &[String]) -> Done {
    let [text, ..] = rest else { return Err(Stop::Usage) };
    let project = args.option("--project").unwrap_or(route::ALL_PROJECTS);
    // The accounts' MCP servers, which the app reads in the background, for Jev's `needs`, and
    // what was said in the sessions, which it indexes in the background, for what they share
    // with the text.
    let mut board = engine::snapshot(false, false);
    let cfg = config::current().map_err(failed)?;
    board.mcp = crate::mcp::read_all(&cfg.accounts);
    crate::search::build_now(crate::search::Scope { accounts: cfg.accounts.clone(), days: cfg.search.days });
    let params = params::Route { project: project.to_string(), text: text.clone() };
    Ok(pretty(&actions::local(&engine::Handle::over(board), &From::Device, api::ROUTE, "", params).map_err(Stop::Failed)?))
}

fn accounts(_: &Args, _: &[String]) -> Done {
    let cfg = config::current().map_err(|e| Stop::Failed(format!("{e:#}")))?;
    let list: Vec<model::AccountInfo> = cfg.accounts.iter().map(usage::describe).collect();
    Ok(pretty(&list))
}

/// A repo and a PR's number, as two arguments.
fn repo_and_number(rest: &[String]) -> Result<(&str, u64), Stop> {
    match rest {
        [repo, number, ..] => number.parse().map(|n| (repo.as_str(), n)).map_err(|_| Stop::Usage),
        _ => Err(Stop::Usage),
    }
}

fn pr_card(_: &Args, rest: &[String]) -> Done {
    let (repo, number) = repo_and_number(rest)?;
    Ok(pretty(&engine::snapshot_pr_card(repo, number).map_err(failed)?))
}

fn pr(_: &Args, rest: &[String]) -> Done {
    let (repo, number) = repo_and_number(rest)?;
    Ok(pretty(&github::detail(repo, number).map_err(failed)?))
}

/// What an automatic review of the PR would start (autoreview.rs `plan`), without starting it.
fn review_plan(_: &Args, rest: &[String]) -> Done {
    let (repo, number) = repo_and_number(rest)?;
    let board = engine::snapshot(true, false);
    let cfg = config::current().map_err(|e| Stop::Failed(format!("{e:#}")))?;
    Ok(pretty(&autoreview::plan(&board, &cfg, &format!("{repo}#{number}")).map_err(failed)?))
}

fn merge_box(_: &Args, rest: &[String]) -> Done {
    let (repo, number) = repo_and_number(rest)?;
    Ok(pretty(&crate::prmerge::merge_box(repo, number).map_err(failed)?))
}

fn open(_: &Args, rest: &[String]) -> Done {
    let [lane, target, ..] = rest else { return Err(Stop::Usage) };
    engine::open_on(&engine::snapshot(false, false), lane, target).map_err(failed)?;
    Ok(String::new())
}

/// Where a click on a web link opens it, said without opening it.
fn link(_: &Args, rest: &[String]) -> Done {
    let [url, ..] = rest else { return Err(Stop::Usage) };
    let Some(lane) = engine::link_opens_as(&engine::snapshot(false, false), url) else {
        return Ok("In the default browser: it isn't a link into a worktree's app that opens in a Chrome profile.".into());
    };
    let name = lane.preview.as_ref().and_then(|p| p.chrome_profile.clone()).unwrap_or_default();
    Ok(match chrome::resolve(&name) {
        Ok(p) => format!("In Chrome's {} profile ({}), as {}'s preview opens.", p.name, p.dir, lane.path),
        Err(e) => format!("In the default browser, though it's {}'s app: {}", lane.path, util::words(e)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(words: &[&str]) -> Vec<String> {
        std::iter::once("codealong").chain(words.iter().copied()).map(String::from).collect()
    }

    #[test]
    fn a_modes_arguments_follow_its_flag_and_options_go_anywhere() {
        let l = line(&["--snapshot", "--transcript", "claude:claude:abc", "--before", "4096"]);
        let args = Args(&l);
        assert!(args.has("--transcript") && !args.has("--table"));
        assert_eq!(args.after("--transcript").first().map(String::as_str), Some("claude:claude:abc"));
        assert_eq!(args.option("--before"), Some("4096"));
        assert!(args.after("--send").is_empty() && args.option("--project").is_none());
        // An option ahead of the mode is still its option.
        let l = line(&["--snapshot", "--project", "Spur", "--route", "fix the build"]);
        let args = Args(&l);
        assert_eq!((args.option("--project"), args.after("--route")), (Some("Spur"), &["fix the build".to_string()][..]));
    }

    #[test]
    fn the_first_mode_in_the_tables_order_runs_and_a_short_line_is_a_usage_error() {
        let pick = |words: &[&str]| {
            let l = line(words);
            MODES.iter().find(|m| Args(&l).has(m.flag)).map(|m| m.flag)
        };
        assert_eq!(pick(&["--snapshot", "--table", "--no-github"]), None);
        assert_eq!(pick(&["--snapshot", "--accounts"]), Some("--accounts"));
        // `--pr` isn't taken for `--pr-card`, nor a session's mode for an option after it.
        assert_eq!(pick(&["--snapshot", "--pr-card", "o/r", "7"]), Some("--pr-card"));
        assert_eq!(pick(&["--snapshot", "--start-plan", "/r", "claude", "terminal", "--worktree"]), Some("--start-plan"));
        // Each of these stops at its arguments, before anything is read or done.
        for words in [&["--snapshot", "--send", "claude:claude:abc"][..], &["--snapshot", "--stop"], &["--snapshot", "--answer", "k", "id"], &["--snapshot", "--answer", "k", "id", "maybe"], &["--snapshot", "--rename", "k"], &["--snapshot", "--pr", "o/r", "seven"], &["--snapshot", "--start-plan", "/r", "claude", "the moon"], &["--snapshot", "--side", "k"]] {
            assert_eq!(cli(&line(words)), 2, "{words:?}");
        }
    }

    #[test]
    fn every_mode_has_its_line_in_the_notes() {
        // CLAUDE.md lists the modes for whoever checks behaviour without a window.
        let notes = include_str!("../../CLAUDE.md");
        for m in MODES {
            assert!(notes.contains(&format!("--snapshot {}", m.flag)), "CLAUDE.md doesn't say what --snapshot {} does", m.flag);
        }
    }
}

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
    Mode { flag: "--fixture", takes: "[name | bar | accounts | found | setup | config | start-options | skills [folder…] | editor-icons | shell-commands | transcript <session key> [--before <cursor>] | pending <session key>]", run: fixture },
    Mode { flag: "--pending", takes: "<session key>", run: pending },
    Mode { flag: "--answer", takes: "<session key> <id> <allow|deny|always> [answers to its questions, as JSON]", run: answer },
    // Before `--stop`, which is also its option (`OWN_OPTIONS`): after it, `--done <key> --stop`
    // was read as `--stop` with no key.
    Mode { flag: "--done", takes: "<session key> [--stop] [--end-helpers]", run: done },
    Mode { flag: "--stop", takes: "<session key>", run: stop },
    Mode { flag: "--steer", takes: "<session key> <queued message id>", run: steer },
    Mode { flag: "--unqueue", takes: "<session key> <queued message id>", run: unqueue },
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
    Mode { flag: "--start-plan", takes: "<folder> <account> <terminal|background|claude-app|codex-app|codealong> [prompt] [--model <model>] [--effort <level>] [--worktree]", run: start_plan },
    Mode { flag: "--route", takes: "<text> [--project <name>] [--decider <name>]", run: route_text },
    Mode { flag: "--routes", takes: "[days] [--decider <name>] [--most <n>]", run: routes },
    Mode { flag: "--config", takes: "", run: |_, _| Ok(pretty(&config::view())) },
    Mode { flag: "--setup", takes: "", run: |_, _| Ok(pretty(&crate::setup::view().map_err(|e| Stop::Failed(format!("{e:#}")))?)) },
    Mode { flag: "--found", takes: "", run: |_, _| Ok(pretty(&detect::unlisted().map_err(|e| Stop::Failed(format!("{e:#}")))?)) },
    Mode { flag: "--accounts", takes: "", run: accounts },
    Mode { flag: "--training-off", takes: "<account>", run: training_off },
    Mode { flag: "--training", takes: "", run: |_, _| Ok(pretty(&crate::training::read_all(&config::current().map_err(failed)?.accounts))) },
    Mode { flag: "--use-reset", takes: "<account> [credit id]", run: use_reset },
    Mode { flag: "--resets", takes: "", run: |_, _| Ok(pretty(&crate::resets::read_all(&config::current().map_err(failed)?.accounts))) },
    Mode { flag: "--mcp", takes: "", run: |_, _| Ok(pretty(&crate::mcp::read_all(&config::current().map_err(failed)?.accounts))) },
    Mode { flag: "--pr-side", takes: "<owner/name> <number> <account> <question> [a follow-up…] [--model <model>] [--effort <level>]", run: pr_side },
    Mode { flag: "--pr-card", takes: "<owner/name> <number>", run: pr_card },
    Mode { flag: "--pr-draft", takes: "<owner/name> <number>", run: |_, rest| { let (repo, number) = repo_and_number(rest)?; Ok(pretty(&crate::github::your_review(repo, number).map_err(failed)?)) } },
    Mode { flag: "--pr-since", takes: "<owner/name> <commit> <commit>", run: |_, rest| match rest { [repo, from, to] => Ok(pretty(&github::since(repo, from, to).map_err(failed)?)), _ => Err(Stop::Usage) } },
    Mode { flag: "--pr", takes: "<owner/name> <number>", run: pr },
    Mode { flag: "--merge-box", takes: "<owner/name> <number>", run: merge_box },
    Mode { flag: "--review-plan", takes: "<owner/name> <number> [--usage]", run: review_plan },
    Mode { flag: "--open", takes: "<lane id> <target id>", run: open },
    Mode { flag: "--link", takes: "<url>", run: link },
    Mode { flag: "--checkout", takes: "", run: |_, _| Ok(crate::CHECKOUT.unwrap_or_default().to_string()) },
    Mode { flag: "--improve", takes: "", run: |_, _| Ok(pretty(&board_now().read(|b| b.improve.clone()))) },
    Mode { flag: "--tab-picture", takes: "<session key> <file>", run: tab_picture },
    Mode { flag: "--tokens", takes: "[days] [--account <name>] [--session <key>] [--into <ledger file>]", run: tokens },
    Mode { flag: "--jev-usage", takes: "[days]", run: jev_usage },
    Mode { flag: "--analytics", takes: "[days] [--account <name>] [--into <ledger file>]", run: analytics },
    Mode { flag: "--stance", takes: "[how many] --into <ledger file> [--decider <name>]", run: stance },
    Mode { flag: "--stale-terminals", takes: "[how long unused: 90m, 24h, 3d, 1w]", run: stale_terminals },
    Mode { flag: "--hearing", takes: "", run: hearing },
    Mode { flag: "--transcribe", takes: "<sound file> [locale]", run: transcribe },
    Mode { flag: "--hibernate", takes: "[how long idle: 30m, 2h, 1d]", run: hibernating },
    Mode { flag: "--schedules", takes: "", run: schedules },
    Mode { flag: "--harness-schedules", takes: "", run: harness_schedules },
    Mode { flag: "--schedule-plan", takes: "<schedule>", run: schedule_plan },
    Mode { flag: "--listen-plan", takes: "[what you'd type] [--in <worktree folder>]", run: listen_plan },
    Mode { flag: "--schedule-check", takes: "<schedule>", run: schedule_check },
    Mode { flag: "--schedule-run", takes: "<schedule>", run: schedule_run },
    Mode { flag: "--schedule-times", takes: "<time, as a schedule writes it>", run: |_, rest| Ok(pretty(&crate::schedules::times(&rest.join(" "), util::now_ms(), &crate::schedules::rule::Local))) },
];

/// `codealong --snapshot --fixture [what]`: what a mode prints, for a made-up Mac in place of
/// this one (fixture.rs). `what` names the mode without its dashes: `bar`, `accounts`, `found`,
/// `setup`, `config`, `start-options`, `skills [folder…]`, `editor-icons`, `shell-commands`, and
/// for one of its sessions `transcript <session key> [--before <cursor>]` and `pending <session
/// key>`; without one, the board. It reads nothing of this Mac's but the time and its time
/// zone, so it's the same on every Mac: what `PREVIEW_FIXTURE=1 scripts/preview.sh` and
/// `scripts/checks.sh --fixture` render. `name` prints what the made-up Mac is called and nothing else, which is how a script
/// knows the engine it runs has one: a build from before it did took `--fixture` for nothing and
/// printed this Mac's board.
#[cfg(debug_assertions)]
fn fixture(args: &Args, rest: &[String]) -> Done {
    use crate::fixture;
    // It's a Mac, with a Mac's terminals in its settings, as the checks' browser is a Mac's.
    if !crate::os::is_macos() {
        return Err(failed(crate::os::only_on_macos("The made-up Mac the browser checks run against")));
    }
    let board = fixture::board(util::now_ms());
    let (what, more) = match rest {
        [] => ("board", rest),
        [what, more @ ..] => (what.as_str(), more),
    };
    // One of the board's sessions, by its key.
    let session = || match more.first() {
        Some(key) if board.session(key).is_some() => Ok(key.as_str()),
        Some(key) => Err(Stop::Failed(format!("The made-up board has no session {key}; `codealong --snapshot --fixture` lists them."))),
        None => Err(Stop::Usage),
    };
    Ok(match what {
        "name" => fixture::NAME.to_string(),
        "board" => pretty(&board),
        "bar" => pretty(&barfeed::feed(crate::shown::Shown::here(&board))),
        "accounts" => pretty(&fixture::accounts()),
        "found" => pretty(&fixture::found()),
        "setup" => pretty(&fixture::setup()),
        "config" => pretty(&fixture::config_view()),
        "start-options" => pretty(&fixture::start_options(&board)),
        "skills" => pretty(&fixture::skills(&more.iter().filter(|a| !a.starts_with("--")).cloned().collect::<Vec<_>>())),
        "editor-icons" => serde_json::to_string(&fixture::editor_icons()).unwrap_or_default(),
        "shell-commands" => serde_json::to_string(&fixture::shell_commands()).unwrap_or_default(),
        "transcript" => pretty(&fixture::page(&board, session()?, args.option("--before").and_then(|b| b.parse().ok()))),
        "pending" => pretty(&fixture::pending(&board, session()?)),
        _ => return Err(Stop::Usage),
    })
}

/// The made-up Mac is in debug builds only, and a build without it says so rather than print
/// this Mac's data to a check that asked for the made-up one.
#[cfg(not(debug_assertions))]
fn fixture(_: &Args, _: &[String]) -> Done {
    Err(Stop::Failed("This build has no made-up board: `--snapshot --fixture` is a debug build's, which scripts/engine.sh builds.".into()))
}

/// `codealong --snapshot --tab-picture <session key> <file>`: the terminal tab a session runs in,
/// as Improve Codealong finds it for a picture when a window shows the session (improve.rs), and
/// the picture, written to the file. It's taken as the process that runs this, which macOS lets
/// picture another app's window only with the Screen Recording permission of the app it runs in.
fn tab_picture(_: &Args, rest: &[String]) -> Done {
    let [key, file] = rest else { return Err(Stop::Usage) };
    let tab = crate::improve::tab_of(&engine::snapshot(false, false), key, crate::improve::TAB_FIND_WAIT).map_err(failed)?;
    let Some(tab) = tab else { return Err(Stop::Failed("It isn't live in a terminal tab whose window Codealong can find.".into())) };
    let Some(png) = crate::improve::tab_png(&tab) else {
        return Err(Stop::Failed(format!("Found its tab, {:?}, but no picture of it: Codealong may not record the screen, or another of {}'s windows has that title.", tab.window.title, tab.window.terminal)));
    };
    std::fs::write(file, png).map_err(|e| failed(e.into()))?;
    Ok(pretty(&tab))
}

/// `codealong --snapshot --stale-terminals [wait]`: the terminal tabs File › Close stale
/// terminals… would list, unused for the wait (a day when left out), closing nothing.
fn stale_terminals(_: &Args, rest: &[String]) -> Done {
    let after_ms = match rest.first() {
        Some(wait) => crate::idle::parse_after(wait).ok_or(Stop::Usage)?,
        None => util::DAY_MS,
    };
    Ok(pretty(&crate::stale::plan_now(&engine::snapshot(false, false), crate::stale::Uses::load(), after_ms)))
}

/// `codealong --snapshot --hearing`: what this Mac can hear with, and the apps that record or play
/// now (ear.rs), asking the person for nothing.
fn hearing(_: &Args, _: &[String]) -> Done {
    let cfg = config::current().map_err(failed)?;
    let status = crate::ear::status(&cfg.voice.locale);
    let apps = crate::ear::apps().map_err(|e| e.message);
    Ok(pretty(&serde_json::json!({ "status": status, "apps": apps.as_ref().ok(), "appsError": apps.as_ref().err() })))
}

/// `codealong --snapshot --transcribe <sound file> [locale]`: a recording's words as a listening
/// would write them to its transcript (listen.rs `line`), through the helper as a meeting goes
/// through it, the file in place of the microphone.
fn transcribe(_: &Args, rest: &[String]) -> Done {
    use crate::ear::{Ear, Event, From, Source, Spec};
    let path = rest.first().ok_or(Stop::Usage)?;
    let path = dunce::canonicalize(path).map_err(|e| Stop::Failed(format!("{path}: {e}")))?;
    let spec = Spec { sources: vec![Source { name: crate::listen::THEM.into(), from: From::File { path: path.to_string_lossy().into_owned(), paced: false } }], locale: rest.get(1).cloned().unwrap_or_default(), hints: Vec::new() };
    let (tx, rx) = std::sync::mpsc::channel();
    let ear = Ear::start(&spec, move |event| {
        let _ = tx.send(event);
    })
    .map_err(|e| Stop::Failed(e.message))?;
    let mut out = String::new();
    let mut failed = None;
    // A minute of nothing at all is a helper that's stuck; a long recording goes on saying things.
    while let Ok(event) = rx.recv_timeout(std::time::Duration::from_secs(60)) {
        match event {
            Event::Final { source, text, .. } => out.push_str(&crate::listen::line("00:00:00", &source, &text)),
            Event::Failed(refused) => failed = Some(refused.message),
            Event::Lost { message, .. } => failed = Some(message),
            Event::Closed { last_words } => failed = Some(crate::ear::stopped(&last_words)),
            Event::Ended { .. } => break,
            _ => continue,
        }
        if failed.is_some() {
            break;
        }
    }
    drop(ear);
    match failed {
        Some(why) => Err(Stop::Failed(why)),
        None => Ok(out.trim_end().to_string()),
    }
}

/// `codealong --snapshot --hibernate [wait]`: the sessions that would hibernate now and why the
/// rest stay, after config.toml's `[sessions] hibernate_after` (two hours while that's off) or
/// the wait given, closing nothing.
fn hibernating(_: &Args, rest: &[String]) -> Done {
    let cfg = config::current().map_err(failed)?;
    let after_ms = match rest.first() {
        Some(wait) => crate::idle::parse_after(wait).ok_or(Stop::Usage)?,
        None => Some(crate::hibernate::after_ms(&cfg)).filter(|ms| *ms > 0).unwrap_or(2 * 3_600_000),
    };
    Ok(pretty(&crate::hibernate::plan_now(&engine::snapshot(false, false), &cfg, after_ms)))
}

/// `codealong --snapshot --schedules`: the schedules as the Schedules view lists them, with when
/// each runs next and its latest runs.
fn schedules(_: &Args, _: &[String]) -> Done {
    let board = engine::snapshot(false, false);
    let cfg = config::current().map_err(failed)?;
    let (file, runs) = (crate::schedules::load().map_err(failed)?, crate::schedules::load_runs().map_err(failed)?);
    Ok(pretty(&crate::schedules::items(&file, &runs, &board, &cfg, util::now_ms(), &crate::schedules::rule::Local)))
}

/// `codealong --snapshot --harness-schedules`: the schedules the harnesses keep themselves, as the
/// Schedules view lists them under its own (schedules/harnesses.rs).
fn harness_schedules(_: &Args, _: &[String]) -> Done {
    let board = engine::snapshot(false, false);
    let cfg = config::current().map_err(failed)?;
    Ok(pretty(&crate::schedules::harnesses::list(&cfg, &board, util::now_ms())))
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

/// `codealong --snapshot --listen-plan [what you'd type] [--in <worktree folder>]`: what Listen
/// now would start for a meeting that's in no calendar (meetings.rs `plan`), without starting a
/// session or hearing anything: the session, where and on what, and where its files would go.
fn listen_plan(args: &Args, rest: &[String]) -> Done {
    let words = rest.first().filter(|w| !w.starts_with("--")).cloned().unwrap_or_default();
    let lane = args.option("--in").map(|folder| util::expand(folder).to_string_lossy().into_owned()).unwrap_or_default();
    let board = engine::snapshot(false, false);
    let cfg = config::current().map_err(failed)?;
    Ok(pretty(&crate::meetings::plan(&cfg, &board, &words, &lane, util::now_ms()).map_err(failed)?))
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

/// `codealong --snapshot --jev-usage [days]`: what Settings › Decisions says Codealong's requests
/// to decision models used each of the last `days` days (30 unless given), by feature and by
/// model, read from the app's ledger.
fn jev_usage(_: &Args, rest: &[String]) -> Done {
    let days: u32 = match rest.first() {
        Some(d) => d.parse().map_err(|_| Stop::Usage)?,
        None => 30,
    };
    Ok(pretty(&crate::ledger::jev_report(days).map_err(failed)?))
}

/// `codealong --snapshot --analytics [days]`: what the Analytics window's Your part and Sessions show
/// for the last `days` days (30 unless given), whether or not the experiment is on: this Mac's
/// records read afresh into a ledger of the check's own (a temporary file, or `--into` one to keep
/// and read on from), never the app's, with where each session came from as a board read now says.
/// Only records written in those days are read, and nothing here was ever marked done.
fn analytics(args: &Args, rest: &[String]) -> Done {
    let days: u32 = match rest.first().filter(|d| !d.starts_with("--")) {
        Some(d) => d.parse().map_err(|_| Stop::Usage)?,
        None => 30,
    };
    let cfg = config::current().map_err(failed)?;
    let keep = args.option("--into").map(std::path::PathBuf::from);
    let file = keep.clone().unwrap_or_else(|| std::env::temp_dir().join(format!("codealong-analytics-check-{}.sqlite", std::process::id())));
    let db = crate::ledger::open(&file).map_err(failed)?;
    let now = util::now_ms();
    let since = now - (days as i64 + 1) * util::DAY_MS;
    let mut caught = crate::ledger::catch_up_since(&db, &cfg.accounts, since);
    let board = board_now();
    let (schedules, runs) = (crate::schedules::load().unwrap_or_default(), crate::schedules::load_runs().unwrap_or_default());
    board.read(|b| crate::began::note(crate::began::noted_on(b, &crate::schedules::own_sessions(&schedules, &runs, b))));
    crate::turns::pass_since(&db, &cfg.accounts, &mut caught, since);
    let report = board.read(|b| crate::analytics::report_in(&db, &cfg, Some(b), days, args.option("--account"), now)).map_err(failed)?;
    let tokens = crate::ledger::report_in(&db, &config::Config { experimental: config::ExperimentalCfg { analytics: true, ..cfg.experimental.clone() }, ..cfg.clone() }, days, args.option("--account"), now).map_err(failed)?;
    drop(db);
    if keep.is_none() {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", file.display()));
        }
    }
    Ok(pretty(&serde_json::json!({ "analytics": report, "origins": tokens.origins, "hands": tokens.hands, "subagents": tokens.subagents, "total": tokens.total, "problem": caught.problem() })))
}

/// The settings to ask with, and the model a `--decider <name>` names (one of the
/// `[[decisions.models]]`, or `<provider>:<model>`): config.toml's, or with that model as the
/// default (`decisions::trying`), so what goes by the default goes to it and what a project or
/// boundary group chose doesn't.
fn deciding(args: &Args) -> Result<(config::Config, Option<crate::decisions::Decider>), Stop> {
    let cfg = config::current().map_err(failed)?;
    match args.option("--decider") {
        Some(spec) => {
            let trial = crate::decisions::named(&cfg, spec).map_err(failed)?;
            Ok((crate::decisions::trying(&cfg, &trial), Some(trial)))
        }
        None => Ok((cfg, None)),
    }
}

/// `codealong --snapshot --stance [how many] --into <ledger file> [--decider <name>]`: reads the
/// newest prompts that wait on a read in a check's ledger (one `--snapshot --analytics --into`
/// made), 10 unless given, as the app would with `[decisions] prompts` on, and prints each with
/// what the model made of it: for trying the question's wording, or another model on the same
/// prompts. Real: it asks the decision model, and keeps each read in that ledger.
fn stance(args: &Args, rest: &[String]) -> Done {
    let n: usize = match rest.first().filter(|d| !d.starts_with("--")) {
        Some(d) => d.parse().map_err(|_| Stop::Usage)?,
        None => 10,
    };
    let file = args.option("--into").ok_or(Stop::Usage)?;
    let (_, trial) = deciding(args)?;
    let board = board_now().board();
    let db = crate::ledger::open(std::path::Path::new(file)).map_err(failed)?;
    let mut out = Vec::new();
    for u in crate::stance::unread(&db, n).map_err(failed)? {
        // The settings as they are now, for each: a project switched off meanwhile is. A trial
        // asks the model it names or nothing: a prompt whose project or boundary group chose
        // another goes to none here.
        let now = match &trial {
            Some(t) => crate::decisions::trying(&config::current().map_err(failed)?, t),
            None => config::current().map_err(failed)?,
        };
        let project_of = |key: &str| crate::decisions::session_projects(&now, &board.lanes, key);
        let decider = crate::stance::decider(&now, &u, &project_of).filter(|d| trial.as_ref().is_none_or(|t| d.is(&t.models[0])));
        let Some(decider) = decider else {
            out.push(serde_json::json!({ "prompt": u, "stance": null, "kept": "Its project or boundary group keeps its words from this model." }));
            continue;
        };
        let answer = crate::decisions::ask(crate::jev::Feature::Prompts, &decider, &crate::stance::request(decider.model(), &u)).map_err(failed)?;
        let read = crate::stance::read(&answer);
        if let (Some((stance, sure)), Ok(id)) = (&read, u.id.parse::<i64>()) {
            crate::stance::keep_read(&db, id, stance, *sure, answer["model"].as_str().unwrap_or(decider.model())).map_err(failed)?;
        }
        out.push(serde_json::json!({ "prompt": u, "stance": read.map(|(s, _)| s), "probabilities": crate::jev::choice(&answer, "stance"), "model": answer["model"] }));
    }
    Ok(pretty(&out))
}

/// `codealong --snapshot --routes [days] [--decider <name>] [--most <n>]`: how often what you did
/// with what you typed in the quick panel was Jev's first suggestion, over the last `days` (7 by
/// default), with the times it wasn't (routelog.rs). With `--decider`, the latest `most` of them
/// (20 unless given) are asked again of that model on today's board, each set beside what you did
/// and where it was among Jev's suggestions then (`routelog::replay`). Real then: each is a
/// request to the model.
fn routes(args: &Args, rest: &[String]) -> Done {
    let days: u32 = match rest.first().filter(|d| !d.starts_with("--")) {
        Some(d) => d.parse().map_err(|_| Stop::Usage)?,
        None => 7,
    };
    let since = crate::routelog::since_days(days, crate::util::now_ms());
    let Some(spec) = args.option("--decider") else {
        return Ok(pretty(&crate::routelog::report(since).map_err(failed)?));
    };
    let most: usize = match args.option("--most") {
        Some(n) => n.parse().map_err(|_| Stop::Usage)?,
        None => 20,
    };
    let (cfg, trial) = deciding(args)?;
    let trial = trial.ok_or(Stop::Usage)?;
    let board = route_board(&cfg);
    Ok(pretty(&crate::routelog::replay(spec, since, most, |k| route_trial(&board, &trial, &k.project, &k.text)).map_err(failed)?))
}

/// What a trial model makes of what was typed in a project's panel, asked only when that
/// project's words may go to it under config.toml as it is now (`decisions::trying`, read again
/// for each, so a project switched off meanwhile is): All projects' go by the default, which it
/// stands in for.
fn route_trial(board: &model::Board, trial: &crate::decisions::Decider, project: &str, text: &str) -> anyhow::Result<route::Route> {
    let cfg = crate::decisions::trying(&config::current()?, trial);
    let in_project = Some(project).filter(|p| *p != route::ALL_PROJECTS);
    if !crate::decisions::for_project(&cfg, in_project).is_some_and(|d| d.is(&trial.models[0])) {
        anyhow::bail!("{project} chose another decision model, so what's typed there doesn't go to this one.");
    }
    route::route_on(board, &cfg, trial, project, text)
}

/// The board a route is asked on here: read now, with the accounts' MCP servers, which the app
/// reads in the background, for the `needs` question, and what was said in the sessions, which it
/// indexes in the background, for what they share with the text.
fn route_board(cfg: &config::Config) -> model::Board {
    let mut board = engine::snapshot(false, false);
    board.mcp = crate::mcp::read_all(&cfg.accounts);
    crate::search::build_now(crate::search::Scope { accounts: cfg.accounts.clone(), days: cfg.search.days });
    board
}

/// `codealong --snapshot --training-off <account>`: turns training off for the account at its
/// provider, as the usage bar's flag does, and prints where it stands after. Real.
fn training_off(_: &Args, rest: &[String]) -> Done {
    let [name] = rest else { return Err(Stop::Usage) };
    let cfg = config::current().map_err(failed)?;
    let a = cfg.accounts.iter().find(|a| &a.name == name).ok_or_else(|| Stop::Failed(format!("There's no account named {name}.")))?;
    Ok(pretty(&crate::training::stop(a).map_err(failed)?))
}

/// `codealong --snapshot --use-reset <account> [credit id]`: uses one of the account's limit
/// resets, the one named or else the first to expire, as the usage bar's card does, and prints how
/// it went. Real: it spends the reset, so only that line goes: anything else on it (a `--dry-run`,
/// say, which nothing here reads) is a usage error rather than a reset spent.
fn use_reset(args: &Args, _: &[String]) -> Done {
    let (name, credit) = reset_line(args).ok_or(Stop::Usage)?;
    let cfg = config::current().map_err(failed)?;
    let a = cfg.accounts.iter().find(|a| &a.name == name).ok_or_else(|| Stop::Failed(format!("There's no account named {name}.")))?;
    let credit = match credit {
        Some(c) => c.to_string(),
        None => crate::resets::first_to_expire(a).map_err(failed)?,
    };
    // This process ends with the use: what's left is printed, not kept.
    Ok(pretty(&crate::resets::use_one(a, &credit, |_| {}).map_err(failed)?))
}

/// The account and credit of a `--use-reset` line: the program, `--snapshot`, the flag, the
/// account and maybe a credit, and nothing else on it.
fn reset_line<'a>(args: &Args<'a>) -> Option<(&'a String, Option<&'a str>)> {
    let at = args.0.iter().position(|a| a == "--use-reset")?;
    // What comes before is the program, wherever it is, and `--snapshot`, once.
    let fits = match &args.0[..at] {
        [snapshot] => snapshot == "--snapshot",
        [program, snapshot] => !program.starts_with('-') && snapshot == "--snapshot",
        _ => false,
    };
    if !fits {
        return None;
    }
    match &args.0[at + 1..] {
        [name] if !name.is_empty() && !name.starts_with('-') => Some((name, None)),
        [name, credit] if !name.is_empty() && !name.starts_with('-') && !credit.trim().is_empty() && !credit.starts_with('-') => Some((name, Some(credit.as_str()))),
        _ => None,
    }
}

/// `codealong --snapshot --tokens [days]`: what the Analytics window shows for the last `days` days
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
/// `codealong --snapshot --route <text> [--project <name>] [--decider <name>]`: where the quick
/// panel would suggest sending it (asks the project's decision model; needs its key, and
/// `[decisions] quick_panel` on), or with `--decider` where that model would.
/// `codealong --snapshot --jev-usage [days]`: what Codealong's requests to decision models used,
/// by day, feature and model, as Settings › Decisions gets it.
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
/// A mode's option that is another mode's flag: with it, the line still names one mode.
const OWN_OPTIONS: &[(&str, &str)] = &[("--done", "--stop")];

/// The mode a line asks for: none (the board), or the one it names. Two on a line is a usage
/// error: only the first in `MODES`' order would run, whatever the line was written to do, so
/// `--remove-worktree <folder> --dry-run --stop <key>` stopped the session.
fn mode_of(args: &Args) -> Result<Option<&'static Mode>, String> {
    let named: Vec<&'static Mode> = MODES.iter().filter(|m| args.has(m.flag)).collect();
    // And once: of a flag written twice only the first is read, so `--done B --stop --stop A`
    // ended B and passed A over.
    if let Some(twice) = named.iter().find(|m| args.0.iter().filter(|word| *word == m.flag).count() > 1) {
        return Err(format!("{} is on the line twice.", twice.flag));
    }
    match named.as_slice() {
        [] => Ok(None),
        [mode] => Ok(Some(mode)),
        [mode, option] if OWN_OPTIONS.contains(&(mode.flag, option.flag)) && as_its_option(args, mode.flag, option.flag) => Ok(Some(mode)),
        [first, second, ..] => Err(format!("{} and {} are two modes, and a line runs one.", first.flag, second.flag)),
    }
}

/// Whether `option` is on the line as `mode`'s option: after the mode's flag, with nothing of
/// its own after it. `--stop <key>` anywhere is the mode, so `--stop A --done B` is two modes,
/// where taking the two flags for one mode ended B with its work.
fn as_its_option(args: &Args, mode: &str, option: &str) -> bool {
    let at = |flag: &str| args.0.iter().position(|a| a == flag);
    match (at(mode), at(option)) {
        (Some(mode), Some(option)) => option > mode && args.0.get(option + 1).is_none_or(|next| next.starts_with("--")),
        _ => false,
    }
}

pub fn cli(args: &[String]) -> i32 {
    let args = Args(args);
    let mode = match mode_of(&args) {
        Ok(Some(mode)) => mode,
        Ok(None) => return print_board(&args),
        Err(why) => {
            eprintln!("{why}");
            return 2;
        }
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
        println!("{}", pretty(&barfeed::feed(crate::shown::Shown::here(&board))));
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

fn unqueue(_: &Args, rest: &[String]) -> Done {
    let [key, id, ..] = rest else { return Err(Stop::Usage) };
    Ok(pretty(&act(api::SESSION_UNQUEUE, key, params::Steer { id: id.clone(), ..Default::default() })?))
}

fn done(args: &Args, rest: &[String]) -> Done {
    let [key, ..] = rest else { return Err(Stop::Usage) };
    // As a window told to end them names them (menus.ts `unfinishedHelpers`, `helpersChosen`):
    // each helper under it whose work isn't over, at any depth, and every helper under one of
    // those. Each once, though origins name each other.
    let end_helpers = if args.has("--end-helpers") {
        let board = engine::snapshot(false, false);
        let (mut seen, mut ending) = (std::collections::HashSet::from([key.clone()]), std::collections::HashSet::new());
        let (mut walk, mut named) = (vec![key.clone()], Vec::new());
        while let Some(parent) = walk.pop() {
            for h in crate::board::helpers_of(&board.lanes, &parent) {
                if !seen.insert(h.key.clone()) {
                    continue;
                }
                walk.push(h.key.clone());
                if h.unfinished() || ending.contains(&parent) {
                    ending.insert(h.key.clone());
                    named.push(h.key.clone());
                }
            }
        }
        named
    } else {
        Vec::new()
    };
    let v = act(api::SESSION_DONE, key, params::Done { stop: args.has("--stop"), end_helpers, ..Default::default() })?;
    Ok(v.get("message").and_then(Value::as_str).unwrap_or_default().to_string())
}

fn remove_worktree(args: &Args, rest: &[String]) -> Done {
    let [folder, ..] = rest else { return Err(Stop::Usage) };
    // The whole line is this mode's, as a reset's is (`reset_line`): the program, `--snapshot`,
    // the flag with its folder, and `--dry-run` before or after. One mistyped (`--dryrun`),
    // anywhere on the line, would otherwise be passed over and the worktree removed.
    let at = args.0.iter().position(|a| a == "--remove-worktree").unwrap_or_default();
    let stray = args.0.iter().enumerate().any(|(i, word)| i != at + 1 && !(i == 0 && !word.starts_with('-')) && !["--snapshot", "--remove-worktree", "--dry-run"].contains(&word.as_str()));
    let once = args.0.iter().filter(|word| *word == "--remove-worktree").count() == 1;
    if stray || !once || folder.starts_with('-') {
        return Err(Stop::Usage);
    }
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
        if let Err(e) = side::ask_pr(&board, &cfg, &key, account, on, q, None) {
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
    start::note_isolation(&mut options, &cfg.accounts, &engine::snapshot(false, false));
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
    let (cfg, trial) = deciding(args)?;
    let board = route_board(&cfg);
    // Another model by hand is asked what you typed whatever the panel's switch says, the
    // sessions' words where it may read them; the panel's own way is the app's request, as
    // another device asks it.
    if let Some(trial) = trial {
        return Ok(pretty(&route_trial(&board, &trial, project, text).map_err(failed)?));
    }
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

/// What an automatic review of the PR would start or go on in (autoreview.rs `plan`), without
/// doing it.
fn review_plan(args: &Args, rest: &[String]) -> Done {
    let (repo, number) = repo_and_number(rest)?;
    // With `--usage`, a kept reviewer on an account at its limit is passed over as the server
    // would (`kept_session`).
    let board = engine::snapshot(true, args.has("--usage"));
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
    let accounts = config::current().map_err(failed)?.accounts;
    Ok(match engine::link_opens(&engine::snapshot(false, false), &accounts, url) {
        engine::LinkOpens::Preview(lane) => {
            let name = lane.preview.as_ref().and_then(|p| p.chrome_profile.clone()).unwrap_or_default();
            match chrome::resolve(&name) {
                Ok(p) => format!("In Chrome's {} profile ({}), as {}'s preview opens.", p.name, p.dir, lane.path),
                Err(e) => format!("In the default browser, though it's {}'s app: {}", lane.path, util::words(e)),
            }
        }
        engine::LinkOpens::Account { account, profile } => match engine::account_profile(&account, &profile) {
            Ok(p) => format!("In Chrome's {} profile ({}), as {account}'s sessions made it.", p.name, p.dir),
            Err(e) => format!("In the default browser, though {account}'s sessions made it: {}", util::words(e)),
        },
        engine::LinkOpens::Default => "In the default browser: it isn't a link into a worktree's app that opens in a Chrome profile, nor a page made on an account that names one.".into(),
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
            mode_of(&Args(&l)).ok().flatten().map(|m| m.flag)
        };
        assert_eq!(pick(&["--snapshot", "--table", "--no-github"]), None);
        assert_eq!(pick(&["--snapshot", "--accounts"]), Some("--accounts"));
        // `--pr` isn't taken for `--pr-card`, nor a session's mode for an option after it.
        assert_eq!(pick(&["--snapshot", "--pr-card", "o/r", "7"]), Some("--pr-card"));
        assert_eq!(pick(&["--snapshot", "--start-plan", "/r", "claude", "terminal", "--worktree"]), Some("--start-plan"));
        // Nor `--done` for its own `--stop`, which is a mode too.
        assert_eq!(pick(&["--snapshot", "--done", "claude:claude:abc", "--stop"]), Some("--done"));
        assert_eq!(pick(&["--snapshot", "--stop", "claude:claude:abc"]), Some("--stop"));
        // A dry run mistyped isn't a removal.
        for words in [&["--snapshot", "--remove-worktree", "/r/atlas-spare", "--dryrun"][..], &["--snapshot", "--dryrun", "--remove-worktree", "/r/atlas-spare"], &["--snapshot", "--remove-worktree", "--dry-run"], &["--snapshot", "--remove-worktree", "/r/atlas-spare", "/r/kestrel-spare"], &["--snapshot", "--remove-worktree", "/r/atlas-spare", "--remove-worktree"]] {
            assert_eq!(cli(&line(words)), 2, "{words:?}");
        }
        // Two modes on a line run neither: the first in the table's order would have run, here
        // `--stop` on a session, whatever the line set out to do.
        assert_eq!(pick(&["--snapshot", "--done", "claude:claude:abc", "--stop", "--end-helpers"]), Some("--done"));
        assert!(mode_of(&Args(&line(&["--snapshot", "--done", "claude:claude:xyz", "--stop", "--stop", "claude:claude:abc"]))).is_err(), "a flag twice");
        // `--stop <key>` is the mode wherever it is: with `--done` of another session it's two.
        for words in [&["--snapshot", "--remove-worktree", "/r/atlas-spare", "--dry-run", "--stop", "claude:claude:abc"][..], &["--snapshot", "--stop", "claude:claude:abc", "--done", "claude:claude:xyz"], &["--snapshot", "--done", "claude:claude:xyz", "--stop", "claude:claude:abc"], &["--snapshot", "--stop", "claude:claude:abc", "--done", "claude:claude:abc", "--rename"], &["--snapshot", "--accounts", "--found"]] {
            assert!(mode_of(&Args(&line(words))).is_err(), "{words:?}");
            assert_eq!(cli(&line(words)), 2, "{words:?}");
        }
        // Each of these stops at its arguments, before anything is read or done.
        // A reset is spent only for the line as written, with nothing else on it.
        for words in [&["--snapshot", "--use-reset"][..], &["--snapshot", "--dry-run", "--use-reset", "atlas"], &["--snapshot", "--use-reset", "atlas", "--dry-run"], &["--snapshot", "--help", "--use-reset", "atlas"], &["--snapshot", "--use-reset", "atlas", "c1", "c2"]] {
            assert_eq!(cli(&line(words)), 2, "{words:?}");
        }
        // As a real line has it, the program first (`line` puts it there).
        let reset = |l: Vec<String>| reset_line(&Args(&l)).map(|(name, credit)| (name.clone(), credit.map(str::to_string)));
        assert_eq!(reset(line(&["--snapshot", "--use-reset", "atlas"])), Some(("atlas".into(), None)));
        assert_eq!(reset(line(&["--snapshot", "--use-reset", "atlas", "c1"])), Some(("atlas".into(), Some("c1".into()))));
        assert_eq!(reset(["--snapshot", "--use-reset", "atlas"].map(String::from).to_vec()), Some(("atlas".into(), None)), "or without it");
        assert_eq!(reset(line(&["--snapshot", "kestrel", "--use-reset", "atlas"])), None, "a word of its own before the flag");
        assert_eq!(reset(line(&["--snapshot", "--snapshot", "--use-reset", "atlas"])), None, "`--snapshot` twice");
        assert_eq!(reset(line(&["--snapshot", "--use-reset", "atlas", " "])), None, "a blank credit");
        for words in [&["--snapshot", "--send", "claude:claude:abc"][..], &["--snapshot", "--stop"], &["--snapshot", "--answer", "k", "id"], &["--snapshot", "--answer", "k", "id", "maybe"], &["--snapshot", "--rename", "k"], &["--snapshot", "--pr", "o/r", "seven"], &["--snapshot", "--start-plan", "/r", "claude", "the moon"], &["--snapshot", "--side", "k"]] {
            assert_eq!(cli(&line(words)), 2, "{words:?}");
        }
    }

    /// `scripts/preview.sh` takes an engine for one that has a made-up Mac only when `--snapshot
    /// --fixture name` answers with the name the script knows it by, so the answer here is the
    /// mode's own, asked as `cli` asks a mode. An engine without the word answers otherwise, as
    /// one from before the mode did (`--fixture` was nothing to it, and it printed this Mac's
    /// board), and the script refuses it.
    #[cfg(debug_assertions)]
    #[test]
    fn the_made_up_mac_answers_to_the_name_preview_sh_asks_for() {
        let _mac = crate::os::AsIf::new(crate::os::Os::MacOs);
        let l = line(&["--snapshot", "--fixture", "name"]);
        let args = Args(&l);
        let mode = mode_of(&args).ok().flatten().expect("--fixture is a mode");
        let name = (mode.run)(&args, args.after(mode.flag)).ok().expect("the made-up Mac has a name");
        assert_eq!(name, crate::fixture::NAME);
        let script = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/preview.sh")).expect("scripts/preview.sh");
        assert!(script.contains(&format!("of=\"{name}\"")), "scripts/preview.sh doesn't ask for {name:?}");
    }

    #[test]
    fn every_mode_has_its_line_in_the_notes() {
        // .claude/rules/snapshot.md lists the modes for whoever checks behaviour without a window,
        // each with its arguments as `MODES` has them, so the list can't fall behind the code.
        // CLAUDE.md's size limit and its reference to snapshot.md are held in src/conventions.test.ts
        // so editing CLAUDE.md doesn't recompile the engine.
        let notes = include_str!("../../.claude/rules/snapshot.md");
        for m in MODES {
            let line = format!("`--snapshot {}`", format!("{} {}", m.flag, m.takes).trim());
            assert!(notes.contains(&line), ".claude/rules/snapshot.md has no line starting {line}: say there what the mode does, with its arguments as MODES has them");
        }
    }
}

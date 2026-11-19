//! `codealong tokens` and `codealong analytics`: what sessions used and cost, and your part in
//! the work. Both are read from the ledger's file (ledger.rs), which the app fills as it runs.
//! Analytics is an experiment: `analytics_on` turns its commands down, and `tokens --by began`
//! and `--by turn` with them, until `[experimental] analytics` is on.

use anyhow::{bail, Result};
use serde_json::json;

use crate::config::Config;
use crate::util::{now_ms, tilde};

use super::*;

// ---- tokens ----

/// The token ledger's report (ledger.rs), read from its file: the app fills it as it runs, and
/// SQLite lets this read while the app writes.
pub(super) fn tokens(c: &mut Client, days: u32, account: Option<&str>, by: TokensBy, json: bool) -> Result<()> {
    let cfg = c.cfg()?.clone();
    let file = crate::ledger::path();
    if !file.exists() {
        bail!("Nothing is recorded yet: the app reads what sessions used into {} as it runs.", tilde(&file));
    }
    let db = crate::ledger::open(&file)?;
    let r = crate::ledger::report_in(&db, &cfg, days.max(1), account, now_ms())?;
    if json {
        return print_json(&r);
    }
    let dollars = |n: f64| if n >= 1000.0 { format!("${n:.0}") } else { format!("${n:.2}") };
    let row = |name: String, s: &crate::ledger::Spent| {
        let t = &s.tokens;
        let into = t.input + t.cache_read + t.cache_write;
        let cache = if into > 0 { format!("{:.0}%", t.cache_read as f64 * 100.0 / into as f64) } else { String::new() };
        let api = format!("{}{}", dollars(s.api), if s.unpriced > 0 { " +unpriced" } else { "" });
        vec![name, t.total().to_string(), cache, api, if s.paid > 0.0 { dollars(s.paid) } else { String::new() }]
    };
    let rows: Vec<Vec<String>> = match by {
        TokensBy::Account => r.accounts.iter().map(|s| row(s.key.clone(), s)).collect(),
        TokensBy::Model => r.models.iter().map(|s| row(s.key.clone(), s)).collect(),
        TokensBy::Folder => r.folders.iter().map(|s| row(tilde(std::path::Path::new(&s.key)), s)).collect(),
        TokensBy::Session => r.sessions.iter().map(|s| row(s.key.clone(), s)).collect(),
        TokensBy::Began | TokensBy::Turn => {
            analytics_on(&cfg)?;
            let (rows, named): (_, fn(&str) -> &str) = if by == TokensBy::Began { (&r.origins, began_words) } else { (&r.hands, hand_words) };
            rows.iter().map(|s| row(named(&s.key).to_string(), s)).collect()
        }
        TokensBy::Day => r
            .days
            .iter()
            .rev()
            .map(|d| vec![d.day.clone(), d.tokens.to_string(), String::new(), dollars(d.api.values().sum()), if d.paid > 0.0 { dollars(d.paid) } else { String::new() }])
            .collect(),
    };
    let what = match by {
        TokensBy::Account => "ACCOUNT",
        TokensBy::Model => "MODEL",
        TokensBy::Folder => "FOLDER",
        TokensBy::Session => "SESSION",
        TokensBy::Day => "DAY",
        TokensBy::Began => "SESSIONS BEGUN BY",
        TokensBy::Turn => "TURNS BEGUN BY",
    };
    print!("{}", table_text(&[what, "TOKENS", "CACHED", "AT API PRICES", "PAID"], rows));
    let lever = if r.paid > 0.0 { format!(", {:.1}× what was paid", r.total.api / r.paid) } else { String::new() };
    println!("\n{} days: {} at API prices, {} paid{lever}.", days.max(1), dollars(r.total.api), dollars(r.paid));
    if !r.unpriced.is_empty() {
        println!("No price for {}: add [prices.\"<model>\"] to config.toml.", r.unpriced.join(", "));
    }
    Ok(())
}

// ---- analytics ----

fn analytics_on(cfg: &Config) -> Result<()> {
    if !cfg.experimental.analytics {
        bail!("Analytics are an experiment, and they're turned off. Turn them on in Settings › Experimental, or with `analytics = true` under `[experimental]` in config.toml.");
    }
    Ok(())
}

/// How a session began (began.rs `Began::word`), as a row names it: led by who began it in the
/// words of a turn's, as the Analytics window has it (analytics.ts `BEGAN`).
fn began_words(word: &str) -> &str {
    match word {
        "you" => "You",
        "fork" => "You: forked from another",
        "helper" => "Delegated: helper",
        "agent" => "Delegated: new session",
        "review" => "Automation: PR review",
        "schedule" => "Automation: scheduled run",
        "headless" => "Headless run",
        "" => "Not known",
        other => other,
    }
}

/// Who began a turn (turns.rs `Source::word`), as a row names it and the Analytics window does
/// (analytics.ts `HANDS`): another session's agent is Delegated, Codealong by itself Automation.
fn hand_words(word: &str) -> &str {
    match word {
        "you" => "You",
        "agent" => "Delegated",
        "codealong" => "Automation",
        "itself" => "The session itself",
        "" => "Not known",
        other => other,
    }
}

/// What a prompt did (stance.rs `STANCES`), as a row names it.
fn stance_words(word: &str) -> &str {
    match word {
        "sets-out" => "Set out new work",
        "agrees" => "Agreed",
        "steers" => "Steered",
        "redirects" => "Redirected",
        "asks" => "Asked something",
        "command" => "Ran a command",
        "" => "Not read",
        other => other,
    }
}

/// The app's ledger, which the analytics are kept in with the tokens.
fn analytics_ledger(c: &mut Client) -> Result<rusqlite::Connection> {
    analytics_on(c.cfg()?)?;
    let file = crate::ledger::path();
    if !file.exists() {
        bail!("Nothing is recorded yet: the app reads what came in to sessions into {} as it runs.", tilde(&file));
    }
    crate::ledger::open(&file)
}

/// The analytics report (analytics.rs), read from the ledger's file as `tokens` reads it, with
/// the sessions left hanging from the board.
pub(super) fn analytics(c: &mut Client, days: u32, account: Option<&str>, json: bool) -> Result<()> {
    let db = analytics_ledger(c)?;
    let cfg = c.cfg()?.clone();
    let days = days.max(1);
    let now = now_ms();
    let board = c.board().ok().cloned();
    let r = crate::analytics::report_in(&db, &cfg, board.as_ref(), days, account, now)?;
    let spent = crate::ledger::report_in(&db, &cfg, days, account, now)?;
    if json {
        return print_json(&json!({ "analytics": r, "origins": spent.origins, "hands": spent.hands, "subagents": spent.subagents, "total": spent.total }));
    }
    let share = |n: u64, of: u64| if of > 0 { format!("{:.0}%", n as f64 * 100.0 / of as f64) } else { String::new() };
    let dollars = |n: f64| if n >= 1000.0 { format!("${n:.0}") } else { format!("${n:.2}") };
    let part = &r.part;
    let came: u64 = part.sources.iter().map(|s| s.n).sum();
    println!("The last {days} days.\n");
    print!("{}", table_text(&["WHAT CAME IN, FROM", "", "SHARE"], part.sources.iter().map(|s| vec![hand_words(&s.key).to_string(), s.n.to_string(), share(s.n, came)]).collect()));
    let cost = |key: &str| spent.hands.iter().find(|h| h.key == key).map(|h| h.api).unwrap_or(0.0);
    let rows = part.hands.iter().map(|s| vec![hand_words(&s.key).to_string(), s.n.to_string(), share(s.n, came), dollars(cost(&s.key))]).collect();
    println!();
    print!("{}", table_text(&["TURNS BEGUN BY", "", "SHARE", "AT API PRICES"], rows));
    if let Some(sub) = spent.subagents.as_ref().filter(|s| s.api > 0.0) {
        println!("Subagents made {} of it.", dollars(sub.api));
    }
    let yours: u64 = part.stances.iter().map(|s| s.n).sum();
    println!();
    print!("{}", table_text(&["YOUR PROMPTS", "", "SHARE"], part.stances.iter().map(|s| vec![stance_words(&s.key).to_string(), s.n.to_string(), share(s.n, yours)]).collect()));
    let m = &part.marks;
    println!("\n{} sent while the agent worked, {} after you stopped its turn. Answers to its questions: {} its first choice, {} another, {} your own words. From Codealong: {} calls allowed, {} denied, {} suggested replies sent as written.", m.mid, m.after_stop, m.picked_first, m.picked_offered, m.own_words, m.allowed, m.denied, m.suggested);
    if part.unread > 0 {
        println!("{} of your prompts wait on a read{}", part.unread, if part.reading { ": Jev reads a few each minute." } else { ": turn on Have Jev read what your prompts did in Settings › Experimental, or have an agent read them (`codealong analytics prompts`)." });
    }
    let o = &r.outcomes;
    let began: u64 = o.began.iter().map(|b| b.n).sum();
    println!();
    print!("{}", table_text(&["SESSIONS BEGUN BY", "", "SHARE"], o.began.iter().map(|b| vec![began_words(&b.key).to_string(), b.n.to_string(), share(b.n, began)]).collect()));
    println!("\nYou marked {} done or archived: {} finished, {} that looked finished, {} unfinished, {} nothing is known of; you took {} up again.", o.left, o.finished, o.looked_finished, o.unfinished, o.unknown, o.reopened);
    for l in o.list.iter().take(15) {
        println!("  {}  {}: {}{}", l.session, l.title, l.said, if l.reopened { " (taken up again)" } else { "" });
    }
    if !o.hanging.is_empty() {
        println!("\n{} still open with work left undone, untouched for {} days or more:", o.hanging.len(), crate::dismissed::HANGING_DAYS);
        for h in o.hanging.iter().take(15) {
            println!("  {}  {}: {}", h.session, h.title, h.said);
        }
    }
    Ok(())
}

pub(super) fn analytics_prompts(c: &mut Client, limit: usize, json: bool) -> Result<()> {
    let db = analytics_ledger(c)?;
    let mut unread = crate::stance::unread(&db, limit.max(1))?;
    // A boundary group's prompts, and the replies before them, stay with sessions in it
    // (boundary.rs), by the account in the key, whether or not the board still lists it.
    let cfg = c.cfg()?.clone();
    if cfg.routing.groups.iter().any(|g| g.boundary) {
        let reader = c.reader()?;
        unread.retain(|u| u.session.split(':').nth(1).is_none_or(|account| crate::boundary::may_read(&cfg, reader.as_ref(), account)));
    }
    if json {
        return print_json(&unread);
    }
    if unread.is_empty() {
        println!("No prompt of yours waits on a read.");
    }
    for u in &unread {
        println!("{}  {}  {}", u.id, u.session, u.title);
        if !u.before.is_empty() {
            println!("  The agent had said: {}", crate::util::one_line(&u.before, 300));
        }
        println!("  You{}: {}\n", if u.after_stop { ", having stopped its turn" } else if u.mid { ", while it worked" } else { "" }, crate::util::one_line(&u.text, 300));
    }
    Ok(())
}

pub(super) fn analytics_tag(c: &mut Client, id: &str, stance: &str, json: bool) -> Result<()> {
    let db = analytics_ledger(c)?;
    let by = find_me(c).ok().and_then(|(me, _)| me);
    crate::analytics::tag(&db, id, stance, by.as_deref())?;
    if json {
        return print_json(&json!({ "id": id, "stance": stance }));
    }
    println!("Noted: {}.", stance_words(stance).to_lowercase());
    Ok(())
}

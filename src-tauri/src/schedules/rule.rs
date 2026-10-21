//! When a schedule's time comes: the few words a schedule's time is written in, read into a
//! rule, said back in words, and the next time the rule names after a given one.
//!
//! What's written: `weekdays 9:00`, `mon,thu 10:30`, `daily 18:00`, `9am` (every day), `every 2h`,
//! `every 30m weekdays 09:00-18:00`, `monthly 1 9:00` (`last` for the month's last day), `once
//! 2026-10-21 15:00`, and `cron` with cron's five fields (`cron 0 9 * * 1-5`) for anything else.
//! Times are on this Mac's clock, in its time zone as it is on the day, and go by the wall clock:
//! 9:00 stays 9:00 across a change to summer time. A time the change skips (2:30 on the night
//! the clocks go forward) runs as the clock gives it (`mktime`'s 3:30), and one it repeats runs
//! once, since each day has one instant for each time. An interval that divides a day runs from
//! midnight (`every 2h` at 0:00, 2:00…), or from the start of its hours when it has some; one
//! that doesn't counts from when the schedule was made.
//!
//! The clock is a `Zone`, so the rules are tested on made-up ones, a zone with summer time
//! among them.

use crate::util::{civil_from_days, days_from_civil};

/// No rule runs more often than this: a schedule starts or tells an agent something each time,
/// and every few seconds would only burn usage.
pub const MIN_GAP_MS: i64 = 5 * 60_000;
const MINUTE_MS: i64 = 60_000;
const DAY_MINUTES: i64 = 24 * 60;
/// How far ahead a rule's next time is looked for: cron can name a day that comes once in
/// years (February 29th on a Monday).
const LOOK_AHEAD_DAYS: i64 = 366 * 8;

/// A date and time on a clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
}

impl Civil {
    /// Days since 1970-01-01 of its date.
    fn days(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }
}

/// A clock's time zone: what the clock says at an instant, and the instant a time on it names.
pub trait Zone {
    fn civil(&self, ms: i64) -> Civil;
    /// The instant at that time on that date, or none where the clock has no such time.
    fn at(&self, year: i64, month: i64, day: i64, hour: i64, minute: i64) -> Option<i64>;
}

/// This Mac's clock, in its time zone.
pub struct Local;

impl Zone for Local {
    fn civil(&self, ms: i64) -> Civil {
        let secs = ms.div_euclid(1000) as libc::time_t;
        // SAFETY: localtime_r fills the struct it's given; a zeroed tm is a valid one.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&secs, &mut tm) };
        Civil { year: tm.tm_year as i64 + 1900, month: tm.tm_mon as i64 + 1, day: tm.tm_mday as i64, hour: tm.tm_hour as i64, minute: tm.tm_min as i64 }
    }

    fn at(&self, year: i64, month: i64, day: i64, hour: i64, minute: i64) -> Option<i64> {
        crate::util::local_ms(year, month, day, hour, minute, 0, 0)
    }
}

/// Days of the week, Sunday as 0, as a set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Days(u8);

impl Days {
    pub const ALL: Days = Days(0b111_1111);
    const WEEKDAYS: Days = Days(0b011_1110);
    const WEEKENDS: Days = Days(0b100_0001);

    fn has(self, wday: i64) -> bool {
        self.0 & (1 << wday) != 0
    }

    /// As a sentence starts with them: "Every day", "Weekdays", "Mondays and Thursdays".
    fn words(self) -> String {
        const NAMES: [&str; 7] = ["Sundays", "Mondays", "Tuesdays", "Wednesdays", "Thursdays", "Fridays", "Saturdays"];
        match self {
            Days::ALL => "Every day".into(),
            Days::WEEKDAYS => "Weekdays".into(),
            Days::WEEKENDS => "Weekends".into(),
            _ => {
                // Monday first, as a week is read.
                let names: Vec<String> = [1, 2, 3, 4, 5, 6, 0].into_iter().filter(|&d| self.has(d)).map(|d| NAMES[d as usize].to_string()).collect();
                crate::util::listed(&names, "and")
            }
        }
    }
}

/// The day of the week of a date, as days since 1970-01-01: Sunday is 0 (1970-01-01 was a
/// Thursday).
fn weekday(days: i64) -> i64 {
    (days + 4).rem_euclid(7)
}

/// A schedule's time, as read from its words.
#[derive(Clone, Debug, PartialEq)]
pub enum Rule {
    /// At these times, minutes into the day, on these days.
    Times { days: Days, times: Vec<i64> },
    /// At these times on these days of each month: 1 to 31, and 0 for its last.
    Monthly { dates: Vec<i64>, times: Vec<i64> },
    /// Every so many minutes, on these days, within these minutes of the day when it has some.
    Every { minutes: i64, days: Days, window: Option<(i64, i64)> },
    /// Cron's five fields, each the values it takes.
    Cron(Cron),
    /// Once, at that time on that date.
    Once(Civil),
}

/// Cron's fields as the values each takes. `dom_any` and `dow_any` say a field was `*`: cron runs
/// on a day either names when both name some, and on the days the other names when one is `*`.
#[derive(Clone, Debug, PartialEq)]
pub struct Cron {
    pub text: String,
    minutes: Vec<i64>,
    hours: Vec<i64>,
    doms: Vec<i64>,
    months: Vec<i64>,
    dows: Vec<i64>,
    dom_any: bool,
    dow_any: bool,
}

impl Cron {
    fn on(&self, days: i64) -> bool {
        let (_, month, day) = civil_from_days(days);
        if !self.months.contains(&month) {
            return false;
        }
        let dom = self.doms.contains(&day);
        let dow = self.dows.contains(&weekday(days));
        match (self.dom_any, self.dow_any) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => dom || dow,
        }
    }
}

/// What's written in place of a rule that doesn't read, as the error ends.
const EXAMPLES: &str = "Try “weekdays 9:00”, “every 2h”, “monthly 1 9:00”, “once 2026-10-21 15:00” or “cron 0 9 * * 1-5”.";

/// Reads a schedule's time from its words, or says why it can't.
pub fn parse(text: &str) -> Result<Rule, String> {
    let lower = text.to_lowercase();
    // Cron's fields take lists (`0,30`), which the other forms write as words.
    let fields: Vec<String> = lower.split_whitespace().map(str::to_string).collect();
    if fields.first().map(String::as_str) == Some("cron") {
        return Ok(Rule::Cron(cron(&fields[1..])?));
    }
    let words: Vec<String> = lower.replace(',', " ").split_whitespace().map(str::to_string).collect();
    let rule = match words.first().map(String::as_str) {
        None => return Err(format!("Say when it runs. {EXAMPLES}")),
        Some("once") => once(&words[1..])?,
        Some("every") if !matches!(words.get(1).map(String::as_str), Some("day" | "weekday" | "weekdays")) => every(&words[1..])?,
        Some("monthly") => monthly(&words[1..])?,
        _ => times(&words)?,
    };
    Ok(rule)
}

/// Reads a rule and checks it never runs twice within `MIN_GAP_MS` (`shortest_gap`).
pub fn parse_checked(text: &str) -> Result<Rule, String> {
    let rule = parse(text)?;
    if shortest_gap(&rule).is_some_and(|m| m * MINUTE_MS < MIN_GAP_MS) {
        return Err("That runs more often than every 5 minutes. Each run starts or tells an agent something, so leave at least 5 minutes between them.".into());
    }
    Ok(rule)
}

/// The shortest time, in minutes, between two of a rule's runs, worked out from what it names
/// rather than from a few of its times, so a close pair late in a long list is found too. Times on
/// two days are taken as on days in a row (a day's last and the next one's first), which is the
/// closest they come. None for one that runs once.
pub fn shortest_gap(rule: &Rule) -> Option<i64> {
    // The gaps between sorted values in a cycle of `round`: each to the next, and the last round
    // to the first.
    let cyclic = |values: &[i64], round: i64| -> Option<i64> {
        let first = *values.first()?;
        let last = *values.last()?;
        values.windows(2).map(|w| w[1] - w[0]).chain(std::iter::once(round - last + first)).min()
    };
    match rule {
        Rule::Times { times, .. } | Rule::Monthly { times, .. } => cyclic(times, DAY_MINUTES),
        Rule::Every { minutes, .. } => Some(*minutes),
        Rule::Cron(c) => {
            let within = c.minutes.windows(2).map(|w| w[1] - w[0]).min();
            // From an hour's last minute to the next hour's first.
            let hours = cyclic(&c.hours, 24)?;
            let across = hours * 60 - c.minutes.last()? + c.minutes.first()?;
            Some(within.map_or(across, |w| w.min(across)))
        }
        Rule::Once(_) => None,
    }
}

/// A time of day as written: `9`, `9am`, `9:30`, `09:30pm`, `21:15`, `noon`, `midnight`. `pm`
/// may come as the next word, which says how many words it took.
fn time_of_day(word: &str, next: Option<&str>) -> Option<(i64, usize)> {
    match word {
        "noon" => return Some((12 * 60, 1)),
        "midnight" => return Some((0, 1)),
        _ => {}
    }
    let (body, mut half, mut took) = if let Some(b) = word.strip_suffix("am") {
        (b, Some(false), 1)
    } else if let Some(b) = word.strip_suffix("pm") {
        (b, Some(true), 1)
    } else {
        (word, None, 1)
    };
    if half.is_none() {
        match next {
            Some("am") => (half, took) = (Some(false), 2),
            Some("pm") => (half, took) = (Some(true), 2),
            _ => {}
        }
    }
    let (h, m) = match body.split_once(':') {
        Some((h, m)) if m.len() == 2 => (h.parse::<i64>().ok()?, m.parse::<i64>().ok()?),
        Some(_) => return None,
        None => (body.parse::<i64>().ok()?, 0),
    };
    if body.is_empty() || !body.chars().all(|c| c.is_ascii_digit() || c == ':') || m > 59 {
        return None;
    }
    let h = match half {
        Some(pm) if (1..=12).contains(&h) => h % 12 + if pm { 12 } else { 0 },
        Some(_) => return None,
        None if h <= 23 => h,
        None => return None,
    };
    Some((h * 60 + m, took))
}

/// A day of the week's name, short or long, singular or plural: Sunday is 0.
fn day_name(word: &str) -> Option<i64> {
    let w = word.trim_end_matches('s');
    let names = [["sun", "sunday"], ["mon", "monday"], ["tue", "tuesday"], ["wed", "wednesday"], ["thu", "thursday"], ["fri", "friday"], ["sat", "saturday"]];
    let found = names.iter().position(|n| n.contains(&w) || (w.len() >= 3 && n[1].starts_with(w)));
    found.map(|i| i as i64)
}

/// Words that name days: `daily`, `weekdays`, `weekends`, a day's name, or a range of them
/// (`mon-fri`). None for a word that names none.
fn days_of(word: &str) -> Option<Days> {
    match word {
        "daily" | "everyday" | "day" => return Some(Days::ALL),
        "weekday" | "weekdays" => return Some(Days::WEEKDAYS),
        "weekend" | "weekends" => return Some(Days::WEEKENDS),
        _ => {}
    }
    if let Some((a, b)) = word.split_once('-') {
        let (a, b) = (day_name(a)?, day_name(b)?);
        let mut bits = 0u8;
        let mut d = a;
        loop {
            bits |= 1 << d;
            if d == b {
                break;
            }
            d = (d + 1) % 7;
        }
        return Some(Days(bits));
    }
    day_name(word).map(|d| Days(1 << d))
}

/// `[days…] time [time…]`: days and times of the week, every day when no day is named.
fn times(words: &[String]) -> Result<Rule, String> {
    let mut days: Option<u8> = None;
    let mut times = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let w = words[i].as_str();
        if matches!(w, "at" | "and" | "on" | "every") {
            i += 1;
            continue;
        }
        if let Some(d) = days_of(w) {
            days = Some(days.unwrap_or(0) | d.0);
            i += 1;
            continue;
        }
        match time_of_day(w, words.get(i + 1).map(String::as_str)) {
            Some((t, took)) => {
                times.push(t);
                i += took;
            }
            None => return Err(format!("Couldn't read “{w}”. {EXAMPLES}")),
        }
    }
    if times.is_empty() {
        return Err(format!("Say what time it runs. {EXAMPLES}"));
    }
    times.sort_unstable();
    times.dedup();
    Ok(Rule::Times { days: Days(days.unwrap_or(Days::ALL.0)), times })
}

/// `every <n>m|h [days…] [HH:MM-HH:MM]`.
fn every(words: &[String]) -> Result<Rule, String> {
    let Some(first) = words.first() else { return Err(format!("Say how often. {EXAMPLES}")) };
    // `every 2 hours` as well as `every 2h`; `every hour` as `every 1h`.
    let (amount, mut rest) = match first.find(|c: char| !c.is_ascii_digit()) {
        Some(0) => ("1".to_string() + first, &words[1..]),
        Some(_) => (first.clone(), &words[1..]),
        None => match words.get(1) {
            Some(unit) => (format!("{first}{unit}"), &words[2..]),
            None => return Err(format!("Say minutes or hours: “every {first}m” or “every {first}h”.")),
        },
    };
    let split = amount.find(|c: char| !c.is_ascii_digit()).unwrap_or(amount.len());
    let (n, unit) = amount.split_at(split);
    let n: i64 = n.parse().map_err(|_| format!("Couldn't read “every {first}”. {EXAMPLES}"))?;
    let minutes = match unit {
        "m" | "min" | "mins" | "minute" | "minutes" => n,
        "h" | "hr" | "hrs" | "hour" | "hours" => n * 60,
        _ => return Err(format!("Couldn't read “every {first}”: say minutes or hours (“every 30m”, “every 2h”). For days, write the time: “daily 9:00”.")),
    };
    if minutes <= 0 || minutes >= DAY_MINUTES {
        return Err("An interval runs from 5 minutes to under a day. For once a day or less, write the time: “daily 9:00”.".into());
    }
    let mut days: Option<u8> = None;
    let mut window = None;
    while let Some(w) = rest.first() {
        rest = &rest[1..];
        if matches!(w.as_str(), "on" | "and" | "between") {
            continue;
        }
        if let Some(d) = days_of(w) {
            days = Some(days.unwrap_or(0) | d.0);
            continue;
        }
        let range = w.split_once('-').and_then(|(a, b)| Some((time_of_day(a, None)?.0, time_of_day(b, None)?.0)));
        match range {
            Some((a, b)) if a < b => window = Some((a, b)),
            Some(_) => return Err(format!("“{w}” ends before it starts; hours that run past midnight aren't taken.")),
            None => return Err(format!("Couldn't read “{w}”. Hours are written “09:00-18:00”.")),
        }
    }
    Ok(Rule::Every { minutes, days: Days(days.unwrap_or(Days::ALL.0)), window })
}

/// `monthly <day|last>… time…`.
fn monthly(words: &[String]) -> Result<Rule, String> {
    let mut dates = Vec::new();
    let mut times = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let raw = words[i].as_str();
        // `1st`, `22nd`: the number, where what's left is one.
        let w = ["st", "nd", "rd", "th"].iter().find_map(|s| raw.strip_suffix(s).filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))).unwrap_or(raw);
        if matches!(w, "on" | "the" | "at" | "and" | "day" | "of") {
            i += 1;
            continue;
        }
        if w == "last" {
            dates.push(0);
            i += 1;
            continue;
        }
        if !words[i].contains(':') && !words[i].ends_with("am") && !words[i].ends_with("pm") && words.get(i + 1).is_none_or(|n| n != "am" && n != "pm") {
            if let Ok(d) = w.parse::<i64>() {
                if !(1..=31).contains(&d) {
                    return Err(format!("A month has no day {d}."));
                }
                dates.push(d);
                i += 1;
                continue;
            }
        }
        match time_of_day(&words[i], words.get(i + 1).map(String::as_str)) {
            Some((t, took)) => {
                times.push(t);
                i += took;
            }
            None => return Err(format!("Couldn't read “{}”. Write the day and the time: “monthly 1 9:00”, “monthly last 17:00”.", words[i])),
        }
    }
    if dates.is_empty() || times.is_empty() {
        return Err("Write the day and the time: “monthly 1 9:00”, “monthly last 17:00”.".into());
    }
    dates.sort_unstable();
    dates.dedup();
    times.sort_unstable();
    times.dedup();
    Ok(Rule::Monthly { dates, times })
}

/// `once YYYY-MM-DD time`.
fn once(words: &[String]) -> Result<Rule, String> {
    let usage = "Write the date and the time: “once 2026-10-21 15:00”.";
    let Some(date) = words.first() else { return Err(usage.into()) };
    let parts: Vec<i64> = date.split('-').map(str::parse).collect::<Result<_, _>>().map_err(|_| usage.to_string())?;
    let [year, month, day] = parts[..] else { return Err(usage.into()) };
    if !(1..=12).contains(&month) || day < 1 || civil_from_days(days_from_civil(year, month, day)) != (year, month, day) {
        return Err(format!("{date} isn't a date on the calendar."));
    }
    let rest: Vec<&str> = words[1..].iter().map(String::as_str).filter(|w| *w != "at").collect();
    let (minutes, took) = rest.first().and_then(|w| time_of_day(w, rest.get(1).copied())).ok_or_else(|| usage.to_string())?;
    if rest.len() > took {
        return Err(usage.into());
    }
    Ok(Rule::Once(Civil { year, month, day, hour: minutes / 60, minute: minutes % 60 }))
}

/// Cron's five fields.
fn cron(fields: &[String]) -> Result<Cron, String> {
    let [m, h, dom, mon, dow] = fields else { return Err("Cron takes five fields: minute, hour, day of the month, month and day of the week (“cron 0 9 * * 1-5”).".into()) };
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    const DOWS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
    let mut dows = field(dow, 0, 7, &DOWS, 0)?;
    // 7 is Sunday too.
    for d in dows.iter_mut() {
        *d %= 7;
    }
    dows.sort_unstable();
    dows.dedup();
    Ok(Cron {
        text: fields.join(" "),
        minutes: field(m, 0, 59, &[], 0)?,
        hours: field(h, 0, 23, &[], 0)?,
        doms: field(dom, 1, 31, &[], 1)?,
        months: field(mon, 1, 12, &MONTHS, 1)?,
        dows,
        dom_any: dom == "*",
        dow_any: dow == "*",
    })
}

/// One cron field's values: `*`, `n`, `a-b`, with `/step`, in lists; `names` stand for the
/// numbers from `first_name` on.
fn field(text: &str, lo: i64, hi: i64, names: &[&str], first_name: i64) -> Result<Vec<i64>, String> {
    let value = |s: &str| -> Result<i64, String> {
        if let Some(i) = names.iter().position(|n| s.starts_with(n)) {
            return Ok(i as i64 + first_name);
        }
        let v: i64 = s.parse().map_err(|_| format!("Couldn't read “{s}” in cron's “{text}”."))?;
        if v < lo || v > hi {
            return Err(format!("{v} is out of range in cron's “{text}” ({lo} to {hi})."));
        }
        Ok(v)
    };
    let mut out = Vec::new();
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (r, s.parse::<i64>().ok().filter(|&s| s > 0).ok_or_else(|| format!("Couldn't read the step in cron's “{text}”."))?),
            None => (part, 1),
        };
        let (a, b) = match range {
            "*" => (lo, hi),
            r => match r.split_once('-') {
                Some((a, b)) => (value(a)?, value(b)?),
                // `5/15` runs from 5 to the field's end.
                None if step > 1 => (value(r)?, hi),
                None => (value(r)?, value(r)?),
            },
        };
        if a > b {
            return Err(format!("“{range}” runs backwards in cron's “{text}”."));
        }
        out.extend((a..=b).step_by(step as usize));
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

/// A time of day in words: `9:00`, `13:30`.
pub fn clock(minutes: i64) -> String {
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

/// The rule said back in a few words: "Weekdays at 9:00", "Every 2 hours".
pub fn words(rule: &Rule) -> String {
    let at = |times: &[i64]| crate::util::listed(&times.iter().map(|&t| clock(t)).collect::<Vec<_>>(), "and");
    match rule {
        Rule::Times { days, times } => format!("{} at {}", days.words(), at(times)),
        Rule::Monthly { dates, times } => {
            // The last day after the numbered ones, as it comes in the month.
            let named: Vec<String> = dates.iter().filter(|&&d| d != 0).map(|&d| format!("the {}", ordinal(d))).chain(dates.contains(&0).then(|| "the last day".to_string())).collect();
            format!("Monthly on {} at {}", crate::util::listed(&named, "and"), at(times))
        }
        Rule::Every { minutes, days, window } => {
            let often = match minutes {
                60 => "Every hour".to_string(),
                m if m % 60 == 0 => format!("Every {} hours", m / 60),
                1 => "Every minute".to_string(),
                m => format!("Every {m} minutes"),
            };
            let on = (*days != Days::ALL).then(|| days.words().to_lowercase());
            let hours = window.map(|(a, b)| format!("{}–{}", clock(a), clock(b)));
            match (on, hours) {
                (None, None) => often,
                (Some(d), None) => format!("{often}, {d}"),
                (None, Some(h)) => format!("{often}, {h}"),
                (Some(d), Some(h)) => format!("{often}, {d} {h}"),
            }
        }
        Rule::Cron(c) => format!("Cron {}", c.text),
        Rule::Once(c) => format!("Once, {}-{:02}-{:02} at {}", c.year, c.month, c.day, clock(c.hour * 60 + c.minute)),
    }
}

fn ordinal(n: i64) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// The next time the rule names after `after`, or none when it names no more (a time that was
/// once and has gone). `anchor` is when the schedule was made, which an interval that doesn't
/// divide a day counts from.
pub fn next_after(rule: &Rule, after: i64, anchor: i64, zone: &dyn Zone) -> Option<i64> {
    match rule {
        Rule::Once(c) => zone.at(c.year, c.month, c.day, c.hour, c.minute).filter(|&t| t > after),
        Rule::Every { minutes, days, window } if window.is_none() && DAY_MINUTES % minutes != 0 => {
            let step = minutes * MINUTE_MS;
            let mut t = anchor + ((after - anchor).div_euclid(step) + 1) * step;
            for _ in 0..100_000 {
                let c = zone.civil(t);
                if days.has(weekday(c.days())) {
                    return Some(t);
                }
                t += step;
            }
            None
        }
        _ => {
            let first = zone.civil(after).days();
            (first..first + LOOK_AHEAD_DAYS).find_map(|d| on_day(rule, d, zone).into_iter().find(|&t| t > after))
        }
    }
}

/// The times the rule names on a day, in order.
fn on_day(rule: &Rule, days: i64, zone: &dyn Zone) -> Vec<i64> {
    let (year, month, day) = civil_from_days(days);
    let at = |minutes: i64| zone.at(year, month, day, minutes / 60, minutes % 60);
    let mut out: Vec<i64> = match rule {
        Rule::Times { days: d, times } if d.has(weekday(days)) => times.iter().filter_map(|&t| at(t)).collect(),
        Rule::Monthly { dates, times } => {
            let month_ends = civil_from_days(days + 1).2 == 1;
            if dates.contains(&day) || (dates.contains(&0) && month_ends) {
                times.iter().filter_map(|&t| at(t)).collect()
            } else {
                Vec::new()
            }
        }
        Rule::Every { minutes, days: d, window } if d.has(weekday(days)) => {
            let (from, to) = window.unwrap_or((0, DAY_MINUTES - 1));
            (0..).map(|k| from + k * minutes).take_while(|&m| m <= to).filter_map(at).collect()
        }
        Rule::Cron(c) if c.on(days) => c.hours.iter().flat_map(|&h| c.minutes.iter().map(move |&m| h * 60 + m)).filter_map(at).collect(),
        _ => Vec::new(),
    };
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A clock some hours off UTC, with summer time from `spring` to `fall` (days since 1970)
    /// adding an hour at 2:00, as US zones do.
    pub struct Fake {
        pub offset_hours: i64,
        pub summer: Option<(i64, i64)>,
    }

    impl Fake {
        pub fn utc() -> Fake {
            Fake { offset_hours: 0, summer: None }
        }

        fn offset_at_days(&self, days: i64, hour: i64) -> i64 {
            match self.summer {
                Some((spring, fall)) if (days > spring || (days == spring && hour >= 2)) && (days < fall || (days == fall && hour < 1)) => self.offset_hours + 1,
                _ => self.offset_hours,
            }
        }
    }

    impl Zone for Fake {
        fn civil(&self, ms: i64) -> Civil {
            // Standard time first, then summer time if that's what the clock says there.
            let std = ms + self.offset_hours * 3_600_000;
            let (d, h) = (std.div_euclid(86_400_000), std.rem_euclid(86_400_000) / 3_600_000);
            let local = ms + self.offset_at_days(d, h) * 3_600_000;
            let (days, rest) = (local.div_euclid(86_400_000), local.rem_euclid(86_400_000));
            let (year, month, day) = civil_from_days(days);
            Civil { year, month, day, hour: rest / 3_600_000, minute: rest / 60_000 % 60 }
        }

        fn at(&self, year: i64, month: i64, day: i64, hour: i64, minute: i64) -> Option<i64> {
            let days = days_from_civil(year, month, day);
            // The hour the clocks skip comes out an hour later, as mktime gives it.
            let (hour, offset) = match self.summer {
                Some((spring, _)) if days == spring && hour == 2 => (hour + 1, self.offset_hours + 1),
                _ => (hour, self.offset_at_days(days, hour)),
            };
            Some(((days * 24 + hour - offset) * 60 + minute) * 60_000)
        }
    }

    pub fn ms(zone: &dyn Zone, y: i64, mo: i64, d: i64, h: i64, mi: i64) -> i64 {
        zone.at(y, mo, d, h, mi).unwrap()
    }

    fn when(zone: &dyn Zone, t: i64) -> (i64, i64, i64, i64, i64) {
        let c = zone.civil(t);
        (c.year, c.month, c.day, c.hour, c.minute)
    }

    fn next(text: &str, zone: &dyn Zone, from: i64) -> Option<(i64, i64, i64, i64, i64)> {
        let rule = parse(text).unwrap();
        next_after(&rule, from, from, zone).map(|t| when(zone, t))
    }

    #[test]
    fn reads_times_of_the_week() {
        let z = Fake::utc();
        // 2026-10-20 is a Tuesday.
        let tue_noon = ms(&z, 2026, 10, 20, 12, 0);
        assert_eq!(next("weekdays 9:00", &z, tue_noon), Some((2026, 10, 21, 9, 0)));
        assert_eq!(next("weekdays 13:30", &z, tue_noon), Some((2026, 10, 20, 13, 30)));
        assert_eq!(next("9am", &z, tue_noon), Some((2026, 10, 21, 9, 0)));
        assert_eq!(next("daily 6:15 pm", &z, tue_noon), Some((2026, 10, 20, 18, 15)));
        assert_eq!(next("mon,thu 10:30", &z, tue_noon), Some((2026, 10, 22, 10, 30)));
        assert_eq!(next("weekends noon", &z, tue_noon), Some((2026, 10, 24, 12, 0)));
        assert_eq!(next("fri-mon 8:00", &z, tue_noon), Some((2026, 10, 23, 8, 0)));
        assert_eq!(next("Mondays at 9:00 and 17:00", &z, tue_noon), Some((2026, 10, 26, 9, 0)));
        assert_eq!(next("every day 9:00", &z, tue_noon), Some((2026, 10, 21, 9, 0)));
        // A time exactly now has run: the next one is after it.
        assert_eq!(next("12:00", &z, tue_noon), Some((2026, 10, 21, 12, 0)));
    }

    #[test]
    fn reads_intervals() {
        let z = Fake::utc();
        let at = ms(&z, 2026, 10, 20, 12, 7);
        assert_eq!(next("every 2h", &z, at), Some((2026, 10, 20, 14, 0)));
        assert_eq!(next("every 15m", &z, at), Some((2026, 10, 20, 12, 15)));
        assert_eq!(next("every hour", &z, at), Some((2026, 10, 20, 13, 0)));
        assert_eq!(next("every 2 hours weekdays 9:00-17:00", &z, at), Some((2026, 10, 20, 13, 0)));
        let evening = ms(&z, 2026, 10, 23, 18, 0);
        // A Friday evening: Monday's first.
        assert_eq!(next("every 2h weekdays 09:00-17:00", &z, evening), Some((2026, 10, 26, 9, 0)));
        // 7h doesn't divide a day: it counts from when the schedule was made.
        let rule = parse("every 7h").unwrap();
        let made = ms(&z, 2026, 10, 20, 1, 0);
        assert_eq!(next_after(&rule, at, made, &z).map(|t| when(&z, t)), Some((2026, 10, 20, 15, 0)));
    }

    #[test]
    fn reads_months_once_and_cron() {
        let z = Fake::utc();
        let at = ms(&z, 2026, 10, 20, 12, 0);
        assert_eq!(next("monthly 1 9:00", &z, at), Some((2026, 11, 1, 9, 0)));
        assert_eq!(next("monthly last 17:00", &z, at), Some((2026, 10, 31, 17, 0)));
        assert_eq!(next("monthly 31 9:00", &z, ms(&z, 2026, 11, 1, 0, 0)), Some((2026, 12, 31, 9, 0)));
        assert_eq!(next("monthly the 15th at 8am", &z, at), Some((2026, 11, 15, 8, 0)));
        assert_eq!(next("once 2026-10-21 15:00", &z, at), Some((2026, 10, 21, 15, 0)));
        assert_eq!(next("once 2026-10-19 15:00", &z, at), None);
        assert_eq!(next("cron 0 9 * * 1-5", &z, at), Some((2026, 10, 21, 9, 0)));
        assert_eq!(next("cron */20 * * * *", &z, ms(&z, 2026, 10, 20, 12, 1)), Some((2026, 10, 20, 12, 20)));
        assert_eq!(next("cron 30 8 1 jan *", &z, at), Some((2027, 1, 1, 8, 30)));
        // Day of the month or day of the week, as cron has it when both are named.
        assert_eq!(next("cron 0 9 1 * fri", &z, at), Some((2026, 10, 23, 9, 0)));
        assert_eq!(next("cron 0 9 * * 7", &z, at), Some((2026, 10, 25, 9, 0)));
        // Cron's lists, with commas.
        assert_eq!(next("cron 0,30 9 * * *", &z, ms(&z, 2026, 10, 21, 9, 0)), Some((2026, 10, 21, 9, 30)));
        assert_eq!(next("cron 0 9 * * mon,fri", &z, at), Some((2026, 10, 23, 9, 0)));
    }

    #[test]
    fn says_why_it_cant_read_one() {
        assert!(parse("").unwrap_err().contains("Say when"));
        assert!(parse("tomorrowish").unwrap_err().contains("Couldn't read “tomorrowish”"));
        assert!(parse("weekdays").unwrap_err().contains("what time"));
        assert!(parse("every 2d").unwrap_err().contains("daily 9:00"));
        assert!(parse("every 2h 18:00-09:00").unwrap_err().contains("past midnight"));
        assert!(parse("cron 0 9 * *").unwrap_err().contains("five fields"));
        assert!(parse("cron 0 25 * * *").unwrap_err().contains("out of range"));
        assert!(parse("once 2026-02-30 9:00").unwrap_err().contains("isn't a date"));
        assert!(parse("monthly 32 9:00").unwrap_err().contains("no day 32"));
        assert!(parse("25:00").is_err());
        assert!(parse("13pm").is_err());
        assert!(parse_checked("every 1m").unwrap_err().contains("every 5 minutes"));
        assert!(parse_checked("cron * * * * *").unwrap_err().contains("every 5 minutes"));
        assert!(parse_checked("every 5m").is_ok());
        // A close pair after many times is found, not only among the first few.
        let many = (0..6).map(|h| format!("{h}:00")).collect::<Vec<_>>().join(" ");
        assert!(parse_checked(&format!("daily {many} 6:00 6:01")).unwrap_err().contains("every 5 minutes"));
        assert!(parse_checked("daily 23:58 0:01").unwrap_err().contains("every 5 minutes"));
        assert!(parse_checked("cron 59,0 9,10 * * *").unwrap_err().contains("every 5 minutes"));
        assert!(parse_checked("cron 0,30 9 * * *").is_ok());
        assert!(parse_checked("once 2026-10-21 9:00").is_ok());
    }

    #[test]
    fn says_a_rule_in_words() {
        let say = |t: &str| words(&parse(t).unwrap());
        assert_eq!(say("weekdays 9:00"), "Weekdays at 9:00");
        assert_eq!(say("mon,thu 10:30 4pm"), "Mondays and Thursdays at 10:30 and 16:00");
        assert_eq!(say("8am"), "Every day at 8:00");
        assert_eq!(say("every 2h"), "Every 2 hours");
        assert_eq!(say("every 30m weekdays 9:00-18:00"), "Every 30 minutes, weekdays 9:00–18:00");
        assert_eq!(say("monthly 1 last 9:00"), "Monthly on the 1st and the last day at 9:00");
        assert_eq!(say("once 2026-10-21 3pm"), "Once, 2026-10-21 at 15:00");
        assert_eq!(say("cron 0 9 * * 1-5"), "Cron 0 9 * * 1-5");
    }

    #[test]
    fn keeps_to_the_wall_clock_across_summer_time() {
        // A zone 8 hours behind UTC, with summer time from 2026-03-08 to 2026-11-01 at 2:00.
        let z = Fake { offset_hours: -8, summer: Some((days_from_civil(2026, 3, 8), days_from_civil(2026, 11, 1))) };
        let before = ms(&z, 2026, 3, 7, 12, 0);
        let first = next_after(&parse("daily 9:00").unwrap(), before, before, &z).unwrap();
        let second = next_after(&parse("daily 9:00").unwrap(), first, before, &z).unwrap();
        assert_eq!((when(&z, first), when(&z, second)), ((2026, 3, 8, 9, 0), (2026, 3, 9, 9, 0)));
        // The day the clocks go forward is an hour short.
        assert_eq!(second - first, 24 * 3_600_000);
        let saturday = ms(&z, 2026, 3, 7, 9, 0);
        assert_eq!(first - saturday, 23 * 3_600_000);
        // 2:30 doesn't happen that night: it runs at 3:30, once.
        let skipped = next_after(&parse("daily 2:30").unwrap(), ms(&z, 2026, 3, 8, 0, 0), 0, &z).unwrap();
        assert_eq!(when(&z, skipped), (2026, 3, 8, 3, 30));
    }
}

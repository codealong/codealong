//! When a snooze asked for at the command ends (`codealong session snooze`, `codealong pr
//! snooze`), from what's typed, and that time said back as the app's menus say it (snooze.ts
//! `untilWords`). What's typed is a span from now (`30m`, `in 2h`, `3d`: idle.rs `parse_after`),
//! a time of day (`5pm`, `17:30`: the next one, read as a schedule's time is, rule.rs
//! `time_of_day`), a day (`tomorrow`, `fri`, `next week`, `2026-11-03`: read as a todo's due day
//! is, todos.rs `due_from_words`) at 9 AM, the hour the menus' next morning ends at, or a day and a
//! time (`tomorrow 2pm`, `fri 17:00`, `2026-11-03 15:00`). Times are on this Mac's clock, a `Zone`
//! so the rules are tested on made-up ones.

use crate::schedules::rule::{time_of_day, Zone};
use crate::util::{civil_from_days, days_from_civil};

/// The hour a snooze to a day ends at, as the menus' next morning does.
const MORNING: i64 = 9;

const EXAMPLES: &str = "Give a span (30m, 2h, 3d), a time (5pm, 17:30), a day (tomorrow, fri, next week, 2026-11-03) or both (tomorrow 2pm, fri 17:00).";

/// When a snooze typed as `text` ends, after `now`, or why it can't be read or has passed.
pub fn until(text: &str, now: i64, zone: &dyn Zone) -> Result<i64, String> {
    let lower = text.trim().to_lowercase();
    // `at` reads as nothing more: `tomorrow at 2pm`.
    let words: Vec<&str> = lower.split_whitespace().filter(|&w| w != "at").collect();
    if words.is_empty() {
        return Err(format!("Say when it comes back. {EXAMPLES}"));
    }
    let span = words.strip_prefix(&["in"]).unwrap_or(&words);
    if let [one] = span {
        if let Some(ms) = crate::idle::parse_after(one) {
            return Ok(now + ms);
        }
    }
    // A time of day at the end: one word (`5pm`), or two (`5 pm`).
    let n = words.len();
    let time = match n {
        1 => time_of_day(words[0], None),
        _ => time_of_day(words[n - 2], Some(words[n - 1])).or_else(|| time_of_day(words[n - 1], None)),
    };
    let (minutes, day_words) = match time {
        Some((m, took)) => (Some(m), &words[..n - took]),
        None => (None, &words[..]),
    };
    let unread = || format!("Couldn't read “{}”. {EXAMPLES}", text.trim());
    let now_civil = zone.civil(now);
    let today = days_from_civil(now_civil.year, now_civil.month, now_civil.day);
    let at = |days: i64, minutes: i64| {
        let (y, m, d) = civil_from_days(days);
        zone.at(y, m, d, minutes / 60, minutes % 60)
    };
    // A time alone is the next one: today's while it's still ahead, else tomorrow's.
    if day_words.is_empty() {
        let minutes = minutes.ok_or_else(unread)?;
        return [today, today + 1].into_iter().filter_map(|d| at(d, minutes)).find(|&t| t > now).ok_or_else(unread);
    }
    let day = crate::todos::due_from_words(&day_words.join(" "), today).ok().and_then(|d| crate::todos::due_day(&d)).ok_or_else(unread)?;
    let t = at(day, minutes.unwrap_or(MORNING * 60)).ok_or_else(unread)?;
    if t <= now {
        return Err(format!("{} has passed.", words_at(t, now, zone)));
    }
    Ok(t)
}

/// A time as the app's menus say when a snooze ends (snooze.ts `untilWords`): `3:40 PM` today,
/// `tomorrow 9 AM`, `Fri 9 AM` within the week, `Oct 3, 9 AM` beyond.
pub fn words_at(at: i64, now: i64, zone: &dyn Zone) -> String {
    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let (a, n) = (zone.civil(at), zone.civil(now));
    let day = days_from_civil(a.year, a.month, a.day);
    let half = if a.hour < 12 { "AM" } else { "PM" };
    let hour = match a.hour % 12 {
        0 => 12,
        h => h,
    };
    let clock = if a.minute == 0 { format!("{hour} {half}") } else { format!("{hour}:{:02} {half}", a.minute) };
    // A day gone by has its date, as one more than a week off does.
    match day - days_from_civil(n.year, n.month, n.day) {
        0 => clock,
        1 => format!("tomorrow {clock}"),
        // 1970-01-01 was a Thursday.
        2..=6 => format!("{} {clock}", WEEKDAYS[(day + 4).rem_euclid(7) as usize]),
        _ => format!("{} {}, {clock}", MONTHS[(a.month - 1) as usize], a.day),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedules::rule::tests::{ms, Fake};

    // Wednesday, 2026-10-28, 2:10 PM on a clock 7 hours behind UTC.
    fn zone() -> Fake {
        Fake { offset_hours: -7, summer: None }
    }
    fn now() -> i64 {
        ms(&zone(), 2026, 10, 28, 14, 10)
    }
    fn until_at(text: &str) -> Result<i64, String> {
        until(text, now(), &zone())
    }

    #[test]
    fn reads_a_span_from_now() {
        assert_eq!(until_at("30m"), Ok(now() + 30 * 60_000));
        assert_eq!(until_at("in 2h"), Ok(now() + 2 * 3_600_000));
        assert_eq!(until_at("3d"), Ok(now() + 3 * 86_400_000));
    }

    #[test]
    fn reads_a_time_as_the_next_one() {
        let z = zone();
        assert_eq!(until_at("5pm"), Ok(ms(&z, 2026, 10, 28, 17, 0)));
        assert_eq!(until_at("17:30"), Ok(ms(&z, 2026, 10, 28, 17, 30)));
        assert_eq!(until_at("5 pm"), Ok(ms(&z, 2026, 10, 28, 17, 0)));
        // Past today's, tomorrow's.
        assert_eq!(until_at("9am"), Ok(ms(&z, 2026, 10, 29, 9, 0)));
        assert_eq!(until_at("noon"), Ok(ms(&z, 2026, 10, 29, 12, 0)));
    }

    #[test]
    fn reads_a_day_at_nine_or_at_its_time() {
        let z = zone();
        assert_eq!(until_at("tomorrow"), Ok(ms(&z, 2026, 10, 29, 9, 0)));
        assert_eq!(until_at("Tomorrow 2pm"), Ok(ms(&z, 2026, 10, 29, 14, 0)));
        assert_eq!(until_at("tomorrow at 2 pm"), Ok(ms(&z, 2026, 10, 29, 14, 0)));
        assert_eq!(until_at("fri 17:00"), Ok(ms(&z, 2026, 10, 30, 17, 0)));
        assert_eq!(until_at("next week"), Ok(ms(&z, 2026, 11, 2, 9, 0)));
        assert_eq!(until_at("2026-11-03 15:00"), Ok(ms(&z, 2026, 11, 3, 15, 0)));
        assert_eq!(until_at("2026-11-03"), Ok(ms(&z, 2026, 11, 3, 9, 0)));
        assert_eq!(until_at("today 6pm"), Ok(ms(&z, 2026, 10, 28, 18, 0)));
    }

    #[test]
    fn says_what_it_cant_read_or_what_has_passed() {
        assert!(until_at("").unwrap_err().starts_with("Say when it comes back."));
        assert!(until_at("whenever").unwrap_err().starts_with("Couldn't read “whenever”."));
        assert!(until_at("tomorrow at nine").is_err());
        assert_eq!(until_at("today 9am"), Err("9 AM has passed.".into()));
        assert_eq!(until_at("2026-10-01"), Err("Oct 1, 9 AM has passed.".into()));
    }

    #[test]
    fn says_a_time_as_the_menus_do() {
        let z = zone();
        let at = |d: i64, h: i64, m: i64| ms(&z, 2026, 10, d, h, m);
        assert_eq!(words_at(at(28, 15, 40), now(), &z), "3:40 PM");
        assert_eq!(words_at(at(28, 12, 0), now(), &z), "12 PM");
        assert_eq!(words_at(at(29, 9, 0), now(), &z), "tomorrow 9 AM");
        assert_eq!(words_at(at(30, 0, 5), now(), &z), "Fri 12:05 AM");
        assert_eq!(words_at(ms(&z, 2026, 11, 4, 9, 0), now(), &z), "Nov 4, 9 AM");
    }
}

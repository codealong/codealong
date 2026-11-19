//! Carrying on after a fall: a loop that everything waits on is run again when it panics,
//! rather than left dead with the process up around it. The engine's thread runs under it
//! (engine.rs `run_guarded`, which says what the engine does about a fall); what's here knows
//! nothing of the engine, so it's tested with work that falls over on purpose and a made-up
//! clock.
//!
//! - `carrying_on` runs work until it returns, again each time it panics.
//! - `supervised` is that for work in stages: a first one that has to go through once before
//!   anything else is right, one that picks up after a fall, and the loop. It counts the falls
//!   running with no whole pass of the work between them (`Fall::short`): `ENOUGH` of those
//!   and the work can't get going, which is for whoever runs it to act on. Falls of work that
//!   gets through passes are never that, however many: it's doing its work between them.
//! - `Falls` says how long to wait before going on, longer as falls come close together.
//!
//! Work that another thread only waits for the end of doesn't need this: that's
//! `throttle::background`, which hands the work's end to whoever marked it as on its way.

use std::time::{Duration, Instant};

/// This many falls running with no whole pass between them, and the work can't get going.
pub const ENOUGH: u32 = 5;

/// What `supervised` says of a fall.
pub struct Fall {
    /// What the panic said.
    pub why: String,
    /// How many falls running have come with no whole pass of the work between them: in its
    /// first stage, picking up after a fall, or in the loop before it got through one (an
    /// engine that reads everything again and then can't compose a board). This one counts,
    /// so it's 1 for a fall after work that was going.
    pub short: u32,
}

/// How long to wait before going on after a fall: nothing the first time, then longer each time
/// it falls again within a minute of the last, up to half a minute. Something tripped on at
/// every try would otherwise have the work fall over as fast as it can start.
#[derive(Default)]
pub struct Falls {
    /// The falls in a row, each within `Falls::CLOSE` of the one before.
    row: u32,
    last: Option<Instant>,
}

impl Falls {
    const CLOSE: Duration = Duration::from_secs(60);
    pub const LONGEST: Duration = Duration::from_secs(30);

    pub fn fell(&mut self, now: Instant) -> Duration {
        let close = self.last.is_some_and(|last| now.saturating_duration_since(last) < Self::CLOSE);
        self.row = if close { self.row + 1 } else { 1 };
        self.last = Some(now);
        match self.row {
            1 => Duration::ZERO,
            row => Duration::from_millis(250 * (1u64 << (row - 2).min(10))).min(Self::LONGEST),
        }
    }
}

/// What a panic said, for a notice: its message where it had one in words.
pub fn panic_words(panic: &(dyn std::any::Any + Send)) -> String {
    panic.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| panic.downcast_ref::<String>().cloned()).unwrap_or_else(|| "it didn't say why".into())
}

/// Runs `work` on `state` until it returns, and again each time it panics: `fell` is told what
/// the panic said and answers how long to wait first, and `work` is told it's going again. The
/// panic's own words and place are in the log, where the default hook wrote them.
pub fn carrying_on<T>(state: &mut T, mut work: impl FnMut(&mut T, bool), mut fell: impl FnMut(&mut T, String) -> Duration) {
    let mut again = false;
    loop {
        let went = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(state, again)));
        let Err(panic) = went else { return };
        again = true;
        let wait = fell(state, panic_words(panic.as_ref()));
        std::thread::sleep(wait);
    }
}

/// Runs work in stages on `state` until `run` returns, through any that fall over
/// (`carrying_on`): `first` until it has gone through once, and after that `again` before `run`
/// goes on from a fall. `passed` says whether the work has got through a whole pass since it
/// was last asked, and `fell` is told of each fall and how many running have come without one.
pub fn supervised<T>(
    state: &mut T,
    mut first: impl FnMut(&mut T),
    mut again: impl FnMut(&mut T),
    mut run: impl FnMut(&mut T),
    mut passed: impl FnMut(&mut T) -> bool,
    mut fell: impl FnMut(&mut T, &Fall) -> Duration,
) {
    let mut started = false;
    let mut short = 0u32;
    carrying_on(
        state,
        |state, after_a_fall| {
            if !started {
                first(state);
                started = true;
            } else if after_a_fall {
                again(state);
            }
            run(state)
        },
        |state, why| {
            short = if passed(state) { 1 } else { short + 1 };
            fell(state, &Fall { why, short })
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_that_falls_over_is_carried_on_and_says_why() {
        // What ran, in order: the first run falls over twice and the third time ends.
        let mut ran: Vec<String> = Vec::new();
        carrying_on(
            &mut ran,
            |ran, again| {
                ran.push(format!("run, again: {again}"));
                let runs = ran.iter().filter(|r| r.starts_with("run")).count();
                if runs < 3 {
                    panic!("fell over on run {runs}");
                }
            },
            |ran, why| {
                ran.push(format!("said: {why}"));
                Duration::ZERO
            },
        );
        assert_eq!(ran, ["run, again: false", "said: fell over on run 1", "run, again: true", "said: fell over on run 2", "run, again: true"]);
        // A panic with no words still has something said of it.
        assert_eq!(panic_words(&7u8), "it didn't say why");
    }

    #[test]
    fn the_first_stage_is_run_until_it_goes_through_and_a_later_fall_picks_up() {
        // The first stage falls over once, and the loop once after it: what ran, in order.
        let mut ran: Vec<&str> = Vec::new();
        supervised(
            &mut ran,
            |ran| {
                ran.push("first");
                if ran.iter().filter(|r| **r == "first").count() == 1 {
                    panic!("the first stage fell over");
                }
            },
            |ran| ran.push("again"),
            |ran| {
                ran.push("run");
                if ran.iter().filter(|r| **r == "run").count() == 1 {
                    panic!("the loop fell over");
                }
            },
            |_| false,
            |ran, _| {
                ran.push("fell");
                Duration::ZERO
            },
        );
        // Nothing is picked up, and the loop isn't entered, before the first has gone through.
        assert_eq!(ran, ["first", "fell", "first", "run", "fell", "again", "run"]);
    }

    /// Work whose stages fall over as a test says: how often each has run, and whether a whole
    /// pass has been got through since that was last asked.
    #[derive(Default)]
    struct Work {
        firsts: u32,
        agains: u32,
        runs: u32,
        passed: bool,
    }

    #[test]
    fn falls_with_no_pass_between_them_are_counted_and_a_pass_starts_the_count_again() {
        // The first stage falls three times; the loop then gets through a pass and falls;
        // picking up falls twice; the loop passes and falls once more, and then ends.
        let mut work = Work::default();
        let mut shorts: Vec<u32> = Vec::new();
        supervised(
            &mut work,
            |work| {
                work.firsts += 1;
                if work.firsts <= 3 {
                    panic!("first");
                }
            },
            |work| {
                work.agains += 1;
                if work.agains <= 2 {
                    panic!("again");
                }
            },
            |work| {
                work.runs += 1;
                work.passed = true;
                if work.runs <= 2 {
                    panic!("run");
                }
            },
            |work| std::mem::take(&mut work.passed),
            |_, fall| {
                shorts.push(fall.short);
                Duration::ZERO
            },
        );
        // Work that falls now and then between its passes is never "can't get going".
        assert_eq!(shorts, [1, 2, 3, 1, 2, 3, 1]);
        assert!(shorts.iter().all(|short| *short < ENOUGH));
    }

    #[test]
    fn work_that_picks_up_and_never_gets_through_a_pass_cant_get_going() {
        // Reading again works every time, and the pass after it falls every time before it's
        // through: the loop is reached, and nothing is ever done in it.
        let mut work = Work::default();
        let mut shorts: Vec<u32> = Vec::new();
        supervised(
            &mut work,
            |work| work.firsts += 1,
            |work| work.agains += 1,
            |work| {
                work.runs += 1;
                if work.runs <= 6 {
                    panic!("the pass fell over before it was through");
                }
            },
            |work| std::mem::take(&mut work.passed),
            |_, fall| {
                shorts.push(fall.short);
                Duration::ZERO
            },
        );
        assert_eq!(shorts, [1, 2, 3, 4, 5, 6]);
        assert!(shorts.contains(&ENOUGH), "reaching the loop isn't getting going");
        assert_eq!((work.firsts, work.agains), (1, 6));
    }

    #[test]
    fn falls_close_together_wait_longer_each_time_and_one_alone_doesnt_wait() {
        let start = Instant::now();
        let at = |secs: u64| start + Duration::from_secs(secs);
        let mut falls = Falls::default();
        assert_eq!(falls.fell(at(0)), Duration::ZERO, "the first is carried on from at once");
        assert_eq!(falls.fell(at(1)), Duration::from_millis(250));
        assert_eq!(falls.fell(at(2)), Duration::from_millis(500));
        assert_eq!(falls.fell(at(3)), Duration::from_secs(1));
        for second in 4..20 {
            falls.fell(at(second));
        }
        assert_eq!(falls.fell(at(20)), Falls::LONGEST, "never longer than half a minute");
        // A minute's quiet, and it's the first again.
        assert_eq!(falls.fell(at(20) + Falls::CLOSE), Duration::ZERO);
    }
}

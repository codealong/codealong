//! The engine's thread under a guard: what the engine does about a fall (a panic in anything on
//! its one thread), on carryon.rs's loop. The first scan and the loop run under `run_guarded`;
//! what's read again after a fall is `Engine::pick_up`, and the notice it's said as
//! `Engine::fell_over`, both the engine's own. An engine that can't get a board out is the
//! host's to act on (`when_spent`, `give_up`): serve.rs ends the server it runs.

use std::sync::mpsc::Receiver;
use std::time::Instant;

use super::{Engine, Msg};

/// Runs the engine until its channel closes, and carries on when its thread falls over (a panic
/// in anything it does: a rule's `expect`, an index past the end). Everything waits on this
/// thread: left to die, the server stays up with nothing behind it, every window showing the
/// board as it last was and every request waiting for good.
///
/// The loop that does it, its stages and its waits are carryon.rs's. The first scan is under
/// the guard too, and is run again until it has gone through once: nothing the engine holds is
/// right before it has. After that a fall is carried on from: the engine says so as a notice,
/// reads again everything a change could have told it (`Engine::pick_up`), and goes on. What it
/// was doing when it fell is lost, and the panic's own words and place are in the log, where
/// the default hook wrote them.
///
/// An engine that falls now and then between its work is carried on however often. One that
/// can't get a board out (`carryon::ENOUGH` falls running with none published between them: in
/// the first scan, in reading again after a fall, or composing the board after that) isn't
/// working at all, and its requests and the watcher's events pile up unread: that's the
/// host's to act on (`give_up`), and the thread ends there. With no host to act (the app,
/// running the server itself), that leaves what the engine left before there was a guard: the
/// receiver goes with the thread, so what's asked of the engine fails at once, where trying on
/// would keep every request waiting behind an engine that doesn't get to it.
pub(super) fn run_guarded(engine: &mut Engine, rx: &Receiver<Msg>) {
    let mut falls = crate::carryon::Falls::default();
    crate::carryon::supervised(
        engine,
        |engine| {
            engine.full_scan();
            crate::skills::warm(&engine.cfg.accounts);
            engine.got_going = true;
        },
        Engine::pick_up,
        |engine| engine.run(rx),
        // A board got out since the last fall: the engine is doing its work.
        |engine| std::mem::take(&mut engine.passed),
        |engine, fall| {
            let wait = falls.fell(Instant::now());
            engine.fell_over(&fall.why);
            if fall.short >= crate::carryon::ENOUGH {
                give_up(&fall.why, engine.got_going);
                return None;
            }
            Some(wait)
        },
    );
}

/// What the host does about an engine that can't get going, when it has said (`when_spent`).
static SPENT: std::sync::OnceLock<Box<dyn Fn(&str, bool) + Send + Sync>> = std::sync::OnceLock::new();

/// Says what's to be done when the engine can't get going: it couldn't be made, or fell over
/// `carryon::ENOUGH` times running without a board between them. `then` is told why, and
/// whether this engine ever got through its first scan: one that worked and stopped is another
/// matter than one that never started. (Not whether a board ever got out: the first scan
/// publishes one partway.) The server run by itself ends its process (serve.rs `run`). A host
/// that hasn't said (the app, running the server itself when it couldn't start one) has nothing
/// to hand over to: the engine's thread ends there, as a panic ended it before there was a
/// guard, and what's asked of it fails at once.
pub fn when_spent(then: impl Fn(&str, bool) + Send + Sync + 'static) {
    let _ = SPENT.set(Box::new(then));
}

/// Tells the log, and the host if it asked to be told (`when_spent`), that the engine can't get
/// going. Nothing here may stop what follows it, so the log is written without `eprintln!`,
/// which panics when it can't write.
pub(super) fn give_up(why: &str, had_got_going: bool) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "Codealong's engine can't get going: it has fallen over {} times running with no board between them (the last: {why}).", crate::carryon::ENOUGH);
    if let Some(then) = SPENT.get() {
        then(why, had_got_going);
    }
}

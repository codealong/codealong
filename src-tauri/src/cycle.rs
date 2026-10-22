//! Switching the mode of a session that runs in a terminal: Claude Code's and Antigravity's
//! terminal UIs step through their modes with Shift+Tab, mid-turn too, and Codealong presses it
//! in the session's tab (Ghostty, iTerm2, tmux; Terminal presses no key alone). Their readers
//! list the modes in that order (`Session::modes`), and `overlay` keeps them where the tab takes
//! keys. Antigravity writes each switch to its log as it takes it, so a switch watches it there
//! (`Adapter::mode_said`). Claude Code writes the mode only with the next prompt, so the mode a
//! press leads to is the next in its order, held as expected (`SessionMode::expected`) until its
//! record says.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};

use crate::config::AccountCfg;
use crate::harness::Ctx;
use crate::launch::{self, Key};
use crate::model::{Session, SessionMode};
use crate::send::{quoted, ModeSwitched};

/// How long a press gets for its harness to say it took it, and how often that's looked for.
/// Antigravity wrote each switch to its log within milliseconds (1.4.5, 2026-10-22).
const TAKEN_WAIT: Duration = Duration::from_secs(3);
const LOOK_EVERY: Duration = Duration::from_millis(50);
/// Between presses nothing confirms, as `antigravity.rs` presses its keys.
const PRESS_GAP: Duration = Duration::from_millis(100);

/// A mode Codealong pressed a session to, which its harness hasn't written yet.
#[derive(Debug, Clone, PartialEq)]
struct Expected {
    /// The process it was pressed in: another starts in a mode of its own.
    pid: u32,
    /// The mode, by its id.
    mode: String,
    /// The mode its record said it was in before the first press.
    recorded: Option<String>,
    /// When it was last pressed.
    at: i64,
}

fn expected() -> MutexGuard<'static, Option<HashMap<String, Expected>>> {
    static EXPECTED: Mutex<Option<HashMap<String, Expected>>> = Mutex::new(None);
    EXPECTED.lock().unwrap_or_else(|e| e.into_inner())
}

/// Keeps a session's `modes` where Codealong can press Shift+Tab in its terminal, and lays the
/// mode a press is expected to have led to over the one its record says, until the record says
/// more: a prompt since, which writes the mode it went in, or another mode written. Asked of
/// every session each time the board is put together; only a running one in a terminal whose
/// harness lists its modes costs a look up its process tree, as `Adapter::steers` does.
pub fn overlay(s: &mut Session) {
    if s.hosted {
        return;
    }
    let presses = s.live && !s.modes.is_empty() && s.pid.is_some_and(|pid| launch::terminal_kind(pid).is_some_and(|t| t.presses_keys()));
    let mut all = expected();
    let all = all.get_or_insert_with(HashMap::new);
    if !presses {
        s.modes.clear();
        if let Some(m) = s.mode.as_mut() {
            m.id.clear();
        }
        all.remove(&s.key);
        return;
    }
    if let Some(mode) = holds(all.get(&s.key), s) {
        s.mode = Some(SessionMode { expected: true, ..mode });
    } else {
        all.remove(&s.key);
    }
}

/// The mode an expectation still lays over a session read from its record, if it does.
fn holds(e: Option<&Expected>, s: &Session) -> Option<SessionMode> {
    let e = e?;
    let recorded = s.mode.as_ref().map(|m| m.id.clone()).filter(|id| !id.is_empty());
    if s.pid != Some(e.pid) || s.last_prompt_at > e.at || recorded != e.recorded || recorded.as_deref() == Some(e.mode.as_str()) {
        return None;
    }
    s.modes.iter().find(|m| m.id == e.mode).cloned()
}

/// Switches a session in a terminal to `to`, one of its `modes`, by pressing Shift+Tab there as
/// often as it takes. Not while it waits on you, where Shift+Tab goes to what it asks: Claude
/// Code's permission prompts take it as their own (`confirm:cycleMode`), and what that does to
/// the call that waits wasn't looked into.
pub fn switch(account: &AccountCfg, s: &Session, to: &SessionMode) -> Result<ModeSwitched> {
    let title = quoted(&s.title);
    let pid = s.pid.filter(|_| s.live).ok_or_else(|| anyhow!("{title} isn't running any more."))?;
    if crate::send::waits_on_you(s) {
        bail!("{title} is waiting on you, and Shift+Tab in its terminal would go to what it asks: answer it first.");
    }
    let Some(host) = launch::terminal_host(pid, Path::new(&s.cwd))? else {
        bail!("{title} isn't in a terminal Codealong can press keys in ({}): switch its mode there.", launch::presses_keys_in());
    };
    let named = |id: &str| s.modes.iter().find(|m| m.id == id).map_or_else(|| id.to_string(), |m| m.label.clone());
    let press = || launch::press_key(&host, Key::BackTab);
    // Its harness says each switch: pressed until it's there, or round once.
    if let Some(mut said) = crate::harness::adapter(s.harness).mode_said(&Ctx { account }, s) {
        let mut now = said().ok_or_else(|| anyhow!("Codealong can't tell which mode {title} is in now: switch it in its terminal."))?;
        for _ in 0..s.modes.len() {
            if now == to.id {
                break;
            }
            press()?;
            now = taken(&mut said, &now).ok_or_else(|| anyhow!("{title}'s terminal didn't take Shift+Tab: it's still in {} mode.", named(&now)))?;
        }
        let message = if now == to.id { format!("{title} is in {} mode now.", to.label) } else { format!("{title} is in {} mode: Shift+Tab went round without coming to {}.", named(&now), to.label) };
        return Ok(ModeSwitched { message, mode: now });
    }
    // Nothing says until its next prompt: pressed as often as its order takes from the mode it's
    // in as far as Codealong knows, and expected to be there.
    let ids: Vec<&str> = s.modes.iter().map(|m| m.id.as_str()).collect();
    let presses = presses_to(&ids, s.mode.as_ref().map(|m| m.id.as_str()), &to.id).ok_or_else(|| anyhow!("{:?} isn't a mode {title} steps through.", to.id))?;
    if presses == 0 {
        return Ok(ModeSwitched { message: format!("{title} is in {} mode already.", to.label), mode: to.id.clone() });
    }
    for i in 0..presses {
        if i > 0 {
            std::thread::sleep(PRESS_GAP);
        }
        press()?;
    }
    expect(s, pid, &to.id, crate::util::now_ms());
    let times = if presses == 1 { String::new() } else { format!(" {presses} times") };
    let message = format!("{title} should be in {} mode now: Codealong pressed Shift+Tab in its terminal{times}, and {} says which mode it's in only with your next message.", to.label, s.harness.word());
    Ok(ModeSwitched { message, mode: to.id.clone() })
}

/// How many presses step from `from` to `to` in `order`, round again at its end: one past the
/// first to get into it from a mode it doesn't list (Claude Code's Don't Ask steps to Manual).
fn presses_to(order: &[&str], from: Option<&str>, to: &str) -> Option<usize> {
    let to = order.iter().position(|id| *id == to)?;
    Some(match order.iter().position(|id| Some(*id) == from) {
        Some(at) => (to + order.len() - at) % order.len(),
        None => to + 1,
    })
}

/// Holds `mode` as the one a session was pressed to at `at`, over what its record says, which
/// is the mode before the first of the presses still unconfirmed.
fn expect(s: &Session, pid: u32, mode: &str, at: i64) {
    let mut all = expected();
    let all = all.get_or_insert_with(HashMap::new);
    let recorded = match all.get(&s.key) {
        Some(e) if e.pid == pid => e.recorded.clone(),
        _ => s.mode.as_ref().map(|m| m.id.clone()).filter(|id| !id.is_empty()),
    };
    all.insert(s.key.clone(), Expected { pid, mode: mode.to_string(), recorded, at });
}

/// What `said` says once it's no longer `before`, within `TAKEN_WAIT`.
fn taken(said: &mut dyn FnMut() -> Option<String>, before: &str) -> Option<String> {
    let until = Instant::now() + TAKEN_WAIT;
    loop {
        if let Some(now) = said().filter(|m| m != before) {
            return Some(now);
        }
        if Instant::now() >= until {
            return None;
        }
        std::thread::sleep(LOOK_EVERY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Harness, ModeKind, Status, Surface};

    fn claude(mode: &str) -> Session {
        let modes: Vec<SessionMode> = ["default", "acceptEdits", "plan", "auto"].iter().map(|id| crate::claude::session_mode(id).unwrap().with_id(id)).collect();
        Session {
            title: "Fix the build".into(),
            surface: Surface::Cli,
            status: Status::Working,
            live: true,
            pid: Some(7),
            last_prompt_at: 1_000,
            mode: modes.iter().find(|m| m.id == mode).cloned(),
            modes,
            ..Session::new(Harness::Claude, "claude", "s1")
        }
    }

    #[test]
    fn counts_the_presses_round_its_order() {
        let order = ["default", "acceptEdits", "plan", "auto"];
        assert_eq!(presses_to(&order, Some("auto"), "default"), Some(1));
        assert_eq!(presses_to(&order, Some("auto"), "plan"), Some(3));
        assert_eq!(presses_to(&order, Some("plan"), "acceptEdits"), Some(3));
        assert_eq!(presses_to(&order, Some("plan"), "plan"), Some(0));
        assert_eq!(presses_to(&order, Some("dontAsk"), "default"), Some(1), "Don't Ask steps to Manual");
        assert_eq!(presses_to(&order, None, "acceptEdits"), Some(2));
        assert_eq!(presses_to(&order, Some("auto"), "bypassPermissions"), None);
    }

    /// The mode a press leads to holds until the session's record says more.
    #[test]
    fn an_expected_mode_holds_until_its_record_says() {
        let s = claude("auto");
        let at = |pid: u32, mode: &str, recorded: &str, at: i64| Expected { pid, mode: mode.into(), recorded: Some(recorded.into()), at };
        let shown = |e: &Expected, s: &Session| holds(Some(e), s).map(|m| m.id);
        let pressed = at(7, "plan", "auto", 2_000);
        assert_eq!(shown(&pressed, &s).as_deref(), Some("plan"));
        assert_eq!(holds(None, &s), None);
        // A prompt since writes the mode it went in.
        assert_eq!(shown(&pressed, &Session { last_prompt_at: 2_500, ..s.clone() }), None);
        // Another mode written, or the one expected.
        assert_eq!(shown(&pressed, &claude("acceptEdits")), None);
        assert_eq!(shown(&at(7, "auto", "auto", 2_000), &s), None);
        // Another process starts in its own.
        assert_eq!(shown(&pressed, &Session { pid: Some(8), ..s.clone() }), None);
        assert_eq!(shown(&at(7, "bypassPermissions", "auto", 2_000), &s), None, "a mode it doesn't step through");
    }

    /// Pressed again, an expectation keeps the mode recorded before the first press.
    #[test]
    fn pressed_again_it_keeps_what_the_record_said() {
        let mut s = claude("auto");
        s.key = "claude:claude:cycle-test".into();
        expect(&s, 7, "default", 2_000);
        let mut shown = s.clone();
        overlay_expected(&mut shown);
        assert_eq!(shown.mode.as_ref().map(|m| (m.id.as_str(), m.expected, m.kind)), Some(("default", true, ModeKind::Asks)));
        expect(&shown, 7, "acceptEdits", 3_000);
        let mut again = s.clone();
        overlay_expected(&mut again);
        assert_eq!(again.mode.as_ref().map(|m| m.id.as_str()), Some("acceptEdits"));
        // Its next prompt went in Accept edits: the record says, and the expectation goes.
        let mut recorded = Session { last_prompt_at: 4_000, ..claude("acceptEdits") };
        recorded.key = s.key.clone();
        overlay_expected(&mut recorded);
        assert!(recorded.mode.as_ref().is_some_and(|m| !m.expected));
        assert!(expected().as_ref().is_some_and(|all| !all.contains_key(&s.key)));
    }

    /// `overlay` without the look at the process tree.
    fn overlay_expected(s: &mut Session) {
        let mut all = expected();
        let all = all.get_or_insert_with(HashMap::new);
        match holds(all.get(&s.key), s) {
            Some(mode) => s.mode = Some(SessionMode { expected: true, ..mode }),
            None => {
                all.remove(&s.key);
            }
        }
    }

    #[test]
    fn waits_for_its_harness_to_say_it_took_the_press() {
        let mut said = ["default", "default", "accept-edits"].into_iter().map(String::from);
        let mut next = || said.next();
        assert_eq!(taken(&mut next, "default").as_deref(), Some("accept-edits"));
        let mut never = || Some("plan".to_string());
        let started = Instant::now();
        assert_eq!(taken(&mut never, "plan"), None);
        assert!(started.elapsed() >= TAKEN_WAIT);
    }
}

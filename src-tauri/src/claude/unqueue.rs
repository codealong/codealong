//! A message of yours that Claude Code holds until the step its turn is on ends, taken out so it
//! never goes (`Adapter::remove_queued`). Claude Code takes none out alone: ↑ in its box gives
//! back everything of yours it holds at once (2.1.312's `popAllEditable`, a `popAll` line in the
//! transcript for each), and a key to pick one is only behind `CLAUDE_CODE_KB_COHESION_FIXES`. Its
//! box is emptied then without Ctrl+C, which stops the turn as it clears the box while one runs
//! (`app:interrupt`, seen through a background job's attach on 2026-10-29: the running command was
//! "rejected"). Ctrl+U and Ctrl+K only edit the box, and do nothing in an empty one. What else
//! came back is typed in again, in the order it was held, as a send from Codealong types it, and
//! goes on being held. Checked on 2.1.312 through a background job's attach (2026-10-29): three
//! held, all three given back, the box empty, the command the turn ran carrying on, and the two
//! typed in again held anew. See .claude/rules/claude-code.md.

use std::path::PathBuf;

use anyhow::{bail, Result};

use super::daemon::{daemon_say, daemon_type};
use super::transcript;
use crate::harness::{Requeued, Unqueued};
use crate::launch::{Key, Submit, TerminalHost, Typed};
use crate::model::QueuedMessage;
use crate::send::quoted;

/// ↑. In an empty box Claude Code puts back what was last cleared from it, once, if anything was
/// (`restoreCleared`, which comes first: a clear's text came back on 2026-10-29), and otherwise
/// gives back what it holds into the box, each on a line of its own, the cursor after the last.
pub(super) const UP: &str = "\x1b[A";

/// Ctrl+U, a burst of them: each kills the row before the cursor as the box wraps it, or the line
/// break just before it (`deleteToLineStart`).
const KILLS_BACK: &str = match std::str::from_utf8(&[0x15; BURST]) {
    Ok(keys) => keys,
    Err(_) => panic!(),
};

/// Ctrl+K, a burst of them: each kills from the cursor to the row's end, or the line break just
/// after it (`deleteToLineEnd`). For what's after the cursor: a draft typed in the box before ↑
/// stays after what came back, the cursor where it was in it.
const KILLS_ON: &str = match std::str::from_utf8(&[0x0b; BURST]) {
    Ok(keys) => keys,
    Err(_) => panic!(),
};

/// How many keys go in one burst: they arrive together and are taken one by one, as Claude Code's
/// send-now keys are.
const BURST: usize = 32;

/// The fewest columns the box is taken to wrap at, to count its rows: a terminal is seldom
/// narrower, and a background job's box is as wide as the attach makes it (200, daemon.rs).
const NARROWEST: usize = 20;

const GONE: &str = "Claude Code doesn't hold that message any more: its turn has taken it in.";

/// A session's queue as `remove` works it.
pub(super) trait Queue {
    /// What it holds of yours now, in order, as its words were written to the queue
    /// (`transcript::holding`).
    fn held(&self) -> Vec<QueuedMessage>;
    /// What it gave back into its box since `since` (ms), in the order it held them
    /// (`transcript::given_back`).
    fn given_back(&self, since: i64) -> Vec<String>;
    /// Presses each of `keys` in its box, a moment apart.
    fn press(&mut self, keys: &[&'static str]) -> Result<()>;
    /// Types `text` into its box and presses Return, as a send does.
    fn type_in(&mut self, text: &str) -> Result<()>;
    /// The id it holds `text`, typed in since `since`, under (`transcript::queued_as`).
    fn held_as(&self, text: &str, since: i64) -> Option<String>;
}

/// Takes the message it holds as `id` out of what the session (`title`) holds: everything comes
/// back into its box, which is emptied, and the others are typed in again. Not where one of the
/// others holds a paste or an image by a placeholder (`[Pasted text #1 +40 lines]`), which typed
/// in again would be those words alone. ↑ puts back a box cleared before instead, once, so it's
/// pressed again when nothing came back. Says what happened, with the ids the others are held
/// under now.
pub(super) fn remove(q: &mut dyn Queue, title: &str, id: &str) -> Result<Unqueued> {
    let title = quoted(title);
    let held = q.held();
    let Some(at) = held.iter().position(|m| m.id == id) else { bail!(GONE) };
    if let Some(m) = held.iter().enumerate().find(|&(i, m)| i != at && by_placeholder(&m.text)).map(|(_, m)| m) {
        bail!("Claude Code gives back everything it holds at once, and {} has a paste or an image in it that Codealong can't type in again. Take it out in {title}'s terminal: ↑ there brings them all back into its box.", clipped(&m.text));
    }
    let keys = take_back_keys(&held);
    let mut back = Vec::new();
    for _ in 0..2 {
        let since = crate::util::now_ms();
        q.press(&keys)?;
        back = q.given_back(since);
        if !back.is_empty() || !q.held().iter().any(|m| m.id == id) {
            break;
        }
    }
    // Each by the id it was held under, in the order it was held.
    let mut ids = held.iter();
    let back: Vec<(Option<&QueuedMessage>, String)> = back.into_iter().map(|text| (ids.find(|m| m.text == text), text)).collect();
    let took = back.iter().any(|(m, _)| m.is_some_and(|m| m.id == id));
    let others: Vec<_> = back.iter().filter(|(m, _)| !m.is_some_and(|m| m.id == id)).collect();
    let mut requeued = Vec::new();
    for (i, (m, text)) in others.iter().enumerate() {
        let since = crate::util::now_ms();
        if let Err(e) = q.type_in(text) {
            let lost: Vec<_> = others[i..].iter().map(|(_, t)| clipped(t)).collect();
            let (them, it) = if lost.len() > 1 { ("them", "these") } else { ("it", "this") };
            let done = if took { "Took it out" } else { "It wasn't taken out" };
            bail!("{done}, but Claude Code gave back what else it held with it, and Codealong couldn't type {it} in again ({e}): {}. Send {them} again.", lost.join(", "));
        }
        if let (Some(m), Some(now)) = (m, q.held_as(text, since)) {
            requeued.push(Requeued { was: m.id.clone(), now });
        }
    }
    if !took {
        if q.held().iter().any(|m| m.id == id) {
            bail!("Claude Code didn't give back what it holds for {title}, so it's still held. Take it out in its terminal: ↑ there brings it back into its box.");
        }
        bail!(GONE);
    }
    let message = match others.len() {
        0 => format!("Took it out of what {title} holds: it won't be sent."),
        n => format!("Took it out of what {title} holds. Claude Code gives back everything it holds at once, so {} went back in after it.", if n == 1 { "the other one".to_string() } else { format!("the {n} others") }),
    };
    Ok(Unqueued { message, requeued })
}

/// ↑, then enough Ctrl+U to empty the box of everything held from its end, and as many Ctrl+K,
/// with a burst more of each for a draft that was there already.
fn take_back_keys(held: &[QueuedMessage]) -> Vec<&'static str> {
    let bursts = kills(held).div_ceil(BURST) + 1;
    std::iter::once(UP).chain(std::iter::repeat_n(KILLS_BACK, bursts)).chain(std::iter::repeat_n(KILLS_ON, bursts)).collect()
}

/// How many Ctrl+U empty the box of `held` from its end: a row each as the box wraps it at its
/// narrowest, and each line's break.
fn kills(held: &[QueuedMessage]) -> usize {
    held.iter().flat_map(|m| m.text.split('\n')).map(|line| line.chars().count() / NARROWEST + 2).sum()
}

/// Whether Claude Code holds part of it by a placeholder: a long paste, an image, a paste cut
/// short. Its words name it and the box gives back what it stands for, but typed in, the words
/// are all that go.
fn by_placeholder(text: &str) -> bool {
    ["[Pasted text #", "[Image #", "[...Truncated text #"].iter().any(|p| text.contains(p))
}

fn clipped(text: &str) -> String {
    quoted(&crate::jev::clip(text, 60))
}

/// A running session's queue: its transcript, and the keys that reach its box.
pub(super) struct Live {
    pub path: PathBuf,
    pub way: Way,
}

pub(super) enum Way {
    /// A background job, through its account's service.
    Job { socket: PathBuf, key_file: PathBuf, job: String },
    /// The terminal tab it runs in.
    Tab { host: TerminalHost, pid: u32, cwd: PathBuf },
}

impl Queue for Live {
    fn held(&self) -> Vec<QueuedMessage> {
        transcript::holding(&self.path)
    }

    fn given_back(&self, since: i64) -> Vec<String> {
        transcript::given_back(&self.path, since)
    }

    fn press(&mut self, keys: &[&'static str]) -> Result<()> {
        match &self.way {
            Way::Job { socket, key_file, job } => daemon_type(socket, key_file, job, &keys.iter().map(|k| k.as_bytes()).collect::<Vec<_>>()),
            Way::Tab { host, .. } => keys.iter().try_for_each(|k| crate::launch::press_key(host, Key::Keys(k))),
        }
    }

    fn type_in(&mut self, text: &str) -> Result<()> {
        let typed = crate::send::boxed(text);
        match &self.way {
            Way::Job { socket, key_file, job } => daemon_say(socket, key_file, job, &typed),
            Way::Tab { pid, cwd, .. } => match crate::launch::terminal_type_session(*pid, cwd, &typed, false, Submit::Return, false)? {
                Typed::Into(_) => Ok(()),
                Typed::Nowhere(why) => bail!("{why}"),
            },
        }
    }

    fn held_as(&self, text: &str, since: i64) -> Option<String> {
        transcript::queued_as(&self.path, text, since)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A queue that holds `held`, gives back what ↑ would, and keeps what's pressed and typed.
    struct Fake {
        held: Vec<QueuedMessage>,
        /// A box cleared before, which the first ↑ puts back instead.
        cleared: bool,
        /// The turn takes in what's held before ↑ comes.
        taken: bool,
        back: Vec<String>,
        pressed: usize,
        typed: Vec<String>,
        fails_at: Option<usize>,
    }

    fn held(texts: &[&str]) -> Fake {
        let held = texts.iter().enumerate().map(|(i, t)| QueuedMessage { id: format!("q{i}"), at: i as i64, text: t.to_string() }).collect();
        Fake { held, cleared: false, taken: false, back: Vec::new(), pressed: 0, typed: Vec::new(), fails_at: None }
    }

    impl Queue for Fake {
        fn held(&self) -> Vec<QueuedMessage> {
            self.held.clone()
        }
        fn given_back(&self, _: i64) -> Vec<String> {
            self.back.clone()
        }
        fn press(&mut self, keys: &[&'static str]) -> Result<()> {
            assert_eq!(keys[0], UP);
            self.pressed += 1;
            if self.taken {
                self.held.clear();
            } else if !std::mem::take(&mut self.cleared) {
                self.back = self.held.drain(..).map(|m| m.text).collect();
            }
            Ok(())
        }
        fn type_in(&mut self, text: &str) -> Result<()> {
            if self.fails_at == Some(self.typed.len()) {
                bail!("its tab is gone");
            }
            self.typed.push(text.to_string());
            let n = self.held.len();
            self.held.push(QueuedMessage { id: format!("new{n}"), at: 9, text: text.to_string() });
            Ok(())
        }
        fn held_as(&self, text: &str, _: i64) -> Option<String> {
            self.held.iter().rev().find(|m| m.text == text).map(|m| m.id.clone())
        }
    }

    /// Everything comes back, and the others go in again in the order they were held, under ids
    /// the window can follow them by; the same words held twice are told apart by their places.
    #[test]
    fn the_others_go_back_in_after_it_in_order() {
        let mut q = held(&["first", "drop this", "first", "last"]);
        let said = remove(&mut q, "Fix it", "q1").unwrap();
        assert_eq!(q.typed, ["first", "first", "last"]);
        assert_eq!(said.requeued, [("q0", "new0"), ("q2", "new1"), ("q3", "new2")].map(|(was, now)| Requeued { was: was.into(), now: now.into() }));
        assert_eq!(said.message, "Took it out of what \u{201c}Fix it\u{201d} holds. Claude Code gives back everything it holds at once, so the 3 others went back in after it.");
        let mut q = held(&["only"]);
        assert_eq!(remove(&mut q, "Fix it", "q0").unwrap(), Unqueued { message: "Took it out of what \u{201c}Fix it\u{201d} holds: it won't be sent.".into(), requeued: Vec::new() });
        assert_eq!((q.pressed, q.typed.len()), (1, 0));
    }

    /// ↑ puts back a box cleared before, once, so it's pressed again; a turn that took it in
    /// meanwhile is said, with nothing typed.
    #[test]
    fn up_is_pressed_again_after_it_put_back_a_cleared_box() {
        let mut q = Fake { cleared: true, ..held(&["drop this", "keep"]) };
        remove(&mut q, "Fix it", "q0").unwrap();
        assert_eq!((q.pressed, q.typed.clone()), (2, vec!["keep".to_string()]));
        let mut q = Fake { taken: true, ..held(&["drop this"]) };
        assert_eq!(remove(&mut q, "Fix it", "q0").unwrap_err().to_string(), GONE);
        assert_eq!((q.pressed, q.typed.len()), (1, 0));
        assert_eq!(remove(&mut held(&["other"]), "Fix it", "q9").unwrap_err().to_string(), GONE);
    }

    /// Nothing is pressed where another one holds a paste or an image Codealong couldn't type
    /// in again; the one taken out may.
    #[test]
    fn a_paste_or_an_image_among_the_others_is_left_for_the_terminal() {
        let mut q = held(&["keep [Pasted text #1 +40 lines]", "drop this"]);
        let err = remove(&mut q, "Fix it", "q1").unwrap_err().to_string();
        assert!(err.contains("\u{201c}keep [Pasted text #1 +40 lines]\u{201d} has a paste or an image"), "{err}");
        assert_eq!(q.pressed, 0);
        let mut q = held(&["keep", "drop [Image #1]"]);
        assert!(remove(&mut q, "Fix it", "q1").is_ok());
    }

    /// One that can't be typed in again is said, with what's left to send again.
    #[test]
    fn what_couldnt_go_back_in_is_said() {
        let mut q = Fake { fails_at: Some(1), ..held(&["drop this", "one", "two", "three"]) };
        let err = remove(&mut q, "Fix it", "q0").unwrap_err().to_string();
        assert_eq!(err, "Took it out, but Claude Code gave back what else it held with it, and Codealong couldn't type these in again (its tab is gone): \u{201c}two\u{201d}, \u{201c}three\u{201d}. Send them again.");
    }

    /// Enough Ctrl+U for every row and line break held, a burst more, and as many Ctrl+K.
    #[test]
    fn enough_kills_go_to_empty_the_box() {
        assert_eq!((KILLS_BACK.len(), KILLS_BACK.bytes().all(|b| b == 0x15), KILLS_ON.bytes().all(|b| b == 0x0b)), (BURST, true, true));
        let short = held(&["a", "b"]).held;
        assert_eq!(kills(&short), 4);
        assert_eq!(take_back_keys(&short), [UP, KILLS_BACK, KILLS_BACK, KILLS_ON, KILLS_ON]);
        let long = held(&[&"x".repeat(700), "two\nlines"]).held;
        assert_eq!(kills(&long), 35 + 2 + 2 + 2);
        assert_eq!(take_back_keys(&long).len(), 1 + 3 + 3);
    }
}

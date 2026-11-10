//! Messages held for a session that waits on its user to answer a prompt (send.rs
//! `waits_on_you`), sent once it doesn't. A message from another agent (`codealong session send`)
//! or from a session's side conversation never goes to such a session, since typed in it would
//! answer the prompt (send.rs `ensure_not_waiting`), and a prompt can wait for hours on a user
//! who's away: an agent that was turned away had to remember to send again, and the work it
//! meant to hand over often never reached the session (2026-11-09). So the server holds it here
//! instead, in `held-messages.json` next to the config, so it outlasts a relaunch.
//!
//! Every board the engine publishes is looked at (serve.rs). A message goes once the session it's
//! for no longer waits on its user to answer a prompt, whether they answered, denied the call or
//! closed the session, the same way it would have gone then (send.rs `send_here`, with the sender
//! it was sent with), led by a line saying when it was sent and that it was held. Several for one
//! session go in the order they were sent. One that finds the session waiting again is held on;
//! one that fails is tried again a little later, a few times. A message that can't go, or is still
//! held a day on, is dropped, and the session that sent it is told why, as Codealong's own message
//! (send.rs `Sender::Codealong`), held here too while that session waits on its user. A message
//! from no session Codealong knows, and Codealong's own word about a message, tell nobody.
//!
//! Only the server sends what's held, so only the server holds (`init`): the command run where
//! it can't reach one (an agent's sandbox) sends by itself, and is turned away as before.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::engine::Handle;
use crate::model::Board;
use crate::send::{self, quoted, Sender, Sent, WaitsOnItsUser};

/// How long a message is held before it's given up on: the prompt likely waits on a user who's
/// away, and what an agent meant to say a day ago is likely stale.
const KEEP_MS: i64 = 24 * 60 * 60 * 1000;

/// How many messages one session may have held at once.
const MOST: usize = 20;

/// How many times a send is tried before it's given up on, and how long after a failed one it's
/// tried again: the first wait, doubled each time.
const TRIES: u32 = 4;
const RETRY_MS: i64 = 30_000;

/// Who a held message is from, as send.rs sends it (`Sender`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum By {
    /// An agent, through `codealong session send` (`Sender::Agent`).
    Agent,
    /// The session's own side conversation (`Sender::Side`).
    Side,
    /// Codealong, telling the session that sent a held message why it didn't go.
    Codealong,
}

/// A message held for a session.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Held {
    pub id: String,
    /// The session it's for.
    pub to: String,
    /// What that session was called when it was held, to name it in word that it didn't go.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    pub by: By,
    /// The session that sent it, where the command found one: an agent's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub text: String,
    /// Into the turn the session is taking at once, as it was asked (`--now`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub now: bool,
    /// When it was sent (ms).
    pub at: i64,
    /// Sends that failed in a row.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub failures: u32,
    /// Not tried again before this (ms).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub next_at: i64,
}

fn is_zero<T: Default + PartialEq>(n: &T) -> bool {
    *n == T::default()
}

impl Held {
    /// The sender it goes as.
    fn sender(&self) -> Sender {
        match self.by {
            By::Agent => Sender::Agent(self.from.clone()),
            By::Side => Sender::Side,
            By::Codealong => Sender::Codealong,
        }
    }

    /// The text it goes with at `now` (ms): a line saying when it was sent and that it was held,
    /// then the message. Codealong's word about a message goes as it is, held only because its
    /// session was being sent something already.
    fn text_at(&self, now: i64) -> String {
        if self.by == By::Codealong {
            return self.text.clone();
        }
        format!("Sent {}; Codealong held it while you waited on your user to answer a prompt.\n\n{}", crate::jev::ago_words(self.at, now), self.text.trim())
    }
}

/// What holding a message came to.
#[derive(Debug, PartialEq)]
pub enum Holding {
    Held,
    /// The same message from the same sender is held for the session already.
    Already,
    /// The session has `MOST` held already.
    Full,
}

/// How sending a held message went.
#[derive(Debug, PartialEq)]
pub enum Went {
    Sent,
    /// The session waits on its user again: it's held on.
    Waits,
    Failed(String),
}

/// The messages held for sessions, and which of them a send has now.
#[derive(Debug, Default)]
pub struct Outbox {
    held: Vec<Held>,
    sending: HashSet<String>,
}

impl Outbox {
    fn new(held: Vec<Held>) -> Outbox {
        Outbox { held, sending: HashSet::new() }
    }

    /// Holds `h` for its session, after the ones held for it already.
    pub fn hold(&mut self, h: Held) -> Holding {
        let theirs = || self.held.iter().filter(|o| o.to == h.to);
        if theirs().any(|o| o.by == h.by && o.from == h.from && o.text.trim() == h.text.trim()) {
            return Holding::Already;
        }
        if theirs().count() >= MOST {
            return Holding::Full;
        }
        self.held.push(h);
        Holding::Held
    }

    /// Drops the messages held too long at `now` (ms), their senders owed word of it. Whether any
    /// were.
    pub fn expire(&mut self, now: i64) -> bool {
        let (stale, kept): (Vec<Held>, Vec<Held>) = std::mem::take(&mut self.held).into_iter().partition(|h| !self.sending.contains(&h.id) && now - h.at > KEEP_MS);
        self.held = kept;
        for h in &stale {
            self.dropped(h, "its user didn't answer the prompt it waits on within a day, so Codealong stopped holding it", now);
        }
        !stale.is_empty()
    }

    /// The messages to send now, as `board` shows the sessions at `now` (ms), in the order each
    /// session's were sent: for each session that doesn't wait on its user to answer a prompt,
    /// those held for it up to one still waiting to be tried again. One the board doesn't have is
    /// sent too, and the send says why it can't go. None while the board is still being read,
    /// which lists nothing yet.
    pub fn due(&mut self, board: &Board, now: i64) -> Vec<Held> {
        if board.loading {
            return Vec::new();
        }
        let mut blocked: HashSet<&str> = HashSet::new();
        let mut out = Vec::new();
        for h in &self.held {
            if blocked.contains(h.to.as_str()) {
                continue;
            }
            let waits = board.session(&h.to).is_some_and(|(_, s)| send::waits_on_you(s));
            // One being sent, or waiting to be tried again, keeps the later ones back.
            if waits || self.sending.contains(&h.id) || now < h.next_at {
                blocked.insert(&h.to);
                continue;
            }
            out.push(h.clone());
        }
        self.sending.extend(out.iter().map(|h| h.id.clone()));
        out
    }

    /// How sending the held message `id` went, at `now` (ms): sent, it's held no longer; turned
    /// away because the session waits on its user again, it's held on as it was; failed, it's
    /// tried again later, or dropped after `TRIES`, its sender owed word of it.
    pub fn went(&mut self, id: &str, went: Went, now: i64) {
        self.sending.remove(id);
        let Some(i) = self.held.iter().position(|h| h.id == id) else { return };
        match went {
            Went::Sent => {
                self.held.remove(i);
            }
            Went::Waits => {}
            Went::Failed(why) => {
                let h = &mut self.held[i];
                h.failures += 1;
                if h.failures < TRIES {
                    h.next_at = now + (RETRY_MS << (h.failures - 1).min(10));
                } else {
                    let h = self.held.remove(i);
                    self.dropped(&h, &why, now);
                }
            }
        }
    }

    /// Owes the session that sent `h` word that it was dropped, and why, as a message from
    /// Codealong held here too: it goes on the next board where that session doesn't wait on its
    /// user. A message from no session, or Codealong's own word, tells nobody.
    fn dropped(&mut self, h: &Held, why: &str, now: i64) {
        eprintln!("Codealong dropped a message held for {} since {}: {why}", h.to, h.at);
        let (By::Agent, Some(from)) = (&h.by, &h.from) else { return };
        let said = crate::jev::clip(&h.text, 300);
        let to = if h.title.trim().is_empty() { "the session".to_string() } else { quoted(&h.title) };
        let text = format!("Your message to {to} ({}), sent {} while it waited on its user to answer a prompt, wasn't sent: {}. It began: “{said}”", h.to, crate::jev::ago_words(h.at, now), why.trim().trim_end_matches('.'));
        let notice = Held { id: next_id(now), to: from.clone(), title: String::new(), by: By::Codealong, from: None, text, now: false, at: now, failures: 0, next_at: 0 };
        self.hold(notice);
    }
}

/// A new held message's id: when it was sent, and a count, so two sent in the same millisecond
/// differ.
fn next_id(at: i64) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!("{at}-{}", N.fetch_add(1, Ordering::Relaxed))
}

/// Set once the server holds and sends messages (`init`).
static RUNS: AtomicBool = AtomicBool::new(false);

fn path() -> PathBuf {
    crate::config::support_dir().join("held-messages.json")
}

fn outbox() -> &'static Mutex<Outbox> {
    static OUTBOX: OnceLock<Mutex<Outbox>> = OnceLock::new();
    OUTBOX.get_or_init(|| Mutex::new(Outbox::new(crate::store::load_whole(&path(), "held messages"))))
}

/// Saves what's held, which a relaunch reads again (`init`).
fn save(out: &Outbox) {
    if let Err(e) = crate::store::save_whole(&path(), &out.held) {
        eprintln!("Couldn't save the held messages: {e:#}");
    }
}

/// Has this process hold messages for sessions that wait on their user, and send them as boards
/// come (`send_due`): the server, which runs for as long as the sessions do. What a server before
/// it held is read now, to go as the first boards allow.
pub fn init() {
    let n = outbox().lock().map(|o| o.held.len()).unwrap_or_default();
    if n > 0 {
        eprintln!("Holding {n} message(s) for sessions that waited on their user, from before this server started.");
    }
    RUNS.store(true, Ordering::SeqCst);
}

/// Sends `text` to the session `key` as send.rs `send_here` does, and where it waits on its user
/// to answer a prompt, holds a message from an agent or a side conversation to send once it
/// doesn't (`Outbox`), in a server that sends it then. The answer says it's held.
pub fn send_or_hold(engine: &Handle, key: &str, text: &str, sender: Sender, now: bool) -> Result<Sent> {
    let (by, from) = match &sender {
        Sender::Agent(from) => (Some(By::Agent), from.clone()),
        Sender::Side => (Some(By::Side), None),
        _ => (None, None),
    };
    let sent = send::send_here(engine, key, text, sender, now);
    let (Some(by), true, Err(e)) = (by, RUNS.load(Ordering::SeqCst), &sent) else { return sent };
    let Some(title) = e.downcast_ref::<WaitsOnItsUser>().map(|w| w.title.clone()) else { return sent };
    let at = crate::util::now_ms();
    let told = from.is_some();
    let h = Held { id: next_id(at), to: key.to_string(), title: title.clone(), by, from, text: text.to_string(), now, at, failures: 0, next_at: 0 };
    let mut out = outbox().lock().unwrap_or_else(|e| e.into_inner());
    let holding = out.hold(h);
    if holding == Holding::Held {
        eprintln!("Holding a message for {key} until its user answers the prompt it waits on.");
        save(&out);
    }
    Ok(send::sent(held_words(&title, holding, told)?))
}

/// What the sender is told of a message held, or not held, for the session `title`; `told` says
/// whether a session sent it, which is told if it's dropped.
fn held_words(title: &str, holding: Holding, told: bool) -> Result<String> {
    let title = quoted(title);
    Ok(match holding {
        Holding::Held => {
            let tell = if told { " If it can't be sent by then, this session is told why." } else { "" };
            format!("{title} is waiting on its user to answer a prompt, so Codealong holds your message and sends it once they have, for up to a day.{tell} Don't send it again.")
        }
        Holding::Already => format!("Codealong already holds that message for {title} until its user answers the prompt it waits on, so it wasn't held twice."),
        Holding::Full => anyhow::bail!("{title} is waiting on its user to answer a prompt, and Codealong already holds {MOST} messages for it, so nothing was sent. Try again once they have."),
    })
}

/// Sends what's held for sessions `board` shows no longer waiting on their user, in the server
/// (serve.rs, for each board the engine publishes): one thread for the batch, in the order they
/// were sent, since two sends typing into one terminal at once would interleave.
pub fn send_due(engine: &Handle, board: &Board) {
    if !RUNS.load(Ordering::SeqCst) {
        return;
    }
    let due = match outbox().lock() {
        Ok(mut out) if !out.held.is_empty() => {
            if out.expire(board.generated_at) {
                save(&out);
            }
            out.due(board, board.generated_at)
        }
        _ => return,
    };
    if due.is_empty() {
        return;
    }
    let engine = engine.clone();
    std::thread::spawn(move || send_now(&engine, due));
}

/// Sends each held message as it would have gone, and notes how it went. Once one for a session
/// doesn't go, the ones after it are held on as they were, so none goes ahead of it.
fn send_now(engine: &Handle, due: Vec<Held>) {
    let mut stopped: HashSet<String> = HashSet::new();
    for h in due {
        if stopped.contains(&h.to) {
            outbox().lock().unwrap_or_else(|e| e.into_inner()).went(&h.id, Went::Waits, crate::util::now_ms());
            continue;
        }
        let now = crate::util::now_ms();
        let text = h.text_at(now);
        // One that falls over counts as failed, so it isn't left as being sent for good.
        let went = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| send::send_here(engine, &h.to, &text, h.sender(), h.now)));
        let went = match went {
            Ok(Ok(_)) => Went::Sent,
            Ok(Err(e)) if e.is::<WaitsOnItsUser>() => Went::Waits,
            Ok(Err(e)) => Went::Failed(format!("{e:#}")),
            Err(_) => Went::Failed("sending it stopped partway".into()),
        };
        match &went {
            Went::Sent => eprintln!("Sent {} the message held for it since {}.", h.to, h.at),
            Went::Failed(why) => eprintln!("Couldn't send {} the message held for it: {why}", h.to),
            Went::Waits => {}
        }
        if went != Went::Sent {
            stopped.insert(h.to.clone());
        }
        let mut out = outbox().lock().unwrap_or_else(|e| e.into_inner());
        out.went(&h.id, went, crate::util::now_ms());
        save(&out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Session, Status};

    const NOW: i64 = 1_800_000_000_000;
    const TO: &str = "claude:claude:to";
    const FROM: &str = "claude:claude2:from";

    fn session(key: &str, status: Status) -> Session {
        Session { status, live: true, ..crate::fixtures::session(key) }
    }

    fn board(sessions: Vec<Session>) -> Board {
        Board { lanes: vec![crate::fixtures::lane("/r/app", sessions)], ..Board::default() }
    }

    fn held(text: &str, at: i64) -> Held {
        Held { id: format!("{text}@{at}"), to: TO.into(), title: "Fix the build".into(), by: By::Agent, from: Some(FROM.into()), text: text.into(), now: false, at, failures: 0, next_at: 0 }
    }

    fn ids(due: &[Held]) -> Vec<&str> {
        due.iter().map(|h| h.id.as_str()).collect()
    }

    #[test]
    fn a_message_is_held_while_its_session_waits_on_its_user_and_sent_in_order_once_it_doesnt() {
        let mut out = Outbox::default();
        assert_eq!(out.hold(held("first", NOW)), Holding::Held);
        assert_eq!(out.hold(held("second", NOW + 1)), Holding::Held);
        let waiting = board(vec![session(TO, Status::Needs), session(FROM, Status::Working)]);
        assert!(out.due(&waiting, NOW + 60_000).is_empty());
        // One whose reply only asks something takes a message like any prompt.
        let asks = board(vec![Session { asks: true, ..session(TO, Status::Needs) }]);
        assert_eq!(ids(&out.due(&asks, NOW + 60_000)), ["first@1800000000000", "second@1800000000001"]);
        // Being sent, they aren't taken again.
        assert!(out.due(&asks, NOW + 61_000).is_empty());
        out.went("first@1800000000000", Went::Sent, NOW + 62_000);
        out.went("second@1800000000001", Went::Sent, NOW + 62_000);
        assert!(out.held.is_empty());
    }

    #[test]
    fn nothing_is_sent_while_the_board_is_still_being_read() {
        let mut out = Outbox::default();
        out.hold(held("hi", NOW));
        let loading = Board { loading: true, ..board(vec![]) };
        assert!(out.due(&loading, NOW + 1).is_empty());
        // Once read, a session the board doesn't have is sent to anyway, and the send says why.
        assert_eq!(out.due(&board(vec![]), NOW + 1).len(), 1);
    }

    #[test]
    fn the_same_message_is_held_once_and_a_session_holds_only_so_many() {
        let mut out = Outbox::default();
        assert_eq!(out.hold(held("same", NOW)), Holding::Held);
        assert_eq!(out.hold(Held { text: " same ".into(), ..held("same", NOW + 5) }), Holding::Already);
        // Another sender's, or another session's, isn't the same.
        assert_eq!(out.hold(Held { from: Some("claude:claude:other".into()), ..held("same", NOW + 6) }), Holding::Held);
        assert_eq!(out.hold(Held { to: "claude:claude:else".into(), ..held("same", NOW + 7) }), Holding::Held);
        for i in 0..MOST - 2 {
            assert_eq!(out.hold(held(&format!("m{i}"), NOW + 10 + i as i64)), Holding::Held);
        }
        assert_eq!(out.hold(held("one too many", NOW + 100)), Holding::Full);
    }

    #[test]
    fn a_session_waiting_again_holds_it_on_and_a_failed_send_is_tried_again_then_dropped_and_told() {
        let mut out = Outbox::default();
        out.hold(held("first", NOW));
        out.hold(held("second", NOW + 1));
        let free = board(vec![session(TO, Status::Working), session(FROM, Status::Working)]);
        let due = out.due(&free, NOW + 10_000);
        assert_eq!(due.len(), 2);
        // It waited again by the time the first went: both are held as they were.
        out.went(&due[0].id, Went::Waits, NOW + 11_000);
        out.went(&due[1].id, Went::Waits, NOW + 11_000);
        assert_eq!(out.held.iter().map(|h| h.failures).collect::<Vec<_>>(), [0, 0]);
        // A failure holds back the ones after it until it's tried again.
        let due = out.due(&free, NOW + 12_000);
        out.went(&due[0].id, Went::Failed("the terminal didn't answer".into()), NOW + 13_000);
        out.went(&due[1].id, Went::Waits, NOW + 13_000);
        assert!(out.due(&free, NOW + 13_000 + RETRY_MS - 1).is_empty());
        let mut at = NOW + 13_000 + RETRY_MS;
        for tried in 2..=TRIES {
            let due = out.due(&free, at);
            assert_eq!(ids(&due), ["first@1800000000000", "second@1800000000001"], "try {tried}");
            // The batch holds back the one after a failure (`send_now`).
            out.went(&due[0].id, Went::Failed("the terminal didn't answer.".into()), at);
            out.went(&due[1].id, Went::Waits, at);
            at += RETRY_MS << (tried - 1);
        }
        // Dropped after its last try, its sender owed word of it, and the next goes.
        assert!(out.held.iter().all(|h| h.id != "first@1800000000000"));
        let due = out.due(&free, at);
        assert_eq!(due.iter().map(|h| (h.to.as_str(), &h.by)).collect::<Vec<_>>(), [(TO, &By::Agent), (FROM, &By::Codealong)]);
        let notice = &due[1].text;
        assert!(notice.starts_with("Your message to “Fix the build” (claude:claude:to), sent "), "{notice}");
        // It goes as it is, without the line a held agent's message gets.
        assert_eq!(&due[1].text_at(at), notice);
        assert!(notice.ends_with("while it waited on its user to answer a prompt, wasn't sent: the terminal didn't answer. It began: “first”"), "{notice}");
    }

    #[test]
    fn a_message_held_a_day_is_dropped_and_its_sender_told_unless_it_has_none() {
        let mut out = Outbox::default();
        out.hold(held("old news", NOW));
        out.hold(Held { id: "nobody".into(), from: None, ..held("from no session", NOW) });
        out.hold(Held { id: "side".into(), by: By::Side, from: None, ..held("from its side", NOW) });
        let waiting = board(vec![session(TO, Status::Needs), session(FROM, Status::Idle)]);
        assert!(!out.expire(NOW + KEEP_MS));
        assert!(out.due(&waiting, NOW + KEEP_MS).is_empty());
        // Dropped, and one notice in their place: what's held changed though its count didn't.
        assert!(out.expire(NOW + KEEP_MS + 1));
        let due = out.due(&waiting, NOW + KEEP_MS + 1);
        assert_eq!(due.len(), 1);
        assert_eq!((due[0].to.as_str(), &due[0].by), (FROM, &By::Codealong));
        assert!(due[0].text.contains("wasn't sent: its user didn't answer the prompt it waits on within a day, so Codealong stopped holding it. It began: “old news”"), "{}", due[0].text);
        // Codealong's own word that doesn't go tells nobody in turn.
        for _ in 0..TRIES {
            out.went(&due[0].id, Went::Failed("gone".into()), NOW);
            out.held.iter_mut().for_each(|h| h.next_at = 0);
        }
        assert!(out.held.is_empty());
    }

    #[test]
    fn a_held_message_goes_as_it_was_sent_and_says_it_was_held() {
        let h = held("Ship it", NOW);
        assert_eq!(h.sender(), Sender::Agent(Some(FROM.into())));
        assert_eq!(Held { by: By::Side, from: None, ..h.clone() }.sender(), Sender::Side);
        assert_eq!(h.text_at(NOW + 2 * 60 * 60 * 1000), "Sent 2 hours ago; Codealong held it while you waited on your user to answer a prompt.\n\nShip it");
    }

    #[test]
    fn the_sender_is_told_its_message_is_held() {
        let said = held_words("Fix the build", Holding::Held, true).unwrap();
        assert_eq!(said, "“Fix the build” is waiting on its user to answer a prompt, so Codealong holds your message and sends it once they have, for up to a day. If it can't be sent by then, this session is told why. Don't send it again.");
        assert!(!held_words("Fix the build", Holding::Held, false).unwrap().contains("this session is told"));
        assert!(held_words("Fix the build", Holding::Already, true).unwrap().contains("wasn't held twice"));
        assert!(held_words("Fix the build", Holding::Full, true).unwrap_err().to_string().contains("nothing was sent"));
    }

    #[test]
    fn what_is_held_reads_back_as_it_was_written() {
        let h = Held { now: true, failures: 2, next_at: NOW + 5, ..held("Ship it", NOW) };
        let back: Vec<Held> = serde_json::from_str(&serde_json::to_string(&vec![h.clone(), held("plain", NOW)]).unwrap()).unwrap();
        assert_eq!(back, vec![h, held("plain", NOW)]);
        assert_eq!(serde_json::to_value(held("plain", NOW)).unwrap(), serde_json::json!({ "id": "plain@1800000000000", "to": TO, "title": "Fix the build", "by": "agent", "from": FROM, "text": "plain", "at": NOW }));
    }
}

//! A device that only watches: it links with devices by their link codes, keeps each one's feed,
//! and asks them things (a page of a conversation, what a session waits on you for, and, where
//! remote control is on, to send a prompt, answer an approval or ask on the side), but has no
//! board to share and answers nothing. The phone app (codealong-ios) runs one, and the engine's tests run one against
//! its peers.rs.
//!
//! It keeps what it knows in a folder of its own, in the shapes the Mac app uses for the same
//! things: `devices.json` for the devices it's linked with and `device-feeds.json` for what each
//! last said, so it shows the last feeds at once, before it has connected.
//!
//! Threads and channels, like the engine: `start` binds an iroh endpoint and keeps a connection to
//! each linked device, a thread reading and one writing for each; `stop` closes them all, as the
//! phone does when it goes to the background.

use std::collections::{HashMap, HashSet};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use iroh::{PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::{self, params};
use crate::feed::Replica;
use crate::net::{Iroh, Ticket};
use crate::proto::{self, Msg, VERSION};
use crate::wire::Wire;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
/// A connection with nothing coming in for this long is gone; the other device pings well within it.
const IDLE_TIMEOUT: Duration = Duration::from_secs(80);
const PING_EVERY: Duration = Duration::from_secs(25);
/// A linked device that didn't answer is dialed again after this, then twice as long each time
/// up to `DIAL_MAX`: a phone is open for minutes, so it can't wait as long as a Mac does.
const DIAL_FIRST: Duration = Duration::from_secs(2);
const DIAL_MAX: Duration = Duration::from_secs(60);
const LINK_WAIT: Duration = Duration::from_secs(20);
const SAVE_EVERY: Duration = Duration::from_secs(30);
/// How long a read may take, and a request that acts on a session, which can type into a
/// terminal or wait on the Codex app.
const READ_WAIT: Duration = Duration::from_secs(15);
/// Jev answers in a second or two, but the Mac may be busy.
const ROUTE_WAIT: Duration = Duration::from_secs(20);
const ACT_WAIT: Duration = Duration::from_secs(40);
/// How long being done with a session is given: removing a worktree its harness made can take
/// most of a minute there, as the engine's actions.rs waits for it.
const DONE_WAIT: Duration = Duration::from_secs(75);
/// How long a PR's detail, review or merge is given: each is GitHub requests on the Mac, one after
/// another (a merge and then its branch's deletion, a review after looking for your draft), each
/// allowed a minute there.
const PR_WAIT: Duration = Duration::from_secs(150);
/// How long an image is given to go one way or the other: a few megabytes, over whatever
/// connection the phone has.
const IMAGE_WAIT: Duration = Duration::from_secs(60);

/// A device this one is linked with, as `devices.json` keeps it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Linked {
    /// The device key, as hex.
    pub key: String,
    pub name: String,
    pub linked_at: i64,
}

#[derive(Serialize, Deserialize, Default)]
struct LinkedFile {
    linked: Vec<Linked>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct Cached {
    last_seen: i64,
    replica: Replica,
}

/// A linked device, as an app shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub key: String,
    /// The first five bytes of its key, as hex: what its keys carry here (`feed::remote_key`).
    pub short: String,
    pub name: String,
    pub online: bool,
    /// When the connection opened.
    pub since: Option<i64>,
    /// When it last said anything.
    pub last_seen: Option<i64>,
    /// It takes prompts, answers and starts from the devices it's linked with.
    pub remote_control: bool,
    pub app: String,
    /// Its first scan hasn't finished.
    pub loading: bool,
    /// It has this device on its list too; false only when it forgot this one.
    pub linked_back: bool,
}

struct Conn {
    serial: u64,
    /// What the other device knows beyond version 1, from its hello.
    knows: Vec<String>,
    tx: Sender<Msg>,
    /// Ends the connection: its reader stops, so nothing more is taken from it.
    close: Arc<dyn Fn() + Send + Sync>,
    name: String,
    app: String,
    linked_there: bool,
    since: i64,
}

#[derive(Default)]
struct State {
    running: bool,
    linked: Vec<Linked>,
    replicas: HashMap<String, Cached>,
    conns: HashMap<String, Conn>,
    /// Requests waiting for an answer, by id: the device asked, and where its answer goes.
    pending: HashMap<u64, (String, Sender<std::result::Result<Value, String>>)>,
    /// Link codes in use: the device's key, and where its answer goes.
    linking: HashMap<String, Sender<std::result::Result<String, String>>>,
    /// Replies being written in sessions this device watches, by device key, then session key.
    streams: HashMap<String, HashMap<String, api::Stream>>,
    /// Side conversations of the sessions this device asked about, as each device last said, by
    /// device key.
    sides: HashMap<String, Sides>,
    cache_dirty: bool,
}

/// One device's side conversations, as it last said (`Msg::Side`) or was read: each session's
/// open one, and the ones closed while their copies answer. What it says comes from more than one
/// thread there (a copy's, a close), so what says less than it said before came late, and is
/// dropped, as the Mac's windows drop it (src/side.ts).
#[derive(Default)]
struct Sides {
    /// By session key.
    open: HashMap<String, api::Side>,
    /// By id.
    closing: HashMap<String, api::Side>,
    /// How many questions each one had as it closed, and each closed one as it stopped answering,
    /// by id: one is open again only once it's asked again, and answers again never.
    closed_at: HashMap<String, usize>,
    stopped_at: HashMap<String, usize>,
    /// How often each session's were told of, so a read that answers after one was is dropped:
    /// what was told is newer.
    told: HashMap<String, u64>,
}

impl Sides {
    fn take(&mut self, side: api::Side) {
        let turns = side.turns.len();
        if side.closed {
            if self.stopped_at.get(&side.id).is_some_and(|n| turns <= *n) {
                return;
            }
            // Open again with a question more since.
            let open = self.open.get(&side.key).filter(|o| o.id == side.id);
            if open.is_some_and(|o| o.turns.len() > turns) {
                return;
            }
            if open.is_some() {
                self.open.remove(&side.key);
            }
            let at = self.closed_at.entry(side.id.clone()).or_default();
            *at = (*at).max(turns);
            if !side.running {
                self.stopped_at.insert(side.id.clone(), turns);
            }
            self.closing.insert(side.id.clone(), side);
        } else if side.turns.is_empty() {
            self.open.remove(&side.key);
        } else if !self.closed_at.get(&side.id).is_some_and(|n| turns <= *n) {
            // Asked again as it was read, one closed is the open one.
            self.closing.remove(&side.id);
            self.open.insert(side.key.clone(), side);
        }
    }

    fn told(&self, key: &str) -> u64 {
        self.told.get(key).copied().unwrap_or(0)
    }
}

struct Inner {
    secret: SecretKey,
    key: PublicKey,
    hex: String,
    name: String,
    app: String,
    /// `proto::PHONE` or `proto::TABLET`: Macs don't dial a device that only watches, or ask it to search.
    kind: String,
    dir: PathBuf,
    on_change: Box<dyn Fn() + Send + Sync>,
    st: Mutex<State>,
    /// Goes up with every stop, so the threads of an earlier start end.
    gen: AtomicU64,
    ids: AtomicU64,
    iroh: Mutex<Option<Arc<Iroh>>>,
    /// Held while an image goes to a Mac (`keep_image`): one at a time, since one whose answer
    /// doesn't come in time stays queued on its connection, and they'd pile up.
    sending_image: std::sync::Mutex<()>,
}

#[derive(Clone)]
pub struct Client(Arc<Inner>);

impl Client {
    /// A client with this device's key, its name, app version and kind (`proto::PHONE` or
    /// `proto::TABLET`) as the other devices see them, keeping its files in `dir`. `on_change` is
    /// called, on some thread, whenever what `devices` and `with_feeds` give may have changed.
    pub fn new(dir: PathBuf, secret: SecretKey, name: String, app: String, kind: &str, on_change: Box<dyn Fn() + Send + Sync>) -> Client {
        let key = secret.public();
        let st = State { linked: load_linked(&dir), replicas: load_cache(&dir), ..Default::default() };
        Client(Arc::new(Inner { hex: proto::key_hex(&key), key, secret, name, app, kind: kind.to_string(), dir, on_change, st: Mutex::new(st), gen: AtomicU64::new(0), ids: AtomicU64::new(1), iroh: Mutex::new(None), sending_image: std::sync::Mutex::new(()) }))
    }

    /// This device's key, as hex.
    pub fn key(&self) -> String {
        self.0.hex.clone()
    }

    /// Binds this device's iroh endpoint and keeps a connection to each linked device, until `stop`.
    pub fn start(&self) -> Result<()> {
        let inner = &self.0;
        {
            let mut st = inner.lock();
            if st.running {
                return Ok(());
            }
            st.running = true;
        }
        let gen = inner.gen.load(Ordering::SeqCst);
        let iroh = match Iroh::start(inner.secret.clone()) {
            Ok(i) => Arc::new(i),
            Err(e) => {
                inner.lock().running = false;
                return Err(e);
            }
        };
        *inner.iroh.lock().unwrap_or_else(|e| e.into_inner()) = Some(iroh.clone());
        let i = inner.clone();
        std::thread::Builder::new().name("client-dialer".into()).spawn(move || dial_loop(i, iroh, gen))?;
        Ok(())
    }

    /// Closes every connection and the endpoint, and saves what it knows.
    pub fn stop(&self) {
        let inner = &self.0;
        inner.gen.fetch_add(1, Ordering::SeqCst);
        let closes: Vec<Arc<dyn Fn() + Send + Sync>> = {
            let mut st = inner.lock();
            st.running = false;
            let now = now_ms();
            let keys: Vec<String> = st.conns.keys().cloned().collect();
            for k in &keys {
                if let Some(c) = st.replicas.get_mut(k) {
                    c.last_seen = now;
                }
            }
            st.cache_dirty = true;
            st.conns.drain().map(|(_, c)| c.close).collect()
        };
        for close in closes {
            close();
        }
        if let Some(iroh) = inner.iroh.lock().unwrap_or_else(|e| e.into_inner()).take() {
            iroh.close();
        }
        inner.save_cache();
        inner.changed();
    }

    /// Links with the device whose link code this is: dials it through iroh and presents the
    /// code's secret. Returns the device's name.
    pub fn link(&self, code: &str) -> std::result::Result<String, String> {
        let t = Ticket::decode(code).map_err(|e| e.to_string())?;
        if let Some(name) = self.already_linked(&t)? {
            return Ok(name);
        }
        let iroh = self.0.iroh.lock().unwrap_or_else(|e| e.into_inner()).clone().ok_or("Codealong isn't connected yet; try again in a moment.")?;
        let wire = iroh.connect(t.key).map_err(|e| format!("Couldn't reach that device: {e:#}"))?;
        self.link_over(t, wire)
    }

    /// Links over a connection made some other way: a loopback one in a test, say.
    pub fn link_wire(&self, code: &str, wire: Wire) -> std::result::Result<String, String> {
        let t = Ticket::decode(code).map_err(|e| e.to_string())?;
        if let Some(name) = self.already_linked(&t)? {
            return Ok(name);
        }
        self.link_over(t, wire)
    }

    /// The device's name when a code for it would change nothing: it's linked here, and a live
    /// connection says it has this device too. Otherwise the code goes to it, since it may have
    /// forgotten this device (its list was edited, or this device's key changed).
    fn already_linked(&self, t: &Ticket) -> std::result::Result<Option<String>, String> {
        if t.key == self.0.key {
            return Err("That's this device's own key.".into());
        }
        let hex = proto::key_hex(&t.key);
        let st = self.0.lock();
        let both = st.linked.iter().any(|l| l.key == hex) && st.conns.get(&hex).is_some_and(|c| c.linked_there);
        Ok(both.then(|| self.0.name_of(&st, &hex)))
    }

    fn link_over(&self, t: Ticket, wire: Wire) -> std::result::Result<String, String> {
        let inner = &self.0;
        let hex = proto::key_hex(&t.key);
        let (tx, rx) = channel();
        {
            let mut st = inner.lock();
            st.running = true;
            st.linking.insert(hex.clone(), tx);
        }
        let (i, gen) = (inner.clone(), inner.gen.load(Ordering::SeqCst));
        std::thread::spawn(move || run(i, wire, t.key, Some(t.secret), gen));
        let got = rx.recv_timeout(LINK_WAIT).unwrap_or_else(|_| Err("The other device didn't answer in time.".into()));
        inner.lock().linking.remove(&hex);
        got
    }

    /// Runs a connection to a linked device made some other way: a TCP stream on the tailnet, or
    /// a loopback one in a test.
    pub fn connect_wire(&self, wire: Wire, key: PublicKey) {
        let inner = self.0.clone();
        inner.lock().running = true;
        let gen = inner.gen.load(Ordering::SeqCst);
        std::thread::spawn(move || run(inner, wire, key, None, gen));
    }

    /// Unlinks a device, here and, when it's connected, there.
    pub fn unlink(&self, key: &str) -> std::result::Result<(), String> {
        let inner = &self.0;
        let mut st = inner.lock();
        if !st.linked.iter().any(|l| l.key == key) {
            return Err("That device isn't linked.".into());
        }
        if let Some(c) = st.conns.get(key) {
            let _ = c.tx.send(Msg::Unlinked);
            let _ = c.tx.send(Msg::Standing { linked: false });
        }
        inner.forget(&mut st, key);
        drop(st);
        inner.changed();
        Ok(())
    }

    /// Asks a linked device, by its key or short id, to run a request, and waits for its answer.
    /// One that acts and isn't answered in time may still be done there, and says so, so that
    /// it isn't asked for twice.
    pub fn ask(&self, device: &str, method: &str, params: Value, timeout: Duration) -> std::result::Result<Value, String> {
        let acts = api::reach(method) == Some(api::Reach::Control);
        let inner = &self.0;
        let (id, rx, name) = {
            let mut st = inner.lock();
            let Some(key) = st.linked.iter().find(|l| l.key == device || short_of(&l.key) == device).map(|l| l.key.clone()) else {
                return Err("That device isn't linked with this one any more.".into());
            };
            let name = inner.name_of(&st, &key);
            let Some(tx) = st.conns.get(&key).map(|c| c.tx.clone()) else {
                return Err(format!("{name} isn't connected."));
            };
            let id = inner.ids.fetch_add(1, Ordering::SeqCst);
            let (s, r) = channel();
            st.pending.insert(id, (key, s));
            let _ = tx.send(Msg::Req { id, method: method.to_string(), params });
            (id, r, name)
        };
        match rx.recv_timeout(timeout) {
            Ok(r) => r,
            Err(_) => {
                inner.lock().pending.remove(&id);
                Err(if acts { format!("{name} didn't answer in time, so it may still have done it: look before you ask again.") } else { format!("{name} didn't answer in time.") })
            }
        }
    }

    /// A page of a session's conversation on `device`, back from `before` (from the newest when
    /// none). `key` is the session's key there.
    pub fn page(&self, device: &str, key: &str, before: Option<u64>) -> std::result::Result<api::Page, String> {
        read(self.ask(device, api::SESSION_PAGE, of(params::Page { key: key.into(), before }), READ_WAIT)?)
    }

    /// Sessions on `device` whose prompts and replies have every word. Keys are the device's own.
    pub fn search(&self, device: &str, words: &[String]) -> std::result::Result<api::Found, String> {
        read(self.ask(device, api::SEARCH, of(params::Search { words: words.to_vec() }), READ_WAIT)?)
    }

    /// Where `text` goes on `device`'s board, as its quick panel would suggest (`api::Route`), in
    /// `project` (its name there; empty for all). Session keys and lane ids are the device's own.
    pub fn route(&self, device: &str, project: &str, text: &str) -> std::result::Result<api::Route, String> {
        read(self.ask(device, api::ROUTE, of(params::Route { project: project.into(), text: text.into() }), ROUTE_WAIT)?)
    }

    /// What a session on `device` said and did after `after`.
    pub fn newer(&self, device: &str, key: &str, after: u64) -> std::result::Result<api::Page, String> {
        read(self.ask(device, api::SESSION_NEWER, of(params::Newer { key: key.into(), after }), READ_WAIT)?)
    }

    /// What a session on `device` waits on you to allow, when it's a tool call it can answer.
    pub fn pending(&self, device: &str, key: &str) -> std::result::Result<Option<api::Pending>, String> {
        read(self.ask(device, api::SESSION_PENDING, of(params::Session { key: key.into() }), READ_WAIT)?)
    }

    /// Answers what a session on `device` waits on, if it's still call `answer.id`: allows or
    /// denies it, answers the questions it asks (`answers`), or denies it saying what to do
    /// instead (`message`). `key` is the session's key there. Says how it went.
    pub fn answer(&self, device: &str, key: &str, answer: params::Answer) -> std::result::Result<String, String> {
        read(self.ask(device, api::SESSION_ANSWER, of(params::Answer { key: key.into(), ..answer }), ACT_WAIT)?)
    }

    /// Sends a session on `device` a prompt, where it runs there; with `now`, into the turn it's
    /// taking at once (one whose feed says `steerable`).
    pub fn send(&self, device: &str, key: &str, text: &str, now: bool) -> std::result::Result<api::Sent, String> {
        read(self.ask(device, api::SESSION_SEND, of(params::Send { key: key.into(), text: text.into(), now }), ACT_WAIT)?)
    }

    /// How a message sent to a session on `device` would reach it, or None for one Codealong runs
    /// there.
    pub fn way(&self, device: &str, key: &str) -> std::result::Result<Option<api::Way>, String> {
        read(self.ask(device, api::SESSION_WAY, of(params::Session { key: key.into() }), READ_WAIT)?)
    }

    /// Goes on with a session on `device` that isn't running, where `resume` says, on `prompt`
    /// when it isn't empty. Says how it went, as a send does.
    pub fn resume(&self, device: &str, key: &str, resume: api::ResumeIn, prompt: &str) -> std::result::Result<api::Sent, String> {
        read(self.ask(device, api::SESSION_RESUME, of(params::Resume { key: key.into(), resume, prompt: prompt.into() }), ACT_WAIT)?)
    }

    /// Switches a running session on `device` to another of its account's models, for it alone.
    /// Says what happened.
    pub fn switch_model(&self, device: &str, key: &str, model: &str) -> std::result::Result<String, String> {
        read(self.ask(device, api::SESSION_MODEL, of(params::Model { key: key.into(), model: model.into() }), ACT_WAIT)?)
    }

    /// Switches a running session on `device` to another of its account's effort levels, for it
    /// alone. Says what happened.
    pub fn switch_effort(&self, device: &str, key: &str, effort: &str) -> std::result::Result<String, String> {
        read(self.ask(device, api::SESSION_EFFORT, of(params::Effort { key: key.into(), effort: effort.into() }), ACT_WAIT)?)
    }

    /// Switches a session `device` runs itself to another of the modes its agent offers, by its
    /// id, for it alone. Says what happened, and the mode it's in now.
    pub fn switch_mode(&self, device: &str, key: &str, mode: &str) -> std::result::Result<api::ModeSwitched, String> {
        read(self.ask(device, api::SESSION_MODE, of(params::Mode { key: key.into(), mode: mode.into() }), ACT_WAIT)?)
    }

    /// Names a session on `device` where its harness keeps its name.
    pub fn rename(&self, device: &str, key: &str, name: &str) -> std::result::Result<(), String> {
        self.ask(device, api::SESSION_RENAME, of(params::Rename { key: key.into(), name: name.into() }), ACT_WAIT).map(|_| ())
    }

    pub fn archive(&self, device: &str, key: &str, archived: bool) -> std::result::Result<(), String> {
        self.ask(device, api::SESSION_ARCHIVE, of(params::Archive { key: key.into(), archived }), ACT_WAIT).map(|_| ())
    }

    /// Marks sessions on `device` seen, so Your turn clears there and everywhere else.
    pub fn mark_seen(&self, device: &str, keys: &[String]) -> std::result::Result<(), String> {
        self.ask(device, api::MARK_SEEN, of(params::Keys { keys: keys.to_vec() }), ACT_WAIT).map(|_| ())
    }

    /// Marks sessions on `device` unread, so each is Your turn again there and everywhere else
    /// until it's seen or prompted.
    pub fn mark_unread(&self, device: &str, keys: &[String]) -> std::result::Result<(), String> {
        self.ask(device, api::MARK_UNREAD, of(params::Keys { keys: keys.to_vec() }), ACT_WAIT).map(|_| ())
    }

    /// Snoozes sessions on `device` until `until` (ms since the epoch), or wakes them with none:
    /// each leaves Needs you and Your turn there and everywhere else until then.
    pub fn snooze(&self, device: &str, keys: &[String], until: Option<i64>) -> std::result::Result<(), String> {
        self.ask(device, api::SESSION_SNOOZE, of(params::Snooze { keys: keys.to_vec(), until }), ACT_WAIT).map(|_| ())
    }

    /// Quiets sessions on `device`, or makes them heard again: a quiet one's turns don't wait on
    /// you there or anywhere else, and it waits on you only when it needs you.
    pub fn quiet(&self, device: &str, keys: &[String], quiet: bool) -> std::result::Result<(), String> {
        self.ask(device, api::SESSION_QUIET, of(params::Quiet { keys: keys.to_vec(), quiet }), ACT_WAIT).map(|_| ())
    }

    /// Pins sessions on `device` at the top of its pinned ones, or unpins them: the Sessions view
    /// there lists them first, and the devices it tells list them as pinned.
    pub fn pin(&self, device: &str, keys: &[String], pinned: bool) -> std::result::Result<(), String> {
        self.ask(device, api::SESSION_PIN, of(params::Pin { keys: keys.to_vec(), pinned }), ACT_WAIT).map(|_| ())
    }

    /// A PR's description, reviews, comments and changed files, as `device`'s `gh` reads them
    /// from GitHub: two GitHub requests there, so it's given as long as an act.
    pub fn pr_detail(&self, device: &str, repo: &str, number: u64) -> std::result::Result<crate::pr::Detail, String> {
        read(self.ask(device, api::PR_DETAIL, of(params::Pr { repo: repo.into(), number }), PR_WAIT)?)
    }

    /// What a PR's merge box offers you, as `device`'s `gh` reads it.
    pub fn pr_merge_box(&self, device: &str, repo: &str, number: u64) -> std::result::Result<crate::pr::MergeBox, String> {
        read(self.ask(device, api::PR_MERGE_BOX, of(params::Pr { repo: repo.into(), number }), ACT_WAIT)?)
    }

    /// Reviews a PR as you on GitHub, through `device`'s `gh`. `draft` is the stamp of the draft
    /// review shown with it (`pr::Draft::stamp`), none for none shown. Only a Mac that checks it
    /// (`proto::PR_DRAFTS`) is asked: one from before would submit any draft of yours unseen.
    pub fn pr_review(&self, device: &str, repo: &str, number: u64, verdict: crate::pr::Verdict, body: &str, draft: Option<&str>) -> std::result::Result<crate::pr::Reviewed, String> {
        let hex = self.key_of(device).ok_or("That device isn't linked.")?;
        let checks = self.0.lock().conns.get(&hex).map(|c| (c.name.clone(), c.knows.iter().any(|k| k == proto::PR_DRAFTS)));
        if let Some((name, false)) = checks {
            return Err(format!("Update Codealong on {name} to review from here: its version would submit a draft review of yours without checking it's the one shown."));
        }
        let asked = self.ask(device, api::PR_REVIEW, of(params::PrReview { repo: repo.into(), number, verdict, body: body.into(), draft: draft.map(String::from) }), PR_WAIT)?;
        // Macs before the draft was said answer the time the board showed it.
        Ok(serde_json::from_value(asked).unwrap_or_default())
    }

    /// Does what a PR's merge box does, as you on GitHub, through `device`'s `gh`.
    pub fn pr_merge(&self, device: &str, repo: &str, number: u64, id: &str, op: crate::pr::Op) -> std::result::Result<crate::pr::Merged, String> {
        read(self.ask(device, api::PR_MERGE, of(params::PrMerge { repo: repo.into(), number, id: id.into(), op }), PR_WAIT)?)
    }

    /// Keeps an image on `device` for a prompt to it (`api::IMAGE_KEEP`): how the prompt refers
    /// to it.
    pub fn keep_image(&self, device: &str, name: &str, bytes: &[u8]) -> std::result::Result<api::Kept, String> {
        // Refused here: one past the line's length would close the connection, and with it
        // everything else under way on it.
        if bytes.len() > api::IMAGE_MAX_BYTES {
            return Err(format!("That image is over {} MB.", api::IMAGE_MAX_BYTES / (1024 * 1024)));
        }
        let _one_at_a_time = self.0.sending_image.lock().unwrap_or_else(|e| e.into_inner());
        read(self.ask(device, api::IMAGE_KEEP, of(params::ImageKeep { name: name.into(), data: data_encoding::BASE64.encode(bytes) }), IMAGE_WAIT)?)
    }

    /// An image a prompt on `device` refers to (`images/<hash>.<ext>`): its bytes.
    pub fn read_image(&self, device: &str, path: &str) -> std::result::Result<Vec<u8>, String> {
        let image: api::Image = read(self.ask(device, api::IMAGE_READ, of(params::ImageRead { path: path.into() }), IMAGE_WAIT)?)?;
        data_encoding::BASE64.decode(image.data.as_bytes()).map_err(|_| "The image didn't arrive whole.".to_string())
    }

    /// Snoozes your review of PRs on `device` (`owner/name#number`) until `until`, or wakes it.
    pub fn pr_snooze(&self, device: &str, keys: &[String], until: Option<i64>) -> std::result::Result<(), String> {
        self.ask(device, api::PR_SNOOZE, of(params::Snooze { keys: keys.to_vec(), until }), ACT_WAIT).map(|_| ())
    }

    /// Where each of `device`'s accounts can start a session, and the models it can ask for.
    pub fn start_options(&self, device: &str) -> std::result::Result<api::StartOptions, String> {
        read(self.ask(device, api::START_OPTIONS, json!({}), READ_WAIT)?)
    }

    /// Starts a session on `device`, in one of its worktrees (`spec.lane_id`, its own id there).
    pub fn start_session(&self, device: &str, spec: &api::StartSpec) -> std::result::Result<api::Started, String> {
        read(self.ask(device, api::START, of(params::Start { spec: spec.clone() }), ACT_WAIT)?)
    }

    /// Forks a session on `device` into a copy that goes on without a window there, on `prompt`
    /// when it isn't empty.
    pub fn fork(&self, device: &str, key: &str, prompt: &str) -> std::result::Result<api::Started, String> {
        read(self.ask(device, api::SESSION_FORK, of(params::Fork { key: key.into(), prompt: prompt.into() }), ACT_WAIT)?)
    }

    /// Done with a session on `device`: it ends as far as that Mac can end it, and is marked seen
    /// and archived there. Work under way holds it (`done` false, `message` says what) unless
    /// `stop` ends that too. Macs before 2026-10-07 refuse it as a request they don't know.
    pub fn done(&self, device: &str, key: &str, stop: bool) -> std::result::Result<api::Done, String> {
        read(self.ask(device, api::SESSION_DONE, of(params::Done { key: key.into(), stop }), DONE_WAIT)?)
    }

    /// Asks `device` to send the replies of these sessions (keys there) as they're written, in
    /// place of any it sent before; none stops them. Also reads where each stands now, since
    /// only what's written from here on is sent. Says false when the device doesn't know how.
    pub fn watch(&self, device: &str, keys: &[String]) -> bool {
        let Some(hex) = self.key_of(device) else { return false };
        let knows = self.0.lock().conns.get(&hex).is_some_and(|c| c.knows.iter().any(|k| k == proto::WATCH));
        if !knows {
            return false;
        }
        if self.ask(&hex, api::SESSION_WATCH, of(params::Keys { keys: keys.to_vec() }), READ_WAIT).is_err() {
            return false;
        }
        for key in keys {
            if let Ok(Some(s)) = self.ask(&hex, api::SESSION_STREAM, of(params::Session { key: key.clone() }), READ_WAIT).and_then(read::<Option<api::Stream>>) {
                let mut st = self.0.lock();
                if !(s.text.is_empty() && s.tool.is_none()) {
                    st.streams.entry(hex.clone()).or_default().insert(key.clone(), s);
                }
            }
        }
        self.0.changed();
        true
    }

    /// The reply a watched session on `device` is writing, if it is.
    pub fn stream(&self, device: &str, key: &str) -> Option<api::Stream> {
        let hex = self.key_of(device)?;
        self.0.lock().streams.get(&hex).and_then(|m| m.get(key)).cloned()
    }

    /// Stops the turn a session on `device` is taking (one whose feed says `stoppable`). Says what
    /// happened; None from Macs before 2026-09-29, which stop only sessions Codealong runs.
    pub fn cancel(&self, device: &str, key: &str) -> std::result::Result<Option<String>, String> {
        read(self.ask(device, api::SESSION_CANCEL, of(params::Session { key: key.into() }), ACT_WAIT)?)
    }

    /// Asks a session's side conversation on `device` (`key` there) `text`: a throwaway copy of
    /// its conversation answers there, and the session carries on, unaware. `from` goes on in one
    /// kept from before (`api::SideKept::id`), which only a Mac that says `proto::SIDE_FROM` is
    /// asked: one from before would ask a new copy that knows none of it. The side conversation
    /// with the question in it is `side`'s from then on, and the answer comes there as it's
    /// written. Macs before 2026-10-23 refuse it as a request they don't know.
    pub fn side_ask(&self, device: &str, key: &str, text: &str, from: Option<&str>) -> std::result::Result<api::Side, String> {
        let hex = self.key_of(device).ok_or("That device isn't linked.")?;
        if from.is_some() {
            let checks = self.0.lock().conns.get(&hex).map(|c| (c.name.clone(), c.knows.iter().any(|k| k == proto::SIDE_FROM)));
            if let Some((name, false)) = checks {
                return Err(format!("Update Codealong on {name} to go on with a side conversation from before from here: its version would ask a new copy that knows none of it."));
            }
        }
        let told = self.side_told(&hex, key);
        let side: api::Side = read(self.ask(&hex, api::SIDE_ASK, of(params::Side { key: key.into(), text: text.into(), from: from.map(String::from) }), ACT_WAIT)?)?;
        self.side_read(&hex, key, told, Some(side.clone()));
        Ok(side)
    }

    /// A session's open side conversation on `device`, read from it now, as `side` has it from
    /// then on: one begun before, or gone on, or closed, while the two weren't connected.
    pub fn side_get(&self, device: &str, key: &str) -> std::result::Result<Option<api::Side>, String> {
        let hex = self.key_of(device).ok_or("That device isn't linked.")?;
        let told = self.side_told(&hex, key);
        let side: Option<api::Side> = read(self.ask(&hex, api::SIDE_GET, of(params::Session { key: key.into() }), READ_WAIT)?)?;
        self.side_read(&hex, key, told, side.clone());
        Ok(side)
    }

    /// The side conversations kept for a session on `device`, the latest first: the open one too,
    /// as far as it's been answered.
    pub fn side_past(&self, device: &str, key: &str) -> std::result::Result<Vec<api::SideKept>, String> {
        read(self.ask(device, api::SIDE_PAST, of(params::Session { key: key.into() }), READ_WAIT)?)
    }

    /// Closes a session's side conversation on `device`: a copy that's answering answers what it
    /// was asked before it ends, as `side_closing` has it, and what was said is kept.
    pub fn side_close(&self, device: &str, key: &str) -> std::result::Result<(), String> {
        let hex = self.key_of(device).ok_or("That device isn't linked.")?;
        let told = self.side_told(&hex, key);
        self.ask(&hex, api::SIDE_CLOSE, of(params::Session { key: key.into() }), ACT_WAIT)?;
        let mut st = self.0.lock();
        if let Some(sides) = st.sides.get_mut(&hex).filter(|s| s.told(key) == told) {
            // It's open again only once it's asked again: what said it was open came before.
            if let Some(open) = sides.open.remove(key) {
                let at = sides.closed_at.entry(open.id).or_default();
                *at = (*at).max(open.turns.len());
            }
        }
        drop(st);
        self.0.changed();
        Ok(())
    }

    /// Forgets one of a session's kept side conversations on `device`, stopping it if it was
    /// closed and is still answering.
    pub fn side_forget(&self, device: &str, key: &str, id: &str) -> std::result::Result<(), String> {
        self.ask(device, api::SIDE_FORGET, of(params::SideForget { key: key.into(), id: id.into() }), ACT_WAIT).map(|_| ())
    }

    /// A session's open side conversation on `device`, as that device last said, once it's been
    /// asked about here (`side_ask`, `side_get`, `side_close`); none when it has none.
    pub fn side(&self, device: &str, key: &str) -> Option<api::Side> {
        let hex = self.key_of(device)?;
        self.0.lock().sides.get(&hex).and_then(|s| s.open.get(key)).cloned()
    }

    /// A side conversation on `device` closed while its copy answered, by id, as that device last
    /// said: `running` until it has answered.
    pub fn side_closing(&self, device: &str, id: &str) -> Option<api::Side> {
        let hex = self.key_of(device)?;
        self.0.lock().sides.get(&hex).and_then(|s| s.closing.get(id)).cloned()
    }

    fn side_told(&self, hex: &str, key: &str) -> u64 {
        self.0.lock().sides.get(hex).map_or(0, |s| s.told(key))
    }

    /// Takes what a read said of a session's open side conversation (none: it has none), unless
    /// the device told of it since `told`, which is newer.
    fn side_read(&self, hex: &str, key: &str, told: u64, side: Option<api::Side>) {
        let mut st = self.0.lock();
        let sides = st.sides.entry(hex.to_string()).or_default();
        if sides.told(key) != told {
            return;
        }
        sides.take(side.unwrap_or_else(|| api::Side { key: key.into(), ..Default::default() }));
        drop(st);
        self.0.changed();
    }

    /// A linked device's whole key, from it or its short id.
    fn key_of(&self, device: &str) -> Option<String> {
        self.0.lock().linked.iter().find(|l| l.key == device || short_of(&l.key) == device).map(|l| l.key.clone())
    }

    /// The linked devices, in the order they were linked.
    pub fn devices(&self) -> Vec<Device> {
        let st = self.0.lock();
        self.0.devices(&st)
    }

    /// Runs `f` over each linked device and the last feed it sent, in the order they were linked.
    pub fn with_feeds<R>(&self, f: impl FnOnce(&[(Device, &Replica)]) -> R) -> R {
        let st = self.0.lock();
        let empty = Replica::default();
        let pairs: Vec<(Device, &Replica)> = self.0.devices(&st).into_iter().map(|d| {
            let r = st.replicas.get(&d.key).map(|c| &c.replica).unwrap_or(&empty);
            (d, r)
        }).collect();
        f(&pairs)
    }
}

/// An answer read as what the request returns.
/// A request's params as the JSON it carries.
fn of(p: impl Serialize) -> Value {
    serde_json::to_value(p).unwrap_or_default()
}

fn read<T: serde::de::DeserializeOwned>(v: Value) -> std::result::Result<T, String> {
    serde_json::from_value(v).map_err(|e| format!("The other device's answer didn't read: {e}"))
}

/// A key's short id: its first five bytes, as hex.
fn short_of(hex: &str) -> &str {
    &hex[..hex.len().min(10)]
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn changed(&self) {
        (self.on_change)();
    }

    fn name_of(&self, st: &State, key: &str) -> String {
        st.replicas.get(key).map(|c| c.replica.device.name.clone()).filter(|n| !n.is_empty())
            .or_else(|| st.conns.get(key).map(|c| c.name.clone()))
            .or_else(|| st.linked.iter().find(|l| l.key == key).map(|l| l.name.clone()))
            .unwrap_or_else(|| "The other device".into())
    }

    fn devices(&self, st: &State) -> Vec<Device> {
        st.linked
            .iter()
            .map(|l| {
                let conn = st.conns.get(&l.key);
                let cached = st.replicas.get(&l.key);
                Device {
                    key: l.key.clone(),
                    short: short_of(&l.key).to_string(),
                    name: self.name_of(st, &l.key),
                    online: conn.is_some(),
                    since: conn.map(|c| c.since),
                    last_seen: cached.map(|c| c.last_seen).filter(|t| *t > 0),
                    remote_control: cached.is_some_and(|c| c.replica.device.remote_control),
                    app: conn.map(|c| c.app.clone()).or_else(|| cached.map(|c| c.replica.device.app.clone())).unwrap_or_default(),
                    loading: cached.is_some_and(|c| c.replica.device.loading),
                    linked_back: conn.is_none_or(|c| c.linked_there),
                }
            })
            .collect()
    }

    fn forget(&self, st: &mut State, key: &str) {
        st.linked.retain(|l| l.key != key);
        st.replicas.remove(key);
        st.sides.remove(key);
        save_linked(&self.dir, &st.linked);
        st.cache_dirty = true;
    }

    fn save_cache(&self) {
        let cache = {
            let mut st = self.lock();
            if !st.cache_dirty {
                return;
            }
            st.cache_dirty = false;
            let linked: HashSet<&String> = st.linked.iter().map(|l| &l.key).collect();
            st.replicas.iter().filter(|(k, _)| linked.contains(k)).map(|(k, v)| (k.clone(), v.clone())).collect::<HashMap<_, _>>()
        };
        write_json(&self.dir.join("device-feeds.json"), &cache);
    }
}

/// Keeps a connection to each linked device while the client runs: dials the ones that aren't
/// connected at once, then less often each time one doesn't answer.
fn dial_loop(inner: Arc<Inner>, iroh: Arc<Iroh>, gen: u64) {
    let mut wait: HashMap<String, (Instant, Duration)> = HashMap::new();
    let dialing: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let mut saved = Instant::now();
    while inner.gen.load(Ordering::SeqCst) == gen {
        let todo: Vec<String> = {
            let st = inner.lock();
            st.linked.iter().filter(|l| !st.conns.contains_key(&l.key) && !st.linking.contains_key(&l.key)).map(|l| l.key.clone()).collect()
        };
        for hex in todo {
            if wait.get(&hex).is_some_and(|(at, d)| at.elapsed() < *d) {
                continue;
            }
            let next = wait.get(&hex).map(|(_, d)| (*d * 2).min(DIAL_MAX)).unwrap_or(DIAL_FIRST);
            wait.insert(hex.clone(), (Instant::now(), next));
            let Ok(key) = proto::parse_key(&hex) else { continue };
            if !dialing.lock().map(|mut d| d.insert(hex.clone())).unwrap_or(false) {
                continue;
            }
            let (inner, iroh, dialing) = (inner.clone(), iroh.clone(), dialing.clone());
            std::thread::spawn(move || {
                let wire = iroh.connect(key);
                if let Ok(mut d) = dialing.lock() {
                    d.remove(&hex);
                }
                if let Ok(wire) = wire {
                    run(inner, wire, key, None, gen);
                }
            });
        }
        // A device that's connected starts its wait over once it goes away.
        {
            let st = inner.lock();
            wait.retain(|k, _| !st.conns.contains_key(k));
        }
        if saved.elapsed() >= SAVE_EVERY {
            saved = Instant::now();
            inner.save_cache();
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// One connection, from the hello to the end. With `secret`, it asks to link with that link
/// code's secret once the proofs check out.
fn run(inner: Arc<Inner>, wire: Wire, expect: PublicKey, secret: Option<String>, gen: u64) {
    // Only the connection that asks to link answers for the link: an earlier one to the same
    // device, closed as this one opens, has nothing to say about it.
    let linking = secret.is_some();
    let Wire { reader, mut writer, set_timeout, close } = wire;
    let close: Arc<dyn Fn() + Send + Sync> = Arc::from(close);
    let hex = proto::key_hex(&expect);
    let (tx, rx) = channel::<Msg>();
    {
        let close = close.clone();
        std::thread::Builder::new()
            .name("client-writer".into())
            .spawn(move || {
                loop {
                    let msg = match rx.recv_timeout(PING_EVERY) {
                        Ok(m) => m,
                        Err(RecvTimeoutError::Timeout) => Msg::Ping,
                        Err(RecvTimeoutError::Disconnected) => break,
                    };
                    if writer.write_all(msg.line().as_bytes()).and_then(|_| writer.flush()).is_err() {
                        break;
                    }
                }
                close();
            })
            .ok();
    }
    let mut reader = BufReader::with_capacity(64 * 1024, reader);
    set_timeout(Some(HANDSHAKE_TIMEOUT));
    let my_nonce = proto::nonce();
    // A device that only watches runs no schedules, so it isn't sent the ones your machines share.
    let knows = proto::KNOWS.into_iter().filter(|k| *k != proto::SCHEDULES).map(String::from).collect();
    let _ = tx.send(Msg::Hello { v: VERSION, key: inner.hex.clone(), name: inner.name.clone(), app: inner.app.clone(), nonce: my_nonce.clone(), kind: Some(inner.kind.clone()), knows });
    let shook = (|| -> std::result::Result<(String, String, Vec<String>), String> {
        let Ok(Some(Msg::Hello { v, key, name, app, nonce, knows, .. })) = proto::read_msg(&mut reader) else { return Err("The other device didn't say hello.".into()) };
        if v != VERSION {
            return Err(format!("{name} speaks version {v} of Codealong's protocol; update Codealong there or here."));
        }
        let key = proto::parse_key(&key).map_err(|e| e.to_string())?;
        if key != expect {
            return Err("That isn't the device expected.".into());
        }
        let _ = tx.send(Msg::Proof { sig: proto::prove(&inner.secret, &nonce, &key).map_err(|e| e.to_string())? });
        let Ok(Some(Msg::Proof { sig })) = proto::read_msg(&mut reader) else { return Err("The other device didn't prove it holds its key.".into()) };
        proto::check_proof(&key, &sig, &my_nonce, &inner.key).map_err(|e| e.to_string())?;
        Ok((name, app, knows))
    })();
    let (name, app, knows) = match shook {
        Ok(v) => v,
        Err(e) => {
            if linking {
                if let Some(s) = inner.lock().linking.remove(&hex) {
                    let _ = s.send(Err(e));
                }
            }
            drop(tx);
            close();
            return;
        }
    };
    set_timeout(Some(IDLE_TIMEOUT));
    let serial = inner.ids.fetch_add(1, Ordering::SeqCst);
    {
        let mut st = inner.lock();
        if inner.gen.load(Ordering::SeqCst) != gen || !st.running {
            drop(st);
            drop(tx);
            close();
            return;
        }
        if let Some(old) = st.conns.remove(&hex) {
            (old.close)();
        }
        let linked = st.linked.iter().any(|l| l.key == hex);
        if let Some(l) = st.linked.iter_mut().find(|l| l.key == hex && l.name != name) {
            l.name = name.clone();
            save_linked(&inner.dir, &st.linked);
        }
        // What this device still has of the other's last feed, so only what changed since comes.
        let resume = (linked && knows.iter().any(|k| k == proto::HAVE)).then(|| st.replicas.get(&hex).map(|c| c.replica.have())).flatten().filter(|h| !h.is_empty());
        // A feed that resumes sends no PR list, so one kept from a build that sent one is let go
        // when this one won't send any: its reviews would show and fail to act.
        if let Some(c) = st.replicas.get_mut(&hex) {
            c.replica.heard_from(&knows);
        }
        st.conns.insert(hex.clone(), Conn { serial, knows, tx: tx.clone(), close: close.clone(), name: name.clone(), app, linked_there: false, since: now_ms() });
        // Before the standing, which the other side's first feed waits for.
        if let Some(lanes) = resume {
            let _ = tx.send(Msg::Have { lanes });
        }
        let _ = tx.send(Msg::Standing { linked });
        if let Some(secret) = secret {
            let _ = tx.send(Msg::LinkAsk { secret: Some(secret) });
        }
    }
    inner.changed();

    while let Ok(Some(msg)) = proto::read_msg(&mut reader) {
        handle(&inner, &hex, serial, &tx, msg);
    }

    let mut st = inner.lock();
    if st.conns.get(&hex).is_some_and(|c| c.serial == serial) {
        st.conns.remove(&hex);
        if let Some(c) = st.replicas.get_mut(&hex) {
            c.last_seen = now_ms();
            st.cache_dirty = true;
        }
        let gone: Vec<u64> = st.pending.iter().filter(|(_, (k, _))| *k == hex).map(|(id, _)| *id).collect();
        for id in gone {
            if let Some((_, s)) = st.pending.remove(&id) {
                let _ = s.send(Err(format!("{name} went away.")));
            }
        }
        if linking {
            if let Some(s) = st.linking.remove(&hex) {
                let _ = s.send(Err(format!("{name} went away before it answered.")));
            }
        }
        // A reply half written when the connection went is no longer being written, as far as
        // this device can tell.
        st.streams.remove(&hex);
    }
    drop(st);
    inner.changed();
    drop(tx);
    close();
}

fn handle(inner: &Arc<Inner>, key: &str, serial: u64, tx: &Sender<Msg>, msg: Msg) {
    let mut st = inner.lock();
    // A connection that was replaced or closed is on its way out: it's owed nothing.
    if !st.conns.get(key).is_some_and(|c| c.serial == serial) {
        return;
    }
    match msg {
        Msg::Feed(f) => {
            if !st.linked.iter().any(|l| l.key == key) {
                return;
            }
            let c = st.replicas.entry(key.to_string()).or_default();
            c.replica.apply(f);
            c.last_seen = now_ms();
            st.cache_dirty = true;
        }
        Msg::Req { id, .. } => {
            let _ = tx.send(Msg::Res { id, ok: false, result: None, error: Some(format!("{} only watches, so it has nothing to answer.", inner.name)) });
            return;
        }
        Msg::Res { id, ok, result, error } => {
            if st.pending.get(&id).is_some_and(|(k, _)| k == key) {
                if let Some((_, s)) = st.pending.remove(&id) {
                    let _ = s.send(if ok { Ok(result.unwrap_or(Value::Null)) } else { Err(error.unwrap_or_else(|| "The other device said no.".into())) });
                }
            }
            return;
        }
        Msg::Standing { linked } => {
            if let Some(c) = st.conns.get_mut(key) {
                c.linked_there = linked;
            }
        }
        Msg::LinkAsk { .. } => {
            // This device makes no link codes and has no one to ask, so only a device it's linked
            // with already hears yes.
            let accepted = st.linked.iter().any(|l| l.key == key);
            let _ = tx.send(Msg::LinkAnswer { accepted });
            return;
        }
        Msg::LinkAnswer { accepted } => {
            let waiter = st.linking.remove(key);
            if accepted {
                let name = st.conns.get(key).map(|c| c.name.clone()).unwrap_or_default();
                if !st.linked.iter().any(|l| l.key == key) {
                    st.linked.push(Linked { key: key.to_string(), name: name.clone(), linked_at: now_ms() });
                    save_linked(&inner.dir, &st.linked);
                }
                let _ = tx.send(Msg::Standing { linked: true });
                if let Some(w) = waiter {
                    let _ = w.send(Ok(name));
                }
            } else if let Some(w) = waiter {
                let _ = w.send(Err("The other device didn't take that code; make a new one there.".into()));
            }
        }
        Msg::Unlinked => {
            if st.linked.iter().any(|l| l.key == key) {
                inner.forget(&mut st, key);
            }
            if let Some(c) = st.conns.get_mut(key) {
                c.linked_there = false;
            }
        }
        Msg::Ping => {
            let _ = tx.send(Msg::Pong);
            return;
        }
        Msg::Stream(s) => {
            let replies = st.streams.entry(key.to_string()).or_default();
            // Both empty: the turn ended.
            if s.text.is_empty() && s.tool.is_none() {
                replies.remove(&s.key);
            } else {
                replies.insert(s.key.clone(), s);
            }
        }
        Msg::Side(side) => {
            let sides = st.sides.entry(key.to_string()).or_default();
            *sides.told.entry(side.key.clone()).or_default() += 1;
            sides.take(side);
        }
        // This device has no feed to pick up.
        Msg::Have { .. } => return,
        Msg::Pong | Msg::Hello { .. } | Msg::Proof { .. } | Msg::Unknown => return,
    }
    drop(st);
    inner.changed();
}

fn load_linked(dir: &Path) -> Vec<Linked> {
    std::fs::read(dir.join("devices.json")).ok().and_then(|b| serde_json::from_slice::<LinkedFile>(&b).ok()).map(|f| f.linked).unwrap_or_default()
}

fn save_linked(dir: &Path, linked: &[Linked]) {
    write_json(&dir.join("devices.json"), &LinkedFile { linked: linked.to_vec() });
}

fn load_cache(dir: &Path) -> HashMap<String, Cached> {
    std::fs::read(dir.join("device-feeds.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Writes a file whole or not at all: to a temporary file first, then renamed over the old one.
fn write_json<T: Serialize>(path: &Path, v: &T) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(bytes) = serde_json::to_vec(v) else { return };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_links_and_feeds_across_launches() {
        let dir = crate::fixtures::scratch("client");
        let key = SecretKey::generate().public();
        let hex = proto::key_hex(&key);
        save_linked(&dir, &[Linked { key: hex.clone(), name: "Mac mini".into(), linked_at: 1 }]);
        let mut replica = Replica::default();
        replica.device.name = "Gwen's Mac mini".into();
        replica.device.remote_control = true;
        write_json(&dir.join("device-feeds.json"), &HashMap::from([(hex.clone(), Cached { last_seen: 5, replica })]));

        let c = Client::new(dir.clone(), SecretKey::generate(), "iPhone".into(), "0.1.0".into(), proto::PHONE, Box::new(|| {}));
        let d = c.devices();
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].name.as_str(), d[0].online, d[0].remote_control, d[0].last_seen), ("Gwen's Mac mini", false, true, Some(5)));
        assert_eq!(d[0].short, &hex[..10]);
        assert!(c.ask(&hex[..10], "session-page", Value::Null, Duration::from_millis(10)).unwrap_err().contains("isn't connected"));
        c.unlink(&hex).unwrap();
        assert!(Client::new(dir.clone(), SecretKey::generate(), "iPhone".into(), "0.1.0".into(), proto::PHONE, Box::new(|| {})).devices().is_empty());
    }

    #[test]
    fn a_side_conversation_told_of_late_is_left_as_it_is_now() {
        let side = |answers: &[&str]| api::Side { id: "7-1".into(), key: "k".into(), turns: answers.iter().map(|a| api::SideTurn { question: "Why?".into(), answer: (*a).into(), ..Default::default() }).collect(), running: true, ..Default::default() };
        let mut sides = Sides::default();
        sides.take(side(&["Be"]));
        // Closed while it answers: none open, and the closed one answering.
        sides.take(api::Side { closed: true, ..side(&["Because"]) });
        assert!(sides.open.is_empty() && sides.closing["7-1"].running);
        // What said it was open before it closed, come late.
        sides.take(side(&["Because"]));
        assert!(sides.open.is_empty());
        // It answered, and then what said it was answering came late.
        sides.take(api::Side { closed: true, running: false, ..side(&["Because it is."]) });
        sides.take(api::Side { closed: true, ..side(&["Because it"]) });
        assert!(!sides.closing["7-1"].running);
        assert_eq!(sides.closing["7-1"].turns[0].answer, "Because it is.");
        // Asked again as it was read, it's the open one.
        sides.take(side(&["Because it is.", ""]));
        assert_eq!(sides.open["k"].turns.len(), 2);
        assert!(sides.closing.is_empty());
        // None open.
        sides.take(api::Side { key: "k".into(), ..Default::default() });
        assert!(sides.open.is_empty());
    }
}

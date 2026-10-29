//! Your other devices (`[devices]` in config.toml). Codealong on each device finds the others, on your
//! tailnet (tailnet.rs) or, once linked, anywhere through iroh (irohnet.rs), and keeps a
//! connection open to each. Over it (peerproto.rs) each device sends the other its feed
//! (peerfeed.rs), so the windows here show that device's worktrees and sessions beside this one's,
//! and asks it things: a page of a session's conversation, what a session waits on you for, a
//! search of what was said, and, when remote control is on over there, to send a prompt or
//! answer an approval. The device that's asked runs the same function a window's command runs
//! there (`Hooks::answer`, in lib.rs), as server.rs does for the `codealong` command.
//!
//! Trust goes by device key, and each device keeps its own list of the devices it's linked with
//! (`devices.json`). Being on the same tailnet links nothing: you link a device once, by answering
//! its ask on the device it asks (both show the same six-digit code), or with a link code made on
//! one and entered on the other. A device sends its feed only to devices it has linked and answers
//! only them. What each linked device last said is kept in `device-feeds.json`, so a device that's
//! asleep still shows as it was when last seen.
//!
//! Everything here is threads and channels, like the engine: a thread per connection reading,
//! one writing, and a feeder that turns board changes into feeds at most once a second and
//! hands the windows what they show of other devices (`DevicesView`).

use std::collections::{HashMap, HashSet};
use std::io::{BufReader, Write};
use std::net::{IpAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, sync_channel, RecvTimeoutError, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use iroh::{PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{DevicesCfg, ProjectCfg};
use crate::irohnet::{Iroh, Ticket};
use crate::model::{Board, DeviceTag, Lane, Usage};
use codealong_peer::api::Side;
use crate::peerfeed::{self, DeviceInfo, Outbox, Replica, Snapshot};
use crate::peerproto::{self, Msg, VERSION};
use crate::tailnet;
use crate::util::now_ms;

const APP: &str = env!("CARGO_PKG_VERSION");
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
/// A connection with nothing coming in for this long is gone (a device that slept, a network that
/// changed); pings go out well within it.
const IDLE_TIMEOUT: Duration = Duration::from_secs(80);
const PING_EVERY: Duration = Duration::from_secs(25);
/// Feeds go out at most this often, and a board is read this long after it changed, when the
/// engine has put it where `Hooks::board` reads.
const FEED_EVERY: Duration = Duration::from_secs(1);
/// A phone's or a tablet's go out this often: it's open while someone looks at it, waiting to see
/// a message they sent taken, and it's sent only what changed. About as often as the engine
/// publishes.
const WATCHER_FEED_EVERY: Duration = Duration::from_millis(250);
const SETTLE: Duration = Duration::from_millis(150);
/// The feeder looks at least this often for streams, the view and the cache.
const FEEDER_WAIT: Duration = Duration::from_millis(200);
const VIEW_EVERY: Duration = Duration::from_millis(300);
const CACHE_EVERY: Duration = Duration::from_secs(60);
const TAILNET_EVERY: Duration = Duration::from_secs(20);
/// The device with the lower tailnet address dials; the other dials too once one has been online
/// this long without a connection, in case the first can't.
const DIAL_ANYWAY_AFTER: Duration = Duration::from_secs(45);
const IROH_FIRST_DIAL: Duration = Duration::from_secs(30);
const IROH_DIAL_EVERY: Duration = Duration::from_secs(30);
const LINK_CODE_FOR_MS: i64 = 10 * 60 * 1000;
/// A reply streams out at most this often: acp.rs reports every chunk, and each report is the
/// reply so far.
const STREAM_EVERY: Duration = Duration::from_millis(150);
/// How many messages may wait to go out on a connection. A device that keeps asking and stops
/// reading the answers is cut off once this many wait, rather than having them kept for it.
const QUEUE_MAX: usize = 512;
/// How many bytes of answers may wait to go out on a connection, the same way: an answer can be
/// an image of megabytes (`api::IMAGE_READ`), and a few hundred of those waiting is gigabytes.
const QUEUE_BYTES_MAX: usize = 64 * 1024 * 1024;
/// How many of a linked device's requests may run here at once, each on a thread of its own:
/// more than any device's windows ask for together. One that asks for more is cut off.
const REQUESTS_MAX: usize = 32;

/// A connection to another device as the rest of this module sees it, whatever carries it.
pub use codealong_peer::wire::Wire;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    Tailscale,
    Iroh,
    /// Tests, over a loopback connection.
    Test,
}

/// What this module needs from the app, so it runs without one in tests.
pub struct Hooks {
    /// The board as it stands, for the feed.
    pub board: Box<dyn Fn() -> Board + Send + Sync>,
    /// Answers a linked device's request, as the matching Tauri command would here; with true
    /// when the device only watches (a phone), which runs no agents, so what it sends is yours.
    pub answer: Box<dyn Fn(&str, Value, bool) -> Result<Value, String> + Send + Sync>,
    /// Hands the windows what they show of other devices.
    pub emit: Box<dyn Fn(&DevicesView) + Send + Sync>,
    /// Hands the windows a side conversation of a session on another device as it changes there,
    /// by the key the session has here (`Msg::Side`).
    pub side: Box<dyn Fn(Side) + Send + Sync>,
}

/// A device on this device's list, as `devices.json` keeps it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Linked {
    /// The device key, as hex.
    pub key: String,
    pub name: String,
    pub linked_at: i64,
    #[serde(default)]
    pub via: String,
    /// The color you gave it; none takes the one its key picks (`color_of`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// What kind of device it is, as its hello said (`peerproto::MAC`, `PHONE`, `TABLET`); none for
    /// devices from before kinds, which are Macs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl Linked {
    /// It only watches (a phone or a tablet): it dials the devices it's linked with itself, and
    /// has no board to show or search.
    pub fn watches(&self) -> bool {
        peerproto::watches(self.kind.as_deref())
    }
}

/// The label colors a device can have, as the windows name them.
pub const COLORS: [&str; 8] = ["gray", "red", "pink", "purple", "indigo", "blue", "teal", "green"];
/// The ones a key picks from when you haven't chosen: none that reads as a status (red, gray).
const PICKED: [&str; 6] = ["purple", "teal", "indigo", "pink", "blue", "green"];

/// A linked device's color: yours, else one its key picks, the same on every device that has it.
fn color_of(l: &Linked) -> String {
    match &l.color {
        Some(c) if COLORS.contains(&c.as_str()) => c.clone(),
        _ => PICKED[u8::from_str_radix(l.key.get(..2).unwrap_or("0"), 16).unwrap_or(0) as usize % PICKED.len()].to_string(),
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct Cached {
    last_seen: i64,
    /// When its last feed came: a connection that closes moves `last_seen`, not this.
    #[serde(default)]
    fed_at: i64,
    replica: Replica,
}

/// What the windows show of other devices, and what Settings › Devices shows.
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct DevicesView {
    pub enabled: bool,
    pub me: MeView,
    /// Linked devices with boards of their own (Macs), in the order they were linked.
    pub devices: Vec<DeviceView>,
    /// Linked phones and tablets, which only watch: no boards, so they're in none of the views.
    pub phones: Vec<DeviceView>,
    /// Your devices on your tailnet running Codealong that aren't linked yet.
    pub found: Vec<FoundView>,
    /// Devices asking to be linked, waiting for your answer here.
    pub asks: Vec<AskView>,
    /// Every linked device's worktrees, each lane id and session key with `@<short>:` in front.
    pub lanes: Vec<Lane>,
    /// This machine and your others that share automations, for the engine (shared.rs): not
    /// the windows'.
    #[serde(skip)]
    pub machines: crate::shared::Machines,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct MeView {
    pub key: String,
    pub short: String,
    pub name: String,
    /// "off", "missing" (not installed), "stopped" (not connected or signed out), "running".
    pub tailscale: String,
    pub tailnet: Option<String>,
    /// Where this device listens, when it does.
    pub address: Option<String>,
    /// "off", "starting", "online", "offline", or why it didn't start.
    pub iroh: String,
    pub remote_control: bool,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct DeviceView {
    pub key: String,
    pub short: String,
    pub name: String,
    /// One of `COLORS`: the badge and edge its lanes and sessions get in the windows.
    pub color: String,
    pub online: bool,
    pub via: Option<Via>,
    /// When the connection opened.
    pub since: Option<i64>,
    pub last_seen: Option<i64>,
    pub remote_control: bool,
    pub app: String,
    /// It has this device on its list too. A link made from either side is made on both, so this
    /// is false only when the other device forgot this one.
    pub linked_back: bool,
    pub loading: bool,
    pub usage: Vec<Usage>,
    /// `mac`, `phone` or `tablet` (`peerproto::MAC`…).
    pub kind: String,
    /// Where it stands for the automations your machines share; none from one that shares none.
    pub automations: Option<codealong_peer::feed::Rank>,
    /// Its projects as it names them, from its worktrees as they came (`projects_there`): Settings ›
    /// Devices' Sync projects asks an agent to match this device's to them (src/agentTasks.ts).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub projects: Vec<ProjectThere>,
}

/// A project as another device names it, with its repos as config.toml names them
/// (`github.com/owner/name`), or a checkout's folder there when it has no GitHub remote.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ProjectThere {
    pub name: String,
    pub repos: Vec<String>,
}

/// Whether what a device's worktrees say is current: it's connected, and a feed has come since the
/// connection opened (its first always comes, with the device's own state). Until then they're as
/// they were when it was last seen, so its worktrees show as away, and the bar doesn't take what
/// changed there meanwhile as news (barfeed.rs).
fn caught_up(connected_since: Option<i64>, fed_at: i64) -> bool {
    connected_since.is_some_and(|since| fed_at >= since)
}

/// What a device's own worktrees say of its projects, in the order they come, before
/// `project_here` puts them into this device's: each repo once, by what GitHub calls it now. Its
/// chats and its worktrees in none of its projects (Elsewhere) are left out.
fn projects_there(lanes: &[Lane]) -> Vec<ProjectThere> {
    let mut out: Vec<ProjectThere> = Vec::new();
    for l in lanes.iter().filter(|l| !l.starts_chats && l.project != crate::board::ELSEWHERE) {
        let repo = l.repo_slug_now.as_ref().or(l.repo_slug.as_ref()).map(|s| format!("github.com/{s}")).or_else(|| (l.origin == crate::model::Origin::Main).then(|| l.path.clone()));
        let at = match out.iter().position(|p| p.name == l.project) {
            Some(i) => i,
            None => {
                out.push(ProjectThere { name: l.project.clone(), repos: Vec::new() });
                out.len() - 1
            }
        };
        if let Some(repo) = repo.filter(|r| !out[at].repos.contains(r)) {
            out[at].repos.push(repo);
        }
    }
    out
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct FoundView {
    pub key: String,
    pub short: String,
    pub name: String,
    pub code: String,
    /// You asked it to link; it's waiting for an answer there.
    pub asking: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AskView {
    pub key: String,
    pub short: String,
    pub name: String,
    pub code: String,
}

/// A linked device's answer to a search, for the quick panel.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DeviceFound {
    pub device: String,
    pub name: String,
    pub found: Option<crate::search::Found>,
    pub error: Option<String>,
}

struct Me {
    secret: SecretKey,
    key: PublicKey,
    hex: String,
    short: String,
}

/// Ends a connection and keeps why: the first reason given is the one its close is written down
/// with (`log_closed`), whatever closes it after.
#[derive(Clone)]
struct Closer {
    close: Arc<dyn Fn() + Send + Sync>,
    why: Arc<Mutex<Option<String>>>,
}

impl Closer {
    fn new(close: Arc<dyn Fn() + Send + Sync>) -> Closer {
        Closer { close, why: Arc::new(Mutex::new(None)) }
    }

    fn close(&self, why: impl Into<String>) {
        self.why.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(|| why.into());
        (self.close)();
    }

    /// Why this device closed it, when it did.
    fn why(&self) -> Option<String> {
        self.why.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// What goes out on one connection, waiting for its writer: at most `QUEUE_MAX` messages, and
/// answers of at most `QUEUE_BYTES_MAX` bytes. A message that finds it full closes the
/// connection, which is how a device that doesn't read is let go.
#[derive(Clone)]
struct Out {
    tx: SyncSender<Going>,
    close: Closer,
    /// The bytes of answers waiting (`Going::Line`).
    waiting: Arc<AtomicUsize>,
}

/// A message on its way out: most as they are, for the writer to put in words; an answer already
/// in words, so what waits is counted in bytes.
enum Going {
    Msg(Msg),
    Line(String),
}

impl Out {
    fn new(tx: SyncSender<Going>, close: Closer) -> Out {
        Out { tx, close, waiting: Arc::new(AtomicUsize::new(0)) }
    }

    fn send(&self, msg: Msg) -> std::result::Result<(), &'static str> {
        self.push(Going::Msg(msg))
    }

    /// An answer to a request, put in words here, on the request's thread.
    fn answer(&self, msg: Msg) -> std::result::Result<(), &'static str> {
        let line = msg.line();
        let n = line.len();
        if self.waiting.fetch_add(n, Ordering::SeqCst) + n > QUEUE_BYTES_MAX {
            self.waiting.fetch_sub(n, Ordering::SeqCst);
            self.close.close(format!("it fell behind: more than {} MB of answers waited to go to it", QUEUE_BYTES_MAX >> 20));
            return Err("the connection was too far behind, and is closed");
        }
        self.push(Going::Line(line)).inspect_err(|_| {
            self.waiting.fetch_sub(n, Ordering::SeqCst);
        })
    }

    fn push(&self, going: Going) -> std::result::Result<(), &'static str> {
        match self.tx.try_send(going) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                self.close.close(format!("it fell behind: {QUEUE_MAX} messages waited to go to it"));
                Err("the connection was too far behind, and is closed")
            }
            Err(TrySendError::Disconnected(_)) => Err("the connection is closed"),
        }
    }
}

/// A request running for a device: it frees its place when it's done, however it ends.
struct Running(Arc<AtomicUsize>);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

struct Conn {
    serial: u64,
    dialed: bool,
    via: Via,
    /// The tailnet user whose machine it's on, from `tailscale whois`.
    peer_user: Option<String>,
    addr: Option<IpAddr>,
    tx: Out,
    /// Ends the connection: its reader stops, so nothing more is taken from it.
    close: Closer,
    name: String,
    app: String,
    /// What kind of device it is, from its hello.
    kind: Option<String>,
    /// The sessions here whose replies it asked for as they're written (`api::SESSION_WATCH`).
    watching: HashSet<String>,
    /// It said whether it has this device linked (`Standing`). Its first feed waits for that: a
    /// device that picks up where its copy left off says so (`Have`) just before.
    heard: bool,
    linked_there: bool,
    since: i64,
    outbox: Outbox,
    /// What it knows beyond version 1, from its hello (`peerproto::KNOWS`).
    knows: Vec<String>,
    /// The board (`State::boards`) its last feed was cut from, and when a feed last went to it.
    fed_board: u64,
    fed_at: Option<Instant>,
}

struct Ask {
    key: String,
    name: String,
    code: String,
}

#[derive(Default)]
struct State {
    cfg: DevicesCfg,
    /// This device's `[[projects]]`, which other devices' worktrees are put into (`project_here`).
    projects: Vec<ProjectCfg>,
    /// The project this device's own worktrees of each repo are in (`repos_here`).
    repos_here: HashMap<String, String>,
    error: Option<String>,
    /// Goes up with every board the engine publishes, and whenever every device should be sent
    /// what changed; a connection whose last feed is from an earlier one is behind.
    boards: u64,
    board_at: Option<Instant>,
    linked: Vec<Linked>,
    conns: HashMap<String, Conn>,
    replicas: HashMap<String, Cached>,
    asks: Vec<Ask>,
    /// Devices this one asked to link, waiting for their answer.
    asking: HashSet<String>,
    /// Link codes made here and not used yet: the secret and when it stops working.
    codes: Vec<(String, i64)>,
    /// Why a device that said yes to this one's ask still isn't linked here: its link couldn't
    /// be saved. For `link_with_code`, which waits on the answer.
    link_failed: HashMap<String, String>,
    pending: HashMap<u64, (String, Sender<std::result::Result<Value, String>>)>,
    tailnet: Option<tailnet::Status>,
    tailnet_error: Option<String>,
    /// Your tailnet login: an ask to link without a code has to come from a machine of yours.
    my_user: Option<String>,
    listening: Option<String>,
    iroh: String,
    view_dirty: bool,
    view_at: Option<Instant>,
    cache_dirty: bool,
    cached_at: Option<Instant>,
    /// Hosted sessions' replies as they're written, the newest per session, for connections
    /// watching them; sent at most every `STREAM_EVERY`.
    streams: HashMap<String, codealong_peer::api::Stream>,
    streamed_at: Option<Instant>,
    /// Side conversations as they changed, for devices that asked about their sessions: sent with
    /// the streams, the latest of each (`side_queued`).
    sides: Vec<Side>,
    /// The sessions here whose side conversations each device has asked about (`follows_sides`),
    /// by its key: sent to it as they change (`Msg::Side`), when it knows them, over whichever
    /// connection it has. Kept across its connections, so an answer it asked for goes on reaching
    /// it after it comes back.
    sides_asked: HashMap<String, HashSet<String>>,
}

struct Inner {
    me: OnceLock<Me>,
    /// The computer's name, read when Devices is first turned on.
    computer: OnceLock<String>,
    dir: PathBuf,
    hooks: Hooks,
    st: Mutex<State>,
    wake: Condvar,
    /// Goes up whenever the transports stop, so their threads know to end.
    gen: AtomicU64,
    ids: AtomicU64,
    iroh: Mutex<Option<Arc<Iroh>>>,
    /// Request threads started, for tests.
    #[cfg(test)]
    requests: AtomicU64,
}

#[derive(Clone)]
pub struct Peers(Arc<Inner>);

impl Peers {
    /// Starts the feeder. Nothing reaches the network, and the device key isn't read, until
    /// `[devices]` is turned on (`configure`).
    pub fn new(dir: PathBuf, hooks: Hooks) -> Peers {
        let st = State { linked: load_linked(&dir), replicas: load_cache(&dir), iroh: "off".into(), ..Default::default() };
        let inner = Arc::new(Inner {
            me: OnceLock::new(),
            computer: OnceLock::new(),
            dir,
            hooks,
            st: Mutex::new(st),
            wake: Condvar::new(),
            gen: AtomicU64::new(0),
            ids: AtomicU64::new(1),
            iroh: Mutex::new(None),
            #[cfg(test)]
            requests: AtomicU64::new(0),
        });
        let feeder = inner.clone();
        std::thread::Builder::new().name("peers-feeder".into()).spawn(move || feed_loop(feeder)).expect("the feeder thread");
        Peers(inner)
    }

    /// For tests: a device with its key already.
    #[cfg(test)]
    fn with_key(dir: PathBuf, key: SecretKey, hooks: Hooks) -> Peers {
        let p = Peers::new(dir, hooks);
        let _ = p.0.computer.set("Test device".into());
        p.0.set_key(key);
        p
    }

    /// Called with every board the engine publishes: cheap, since the feed is cut later on
    /// the feeder's thread. `[devices]` rides on the board, so a change to it lands here too.
    pub fn publish(&self, board: &Board) {
        let cfg_changed = self.0.lock().cfg != board.devices;
        if cfg_changed {
            self.configure(&board.devices);
        }
        let here = repos_here(&board.lanes);
        let mut st = self.0.lock();
        st.boards += 1;
        st.board_at = Some(Instant::now());
        if st.projects != board.project_rules || st.repos_here != here {
            st.projects = board.project_rules.clone();
            st.repos_here = here;
            st.view_dirty = true;
        }
        drop(st);
        self.0.wake.notify_all();
    }

    /// Takes `[devices]` as it now is, starting or stopping what changed.
    pub fn configure(&self, cfg: &DevicesCfg) {
        let inner = &self.0;
        let mut st = inner.lock();
        let before = std::mem::replace(&mut st.cfg, cfg.clone());
        st.view_dirty = true;
        st.boards += 1;
        let transports = |c: &DevicesCfg| (c.enabled, c.tailscale, c.iroh, c.port);
        if transports(&before) == transports(cfg) && st.error.is_none() {
            drop(st);
            inner.wake.notify_all();
            return;
        }
        // Stop what runs, then start what's wanted.
        let gen = inner.gen.fetch_add(1, Ordering::SeqCst) + 1;
        for (_, c) in st.conns.drain() {
            c.close.close("[devices] changed here, so every connection starts again");
        }
        st.asks.clear();
        st.asking.clear();
        st.tailnet = None;
        st.tailnet_error = None;
        st.listening = None;
        st.iroh = "off".into();
        st.error = None;
        drop(st);
        if let Some(iroh) = inner.iroh.lock().ok().and_then(|mut i| i.take()) {
            std::thread::spawn(move || iroh.close());
        }
        if cfg.enabled {
            if inner.computer.get().is_none() {
                let _ = inner.computer.set(computer_name());
            }
            if inner.me.get().is_none() {
                match crate::devicekey::load_or_create() {
                    Ok(k) => inner.set_key(k),
                    Err(e) => {
                        inner.lock().error = Some(format!("Couldn't read this device's key from the keychain: {e:#}"));
                        inner.wake.notify_all();
                        return;
                    }
                }
            }
            if cfg.tailscale {
                let i = inner.clone();
                std::thread::Builder::new().name("peers-tailnet".into()).spawn(move || tailnet_loop(i, gen)).ok();
            }
            if cfg.iroh {
                let i = inner.clone();
                std::thread::Builder::new().name("peers-iroh".into()).spawn(move || iroh_loop(i, gen)).ok();
            }
        }
        inner.wake.notify_all();
    }

    pub fn view(&self) -> DevicesView {
        let st = self.0.lock();
        self.0.view(&st)
    }

    /// A hosted session's reply as it's written (acp.rs), for the linked devices watching it. The
    /// newest per session goes out at most every `STREAM_EVERY`.
    pub fn stream(&self, s: codealong_peer::api::Stream) {
        let mut st = self.0.lock();
        if !st.conns.values().any(|c| c.watching.contains(&s.key)) {
            return;
        }
        st.streams.insert(s.key.clone(), s);
        drop(st);
        self.0.wake.notify_all();
    }

    /// A side conversation as it changes here (side.rs), for the linked devices that asked about
    /// its session. Sent with the streams, at most every `STREAM_EVERY`.
    pub fn side(&self, side: &Side) {
        let mut st = self.0.lock();
        if !st.sides_asked.values().any(|keys| keys.contains(&side.key)) {
            return;
        }
        side_queued(&mut st.sides, side.clone());
        drop(st);
        self.0.wake.notify_all();
    }

    /// Asks a device of yours on your tailnet to link. Returns the code both devices show.
    pub fn link(&self, key: &str) -> std::result::Result<String, String> {
        let inner = &self.0;
        let me = inner.me.get().ok_or("Turn on Devices first.")?;
        let mut st = inner.lock();
        let conn = st.conns.get(key).ok_or("That device isn't connected any more.")?;
        if conn.peer_user.is_none() || conn.peer_user != st.my_user {
            return Err("Only your own devices on your tailnet can be linked this way; use a link code.".into());
        }
        let _ = conn.tx.send(Msg::LinkAsk { secret: None });
        let code = peerproto::link_code(&me.key, &peerproto::parse_key(key).map_err(|e| e.to_string())?);
        st.asking.insert(key.to_string());
        st.view_dirty = true;
        drop(st);
        inner.wake.notify_all();
        Ok(code)
    }

    /// Your answer to a device asking to link.
    pub fn answer_link(&self, key: &str, accept: bool) -> std::result::Result<(), String> {
        let inner = &self.0;
        let mut st = inner.lock();
        let i = st.asks.iter().position(|a| a.key == key).ok_or("That device isn't asking any more.")?;
        let ask = st.asks.remove(i);
        let Some(conn) = st.conns.get(key) else { return Err(format!("{} went away.", ask.name)) };
        let (tx, via) = (conn.tx.clone(), conn.via);
        // A link this device can't keep isn't made: the other is told no, and you why.
        let linked = if accept { inner.add_linked(&mut st, key, &ask.name, via).map_err(|e| format!("{} isn't linked: {e}", ask.name)) } else { Ok(()) };
        let accepted = accept && linked.is_ok();
        let _ = tx.send(Msg::LinkAnswer { accepted });
        if accepted {
            let _ = tx.send(Msg::Standing { linked: true });
        }
        st.view_dirty = true;
        drop(st);
        inner.wake.notify_all();
        linked
    }

    /// Gives a linked device a color of your choosing; none goes back to the one its key picks.
    pub fn set_color(&self, key: &str, color: Option<String>) -> std::result::Result<(), String> {
        if let Some(c) = &color {
            if !COLORS.contains(&c.as_str()) {
                return Err(format!("{c} isn't one of the colors: {}.", COLORS.join(", ")));
            }
        }
        let inner = &self.0;
        let mut st = inner.lock();
        if !Inner::is_linked(&st, key) {
            return Err("That device isn't linked.".into());
        }
        inner.change_linked(&mut st, |linked| {
            if let Some(l) = linked.iter_mut().find(|l| l.key == key) {
                l.color = color;
            }
        })?;
        st.view_dirty = true;
        drop(st);
        inner.wake.notify_all();
        Ok(())
    }

    /// Stops trusting a linked device, and tells it so when it's connected. Done only once
    /// devices.json no longer has it: trust left there would come back at the next launch, so a
    /// device that can't be taken off the list stays linked, here and there, and you hear why.
    pub fn unlink(&self, key: &str) -> std::result::Result<(), String> {
        let inner = &self.0;
        let mut st = inner.lock();
        if !st.linked.iter().any(|l| l.key == key) {
            return Err("That device isn't linked.".into());
        }
        let name = inner.name_of(&st, key);
        inner.forget(&mut st, key).map_err(|e| format!("{name} is still linked: {e}"))?;
        if let Some(conn) = st.conns.get_mut(key) {
            let _ = conn.tx.send(Msg::Unlinked);
            let _ = conn.tx.send(Msg::Standing { linked: false });
            conn.outbox.restart();
        }
        drop(st);
        inner.wake.notify_all();
        Ok(())
    }

    /// A link code for a device that isn't on your tailnet; it works once, for ten minutes.
    pub fn link_code(&self) -> std::result::Result<String, String> {
        let inner = &self.0;
        let mut st = inner.lock();
        if !st.cfg.enabled || !st.cfg.iroh {
            return Err("Link codes go through iroh; turn it on in Settings › Devices first.".into());
        }
        let me = inner.me.get().ok_or("This device's key isn't ready yet.")?;
        let t = Ticket::new(me.key);
        let now = now_ms();
        st.codes.retain(|(_, until)| *until > now);
        st.codes.push((t.secret.clone(), now + LINK_CODE_FOR_MS));
        Ok(t.encode())
    }

    /// Links with the device whose link code this is: dials it through iroh and asks with the
    /// code's secret. Returns the other device's name.
    pub fn link_with_code(&self, code: &str) -> std::result::Result<String, String> {
        let inner = &self.0;
        let t = Ticket::decode(code).map_err(|e| e.to_string())?;
        let me = inner.me.get().ok_or("Turn on Devices first.")?;
        if t.key == me.key {
            return Err("That's this device's own link code; enter it on the other device.".into());
        }
        let hex = peerproto::key_hex(&t.key);
        if let Some(l) = inner.lock().linked.iter().find(|l| l.key == hex) {
            return Ok(l.name.clone());
        }
        inner.lock().link_failed.remove(&hex);
        let iroh = inner.iroh.lock().ok().and_then(|i| i.clone()).ok_or("iroh isn't running; turn it on in Settings › Devices.")?;
        let wire = iroh.connect(t.key).map_err(|e| format!("Couldn't reach that device: {e:#}"))?;
        let gen = inner.gen.load(Ordering::SeqCst);
        let i = inner.clone();
        let secret = t.secret.clone();
        std::thread::spawn(move || run(i, wire, Via::Iroh, true, Some(t.key), None, None, Some(Some(secret)), gen));
        let until = Instant::now() + Duration::from_secs(20);
        let mut asked = false;
        while Instant::now() < until {
            std::thread::sleep(Duration::from_millis(200));
            let mut st = inner.lock();
            if let Some(l) = st.linked.iter().find(|l| l.key == hex) {
                return Ok(l.name.clone());
            }
            if let Some(why) = st.link_failed.remove(&hex) {
                return Err(why);
            }
            asked |= st.asking.contains(&hex);
            if asked && !st.asking.contains(&hex) {
                return Err("The other device didn't take that code; make a new one there.".into());
            }
        }
        Err("The other device didn't answer in time.".into())
    }

    /// What a linked device is called, by its short id, for words about it.
    pub fn name(&self, short: &str) -> String {
        let st = self.0.lock();
        let key = st.linked.iter().find(|l| short_of(&l.key) == short).map(|l| l.key.clone()).unwrap_or_default();
        self.0.name_of(&st, &key)
    }

    /// Whether a linked device, by its short id, said as it connected that it knows `word`
    /// (codealong-peer's `proto::KNOWS`); none while it isn't connected, which asking it says.
    pub fn knows(&self, short: &str, word: &str) -> Option<bool> {
        let st = self.0.lock();
        st.conns.iter().find(|(key, _)| short_of(key) == short).map(|(_, c)| c.knows.iter().any(|k| k == word))
    }

    /// Asks a linked device, by its short id, to run a request, and waits for its answer.
    pub fn ask(&self, short: &str, method: &str, params: Value, timeout: Duration) -> std::result::Result<Value, String> {
        let inner = &self.0;
        let (id, rx, name) = {
            let mut st = inner.lock();
            let Some(key) = st.linked.iter().find(|l| short_of(&l.key) == short).map(|l| l.key.clone()) else {
                return Err("That device isn't linked with this one any more.".into());
            };
            let name = inner.name_of(&st, &key);
            let Some(tx) = st.conns.get(&key).map(|c| c.tx.clone()) else {
                let seen = st.replicas.get(&key).map(|c| c.last_seen).filter(|t| *t > 0);
                return Err(match seen {
                    Some(t) => format!("{name} isn't connected; it was last seen {}.", crate::jev::ago_words(t, now_ms())),
                    None => format!("{name} isn't connected."),
                });
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
                Err(format!("{name} didn't answer in time."))
            }
        }
    }

    /// Searches what was said on every linked device that's connected, at once, each answer with
    /// its session keys made this device's (`@<short>:`).
    pub fn search(&self, words: &[String]) -> Vec<DeviceFound> {
        let targets: Vec<(String, String)> = {
            let st = self.0.lock();
            st.linked.iter().filter(|l| st.conns.contains_key(&l.key) && !l.watches()).map(|l| (short_of(&l.key).to_string(), self.0.name_of(&st, &l.key))).collect()
        };
        let handles: Vec<_> = targets
            .into_iter()
            .map(|(short, name)| {
                let me = self.clone();
                let words = words.to_vec();
                std::thread::spawn(move || {
                    // How long a device is given, and the keys its sessions have here, are the request's (actions.rs).
                    let got = crate::actions::ask_device(&me, codealong_peer::api::SEARCH, &short, crate::actions::params::Search { words });
                    match got.and_then(crate::actions::typed::<crate::search::Found>) {
                        Ok(found) => DeviceFound { device: short, name, found: Some(found), error: None },
                        Err(e) => DeviceFound { device: short, name, found: None, error: Some(e) },
                    }
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    }

    /// Runs a connection that's already open, on threads of its own, as a transport would.
    #[cfg(test)]
    fn attach(&self, wire: Wire, via: Via, dialed: bool, expect: Option<PublicKey>, peer_user: Option<String>, addr: Option<IpAddr>) {
        let inner = self.0.clone();
        let gen = inner.gen.load(Ordering::SeqCst);
        std::thread::spawn(move || run(inner, wire, via, dialed, expect, peer_user, addr, None, gen));
    }
}

/// A key's short id: the first five bytes of the key, as hex.
fn short_of(hex: &str) -> &str {
    &hex[..hex.len().min(10)]
}

/// Whether a device that asks `method` about a session is sent its side conversations as they
/// change from then on (`Msg::Side`): it asked one, or reads or closes them, as a window showing
/// the session does.
fn follows_sides(method: &str) -> bool {
    use codealong_peer::api;
    [api::SIDE_ASK, api::SIDE_GET, api::SIDE_PAST, api::SIDE_CLOSE].contains(&method)
}

/// Queues a side conversation as it changed, in place of what's queued of it, so each goes once
/// and as it is now. Of the same session's, one ended or closed is another (its id, `closed`), and
/// none open is one too (no id): a later one takes its place at the end, after what happened
/// before it, as the windows here were told.
fn side_queued(queue: &mut Vec<Side>, side: Side) {
    queue.retain(|q| (&q.key, &q.id, q.closed) != (&side.key, &side.id, side.closed));
    queue.push(side);
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_key(&self, secret: SecretKey) {
        let key = secret.public();
        let _ = self.me.set(Me { hex: peerproto::key_hex(&key), short: peerproto::short(&key), key, secret });
    }

    /// This device's name on the others: config.toml's, else the computer's.
    fn my_name(&self, st: &State) -> String {
        let n = st.cfg.name.trim();
        if !n.is_empty() {
            return n.to_string();
        }
        self.computer.get().cloned().unwrap_or_else(|| "This device".into())
    }

    fn name_of(&self, st: &State, key: &str) -> String {
        st.replicas.get(key).map(|c| c.replica.device.name.clone()).filter(|n| !n.is_empty())
            .or_else(|| st.conns.get(key).map(|c| c.name.clone()))
            .or_else(|| st.linked.iter().find(|l| l.key == key).map(|l| l.name.clone()))
            .unwrap_or_else(|| "The other device".into())
    }

    fn is_linked(st: &State, key: &str) -> bool {
        st.linked.iter().any(|l| l.key == key)
    }

    /// Changes the list of linked devices, and takes the change here only once devices.json has
    /// it: what's trusted here is then always what the next launch trusts. A change that
    /// couldn't be saved is said as well as returned, for the ones made in the background.
    fn change_linked(&self, st: &mut State, change: impl FnOnce(&mut Vec<Linked>)) -> std::result::Result<(), String> {
        let mut linked = st.linked.clone();
        change(&mut linked);
        save_linked(&self.dir, &linked).map_err(|e| format!("{e:#}"))?;
        st.linked = linked;
        Ok(())
    }

    fn add_linked(&self, st: &mut State, key: &str, name: &str, via: Via) -> std::result::Result<(), String> {
        st.asking.remove(key);
        st.asks.retain(|a| a.key != key);
        if !Self::is_linked(st, key) {
            let via = match via {
                Via::Tailscale => "tailscale",
                Via::Iroh => "iroh",
                Via::Test => "test",
            };
            let kind = st.conns.get(key).and_then(|c| c.kind.clone());
            let device = Linked { key: key.to_string(), name: name.to_string(), linked_at: now_ms(), via: via.into(), color: None, kind };
            if let Err(e) = self.change_linked(st, |linked| linked.push(device)) {
                st.view_dirty = true;
                return Err(e);
            }
        }
        if let Some(c) = st.conns.get_mut(key) {
            c.outbox.restart();
        }
        // The new device gets a whole feed at once.
        st.boards += 1;
        st.board_at = None;
        st.view_dirty = true;
        Ok(())
    }

    /// Takes a device off the list, and forgets what it last said, once devices.json no longer
    /// has it; when that can't be saved, it stays linked.
    fn forget(&self, st: &mut State, key: &str) -> std::result::Result<(), String> {
        self.change_linked(st, |linked| linked.retain(|l| l.key != key))?;
        st.replicas.remove(key);
        st.sides_asked.remove(key);
        st.cache_dirty = true;
        st.view_dirty = true;
        Ok(())
    }

    /// Runs a linked device's request, after checking it may make it.
    fn answer(&self, from: &str, method: &str, params: Value) -> std::result::Result<Value, String> {
        let (linked, watching, control, me, them, older, notes) = {
            let st = self.lock();
            let watching = st.linked.iter().find(|l| l.key == from).is_some_and(Linked::watches);
            let older = st.conns.get(from).is_some_and(|c| !peerfeed::knows_every_harness(&c.knows));
            let notes = st.conns.get(from).is_some_and(|c| !c.knows.iter().any(|k| k == peerproto::PEER_ITEMS));
            (Self::is_linked(&st, from), watching, st.cfg.remote_control, self.my_name(&st), self.name_of(&st, from), older, notes)
        };
        if !linked {
            return Err(format!("{me} isn't linked with this device."));
        }
        // Who may ask for what is api::REQUESTS's: reading and listing for any linked device,
        // driving an agent only with remote control on here.
        let acts = codealong_peer::api::reach(method) == Some(codealong_peer::api::Reach::Control);
        if acts && !control {
            return Err(format!("Remote control is off on {me}; turn it on there in Settings › Devices."));
        }
        let key = params.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
        // Before it's answered, so the question it asks and the answer as it's written reach it.
        if follows_sides(method) && !key.is_empty() {
            let mut st = self.lock();
            if st.conns.get(from).is_some_and(|c| c.knows.iter().any(|k| k == peerproto::SIDES)) {
                st.sides_asked.entry(from.to_string()).or_default().insert(key.clone());
            }
        }
        let started = std::time::Instant::now();
        let mut got = (self.hooks.answer)(method, params, watching);
        if let Ok(v) = &mut got {
            if older {
                peerfeed::for_older(v);
            }
            if notes && (method == codealong_peer::api::SESSION_PAGE || method == codealong_peer::api::SESSION_NEWER) {
                peerfeed::peer_items_as_notes(v);
            }
        }
        if acts {
            audit(&them, short_of(from), method, &key, &got, started.elapsed());
        }
        got
    }

    fn device_info(&self, st: &State, board: &Board) -> DeviceInfo {
        DeviceInfo { name: self.my_name(st), app: APP.into(), remote_control: st.cfg.remote_control, loading: board.loading, shares: Some(st.cfg.automations) }
    }

    fn view(&self, st: &State) -> DevicesView {
        let (key, short) = self.me.get().map(|m| (m.hex.clone(), m.short.clone())).unwrap_or_default();
        let tailscale = if !st.cfg.tailscale || !st.cfg.enabled {
            "off"
        } else if tailnet::cli().is_none() {
            "missing"
        } else if st.tailnet.as_ref().is_some_and(|s| s.running) {
            "running"
        } else {
            "stopped"
        };
        let me = MeView {
            key,
            short,
            name: self.my_name(st),
            tailscale: tailscale.into(),
            tailnet: st.tailnet.as_ref().map(|s| s.tailnet.clone()).filter(|t| !t.is_empty()),
            address: st.listening.clone(),
            iroh: st.iroh.clone(),
            remote_control: st.cfg.remote_control,
            error: st.error.clone().or_else(|| st.tailnet_error.clone()),
        };
        let view_of = |l: &Linked| {
                let conn = st.conns.get(&l.key);
                let cached = st.replicas.get(&l.key);
                DeviceView {
                    key: l.key.clone(),
                    short: short_of(&l.key).to_string(),
                    name: self.name_of(st, &l.key),
                    color: color_of(l),
                    online: conn.is_some(),
                    via: conn.map(|c| c.via),
                    since: conn.map(|c| c.since),
                    last_seen: cached.map(|c| c.last_seen).filter(|t| *t > 0),
                    remote_control: cached.is_some_and(|c| c.replica.device.remote_control),
                    app: conn.map(|c| c.app.clone()).or_else(|| cached.map(|c| c.replica.device.app.clone())).unwrap_or_default(),
                    linked_back: conn.is_none_or(|c| c.linked_there),
                    loading: cached.is_some_and(|c| c.replica.device.loading),
                    usage: cached.map(|c| c.replica.usage.clone()).unwrap_or_default(),
                    kind: l.kind.clone().unwrap_or_else(|| peerproto::MAC.into()),
                    automations: cached.and_then(|c| c.replica.device.shares),
                    projects: cached.map(|c| projects_there(&c.replica.lanes)).unwrap_or_default(),
                }
            };
        let devices = st.linked.iter().filter(|l| !l.watches()).map(view_of).collect();
        let phones = st.linked.iter().filter(|l| l.watches()).map(view_of).collect();
        let my_key = self.me.get().map(|m| m.key);
        let mut found: Vec<FoundView> = st
            .conns
            .iter()
            .filter(|(k, c)| !Self::is_linked(st, k) && c.peer_user.is_some() && c.peer_user == st.my_user)
            .map(|(k, c)| FoundView {
                key: k.clone(),
                short: short_of(k).to_string(),
                name: c.name.clone(),
                code: match (my_key, peerproto::parse_key(k)) {
                    (Some(a), Ok(b)) => peerproto::link_code(&a, &b),
                    _ => String::new(),
                },
                asking: st.asking.contains(k),
            })
            .collect();
        found.sort_by(|a, b| a.name.cmp(&b.name));
        let asks = st.asks.iter().map(|a| AskView { key: a.key.clone(), short: short_of(&a.key).to_string(), name: a.name.clone(), code: a.code.clone() }).collect();
        let lanes = if st.cfg.enabled {
            st.linked
                .iter()
                .filter(|l| !l.watches())
                .filter_map(|l| {
                    let c = st.replicas.get(&l.key)?;
                    let online = caught_up(st.conns.get(&l.key).map(|conn| conn.since), c.fed_at);
                    let tag = DeviceTag { name: self.name_of(st, &l.key), short: short_of(&l.key).to_string(), color: color_of(l), online, last_seen: Some(c.last_seen).filter(|t| *t > 0) };
                    Some(peerfeed::for_windows(short_of(&l.key), &tag, &c.replica))
                })
                .flatten()
                .map(|mut lane| {
                    project_here(&mut lane, &st.projects, &st.repos_here);
                    lane
                })
                .collect()
        } else {
            Vec::new()
        };
        DevicesView { enabled: st.cfg.enabled, me, devices, phones, found, asks, lanes, machines: self.machines(st) }
    }

    /// This machine and your others that share automations, as their feeds last said, a machine
    /// that's away with what it said before it went (`device-feeds.json`).
    fn machines(&self, st: &State) -> crate::shared::Machines {
        use crate::shared::{Machine, Machines, Me};
        let Some(me) = self.me.get().filter(|_| st.cfg.enabled) else { return Machines::default() };
        let others = st
            .linked
            .iter()
            .filter(|l| !l.watches())
            .filter_map(|l| {
                let c = st.replicas.get(&l.key)?;
                Some(Machine {
                    key: l.key.clone(),
                    name: self.name_of(st, &l.key),
                    rank: c.replica.device.shares?,
                    online: st.conns.contains_key(&l.key),
                    last_seen: Some(c.last_seen).filter(|t| *t > 0),
                    fed_at: Some(c.fed_at).filter(|t| *t > 0),
                    prs: c.replica.prs.clone(),
                    schedules: c.replica.schedules.clone(),
                })
            })
            .collect();
        Machines { me: Some(Me { key: me.hex.clone(), rank: st.cfg.automations }), others }
    }
}

/// Puts another device's worktree into one of this device's projects. Projects line up by repo,
/// not by name: a repo is the same everywhere, while what a device calls its project, and where
/// its folders are, are that device's own. So the worktree goes where this device's own
/// worktrees of that repo are; failing that, by this device's own patterns; failing that, into a
/// project here with the name the other device gave it; failing that, it keeps that name, and
/// shows as a project of its own. A repo goes by the name its remote has and by the one GitHub
/// gives it now, so a checkout from before a repo, or its owner, was renamed still lines up.
fn project_here(lane: &mut Lane, projects: &[ProjectCfg], repos_here: &HashMap<String, String>) {
    let slugs: Vec<String> = [&lane.repo_slug, &lane.repo_slug_now].into_iter().flatten().map(|s| s.to_lowercase()).collect();
    let by_worktrees = slugs.iter().find_map(|s| repos_here.get(s)).cloned();
    let by_pattern = || {
        let matches = |p: &&ProjectCfg| slugs.iter().any(|s| p.repos.iter().any(|pat| crate::repos::matches(&format!("github.com/{s}"), pat)));
        projects.iter().find(matches).map(|p| p.name.clone())
    };
    let by_name = || projects.iter().find(|p| p.name.eq_ignore_ascii_case(&lane.project)).map(|p| p.name.clone());
    let project = by_worktrees.or_else(by_pattern).or_else(by_name);
    if let Some(project) = project {
        lane.project = project;
        // Only worktrees in no project stay off the board; this one is in one here.
        lane.hidden = false;
    }
}

/// The project this device's worktrees of each repo are in, by every name the repo goes by
/// (lowercase `owner/name`): its remote's, and the one GitHub gives it now. Elsewhere is no
/// project, so it's left out.
fn repos_here(lanes: &[Lane]) -> HashMap<String, String> {
    let mut here = HashMap::new();
    for lane in lanes.iter().filter(|l| l.project != crate::board::ELSEWHERE) {
        for slug in [&lane.repo_slug, &lane.repo_slug_now].into_iter().flatten() {
            here.entry(slug.to_lowercase()).or_insert_with(|| lane.project.clone());
        }
    }
    here
}

/// Turns board changes into feeds, at most once a second for a Mac and more often for a phone
/// (`feed_every`), and hands the windows the view.
fn feed_loop(inner: Arc<Inner>) {
    let mut st = inner.lock();
    let mut wait = FEEDER_WAIT;
    loop {
        st = inner.wake.wait_timeout(st, wait).map(|(g, _)| g).unwrap_or_else(|e| e.into_inner().0);
        let now = Instant::now();

        let due: Vec<String> = if st.cfg.enabled { feeds_behind(&st).into_iter().filter(|(_, at)| at.is_none_or(|t| t <= now)).map(|(k, _)| k).collect() } else { Vec::new() };
        if !due.is_empty() {
            let boards = st.boards;
            drop(st);
            let board = (inner.hooks.board)();
            st = inner.lock();
            let snap = peerfeed::snapshot(&board, inner.device_info(&st, &board));
            send_feeds(&mut st, &snap, &due, boards, now);
        }

        if (!st.streams.is_empty() || !st.sides.is_empty()) && st.streamed_at.is_none_or(|t| now.duration_since(t) >= STREAM_EVERY) {
            st.streamed_at = Some(now);
            let streams: Vec<codealong_peer::api::Stream> = st.streams.drain().map(|(_, s)| s).collect();
            let sides = std::mem::take(&mut st.sides);
            let linked: HashSet<String> = st.linked.iter().map(|l| l.key.clone()).collect();
            for (key, conn) in st.conns.iter() {
                if !linked.contains(key) {
                    continue;
                }
                for s in streams.iter().filter(|s| conn.watching.contains(&s.key)) {
                    let _ = conn.tx.send(Msg::Stream(s.clone()));
                }
                let Some(asked) = st.sides_asked.get(key).filter(|_| conn.knows.iter().any(|k| k == peerproto::SIDES)) else { continue };
                for s in sides.iter().filter(|s| asked.contains(&s.key)) {
                    let _ = conn.tx.send(Msg::Side(s.clone()));
                }
            }
        }

        if st.view_dirty && st.view_at.is_none_or(|t| now.duration_since(t) >= VIEW_EVERY) {
            st.view_dirty = false;
            st.view_at = Some(now);
            let view = inner.view(&st);
            drop(st);
            (inner.hooks.emit)(&view);
            st = inner.lock();
        }

        if st.cache_dirty && st.cached_at.is_none_or(|t| now.duration_since(t) >= CACHE_EVERY) {
            st.cache_dirty = false;
            st.cached_at = Some(now);
            let cache: HashMap<String, Cached> = st.replicas.iter().filter(|(k, _)| Inner::is_linked(&st, k)).map(|(k, v)| (k.clone(), v.clone())).collect();
            drop(st);
            save_cache(&inner.dir, &cache);
            st = inner.lock();
        }

        // Woken when the next feed is due, rather than at the next look after it.
        let next = if st.cfg.enabled { feeds_behind(&st).into_iter().filter_map(|(_, at)| at).min() } else { None };
        wait = next.map_or(FEEDER_WAIT, |t| t.saturating_duration_since(Instant::now()).clamp(Duration::from_millis(5), FEEDER_WAIT));
    }
}

/// The linked devices that heard back and whose last feed is from an earlier board, each with
/// when its next may go (None: now).
fn feeds_behind(st: &State) -> Vec<(String, Option<Instant>)> {
    st.conns
        .iter()
        .filter(|(k, c)| c.heard && c.fed_board != st.boards && Inner::is_linked(st, k))
        .map(|(k, c)| (k.clone(), feed_due(st.board_at, c.fed_at, feed_every(c.kind.as_deref()))))
        .collect()
}

/// When a device's next feed may go, None for now: once the board has settled, and `every`
/// after the last one that went to it.
fn feed_due(board_at: Option<Instant>, fed_at: Option<Instant>, every: Duration) -> Option<Instant> {
    board_at.map(|t| t + SETTLE).max(fed_at.map(|t| t + every))
}

/// How often a device of this kind, as its hello said, is sent a feed.
fn feed_every(kind: Option<&str>) -> Duration {
    if peerproto::watches(kind) {
        WATCHER_FEED_EVERY
    } else {
        FEED_EVERY
    }
}

/// Sends `snap`, cut from board `boards`, to the devices in `to`: each what changed since its
/// last feed. One that had nothing new can have the next one as soon as it's due.
fn send_feeds(st: &mut State, snap: &Snapshot, to: &[String], boards: u64, now: Instant) {
    let linked: HashSet<String> = st.linked.iter().map(|l| l.key.clone()).collect();
    for key in to {
        let Some(conn) = st.conns.get_mut(key).filter(|c| c.heard && linked.contains(key)) else { continue };
        conn.fed_board = boards;
        if let Some(msg) = conn.outbox.next(snap) {
            // By its name and short id, as devices.log names it: two devices can share a name.
            if crate::perf::enabled() {
                crate::perf::log(&peerfeed::perf_line(&format!("{} ({})", conn.name, short_of(key)), &msg));
            }
            let _ = conn.tx.send(Msg::Feed(msg));
            conn.fed_at = Some(now);
        }
    }
}

/// One connection, from the hello to the end. `ask_link` asks to link as soon as the proofs
/// check out, with a link code's secret when there is one.
#[allow(clippy::too_many_arguments)]
fn run(inner: Arc<Inner>, wire: Wire, via: Via, dialed: bool, expect: Option<PublicKey>, peer_user: Option<String>, addr: Option<IpAddr>, ask_link: Option<Option<String>>, gen: u64) {
    // A person on the other device waits on what comes through here.
    crate::os::system::user_initiated_thread();
    let Wire { reader, mut writer, set_timeout, close } = wire;
    let close: Arc<dyn Fn() + Send + Sync> = Arc::from(close);
    let Some(me) = inner.me.get() else {
        close();
        return;
    };
    let (queue, rx) = sync_channel::<Going>(QUEUE_MAX);
    let closer = Closer::new(close.clone());
    let tx = Out::new(queue, closer.clone());
    // This connection's requests running now (`REQUESTS_MAX`).
    let running = Arc::new(AtomicUsize::new(0));
    {
        let (close, closer) = (close.clone(), closer.clone());
        let waiting = tx.waiting.clone();
        std::thread::Builder::new()
            .name("peer-writer".into())
            .spawn(move || {
                loop {
                    let (line, counted) = match rx.recv_timeout(PING_EVERY) {
                        Ok(Going::Msg(m)) => (m.line(), false),
                        Ok(Going::Line(line)) => (line, true),
                        Err(RecvTimeoutError::Timeout) => (Msg::Ping.line(), false),
                        Err(RecvTimeoutError::Disconnected) => break,
                    };
                    let written = writer.write_all(line.as_bytes()).and_then(|_| writer.flush());
                    // An answer's bytes stop waiting once it's written.
                    if counted {
                        waiting.fetch_sub(line.len(), Ordering::SeqCst);
                    }
                    if let Err(e) = written {
                        closer.close(format!("writing to it failed: {e}"));
                        break;
                    }
                }
                close();
            })
            .ok();
    }
    let mut reader = BufReader::with_capacity(64 * 1024, reader);
    set_timeout(Some(HANDSHAKE_TIMEOUT));
    let my_nonce = peerproto::nonce();
    let my_name = inner.my_name(&inner.lock());
    let _ = tx.send(Msg::Hello { v: VERSION, key: me.hex.clone(), name: my_name, app: APP.into(), nonce: my_nonce.clone(), kind: Some(peerproto::MAC.into()), knows: peerproto::KNOWS.map(String::from).to_vec() });
    let shook = (|| -> std::result::Result<(PublicKey, String, String, Option<String>, Vec<String>), String> {
        let Ok(Some(Msg::Hello { v, key, name, app, nonce, kind, knows })) = peerproto::read_msg(&mut reader) else { return Err("no hello".into()) };
        if v != VERSION {
            return Err(format!("{name} speaks version {v} of Codealong's protocol; update Codealong on both devices."));
        }
        let key = peerproto::parse_key(&key).map_err(|e| e.to_string())?;
        if key == me.key || expect.is_some_and(|e| e != key) {
            return Err("not the device expected".into());
        }
        let _ = tx.send(Msg::Proof { sig: peerproto::prove(&me.secret, &nonce, &key).map_err(|e| e.to_string())? });
        let Ok(Some(Msg::Proof { sig })) = peerproto::read_msg(&mut reader) else { return Err("no proof".into()) };
        peerproto::check_proof(&key, &sig, &my_nonce, &me.key).map_err(|e| e.to_string())?;
        Ok((key, name, app, kind, knows))
    })();
    let Ok((key, name, app, kind, knows)) = shook else {
        drop(tx);
        close();
        return;
    };
    set_timeout(Some(IDLE_TIMEOUT));
    let hex = peerproto::key_hex(&key);
    let serial = inner.ids.fetch_add(1, Ordering::SeqCst);
    {
        let mut st = inner.lock();
        if inner.gen.load(Ordering::SeqCst) != gen || !st.cfg.enabled {
            drop(st);
            drop(tx);
            close();
            return;
        }
        if let Some(old) = st.conns.get(&hex) {
            // Both devices may dial at once. Each keeps the connection the lower key dialed, so
            // both keep the same one; two dialed by the same device, the newer.
            let keep_new = old.dialed == dialed || (dialed == (me.key.as_bytes() < key.as_bytes()));
            if !keep_new {
                drop(st);
                drop(tx);
                close();
                return;
            }
            old.close.close(format!("a newer connection to it took its place, {}", via_words(via)));
        }
        let linked = Inner::is_linked(&st, &hex);
        if linked {
            if st.linked.iter().any(|l| l.key == hex && (l.name != name || l.kind != kind)) {
                // Only its name and kind as it says them now: one that can't be saved is a
                // notice, and the old ones do meanwhile.
                let _ = inner.change_linked(&mut st, |linked| {
                    if let Some(l) = linked.iter_mut().find(|l| l.key == hex) {
                        l.name = name.clone();
                        l.kind = kind.clone();
                    }
                });
            }
            st.boards += 1;
            st.board_at = None;
        }
        // What it doesn't know any more, back on another build, goes from the copy kept of it.
        if let Some(c) = st.replicas.get_mut(&hex).filter(|_| linked) {
            c.replica.heard_from(&knows);
        }
        // A device that knows how gets only what changed since its copy of this one's last feed.
        let resume = (linked && knows.iter().any(|k| k == peerproto::HAVE)).then(|| st.replicas.get(&hex).map(|c| c.replica.have())).flatten().filter(|h| !h.is_empty());
        st.conns.insert(hex.clone(), Conn { serial, dialed, via, peer_user, addr, tx: tx.clone(), close: closer.clone(), name: name.clone(), app, kind, watching: HashSet::new(), heard: false, linked_there: false, since: now_ms(), outbox: Outbox::for_device(&knows), knows, fed_board: 0, fed_at: None });
        st.view_dirty = true;
        if let Some(lanes) = resume {
            let _ = tx.send(Msg::Have { lanes });
        }
        let _ = tx.send(Msg::Standing { linked });
        if let Some(secret) = ask_link {
            st.asking.insert(hex.clone());
            let _ = tx.send(Msg::LinkAsk { secret });
        }
    }
    inner.wake.notify_all();

    let opened = Instant::now();
    let ended = loop {
        match peerproto::read_msg(&mut reader) {
            Ok(Some(msg)) => handle(&inner, &hex, serial, &tx, &running, msg),
            Ok(None) => break None,
            Err(e) => break Some(e),
        }
    };

    let mut st = inner.lock();
    let linked = Inner::is_linked(&st, &hex);
    if st.conns.get(&hex).is_some_and(|c| c.serial == serial) {
        st.conns.remove(&hex);
        st.asks.retain(|a| a.key != hex);
        st.asking.remove(&hex);
        // A phone sends no feed, so all that's kept for it is when it was last here.
        if linked {
            st.replicas.entry(hex.clone()).or_default().last_seen = now_ms();
            st.cache_dirty = true;
        }
        let gone: Vec<u64> = st.pending.iter().filter(|(_, (k, _))| *k == hex).map(|(id, _)| *id).collect();
        for id in gone {
            if let Some((_, s)) = st.pending.remove(&id) {
                let _ = s.send(Err(format!("{name} went away.")));
            }
        }
        st.view_dirty = true;
    }
    drop(st);
    inner.wake.notify_all();
    // Only a linked device's: one on your tailnet that isn't keeps a connection only to be found.
    if linked {
        log_closed(&name, &hex, via, opened.elapsed(), &closer.why().unwrap_or_else(|| ended_why(ended.as_ref())));
    }
    drop(tx);
    close();
}

/// Why a connection ended that this device didn't close, from how its reader stopped.
fn ended_why(e: Option<&std::io::Error>) -> String {
    use std::io::ErrorKind;
    match e {
        None => "it closed the connection".into(),
        // A read timeout: TCP's says it would block, iroh's that it timed out (codealong-peer's net.rs).
        Some(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => format!("nothing came from it for {} s", IDLE_TIMEOUT.as_secs()),
        Some(e) => format!("reading from it failed: {e}"),
    }
}

/// The way a connection goes, for words about it.
fn via_words(via: Via) -> &'static str {
    match via {
        Via::Tailscale => "over Tailscale",
        Via::Iroh => "through iroh",
        Via::Test => "in a test",
    }
}

/// How long a connection was open, for its line in devices.log: 12 s, 3 min 4 s, 2 h 5 min.
fn open_for(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..=59 => format!("{s} s"),
        60..=3599 => format!("{} min {} s", s / 60, s % 60),
        _ => format!("{} h {} min", s / 3600, s % 3600 / 60),
    }
}

fn handle(inner: &Arc<Inner>, key: &str, serial: u64, tx: &Out, running: &Arc<AtomicUsize>, msg: Msg) {
    // A connection that was replaced or closed is on its way out: it's owed nothing.
    if !inner.lock().conns.get(key).is_some_and(|c| c.serial == serial) {
        return;
    }
    match msg {
        Msg::Feed(f) => {
            let mut st = inner.lock();
            if Inner::is_linked(&st, key) {
                let c = st.replicas.entry(key.to_string()).or_default();
                c.replica.apply(f);
                c.last_seen = now_ms();
                c.fed_at = c.last_seen;
                st.view_dirty = true;
                st.cache_dirty = true;
            }
        }
        // Which sessions' replies this connection wants as they're written: its own business,
        // so it's kept here rather than asked of the engine.
        Msg::Req { id, method, params } if method == codealong_peer::api::SESSION_WATCH => {
            let mut st = inner.lock();
            let linked = Inner::is_linked(&st, key);
            let reply = match st.conns.get_mut(key) {
                Some(c) if linked => {
                    c.watching = codealong_peer::api::params::Keys::deserialize(&params).map(|p| p.keys.into_iter().collect()).unwrap_or_default();
                    Msg::Res { id, ok: true, result: None, error: None }
                }
                _ => Msg::Res { id, ok: false, result: None, error: Some(format!("{} isn't linked with this device.", inner.my_name(&st))) },
            };
            let _ = tx.send(reply);
            return;
        }
        Msg::Req { id, method, params } => {
            // Proving its key opens a connection for any device, so a device that isn't linked
            // is answered here, before anything is started for it (`answer` checks again, for
            // one unlinked while its request waited).
            let stranger = {
                let st = inner.lock();
                (!Inner::is_linked(&st, key)).then(|| inner.my_name(&st))
            };
            if let Some(me) = stranger {
                let _ = tx.send(Msg::Res { id, ok: false, result: None, error: Some(format!("{me} isn't linked with this device.")) });
                return;
            }
            if running.fetch_add(1, Ordering::SeqCst) >= REQUESTS_MAX {
                running.fetch_sub(1, Ordering::SeqCst);
                tx.close.close(format!("it had more than {REQUESTS_MAX} requests running here at once"));
                return;
            }
            let place = Running(running.clone());
            let (inner, key, out) = (inner.clone(), key.to_string(), tx.clone());
            #[cfg(test)]
            inner.requests.fetch_add(1, Ordering::SeqCst);
            let started = std::thread::Builder::new().name("peer-request".into()).spawn(move || {
                crate::os::system::user_initiated_thread();
                let reply = match inner.answer(&key, &method, params) {
                    Ok(v) => Msg::Res { id, ok: true, result: Some(v), error: None },
                    Err(e) => Msg::Res { id, ok: false, result: None, error: Some(e) },
                };
                // Its place is kept until the answer is in words and counted (`Out::answer`), so
                // no more requests run than may while big answers are being written.
                let _ = out.answer(reply);
                drop(place);
            });
            if started.is_err() {
                let _ = tx.send(Msg::Res { id, ok: false, result: None, error: Some("This device is too busy to answer; try again in a moment.".into()) });
            }
            return;
        }
        Msg::Res { id, ok, result, error } => {
            let mut st = inner.lock();
            if st.pending.get(&id).is_some_and(|(k, _)| k == key) {
                if let Some((_, s)) = st.pending.remove(&id) {
                    let _ = s.send(if ok { Ok(result.unwrap_or(Value::Null)) } else { Err(error.unwrap_or_else(|| "The other device said no.".into())) });
                }
            }
            return;
        }
        Msg::Standing { linked } => {
            let mut st = inner.lock();
            if let Some(c) = st.conns.get_mut(key) {
                c.linked_there = linked;
                c.heard = true;
            }
            // Its first feed was waiting for this.
            if Inner::is_linked(&st, key) {
                st.boards += 1;
                st.board_at = None;
            }
            st.view_dirty = true;
        }
        Msg::LinkAsk { secret } => {
            let mut st = inner.lock();
            let Some(conn) = st.conns.get(key) else { return };
            let (name, via, theirs) = (conn.name.clone(), conn.via, conn.peer_user.clone());
            let accept = if Inner::is_linked(&st, key) {
                Some(true)
            } else if let Some(secret) = secret {
                // A link code made here, used once, in time.
                let now = now_ms();
                st.codes.retain(|(_, until)| *until > now);
                let found = st.codes.iter().position(|(s, _)| *s == secret);
                if let Some(i) = found {
                    st.codes.remove(i);
                }
                Some(found.is_some())
            } else if theirs.is_some() && theirs == st.my_user {
                // Your own device on your tailnet: you answer here (`answer_link`).
                let code = match (inner.me.get(), peerproto::parse_key(key)) {
                    (Some(m), Ok(k)) => peerproto::link_code(&m.key, &k),
                    _ => String::new(),
                };
                st.asks.retain(|a| a.key != key);
                st.asks.push(Ask { key: key.to_string(), name: name.clone(), code });
                st.view_dirty = true;
                None
            } else {
                Some(false)
            };
            if let Some(accepted) = accept {
                // A link this device can't keep is answered no (the notice says why).
                let accepted = accepted && inner.add_linked(&mut st, key, &name, via).is_ok();
                let _ = tx.send(Msg::LinkAnswer { accepted });
                if accepted {
                    let _ = tx.send(Msg::Standing { linked: true });
                }
            }
        }
        Msg::LinkAnswer { accepted } => {
            let mut st = inner.lock();
            if st.asking.remove(key) {
                if accepted {
                    let (name, via) = st.conns.get(key).map(|c| (c.name.clone(), c.via)).unwrap_or(("device".into(), Via::Test));
                    match inner.add_linked(&mut st, key, &name, via) {
                        Ok(()) => {
                            let _ = tx.send(Msg::Standing { linked: true });
                        }
                        Err(e) => {
                            // It said yes, but this device can't keep the link: it's told to
                            // forget this one too, so neither trusts the other.
                            let _ = tx.send(Msg::Unlinked);
                            st.link_failed.insert(key.to_string(), format!("{name} isn't linked: {e}"));
                        }
                    }
                }
                st.view_dirty = true;
            }
        }
        Msg::Unlinked => {
            let mut st = inner.lock();
            if Inner::is_linked(&st, key) {
                // One that can't be taken off the list stays on it, as devices.json has it, and
                // the notice says why.
                let _ = inner.forget(&mut st, key);
            }
            if let Some(c) = st.conns.get_mut(key) {
                c.linked_there = false;
                c.outbox.restart();
            }
        }
        Msg::Ping => {
            let _ = tx.send(Msg::Pong);
            return;
        }
        Msg::Have { lanes } => {
            let mut st = inner.lock();
            if Inner::is_linked(&st, key) {
                if let Some(c) = st.conns.get_mut(key) {
                    c.outbox.resume(lanes);
                }
            }
            return;
        }
        // Another Mac's replies aren't shown here yet: its panel reads the conversation.
        Msg::Stream(_) => return,
        // A side conversation asked about here, of a session there.
        Msg::Side(mut side) => {
            let linked = Inner::is_linked(&inner.lock(), key);
            if linked {
                side.key = peerfeed::remote_key(short_of(key), &side.key);
                (inner.hooks.side)(side);
            }
            return;
        }
        Msg::Pong | Msg::Hello { .. } | Msg::Proof { .. } | Msg::Unknown => return,
    }
    inner.wake.notify_all();
}

/// Finds your devices on your tailnet, listens there, and dials the ones not connected.
fn tailnet_loop(inner: Arc<Inner>, gen: u64) {
    crate::os::system::utility_thread();
    let mut listener: Option<(IpAddr, u16, std::net::TcpListener)> = None;
    let mut first_seen: HashMap<IpAddr, Instant> = HashMap::new();
    let dialing: Arc<Mutex<HashSet<IpAddr>>> = Arc::new(Mutex::new(HashSet::new()));
    let mut looked: Option<Instant> = None;
    while inner.gen.load(Ordering::SeqCst) == gen {
        if looked.is_none_or(|t| t.elapsed() >= TAILNET_EVERY) {
            looked = Some(Instant::now());
            let port = inner.lock().cfg.port;
            match tailnet::status() {
                Ok(s) => {
                    let ip = s.my_ipv4().filter(|_| s.running);
                    // Listen on this device's tailnet address, and follow it if it changes.
                    if listener.as_ref().map(|(i, p, _)| (*i, *p)) != ip.map(|i| (i, port)) {
                        listener = None;
                        if let Some(ip) = ip {
                            match tailnet::listen(ip, port) {
                                Ok(l) => listener = Some((ip, port, l)),
                                Err(e) => inner.lock().tailnet_error = Some(format!("{e:#}")),
                            }
                        }
                    }
                    let mut st = inner.lock();
                    st.listening = listener.as_ref().map(|(i, p, _)| format!("{i}:{p}"));
                    if listener.is_some() {
                        st.tailnet_error = None;
                    }
                    st.my_user = Some(s.me.user.clone()).filter(|u| !u.is_empty());
                    let connected: HashSet<IpAddr> = st.conns.values().filter_map(|c| c.addr).collect();
                    drop(st);
                    let mine = s.my_devices();
                    first_seen.retain(|ip, _| mine.iter().any(|(_, i)| i == ip));
                    for (node, peer_ip) in mine {
                        let seen = *first_seen.entry(peer_ip).or_insert_with(Instant::now);
                        let lower = ip.is_some_and(|me| ip_before(me, peer_ip));
                        if connected.contains(&peer_ip) || !(lower || seen.elapsed() >= DIAL_ANYWAY_AFTER) {
                            continue;
                        }
                        if !dialing.lock().map(|mut d| d.insert(peer_ip)).unwrap_or(false) {
                            continue;
                        }
                        let (inner, dialing, user) = (inner.clone(), dialing.clone(), node.user.clone());
                        std::thread::spawn(move || {
                            if let Ok(w) = tailnet::dial(peer_ip, port).and_then(Wire::tcp) {
                                // `run` returns when the connection ends; the address is free to
                                // dial again as soon as it's connected, since that's checked.
                                if let Ok(mut d) = dialing.lock() {
                                    d.remove(&peer_ip);
                                }
                                run(inner, w, Via::Tailscale, true, None, Some(user), Some(peer_ip), None, gen);
                            } else if let Ok(mut d) = dialing.lock() {
                                d.remove(&peer_ip);
                            }
                        });
                    }
                    let mut st = inner.lock();
                    st.tailnet = Some(s);
                    st.view_dirty = true;
                }
                Err(e) => {
                    listener = None;
                    let mut st = inner.lock();
                    st.tailnet = None;
                    st.listening = None;
                    st.tailnet_error = Some(format!("{e:#}"));
                    st.view_dirty = true;
                }
            }
            inner.wake.notify_all();
        }
        if let Some((_, _, l)) = &listener {
            while let Ok((stream, from)) = l.accept() {
                let inner = inner.clone();
                std::thread::spawn(move || accept_tailnet(inner, stream, from.ip(), gen));
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// A connection from the tailnet: only from a machine of yours.
fn accept_tailnet(inner: Arc<Inner>, stream: TcpStream, from: IpAddr, gen: u64) {
    let Ok(who) = tailnet::whois(from) else { return };
    let mine = inner.lock().my_user.clone();
    if mine.as_deref() != Some(who.user.as_str()) {
        return;
    }
    if let Ok(w) = Wire::tcp(stream) {
        run(inner, w, Via::Tailscale, false, None, Some(who.user), Some(from), None, gen);
    }
}

/// The name you gave this device in System Settings › General › Sharing.
fn computer_name() -> String {
    crate::procs::run("/usr/sbin/scutil", &["--get", "ComputerName"], None, Duration::from_secs(3)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "This device".into())
}

fn ip_before(a: IpAddr, b: IpAddr) -> bool {
    match (a, b) {
        (IpAddr::V4(a), IpAddr::V4(b)) => a.octets() < b.octets(),
        (a, b) => a.to_string() < b.to_string(),
    }
}

/// Starts iroh, takes connections from it, and dials linked devices that aren't connected.
fn iroh_loop(inner: Arc<Inner>, gen: u64) {
    let Some(me) = inner.me.get() else { return };
    inner.lock().iroh = "starting".into();
    inner.lock().view_dirty = true;
    inner.wake.notify_all();
    let iroh = match Iroh::start(me.secret.clone()) {
        Ok(i) => Arc::new(i),
        Err(e) => {
            inner.lock().iroh = format!("{e:#}");
            inner.lock().view_dirty = true;
            inner.wake.notify_all();
            return;
        }
    };
    if inner.gen.load(Ordering::SeqCst) != gen {
        iroh.close();
        return;
    }
    if let Ok(mut slot) = inner.iroh.lock() {
        *slot = Some(iroh.clone());
    }
    {
        let (inner, iroh) = (inner.clone(), iroh.clone());
        std::thread::Builder::new()
            .name("peers-iroh-accept".into())
            .spawn(move || {
                while let Some(incoming) = iroh.accept() {
                    if inner.gen.load(Ordering::SeqCst) != gen {
                        break;
                    }
                    let (inner, iroh) = (inner.clone(), iroh.clone());
                    std::thread::spawn(move || {
                        if let Ok((key, wire)) = iroh.finish(incoming) {
                            run(inner, wire, Via::Iroh, false, Some(key), None, None, None, gen);
                        }
                    });
                }
            })
            .ok();
    }
    let online = iroh.online(Duration::from_secs(20));
    inner.lock().iroh = if online { "online".into() } else { "offline".into() };
    inner.lock().view_dirty = true;
    inner.wake.notify_all();

    // Linked devices that aren't connected, tried through iroh after the tailnet has had its turn,
    // then less often each time one doesn't answer, up to every ten minutes.
    let mut wait: HashMap<String, (Instant, Duration)> = HashMap::new();
    let started = Instant::now();
    let dialing: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    while inner.gen.load(Ordering::SeqCst) == gen {
        std::thread::sleep(Duration::from_secs(1));
        if started.elapsed() < IROH_FIRST_DIAL {
            continue;
        }
        let todo: Vec<String> = {
            let st = inner.lock();
            st.linked.iter().filter(|l| !st.conns.contains_key(&l.key) && !l.watches()).map(|l| l.key.clone()).collect()
        };
        for hex in todo {
            if wait.get(&hex).is_some_and(|(at, d)| at.elapsed() < *d) {
                continue;
            }
            let backoff = wait.get(&hex).map(|(_, d)| (*d * 2).min(Duration::from_secs(600))).unwrap_or(IROH_DIAL_EVERY);
            wait.insert(hex.clone(), (Instant::now(), backoff));
            let Ok(key) = peerproto::parse_key(&hex) else { continue };
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
                    run(inner, wire, Via::Iroh, true, Some(key), None, None, None, gen);
                }
            });
        }
        // A device that connected any way starts its wait over.
        let st = inner.lock();
        wait.retain(|k, _| !st.conns.contains_key(k));
    }
}

/// Every request that acts on a session here from another device, what it was, how it went and
/// how long it took (the device stops waiting after a while: codealong-peer's `ACT_WAIT`), in
/// devices.log. The prompt itself isn't written down.
fn audit(from: &str, short: &str, method: &str, key: &str, got: &std::result::Result<Value, String>, took: std::time::Duration) {
    let took = format!("{:.1} s", took.as_secs_f64());
    let outcome = match got {
        Ok(_) => format!("done in {took}"),
        Err(e) => format!("refused after {took}: {e}"),
    };
    log_device(&format!("{} {from} ({short}) {method} {key}: {outcome}\n", crate::perf::stamp()));
}

/// Every connection to a linked device that closed, how long it was open and why, in
/// devices.log: a device that drops out of view here for a moment says why there, and its side
/// of it is in its own devices.log.
fn log_closed(name: &str, hex: &str, via: Via, open: Duration, why: &str) {
    log_device(&format!("{} {name} ({}) connection {} closed after {}: {why}\n", crate::perf::stamp(), short_of(hex), via_words(via), open_for(open)));
}

/// A line in `~/Library/Logs/Codealong/devices.log`. Not from tests, whose devices are their own.
fn log_device(line: &str) {
    if cfg!(test) {
        return;
    }
    let dir = crate::config::logs_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("devices.log")) {
        let _ = f.write_all(line.as_bytes());
    }
}

fn linked_path(dir: &Path) -> PathBuf {
    dir.join("devices.json")
}

fn cache_path(dir: &Path) -> PathBuf {
    dir.join("device-feeds.json")
}

/// `devices.json`, the devices this one trusts.
#[derive(Serialize, Deserialize, Default)]
struct LinkedFile {
    #[serde(default)]
    linked: Vec<Linked>,
}

/// The devices this one trusts. A devices.json that doesn't read trusts none, and is left as it
/// is with a notice, rather than read as none linked and saved over (`store::load_whole`).
fn load_linked(dir: &Path) -> Vec<Linked> {
    crate::store::load_whole::<LinkedFile>(&linked_path(dir), "a list of linked devices").linked
}

/// Writes devices.json; a failure is also a notice (`store::save_whole`).
fn save_linked(dir: &Path, linked: &[Linked]) -> Result<()> {
    crate::store::save_whole(&linked_path(dir), &LinkedFile { linked: linked.to_vec() })
}

/// Read leniently: what each device last said is a cache, filled again as each connects.
fn load_cache(dir: &Path) -> HashMap<String, Cached> {
    crate::store::read_or_default(&cache_path(dir))
}

fn save_cache(dir: &Path, cache: &HashMap<String, Cached>) {
    if let Ok(text) = serde_json::to_string(cache) {
        let _ = crate::store::write(&cache_path(dir), text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Session, Status};
    use serde_json::json;
    use std::net::TcpListener;

    #[test]
    fn a_device_that_leaves_big_answers_waiting_is_let_go() {
        let (tx, rx) = sync_channel::<Going>(QUEUE_MAX);
        let closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let shut = closed.clone();
        let out = Out::new(tx, Closer::new(Arc::new(move || shut.store(true, Ordering::SeqCst))));
        let big = |id| Msg::Res { id, ok: true, result: Some(json!("x".repeat(QUEUE_BYTES_MAX / 2))), error: None };
        out.answer(big(1)).unwrap();
        assert!(out.waiting.load(Ordering::SeqCst) > QUEUE_BYTES_MAX / 2);
        out.send(Msg::Ping).unwrap();
        assert!(!closed.load(Ordering::SeqCst), "half the bytes, and a message that isn't an answer");
        assert!(out.answer(big(2)).is_err());
        assert!(closed.load(Ordering::SeqCst));
        assert_eq!(out.close.why().as_deref(), Some("it fell behind: more than 64 MB of answers waited to go to it"));
        // What was refused isn't counted as waiting.
        assert!(out.waiting.load(Ordering::SeqCst) < QUEUE_BYTES_MAX);
        assert!(matches!(rx.try_recv(), Ok(Going::Line(_))));
    }

    #[test]
    fn a_closed_connection_says_why_by_the_first_reason_given() {
        let closes = Arc::new(AtomicUsize::new(0));
        let counted = closes.clone();
        let closer = Closer::new(Arc::new(move || _ = counted.fetch_add(1, Ordering::SeqCst)));
        assert_eq!(closer.why(), None, "not closed here: its reader says how it ended");
        closer.close("it had more than 32 requests running here at once");
        // Its writer, failing once the connection is shut, closes it again.
        closer.clone().close("writing to it failed: Broken pipe");
        assert_eq!(closer.why().as_deref(), Some("it had more than 32 requests running here at once"));
        assert_eq!(closes.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_connection_closed_over_there_says_how_its_reader_stopped() {
        use std::io::{Error, ErrorKind};
        assert_eq!(ended_why(None), "it closed the connection");
        assert_eq!(ended_why(Some(&Error::from(ErrorKind::WouldBlock))), "nothing came from it for 80 s");
        assert_eq!(ended_why(Some(&Error::new(ErrorKind::TimedOut, "The other device went quiet."))), "nothing came from it for 80 s");
        assert_eq!(ended_why(Some(&Error::new(ErrorKind::ConnectionReset, "reset"))), "reading from it failed: reset");
        assert_eq!((open_for(Duration::from_secs(12)), open_for(Duration::from_secs(184)), open_for(Duration::from_secs(7_500))), ("12 s".into(), "3 min 4 s".into(), "2 h 5 min".into()));
    }

    fn session(key: &str, status: Status) -> Session {
        Session { status, pid: Some(7), status_since: Some(1), last_activity: 1, last_prompt_at: 1, open_target: String::new(), ..crate::fixtures::session(key) }
    }

    fn lane(id: &str, sessions: Vec<Session>) -> Lane {
        Lane { callsign: "c".into(), ..crate::fixtures::lane(id, sessions) }
    }

    struct Device {
        peers: Peers,
        board: Arc<Mutex<Board>>,
        views: Arc<Mutex<Vec<DevicesView>>>,
        asked: Arc<Mutex<Vec<(String, Value)>>>,
        /// The side conversations of other devices' sessions its windows were handed.
        sides: Arc<Mutex<Vec<Side>>>,
        dir: PathBuf,
    }

    fn device(name: &str, lanes: Vec<Lane>) -> Device {
        device_where(name, lanes, |_| {})
    }

    /// A device whose folder `before` gets ready before it starts.
    fn device_where(name: &str, lanes: Vec<Lane>, before: impl FnOnce(&Path)) -> Device {
        // Tests run side by side; each device gets a folder of its own.
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!("codealong-peers-{}-{name}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        before(&dir);
        let cfg = DevicesCfg { enabled: true, name: name.into(), tailscale: false, iroh: false, ..DevicesCfg::default() };
        // The board carries `[devices]`, as the engine's does, so publishing it keeps them on.
        let board = Arc::new(Mutex::new(Board { lanes, devices: cfg.clone(), ..Board::default() }));
        let views = Arc::new(Mutex::new(Vec::new()));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let sides = Arc::new(Mutex::new(Vec::new()));
        let (b, v, a, sd) = (board.clone(), views.clone(), asked.clone(), sides.clone());
        let hooks = Hooks {
            board: Box::new(move || b.lock().unwrap().clone()),
            answer: Box::new(move |method, params, watching| {
                a.lock().unwrap().push((method.to_string(), json!({ "params": params, "watching": watching })));
                match method {
                    "session-page" => Ok(json!({ "items": [], "before": null, "after": 0, "key": params["key"] })),
                    "search" => Ok(json!({ "hits": [{ "key": "claude:claude:s1", "snippet": "the webhook", "you": true, "at": 5 }], "building": false, "off": false })),
                    "session-send" => Ok(json!({ "message": "Sent.", "opened": false })),
                    "session-stream" | "side-get" => Ok(Value::Null),
                    "session-rename" | "mark-seen" | "session-archive" => Ok(Value::Null),
                    // As a Mac does that knows it, asked to end work under way; else as one from
                    // before it does.
                    "session-done" if params["stop"] == true => Ok(json!({ "done": true, "message": "Done with “s2”: its turn is stopped and it's archived.", "noted": false })),
                    "session-done" => Err("Codealong on this device doesn't know how to session-done yet; update it here.".into()),
                    // A request that takes a while, as a side question does.
                    "slow" => {
                        std::thread::sleep(Duration::from_millis(800));
                        Ok(Value::Null)
                    }
                    "start-options" => Ok(json!({ "accounts": [{ "name": "claude", "harness": "claude", "places": [{ "place": "background", "label": "Background" }], "chosen": null, "unavailable": null, "defaultModel": "opus", "preferred": true }], "codexProjects": [], "claudeModels": [{ "id": "opus", "label": "Opus" }] })),
                    _ => Err(format!("no {method}")),
                }
            }),
            emit: Box::new(move |view| v.lock().unwrap().push(view.clone())),
            side: Box::new(move |s| sd.lock().unwrap().push(s)),
        };
        let peers = Peers::with_key(dir.clone(), SecretKey::generate(), hooks);
        peers.configure(&cfg);
        peers.0.lock().my_user = Some("sam@example.com".into());
        Device { peers, board, views, asked, sides, dir }
    }

    /// Joins two devices over a loopback TCP connection, as the tailnet would, both machines Sam's.
    fn join(a: &Device, b: &Device) {
        join_as(a, "sam@example.com", b, "sam@example.com");
    }

    /// Joins two devices, `a` on `a_user`'s machine and `b` on `b_user`'s, as `tailscale whois`
    /// would tell each about the other.
    fn join_as(a: &Device, a_user: &str, b: &Device, b_user: &str) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let dialer = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (accepted, _) = l.accept().unwrap();
        a.peers.attach(Wire::tcp(dialer).unwrap(), Via::Test, true, None, Some(b_user.into()), None);
        b.peers.attach(Wire::tcp(accepted).unwrap(), Via::Test, false, None, Some(a_user.into()), None);
    }

    fn until(what: &str, f: impl Fn() -> bool) {
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end {
            if f() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("timed out waiting for {what}");
    }

    fn key_of(m: &Device) -> String {
        m.peers.0.me.get().unwrap().hex.clone()
    }

    fn link(a: &Device, b: &Device) {
        until("b to be found", || a.peers.view().found.iter().any(|f| f.key == key_of(b)));
        let code = a.peers.link(&key_of(b)).unwrap();
        until("b to be asked", || b.peers.view().asks.iter().any(|x| x.key == key_of(a)));
        assert_eq!(b.peers.view().asks[0].code, code, "both devices show the same code");
        b.peers.answer_link(&key_of(a), true).unwrap();
        until("both linked", || a.peers.view().devices.iter().any(|d| d.online && d.key == key_of(b)) && b.peers.view().devices.iter().any(|d| d.online && d.key == key_of(a)));
    }

    /// The phone's client (codealong-peer's device that only watches) links with a Mac's link
    /// code, gets its feed, reads through the Mac's own functions, and is refused what needs
    /// remote control there.
    /// A loopback connection to `mac`, as a phone's reconnect through iroh would reach it.
    fn reconnect(mac: &Device) -> Wire {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let dialer = TcpStream::connect(("127.0.0.1", l.local_addr().unwrap().port())).unwrap();
        let (accepted, _) = l.accept().unwrap();
        mac.peers.attach(Wire::tcp(accepted).unwrap(), Via::Test, false, None, None, None);
        Wire::tcp(dialer).unwrap()
    }

    #[test]
    fn a_phone_is_fed_sooner_than_a_mac_once_the_board_settles() {
        let t = Instant::now();
        // Nothing went to it yet: once the board settles, or at once when a feed was asked for
        // (a device just linked).
        assert_eq!(feed_due(Some(t), None, FEED_EVERY), Some(t + SETTLE));
        assert_eq!(feed_due(None, None, FEED_EVERY), None);
        // A feed went to it just before the board changed: a Mac waits a second after that one,
        // a phone or a tablet a quarter.
        let fed = t - Duration::from_millis(50);
        assert_eq!(feed_due(Some(t), Some(fed), feed_every(Some(peerproto::MAC))), Some(fed + FEED_EVERY));
        assert_eq!(feed_due(Some(t), Some(fed), feed_every(Some(peerproto::PHONE))), Some(fed + WATCHER_FEED_EVERY));
        assert_eq!(feed_every(Some(peerproto::TABLET)), WATCHER_FEED_EVERY);
        // A device from before hellos said what it is is a Mac.
        assert_eq!(feed_every(None), FEED_EVERY);
    }

    #[test]
    fn a_watching_device_links_with_a_code_and_reads() {
        let mac = device("Mini", vec![lane("/Users/sam/app", vec![session("claude:claude:s2", Status::Needs)])]);
        // A link code as Settings › Devices makes one, without starting iroh.
        let secret = "00112233445566778899".to_string();
        mac.peers.0.lock().codes.push((secret.clone(), now_ms() + LINK_CODE_FOR_MS));
        let code = Ticket { key: mac.peers.0.me.get().unwrap().key, secret }.encode();

        let dir = mac.dir.with_extension("phone");
        let phone = codealong_peer::client::Client::new(dir.clone(), SecretKey::generate(), "iPhone".into(), "0.1.0".into(), peerproto::PHONE, Box::new(|| {}));
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let dialer = TcpStream::connect(("127.0.0.1", l.local_addr().unwrap().port())).unwrap();
        let (accepted, _) = l.accept().unwrap();
        // The Mac takes it as it takes a connection through iroh: from no tailnet user.
        mac.peers.attach(Wire::tcp(accepted).unwrap(), Via::Test, false, None, None, None);
        assert_eq!(phone.link_wire(&code, Wire::tcp(dialer).unwrap()).unwrap(), "Mini");

        // Listed as a phone, apart from the devices whose boards the views show.
        until("the Mac to list the phone", || mac.peers.view().phones.iter().any(|d| d.name == "iPhone" && d.online && d.kind == "phone"));
        assert!(mac.peers.view().devices.is_empty());
        assert!(mac.peers.0.lock().linked[0].watches(), "devices.json remembers it's a phone");
        until("the Mac's feed", || phone.with_feeds(|f| f.first().is_some_and(|(_, r)| r.lanes.len() == 1)));
        let d = phone.devices().remove(0);
        assert!(d.online && d.linked_back && !d.remote_control, "{d:?}");
        assert_eq!(d.name, "Mini");
        assert_eq!(phone.with_feeds(|f| f[0].1.lanes[0].sessions[0].status), Status::Needs);

        let page = phone.page(&d.short, "claude:claude:s2", None).unwrap();
        assert!(page.items.is_empty() && page.before.is_none());
        assert_eq!(mac.asked.lock().unwrap().last().unwrap().1["params"]["key"], "claude:claude:s2");
        let refused = phone.send(&d.short, "claude:claude:s2", "hi", false).unwrap_err();
        assert!(refused.contains("Remote control is off on Mini"), "{refused}");
        // Naming a session changes how it's listed, not what its agent does: it needs no remote
        // control.
        phone.rename(&d.short, "claude:claude:s2", "Fix the build").unwrap();
        // Asked by a phone, which runs no agents: what it sends is its user's own.
        assert_eq!(*mac.asked.lock().unwrap().last().unwrap(), ("session-rename".to_string(), json!({ "params": { "key": "claude:claude:s2", "name": "Fix the build" }, "watching": true })));
        // Where it could start a session reads without remote control; starting one needs it.
        let options = phone.start_options(&d.short).unwrap();
        assert_eq!((options.accounts[0].name.as_str(), options.accounts[0].places[0].place), ("claude", codealong_peer::api::Place::Background));
        let spec = codealong_peer::api::StartSpec { lane_id: "/Users/sam/app".into(), account: "claude".into(), place: codealong_peer::api::Place::Background, prompt: "Fix the build".into(), model: None, effort: None, worktree: false, todos: vec![], from: None, helper: false, report: false };
        assert!(phone.start_session(&d.short, &spec).unwrap_err().contains("Remote control is off on Mini"));
        // A phone has nothing to search, so a search on the Mac doesn't ask it.
        assert!(mac.peers.search(&["webhook".into()]).is_empty());

        // Gone, it's remembered with when it was last here.
        assert_eq!(phone.with_feeds(|f| f[0].1.rev), 1, "one whole feed so far");
        phone.stop();
        until("the Mac to see the phone go", || mac.peers.view().phones.first().is_some_and(|p| !p.online && p.last_seen.is_some()));

        // Back after the Mac changed: it picks up where its copy left off, rather than taking
        // everything again.
        mac.board.lock().unwrap().lanes[0].sessions[0].status = Status::Unread;
        mac.peers.publish(&mac.board.lock().unwrap());
        phone.connect_wire(reconnect(&mac), mac.peers.0.me.get().unwrap().key);
        until("the change to arrive", || phone.with_feeds(|f| f[0].1.lanes.first().is_some_and(|l| l.sessions[0].status == Status::Unread)));
        assert_eq!(phone.with_feeds(|f| f[0].1.rev), 2, "only what changed, after its Have");

        // A reply being written reaches the phone watching its session, and goes with the turn.
        assert!(phone.watch(&d.short, &["claude:claude:s2".to_string()]));
        mac.peers.stream(codealong_peer::api::Stream { key: "claude:claude:s2".into(), text: "Looking at the build".into(), tool: None });
        until("the reply", || phone.stream(&d.short, "claude:claude:s2").is_some_and(|s| s.text == "Looking at the build"));
        mac.peers.stream(codealong_peer::api::Stream { key: "claude:claude:s2".into(), text: String::new(), tool: None });
        until("the turn's end", || phone.stream(&d.short, "claude:claude:s2").is_none());
        // Stopping a turn acts on the session, so it needs remote control there, and so does
        // being done with it, which closes its terminal.
        assert!(phone.cancel(&d.short, "claude:claude:s2").unwrap_err().contains("Remote control is off on Mini"));
        assert!(phone.done(&d.short, "claude:claude:s2", false).unwrap_err().contains("Remote control is off on Mini"));

        // A Mac that forgot the phone without saying so (its list edited): the phone still has
        // it, and a new code links them again.
        {
            let mut st = mac.peers.0.lock();
            let key = st.linked[0].key.clone();
            mac.peers.0.forget(&mut st, &key).unwrap();
        }
        phone.connect_wire(reconnect(&mac), mac.peers.0.me.get().unwrap().key);
        until("the phone to hear it's forgotten", || phone.devices().first().is_some_and(|d| d.online && !d.linked_back));
        let secret = "99887766554433221100".to_string();
        mac.peers.0.lock().codes.push((secret.clone(), now_ms() + LINK_CODE_FOR_MS));
        let again = Ticket { key: mac.peers.0.me.get().unwrap().key, secret }.encode();
        assert_eq!(phone.link_wire(&again, reconnect(&mac)).unwrap(), "Mini");
        until("linked again on both sides", || mac.peers.view().phones.first().is_some_and(|p| p.online) && phone.devices().first().is_some_and(|d| d.linked_back));

        // Unlinking on the phone unlinks the Mac too.
        phone.unlink(&d.key).unwrap();
        until("the Mac to forget the phone", || mac.peers.view().phones.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_watching_device_follows_the_side_conversations_it_asks_about() {
        use codealong_peer::api::SideTurn;
        let mac = device("Mini", vec![lane("/Users/sam/app", vec![session("claude:claude:s2", Status::Needs)])]);
        let secret = "00112233445566778899".to_string();
        mac.peers.0.lock().codes.push((secret.clone(), now_ms() + LINK_CODE_FOR_MS));
        let code = Ticket { key: mac.peers.0.me.get().unwrap().key, secret }.encode();
        let dir = mac.dir.with_extension("phone");
        let phone = codealong_peer::client::Client::new(dir.clone(), SecretKey::generate(), "iPhone".into(), "0.1.0".into(), peerproto::PHONE, Box::new(|| {}));
        assert_eq!(phone.link_wire(&code, reconnect(&mac)).unwrap(), "Mini");
        let d = phone.devices().remove(0);
        let side = |answer: &str| Side { id: "7-1".into(), key: "claude:claude:s2".into(), turns: vec![SideTurn { question: "Why?".into(), answer: answer.into(), ..Default::default() }], running: true, ..Default::default() };

        // Nothing goes to it before it asks about them; read, it has none open.
        mac.peers.side(&side("Not yet"));
        assert_eq!(phone.side_get(&d.short, "claude:claude:s2").unwrap(), None);
        assert_eq!(phone.side(&d.short, "claude:claude:s2"), None);
        // From then on each change goes, as the Mac's windows are told.
        mac.peers.side(&side("Because"));
        until("the answer as it's written", || phone.side(&d.short, "claude:claude:s2").is_some_and(|s| s.turns[0].answer == "Because"));
        // Closed while its copy answers, it's read by its id until it has answered.
        mac.peers.side(&Side { key: "claude:claude:s2".into(), ..Side::default() });
        mac.peers.side(&Side { closed: true, ..side("Because it") });
        until("the close", || phone.side_closing(&d.short, "7-1").is_some_and(|s| s.running));
        assert_eq!(phone.side(&d.short, "claude:claude:s2"), None);
        mac.peers.side(&Side { closed: true, running: false, ..side("Because it is.") });
        until("the answer", || phone.side_closing(&d.short, "7-1").is_some_and(|s| !s.running));
        // Asking acts as the session may, so it needs remote control there.
        assert!(phone.side_ask(&d.short, "claude:claude:s2", "And then?", None).unwrap_err().contains("Remote control is off on Mini"));
        phone.unlink(&d.key).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn linked_macs_share_their_boards_and_answer_each_other() {
        let a = device("Book", vec![lane("/a", vec![session("claude:claude:s1", Status::Working)])]);
        let b = device("Mini", vec![lane("/Users/sam/app", vec![session("claude:claude:s2", Status::Needs)])]);
        join(&a, &b);
        // Found on the tailnet, but nothing is shared before they're linked.
        until("a to find b", || a.peers.view().found.len() == 1);
        assert!(a.peers.view().lanes.is_empty() && a.peers.view().devices.is_empty());
        link(&a, &b);

        // Each gets the other's worktrees, with keys that name the device they're on.
        let b_short = short_of(&key_of(&b)).to_string();
        until("a to get b's lanes", || a.peers.view().lanes.len() == 1);
        let lanes = a.peers.view().lanes;
        assert_eq!(lanes[0].id, format!("@{b_short}:/Users/sam/app"));
        assert_eq!(lanes[0].sessions[0].key, format!("@{b_short}:claude:claude:s2"));
        until("b to get a's lanes", || b.peers.view().lanes.len() == 1);
        assert_eq!(a.peers.view().devices[0].name, "Mini");

        // A change on the Mini shows on the MacBook.
        b.board.lock().unwrap().lanes[0].sessions[0].status = Status::Unread;
        b.peers.publish(&b.board.lock().unwrap());
        until("the change to arrive", || a.peers.view().lanes.first().is_some_and(|l| l.sessions[0].status == Status::Unread));

        // Asking the Mini runs its own function, with its own key.
        let page = a.peers.ask(&b_short, "session-page", json!({ "key": "claude:claude:s2" }), Duration::from_secs(5)).unwrap();
        assert_eq!(page["key"], "claude:claude:s2");
        let found = a.peers.search(&["webhook".into()]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].found.as_ref().unwrap().hits[0].key, format!("@{b_short}:claude:claude:s1"));

        // Acting on a session needs remote control on at the device asked.
        let refused = a.peers.ask(&b_short, "session-send", json!({ "key": "claude:claude:s2", "text": "hi" }), Duration::from_secs(5)).unwrap_err();
        assert!(refused.contains("Remote control is off on Mini"), "{refused}");
        assert!(!b.asked.lock().unwrap().iter().any(|(m, _)| m == "session-send"), "the Mini never ran it");
        let on = DevicesCfg { remote_control: true, ..b.peers.0.lock().cfg.clone() };
        b.board.lock().unwrap().devices = on.clone();
        b.peers.configure(&on);
        let sent = a.peers.ask(&b_short, "session-send", json!({ "key": "claude:claude:s2", "text": "hi" }), Duration::from_secs(5)).unwrap();
        assert_eq!(sent["message"], "Sent.");
        // An agent on the other Mac may have asked it: what it sends isn't taken as yours.
        assert_eq!(b.asked.lock().unwrap().last().unwrap().1["watching"], false);

        // The link is kept on disk on both sides.
        assert!(load_linked(&a.dir).iter().any(|l| l.key == key_of(&b) && l.name == "Mini"));
        assert!(load_linked(&b.dir).iter().any(|l| l.key == key_of(&a)));
        assert!(!a.views.lock().unwrap().is_empty(), "the windows got views");

        // Unlinking on one side unlinks both, and the lanes go.
        a.peers.unlink(&key_of(&b)).unwrap();
        until("b to forget a", || b.peers.view().devices.is_empty());
        assert!(a.peers.view().lanes.is_empty());
        assert!(a.peers.ask(&b_short, "search", json!({}), Duration::from_secs(1)).is_err());
        for m in [&a, &b] {
            let _ = std::fs::remove_dir_all(&m.dir);
        }
    }

    #[test]
    fn a_side_conversation_asked_about_from_another_machine_goes_to_it_as_it_changes() {
        use codealong_peer::api::SideTurn;
        let a = device("Book", vec![]);
        let b = device("Mini", vec![lane("/Users/sam/app", vec![session("claude:claude:s2", Status::Needs)])]);
        join(&a, &b);
        link(&a, &b);
        let b_short = short_of(&key_of(&b)).to_string();
        let side = |answer: &str| Side { id: "7-1".into(), key: "claude:claude:s2".into(), turns: vec![SideTurn { question: "Why?".into(), answer: answer.into(), ..Default::default() }], running: true, ..Default::default() };
        // Nothing of the Mini's side conversations goes to the Book before it asks about them.
        b.peers.side(&side("Not yet"));
        a.peers.ask(&b_short, "side-get", json!({ "key": "claude:claude:s2" }), Duration::from_secs(5)).unwrap();
        // From then on each change goes, by the key the session has on the Book; another
        // session's doesn't.
        b.peers.side(&Side { key: "claude:claude:other".into(), ..side("Elsewhere") });
        b.peers.side(&side("Because"));
        b.peers.side(&Side { running: false, ..side("Because it is.") });
        until("the answer", || a.sides.lock().unwrap().iter().any(|s| !s.running));
        let got = a.sides.lock().unwrap().clone();
        assert!(got.iter().all(|s| s.key == format!("@{b_short}:claude:claude:s2")), "{got:?}");
        assert!(!got.iter().any(|s| s.turns[0].answer == "Not yet"), "{got:?}");
        assert_eq!(got.last().unwrap().turns[0].answer, "Because it is.");
        // Connected again, the Book still gets what it asked about, without asking again.
        let serial = |d: &Device| d.peers.0.lock().conns.values().next().map(|c| c.serial);
        let before = serial(&b);
        join(&a, &b);
        until("the new connection", || serial(&b) != before && serial(&b).is_some());
        b.peers.side(&Side { turns: vec![SideTurn { question: "And now?".into(), answer: "Still.".into(), ..Default::default() }], ..side("") });
        until("what it said since", || a.sides.lock().unwrap().iter().any(|s| s.turns[0].answer == "Still."));
        for m in [&a, &b] {
            let _ = std::fs::remove_dir_all(&m.dir);
        }
    }

    #[test]
    fn a_side_conversation_queued_to_go_keeps_what_happened_in_order() {
        use codealong_peer::api::SideTurn;
        let open = |id: &str, answer: &str| Side { id: id.into(), key: "k".into(), turns: vec![SideTurn { question: "Why?".into(), answer: answer.into(), ..Default::default() }], running: true, ..Default::default() };
        let gone = || Side { key: "k".into(), ..Default::default() };
        let mut queue = Vec::new();
        side_queued(&mut queue, open("a", "Be"));
        side_queued(&mut queue, open("a", "Because"));
        // Closed while it answers: none open, and the one closed still answering.
        side_queued(&mut queue, gone());
        side_queued(&mut queue, Side { closed: true, ..open("a", "Because it") });
        // Another asked and closed after it.
        side_queued(&mut queue, open("b", "Yes"));
        side_queued(&mut queue, gone());
        side_queued(&mut queue, Side { closed: true, running: false, ..open("a", "Because it is.") });
        let said: Vec<(&str, bool, &str)> = queue.iter().map(|s| (s.id.as_str(), s.closed, s.turns.first().map_or("", |t| t.answer.as_str()))).collect();
        // The latest of each, after what happened before it: none is open at the end.
        assert_eq!(said, [("a", false, "Because"), ("b", false, "Yes"), ("", false, ""), ("a", true, "Because it is.")]);
    }

    #[test]
    fn linked_machines_say_where_they_stand_for_the_automations_they_share() {
        use codealong_peer::feed::Rank;
        let a = device("Book", vec![]);
        let b = device("Mini", vec![]);
        let backup = DevicesCfg { automations: Rank::Backup, ..b.peers.0.lock().cfg.clone() };
        b.board.lock().unwrap().devices = backup.clone();
        b.peers.configure(&backup);
        join(&a, &b);
        link(&a, &b);
        // Each knows itself, and the other as its feed says, for the engine (shared.rs).
        until("a to hear where b stands", || a.peers.view().machines.others.iter().any(|m| m.key == key_of(&b) && m.rank == Rank::Backup && m.online));
        let mine = a.peers.view().machines.me.unwrap();
        assert_eq!((mine.key, mine.rank), (key_of(&a), Rank::First));
        assert_eq!(a.peers.view().devices[0].automations, Some(Rank::Backup), "Settings shows it");
        until("b to hear where a stands", || b.peers.view().machines.others.iter().any(|m| m.name == "Book" && m.rank == Rank::First));
        // Off, a machine shares nothing.
        let off = DevicesCfg { enabled: false, ..a.peers.0.lock().cfg.clone() };
        a.peers.configure(&off);
        assert_eq!(a.peers.view().machines, crate::shared::Machines::default());
        for m in [&a, &b] {
            let _ = std::fs::remove_dir_all(&m.dir);
        }
    }

    /// A schedule shared from one machine reaches the other with its runs there, whose sessions the
    /// other finds among that machine's worktrees, as the rows under a schedule look them up.
    #[test]
    fn a_shared_schedules_runs_on_one_machine_are_found_on_the_other() {
        let a = device("Book", vec![]);
        let b = device("Mini", vec![lane("/Users/sam/infra", vec![session("claude:claude:run1", Status::Idle)])]);
        let schedule = crate::schedules::Schedule { id: "s-1".into(), name: "Triage".into(), runs_on: crate::schedules::RunsOn::One { prefer: String::new() }, ..Default::default() };
        let run = crate::schedules::RunRecord { at: 5, why: "9:00".into(), session: Some("claude:claude:run1".into()), ..Default::default() };
        let copy = codealong_peer::schedules::SharedSchedule { id: "s-1".into(), schedule: serde_json::to_value(&schedule).unwrap(), able: true, runs: vec![serde_json::to_value(&run).unwrap()], ..Default::default() };
        b.board.lock().unwrap().shared_schedules = codealong_peer::schedules::Shared { list: vec![copy], gone: Vec::new() };
        join(&a, &b);
        link(&a, &b);
        until("a to get b's schedule", || a.peers.view().machines.others.iter().any(|m| m.schedules.get("s-1").is_some_and(|x| !x.runs.is_empty())));
        until("a to get b's lanes", || a.peers.view().lanes.len() == 1);
        let view = a.peers.view();
        let runs = crate::schedules::machines::their_runs("s-1", &view.machines);
        let keys: Vec<&str> = view.lanes.iter().flat_map(|l| l.sessions.iter().map(|s| s.key.as_str())).collect();
        assert_eq!(runs.len(), 1);
        assert!(keys.contains(&runs[0].session.as_deref().unwrap()), "{runs:?} among {keys:?}");
        for m in [&a, &b] {
            let _ = std::fs::remove_dir_all(&m.dir);
        }
    }

    #[test]
    fn a_session_on_a_mac_that_wont_end_it_is_still_done_with() {
        let a = device("Book", vec![]);
        let b = device("Mini", vec![lane("/Users/sam/app", vec![Session { title: "Fix login".into(), ..session("claude:claude:s2", Status::Unread) }])]);
        join(&a, &b);
        link(&a, &b);
        until("a to get b's lanes", || a.peers.view().lanes.len() == 1);
        let engine = crate::engine::Handle::over(Board { lanes: a.peers.view().lanes, ..Board::default() });
        let key = format!("@{}:claude:claude:s2", short_of(&key_of(&b)));
        let done = |stop: bool| crate::actions::run(&engine, &a.peers, codealong_peer::api::SESSION_DONE, &key, codealong_peer::api::params::Done { stop, ..Default::default() }).unwrap();
        let asked = || b.asked.lock().unwrap().iter().map(|(m, p)| (m.clone(), p["params"].clone())).collect::<Vec<_>>();
        let organized = || vec![("mark-seen".to_string(), json!({ "keys": ["claude:claude:s2"] })), ("session-archive".to_string(), json!({ "key": "claude:claude:s2", "archived": true }))];

        // Remote control off: the Mini ends nothing, but marks it seen and archives it, and Done
        // says why its terminal is still open.
        let answer = done(false);
        assert_eq!((answer["done"].clone(), answer["noted"].clone()), (json!(true), json!(true)), "{answer}");
        assert_eq!(answer["message"], "Done with “Fix login”: it's archived, but still open on Mini. Remote control is off on Mini; turn it on there in Settings › Devices.");
        assert_eq!(asked(), organized(), "the Mini never ran Done");

        // Remote control on, at a Mini from before it knew Done: the same, and it says to update it.
        let on = DevicesCfg { remote_control: true, ..b.peers.0.lock().cfg.clone() };
        b.board.lock().unwrap().devices = on.clone();
        b.peers.configure(&on);
        b.asked.lock().unwrap().clear();
        let answer = done(false);
        assert_eq!(answer["message"], "Done with “Fix login”: it's archived, but still open on Mini. Codealong there is from before it could be asked to close it: update it on Mini.");
        assert_eq!(asked(), [vec![("session-done".to_string(), json!({ "key": "claude:claude:s2", "stop": false }))], organized()].concat());

        // A Mini that knows it does it all, and its answer stands.
        b.asked.lock().unwrap().clear();
        let answer = done(true);
        assert_eq!(answer, json!({ "done": true, "message": "Done with “s2”: its turn is stopped and it's archived.", "noted": false }));
        assert_eq!(asked(), vec![("session-done".to_string(), json!({ "key": "claude:claude:s2", "stop": true }))]);
        for m in [&a, &b] {
            let _ = std::fs::remove_dir_all(&m.dir);
        }
    }

    #[test]
    fn saying_no_links_nothing_and_strangers_cant_ask() {
        let a = device("Book", vec![]);
        let b = device("Mini", vec![]);
        join(&a, &b);
        until("a to find b", || a.peers.view().found.len() == 1);
        a.peers.link(&key_of(&b)).unwrap();
        until("b to be asked", || !b.peers.view().asks.is_empty());
        b.peers.answer_link(&key_of(&a), false).unwrap();
        until("a to stop asking", || !a.peers.view().found[0].asking);
        assert!(a.peers.view().devices.is_empty() && b.peers.view().devices.is_empty());

        // A device that isn't yours on the tailnet (another user) isn't found and can't ask.
        let c = device("Alex", vec![]);
        c.peers.0.lock().my_user = Some("alex@example.com".into());
        join_as(&c, "alex@example.com", &a, "sam@example.com");
        until("a to see c connected", || a.peers.0.lock().conns.len() == 2);
        assert_eq!(a.peers.view().found.len(), 1, "only b");
        let alex_key = key_of(&c);
        let a_key = key_of(&a);
        let tx = c.peers.0.lock().conns.get(&a_key).unwrap().tx.clone();
        tx.send(Msg::LinkAsk { secret: None }).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(a.peers.view().asks.is_empty() && !a.peers.view().devices.iter().any(|d| d.key == alex_key));
        for m in [&a, &b, &c] {
            let _ = std::fs::remove_dir_all(&m.dir);
        }
    }

    #[test]
    fn a_device_stays_linked_until_devices_json_no_longer_has_it() {
        let a = device("Book", vec![]);
        let b = device("Mini", vec![]);
        join(&a, &b);
        link(&a, &b);
        let (a_key, b_key) = (key_of(&a), key_of(&b));
        // devices.json can't be replaced (a folder in its place, as a full disk would refuse it).
        let file = linked_path(&a.dir);
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        let e = a.peers.unlink(&b_key).unwrap_err();
        assert!(e.starts_with("Mini is still linked: Couldn't write ") && e.contains("devices.json"), "{e}");
        assert!(crate::store::problems(&a.dir).iter().any(|(f, why)| f == "devices.json" && e.ends_with(why.as_str())), "a notice says so too");
        // Still trusted here, as the next launch would have it, and the Mini wasn't told otherwise.
        assert!(a.peers.view().devices.iter().any(|d| d.key == b_key));
        std::thread::sleep(Duration::from_millis(300));
        assert!(b.peers.view().devices.iter().any(|d| d.key == a_key && d.linked_back));
        // A color that can't be saved isn't taken either.
        assert!(a.peers.set_color(&b_key, Some("green".into())).is_err());
        assert_eq!(a.peers.view().devices[0].color, color_of(&Linked { color: None, ..a.peers.0.lock().linked[0].clone() }));

        // Once it can be written, the unlink goes through on both.
        std::fs::remove_dir(&file).unwrap();
        a.peers.unlink(&b_key).unwrap();
        assert!(load_linked(&a.dir).is_empty() && a.peers.view().devices.is_empty());
        until("b to forget a", || b.peers.view().devices.is_empty());
        assert!(crate::store::problems(&a.dir).is_empty());
        for m in [&a, &b] {
            let _ = std::fs::remove_dir_all(&m.dir);
        }
    }

    #[test]
    fn a_devices_json_that_does_not_read_trusts_no_one_and_links_no_one() {
        let newer = r#"{"linked": {"by-key": {}}}"#;
        let a = device_where("Book", vec![], |dir| std::fs::write(linked_path(dir), newer).unwrap());
        let b = device("Mini", vec![]);
        assert!(a.peers.view().devices.is_empty());
        assert!(crate::store::problems(&a.dir).iter().any(|(f, why)| f == "devices.json" && why.starts_with("devices.json doesn't read as a list of linked devices")));
        join(&a, &b);
        until("b to find a", || b.peers.view().found.iter().any(|f| f.key == key_of(&a)));
        b.peers.link(&key_of(&a)).unwrap();
        until("a to be asked", || !a.peers.view().asks.is_empty());
        let e = a.peers.answer_link(&key_of(&b), true).unwrap_err();
        assert!(e.starts_with("Mini isn't linked: devices.json doesn't read as a list of linked devices"), "{e}");
        // The Mini heard no, and nothing was written over the file.
        until("b to stop asking", || !b.peers.view().found[0].asking);
        assert!(a.peers.view().devices.is_empty() && b.peers.view().devices.is_empty());
        assert_eq!(std::fs::read_to_string(linked_path(&a.dir)).unwrap(), newer);
        for m in [&a, &b] {
            let _ = std::fs::remove_dir_all(&m.dir);
        }
    }

    /// Sends a request over `from`'s connection to `to` as `ask` would, without `ask`'s check that
    /// they're linked, and gives back where its answer arrives.
    fn ask_anyway(from: &Device, to: &Device, id: u64, method: &str) -> std::sync::mpsc::Receiver<std::result::Result<Value, String>> {
        let to_key = key_of(to);
        let (s, r) = channel();
        let mut st = from.peers.0.lock();
        st.pending.insert(id, (to_key.clone(), s));
        st.conns.get(&to_key).unwrap().tx.send(Msg::Req { id, method: method.into(), params: json!({ "key": "claude:claude:s1" }) }).unwrap();
        r
    }

    #[test]
    fn a_device_that_isnt_linked_is_refused_without_anything_started_for_it() {
        let a = device("Book", vec![]);
        let c = device("Alex", vec![]);
        join(&c, &a);
        until("connected", || a.peers.0.lock().conns.len() == 1 && c.peers.0.lock().conns.len() == 1);
        // Its key proved, but not linked: every request is answered no, and no thread is started.
        let answers: Vec<_> = (0..200).map(|i| ask_anyway(&c, &a, 1_000_000 + i, "session-page")).collect();
        for r in answers {
            assert_eq!(r.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err(), "Book isn't linked with this device.");
        }
        assert_eq!(a.peers.0.requests.load(Ordering::SeqCst), 0);
        assert!(a.asked.lock().unwrap().is_empty());
        // Linked, its requests run.
        link(&c, &a);
        let page = c.peers.ask(short_of(&key_of(&a)), "session-page", json!({ "key": "claude:claude:s1" }), Duration::from_secs(5)).unwrap();
        assert_eq!(page["key"], "claude:claude:s1");
        assert_eq!(a.peers.0.requests.load(Ordering::SeqCst), 1);
        for m in [&a, &c] {
            let _ = std::fs::remove_dir_all(&m.dir);
        }
    }

    #[test]
    fn a_device_that_asks_and_never_reads_is_cut_off() {
        let a = device("Book", vec![]);
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let stream = TcpStream::connect(("127.0.0.1", l.local_addr().unwrap().port())).unwrap();
        let (accepted, _) = l.accept().unwrap();
        a.peers.attach(Wire::tcp(accepted).unwrap(), Via::Test, false, None, None, None);
        // Its side of the hello and the proofs, by hand.
        let secret = SecretKey::generate();
        let hex = peerproto::key_hex(&secret.public());
        let (mut w, mut r) = (stream.try_clone().unwrap(), BufReader::new(stream));
        let hello = Msg::Hello { v: VERSION, key: hex.clone(), name: "Stranger".into(), app: "0".into(), nonce: peerproto::nonce(), kind: Some(peerproto::MAC.into()), knows: vec![] };
        w.write_all(hello.line().as_bytes()).unwrap();
        let Ok(Some(Msg::Hello { key: mac, nonce, .. })) = peerproto::read_msg(&mut r) else { panic!("no hello") };
        let sig = peerproto::prove(&secret, &nonce, &peerproto::parse_key(&mac).unwrap()).unwrap();
        w.write_all(Msg::Proof { sig }.line().as_bytes()).unwrap();
        assert!(matches!(peerproto::read_msg(&mut r), Ok(Some(Msg::Proof { .. }))));
        until("connected", || a.peers.0.lock().conns.contains_key(&hex));
        // It asks and asks, and reads none of the answers.
        let flood = std::thread::spawn(move || {
            let line = Msg::Req { id: 1, method: "session-page".into(), params: json!({}) }.line();
            (0..2_000_000).any(|_| w.write_all(line.as_bytes()).is_err())
        });
        until("the Mac to let it go", || !a.peers.0.lock().conns.contains_key(&hex));
        assert!(flood.join().unwrap(), "its connection is closed");
        assert_eq!(a.peers.0.requests.load(Ordering::SeqCst), 0);
        let _ = std::fs::remove_dir_all(&a.dir);
    }

    #[test]
    fn a_linked_device_asking_for_too_much_at_once_is_cut_off() {
        let a = device("Book", vec![]);
        let b = device("Mini", vec![]);
        join(&a, &b);
        link(&a, &b);
        let answers: Vec<_> = (0..REQUESTS_MAX as u64 + 8).map(|i| ask_anyway(&a, &b, 1_000_000 + i, "slow")).collect();
        until("the Mini to let it go", || b.peers.0.lock().conns.is_empty());
        assert_eq!(b.peers.0.requests.load(Ordering::SeqCst), REQUESTS_MAX as u64, "no more than that ran");
        assert!(answers.iter().any(|r| r.recv_timeout(Duration::from_secs(5)).unwrap().is_err_and(|e| e.ends_with("went away."))));
        for m in [&a, &b] {
            let _ = std::fs::remove_dir_all(&m.dir);
        }
    }

    #[test]
    fn a_link_code_links_once() {
        let a = device("Book", vec![]);
        let b = device("Office", vec![]);
        // Not on the same tailnet account: only a link code will do.
        b.peers.0.lock().my_user = Some("work@example.com".into());
        // Link codes need iroh on; the code's secret is what's checked, so the transport here is
        // loopback all the same.
        a.peers.0.lock().cfg.iroh = true;
        let code = a.peers.link_code().unwrap();
        let t = Ticket::decode(&code).unwrap();
        assert_eq!(t.key, a.peers.0.me.get().unwrap().key);
        join(&b, &a);
        until("connected", || a.peers.0.lock().conns.len() == 1);
        let a_key = key_of(&a);
        let tx = b.peers.0.lock().conns.get(&a_key).unwrap().tx.clone();
        b.peers.0.lock().asking.insert(a_key.clone());
        tx.send(Msg::LinkAsk { secret: Some(t.secret.clone()) }).unwrap();
        until("linked by code", || a.peers.view().devices.len() == 1 && b.peers.view().devices.len() == 1);
        // The code is used up.
        assert!(a.peers.0.lock().codes.is_empty());
        for m in [&a, &b] {
            let _ = std::fs::remove_dir_all(&m.dir);
        }
    }

    #[test]
    fn other_devices_worktrees_go_into_this_devices_projects_by_repo() {
        let projects = vec![
            ProjectCfg { name: "Spur".into(), repos: vec!["github.com/spurhq/*".into()], folders: vec![], chrome_profile: None },
            ProjectCfg { name: "Spruce".into(), repos: vec!["github.com/sprucehq/*".into()], folders: vec![], chrome_profile: None },
            ProjectCfg { name: "Notes".into(), repos: vec![], folders: vec!["~/notes".into()], chrome_profile: None },
        ];
        let with = |slug: Option<&str>, project: &str, hidden: bool| {
            let mut l = lane("/x", vec![]);
            l.repo_slug = slug.map(str::to_string);
            l.project = project.into();
            l.hidden = hidden;
            l
        };
        let none = HashMap::new();
        // The Mac mini calls it "spurhq" (or has no project for it at all): here it's Spur.
        let mut l = with(Some("SpurHQ/spur"), "spurhq", false);
        project_here(&mut l, &projects, &none);
        assert_eq!(l.project, "Spur");
        let mut l = with(Some("spurhq/beacon"), "Elsewhere", true);
        project_here(&mut l, &projects, &none);
        assert_eq!((l.project.as_str(), l.hidden), ("Spur", false), "out of Elsewhere, and shown");
        // Its checkout still has the org's old name; GitHub's name for it now is Spruce's.
        let mut l = with(Some("sparkhq/spark"), "sparkhq", false);
        project_here(&mut l, &projects, &none);
        assert_eq!(l.project, "sparkhq", "nothing here says it's the same repo");
        l.repo_slug_now = Some("sprucehq/spark".into());
        project_here(&mut l, &projects, &none);
        assert_eq!(l.project, "Spruce");
        // Where this device's own worktrees of the repo are comes first: a project that has
        // the repo by its folder, or by the old name that this device's own checkout has.
        let mut mine = with(Some("acme/notes"), "Notes", false);
        let mut old = with(Some("sparkhq/spark"), "sparkhq", false);
        old.repo_slug_now = Some("sprucehq/spark".into());
        let here = repos_here(&[mine.clone(), old, with(Some("acme/app"), "Elsewhere", true)]);
        let mut l = with(Some("acme/notes"), "Acme", false);
        project_here(&mut l, &projects, &here);
        assert_eq!(l.project, "Notes");
        let mut l = with(Some("SpruceHQ/spark"), "Spruce", false);
        project_here(&mut l, &[], &here);
        assert_eq!(l.project, "sparkhq", "the Mac mini's view of the MacBook's worktree");
        // Elsewhere isn't a project: its repos go by the rest.
        let mut l = with(Some("acme/app"), "Acme", false);
        project_here(&mut l, &projects, &here);
        assert_eq!(l.project, "Acme");
        mine.project = "Elsewhere".into();
        assert!(!repos_here(&[mine]).contains_key("acme/notes"));
        // No repo to go by: a project of the same name takes it.
        let mut l = with(None, "notes", false);
        project_here(&mut l, &projects, &none);
        assert_eq!(l.project, "Notes");
        // Nothing here matches: it keeps the other device's project, or stays in Elsewhere.
        let mut l = with(Some("acme/app"), "Acme", false);
        project_here(&mut l, &projects, &none);
        assert_eq!(l.project, "Acme");
        let mut l = with(Some("acme/app"), "Elsewhere", true);
        project_here(&mut l, &projects, &none);
        assert_eq!((l.project.as_str(), l.hidden), ("Elsewhere", true));
    }

    #[test]
    fn another_devices_projects_are_read_as_it_names_them() {
        let with = |id: &str, slug: Option<&str>, project: &str, origin: crate::model::Origin| {
            let mut l = lane(id, vec![]);
            (l.path, l.repo_slug, l.project, l.origin) = (id.into(), slug.map(str::to_string), project.into(), origin);
            l
        };
        use crate::model::Origin::{Main, Other};
        let mut renamed = with("/r/api", Some("northwind/api-old"), "Northwind", Main);
        renamed.repo_slug_now = Some("northwind/api".into());
        let mut chats = with("/r/chats", None, "Kestrel", Main);
        chats.starts_chats = true;
        let lanes = [
            with("/r/app", Some("northwind/app"), "Northwind", Main),
            with("/r/app-fix", Some("northwind/app"), "Northwind", Other),
            renamed,
            // No GitHub remote: its folder there, once.
            with("/r/notes", None, "Atlas", Main),
            with("/r/notes-wt", None, "Atlas", Other),
            with("/r/scratch", Some("me/scratch"), "Elsewhere", Main),
            chats,
        ];
        let p = |name: &str, repos: &[&str]| ProjectThere { name: name.into(), repos: repos.iter().map(|r| r.to_string()).collect() };
        assert_eq!(projects_there(&lanes), vec![p("Northwind", &["github.com/northwind/app", "github.com/northwind/api"]), p("Atlas", &["/r/notes"])]);
    }

    #[test]
    fn a_devices_worktrees_are_current_once_a_feed_came_on_its_connection() {
        assert!(!caught_up(None, 2_000), "not connected");
        assert!(!caught_up(Some(3_000), 2_000), "connected, its last feed from before");
        assert!(caught_up(Some(3_000), 3_000));
        assert!(caught_up(Some(3_000), 4_000));
    }

    #[test]
    fn devices_have_a_color_their_key_picks_until_you_choose() {
        let l = |key: &str, color: Option<&str>| Linked { key: key.into(), name: "d".into(), linked_at: 0, via: String::new(), color: color.map(str::to_string), kind: None };
        // The same key picks the same color everywhere, never red or gray.
        assert_eq!(color_of(&l("00ab", None)), "purple");
        assert_eq!(color_of(&l("01ab", None)), "teal");
        assert!(!["red", "gray"].contains(&color_of(&l("ff00", None)).as_str()));
        assert_eq!(color_of(&l("00ab", Some("red"))), "red");
        assert_eq!(color_of(&l("00ab", Some("plaid"))), "purple", "an unknown color is ignored");

        let a = device("Book", vec![]);
        let b = device("Mini", vec![lane("/x", vec![])]);
        join(&a, &b);
        link(&a, &b);
        let bk = key_of(&b);
        until("b's lanes", || a.peers.view().lanes.len() == 1);
        a.peers.set_color(&bk, Some("green".into())).unwrap();
        assert!(a.peers.set_color(&bk, Some("plaid".into())).is_err());
        let view = a.peers.view();
        assert_eq!(view.devices[0].color, "green");
        let tag = view.lanes[0].device.clone().unwrap();
        assert_eq!((tag.color.as_str(), tag.name.as_str(), tag.online), ("green", "Mini", true));
        assert_eq!(load_linked(&a.dir)[0].color.as_deref(), Some("green"), "kept on disk");
        for m in [&a, &b] {
            let _ = std::fs::remove_dir_all(&m.dir);
        }
    }

    #[test]
    fn both_macs_keep_the_same_connection_when_both_dial() {
        let a = device("Book", vec![]);
        let b = device("Mini", vec![]);
        join(&a, &b);
        join(&b, &a);
        std::thread::sleep(Duration::from_millis(500));
        until("one connection each", || a.peers.0.lock().conns.len() == 1 && b.peers.0.lock().conns.len() == 1);
        // Whichever survived, the two ends agree: one side dialed it and the other didn't.
        let (ad, bd) = (a.peers.0.lock().conns.values().next().unwrap().dialed, b.peers.0.lock().conns.values().next().unwrap().dialed);
        assert_ne!(ad, bd);
        for m in [&a, &b] {
            let _ = std::fs::remove_dir_all(&m.dir);
        }
    }
}

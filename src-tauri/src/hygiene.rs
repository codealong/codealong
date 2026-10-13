//! What's left running of old sessions, for `codealong hygiene`: a Mac with dozens of agents
//! fills its memory with processes nobody will use again, and an orchestrator's loop ends them.
//!
//! Four kinds, all judged against the board, so this never runs without the app's (`judge`):
//!
//! - a session's own process (the agent, with what it runs), once the session has been quiet
//!   for the wait and isn't working or waiting on its user, unless it's archived. Only a
//!   terminal's (`Surface::Cli`), never an app's or one Codealong runs itself, and never a
//!   process another live session that isn't old shares (OpenCode shows several in one);
//! - older copies of a background job's attach client: its `attach_command` run in a second tab,
//!   the newest kept;
//! - a harness's background process whose parent has gone (`Adapter::leftover`), unless a live
//!   session runs under it;
//! - dev servers and watchers (`is_dev`, or anything listening on a port) in a linked worktree
//!   none of whose sessions has been active for the wait. Never in a repo's main checkout, where
//!   a person runs theirs.
//!
//! Ending one (`end`) checks first that its pid still runs the same command, asks it and then its
//! child processes to end (SIGTERM), kills what's still there after a while (SIGKILL), and sweeps
//! once more for dev processes that came back in the worktrees it cleared: an Nx daemon starts
//! again as its dev server shuts down. Every process is the user's own, and a session's
//! conversation stays on disk to resume.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::idle::span;
use crate::model::{Board, Lane, Origin, Session, Status, Surface};

/// A process as `ps` lists it, with how long it has run.
#[derive(Clone, Debug, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub age_ms: i64,
    pub command: String,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A quiet session's own process.
    Session,
    /// An older copy of a background job's attach client.
    Attach,
    /// A harness's background process whose parent has gone.
    Orphan,
    /// A dev server or watcher in a linked worktree with no session at work.
    Dev,
}

/// A process to end, with its child processes.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub pid: u32,
    pub kind: Kind,
    /// Why, in words.
    pub why: String,
    pub command: String,
    pub age_ms: i64,
    /// How long what it belongs to has been quiet: its session, or its worktree's sessions.
    pub quiet_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    /// A background job's process: its harness stops the job, as Close does, rather than a
    /// signal that its service may take for a crash.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub job: bool,
    /// Its child processes and theirs, by pid, as listed when it was found.
    pub children: Vec<u32>,
    /// What it and they hold of memory, compressed and swapped included, in bytes.
    pub memory: u64,
}

/// What a process can be judged by besides the board and the process table: the folder it runs
/// in, and the processes listening on a port.
pub struct Scene<'a> {
    pub board: &'a Board,
    pub procs: &'a [Proc],
    pub now: i64,
    pub after_ms: i64,
    /// Never these: the command's own process and those above it, the app.
    pub protected: &'a HashSet<u32>,
    pub cwd: &'a dyn Fn(u32) -> Option<String>,
    pub listening: &'a HashSet<u32>,
    /// A linked worktree, by its folder (`.git` is a file there, not a folder).
    pub linked: &'a dyn Fn(&str) -> bool,
}

/// Everything to end, the session processes first.
pub fn judge(scene: &Scene) -> Vec<Found> {
    let Scene { board, procs, now, after_ms, protected, .. } = *scene;
    let by_pid: HashMap<u32, &Proc> = procs.iter().map(|p| (p.pid, p)).collect();
    let kids = children_of(procs);
    let sessions: Vec<(&Session, &Lane)> = board.lanes.iter().filter(|l| l.device.is_none()).flat_map(|l| l.sessions.iter().filter(|s| s.visitor.is_none()).map(move |s| (s, l))).collect();
    let quiet = |s: &Session| now - s.last_activity;
    let old = |s: &Session| quiet(s) >= after_ms && (s.archived || !matches!(s.status, Status::Working | Status::Needs | Status::Error));
    let mut found = Vec::new();

    // Sessions' own processes.
    for (s, lane) in &sessions {
        let Some(pid) = s.pid.filter(|p| by_pid.contains_key(p) && !protected.contains(p)) else { continue };
        if !s.live || s.surface != Surface::Cli || s.hosted || !old(s) {
            continue;
        }
        if sessions.iter().any(|(o, _)| o.key != s.key && o.live && o.pid == Some(pid) && !old(o)) || found.iter().any(|f: &Found| f.pid == pid) {
            continue;
        }
        let archived = if s.archived { ", archived" } else { "" };
        found.push(Found {
            pid,
            kind: Kind::Session,
            why: format!("“{}” ({}{archived}) has been quiet {}", s.title, word(&s.status), span(quiet(s))),
            command: by_pid[&pid].command.clone(),
            age_ms: by_pid[&pid].age_ms,
            quiet_ms: quiet(s),
            session: Some(s.key.clone()),
            worktree: Some(lane.id.clone()),
            job: s.attach_command.is_some(),
            children: Vec::new(),
            memory: 0,
        });
    }

    // Older copies of a background job's attach client.
    for (s, lane) in &sessions {
        let Some(attach) = s.attach_command.as_deref() else { continue };
        let mut clients: Vec<&Proc> = procs.iter().filter(|p| same_args(&p.command, attach)).collect();
        clients.sort_by_key(|p| p.age_ms);
        for p in clients.into_iter().skip(1).filter(|p| p.age_ms >= after_ms && !protected.contains(&p.pid)) {
            found.push(Found {
                pid: p.pid,
                kind: Kind::Attach,
                why: format!("an older copy of `{attach}` for “{}”, open {}", s.title, span(p.age_ms)),
                command: p.command.clone(),
                age_ms: p.age_ms,
                quiet_ms: p.age_ms,
                session: Some(s.key.clone()),
                worktree: Some(lane.id.clone()),
                job: false,
                children: Vec::new(),
                memory: 0,
            });
        }
    }

    // A harness's background processes left with no parent.
    let live_pids: HashSet<u32> = sessions.iter().filter(|(s, _)| s.live).filter_map(|(s, _)| s.pid).collect();
    for p in procs {
        if p.ppid != 1 || p.age_ms < after_ms || protected.contains(&p.pid) || !crate::harness::all().any(|a| a.leftover(&p.command)) {
            continue;
        }
        if descendants(&kids, p.pid).iter().chain([&p.pid]).any(|x| live_pids.contains(x)) {
            continue;
        }
        found.push(Found {
            pid: p.pid,
            kind: Kind::Orphan,
            why: format!("a background process whose parent has gone, running {}", span(p.age_ms)),
            command: p.command.clone(),
            age_ms: p.age_ms,
            quiet_ms: p.age_ms,
            session: None,
            worktree: None,
            job: false,
            children: Vec::new(),
            memory: 0,
        });
    }

    // Dev stacks in linked worktrees nobody's at work in.
    let quiet_lanes: Vec<(&Lane, i64)> = board
        .lanes
        .iter()
        .filter(|l| l.device.is_none() && l.origin != Origin::Main && (scene.linked)(&l.id))
        .filter_map(|l| {
            let busy = l.sessions.iter().any(|s| !s.archived && matches!(s.status, Status::Working | Status::Needs | Status::Error));
            let last = l.sessions.iter().map(|s| s.last_activity).chain([l.last_activity]).max().unwrap_or(0);
            (!busy && now - last >= after_ms).then_some((l, now - last))
        })
        .collect();
    let all_lanes: Vec<&str> = board.lanes.iter().filter(|l| l.device.is_none()).map(|l| l.id.as_str()).collect();
    let in_session: HashSet<u32> = live_pids.iter().flat_map(|&p| descendants(&kids, p).into_iter().chain([p])).collect();
    let dev_lane = |p: &Proc| -> Option<&str> {
        if !(is_dev(&p.command) || scene.listening.contains(&p.pid)) || in_session.contains(&p.pid) || protected.contains(&p.pid) {
            return None;
        }
        lane_of(&p.command, (scene.cwd)(p.pid).as_deref(), &all_lanes)
    };
    let mut roots: BTreeSet<u32> = BTreeSet::new();
    for p in procs.iter().filter(|p| p.age_ms >= after_ms) {
        let Some(lane) = dev_lane(p) else { continue };
        if !quiet_lanes.iter().any(|(l, _)| l.id == lane) {
            continue;
        }
        // The topmost dev process above it in the same worktree, which takes the rest with it.
        let mut top = p;
        while let Some(parent) = by_pid.get(&top.ppid).filter(|q| q.pid > 1 && dev_lane(q) == Some(lane)) {
            top = parent;
        }
        roots.insert(top.pid);
    }
    for pid in roots {
        let p = by_pid[&pid];
        let Some(lane) = dev_lane(p) else { continue };
        let Some((l, q)) = quiet_lanes.iter().find(|(l, _)| l.id == lane) else { continue };
        found.push(Found {
            pid,
            kind: Kind::Dev,
            why: format!("a dev process in {}, where no session has been active for {}", l.callsign, span(*q)),
            command: p.command.clone(),
            age_ms: p.age_ms,
            quiet_ms: *q,
            session: None,
            worktree: Some(l.id.clone()),
            job: false,
            children: Vec::new(),
            memory: 0,
        });
    }

    // One found inside another's tree goes with it.
    let tops: HashSet<u32> = found.iter().map(|f| f.pid).collect();
    found.retain(|f| !ancestors(&by_pid, f.pid).iter().any(|a| tops.contains(a)));
    for f in &mut found {
        f.children = descendants(&kids, f.pid).into_iter().filter(|p| !protected.contains(p)).collect();
    }
    found
}

fn children_of(procs: &[Proc]) -> HashMap<u32, Vec<u32>> {
    let mut kids: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in procs {
        kids.entry(p.ppid).or_default().push(p.pid);
    }
    kids
}

fn descendants(kids: &HashMap<u32, Vec<u32>>, pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut stack: Vec<u32> = kids.get(&pid).cloned().unwrap_or_default();
    while let Some(p) = stack.pop() {
        if p == pid || out.contains(&p) || out.len() > 4096 {
            continue;
        }
        out.push(p);
        stack.extend(kids.get(&p).into_iter().flatten());
    }
    out
}

fn ancestors(by_pid: &HashMap<u32, &Proc>, pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut p = by_pid.get(&pid).map(|p| p.ppid);
    while let Some(q) = p.filter(|q| *q > 1 && !out.contains(q) && out.len() < 64) {
        out.push(q);
        p = by_pid.get(&q).map(|p| p.ppid);
    }
    out
}

/// Whether a process runs `command` whatever the program's path or wrapper: the same arguments
/// after it. `claude2 attach 1a2b` runs `claude attach 1a2b`.
fn same_args(process: &str, command: &str) -> bool {
    let args = |c: &str| c.split_whitespace().skip(1).map(str::to_string).collect::<Vec<_>>();
    let want = args(command);
    !want.is_empty() && args(process) == want
}

/// The worktree a process belongs to, the deepest of `lanes` (one inside a main checkout's
/// folder is its own): the one it runs in, else the one its program or script is in. Never by
/// another argument: a server in a main checkout given a config from a worktree is the main
/// checkout's.
fn lane_of<'a>(command: &str, cwd: Option<&str>, lanes: &[&'a str]) -> Option<&'a str> {
    let deepest = |path: &str| lanes.iter().copied().filter(|l| path == *l || path.strip_prefix(*l).is_some_and(|rest| rest.starts_with('/'))).max_by_key(|l| l.len());
    let tokens: Vec<&str> = command.split_whitespace().collect();
    cwd.and_then(deepest).or_else(|| program_at(&tokens).into_iter().find_map(|i| deepest(tokens[i])))
}

/// Programs that run the script named after their options: the script is what the process is.
const INTERPRETERS: &[&str] = &["node", "nodejs", "bun", "deno", "tsx", "ts-node", "python", "python3", "ruby", "perl", "bash", "sh", "zsh", "dash", "env", "npx", "bunx", "pnpx"];
/// An interpreter's options that take the next word as their value.
const TAKES_VALUE: &[&str] = &["-r", "--require", "--import", "--loader", "--experimental-loader"];

/// Where a command's program is, by word: the first, and for an interpreter the script it runs.
fn program_at(tokens: &[&str]) -> Vec<usize> {
    let mut at = vec![0];
    let first = tokens.first().map(|t| t.rsplit('/').next().unwrap_or(t)).unwrap_or("");
    if INTERPRETERS.contains(&first) || first.starts_with("python3.") {
        let mut skip = false;
        for (i, t) in tokens.iter().enumerate().skip(1) {
            if std::mem::take(&mut skip) {
                continue;
            }
            if t.starts_with('-') {
                skip = TAKES_VALUE.contains(t);
                continue;
            }
            at.push(i);
            break;
        }
    }
    at
}

/// Dev servers and watchers, by the words of their command: each token's file name, without a
/// script's extension (`node …/vite/bin/vite.js dev` is `node vite dev`).
const DEV_WORDS: &[&[&str]] = &[
    &["vite"],
    &["next", "dev"],
    &["next", "start"],
    &["next-server"],
    &["nest", "start"],
    &["webpack", "serve"],
    &["webpack", "--watch"],
    &["webpack-dev-server"],
    &["tsc", "--watch"],
    &["tsc", "-w"],
    &["tsx", "watch"],
    &["ts-node-dev"],
    &["nodemon"],
    &["turbo", "dev"],
    &["turbo", "run", "dev"],
    &["storybook", "dev"],
    &["start-storybook"],
    &["concurrently"],
    &["esbuild", "--watch"],
    &["astro", "dev"],
    &["remix", "dev"],
    &["ng", "serve"],
    &["react-scripts", "start"],
    &["vue-cli-service", "serve"],
    &["graphql-codegen", "--watch"],
    &["graphql-codegen", "-w"],
    &["typed-scss-modules"],
    &["rails", "server"],
    &["rails", "s"],
    &["puma"],
    &["uvicorn"],
    &["flask", "run"],
    &["manage.py", "runserver"],
    &["cargo", "watch"],
    &["cargo-watch"],
    &["npm", "run", "dev"],
    &["npm", "start"],
    &["npm", "run", "start"],
    &["yarn", "dev"],
    &["yarn", "start"],
    &["yarn", "run", "dev"],
    &["pnpm", "dev"],
    &["pnpm", "start"],
    &["pnpm", "run", "dev"],
    &["bun", "dev"],
    &["bun", "run", "dev"],
    &["nx", "serve"],
    &["nx", "run"],
    &["nx", "run-many"],
    &["dev.sh"],
];

/// And by a path in their command, for the ones whose words are a script's: Nx's daemon, and the
/// executor `nx run-many` runs each target's server under.
const DEV_PATHS: &[&str] = &["/nx/src/daemon/", "/nx/dist/src/daemon/", "/nx/dist/bin/run-executor"];

/// Whether a command is a dev server or watcher: its program, or the script an interpreter runs,
/// starts one of `DEV_WORDS` or has one of `DEV_PATHS`. Only there, so an editor with `dev.sh`
/// open isn't one.
pub fn is_dev(command: &str) -> bool {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    let words: Vec<String> = tokens
        .iter()
        .map(|t| {
            let name = t.rsplit('/').next().unwrap_or(t).to_lowercase();
            [".js", ".mjs", ".cjs", ".ts"].iter().find_map(|x| name.strip_suffix(x).map(str::to_string)).unwrap_or(name)
        })
        .collect();
    program_at(&tokens).into_iter().any(|i| {
        DEV_WORDS.iter().any(|pat| words.get(i..i + pat.len()).is_some_and(|w| w.iter().zip(pat.iter()).all(|(a, b)| a == b))) || DEV_PATHS.iter().any(|p| tokens[i].contains(p))
    })
}

fn word(status: &Status) -> String {
    format!("{status:?}").to_lowercase()
}

// ---- the Mac ----

/// This Mac's load, memory and swap.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct System {
    /// The load averages over 1, 5 and 15 minutes, and how many CPUs share it.
    pub load: [f64; 3],
    pub cpus: u32,
    /// Memory in bytes, and how much of it is free as macOS counts it for memory pressure.
    pub memory: u64,
    pub free_percent: Option<u32>,
    /// What the compressor holds, in bytes.
    pub compressed: Option<u64>,
    pub swap_total: u64,
    pub swap_used: u64,
}

fn sysctl<T: Copy>(name: &str) -> Option<T> {
    let name = std::ffi::CString::new(name).ok()?;
    // SAFETY: sysctlbyname writes at most `size` bytes into a zeroed plain-old-data value, and
    // the size it reports back is checked against the value's.
    unsafe {
        let mut value: T = std::mem::zeroed();
        let mut size = std::mem::size_of::<T>();
        let ok = libc::sysctlbyname(name.as_ptr(), &mut value as *mut T as *mut libc::c_void, &mut size, std::ptr::null_mut(), 0) == 0;
        (ok && size == std::mem::size_of::<T>()).then_some(value)
    }
}

pub fn system() -> System {
    let mut load = [0f64; 3];
    // SAFETY: getloadavg fills at most three doubles.
    unsafe { libc::getloadavg(load.as_mut_ptr(), 3) };
    let swap: Option<libc::xsw_usage> = sysctl("vm.swapusage");
    System {
        load,
        cpus: sysctl::<i32>("hw.ncpu").unwrap_or(0).max(0) as u32,
        memory: sysctl::<u64>("hw.memsize").unwrap_or(0),
        free_percent: sysctl::<i32>("kern.memorystatus_level").map(|l| l.max(0) as u32),
        compressed: sysctl::<u64>("vm.compressor_bytes_used"),
        swap_total: swap.map_or(0, |s| s.xsu_total),
        swap_used: swap.map_or(0, |s| s.xsu_used),
    }
}

/// What a process of yours holds of memory, compressed and swapped included: Activity
/// Monitor's Memory column. None for one that's gone or another user's.
pub fn footprint(pid: u32) -> Option<u64> {
    // SAFETY: proc_pid_rusage fills a plain-old-data struct of the flavor asked for.
    unsafe {
        let mut info: libc::rusage_info_v4 = std::mem::zeroed();
        let ok = libc::proc_pid_rusage(pid as libc::c_int, libc::RUSAGE_INFO_V4, &mut info as *mut libc::rusage_info_v4 as *mut libc::rusage_info_t) == 0;
        ok.then_some(info.ri_phys_footprint)
    }
}

/// Every process, from one `ps`; an error when it doesn't answer, so nothing is judged on half a
/// table. `ps` on a Mac that stalls can take a long while, so it's given one.
pub fn processes() -> anyhow::Result<Vec<Proc>> {
    let out = crate::procs::run("/bin/ps", &["-axww", "-o", "pid=,ppid=,etime=,command="], None, Duration::from_secs(60))?;
    Ok(parse_ps(&out))
}

fn parse_ps(out: &str) -> Vec<Proc> {
    out.lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let pid = f.next()?.parse().ok()?;
            let ppid = f.next()?.parse().ok()?;
            let etime = f.next()?;
            let first = f.next()?;
            let command = line[line.find(etime)? + etime.len()..].trim_start();
            debug_assert!(command.starts_with(first));
            Some(Proc { pid, ppid, age_ms: elapsed_ms(etime)?, command: command.to_string() })
        })
        .collect()
}

/// `ps`'s `etime`: `[[dd-]hh:]mm:ss`.
fn elapsed_ms(etime: &str) -> Option<i64> {
    let (days, rest) = match etime.split_once('-') {
        Some((d, r)) => (d.parse::<i64>().ok()?, r),
        None => (0, etime),
    };
    let secs = rest.split(':').try_fold(0i64, |acc, part| part.parse::<i64>().ok().map(|n| acc * 60 + n))?;
    Some((days * 86_400 + secs) * 1000)
}

// ---- ending them ----

/// How a found process ended.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Ended {
    pub pid: u32,
    pub kind: Kind,
    pub why: String,
    pub memory: u64,
    /// `ended`, `still running` (with the pids left), or `skipped` and why.
    pub result: String,
}

/// A process to signal, known by its pid and when it started, so a pid that has gone to another
/// process meanwhile is never signalled.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Target {
    pid: u32,
    started: i64,
}

impl Target {
    fn of(pid: u32) -> Option<Target> {
        (pid > 1).then(|| crate::procs::proc_start_ms(pid)).flatten().map(|started| Target { pid, started })
    }

    /// It's still the process it was.
    fn here(&self) -> bool {
        crate::procs::proc_start_ms(self.pid) == Some(self.started)
    }
}

fn signal(targets: &[Target], sig: libc::c_int) {
    for t in targets.iter().filter(|t| t.here()) {
        // SAFETY: kill only sends a signal; a pid that's gone or not ours fails harmlessly.
        unsafe { libc::kill(t.pid as libc::pid_t, sig) };
    }
}

fn wait_gone(targets: &[Target], within: Duration) -> Vec<Target> {
    let until = Instant::now() + within;
    loop {
        let left: Vec<Target> = targets.iter().copied().filter(Target::here).collect();
        if left.is_empty() || Instant::now() >= until {
            return left;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Ends what `judge` found, each with its child processes, and says how each went. Each tree is
/// read again first: a root whose command isn't what it was is left alone, its pid having gone
/// to something else, and every process is signalled only while it's the one that was read. A
/// background job is stopped by its harness (`Adapter::stop`), as Close does, and signalled only
/// if it's still there after. `board` is the app's, read just before.
pub fn end(found: &[Found], protected: &HashSet<u32>, board: &Board, cfg: &crate::config::Config) -> Vec<Ended> {
    let began = Instant::now();
    let mut out: Vec<Ended> = found.iter().map(|f| Ended { pid: f.pid, kind: f.kind, why: f.why.clone(), memory: f.memory, result: String::new() }).collect();
    let table = match processes() {
        Ok(t) => t,
        Err(e) => {
            let why = format!("skipped: `ps` didn't answer ({})", crate::util::words(e));
            out.iter_mut().for_each(|o| o.result = why.clone());
            return out;
        }
    };
    let kids = children_of(&table);
    let mut trees: Vec<(usize, Target, Vec<Target>)> = Vec::new();
    for (i, f) in found.iter().enumerate() {
        let same = table.iter().any(|p| p.pid == f.pid && p.command.chars().take(200).eq(f.command.chars().take(200)));
        let Some(root) = Target::of(f.pid).filter(|_| same) else {
            out[i].result = "skipped: it's gone, or its pid runs something else now".into();
            continue;
        };
        let children = descendants(&kids, f.pid).into_iter().filter(|p| !protected.contains(p)).filter_map(Target::of).collect();
        trees.push((i, root, children));
    }
    let mut stopped: HashMap<usize, Result<(), String>> = HashMap::new();
    for (i, _, _) in &trees {
        let Some(key) = found[*i].session.as_deref().filter(|_| found[*i].job) else { continue };
        let result = crate::harness::find(board, cfg, key).and_then(|(s, account)| crate::harness::adapter(s.harness).stop(&crate::harness::Ctx { account }, s));
        stopped.insert(*i, result.map(|_| ()).map_err(crate::util::words));
    }
    // The roots first, so an agent can close what it started itself; then what's left of each tree.
    let roots: Vec<Target> = trees.iter().map(|t| t.1).collect();
    let asked: Vec<Target> = trees.iter().filter(|t| !stopped.contains_key(&t.0)).map(|t| t.1).collect();
    signal(&asked, libc::SIGTERM);
    wait_gone(&roots, Duration::from_secs(10));
    let all: Vec<Target> = trees.iter().flat_map(|(_, root, children)| std::iter::once(*root).chain(children.iter().copied())).collect();
    let rest = wait_gone(&all, Duration::ZERO);
    signal(&rest, libc::SIGTERM);
    let rest = wait_gone(&rest, Duration::from_secs(5));
    signal(&rest, libc::SIGKILL);
    let rest = wait_gone(&rest, Duration::from_secs(3));

    // Dev processes that started again in the worktrees just cleared, as an Nx daemon does: only
    // ones that started since, and none a live session runs.
    let cleared: Vec<&str> = trees.iter().filter(|t| found[t.0].kind == Kind::Dev).filter_map(|t| found[t.0].worktree.as_deref()).collect();
    if !cleared.is_empty() {
        std::thread::sleep(Duration::from_secs(3));
        if let Ok(now) = processes() {
            let lanes: Vec<&str> = board.lanes.iter().filter(|l| l.device.is_none()).map(|l| l.id.as_str()).collect();
            let kids = children_of(&now);
            let live: HashSet<u32> = board.lanes.iter().flat_map(|l| &l.sessions).filter(|s| s.live).filter_map(|s| s.pid).flat_map(|p| descendants(&kids, p).into_iter().chain([p])).collect();
            let since_ms = began.elapsed().as_millis() as i64 + 1_000;
            let again: Vec<Target> = now
                .iter()
                .filter(|p| p.age_ms <= since_ms && !protected.contains(&p.pid) && !live.contains(&p.pid) && is_dev(&p.command))
                .filter(|p| {
                    let cwd = crate::procs::cwd_of(p.pid).map(|c| c.to_string_lossy().into_owned());
                    lane_of(&p.command, cwd.as_deref(), &lanes).is_some_and(|l| cleared.contains(&l))
                })
                .filter_map(|p| Target::of(p.pid))
                .collect();
            signal(&again, libc::SIGTERM);
            signal(&wait_gone(&again, Duration::from_secs(5)), libc::SIGKILL);
        }
    }
    for (i, root, children) in &trees {
        let left: Vec<u32> = std::iter::once(root).chain(children).filter(|t| rest.contains(t)).map(|t| t.pid).collect();
        out[*i].result = match (left.is_empty(), stopped.get(i)) {
            (false, _) => format!("still running: {left:?}"),
            (true, Some(Err(why))) => format!("ended (its harness couldn't stop it: {why})"),
            (true, _) => "ended".into(),
        };
    }
    out
}

/// Appends a line for each to the log: when, pid, memory, how it went, kind, why and command.
pub fn log(path: &Path, ended: &[Ended], found: &[Found]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    let stamp = crate::perf::stamp();
    for e in ended {
        let command = found.iter().find(|f| f.pid == e.pid).map(|f| crate::util::truncate(&f.command, 150)).unwrap_or_default();
        writeln!(file, "{stamp}\t{}\t{}MB\t{}\t{}\t{}\t{command}", e.pid, e.memory / 1_048_576, e.result, word_kind(e.kind), e.why)?;
    }
    Ok(())
}

fn word_kind(k: Kind) -> &'static str {
    match k {
        Kind::Session => "session",
        Kind::Attach => "attach",
        Kind::Orphan => "orphan",
        Kind::Dev => "dev",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{lane, session};

    const NOW: i64 = 1_800_000_000_000;
    const H: i64 = 3_600_000;

    fn proc(pid: u32, ppid: u32, age_h: i64, command: &str) -> Proc {
        Proc { pid, ppid, age_ms: age_h * H, command: command.into() }
    }

    fn quiet(key: &str, pid: u32, hours: i64, status: Status) -> Session {
        Session { pid: Some(pid), status, last_activity: NOW - hours * H, ..session(key) }
    }

    fn judged(board: &Board, procs: &[Proc], cwd: &dyn Fn(u32) -> Option<String>) -> Vec<(u32, Kind)> {
        let protected = HashSet::from([999]);
        let listening = HashSet::new();
        let scene = Scene { board, procs, now: NOW, after_ms: 24 * H, protected: &protected, cwd, listening: &listening, linked: &|l: &str| l.contains("/wt/") };
        judge(&scene).into_iter().map(|f| (f.pid, f.kind)).collect()
    }

    #[test]
    fn a_quiet_sessions_process_goes_unless_it_works_or_waits() {
        let board = Board {
            lanes: vec![lane(
                "/r",
                vec![
                    quiet("old", 10, 30, Status::Idle),
                    quiet("fresh", 11, 2, Status::Idle),
                    quiet("waits", 12, 30, Status::Needs),
                    Session { archived: true, ..quiet("archived", 13, 30, Status::Needs) },
                    quiet("mine", 999, 30, Status::Idle),
                ],
            )],
            ..Board::default()
        };
        let procs = [proc(10, 1, 40, "claude --resume old"), proc(11, 1, 40, "claude"), proc(12, 1, 40, "claude"), proc(13, 1, 40, "claude"), proc(14, 10, 40, "node mcp.js"), proc(999, 1, 40, "claude")];
        assert_eq!(judged(&board, &procs, &|_| None), vec![(10, Kind::Session), (13, Kind::Session)]);
    }

    #[test]
    fn an_apps_or_codealongs_own_session_is_left_alone() {
        let app = Session { surface: Surface::App, ..quiet("app", 10, 30, Status::Idle) };
        let hosted = Session { hosted: true, ..quiet("hosted", 11, 30, Status::Idle) };
        let board = Board { lanes: vec![lane("/r", vec![app, hosted])], ..Board::default() };
        assert_eq!(judged(&board, &[proc(10, 1, 40, "Claude"), proc(11, 1, 40, "acp")], &|_| None), vec![]);
    }

    #[test]
    fn only_older_copies_of_an_attach_client_go() {
        let job = Session { attach_command: Some("claude2 attach 1a2b".into()), ..quiet("job", 50, 1, Status::Working) };
        let board = Board { lanes: vec![lane("/r", vec![job])], ..Board::default() };
        let procs = [proc(50, 1, 40, "claude bg-spare"), proc(20, 1, 30, "claude attach 1a2b"), proc(21, 1, 26, "/x/claude attach 1a2b"), proc(22, 1, 3, "claude attach 1a2b"), proc(23, 1, 30, "claude attach 9z")];
        assert_eq!(judged(&board, &procs, &|_| None), vec![(21, Kind::Attach), (20, Kind::Attach)]);
    }

    #[test]
    fn dev_stacks_go_only_from_quiet_linked_worktrees() {
        let board = Board {
            lanes: vec![
                lane("/r", vec![]),
                crate::model::Lane { origin: Origin::Other, last_activity: NOW - 30 * H, ..lane("/r/wt/old", vec![]) },
                crate::model::Lane { origin: Origin::Other, last_activity: NOW - 30 * H, ..lane("/r/wt/busy", vec![quiet("w", 70, 1, Status::Idle)]) },
            ],
            ..Board::default()
        };
        let procs = [
            proc(30, 1, 40, "zsh"),
            proc(31, 30, 40, "npm run dev"),
            proc(32, 31, 40, "node /r/wt/old/node_modules/.bin/vite"),
            proc(33, 1, 40, "node /r/node_modules/.bin/vite"),
            proc(34, 1, 40, "node /r/wt/busy/node_modules/.bin/vite"),
            proc(35, 1, 40, "node /r/wt/old/node_modules/nx/src/daemon/server/start.js"),
            proc(36, 1, 40, "node /r/wt/old/scripts/seed.js"),
            // A main checkout's server given a worktree's config is the main checkout's.
            proc(37, 1, 40, "node /r/node_modules/.bin/vite --config /r/wt/old/vite.config.ts"),
            // An editor with a dev script open isn't a dev process.
            proc(38, 1, 40, "vim /r/wt/old/scripts/dev.sh"),
        ];
        let cwd = |pid: u32| match pid {
            31 => Some("/r/wt/old".to_string()),
            37 => Some("/r".to_string()),
            _ => None,
        };
        assert_eq!(judged(&board, &procs, &cwd), vec![(31, Kind::Dev), (35, Kind::Dev)]);
        // Without its folder, the program's own path says where it is, not another argument's.
        assert_eq!(judged(&board, &procs, &|pid| (pid == 31).then(|| "/r/wt/old".to_string())), vec![(31, Kind::Dev), (35, Kind::Dev)]);
    }

    #[test]
    fn an_orphaned_background_process_goes_unless_a_live_session_runs_under_it() {
        let job = Session { attach_command: Some("claude attach 1a2b".into()), ..quiet("job", 41, 1, Status::Working) };
        let board = Board { lanes: vec![lane("/r", vec![job])], ..Board::default() };
        let procs = [
            proc(40, 1, 30, "claude bg-pty-host --bg-pty-host /tmp/a.sock 200 50 -- /x/2.1.296 --bg-spare /tmp/a.claim.sock"),
            proc(41, 40, 30, "claude bg-spare --bg-spare /tmp/a.claim.sock"),
            proc(42, 1, 30, "claude bg-pty-host --bg-pty-host /tmp/b.sock 200 50 -- /x/2.1.296 --bg-spare /tmp/b.claim.sock"),
            proc(43, 7, 30, "claude bg-spare --bg-spare /tmp/c.claim.sock"),
        ];
        assert_eq!(judged(&board, &procs, &|_| None), vec![(42, Kind::Orphan)]);
    }

    #[test]
    fn dev_commands_are_known_by_their_words() {
        assert!(is_dev("node /w/node_modules/vite/bin/vite.js --port 5173"));
        assert!(is_dev("next-server (v15.1.0)"));
        assert!(is_dev("/usr/local/bin/node /w/node_modules/.bin/tsc --watch -p ."));
        assert!(is_dev("node /w/node_modules/.bin/nx run-many --parallel -t serve -p server,app"));
        assert!(is_dev("/bin/node /w/node_modules/nx/dist/bin/run-executor.js"));
        assert!(is_dev("/bin/bash /w/scripts/dev.sh"));
        assert!(!is_dev("node /w/scripts/invite-users.js"));
        assert!(!is_dev("npx vitest run"));
        assert!(!is_dev("/bin/zsh -l"));
        assert!(!is_dev("vim /w/scripts/dev.sh"));
        assert!(!is_dev("less /w/node_modules/nx/src/daemon/server.js"));
        assert!(is_dev("node --max-old-space-size=8192 -r /w/register.js /w/node_modules/nx/dist/src/daemon/server/start.js"));
        assert!(is_dev("bash -c npm run dev"));
    }

    #[test]
    fn ending_takes_a_process_with_its_children_and_leaves_a_pid_that_runs_something_else() {
        let spawn = |script: &str| {
            let mut child = std::process::Command::new("/bin/sh").args(["-c", script]).spawn().unwrap();
            let pid = child.id();
            // Reaped as it ends, so it doesn't linger as a zombie that still answers to its pid.
            std::thread::spawn(move || child.wait());
            pid
        };
        let target = spawn("sleep 300 & sleep 301 & wait");
        let bystander = spawn("sleep 302");
        std::thread::sleep(Duration::from_millis(500));
        let procs = processes().unwrap();
        let found = |pid: u32, command: Option<&str>| Found {
            pid,
            kind: Kind::Dev,
            why: "test".into(),
            command: command.map(str::to_string).unwrap_or_else(|| procs.iter().find(|p| p.pid == pid).unwrap().command.clone()),
            age_ms: 0,
            quiet_ms: 0,
            session: None,
            worktree: None,
            job: false,
            children: descendants(&children_of(&procs), pid),
            memory: 0,
        };
        let kids = found(target, None).children;
        assert_eq!(kids.len(), 2);
        let ended = end(&[found(target, None), found(bystander, Some("/bin/sh -c something else"))], &HashSet::new(), &Board::default(), &crate::config::Config::default());
        assert_eq!(ended[0].result, "ended");
        assert!(ended[1].result.starts_with("skipped"));
        assert!(kids.iter().chain([&target]).all(|&p| !crate::procs::pid_alive(p)));
        assert!(crate::procs::pid_alive(bystander));
        signal(&Target::of(bystander).into_iter().collect::<Vec<_>>(), libc::SIGKILL);
    }

    #[test]
    fn ps_lines_and_elapsed_times_read() {
        let procs = parse_ps("  412     1 02-03:05:15 /usr/bin/claude daemon run\n  413   412    15:10 claude --resume ab cd\n");
        assert_eq!(procs[0], Proc { pid: 412, ppid: 1, age_ms: ((2 * 24 + 3) * 3600 + 5 * 60 + 15) * 1000, command: "/usr/bin/claude daemon run".into() });
        assert_eq!(procs[1].command, "claude --resume ab cd");
        assert_eq!(procs[1].age_ms, (15 * 60 + 10) * 1000);
    }
}

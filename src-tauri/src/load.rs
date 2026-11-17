//! How busy this machine is, read every half a minute for the board (`Board::load`) and for your
//! other machines (`DeviceInfo::load`, peers.rs). The windows show it where you pick a machine to
//! start a session on, and the quick panel offers a calm machine when the one a start goes to is
//! strained (src/machineLoad.ts).
//!
//! A reading is a few system calls (os::system: the load average and a few sysctls), no process.
//! It's put in a word, calm, busy or strained, with some slack between the levels so that a machine
//! near a line doesn't flip at each reading, and its numbers are rounded, so the board, and the
//! feeds that follow it, change only when they move: under a steady load, every few minutes.

use codealong_peer::model::{Load, LoadLevel};

use crate::os::system::{Pressure, System};

/// How often the machine is read, in ms.
const EVERY_MS: i64 = 30_000;

/// The load per CPU (over five minutes) at which a machine is busy, every CPU having work, and
/// below which it's calm again: a little lower, so a machine that hovers there doesn't change at
/// each reading.
const BUSY_AT: f64 = 1.0;
const CALM_BELOW: f64 = 0.8;
/// The load per CPU at which a machine is strained, three more threads waiting for each CPU, and
/// below which it's only busy again.
const STRAINED_AT: f64 = 4.0;
const BUSY_BELOW: f64 = 3.0;

/// Where the OS says nothing of memory pressure (Linux, Windows): the share of memory free, in
/// percent, at or under which a machine is busy, and strained; and how much more must be free
/// for it to come back down a level.
const FREE_BUSY: u32 = 20;
const FREE_STRAINED: u32 = 10;
const FREE_SLACK: u32 = 5;

/// The levels the CPUs and memory each put the machine at on the reading before: each comes back
/// down only past its own slack.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Was {
    cpu: Option<LoadLevel>,
    memory: Option<LoadLevel>,
}

/// This machine's load, as last read.
#[derive(Default)]
pub struct Reading {
    read_at: Option<i64>,
    was: Was,
    now: Option<Load>,
}

impl Reading {
    /// The engine's tick: reads the machine when a reading is due, or the clock went back past the
    /// last one. True when what the board says of it changed.
    pub fn on_tick(&mut self, now: i64, read: impl FnOnce() -> System) -> bool {
        if self.read_at.is_some_and(|t| (0..EVERY_MS).contains(&(now - t))) {
            return false;
        }
        self.read_at = Some(now);
        let (next, was) = load_of(&read(), self.was).map_or((None, Was::default()), |(l, w)| (Some(l), w));
        self.was = was;
        let changed = next != self.now;
        self.now = next;
        changed
    }

    pub fn now(&self) -> Option<Load> {
        self.now.clone()
    }
}

/// A reading put in a word, with the levels it leaves for the next (`was` the last one's); None
/// when it measured nothing: no CPUs counted, or neither a load average (Windows keeps none) nor
/// memory read, which would make a machine look calm for want of a measurement.
pub fn load_of(s: &System, was: Was) -> Option<(Load, Was)> {
    if s.cpus == 0 {
        return None;
    }
    let per_cpu = (!crate::os::is_windows()).then(|| s.load[1] / s.cpus as f64);
    let cpu = per_cpu.map(|l| cpu_level(l, was.cpu));
    let memory = memory_level(s, was.memory);
    if cpu.is_none() && memory.is_none() {
        return None;
    }
    let level = cpu.unwrap_or(LoadLevel::Calm).max(memory.unwrap_or(LoadLevel::Calm));
    let memory_short = match s.pressure {
        Some(p) => p != Pressure::Normal,
        None => memory.is_some_and(|m| m >= LoadLevel::Busy),
    };
    let load = Load { level, per_cpu: per_cpu.map(rounded), cpus: s.cpus, memory_free: s.free_percent.map(|f| (f + 2) / 5 * 5), memory_short };
    Some((load, Was { cpu, memory }))
}

/// The level the load per CPU puts a machine at. Going down waits for it to fall a little under
/// the line it crossed going up.
fn cpu_level(per_cpu: f64, was: Option<LoadLevel>) -> LoadLevel {
    let was = was.unwrap_or(LoadLevel::Calm);
    if per_cpu >= STRAINED_AT || (was == LoadLevel::Strained && per_cpu >= BUSY_BELOW) {
        LoadLevel::Strained
    } else if per_cpu >= BUSY_AT || (was >= LoadLevel::Busy && per_cpu >= CALM_BELOW) {
        LoadLevel::Busy
    } else {
        LoadLevel::Calm
    }
}

/// The level memory puts a machine at: macOS's pressure where it says, which has slack of its own,
/// else what's free, which goes down a level only once `FREE_SLACK` more is free. None when
/// neither was read.
fn memory_level(s: &System, was: Option<LoadLevel>) -> Option<LoadLevel> {
    let was = was.unwrap_or(LoadLevel::Calm);
    Some(match (s.pressure, s.free_percent) {
        (Some(Pressure::Critical), _) => LoadLevel::Strained,
        (Some(Pressure::Warn), _) => LoadLevel::Busy,
        (Some(Pressure::Normal), _) => LoadLevel::Calm,
        (None, None) => return None,
        (None, Some(f)) if f <= FREE_STRAINED || (was == LoadLevel::Strained && f <= FREE_STRAINED + FREE_SLACK) => LoadLevel::Strained,
        (None, Some(f)) if f <= FREE_BUSY || (was >= LoadLevel::Busy && f <= FREE_BUSY + FREE_SLACK) => LoadLevel::Busy,
        (None, Some(_)) => LoadLevel::Calm,
    })
}

/// A load in words, for the command: `strained: 27 per CPU of 10, 40% of memory free`.
pub fn words(l: &Load) -> String {
    let level = match l.level {
        LoadLevel::Calm => "calm",
        LoadLevel::Busy => "busy",
        LoadLevel::Strained => "strained",
    };
    let mut parts = Vec::new();
    if let Some(p) = l.per_cpu {
        parts.push(format!("{p} per CPU of {}", l.cpus));
    }
    if let Some(f) = l.memory_free {
        parts.push(format!("{f}% of memory free"));
    }
    if l.memory_short {
        parts.push("memory getting short".into());
    }
    if parts.is_empty() {
        level.into()
    } else {
        format!("{level}: {}", parts.join(", "))
    }
}

/// A load per CPU as it's shown: to a tenth under 2, where calm and busy part, and whole from
/// there, so a busy machine's number doesn't change at each reading.
fn rounded(per_cpu: f64) -> f64 {
    if per_cpu < 2.0 {
        (per_cpu * 10.0).round() / 10.0
    } else {
        per_cpu.round()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os::{self, AsIf, Os};

    fn machine(five_minutes: f64, cpus: u32, free: Option<u32>, pressure: Option<Pressure>) -> System {
        System { load: [five_minutes, five_minutes, five_minutes], cpus, memory: 32 << 30, free_percent: free, pressure, ..Default::default() }
    }

    fn read(s: &System, was: Was) -> Load {
        load_of(s, was).unwrap().0
    }

    fn level(s: &System) -> LoadLevel {
        read(s, Was::default()).level
    }

    #[test]
    fn the_load_per_cpu_says_how_busy_a_machine_is() {
        // On an OS that keeps a load average, whatever this test runs on.
        let _on = AsIf::new(Os::MacOs);
        let normal = Some(Pressure::Normal);
        assert_eq!(level(&machine(4.0, 10, Some(60), normal)), LoadLevel::Calm);
        assert_eq!(level(&machine(12.0, 10, Some(60), normal)), LoadLevel::Busy);
        assert_eq!(level(&machine(270.0, 10, Some(38), normal)), LoadLevel::Strained);
        // The five minutes' average, not the last minute's.
        let s = System { load: [90.0, 5.0, 2.0], ..machine(0.0, 10, Some(60), normal) };
        assert_eq!(level(&s), LoadLevel::Calm);
    }

    #[test]
    fn a_machine_near_a_line_stays_where_it_was() {
        let _on = AsIf::new(Os::MacOs);
        let at = |per_cpu: f64, was| read(&machine(per_cpu * 10.0, 10, Some(60), Some(Pressure::Normal)), Was { cpu: Some(was), memory: Some(LoadLevel::Calm) }).level;
        // Up at the line, down only a little under it.
        assert_eq!(at(0.9, LoadLevel::Calm), LoadLevel::Calm);
        assert_eq!(at(0.9, LoadLevel::Busy), LoadLevel::Busy);
        assert_eq!(at(0.7, LoadLevel::Busy), LoadLevel::Calm);
        assert_eq!(at(3.5, LoadLevel::Busy), LoadLevel::Busy);
        assert_eq!(at(3.5, LoadLevel::Strained), LoadLevel::Strained);
        assert_eq!(at(2.9, LoadLevel::Strained), LoadLevel::Busy);
        assert_eq!(at(0.5, LoadLevel::Strained), LoadLevel::Calm);
    }

    #[test]
    fn memory_short_keeps_a_machine_busy_or_strained_whatever_its_cpus() {
        let _on = AsIf::new(Os::MacOs);
        assert_eq!(level(&machine(1.0, 10, Some(30), Some(Pressure::Warn))), LoadLevel::Busy);
        assert_eq!(level(&machine(1.0, 10, Some(5), Some(Pressure::Critical))), LoadLevel::Strained);
        assert!(read(&machine(1.0, 10, Some(30), Some(Pressure::Warn)), Was::default()).memory_short);
        // macOS's pressure, not its free share, which runs low with memory to spare.
        let mac = read(&machine(1.0, 10, Some(8), Some(Pressure::Normal)), Was::default());
        assert_eq!((mac.level, mac.memory_short), (LoadLevel::Calm, false));
        // Elsewhere, what's free.
        assert_eq!(level(&machine(1.0, 10, Some(15), None)), LoadLevel::Busy);
        assert_eq!(level(&machine(1.0, 10, Some(8), None)), LoadLevel::Strained);
        assert!(read(&machine(1.0, 10, Some(15), None), Was::default()).memory_short);
    }

    #[test]
    fn what_is_free_comes_back_down_a_level_only_past_its_slack() {
        let _on = AsIf::new(Os::Linux);
        let at = |free: u32, was| read(&machine(1.0, 10, Some(free), None), Was { cpu: Some(LoadLevel::Calm), memory: Some(was) }).level;
        assert_eq!(at(11, LoadLevel::Busy), LoadLevel::Busy);
        assert_eq!(at(11, LoadLevel::Strained), LoadLevel::Strained);
        assert_eq!(at(16, LoadLevel::Strained), LoadLevel::Busy);
        assert_eq!(at(24, LoadLevel::Busy), LoadLevel::Busy);
        assert_eq!(at(26, LoadLevel::Busy), LoadLevel::Calm);
        // The CPUs' level doesn't hold memory's up: busy CPUs with memory to spare aren't short.
        let l = read(&machine(20.0, 10, Some(24), None), Was { cpu: Some(LoadLevel::Busy), memory: Some(LoadLevel::Calm) });
        assert_eq!((l.level, l.memory_short), (LoadLevel::Busy, false));
    }

    #[test]
    fn its_numbers_are_rounded_so_it_changes_only_when_they_move() {
        let _on = AsIf::new(Os::MacOs);
        let l = read(&machine(1.53 * 4.0, 4, Some(38), None), Was::default());
        assert_eq!((l.per_cpu, l.memory_free, l.cpus), (Some(1.5), Some(40), 4));
        // Whole numbers once busy, so a busy machine doesn't change at each reading.
        assert_eq!(read(&machine(51.4, 10, Some(37), None), Was::default()).per_cpu, Some(5.0));
        assert_eq!(read(&machine(271.4, 10, Some(37), None), Was::default()).per_cpu, Some(27.0));
    }

    #[test]
    fn windows_says_nothing_of_its_cpus_and_nothing_measured_says_nothing() {
        os::as_if(Os::Windows, || {
            let l = read(&machine(0.0, 8, Some(50), None), Was::default());
            assert_eq!((l.per_cpu, l.level), (None, LoadLevel::Calm));
            assert_eq!(level(&machine(0.0, 8, Some(9), None)), LoadLevel::Strained);
            // Its memory not read either: no word rather than a calm one.
            assert_eq!(load_of(&machine(0.0, 8, None, None), Was::default()), None);
        });
        assert_eq!(load_of(&machine(3.0, 0, Some(50), None), Was::default()), None);
    }

    #[test]
    fn a_load_in_words_says_its_level_and_numbers() {
        let _on = AsIf::new(Os::MacOs);
        let l = read(&machine(270.0, 10, Some(38), Some(Pressure::Warn)), Was::default());
        assert_eq!(words(&l), "strained: 27 per CPU of 10, 40% of memory free, memory getting short");
        let windows = Load { level: LoadLevel::Calm, per_cpu: None, cpus: 8, memory_free: None, memory_short: false };
        assert_eq!(words(&windows), "calm");
    }

    #[test]
    fn it_reads_the_machine_every_half_a_minute_and_says_when_the_board_changes() {
        let _on = AsIf::new(Os::MacOs);
        let mut r = Reading::default();
        let reads = std::cell::Cell::new(0);
        let read = |per_cpu: f64| {
            reads.set(reads.get() + 1);
            machine(per_cpu * 10.0, 10, Some(60), Some(Pressure::Normal))
        };
        assert!(r.on_tick(0, || read(0.5)));
        assert_eq!(r.now().unwrap().level, LoadLevel::Calm);
        // Not due yet: not read.
        assert!(!r.on_tick(10_000, || read(5.0)));
        assert_eq!(reads.get(), 1);
        // Read again, the same: nothing to publish.
        assert!(!r.on_tick(30_000, || read(0.5)));
        assert!(r.on_tick(60_000, || read(5.0)));
        assert_eq!(r.now().unwrap().level, LoadLevel::Strained);
        // Its slack carries over from one reading to the next.
        assert!(r.on_tick(90_000, || read(3.5)));
        assert_eq!(r.now().unwrap().level, LoadLevel::Strained);
        assert_eq!(reads.get(), 4);
        // The clock set back an hour: read again at once, not an hour later.
        assert!(r.on_tick(90_000 - 3_600_000, || read(0.5)));
        assert_eq!(r.now().unwrap().level, LoadLevel::Calm);
        assert_eq!(reads.get(), 5);
    }
}

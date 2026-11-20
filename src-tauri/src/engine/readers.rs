//! The harnesses' readers as the engine drives them (`harness::Reader`, one for each harness):
//! the scan of each, what each is given to go by as it reads (`look`), the glance at what the
//! watcher never hears of, the tick, and taking in what a read came to (`take`). And the process
//! table they all look at, listed off this thread once a tick (`list_processes`,
//! `on_processes`), so that no reader runs `ps` here.

use super::*;

/// How far back sessions are read.
const WINDOW_MS: i64 = 14 * DAY_MS;

impl Engine {
    /// What a reader goes by as it reads (`harness::Look`), with the processes last listed
    /// (`processes`), or none before the first scan has: never a `ps` of its own. Borrows only
    /// what you've seen, so the readers can be changed meanwhile.
    pub(super) fn look<'a>(seen: &'a SeenState, processes: &Option<Arc<procs::Listed>>) -> harness::Look<'a> {
        let now = now_ms();
        harness::Look { seen, since: now - WINDOW_MS, now, ps: procs::Listing::of(processes.clone().unwrap_or_default()) }
    }

    /// Takes in what a read came to: the board is to be published again, and the lines sessions
    /// wrote are checked. True when a session appeared, which may need a lane (`place_sessions`).
    pub(super) fn take(&mut self, applied: harness::Applied) -> bool {
        // Its callers name the reads that asked (`asked`), by harness or account.
        self.dirty |= applied.dirty;
        for l in applied.lines {
            self.check_new_lines(&l.path, l.from, l.to, &l.key);
        }
        self.read_found_logs(applied.found);
        applied.added
    }

    /// Has each reader ask what only a running service knows (`Reader::poll`).
    pub(super) fn poll_readers(&mut self) {
        for r in &mut self.readers {
            let t = Instant::now();
            r.poll(&harness::accounts_of(&self.cfg, r.harness()));
            perf::took(|| format!("poll {}", r.harness().config()), t.elapsed());
        }
    }

    /// Has each reader look at what the watcher never hears of (`Reader::glance`).
    pub(super) fn glance_readers(&mut self) {
        let look = Self::look(&self.seen, &self.processes);
        let mut all = harness::Applied::default();
        let mut asked = Vec::new();
        for r in &mut self.readers {
            let t = Instant::now();
            let applied = r.glance(&harness::accounts_of(&self.cfg, r.harness()), &look);
            perf::took(|| format!("glance {}", r.harness().config()), t.elapsed());
            if applied.dirty && perf::enabled() {
                asked.push(format!("glance {}", r.harness().config()));
            }
            all.and(applied);
        }
        for a in asked {
            self.asked(|| a);
        }
        if self.take(all) {
            self.place_sessions();
        }
    }

    pub(super) fn scan_sessions(&mut self) {
        let mut look = Self::look(&self.seen, &self.processes);
        // At launch nothing has listed the processes yet: the first reader that looks lists them,
        // here, once, and the looks go on with that until the first tick's.
        if self.processes.is_none() {
            look.ps = procs::Listing::default();
        }
        for r in &mut self.readers {
            r.scan(&harness::accounts_of(&self.cfg, r.harness()), &look);
        }
        if self.processes.is_none() {
            self.processes = look.ps.made();
        }
    }

    pub(super) fn session_cwds(&self) -> BTreeSet<String> {
        self.readers.iter().flat_map(|r| r.cwds()).collect()
    }

    /// Every session's record that's a file of lines, with the session's key (`Reader::logs`).
    pub(super) fn session_logs(&self) -> Vec<(PathBuf, String)> {
        self.readers.iter().flat_map(|r| r.logs()).collect()
    }

    /// Lists the processes for the readers' tick on a thread of its own: `ps` takes seconds at a
    /// high load, and the engine's thread would wait for it, and every change behind it. Not
    /// while a listing is on its way.
    pub(super) fn list_processes(&mut self) {
        if self.processes_inflight {
            return;
        }
        self.processes_inflight = true;
        let tx = self.tx.clone();
        // `on_processes` hears that the listing is over however it ends, so the next one can go.
        let work = || {
            let t = Instant::now();
            let listed = procs::list(PS_WAIT);
            perf::took(|| format!("ps for the tick, off the engine's thread ({} processes),", listed.lines.len()), t.elapsed());
            listed
        };
        throttle::background("codealong-ps", work, move |listed| {
            let _ = tx.send(Msg::Processes(listed));
        });
    }

    /// The processes are listed: the readers look at what came or went (`Reader::processes`),
    /// and Close stale terminals at when each tab was used (`stale::Uses`).
    /// One listing that fell over lists none, which says nothing: the readers are left as they
    /// are, and the looks go on with the last that worked.
    pub(super) fn on_processes(&mut self, listed: Option<procs::Listed>) {
        self.processes_inflight = false;
        let Some(listed) = listed.filter(|l| !l.lines.is_empty()) else { return };
        if let Some(Ok(mut uses)) = self.terminal_use.as_ref().map(|u| u.lock()) {
            let now = crate::util::now_ms();
            if uses.on_look(now, &crate::stale::device_times(&listed.lines)) {
                uses.save(now);
            }
        }
        self.processes = Some(Arc::new(listed));
        self.read_with_readers("processes", |r, accounts, look| r.processes(accounts, look));
    }

    /// The readers' tick: what the watcher doesn't say of the files (one its writer holds open).
    pub(super) fn tick_readers(&mut self) {
        self.read_with_readers("tick", |r, accounts, look| r.tick(accounts, look));
    }

    /// Has each reader read something (`what`, for the log), and takes in what it came to.
    fn read_with_readers(&mut self, what: &str, mut read: impl FnMut(&mut Box<dyn harness::Reader>, &[AccountCfg], &harness::Look) -> harness::Applied) {
        let look = Self::look(&self.seen, &self.processes);
        let mut all = harness::Applied::default();
        let mut asked = Vec::new();
        for r in &mut self.readers {
            let t = Instant::now();
            let applied = read(r, &harness::accounts_of(&self.cfg, r.harness()), &look);
            perf::took(|| format!("{what} {}", r.harness().config()), t.elapsed());
            if applied.dirty && perf::enabled() {
                asked.push(format!("{what} {}", r.harness().config()));
            }
            all.and(applied);
        }
        for a in asked {
            self.asked(|| a);
        }
        if self.take(all) {
            self.place_sessions();
        }
    }
}

#[cfg(test)]
mod tests {
    /// The tick's process table is listed on a thread of its own, and not again while a listing
    /// is out: one that fell over has to be over too. Each that worked goes to the readers as it
    /// lands; between listings they look at the last that worked, and never run `ps`.
    #[test]
    fn a_process_listing_that_falls_over_is_over() {
        use super::*;
        /// A reader that notes the processes it's given.
        struct Noting(Arc<Mutex<Vec<Vec<u32>>>>);
        impl harness::Reader for Noting {
            fn harness(&self) -> Harness {
                Harness::Codex
            }
            fn scan(&mut self, _: &[AccountCfg], _: &harness::Look) {}
            fn watch(&self, _: &AccountCfg) -> Vec<(PathBuf, bool)> {
                vec![]
            }
            fn owns(&self, _: &AccountCfg, _: &Path) -> bool {
                false
            }
            fn apply(&mut self, _: &AccountCfg, _: &[PathBuf], _: &harness::Look) -> harness::Applied {
                harness::Applied::default()
            }
            fn processes(&mut self, _: &[AccountCfg], look: &harness::Look) -> harness::Applied {
                self.0.lock().unwrap().push(look.ps.all().iter().map(|p| p.pid).collect());
                harness::Applied::default()
            }
            fn sessions(&self, _: &AccountCfg, _: &harness::Look) -> Vec<Session> {
                vec![]
            }
            fn cwds(&self) -> Vec<String> {
                vec![]
            }
        }
        let (tx, rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        let noted = Arc::new(Mutex::new(Vec::new()));
        engine.readers = vec![Box::new(Noting(noted.clone()))];
        // Before anything has listed them, a look has none, and lists none itself.
        let look = Engine::look(&engine.seen, &engine.processes);
        assert_eq!((look.ps.at(), look.ps.all().len()), (Some(0), 0));
        engine.processes_inflight = true;
        engine.list_processes();
        assert!(rx.try_recv().is_err(), "one listing at a time");
        engine.on_processes(None);
        assert!(!engine.processes_inflight, "the next listing can go");
        let row = |pid: u32, command: &str| procs::PsLine { pid, ppid: 1, tty: "??".into(), cpu: None, command: command.into() };
        let listed = procs::Listed { at: 1_792_000_000_000, lines: vec![row(7, "codex")] };
        engine.on_processes(Some(procs::Listed { at: 1_792_000_000_000, lines: vec![row(7, "codex")] }));
        engine.on_processes(Some(procs::Listed { at: 1_792_000_010_000, lines: vec![] }));
        assert_eq!(*noted.lock().unwrap(), [vec![7]], "the readers hear of the one that worked, as it lands");
        let look = Engine::look(&engine.seen, &engine.processes);
        assert_eq!((look.ps.at(), look.ps.all()), (Some(listed.at), &listed.lines[..]));
    }
}

//! What the engine has read in the background, each on a clock of its own and through
//! `throttle::background`, so that a read that falls over is still over: plan usage for the
//! harnesses that keep it somewhere slow to ask, the repos' team files, the accounts' data
//! settings, limit resets, sign-ins and MCP servers, and the terminals of starts that wait for
//! their session. Each but usage is a struct of its own that says what's due (`TrainingPoll`,
//! `ResetPoll`, `starting::Screens`…): the read is started here, and `run` hands its answer back.

use super::*;

/// A read that failed for a reason other than sign-in keeps the last good numbers; its error
/// says why they aren't fresh and `as_of` says how old they are.
fn keep_last_good(prev: Option<&Usage>, mut fresh: Usage) -> Usage {
    if fresh.error.is_some() && !fresh.needs_sign_in {
        if let Some(prev) = prev.filter(|p| !p.meters.is_empty()) {
            fresh.meters = prev.meters.clone();
            fresh.plan = fresh.plan.or_else(|| prev.plan.clone());
            fresh.as_of = prev.as_of;
        }
    }
    fresh
}

impl Engine {
    /// Reads usage on a background thread for the accounts whose harness keeps it somewhere slow
    /// to ask (`Adapter::polls_usage`: Claude Code's endpoint). Usage that comes with the
    /// sessions is the reader's (`Reader::usage`: Codex's, in its rollouts).
    pub(super) fn fetch_usage(&mut self) {
        if self.usage_inflight {
            return;
        }
        let accounts: Vec<AccountCfg> = self.cfg.accounts.iter().filter(|a| harness::of(a).is_some_and(|h| h.polls_usage(a))).cloned().collect();
        self.usage_last = Some(Instant::now());
        if accounts.is_empty() {
            return;
        }
        self.usage_inflight = true;
        let tx = self.tx.clone();
        // `on_usage` hears that the read is over however it ends, so usage is read again.
        throttle::background("codealong-usage", move || usage::read_all(&accounts), move |list| {
            let _ = tx.send(Msg::UsageDone(list));
        });
    }

    /// Reads the team files of the repos checked out here that are due (`TeamFiles::due`), on a
    /// background thread: from each repo's default branch and the branches its open PRs merge
    /// into.
    pub(super) fn read_team_files(&mut self) {
        let asks = self.team_file_asks();
        if asks.is_empty() {
            return;
        }
        let tx = self.tx.clone();
        // `on_read` hears that the read is over however it ends, so the next one can go.
        throttle::background("codealong-team-files", move || asks.iter().map(teamfile::read).collect(), move |answers| {
            let _ = tx.send(Msg::TeamFiles(answers));
        });
    }

    /// Reads the data settings of the accounts that are due (`TrainingPoll::due`), on a
    /// background thread.
    pub(super) fn read_training(&mut self) {
        if self.passing {
            return;
        }
        let asks = self.training.due(&self.cfg.accounts, Instant::now());
        if asks.is_empty() {
            return;
        }
        let tx = self.tx.clone();
        // `on_read` hears that the read is over however it ends, so the next one can go.
        throttle::background("codealong-training", move || training::read_all(&asks), move |list| {
            let _ = tx.send(Msg::TrainingRead(list));
        });
    }

    /// Reads the limit resets of the accounts that are due (`ResetPoll::due`), on a background
    /// thread.
    pub(super) fn read_resets(&mut self) {
        if self.passing {
            return;
        }
        let asks = self.resets.due(&self.cfg.accounts, Instant::now());
        if asks.is_empty() {
            return;
        }
        let tx = self.tx.clone();
        // `on_read` hears that the read is over however it ends, so the next one can go.
        throttle::background("codealong-resets", move || crate::resets::read_all(&asks), move |list| {
            let _ = tx.send(Msg::ResetsRead(list));
        });
    }

    /// Reads the boundary groups' members' sign-ins when they're due (`SignInPoll::due`), on a
    /// background thread: the provider is asked only of one whose answer is getting old.
    pub(super) fn read_sign_ins(&mut self) {
        let asks = self.sign_ins.due(&self.cfg, Instant::now());
        if asks.is_empty() {
            return;
        }
        let tx = self.tx.clone();
        // `on_read` hears that the read is over however it ends, so the next one can go.
        throttle::background("codealong-sign-ins", move || asks.iter().map(|a| crate::signin::current(a, now_ms(), crate::signin::Ask::Warm)).collect(), move |list| {
            let _ = tx.send(Msg::SignInsRead(list));
        });
    }

    /// Reads the MCP servers of the accounts that are due (`McpPoll::due`), on a background
    /// thread.
    pub(super) fn read_mcp(&mut self) {
        if self.passing {
            return;
        }
        let asks = self.mcp.due(&self.cfg.accounts, Instant::now());
        if asks.is_empty() {
            return;
        }
        let tx = self.tx.clone();
        // `on_read` hears that the read is over however it ends, so the next one can go.
        throttle::background("codealong-mcp", move || crate::mcp::read_all(&asks), move |list| {
            let _ = tx.send(Msg::McpRead(list));
        });
    }

    /// Reads the terminals of starts waiting in one for their session that are due
    /// (`starting::Screens::due`), on a background thread, their processes found in the latest
    /// listing.
    pub(super) fn read_screens(&mut self) {
        if self.passing {
            return;
        }
        let Some(listed) = self.processes.clone() else { return };
        let now = now_ms();
        let waiting = self.board.read().map(|b| origins::waiting_for(&b, now)).unwrap_or_default();
        let asks = self.screens.due(&waiting, now);
        if asks.is_empty() {
            return;
        }
        // Every start still waiting in a terminal, which a read asks whether another could claim
        // the process it finds.
        let all: Vec<crate::starting::Ask> = waiting.iter().filter(|p| p.terminal).map(crate::starting::Ask::of).collect();
        let tx = self.tx.clone();
        // `on_read` hears that the read is over however it ends, so the next one can go.
        throttle::background("codealong-screens", move || crate::starting::read(&asks, &all, &listed.lines), move |found| {
            let _ = tx.send(Msg::Screens(found));
        });
    }

    /// The team files due to be read now (`TeamFiles::due`), marked under way.
    pub(super) fn team_file_asks(&mut self) -> Vec<teamfile::Ask> {
        let repos: Vec<teamfile::Repo> = self
            .checkouts
            .iter()
            .filter_map(|(_, c)| {
                let (owner, name) = c.github.clone()?;
                let default = self.default_branch.get(&c.root).cloned().flatten().and_then(|b| b.strip_prefix("origin/").map(String::from));
                Some(teamfile::Repo { slug: format!("{owner}/{name}"), root: c.root.clone(), default })
            })
            .collect();
        let open = self.github.prs().iter().chain(self.github.requested()).flat_map(|(slug, prs)| prs.iter().filter(|p| p.state.is_open()).map(move |p| (slug.as_str(), p.base_ref.as_deref())));
        let bases = teamfile::bases(open);
        self.team_files.due(&repos, &bases, Instant::now())
    }

    /// The usage read is over. One that fell over, or never started, read nothing: the numbers
    /// and the backoff stay as they were, and the next read goes at its usual time.
    pub(super) fn on_usage(&mut self, list: Option<Vec<Usage>>) {
        self.usage_inflight = false;
        let Some(list) = list else { return };
        self.usage_backoff = list.iter().any(|u| u.rate_limited);
        for u in list {
            let u = keep_last_good(self.usage.get(&u.account), u);
            self.usage.insert(u.account.clone(), u);
        }
        // This engine's tests make their numbers up.
        if !cfg!(test) {
            usage::keep(&self.cfg.accounts, &self.usage);
        }
        self.mark_dirty("usage");
    }

    /// Loads cached GitHub org avatars for the current projects and schedules background fetches
    /// for any missing or stale ones (orgavatars.rs).
    pub(super) fn refresh_org_avatars(&mut self) {
        let owners = crate::orgavatars::project_owners(&self.cfg.projects, &self.seeds, |slug| self.github.renamed(slug).map(String::as_str));
        if self.org_avatars.load_cached(&owners) {
            self.mark_dirty("org avatars");
        }
        if self.passing {
            return;
        }
        let Some(due) = self.org_avatars.to_fetch(&owners, Instant::now(), now_ms()) else { return };
        let tx = self.tx.clone();
        throttle::background(
            "codealong-org-avatars",
            move || {
                crate::os::system::utility_thread();
                due.into_iter()
                    .map(|owner| {
                        let outcome = crate::orgavatars::fetch(&owner);
                        (owner, outcome)
                    })
                    .collect::<Vec<_>>()
            },
            move |results| {
                let _ = tx.send(Msg::OrgAvatars(results));
            },
        );
    }

    /// The background fetch of GitHub org avatars finished.
    pub(super) fn on_org_avatars(&mut self, results: Option<Vec<(String, crate::orgavatars::Outcome)>>) {
        if self.org_avatars.on_fetched(results, now_ms()) {
            self.mark_dirty("org avatars");
        }
    }

    /// Clones a repo a project covers that GitHub's search found PRs waiting on your review in,
    /// with no checkout here, while reviews are on for it (clones.rs), one at a time. Never in a
    /// test, where `gh` would clone for real: what's due there stays under way.
    pub(super) fn clone_covered(&mut self) {
        if self.passing || !self.github.names_known(&self.seeds) {
            return;
        }
        let root = crate::clones::root(&self.cfg.scan);
        let Some(c) = self.clones.due(&self.github.not_here(&self.seeds), &self.cfg, root.as_deref(), Instant::now()) else { return };
        if cfg!(test) {
            return;
        }
        let tx = self.tx.clone();
        let back = c.clone();
        throttle::background(
            "codealong-clone",
            move || {
                crate::os::system::utility_thread();
                crate::clones::run(&c)
            },
            move |went| {
                let _ = tx.send(Msg::Cloned(back, went));
            },
        );
    }

    /// A clone `clone_covered` started is over: one that's there is read with the repos, and
    /// GitHub read again in full, its search too, whose answer before the clone left the repo's
    /// PRs out, and whose answer starts the next repo waiting; one that isn't says why as a
    /// notice, and the next starts with the next search's answer.
    pub(super) fn on_cloned(&mut self, c: crate::clones::ToClone, went: Option<crate::clones::Cloned>) {
        if self.clones.on_cloned(&c, went) {
            self.rescan_repos();
            self.github.on_changed(&self.seeds, Now::real());
        } else {
            self.mark_dirty("clones");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::keep_last_good;
    use crate::model::{Harness, Meter, Severity, Usage};

    fn usage(error: Option<&str>, needs_sign_in: bool, percent: f64) -> Usage {
        Usage {
            account: "claude".into(),
            harness: Harness::Claude,
            plan: Some("Max 20x".into()),
            meters: if error.is_some() {
                vec![]
            } else {
                vec![Meter { key: "week".into(), label: "Week".into(), window: "Weekly".into(), percent, resets_at: None, severity: Severity::Normal, model: None }]
            },
            as_of: if error.is_some() { None } else { Some(1) },
            error: error.map(str::to_string),
            needs_sign_in,
            loading: false,
            no_limits: false,
            preferred: false,
            rate_limited: false,
        }
    }

    /// Usage is read on a thread nobody watches, and isn't read again while a read is on its
    /// way: one that fell over has to be over too.
    #[test]
    fn a_usage_read_that_falls_over_is_over() {
        use super::*;
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        engine.usage.insert("claude".into(), usage(None, false, 47.0));
        (engine.usage_inflight, engine.usage_backoff) = (true, true);
        engine.on_usage(None);
        assert!(!engine.usage_inflight, "the next read can go");
        assert!(engine.usage_backoff && !engine.dirty, "nothing was read: nothing else changes");
        assert_eq!(engine.usage["claude"].meters[0].percent, 47.0);

        engine.usage_inflight = true;
        engine.on_usage(Some(vec![usage(None, false, 50.0)]));
        assert!(!engine.usage_inflight && !engine.usage_backoff && engine.dirty);
        assert_eq!(engine.usage["claude"].meters[0].percent, 50.0);
    }

    #[test]
    fn a_failed_usage_read_keeps_the_last_numbers() {
        let good = usage(None, false, 47.0);
        let kept = keep_last_good(Some(&good), usage(Some("Couldn't read usage: HTTP 500"), false, 0.0));
        assert_eq!(kept.meters, good.meters);
        assert_eq!(kept.as_of, Some(1));
        assert!(kept.error.is_some());
        // Signing out isn't papered over.
        let signed_out = keep_last_good(Some(&good), usage(Some("Sign-in expired"), true, 0.0));
        assert!(signed_out.meters.is_empty());
    }

    #[test]
    fn fetched_org_avatars_mark_dirty_and_appear_on_the_board() {
        use super::*;
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        engine.cfg.projects.push(crate::config::ProjectCfg {
            name: "Northwind".into(),
            repos: vec!["github.com/northwind/*".into()],
            folders: Vec::new(),
            chrome_profile: None,
            default: None,
            decisions: None,
        });
        engine.seeds.push(LaneSeed {
            path: PathBuf::from("/work/northwind-app"),
            project: "Northwind".into(),
            repo: "app".into(),
            repo_slug: Some("northwind/app".into()),
            head_owners: Vec::new(),
            callsign: "app".into(),
            origin: Origin::Main,
            branch: None,
            head: None,
            default_branch: None,
            elsewhere: false,
            chats: false,
            starts_chats: false,
            exact: false,
        });
        let url = "data:image/png;base64,iVBORw==".to_string();
        engine.on_org_avatars(Some(vec![("northwind".into(), crate::orgavatars::Outcome::Avatar(url.clone()))]));
        assert!(engine.dirty);
        let board = engine.compose();
        let proj = board.projects.iter().find(|p| p.name == "Northwind").unwrap();
        assert_eq!(proj.icon.as_deref(), Some(url.as_str()));

        // A manual refresh schedules a re-fetch and updates the board when the avatar changes.
        engine.dirty = false;
        engine.org_avatars.on_refresh();
        let owners = crate::orgavatars::project_owners(&engine.cfg.projects, &engine.seeds, |_| None);
        assert_eq!(engine.org_avatars.to_fetch(&owners, Instant::now(), now_ms()), Some(vec!["northwind".to_string()]));
        let updated = "data:image/png;base64,R0lGOD==".to_string();
        engine.on_org_avatars(Some(vec![("northwind".into(), crate::orgavatars::Outcome::Avatar(updated.clone()))]));
        assert!(engine.dirty);
        let board = engine.compose();
        let proj = board.projects.iter().find(|p| p.name == "Northwind").unwrap();
        assert_eq!(proj.icon.as_deref(), Some(updated.as_str()));
    }
}

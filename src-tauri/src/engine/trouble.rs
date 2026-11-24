//! What goes wrong in the background, said on the board as a notice until it goes right again:
//! the kinds of it (`Trouble`), each with its notice's key and title; how the engine notes one
//! (`set_trouble`, `read_or_warn`); and the notices the board carries (`Engine::notices`), in the
//! order the windows have always had them. A new kind is a variant of `Trouble`, with its line in
//! `Trouble::notice` and its place in `Engine::notices`; what finds it wrong off the engine's
//! thread says so with `Handle::went`.

use super::*;

/// What can go wrong in the background that the engine says as a notice until it goes right
/// again (`Engine::notices`): each has one sentence at most, the last that went wrong with it.
/// A new one is a variant here, with its notice's key and title in `Trouble::notice` and its
/// place among the notices in `Engine::notices`. What finds it wrong off the engine's thread
/// tells the engine with `Handle::went`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Trouble {
    /// config.toml doesn't read, couldn't be saved, or names a sound that isn't there.
    Config,
    /// The folders that couldn't be watched.
    Watch,
    /// Why todos.json couldn't be read or saved.
    Todos,
    /// Why the lock on todos.json couldn't be had, when a change was turned away for it.
    TodosLock,
    /// Why state.json couldn't be read or saved.
    State,
    /// Why pr-links.json didn't read.
    PrLinks,
    /// Why previews.json didn't read.
    Previews,
    /// Why worktrees.json didn't read.
    Reusable,
    /// Why moving a session a usage limit stopped didn't work, when Codealong moved it itself
    /// (limits.rs).
    Limits,
    /// Why a session wasn't told its PR was approved, when Codealong sends that itself
    /// (nudges.rs).
    Nudges,
    /// Why a session wasn't told what a command you ran for it printed (terminal.rs
    /// `report_ended`).
    TerminalRuns,
    /// Why a session wasn't told to carry on after a lost connection, a usage limit or a sign-in
    /// (reconnect.rs).
    Reconnect,
    /// Why a reviewer wasn't asked to look again at a PR of yours after its fixes
    /// (rerequest.rs).
    Rerequests,
    /// Why a session started while the screen was locked didn't open in your terminal once it
    /// wasn't (launch.rs `show_unlocked`).
    Unlocked,
    /// Why a session wasn't told what its helper did (helpers.rs).
    Helpers,
    /// Which messages held for sessions that waited on their user were dropped, and why
    /// (outbox.rs).
    HeldMessages,
    /// Why the last emblem drawn wasn't, until one is.
    Emblems,
    /// What the engine's own thread fell over on, when it did and carried on (`run_guarded`).
    Engine,
}

impl Trouble {
    #[cfg(test)]
    const ALL: [Trouble; 18] = {
        use Trouble::*;
        [Engine, Config, Watch, Todos, TodosLock, State, PrLinks, Previews, Reusable, Limits, Nudges, TerminalRuns, Reconnect, Rerequests, Unlocked, Helpers, HeldMessages, Emblems]
    };

    /// Its notice's key, which the windows know a notice by, and its title.
    fn notice(self) -> (&'static str, &'static str) {
        match self {
            Trouble::Engine => ("engine", "Codealong hit a problem and carried on"),
            Trouble::Config => ("config", "config.toml has a problem"),
            Trouble::Watch => ("watch", "Live updates are off"),
            Trouble::Todos => ("todos", "todos.json has a problem"),
            Trouble::TodosLock => ("todos-lock", "Todos can't be changed"),
            Trouble::State => ("state", "state.json has a problem"),
            Trouble::PrLinks => ("pr-links", "PR links weren't read"),
            Trouble::Previews => ("previews", "Previews weren't read"),
            Trouble::Reusable => ("reusable", "Reusable worktrees weren't read"),
            Trouble::Limits => ("limits", "A session didn't move"),
            Trouble::Nudges => ("nudges", "A session wasn't told its PR was approved"),
            Trouble::TerminalRuns => ("terminal-runs", "A session wasn't told what a command printed"),
            Trouble::Reconnect => ("reconnect", "A session wasn't told to carry on"),
            Trouble::Rerequests => ("rerequests", "A reviewer wasn't asked to look again"),
            Trouble::Unlocked => ("unlocked", "A session didn't open in your terminal"),
            Trouble::Helpers => ("helpers", "A session wasn't told what its helper did"),
            Trouble::HeldMessages => ("held-messages", "A message held for a session wasn't sent"),
            Trouble::Emblems => ("emblems", "An emblem wasn't drawn"),
        }
    }
}

/// The notice for a file of Codealong's whose last change couldn't be made (`store::problems`).
pub fn store_notice(file: &str, why: &str) -> Notice {
    Notice { key: file.to_string(), title: format!("{file} has a problem"), text: why.to_string(), ..Default::default() }
}

impl Engine {
    /// Why `what` is going wrong, while it is.
    pub(super) fn trouble(&self, what: Trouble) -> Option<&String> {
        self.troubles.get(&what)
    }

    /// `what` went wrong, with why, or went right, with none.
    pub(super) fn set_trouble(&mut self, what: Trouble, why: Option<String>) {
        match why {
            Some(why) => self.troubles.insert(what, why),
            None => self.troubles.remove(&what),
        };
    }

    /// What one of the stores' files said, or none, with why as `what`'s trouble for the board
    /// (`Engine::notices`): what was read before stays as it is until the file reads again.
    pub(super) fn read_or_warn<T>(&mut self, what: Trouble, read: Result<T>) -> Option<T> {
        self.set_trouble(what, read.as_ref().err().map(|e| format!("{e:#}")));
        read.ok()
    }

    /// What isn't working in the background, as the windows say it: a few words for each, and
    /// the whole of it.
    pub(super) fn notices(&self) -> Vec<Notice> {
        let held = held_warning(&self.reads.out_for(HELD_WARN_AFTER));
        let team_files = self.team_files.notice();
        let clones = self.clones.notice();
        let ledger = crate::ledger::problem();
        let jev_usage = crate::ledger::jev_problem();
        let review = self.review_warning.as_ref();
        let said = |key: &str, title: &str, text: Option<&String>| Some(Notice { key: key.into(), title: title.into(), text: text?.clone(), ..Default::default() });
        let trouble = |what: Trouble| {
            let (key, title) = what.notice();
            said(key, title, self.trouble(what))
        };
        [
            // Its title counts the falls, so that one dismissed doesn't hide the next: a window
            // knows a notice it dismissed by its key and title.
            trouble(Trouble::Engine).map(|n| if self.fell > 1 { Notice { title: format!("{}, {} times", n.title, self.fell), ..n } } else { n }),
            trouble(Trouble::Config),
            self.github.notice(),
            trouble(Trouble::Watch),
            trouble(Trouble::Todos),
            trouble(Trouble::TodosLock),
            trouble(Trouble::State),
            trouble(Trouble::PrLinks),
            trouble(Trouble::Previews),
            trouble(Trouble::Reusable),
            said("held", "Waiting to open a folder", held.as_ref()),
            trouble(Trouble::Limits),
            trouble(Trouble::Nudges),
            trouble(Trouble::TerminalRuns),
            trouble(Trouble::Reconnect),
            trouble(Trouble::Rerequests),
            trouble(Trouble::Unlocked),
            trouble(Trouble::Helpers),
            trouble(Trouble::HeldMessages),
            trouble(Trouble::Emblems),
            said("reviews", "A pull request wasn't reviewed", review.map(|s| &s.text)).map(|n| Notice { review_again: review.and_then(|s| s.again.clone()), ..n }),
            said("team-files", "A repo's team settings aren't used", team_files.as_ref()),
            said("clones", "A repo wasn't cloned for its review", clones.as_ref()),
            said("ledger", "Usage is missing some sessions", ledger.as_ref()),
            said("jev-usage", "Jev's usage is missing requests", jev_usage.as_ref()),

        ]
        .into_iter()
        .flatten()
        .chain(self.store_warnings.iter().map(|(file, why)| store_notice(file, why)))
        .collect()
    }
}

/// Says which folders the scan has been waiting a while to open: most likely macOS is asking
/// whether Codealong may, and nobody has answered yet.
fn held_warning(paths: &[PathBuf]) -> Option<String> {
    let first = tilde(paths.first()?);
    let (what, them) = match paths.len() {
        1 => (first, "it"),
        2 => (format!("{first} and 1 more folder"), "them"),
        n => (format!("{first} and {} more folders", n - 1), "them"),
    };
    Some(format!("Waiting to open {what}. If macOS is asking whether Codealong may access {them}, answer on this Mac."))
}

#[cfg(test)]
mod tests {
    /// A repo that wasn't cloned for its review says why, until it is (clones.rs).
    #[test]
    fn a_repo_that_wasnt_cloned_is_a_notice() {
        use super::*;

        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        let c = crate::clones::ToClone { slug: "acme/plugins".into(), into: vec![PathBuf::from("/code/plugins")] };
        engine.on_cloned(c, Some(Err("gh repo clone into ~/code/plugins failed: offline.".into())));
        assert!(engine.dirty, "the board says so");
        let said = engine.notices().into_iter().find(|n| n.key == "clones").unwrap();
        assert_eq!(said.title, "A repo wasn't cloned for its review");
        assert!(said.text.starts_with("acme/plugins wasn't cloned to review the pull request that waits on you there: gh repo clone into ~/code/plugins failed: offline."));
        assert!(engine.compose().notices.contains(&said));
    }

    /// GitHub's notice is one of the board's, in the place it always had: after config.toml's.
    #[test]
    fn what_went_wrong_with_github_is_a_notice() {
        use super::*;

        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        engine.set_trouble(Trouble::Config, Some("config.toml doesn't read.".into()));
        engine.set_trouble(Trouble::Watch, Some("Couldn't watch 2 folders; their lanes update every 10 seconds.".into()));
        let keys = |e: &Engine| e.notices().into_iter().map(|n| n.key).collect::<Vec<_>>();
        assert_eq!(keys(&engine), ["config", "watch"]);

        engine.on_github(ghpoll::Event::Glance(Err(github::Failure::RateLimited { resets: None })));
        assert!(engine.dirty, "the board says so");
        assert_eq!(keys(&engine), ["config", "github", "watch"]);
        let said = engine.notices().remove(1);
        assert_eq!((said.title.as_str(), said.until), ("GitHub rate limit reached", None));
        assert_eq!(said.text, "GitHub: your account's rate limit is used up, so PRs wait until it resets. `gh` and your agents draw on the same limit.");
        assert!(engine.compose().notices.contains(&said));

        // The next glance that's answered ends it.
        engine.on_github(ghpoll::Event::Glance(Ok(github::Glance::default())));
        assert_eq!(keys(&engine), ["config", "watch"]);
    }

    /// Whatever goes wrong in the background is a notice of its own, with the key and the place
    /// it always had, until it goes right again.
    #[test]
    fn each_trouble_is_a_notice_until_it_goes_right() {
        use super::*;

        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        let keys = |e: &Engine| e.notices().into_iter().map(|n| n.key).collect::<Vec<_>>();
        assert!(keys(&engine).is_empty());
        for what in Trouble::ALL {
            engine.set_trouble(what, Some(format!("{what:?} went wrong.")));
            let said = engine.notices();
            assert_eq!(said.len(), 1, "{what:?}");
            assert_eq!((said[0].key.as_str(), said[0].title.as_str(), said[0].text.clone()), (what.notice().0, what.notice().1, format!("{what:?} went wrong.")));
            engine.set_trouble(what, None);
            assert!(keys(&engine).is_empty(), "{what:?}");
        }
        for what in Trouble::ALL {
            engine.set_trouble(what, Some("It went wrong.".into()));
        }
        assert_eq!(
            keys(&engine),
            ["engine", "config", "watch", "todos", "todos-lock", "state", "pr-links", "previews", "reusable", "limits", "nudges", "terminal-runs", "reconnect", "rerequests", "unlocked", "helpers", "held-messages", "emblems"]
        );
    }

    #[test]
    fn each_fall_is_a_notice_of_its_own() {
        use super::*;
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        engine.fell_over("index out of bounds");
        let first = engine.notices().remove(0);
        assert_eq!((first.key.as_str(), first.title.as_str()), ("engine", "Codealong hit a problem and carried on"));
        assert!(first.text.contains("index out of bounds"), "{}", first.text);
        engine.fell_over("a bay with nobody in it");
        let second = engine.notices().remove(0);
        // By its title a window tells it from the one it may have dismissed.
        assert_eq!(second.title, "Codealong hit a problem and carried on, 2 times");
        assert!(second.text.contains("a bay with nobody in it"));
    }

    /// A file noted in the background (where a session came from, a side conversation) has
    /// nobody waiting to hear that it couldn't be: the board says so.
    #[test]
    fn a_background_change_that_could_not_be_made_is_a_notice() {
        use super::*;

        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        assert!(engine.notices().is_empty());
        engine.store_warnings = vec![("origins.json".into(), "origins.json doesn't read as a list of where sessions came from.".into())];
        let said = engine.notices();
        assert_eq!(said.len(), 1);
        assert_eq!((said[0].key.as_str(), said[0].title.as_str(), said[0].until), ("origins.json", "origins.json has a problem", None));
        assert_eq!(said[0].text, "origins.json doesn't read as a list of where sessions came from.");
    }

    #[test]
    fn the_board_says_which_folders_it_waits_to_open() {
        use super::*;
        assert_eq!(held_warning(&[]), None);
        assert_eq!(
            held_warning(&[PathBuf::from("/Volumes/T7/wt")]).as_deref(),
            Some("Waiting to open /Volumes/T7/wt. If macOS is asking whether Codealong may access it, answer on this Mac.")
        );
        let three = held_warning(&["/Volumes/T7/a", "/Volumes/T7/b", "/Volumes/T7/c"].map(PathBuf::from)).unwrap();
        assert!(three.starts_with("Waiting to open /Volumes/T7/a and 2 more folders. "), "{three}");
    }
}

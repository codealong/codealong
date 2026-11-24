//! Cloning the repos your review is asked for in that have no checkout here. GitHub's search
//! for the PRs that wait on your review finds them in any repo (ghpoll.rs `not_here`), but the
//! board lists a PR only in a repo it has a checkout of (`board::pr_items`), and a review starts
//! in one (side.rs `pr_lane`): a PR in a repo you never cloned went unlisted and unreviewed.
//!
//! So while automatic reviews are on for such a repo (`[reviews] auto`, or its own
//! `[repos."github.com/…"]`) and a project's `repos` pattern covers it (`github.com/acme/*`),
//! it's cloned under its own name (`folder_for`) into the first `scan` folder that isn't a
//! checkout itself, where the scan would never look (`root`, `due`), with `gh repo clone`, which clones as gh's
//! git protocol for GitHub says (`run`). Not before what GitHub calls each repo here now is known
//! (ghpoll.rs `names_known`): a renamed repo's checkout would look missing under its new name,
//! and be cloned again (engine/polls.rs `clone_covered`). The repos are read again and
//! GitHub asked again then, and the PR is listed and reviewed as any other. A repo no project
//! covers stays out, as before, and nothing is cloned while reviews are off for it. A folder of
//! that name there already is read again when it's a checkout of the repo (one cloned by hand,
//! which the scan hasn't read since), and is otherwise left alone, saying so. A repo is tried once
//! an hour at most, and one that didn't clone says why as a notice (`notice`) until it does, or
//! no longer waits to.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::config::{repo_key, Config};

/// How long after a try a repo is tried again, whatever came of it.
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
/// How long a clone may take before it's given up on.
const CLONE_WAIT: Duration = Duration::from_secs(15 * 60);

/// A repo to clone: `slug` (owner/name, as GitHub names it now) into the folder `into`.
#[derive(Debug, Clone, PartialEq)]
pub struct ToClone {
    pub slug: String,
    pub into: PathBuf,
}

/// How a clone went: the repo's checkout is in its folder now, or it isn't, and why.
pub type Cloned = Result<(), String>;

/// The clones under way, when each repo was last tried, and why the ones that didn't clone
/// didn't. Pure, so its rules are tested with made-up clocks; the engine runs what `due` hands
/// out in the background (engine/polls.rs `clone_covered`).
#[derive(Default)]
pub struct Clones {
    running: HashSet<String>,
    tried: BTreeMap<String, Instant>,
    failed: BTreeMap<String, String>,
}

impl Clones {
    /// The repos to clone into `root` (the folder `root` picks) now, of those GitHub's search found PRs that wait
    /// on your review in with no checkout here (`not_here`), each marked under way. A failure is
    /// forgotten once its repo no longer waits to be cloned.
    pub fn due(&mut self, not_here: &BTreeSet<String>, cfg: &Config, root: Option<&Path>, now: Instant) -> Vec<ToClone> {
        self.failed.retain(|slug, _| not_here.contains(slug) && wanted(cfg, slug));
        let Some(root) = root else { return Vec::new() };
        let mut due = Vec::new();
        for slug in not_here.iter().filter(|s| wanted(cfg, s)) {
            let key = slug.to_lowercase();
            if self.running.contains(&key) || self.tried.get(&key).is_some_and(|t| now.duration_since(*t) < RETRY_AFTER) {
                continue;
            }
            self.tried.insert(key.clone(), now);
            let Some(folder) = folder_for(slug) else {
                self.failed.insert(slug.clone(), "its name isn't one a folder can have.".into());
                continue;
            };
            self.running.insert(key);
            due.push(ToClone { slug: slug.clone(), into: root.join(folder) });
        }
        due
    }

    /// What a clone `due` handed out came to (none when its work fell over). True when the repo
    /// is in its folder now, for the repos to be read again.
    pub fn on_cloned(&mut self, c: &ToClone, went: Option<Cloned>) -> bool {
        self.running.remove(&c.slug.to_lowercase());
        match went.unwrap_or_else(|| Err("its clone fell over".into())) {
            Ok(()) => {
                self.failed.remove(&c.slug);
                true
            }
            Err(why) => {
                self.failed.insert(c.slug.clone(), why);
                false
            }
        }
    }

    /// Why the repos that didn't clone didn't, a line each, while any is still to be.
    pub fn notice(&self) -> Option<String> {
        let lines: Vec<String> = self.failed.iter().map(|(slug, why)| format!("{slug} wasn't cloned to review the pull request that waits on you there: {why} It's tried again within the hour.")).collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }
}

/// The folder repos are cloned into: the first `scan` folder that isn't a checkout itself, whose
/// insides the scan never reads (`repos::discover`).
pub fn root(scan: &[String]) -> Option<PathBuf> {
    scan.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).map(crate::util::expand).find(|r| !r.join(".git").exists())
}

/// Whether a PR waiting on your review in `slug` has its repo cloned: automatic reviews are on for
/// it, as your table for it says if any, and a project's `repos` pattern covers it. The table is
/// found as its review finds it (teamfile.rs `RepoSettings::mine_of`), by the slug as GitHub
/// names the repo, which its clone's remote will have.
fn wanted(cfg: &Config, slug: &str) -> bool {
    let key = repo_key(slug);
    let auto = cfg.repos.get(&key).and_then(|r| r.reviews.auto).unwrap_or(cfg.reviews.auto);
    auto && cfg.projects.iter().any(|p| p.repos.iter().any(|pat| crate::repos::matches(&key, pat)))
}

/// The folder a repo is cloned into, by its name: one the scan reads, which passes over a hidden
/// folder (`.github`), `target` and `node_modules` (`repos::passed_over`), so those go in as
/// `<owner>-<name>`. None for a slug that isn't GitHub's, whose names are letters, digits, `.`,
/// `-` and `_`, so nothing reaches outside the scan folder.
fn folder_for(slug: &str) -> Option<String> {
    let (owner, name) = slug.split_once('/')?;
    let plain = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if !plain(owner) || !plain(name) || owner.starts_with('.') {
        return None;
    }
    Some(if crate::repos::passed_over(name) { format!("{owner}-{name}") } else { name.to_string() })
}

/// Clones `c`, or finds it's there: a folder of its name that's a checkout of the repo is, and
/// any other is in the way.
pub fn run(c: &ToClone) -> Cloned {
    let into = crate::util::tilde(&c.into);
    if c.into.exists() {
        let identity = crate::repos::load_checkout(&c.into).and_then(|ch| ch.identity);
        return match identity {
            Some(id) if id.eq_ignore_ascii_case(&repo_key(&c.slug)) => Ok(()),
            _ => Err(format!("{into} is there already, and isn't its checkout. Move that folder, or clone the repo into another scan folder yourself.")),
        };
    }
    let into_arg = c.into.to_string_lossy();
    crate::procs::run("gh", &["repo", "clone", &c.slug, &into_arg, "--", "--quiet"], None, CLONE_WAIT).map(|_| ()).map_err(|e| format!("gh repo clone into {into} failed: {}.", crate::util::words(e).trim_end_matches('.')))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProjectCfg, RepoCfg};

    fn cfg(auto: bool) -> Config {
        let mut cfg = Config { scan: vec!["/code".into(), "/more".into()], ..Config::default() };
        cfg.reviews.auto = auto;
        cfg.projects = vec![ProjectCfg { name: "Acme".into(), repos: vec!["github.com/acme/*".into()], folders: Vec::new(), chrome_profile: None, default: None, decisions: None }];
        cfg
    }

    /// The scan folder the tests clone into.
    fn code() -> Option<&'static Path> {
        Some(Path::new("/code"))
    }

    fn slugs(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn clones_a_repo_a_project_covers_into_the_first_scan_folder_while_reviews_are_on() {
        let mut c = Clones::default();
        let now = Instant::now();
        let due = c.due(&slugs(&["Acme/Plugins", "other/thing"]), &cfg(true), code(), now);
        assert_eq!(due, vec![ToClone { slug: "Acme/Plugins".into(), into: PathBuf::from("/code/Plugins") }], "no project covers other/thing");
        // Under way, it isn't handed out again.
        assert!(c.due(&slugs(&["Acme/Plugins"]), &cfg(true), code(), now).is_empty());
        // Nor with reviews off.
        assert!(Clones::default().due(&slugs(&["acme/plugins"]), &cfg(false), code(), now).is_empty());
    }

    #[test]
    fn a_repos_own_table_turns_reviews_on_or_off_for_it() {
        let now = Instant::now();
        let mut off = cfg(true);
        off.repos.insert("github.com/acme/quiet".into(), RepoCfg { reviews: crate::config::ReviewsPatch { auto: Some(false), ..Default::default() }, ..Default::default() });
        let due = Clones::default().due(&slugs(&["acme/quiet", "acme/loud"]), &off, code(), now);
        assert_eq!(due.iter().map(|c| c.slug.as_str()).collect::<Vec<_>>(), ["acme/loud"]);
        // As the review it's cloned for finds it.
        let settings = crate::teamfile::RepoSettings { mine: off.repos.clone(), ..Default::default() };
        assert_eq!(settings.mine_of("acme/quiet").and_then(|r| r.reviews.auto), Some(false));
        let mut on = cfg(false);
        on.repos.insert("github.com/acme/loud".into(), RepoCfg { reviews: crate::config::ReviewsPatch { auto: Some(true), ..Default::default() }, ..Default::default() });
        let due = Clones::default().due(&slugs(&["acme/quiet", "acme/loud"]), &on, code(), now);
        assert_eq!(due.iter().map(|c| c.slug.as_str()).collect::<Vec<_>>(), ["acme/loud"]);
    }

    #[test]
    fn a_repo_is_tried_once_an_hour_and_says_why_it_failed_until_it_no_longer_waits() {
        let mut c = Clones::default();
        let now = Instant::now();
        let waiting = slugs(&["acme/plugins"]);
        let first = c.due(&waiting, &cfg(true), code(), now).remove(0);
        assert!(!c.on_cloned(&first, Some(Err("Repository not found.".into()))));
        assert!(c.notice().is_some_and(|n| n.starts_with("acme/plugins wasn't cloned") && n.contains("Repository not found.")));
        assert!(c.due(&waiting, &cfg(true), code(), now + Duration::from_secs(59 * 60)).is_empty(), "not within the hour");
        let again = c.due(&waiting, &cfg(true), code(), now + RETRY_AFTER).remove(0);
        // Work that fell over is a failure too.
        assert!(!c.on_cloned(&again, None));
        assert!(c.notice().is_some_and(|n| n.contains("its clone fell over")));
        // Once it's there it's read again, and says nothing.
        let third = c.due(&waiting, &cfg(true), code(), now + RETRY_AFTER * 2).remove(0);
        assert!(c.on_cloned(&third, Some(Ok(()))));
        assert_eq!(c.notice(), None);
        // A failure goes once its repo no longer waits to be cloned.
        let fourth = c.due(&waiting, &cfg(true), code(), now + RETRY_AFTER * 3).remove(0);
        c.on_cloned(&fourth, Some(Err("offline".into())));
        c.due(&BTreeSet::new(), &cfg(true), code(), now + RETRY_AFTER * 3);
        assert_eq!(c.notice(), None);
    }

    #[test]
    fn clones_nothing_without_a_scan_folder() {
        assert!(Clones::default().due(&slugs(&["acme/plugins"]), &cfg(true), None, Instant::now()).is_empty());
    }

    #[test]
    fn a_name_the_scan_passes_over_is_cloned_with_its_owner_and_one_no_folder_can_have_says_so() {
        let mut c = Clones::default();
        let due = c.due(&slugs(&["acme/.github", "acme/target", "acme/node_modules", "acme/plugins.v2_x"]), &cfg(true), code(), Instant::now());
        let into: Vec<PathBuf> = due.iter().map(|c| c.into.clone()).collect();
        let want: Vec<PathBuf> = ["acme-.github", "acme-node_modules", "plugins.v2_x", "acme-target"].iter().map(|f| Path::new("/code").join(f)).collect();
        assert_eq!(into, want);
        assert!(into.iter().all(|f| f.file_name().is_some_and(|n| !crate::repos::passed_over(&n.to_string_lossy()))));
        assert_eq!(c.due(&slugs(&["acme/a/b", "acme/", "acme/x y"]), &cfg(true), code(), Instant::now()), vec![]);
        assert_eq!(c.notice().map(|n| n.lines().count()), Some(3), "each says why");
    }

    #[test]
    fn clones_into_the_first_scan_folder_that_isnt_a_checkout_itself() {
        let dir = std::env::temp_dir().join(format!("codealong-clone-root-{}", std::process::id()));
        let repo = dir.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let scan = |list: &[&PathBuf]| list.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert_eq!(root(&[" ".to_string()]), None);
        assert_eq!(root(&scan(&[&repo, &dir])), Some(dir.clone()), "the scan never looks inside a checkout");
        assert_eq!(root(&scan(&[&dir, &repo])), Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_in_the_way_is_left_alone_and_one_already_its_checkout_is_there() {
        let dir = std::env::temp_dir().join(format!("codealong-clones-{}", std::process::id()));
        let checkout = dir.join("plugins");
        std::fs::create_dir_all(checkout.join(".git")).unwrap();
        std::fs::write(checkout.join(".git/config"), "[remote \"origin\"]\n\turl = git@github.com:Acme/Plugins.git\n").unwrap();
        assert_eq!(run(&ToClone { slug: "acme/plugins".into(), into: checkout.clone() }), Ok(()));
        let other = dir.join("other");
        std::fs::create_dir_all(&other).unwrap();
        assert!(run(&ToClone { slug: "acme/other".into(), into: other }).is_err_and(|why| why.contains("is there already")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clones_with_gh_into_its_folder() {
        let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = asked.clone();
        crate::procs::stand_in::set(Some(Box::new(move |program, args, _, _| {
            seen.borrow_mut().push(format!("{program} {}", args.join(" ")));
            Ok(String::new())
        })));
        let went = run(&ToClone { slug: "acme/new".into(), into: PathBuf::from("/nowhere/codealong-clones/new") });
        crate::procs::stand_in::set(None);
        assert_eq!(went, Ok(()));
        assert_eq!(*asked.borrow(), ["gh repo clone acme/new /nowhere/codealong-clones/new -- --quiet"]);
    }
}

//! Cloning the repos your review is asked for in that have no checkout here. GitHub's search
//! for the PRs that wait on your review finds them in any repo (ghpoll.rs `not_here`), but the
//! board lists a PR only in a repo it has a checkout of (`board::pr_items`), and a review starts
//! in one (side.rs `pr_lane`): a PR in a repo you never cloned went unlisted and unreviewed.
//!
//! So while automatic reviews are on for such a repo (`[reviews] auto`, or its own
//! `[repos."github.com/…"]`) and a project's `repos` pattern covers it (`github.com/acme/*`),
//! it's cloned into the first `scan` folder that's there and isn't a checkout itself, where the
//! scan would never look (`root`, `due`), under its own name, else as `<owner>-<name>` where that
//! name is taken or the scan passes it over (`folder_for`), with `gh repo clone`, which clones as
//! gh's git protocol for GitHub says (`run`). One at a time, at a low priority. Not before what
//! GitHub calls each repo here now is known (ghpoll.rs `names_known`): a renamed repo's checkout
//! would look missing under its new name, and be cloned again (engine/polls.rs
//! `clone_covered`). The repos are read again and GitHub asked again then, and the PR is listed
//! and reviewed as any other. A repo no project covers stays out, as before, and nothing is
//! cloned while reviews are off for it. A folder there already that's a whole checkout of the
//! repo (one cloned by hand, which the scan hasn't read since) is read as it is. A repo is tried
//! once an hour at most, and one that didn't clone says why as a notice (`notice`) until it
//! does, or no longer waits to.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::config::{repo_key, Config};

/// How long after a try a repo is tried again, whatever came of it.
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
/// How long a clone may take before it's given up on.
const CLONE_WAIT: Duration = Duration::from_secs(15 * 60);
/// How long what a try left (`staging`) stays before a later try clears it: the `git clone` that
/// outlived its `gh` is long over by then.
const LEFT_FOR: Duration = Duration::from_secs(24 * 60 * 60);

/// A repo to clone: `slug` (owner/name, as GitHub names it now) into the first of the folders
/// `into` that's free or holds its checkout already (`run`).
#[derive(Debug, Clone, PartialEq)]
pub struct ToClone {
    pub slug: String,
    pub into: Vec<PathBuf>,
}

/// How a clone went: the repo's checkout is in a folder of the scan now, or it isn't, and why.
pub type Cloned = Result<(), String>;

/// The clone under way, when each repo was last tried, and why the ones that didn't clone
/// didn't. Pure, so its rules are tested with made-up clocks; the engine runs what `due` hands
/// out in the background (engine/polls.rs `clone_covered`).
#[derive(Default)]
pub struct Clones {
    running: Option<String>,
    tried: BTreeMap<String, Instant>,
    failed: BTreeMap<String, String>,
}

impl Clones {
    /// The repo to clone into `root` (the folder `root` picks) now, of those GitHub's search
    /// found PRs that wait on your review in with no checkout here (`not_here`), marked under
    /// way: one at a time, so two can't make for the same folder, and a first launch among an
    /// org's requests doesn't clone them all at once. One never tried goes first, then the one
    /// tried longest ago, so repos that keep failing don't keep the rest waiting. A failure is
    /// forgotten once its repo no longer waits to be cloned.
    pub fn due(&mut self, not_here: &BTreeSet<String>, cfg: &Config, root: Option<&Path>, now: Instant) -> Option<ToClone> {
        self.failed.retain(|slug, _| not_here.contains(slug) && wanted(cfg, slug));
        let root = root.filter(|_| self.running.is_none())?;
        let mut waiting: Vec<(Option<Instant>, &String)> = not_here.iter().filter(|s| wanted(cfg, s)).map(|s| (self.tried.get(&s.to_lowercase()).copied(), s)).filter(|(t, _)| t.is_none_or(|t| now.duration_since(t) >= RETRY_AFTER)).collect();
        waiting.sort();
        for (_, slug) in waiting {
            self.tried.insert(slug.to_lowercase(), now);
            let Some(folders) = folder_for(slug) else {
                self.failed.insert(slug.clone(), "its name isn't one a folder can have.".into());
                continue;
            };
            self.running = Some(slug.clone());
            return Some(ToClone { slug: slug.clone(), into: folders.iter().map(|f| root.join(f)).collect() });
        }
        None
    }

    /// What a clone `due` handed out came to (none when its work fell over). True when the repo
    /// is in a folder of the scan now, for the repos to be read again.
    pub fn on_cloned(&mut self, c: &ToClone, went: Option<Cloned>) -> bool {
        self.running = None;
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

/// The folder repos are cloned into: the first `scan` folder that's there (not a drive that isn't
/// plugged in, which git would make a folder for on this one) and isn't a checkout itself, whose
/// insides the scan never reads (`repos::discover`).
pub fn root(scan: &[String]) -> Option<PathBuf> {
    scan.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).map(crate::util::expand).find(|r| r.is_dir() && !r.join(".git").exists())
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

/// The folders a repo may be cloned into, in order: its name, then `<owner>-<name>` for when
/// another repo or folder has that name already (`api`, `docs`). Only the second for a name the
/// scan passes over: a hidden folder (`.github`), `target` and `node_modules`
/// (`repos::passed_over`). None for a slug that isn't GitHub's, whose names are letters, digits,
/// `.`, `-` and `_`, so nothing reaches outside the scan folder.
fn folder_for(slug: &str) -> Option<Vec<String>> {
    let (owner, name) = slug.split_once('/')?;
    let plain = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if !plain(owner) || !plain(name) || owner.starts_with('.') {
        return None;
    }
    let owned = format!("{owner}-{name}");
    Some(if crate::repos::passed_over(name) { vec![owned] } else { vec![name.to_string(), owned] })
}

/// Clones `c` with `gh`, at a low priority, as nobody waits on it.
pub fn run(c: &ToClone) -> Cloned {
    let attempt = format!("{}-{}", std::process::id(), crate::util::now_ms());
    run_with(c, &|program, args| crate::procs::run_background(program, args, None, CLONE_WAIT), &attempt, SystemTime::now())
}

/// The hidden folder beside `into` that try `attempt` makes its clone in (`run_with`), which the
/// scan passes over (`repos::passed_over`), so a clone cut off partway (Codealong quitting, a
/// timeout, which stops `gh` and not the `git clone` it started) is never read as the repo. Each
/// try has its own: a `git clone` that outlived its `gh` may still be writing to an earlier one.
fn staging(into: &Path, attempt: &str) -> PathBuf {
    into.with_file_name(format!(".{}.codealong-clone-{attempt}", into.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()))
}

/// Clears what tries made beside `into` (`staging`) that was last written `LEFT_FOR` or more
/// before `now`. One that won't go is left: it's only space, hidden from the scan, and the next
/// try clears it again.
fn clear_left(into: &Path, now: SystemTime) {
    let (Some(parent), Some(name)) = (into.parent(), into.file_name()) else { return };
    let prefix = format!(".{}.codealong-clone-", name.to_string_lossy());
    let Ok(entries) = std::fs::read_dir(parent) else { return };
    for e in entries.flatten().filter(|e| e.file_name().to_string_lossy().starts_with(&prefix)) {
        let old = e.metadata().and_then(|m| m.modified()).is_ok_and(|at| now.duration_since(at).is_ok_and(|age| age >= LEFT_FOR));
        if old {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// Clones `c` into the first of its folders that's free, or finds it there already: a folder that
/// holds the repo's checkout, whole (its HEAD a commit: one cloned by hand and cut off has none
/// yet), is it, and one that holds anything else is passed over. The clone is made in a hidden
/// folder beside it of this try's own (`staging`, `attempt`), and moved into place once `gh` is
/// done; what tries a day before `now` left there is cleared first (`clear_left`). `exec` runs a
/// program, as `run` does.
fn run_with(c: &ToClone, exec: &dyn Fn(&str, &[&str]) -> anyhow::Result<String>, attempt: &str, now: SystemTime) -> Cloned {
    for into in &c.into {
        let at = crate::util::tilde(into);
        if !into.exists() {
            clear_left(into, now);
            let made = staging(into, attempt);
            let arg = made.to_string_lossy();
            exec("gh", &["repo", "clone", &c.slug, &arg, "--", "--quiet"]).map_err(|e| format!("gh repo clone into {} failed: {}.", crate::util::tilde(&made), crate::util::words(e).trim_end_matches('.')))?;
            return std::fs::rename(&made, into).map_err(|e| format!("moving its clone to {at} failed: {e}."));
        }
        let identity = crate::repos::load_checkout(into).and_then(|ch| ch.identity);
        if identity.is_some_and(|id| id.eq_ignore_ascii_case(&repo_key(&c.slug))) {
            let path = into.to_string_lossy();
            return match exec("git", &["-C", &path, "rev-parse", "--verify", "--quiet", "HEAD"]) {
                Ok(_) => Ok(()),
                Err(_) => Err(format!("{at} is a clone of it that didn't finish. Remove that folder and it's cloned again.")),
            };
        }
    }
    let taken: Vec<String> = c.into.iter().map(|p| crate::util::tilde(p)).collect();
    Err(format!("{} {} there already, and not its checkout. Move one, or clone the repo into a scan folder yourself.", taken.join(" and "), if taken.len() == 1 { "is" } else { "are" }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProjectCfg, RepoCfg};
    use std::cell::RefCell;

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

    fn at(folders: &[&str]) -> Vec<PathBuf> {
        folders.iter().map(|f| Path::new("/code").join(f)).collect()
    }

    /// A folder of its own for a test, gone first if a run before left it.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("codealong-clones-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A checkout of `slug` in `dir`, as far as its `.git` says.
    fn checkout_of(dir: &Path, slug: &str) {
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join(".git/config"), format!("[remote \"origin\"]\n\turl = git@github.com:{slug}.git\n")).unwrap();
    }

    #[test]
    fn clones_a_repo_a_project_covers_into_the_first_scan_folder_while_reviews_are_on() {
        let mut c = Clones::default();
        let now = Instant::now();
        let due = c.due(&slugs(&["Acme/Plugins", "other/thing"]), &cfg(true), code(), now);
        assert_eq!(due, Some(ToClone { slug: "Acme/Plugins".into(), into: at(&["Plugins", "Acme-Plugins"]) }), "no project covers other/thing");
        // Nor with reviews off.
        assert_eq!(Clones::default().due(&slugs(&["acme/plugins"]), &cfg(false), code(), now), None);
    }

    #[test]
    fn repos_that_keep_failing_dont_keep_the_rest_waiting() {
        let mut c = Clones::default();
        let now = Instant::now();
        let mut tried = Vec::new();
        let mut tick = |c: &mut Clones, waiting: &[&str], minutes: u64| {
            if let Some(due) = c.due(&slugs(waiting), &cfg(true), code(), now + Duration::from_secs(60 * minutes)) {
                tried.push(due.slug.clone());
                c.on_cloned(&due, Some(Err("refused".into())));
            }
        };
        tick(&mut c, &["acme/a", "acme/b"], 0);
        tick(&mut c, &["acme/a", "acme/b"], 10);
        // An hour on, acme/a may be tried again, but acme/z never was.
        tick(&mut c, &["acme/a", "acme/b", "acme/z"], 60);
        tick(&mut c, &["acme/a", "acme/b", "acme/z"], 70);
        tick(&mut c, &["acme/a", "acme/b", "acme/z"], 80);
        assert_eq!(tried, ["acme/a", "acme/b", "acme/z", "acme/a", "acme/b"], "one never tried, then the one tried longest ago");
    }

    #[test]
    fn one_repo_is_cloned_at_a_time() {
        let mut c = Clones::default();
        let now = Instant::now();
        let waiting = slugs(&["acme/api", "acme/web"]);
        let first = c.due(&waiting, &cfg(true), code(), now).unwrap();
        assert_eq!(first.slug, "acme/api");
        assert_eq!(c.due(&waiting, &cfg(true), code(), now), None, "not while one is under way");
        c.on_cloned(&first, Some(Ok(())));
        assert_eq!(c.due(&waiting, &cfg(true), code(), now).map(|c| c.slug), Some("acme/web".into()));
    }

    #[test]
    fn a_repos_own_table_turns_reviews_on_or_off_for_it() {
        let now = Instant::now();
        let table = |auto| RepoCfg { reviews: crate::config::ReviewsPatch { auto: Some(auto), ..Default::default() }, ..Default::default() };
        let mut off = cfg(true);
        off.repos.insert("github.com/acme/quiet".into(), table(false));
        assert_eq!(Clones::default().due(&slugs(&["acme/quiet", "acme/loud"]), &off, code(), now).map(|c| c.slug), Some("acme/loud".into()));
        // As the review it's cloned for finds it.
        let settings = crate::teamfile::RepoSettings { mine: off.repos.clone(), ..Default::default() };
        assert_eq!(settings.mine_of("acme/quiet").and_then(|r| r.reviews.auto), Some(false));
        let mut on = cfg(false);
        on.repos.insert("github.com/acme/loud".into(), table(true));
        assert_eq!(Clones::default().due(&slugs(&["acme/quiet", "acme/loud"]), &on, code(), now).map(|c| c.slug), Some("acme/loud".into()));
    }

    #[test]
    fn a_repo_is_tried_once_an_hour_and_says_why_it_failed_until_it_no_longer_waits() {
        let mut c = Clones::default();
        let now = Instant::now();
        let waiting = slugs(&["acme/plugins"]);
        let first = c.due(&waiting, &cfg(true), code(), now).unwrap();
        assert!(!c.on_cloned(&first, Some(Err("Repository not found.".into()))));
        assert!(c.notice().is_some_and(|n| n.starts_with("acme/plugins wasn't cloned") && n.contains("Repository not found.")));
        assert_eq!(c.due(&waiting, &cfg(true), code(), now + Duration::from_secs(59 * 60)), None, "not within the hour");
        let again = c.due(&waiting, &cfg(true), code(), now + RETRY_AFTER).unwrap();
        // Work that fell over is a failure too.
        assert!(!c.on_cloned(&again, None));
        assert!(c.notice().is_some_and(|n| n.contains("its clone fell over")));
        // Once it's there it's read again, and says nothing.
        let third = c.due(&waiting, &cfg(true), code(), now + RETRY_AFTER * 2).unwrap();
        assert!(c.on_cloned(&third, Some(Ok(()))));
        assert_eq!(c.notice(), None);
        // A failure goes once its repo no longer waits to be cloned.
        let fourth = c.due(&waiting, &cfg(true), code(), now + RETRY_AFTER * 3).unwrap();
        c.on_cloned(&fourth, Some(Err("offline".into())));
        c.due(&BTreeSet::new(), &cfg(true), code(), now + RETRY_AFTER * 3);
        assert_eq!(c.notice(), None);
    }

    #[test]
    fn clones_nothing_without_a_scan_folder() {
        assert_eq!(Clones::default().due(&slugs(&["acme/plugins"]), &cfg(true), None, Instant::now()), None);
    }

    #[test]
    fn a_name_the_scan_passes_over_is_cloned_with_its_owner_and_one_no_folder_can_have_says_so() {
        for (slug, folders) in [("acme/.github", vec!["acme-.github"]), ("acme/target", vec!["acme-target"]), ("acme/node_modules", vec!["acme-node_modules"]), ("acme/plugins.v2_x", vec!["plugins.v2_x", "acme-plugins.v2_x"])] {
            let due = Clones::default().due(&slugs(&[slug]), &cfg(true), code(), Instant::now()).unwrap();
            assert_eq!(due.into, at(&folders), "{slug}");
            assert!(due.into.iter().all(|f| f.file_name().is_some_and(|n| !crate::repos::passed_over(&n.to_string_lossy()))));
        }
        let mut c = Clones::default();
        assert_eq!(c.due(&slugs(&["acme/a/b", "acme/", "acme/x y"]), &cfg(true), code(), Instant::now()), None);
        assert_eq!(c.notice().map(|n| n.lines().count()), Some(3), "each says why");
    }

    #[test]
    fn clones_into_the_first_scan_folder_that_is_there_and_isnt_a_checkout_itself() {
        let dir = scratch("root");
        let repo = dir.join("repo");
        checkout_of(&repo, "acme/repo");
        let gone = dir.join("unplugged");
        let scan = |list: &[&PathBuf]| list.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert_eq!(root(&[" ".to_string()]), None);
        assert_eq!(root(&scan(&[&repo, &dir])), Some(dir.clone()), "the scan never looks inside a checkout");
        assert_eq!(root(&scan(&[&gone, &dir])), Some(dir.clone()), "nor in a folder that isn't there");
        assert_eq!(root(&scan(&[&dir, &repo])), Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Runs `c` with programs that answer as `answer` says, and what was run. A clone that
    /// answers makes its folder, as `gh` would.
    fn ran(c: &ToClone, answer: impl Fn(&str) -> bool) -> (Cloned, Vec<String>) {
        ran_at(c, answer, "t1", SystemTime::now())
    }

    /// `ran`, as try `attempt` at `now`.
    fn ran_at(c: &ToClone, answer: impl Fn(&str) -> bool, attempt: &str, now: SystemTime) -> (Cloned, Vec<String>) {
        let asked = RefCell::new(Vec::new());
        let went = run_with(c, &|program, args| {
            let line = format!("{program} {}", args.join(" "));
            asked.borrow_mut().push(line.clone());
            if !answer(&line) {
                return Err(anyhow::anyhow!("{program} failed"));
            }
            if args.starts_with(&["repo", "clone"]) {
                std::fs::create_dir_all(Path::new(args[3]).join(".git")).unwrap();
            }
            Ok(String::new())
        }, attempt, now);
        (went, asked.into_inner())
    }

    #[test]
    fn clones_with_gh_beside_the_first_free_folder_and_moves_it_there_once_done() {
        let dir = scratch("free");
        let (api, owned) = (dir.join("api"), dir.join("acme-api"));
        let c = ToClone { slug: "acme/api".into(), into: vec![api.clone(), owned.clone()] };
        let (went, asked) = ran(&c, |_| true);
        assert_eq!(went, Ok(()));
        assert_eq!(asked, [format!("gh repo clone acme/api {} -- --quiet", dir.join(".api.codealong-clone-t1").to_string_lossy())]);
        assert!(api.join(".git").is_dir() && !dir.join(".api.codealong-clone-t1").exists());
        assert!(crate::repos::passed_over(".api.codealong-clone-t1"), "the scan never reads one under way");
        // Another repo's checkout, or any other folder, of its name: the next one.
        checkout_of(&api, "beta/api");
        let (_, asked) = ran(&c, |_| true);
        assert_eq!(asked, [format!("gh repo clone acme/api {} -- --quiet", dir.join(".acme-api.codealong-clone-t1").to_string_lossy())]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_clone_cut_off_is_left_where_the_scan_never_reads_it_until_a_day_on() {
        let dir = scratch("cut");
        let api = dir.join("api");
        let c = ToClone { slug: "acme/api".into(), into: vec![api.clone()] };
        let (went, _) = ran_at(&c, |line| !line.starts_with("gh"), "t1", SystemTime::now());
        assert!(went.is_err_and(|why| why.contains("gh repo clone into")));
        // As though its `git clone` outlived `gh`, still writing there.
        let left = dir.join(".api.codealong-clone-t1");
        checkout_of(&left, "acme/api");
        assert!(!api.exists(), "nothing's in its place");
        // The next try clones beside it, leaving it be.
        let (went, asked) = ran_at(&c, |_| true, "t2", SystemTime::now());
        assert_eq!(went, Ok(()));
        assert!(asked[0].contains(".api.codealong-clone-t2") && left.exists());
        // A day on, a try clears it.
        std::fs::remove_dir_all(&api).unwrap();
        let (went, _) = ran_at(&c, |_| true, "t3", SystemTime::now() + LEFT_FOR);
        assert_eq!(went, Ok(()));
        assert!(!left.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_whole_checkout_of_it_is_there_one_cut_off_isnt_and_folders_in_the_way_say_so() {
        let dir = scratch("there");
        let (api, owned) = (dir.join("api"), dir.join("acme-api"));
        let c = ToClone { slug: "acme/api".into(), into: vec![api.clone(), owned.clone()] };
        checkout_of(&api, "beta/api");
        checkout_of(&owned, "Acme/API");
        let (went, asked) = ran(&c, |_| true);
        assert_eq!(went, Ok(()), "a checkout of it already, in any case");
        assert_eq!(asked, [format!("git -C {} rev-parse --verify --quiet HEAD", owned.to_string_lossy())]);
        let (went, _) = ran(&c, |line| !line.contains("rev-parse"));
        assert!(went.is_err_and(|why| why.contains("didn't finish")), "a clone cut off partway has no HEAD yet");
        std::fs::remove_dir_all(&owned).unwrap();
        std::fs::create_dir_all(&owned).unwrap();
        let (went, asked) = ran(&c, |_| true);
        assert!(went.is_err_and(|why| why.contains("are there already")));
        assert!(asked.is_empty(), "nothing's cloned over a folder");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

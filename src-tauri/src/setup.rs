//! First run, and the setup wizard (src/ui/SetupDialog.tsx) that has you confirm what it found:
//! the agent accounts on this Mac, the folders to search for repos, and the apps to open things
//! in. A first run writes config.toml from what it finds (engine.rs `first_run_config`, with
//! `worked_repos`, `scan_folders` and `first_editor` here); the wizard then shows over the main
//! window until it's finished or skipped (state.json's `setupDone`), and Set up Codealong… in the
//! app menu shows it again.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{self, AccountCfg};
use crate::detect::{self, Found};
use crate::held::Held;
use crate::util::{expand_in, home, now_ms, tilde_in, DAY_MS};

/// How far back the folders your agents worked in count.
const WORKED_DAYS: i64 = 30;

/// How long the wizard waits on a folder's repos being counted: a folder macOS holds for a
/// privacy prompt shows without a count rather than keep the wizard waiting.
const COUNT_WAIT: Duration = Duration::from_secs(5);

/// How long the wizard waits for a first run to write config.toml (`view_once_read`).
const FIRST_RUN_WAIT: Duration = Duration::from_secs(120);

/// Folders people keep code in, right in their home folder, matched whatever their case. None
/// that macOS guards with a privacy prompt (Documents, Desktop): looking in one would ask.
const USUAL: &[&str] = &["projects", "code", "src", "dev", "developer", "work", "git", "github", "repos", "workspace"];

/// What the setup wizard offers.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    /// The accounts config.toml lists, in its order, then the others on this Mac.
    pub accounts: Vec<Account>,
    /// The folders config.toml searches, then the ones your agents worked in and the usual
    /// places for code that hold repos.
    pub folders: Vec<Folder>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    #[serde(flatten)]
    pub found: Found,
    /// config.toml lists it.
    pub listed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    /// With `~` for your home folder, as config.toml's `scan` writes it.
    pub path: String,
    /// It's there: config.toml can list one that isn't.
    pub exists: bool,
    /// The repos in it, three levels deep, as the board finds them; none when it couldn't be
    /// read in time.
    pub repos: Option<usize>,
    /// Of those, the ones your agents worked in lately.
    pub worked: usize,
    /// config.toml's `scan` lists it.
    pub scanned: bool,
}

/// What the setup wizard offers, once the app's engine has read config.toml or written a first
/// run's. Offered before, it would start from the defaults, and Done would save them over what
/// the first run found (its projects). A first run's reads take seconds; the wizard says it's
/// looking meanwhile, and after a couple of minutes goes on with what there is.
pub fn view_once_read() -> anyhow::Result<View> {
    let start = Instant::now();
    while !config::read_by_the_app() && start.elapsed() < FIRST_RUN_WAIT {
        std::thread::sleep(Duration::from_millis(100));
    }
    view()
}

/// What the setup wizard offers. An error when config.toml doesn't parse: every account on this
/// Mac would read as unlisted.
pub fn view() -> anyhow::Result<View> {
    let cfg = config::current()?;
    let accounts = accounts_of(detect::find_all(), &cfg.accounts, &crate::state::known_accounts());
    let everyone: Vec<AccountCfg> = accounts.iter().map(|a| a.found.account()).collect();
    let worked = worked_repos(&everyone);
    let h = home();
    let mut paths: Vec<PathBuf> = cfg.scan.iter().map(|s| crate::util::expand(s)).collect();
    paths.extend(scan_folders(worked.keys().map(PathBuf::as_path), &h));
    paths.extend(usual_folders(&h));
    let mut seen = HashSet::new();
    paths.retain(|p| seen.insert(p.clone()));
    let roots = |root: &Path| crate::repos::discover(&[root.to_path_buf()], 3).into_iter().map(|c| c.root).collect::<Vec<_>>();
    let repos = Held::new("scan root", roots, || {}).read(paths.clone(), COUNT_WAIT);
    Ok(View { accounts, folders: folders_of(&paths, &cfg.scan, &repos, &worked, &h, &|p: &Path| p.is_dir()) })
}

/// Whether the main window shows the wizard by itself: state.json hasn't been through it, or
/// there's no state.json yet. Read from the file rather than asked of the engine, which a first
/// run's scan can keep busy for many seconds. A state.json that doesn't read isn't a first run's.
pub fn needed() -> bool {
    crate::state::SeenState::load().is_ok_and(|s| !s.setup_done)
}

/// The repos your agents worked in lately, by main checkout, with how many of their sessions did.
pub fn worked_repos(accounts: &[AccountCfg]) -> BTreeMap<PathBuf, usize> {
    let since = now_ms() - WORKED_DAYS * DAY_MS;
    let mut cwds: Vec<String> = Vec::new();
    for a in accounts {
        if let Some(h) = crate::harness::of(a) {
            cwds.extend(h.session_cwds(&a.home_path(), since));
        }
    }
    // A folder macOS holds for a privacy prompt is left out rather than waited on (held.rs).
    let located = Held::new("session folder", crate::repos::locate, || {}).read(cwds.iter().map(PathBuf::from), crate::engine::HELD_WAIT);
    let mut per_repo: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for cwd in &cwds {
        if let Some(Some((_, main))) = located.get(Path::new(cwd)) {
            *per_repo.entry(main.clone()).or_default() += 1;
        }
    }
    per_repo
}

/// The folders to search so that these repos are found: each one's parent, or the repo itself
/// where that's your home folder (searching all of it would be slow, and would ask for Documents
/// and Desktop), the folders with the most repos first. One inside another that already finds
/// its repos is left out: the search goes three levels deep. Repos outside your home folder or
/// in a hidden folder (an agent's scratch clone in `~/.claude`, say) aren't where you keep code.
pub fn scan_folders<'a>(repos: impl IntoIterator<Item = &'a Path>, home: &Path) -> Vec<PathBuf> {
    // A repo of your home folder itself (dotfiles) is left out with the rest of what's outside it.
    let yours = |repo: &Path| repo.strip_prefix(home).is_ok_and(|rest| rest.components().next().is_some() && !rest.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.')));
    let mut counts: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for repo in repos.into_iter().filter(|r| yours(r)) {
        let folder = match repo.parent() {
            Some(p) if p != home => p.to_path_buf(),
            _ => repo.to_path_buf(),
        };
        *counts.entry(folder).or_default() += 1;
    }
    // Shallowest first, each kept unless one kept already finds its repos: they're one level
    // below it, so a folder at most two above it does, three levels deep.
    let mut by_depth: Vec<(PathBuf, usize)> = counts.into_iter().collect();
    by_depth.sort_by_key(|(f, _)| f.components().count());
    let mut kept: Vec<(PathBuf, usize)> = Vec::new();
    for (f, n) in by_depth {
        if !kept.iter().any(|(o, _)| f.strip_prefix(o).is_ok_and(|rest| rest.components().count() <= 2)) {
            kept.push((f, n));
        }
    }
    kept.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    kept.into_iter().map(|(f, _)| f).collect()
}

/// The editor a first run starts config.toml with: the first one installed, in the Open in
/// menus' order, or none when there's none.
pub fn first_editor() -> Option<String> {
    crate::editors::installed().into_iter().next().map(|p| p.id)
}

/// The usual folders for code that are in your home folder, with their names as they're written.
fn usual_folders(h: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(h) else { return Vec::new() };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()) && USUAL.contains(&e.file_name().to_string_lossy().to_lowercase().as_str()))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

/// The accounts config.toml lists, each as this Mac has it when it has it, then the ones it
/// doesn't, named apart from them as Settings names them (`detect::not_configured`).
fn accounts_of(found: Vec<Found>, config: &[AccountCfg], known: &[String]) -> Vec<Account> {
    let mut rows: Vec<Account> = config
        .iter()
        .map(|a| {
            let key = detect::folder_key(&a.home);
            let found = match found.iter().find(|f| detect::folder_key(&f.home) == key) {
                Some(f) => Found { name: a.name.clone(), home: a.home.clone(), command: a.command.clone(), new: false, ..f.clone() },
                None => Found::of_listed(a),
            };
            Account { found, listed: true }
        })
        .collect();
    rows.extend(detect::not_configured(found, config, known).into_iter().map(|found| Account { found, listed: false }));
    rows
}

/// The rows for the folders, in the order they're offered: the ones config.toml searches as it
/// lists them, then the rest, those your agents worked in most first. A suggestion without a repo
/// in it is no suggestion.
fn folders_of(paths: &[PathBuf], scan: &[String], repos: &HashMap<PathBuf, Vec<PathBuf>>, worked: &BTreeMap<PathBuf, usize>, h: &Path, exists: &dyn Fn(&Path) -> bool) -> Vec<Folder> {
    let scanned: HashSet<PathBuf> = scan.iter().map(|s| expand_in(s, h)).collect();
    let mut rows: Vec<Folder> = paths
        .iter()
        .map(|p| {
            let found = repos.get(p);
            Folder {
                path: tilde_in(p, h),
                exists: exists(p),
                repos: found.map(Vec::len),
                worked: found.map(|rs| rs.iter().filter(|r| worked.contains_key(*r)).count()).unwrap_or(0),
                scanned: scanned.contains(p),
            }
        })
        .filter(|f| f.scanned || (f.exists && f.repos != Some(0)))
        .collect();
    rows.sort_by_key(|f| if f.scanned { (0, 0, 0) } else { (1, usize::MAX - f.worked, usize::MAX - f.repos.unwrap_or(0)) });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn folders_to_search_are_the_repos_parents() {
        let h = p("/Users/me");
        let repos = [p("/Users/me/projects/a"), p("/Users/me/projects/b"), p("/Users/me/work/c"), p("/Users/me/dotfiles"), p("/Users/me/.claude/jobs/1/tmp/x"), p("/tmp/scratch/y"), p("/Users/me")];
        let got = scan_folders(repos.iter().map(PathBuf::as_path), &h);
        assert_eq!(got, [p("/Users/me/projects"), p("/Users/me/dotfiles"), p("/Users/me/work")], "most repos first, a repo right in the home folder searched itself, and none hidden, outside it or of it");
    }

    #[test]
    fn a_folder_inside_one_that_finds_its_repos_is_left_out() {
        let h = p("/Users/me");
        let repos = [p("/Users/me/code/a"), p("/Users/me/code/clients/b"), p("/Users/me/code/x/y/z/deep")];
        let got = scan_folders(repos.iter().map(PathBuf::as_path), &h);
        assert_eq!(got, [p("/Users/me/code"), p("/Users/me/code/x/y/z")], "clients is two levels in and found from code; x/y/z is too deep to be");
        // Left out only for one that's kept: code finds a/b's repos, not a/b/c/d's.
        let chain = [p("/Users/me/code/r"), p("/Users/me/code/a/b/r"), p("/Users/me/code/a/b/c/d/r")];
        let got = scan_folders(chain.iter().map(PathBuf::as_path), &h);
        assert_eq!(got, [p("/Users/me/code"), p("/Users/me/code/a/b/c/d")]);
    }

    fn found(name: &str, harness: &str, home: &str) -> Found {
        Found { name: name.into(), harness: harness.into(), home: home.into(), command: None, via: None, state: detect::State::Active, signed_in: true, folder_exists: true, email: Some("you@example.com".into()), sign_in_note: None, last_used: Some(1), new: true }
    }

    #[test]
    fn listed_accounts_come_first_with_their_own_names_then_the_others() {
        let on_mac = vec![found("claude", "claude", "~/.claude"), found("codex", "codex", "~/.codex"), found("claude-work", "claude", "~/.claude-work")];
        let config = vec![AccountCfg { name: "work".into(), ..found("claude-work", "claude", "~/.claude-work").account() }, found("gone", "codex", "~/.codex-gone").account()];
        let rows = accounts_of(on_mac, &config, &[]);
        let said: Vec<(&str, bool, bool)> = rows.iter().map(|r| (r.found.name.as_str(), r.listed, r.found.signed_in)).collect();
        assert_eq!(said, [("work", true, true), ("gone", true, false), ("claude", false, true), ("codex", false, true)]);
        assert!(!rows[0].found.new && rows[2].found.new, "a listed account isn't news");
    }

    #[test]
    fn folders_listed_first_then_where_agents_worked_and_none_without_repos() {
        let h = p("/Users/me");
        let paths = [p("/Users/me/projects"), p("/Users/me/gone"), p("/Users/me/code"), p("/Users/me/work"), p("/Users/me/src")];
        let scan = vec!["~/projects".to_string(), "~/gone".to_string()];
        let repos: HashMap<PathBuf, Vec<PathBuf>> = HashMap::from([
            (p("/Users/me/projects"), vec![p("/Users/me/projects/a")]),
            (p("/Users/me/code"), vec![p("/Users/me/code/b"), p("/Users/me/code/c")]),
            (p("/Users/me/work"), vec![p("/Users/me/work/d")]),
            (p("/Users/me/src"), vec![]),
        ]);
        let worked = BTreeMap::from([(p("/Users/me/work/d"), 3)]);
        let rows = folders_of(&paths, &scan, &repos, &worked, &h, &|p: &Path| !p.ends_with("gone"));
        let said: Vec<(&str, bool, Option<usize>, usize, bool)> = rows.iter().map(|f| (f.path.as_str(), f.exists, f.repos, f.worked, f.scanned)).collect();
        assert_eq!(said, [("~/projects", true, Some(1), 0, true), ("~/gone", false, None, 0, true), ("~/work", true, Some(1), 1, false), ("~/code", true, Some(2), 0, false)]);
    }
}

//! A repo's settings for its team: `.codealong/config.toml` in the repo, which every teammate's
//! Codealong reads for that repo's pull requests. It sets what goes for everyone there: words added
//! to what a reviewer is told (`instructions`, or `instructions_file`, a Markdown file beside
//! it), the verdicts never submitted as anyone's review (`never_submit`), and what a nudge tells
//! about (`nudge_about`). What spends a person's usage or posts as them (reviews turned on, where
//! they go, nudges by default, asking for a re-review after fixes) the team only suggests: each
//! person takes it, which writes it into their own config.toml (`config::take_suggestions`), or
//! doesn't. Accounts, models, a person's own prompt and what's submitted as them are theirs alone,
//! and a file that sets one of them is an error and goes unused, as one that doesn't parse does. A key this Codealong doesn't know
//! (one from a newer Codealong) is passed over and said, so teammates on other builds still read
//! the rest.
//!
//! The file is read from the branch a pull request merges into, as this Mac last fetched it
//! (`refs/remotes/origin/<branch>`), never from a worktree: a review runs in the worktree that
//! holds the PR's branch, and a PR mustn't write its own reviewer's instructions or turn on what
//! goes for it. So a change to the file goes for the team once it's merged. Settings shows the
//! default branch's. It's read again when a checkout's refs move (a fetch) and every few minutes
//! in case the watcher missed one (`TeamFiles`).

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config::{NudgeAbout, OwnReviewsPatch, PullRequestsCfg, PullRequestsPatch, RepoCfg, ReviewPost, ReviewsCfg};
use crate::model::PrItem;
use crate::github::Verdict;

/// Where the team's settings are in a repo.
pub const FILE: &str = ".codealong/config.toml";
const DIR: &str = ".codealong";
/// The most words a team can add to a reviewer's instructions: they're typed into each review.
const MOST_WORDS: usize = 16_000;
/// How often every repo's file is looked at again, when no fetch said its refs moved.
const EVERY: Duration = Duration::from_secs(5 * 60);
const GIT_TIMEOUT: Duration = Duration::from_secs(15);

/// What a team's file sets.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TeamCfg {
    /// Added to what the reviewer of a PR here that waits on your review is told (`[reviews]`).
    pub review_instructions: String,
    /// Added to what the reviewer of a PR of yours here is told (`[reviews.own]`).
    pub own_review_instructions: String,
    /// Verdicts never submitted as anyone's review here, whatever their `[reviews] submit` says.
    pub never_submit: Vec<Verdict>,
    /// What a nudge tells a PR's session about, for everyone.
    pub nudge_about: Option<NudgeAbout>,
    /// What the team suggests each person turns on, for them to take or not.
    pub suggests: RepoCfg,
}

/// A team's file as read: what it sets, and the keys it has that were passed over.
pub type Parsed = Result<(TeamCfg, Vec<String>), String>;

/// What each top-level table a person has in config.toml is, when a team's file has it: theirs.
const PERSONAL_TABLES: [&str; 14] = ["accounts", "apps", "appearance", "shortcuts", "projects", "jev", "search", "devices", "updates", "limits", "sessions", "debug", "scan", "repos"];

/// Reads a team's file: `text`, with `file` reading another file in `.codealong/` by its path in
/// the repo (none when there's no such file). An error says what's wrong, and the file goes unused.
pub fn parse(text: &str, file: &dyn Fn(&str) -> Result<Option<String>, String>) -> Parsed {
    let table: toml::Table = text.parse().map_err(|e: toml::de::Error| e.message().trim().to_string())?;
    let mut cfg = TeamCfg::default();
    let mut skipped = Vec::new();
    for (key, value) in &table {
        match key.as_str() {
            "reviews" => reviews(table_of(value, "[reviews]")?, &mut cfg, &mut skipped, file)?,
            "pull_requests" => pull_requests(table_of(value, "[pull_requests]")?, &mut cfg, &mut skipped)?,
            k if PERSONAL_TABLES.contains(&k) => return Err(format!("{k} is each person's own: it goes in their config.toml, not the repo's.")),
            k => skipped.push(k.to_string()),
        }
    }
    Ok((cfg, skipped))
}

fn table_of<'a>(value: &'a toml::Value, name: &str) -> Result<&'a toml::Table, String> {
    value.as_table().ok_or_else(|| format!("{name} should be a table."))
}

fn typed<T: serde::de::DeserializeOwned>(value: &toml::Value, name: &str) -> Result<T, String> {
    value.clone().try_into().map_err(|e: toml::de::Error| format!("{name}: {}", e.message().trim()))
}

fn reviews(t: &toml::Table, cfg: &mut TeamCfg, skipped: &mut Vec<String>, file: &dyn Fn(&str) -> Result<Option<String>, String>) -> Result<(), String> {
    let s = &mut cfg.suggests.reviews;
    for (key, value) in t {
        let name = format!("[reviews] {key}");
        match key.as_str() {
            "auto" => s.auto = Some(typed(value, &name)?),
            "post" => s.post = Some(typed::<ReviewPost>(value, &name)?),
            "never_submit" => cfg.never_submit = typed(value, &name)?,
            "instructions" | "instructions_file" => {}
            "own" => own(table_of(value, "[reviews.own]")?, &mut s.own, &mut cfg.own_review_instructions, skipped, file)?,
            "account" | "model" | "effort" => return Err(format!("{name} is each person's own: the accounts and models reviews run on are theirs, in their config.toml.")),
            "prompt" => return Err(format!("{name} is each person's own. The team's words go in [reviews] instructions, which are added to it.")),
            "submit" => return Err(format!("{name} is each person's own: what's submitted as their review is theirs to say. never_submit keeps a verdict from being submitted.")),
            _ => skipped.push(format!("reviews.{key}")),
        }
    }
    cfg.review_instructions = instructions(t, "[reviews]", file)?;
    Ok(())
}

fn own(t: &toml::Table, s: &mut OwnReviewsPatch, words: &mut String, skipped: &mut Vec<String>, file: &dyn Fn(&str) -> Result<Option<String>, String>) -> Result<(), String> {
    for (key, value) in t {
        let name = format!("[reviews.own] {key}");
        match key.as_str() {
            "auto" => s.auto = Some(typed(value, &name)?),
            "send" => s.send = Some(typed(value, &name)?),
            "instructions" | "instructions_file" => {}
            "account" | "model" | "effort" => return Err(format!("{name} is each person's own: the accounts and models reviews run on are theirs, in their config.toml.")),
            "prompt" => return Err(format!("{name} is each person's own. The team's words go in [reviews.own] instructions, which are added to it.")),
            _ => skipped.push(format!("reviews.own.{key}")),
        }
    }
    *words = instructions(t, "[reviews.own]", file)?;
    Ok(())
}

fn pull_requests(t: &toml::Table, cfg: &mut TeamCfg, skipped: &mut Vec<String>) -> Result<(), String> {
    let s: &mut PullRequestsPatch = &mut cfg.suggests.pull_requests;
    for (key, value) in t {
        let name = format!("[pull_requests] {key}");
        match key.as_str() {
            "nudge_by_default" => s.nudge_by_default = Some(typed(value, &name)?),
            "nudge_about" => cfg.nudge_about = Some(typed(value, &name)?),
            "rerequest_after_fixes" => s.rerequest_after_fixes = Some(typed(value, &name)?),
            _ => skipped.push(format!("pull_requests.{key}")),
        }
    }
    Ok(())
}

/// A table's `instructions`, or the file its `instructions_file` names in `.codealong/`; not both.
fn instructions(t: &toml::Table, table: &str, file: &dyn Fn(&str) -> Result<Option<String>, String>) -> Result<String, String> {
    let inline: Option<String> = t.get("instructions").map(|v| typed(v, &format!("{table} instructions"))).transpose()?;
    let named: Option<String> = t.get("instructions_file").map(|v| typed(v, &format!("{table} instructions_file"))).transpose()?;
    let words = match (inline, named) {
        (Some(_), Some(_)) => return Err(format!("{table} has instructions and instructions_file; use one of them.")),
        (Some(w), None) => w,
        (None, Some(name)) => {
            let path = in_dir(&name).ok_or_else(|| format!("{table} instructions_file is {name:?}; name a file in {DIR}/, such as \"review.md\"."))?;
            file(&path)?.ok_or_else(|| format!("{table} instructions_file is {name:?}, which isn't in {DIR}/."))?
        }
        (None, None) => String::new(),
    };
    let words = words.trim().to_string();
    if words.chars().count() > MOST_WORDS {
        return Err(format!("{table}'s instructions are over {MOST_WORDS} characters; they're typed into every review, so keep them shorter."));
    }
    Ok(words)
}

/// The path in the repo of a file a team's file names in `.codealong/`, when it's one there.
fn in_dir(name: &str) -> Option<String> {
    let name = name.trim();
    let fine = !name.is_empty() && !name.starts_with('/') && !name.contains('\\') && name.split('/').all(|part| !part.is_empty() && part != "." && part != "..");
    fine.then(|| format!("{DIR}/{name}"))
}

/// Why a link in `.codealong/` isn't read: a committed link is only where it points, and one in a
/// checkout could reach outside the repo.
const LINK: &str = "is a link; the team's settings are read from files themselves";

/// What `git ls-tree -r` lists of `.codealong/` at a commit, by path: whether each is a file of its
/// own (a link is mode 120000, a file 100644 or 100755).
fn tree_files(listed: &str) -> HashMap<&str, bool> {
    listed
        .lines()
        .filter_map(|l| {
            let (meta, path) = l.split_once('\t')?;
            Some((path.trim(), meta.split_whitespace().next()?.starts_with("100")))
        })
        .collect()
}

/// The team's file at `commit` in the repo checked out at `root`: none when there's none there,
/// else what it came to. An error when git couldn't be asked.
pub fn read_at(root: &Path, commit: &str) -> Result<Option<Parsed>, String> {
    let listed = git(root, &["ls-tree", "-r", commit, "--", &format!("{DIR}/")])?;
    let files = tree_files(&listed);
    match files.get(FILE) {
        None => return Ok(None),
        Some(false) => return Ok(Some(Err(format!("{FILE} {LINK}.")))),
        Some(true) => {}
    }
    let text = git(root, &["cat-file", "blob", &format!("{commit}:{FILE}")])?;
    let file = |path: &str| match files.get(path) {
        Some(true) => git(root, &["cat-file", "blob", &format!("{commit}:{path}")]).map(Some),
        Some(false) => Err(format!("{path} {LINK}.")),
        None => Ok(None),
    };
    Ok(Some(parse(&text, &file)))
}

/// The team's file as a folder's working tree has it, for `codealong config check --repo`: none
/// when it has none. A link on the way to a file, `.codealong` itself too, is refused, as a
/// committed one is.
pub fn read_folder(folder: &Path) -> Option<Parsed> {
    let plain = |rel: &str| -> Result<Option<String>, String> {
        let mut at = folder.to_path_buf();
        for part in Path::new(rel).components() {
            at.push(part);
            match std::fs::symlink_metadata(&at) {
                Ok(m) if m.file_type().is_symlink() => return Err(format!("{} {LINK}.", at.strip_prefix(folder).unwrap_or(&at).display())),
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(format!("reading {rel}: {e}")),
            }
        }
        if !at.is_file() {
            return Ok(None);
        }
        std::fs::read_to_string(&at).map(Some).map_err(|e| format!("reading {rel}: {e}"))
    };
    match plain(FILE) {
        Ok(None) => None,
        Ok(Some(text)) => Some(parse(&text, &plain)),
        Err(e) => Some(Err(e)),
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let dir = root.to_string_lossy();
    let mut full = vec!["-C", dir.as_ref()];
    full.extend_from_slice(args);
    crate::procs::run_background("git", &full, None, GIT_TIMEOUT).map_err(|e| format!("{e:#}"))
}

/// The commits `origin`'s branches are at in the checkout at `root`, by branch, for those of
/// `branches` it has.
fn commits(root: &Path, branches: &[String]) -> Result<HashMap<String, String>, String> {
    let refs: Vec<String> = branches.iter().map(|b| format!("refs/remotes/origin/{b}")).collect();
    let mut args = vec!["for-each-ref", "--format=%(objectname) %(refname)"];
    args.extend(refs.iter().map(String::as_str));
    let out = git(root, &args)?;
    Ok(out
        .lines()
        .filter_map(|l| {
            let (oid, name) = l.trim().split_once(' ')?;
            Some((name.strip_prefix("refs/remotes/origin/")?.to_string(), oid.to_string()))
        })
        .collect())
}

/// A branch's file as read: the commit it was read at, and what it came to (none: the branch
/// has no file).
#[derive(Debug, Clone, PartialEq)]
pub struct Read {
    pub commit: String,
    pub file: Option<Parsed>,
}

/// What to read of one repo: its checkout, the branches whose files go for its PRs, and the
/// commit each was last read at.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    pub slug: String,
    pub root: PathBuf,
    pub branches: Vec<String>,
    pub known: HashMap<String, String>,
}

/// What reading one repo came to: each branch read anew (one at a commit already read isn't), the
/// branches it no longer has, or why it couldn't be read.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub slug: String,
    pub reads: HashMap<String, Read>,
    pub gone: Vec<String>,
    pub failed: Option<String>,
}

/// Reads what `ask` asks, on the calling thread (`TeamFiles::due` hands them out).
pub fn read(ask: &Ask) -> Answer {
    let mut answer = Answer { slug: ask.slug.clone(), reads: HashMap::new(), gone: Vec::new(), failed: None };
    let at = match commits(&ask.root, &ask.branches) {
        Ok(at) => at,
        Err(e) => {
            answer.failed = Some(e);
            return answer;
        }
    };
    for branch in &ask.branches {
        let Some(commit) = at.get(branch) else {
            answer.gone.push(branch.clone());
            continue;
        };
        if ask.known.get(branch) == Some(commit) {
            continue;
        }
        match read_at(&ask.root, commit) {
            Ok(file) => {
                answer.reads.insert(branch.clone(), Read { commit: commit.clone(), file });
            }
            Err(e) => answer.failed = Some(e),
        }
    }
    answer
}

/// One repo's checkout here, as the engine knows it, for `TeamFiles::due`.
pub struct Repo {
    pub slug: String,
    pub root: PathBuf,
    /// Its default branch, as its checkout says (`main`, not `origin/main`).
    pub default: Option<String>,
}

/// A branch's team file as the windows get it (`Board::team_files`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TeamFile {
    /// `owner/name`.
    pub repo: String,
    pub branch: String,
    /// It's the repo's default branch, which Settings shows.
    pub default: bool,
    /// The commit it was read at.
    pub commit: String,
    /// What it sets; none when it goes unused (`error`).
    pub settings: Option<TeamCfg>,
    /// Why it goes unused.
    pub error: Option<String>,
    /// Keys it has that this Codealong passed over.
    pub skipped: Vec<String>,
    /// What it suggests that isn't how things go for you in its repo now: what Use them would
    /// change (`config::suggestions_left`). Set by the engine.
    pub offers: RepoCfg,
}

/// Every repo's team file as last read, kept by the engine. A repo is read again when its
/// checkout's refs move (`on_refs_moved`: a fetch) and every few minutes regardless, one read of
/// every repo at a time, in the background.
#[derive(Default)]
pub struct TeamFiles {
    /// By repo, then branch.
    reads: BTreeMap<String, BTreeMap<String, Read>>,
    /// Each repo's default branch.
    defaults: HashMap<String, String>,
    /// Repos whose refs moved since they were last read.
    stale: HashSet<String>,
    /// Why a repo couldn't be read, by repo; what was read before stays.
    failed: BTreeMap<String, String>,
    /// The repos checked out here, as `due` was last told.
    here: HashSet<String>,
    running: bool,
    last: Option<Instant>,
}

impl TeamFiles {
    /// A checkout's refs moved: its repo is read again on the next `due`.
    pub fn on_refs_moved(&mut self, slug: &str) {
        self.stale.insert(slug.to_string());
    }

    /// What to read now, at `now`: the repos whose refs moved, ones with a branch not read yet (a
    /// PR into a new branch), or all of them once `EVERY` has passed; nothing while a read is
    /// under way. `bases` are the branches open PRs merge into, by repo, read with each repo's
    /// default branch. A repo that couldn't be read waits for the next time round. Marks a read
    /// under way when it hands one out; `on_read` takes the mark off.
    pub fn due(&mut self, repos: &[Repo], bases: &HashMap<String, HashSet<String>>, now: Instant) -> Vec<Ask> {
        self.here = repos.iter().map(|r| r.slug.clone()).collect();
        if self.running {
            return Vec::new();
        }
        let all = self.last.is_none_or(|t| now.duration_since(t) >= EVERY);
        let mut asks = Vec::new();
        for r in repos {
            let mut branches: Vec<String> = r.default.iter().cloned().chain(bases.get(&r.slug).into_iter().flatten().cloned()).collect::<HashSet<_>>().into_iter().collect();
            branches.sort();
            let unread = !self.failed.contains_key(&r.slug) && branches.iter().any(|b| !self.reads.get(&r.slug).is_some_and(|read| read.contains_key(b)));
            if !(all || unread || self.stale.contains(&r.slug)) {
                continue;
            }
            if let Some(d) = &r.default {
                self.defaults.insert(r.slug.clone(), d.clone());
            }
            if branches.is_empty() {
                continue;
            }
            let known = self.reads.get(&r.slug).map(|b| b.iter().filter(|(name, _)| branches.contains(name)).map(|(name, read)| (name.clone(), read.commit.clone())).collect()).unwrap_or_default();
            asks.push(Ask { slug: r.slug.clone(), root: r.root.clone(), branches, known });
        }
        // Repos no longer here are forgotten.
        let here: HashSet<&str> = repos.iter().map(|r| r.slug.as_str()).collect();
        self.reads.retain(|slug, _| here.contains(slug.as_str()));
        self.failed.retain(|slug, _| here.contains(slug.as_str()));
        if all {
            self.last = Some(now);
        }
        self.stale.retain(|s| !asks.iter().any(|a| &a.slug == s));
        self.running = !asks.is_empty();
        asks
    }

    /// The read is over: none when it fell over, which reads nothing. True when what the board
    /// shows changed.
    pub fn on_read(&mut self, answers: Option<Vec<Answer>>) -> bool {
        self.running = false;
        let Some(answers) = answers else { return false };
        let mut changed = false;
        for a in answers {
            let reads = self.reads.entry(a.slug.clone()).or_default();
            // A branch the checkout doesn't have is read too: it has no file.
            for branch in &a.gone {
                let none = Read { commit: String::new(), file: None };
                changed |= reads.get(branch) != Some(&none);
                reads.insert(branch.clone(), none);
            }
            for (branch, read) in a.reads {
                if reads.get(&branch) != Some(&read) {
                    reads.insert(branch, read);
                    changed = true;
                }
            }
            let was = match a.failed {
                Some(why) => self.failed.insert(a.slug, why.clone()) != Some(why),
                None => self.failed.remove(&a.slug).is_some(),
            };
            changed |= was;
        }
        changed
    }

    /// Each repo's settings where they aren't yours everywhere, with your tables `mine`.
    pub fn settings(&self, mine: &BTreeMap<String, RepoCfg>) -> RepoSettings {
        let mut team = HashMap::new();
        for (slug, branches) in &self.reads {
            for (branch, read) in branches {
                if let Some(Ok((cfg, _))) = &read.file {
                    team.insert((slug.clone(), branch.clone()), cfg.clone());
                }
            }
        }
        let read = self.reads.iter().flat_map(|(slug, branches)| branches.keys().map(move |b| (slug.clone(), b.clone()))).collect();
        RepoSettings { mine: mine.clone(), team, defaults: self.defaults.clone(), read, here: self.here.clone(), unread: self.last.is_none() }
    }

    /// Every team file read, as the windows get them; `offers` left empty for the engine.
    pub fn files(&self) -> Vec<TeamFile> {
        let mut out = Vec::new();
        for (slug, branches) in &self.reads {
            for (branch, read) in branches {
                let Some(file) = &read.file else { continue };
                let (settings, error, skipped) = match file {
                    Ok((cfg, skipped)) => (Some(cfg.clone()), None, skipped.clone()),
                    Err(e) => (None, Some(e.clone()), Vec::new()),
                };
                let default = self.defaults.get(slug) == Some(branch);
                out.push(TeamFile { repo: slug.clone(), branch: branch.clone(), default, commit: read.commit.clone(), settings, error, skipped, offers: RepoCfg::default() });
            }
        }
        out
    }

    /// What's wrong with the team files, for a notice: each that goes unused and why, each with
    /// keys passed over, and each repo that couldn't be read.
    pub fn notice(&self) -> Option<String> {
        let mut lines = Vec::new();
        for f in self.files() {
            let short = &f.commit[..f.commit.len().min(7)];
            if let Some(e) = &f.error {
                lines.push(format!("{}'s {FILE} on {} ({short}) isn't used: {e}", f.repo, f.branch));
            } else if !f.skipped.is_empty() {
                lines.push(format!("{}'s {FILE} on {} ({short}) has settings this Codealong doesn't know, passed over: {}.", f.repo, f.branch, f.skipped.join(", ")));
            }
        }
        for (slug, why) in &self.failed {
            lines.push(format!("{slug}'s {FILE} couldn't be read: {why}"));
        }
        (!lines.is_empty()).then(|| lines.join("\n"))
    }
}

/// Each repo's settings where they aren't yours everywhere, for what acts on its pull requests:
/// your `[repos]` tables, and what the team files read set (`TeamFiles::settings`). The rules
/// that put them together are config.rs's (`reviews_for`, `pull_requests_for`).
#[derive(Debug, Clone, Default)]
pub struct RepoSettings {
    /// Your tables, by key (`github.com/owner/name`).
    pub mine: BTreeMap<String, RepoCfg>,
    /// What each team file read sets, by repo slug and branch.
    pub team: HashMap<(String, String), TeamCfg>,
    /// Each repo's default branch, for a PR that names no base.
    pub defaults: HashMap<String, String>,
    /// The branches read, by repo slug and branch, with a file or without one (`pending`).
    pub read: HashSet<(String, String)>,
    /// The repos checked out here, whose files are read.
    pub here: HashSet<String>,
    /// Nothing has been read yet: the engine has only just started.
    pub unread: bool,
}

impl RepoSettings {
    /// Whether what `pr`'s repo's team says hasn't been read yet: nothing has been read since
    /// Codealong started, or its repo is checked out here and its base branch hasn't been read
    /// (with a file or without). What the team sets for a review (words for its reviewer, verdicts
    /// never submitted) goes for it, so it waits for this (autoreview.rs `due`).
    pub fn pending(&self, pr: &PrItem) -> bool {
        if self.unread {
            return true;
        }
        if !self.here.contains(&pr.repo_slug) {
            return false;
        }
        match pr.base.as_deref().or_else(|| self.defaults.get(&pr.repo_slug).map(String::as_str)) {
            Some(branch) => !self.read.contains(&(pr.repo_slug.clone(), branch.to_string())),
            None => false,
        }
    }

    /// The team's settings for a PR into `base` of repo `slug`: its base branch's file.
    pub fn team_of(&self, slug: &str, base: Option<&str>) -> Option<&TeamCfg> {
        let branch = base.or_else(|| self.defaults.get(slug).map(String::as_str))?;
        self.team.get(&(slug.to_string(), branch.to_string()))
    }

    /// Your table for repo `slug`.
    pub fn mine_of(&self, slug: &str) -> Option<&RepoCfg> {
        self.mine.get(&crate::config::repo_key(slug))
    }

    /// What's been read and which repos are here, in order, for telling a board from the last
    /// (engine.rs `board_hash`): none of it is sent to the windows, and a branch read without a
    /// file changes nothing that is, yet what waits on it (`pending`) goes on once it's read.
    pub fn readiness(&self) -> (bool, Vec<&(String, String)>, Vec<&String>, Vec<(&String, &String)>) {
        let mut read: Vec<_> = self.read.iter().collect();
        read.sort();
        let mut here: Vec<_> = self.here.iter().collect();
        here.sort();
        let mut defaults: Vec<_> = self.defaults.iter().collect();
        defaults.sort();
        (self.unread, read, here, defaults)
    }

    /// `[reviews]` as it goes for `pr`, `global` itself where nothing's set for its repo.
    pub fn reviews<'a>(&self, global: &'a ReviewsCfg, pr: &PrItem) -> Cow<'a, ReviewsCfg> {
        match (self.team_of(&pr.repo_slug, pr.base.as_deref()), self.mine_of(&pr.repo_slug)) {
            (None, None) => Cow::Borrowed(global),
            (team, mine) => Cow::Owned(crate::config::reviews_for(global, team, mine)),
        }
    }

    /// `[pull_requests]` as it goes for `pr`.
    pub fn pull_requests<'a>(&self, global: &'a PullRequestsCfg, pr: &PrItem) -> Cow<'a, PullRequestsCfg> {
        match (self.team_of(&pr.repo_slug, pr.base.as_deref()), self.mine_of(&pr.repo_slug)) {
            (None, None) => Cow::Borrowed(global),
            (team, mine) => Cow::Owned(crate::config::pull_requests_for(global, team, mine)),
        }
    }

    /// What a nudge on `pr` tells about: `global` (`[pull_requests] nudge_about`), else as its
    /// repo's team or your table for it says.
    pub fn nudge_about(&self, global: NudgeAbout, pr: &PrItem) -> NudgeAbout {
        self.pull_requests(&PullRequestsCfg { nudge_about: global, ..PullRequestsCfg::default() }, pr).nudge_about
    }

    /// Whether any reviews are on: everywhere, or in a repo your table turns them on for.
    pub fn reviews_on(&self, global: &ReviewsCfg) -> bool {
        global.auto || global.own.auto || self.mine.values().any(|m| m.reviews.auto == Some(true) || m.reviews.own.auto == Some(true))
    }

    /// `nudge_by_default` for each repo your table sets it for, by slug.
    pub fn nudges_by_default(&self) -> HashMap<String, bool> {
        self.mine.iter().filter_map(|(key, m)| Some((key.strip_prefix("github.com/")?.to_string(), m.pull_requests.nudge_by_default?))).collect()
    }
}

/// The open PRs' base branches, by repo slug: the branches whose team files go for them.
pub fn bases<'a>(prs: impl IntoIterator<Item = (&'a str, Option<&'a str>)>) -> HashMap<String, HashSet<String>> {
    let mut out: HashMap<String, HashSet<String>> = HashMap::new();
    for (slug, base) in prs {
        if let Some(b) = base.filter(|b| !b.is_empty()) {
            out.entry(slug.to_string()).or_default().insert(b.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Result<Option<String>, String> {
        Ok(None)
    }

    #[test]
    fn a_team_sets_words_and_limits_and_suggests_the_rest() {
        let text = r#"
[reviews]
auto = true
post = "comment"
instructions = "  Check migrations run both ways.  "
never_submit = ["approve"]

[reviews.own]
auto = true
send = false
instructions_file = "own-review.md"

[pull_requests]
nudge_by_default = true
nudge_about = "feedback-and-checks"
rerequest_after_fixes = true
"#;
        let file = |path: &str| Ok((path == ".codealong/own-review.md").then(|| "Mind the tests.\n".to_string()));
        let (cfg, skipped) = parse(text, &file).unwrap();
        assert!(skipped.is_empty());
        assert_eq!(cfg.review_instructions, "Check migrations run both ways.");
        assert_eq!(cfg.own_review_instructions, "Mind the tests.");
        assert_eq!(cfg.never_submit, vec![Verdict::Approve]);
        assert_eq!(cfg.nudge_about, Some(NudgeAbout::FeedbackAndChecks));
        let s = &cfg.suggests;
        assert_eq!((s.reviews.auto, s.reviews.post), (Some(true), Some(ReviewPost::Comment)));
        assert_eq!((s.reviews.own.auto, s.reviews.own.send), (Some(true), Some(false)));
        assert_eq!(s.pull_requests, PullRequestsPatch { nudge_by_default: Some(true), nudge_about: None, rerequest_after_fixes: Some(true) });
        // None of a person's own settings: those aren't the team's to suggest.
        assert_eq!((&s.reviews.account, &s.reviews.model, &s.reviews.prompt, s.reviews.submit), (&None, &None, &None, None));
        assert_eq!(parse("", &none).unwrap(), (TeamCfg::default(), Vec::new()));
    }

    #[test]
    fn what_is_each_persons_own_makes_the_file_go_unused() {
        for (text, says) in [
            ("[reviews]\naccount = \"work\"", "[reviews] account is each person's own"),
            ("[reviews]\nmodel = \"opus\"", "[reviews] model is each person's own"),
            ("[reviews]\nprompt = \"Be brief.\"", "[reviews] instructions"),
            ("[reviews]\nsubmit = \"approve\"", "never_submit"),
            ("[reviews.own]\naccount = \"work\"", "[reviews.own] account is each person's own"),
            ("[[accounts]]\nname = \"work\"", "accounts is each person's own"),
            ("[limits]\nwhen_hit = \"move\"", "limits is each person's own"),
        ] {
            let e = parse(text, &none).unwrap_err();
            assert!(e.contains(says), "{text}: {e}");
        }
    }

    #[test]
    fn a_value_that_isnt_one_is_an_error_and_an_unknown_key_is_passed_over() {
        assert!(parse("[reviews]\npost = \"loudly\"", &none).unwrap_err().starts_with("[reviews] post"));
        assert!(parse("[reviews]\nnever_submit = [\"merge\"]", &none).unwrap_err().starts_with("[reviews] never_submit"));
        assert!(parse("[reviews]\nauto = \"yes\"", &none).unwrap_err().starts_with("[reviews] auto"));
        assert!(parse("[reviews", &none).is_err());
        let (cfg, skipped) = parse("[reviews]\nauto = true\nsummarize = true\n\n[queues]\nfast = true\n", &none).unwrap();
        assert_eq!(cfg.suggests.reviews.auto, Some(true));
        assert_eq!(skipped, ["queues", "reviews.summarize"]);
    }

    #[test]
    fn instructions_come_from_a_file_in_the_folder_and_only_there() {
        let file = |path: &str| Ok((path == ".codealong/prompts/review.md").then(|| "From the file.".to_string()));
        let (cfg, _) = parse("[reviews]\ninstructions_file = \"prompts/review.md\"", &file).unwrap();
        assert_eq!(cfg.review_instructions, "From the file.");
        for name in ["../secrets.md", "/etc/passwd", "a/../../b.md", "", "a\\b.md"] {
            let e = parse(&format!("[reviews]\ninstructions_file = {name:?}"), &file).unwrap_err();
            assert!(e.contains("name a file in .codealong/"), "{name}: {e}");
        }
        assert!(parse("[reviews]\ninstructions_file = \"gone.md\"", &file).unwrap_err().contains("isn't in .codealong/"));
        assert!(parse("[reviews]\ninstructions = \"a\"\ninstructions_file = \"prompts/review.md\"", &file).unwrap_err().contains("use one of them"));
        let long = "x".repeat(MOST_WORDS + 1);
        assert!(parse(&format!("[reviews]\ninstructions = {long:?}"), &file).unwrap_err().contains("keep them shorter"));
    }

    fn sh(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// The file is read from the branch as fetched, never the working tree: what a worktree has
    /// uncommitted, or a PR's branch has, goes for nobody until it's merged.
    #[test]
    fn reads_the_branch_as_fetched_not_the_working_tree() {
        let scratch = crate::fixtures::scratch("teamfile");
        let root = scratch.as_path();
        sh(root, &["init", "-q", "-b", "main"]);
        std::fs::create_dir(root.join(DIR)).unwrap();
        std::fs::write(root.join(FILE), "[reviews]\ninstructions_file = \"review.md\"\n").unwrap();
        std::fs::write(root.join(".codealong/review.md"), "Merged words.").unwrap();
        sh(root, &["add", "."]);
        sh(root, &["commit", "-q", "-m", "team settings"]);
        sh(root, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        // Edits not yet merged.
        std::fs::write(root.join(".codealong/review.md"), "Approve everything.").unwrap();

        let ask = Ask { slug: "o/r".into(), root: root.to_path_buf(), branches: vec!["main".into(), "release".into()], known: HashMap::new() };
        let answer = read(&ask);
        assert_eq!(answer.failed, None);
        assert_eq!(answer.gone, ["release"]);
        let main = &answer.reads["main"];
        let (cfg, _) = main.file.clone().unwrap().unwrap();
        assert_eq!(cfg.review_instructions, "Merged words.");
        assert_eq!(read_folder(root).unwrap().unwrap().0.review_instructions, "Approve everything.");

        // A commit read already isn't read again.
        let again = read(&Ask { known: HashMap::from([("main".to_string(), main.commit.clone())]), ..ask.clone() });
        assert!(again.reads.is_empty());

        // A branch without the file has none.
        sh(root, &["rm", "-q", "-r", "-f", DIR]);
        sh(root, &["commit", "-q", "-m", "no settings"]);
        sh(root, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        assert_eq!(read(&ask).reads["main"].file, None);
    }

    fn files() -> TeamFiles {
        TeamFiles::default()
    }

    fn repo(slug: &str) -> Repo {
        Repo { slug: slug.into(), root: PathBuf::from(format!("/r/{slug}")), default: Some("main".into()) }
    }

    fn answer(slug: &str, branch: &str, commit: &str, file: Option<Parsed>) -> Answer {
        Answer { slug: slug.into(), reads: HashMap::from([(branch.to_string(), Read { commit: commit.into(), file })]), gone: Vec::new(), failed: None }
    }

    #[test]
    fn every_repo_is_read_now_and_then_and_one_whose_refs_moved_at_once() {
        let mut t = files();
        let t0 = Instant::now();
        let repos = [repo("o/a"), repo("o/b")];
        let bases = bases([("o/a", Some("release")), ("o/a", Some("main")), ("o/b", None)]);
        let asks = t.due(&repos, &bases, t0);
        assert_eq!(asks.iter().map(|a| (a.slug.as_str(), a.branches.clone())).collect::<Vec<_>>(), [("o/a", vec!["main".to_string(), "release".to_string()]), ("o/b", vec!["main".to_string()])]);
        // One at a time.
        assert!(t.due(&repos, &bases, t0).is_empty());
        let team = TeamCfg { review_instructions: "Words.".into(), ..TeamCfg::default() };
        // Every branch asked about is read: main with a file, the rest without.
        let all_read = |team: &TeamCfg| vec![answer("o/a", "main", "c1", Some(Ok((team.clone(), Vec::new())))), answer("o/a", "release", "c2", None), answer("o/b", "main", "c3", None)];
        assert!(t.on_read(Some(all_read(&team))));
        let set = t.settings(&BTreeMap::new());
        assert_eq!(set.team_of("o/a", None), Some(&team));
        assert_eq!(set.team_of("o/a", Some("main")), Some(&team));
        assert_eq!(set.team_of("o/a", Some("release")), None);
        // The same again changes nothing.
        assert!(t.due(&repos, &bases, t0 + EVERY).len() == 2);
        assert!(!t.on_read(Some(all_read(&team))));
        // Nothing's due until a fetch moves a repo's refs, or the time comes round again.
        assert!(t.due(&repos, &bases, t0 + EVERY + Duration::from_secs(1)).is_empty());
        t.on_refs_moved("o/b");
        let asks = t.due(&repos, &bases, t0 + EVERY + Duration::from_secs(2));
        assert_eq!(asks.iter().map(|a| a.slug.as_str()).collect::<Vec<_>>(), ["o/b"]);
        // A read that fell over reads nothing and lets the next one go.
        assert!(!t.on_read(None));
        assert_eq!(t.due(&repos, &bases, t0 + EVERY * 2).len(), 2);
        // What was asked knows the commit each branch was read at.
        t.on_read(None);
        let asks = t.due(&repos, &bases, t0 + EVERY * 3);
        assert_eq!(asks[0].known, HashMap::from([("main".to_string(), "c1".to_string()), ("release".to_string(), "c2".to_string())]));
    }

    fn pr_into(slug: &str, base: &str) -> PrItem {
        use crate::model::{Checks, Merge, PrState};
        PrItem {
            key: format!("{slug}#1"),
            repo_slug: slug.into(),
            repo: "r".into(),
            project: "P".into(),
            number: 1,
            title: "Add teams".into(),
            url: format!("https://github.com/{slug}/pull/1"),
            state: PrState::Open,
            author: Some("ana".into()),
            mine: false,
            review_requested: true,
            requested_by: None,
            requested_at: None,
            reviewers_asked: Vec::new(),
            snoozed_until: None,
            nudge: false,
            nudge_by_default: false,
            primary: None,
            primary_by: None,
            head: "teams".into(),
            base: Some(base.into()),
            checks: Checks::default(),
            merge: Merge::Unknown,
            additions: 1,
            deletions: 0,
            changed_files: 1,
            created_at: 0,
            updated_at: 0,
            merged_at: None,
            closed_at: None,
            lanes: Vec::new(),
            sessions: Vec::new(),
            head_oid: Some("1111111".into()),
            authored_by: None,
            labels: Vec::new(),
            auto_review: None,
            last_review: None,
            stale_approval: None,
            turn: None,
            shared_review: Default::default(),
        }
    }

    /// What a PR's repo's team sets waits to be read before anything acts on it: from launch until
    /// the first read, and for a branch a PR newly merges into, which is read at once rather than
    /// when the time comes round. A branch the checkout doesn't have counts as read, with no file;
    /// a repo that couldn't be read is asked again only when the time comes round.
    #[test]
    fn a_prs_team_settings_are_pending_until_its_base_branch_is_read() {
        let mut t = files();
        let t0 = Instant::now();
        let repos = [repo("o/a")];
        let pr = pr_into("o/a", "main");
        assert!(t.settings(&BTreeMap::new()).pending(&pr), "nothing's read at launch");
        assert!(t.settings(&BTreeMap::new()).pending(&pr_into("o/elsewhere", "main")), "nor for a repo that isn't here, yet");
        assert_eq!(t.due(&repos, &HashMap::new(), t0).len(), 1);
        assert!(t.settings(&BTreeMap::new()).pending(&pr), "asked, not answered");
        assert!(!t.settings(&BTreeMap::new()).pending(&pr_into("o/elsewhere", "main")), "a repo that isn't here has nothing to read");
        t.on_read(Some(vec![answer("o/a", "main", "c1", None)]));
        assert!(!t.settings(&BTreeMap::new()).pending(&pr), "read, with no file");

        // A PR into a new branch: read at once.
        let hotfix = pr_into("o/a", "hotfix");
        assert!(t.settings(&BTreeMap::new()).pending(&hotfix));
        let bases = bases([("o/a", Some("hotfix"))]);
        let asks = t.due(&repos, &bases, t0 + Duration::from_secs(1));
        assert_eq!(asks.iter().map(|a| a.branches.clone()).collect::<Vec<_>>(), [vec!["hotfix".to_string(), "main".to_string()]]);
        // The checkout has no such branch: read, with no file.
        t.on_read(Some(vec![Answer { slug: "o/a".into(), reads: HashMap::new(), gone: vec!["hotfix".into()], failed: None }]));
        assert!(!t.settings(&BTreeMap::new()).pending(&hotfix));
        assert!(t.due(&repos, &bases, t0 + Duration::from_secs(2)).is_empty(), "nothing's left to read");

        // A repo that couldn't be read stays pending, and is asked again when the time comes.
        let release = bases_with(&bases, "o/a", "release");
        assert_eq!(t.due(&repos, &release, t0 + Duration::from_secs(3)).len(), 1);
        t.on_read(Some(vec![Answer { slug: "o/a".into(), reads: HashMap::new(), gone: Vec::new(), failed: Some("git timed out".into()) }]));
        assert!(t.settings(&BTreeMap::new()).pending(&pr_into("o/a", "release")));
        assert!(t.due(&repos, &release, t0 + Duration::from_secs(4)).is_empty(), "not again at once");
        assert_eq!(t.due(&repos, &release, t0 + EVERY).len(), 1);
    }

    fn bases_with(bases: &HashMap<String, HashSet<String>>, slug: &str, branch: &str) -> HashMap<String, HashSet<String>> {
        let mut out = bases.clone();
        out.entry(slug.to_string()).or_default().insert(branch.to_string());
        out
    }

    /// A link in `.codealong/` is refused, committed or in a checkout: committed, it's only where
    /// it points; in a checkout, it could read a file outside the repo.
    #[test]
    fn a_link_in_the_folder_is_refused() {
        let scratch = crate::fixtures::scratch("teamlink");
        let dir = scratch.to_path_buf();
        std::fs::create_dir_all(dir.join(DIR)).unwrap();
        let outside = dir.join("outside.md");
        std::fs::write(&outside, "Secret words.").unwrap();
        std::fs::write(dir.join(FILE), "[reviews]\ninstructions_file = \"review.md\"\n").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join(".codealong/review.md")).unwrap();
        let e = read_folder(&dir).unwrap().unwrap_err();
        assert!(e.contains(".codealong/review.md is a link"), "{e}");
        sh(&dir, &["init", "-q", "-b", "main"]);
        sh(&dir, &["add", "."]);
        sh(&dir, &["commit", "-q", "-m", "a link"]);
        let commit = git(&dir, &["rev-parse", "HEAD"]).unwrap();
        let e = read_at(&dir, commit.trim()).unwrap().unwrap().unwrap_err();
        assert!(e.contains(".codealong/review.md is a link"), "{e}");
        // The folder itself a link: refused too.
        let real = dir.join("real");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("config.toml"), "").unwrap();
        let linked = dir.join("linked");
        std::fs::create_dir_all(&linked).unwrap();
        std::os::unix::fs::symlink(&real, linked.join(DIR)).unwrap();
        assert!(read_folder(&linked).unwrap().unwrap_err().contains(".codealong is a link"));
    }

    #[test]
    fn a_file_that_goes_unused_and_a_repo_that_wasnt_read_are_said() {
        let mut t = files();
        let repos = [repo("o/a")];
        t.due(&repos, &HashMap::new(), Instant::now());
        t.on_read(Some(vec![answer("o/a", "main", "abcdef123", Some(Err("[reviews] account is each person's own.".into())))]));
        assert_eq!(t.settings(&BTreeMap::new()).team_of("o/a", None), None);
        let said = t.notice().unwrap();
        assert!(said.contains("o/a's .codealong/config.toml on main (abcdef1) isn't used: [reviews] account"), "{said}");
        let f = &t.files()[0];
        assert!(f.default && f.settings.is_none());

        t.due(&repos, &HashMap::new(), Instant::now() + EVERY);
        assert!(t.on_read(Some(vec![Answer { slug: "o/a".into(), reads: HashMap::new(), gone: Vec::new(), failed: Some("git timed out".into()) }])));
        assert!(t.notice().unwrap().contains("o/a's .codealong/config.toml couldn't be read: git timed out"));
        // What was read stays.
        assert_eq!(t.files().len(), 1);
    }
}

//! GitHub organization avatars for projects, cached locally as `data:image/…` URLs.
//!
//! A project whose repos belong to one GitHub owner gets that owner's avatar when GitHub says the
//! owner is an organization (personal accounts keep the folder icon, as `github::display_name`
//! leaves personal names out of project names). Avatars are cached under
//! `<cache_dir>/org-avatars/<owner>.json` for a week so restarts and snapshots show them without
//! hitting the network, and failures back off for an hour.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::board::{LaneSeed, ELSEWHERE};
use crate::config::ProjectCfg;
use crate::{http, procs, repos, store};

/// How long a cached org avatar (or a "not an org" answer) stays fresh before re-checking.
const FRESH_FOR_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// How long to wait before retrying an owner whose lookup failed.
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
/// Largest avatar image accepted into the cache (64px avatars are a few KiB).
const MAX_BYTES: usize = 64 * 1024;
/// Marker returned by `gh api` when the owner exists but is not an Organization.
const NOT_ORG: &str = "__not_org__";

/// The result of asking GitHub for one owner's organization avatar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The owner is a GitHub organization and its avatar was downloaded.
    Avatar(String),
    /// The owner is not a GitHub organization (e.g. a personal user) or has no avatar.
    NotOrg,
    /// `gh` or curl failed (offline, rate-limited, etc.); retry later.
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CachedEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data_url: Option<String>,
    #[serde(default)]
    not_org: bool,
    fetched_at: i64,
}

/// Whether `owner` is a valid GitHub login (safe as a URL segment and cache filename).
pub fn valid_owner(owner: &str) -> bool {
    !owner.is_empty()
        && owner.len() <= 39
        && !owner.starts_with('-')
        && !owner.ends_with('-')
        && owner.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Extracts a single literal GitHub owner from a `ProjectCfg.repos` pattern such as
/// `github.com/acme/*` or `github.com/acme/app`. Returns `None` for non-GitHub patterns
/// or wildcards in the owner position.
pub fn owner_from_pattern(pattern: &str) -> Option<String> {
    let (owner, _) = repos::github_of(pattern.trim())?;
    let lower = owner.to_ascii_lowercase();
    valid_owner(&lower).then_some(lower)
}

/// Maps each project name to its single GitHub owner (lowercased), when unambiguous.
///
/// Prefers the project's actual lanes (`seed.repo_slug`, using `renamed` when GitHub says the
/// repo or org moved); if a project has no GitHub lanes yet, falls back to its configured `repos`
/// patterns. Multi-org projects and `Elsewhere` get none.
pub fn project_owners<'a>(
    projects: &[ProjectCfg],
    seeds: &[LaneSeed],
    renamed: impl Fn(&str) -> Option<&'a str>,
) -> HashMap<String, String> {
    let mut lane_owners: HashMap<&str, BTreeSet<String>> = HashMap::new();
    for s in seeds {
        if s.project == ELSEWHERE {
            continue;
        }
        if let Some(slug) = s.repo_slug.as_deref() {
            let current = renamed(slug).unwrap_or(slug);
            if let Some((owner, _)) = current.split_once('/') {
                let lower = owner.to_ascii_lowercase();
                if valid_owner(&lower) {
                    lane_owners.entry(s.project.as_str()).or_default().insert(lower);
                }
            }
        }
    }

    let mut out = HashMap::new();
    for p in projects {
        if p.name == ELSEWHERE {
            continue;
        }
        if let Some(owners) = lane_owners.get(p.name.as_str()) {
            if owners.len() == 1 {
                out.insert(p.name.clone(), owners.iter().next().unwrap().clone());
            }
            continue;
        }
        let pattern_owners: BTreeSet<String> = p.repos.iter().filter_map(|r| owner_from_pattern(r)).collect();
        if pattern_owners.len() == 1 {
            out.insert(p.name.clone(), pattern_owners.into_iter().next().unwrap());
        }
    }
    // Also cover any non-Elsewhere project present in seeds (e.g. suggested/dynamic) not in cfg.
    for (proj, owners) in lane_owners {
        if !out.contains_key(proj) && owners.len() == 1 && !projects.iter().any(|p| p.name == proj) {
            out.insert(proj.to_string(), owners.into_iter().next().unwrap());
        }
    }
    out
}

/// Appends `s=64` so GitHub serves a crisp 64×64 avatar instead of the full-resolution image.
pub fn sized_avatar_url(url: &str) -> String {
    if url.contains("s=") || url.contains("size=") {
        url.to_string()
    } else if url.contains('?') {
        format!("{url}&s=64")
    } else {
        format!("{url}?s=64")
    }
}

/// Identifies common raster image formats from their magic bytes.
pub fn detect_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else {
        None
    }
}

/// Encodes raw image bytes into a `data:image/…;base64,…` URL after validating size and magic bytes.
pub fn encode_data_url(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return None;
    }
    let mime = detect_mime(bytes)?;
    Some(format!("data:{mime};base64,{}", data_encoding::BASE64.encode(bytes)))
}

/// Queries GitHub for `owner` via `gh` and downloads its 64px avatar when it is an Organization.
pub fn fetch(owner: &str) -> Outcome {
    if !valid_owner(owner) {
        return Outcome::NotOrg;
    }
    let jq = format!(r#"if .type == "Organization" then (.avatar_url // "") else "{NOT_ORG}" end"#);
    let Ok(out) = procs::run("gh", &["api", &format!("users/{owner}"), "--jq", &jq], None, Duration::from_secs(5)) else {
        return Outcome::Failed;
    };
    let line = out.trim();
    if line == NOT_ORG || line.is_empty() {
        return Outcome::NotOrg;
    }
    if !line.starts_with("https://") {
        return Outcome::Failed;
    }
    let url = sized_avatar_url(line);
    let Ok(bytes) = procs::run_bytes(
        http::curl(),
        &["-q", "-sS", "-f", "-L", "--max-time", "10", "--max-filesize", "65536", &url],
        Duration::from_secs(15),
    ) else {
        return Outcome::Failed;
    };
    match encode_data_url(&bytes) {
        Some(data_url) => Outcome::Avatar(data_url),
        None => Outcome::Failed,
    }
}

/// Engine subsystem that caches GitHub org avatars on disk and schedules background fetches.
pub struct OrgAvatars {
    /// Cache directory (`<cache_dir>/org-avatars`); empty disables disk I/O (used in tests).
    dir: PathBuf,
    entries: HashMap<String, CachedEntry>,
    disk_checked: HashSet<String>,
    asked: HashMap<String, Instant>,
    inflight: bool,
}

impl OrgAvatars {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, entries: HashMap::new(), disk_checked: HashSet::new(), asked: HashMap::new(), inflight: false }
    }

    fn cache_path(&self, owner: &str) -> Option<PathBuf> {
        (!self.dir.as_os_str().is_empty() && valid_owner(owner)).then(|| self.dir.join(format!("{owner}.json")))
    }

    /// Reads any not-yet-checked owners from the local disk cache into memory.
    /// Returns `true` if any avatar URL was loaded.
    pub fn load_cached(&mut self, owners: &HashMap<String, String>) -> bool {
        let mut loaded_any = false;
        for owner in owners.values() {
            if !self.disk_checked.insert(owner.clone()) {
                continue;
            }
            let Some(path) = self.cache_path(owner) else { continue };
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Ok(entry) = serde_json::from_str::<CachedEntry>(&text) else { continue };
            if entry.data_url.is_some() {
                loaded_any = true;
            }
            self.entries.insert(owner.clone(), entry);
        }
        loaded_any
    }

    /// Returns the cached `data:image/…` URL for `owner`, if any.
    pub fn icon_for(&self, owner: &str) -> Option<&str> {
        self.entries.get(owner).and_then(|e| e.data_url.as_deref())
    }

    /// Returns the owners that need fetching now (missing or older than `FRESH_FOR_MS`, and not
    /// asked within `RETRY_AFTER`), marking a fetch inflight.
    pub fn to_fetch(&mut self, owners: &HashMap<String, String>, now: Instant, now_ms: i64) -> Option<Vec<String>> {
        if self.inflight {
            return None;
        }
        let due: BTreeSet<String> = owners
            .values()
            .filter(|owner| valid_owner(owner))
            .filter(|owner| {
                let fresh = self.entries.get(*owner).is_some_and(|e| now_ms.saturating_sub(e.fetched_at) < FRESH_FOR_MS);
                let recent = self.asked.get(*owner).is_some_and(|t| now.duration_since(*t) < RETRY_AFTER);
                !fresh && !recent
            })
            .cloned()
            .collect();
        if due.is_empty() {
            return None;
        }
        for owner in &due {
            self.asked.insert(owner.clone(), now);
        }
        self.inflight = true;
        Some(due.into_iter().collect())
    }

    /// Records the results of a background fetch, writing successful/non-org entries to disk.
    /// Returns `true` when any visible avatar URL changed.
    pub fn on_fetched(&mut self, results: Option<Vec<(String, Outcome)>>, now_ms: i64) -> bool {
        self.inflight = false;
        let Some(results) = results else { return false };
        let mut changed = false;
        for (owner, outcome) in results {
            let entry = match outcome {
                Outcome::Avatar(url) => CachedEntry { data_url: Some(url), not_org: false, fetched_at: now_ms },
                Outcome::NotOrg => CachedEntry { data_url: None, not_org: true, fetched_at: now_ms },
                Outcome::Failed => continue,
            };
            let prev = self.entries.get(&owner).and_then(|e| e.data_url.as_deref());
            if prev != entry.data_url.as_deref() {
                changed = true;
            }
            if let Some(path) = self.cache_path(&owner) {
                write_cache(&path, &entry);
            }
            self.disk_checked.insert(owner.clone());
            self.entries.insert(owner, entry);
        }
        changed
    }
}

fn write_cache(path: &Path, entry: &CachedEntry) {
    let _ = store::save(path, entry);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Origin;

    fn seed(project: &str, slug: Option<&str>) -> LaneSeed {
        LaneSeed {
            path: PathBuf::from(format!("/work/{project}")),
            project: project.into(),
            repo: "app".into(),
            repo_slug: slug.map(str::to_string),
            head_owners: Vec::new(),
            callsign: "app".into(),
            origin: Origin::Main,
            branch: None,
            head: None,
            default_branch: None,
            elsewhere: project == ELSEWHERE,
            chats: false,
            starts_chats: false,
            exact: false,
        }
    }

    fn project(name: &str, repos: &[&str]) -> ProjectCfg {
        ProjectCfg {
            name: name.into(),
            repos: repos.iter().map(|s| (*s).into()).collect(),
            folders: Vec::new(),
            chrome_profile: None,
            default: None,
            decisions: None,
        }
    }

    #[test]
    fn extracts_owner_from_github_patterns() {
        assert_eq!(owner_from_pattern("github.com/Northwind/*").as_deref(), Some("northwind"));
        assert_eq!(owner_from_pattern("github.com/atlas-hq/importer").as_deref(), Some("atlas-hq"));
        assert_eq!(owner_from_pattern("github.com/*/repo"), None);
        assert_eq!(owner_from_pattern("gitlab.com/acme/*"), None);
        assert_eq!(owner_from_pattern("github.com/../etc"), None);
    }

    #[test]
    fn derives_single_owner_per_project_and_skips_multi_org_or_elsewhere() {
        let projects = vec![
            project("Northwind", &["github.com/northwind/*"]),
            project("Multi", &["github.com/one/*", "github.com/two/*"]),
            project("Empty", &["github.com/kestrel/*"]),
            project("Local", &[]),
        ];
        let seeds = vec![
            seed("Northwind", Some("northwind/checkout")),
            seed("Northwind", Some("Northwind/webhooks")),
            seed("Multi", Some("one/a")),
            seed("Multi", Some("two/b")),
            seed("Local", None),
            seed(ELSEWHERE, Some("stray/repo")),
        ];
        let owners = project_owners(&projects, &seeds, |_| None);
        assert_eq!(owners.get("Northwind").map(String::as_str), Some("northwind"));
        assert_eq!(owners.get("Empty").map(String::as_str), Some("kestrel"));
        assert!(!owners.contains_key("Multi"));
        assert!(!owners.contains_key("Local"));
        assert!(!owners.contains_key(ELSEWHERE));
    }

    #[test]
    fn uses_renamed_slug_for_owner_resolution() {
        let projects = vec![project("Atlas", &[])];
        let seeds = vec![seed("Atlas", Some("old-atlas/app"))];
        let owners = project_owners(&projects, &seeds, |s| (s == "old-atlas/app").then_some("new-atlas/app"));
        assert_eq!(owners.get("Atlas").map(String::as_str), Some("new-atlas"));
    }

    #[test]
    fn validates_image_bytes_and_sizes_avatar_url() {
        assert_eq!(sized_avatar_url("https://avatars.githubusercontent.com/u/1?v=4"), "https://avatars.githubusercontent.com/u/1?v=4&s=64");
        assert_eq!(sized_avatar_url("https://avatars.githubusercontent.com/u/1"), "https://avatars.githubusercontent.com/u/1?s=64");
        assert_eq!(sized_avatar_url("https://avatars.githubusercontent.com/u/1?s=32"), "https://avatars.githubusercontent.com/u/1?s=32");

        let png = b"\x89PNG\r\n\x1a\nrest";
        assert_eq!(detect_mime(png), Some("image/png"));
        assert!(encode_data_url(png).unwrap().starts_with("data:image/png;base64,"));
        assert_eq!(encode_data_url(b"<html>not an image</html>"), None);
        assert_eq!(encode_data_url(&vec![0x89; MAX_BYTES + 1]), None);
    }

    #[test]
    fn caches_avatars_on_disk_and_respects_ttl_and_retry_backoff() {
        let dir = crate::fixtures::scratch("org-avatars-cache");
        let t0 = Instant::now();
        let now_ms = 1_700_000_000_000i64;
        let owners = HashMap::from([("Acme".into(), "acme".into()), ("Personal".into(), "sam".into())]);

        let mut store = OrgAvatars::new(dir.to_path_buf());
        assert!(!store.load_cached(&owners));
        let due = store.to_fetch(&owners, t0, now_ms).unwrap();
        assert_eq!(due, vec!["acme".to_string(), "sam".to_string()]);
        // While inflight, nothing else is scheduled.
        assert_eq!(store.to_fetch(&owners, t0, now_ms), None);

        let url = "data:image/png;base64,iVBORw==".to_string();
        assert!(store.on_fetched(Some(vec![("acme".into(), Outcome::Avatar(url.clone())), ("sam".into(), Outcome::NotOrg)]), now_ms));
        assert_eq!(store.icon_for("acme"), Some(url.as_str()));
        assert_eq!(store.icon_for("sam"), None);
        // Fresh entries are not fetched again.
        assert_eq!(store.to_fetch(&owners, t0 + Duration::from_secs(60), now_ms + 60_000), None);
        // After TTL expires, they are fetched again.
        let stale = store.to_fetch(&owners, t0 + RETRY_AFTER * 2, now_ms + FRESH_FOR_MS + 1).unwrap();
        assert_eq!(stale, vec!["acme".to_string(), "sam".to_string()]);
        // A failed fetch clears inflight and keeps the previously cached avatar.
        assert!(!store.on_fetched(Some(vec![("acme".into(), Outcome::Failed)]), now_ms + FRESH_FOR_MS + 1));
        assert_eq!(store.icon_for("acme"), Some(url.as_str()));
        // Backoff prevents immediate retry after failure.
        assert_eq!(store.to_fetch(&owners, t0 + RETRY_AFTER * 2 + Duration::from_secs(60), now_ms + FRESH_FOR_MS + 60_000), None);

        // A fresh instance (simulating restart) reads the disk cache immediately.
        let mut restarted = OrgAvatars::new(dir.to_path_buf());
        assert!(restarted.load_cached(&owners));
        assert_eq!(restarted.icon_for("acme"), Some(url.as_str()));
        assert_eq!(restarted.icon_for("sam"), None);
        assert_eq!(restarted.to_fetch(&owners, t0, now_ms + 1_000), None);
    }
}

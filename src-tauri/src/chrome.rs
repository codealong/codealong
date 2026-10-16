//! Chrome's profiles, and opening a page in one of them. A worktree's app often signs in with
//! one Google account and not another, so a preview can name the profile it opens in: its
//! project's `chrome_profile` in config.toml, or its worktree's own (previews.rs). A link into
//! the app, in a conversation or a PR's description, opens there too (`board::app_of_link`).
//!
//! Chrome lists its profiles in `Local State` (JSON) under `profile.info_cache`, keyed by folder
//! (`Default`, `Profile 12`), each with the `name` its profile picker shows and the Google
//! account signed in to it (`user_name`); `profile.profiles_order` is the picker's order.
//! A running app only gets arguments when a new copy starts, so the page opens through `open -n`
//! with `--profile-directory`: the new copy hands the URL to the running Chrome, which opens it
//! in that profile's window, and quits. LaunchServices starts it, so it's never Codealong's child.
//! On Linux Chrome keeps the same files in `~/.config/google-chrome`, and its command takes the
//! same arguments, started on its own (`os::open::start_detached`).

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde::Serialize;

use crate::os::{self, open::App, Os};
use crate::util::home;

pub const BUNDLE_ID: &str = "com.google.Chrome";

/// A Chrome profile, as config.toml and the pickers name it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// What config.toml names it by: its name, or its folder when another profile has that name.
    pub id: String,
    /// The name Chrome's profile picker shows.
    pub name: String,
    /// Its folder in Chrome's data folder: `Default`, `Profile 12`.
    pub dir: String,
    /// The Google account signed in to it.
    pub email: Option<String>,
}

/// Chrome's data folder, with a folder for each profile and `Local State`.
pub fn data_dir() -> PathBuf {
    crate::os::dirs::app_data(&home(), "Google/Chrome", "google-chrome")
}

fn local_state() -> PathBuf {
    data_dir().join("Local State")
}

/// Chrome's profiles in its picker's order; none when Chrome isn't set up here.
pub fn profiles() -> Vec<Profile> {
    std::fs::read_to_string(local_state()).map(|t| parse(&t)).unwrap_or_default()
}

fn parse(text: &str) -> Vec<Profile> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    let Some(cache) = v.pointer("/profile/info_cache").and_then(|c| c.as_object()) else { return Vec::new() };
    let order: Vec<&str> = v.pointer("/profile/profiles_order").and_then(|o| o.as_array()).map(|o| o.iter().filter_map(|d| d.as_str()).collect()).unwrap_or_default();
    let mut dirs: Vec<&String> = cache.keys().collect();
    // The picker's order, then any it doesn't list by folder.
    dirs.sort_by_key(|d| (order.iter().position(|o| o == d).unwrap_or(usize::MAX), d.as_str()));
    let text = |d: &serde_json::Value, key: &str| d.get(key).and_then(|x| x.as_str()).map(str::trim).filter(|x| !x.is_empty()).map(str::to_string);
    let mut out: Vec<Profile> = dirs
        .into_iter()
        .map(|dir| {
            let d = &cache[dir];
            Profile { id: String::new(), name: text(d, "name").unwrap_or_else(|| dir.clone()), dir: dir.clone(), email: text(d, "user_name") }
        })
        .collect();
    let names: Vec<String> = out.iter().map(|p| p.name.to_lowercase()).collect();
    for p in &mut out {
        let shared = names.iter().filter(|n| **n == p.name.to_lowercase()).count() > 1;
        p.id = if shared { p.dir.clone() } else { p.name.clone() };
    }
    out
}

/// The profile `wanted` names: by name, then by the account signed in to it, then by folder,
/// ignoring case.
pub fn find<'a>(profiles: &'a [Profile], wanted: &str) -> Option<&'a Profile> {
    let w = wanted.trim();
    profiles
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(w))
        .or_else(|| profiles.iter().find(|p| p.email.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(w))))
        .or_else(|| profiles.iter().find(|p| p.dir.eq_ignore_ascii_case(w)))
}

/// The profile `wanted` names on this Mac, or why there's none, in words.
pub fn resolve(wanted: &str) -> Result<Profile> {
    let all = profiles();
    resolve_in(&all, wanted)
}

fn resolve_in(all: &[Profile], wanted: &str) -> Result<Profile> {
    if all.is_empty() {
        bail!("Chrome lists no profiles on this Mac, so nothing can open in “{}”.", wanted.trim());
    }
    find(all, wanted).cloned().ok_or_else(|| {
        let names: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        anyhow!("Chrome has no profile named “{}”. Its profiles: {}.", wanted.trim(), names.join(", "))
    })
}

/// Opens a web page in one of Chrome's profiles, starting Chrome if it isn't running.
pub fn open(url: &str, profile: &Profile) -> Result<()> {
    let dir = format!("--profile-directory={}", profile.dir);
    let opened = match os::current() {
        Os::MacOs => os::open::launch(App::Id(BUNDLE_ID), true, &[&dir, url], Duration::from_secs(10)),
        Os::Linux => os::open::start_detached(COMMAND, &[&dir, url], Duration::from_secs(10)),
    };
    opened.map_err(|e| anyhow!("Couldn't open {url} in Chrome's {} profile: {e}", profile.name))
}

/// Opens a web page in Chrome, in whichever profile it opens pages in.
pub fn open_page(url: &str) -> Result<()> {
    let opened = match os::current() {
        Os::MacOs => os::open::in_app(App::Id(BUNDLE_ID), Some(url), Duration::from_secs(10)),
        Os::Linux => os::open::start_detached(COMMAND, &[url], Duration::from_secs(10)),
    };
    opened.map_err(|e| anyhow!("Couldn't open {url} in Chrome: {e}"))
}

/// Chrome's command on Linux.
const COMMAND: &str = "google-chrome";

#[cfg(test)]
mod tests {
    use super::*;

    const STATE: &str = r#"{"profile": {
        "profiles_order": ["Default", "Profile 12", "Profile 3", "Profile 4"],
        "info_cache": {
            "Profile 3": {"name": "Dev", "user_name": ""},
            "Default": {"name": "Gwen", "user_name": "me@example.com"},
            "Profile 12": {"name": "Work", "user_name": "me@work.example"},
            "Profile 4": {"name": "dev"},
            "Profile 9": {"name": "Spare"}
        }
    }}"#;

    #[test]
    fn reads_profiles_in_the_pickers_order() {
        let all = parse(STATE);
        let dirs: Vec<&str> = all.iter().map(|p| p.dir.as_str()).collect();
        assert_eq!(dirs, ["Default", "Profile 12", "Profile 3", "Profile 4", "Profile 9"]);
        assert_eq!(all[0].email.as_deref(), Some("me@example.com"));
        assert_eq!(all[2].email, None);
        // Two profiles share a name, so config.toml names each by its folder.
        let ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["Gwen", "Work", "Profile 3", "Profile 4", "Spare"]);
        assert!(parse("{}").is_empty());
        assert!(parse("not json").is_empty());
    }

    #[test]
    fn finds_a_profile_by_name_account_or_folder() {
        let all = parse(STATE);
        let dir = |w: &str| find(&all, w).map(|p| p.dir.as_str());
        assert_eq!(dir("work"), Some("Profile 12"));
        assert_eq!(dir(" Work "), Some("Profile 12"));
        assert_eq!(dir("me@work.example"), Some("Profile 12"));
        assert_eq!(dir("profile 3"), Some("Profile 3"));
        assert_eq!(dir("Default"), Some("Default"));
        assert_eq!(dir("Nope"), None);
        assert!(resolve_in(&all, "Nope").unwrap_err().to_string().contains("Its profiles: Gwen, Work, Profile 3"));
        assert!(resolve_in(&[], "Work").unwrap_err().to_string().contains("no profiles"));
    }
}

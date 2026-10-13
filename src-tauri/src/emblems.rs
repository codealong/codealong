//! Session emblems (`[jev] emblems`): a small icon in a color beside each session's title, so the
//! sessions beside each other read apart at a glance and one is remembered as "the teal bug".
//!
//! - The icon stands for what the session works on. Jev picks it from `ICONS` by the session's
//!   title and first prompt (or its latest, where its harness keeps only that), once it has been
//!   sent one, and it's kept for the session's life (emblems.json): it doesn't change under you
//!   as the title does.
//! - It's drawn on a radial gradient of two tones lit from its top left corner. Over most of it
//!   is a color of Codealong's own, given as it first sees the session: never the session's
//!   harness's, then the color the fewest of its project's current sessions have, then the
//!   fewest of the board's (`pick_color`), kept with the icon. In the corner is the color its harness gives it, Claude Code's `/color`
//!   (`Session::color`), as it stands now; without one the gradient is Codealong's color alone.
//!
//! The colors are the label colors (`--label-*`), which name your own things: there's no
//! amber among them, since amber means only that something needs you, so a harness's yellow or
//! orange is drawn in the nearest color that isn't (`palette_color`). The engine asks Jev and
//! gives colors (`engine.rs` `look_emblems`); `Store::emblem_of` is what each session shows.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::jev;
use crate::model::{Emblem, Session};
use crate::util::DAY_MS;

/// The colors an emblem is drawn in, as the windows' `--label-<color>` tokens name them.
pub const COLORS: [&str; 7] = ["blue", "teal", "green", "purple", "pink", "red", "indigo"];

/// A color as another program names it, as one of `COLORS`: Claude Code's `/color` takes red,
/// blue, green, yellow, purple, orange, pink and cyan. None for one it doesn't know.
pub fn palette_color(name: &str) -> Option<&'static str> {
    Some(match name.trim().to_lowercase().as_str() {
        "blue" => "blue",
        "cyan" | "teal" => "teal",
        // No amber: it means only that something needs you.
        "green" | "yellow" | "lime" => "green",
        "purple" | "violet" => "purple",
        "pink" | "magenta" => "pink",
        "red" | "orange" => "red",
        "indigo" => "indigo",
        _ => return None,
    })
}

/// The icons Jev picks from, by name in the windows' icon set (Tabler's outline icons), with
/// what each stands for. `code` is for work with no clearer subject.
pub const ICONS: [(&str, &str); 59] = [
    ("bug", "Fixing a bug: something broken, a crash, an error, a regression, a flaky or failing test."),
    ("test-pipe", "Writing or running tests: test coverage, a test harness, checks."),
    ("palette", "Visual design: colors, styles, CSS, themes, dark mode, how something looks."),
    ("layout", "A screen's layout: UI components, a panel, a dialog, a page, how a window is arranged."),
    ("typography", "Words on screen: fonts, type sizes, the wording of labels and messages in a UI."),
    ("database", "Stored data: databases, schemas, migrations, queries, SQL, caches."),
    ("book", "Documentation: a guide, a README, notes, writing down how something works."),
    ("rocket", "Shipping: a release, a deploy, publishing, launching, a version bump."),
    ("git-merge", "Git chores: branches, merges, rebases, commits, landing a pull request."),
    ("compass", "Research: exploring a codebase, finding out why or how something happens, comparing options."),
    ("bolt", "Performance: speed, slowness, latency, memory use, profiling, optimizing."),
    ("shield", "Security and privacy: vulnerabilities, permissions, sandboxing, secrets."),
    ("key", "Sign-in and access: authentication, logins, accounts, tokens, API keys, the keychain."),
    ("cloud", "Cloud infrastructure: hosting, Cloudflare, AWS, workers, buckets, DNS."),
    ("server", "A backend service: a server, daemons, background workers, queues."),
    ("api", "An API or protocol: endpoints, HTTP requests, webhooks, JSON, RPC, an SDK."),
    ("terminal-2", "The command line: a CLI tool, shell scripts, how a terminal behaves."),
    ("device-mobile", "A phone app: iPhone, iOS, Android, mobile layouts."),
    ("device-desktop", "A desktop app: a Mac app, native windows, menus, the menu bar."),
    ("world", "A website: web pages, a landing page, the browser, SEO."),
    ("message", "Conversations: chat, messages, replies, threads, transcripts."),
    ("mail", "Email: sending mail, inboxes, newsletters."),
    ("bell", "Notifications: alerts, badges, sounds, reminders."),
    ("chart-line", "Data and metrics: analytics, usage, charts, dashboards, reports."),
    ("table", "Tables of data: spreadsheets, CSV, rows and columns, a data grid."),
    ("calendar", "Dates and schedules: calendars, events, planning, due dates."),
    ("clock", "Timing: timers, cron jobs, timeouts, polling, durations."),
    ("tool", "Developer tooling: build scripts, CI, linting, configuration, the dev setup."),
    ("package", "Dependencies and packaging: upgrades, npm, cargo, bundling, installers."),
    ("recycle", "Cleanup: refactoring, restructuring, renaming, removing old code, simplifying."),
    ("sparkles", "AI: models, prompts, LLM features, how agents behave, evaluations."),
    ("robot", "Automation: agents working on their own, bots, scheduled jobs, workflows."),
    ("keyboard", "The keyboard: shortcuts, key handling, typing, focus."),
    ("photo", "Images: photos, screenshots, icons, image processing."),
    ("movie", "Video and motion: animation, recordings, GIFs, transitions."),
    ("music", "Audio: sounds, music, voice, speech."),
    ("map", "Places: maps, locations, geography, addresses."),
    ("credit-card", "Money: payments, billing, pricing, subscriptions, invoices, finance."),
    ("users", "People: users, teams, profiles, sharing, collaboration."),
    ("file-text", "Files and documents: PDFs, parsing, importing, exporting, file formats."),
    ("pencil", "Writing prose: drafting a letter, an essay, a post, editing text."),
    ("plug", "Integrations: connecting another service, connectors, MCP servers."),
    ("puzzle", "Add-ons: a plugin, a skill, an extension or a module that adds a capability."),
    ("network", "Networking: syncing between machines, peers, devices, connections."),
    ("cpu", "Low-level systems: processes, threads, memory, the operating system, hardware."),
    ("flask", "An experiment: a prototype, a spike, trying an idea out."),
    ("bulb", "Ideas and planning: brainstorming, a proposal, a spec, product thinking."),
    ("checklist", "Tasks: todos, checklists, a tracker, triage."),
    ("school", "Learning: a tutorial, explaining a concept, teaching."),
    ("home", "Home life: the household, family matters, personal errands."),
    ("heart", "Health and wellbeing."),
    ("building-store", "Commerce: a shop, products, orders, a marketplace."),
    ("language", "Languages: translation, localization, internationalization."),
    ("accessible", "Accessibility: screen readers, contrast, keyboard access."),
    ("brand-github", "GitHub itself: issues, Actions, the gh command, repository settings."),
    ("gauge", "Limits: rate limits, usage limits, quotas, capacity."),
    ("cube", "Graphics and games: 3D, rendering, game logic."),
    ("mood-smile", "Something fun: a small delight, an easter egg, a casual chat."),
    ("code", "Writing code with no clearer subject than the ones above."),
];

/// Sessions quiet for longer than this aren't asked about or counted among a project's current
/// ones: turning emblems on shouldn't ask about every session of the past two weeks.
const RECENT_MS: i64 = 2 * DAY_MS;
/// Emblems of sessions not seen for this long are dropped from emblems.json.
const KEEP_MS: i64 = 30 * DAY_MS;
/// How long a session whose question failed waits before it's asked again.
const RETRY_MS: i64 = 5 * 60_000;

fn hash(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// What the session is about, for Jev: the first prompt you gave it, else the latest where its
/// harness keeps only that (Codex, OpenCode). None while it has been sent nothing: its title then
/// is its harness's word for an empty one ("Codex thread"), which an icon kept for its life
/// mustn't be picked from.
fn about(s: &Session) -> Option<&str> {
    [s.first_prompt.as_deref(), s.last_prompt.as_deref()].into_iter().flatten().map(str::trim).find(|p| !p.is_empty())
}

/// Whether the session counts among those it's set apart from, and gets an emblem of its own now:
/// not archived and active lately, or running.
fn current(s: &Session, now: i64) -> bool {
    !s.archived && (s.live || now - s.last_activity < RECENT_MS)
}

/// Codealong's color for a session: never its harness's (`own`), so its two tones differ; then
/// the one the fewest of its project's current sessions have, then the fewest of the whole
/// board's; among those, an order from the session's key, so sessions that come at once don't all
/// take the first. `taken` is each other current session's project and Codealong color.
pub fn pick_color(key: &str, project: &str, own: Option<&str>, taken: &[(&str, &str)]) -> &'static str {
    let start = (hash(key) % COLORS.len() as u64) as usize;
    let uses = |c: &str, here: bool| taken.iter().filter(|(p, t)| *t == c && (!here || *p == project)).count();
    (0..COLORS.len()).map(|i| COLORS[(start + i) % COLORS.len()]).min_by_key(|c| (Some(*c) == own, uses(c, true), uses(c, false))).unwrap_or(COLORS[0])
}

/// Jev's question: which icon stands for what the session works on.
pub fn request(s: &Session, model: &str) -> Value {
    let mut state = Map::new();
    state.insert("session title".into(), json!(jev::clip(&s.title, 200)));
    if let Some(p) = about(s) {
        let said = if s.first_prompt.as_deref().is_some_and(|f| f.trim() == p) { "first prompt" } else { "latest prompt" };
        state.insert(said.into(), json!(jev::clip(p, 600)));
    }
    jev::body(
        model,
        Value::Object(state),
        json!({
            "icon": {
                "type": "choice",
                "instructions": "Which icon best stands for what this coding session works on? Go by its subject, not by the kind of work every session does.",
                "criteria": Value::Object(ICONS.iter().map(|(k, v)| (k.to_string(), json!(v))).collect()),
            },
        }),
    )
}

/// The icon Jev picked, when it's one of `ICONS`.
pub fn read(response: &Value) -> Option<&'static str> {
    let (pick, _) = jev::choice(response, "icon").into_iter().next()?;
    ICONS.iter().map(|(name, _)| *name).find(|name| *name == pick)
}

/* ---- Kept emblems ---- */

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Kept {
    /// The color Codealong gave it.
    pub color: String,
    /// The icon Jev picked, once it has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// When the session was last seen, to drop old ones.
    pub seen_at: i64,
}

/// Emblems by session key, as emblems.json keeps them.
#[derive(Default)]
pub struct Store {
    pub kept: HashMap<String, Kept>,
    /// Sessions whose question failed, and when: asked again only after a while.
    failed: HashMap<String, i64>,
    dirty: bool,
}

fn path() -> PathBuf {
    crate::config::support_dir().join("emblems.json")
}

impl Store {
    pub fn load() -> Store {
        // A file that doesn't read is left as it is, and a notice says so: sessions are
        // remembered by how they look, so their emblems aren't given out again over it.
        Store { kept: crate::store::load_whole(&path(), "the sessions' emblems"), ..Default::default() }
    }

    /// What the session shows: the color Codealong gave it and its harness's as it stands now,
    /// with the icon Jev picked. None until it has an icon.
    pub fn emblem_of(&self, s: &Session) -> Option<Emblem> {
        let kept = self.kept.get(&s.key)?;
        Some(Emblem { color: kept.color.clone(), harness_color: s.color.clone(), icon: kept.icon.clone()? })
    }

    /// Looks over the board's sessions, each with its project: gives a color to each current one
    /// seen for the first time, and says which to ask Jev for an icon, by key. Each session once,
    /// however many lanes list it.
    pub fn look<'a>(&mut self, sessions: impl IntoIterator<Item = (&'a str, &'a Session)>, now: i64) -> Vec<String> {
        let mut by_key: HashMap<&str, (&str, &Session)> = HashMap::new();
        for (project, s) in sessions {
            by_key.entry(s.key.as_str()).or_insert((project, s));
        }
        let mut keys: Vec<&str> = by_key.keys().copied().collect();
        // The oldest first, so colors are given in the order the sessions came.
        keys.sort_by_key(|k| (by_key[k].1.started_at.unwrap_or(by_key[k].1.last_prompt_at), *k));
        let mut taken: Vec<(String, String)> = Vec::new();
        let mut fresh: Vec<&str> = Vec::new();
        for k in &keys {
            let (project, s) = by_key[k];
            self.seen(k, now);
            if !current(s, now) {
                continue;
            }
            if let Some(kept) = self.kept.get(*k) {
                taken.push((project.to_string(), kept.color.clone()));
            }
            if !self.kept.contains_key(*k) && about(s).is_some() {
                fresh.push(k);
            }
        }
        for k in fresh {
            let (project, s) = by_key[k];
            let pairs: Vec<(&str, &str)> = taken.iter().map(|(p, c)| (p.as_str(), c.as_str())).collect();
            let color = pick_color(k, project, s.color.as_deref(), &pairs).to_string();
            taken.push((project.to_string(), color.clone()));
            self.kept.insert(k.to_string(), Kept { color, icon: None, seen_at: now });
            self.dirty = true;
        }
        keys.into_iter()
            .filter(|k| {
                let s = by_key[k].1;
                current(s, now) && about(s).is_some() && self.kept.get(*k).is_some_and(|m| m.icon.is_none()) && self.failed.get(*k).is_none_or(|at| now - at > RETRY_MS)
            })
            .map(String::from)
            .collect()
    }

    pub fn answered(&mut self, key: &str, icon: &str, now: i64) {
        self.failed.remove(key);
        if let Some(m) = self.kept.get_mut(key) {
            m.icon = Some(icon.to_string());
            m.seen_at = now;
            self.dirty = true;
        }
    }

    pub fn failed(&mut self, key: &str, now: i64) {
        self.failed.insert(key.to_string(), now);
    }

    /// Notes that a session with an emblem is still around.
    fn seen(&mut self, key: &str, now: i64) {
        if let Some(m) = self.kept.get_mut(key) {
            // Written at most once a day per session: this runs on every publish.
            if now - m.seen_at > DAY_MS {
                m.seen_at = now;
                self.dirty = true;
            }
        }
    }

    /// Writes emblems.json when something changed, dropping emblems of sessions not seen for a month.
    pub fn save(&mut self, now: i64) {
        if !self.dirty {
            return;
        }
        self.kept.retain(|_, m| now - m.seen_at < KEEP_MS);
        self.dirty = false;
        // Not over a file that didn't read: save_whole leaves it and its notice says why.
        let _ = crate::store::save_whole(&path(), &self.kept);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Harness;

    const NOW: i64 = 40 * DAY_MS;

    fn session(id: &str, prompt: &str) -> Session {
        Session {
            title: prompt.into(),
            first_prompt: Some(prompt.into()),
            started_at: Some(NOW - 3_600_000),
            last_activity: NOW - 60_000,
            last_prompt_at: NOW - 600_000,
            ..Session::new(Harness::Claude, "claude", id)
        }
    }

    #[test]
    fn a_harness_color_is_drawn_without_amber() {
        assert_eq!(["red", "blue", "green", "yellow", "purple", "orange", "pink", "cyan"].map(palette_color), [Some("red"), Some("blue"), Some("green"), Some("green"), Some("purple"), Some("red"), Some("pink"), Some("teal")]);
        assert_eq!(palette_color(" Teal "), Some("teal"));
        assert_eq!(palette_color("default"), None);
        assert!(["red", "blue", "green", "yellow", "purple", "orange", "pink", "cyan"].iter().filter_map(|c| palette_color(c)).all(|c| COLORS.contains(&c)));
    }

    #[test]
    fn a_new_color_is_the_one_fewest_sessions_beside_it_have() {
        // Every color but indigo is taken once in this project: indigo, whatever the key.
        let here: Vec<(&str, &str)> = COLORS.iter().filter(|c| **c != "indigo").map(|c| ("A", *c)).collect();
        assert_eq!(pick_color("k1", "A", None, &here), "indigo");
        assert_eq!(pick_color("k2", "A", None, &here), "indigo");
        // Never the harness's own, which is the other tone.
        assert_ne!(pick_color("k1", "A", Some("indigo"), &here), "indigo");
        // Another project's sessions count only to break a tie in this one.
        let mut taken = vec![("A", "blue"), ("A", "teal"), ("A", "green"), ("A", "purple"), ("A", "pink")];
        taken.extend([("B", "red"), ("B", "red")]);
        assert_eq!(pick_color("k3", "A", None, &taken), "indigo");
        // With nothing taken, sessions that come at once spread over the colors by key.
        let picks: std::collections::HashSet<&str> = (0..40).map(|i| pick_color(&format!("claude:claude:{i}"), "A", None, &[])).collect();
        assert!(picks.len() > 3, "{picks:?}");
    }

    #[test]
    fn gives_each_current_session_a_color_once_and_asks_for_its_icon() {
        let mut store = Store::default();
        let a = session("a", "Fix the flaky login test");
        let b = Session { started_at: Some(NOW - 1_800_000), ..session("b", "Dark mode for the settings") };
        let old = Session { last_activity: NOW - 3 * DAY_MS, ..session("old", "Old work") };
        let archived = Session { archived: true, ..session("arch", "Archived work") };
        // Sent nothing yet, its title is its harness's word for an empty one.
        let empty = Session { first_prompt: None, title: "Codex thread".into(), ..session("empty12", "") };
        let board = [("A", &a), ("A", &b), ("A", &old), ("A", &archived), ("A", &empty), ("B", &a)];
        let asks = store.look(board, NOW);
        assert_eq!(asks, vec![a.key.clone(), b.key.clone()], "the oldest first, each once");
        assert_ne!(store.kept[&a.key].color, store.kept[&b.key].color, "two sessions side by side");
        assert!(!store.kept.contains_key(&old.key) && !store.kept.contains_key(&archived.key) && !store.kept.contains_key(&empty.key));
        assert_eq!(store.emblem_of(&a), None, "no emblem until it has an icon");
        // A color given stays; asked again until Jev answers, then not.
        let color = store.kept[&a.key].color.clone();
        store.answered(&a.key, "bug", NOW);
        assert_eq!(store.look(board, NOW + 1000), vec![b.key.clone()]);
        assert_eq!(store.emblem_of(&a), Some(Emblem { color: color.clone(), harness_color: None, icon: "bug".into() }));
        // Its harness's color is the other tone, as it stands now; Codealong's stays.
        let teal = Session { color: Some("teal".into()), ..a.clone() };
        assert_eq!(store.emblem_of(&teal), Some(Emblem { color: color.clone(), harness_color: Some("teal".into()), icon: "bug".into() }));
    }

    /// Codealong's tone is never the harness's, and only Codealong's tones count for the others:
    /// a harness's colors are yours to pick, and may well repeat.
    #[test]
    fn codealongs_tone_is_its_own() {
        let mut store = Store::default();
        let sessions: Vec<Session> = (0..COLORS.len()).map(|i| Session { color: Some("blue".into()), ..session(&format!("s{i}"), "Some work") }).collect();
        store.look(sessions.iter().map(|s| ("A", s)), NOW);
        let given: std::collections::HashSet<&str> = sessions.iter().map(|s| store.kept[&s.key].color.as_str()).collect();
        assert!(!given.contains("blue"), "{given:?}");
        assert_eq!(given.len(), COLORS.len() - 1, "spread over the rest: {given:?}");
    }

    /// Where a harness keeps only the latest prompt (Codex, OpenCode), that's what Jev goes by.
    #[test]
    fn goes_by_the_latest_prompt_where_theres_no_first() {
        let codex = Session { first_prompt: None, last_prompt: Some("Speed up the search index".into()), title: "Speed up the search index".into(), ..session("cx", "") };
        let mut store = Store::default();
        assert_eq!(store.look([("A", &codex)], NOW), vec![codex.key.clone()]);
        let body = request(&codex, "m");
        assert_eq!(body["state"]["latest prompt"], "Speed up the search index");
        assert!(body["state"].get("first prompt").is_none());
        let claude = Session { last_prompt: Some("And the docs".into()), ..session("cl", "Fix the login") };
        assert_eq!(request(&claude, "m")["state"]["first prompt"], "Fix the login", "the first says what it's for");
    }

    #[test]
    fn waits_after_a_failure() {
        let mut store = Store::default();
        let a = session("a", "Fix the login");
        assert_eq!(store.look([("A", &a)], NOW).len(), 1);
        store.failed(&a.key, NOW);
        assert!(store.look([("A", &a)], NOW + 60_000).is_empty());
        assert_eq!(store.look([("A", &a)], NOW + RETRY_MS + 1).len(), 1);
    }

    #[test]
    fn asks_one_choice_over_the_icons_and_reads_it_back() {
        let s = session("a", "Make the settings dialog look right in dark mode");
        let body = request(&s, "jev-latest");
        assert_eq!(body["state"]["first prompt"], "Make the settings dialog look right in dark mode");
        assert_eq!(body["questions"]["icon"]["type"], "choice");
        assert_eq!(body["questions"]["icon"]["criteria"].as_object().unwrap().len(), ICONS.len());
        assert_eq!(read(&json!({ "answers": { "icon": { "probabilities": { "palette": 0.8, "layout": 0.2 } } } })), Some("palette"));
        assert_eq!(read(&json!({ "answers": { "icon": { "probabilities": { "not-an-icon": 0.9 } } } })), None);
        assert_eq!(read(&json!({ "answers": {} })), None);
    }

    /// Every icon Jev can pick is one the windows can draw, and Jev's limit of 255 options holds.
    #[test]
    fn every_icon_is_in_the_windows_icon_set() {
        let icons = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/ui/icons.ts")).unwrap();
        for (name, _) in ICONS {
            assert!(icons.contains(&format!("\n \"{name}\": [")), "{name} isn't in src/ui/icons.ts");
        }
        let names: std::collections::HashSet<&str> = ICONS.iter().map(|(n, _)| *n).collect();
        assert_eq!(names.len(), ICONS.len(), "an icon listed twice");
        assert!(ICONS.len() <= 255);
    }
}

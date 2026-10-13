//! Session emblems (`[jev] emblems`): a small icon in a color beside each session's title, so the
//! sessions beside each other read apart at a glance and one is remembered as "the teal bug".
//!
//! - The icon stands for what the session works on. Jev picks it from `ICONS` by the session's
//!   title and first prompt (or its latest, where its harness keeps only that), once it has been
//!   sent one, and it's kept for the session's life (emblems.json): it doesn't change under you
//!   as the title does.
//! - It's drawn on a radial gradient of two colors that go together (`PAIRS`), lit from one of
//!   eight places around its edge: a main color over most of it and an accent where the light
//!   falls. Codealong gives each session its pair and its light as it first sees it, so the
//!   sessions of a project differ as much as they can (`pick`), and keeps them with the icon.
//!   The color its harness gives it, Claude Code's `/color` (`Session::color`), is the accent
//!   while it has one, with a main color that goes with it (`tones`).
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

/// The colors an emblem is drawn in, as the windows' `--emblem-<color>` tokens name them, in hue
/// order: round the color wheel about 21° apart in OKLCH, skipping amber, which means only that
/// something needs you. Seven are the label colors; the rest sit between them.
pub const COLORS: [&str; 13] = ["red", "rose", "pink", "magenta", "violet", "purple", "indigo", "blue", "sky", "teal", "mint", "green", "lime"];

/// The colors an emblem's two tones are drawn in together: those whose hues are at most 90° apart
/// in OKLCH in both themes, up to four steps along `COLORS`, near enough on the color wheel to sit
/// together. Farther ones clash (red and lime, across where amber would be; violet and sky, 91°).
/// Every color is in three pairs at least. A test holds this to the tokens' colors.
pub const PAIRS: [(&str, &str); 37] = [
    ("red", "rose"),
    ("red", "pink"),
    ("red", "magenta"),
    ("red", "violet"),
    ("rose", "pink"),
    ("rose", "magenta"),
    ("rose", "violet"),
    ("rose", "purple"),
    ("pink", "magenta"),
    ("pink", "violet"),
    ("pink", "purple"),
    ("pink", "indigo"),
    ("magenta", "violet"),
    ("magenta", "purple"),
    ("magenta", "indigo"),
    ("magenta", "blue"),
    ("violet", "purple"),
    ("violet", "indigo"),
    ("violet", "blue"),
    ("purple", "indigo"),
    ("purple", "blue"),
    ("purple", "sky"),
    ("indigo", "blue"),
    ("indigo", "sky"),
    ("indigo", "teal"),
    ("blue", "sky"),
    ("blue", "teal"),
    ("blue", "mint"),
    ("sky", "teal"),
    ("sky", "mint"),
    ("sky", "green"),
    ("teal", "mint"),
    ("teal", "green"),
    ("teal", "lime"),
    ("mint", "green"),
    ("mint", "lime"),
    ("green", "lime"),
];

/// How many places the light can fall on an emblem: clockwise around its edge from the top left
/// corner, as the windows draw them (primitives.tsx `EMBLEM_LIGHTS`).
pub const LIGHTS: u8 = 8;

/// Which set of colors and pairs emblems are given from now; one more each time `COLORS` or
/// `PAIRS` changes. A current session's emblem given from an earlier set is given its tones
/// again, keeping its icon, so the change shows on every session at once rather than only new ones.
pub const PALETTE: u8 = 2;

/// The colors that go with one, in `PAIRS`' order.
pub fn partners(color: &str) -> Vec<&'static str> {
    PAIRS.iter().filter_map(|(a, b)| if *a == color { Some(*b) } else if *b == color { Some(*a) } else { None }).collect()
}

/// A color as another program names it, as one of `COLORS`: Claude Code's `/color` takes red,
/// blue, green, yellow, purple, orange, pink and cyan. None for one it doesn't know.
pub fn palette_color(name: &str) -> Option<&'static str> {
    let name = name.trim().to_lowercase();
    match name.as_str() {
        "cyan" => Some("teal"),
        // No amber: it means only that something needs you. Yellow goes to the yellowish green.
        "yellow" => Some("lime"),
        "orange" => Some("red"),
        "fuchsia" => Some("magenta"),
        _ => COLORS.iter().copied().find(|c| *c == name),
    }
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

/// What another current session shows, for `pick` to set a new one apart from.
#[derive(Clone, Debug, PartialEq)]
pub struct Taken {
    pub project: String,
    pub color: String,
    pub accent: String,
    pub light: u8,
}

/// The list rotated to start where the key says, so sessions that come at once and tie don't all
/// take the first.
fn from_key<T: Copy>(key: &str, salt: &str, list: &[T]) -> Vec<T> {
    let start = (hash(&format!("{salt}{key}")) % list.len().max(1) as u64) as usize;
    (0..list.len()).map(|i| list[(start + i) % list.len()]).collect()
}

/// A session's main color, accent and light, as Codealong gives them. With its harness's color
/// (`own`) the accent is that, and the main color the partner of it the fewest of its project's
/// current sessions have; without one, the pair whose main color the fewest have, then the pair
/// itself the fewest, in this project, then on the whole board. Then the light the fewest of the
/// project's sessions in that main color have, then the fewest of the project's. `taken` is what
/// each other current session shows.
pub fn pick(key: &str, project: &str, own: Option<&str>, taken: &[Taken]) -> (&'static str, &'static str, u8) {
    let own = own.filter(|c| COLORS.contains(c));
    let both_ways: Vec<(&'static str, &'static str)> = PAIRS.iter().flat_map(|(a, b)| [(*a, *b), (*b, *a)]).collect();
    let with_own: Vec<(&'static str, &'static str)> = both_ways.iter().copied().filter(|(_, accent)| Some(*accent) == own).collect();
    let pairs = if with_own.is_empty() { both_ways } else { with_own };
    let count = |here: bool, f: &dyn Fn(&Taken) -> bool| taken.iter().filter(|t| (!here || t.project == project) && f(t)).count();
    let (color, accent) = from_key(key, "pair", &pairs)
        .into_iter()
        .min_by_key(|(c, a)| {
            let main = |t: &Taken| t.color == *c;
            let pair = |t: &Taken| t.color == *c && t.accent == *a;
            (count(true, &main), count(true, &pair), count(false, &main), count(false, &pair))
        })
        .unwrap_or(pairs[0]);
    let lights: Vec<u8> = (0..LIGHTS).collect();
    let light = from_key(key, "light", &lights).into_iter().min_by_key(|l| (count(true, &|t: &Taken| t.color == color && t.light == *l), count(true, &|t: &Taken| t.light == *l))).unwrap_or(0);
    (color, accent, light)
}

/// The two tones a session shows: what Codealong gave it, or with its harness's color (`own`) as
/// the accent, a main color that goes with it: Codealong's own when it does, the accent Codealong
/// gave it when that does (`/color` set to its main color swaps them), else a partner of it.
/// Emblems kept before they had accents get a partner of their color.
pub fn tones(key: &str, kept: &Kept, own: Option<&str>) -> (String, String) {
    let accent = kept.accent.clone().unwrap_or_else(|| from_key(key, "pair", &partners(&kept.color)).first().copied().unwrap_or("teal").to_string());
    // A color the windows can't draw (none comes through `palette_color`) is left out.
    let Some(own) = own.filter(|c| COLORS.contains(c)) else { return (kept.color.clone(), accent) };
    let goes = partners(own);
    let main = [kept.color.as_str(), accent.as_str()].into_iter().find(|c| goes.contains(c)).map(String::from);
    (main.unwrap_or_else(|| from_key(key, "pair", &goes).first().copied().unwrap_or("teal").to_string()), own.to_string())
}

/// Where the light falls on a session's emblem: where Codealong put it, or from its key for an
/// emblem kept before it had one.
fn light_of(key: &str, kept: &Kept) -> u8 {
    kept.light.filter(|l| *l < LIGHTS).unwrap_or((hash(&format!("light{key}")) % LIGHTS as u64) as u8)
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
    /// The main color Codealong gave it.
    pub color: String,
    /// The accent Codealong gave it, a color that goes with `color`; its harness's color stands
    /// over it while it has one (`tones`). None in emblems kept before emblems had two.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    /// Where Codealong put its light, 0 to `LIGHTS` - 1. None in emblems kept before that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<u8>,
    /// The `PALETTE` its tones were given from. None in emblems given before there was one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<u8>,
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

    /// What the session shows: the tones and light Codealong gave it, its harness's color as the
    /// accent while it has one, with the icon Jev picked. None until it has an icon.
    pub fn emblem_of(&self, s: &Session) -> Option<Emblem> {
        let kept = self.kept.get(&s.key)?;
        let icon = kept.icon.clone()?;
        let (color, accent) = tones(&s.key, kept, s.color.as_deref());
        Some(Emblem { color, accent, light: light_of(&s.key, kept), icon })
    }

    /// What a session with an emblem shows, icon or not, for the sessions after it to be set
    /// apart from.
    fn taken(&self, project: &str, s: &Session) -> Option<Taken> {
        let kept = self.kept.get(&s.key)?;
        let (color, accent) = tones(&s.key, kept, s.color.as_deref());
        Some(Taken { project: project.to_string(), color, accent, light: light_of(&s.key, kept) })
    }

    /// Looks over the board's sessions, each with its project: gives tones and a light to each
    /// current one seen for the first time, and says which to ask Jev for an icon, by key. Each
    /// session once, however many lanes list it.
    pub fn look<'a>(&mut self, sessions: impl IntoIterator<Item = (&'a str, &'a Session)>, now: i64) -> Vec<String> {
        let mut by_key: HashMap<&str, (&str, &Session)> = HashMap::new();
        for (project, s) in sessions {
            by_key.entry(s.key.as_str()).or_insert((project, s));
        }
        let mut keys: Vec<&str> = by_key.keys().copied().collect();
        // The oldest first, so colors are given in the order the sessions came.
        keys.sort_by_key(|k| (by_key[k].1.started_at.unwrap_or(by_key[k].1.last_prompt_at), *k));
        let mut taken: Vec<Taken> = Vec::new();
        // Seen for the first time, or given their tones from an earlier palette.
        let mut fresh: Vec<&str> = Vec::new();
        for k in &keys {
            let (project, s) = by_key[k];
            self.seen(k, now);
            if !current(s, now) {
                continue;
            }
            let earlier = self.kept.get(*k).is_some_and(|m| m.palette != Some(PALETTE));
            match self.taken(project, s) {
                Some(_) if earlier => fresh.push(k),
                Some(t) => taken.push(t),
                None if about(s).is_some() => fresh.push(k),
                None => {}
            }
        }
        for k in fresh {
            let (project, s) = by_key[k];
            let (color, accent, light) = pick(k, project, s.color.as_deref(), &taken);
            let icon = self.kept.get(k).and_then(|m| m.icon.clone());
            self.kept.insert(k.to_string(), Kept { color: color.into(), accent: Some(accent.into()), light: Some(light), palette: Some(PALETTE), icon, seen_at: now });
            taken.extend(self.taken(project, s));
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
        assert_eq!(["red", "blue", "green", "yellow", "purple", "orange", "pink", "cyan"].map(palette_color), [Some("red"), Some("blue"), Some("green"), Some("lime"), Some("purple"), Some("red"), Some("pink"), Some("teal")]);
        assert_eq!(palette_color("magenta"), Some("magenta"));
        assert_eq!(palette_color(" Teal "), Some("teal"));
        assert_eq!(palette_color("default"), None);
        assert!(["red", "blue", "green", "yellow", "purple", "orange", "pink", "cyan"].iter().filter_map(|c| palette_color(c)).all(|c| COLORS.contains(&c)));
    }

    fn taken(project: &str, color: &str, accent: &str, light: u8) -> Taken {
        Taken { project: project.into(), color: color.into(), accent: accent.into(), light }
    }

    /// A token's color in a theme, through any alias (`{label-red}`).
    fn token_hex(tokens: &Value, name: &str, theme: &str) -> String {
        let t = tokens["color"]["tokens"].as_array().unwrap().iter().find(|t| t["name"] == name).unwrap_or_else(|| panic!("no {name} token"));
        let v = t["value"].get(theme).unwrap_or(&t["value"]).as_str().unwrap();
        match v.strip_prefix('{').and_then(|v| v.strip_suffix('}')) {
            Some(alias) => token_hex(tokens, alias, theme),
            None => v.to_string(),
        }
    }

    /// A color's hue in OKLCH, which spaces hues as the eye does.
    fn oklch_hue(hex: &str) -> f64 {
        let lin = |c: f64| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        let [r, g, b] = [1, 3, 5].map(|i| lin(u8::from_str_radix(&hex[i..i + 2], 16).unwrap() as f64 / 255.0));
        let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
        let a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
        let bb = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
        bb.atan2(a).to_degrees().rem_euclid(360.0)
    }

    /// The colors are the windows' `emblem-*` tokens, in hue order, and the pairs are all of those
    /// at most 90° apart in OKLCH in both themes, as the tokens have them, so a change to a color
    /// can't leave a pair that clashes.
    #[test]
    fn the_pairs_are_the_colors_that_go_together() {
        let tokens: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../design/tokens.json")).unwrap()).unwrap();
        let hue = |c: &str, theme: &str| oklch_hue(&token_hex(&tokens, &format!("emblem-{c}"), theme));
        let apart = |a: &str, b: &str, theme: &str| {
            let d = (hue(a, theme) - hue(b, theme)).abs();
            d.min(360.0 - d)
        };
        // Round the wheel in one direction, as COLORS lists them.
        for w in COLORS.windows(2) {
            let step = (hue(w[0], "light") - hue(w[1], "light")).rem_euclid(360.0);
            assert!((10.0..35.0).contains(&step), "{} to {} is {step:.0}°", w[0], w[1]);
        }
        let mut want: Vec<(&str, &str)> = Vec::new();
        for (i, a) in COLORS.iter().enumerate() {
            for b in &COLORS[i + 1..] {
                if ["light", "dark"].iter().all(|theme| apart(a, b, theme) <= 90.0) {
                    want.push((a, b));
                }
            }
        }
        let norm = |(a, b): (&str, &str)| if a < b { (a.to_string(), b.to_string()) } else { (b.to_string(), a.to_string()) };
        let mut have: Vec<_> = PAIRS.iter().map(|p| norm(*p)).collect();
        let mut want: Vec<_> = want.into_iter().map(norm).collect();
        have.sort();
        want.sort();
        assert_eq!(have, want);
        assert!(COLORS.iter().all(|c| partners(c).len() >= 3), "every color in three pairs at least");
    }

    #[test]
    fn a_new_emblem_is_set_apart_from_its_projects() {
        // Every main color but indigo is taken once in this project: indigo, whatever the key.
        let here: Vec<Taken> = COLORS.iter().filter(|c| **c != "indigo").map(|c| taken("A", c, partners(c)[0], 0)).collect();
        for key in ["k1", "k2", "k3"] {
            let (color, accent, _) = pick(key, "A", None, &here);
            assert_eq!(color, "indigo");
            assert!(partners("indigo").contains(&accent), "{accent} goes with indigo");
        }
        // Another project's sessions count only to break a tie in this one.
        let mut there = here.clone();
        there.retain(|t| t.color != "red");
        there.extend([taken("B", "indigo", "teal", 0), taken("B", "indigo", "pink", 1)]);
        assert_eq!(pick("k4", "A", None, &there).0, "red");
        // Two sessions in one main color get different lights.
        let reds: Vec<Taken> = (0..LIGHTS - 1).map(|l| taken("A", "red", "pink", l)).collect();
        let only: Vec<Taken> = COLORS.iter().filter(|c| **c != "red").flat_map(|c| (0..LIGHTS).map(move |l| taken("A", c, partners(c)[0], l))).chain(reds).collect();
        let (color, accent, light) = pick("k5", "A", None, &only);
        assert_eq!((color, light), ("red", LIGHTS - 1));
        assert!(accent != "pink" && partners("red").contains(&accent), "a pair of red's not taken yet: {accent}");
        // With nothing taken, sessions that come at once spread by key, in pairs that go together.
        let picks: Vec<_> = (0..40).map(|i| pick(&format!("claude:claude:{i}"), "A", None, &[])).collect();
        assert!(picks.iter().map(|p| p.0).collect::<std::collections::HashSet<_>>().len() > 3);
        assert!(picks.iter().map(|p| p.2).collect::<std::collections::HashSet<_>>().len() > 3);
        assert!(picks.iter().all(|(c, a, _)| partners(c).contains(a)));
    }

    /// With its harness's color, that's the accent, and the main color one that goes with it.
    #[test]
    fn a_harness_color_is_the_accent() {
        let (color, accent, _) = pick("k", "A", Some("red"), &[]);
        assert_eq!(accent, "red");
        assert!(partners("red").contains(&color));
        let kept = Kept { color: "teal".into(), accent: Some("blue".into()), light: Some(3), palette: Some(PALETTE), icon: None, seen_at: NOW };
        assert_eq!(tones("k", &kept, None), ("teal".into(), "blue".into()));
        // `/color` set later: the main color stays where it goes with it.
        assert_eq!(tones("k", &kept, Some("green")), ("teal".into(), "green".into()));
        // Set to the main color itself, the two swap.
        assert_eq!(tones("k", &kept, Some("teal")), ("blue".into(), "teal".into()));
        // To one neither goes with, a partner of it.
        let (main, accent) = tones("k", &kept, Some("red"));
        assert_eq!(accent, "red");
        assert!(partners("red").contains(&main.as_str()), "{main}");
        // A color the windows can't draw is left out.
        assert_eq!(tones("k", &kept, Some("mauve")), ("teal".into(), "blue".into()));
        // An emblem kept before emblems had two tones gets a partner of its color, and a light.
        let old = Kept { color: "teal".into(), accent: None, light: None, palette: None, icon: None, seen_at: NOW };
        assert!(partners("teal").contains(&tones("k", &old, None).1.as_str()));
        assert!(light_of("k", &old) < LIGHTS);
        let kept: Kept = serde_json::from_str(r#"{"color":"teal","icon":"bug","seenAt":1}"#).unwrap();
        assert_eq!((kept.accent, kept.light), (None, None), "emblems.json from before reads");
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
        // What's given stays; asked again until Jev answers, then not.
        let given = store.kept[&a.key].clone();
        store.answered(&a.key, "bug", NOW);
        assert_eq!(store.look(board, NOW + 1000), vec![b.key.clone()]);
        let shown = store.emblem_of(&a).unwrap();
        assert_eq!((Some(shown.color), Some(shown.accent), Some(shown.light), shown.icon.as_str()), (Some(given.color), given.accent, given.light, "bug"));
    }

    /// Sessions whose harness gives them all one color still read apart: the main colors and
    /// lights Codealong gives them differ.
    #[test]
    fn one_harness_color_for_all_still_reads_apart() {
        let mut store = Store::default();
        let sessions: Vec<Session> = (0..4).map(|i| Session { color: Some("blue".into()), ..session(&format!("s{i}"), "Some work") }).collect();
        store.look(sessions.iter().map(|s| ("A", s)), NOW);
        let shown: Vec<(String, String, u8)> = sessions.iter().map(|s| store.taken("A", s).map(|t| (t.color, t.accent, t.light)).unwrap()).collect();
        assert!(shown.iter().all(|(c, a, _)| a == "blue" && partners("blue").contains(&c.as_str())), "{shown:?}");
        let mains: std::collections::HashSet<&str> = shown.iter().map(|s| s.0.as_str()).collect();
        assert_eq!(mains.len(), sessions.len(), "no partner of blue twice while others are left: {shown:?}");
        assert_eq!(shown.iter().collect::<std::collections::HashSet<_>>().len(), shown.len(), "no two alike: {shown:?}");
    }

    /// A current session whose emblem was given from an earlier palette is given its tones again
    /// from this one, keeping its icon; one that isn't current, or was given from this one, keeps
    /// what it has.
    #[test]
    fn an_earlier_palettes_emblem_is_given_new_tones() {
        let mut store = Store::default();
        let now = session("now", "Fix the login");
        let quiet = Session { last_activity: NOW - 3 * DAY_MS, ..session("quiet", "Old work") };
        let fresh = session("fresh", "Docs");
        let earlier = Kept { color: "green".into(), accent: Some("indigo".into()), light: Some(1), palette: None, icon: Some("bug".into()), seen_at: NOW };
        store.kept.insert(now.key.clone(), earlier.clone());
        store.kept.insert(quiet.key.clone(), earlier.clone());
        store.kept.insert(fresh.key.clone(), Kept { palette: Some(PALETTE), ..earlier.clone() });
        let asks = store.look([("A", &now), ("A", &quiet), ("A", &fresh)], NOW);
        assert!(asks.is_empty(), "the icons stay, so Jev isn't asked again");
        let given = &store.kept[&now.key];
        assert_eq!((given.palette, given.icon.as_deref()), (Some(PALETTE), Some("bug")));
        assert!(partners(&given.color).contains(&given.accent.as_deref().unwrap()), "{given:?}");
        assert_eq!(store.kept[&quiet.key], earlier, "not current: as it was");
        assert_eq!(store.kept[&fresh.key].palette, Some(PALETTE));
        assert_eq!((store.kept[&fresh.key].color.as_str(), store.kept[&fresh.key].light), ("green", Some(1)), "from this palette: as it was");
        // Given again once only.
        let again = store.kept[&now.key].clone();
        store.look([("A", &now)], NOW + 1000);
        assert_eq!(store.kept[&now.key], again);
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

    /// The windows draw as many lights as Codealong gives out.
    #[test]
    fn the_windows_draw_every_light() {
        let ui = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/ui/primitives.tsx")).unwrap();
        let start = ui.find("export const EMBLEM_LIGHTS = [").expect("EMBLEM_LIGHTS in primitives.tsx");
        let list = &ui[start..start + ui[start..].find("];").unwrap()];
        assert_eq!(list.matches('%').count(), 2 * LIGHTS as usize, "{list}");
    }
}

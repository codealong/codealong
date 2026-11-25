//! Experimental per-session AI backgrounds (`[backgrounds]` + `[experimental] session_themes`):
//! draws a bespoke vertical fine-art background per top-level session using an account whose
//! harness draws (Codex or Antigravity), keeps it compact under `images/`, extracts its dominant
//! accent color and center reading luminance, and caches it for the session's life
//! (`backgrounds.json`).
//!
//! Runs automatically once enabled, skipping helpers, headless runs, and automatic PR review
//! sessions so only your own top-level sessions spend image turns. Prompt resolution follows:
//! 1. Per-project `background_prompt` in `config.toml` (`[[projects]]`)
//! 2. Repo team file `.codealong/config.toml` (`[backgrounds] prompt` or `prompt_file`)
//! 3. Global `[backgrounds] prompt` in `config.toml`
//! 4. Built-in `DEFAULT_STYLE` (high-variance editorial scenes with calm reading negative space).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::{AccountCfg, Config};
use crate::model::{Board, Session, SessionBackdrop};
use crate::teamfile::TeamCfg;
use crate::util::DAY_MS;

/// Default style instructions when neither project, repo team file, nor global `[backgrounds]`
/// specifies one. Encourages strong visual variance across sessions while protecting legibility.
pub const DEFAULT_STYLE: &str = "Choose a distinct artistic medium and serene subject inspired by the session (for example: minimalist Japanese woodblock ridges, dappled botanical shadows on linen, fluted frosted-glass light caustics, sweeping matte gouache architecture, ceramic glaze washes, or calm celestial mist). Keep 70% of the center and left area smooth, low-contrast negative space so UI text stays effortlessly readable, placing richer detail along the top, right, and bottom edges. Subtle matte film or paper grain. Strictly no text, letters, logos, or borders.";

/// Maximum image dimension stored for a session background: sharp on Retina panels while keeping
/// file size small (~45–65 KB as JPEG).
const MAX_PX: usize = 760;

/// Only sessions active within the last 48 hours get automatic backgrounds so turning the
/// experiment on never backfills weeks of history.
const RECENT_MS: i64 = 2 * DAY_MS;

/// Unused backgrounds older than this are pruned from `backgrounds.json`.
const KEEP_MS: i64 = 30 * DAY_MS;

/// Backoff after a drawing failure (e.g. quota limit or signed-out account).
const RETRY_MS: i64 = 5 * 60_000;

/// `[backgrounds]` in `config.toml` (only active when `[experimental] session_themes` is on).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BackgroundsCfg {
    /// Draw a bespoke AI background for each new top-level session (off by default).
    #[serde(default)]
    pub enabled: bool,
    /// Account that draws backgrounds (Codex or Antigravity). Empty uses the emblems drawing
    /// account when it draws, else the first enabled account whose harness draws.
    #[serde(default)]
    pub account: String,
    /// Global style prompt added to every session background. Empty uses `DEFAULT_STYLE`.
    #[serde(default)]
    pub prompt: String,
}

const BACKGROUNDS_COMMENT: &str = "# Per-session AI backgrounds, once [experimental] session_themes is on. enabled: draw one for each\n# top-level session (off by default; skips helpers and headless runs). account: Codex or Antigravity\n# account to draw with (\"\" uses the emblems account or first drawing account). prompt: global style.\n";

/// Patches `[backgrounds]` in a `config.toml` document in place; only added when non-default or
/// already present in the file.
pub fn patch_toml(doc: &mut toml_edit::DocumentMut, cfg: &BackgroundsCfg) {
    use toml_edit::{Item, Table, Value};
    if *cfg == BackgroundsCfg::default() && doc.get("backgrounds").is_none() {
        return;
    }
    if !doc.get("backgrounds").is_some_and(Item::is_table_like) {
        let mut t = Table::new();
        t.decor_mut().set_prefix(format!("\n{BACKGROUNDS_COMMENT}"));
        doc.insert("backgrounds", Item::Table(t));
    }
    doc["backgrounds"]["enabled"] = Item::Value(Value::from(cfg.enabled));
    if !cfg.account.trim().is_empty() || doc["backgrounds"].get("account").is_some() {
        doc["backgrounds"]["account"] = Item::Value(Value::from(cfg.account.trim()));
    }
    if !cfg.prompt.trim().is_empty() || doc["backgrounds"].get("prompt").is_some() {
        doc["backgrounds"]["prompt"] = Item::Value(Value::from(cfg.prompt.trim()));
    }
}

/// Renders `[backgrounds]` for a freshly written `config.toml` when non-default.
pub fn render_toml(cfg: &BackgroundsCfg) -> String {
    if *cfg == BackgroundsCfg::default() {
        return String::new();
    }
    let mut s = format!("\n{BACKGROUNDS_COMMENT}[backgrounds]\nenabled = {}\n", cfg.enabled);
    if !cfg.account.trim().is_empty() {
        s.push_str(&format!("account = {}\n", toml_edit::Value::from(cfg.account.trim())));
    }
    if !cfg.prompt.trim().is_empty() {
        s.push_str(&format!("prompt = {}\n", toml_edit::Value::from(cfg.prompt.trim())));
    }
    s
}

/// The enabled drawing account to use for AI backgrounds: `[backgrounds] account`, else
/// `[emblems] account`, else the first enabled account whose harness draws (Codex or Antigravity).
pub fn drawing_account(cfg: &Config) -> Option<&AccountCfg> {
    let draws = |a: &&AccountCfg| !a.disabled && crate::harness::of(a).is_some_and(|h| h.draws());
    let bg = cfg.backgrounds.account.trim();
    if !bg.is_empty() {
        if let Some(a) = cfg.accounts.iter().filter(draws).find(|a| a.name == bg) {
            return Some(a);
        }
    }
    let emb = cfg.emblems.account.trim();
    if !emb.is_empty() {
        if let Some(a) = cfg.accounts.iter().filter(draws).find(|a| a.name == emb) {
            return Some(a);
        }
    }
    cfg.accounts.iter().find(draws)
}

/// Effective style prompt for a session in `project` / `team`:
/// project personal override -> repo `.codealong/config.toml` -> global `[backgrounds] prompt` -> `DEFAULT_STYLE`.
pub fn effective_prompt<'a>(cfg: &'a Config, team: Option<&'a TeamCfg>, project: &str) -> &'a str {
    if let Some(p) = cfg
        .projects
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(project))
        .and_then(|p| p.background_prompt.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return p;
    }
    if let Some(t) = team.map(|t| t.background_prompt.trim()).filter(|s| !s.is_empty()) {
        return t;
    }
    let global = cfg.backgrounds.prompt.trim();
    if !global.is_empty() {
        return global;
    }
    DEFAULT_STYLE
}

/// Whether `s` should automatically get an AI background drawn: only top-level interactive
/// sessions (never helpers, automatic PR reviews, headless runs, or archived sessions).
pub fn eligible(s: &Session) -> bool {
    !s.archived && s.visitor.is_none() && s.helper.is_none() && !s.headless && crate::emblems::about(s).is_some()
}

/// Builds the full image-generation prompt for a session background, guided by the session's
/// subject, emblem, sibling scenes to avoid repeating (`taken`), and resolved style prompt.
pub fn draw_prompt(s: &Session, taken: &[String], theme_mode: &str, style: &str) -> String {
    let mut p = String::from(
        "Draw a vertical 3:4 minimalist fine-art UI background for a coding session with your image generation tool: one picture. Use the tool directly; don't read any files or skills, and don't write code.\n\n",
    );
    p.push_str(&format!("The session is called “{}”.", crate::jev::clip(&s.title, 200)));
    if let Some(asked) = crate::emblems::about(s).filter(|a| a.trim() != s.title.trim()) {
        p.push_str(&format!(" It was asked: “{}”", crate::jev::clip(asked, 500)));
    }
    if let Some(emblem) = s.emblem.as_ref().filter(|e| !e.name.trim().is_empty()) {
        p.push_str(&format!(" Its emblem is “{}”.", crate::jev::clip(&emblem.name, 60)));
    }
    if let Some(color) = s.color.as_deref().filter(|c| !c.trim().is_empty()) {
        p.push_str(&format!(" Harmonize with a {color} palette."));
    }
    if !taken.is_empty() {
        p.push_str(&format!(
            "\n\nNeighboring active sessions already show: {}. Pick a noticeably different visual subject, color mood, and composition so this session reads apart at a glance.",
            crate::util::listed(taken, "and")
        ));
    }
    if theme_mode == "dark" {
        p.push_str("\n\nTone: tuned for dark mode (deep matte charcoal/twilight ground with subtle luminous accents near the edges; dark low-contrast center for light text).");
    } else if theme_mode == "light" {
        p.push_str("\n\nTone: tuned for light mode (airy pale off-white/warm paper ground across the center with soft editorial color near the edges for dark text).");
    }
    let style = if style.trim().is_empty() { DEFAULT_STYLE } else { style.trim() };
    p.push_str(&format!(
        "\n\nStyle instructions: {style}\n\nOnce it's drawn, answer with only a short 2 to 4 word label describing the scene (for example: “Indigo woodblock ridges”)."
    ));
    p
}

/// Runs `account`'s drawing provider on `prompt`, processes & stores the image under `images/`,
/// and extracts its dominant accent color and center luminance.
pub fn draw(account: &AccountCfg, cwd: &Path, prompt: &str) -> Result<SessionBackdrop> {
    let adapter = crate::harness::of(account).ok_or_else(|| anyhow!("{} isn't an account Codealong knows how to run.", account.name))?;
    std::fs::create_dir_all(cwd).with_context(|| format!("making {}", cwd.display()))?;
    let drawn = adapter.draw(&crate::harness::Ctx { account }, cwd, prompt)?;
    let (stored_bytes, accent, luminance) = process_image(&drawn.image)?;
    let kept = crate::attachments::keep_image_in(&crate::attachments::images_dir(), &stored_bytes)
        .ok_or_else(|| anyhow!("Codealong couldn't keep the background {} drew.", account.name))?;
    let picture = kept.strip_prefix(crate::attachments::IMAGES).unwrap_or(&kept).to_string();
    Ok(SessionBackdrop {
        picture,
        name: label_of(&drawn.said),
        accent,
        luminance,
    })
}

/// Where the drawing provider runs: an empty folder of Codealong's own.
pub fn drawing_dir() -> PathBuf {
    crate::config::support_dir().join("background-drawing")
}

fn label_of(said: &str) -> String {
    let line = said.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("");
    let clean = line.trim_matches(|c: char| "\"'`*_.“”‘’".contains(c)).trim();
    if clean.is_empty() {
        "Generated background".to_string()
    } else {
        crate::jev::clip(clean, 48)
    }
}

/// Resizes a drawn PNG/JPEG to a compact display image (~760px max side JPEG on macOS, original
/// bytes elsewhere) and samples a small PNG to extract dominant accent hex + center luminance.
pub fn process_image(bytes: &[u8]) -> Result<(Vec<u8>, Option<String>, u8)> {
    let Some(ext) = crate::attachments::image_kind(bytes) else {
        bail!("The generated background isn't an image format Codealong recognizes.");
    };
    let display = if crate::os::current() == crate::os::Os::MacOs {
        sips_convert(bytes, ext, "jpeg", Some(MAX_PX)).unwrap_or_else(|| bytes.to_vec())
    } else {
        bytes.to_vec()
    };
    let sample_png = if ext == "png" {
        Some(bytes.to_vec())
    } else if crate::os::current() == crate::os::Os::MacOs {
        sips_convert(bytes, ext, "png", Some(48))
    } else {
        None
    };
    let (accent, luminance) = sample_png
        .as_deref()
        .and_then(|p| crate::emblems::decode(p).ok())
        .map(|(w, h, rgba)| analyze_pixels(&rgba, w, h))
        .unwrap_or((None, 128));
    Ok((display, accent, luminance))
}

/// Converts/resizes image bytes using macOS's built-in `sips`.
pub(crate) fn sips_convert(bytes: &[u8], in_ext: &str, out_format: &str, max_px: Option<usize>) -> Option<Vec<u8>> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let id = SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let out_ext = if out_format == "jpeg" { "jpg" } else { out_format };
    let src = std::env::temp_dir().join(format!("codealong-bg-in-{pid}-{id}.{in_ext}"));
    let dst = std::env::temp_dir().join(format!("codealong-bg-out-{pid}-{id}.{out_ext}"));
    std::fs::write(&src, bytes).ok()?;
    let max_str = max_px.map(|n| n.to_string());
    let mut args: Vec<&str> = vec!["-s", "format", out_format];
    if out_format == "jpeg" {
        args.extend(["-s", "formatOptions", "84"]);
    }
    if let Some(ref m) = max_str {
        args.extend(["-Z", m.as_str()]);
    }
    let src_s = src.to_str()?;
    let dst_s = dst.to_str()?;
    args.extend([src_s, "--out", dst_s]);
    let ok = crate::procs::run("/usr/bin/sips", &args, None, Duration::from_secs(20)).is_ok();
    let out = if ok { std::fs::read(&dst).ok() } else { None };
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&dst);
    out
}

/// Extracts `(dominant_accent_hex, center_luminance_0_255)` from decoded RGBA pixels.
pub fn analyze_pixels(rgba: &[u8], w: usize, h: usize) -> (Option<String>, u8) {
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return (None, 128);
    }
    // Center reading region luminance (20%..80% horizontally and vertically).
    let (x0, x1) = (w / 5, (w * 4 / 5).max(w / 5 + 1).min(w));
    let (y0, y1) = (h / 5, (h * 4 / 5).max(h / 5 + 1).min(h));
    let mut lum_sum = 0.0f64;
    let mut lum_count = 0.0f64;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * w + x) * 4;
            let l = 0.2126 * f64::from(rgba[i]) + 0.7152 * f64::from(rgba[i + 1]) + 0.0722 * f64::from(rgba[i + 2]);
            lum_sum += l;
            lum_count += 1.0;
        }
    }
    let luminance = if lum_count > 0.0 { (lum_sum / lum_count).round().clamp(0.0, 255.0) as u8 } else { 128 };

    // Dominant chromatic accent across the whole artwork, grouped into 24 hue buckets (15° each)
    // so two-tone artwork (e.g. teal + warm terracotta) keeps its true dominant color instead of
    // averaging complementary hues into muddy grey.
    const BINS: usize = 24;
    let mut bins = [(0.0f64, 0.0f64, 0.0f64, 0.0f64); BINS];
    for p in rgba.chunks_exact(4) {
        if p[3] < 128 {
            continue;
        }
        let r = f64::from(p[0]) / 255.0;
        let g = f64::from(p[1]) / 255.0;
        let b = f64::from(p[2]) / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let sat = if max > 0.0 { delta / max } else { 0.0 };
        if delta < 0.02 || sat < 0.10 || max < 0.10 || min > 0.95 {
            continue;
        }
        let mut hue = if (max - r).abs() < f64::EPSILON {
            60.0 * (((g - b) / delta) % 6.0)
        } else if (max - g).abs() < f64::EPSILON {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        if hue < 0.0 {
            hue += 360.0;
        }
        let bin = ((hue / 15.0).floor() as usize) % BINS;
        let weight = sat * sat * (0.3 + 0.7 * max);
        bins[bin].0 += r * weight;
        bins[bin].1 += g * weight;
        bins[bin].2 += b * weight;
        bins[bin].3 += weight;
    }
    // Score each bucket together with its immediate 15° neighbors so hues straddling a bin edge
    // don't split, then average pixels from that dominant hue family.
    let best = (0..BINS)
        .max_by(|&a, &b| {
            let score = |i: usize| bins[(i + BINS - 1) % BINS].3 * 0.5 + bins[i].3 + bins[(i + 1) % BINS].3 * 0.5;
            score(a).total_cmp(&score(b))
        })
        .unwrap_or(0);
    let mut r_sum = bins[best].0;
    let mut g_sum = bins[best].1;
    let mut b_sum = bins[best].2;
    let mut w_sum = bins[best].3;
    for neighbor in [(best + BINS - 1) % BINS, (best + 1) % BINS] {
        r_sum += bins[neighbor].0 * 0.5;
        g_sum += bins[neighbor].1 * 0.5;
        b_sum += bins[neighbor].2 * 0.5;
        w_sum += bins[neighbor].3 * 0.5;
    }
    let accent = if w_sum > 0.02 {
        let rr = ((r_sum / w_sum) * 255.0).round().clamp(0.0, 255.0) as u8;
        let gg = ((g_sum / w_sum) * 255.0).round().clamp(0.0, 255.0) as u8;
        let bb = ((b_sum / w_sum) * 255.0).round().clamp(0.0, 255.0) as u8;
        Some(format!("#{rr:02x}{gg:02x}{bb:02x}"))
    } else {
        None
    };
    (accent, luminance)
}

/* ---- Persistence & queue store (`backgrounds.json`) ---- */

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Entry {
    #[serde(flatten)]
    bg: SessionBackdrop,
    seen: i64,
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    #[serde(default)]
    sessions: HashMap<String, Entry>,
}

pub struct Store {
    path: PathBuf,
    entries: HashMap<String, Entry>,
    /// Currently drawing session key, one at a time.
    pub drawing: Option<String>,
    /// Failure backoff per session key.
    failed: HashMap<String, i64>,
    /// Global cooldown after a failure so repeated quota errors don't spin.
    failed_at: Option<i64>,
    /// Active sessions per project (key -> project, scene name) for `taken`.
    projects: HashMap<String, String>,
    dirty: bool,
}

impl Store {
    pub fn open(path: PathBuf) -> Store {
        let file: File = crate::store::read_or_default(&path);
        Store {
            path,
            entries: file.sessions,
            drawing: None,
            failed: HashMap::new(),
            failed_at: None,
            projects: HashMap::new(),
            dirty: false,
        }
    }

    pub fn backdrop_of(&self, s: &Session, enabled: bool) -> Option<SessionBackdrop> {
        if !enabled {
            return None;
        }
        if let Some(e) = self.entries.get(&s.key) {
            return Some(e.bg.clone());
        }
        None
    }

    /// Kept image filenames currently referenced by `backgrounds.json`, so attachment GC leaves
    /// them alone.
    pub fn pictures(&self) -> HashSet<String> {
        self.entries.values().map(|e| e.bg.picture.clone()).collect()
    }

    pub fn can_draw(&self, now: i64) -> bool {
        self.drawing.is_none() && self.failed_at.is_none_or(|at| now - at >= RETRY_MS)
    }

    /// Returns eligible recent sessions missing a background, newest first.
    pub fn look<'a>(&mut self, sessions: impl Iterator<Item = (&'a str, &'a Session)>, now: i64) -> Vec<String> {
        self.projects.clear();
        let mut wanting: Vec<(i64, String)> = Vec::new();
        for (project, s) in sessions {
            if let Some(e) = self.entries.get_mut(&s.key) {
                if e.seen < now - DAY_MS / 2 {
                    e.seen = now;
                    self.dirty = true;
                }
            }
            if !eligible(s) || now - s.last_activity > RECENT_MS {
                continue;
            }
            self.projects.insert(s.key.clone(), project.to_string());
            if self.entries.contains_key(&s.key) {
                continue;
            }
            if self.failed.get(&s.key).is_some_and(|at| now - at < RETRY_MS) {
                continue;
            }
            wanting.push((s.last_activity, s.key.clone()));
        }
        let before = self.entries.len();
        self.entries.retain(|_, e| now - e.seen <= KEEP_MS);
        if self.entries.len() != before {
            self.dirty = true;
        }
        wanting.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        wanting.into_iter().map(|(_, k)| k).collect()
    }

    /// Scene labels already used by active sibling sessions in the same project.
    pub fn taken(&self, key: &str) -> Vec<String> {
        let Some(project) = self.projects.get(key) else { return Vec::new() };
        let mut names: Vec<String> = self
            .projects
            .iter()
            .filter(|(k, p)| *k != key && *p == project)
            .filter_map(|(k, _)| self.entries.get(k).map(|e| e.bg.name.clone()))
            .filter(|n| !n.trim().is_empty())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn drawn(&mut self, key: &str, bg: SessionBackdrop, now: i64) {
        if self.drawing.as_deref() == Some(key) {
            self.drawing = None;
        }
        self.failed.remove(key);
        self.failed_at = None;
        self.entries.insert(key.to_string(), Entry { bg, seen: now });
        self.dirty = true;
    }

    pub fn draw_failed(&mut self, key: &str, now: i64) {
        if self.drawing.as_deref() == Some(key) {
            self.drawing = None;
        }
        self.failed.insert(key.to_string(), now);
        self.failed_at = Some(now);
    }

    /// Drops any kept background and backoff for `key` so a fresh drawing starts immediately.
    pub fn reset(&mut self, key: &str) {
        if self.entries.remove(key).is_some() {
            self.dirty = true;
        }
        self.failed.remove(key);
        self.failed_at = None;
    }

    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let file = File { sessions: self.entries.clone() };
        let _ = crate::store::noted(&self.path, crate::store::save(&self.path, &file));
    }
}

/// Finds the team config for the repo `repo` belongs to on `board`.
pub fn team_for<'a>(board: &'a Board, repo: &str) -> Option<&'a TeamCfg> {
    board
        .team_files
        .iter()
        .find(|t| t.repo.eq_ignore_ascii_case(repo) || t.repo.ends_with(&format!("/{repo}")))
        .and_then(|t| t.settings.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AccountCfg, ProjectCfg};
    use crate::model::{Harness, Helper};

    fn acct(name: &str, harness: &str) -> AccountCfg {
        AccountCfg {
            name: name.into(),
            harness: harness.into(),
            home: format!("~/.{name}"),
            command: None,
            start: None,
            acp: None,
            monthly: None,
            per_token: false,
            usage_cap: None,
            disabled: false,
            chrome_profile: None,
        }
    }

    #[test]
    fn picks_antigravity_or_codex_as_drawing_account() {
        let mut cfg = Config::default();
        cfg.accounts = vec![acct("claude", "claude"), acct("agy", "antigravity"), acct("codex", "codex")];
        // Falls back to first drawing account (antigravity) when none named.
        assert_eq!(drawing_account(&cfg).map(|a| a.name.as_str()), Some("agy"));
        // Explicit backgrounds.account wins.
        cfg.backgrounds.account = "codex".into();
        assert_eq!(drawing_account(&cfg).map(|a| a.name.as_str()), Some("codex"));
        // Disabled account falls through to the other drawer.
        cfg.accounts[2].disabled = true;
        assert_eq!(drawing_account(&cfg).map(|a| a.name.as_str()), Some("agy"));
    }

    #[test]
    fn prompt_precedence_is_project_then_team_then_global_then_default() {
        let mut cfg = Config::default();
        let team = TeamCfg { background_prompt: "Team woodblock style.".into(), ..TeamCfg::default() };
        assert_eq!(effective_prompt(&cfg, None, "App"), DEFAULT_STYLE);

        cfg.backgrounds.prompt = "Global gouache style.".into();
        assert_eq!(effective_prompt(&cfg, None, "App"), "Global gouache style.");
        assert_eq!(effective_prompt(&cfg, Some(&team), "App"), "Team woodblock style.");

        cfg.projects.push(ProjectCfg {
            name: "App".into(),
            repos: Vec::new(),
            folders: Vec::new(),
            chrome_profile: None,
            default: None,
            decisions: None,
            background_prompt: Some("Project botanical style.".into()),
        });
        assert_eq!(effective_prompt(&cfg, Some(&team), "App"), "Project botanical style.");
    }

    #[test]
    fn skips_helpers_and_headless_sessions() {
        let mut normal = Session::new(Harness::Claude, "claude", "s1");
        normal.first_prompt = Some("Build cart checkout".into());
        assert!(eligible(&normal));

        let mut helper = normal.clone();
        helper.helper = Some(Helper::default());
        assert!(!eligible(&helper));

        let mut headless = normal.clone();
        headless.headless = true;
        assert!(!eligible(&headless));
    }

    #[test]
    fn analyzes_dominant_accent_and_center_luminance() {
        // 10x10 solid soft indigo image -> dark-mid luminance, indigo accent.
        let rgba: Vec<u8> = (0..100).flat_map(|_| [60u8, 80, 180, 255]).collect();
        let (accent, lum) = analyze_pixels(&rgba, 10, 10);
        assert_eq!(accent.as_deref(), Some("#3c50b4"));
        assert!(lum < 110);

        // Two-tone image (70% teal + 30% complementary coral) picks teal cleanly rather than
        // averaging complementary hues into muddy grey.
        let two_tone: Vec<u8> = (0..100)
            .flat_map(|i| if i < 70 { [36u8, 150, 140, 255] } else { [220u8, 95, 70, 255] })
            .collect();
        let (accent2, _) = analyze_pixels(&two_tone, 10, 10);
        assert_eq!(accent2.as_deref(), Some("#24968c"));
    }
}

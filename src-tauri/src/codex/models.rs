//! The models a Codex account offers and the one it starts on: the list Codex fetches for the
//! account and keeps in `<CODEX_HOME>/models_cache.json`, which its own `/model` picker shows, and
//! the `model` in its config.toml. See .claude/rules/codex.md.

use std::path::Path;

use serde::Deserialize;

use crate::start::ModelOption;

/// The effort levels `-c model_reasoning_effort=<level>` takes, lowest first, named as Codex
/// describes them (0.162.1's list). Not every model takes every level: GPT-6-Luna has no `ultra`,
/// which is `max` with Codex handing parts of the task to agents of its own.
pub const EFFORTS: [(&str, &str); 6] = [("low", "Low"), ("medium", "Medium"), ("high", "High"), ("xhigh", "Extra high"), ("max", "Max"), ("ultra", "Ultra")];

#[derive(Deserialize)]
struct Cache {
    #[serde(default)]
    models: Vec<Cached>,
}

/// A model as the cache lists it; it also has its context window, tools…
#[derive(Deserialize)]
struct Cached {
    slug: String,
    #[serde(default)]
    display_name: String,
    /// `list` for the ones its picker shows, `hide` for the rest (`codex-auto-review`, the model
    /// its automatic approvals run on).
    #[serde(default)]
    visibility: String,
    /// Where its picker lists it, lowest first.
    #[serde(default)]
    priority: i64,
    #[serde(default)]
    supported_reasoning_levels: Vec<Level>,
}

#[derive(Deserialize)]
struct Level {
    effort: String,
}

fn cached(home: &Path) -> Vec<Cached> {
    let Ok(bytes) = std::fs::read(home.join("models_cache.json")) else { return Vec::new() };
    serde_json::from_slice::<Cache>(&bytes).map(|c| c.models).unwrap_or_default()
}

/// The models Codex's picker offers the account, in its order, by the slug `-m` takes. An
/// account Codex hasn't run in yet has no list, and offers none.
pub fn listed(home: &Path) -> Vec<ModelOption> {
    let mut models: Vec<Cached> = cached(home).into_iter().filter(|m| m.visibility == "list" && crate::start::valid_model(&m.slug)).collect();
    models.sort_by_key(|m| m.priority);
    models.into_iter().map(|m| ModelOption { label: if m.display_name.is_empty() { m.slug.clone() } else { m.display_name }, id: m.slug }).collect()
}

/// Whether the account's list has `model`, those its picker hides included: `-m` takes them too.
pub fn known(home: &Path, model: &str) -> bool {
    cached(home).iter().any(|m| m.slug == model)
}

/// The level `model` is asked for when `effort` is picked: `effort` where the account's list says
/// the model takes it, else the nearest below it that it does (GPT-6-Luna has no `ultra`, so
/// `max`), else the nearest above. Codex's terminal UI took `ultra` for Luna (0.162.1), but
/// codex-acp offers only a model's own levels, and leaves another unset. A model the list
/// doesn't have is asked for what was picked.
pub fn nearest_effort<'a>(home: &Path, model: &str, effort: &'a str) -> &'a str {
    let Some(m) = cached(home).into_iter().find(|m| m.slug == model) else { return effort };
    let takes: Vec<&str> = m.supported_reasoning_levels.iter().map(|l| l.effort.as_str()).collect();
    let Some(at) = EFFORTS.iter().position(|(id, _)| *id == effort) else { return effort };
    if takes.is_empty() || takes.contains(&effort) {
        return effort;
    }
    EFFORTS[..at].iter().rev().chain(&EFFORTS[at + 1..]).map(|(id, _)| *id).find(|id| takes.contains(id)).unwrap_or(effort)
}

/// The model the account's config.toml starts Codex on without `-m`: its `profile`'s, where it
/// names one in `profiles` that sets one, else its own `model`.
pub fn default_model(home: &Path) -> Option<String> {
    let text = std::fs::read_to_string(home.join("config.toml")).ok()?;
    let config: toml::Table = toml::from_str(&text).ok()?;
    let profiled = config.get("profile").and_then(|p| p.as_str()).and_then(|p| config.get("profiles")?.get(p)?.get("model"));
    profiled.or_else(|| config.get("model"))?.as_str().map(str::trim).filter(|m| !m.is_empty()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ca-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// As 0.157.1 wrote it, cut down to the fields read and one it isn't.
    const CACHE: &str = r#"{"fetched_at":"2026-10-16T15:34:39Z","client_version":"0.157.1","models":[
        {"slug":"gpt-6-sol","display_name":"GPT-6-Sol","visibility":"list","priority":3,"context_window":272000},
        {"slug":"codex-auto-review","display_name":"Codex Auto Review","visibility":"hide","priority":43},
        {"slug":"gpt-6-astra","display_name":"GPT-6-Astra","visibility":"list","priority":2},
        {"slug":"gpt 6 bad","display_name":"Unsafe","visibility":"list","priority":1},
        {"slug":"gpt-6-luna","visibility":"list","priority":4,"supported_reasoning_levels":[
            {"effort":"low","description":"Fast responses with lighter reasoning"},{"effort":"medium"},{"effort":"high"},{"effort":"xhigh"},{"effort":"max"}]},
        {"slug":"gpt-6-mini","visibility":"list","priority":5,"supported_reasoning_levels":[{"effort":"medium"},{"effort":"high"}]}
    ]}"#;

    #[test]
    fn asks_a_model_for_the_nearest_level_it_takes() {
        let home = scratch("codex-efforts");
        assert_eq!(nearest_effort(&home, "gpt-6-luna", "ultra"), "ultra", "without the list, what was picked");
        std::fs::write(home.join("models_cache.json"), CACHE).unwrap();
        // Luna has no ultra: the nearest below it.
        assert_eq!(nearest_effort(&home, "gpt-6-luna", "ultra"), "max");
        assert_eq!(nearest_effort(&home, "gpt-6-luna", "high"), "high");
        // Nothing below it: the nearest above.
        assert_eq!(nearest_effort(&home, "gpt-6-mini", "low"), "medium");
        assert_eq!(nearest_effort(&home, "gpt-6-mini", "ultra"), "high");
        // A model the list doesn't have, or one that names no levels, gets what was picked.
        assert_eq!(nearest_effort(&home, "gpt-7", "ultra"), "ultra");
        assert_eq!(nearest_effort(&home, "gpt-6-sol", "ultra"), "ultra");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn lists_the_models_codexs_picker_offers_in_its_order() {
        let home = scratch("codex-models");
        assert!(listed(&home).is_empty(), "an account Codex hasn't run in offers none");
        std::fs::write(home.join("models_cache.json"), CACHE).unwrap();
        let models: Vec<(String, String)> = listed(&home).into_iter().map(|m| (m.id, m.label)).collect();
        // Never a name that could leave its quotes on the command line.
        assert_eq!(models, [("gpt-6-astra".into(), "GPT-6-Astra".into()), ("gpt-6-sol".into(), "GPT-6-Sol".into()), ("gpt-6-luna".into(), "gpt-6-luna".into()), ("gpt-6-mini".into(), "gpt-6-mini".into())]);
        // `-m` takes a hidden one too.
        assert!(known(&home, "codex-auto-review") && known(&home, "gpt-6-sol"));
        assert!(!known(&home, "opus"));
        std::fs::write(home.join("models_cache.json"), "{not json").unwrap();
        assert!(listed(&home).is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn reads_the_model_an_account_starts_on() {
        let home = scratch("codex-default-model");
        assert_eq!(default_model(&home), None);
        std::fs::write(home.join("config.toml"), "model = \"gpt-6-astra\"\nmodel_reasoning_effort = \"xhigh\"\n\n[desktop]\nfollowUpQueueMode = \"queue\"\n").unwrap();
        assert_eq!(default_model(&home).as_deref(), Some("gpt-6-astra"));
        std::fs::write(home.join("config.toml"), "[desktop]\nmodel = \"not-this\"\n").unwrap();
        assert_eq!(default_model(&home), None);
        // The profile it starts with comes first, where it sets one.
        std::fs::write(home.join("config.toml"), "model = \"gpt-6-astra\"\nprofile = \"fast\"\n\n[profiles.fast]\nmodel = \"gpt-6-luna\"\n\n[profiles.deep]\nmodel = \"gpt-6-sol\"\n").unwrap();
        assert_eq!(default_model(&home).as_deref(), Some("gpt-6-luna"));
        std::fs::write(home.join("config.toml"), "model = \"gpt-6-astra\"\nprofile = \"plain\"\n\n[profiles.plain]\napproval_policy = \"never\"\n").unwrap();
        assert_eq!(default_model(&home).as_deref(), Some("gpt-6-astra"));
        let _ = std::fs::remove_dir_all(&home);
    }
}

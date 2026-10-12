//! What a model's tokens would cost at its API prices, for the ledger (ledger.rs): what a
//! subscription's work would have cost, and what an account that pays by the token did cost.
//!
//! The prices are `prices.json`, which scripts/prices.mjs writes from LiteLLM's list (the
//! Anthropic and OpenAI chat models, in dollars per million tokens), with `[prices]` in
//! config.toml over them for a model the list doesn't have yet or has wrong. A model neither
//! has is unpriced: the ledger keeps its tokens and says so, never counting them as free.
//!
//! How a call is priced (Anthropic's pricing and LiteLLM's fields): cache writes kept for an
//! hour cost more than five-minute ones; a prompt over a model's threshold (`above`, Claude
//! Haiku 5.5's 100,000 tokens, OpenAI's 272,000) moves the whole call to the higher prices;
//! Claude's fast mode is a multiple of every price (`fast`), OpenAI's priority tier prices of
//! its own (`priority`); a web search is a price per search.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// One price list, in dollars per million tokens. Every field is optional so that config.toml
/// can give just the ones it changes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Rates {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,
    /// A cache write kept five minutes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,
    /// A cache write kept an hour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_1h: Option<f64>,
}

/// A prompt size above which a model's calls cost more, and what they cost then.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Above {
    pub tokens: u64,
    #[serde(flatten)]
    pub rates: Rates,
    /// What they cost then in OpenAI's priority tier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Rates>,
}

/// A model's prices: `prices.json`'s entry, or `[prices."<model>"]` in config.toml.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Price {
    #[serde(flatten)]
    pub rates: Rates,
    /// Dollars per web search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub above: Option<Above>,
    /// OpenAI's priority tier, which Codex's fast mode runs on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Rates>,
    /// Claude's fast mode: every price times this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fast: Option<f64>,
}

#[derive(Deserialize)]
struct Bundled {
    models: BTreeMap<String, Price>,
}

fn bundled() -> &'static BTreeMap<String, Price> {
    static LIST: OnceLock<BTreeMap<String, Price>> = OnceLock::new();
    LIST.get_or_init(|| serde_json::from_str::<Bundled>(include_str!("prices.json")).map(|b| b.models).unwrap_or_default())
}

/// What one call used, as far as its price goes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Used {
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub cache_write_1h: u64,
    pub output: u64,
    pub searches: u64,
    /// The biggest prompt among the calls summed here, against the model's `above`.
    pub prompt: u64,
    pub fast: bool,
}

/// The names a model is looked up by, in order: as written, without a context size in
/// brackets (`fable[1m]`), a provider before a slash (`anthropic/…`), a version after an `@`
/// (Vertex's) or a date at the end (`-20251001`).
fn names(model: &str) -> Vec<String> {
    let mut out = vec![model.to_string()];
    let mut m = model.split('[').next().unwrap_or(model).trim().to_string();
    if let Some((_, rest)) = m.rsplit_once('/') {
        m = rest.to_string();
    }
    if let Some((base, _)) = m.split_once('@') {
        m = base.to_string();
    }
    out.push(m.clone());
    if let Some((base, date)) = m.rsplit_once('-') {
        if date.len() == 8 && date.chars().all(|c| c.is_ascii_digit()) {
            out.push(base.to_string());
        }
    }
    out.dedup();
    out
}

/// Every prompt size some model's prices change above, the bundled list's and config.toml's,
/// smallest first: a report sums calls by which of these their prompts passed (ledger.rs).
pub fn thresholds(overrides: &BTreeMap<String, Price>) -> Vec<u64> {
    let mut out: Vec<u64> = bundled().values().chain(overrides.values()).filter_map(|p| p.above.as_ref().map(|a| a.tokens)).filter(|t| *t > 0).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// A model's prices: config.toml's, over the bundled list's where it names the same model.
pub fn price_of(model: &str, overrides: &BTreeMap<String, Price>) -> Option<Price> {
    for name in names(model) {
        let base = bundled().get(&name);
        let mine = overrides.get(&name);
        match (base, mine) {
            (None, None) => continue,
            (Some(b), None) => return Some(b.clone()),
            (base, Some(m)) => return Some(over(base.cloned().unwrap_or_default(), m)),
        }
    }
    None
}

fn over(base: Price, mine: &Price) -> Price {
    let rates = |b: &Rates, m: &Rates| Rates {
        input: m.input.or(b.input),
        output: m.output.or(b.output),
        cache_read: m.cache_read.or(b.cache_read),
        cache_write: m.cache_write.or(b.cache_write),
        cache_write_1h: m.cache_write_1h.or(b.cache_write_1h),
    };
    Price {
        rates: rates(&base.rates, &mine.rates),
        search: mine.search.or(base.search),
        above: mine.above.clone().or(base.above),
        priority: mine.priority.clone().or(base.priority),
        fast: mine.fast.or(base.fast),
    }
}

/// What `used` costs at `price`, in dollars, or none when the price lacks input or output.
/// A cache price it lacks is taken as the input price, which errs on the high side; an hour's
/// cache write it lacks, as a five-minute one.
pub fn cost(price: &Price, used: &Used) -> Option<f64> {
    let with = |base: &Rates, top: &Rates| over(Price { rates: base.clone(), ..Default::default() }, &Price { rates: top.clone(), ..Default::default() }).rates;
    let long = price.above.as_ref().filter(|a| a.tokens > 0 && used.prompt > a.tokens);
    let mut r = match long {
        Some(a) => with(&price.rates, &a.rates),
        None => price.rates.clone(),
    };
    let mut times = 1.0;
    if used.fast {
        let fast = match long {
            Some(a) => a.priority.as_ref(),
            None => price.priority.as_ref(),
        };
        match (fast, &price.priority, price.fast) {
            (Some(p), _, _) => r = with(&r, p),
            // A long prompt where only a short one's priority prices are listed: the long prices,
            // as older models bill a long prompt in priority (GPT-5.4).
            (None, Some(_), _) => {}
            (None, None, Some(f)) => times = f,
            (None, None, None) => {}
        }
    }
    let input = r.input?;
    let output = r.output?;
    let cache_read = r.cache_read.unwrap_or(input);
    let cache_write = r.cache_write.unwrap_or(input);
    let cache_write_1h = r.cache_write_1h.unwrap_or(cache_write);
    let per_million = used.input as f64 * input
        + used.cache_read as f64 * cache_read
        + used.cache_write as f64 * cache_write
        + used.cache_write_1h as f64 * cache_write_1h
        + used.output as f64 * output;
    Some(per_million / 1e6 * times + used.searches as f64 * price.search.unwrap_or(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn used(input: u64, cache_read: u64, cache_write: u64, cache_write_1h: u64, output: u64) -> Used {
        Used { input, cache_read, cache_write, cache_write_1h, output, prompt: input + cache_read + cache_write + cache_write_1h, ..Default::default() }
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn the_bundled_list_reads_and_has_the_models_sessions_use() {
        for m in ["claude-opus-5-5", "claude-fable-5-1", "claude-sonnet-5-5", "claude-haiku-5-5", "gpt-6-astra"] {
            assert!(bundled().contains_key(m), "{m} isn't in prices.json");
        }
    }

    #[test]
    fn claude_prices_each_kind_of_token_and_an_hours_cache_write_above_a_five_minute_one() {
        // Claude Opus 5.5: $4 in, $20 out, $0.20 cache reads, $5 and $8 cache writes.
        let p = price_of("claude-opus-5-5", &BTreeMap::new()).unwrap();
        let c = cost(&p, &used(1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000)).unwrap();
        assert!(near(c, 4.0 + 0.2 + 5.0 + 8.0 + 20.0), "{c}");
    }

    #[test]
    fn agrees_with_what_claude_code_says_its_sessions_cost() {
        // Two sessions' `lastModelUsage` in Claude Code 2.1.296's .claude.json, cache writes all
        // kept an hour as its sessions write them, and the `costUSD` it worked out.
        let opus = price_of("claude-opus-5-5", &BTreeMap::new()).unwrap();
        let c = cost(&opus, &used(3200, 141_688_446, 0, 632_601, 303_531)).unwrap();
        assert!((c - 39.4819172).abs() < 1e-6, "{c}");
        let fable = price_of("claude-fable-5-1", &BTreeMap::new()).unwrap();
        let c = cost(&fable, &used(8346, 52_442_476, 0, 2_186_741, 184_039)).unwrap();
        assert!((c - 66.130849).abs() < 1e-6, "{c}");
    }

    #[test]
    fn fast_mode_multiplies_every_claude_price() {
        let p = price_of("claude-opus-5-5", &BTreeMap::new()).unwrap();
        let fast = cost(&p, &Used { fast: true, ..used(1_000_000, 0, 0, 0, 1_000_000) }).unwrap();
        assert!(near(fast, 2.0 * 24.0), "{fast}");
    }

    #[test]
    fn a_prompt_over_the_threshold_moves_the_whole_call_to_the_higher_prices() {
        let p = price_of("claude-haiku-5-5", &BTreeMap::new()).unwrap();
        let small = cost(&p, &used(100_000, 0, 0, 0, 0)).unwrap();
        let big = cost(&p, &used(100_001, 0, 0, 0, 0)).unwrap();
        assert!(near(small, 0.01), "{small}");
        assert!(near(big, 100_001.0 * 0.5 / 1e6), "{big}");
    }

    #[test]
    fn openai_fast_mode_is_its_priority_tier() {
        let p = price_of("gpt-6-astra", &BTreeMap::new()).unwrap();
        let fast = cost(&p, &Used { fast: true, ..used(0, 0, 0, 0, 1_000_000) }).unwrap();
        assert!(near(fast, p.priority.as_ref().unwrap().output.unwrap()), "{fast}");
    }

    #[test]
    fn a_long_prompt_in_fast_mode_has_its_own_prices() {
        // GPT-5.6 Sol over 272,000 tokens in priority: $16 in, $60 out a million (OpenAI's
        // fast-mode pricing).
        let p = price_of("gpt-5.6-sol", &BTreeMap::new()).unwrap();
        let c = cost(&p, &Used { fast: true, ..used(300_000, 0, 0, 0, 1_000) }).unwrap();
        assert!(near(c, 0.3 * 16.0 + 0.001 * 60.0), "{c}");
        // Without long-prompt priority prices listed, the long prompt's own prices.
        let mut short_only = p.clone();
        short_only.above.as_mut().unwrap().priority = None;
        let c = cost(&short_only, &Used { fast: true, ..used(300_000, 0, 0, 0, 0) }).unwrap();
        assert!(near(c, 0.3 * 8.0), "{c}");
    }

    #[test]
    fn thresholds_take_in_config_tomls() {
        let mut mine = BTreeMap::new();
        mine.insert("x".to_string(), Price { above: Some(Above { tokens: 64_000, ..Default::default() }), ..Default::default() });
        let t = thresholds(&mine);
        assert!(t.contains(&64_000) && t.contains(&272_000) && t.contains(&100_000), "{t:?}");
        assert!(t.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn names_are_found_without_a_context_size_a_provider_or_a_date() {
        let none = BTreeMap::new();
        assert!(price_of("claude-opus-5-5[1m]", &none).is_some());
        assert!(price_of("anthropic/claude-opus-5-5", &none).is_some());
        assert_eq!(price_of("claude-haiku-4-5-20991231", &none), price_of("claude-haiku-4-5", &none));
        assert!(price_of("gpt-0-unknown", &none).is_none());
    }

    #[test]
    fn config_prices_fill_in_a_model_and_change_one_field_of_another() {
        let mut mine = BTreeMap::new();
        mine.insert("my-own-model".to_string(), Price { rates: Rates { input: Some(1.0), output: Some(2.0), ..Default::default() }, ..Default::default() });
        mine.insert("claude-opus-5-5".to_string(), Price { rates: Rates { output: Some(30.0), ..Default::default() }, ..Default::default() });
        let review = price_of("my-own-model", &mine).unwrap();
        // No cache price given: cache reads cost what input does.
        assert!(near(cost(&review, &used(0, 1_000_000, 0, 0, 1_000_000)).unwrap(), 3.0));
        let opus = price_of("claude-opus-5-5", &mine).unwrap();
        assert_eq!((opus.rates.input, opus.rates.output), (Some(4.0), Some(30.0)));
    }
}

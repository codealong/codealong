// Writes src-tauri/src/prices.json, the API prices Codealong works out what sessions would have
// cost from (src-tauri/src/prices.rs), from LiteLLM's list of model prices: the Anthropic, OpenAI
// and Google Gemini chat models under their own names, in dollars per million tokens. Run it when
// a new model comes out or prices change; `[prices]` in config.toml covers a model in the meantime.
//
//   node scripts/prices.mjs [litellm.json]
//
// Without a file it downloads the list. Claude's fast mode is a multiple LiteLLM gives as
// `provider_specific_entry.fast`; FAST has the ones its list lacks, from Anthropic's pricing (6x
// on Opus 4.6). FIXES is for prices LiteLLM has wrong against the provider's own pricing page
// (empty as of 2026-11-28: Sonnet 5.5's $0.10 cache read matches Anthropic's 0.05x rate).
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const SOURCE = 'https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json';
const PROVIDERS = new Set(['anthropic', 'openai', 'gemini']);
const FAST = { 'claude-opus-4-6': 6 };
const FIXES = {};
// Models a harness records under a name of its own: Codex's automatic approval reviews name the
// server's alias, which has run GPT-5.6 Luna since 2026-07-30 (as ccusage's Codex notes have it).
const ALIASES = { 'codex-auto-review': 'gpt-5.6-luna' };

/** Dollars per token as dollars per million, without float noise. */
const perMillion = (v) => (typeof v === 'number' ? Math.round(v * 1e6 * 1e6) / 1e6 : undefined);

/** The fields of one price, from LiteLLM's names with `suffix` (`_above_272k_tokens`, `_priority`). */
function price(m, suffix = '') {
  const p = {
    input: perMillion(m[`input_cost_per_token${suffix}`]),
    output: perMillion(m[`output_cost_per_token${suffix}`]),
    cache_read: perMillion(m[`cache_read_input_token_cost${suffix}`]),
    cache_write: perMillion(m[`cache_creation_input_token_cost${suffix}`]),
    cache_write_1h: perMillion(m[suffix ? `cache_creation_input_token_cost_above_1hr${suffix}` : 'cache_creation_input_token_cost_above_1hr']),
  };
  return Object.fromEntries(Object.entries(p).filter(([, v]) => v !== undefined));
}

/** One model's entry: its prices, the prompt size above which they change, priority and fast. */
export function entry(name, m) {
  const out = price(m);
  if (out.input === undefined || out.output === undefined || (out.input === 0 && out.output === 0)) return null;
  const search = m.search_context_cost_per_query?.search_context_size_medium;
  if (typeof search === 'number') out.search = search;
  const tiers = Object.keys(m)
    .map((k) => /^input_cost_per_token_above_(\d+)k_tokens$/.exec(k))
    .filter(Boolean)
    .map((x) => Number(x[1]))
    .sort((a, b) => a - b);
  if (tiers.length) {
    out.above = { tokens: tiers[0] * 1000, ...price(m, `_above_${tiers[0]}k_tokens`) };
    // A long prompt in fast mode (OpenAI's priority tier) has prices of its own.
    const fast = price(m, `_above_${tiers[0]}k_tokens_priority`);
    if (fast.input !== undefined) out.above.priority = fast;
  }
  if (typeof m.input_cost_per_token_priority === 'number') out.priority = price(m, '_priority');
  // Its context window, what a session's context meter fills.
  if (typeof m.max_input_tokens === 'number') out.context = m.max_input_tokens;
  const fast = m.provider_specific_entry?.fast ?? FAST[name];
  if (typeof fast === 'number' && fast > 1) out.fast = fast;
  return { ...out, ...FIXES[name] };
}

export function prices(list, asOf) {
  const models = {};
  for (const [raw, m] of Object.entries(list).sort(([a], [b]) => a.localeCompare(b))) {
    if (!PROVIDERS.has(m.litellm_provider)) continue;
    // LiteLLM keys Google Gemini's API models as `gemini/<model>` (and a few bare aliases); keep
    // them under their bare names, preferring a bare entry over a prefixed one when both exist.
    const name = m.litellm_provider === 'gemini' ? raw.replace(/^gemini\//, '') : raw;
    if (name.includes('/') || name.startsWith('ft:')) continue;
    if (m.mode !== 'chat' && m.mode !== 'responses') continue;
    const e = entry(name, m);
    if (e && !(name in models)) models[name] = e;
  }
  const sorted = Object.fromEntries(Object.entries(models).sort(([a], [b]) => a.localeCompare(b)));
  for (const [alias, name] of Object.entries(ALIASES)) if (sorted[name]) sorted[alias] = sorted[name];
  return { source: SOURCE, as_of: asOf, per: 'dollars per million tokens; search is dollars per web search', models: sorted };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const file = process.argv[2];
  const list = file ? JSON.parse(readFileSync(file, 'utf8')) : await (await fetch(SOURCE)).json();
  const out = prices(list, new Date().toISOString().slice(0, 10));
  // A model to a line, so a change in prices reads as one in a diff.
  const lines = Object.entries(out.models).map(([k, v]) => `  ${JSON.stringify(k)}: ${JSON.stringify(v)}`);
  const head = ['source', 'as_of', 'per'].map((k) => ` ${JSON.stringify(k)}: ${JSON.stringify(out[k])},`);
  writeFileSync(resolve(root, 'src-tauri/src/prices.json'), `{\n${head.join('\n')}\n "models": {\n${lines.join(',\n')}\n }\n}\n`);
  console.log(`${Object.keys(out.models).length} models written to src-tauri/src/prices.json`);
}

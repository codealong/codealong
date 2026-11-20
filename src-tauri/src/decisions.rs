//! Decision models: models that answer narrow typed questions about a state with a probability
//! for every option (which session a prompt is for, whether a reply asks you something), quickly
//! and for a fraction of a cent. TypeSafe's Jev was the first, and its request is the one
//! Codealong writes (jev.rs: `body`, the answers' readers, `Images`); each provider has an adapter
//! here that sends it in the provider's own shape and reads the answer back into Jev's
//! (`Adapter`, `adapter`), as each harness has one (harness.rs).
//!
//! Which model a request goes to says where its text goes, not which answers better: a project's
//! text goes to its `decisions`, else to `[decisions] default` (`for_project`); a session's to its
//! boundary group's model first (boundary.rs), and to none when its group and its project name
//! different ones (`for_session`). A request that carries text of several (the quick panel's, a
//! todo's) is asked of its subject's model, and anything whose text may not go there goes in by
//! its title and where it works, as `none` sends it (`Decider::carries`).
//!
//! A fallback is tried only when a model fails for now (rate limited, overloaded, unreachable),
//! and only for text that goes by `[decisions] default`: what a project or boundary group chose
//! goes to that model or to none (`allowed`).
//!
//! Each provider's API key lives in the login keychain (os/secrets.rs) under a service of its
//! own, never in config.toml or on a command line: curl gets it and the request on stdin, and
//! saving it hands it over on stdin too. It's read from the keychain once and kept in memory for
//! a while, since the engine asks often. The provider's usual environment variable works when
//! the keychain has none, for the command line. A Codex or ChatGPT sign-in isn't an API key: it's
//! never read.

mod gemini;
mod openai;
mod typesafe;

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use crate::config::{Config, DecisionModelCfg};
use crate::jev::Feature;

/// The providers config.toml's `provider` takes, as their adapters name them.
pub const PROVIDERS: [&str; 3] = ["typesafe", "openai", "gemini"];

/// How long a key read from the keychain is used before it's read again, so a key replaced
/// from another place (the `security` command) is picked up.
const KEY_FOR: Duration = Duration::from_secs(10 * 60);

/// What a provider takes and answers, and what it costs.
pub trait Adapter: Sync {
    /// Its name in config.toml (`PROVIDERS`).
    fn id(&self) -> &'static str;
    /// Its name in words, for errors and Settings.
    fn word(&self) -> &'static str;
    /// The login keychain's service for its key.
    fn keychain_service(&self) -> &'static str;
    /// The environment variable read for a key when the keychain has none.
    fn key_env(&self) -> &'static str;
    fn endpoint(&self) -> &'static str;
    /// The headers that carry the key, and any other a request needs (curl config lines' values):
    /// one with `images` may need its own.
    fn headers(&self, key: &str, images: bool) -> Vec<String>;
    /// The provider's request for `body`, in Jev's shape with its images as assets (jev.rs
    /// `Images`), on `model`.
    fn request(&self, model: &str, body: &Value) -> Value;
    /// A 200's reply in Jev's shape: `model`, `answers` by question id as Jev gives them, `usage`
    /// with `input_tokens` and `output_tokens` (and `cached_tokens` and `cache_write_tokens`
    /// where the provider caches). An answer the provider refused is left out, as one Jev didn't
    /// give.
    fn answer(&self, model: &str, reply: Value) -> Result<Value>;
    /// Dollars a request that used these tokens costs.
    fn cost(&self, used: &Tokens) -> f64;
}

/// A request's tokens, as the ledger keeps them (`ledger::JevUse`). Cached and cache-write
/// tokens are counted in `input` too.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Tokens {
    pub input: u64,
    pub cached: u64,
    pub cache_write: u64,
    pub output: u64,
}

static ADAPTERS: [&dyn Adapter; 3] = [&typesafe::TypeSafe, &openai::OpenAi, &gemini::Gemini];

/// The adapter for a provider as config.toml names it; None for one Codealong doesn't know,
/// which `config::check_decisions` keeps out of a saved file.
pub fn adapter(provider: &str) -> Option<&'static dyn Adapter> {
    ADAPTERS.iter().copied().find(|a| a.id() == provider.trim())
}

/* ---- Which model ---- */

/// The decision model a request goes to, then the ones it may go on to while the one before fails
/// for now, in order. Never empty.
#[derive(Debug, Clone, PartialEq)]
pub struct Decider {
    pub models: Vec<DecisionModelCfg>,
}

impl Decider {
    /// The provider's model it asks first, which a request's body names.
    pub fn model(&self) -> &str {
        &self.models[0].model
    }

    /// Whether text that may go to `readers` (`for_project`, `for_session`) may go into a request
    /// asked of this one: every model this one may try may read it.
    pub fn carries(&self, readers: Option<&Decider>) -> bool {
        readers.is_some_and(|r| self.models.iter().all(|m| r.models.iter().any(|x| x.name == m.name)))
    }
}

/// Every decision model config.toml names: its `[[decisions.models]]`, and Jev as `jev` on
/// `jev-preview` when none of them is named so.
pub fn models(cfg: &Config) -> Vec<DecisionModelCfg> {
    let mut out = cfg.decisions.models.clone();
    if !out.iter().any(|m| m.name.trim() == "jev") {
        out.insert(0, DecisionModelCfg { name: "jev".into(), provider: "typesafe".into(), model: crate::config::default_jev_model(), fallback: None });
    }
    out
}

/// The decision model config.toml names `name`.
pub fn model_named(cfg: &Config, name: &str) -> Option<DecisionModelCfg> {
    models(cfg).into_iter().find(|m| m.name.trim() == name.trim())
}

/// What text the model `name` was chosen for may go to: that model alone when a project or a
/// boundary group chose it (`chosen`), and when it's `[decisions] default`, its fallbacks after
/// it. None for `none`, or a name config.toml has no model for.
fn allowed(cfg: &Config, name: &str, chosen: bool) -> Option<Decider> {
    let first = model_named(cfg, name)?;
    let mut models = vec![first];
    while !chosen {
        let Some(next) = models.last().and_then(|m| m.fallback.as_deref()).and_then(|f| model_named(cfg, f)) else { break };
        // `config::check_decisions` turns a loop down; one written by hand stops where it turns.
        if models.iter().any(|m| m.name == next.name) {
            break;
        }
        models.push(next);
    }
    Some(Decider { models })
}

/// The `decisions` of the project named `project`, as config.toml has it.
fn project_says<'a>(cfg: &'a Config, project: Option<&str>) -> Option<&'a str> {
    let p = project.map(str::trim)?;
    cfg.projects.iter().find(|x| x.name.trim() == p)?.decisions.as_deref().map(str::trim).filter(|d| !d.is_empty())
}

/// The decision model that may read what's said in `project` (none: in no project of yours):
/// the project's `decisions`, else `[decisions] default` with its fallbacks. None when that's
/// `none`.
pub fn for_project(cfg: &Config, project: Option<&str>) -> Option<Decider> {
    match project_says(cfg, project) {
        Some(d) => allowed(cfg, d, true),
        None => allowed(cfg, cfg.decisions.default.trim(), false),
    }
}

/// The decision model that may read what a session on `account` in `project` says: its boundary
/// group's (Jev's when it names none) before anything else, else its project's
/// (`for_project`). None when that's `none`, and when the group and the project each name a
/// model and they differ: its text goes only where both let it.
pub fn for_session(cfg: &Config, account: &str, project: Option<&str>) -> Option<Decider> {
    match crate::boundary::decisions(cfg, account) {
        Some(group) => {
            if project_says(cfg, project).is_some_and(|p| p != group) {
                return None;
            }
            allowed(cfg, group, true)
        }
        None => for_project(cfg, project),
    }
}

/// `for_session` for a session whose project isn't known (no longer on the board): what every
/// project would give it, when they all agree. None when they don't, as its text might be a
/// project's that keeps it from the model the others give.
pub fn for_session_anywhere(cfg: &Config, account: &str) -> Option<Decider> {
    let first = for_session(cfg, account, None);
    cfg.projects.iter().all(|p| for_session(cfg, account, Some(&p.name)) == first).then_some(first).flatten()
}

/// The project a session's text belongs to: an orchestrator's is the one it keeps watch over
/// (its own session is in Codealong's chats), anyone else's its worktree's.
pub fn project_of<'a>(lane: &'a crate::model::Lane, s: &'a crate::model::Session) -> Option<&'a str> {
    match &s.orchestrator {
        Some(o) => o.project.as_deref(),
        None => Some(lane.project.as_str()),
    }
}

/// `for_session` for a session on the board.
pub fn of_session(cfg: &Config, lane: &crate::model::Lane, s: &crate::model::Session) -> Option<Decider> {
    for_session(cfg, &s.account, project_of(lane, s))
}

/// The decision model `spec` names, alone, for trying one by hand (`--snapshot --decider`): one
/// of config.toml's by its name, or any provider's model as `<provider>:<model>`
/// (`openai:gpt-6-luna`), which needn't be in config.toml.
pub fn named(cfg: &Config, spec: &str) -> Result<Decider> {
    let spec = spec.trim();
    if let Some(m) = model_named(cfg, spec) {
        return Ok(Decider { models: vec![m] });
    }
    match spec.split_once(':') {
        Some((provider, model)) if adapter(provider).is_some() && !model.trim().is_empty() => {
            Ok(Decider { models: vec![DecisionModelCfg { name: spec.to_string(), provider: provider.trim().to_string(), model: model.trim().to_string(), fallback: None }] })
        }
        _ => bail!("{spec:?} is neither one of the [[decisions.models]] nor <provider>:<model>, with the provider one of {}.", PROVIDERS.join(", ")),
    }
}

/// `cfg` with `decider`'s model in the place of every project's choice and the default's, for
/// trying it on real sessions by hand (`--snapshot --decider`): what you asked for goes to it,
/// every project's text but what a boundary group keeps to its own model, which still binds.
pub fn trying(cfg: &Config, decider: &Decider) -> Config {
    let mut out = cfg.clone();
    let m = decider.models[0].clone();
    out.decisions.models.retain(|x| x.name.trim() != m.name);
    out.decisions.models.push(DecisionModelCfg { fallback: None, ..m.clone() });
    out.decisions.default = m.name.clone();
    for p in &mut out.projects {
        p.decisions = Some(m.name.clone());
    }
    out
}

/* ---- Asking ---- */

/// Why a request got no answer for now: rate limited, overloaded, unreachable. A fallback is
/// tried after it, and the worker waits a moment.
#[derive(Debug)]
struct Busy(String);

impl std::fmt::Display for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Busy {}

/// Whether an error is one a provider gave for now (`Busy`).
pub fn is_busy(e: &anyhow::Error) -> bool {
    e.downcast_ref::<Busy>().is_some()
}

/// Asks `decider` for a feature: its first model, then each it may go on to while the one before
/// fails for now. The answer is in Jev's shape (`Adapter::answer`), whoever gave it, with the
/// `provider` and decision model's `decider` beside its `model`; its tokens go in the ledger by
/// the feature that asked (`ledger::note_jev`). A request a provider refused says no usage and
/// isn't kept.
pub fn ask(feature: Feature, decider: &Decider, body: &Value) -> Result<Value> {
    first_answer(decider, |m| ask_one(feature, m, body))
}

/// The first answer `ask` gets of the decider's models in order, going on only past one that
/// failed for now; the last such failure when every one did.
fn first_answer(decider: &Decider, ask: impl Fn(&DecisionModelCfg) -> Result<Value>) -> Result<Value> {
    let mut busy = None;
    for m in &decider.models {
        match ask(m) {
            Err(e) if is_busy(&e) => busy = Some(e),
            done => return done,
        }
    }
    Err(busy.unwrap_or_else(|| anyhow!("There's no decision model to ask.")))
}

fn ask_one(feature: Feature, m: &DecisionModelCfg, body: &Value) -> Result<Value> {
    let a = adapter(&m.provider).ok_or_else(|| anyhow!("Decision model {} names a provider Codealong doesn't know, {:?}.", m.name, m.provider))?;
    let key = api_key(a).ok_or_else(|| anyhow!("There's no {} API key yet. Add it in Settings › Decisions.", a.word()))?;
    let images = crate::jev::has_images(body);
    let (mut status, mut reply) = post(a, &key, &a.request(&m.model, body), images)?;
    // Images fail a whole request a provider won't take them in (400, or 413 for size): asked
    // again without them, the question is answered from the words.
    if images && matches!(status.as_str(), "400" | "413") {
        let plain = crate::jev::without_images(body);
        (status, reply) = post(a, &key, &a.request(&m.model, &plain), false)?;
    }
    let mut response = reply_of(a, &m.model, &status, &reply)?;
    response["provider"] = json!(a.id());
    response["decider"] = json!(m.name);
    crate::ledger::note_jev(used(feature, a, &m.model, &response, crate::util::now_ms()));
    Ok(response)
}

/// What an answer says its request used, for the ledger. The model is the one that answered, or
/// the one asked for when the reply doesn't say.
fn used(feature: Feature, a: &dyn Adapter, model: &str, response: &Value, at: i64) -> crate::ledger::JevUse {
    let usage = &response["usage"];
    let n = |k: &str| usage[k].as_u64().unwrap_or(0);
    crate::ledger::JevUse {
        at,
        feature: feature.name().to_string(),
        provider: a.id().to_string(),
        model: response["model"].as_str().filter(|m| !m.is_empty()).unwrap_or(model).to_string(),
        tokens: Tokens { input: n("input_tokens"), cached: n("cached_tokens"), cache_write: n("cache_write_tokens"), output: n("output_tokens") },
    }
}

/// What a request with `tokens` costs at a provider's prices; nothing for a provider Codealong
/// doesn't know.
pub fn cost(provider: &str, tokens: &Tokens) -> f64 {
    adapter(provider).map_or(0.0, |a| a.cost(tokens))
}

/// One small call, for Settings' Test button: proves the provider's key and the model, and says
/// how fast.
pub fn check(provider: &str, model: &str) -> Result<String> {
    let a = adapter(provider).ok_or_else(|| anyhow!("Codealong doesn't know the provider {provider:?}."))?;
    let m = DecisionModelCfg { name: format!("{}:{}", a.id(), model.trim()), provider: a.id().into(), model: model.trim().into(), fallback: None };
    let started = Instant::now();
    let body = crate::jev::body(model, json!("Fix the flaky login test"), json!({ "task": { "type": "noul", "instructions": "Does this ask for a change to code?" } }));
    let v = ask(Feature::Test, &Decider { models: vec![m] }, &body)?;
    Ok(format!("{} answered in {} ms.", v["model"].as_str().unwrap_or(model), started.elapsed().as_millis()))
}

/* ---- In the background ---- */

/// A request for the worker, and what its answer is for: the id comes back with it. `fill`
/// finishes the body on the worker's thread just before it's sent, for state that takes a slow
/// read (a harness's pending call, over HTTP for OpenCode): the engine's thread never waits on it.
pub struct Job {
    pub id: String,
    pub feature: Feature,
    pub decider: Decider,
    pub body: Value,
    pub fill: Option<Box<dyn FnOnce(&mut Value) + Send>>,
}

/// A thread that asks one request at a time and hands each answer to `done`. The engine sends
/// it jobs and hears back through its own channel, so a slow answer never holds up a board.
pub fn worker(done: impl Fn(String, std::result::Result<Value, String>) + Send + 'static) -> Sender<Job> {
    let (tx, rx) = channel::<Job>();
    std::thread::Builder::new()
        .name("decisions".into())
        .spawn(move || {
            for mut job in rx {
                if let Some(fill) = job.fill.take() {
                    fill(&mut job.body);
                }
                let answer = ask(job.feature, &job.decider, &job.body);
                // Rate limited or overloaded everywhere it may go: wait a moment before the next.
                if answer.as_ref().err().is_some_and(is_busy) {
                    std::thread::sleep(Duration::from_secs(5));
                }
                done(job.id, answer.map_err(|e| format!("{e:#}")));
            }
        })
        .expect("spawning the decisions thread");
    tx
}

/* ---- The request ---- */

/// Escapes a value for a double-quoted string in a curl config file.
fn curl_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The curl config for one request: the key (in a header) and the body both go on curl's stdin.
fn curl_config(a: &dyn Adapter, key: &str, request: &Value, images: bool) -> String {
    let mut out = format!("url = {}\nrequest = \"POST\"\n", curl_quoted(a.endpoint()));
    for h in a.headers(key, images) {
        out.push_str(&format!("header = {}\n", curl_quoted(&h)));
    }
    out.push_str(&format!(
        "header = \"Content-Type: application/json\"\nheader = \"Accept: application/json\"\nheader = \"User-Agent: codealong/{}\"\ndata-binary = {}\n",
        env!("CARGO_PKG_VERSION"),
        curl_quoted(&request.to_string())
    ));
    out
}

/// Sends a request: the HTTP status and the reply. Images take longer to send, so they get
/// longer.
fn post(a: &dyn Adapter, key: &str, request: &Value, images: bool) -> Result<(String, String)> {
    if key.chars().any(|c| c.is_control() || c == '"' || c == '\\') {
        bail!("The {} API key has characters a key can't have. Paste it again in Settings › Decisions.", a.word());
    }
    let mut child = Command::new(crate::http::curl())
        // `-q`, first, as http.rs has it: the key goes to curl as its config, and a `trace` in
        // ~/.curlrc would write it to a file.
        .args(["-q", "-sS", "-m", if images { "40" } else { "15" }, "-w", "\n%{http_code}", "-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("curl: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(curl_config(a, key, request, images).as_bytes())?;
    }
    let out = child.wait_with_output().map_err(|e| anyhow!("curl: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (reply, status) = text.rsplit_once('\n').unwrap_or(("", &text));
    if matches!(status.trim(), "000" | "") {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(Busy(format!("Couldn't reach {}: {}", a.word(), err.trim().trim_start_matches("curl: ").chars().take(160).collect::<String>())).into());
    }
    Ok((status.trim().to_string(), reply.to_string()))
}

/// What a reply says: the answer in Jev's shape, or why there's none. Every provider's error
/// body names its message as `detail` (TypeSafe's, `{error_type, message}` on a 400) or `error`
/// (`{message, …}`).
fn reply_of(a: &dyn Adapter, model: &str, status: &str, reply: &str) -> Result<Value> {
    let who = a.word();
    let detail = || -> String {
        serde_json::from_str::<Value>(reply)
            .ok()
            .and_then(|v| v.get("detail").or_else(|| v.get("error")).or_else(|| v.get("message")).cloned())
            .map(|d| match d.get("message").unwrap_or(&d) {
                Value::String(s) => s.clone(),
                d => d.to_string(),
            })
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect()
    };
    match status {
        "200" => {
            let v: Value = serde_json::from_str(reply).map_err(|_| anyhow!("{who} sent an answer Codealong can't read."))?;
            a.answer(model, v)
        }
        "401" | "403" => {
            forget_key(a);
            bail!("{who} didn't accept the API key. Check it in Settings › Decisions.")
        }
        "400" | "404" | "422" => bail!("{who} couldn't take the request: {}", detail()),
        "413" => bail!("{who} couldn't take a request that big."),
        "429" => Err(Busy(format!("{who} is rate limiting; try again in a moment. {}", detail()).trim_end().to_string()).into()),
        s if s.starts_with('5') => Err(Busy(format!("{who} is overloaded (HTTP {s}); try again in a moment.")).into()),
        other => bail!("{who} answered HTTP {other}. {}", detail()),
    }
}

/* ---- Images, for the providers that take them as parts ---- */

/// A piece of what goes to a model with its images in place: text, or an image (its format,
/// `png` or `jpg`, and its bytes in base64).
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String),
    Image { format: String, data: String },
}

/// `text` cut where it names one of `assets` (jev.rs `Images`), each name an image in its
/// place. Text with none of them is one piece.
fn pieces(text: &str, assets: &Map<String, Value>) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some((at, key)) = assets.keys().filter_map(|k| rest.find(k.as_str()).map(|i| (i, k))).min_by_key(|(i, _)| *i) {
        if at > 0 {
            out.push(Piece::Text(rest[..at].to_string()));
        }
        let a = &assets[key];
        out.push(Piece::Image { format: a["format"].as_str().unwrap_or("png").to_string(), data: a["data_base64"].as_str().unwrap_or_default().to_string() });
        rest = &rest[at + key.len()..];
    }
    if !rest.is_empty() || out.is_empty() {
        out.push(Piece::Text(rest.to_string()));
    }
    out
}

/// An image's media type by the format Jev's asset names.
fn mime(format: &str) -> &'static str {
    if format == "jpg" {
        "image/jpeg"
    } else {
        "image/png"
    }
}

/// An option's description as text: a provider other than TypeSafe takes a string, and a card
/// (an object) goes as its JSON.
fn described(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/* ---- The keys ---- */

static KEYS: Mutex<Vec<(&'static str, Instant, String)>> = Mutex::new(Vec::new());

fn forget_key(a: &dyn Adapter) {
    if let Ok(mut k) = KEYS.lock() {
        k.retain(|(p, _, _)| *p != a.id());
    }
}

/// A provider's API key: the keychain's, else its environment variable.
fn api_key(a: &dyn Adapter) -> Option<String> {
    if let Some(key) = KEYS.lock().ok().and_then(|k| k.iter().find(|(p, at, _)| *p == a.id() && at.elapsed() < KEY_FOR).map(|k| k.2.clone())) {
        return Some(key);
    }
    let key = crate::os::secrets::read(a.keychain_service()).or_else(|| std::env::var(a.key_env()).ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty()))?;
    if let Ok(mut k) = KEYS.lock() {
        k.retain(|(p, _, _)| *p != a.id());
        k.push((a.id(), Instant::now(), key.clone()));
    }
    Some(key)
}

fn adapter_or_err(provider: &str) -> Result<&'static dyn Adapter> {
    adapter(provider).ok_or_else(|| anyhow!("Codealong doesn't know the provider {provider:?}."))
}

/// Whether there's a key for a provider, without reading it.
pub fn has_key(provider: &str) -> bool {
    adapter(provider).is_some_and(|a| crate::os::secrets::has(a.keychain_service()) || std::env::var(a.key_env()).is_ok_and(|k| !k.trim().is_empty()))
}

/// Whether any model `cfg` may ask has a key: what turns the background features' asking on.
pub fn any_key(cfg: &Config) -> bool {
    models(cfg).iter().any(|m| has_key(&m.provider))
}

/// Saves a provider's key in the keychain, replacing one that's there. The command goes to
/// `security` on stdin, so the key never shows in the process list.
pub fn save_key(provider: &str, key: &str) -> Result<()> {
    let a = adapter_or_err(provider)?;
    let key = key.trim();
    if key.is_empty() || key.chars().any(|c| c.is_control() || c.is_whitespace() || c == '"' || c == '\\') {
        bail!("That doesn't look like an API key.");
    }
    let saved = crate::os::secrets::save(a.keychain_service(), key);
    forget_key(a);
    if saved.is_err() || !has_key(provider) {
        bail!("The keychain didn't take the key.");
    }
    Ok(())
}

pub fn delete_key(provider: &str) -> Result<()> {
    let a = adapter_or_err(provider)?;
    crate::os::secrets::delete(a.keychain_service());
    forget_key(a);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GroupMemberCfg, ProjectCfg, RoutingGroupCfg};

    fn model(name: &str, provider: &str, fallback: Option<&str>) -> DecisionModelCfg {
        DecisionModelCfg { name: name.into(), provider: provider.into(), model: format!("{name}-1"), fallback: fallback.map(String::from) }
    }

    /// Jev the default, falling back to Sol; Luna chosen by atlas; kestrel's text kept from all;
    /// a boundary group of claude-work that names nothing (so Jev).
    fn cfg() -> Config {
        let mut cfg = Config::default();
        cfg.decisions.models = vec![model("jev", "typesafe", Some("sol")), model("luna", "openai", Some("jev")), model("sol", "gemini", None)];
        let project = |name: &str, decisions: Option<&str>| ProjectCfg { name: name.into(), repos: vec![], folders: vec![], chrome_profile: None, default: None, decisions: decisions.map(String::from) };
        cfg.projects = vec![project("atlas", Some("luna")), project("kestrel", Some("none")), project("northwind", None), project("harbor", Some("jev"))];
        cfg.routing.groups = vec![RoutingGroupCfg { name: "work".into(), members: vec![GroupMemberCfg { account: "claude-work".into(), ..Default::default() }], boundary: true, ..Default::default() }];
        cfg
    }

    fn names(d: Option<Decider>) -> Option<Vec<String>> {
        d.map(|d| d.models.into_iter().map(|m| m.name).collect())
    }

    #[test]
    fn a_project_goes_to_its_model_and_the_rest_to_the_default_with_its_fallbacks() {
        let cfg = cfg();
        assert_eq!(names(for_project(&cfg, Some("atlas"))), Some(vec!["luna".into()]), "chosen: no fallback");
        assert_eq!(names(for_project(&cfg, Some("kestrel"))), None);
        assert_eq!(names(for_project(&cfg, Some("northwind"))), Some(vec!["jev".into(), "sol".into()]));
        assert_eq!(names(for_project(&cfg, Some("harbor"))), Some(vec!["jev".into()]), "Jev chosen is Jev alone");
        assert_eq!(names(for_project(&cfg, None)), Some(vec!["jev".into(), "sol".into()]));
        let mut off = cfg.clone();
        off.decisions.default = "none".into();
        assert_eq!(names(for_project(&off, Some("northwind"))), None);
        assert_eq!(names(for_project(&off, Some("atlas"))), Some(vec!["luna".into()]));
    }

    #[test]
    fn jev_is_there_without_an_entry_on_its_default_model() {
        let cfg = Config::default();
        assert_eq!(model_named(&cfg, "jev"), Some(DecisionModelCfg { name: "jev".into(), provider: "typesafe".into(), model: "jev-preview".into(), fallback: None }));
        assert_eq!(names(for_project(&cfg, Some("anything"))), Some(vec!["jev".into()]));
    }

    #[test]
    fn a_boundary_groups_sessions_go_to_its_model_and_only_where_their_project_agrees() {
        let mut cfg = cfg();
        cfg.decisions.default = "luna".into();
        // A group that names nothing is Jev's, whatever the default, and never falls back.
        assert_eq!(names(for_session(&cfg, "claude-work", Some("northwind"))), Some(vec!["jev".into()]));
        assert_eq!(names(for_session(&cfg, "claude-work", Some("harbor"))), Some(vec!["jev".into()]));
        assert_eq!(names(for_session(&cfg, "claude-work", Some("atlas"))), None, "atlas chose Luna, the group Jev");
        assert_eq!(names(for_session(&cfg, "claude-work", Some("kestrel"))), None);
        cfg.routing.groups[0].decisions = Some("sol".into());
        assert_eq!(names(for_session(&cfg, "claude-work", Some("northwind"))), Some(vec!["sol".into()]));
        cfg.routing.groups[0].decisions = Some("none".into());
        assert_eq!(names(for_session(&cfg, "claude-work", Some("northwind"))), None);
        // Outside the group, the project's.
        assert_eq!(names(for_session(&cfg, "claude", Some("atlas"))), Some(vec!["luna".into()]));
        assert_eq!(names(for_session(&cfg, "claude", Some("northwind"))), Some(vec!["luna".into(), "jev".into(), "sol".into()]));
    }

    #[test]
    fn a_request_carries_only_text_every_model_it_may_try_may_read() {
        let cfg = cfg();
        let default = for_project(&cfg, Some("northwind")).unwrap();
        let luna = for_project(&cfg, Some("atlas")).unwrap();
        let jev = for_project(&cfg, Some("harbor")).unwrap();
        assert!(luna.carries(Some(&luna)) && default.carries(Some(&default)));
        assert!(!luna.carries(Some(&default)), "the default's text may go to Jev or Sol, not Luna");
        assert!(!default.carries(Some(&luna)));
        assert!(!default.carries(Some(&jev)), "harbor's text may go to Jev alone, and this may fall back to Sol");
        assert!(jev.carries(Some(&default)), "the default's text may go to Jev");
        assert!(!jev.carries(None), "none's text goes nowhere");
    }

    #[test]
    fn a_fallback_is_asked_only_after_a_failure_for_now() {
        let cfg = cfg();
        let default = for_project(&cfg, None).unwrap();
        let asked = std::cell::RefCell::new(Vec::new());
        let answer = |busy: &'static [&'static str]| {
            asked.borrow_mut().clear();
            first_answer(&default, |m| {
                asked.borrow_mut().push(m.name.clone());
                if busy.contains(&m.name.as_str()) {
                    return Err(Busy(format!("{} is rate limiting", m.name)).into());
                }
                Ok(json!({ "decider": m.name }))
            })
        };
        assert_eq!(answer(&[]).unwrap()["decider"], "jev");
        assert_eq!(answer(&["jev"]).unwrap()["decider"], "sol");
        assert_eq!(*asked.borrow(), ["jev", "sol"]);
        let e = answer(&["jev", "sol"]).unwrap_err();
        assert!(is_busy(&e) && e.to_string().contains("sol"), "{e}");
        // A request the model turned down isn't sent elsewhere.
        let refused = first_answer(&default, |m| if m.name == "jev" { anyhow::bail!("TypeSafe couldn't take the request") } else { Ok(json!({})) });
        assert!(refused.is_err());
        // A project's choice never falls back.
        let luna = for_project(&cfg, Some("atlas")).unwrap();
        assert!(is_busy(&first_answer(&luna, |m| Err(Busy(m.name.clone()).into())).unwrap_err()));
    }

    #[test]
    fn a_session_off_the_board_goes_where_every_project_would_send_it() {
        let mut cfg = cfg();
        assert_eq!(names(for_session_anywhere(&cfg, "claude")), None, "atlas and northwind differ");
        assert_eq!(names(for_session_anywhere(&cfg, "claude-work")), None, "atlas and the group differ");
        cfg.projects.retain(|p| p.decisions.is_none());
        assert_eq!(names(for_session_anywhere(&cfg, "claude")), Some(vec!["jev".into(), "sol".into()]));
        assert_eq!(names(for_session_anywhere(&cfg, "claude-work")), Some(vec!["jev".into()]));
    }

    #[test]
    fn a_model_tried_by_hand_stands_in_for_every_choice_but_a_boundary_groups() {
        let cfg = cfg();
        let tried = trying(&cfg, &named(&cfg, "openai:gpt-6-luna").unwrap());
        assert_eq!(names(for_project(&tried, Some("kestrel"))), Some(vec!["openai:gpt-6-luna".into()]));
        assert_eq!(names(for_project(&tried, None)), Some(vec!["openai:gpt-6-luna".into()]));
        assert_eq!(names(for_session(&tried, "claude-work", Some("northwind"))), None, "the group's Jev and the project's stand-in differ");
    }

    #[test]
    fn a_model_is_named_by_hand_from_config_or_as_provider_and_model() {
        let cfg = cfg();
        assert_eq!(names(named(&cfg, "luna").ok()), Some(vec!["luna".into()]));
        let d = named(&cfg, "openai:gpt-6-luna").unwrap();
        assert_eq!((d.models[0].provider.as_str(), d.model()), ("openai", "gpt-6-luna"));
        assert!(named(&cfg, "anthropic:claude").is_err() && named(&cfg, "nothing").is_err());
    }

    #[test]
    fn text_is_cut_where_it_names_an_image() {
        let assets: Map<String, Value> = serde_json::from_value(json!({
            "image-aa": { "type": "image", "format": "png", "data_base64": "AAAA" },
            "image-bb": { "type": "image", "format": "jpg", "data_base64": "BBBB" },
        }))
        .unwrap();
        let img = |f: &str, d: &str| Piece::Image { format: f.into(), data: d.into() };
        let text = |t: &str| Piece::Text(t.into());
        assert_eq!(pieces("{\"m\":\"see image-bb and image-aa\"}", &assets), [text("{\"m\":\"see "), img("jpg", "BBBB"), text(" and "), img("png", "AAAA"), text("\"}")]);
        assert_eq!(pieces("image-aa", &assets), [img("png", "AAAA")]);
        assert_eq!(pieces("nothing", &assets), [text("nothing")]);
        assert_eq!(pieces("", &Map::new()), [text("")]);
    }

    #[test]
    fn a_reply_says_why_there_is_no_answer_and_whether_another_model_may_try() {
        let a = adapter("typesafe").unwrap();
        let refused = r#"{"detail":{"error_type":"api_usage_error","message":"Invalid image dimensions."}}"#;
        let e = reply_of(a, "m", "400", refused).unwrap_err();
        assert_eq!((e.to_string().as_str(), is_busy(&e)), ("TypeSafe couldn't take the request: Invalid image dimensions.", false));
        assert_eq!(reply_of(a, "m", "422", r#"{"detail":"bad"}"#).unwrap_err().to_string(), "TypeSafe couldn't take the request: bad");
        for status in ["429", "529", "503", "500"] {
            assert!(is_busy(&reply_of(a, "m", status, "").unwrap_err()), "{status}");
        }
        let openai = adapter("openai").unwrap();
        let e = reply_of(openai, "m", "400", r#"{"error":{"message":"Unknown question type.","type":"invalid_request_error"}}"#).unwrap_err();
        assert_eq!(e.to_string(), "OpenAI couldn't take the request: Unknown question type.");
        assert!(!is_busy(&reply_of(openai, "m", "401", "").unwrap_err()));
    }

    #[test]
    fn the_key_and_body_are_escaped_for_curl() {
        let a = adapter("typesafe").unwrap();
        let body = json!({ "state": "say \"hi\"\nback\\slash" });
        let cfg = curl_config(a, "tsk_123", &body, false);
        assert!(cfg.contains("header = \"Authorization: Bearer tsk_123\"\n"));
        assert!(cfg.contains(r#"data-binary = "{\"state\":\"say \\\"hi\\\"\\nback\\\\slash\"}""#), "{cfg}");
        assert!(cfg.starts_with("url = \"https://api.typesafe.ai/v1/systemone\"\n"));
        assert!(!cfg.contains("Danger-Zone"));
        assert!(curl_config(a, "k", &body, true).contains("header = \"TypeSafe-Danger-Zone-Images: true\"\n"));
        let gemini = curl_config(adapter("gemini").unwrap(), "g_1", &body, true);
        assert!(gemini.contains("header = \"x-goog-api-key: g_1\"\n") && gemini.starts_with("url = \"https://generativelanguage.googleapis.com/v1beta/decisions\"\n"), "{gemini}");
    }

    #[test]
    fn an_answer_says_what_its_request_used() {
        let reply = json!({ "model": "jev-1.14.0", "answers": {}, "usage": { "input_tokens": 281, "output_tokens": 20 } });
        let u = used(Feature::QuickPanel, adapter("typesafe").unwrap(), "jev-preview", &reply, 1_000);
        assert_eq!((u.at, u.feature.as_str(), u.provider.as_str(), u.model.as_str(), u.tokens.input, u.tokens.output), (1_000, "quick_panel", "typesafe", "jev-1.14.0", 281, 20));
        let u = used(Feature::Test, adapter("gemini").unwrap(), "gemini-decision-1", &json!({ "answers": {} }), 1_000);
        assert_eq!((u.feature.as_str(), u.provider.as_str(), u.model.as_str(), u.tokens.input), ("test", "gemini", "gemini-decision-1", 0));
    }
}

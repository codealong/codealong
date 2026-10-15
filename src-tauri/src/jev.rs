//! TypeSafe's Jev (docs.typesafe.ai): one endpoint that answers typed questions about a state
//! with a probability for every option. Codealong asks it narrow things, each feature behind its
//! own switch in `[jev]` (config.rs): where a prompt goes (route.rs), how a session's turn ended
//! (insights.rs), which todo a session is for (todos), which review comments ask for a change.
//!
//! The API key lives in the login keychain (service `Codealong TypeSafe`), never in config.toml
//! or on a command line: curl gets it and the request on stdin, and saving it goes through
//! `security -i`, which reads its command from stdin too. It's read from the keychain once and
//! kept in memory, since the engine asks often. `TYPESAFE_API_KEY` works when the keychain has
//! none, for the command line.
//!
//! Jev reads questions literally and is weak at dates and numbers: callers hand it times as
//! words (`ago_words`) and keep each question to one plain thing.
//!
//! Images pasted into what a request says go with it (`Images`), as TypeSafe's experiment
//! (docs.typesafe.ai/images): each is an asset of the body, named in the text by a key that Jev
//! reads as the image in its place, and sent with `TypeSafe-Danger-Zone-Images: true`.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const KEYCHAIN_SERVICE: &str = "Codealong TypeSafe";
const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a key read from the keychain is used before it's read again, so a key replaced
/// from another place (the `security` command) is picked up.
const KEY_FOR: Duration = Duration::from_secs(10 * 60);

/// A request's body: the model, the state and the questions.
pub fn body(model: &str, state: Value, questions: Value) -> Value {
    json!({ "model": model, "state": state, "questions": questions })
}

/// Asks Jev. The answer is the whole response: `model`, `answers` by question id, `usage`.
pub fn ask(body: &Value) -> Result<Value> {
    let key = api_key().ok_or_else(|| anyhow!("There's no TypeSafe API key yet. Add it in Settings › Jev."))?;
    let (status, reply) = post(&key, body)?;
    // Images are an experiment of TypeSafe's, and one it won't take fails the whole request
    // (400, or 413 for size): asked again without them, the question is answered from the words.
    if has_images(body) && matches!(status.as_str(), "400" | "413") {
        let (status, reply) = post(&key, &without_images(body))?;
        return answer(&status, &reply);
    }
    answer(&status, &reply)
}

/// One call with a tiny question, for Settings' Test button: proves the key and says how fast.
pub fn check(model: &str) -> Result<String> {
    let started = Instant::now();
    let v = ask(&body(model, json!("Fix the flaky login test"), json!({ "task": { "type": "noul", "instructions": "Does this ask for a change to code?" } })))?;
    Ok(format!("{} answered in {} ms.", v["model"].as_str().unwrap_or(model), started.elapsed().as_millis()))
}

/* ---- Reading answers ---- */

/// A noul's answer, 0 (no) to 1 (yes).
pub fn noul(response: &Value, id: &str) -> Option<f64> {
    response["answers"][id]["noul"].as_f64()
}

/// A choice's options with their probabilities, the likeliest first.
pub fn choice(response: &Value, id: &str) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = response["answers"][id]["probabilities"]
        .as_object()
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_f64()?))).collect())
        .unwrap_or_default();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    out
}

/// A score's value, from 0 to one less than its number of levels; it can land between levels.
pub fn score(response: &Value, id: &str) -> Option<f64> {
    response["answers"][id]["score"].as_f64()
}

/// How sure Jev was of an answer, 0 to 1, when the answer carries it.
pub fn confidence(response: &Value, id: &str) -> Option<f64> {
    response["answers"][id]["confidence"].as_f64()
}

/* ---- Words for the model ---- */

/// A time as words. Jev reads dates as text and can't compare them; words it can.
pub fn ago_words(then: i64, now: i64) -> String {
    let mins = (now - then).max(0) / 60_000;
    match mins {
        0..=1 => "just now".into(),
        2..=59 => format!("{mins} minutes ago"),
        60..=119 => "an hour ago".into(),
        120..=1439 => format!("{} hours ago", mins / 60),
        1440..=2879 => "yesterday".into(),
        _ => format!("{} days ago", mins / 1440),
    }
}

/// Text on one line and at most `max` characters, cut with an ellipsis.
pub fn clip(s: &str, max: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= max {
        return s;
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// The last `max` characters of a text, on one line: where a reply's question or its summary is.
pub fn tail(s: &str, max: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let n = s.chars().count();
    if n <= max {
        return s;
    }
    let mut out = String::from("…");
    out.extend(s.chars().skip(n - max + 1));
    out
}

/* ---- Images ---- */

/// Each side of an image Jev takes, in pixels, and the fewest pixels in all (280 × 280): one
/// outside them fails the whole request, so images are scaled down and padded out to fit. The
/// most pixels, 1568 × 1568, follows from the sides, and a PNG or JPEG that size is under the
/// 16 MiB a file may be.
const SIDE_MIN: u32 = 256;
const SIDE_MAX: u32 = 1568;
const PIXELS_MIN: u64 = 78_400;
/// The pixels a request's images share. An image costs a token for about every 370 pixels
/// (measured on 1.14), so this is about 8,000 tokens; a quick panel's request is near 36,000 of
/// the 64,000 one may have without them.
const PIXELS_PER_REQUEST: u64 = 3_000_000;
/// Images sent with one request at most, the first ones the text has.
const MAX_IMAGES: usize = 4;
/// The most a body with images may be: curl takes it on one line of its config, which can't be
/// over 10 MiB (curl 8.7).
const MAX_BODY: usize = 8 * 1024 * 1024;

/// The images attached to what a request says (`![…](images/…)`, attachments.rs), to send with
/// it. `swap` puts a key in the text where each image was, and `attach` makes each one an asset
/// of the body under its key, which Jev reads as the image in that place.
#[derive(Debug, Default)]
pub struct Images {
    names: Vec<String>,
}

impl Images {
    /// The text with each image in it as its key; one past `MAX_IMAGES` leaves nothing.
    pub fn swap(&mut self, text: &str) -> String {
        crate::attachments::swap_images(text, |name| {
            if !self.names.iter().any(|n| n == name) {
                if self.names.len() >= MAX_IMAGES {
                    return String::new();
                }
                self.names.push(name.to_string());
            }
            image_key(name)
        })
    }

    /// Puts the images in the body as its assets, from the kept images (`attachments::images_dir`).
    pub fn attach(&self, body: &mut Value) {
        self.attach_from(&crate::attachments::images_dir(), body)
    }

    /// An image Jev can't take (one not on this Mac, or that didn't convert), or whose key the
    /// body no longer has (cut off with the text around it), is left out, and its key taken out of
    /// the text: TypeSafe refuses a request with an asset it doesn't name. Images that would make
    /// the body too big for curl are all left out.
    fn attach_from(&self, dir: &Path, body: &mut Value) {
        if self.names.is_empty() {
            return;
        }
        let said = body.to_string();
        let pixels = PIXELS_PER_REQUEST / self.names.len() as u64;
        let mut assets = Map::new();
        let mut gone = Vec::new();
        for name in &self.names {
            let key = image_key(name);
            match said.contains(&key).then(|| ready(&dir.join(name), pixels)).flatten() {
                Some(asset) => {
                    assets.insert(key, asset);
                }
                None => gone.push(key),
            }
        }
        strip_keys(body, &gone);
        if !assets.is_empty() {
            body["assets"] = Value::Object(assets);
        }
        if body.to_string().len() > MAX_BODY {
            *body = without_images(body);
        }
    }
}

/// How the text names an image: by its hash, which no other text has.
fn image_key(name: &str) -> String {
    format!("image-{}", name.split('.').next().unwrap_or(name))
}

fn has_images(body: &Value) -> bool {
    body.get("assets").is_some()
}

/// The body with its images taken out: no assets, and no key left in its text.
fn without_images(body: &Value) -> Value {
    let keys: Vec<String> = body["assets"].as_object().map(|a| a.keys().cloned().collect()).unwrap_or_default();
    let mut out = body.clone();
    if let Some(o) = out.as_object_mut() {
        o.remove("assets");
    }
    strip_keys(&mut out, &keys);
    out
}

fn strip_keys(v: &mut Value, keys: &[String]) {
    match v {
        Value::String(s) => {
            for k in keys {
                if s.contains(k.as_str()) {
                    *s = s.replace(k.as_str(), "");
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|v| strip_keys(v, keys)),
        Value::Object(o) => o.values_mut().for_each(|v| strip_keys(v, keys)),
        _ => {}
    }
}

/// The size an image of `w` × `h` is sent at with `pixels` at most: the picture scaled down to
/// fit (never up), then the image padded out to the least Jev takes. The picture's size first,
/// then the image's.
fn fit(w: u32, h: u32, pixels: u64) -> Option<((u32, u32), (u32, u32))> {
    if w == 0 || h == 0 {
        return None;
    }
    let (wf, hf) = (w as f64, h as f64);
    let scale = (SIDE_MAX as f64 / wf.max(hf)).min((pixels as f64 / (wf * hf)).sqrt()).min(1.0);
    let (sw, sh) = (((wf * scale).round() as u32).max(1), ((hf * scale).round() as u32).max(1));
    let (mut pw, mut ph) = (sw.max(SIDE_MIN), sh.max(SIDE_MIN));
    if (pw as u64) * (ph as u64) < PIXELS_MIN {
        let side = PIXELS_MIN.isqrt() as u32;
        (pw, ph) = (pw.max(side), ph.max(side));
    }
    Some(((sw, sh), (pw, ph)))
}

/// Images made ready lately, by file and pixels: the quick panel asks again each time typing
/// pauses, with the same images.
static READY: Mutex<Vec<(String, u64, Value)>> = Mutex::new(Vec::new());
const READY_KEPT: usize = 8;

fn ready(path: &Path, pixels: u64) -> Option<Value> {
    let id = path.to_string_lossy().to_string();
    if let Some(asset) = READY.lock().ok().and_then(|r| r.iter().find(|(p, n, _)| *p == id && *n == pixels).map(|r| r.2.clone())) {
        return Some(asset);
    }
    let asset = make_ready(path, pixels)?;
    if let Ok(mut r) = READY.lock() {
        r.push((id, pixels, asset.clone()));
        if r.len() > READY_KEPT {
            r.remove(0);
        }
    }
    Some(asset)
}

/// An image as an asset Jev takes: PNG or JPEG, sized by `fit`. One that fits already goes as it
/// is; any other is made so by `sips`, a JPEG staying JPEG and the rest (GIF and WebP, which Jev
/// doesn't take, too) becoming PNG, padded with white.
fn make_ready(path: &Path, pixels: u64) -> Option<Value> {
    const SIPS: &str = "/usr/bin/sips";
    let file = path.to_str()?;
    let ext = path.extension()?.to_str()?;
    let format = if ext == "jpg" { "jpeg" } else { "png" };
    let size = crate::procs::run(SIPS, &["-g", "pixelWidth", "-g", "pixelHeight", file], None, Duration::from_secs(10)).ok()?;
    let side = |name: &str| size.lines().find_map(|l| l.trim().strip_prefix(name)?.trim().parse::<u32>().ok());
    let (w, h) = (side("pixelWidth:")?, side("pixelHeight:")?);
    let (picture, image) = fit(w, h, pixels)?;
    let bytes = if ["png", "jpg"].contains(&ext) && picture == (w, h) && image == (w, h) {
        std::fs::read(path).ok()?
    } else {
        static MADE: AtomicU64 = AtomicU64::new(0);
        let out = std::env::temp_dir().join(format!("codealong-jev-{}-{}.{format}", std::process::id(), MADE.fetch_add(1, Ordering::Relaxed)));
        let (pw, ph, iw, ih) = (picture.0.to_string(), picture.1.to_string(), image.0.to_string(), image.1.to_string());
        let mut args = vec!["-s", "format", format];
        if picture != (w, h) {
            args.extend(["-z", ph.as_str(), pw.as_str()]);
        }
        if image != picture {
            args.extend(["-p", ih.as_str(), iw.as_str(), "--padColor", "FFFFFF"]);
        }
        args.extend([file, "--out", out.to_str()?]);
        let made = crate::procs::run(SIPS, &args, None, Duration::from_secs(20)).ok().and_then(|_| std::fs::read(&out).ok());
        let _ = std::fs::remove_file(&out);
        made?
    };
    Some(json!({ "type": "image", "format": if format == "jpeg" { "jpg" } else { "png" }, "data_base64": data_encoding::BASE64.encode(&bytes) }))
}

/* ---- In the background ---- */

/// A request for the worker, and what its answer is for: the id comes back with it. `fill`
/// finishes the body on the worker's thread just before it's sent, for state that takes a slow
/// read (a harness's pending call, over HTTP for OpenCode): the engine's thread never waits on it.
pub struct Job {
    pub id: String,
    pub body: Value,
    pub fill: Option<Box<dyn FnOnce(&mut Value) + Send>>,
}

/// A thread that asks Jev one request at a time and hands each answer to `done`. The engine
/// sends it jobs and hears back through its own channel, so a slow answer never holds up a
/// board.
pub fn worker(done: impl Fn(String, std::result::Result<Value, String>) + Send + 'static) -> Sender<Job> {
    let (tx, rx) = channel::<Job>();
    std::thread::Builder::new()
        .name("jev".into())
        .spawn(move || {
            for mut job in rx {
                if let Some(fill) = job.fill.take() {
                    fill(&mut job.body);
                }
                let answer = ask(&job.body).map_err(|e| format!("{e:#}"));
                // Rate limited or overloaded: wait a moment before the next one.
                if answer.as_ref().err().is_some_and(|e| e.contains("rate limiting") || e.contains("overloaded")) {
                    std::thread::sleep(Duration::from_secs(5));
                }
                done(job.id, answer);
            }
        })
        .expect("spawning the jev thread");
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

/// The curl config for one request: the key and the body both go on curl's stdin. A body with
/// images says so in a header, which TypeSafe refuses one without.
fn curl_config(key: &str, body: &Value) -> String {
    format!(
        "url = {}\nrequest = \"POST\"\nheader = {}\nheader = \"Content-Type: application/json\"\nheader = \"Accept: application/json\"\nheader = \"User-Agent: codealong/{}\"\n{}data-binary = {}\n",
        curl_quoted(ENDPOINT),
        curl_quoted(&format!("Authorization: Bearer {key}")),
        env!("CARGO_PKG_VERSION"),
        if has_images(body) { "header = \"TypeSafe-Danger-Zone-Images: true\"\n" } else { "" },
        curl_quoted(&body.to_string()),
    )
}

/// Sends a request: the HTTP status and the reply. Images take longer to send, so they get
/// longer.
fn post(key: &str, body: &Value) -> Result<(String, String)> {
    if key.chars().any(|c| c.is_control() || c == '"' || c == '\\') {
        bail!("The TypeSafe API key has characters a key can't have. Paste it again in Settings › Jev.");
    }
    let mut child = Command::new("/usr/bin/curl")
        .args(["-sS", "-m", if has_images(body) { "40" } else { "15" }, "-w", "\n%{http_code}", "-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("curl: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(curl_config(key, body).as_bytes())?;
    }
    let out = child.wait_with_output().map_err(|e| anyhow!("curl: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (reply, status) = text.rsplit_once('\n').unwrap_or(("", &text));
    if matches!(status.trim(), "000" | "") {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("Couldn't reach TypeSafe: {}", err.trim().trim_start_matches("curl: ").chars().take(160).collect::<String>())
    }
    Ok((status.trim().to_string(), reply.to_string()))
}

/// What a reply says: the answer, or why there's none.
fn answer(status: &str, reply: &str) -> Result<Value> {
    // A 400's detail is `{error_type, message}`.
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
        "200" => serde_json::from_str(reply).map_err(|_| anyhow!("TypeSafe sent an answer Codealong can't read.")),
        "401" | "403" => {
            forget_key();
            bail!("TypeSafe didn't accept the API key. Check it in Settings › Jev.")
        }
        "400" | "422" => bail!("TypeSafe couldn't take the request: {}", detail()),
        "413" => bail!("TypeSafe couldn't take a request that big."),
        "429" => bail!("TypeSafe is rate limiting; try again in a moment."),
        "529" => bail!("TypeSafe is overloaded; try again in a moment."),
        other => bail!("TypeSafe answered HTTP {other}. {}", detail()),
    }
}

/* ---- The key ---- */

static KEY: Mutex<Option<(Instant, String)>> = Mutex::new(None);

pub(crate) fn keychain_account() -> String {
    std::env::var("USER").ok().filter(|u| !u.is_empty() && u.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))).unwrap_or_else(|| "codealong".into())
}

fn forget_key() {
    if let Ok(mut k) = KEY.lock() {
        *k = None;
    }
}

/// The API key: the keychain's, else `TYPESAFE_API_KEY`.
fn api_key() -> Option<String> {
    if let Some((at, key)) = KEY.lock().ok().and_then(|k| k.clone()) {
        if at.elapsed() < KEY_FOR {
            return Some(key);
        }
    }
    let key = crate::procs::run("/usr/bin/security", &["find-generic-password", "-a", &keychain_account(), "-s", KEYCHAIN_SERVICE, "-w"], None, KEYCHAIN_TIMEOUT)
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .or_else(|| std::env::var("TYPESAFE_API_KEY").ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty()))?;
    if let Ok(mut k) = KEY.lock() {
        *k = Some((Instant::now(), key.clone()));
    }
    Some(key)
}

/// Whether there's a key, without reading it.
pub fn has_key() -> bool {
    crate::procs::run("/usr/bin/security", &["find-generic-password", "-a", &keychain_account(), "-s", KEYCHAIN_SERVICE], None, KEYCHAIN_TIMEOUT).is_ok()
        || std::env::var("TYPESAFE_API_KEY").is_ok_and(|k| !k.trim().is_empty())
}

/// Saves the key in the keychain, replacing one that's there. The command goes to `security`
/// on stdin, so the key never shows in the process list.
pub fn save_key(key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() || key.chars().any(|c| c.is_control() || c.is_whitespace() || c == '"' || c == '\\') {
        bail!("That doesn't look like an API key.");
    }
    let command = format!("add-generic-password -U -a \"{}\" -s \"{}\" -l \"{}\" -w \"{}\"\n", keychain_account(), KEYCHAIN_SERVICE, KEYCHAIN_SERVICE, key);
    let out = crate::procs::run_stdin("/usr/bin/security", &["-i"], &command, KEYCHAIN_TIMEOUT)?;
    forget_key();
    // `security -i` exits cleanly whatever its command did; errors come back as text.
    if out.to_lowercase().contains("error") || !has_key() {
        bail!("The keychain didn't take the key.");
    }
    Ok(())
}

pub fn delete_key() -> Result<()> {
    let _ = crate::procs::run("/usr/bin/security", &["delete-generic-password", "-a", &keychain_account(), "-s", KEYCHAIN_SERVICE], None, KEYCHAIN_TIMEOUT);
    forget_key();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 10 * 24 * 3600 * 1000;

    #[test]
    fn times_go_in_as_words() {
        assert_eq!(ago_words(NOW - 30_000, NOW), "just now");
        assert_eq!(ago_words(NOW - 5 * 60_000, NOW), "5 minutes ago");
        assert_eq!(ago_words(NOW - 3 * 3_600_000, NOW), "3 hours ago");
        assert_eq!(ago_words(NOW - 30 * 3_600_000, NOW), "yesterday");
        assert_eq!(ago_words(NOW - 4 * 86_400_000, NOW), "4 days ago");
    }

    #[test]
    fn clips_the_start_and_keeps_the_end() {
        assert_eq!(clip("one  two\nthree", 20), "one two three");
        assert_eq!(clip("abcdefghij", 5), "abcd…");
        assert_eq!(tail("abcdefghij", 5), "…ghij");
        assert_eq!(tail("short", 10), "short");
    }

    #[test]
    fn reads_answers() {
        let v = json!({ "answers": {
            "a": { "type": "noul", "noul": 0.9 },
            "b": { "type": "choice", "choice": "x", "probabilities": { "x": 0.7, "y": 0.3 } },
            "c": { "type": "score", "score": 1.4, "confidence": 0.55 },
        }});
        assert_eq!(noul(&v, "a"), Some(0.9));
        assert_eq!((confidence(&v, "c"), confidence(&v, "a")), (Some(0.55), None));
        assert_eq!(choice(&v, "b"), vec![("x".to_string(), 0.7), ("y".to_string(), 0.3)]);
        assert_eq!(score(&v, "c"), Some(1.4));
        assert_eq!(noul(&v, "missing"), None);
    }

    #[test]
    fn images_are_scaled_down_and_padded_out_to_what_jev_takes() {
        // A Retina screenshot alone keeps the most a side may have; four share the pixels.
        assert_eq!(fit(3426, 2094, PIXELS_PER_REQUEST), Some(((1568, 958), (1568, 958))));
        assert_eq!(fit(3426, 2094, PIXELS_PER_REQUEST / 4), Some(((1108, 677), (1108, 677))));
        assert_eq!(fit(1200, 800, PIXELS_PER_REQUEST), Some(((1200, 800), (1200, 800))));
        // Too small, too thin, or just short of the pixels: padded, never scaled up.
        assert_eq!(fit(32, 32, PIXELS_PER_REQUEST), Some(((32, 32), (280, 280))));
        assert_eq!(fit(4000, 100, PIXELS_PER_REQUEST), Some(((1568, 39), (1568, 256))));
        assert_eq!(fit(256, 300, PIXELS_PER_REQUEST), Some(((256, 300), (280, 300))));
        assert_eq!(fit(0, 10, PIXELS_PER_REQUEST), None);
    }

    /// A body as route.rs makes one: a key in the text where the image was.
    fn with_image(text: &str) -> (Images, Value) {
        let mut images = Images::default();
        let message = images.swap(text);
        (images, body("jev-1.14.0", json!({ "message": message }), json!({ "q": { "type": "noul", "instructions": "?" } })))
    }

    #[test]
    fn an_image_goes_as_an_asset_named_where_it_was() {
        let dir = crate::store::test_dir("jev-images");
        // A 1 × 1 PNG, which Jev takes only padded out to 280 × 280.
        let png = data_encoding::BASE64.decode(b"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==").unwrap();
        std::fs::write(dir.join("0123456789abcdef.png"), &png).unwrap();
        let (images, mut b) = with_image("Fix ![image](images/0123456789abcdef.png) and images/0123456789abcdef.png, not ![x](images/fedcba9876543210.png)");
        assert_eq!(b["state"]["message"], "Fix image-0123456789abcdef and image-0123456789abcdef, not image-fedcba9876543210");
        images.attach_from(&dir, &mut b);
        // The image that isn't there is taken out of the text; the other is sent once.
        assert_eq!(b["state"]["message"], "Fix image-0123456789abcdef and image-0123456789abcdef, not ");
        let assets = b["assets"].as_object().unwrap();
        assert_eq!(assets.keys().collect::<Vec<_>>(), ["image-0123456789abcdef"]);
        assert_eq!((&assets["image-0123456789abcdef"]["type"], &assets["image-0123456789abcdef"]["format"]), (&json!("image"), &json!("png")));
        let sent = data_encoding::BASE64.decode(assets["image-0123456789abcdef"]["data_base64"].as_str().unwrap().as_bytes()).unwrap();
        let side = |at: usize| u32::from_be_bytes(sent[at..at + 4].try_into().unwrap());
        assert_eq!((&sent[..8], side(16), side(20)), (&b"\x89PNG\r\n\x1a\n"[..], 280, 280));
        assert!(curl_config("k", &b).contains("header = \"TypeSafe-Danger-Zone-Images: true\"\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_image_cut_off_with_its_text_is_left_out() {
        let dir = crate::store::test_dir("jev-images-cut");
        std::fs::write(dir.join("0123456789abcdef.png"), b"not read").unwrap();
        let (images, mut b) = with_image("Fix ![image](images/0123456789abcdef.png)");
        b["state"]["message"] = json!(clip(b["state"]["message"].as_str().unwrap(), 10));
        images.attach_from(&dir, &mut b);
        assert_eq!((b["state"]["message"].as_str(), b.get("assets")), (Some("Fix image…"), None));
        assert!(!curl_config("k", &b).contains("Danger-Zone"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_request_asked_again_without_images_names_none() {
        let mut images = Images::default();
        let text = (0..6).map(|i| format!("![image](images/{i:016x}.png)")).collect::<Vec<_>>().join(" ");
        assert_eq!(images.swap(&text).split_whitespace().count(), MAX_IMAGES);
        let (_, mut b) = with_image("Fix ![image](images/0123456789abcdef.png) now");
        b["assets"] = json!({ "image-0123456789abcdef": { "type": "image", "format": "png", "data_base64": "AAAA" } });
        let plain = without_images(&b);
        assert_eq!((plain["state"]["message"].as_str(), plain.get("assets")), (Some("Fix  now"), None));
        assert!(has_images(&b) && !has_images(&plain));
    }

    #[test]
    fn a_reply_says_why_there_is_no_answer() {
        let refused = r#"{"detail":{"error_type":"api_usage_error","message":"Invalid image dimensions."}}"#;
        assert_eq!(answer("400", refused).unwrap_err().to_string(), "TypeSafe couldn't take the request: Invalid image dimensions.");
        assert_eq!(answer("422", r#"{"detail":"bad"}"#).unwrap_err().to_string(), "TypeSafe couldn't take the request: bad");
        assert_eq!(answer("200", r#"{"answers":{}}"#).unwrap(), json!({ "answers": {} }));
    }

    #[test]
    fn the_key_and_body_are_escaped_for_curl() {
        let body = json!({ "state": "say \"hi\"\nback\\slash" });
        let cfg = curl_config("tsk_123", &body);
        assert!(cfg.contains("header = \"Authorization: Bearer tsk_123\"\n"));
        assert!(cfg.contains(r#"data-binary = "{\"state\":\"say \\\"hi\\\"\\nback\\\\slash\"}""#), "{cfg}");
        assert!(cfg.starts_with("url = \"https://api.typesafe.ai/v1/systemone\"\n"));
    }
}

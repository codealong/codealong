//! The harness accounts on this Mac, whether config.toml lists them or not: Claude Code config
//! folders, Codex, OpenCode, Antigravity and Cursor, found by the files each keeps, and the wrappers and
//! aliases people make to run a second account (a `claude2` script that sets
//! `CLAUDE_CONFIG_DIR`, an `alias cw='CLAUDE_CONFIG_DIR=~/.claude-work claude'`). config.toml
//! starts from the ones in use on first launch; Settings › Accounts lists the rest under Found
//! on this Mac, and `codealong config show` prints them.
//!
//! Gathering only reads files, and asks the keychain whether a Claude sign-in exists, never for
//! it. What counts, and what each account is called, is decided by pure functions over what was
//! gathered, tested below without this Mac's files.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::AccountCfg;
use crate::util::{expand, home, now_ms, tilde, DAY_MS};

/// Sessions this recent make an account one you use.
const ACTIVE_MS: i64 = 30 * DAY_MS;

/// Wrapper scripts are small; anything bigger is a program, not a wrapper.
const WRAPPER_MAX_BYTES: u64 = 64 * 1024;

/// Where an account stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    /// Sessions in the last 30 days.
    Active,
    /// Signed in, with no sessions lately.
    SignedIn,
    /// Installed or set up in your shell, but neither signed in nor used lately.
    Idle,
}

/// An account Codealong could read.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    /// What the account would be called: the wrapper or alias that runs it, else its folder's.
    pub name: String,
    pub harness: String,
    /// The config folder, with `~` for the home folder.
    pub home: String,
    /// What starts it in a terminal; None when that's the name (or it has no command).
    pub command: Option<String>,
    /// The file the command comes from, when it's one of yours: `~/.local/bin/claude2`, `~/.zshrc`.
    pub via: Option<String>,
    pub state: State,
    pub signed_in: bool,
    /// The folder exists; an installed harness that never ran has none yet.
    pub folder_exists: bool,
    pub email: Option<String>,
    /// What Settings says of its sign-in in place of the email (`Adapter::sign_in_note`).
    pub sign_in_note: Option<String>,
    /// When its sessions last wrote, as far as its files say.
    pub last_used: Option<i64>,
    /// Codealong has never read this folder and nobody dismissed it, so it's worth suggesting.
    pub new: bool,
}

impl Found {
    pub fn account(&self) -> AccountCfg {
        AccountCfg { name: self.name.clone(), harness: self.harness.clone(), home: self.home.clone(), command: self.command.clone(), start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false }
    }
}

/// What finds a harness's accounts on this Mac: its files and commands (`Adapter::spec`).
pub struct Spec {
    pub harness: &'static str,
    /// The command a terminal runs it with, when wrappers and aliases of it run other accounts.
    pub program: Option<&'static str>,
    /// The command a config.toml account gets for the harness's own folder.
    pub default_command: Option<&'static str>,
    /// The variable that moves its folder, and the folder's name under it when it's a parent.
    pub var: Option<(&'static str, Option<&'static str>)>,
    /// Its own folder.
    pub default_dir: &'static str,
    /// The name its own folder and siblings share: `~/.claude`, `~/.claude2`, `~/.claude-work`.
    pub siblings: Option<&'static str>,
}

/// Each harness's (`Adapter::spec`), in the adapters' order.
fn specs() -> Vec<&'static Spec> {
    crate::harness::all().map(|a| a.spec()).collect()
}

fn spec_for_var(var: &str) -> Option<&'static Spec> {
    specs().into_iter().find(|s| s.var.is_some_and(|(v, _)| v == var))
}

// ---- Reading shell setup ----

/// A shell setting that points a harness at a folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    /// The alias, function or wrapper that sets it; None for a plain export, which moves the
    /// folder the harness's own command uses.
    pub name: Option<String>,
    pub var: String,
    /// The value as written: `~/.claude-work`, `$HOME/.claude2`.
    pub value: String,
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The folder variables assigned in a piece of shell, with their values as written:
/// `CLAUDE_CONFIG_DIR=~/.x claude`, `export CODEX_HOME="$HOME/.codex2"`, fish's
/// `set -gx CLAUDE_CONFIG_DIR ~/.x`.
fn assignments(s: &str) -> Vec<(String, String)> {
    // Escaped quotes inside an alias's quotes read as plain quotes here.
    let s = s.replace("\\\"", "\"").replace("\\'", "'");
    let mut out = Vec::new();
    for spec in specs() {
        let Some((var, _)) = spec.var else { continue };
        let mut from = 0;
        while let Some(i) = s[from..].find(var).map(|i| i + from) {
            from = i + var.len();
            if s[..i].chars().next_back().is_some_and(is_word_char) {
                continue;
            }
            let rest = &s[from..];
            let value = if let Some(v) = rest.strip_prefix('=') {
                read_value(v)
            } else if s[..i].trim_end().split_whitespace().next_back().is_some_and(|w| w.starts_with('-')) && s[..i].trim_start().starts_with("set ") {
                // fish: `set -gx VAR value`
                read_value(rest.trim_start())
            } else {
                None
            };
            if let Some(v) = value.filter(|v| !v.is_empty()) {
                out.push((var.to_string(), v));
            }
        }
    }
    out
}

/// The folder variables a wrapper script sets, line by line, leaving out comments.
pub fn script_assignments(text: &str) -> Vec<(String, String)> {
    text.lines().map(str::trim).filter(|l| !l.starts_with('#')).flat_map(assignments).collect()
}

/// A shell word: quoted, or up to the next space or operator.
fn read_value(s: &str) -> Option<String> {
    let mut chars = s.chars();
    match chars.next()? {
        q @ ('"' | '\'') => {
            let inner: String = chars.take_while(|&c| c != q).collect();
            Some(inner)
        }
        _ => Some(s.chars().take_while(|&c| !c.is_whitespace() && !";&|)'\"`".contains(c)).collect()),
    }
}

/// `alias NAME=VALUE` (zsh, bash, fish) or fish's `alias NAME VALUE`: the name and the value.
fn alias(line: &str) -> Option<(String, String)> {
    let rest = line.trim_start().strip_prefix("alias")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let mut rest = rest.trim_start();
    // zsh's `alias -g name=…`
    while rest.starts_with('-') {
        rest = rest.split_once(char::is_whitespace).map(|(_, r)| r.trim_start())?;
    }
    let end = rest.find(|c: char| c == '=' || c.is_whitespace())?;
    let name = rest[..end].trim_matches(|c| c == '"' || c == '\'').to_string();
    let value = rest[end + 1..].trim();
    (!name.is_empty()).then(|| (name, value.to_string()))
}

/// The name a line starts a function with: `name() {`, `function name {`, `function name() {`,
/// or fish's `function name`.
fn function_start(line: &str) -> Option<String> {
    let t = line.trim_start();
    let valid = |n: &str| !n.is_empty() && n.chars().all(|c| is_word_char(c) || "-.:".contains(c));
    if let Some(rest) = t.strip_prefix("function ") {
        let name: String = rest.trim_start().chars().take_while(|&c| is_word_char(c) || "-.:".contains(c)).collect();
        return valid(&name).then_some(name);
    }
    let (name, after) = t.split_once('(')?;
    let name = name.trim_end();
    (valid(name) && !name.contains(char::is_whitespace) && after.trim_start().starts_with(')')).then(|| name.to_string())
}

/// Opens and closes on a line: braces for sh-like shells, block words and `end` for fish.
fn depth_change(line: &str, fish: bool) -> i32 {
    if fish {
        let first = line.split_whitespace().next().unwrap_or("");
        return match first {
            "function" | "if" | "for" | "while" | "switch" | "begin" => 1,
            "end" => -1,
            _ => 0,
        };
    }
    line.matches('{').count() as i32 - line.matches('}').count() as i32
}

/// The harness settings in a shell startup file: aliases and functions that set a harness's
/// folder variable, and top-level exports of one.
pub fn parse_rc(text: &str, fish: bool) -> Vec<Decl> {
    let mut out = Vec::new();
    let mut function: Option<String> = None;
    let mut depth = 0;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if function.is_none() {
            if let Some((name, value)) = alias(line) {
                out.extend(assignments(&value).into_iter().map(|(var, value)| Decl { name: Some(name.clone()), var, value }));
                continue;
            }
            if let Some(name) = function_start(line) {
                depth = depth_change(line, fish);
                // A one-line function (`cw() { CLAUDE_CONFIG_DIR=~/.x claude "$@"; }`) opens and closes here.
                out.extend(assignments(line).into_iter().map(|(var, value)| Decl { name: Some(name.clone()), var, value }));
                if depth > 0 || (!fish && !line.contains('{')) {
                    function = Some(name);
                }
                continue;
            }
            let exported = line.starts_with("export ") || line.starts_with("set ") || line.starts_with("typeset -x") || line.starts_with("declare -x");
            if exported {
                out.extend(assignments(line).into_iter().map(|(var, value)| Decl { name: None, var, value }));
            }
            continue;
        }
        out.extend(assignments(line).into_iter().map(|(var, value)| Decl { name: function.clone(), var, value }));
        depth += depth_change(line, fish);
        if depth <= 0 && (fish || line.contains('}')) {
            function = None;
            depth = 0;
        }
    }
    out
}

/// A folder value as a path: `~`, `$HOME` and `${HOME}` expanded; None when it depends on
/// anything else.
fn resolve(value: &str, home: &Path) -> Option<PathBuf> {
    let v = value.trim().trim_end_matches('/');
    let rest = if v == "~" || v == "$HOME" || v == "${HOME}" {
        ""
    } else if let Some(r) = v.strip_prefix("~/").or_else(|| v.strip_prefix("$HOME/")).or_else(|| v.strip_prefix("${HOME}/")) {
        r
    } else if v.starts_with('/') {
        return (!v.contains('$')).then(|| PathBuf::from(v));
    } else {
        return None;
    };
    (!rest.contains('$')).then(|| if rest.is_empty() { home.to_path_buf() } else { home.join(rest) })
}

// ---- Deciding ----

/// One folder, with what gathering learned about it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Folder {
    pub harness: &'static str,
    pub dir: PathBuf,
    /// The folder the harness's own command uses.
    pub default: bool,
    /// Wrappers, aliases and functions that run it.
    pub named: Vec<Named>,
    /// It holds sessions or a sign-in: the harness has run here.
    pub has_data: bool,
    pub exists: bool,
    pub signed_in: bool,
    pub email: Option<String>,
    pub sign_in_note: Option<String>,
    pub last_used: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Named {
    pub name: String,
    /// The file it's in: `~/.local/bin/claude2`, `~/.zshrc`.
    pub via: String,
    pub wrapper: bool,
}

/// Whether a folder is an account, and where it stands. A sibling folder that nothing names
/// (`~/.claude-old`) counts only when it's in use or signed in; the harness's own folder and
/// ones your shell names count once the harness is installed or has run there.
pub fn decide(f: &Folder, installed: bool, now: i64) -> Option<State> {
    if f.has_data && f.last_used.is_some_and(|t| now - t < ACTIVE_MS) {
        return Some(State::Active);
    }
    if f.signed_in {
        return Some(State::SignedIn);
    }
    ((f.default || !f.named.is_empty()) && (installed || f.has_data)).then_some(State::Idle)
}

/// The name, command and its source for a folder: the harness's own name for its own folder;
/// the wrapper or alias you run it with; else the folder's name, started by setting the
/// harness's variable.
fn naming(spec: &Spec, f: &Folder) -> (String, Option<String>, Option<String>) {
    if f.default {
        return (spec.harness.to_string(), spec.default_command.map(str::to_string), None);
    }
    let from_folder = folder_name(&f.dir);
    let mut named: Vec<&Named> = f.named.iter().collect();
    named.sort_by(|a, b| (a.name != from_folder, !a.wrapper, a.name.len(), &a.name).cmp(&(b.name != from_folder, !b.wrapper, b.name.len(), &b.name)));
    if let Some(n) = named.first() {
        return (sanitize(&n.name), Some(n.name.clone()), Some(n.via.clone()));
    }
    let command = match (spec.var, spec.program) {
        (Some((var, None)), Some(program)) => Some(format!("{var}={} {program}", shell_path(&f.dir))),
        (Some((var, Some(_))), Some(program)) => f.dir.parent().map(|p| format!("{var}={} {program}", shell_path(p))),
        _ => None,
    };
    (from_folder, command, None)
}

/// `~/.claude-work` → `claude-work`.
fn folder_name(dir: &Path) -> String {
    sanitize(dir.file_name().map(|n| n.to_string_lossy().trim_start_matches('.').to_string()).unwrap_or_default().as_str())
}

/// A name config.toml takes: letters, digits, dots, dashes and underscores.
fn sanitize(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '-' }).collect();
    s.trim_matches('-').to_string()
}

/// A folder as a shell word: `~/.claude-work`, or quoted with `$HOME` when it needs quotes,
/// since `~` doesn't expand inside them.
fn shell_path(dir: &Path) -> String {
    let t = tilde(dir);
    if t.chars().all(|c| c.is_ascii_alphanumeric() || "~/._-".contains(c)) {
        return t;
    }
    let body = t.strip_prefix("~/").map(|r| format!("$HOME/{r}")).unwrap_or(t);
    format!("\"{}\"", body.replace('\\', "\\\\").replace('"', "\\\"").replace('`', "\\`"))
}

/// A folder as config.toml and state.json compare it: resolved, without a trailing slash.
pub fn folder_key(home: &str) -> String {
    let p = expand(home.trim());
    let p = std::fs::canonicalize(&p).unwrap_or(p);
    p.to_string_lossy().trim_end_matches('/').to_string()
}

/// Accounts from folders: decided, named, and each name used once.
pub fn build(folders: &[Folder], installed: &dyn Fn(&str) -> bool, now: i64) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    for f in folders {
        let Some(spec) = specs().into_iter().find(|s| s.harness == f.harness) else { continue };
        let Some(state) = decide(f, installed(f.harness), now) else { continue };
        let (name, command, via) = naming(spec, f);
        out.push(Found {
            name: if name.is_empty() { spec.harness.to_string() } else { name },
            harness: f.harness.to_string(),
            home: tilde(&f.dir),
            command,
            via,
            state,
            signed_in: f.signed_in,
            folder_exists: f.exists,
            email: f.email.clone(),
            sign_in_note: f.sign_in_note.clone(),
            last_used: f.last_used,
            new: true,
        });
    }
    unique_names(out, &[])
}

/// Gives each account a name nothing else has, adding 2, 3… to repeats. A command that was
/// the name stays what it was, so it still runs the same thing.
fn unique_names(found: Vec<Found>, taken: &[String]) -> Vec<Found> {
    let mut used: HashSet<String> = taken.iter().cloned().collect();
    found
        .into_iter()
        .map(|mut f| {
            if used.contains(&f.name) {
                let base = f.name.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
                let base = if base.is_empty() { f.name.clone() } else { base };
                let next = (2..).map(|n| format!("{base}{n}")).find(|n| !used.contains(n)).unwrap_or_default();
                // An account with no command of its own is run by its name, which it's losing.
                let by_name = specs().iter().any(|s| s.harness == f.harness && s.program.is_some());
                if f.command.is_none() && by_name {
                    f.command = Some(f.name.clone());
                }
                f.name = next;
            }
            used.insert(f.name.clone());
            f
        })
        .collect()
}

/// The accounts config.toml doesn't list, named apart from the ones it does, each new unless
/// Codealong has read or been told about its folder (`known`, folder keys from state.json).
pub fn not_configured(found: Vec<Found>, accounts: &[AccountCfg], known: &[String]) -> Vec<Found> {
    let configured: HashSet<String> = accounts.iter().map(|a| folder_key(&a.home)).collect();
    let names: Vec<String> = accounts.iter().map(|a| a.name.clone()).collect();
    let rest: Vec<Found> = found
        .into_iter()
        .filter(|f| !configured.contains(&folder_key(&f.home)))
        .map(|mut f| {
            f.new = !known.contains(&folder_key(&f.home));
            f
        })
        .collect();
    unique_names(rest, &names)
}

// ---- Gathering ----

/// Every account on this Mac, configured or not.
pub fn find_all() -> Vec<Found> {
    let h = home();
    let path_dirs: Vec<PathBuf> = crate::procs::user_path().split(':').filter(|p| !p.is_empty()).map(PathBuf::from).collect();
    let decls = shell_decls(&h, &path_dirs);
    let folders = gather(&h, &decls);
    build(&folders, &|harness: &str| installed(harness, &h, &path_dirs), now_ms())
}

/// The accounts config.toml doesn't list, each new unless Codealong has read its folder or
/// been told to leave it: what Settings › Accounts and `codealong config show` list. An error
/// when config.toml doesn't parse: every account on this Mac would read as unlisted, and be
/// offered again.
pub fn unlisted() -> anyhow::Result<Vec<Found>> {
    let accounts = crate::config::current()?.accounts;
    Ok(not_configured(find_all(), &accounts, &crate::state::known_accounts()))
}

/// The accounts to start config.toml with: the ones in use or signed in.
pub fn first_run_accounts() -> Vec<AccountCfg> {
    find_all().into_iter().filter(|f| f.state != State::Idle).map(|f| f.account()).collect()
}

/// Declarations from your shell: wrapper scripts on the PATH and the startup files.
fn shell_decls(h: &Path, path_dirs: &[PathBuf]) -> Vec<(Decl, Named)> {
    let mut out = Vec::new();
    for (file, fish) in [(".zshenv", false), (".zprofile", false), (".zshrc", false), (".zlogin", false), (".bash_profile", false), (".bashrc", false), (".profile", false), (".config/fish/config.fish", true)] {
        let path = h.join(file);
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        for d in parse_rc(&text, fish) {
            let named = Named { name: d.name.clone().unwrap_or_default(), via: tilde(&path), wrapper: false };
            out.push((d, named));
        }
    }
    let mut seen = HashSet::new();
    for dir in path_dirs {
        let mine = dir.starts_with(h);
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            // Outside your home folder, only names that start like a harness's command.
            let harnessy = specs().iter().filter_map(|s| s.program).any(|p| name.starts_with(p) && name != p);
            if !(mine || harnessy) || !seen.insert(name.clone()) {
                continue;
            }
            let path = e.path();
            let Some(text) = small_script(&path) else { continue };
            for (var, value) in script_assignments(&text) {
                let named = Named { name: name.clone(), via: tilde(&path), wrapper: true };
                out.push((Decl { name: Some(name.clone()), var, value }, named));
            }
        }
    }
    out
}

/// A script's text, when the file is a small one that starts with `#!`.
fn small_script(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > WRAPPER_MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    bytes.starts_with(b"#!").then(|| String::from_utf8_lossy(&bytes).into_owned())
}

/// Every folder worth a look, with what it holds: each harness's own folder (or where an
/// export moves it), sibling folders like `~/.claude2`, and folders your shell names.
fn gather(h: &Path, decls: &[(Decl, Named)]) -> Vec<Folder> {
    let mut folders: Vec<Folder> = Vec::new();
    let mut add = |harness: &'static str, dir: PathBuf, default: bool, named: Option<Named>| {
        let key = folder_key(&dir.to_string_lossy());
        if let Some(f) = folders.iter_mut().find(|f| f.harness == harness && folder_key(&f.dir.to_string_lossy()) == key) {
            f.default |= default;
            f.named.extend(named);
        } else {
            folders.push(Folder { harness, dir, default, named: named.into_iter().collect(), ..Folder::default() });
        }
    };
    for spec in specs() {
        let moved = spec.var.and_then(|(var, sub)| {
            decls.iter().filter(|(d, _)| d.name.is_none() && d.var == var).find_map(|(d, _)| resolve(&d.value, h)).map(|p| match sub {
                Some(s) => p.join(s),
                None => p,
            })
        });
        add(spec.harness, moved.unwrap_or_else(|| expand(spec.default_dir)), true, None);
        if let Some(prefix) = spec.siblings {
            let mut names: Vec<String> = std::fs::read_dir(h)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with(prefix))
                .collect();
            names.sort();
            for n in names {
                let dir = h.join(&n);
                if dir.is_dir() {
                    add(spec.harness, dir, false, None);
                }
            }
        }
    }
    for (d, named) in decls {
        let (Some(_), Some(spec)) = (&d.name, spec_for_var(&d.var)) else { continue };
        let Some(dir) = resolve(&d.value, h) else { continue };
        let dir = match spec.var.and_then(|(_, sub)| sub) {
            Some(sub) => dir.join(sub),
            None => dir,
        };
        add(spec.harness, dir, false, Some(named.clone()));
    }
    for f in &mut folders {
        inspect(f);
    }
    folders
}

/// What a folder holds: sessions, a sign-in, and when it was last used.
fn inspect(f: &mut Folder) {
    let d = f.dir.clone();
    f.exists = d.is_dir();
    if !f.exists {
        return;
    }
    let info = crate::usage::describe(&AccountCfg { name: String::new(), harness: f.harness.to_string(), home: d.to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false });
    f.signed_in = info.signed_in;
    f.email = info.email;
    f.sign_in_note = info.sign_in_note;
    let used = crate::harness::named(f.harness).map(|h| h.folder_use(&d)).unwrap_or_default();
    f.has_data = used.has_data;
    f.last_used = used.last_used;
    f.signed_in |= used.signed_in;
}

/// Whether the harness is on this Mac: its command on the PATH or where its installer puts it,
/// or its app.
fn installed(harness: &str, h: &Path, path_dirs: &[PathBuf]) -> bool {
    let on_path = |name: &str| path_dirs.iter().any(|d| is_executable(&d.join(name)));
    crate::harness::named(harness).is_some_and(|a| a.installed(h, &on_path))
}

pub(crate) fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// An app in /Applications or ~/Applications.
pub(crate) fn app(bundle: &str) -> Option<PathBuf> {
    [PathBuf::from("/Applications"), home().join("Applications")].into_iter().map(|d| d.join(bundle)).find(|p| p.is_dir())
}

/// The Codex CLI inside the Codex app, which ships as ChatGPT.app now (an older Codex.app may
/// be there too). The ChatGPT chat app has the same name, so the app is known by its bundle id.
/// From 26.924 the CLI is a package under `codex-cli`; before, a single `codex` binary.
pub fn codex_app_cli() -> Option<PathBuf> {
    let apps = ["ChatGPT.app", "Codex.app"].iter().filter_map(|b| app(b)).filter(|a| bundle_id_is(a, crate::codex::APP_BUNDLE_ID));
    apps.flat_map(|a| ["Contents/Resources/codex-cli/bin/codex", "Contents/Resources/codex"].map(|p| a.join(p))).find(|p| p.is_file())
}

/// Whether an app's Info.plist names this bundle id. A binary plist keeps ASCII strings as they
/// are, so looking for the bytes works for either kind without parsing.
fn bundle_id_is(app: &Path, id: &str) -> bool {
    std::fs::read(app.join("Contents/Info.plist")).is_ok_and(|b| b.windows(id.len()).any(|w| w == id.as_bytes()))
}

/// Whether a command is on the PATH Codealong gives the programs it runs.
pub fn on_user_path(name: &str) -> bool {
    crate::procs::user_path().split(':').filter(|p| !p.is_empty()).any(|d| is_executable(&Path::new(d).join(name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(name: Option<&str>, var: &str, value: &str) -> Decl {
        Decl { name: name.map(str::to_string), var: var.into(), value: value.into() }
    }

    #[test]
    fn reads_aliases_functions_and_exports() {
        let rc = r#"
# a comment with CLAUDE_CONFIG_DIR=~/.nope
export PATH="$HOME/.local/bin:$PATH"
alias cw='CLAUDE_CONFIG_DIR=~/.claude-work claude'
alias c2="CLAUDE_CONFIG_DIR=\"$HOME/.claude2\" claude"
alias ll='ls -la'
cx() { CODEX_HOME=~/.codex-work codex "$@"; }
function claude-personal() {
  export CLAUDE_CONFIG_DIR="$HOME/.config/claude-personal"
  claude "$@"
}
oc2 () {
  XDG_DATA_HOME=~/.oc2 opencode "$@"
}
export CODEX_HOME=~/.config/codex
MY_CLAUDE_CONFIG_DIR=~/.decoy
"#;
        assert_eq!(
            parse_rc(rc, false),
            vec![
                decl(Some("cw"), "CLAUDE_CONFIG_DIR", "~/.claude-work"),
                decl(Some("c2"), "CLAUDE_CONFIG_DIR", "$HOME/.claude2"),
                decl(Some("cx"), "CODEX_HOME", "~/.codex-work"),
                decl(Some("claude-personal"), "CLAUDE_CONFIG_DIR", "$HOME/.config/claude-personal"),
                decl(Some("oc2"), "XDG_DATA_HOME", "~/.oc2"),
                decl(None, "CODEX_HOME", "~/.config/codex"),
            ]
        );
    }

    #[test]
    fn reads_fish() {
        let fish = "set -gx CLAUDE_CONFIG_DIR ~/.claude-main\nalias cw 'CLAUDE_CONFIG_DIR=~/.claude-work claude'\nfunction c3\n  if true\n    set -lx CLAUDE_CONFIG_DIR ~/.claude3\n  end\n  claude $argv\nend\nset -gx EDITOR vim\n";
        assert_eq!(
            parse_rc(fish, true),
            vec![decl(None, "CLAUDE_CONFIG_DIR", "~/.claude-main"), decl(Some("cw"), "CLAUDE_CONFIG_DIR", "~/.claude-work"), decl(Some("c3"), "CLAUDE_CONFIG_DIR", "~/.claude3")]
        );
    }

    #[test]
    fn reads_a_wrapper_script() {
        // This Mac's ~/.local/bin/claude2.
        let script = "#!/bin/sh\n\n# Keep Claude Code's login, settings, and session state separate from ~/.claude.\nexport CLAUDE_CONFIG_DIR=\"$HOME/.claude2\"\n\nexec /Users/me/.local/bin/claude \"$@\"\n";
        assert_eq!(script_assignments(script), vec![("CLAUDE_CONFIG_DIR".to_string(), "$HOME/.claude2".to_string())]);
        assert_eq!(script_assignments("#!/bin/sh\nenv CODEX_HOME=/opt/codex-team codex \"$@\""), vec![("CODEX_HOME".to_string(), "/opt/codex-team".to_string())]);
        // A fish wrapper, and a comment that mentions the variable.
        assert_eq!(script_assignments("#!/usr/bin/env fish\n# CLAUDE_CONFIG_DIR=~/.old\nset -x CLAUDE_CONFIG_DIR ~/.claude-fish\nclaude $argv"), vec![("CLAUDE_CONFIG_DIR".to_string(), "~/.claude-fish".to_string())]);
    }

    #[test]
    fn resolves_only_what_it_can() {
        let h = Path::new("/Users/me");
        assert_eq!(resolve("~/.claude2", h), Some(PathBuf::from("/Users/me/.claude2")));
        assert_eq!(resolve("$HOME/.claude2/", h), Some(PathBuf::from("/Users/me/.claude2")));
        assert_eq!(resolve("${HOME}/x", h), Some(PathBuf::from("/Users/me/x")));
        assert_eq!(resolve("/opt/codex", h), Some(PathBuf::from("/opt/codex")));
        assert_eq!(resolve("$XDG_CONFIG_HOME/claude", h), None);
        assert_eq!(resolve("relative/dir", h), None);
    }

    fn folder(harness: &'static str, dir: &str) -> Folder {
        Folder { harness, dir: PathBuf::from(dir), exists: true, ..Folder::default() }
    }

    const NOW: i64 = 1_800_000_000_000;

    #[test]
    fn decides_what_counts() {
        let used = |mut f: Folder, days: i64| {
            f.has_data = true;
            f.last_used = Some(NOW - days * DAY_MS);
            f
        };
        // In use, whatever else.
        assert_eq!(decide(&used(folder("claude", "/h/.claude-work"), 3), false, NOW), Some(State::Active));
        // A sibling folder with old sessions and no sign-in is someone's backup, not an account.
        assert_eq!(decide(&used(folder("claude", "/h/.claude-old"), 90), true, NOW), None);
        // Signed in but quiet.
        let mut quiet = used(folder("claude", "/h/.claude3"), 90);
        quiet.signed_in = true;
        assert_eq!(decide(&quiet, true, NOW), Some(State::SignedIn));
        // The harness's own folder counts once it's installed, even before it ever ran.
        let mut own = folder("opencode", "/h/.local/share/opencode");
        own.default = true;
        own.exists = false;
        assert_eq!(decide(&own, true, NOW), Some(State::Idle));
        assert_eq!(decide(&own, false, NOW), None);
        // A folder your shell names counts too.
        let mut named = folder("claude", "/h/.config/claude-personal");
        named.named.push(Named { name: "cp".into(), via: "~/.zshrc".into(), wrapper: false });
        assert_eq!(decide(&named, true, NOW), Some(State::Idle));
    }

    #[test]
    fn names_accounts_the_way_you_run_them() {
        let spec = specs()[0];
        let mut own = folder("claude", "/Users/me/.claude");
        own.default = true;
        assert_eq!(naming(spec, &own), ("claude".into(), Some("claude".into()), None));
        // Your wrapper names it and starts it.
        let mut wrapped = folder("claude", "/Users/me/.claude2");
        wrapped.named = vec![
            Named { name: "c2".into(), via: "~/.zshrc".into(), wrapper: false },
            Named { name: "claude2".into(), via: "~/.local/bin/claude2".into(), wrapper: true },
        ];
        assert_eq!(naming(spec, &wrapped), ("claude2".into(), Some("claude2".into()), Some("~/.local/bin/claude2".into())));
        // Nothing runs it: the folder's name, started with the variable set.
        let bare = folder("claude", &home().join(".claude-work").to_string_lossy());
        assert_eq!(naming(spec, &bare), ("claude-work".into(), Some("CLAUDE_CONFIG_DIR=~/.claude-work claude".into()), None));
        let codex = folder("codex", &home().join(".codex2").to_string_lossy());
        assert_eq!(naming(specs()[1], &codex).1, Some("CODEX_HOME=~/.codex2 codex".into()));
        // OpenCode's variable is the folder's parent.
        let oc = folder("opencode", &home().join(".oc2/opencode").to_string_lossy());
        assert_eq!(naming(specs()[2], &oc).1, Some("XDG_DATA_HOME=~/.oc2 opencode".into()));
        // A folder that needs quotes gets $HOME, since ~ doesn't expand inside them.
        assert_eq!(shell_path(&home().join("My Claude")), "\"$HOME/My Claude\"");
    }

    #[test]
    fn builds_accounts_with_names_used_once() {
        let installed = |_: &str| true;
        let mut own = folder("claude", "/Users/me/.claude");
        own.default = true;
        own.has_data = true;
        own.last_used = Some(NOW - DAY_MS);
        own.signed_in = true;
        let mut work = folder("claude", "/Users/me/.claude-work");
        work.signed_in = true;
        let mut codex = folder("codex", "/Users/me/.codex");
        codex.default = true;
        let found = build(&[own, work, codex], &installed, NOW);
        let summary: Vec<(&str, State)> = found.iter().map(|f| (f.name.as_str(), f.state)).collect();
        assert_eq!(summary, vec![("claude", State::Active), ("claude-work", State::SignedIn), ("codex", State::Idle)]);

        // Against config.toml: listed folders drop out, clashing names get a number and keep
        // their command, and folders Codealong has read before aren't new.
        let accounts = vec![AccountCfg { name: "claude-work".into(), harness: "claude".into(), home: "/Users/me/.claude-work-old".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false }];
        let rest = not_configured(found, &accounts, &["/Users/me/.codex".to_string()]);
        let summary: Vec<(&str, Option<&str>, bool)> = rest.iter().map(|f| (f.name.as_str(), f.command.as_deref(), f.new)).collect();
        assert_eq!(
            summary,
            vec![("claude", Some("claude"), true), ("claude-work2", Some("CLAUDE_CONFIG_DIR=/Users/me/.claude-work claude"), true), ("codex", None, false)]
        );
        let rest = not_configured(rest, &[AccountCfg { name: "x".into(), harness: "claude".into(), home: "/Users/me/.claude/".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false }], &[]);
        assert!(rest.iter().all(|f| f.home != "/Users/me/.claude"));
    }

    /// Prints what this Mac has, configured or not: `cargo test this_macs_accounts -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn this_macs_accounts() {
        for f in find_all() {
            println!("{:<12} {:<11} {:<28} {:?} signed_in={} command={:?} via={:?}", f.name, f.harness, f.home, f.state, f.signed_in, f.command, f.via);
            println!("{:<12} signs in with: {}", "", crate::usage::sign_in_command(&f.account()));
        }
    }

    #[test]
    fn tells_a_function_from_a_call() {
        assert_eq!(function_start("cw() {"), Some("cw".into()));
        assert_eq!(function_start("function claude-work {"), Some("claude-work".into()));
        assert_eq!(function_start("function c3"), Some("c3".into()));
        assert_eq!(function_start("echo (hi)"), None);
        assert_eq!(function_start("if [ -f x ]; then"), None);
        assert_eq!(alias("alias -g cw='x'"), Some(("cw".into(), "'x'".into())));
        assert_eq!(alias("aliases=3"), None);
    }
}

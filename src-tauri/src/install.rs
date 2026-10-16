//! Putting Codealong where each account's agents read it: the Codealong plugin in each Claude
//! Code account, and the agent skill where the other harnesses read personal skills.
//!
//! The skill (`skills/codealong` in the repo, `Contents/Resources/skills/codealong` in the
//! bundle) is copied, not linked: not every harness follows links, and the bundle moves when the
//! app is rebuilt. A copy carries `.codealong-version`, the hash of what was copied; at launch
//! the app refreshes copies it made when the skill changes, and never touches a folder it didn't
//! make. A copy whose files no longer hash to its marker was edited by someone, and is left
//! alone until they ask for it to be replaced.
//!
//! The plugin (`plugin/` in the repo, with the marketplace that lists it in `.claude-plugin/`)
//! carries the same skill, the `codealong` command for the Bash tool, and the hooks that hand the
//! app a session's permission prompts. Claude Code installs it with `claude plugin` from a
//! marketplace folder the app keeps in its own folder, copied from the one it ships: the plugin
//! is then the one this app answers, needs no network or access to the repo, and updates when
//! the app does (`claude plugin update` rereads a folder marketplace). A Claude Code account
//! with the plugin gets no skill copy, and one Codealong made before is taken away: two copies of
//! the skill would both show.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::config::{self, AccountCfg};
use crate::util::home;

const MARKER: &str = ".codealong-version";

/// This binary's real path. `current_exe` gives the path the command was run by, which is the
/// symlink `link_command` made when it's run as `codealong`; resolved, so a link is never made
/// to point at itself and the bundle's Resources are found through it.
fn this_binary() -> std::io::Result<PathBuf> {
    std::env::current_exe().and_then(std::fs::canonicalize)
}

/// The skill folder shipped with this binary, when it can be found.
pub fn skill_source() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = this_binary() {
        // Codealong.app/Contents/MacOS/codealong → Codealong.app/Contents/Resources/skills/codealong
        if let Some(contents) = exe.parent().and_then(Path::parent) {
            candidates.push(contents.join("Resources/skills/codealong"));
        }
    }
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../skills/codealong"));
    candidates.into_iter().find(|p| p.join("SKILL.md").is_file()).and_then(|p| std::fs::canonicalize(p).ok())
}

/// Where the account's harness reads personal skills, or None when Codealong doesn't know.
pub fn skill_dir(a: &AccountCfg) -> Option<PathBuf> {
    Some(crate::harness::of(a)?.skills_dir(a)?.join("codealong"))
}

/// The hash of a skill folder's files, so a copy can say what it's a copy of.
pub fn skill_hash(dir: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect(dir, dir, &mut files)?;
    files.sort();
    let mut h = Sha256::new();
    for rel in &files {
        h.update(rel.as_bytes());
        h.update([0]);
        h.update(std::fs::read(dir.join(rel))?);
        h.update([0]);
    }
    Ok(format!("{:x}", h.finalize()))
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if name == MARKER || name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            collect(root, &p, out)?;
        } else if let Ok(rel) = p.strip_prefix(root) {
            out.push(rel.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

/// Copies the skill to `dst`, replacing a copy Codealong made before. A folder there that
/// Codealong didn't make is left alone, with an error.
pub fn copy_skill(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() && !dst.join(MARKER).is_file() {
        bail!("{} exists and Codealong didn't put it there; move it away to install the skill.", dst.display());
    }
    let hash = skill_hash(src)?;
    let parent = dst.parent().context("the skills folder has no parent")?;
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(".codealong-new");
    let _ = std::fs::remove_dir_all(&tmp);
    copy_dir(src, &tmp)?;
    std::fs::write(tmp.join(MARKER), format!("{hash}\n"))?;
    if dst.exists() {
        std::fs::remove_dir_all(dst)?;
    }
    std::fs::rename(&tmp, dst).with_context(|| format!("moving the skill into {}", dst.display()))?;
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)?.flatten() {
        let name = e.file_name();
        let s = name.to_string_lossy();
        if s == MARKER || s.starts_with('.') {
            continue;
        }
        let from = e.path();
        let to = dst.join(&name);
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).with_context(|| format!("copying {}", from.display()))?;
        }
    }
    Ok(())
}

/// A copy of the skill, as it stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CopyState {
    /// Codealong made the folder: it has the marker.
    pub installed: bool,
    /// It's a copy of the skill in this build.
    pub current: bool,
    /// Its files changed since Codealong copied them, so updates leave it alone.
    pub edited: bool,
}

impl CopyState {
    /// A copy Codealong made, of an older skill, that nobody changed: safe to replace.
    pub fn stale(self) -> bool {
        self.installed && !self.current && !self.edited
    }
}

/// Whether a copy Codealong made is at `dst`, whether it's current, and whether it was edited.
pub fn copy_state(src: &Path, dst: &Path) -> CopyState {
    let Ok(marker) = std::fs::read_to_string(dst.join(MARKER)) else { return CopyState::default() };
    let marker = marker.trim();
    let current = skill_hash(src).is_ok_and(|h| h == marker);
    let edited = skill_hash(dst).is_ok_and(|h| h != marker);
    CopyState { installed: true, current, edited }
}

/// At launch: the plugin Codealong installed in a Claude Code account is brought up to date
/// with the one shipped in this build, and a skill copy Codealong made there before goes, since
/// the plugin carries the skill. Then every skill copy Codealong made is brought up to date,
/// unless someone edited it. Accounts and folders without either are left as they are.
pub fn refresh_installed() {
    // Run as the app starts, which always has settings; the command with a config.toml that
    // doesn't parse has no accounts to bring up to date.
    let Ok(cfg) = config::current() else { return };
    let shipped = shipped_version();
    for a in cfg.accounts.iter().filter(|a| takes_plugin(a)) {
        let home = a.home_path();
        let state = plugin_state(&home);
        if state.version.is_none() {
            continue;
        }
        if state.from.is_none() && state.version != shipped {
            if let Err(e) = stage_marketplace().and_then(|_| plugin_verb(&home, "update")) {
                eprintln!("Couldn't update the plugin in {}: {e:#}", home.display());
            }
        }
        // Installed from Codealong's folder and not turned off by anyone, but not on either: the
        // write that turned it on was lost.
        if state.from.is_none() && enabled_entry(&home).is_none() {
            if let Err(e) = plugin_verb(&home, "enable") {
                eprintln!("Couldn't turn the plugin on in {}: {e:#}", home.display());
            }
        }
        if let Err(e) = remove_skill_copy(a) {
            eprintln!("Couldn't take the skill copy out of {}: {e:#}", home.display());
        }
    }
    let Some(src) = skill_source() else { return };
    let known = crate::state::known_accounts();
    for dst in refresh_targets(&cfg.accounts, &known) {
        if copy_state(&src, &dst).stale() {
            if let Err(e) = copy_skill(&src, &dst) {
                eprintln!("Couldn't refresh the skill in {}: {e:#}", dst.display());
            }
        }
    }
}

/// Where copies Codealong made can be: each account's skills folder, and the `skills` folder
/// of every account folder config.toml listed before (Claude Code and Codex keep skills in
/// their own folder), so an account taken out of config.toml still gets updates.
fn refresh_targets(accounts: &[AccountCfg], known: &[String]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = accounts.iter().filter_map(skill_dir).collect();
    for k in known {
        let dir = Path::new(k).join("skills/codealong");
        if !out.contains(&dir) {
            out.push(dir);
        }
    }
    out
}

/// A symlink to this binary in a folder on PATH: the one asked for, else /usr/local/bin when
/// it's writable, else ~/.local/bin (made if needed).
pub fn link_command(dir: Option<PathBuf>) -> Result<PathBuf> {
    let exe = this_binary()?;
    let dir = match dir {
        Some(d) => d,
        None => {
            let usr = PathBuf::from("/usr/local/bin");
            if usr.is_dir() && std::fs::metadata(&usr).map(|m| !m.permissions().readonly()).unwrap_or(false) && writable(&usr) {
                usr
            } else {
                home().join(".local/bin")
            }
        }
    };
    std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
    let link = dir.join("codealong");
    if let Ok(target) = std::fs::read_link(&link) {
        if target == exe {
            return Ok(link);
        }
        std::fs::remove_file(&link)?;
    } else if link.exists() {
        bail!("{} exists and isn't a symlink; move it out of the way.", link.display());
    }
    std::os::unix::fs::symlink(&exe, &link).with_context(|| format!("linking {}", link.display()))?;
    let on_path = std::env::var("PATH").unwrap_or_default().split(':').any(|p| Path::new(p) == dir);
    if !on_path {
        eprintln!("{} isn't on PATH in this shell; add it, or run the command by its path.", dir.display());
    }
    Ok(link)
}

/// The `codealong` command follows the app that's running. A link Codealong made, to the binary
/// inside a Codealong bundle, that points at another copy (the one in Applications, or Codealong
/// Dev) is pointed at this one when it opens: the command and the app it talks to are then the
/// same build. A link of anyone else's making is left as it is, and so is everything when this
/// binary isn't in a bundle (cargo run, tests).
pub fn follow_command_link() {
    let Ok(exe) = this_binary() else { return };
    follow_links_in(&[PathBuf::from("/usr/local/bin"), home().join(".local/bin")], &exe);
}

fn follow_links_in(dirs: &[PathBuf], exe: &Path) {
    if !in_a_bundle(exe) {
        return;
    }
    for dir in dirs {
        let link = dir.join("codealong");
        let Ok(target) = std::fs::read_link(&link) else { continue };
        if target == exe || !in_a_bundle(&target) {
            continue;
        }
        // Made beside it and renamed over it, so the command is never missing in between.
        let next = dir.join(".codealong-link");
        let _ = std::fs::remove_file(&next);
        if std::os::unix::fs::symlink(exe, &next).is_err() || std::fs::rename(&next, &link).is_err() {
            let _ = std::fs::remove_file(&next);
            eprintln!("Couldn't point {} at {}", link.display(), exe.display());
        }
    }
}

/// `…/<name>.app/Contents/MacOS/codealong`.
fn in_a_bundle(exe: &Path) -> bool {
    exe.ends_with("Contents/MacOS/codealong") && exe.ancestors().nth(3).and_then(Path::extension).is_some_and(|e| e == "app")
}

/// What an app opened by hand keeps of its opener's environment, besides the home and user: the
/// settings it reads from there (devicekey.rs, updates.rs), for a copy started to try something.
const CARRIED: [&str; 2] = ["CODEALONG_DEVICE_KEY", "CODEALONG_UPDATE_URL"];

/// Typing `codealong` with nothing after it, in a terminal or an agent's shell, opens the app as
/// the Dock does: this copy hands its bundle to macOS with the Dock's environment and leaves, and
/// the copy macOS opens takes over from any that's running (server::take_over). Run as the
/// shell's child, the app had that shell's environment (an agent's session ID, message socket and
/// token, its editor's variables, a PATH of its own) and passed it to every process it started,
/// long after the session had ended; everything it ran had that session above it, which the
/// command took for the session running it (cli.rs `session_above`); and macOS didn't know it as
/// the app (2026-10-14). `open` hands its own environment to the app it opens, so it gets the
/// Dock's, and `-n` opens a new copy: macOS takes a copy that quit while processes it started
/// still ran (lsappinfo: "exited-with-subordinates") for one still open, and a plain `open` only
/// brings that forward. relaunch.sh opens the app this way too. The exit code to leave with once
/// it's handed over; None opens the app here: as macOS opened it, as the copy before it restarts
/// it for an update (Tauri's restart runs this binary), as the copy it was handed to, or as a
/// binary outside a bundle (cargo run, tauri dev), which has no app to open.
pub fn open_as_the_dock_does() -> Option<i32> {
    // The copy it's handed to opens here whatever else it finds, so however the rest misreads a
    // launch, the app is never handed on from copy to copy. Taken out of the environment, so
    // nothing the app starts inherits it.
    if std::env::var_os(HANDED_OVER).is_some() {
        std::env::remove_var(HANDED_OVER);
        return None;
    }
    let exe = this_binary().ok()?;
    if !in_a_bundle(&exe) {
        return None;
    }
    let pid = std::process::id();
    // LaunchServices knows the copy it opened from its first moment, before it has said it's
    // running, and nothing else: not one a shell started, nor one started in the background whose
    // starter quit, which launchd adopts with whatever environment it had (checked 2026-10-14).
    let by_macos = crate::os::apps::opened_by_macos(pid);
    let (ppid, me) = crate::procs::parent_and_name(pid)?;
    let parent = crate::procs::parent_and_name(ppid).map(|(_, name)| (ppid, name));
    if !opened_by_hand(by_macos, parent.as_ref().map(|(pid, name)| (*pid, name.as_str())), &me) {
        return None;
    }
    let bundle = exe.ancestors().nth(3)?;
    let env = dock_environment(|name| std::env::var(name).ok(), &crate::launch::login_shell(), crate::os::dirs::user_temp_dir().as_deref(), launchd_ssh_agent().as_deref());
    let open = || std::process::Command::new("/usr/bin/open").arg("-n").arg(bundle).env_clear().envs(env.iter().cloned()).env(HANDED_OVER, "1").status();
    // macOS turns an open away while the app's last copy is still quitting (error -600), as the
    // copy an update restarted finds when that copy has only just gone: once more, a moment on.
    let opened = match open() {
        Ok(status) if !status.success() => {
            std::thread::sleep(std::time::Duration::from_secs(2));
            open()
        }
        first => first,
    };
    match opened {
        Ok(status) => Some(status.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("Couldn't open {}: {e}", bundle.display());
            Some(1)
        }
    }
}

/// Set for the copy `open_as_the_dock_does` hands the app to.
const HANDED_OVER: &str = "CODEALONG_HANDED_OVER";

/// Whether a copy was opened by hand: not by macOS (`by_macos`), nor by the copy before it, which
/// is its parent while it quits after restarting it for an update, from its parent's pid and name
/// (None once it's gone) and its own name. A copy an update restarted after the one before it
/// has gone is launchd's, and is handed to macOS as one started in the background is.
fn opened_by_hand(by_macos: bool, parent: Option<(u32, &str)>, me: &str) -> bool {
    !by_macos && !parent.is_some_and(|(pid, name)| pid != 1 && name == me)
}

/// The environment the Dock gives an app, for one opened in its place: the opener's home, user,
/// text encoding and `CARRIED` settings (`caller`), the login shell, the system's PATH (the app
/// adds the login shell's, procs::user_path), the per-user temporary folder and launchd's SSH
/// agent, whatever the opener's shell set those to.
fn dock_environment(caller: impl Fn(&str) -> Option<String>, shell: &str, temp: Option<&str>, ssh_agent: Option<&str>) -> Vec<(String, String)> {
    let mut env = vec![("SHELL".to_string(), shell.to_string()), ("PATH".to_string(), "/usr/bin:/bin:/usr/sbin:/sbin".to_string())];
    for name in ["HOME", "USER", "LOGNAME", "__CF_USER_TEXT_ENCODING"].into_iter().chain(CARRIED) {
        if let Some(value) = caller(name) {
            env.push((name.to_string(), value));
        }
    }
    env.extend(temp.map(|t| ("TMPDIR".to_string(), t.to_string())));
    env.extend(ssh_agent.map(|s| ("SSH_AUTH_SOCK".to_string(), s.to_string())));
    env
}

/// The SSH agent launchd gives apps, where a shell may point SSH_AUTH_SOCK at another.
fn launchd_ssh_agent() -> Option<String> {
    let out = std::process::Command::new("/bin/launchctl").args(["getenv", "SSH_AUTH_SOCK"]).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".codealong-probe-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// The link `link_command` made, when one is in place and points at this binary.
pub fn command_link() -> Option<PathBuf> {
    let exe = this_binary().ok()?;
    [PathBuf::from("/usr/local/bin"), home().join(".local/bin")].into_iter().map(|d| d.join("codealong")).find(|l| std::fs::read_link(l).is_ok_and(|t| t == exe))
}

/// Claude Code's name for the plugin, and for the marketplace it comes from.
pub const PLUGIN: &str = "codealong@codealong";
const MARKETPLACE: &str = "codealong";

/// The marketplace shipped with this binary: a folder with `.claude-plugin/marketplace.json` and
/// the plugin in `plugin/`. The bundle has it in `Resources/plugin-marketplace`; a build from
/// the repo uses the repo, which is one.
pub fn plugin_source() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = this_binary() {
        if let Some(contents) = exe.parent().and_then(Path::parent) {
            candidates.push(contents.join("Resources/plugin-marketplace"));
        }
    }
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join(".."));
    candidates
        .into_iter()
        .find(|p| p.join(".claude-plugin/marketplace.json").is_file() && p.join("plugin/.claude-plugin/plugin.json").is_file())
        .and_then(|p| std::fs::canonicalize(p).ok())
}

/// The plugin's version in a marketplace folder: what Claude Code compares to update it.
pub fn plugin_version(marketplace: &Path) -> Option<String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(marketplace.join("plugin/.claude-plugin/plugin.json")).ok()?).ok()?;
    v.get("version")?.as_str().map(String::from)
}

/// Where Codealong keeps the marketplace Claude Code installs the plugin from: in its own
/// folder, since the bundle moves when the app is rebuilt or replaced.
pub fn marketplace_dir() -> PathBuf {
    config::support_dir().join("plugin-marketplace")
}

/// The parts of a marketplace folder that are copied.
const MARKETPLACE_PARTS: [&str; 2] = [".claude-plugin", "plugin"];

/// The version this build's plugin has once staged: plugin.json's, with a hash of the plugin's
/// files after a `+`. Claude Code updates a plugin whenever its version string changes (checked
/// on 2.1.284), so the plugin Codealong installs updates whenever its files do, the skill
/// included, without anyone bumping the version.
pub fn shipped_version() -> Option<String> {
    let src = plugin_source()?;
    stamped_version(&src, &marketplace_files(&src, skill_source().as_deref()).ok()?)
}

fn stamped_version(src: &Path, files: &[(String, PathBuf)]) -> Option<String> {
    let base = plugin_version(src)?;
    let mut h = Sha256::new();
    for (rel, from) in files {
        h.update(rel.as_bytes());
        h.update([0]);
        h.update(std::fs::read(from).ok()?);
        h.update([0]);
    }
    let hash = format!("{:x}", h.finalize());
    Some(format!("{}+{}", base.split('+').next().unwrap_or(&base), &hash[..8]))
}

/// Puts this build's marketplace in Codealong's folder, unless the same version is there, and
/// returns that version.
fn stage_marketplace() -> Result<String> {
    let src = plugin_source().context("The plugin isn't next to this build of Codealong.")?;
    stage_marketplace_from(&src, skill_source().as_deref(), &marketplace_dir())
}

/// The marketplace's files, each by its place in the staged folder and the file it's copied
/// from, links followed (the repo's plugin links its skill to `skills/codealong`). `skill` is
/// the plugin's skill when it has none of its own: the bundle carries the skill once, in
/// `Resources/skills`.
fn marketplace_files(src: &Path, skill: Option<&Path>) -> Result<Vec<(String, PathBuf)>> {
    let mut files = Vec::new();
    for part in MARKETPLACE_PARTS {
        collect_files(&src.join(part), part, &mut files)?;
    }
    if !files.iter().any(|(rel, _)| rel == "plugin/skills/codealong/SKILL.md") {
        if let Some(skill) = skill {
            collect_files(skill, "plugin/skills/codealong", &mut files)?;
        }
    }
    files.sort();
    Ok(files)
}

fn collect_files(dir: &Path, rel: &str, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name == ".DS_Store" {
            continue;
        }
        let (p, r) = (e.path(), format!("{rel}/{name}"));
        if p.is_dir() {
            collect_files(&p, &r, out)?;
        } else {
            out.push((r, p));
        }
    }
    Ok(())
}

fn stage_marketplace_from(src: &Path, skill: Option<&Path>, dst: &Path) -> Result<String> {
    let files = marketplace_files(src, skill)?;
    let version = stamped_version(src, &files).context("The plugin has no version in its plugin.json.")?;
    if plugin_version(dst).as_deref() == Some(version.as_str()) {
        return Ok(version);
    }
    let tmp = dst.with_extension("new");
    let _ = std::fs::remove_dir_all(&tmp);
    for (rel, from) in &files {
        let to = tmp.join(rel);
        std::fs::create_dir_all(to.parent().context("a file with no folder")?)?;
        std::fs::copy(from, &to).with_context(|| format!("copying {}", from.display()))?;
    }
    // The stamped version, where Claude Code reads it: the plugin's manifest and the marketplace's
    // entry for it.
    let set = |file: &Path, f: &dyn Fn(&mut Value)| -> Result<()> {
        let mut v: Value = serde_json::from_str(&std::fs::read_to_string(file)?)?;
        f(&mut v);
        std::fs::write(file, serde_json::to_string_pretty(&v)? + "\n")?;
        Ok(())
    };
    set(&tmp.join("plugin/.claude-plugin/plugin.json"), &|v| v["version"] = Value::String(version.clone()))?;
    set(&tmp.join(".claude-plugin/marketplace.json"), &|v| {
        for p in v.get_mut("plugins").and_then(Value::as_array_mut).into_iter().flatten() {
            if p.get("name").and_then(Value::as_str) == Some("codealong") {
                p["version"] = Value::String(version.clone());
            }
        }
    })?;
    // The bundle's resources can come without the command's execute bit.
    if let Ok(bins) = std::fs::read_dir(tmp.join("plugin/bin")) {
        use std::os::unix::fs::PermissionsExt;
        for b in bins.flatten() {
            std::fs::set_permissions(b.path(), std::fs::Permissions::from_mode(0o755))?;
        }
    }
    if dst.exists() {
        std::fs::remove_dir_all(dst)?;
    }
    std::fs::rename(&tmp, dst).with_context(|| format!("moving the plugin into {}", dst.display()))?;
    Ok(version)
}

/// The plugin in a Claude Code account, as its own files say.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginState {
    /// The version installed; None when it isn't.
    pub version: Option<String>,
    /// Installed but not turned on: its hooks and skill don't run. Claude Code runs a plugin only
    /// while settings.json's `enabledPlugins` says true: false is turned off (`/plugin`), and none
    /// at all is a write that was lost (another process writing the file over, it looked like).
    pub disabled: bool,
    /// Where its marketplace is when it isn't Codealong's folder: a GitHub repo (`owner/name`), a
    /// URL or a folder. Codealong updates only the plugin it installed from its own.
    pub from: Option<String>,
}

/// Reads the plugin's state from the account's files: `plugins/installed_plugins.json`,
/// `settings.json`'s `enabledPlugins` and `plugins/known_marketplaces.json`.
pub fn plugin_state(home: &Path) -> PluginState {
    plugin_state_in(home, &marketplace_dir())
}

fn plugin_state_in(home: &Path, ours: &Path) -> PluginState {
    let read = |p: PathBuf| std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let version = read(home.join("plugins/installed_plugins.json")).and_then(|v| {
        let entries = v.get("plugins")?.get(PLUGIN)?.as_array()?.clone();
        entries.iter().find(|e| e.get("scope").and_then(Value::as_str) == Some("user"))?.get("version")?.as_str().map(String::from)
    });
    let disabled = version.is_some() && enabled_entry(home) != Some(true);
    let from = marketplace_source(home).filter(|s| !is_ours(s, ours)).map(|s| {
        let f = |k: &str| s.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        match f("source").as_str() {
            "github" => f("repo"),
            "directory" | "file" => crate::util::tilde(Path::new(&f("path"))),
            "git" | "url" => f("url"),
            other => other.to_string(),
        }
    });
    PluginState { version: version.clone(), disabled, from: from.filter(|_| version.is_some()) }
}

/// What settings.json's `enabledPlugins` says of the plugin: true, false, or nothing.
fn enabled_entry(home: &Path) -> Option<bool> {
    let s: Value = serde_json::from_str(&std::fs::read_to_string(home.join("settings.json")).ok()?).ok()?;
    s.get("enabledPlugins")?.get(PLUGIN)?.as_bool()
}

/// Where the account's `codealong` marketplace comes from (`plugins/known_marketplaces.json`).
fn marketplace_source(home: &Path) -> Option<Value> {
    let known: Value = serde_json::from_str(&std::fs::read_to_string(home.join("plugins/known_marketplaces.json")).ok()?).ok()?;
    known.get(MARKETPLACE)?.get("source").cloned()
}

fn is_ours(source: &Value, ours: &Path) -> bool {
    let path = source.get("path").and_then(Value::as_str).map(Path::new);
    source.get("source").and_then(Value::as_str) == Some("directory") && path.is_some_and(|p| p == ours || std::fs::canonicalize(p).ok() == std::fs::canonicalize(ours).ok())
}

/// Whether Codealong goes into the account as a plugin rather than a skill copy: its harness has
/// one (`Adapter::installs_plugin`).
pub fn takes_plugin(a: &AccountCfg) -> bool {
    crate::harness::of(a).is_some_and(|h| h.installs_plugin())
}

/// Runs `claude plugin <args>` for an account: with its folder as `CLAUDE_CONFIG_DIR`, or without
/// one for `~/.claude` (claude/mod.rs `usual_folder`).
fn claude_plugin(home: &Path, args: &[&str]) -> Result<String> {
    let config = format!("CLAUDE_CONFIG_DIR={}", home.display());
    let mut all: Vec<&str> = if crate::claude::usual_folder(home) { vec!["-u", "CLAUDE_CONFIG_DIR"] } else { vec![config.as_str()] };
    all.extend(["claude", "plugin"]);
    all.extend(args);
    crate::procs::run("/usr/bin/env", &all, Some(&crate::util::home()), std::time::Duration::from_secs(120))
}

/// Runs `claude plugin <verb>` on the plugin as the account's own (user scope), the install
/// `plugin_state` reads. Without a scope Claude Code goes by the folder it runs in, and takes the
/// install made for that folder's project when there is one: an account here had one for the
/// home folder beside its own, and Update brought that one up to date every time while the
/// account's own stayed behind, still saying an update was available (checked on 2.1.289,
/// 2026-10-05). Such an install isn't taken away from here: in the home folder it's recorded in
/// `~/.claude/settings.json`, which is also the `~/.claude` account's own settings, and
/// uninstalling it turns the plugin off there.
fn plugin_verb(home: &Path, verb: &str) -> Result<String> {
    claude_plugin(home, &[verb, PLUGIN, "--scope", "user"])
}

/// Installs the plugin in a Claude Code account from Codealong's marketplace, or brings it up
/// to date there, and turns it on; then takes away the skill copy Codealong made there before.
/// A `codealong` marketplace from elsewhere (the GitHub repo) is replaced, taking its copy of the
/// plugin with it.
pub fn install_plugin(a: &AccountCfg) -> Result<()> {
    stage_marketplace()?;
    let dir = marketplace_dir();
    let home = a.home_path();
    let known = marketplace_source(&home);
    let ours = known.as_ref().is_some_and(|s| is_ours(s, &dir));
    if known.is_some() && !ours {
        claude_plugin(&home, &["marketplace", "remove", MARKETPLACE])?;
    }
    if !ours {
        claude_plugin(&home, &["marketplace", "add", &dir.to_string_lossy()])?;
    }
    if plugin_state(&home).version.is_none() {
        plugin_verb(&home, "install")?;
    } else {
        plugin_verb(&home, "update")?;
    }
    if plugin_state(&home).disabled {
        plugin_verb(&home, "enable")?;
    }
    remove_skill_copy(a)
}

/// Takes away the skill copy Codealong made in an account's skills folder, unless someone
/// edited it.
fn remove_skill_copy(a: &AccountCfg) -> Result<()> {
    let Some(dst) = skill_dir(a) else { return Ok(()) };
    let state = copy_state(&skill_source().unwrap_or_default(), &dst);
    if state.installed && !state.edited {
        std::fs::remove_dir_all(&dst).with_context(|| format!("removing {}", dst.display()))?;
    }
    Ok(())
}

/// What Settings › Agents shows: the command link and the skill per account.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsStatus {
    pub command: CommandStatus,
    /// Whether the skill folder is next to this binary at all.
    pub skill_found: bool,
    /// The plugin version this build carries (`shipped_version`), when the plugin is next to it.
    pub plugin_version: Option<String>,
    pub accounts: Vec<AccountSkill>,
    /// What went wrong in an install pass, one line per failure; empty from `status`.
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandStatus {
    /// The symlink `link_command` made, when it's in place.
    pub link: Option<String>,
    /// This binary, which the link points at.
    pub target: String,
    /// Whether the link's folder is on the login shell's PATH, so a terminal finds it.
    pub on_path: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSkill {
    pub name: String,
    pub harness: String,
    /// The skill's folder for this account; None when the harness has none Codealong knows.
    pub path: Option<String>,
    pub installed: bool,
    pub current: bool,
    /// Someone changed the copy; updates leave it alone until it's replaced from here.
    pub edited: bool,
    /// For a Claude Code account, the plugin, which is what Codealong installs there; the skill
    /// fields then say whether a copy Codealong made before is still in its skills folder.
    pub plugin: Option<PluginState>,
}

/// Whether a folder is on a PATH string, by exact entry.
fn dir_on_path(dir: &Path, path: Option<&str>) -> bool {
    path.unwrap_or_default().split(':').any(|p| !p.is_empty() && Path::new(p) == dir)
}

fn command_status() -> CommandStatus {
    let link = command_link();
    let on_path = link.as_ref().and_then(|l| l.parent()).is_some_and(|d| dir_on_path(d, crate::procs::login_path()));
    CommandStatus { link: link.map(|p| p.to_string_lossy().into_owned()), target: this_binary().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(), on_path }
}

/// One account's skill and, for Claude Code, its plugin; `codealong status` prints these too.
pub fn account_skill(source: Option<&Path>, a: &AccountCfg) -> AccountSkill {
    let dir = skill_dir(a);
    let state = match (source, &dir) {
        (Some(s), Some(d)) => copy_state(s, d),
        _ => CopyState::default(),
    };
    let plugin = takes_plugin(a).then(|| plugin_state(&a.home_path()));
    AccountSkill { name: a.name.clone(), harness: a.harness.clone(), path: dir.map(|d| d.to_string_lossy().into_owned()), installed: state.installed, current: state.current, edited: state.edited, plugin }
}

/// The command link and the skill of every account that isn't turned off, as things stand.
pub fn status() -> AgentsStatus {
    let source = skill_source();
    // Settings that don't parse list no accounts, and say why in place of them.
    let (cfg, errors) = match config::current() {
        Ok(cfg) => (cfg, Vec::new()),
        Err(e) => (config::Config::default(), vec![format!("{e:#}")]),
    };
    let plugin_version = shipped_version();
    AgentsStatus { command: command_status(), skill_found: source.is_some(), plugin_version, accounts: cfg.accounts.iter().filter(|a| !a.disabled).map(|a| account_skill(source.as_deref(), a)).collect(), errors }
}

/// What an install pass is asked to do.
#[derive(Debug, Clone, Default)]
pub struct Ask {
    /// Link the command, in `bin` when it's given (`link_command`).
    pub command: bool,
    pub bin: Option<PathBuf>,
    /// The accounts to put Codealong in, by name.
    pub accounts: Vec<String>,
    /// Leave a skill copy someone edited as it is, as installing for every account does. Naming
    /// an account is asking for its copy, so it's replaced then.
    pub keep_edited: bool,
}

/// What an install pass did.
#[derive(Debug, Clone, Default)]
pub struct Done {
    /// The command's link, when it made one.
    pub command: Option<PathBuf>,
    /// The accounts it put Codealong in: the plugin in a Claude Code account, the skill in any
    /// other.
    pub accounts: Vec<String>,
    /// What went wrong, one line each. A failure for one account doesn't stop the others.
    pub errors: Vec<String>,
}

/// Links the command when asked, and puts Codealong in the accounts asked for: the plugin in a
/// Claude Code account, the skill in any other. Settings › Agents and `codealong install` both
/// come to it.
pub fn run(ask: &Ask) -> Done {
    let mut done = Done::default();
    if ask.command {
        match link_command(ask.bin.clone()) {
            Ok(p) => done.command = Some(p),
            Err(e) => done.errors.push(format!("The command wasn't linked: {e:#}")),
        }
    }
    let cfg = match config::current() {
        Ok(cfg) => cfg,
        Err(e) => {
            done.errors.push(format!("{e:#}"));
            return done;
        }
    };
    let source = skill_source();
    for name in &ask.accounts {
        let Some(a) = cfg.accounts.iter().find(|a| &a.name == name) else {
            done.errors.push(format!("There's no account named {name}."));
            continue;
        };
        if takes_plugin(a) {
            match install_plugin(a) {
                Ok(()) => done.accounts.push(a.name.clone()),
                Err(e) => done.errors.push(format!("{}: the plugin wasn't installed: {e:#}", a.name)),
            }
            continue;
        }
        let (Some(src), Some(dir)) = (source.as_deref(), skill_dir(a)) else {
            done.errors.push(match source {
                None => format!("{}: the skill folder isn't next to this binary, so it can't be installed.", a.name),
                Some(_) => format!("{}: {} has no skills folder Codealong knows.", a.name, a.harness),
            });
            continue;
        };
        if ask.keep_edited && copy_state(src, &dir).edited {
            done.errors.push(format!("{}: {} was edited, so it was left alone; `codealong install --skill --account {}` replaces it.", a.name, crate::util::tilde(&dir), a.name));
            continue;
        }
        match copy_skill(src, &dir) {
            Ok(()) => done.accounts.push(a.name.clone()),
            Err(e) => done.errors.push(format!("{}: {e:#}", a.name)),
        }
    }
    done
}

/// Links the command when asked, and puts Codealong in the named accounts (`run`), replacing an
/// edited skill copy, since naming an account is asking for its copy. Then reports the new state,
/// with what went wrong in `errors`.
pub fn install(command: bool, accounts: &[String]) -> Result<AgentsStatus> {
    let done = run(&Ask { command, accounts: accounts.to_vec(), ..Default::default() });
    let mut out = status();
    out.errors = done.errors;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_link_follows_the_app_that_opens() {
        let tmp = crate::fixtures::scratch("link-test");
        let (bin, other_bin) = (tmp.join("bin"), tmp.join("other"));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&other_bin).unwrap();
        let dev = tmp.join("Codealong Dev.app/Contents/MacOS/codealong");
        let installed = tmp.join("Codealong.app/Contents/MacOS/codealong");
        std::os::unix::fs::symlink(&installed, bin.join("codealong")).unwrap();
        // Someone's own link, to a script of theirs: not Codealong's to move.
        std::os::unix::fs::symlink(tmp.join("my-wrapper.sh"), other_bin.join("codealong")).unwrap();

        follow_links_in(&[bin.clone(), other_bin.clone()], &dev);
        assert_eq!(std::fs::read_link(bin.join("codealong")).unwrap(), dev);
        assert_eq!(std::fs::read_link(other_bin.join("codealong")).unwrap(), tmp.join("my-wrapper.sh"));

        // A binary outside a bundle (cargo run) never claims it.
        follow_links_in(&[bin.clone()], &tmp.join("target/debug/codealong"));
        assert_eq!(std::fs::read_link(bin.join("codealong")).unwrap(), dev);
        assert!(!in_a_bundle(Path::new("/Users/me/codealong-target/release/codealong")));
        assert!(in_a_bundle(Path::new("/Applications/Codealong.app/Contents/MacOS/codealong")));
    }

    #[test]
    fn an_app_opened_by_hand_is_opened_by_macos_with_the_docks_environment() {
        assert!(opened_by_hand(false, Some((4321, "zsh")), "codealong"));
        assert!(opened_by_hand(false, Some((4321, "bash")), "codealong"), "relaunch.sh");
        assert!(!opened_by_hand(true, Some((1, "launchd")), "codealong"), "macOS opened it");
        assert!(!opened_by_hand(false, Some((4321, "codealong")), "codealong"), "the copy before it restarts it for an update");
        // Started in the background (`codealong &`, or detached in a session of its own) by a
        // process that quit before it looked, whatever environment it kept.
        assert!(opened_by_hand(false, Some((1, "launchd")), "codealong"));
        assert!(opened_by_hand(false, None, "codealong"));

        let shell: std::collections::HashMap<&str, &str> = [
            ("HOME", "/Users/me"),
            ("USER", "me"),
            ("PATH", "/Users/me/.local/bin:/usr/bin"),
            ("TMPDIR", "/private/tmp/claude-501/"),
            ("SSH_AUTH_SOCK", "/Users/me/.agent.sock"),
            ("CLAUDE_CODE_SESSION_ID", "8a983773"),
            ("CODEALONG_SESSION", "claude:claude:8a983773"),
            ("CODEALONG_DEVICE_KEY", "ab12"),
        ]
        .into();
        let env: std::collections::HashMap<String, String> =
            dock_environment(|name| shell.get(name).map(|v| v.to_string()), "/bin/zsh", Some("/var/folders/x/T/"), Some("/private/tmp/launchd/Listeners")).into_iter().collect();
        assert_eq!(env["HOME"], "/Users/me");
        assert_eq!(env["USER"], "me");
        assert_eq!(env["SHELL"], "/bin/zsh");
        assert_eq!(env["PATH"], "/usr/bin:/bin:/usr/sbin:/sbin");
        assert_eq!(env["TMPDIR"], "/var/folders/x/T/");
        assert_eq!(env["SSH_AUTH_SOCK"], "/private/tmp/launchd/Listeners");
        assert_eq!(env["CODEALONG_DEVICE_KEY"], "ab12");
        assert!(!env.contains_key("CLAUDE_CODE_SESSION_ID") && !env.contains_key("CODEALONG_SESSION"), "{env:?}");
        assert!(!env.contains_key("LOGNAME"), "only what the opener has");
        if crate::os::NATIVE == crate::os::Os::MacOs {
            let temp = crate::os::dirs::user_temp_dir();
            assert!(temp.as_deref().is_some_and(|t| t.ends_with("/T/")), "{temp:?}");
        }
    }

    #[test]
    fn a_folder_is_on_path_only_as_an_entry() {
        let path = Some("/opt/homebrew/bin:/usr/local/bin::/usr/bin");
        assert!(dir_on_path(Path::new("/usr/local/bin"), path));
        assert!(dir_on_path(Path::new("/usr/bin"), path));
        assert!(!dir_on_path(Path::new("/usr/local"), path));
        assert!(!dir_on_path(Path::new("/Users/x/.local/bin"), path));
        assert!(!dir_on_path(Path::new("/usr/bin"), None));
    }

    #[test]
    fn skill_rows_say_installed_and_current() {
        let root = crate::fixtures::scratch("agents");
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("SKILL.md"), "---\nname: codealong\n---\n").unwrap();
        let a = AccountCfg { name: "claude9".into(), harness: "claude".into(), home: root.join("claude9").to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let row = account_skill(Some(&src), &a);
        assert_eq!((row.installed, row.current), (false, false));
        assert!(row.path.as_deref().unwrap().ends_with("claude9/skills/codealong"));
        copy_skill(&src, &skill_dir(&a).unwrap()).unwrap();
        let row = account_skill(Some(&src), &a);
        assert_eq!((row.installed, row.current, row.edited), (true, true, false));
        // Without a source nothing counts as installed, and an unknown harness has no path.
        assert_eq!(account_skill(None, &a).installed, false);
        let other = AccountCfg { name: "x".into(), harness: "other".into(), home: "~/x".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        assert!(account_skill(Some(&src), &other).path.is_none());
    }

    #[test]
    fn refreshes_copies_in_folders_config_toml_listed_before() {
        let claude = AccountCfg { name: "claude".into(), harness: "claude".into(), home: "/Users/me/.claude".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let targets = refresh_targets(&[claude], &["/Users/me/.claude".to_string(), "/Users/me/.claude-old".to_string()]);
        assert_eq!(targets, vec![PathBuf::from("/Users/me/.claude/skills/codealong"), PathBuf::from("/Users/me/.claude-old/skills/codealong")]);
    }

    #[test]
    fn reads_the_plugin_from_the_accounts_own_files() {
        let root = crate::fixtures::scratch("plugin-state");
        let (home, ours) = (root.join("home"), root.join("support/plugin-marketplace"));
        std::fs::create_dir_all(home.join("plugins")).unwrap();
        std::fs::create_dir_all(&ours).unwrap();
        assert_eq!(plugin_state_in(&home, &ours), PluginState::default());
        let write = |f: &str, v: Value| std::fs::write(home.join(f), v.to_string()).unwrap();
        // As Claude Code 2.1.284 wrote them.
        write("plugins/installed_plugins.json", serde_json::json!({ "version": 2, "plugins": { PLUGIN: [{ "scope": "user", "installPath": "/x", "version": "0.2.0" }] } }));
        write("plugins/known_marketplaces.json", serde_json::json!({ MARKETPLACE: { "source": { "source": "directory", "path": ours }, "installLocation": ours } }));
        // Installed, with nothing in settings.json saying it's on: Claude Code doesn't run it.
        assert!(plugin_state_in(&home, &ours).disabled);
        write("settings.json", serde_json::json!({ "enabledPlugins": { PLUGIN: true } }));
        assert_eq!(plugin_state_in(&home, &ours), PluginState { version: Some("0.2.0".into()), disabled: false, from: None });
        assert_eq!(enabled_entry(&home), Some(true));
        write("settings.json", serde_json::json!({ "enabledPlugins": { PLUGIN: false } }));
        assert!(plugin_state_in(&home, &ours).disabled);
        assert_eq!(enabled_entry(&home), Some(false));
        write("plugins/known_marketplaces.json", serde_json::json!({ MARKETPLACE: { "source": { "source": "github", "repo": "codealong/codealong" } } }));
        assert_eq!(plugin_state_in(&home, &ours).from.as_deref(), Some("codealong/codealong"));
    }

    #[test]
    fn stages_the_marketplace_with_its_dot_folders_links_command_and_a_version_of_its_files() {
        let root = crate::fixtures::scratch("stage");
        let src = root.join("repo");
        for d in [".claude-plugin", "plugin/.claude-plugin", "plugin/bin", "skills/codealong"] {
            std::fs::create_dir_all(src.join(d)).unwrap();
        }
        std::fs::write(src.join(".claude-plugin/marketplace.json"), r#"{"name":"codealong","plugins":[{"name":"codealong","source":"./plugin","version":"1.0.0"}]}"#).unwrap();
        std::fs::write(src.join("plugin/.claude-plugin/plugin.json"), r#"{"name":"codealong","version":"1.0.0"}"#).unwrap();
        std::fs::write(src.join("plugin/bin/codealong"), "#!/bin/sh\n").unwrap();
        std::fs::write(src.join("skills/codealong/SKILL.md"), "skill").unwrap();
        std::os::unix::fs::symlink("../skills", src.join("plugin/skills")).unwrap();
        let dst = root.join("staged");
        let v1 = stage_marketplace_from(&src, None, &dst).unwrap();
        assert!(v1.starts_with("1.0.0+") && v1.len() == "1.0.0+".len() + 8, "{v1}");
        assert_eq!(plugin_version(&dst).as_deref(), Some(v1.as_str()));
        let m: Value = serde_json::from_str(&std::fs::read_to_string(dst.join(".claude-plugin/marketplace.json")).unwrap()).unwrap();
        assert_eq!(m["plugins"][0]["version"], Value::String(v1.clone()));
        assert_eq!(std::fs::read_to_string(dst.join("plugin/skills/codealong/SKILL.md")).unwrap(), "skill");
        assert!(!dst.join("plugin/skills").is_symlink(), "a copy, not the link");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(dst.join("plugin/bin/codealong")).unwrap().permissions().mode() & 0o777, 0o755);
        assert!(!dst.join("skills").exists(), "only the marketplace's parts");
        // The same files are the same version; a changed skill is a new one.
        assert_eq!(stage_marketplace_from(&src, None, &dst).unwrap(), v1);
        std::fs::write(src.join("skills/codealong/SKILL.md"), "skill, changed").unwrap();
        let v2 = stage_marketplace_from(&src, None, &dst).unwrap();
        assert!(v2 != v1 && v2.starts_with("1.0.0+"));
        assert_eq!(plugin_version(&dst).as_deref(), Some(v2.as_str()));
        // The bundle's plugin has no skill of its own: the app's copy goes in.
        std::fs::remove_file(src.join("plugin/skills")).unwrap();
        let v3 = stage_marketplace_from(&src, Some(&src.join("skills/codealong")), &dst).unwrap();
        assert_eq!(v3, v2, "the same files, whichever way they came");
        std::fs::remove_dir_all(&dst).unwrap();
        stage_marketplace_from(&src, Some(&src.join("skills/codealong")), &dst).unwrap();
        assert_eq!(std::fs::read_to_string(dst.join("plugin/skills/codealong/SKILL.md")).unwrap(), "skill, changed");
        assert!(!dst.with_extension("new").exists());
    }

    #[test]
    fn this_build_carries_the_plugin() {
        let src = plugin_source().expect("the repo is a marketplace");
        assert!(shipped_version().is_some_and(|v| v.contains('+')));
        let hooks: Value = serde_json::from_str(&std::fs::read_to_string(src.join("plugin/hooks/hooks.json")).unwrap()).unwrap();
        assert!(hooks["hooks"]["PermissionRequest"].is_array());
    }

    /// Against Claude Code itself, in a throwaway account folder set up as a hand install leaves
    /// it (the plugin from the repo's GitHub marketplace, and the skill copied in before):
    /// `cargo test plugin_replaces -- --ignored --nocapture`. Needs `claude` on PATH and git
    /// access to the repo; stages the marketplace in Codealong's own folder, as the app does.
    #[test]
    #[ignore]
    fn plugin_replaces_a_hand_install_and_the_skill_copy() {
        let home = std::env::temp_dir().join(format!("codealong-plugin-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let a = AccountCfg { name: "throwaway".into(), harness: "claude".into(), home: home.to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        claude_plugin(&home, &["marketplace", "add", "codealong/codealong"]).unwrap();
        claude_plugin(&home, &["install", PLUGIN]).unwrap();
        assert_eq!(plugin_state(&home).from.as_deref(), Some("codealong/codealong"));
        copy_skill(&skill_source().unwrap(), &skill_dir(&a).unwrap()).unwrap();
        install_plugin(&a).unwrap();
        let state = plugin_state(&home);
        println!("{state:?}");
        assert_eq!(state, PluginState { version: shipped_version(), disabled: false, from: None });
        assert!(!skill_dir(&a).unwrap().exists(), "the skill copy went");
        // Again: nothing to do, and it says so without failing.
        install_plugin(&a).unwrap();
        plugin_verb(&home, "disable").unwrap();
        assert!(plugin_state(&home).disabled);
        install_plugin(&a).unwrap();
        assert!(!plugin_state(&home).disabled);
        let _ = std::fs::remove_dir_all(&home);
    }

    fn account(harness: &str, home: &str) -> AccountCfg {
        AccountCfg { name: harness.into(), harness: harness.into(), home: home.into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false }
    }

    #[test]
    fn each_harness_has_its_skills_folder() {
        assert!(skill_dir(&account("claude", "~/.claude2")).unwrap().ends_with(".claude2/skills/codealong"));
        assert!(skill_dir(&account("codex", "~/.codex")).unwrap().ends_with(".codex/skills/codealong"));
        assert!(skill_dir(&account("opencode", "~/.local/share/opencode")).unwrap().ends_with(".config/opencode/skills/codealong"));
        assert!(skill_dir(&account("antigravity", "~/.gemini/antigravity")).unwrap().ends_with(".gemini/config/skills/codealong"));
        // In your home, wherever the account's own folder is.
        assert_eq!(skill_dir(&account("cursor", "~/elsewhere/cursor")), Some(home().join(".cursor/skills/codealong")));
        assert!(skill_dir(&account("other", "~/x")).is_none());
    }

    #[test]
    fn copies_replace_only_their_own() {
        let root = crate::fixtures::scratch("install");
        let src = root.join("src");
        std::fs::create_dir_all(src.join("references")).unwrap();
        std::fs::write(src.join("SKILL.md"), "---\nname: codealong\n---\n").unwrap();
        std::fs::write(src.join("references/reference.md"), "# ref\n").unwrap();
        let dst = root.join("skills/codealong");
        copy_skill(&src, &dst).unwrap();
        assert!(dst.join("SKILL.md").is_file() && dst.join("references/reference.md").is_file());
        let fresh = CopyState { installed: true, current: true, edited: false };
        assert_eq!(copy_state(&src, &dst), fresh);
        // Finder's .DS_Store isn't an edit.
        std::fs::write(dst.join(".DS_Store"), "x").unwrap();
        assert_eq!(copy_state(&src, &dst), fresh);
        std::fs::write(src.join("SKILL.md"), "---\nname: codealong\ndescription: more\n---\n").unwrap();
        assert!(copy_state(&src, &dst).stale());
        copy_skill(&src, &dst).unwrap();
        assert_eq!(copy_state(&src, &dst), fresh);
        // An edited copy isn't stale, even once the skill changes: updates leave it alone.
        std::fs::write(dst.join("SKILL.md"), "---\nname: codealong\ndescription: mine\n---\n").unwrap();
        assert_eq!(copy_state(&src, &dst), CopyState { installed: true, current: true, edited: true });
        std::fs::write(src.join("SKILL.md"), "---\nname: codealong\ndescription: newer\n---\n").unwrap();
        let edited = copy_state(&src, &dst);
        assert!(edited.edited && !edited.current && !edited.stale());
        assert!(std::fs::read_to_string(dst.join("SKILL.md")).unwrap().contains("mine"));
        // Someone's own folder of the same name stays.
        let theirs = root.join("theirs/codealong");
        std::fs::create_dir_all(&theirs).unwrap();
        std::fs::write(theirs.join("SKILL.md"), "mine").unwrap();
        assert!(copy_skill(&src, &theirs).is_err());
        assert_eq!(std::fs::read_to_string(theirs.join("SKILL.md")).unwrap(), "mine");
    }
}

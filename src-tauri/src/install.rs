//! Putting the agent skill where each harness reads personal skills, for every account in
//! config.toml. The skill (`skills/codealong` in the repo, `Contents/Resources/skills/codealong`
//! in the bundle) is copied, not linked: not every harness follows links, and the bundle moves
//! when the app is rebuilt. A copy carries `.codealong-version`, the hash of what was copied;
//! at launch the app refreshes copies it made when the skill changes, and never touches a
//! folder it didn't make. A copy whose files no longer hash to its marker was edited by
//! someone, and is left alone until they ask for it to be replaced.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;
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
    let dir = match a.harness.as_str() {
        // Claude Code: <config dir>/skills; a second account's config dir is its home here.
        "claude" => a.home_path().join("skills"),
        "codex" => a.home_path().join("skills"),
        "opencode" => home().join(".config/opencode/skills"),
        "antigravity" => home().join(".gemini/config/skills"),
        _ => return None,
    };
    Some(dir.join("codealong"))
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

/// At launch: every copy Codealong made is brought up to date with the skill shipped in this
/// build, unless someone edited it. Folders without a copy are left as they are.
pub fn refresh_installed() {
    let Some(src) = skill_source() else { return };
    let cfg = config::load().ok().flatten().unwrap_or_default();
    let known = crate::state::SeenState::load().known_accounts;
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

/// What Settings › Agents shows: the command link and the skill per account.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsStatus {
    pub command: CommandStatus,
    /// Whether the skill folder is next to this binary at all.
    pub skill_found: bool,
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

fn account_skill(source: Option<&Path>, a: &AccountCfg) -> AccountSkill {
    let dir = skill_dir(a);
    let state = match (source, &dir) {
        (Some(s), Some(d)) => copy_state(s, d),
        _ => CopyState::default(),
    };
    AccountSkill { name: a.name.clone(), harness: a.harness.clone(), path: dir.map(|d| d.to_string_lossy().into_owned()), installed: state.installed, current: state.current, edited: state.edited }
}

/// The command link and every account's skill, as things stand.
pub fn status() -> AgentsStatus {
    let source = skill_source();
    let cfg = config::load().ok().flatten().unwrap_or_default();
    AgentsStatus { command: command_status(), skill_found: source.is_some(), accounts: cfg.accounts.iter().map(|a| account_skill(source.as_deref(), a)).collect(), errors: Vec::new() }
}

/// Links the command when asked and copies the skill for the named accounts, then reports
/// the new state. Naming an account is asking for its copy, so an edited one is replaced too.
/// A failure for one account doesn't stop the others; each is in `errors`.
pub fn install(command: bool, accounts: &[String]) -> Result<AgentsStatus> {
    let mut errors = Vec::new();
    if command {
        if let Err(e) = link_command(None) {
            errors.push(format!("The command wasn't linked: {e:#}"));
        }
    }
    if !accounts.is_empty() {
        let cfg = config::load().ok().flatten().unwrap_or_default();
        match skill_source() {
            None => errors.push("The skill folder isn't next to this binary, so it can't be installed.".into()),
            Some(source) => {
                for name in accounts {
                    let Some(a) = cfg.accounts.iter().find(|a| &a.name == name) else {
                        errors.push(format!("There's no account named {name}."));
                        continue;
                    };
                    match skill_dir(a) {
                        Some(dir) => {
                            if let Err(e) = copy_skill(&source, &dir) {
                                errors.push(format!("{}: {e:#}", a.name));
                            }
                        }
                        None => errors.push(format!("{}: {} has no skills folder Codealong knows.", a.name, a.harness)),
                    }
                }
            }
        }
    }
    let mut out = status();
    out.errors = errors;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let root = std::env::temp_dir().join(format!("codealong-agents-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("SKILL.md"), "---\nname: codealong\n---\n").unwrap();
        let a = AccountCfg { name: "claude9".into(), harness: "claude".into(), home: root.join("claude9").to_string_lossy().into_owned(), command: None, start: None, preferred: false, acp: None };
        let row = account_skill(Some(&src), &a);
        assert_eq!((row.installed, row.current), (false, false));
        assert!(row.path.as_deref().unwrap().ends_with("claude9/skills/codealong"));
        copy_skill(&src, &skill_dir(&a).unwrap()).unwrap();
        let row = account_skill(Some(&src), &a);
        assert_eq!((row.installed, row.current, row.edited), (true, true, false));
        // Without a source nothing counts as installed, and an unknown harness has no path.
        assert_eq!(account_skill(None, &a).installed, false);
        let other = AccountCfg { name: "x".into(), harness: "other".into(), home: "~/x".into(), command: None, start: None, preferred: false, acp: None };
        assert!(account_skill(Some(&src), &other).path.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn refreshes_copies_in_folders_config_toml_listed_before() {
        let claude = AccountCfg { name: "claude".into(), harness: "claude".into(), home: "/Users/me/.claude".into(), command: None, start: None, preferred: false, acp: None };
        let targets = refresh_targets(&[claude], &["/Users/me/.claude".to_string(), "/Users/me/.claude-old".to_string()]);
        assert_eq!(targets, vec![PathBuf::from("/Users/me/.claude/skills/codealong"), PathBuf::from("/Users/me/.claude-old/skills/codealong")]);
    }

    fn account(harness: &str, home: &str) -> AccountCfg {
        AccountCfg { name: harness.into(), harness: harness.into(), home: home.into(), command: None, start: None, preferred: false, acp: None }
    }

    #[test]
    fn each_harness_has_its_skills_folder() {
        assert!(skill_dir(&account("claude", "~/.claude2")).unwrap().ends_with(".claude2/skills/codealong"));
        assert!(skill_dir(&account("codex", "~/.codex")).unwrap().ends_with(".codex/skills/codealong"));
        assert!(skill_dir(&account("opencode", "~/.local/share/opencode")).unwrap().ends_with(".config/opencode/skills/codealong"));
        assert!(skill_dir(&account("antigravity", "~/.gemini/antigravity")).unwrap().ends_with(".gemini/config/skills/codealong"));
        assert!(skill_dir(&account("other", "~/x")).is_none());
    }

    #[test]
    fn copies_replace_only_their_own() {
        let root = std::env::temp_dir().join(format!("codealong-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
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
        let _ = std::fs::remove_dir_all(&root);
    }
}
